//! AngryOxide crash recovery with exponential backoff

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Recovery manager for AngryOxide subprocess
pub struct RecoveryManager {
    config: RecoveryConfig,
    crash_count: u32,
    last_crash: Option<Instant>,
    next_restart: Option<Instant>,
    state: RecoveryState,
}

/// Recovery configuration
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum crash count before giving up
    pub max_crashes: u32,
    /// Base backoff seconds
    pub base_backoff_secs: u64,
    /// Maximum backoff seconds
    pub max_backoff_secs: u64,
    /// Stable epochs needed to reset crash counter
    pub stable_epochs_for_reset: u32,
    /// Stable epoch counter
    stable_epochs: u32,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_crashes: 10,
            base_backoff_secs: 5,
            max_backoff_secs: 300,
            stable_epochs_for_reset: 10,
            stable_epochs: 0,
        }
    }
}

/// Recovery state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Running,
    Crashed,
    BackingOff,
    Failed,
}

impl RecoveryManager {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            crash_count: 0,
            last_crash: None,
            next_restart: None,
            state: RecoveryState::Running,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(RecoveryConfig::default())
    }

    /// Record a crash event
    pub fn record_crash(&mut self) {
        self.crash_count += 1;
        self.last_crash = Some(Instant::now());
        self.state = RecoveryState::Crashed;

        if self.crash_count >= self.config.max_crashes {
            error!(
                "AngryOxide reached max crash count ({}), stopping permanently",
                self.config.max_crashes
            );
            self.state = RecoveryState::Failed;
        } else {
            let backoff = self.backoff_seconds();
            warn!(
                "AngryOxide crash #{}, will restart in {}s",
                self.crash_count, backoff
            );
            self.next_restart = Some(Instant::now() + Duration::from_secs(backoff));
            self.state = RecoveryState::BackingOff;
        }
    }

    /// Record a stable epoch (no crash)
    pub fn record_stable_epoch(&mut self) {
        if self.state == RecoveryState::Running {
            self.config.stable_epochs += 1;
            if self.config.stable_epochs >= self.config.stable_epochs_for_reset && self.crash_count > 0 {
                info!(
                    "AngryOxide stable for {} epochs, resetting crash counter",
                    self.config.stable_epochs
                );
                self.crash_count = 0;
                self.config.stable_epochs = 0;
            }
        }
    }

    /// Calculate exponential backoff
    fn backoff_seconds(&self) -> u64 {
        let exp = self.config.base_backoff_secs * 2u64.pow(self.crash_count.saturating_sub(1));
        exp.min(self.config.max_backoff_secs)
    }

    /// Check if we should attempt auto-restart
    pub fn should_restart(&self) -> bool {
        matches!(self.state, RecoveryState::BackingOff)
            && self.next_restart.map_or(false, |t| Instant::now() >= t)
    }

    /// Attempt auto-restart
    pub async fn try_auto_restart(&mut self) -> bool {
        if !self.should_restart() {
            return false;
        }

        info!("Auto-restarting AngryOxide after backoff");
        self.next_restart = None;
        self.state = RecoveryState::Running;
        true
    }

    /// Get current state
    pub fn state(&self) -> RecoveryState {
        self.state
    }

    /// Get crash count
    pub fn crash_count(&self) -> u32 {
        self.crash_count
    }

    /// Get time until next restart
    pub fn time_until_restart(&self) -> Option<Duration> {
        self.next_restart.map(|t| t.saturating_duration_since(Instant::now()))
    }

    /// Reset recovery state (e.g., after manual restart)
    pub fn reset(&mut self) {
        self.crash_count = 0;
        self.config.stable_epochs = 0;
        self.last_crash = None;
        self.next_restart = None;
        self.state = RecoveryState::Running;
        info!("Recovery manager reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_manager_new() {
        let mgr = RecoveryManager::with_defaults();
        assert_eq!(mgr.state(), RecoveryState::Running);
        assert_eq!(mgr.crash_count(), 0);
        assert!(!mgr.should_restart());
    }

    #[test]
    fn test_crash_escalation() {
        let mut mgr = RecoveryManager::with_defaults();
        assert_eq!(mgr.state(), RecoveryState::Running);

        mgr.record_crash();
        assert_eq!(mgr.crash_count(), 1);
        assert_eq!(mgr.state(), RecoveryState::Crashed);
        assert!(mgr.next_restart.is_some());

        // Simulate backoff elapsed
        mgr.next_restart = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(mgr.should_restart());
    }

    #[test]
    fn test_backoff_calculation() {
        let mut mgr = RecoveryManager::with_defaults();

        mgr.crash_count = 1;
        assert_eq!(mgr.backoff_seconds(), 5);

        mgr.crash_count = 2;
        assert_eq!(mgr.backoff_seconds(), 10);

        mgr.crash_count = 3;
        assert_eq!(mgr.backoff_seconds(), 20);

        mgr.crash_count = 10;
        assert_eq!(mgr.backoff_seconds(), 300); // capped at max
    }

    #[test]
    fn test_stable_epochs_reset() {
        let mut mgr = RecoveryManager::with_defaults();
        mgr.crash_count = 3;

        for _ in 0..10 {
            mgr.record_stable_epoch();
        }

        assert_eq!(mgr.crash_count(), 0);
    }

    #[test]
    fn test_max_crashes() {
        let mut mgr = RecoveryManager::with_defaults();

        for _ in 0..10 {
            mgr.record_crash();
        }

        assert_eq!(mgr.state(), RecoveryState::Failed);
        assert!(!mgr.should_restart());
    }

    #[test]
    fn test_reset() {
        let mut mgr = RecoveryManager::with_defaults();
        mgr.record_crash();
        mgr.record_crash();
        mgr.reset();

        assert_eq!(mgr.crash_count(), 0);
        assert_eq!(mgr.state(), RecoveryState::Running);
    }

    #[tokio::test]
    async fn test_try_auto_restart() {
        let mut mgr = RecoveryManager::with_defaults();
        mgr.record_crash();

        // Not ready yet
        assert!(!mgr.try_auto_restart().await);

        // Simulate time passing
        mgr.next_restart = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(mgr.try_auto_restart().await);
        assert_eq!(mgr.state(), RecoveryState::Running);
    }
}