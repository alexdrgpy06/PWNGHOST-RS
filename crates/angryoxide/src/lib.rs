//! AngryOxide integration crate
//! 
//! Provides managed subprocess execution, JSON event parsing,
//! and automatic recovery for AngryOxide WiFi attack tool.

pub mod args;
pub mod parser;
pub mod recovery;
pub mod spawn;

pub use args::{AoConfig, build_args, build_personality_args, validate_config};
pub use parser::{AoEvent, parse_ao_line};
pub use recovery::{BackoffRecovery, HealthChecker, HealthCheckConfig, HealthStatus};
pub use spawn::{AngryOxideManager, AngryOxideProcess};

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Quick start: create a managed AngryOxide instance with default config
pub async fn quick_start() -> Result<(AngryOxideManager, mpsc::Receiver<AoEvent>)> {
    let config = Arc::new(AoConfig::default());
    Ok(AngryOxideManager::new(config))
}

/// Quick start with custom config
pub async fn quick_start_with(config: AoConfig) -> Result<(AngryOxideManager, mpsc::Receiver<AoEvent>)> {
    let config = Arc::new(config);
    Ok(AngryOxideManager::new(config))
}

/// Run AngryOxide and process events with a callback
pub async fn run_with_callback<F, Fut>(
    config: AoConfig,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(AoEvent) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let config = Arc::new(config);
    let (manager, mut events) = AngryOxideManager::new(config);
    let manager = Arc::new(manager);
    
    manager.start().await?;
    
    // Spawn monitor (Arc clone for the spawned task)
    let mgr = manager.clone();
    let monitor_handle = tokio::spawn(async move {
        if let Err(e) = mgr.monitor().await {
            tracing::error!("Monitor error: {}", e);
        }
    });

    // Process events
    while let Some(event) = events.recv().await {
        if let Err(e) = callback(event).await {
            tracing::error!("Event callback error: {}", e);
        }
    }

    manager.shutdown().await?;
    monitor_handle.abort();
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ao_config_default() {
        let config = AoConfig::default();
        assert_eq!(config.interface, "wlan0mon");
        assert_eq!(config.rate, 2);
        assert!(config.headless);
    }

    #[test]
    fn test_validate_config_ok() {
        let config = AoConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_fail() {
        let mut config = AoConfig::default();
        config.rate = 5;
        assert!(validate_config(&config).is_err());
    }
}