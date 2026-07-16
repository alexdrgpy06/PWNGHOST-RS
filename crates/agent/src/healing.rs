//! Healing state machine (6-layer self-healing)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Six healing layers in escalation order
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealingLayer {
    FwWatchdog,
    CrashLoop,
    AoBackoff,
    GpioCycle,
    GiveUp,
    UsbLifeline,
}

impl HealingLayer {
    pub fn next(&self) -> Option<Self> {
        match self {
            HealingLayer::FwWatchdog => Some(HealingLayer::CrashLoop),
            HealingLayer::CrashLoop => Some(HealingLayer::AoBackoff),
            HealingLayer::AoBackoff => Some(HealingLayer::GpioCycle),
            HealingLayer::GpioCycle => Some(HealingLayer::GiveUp),
            HealingLayer::GiveUp => Some(HealingLayer::UsbLifeline),
            HealingLayer::UsbLifeline => None,
        }
    }
}

/// Healing actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealingAction {
    None,
    RestartAo,
    PowerCycleGpio,
    EnterSafeMode,
    EnableUsbLifeline,
}

/// Healing configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealingConfig {
    pub crash_window_seconds: u64,
    pub crash_threshold: u32,
    pub gpio_cycle_pin: u8,
    pub gpio_cycle_duration_ms: u64,
    pub max_ao_backoff_attempts: u32,
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            crash_window_seconds: 300,
            crash_threshold: 3,
            gpio_cycle_pin: 22,
            gpio_cycle_duration_ms: 100,
            max_ao_backoff_attempts: 5,
        }
    }
}

/// Healing state
#[derive(Debug, Clone)]
pub struct HealingState {
    pub active_layer: HealingLayer,
    pub layer_started_at: Instant,
    pub consecutive_crashes: u32,
    pub crash_window: VecDeque<Instant>,
    pub total_recoveries: u64,
    pub uptime_since_recovery: Duration,
}

impl HealingState {
    fn new() -> Self {
        Self {
            active_layer: HealingLayer::FwWatchdog,
            layer_started_at: Instant::now(),
            consecutive_crashes: 0,
            crash_window: VecDeque::new(),
            total_recoveries: 0,
            uptime_since_recovery: Duration::ZERO,
        }
    }
}

/// Main healer
pub struct Healer {
    state: HealingState,
    config: HealingConfig,
    escalation_count: u32,
    started_at: Instant,
}

impl Healer {
    pub fn new() -> Self {
        Self::with_config(HealingConfig::default())
    }

    pub fn with_config(config: HealingConfig) -> Self {
        Self {
            state: HealingState::new(),
            config,
            escalation_count: 0,
            started_at: Instant::now(),
        }
    }

    /// Record a crash event
    pub fn report_crash(&mut self) {
        let now = Instant::now();
        self.state.crash_window.push_back(now);
        self.state.consecutive_crashes += 1;
        self.prune_crash_window(now);

        // Escalate the crash-counting layers once the threshold is first
        // reached. Using an exact comparison avoids racing through every
        // layer as further crashes accumulate in the same window.
        if matches!(
            self.state.active_layer,
            HealingLayer::FwWatchdog | HealingLayer::CrashLoop
        ) && self.crashes_in_window(now) == self.config.crash_threshold
        {
            self.escalate();
        }
    }

    /// Record alive heartbeat
    pub fn report_alive(&mut self) {
        let now = Instant::now();
        self.state.crash_window.clear();
        self.state.consecutive_crashes = 0;
        self.state.uptime_since_recovery = now - self.started_at;
    }

    pub fn active_layer(&self) -> HealingLayer {
        self.state.active_layer
    }

    /// Check if action needed
    pub fn should_take_action(&self) -> bool {
        match self.state.active_layer {
            HealingLayer::FwWatchdog | HealingLayer::CrashLoop => {
                self.crashes_in_window(Instant::now()) >= self.config.crash_threshold
            }
            HealingLayer::AoBackoff | HealingLayer::GpioCycle | HealingLayer::GiveUp => true,
            HealingLayer::UsbLifeline => false,
        }
    }

    /// Decide and return action
    pub fn decide(&mut self) -> HealingAction {
        let now = Instant::now();
        self.prune_crash_window(now);
        let crash_count = self.crashes_in_window(now);

        match self.state.active_layer {
            HealingLayer::FwWatchdog => {
                if crash_count >= self.config.crash_threshold {
                    self.escalate();
                    HealingAction::RestartAo
                } else {
                    HealingAction::None
                }
            }

            HealingLayer::CrashLoop => {
                if crash_count >= self.config.crash_threshold {
                    self.escalate();
                    HealingAction::RestartAo
                } else {
                    HealingAction::None
                }
            }

            HealingLayer::AoBackoff => {
                let max_backoff =
                    Self::ao_backoff_max_duration(self.config.max_ao_backoff_attempts);
                if self.time_in_current_layer() > max_backoff {
                    self.escalate();
                    HealingAction::PowerCycleGpio
                } else {
                    HealingAction::RestartAo
                }
            }

            HealingLayer::GpioCycle => {
                self.escalate();
                HealingAction::EnterSafeMode
            }

            HealingLayer::GiveUp => {
                if self.time_in_current_layer() > Duration::from_secs(300) {
                    self.escalate();
                    HealingAction::EnableUsbLifeline
                } else {
                    HealingAction::EnterSafeMode
                }
            }

            HealingLayer::UsbLifeline => HealingAction::None,
        }
    }

    /// Move to next escalation layer
    pub fn escalate(&mut self) {
        if let Some(next) = self.state.active_layer.next() {
            self.state.active_layer = next;
            self.state.layer_started_at = Instant::now();
            self.escalation_count += 1;
        }
    }

    /// De-escalate back to FwWatchdog
    pub fn deescalate(&mut self) {
        self.state.active_layer = HealingLayer::FwWatchdog;
        self.state.layer_started_at = Instant::now();
        self.state.total_recoveries += 1;
    }

    /// Full reset
    pub fn reset(&mut self) {
        self.state = HealingState::new();
        self.escalation_count = 0;
        self.started_at = Instant::now();
    }

    /// Total escalations
    pub fn escalation_count(&self) -> u32 {
        self.escalation_count
    }

    /// Time in current layer
    pub fn time_in_current_layer(&self) -> Duration {
        Instant::now() - self.state.layer_started_at
    }

    /// Calculate max backoff duration
    fn ao_backoff_max_duration(max_attempts: u32) -> Duration {
        let total_secs = if max_attempts < 63 {
            (1u64 << max_attempts).saturating_sub(1)
        } else {
            u64::MAX
        };
        Duration::from_secs(total_secs)
    }

    // Private helpers
    fn prune_crash_window(&mut self, now: Instant) {
        let window = Duration::from_secs(self.config.crash_window_seconds);
        while let Some(&t) = self.state.crash_window.front() {
            if now - t > window {
                self.state.crash_window.pop_front();
            } else {
                break;
            }
        }
    }

    fn crashes_in_window(&self, now: Instant) -> u32 {
        let window = Duration::from_secs(self.config.crash_window_seconds);
        self.state
            .crash_window
            .iter()
            .filter(|&&t| now - t <= window)
            .count() as u32
    }
}

impl Default for Healer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_healer_initial_state() {
        let healer = Healer::new();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);
        assert_eq!(healer.escalation_count(), 0);
        assert!(!healer.should_take_action());
    }

    #[test]
    fn test_crash_escalation() {
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 2,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        healer.report_crash();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);

        healer.report_crash();
        assert_eq!(healer.active_layer(), HealingLayer::CrashLoop);
    }

    #[test]
    fn test_alive_resets_counters() {
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 1,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        healer.report_crash();
        assert_eq!(healer.state.consecutive_crashes, 1);

        healer.report_alive();
        assert_eq!(healer.state.consecutive_crashes, 0);
        assert!(healer.state.crash_window.is_empty());
    }

    #[test]
    fn test_healing_layer_next() {
        assert_eq!(
            HealingLayer::FwWatchdog.next(),
            Some(HealingLayer::CrashLoop)
        );
        assert_eq!(
            HealingLayer::CrashLoop.next(),
            Some(HealingLayer::AoBackoff)
        );
        assert_eq!(
            HealingLayer::AoBackoff.next(),
            Some(HealingLayer::GpioCycle)
        );
        assert_eq!(HealingLayer::GpioCycle.next(), Some(HealingLayer::GiveUp));
        assert_eq!(HealingLayer::GiveUp.next(), Some(HealingLayer::UsbLifeline));
        assert_eq!(HealingLayer::UsbLifeline.next(), None);
    }

    #[test]
    fn test_full_escalation_chain() {
        let mut healer = Healer::new();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);

        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::CrashLoop);

        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::AoBackoff);

        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::GpioCycle);

        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::GiveUp);

        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::UsbLifeline);

        // Top of chain - no further escalation
        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::UsbLifeline);
    }

    #[test]
    fn test_deescalation() {
        let mut healer = Healer::new();
        healer.escalate();
        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::AoBackoff);

        healer.deescalate();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);
        assert_eq!(healer.state.total_recoveries, 1);
    }

    #[test]
    fn test_healing_action_fw_watchdog() {
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 1,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        healer.report_crash();
        let action = healer.decide();
        assert_eq!(action, HealingAction::RestartAo);
    }

    #[test]
    fn test_healing_config_default() {
        let cfg = HealingConfig::default();
        assert_eq!(cfg.crash_window_seconds, 300);
        assert_eq!(cfg.crash_threshold, 3);
        assert_eq!(cfg.gpio_cycle_pin, 22);
        assert_eq!(cfg.gpio_cycle_duration_ms, 100);
        assert_eq!(cfg.max_ao_backoff_attempts, 5);
    }

    #[test]
    fn test_escalation_count() {
        let mut healer = Healer::new();
        assert_eq!(healer.escalation_count(), 0);

        healer.escalate();
        assert_eq!(healer.escalation_count(), 1);

        healer.escalate();
        assert_eq!(healer.escalation_count(), 2);

        healer.deescalate();
        // deescalate doesn't reset counter
        assert_eq!(healer.escalation_count(), 2);

        healer.reset();
        assert_eq!(healer.escalation_count(), 0);
    }

    #[test]
    fn test_layer_timing() {
        let healer = Healer::new();
        let t1 = healer.time_in_current_layer();
        sleep(Duration::from_millis(10));
        let t2 = healer.time_in_current_layer();
        assert!(t2 >= t1);
    }
}
