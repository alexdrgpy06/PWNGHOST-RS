use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Six-layer healing state machine for the self-healing subsystem.
///
/// Escalation order:
///   1. FwWatchdog  – monitor firmware crash indicator
///   2. CrashLoop   – detect rapid crash loops
///   3. AoBackoff   – exponential backoff for AngryOxide restarts
///   4. GpioCycle   – power-cycle the WiFi chip via GPIO
///   5. GiveUp      – stop attacking, enter safe mode
///   6. UsbLifeline – enable USB gadget mode for external recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealingLayer {
    FwWatchdog,
    CrashLoop,
    AoBackoff,
    GpioCycle,
    GiveUp,
    UsbLifeline,
}

impl HealingLayer {
    /// Returns the next layer in the escalation chain, or `None` at the top.
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

/// Configuration for the healing state machine.
#[derive(Debug, Clone)]
pub struct HealingConfig {
    /// How long (seconds) to track crashes in the sliding window.
    pub crash_window_seconds: u64,
    /// Crashes in the window before escalation triggers.
    pub crash_threshold: u32,
    /// GPIO pin used for WL_REG_ON cycling.
    pub gpio_cycle_pin: u8,
    /// How long (ms) to hold the GPIO reset pulse.
    pub gpio_cycle_duration_ms: u64,
    /// Max AngryOxide restart attempts before giving up.
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

/// Tracks the dynamic runtime state of the healing subsystem.
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

/// Action emitted by [`Healer::decide`] for the caller to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealingAction {
    /// No action needed.
    None,
    /// Restart the AngryOxide process.
    RestartAo,
    /// Power-cycle the WiFi chip via GPIO.
    PowerCycleGpio,
    /// Stop attacking and enter safe mode.
    EnterSafeMode,
    /// Enable USB gadget mode for external recovery access.
    EnableUsbLifeline,
}

/// The self-healing orchestrator.
///
/// Drives the 6-layer escalation state machine.  Call [`report_crash`] when
/// the agent detects a failure, then [`decide`] to get the action to take.
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

    /// Record a crash event.
    ///
    /// Adds a timestamp to the sliding crash window and increments the
    /// consecutive-crash counter.  Old entries outside the window are pruned.
    pub fn report_crash(&mut self) {
        let now = Instant::now();
        self.state.crash_window.push_back(now);
        self.state.consecutive_crashes += 1;
        self.prune_crash_window(now);
    }

    /// Record an alive heartbeat.
    ///
    /// Resets all crash counters and updates the total uptime.
    pub fn report_alive(&mut self) {
        let now = Instant::now();
        self.state.crash_window.clear();
        self.state.consecutive_crashes = 0;
        self.state.uptime_since_recovery = now - self.started_at;
    }

    pub fn active_layer(&self) -> HealingLayer {
        self.state.active_layer
    }

    /// Returns `true` when the current layer indicates an action is needed.
    pub fn should_take_action(&self) -> bool {
        match self.state.active_layer {
            HealingLayer::FwWatchdog | HealingLayer::CrashLoop => {
                self.crashes_in_window(Instant::now()) >= self.config.crash_threshold
            }
            HealingLayer::AoBackoff | HealingLayer::GpioCycle | HealingLayer::GiveUp => true,
            HealingLayer::UsbLifeline => false,
        }
    }

    /// Move to the next escalation layer.
    ///
    /// This is a no-op if already at the top layer (`UsbLifeline`).
    pub fn escalate(&mut self) {
        if let Some(next) = self.state.active_layer.next() {
            self.state.active_layer = next;
            self.state.layer_started_at = Instant::now();
            self.escalation_count += 1;
        }
    }

    /// De-escalate back to `FwWatchdog` (e.g. after a full recovery).
    pub fn deescalate(&mut self) {
        self.state.active_layer = HealingLayer::FwWatchdog;
        self.state.layer_started_at = Instant::now();
        self.state.total_recoveries += 1;
    }

    /// Full reset to the initial state.
    pub fn reset(&mut self) {
        self.state = HealingState::new();
        self.escalation_count = 0;
    }

    /// Total number of escalations that have occurred.
    pub fn escalation_count(&self) -> u32 {
        self.escalation_count
    }

    /// Duration spent in the current layer.
    pub fn time_in_current_layer(&self) -> Duration {
        Instant::now() - self.state.layer_started_at
    }

    /// The core state-machine decision logic.
    ///
    /// Examines the current layer and dynamic state, escalates when thresholds
    /// are exceeded, and returns the [`HealingAction`] the caller should
    /// perform.
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
                let max_backoff = Self::ao_backoff_max_duration(self.config.max_ao_backoff_attempts);
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

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Total duration (in seconds) the AoBackoff layer will wait before
    /// escalating:  ∑ 2ⁱ  for i ∈ 0 .. max_attempts.
    ///
    /// For the default of 5 attempts this yields 1+2+4+8+16 = 31 seconds.
    fn ao_backoff_max_duration(max_attempts: u32) -> Duration {
        let total_secs = if max_attempts < 63 {
            (1u64 << max_attempts).saturating_sub(1)
        } else {
            u64::MAX
        };
        Duration::from_secs(total_secs)
    }

    /// Remove crash timestamps that have fallen outside the sliding window.
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

    /// Number of crashes still within the sliding time window.
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

    #[test]
    fn test_healer_initial_state() {
        let healer = Healer::new();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);
        assert_eq!(healer.escalation_count(), 0);
        assert_eq!(healer.state.consecutive_crashes, 0);
        assert!(healer.state.crash_window.is_empty());
    }

    #[test]
    fn test_crash_escalation() {
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 2,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        // Single crash is below threshold
        healer.report_crash();
        assert_eq!(healer.state.crash_window.len(), 1);
        assert_eq!(healer.state.consecutive_crashes, 1);
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);

        // Second crash hits the threshold
        healer.report_crash();
        assert_eq!(healer.state.crash_window.len(), 2);
        assert_eq!(healer.state.consecutive_crashes, 2);
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);

        // decide() escalates on threshold
        let action = healer.decide();
        assert_eq!(action, HealingAction::RestartAo);
        assert_eq!(healer.active_layer(), HealingLayer::CrashLoop);
    }

    #[test]
    fn test_alive_resets_counters() {
        let mut healer = Healer::new();
        healer.report_crash();
        healer.report_crash();
        healer.report_crash();
        assert_eq!(healer.state.consecutive_crashes, 3);

        healer.report_alive();
        assert_eq!(healer.state.consecutive_crashes, 0);
        assert!(healer.state.crash_window.is_empty());
        assert!(healer.state.uptime_since_recovery > Duration::ZERO);
    }

    #[test]
    fn test_layer_timing() {
        let healer = Healer::new();
        let t = healer.time_in_current_layer();
        assert!(t >= Duration::ZERO);
        // Very short sleep to ensure Duration advances
        std::thread::sleep(Duration::from_millis(5));
        let t2 = healer.time_in_current_layer();
        assert!(t2 > t);
    }

    #[test]
    fn test_healing_action_fw_watchdog() {
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 3,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        // Below threshold → None
        healer.report_crash();
        healer.report_crash();
        assert_eq!(healer.decide(), HealingAction::None);

        // Hit threshold → RestartAo
        healer.report_crash();
        assert_eq!(healer.decide(), HealingAction::RestartAo);
        assert_eq!(healer.active_layer(), HealingLayer::CrashLoop);
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

        // Top of chain — escalate is a no-op
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
        // total_recoveries should have incremented
        assert_eq!(healer.state.total_recoveries, 1);
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
        assert_eq!(healer.escalation_count(), 2); // de-escalate does not reset the counter

        healer.escalate();
        assert_eq!(healer.escalation_count(), 3);

        healer.reset();
        assert_eq!(healer.escalation_count(), 0);
    }

    #[test]
    fn test_should_take_action() {
        // FwWatchdog with no crashes → false
        let healer = Healer::new();
        assert!(!healer.should_take_action());

        // After enough crashes → true
        let config = HealingConfig {
            crash_window_seconds: 60,
            crash_threshold: 1,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);
        healer.report_crash();
        assert!(healer.should_take_action());

        // At the top layer → false
        healer.escalate();
        healer.escalate();
        healer.escalate();
        healer.escalate();
        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::UsbLifeline);
        assert!(!healer.should_take_action());
    }

    #[test]
    fn test_healing_layer_next() {
        assert_eq!(HealingLayer::FwWatchdog.next(), Some(HealingLayer::CrashLoop));
        assert_eq!(HealingLayer::CrashLoop.next(), Some(HealingLayer::AoBackoff));
        assert_eq!(HealingLayer::AoBackoff.next(), Some(HealingLayer::GpioCycle));
        assert_eq!(HealingLayer::GpioCycle.next(), Some(HealingLayer::GiveUp));
        assert_eq!(HealingLayer::GiveUp.next(), Some(HealingLayer::UsbLifeline));
        assert_eq!(HealingLayer::UsbLifeline.next(), None);
    }

    #[test]
    fn test_reset() {
        let mut healer = Healer::new();
        healer.report_crash();
        healer.report_crash();
        healer.report_crash();
        healer.escalate();
        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::AoBackoff);
        assert_eq!(healer.escalation_count(), 2);
        assert_eq!(healer.state.consecutive_crashes, 3);

        healer.reset();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);
        assert_eq!(healer.escalation_count(), 0);
        assert_eq!(healer.state.consecutive_crashes, 0);
        assert!(healer.state.crash_window.is_empty());
    }

    #[test]
    fn test_crash_window_pruning() {
        // With a 1-second window, crashes outside it are pruned.
        let config = HealingConfig {
            crash_window_seconds: 1,
            crash_threshold: 5,
            ..Default::default()
        };
        let mut healer = Healer::with_config(config);

        healer.report_crash();
        healer.report_crash();
        assert_eq!(healer.state.crash_window.len(), 2);

        std::thread::sleep(Duration::from_secs(2));

        healer.report_crash();
        // The two old crashes should be pruned; only the new one remains.
        assert_eq!(healer.state.crash_window.len(), 1);
        // consecutive_crashes is *not* reset by pruning — it tracks total
        // consecutive crashes without recovery.
        assert_eq!(healer.state.consecutive_crashes, 3);
    }

    #[test]
    fn test_escalate_stays_at_top() {
        let mut healer = Healer::new();
        // Push to top
        for _ in 0..6 {
            healer.escalate();
        }
        assert_eq!(healer.active_layer(), HealingLayer::UsbLifeline);

        // Additional calls are no-ops
        let before_count = healer.escalation_count();
        healer.escalate();
        healer.escalate();
        assert_eq!(healer.active_layer(), HealingLayer::UsbLifeline);
        assert_eq!(healer.escalation_count(), before_count);
    }

    #[test]
    fn test_healer_with_config_uses_supplied_values() {
        let config = HealingConfig {
            crash_window_seconds: 999,
            crash_threshold: 7,
            gpio_cycle_pin: 17,
            gpio_cycle_duration_ms: 250,
            max_ao_backoff_attempts: 3,
        };
        let _healer = Healer::with_config(config.clone());
        // Config is not exposed directly, but we can verify via behavior:
        // create a crash-heavy healer and check escalation uses threshold=7.
        let mut h = Healer::with_config(config);
        for _ in 0..6 {
            h.report_crash();
        }
        assert_eq!(h.decide(), HealingAction::None);
        h.report_crash(); // 7th crash
        assert_eq!(h.decide(), HealingAction::RestartAo);
    }

    #[test]
    fn test_default_trait() {
        let healer = Healer::default();
        assert_eq!(healer.active_layer(), HealingLayer::FwWatchdog);
    }

    #[test]
    fn test_decide_give_up_returns_enter_safe_mode() {
        let mut healer = Healer::new();
        // Jump directly to GiveUp (FwWatchdog → CrashLoop → AoBackoff → GpioCycle → GiveUp)
        for _ in 0..4 {
            healer.escalate();
        }
        assert_eq!(healer.active_layer(), HealingLayer::GiveUp);
        // Without waiting 300s, decide() should return EnterSafeMode
        assert_eq!(healer.decide(), HealingAction::EnterSafeMode);
    }

    #[test]
    fn test_decide_gpio_cycle_escalates_immediately() {
        let mut healer = Healer::new();
        // Jump to GpioCycle (FwWatchdog → CrashLoop → AoBackoff → GpioCycle)
        for _ in 0..3 {
            healer.escalate();
        }
        assert_eq!(healer.active_layer(), HealingLayer::GpioCycle);
        assert_eq!(healer.decide(), HealingAction::EnterSafeMode);
        // Should have escalated to GiveUp
        assert_eq!(healer.active_layer(), HealingLayer::GiveUp);
    }
}
