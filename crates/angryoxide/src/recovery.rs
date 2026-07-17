use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Exponential backoff recovery state
#[derive(Debug, Clone)]
pub struct BackoffRecovery {
    attempt: u32,
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    last_restart: Option<Instant>,
    last_heartbeat: Option<Instant>,
    stall_threshold: Duration,
    consecutive_failures: u32,
}

impl BackoffRecovery {
    /// Create new recovery state
    pub fn new() -> Self {
        Self {
            attempt: 0,
            max_attempts: 10,
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300), // 5 minutes
            last_restart: None,
            last_heartbeat: None,
            stall_threshold: Duration::from_secs(60), // No output for 60s = stall
            consecutive_failures: 0,
        }
    }

    /// Create with custom parameters
    pub fn with_params(
        max_attempts: u32,
        base_delay: Duration,
        max_delay: Duration,
        stall_threshold: Duration,
    ) -> Self {
        Self {
            max_attempts,
            base_delay,
            max_delay,
            stall_threshold,
            ..Self::new()
        }
    }

    /// Record process start
    pub fn record_start(&mut self) {
        self.last_restart = Some(Instant::now());
        self.last_heartbeat = Some(Instant::now());
        self.attempt = 0;
        self.consecutive_failures = 0;
    }

    /// Record that process is alive (heartbeat)
    pub fn record_alive(&mut self) {
        self.last_heartbeat = Some(Instant::now());
        self.consecutive_failures = 0;
    }

    /// Record process restart
    pub fn record_restart(&mut self) {
        self.attempt = self.attempt.saturating_add(1);
        self.last_restart = Some(Instant::now());
        self.last_heartbeat = Some(Instant::now());
    }

    /// Record a failure
    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Check if we should retry
    pub fn should_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Get current attempt count
    pub fn attempt_count(&self) -> u32 {
        self.attempt
    }

    /// Calculate next delay (exponential backoff with jitter)
    pub fn next_delay(&self) -> Duration {
        if self.attempt == 0 {
            return self.base_delay;
        }

        // Exponential backoff: base * 2^attempt
        let exp_delay = self.base_delay * 2_u32.pow(self.attempt.min(8));
        
        // Cap at max_delay
        let capped = exp_delay.min(self.max_delay);
        
        // Add jitter (±25%)
        let jitter_range = capped.as_millis() as f64 * 0.25;
        let jitter = simple_jitter(jitter_range);
        
        Duration::from_millis(
            (capped.as_millis() as f64 + jitter).max(0.0) as u64
        )
    }

    /// Check if process appears stalled
    pub fn is_stalled(&self) -> bool {
        if let Some(last) = self.last_heartbeat {
            last.elapsed() > self.stall_threshold
        } else {
            false
        }
    }

    /// Get time since last heartbeat
    pub fn time_since_heartbeat(&self) -> Option<Duration> {
        self.last_heartbeat.map(|h| h.elapsed())
    }

    /// Get time since last restart
    pub fn time_since_restart(&self) -> Option<Duration> {
        self.last_restart.map(|r| r.elapsed())
    }

    /// Reset recovery state
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.consecutive_failures = 0;
        self.last_restart = None;
        self.last_heartbeat = None;
    }
}

impl Default for BackoffRecovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Process health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Running normally
    Healthy,
    /// No output for too long
    Stalled,
    /// Process has exited
    Dead,
    /// Currently in backoff/restart
    Recovering,
}

/// Check process health based on running state and recovery state
pub fn check_process_health(recovery: &BackoffRecovery, is_running: bool) -> HealthStatus {
    if !is_running {
        HealthStatus::Dead
    } else if recovery.is_stalled() {
        HealthStatus::Stalled
    } else if recovery.attempt > 0 && recovery.time_since_restart().map_or(false, |d| d < Duration::from_secs(30)) {
        HealthStatus::Recovering
    } else {
        HealthStatus::Healthy
    }
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Stall threshold (no output for this long = stalled)
    pub stall_threshold: Duration,
    /// Maximum restart attempts
    pub max_attempts: u32,
    /// Base backoff delay
    pub base_delay: Duration,
    /// Maximum backoff delay
    pub max_delay: Duration,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(10),
            stall_threshold: Duration::from_secs(60),
            max_attempts: 10,
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(300),
        }
    }
}

/// Health check runner
#[allow(dead_code)]
pub struct HealthChecker {
    config: HealthCheckConfig,
    recovery: BackoffRecovery,
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(HealthCheckConfig::default())
    }
}

impl HealthChecker {
    pub fn new(config: HealthCheckConfig) -> Self {
        let recovery = BackoffRecovery::with_params(
            config.max_attempts,
            config.base_delay,
            config.max_delay,
            config.stall_threshold,
        );
        Self { config, recovery }
    }

    /// Check process and return health status
    pub fn check(&mut self, is_running: bool) -> HealthStatus {
        check_process_health(&self.recovery, is_running)
    }

    /// Record heartbeat
    pub fn heartbeat(&mut self) {
        self.recovery.record_alive();
    }

    /// Get recovery state
    pub fn recovery(&self) -> &BackoffRecovery {
        &self.recovery
    }

    /// Get mutable recovery state
    pub fn recovery_mut(&mut self) -> &mut BackoffRecovery {
        &mut self.recovery
    }

    /// Check if should restart
    pub fn should_restart(&self, is_running: bool) -> bool {
        !is_running && self.recovery.should_retry()
    }

    /// Get next restart delay
    pub fn restart_delay(&self) -> Duration {
        self.recovery.next_delay()
    }
}

/// Simple deterministic jitter using time-based pseudo-random
fn simple_jitter(range: f64) -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let r = (nanos.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fffffff) as f64;
    let normalized = r / 0x7fffffff as f64;
    (normalized * 2.0 - 1.0) * range
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_backoff_delays() {
        let mut recovery = BackoffRecovery::new();
        
        // First attempt
        assert_eq!(recovery.next_delay(), Duration::from_secs(5));
        
        // After restart
        recovery.record_restart();
        let delay1 = recovery.next_delay();
        assert!(delay1 >= Duration::from_secs(7));
        
        // Second restart: base*2^2 = 20s, ±25% jitter → min 15s
        recovery.record_restart();
        let delay2 = recovery.next_delay();
        assert!(delay2 >= Duration::from_secs(15));
    }

    #[test]
    fn test_max_attempts() {
        let mut recovery = BackoffRecovery::with_params(3, Duration::from_secs(1), Duration::from_secs(10), Duration::from_secs(60));
        
        for i in 0..3 {
            assert!(recovery.should_retry(), "Should retry at attempt {}", i);
            recovery.record_restart();
        }
        
        assert!(!recovery.should_retry(), "Should not retry after max attempts");
    }

    #[test]
    fn test_stall_detection() {
        let mut recovery = BackoffRecovery::with_params(
            10, 
            Duration::from_secs(1), 
            Duration::from_secs(10), 
            Duration::from_millis(100)
        );
        
        recovery.record_start();
        assert!(!recovery.is_stalled());
    }

    #[test]
    fn test_health_check() {
        let mut checker = HealthChecker::default();
        
        assert_eq!(checker.check(false), HealthStatus::Dead);
        
        checker.recovery_mut().record_start();
        assert_eq!(checker.check(true), HealthStatus::Healthy);
    }
}