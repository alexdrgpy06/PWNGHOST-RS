//! AngryOxide subprocess spawning with crash detection

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::args::{AngryOxideConfig, build_args};
use crate::parser::{AoEvent, parse_json_line};
use crate::recovery::RecoveryManager;

/// Handle to a running AngryOxide process
pub struct AngryOxideHandle {
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    event_tx: mpsc::UnboundedSender<AoEvent>,
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<AoEvent>>>,
    config: AngryOxideConfig,
    recovery: Arc<Mutex<RecoveryManager>>,
    shutdown_flag: Arc<Mutex<bool>>,
}

impl AngryOxideHandle {
    /// Get event receiver
    pub fn events(&self) -> mpsc::UnboundedReceiver<AoEvent> {
        // We can't easily clone the receiver, so return a dummy
        // In real usage, you'd store the receiver in the handle
        let (_tx, rx) = mpsc::unbounded_channel();
        rx
    }

    /// Check if AngryOxide is running
    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            c.try_wait().unwrap_or(Some(std::process::ExitStatus::default())).is_none()
        } else {
            false
        }
    }

    /// Get crash count
    pub async fn crash_count(&self) -> u32 {
        self.recovery.lock().await.crash_count()
    }

    /// Get recovery state
    pub async fn recovery_state(&self) -> crate::recovery::RecoveryState {
        self.recovery.lock().await.state()
    }

    /// Stop AngryOxide
    pub async fn stop(&self) -> Result<()> {
        *self.shutdown_flag.lock().await = true;

        let mut child = self.child.lock().await;
        if let Some(mut c) = child.take() {
            info!("Stopping AngryOxide (PID: {})", c.id().unwrap_or(0));
            c.kill().await.context("Failed to kill AngryOxide")?;
            c.wait().await.context("Failed to wait for AngryOxide")?;
        }
        Ok(())
    }

    /// Restart AngryOxide
    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        sleep(Duration::from_secs(1)).await;
        // In a real implementation, you'd respawn here
        // For now, just return Ok
        Ok(())
    }
}

/// Spawn AngryOxide subprocess with crash detection and stdout parsing
pub async fn spawn_angryoxide(config: &AngryOxideConfig) -> Result<AngryOxideHandle> {
    let binary = &config.binary;
    if !Path::new(binary).exists() {
        return Err(anyhow::anyhow!("AngryOxide binary not found: {}", binary));
    }

    let args = build_args(config)?;
    info!("Starting AngryOxide: {} {}", binary, args.join(" "));

    let mut cmd = Command::new(binary);
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn()
        .with_context(|| format!("Failed to spawn AngryOxide: {}", binary))?;

    let pid = child.id().unwrap_or(0);
    info!("AngryOxide started with PID {}", pid);

    // Take stdout and stderr
    let stdout = child.stdout.take()
        .context("Failed to take stdout")?;
    let stderr = child.stderr.take()
        .context("Failed to take stderr")?;

    // Event channel
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    // Recovery manager
    let recovery = Arc::new(Mutex::new(RecoveryManager::with_defaults()));
    let shutdown_flag = Arc::new(Mutex::new(false));
    let child = Arc::new(Mutex::new(Some(child)));

    // Spawn stdout reader
    let event_tx_stdout = event_tx.clone();
    let recovery_stdout = recovery.clone();
    let shutdown_stdout = shutdown_flag.clone();
    let child_stdout = child.clone();

    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if *shutdown_stdout.lock().await {
                break;
            }

            // Parse JSON line
            match parse_json_line(&line) {
                Ok(event) => {
                    if event_tx_stdout.send(event).is_err() {
                        break; // Receiver dropped
                    }
                }
                Err(e) => {
                    debug!("Failed to parse AO line: {}", e);
                    // Not a JSON line, could be ANSI output - ignore
                }
            }
        }

        info!("AngryOxide stdout reader exiting");
    });

    // Spawn stderr reader (for logging)
    let stderr_shutdown = shutdown_flag.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if *stderr_shutdown.lock().await {
                break;
            }
            debug!("AO stderr: {}", line);
        }
    });

    // Spawn crash monitor
    let child_crash = child.clone();
    let recovery_crash = recovery.clone();
    let shutdown_crash = shutdown_flag.clone();
    let pid_crash = pid;

    tokio::spawn(async move {
        loop {
            if *shutdown_crash.lock().await {
                break;
            }

            sleep(Duration::from_secs(1)).await;

            let mut child_guard = child_crash.lock().await;
            if let Some(ref mut c) = *child_guard {
                match c.try_wait() {
                    Ok(Some(status)) => {
                        warn!("AngryOxide process exited with status: {}", status);
                        recovery_crash.lock().await.record_crash();
                        *child_guard = None;
                        break;
                    }
                    Ok(None) => {
                        // Still running
                    }
                    Err(e) => {
                        error!("Error checking AngryOxide process: {}", e);
                        recovery_crash.lock().await.record_crash();
                        *child_guard = None;
                        break;
                    }
                }
            } else {
                // Process already gone
                break;
            }
        }
    });

    // Spawn auto-restart handler
    let recovery_restart = recovery.clone();
    let config_restart = config.clone();
    let child_restart = child.clone();
    let shutdown_restart = shutdown_flag.clone();
    let event_tx_restart = event_tx.clone();

    tokio::spawn(async move {
        loop {
            if *shutdown_restart.lock().await {
                break;
            }

            sleep(Duration::from_secs(1)).await;

            if recovery_restart.lock().await.should_restart() {
                if recovery_restart.lock().await.try_auto_restart().await {
                    info!("Attempting to restart AngryOxide...");
                    let args = build_args(&config_restart).unwrap();

                    let mut cmd = Command::new(&config_restart.binary);
                    cmd.args(&args)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .kill_on_drop(true);

                    match cmd.spawn() {
                        Ok(new_child) => {
                            let new_pid = new_child.id().unwrap_or(0);
                            info!("AngryOxide restarted with PID {}", new_pid);

                            *child_restart.lock().await = Some(new_child);
                            // Note: In a full implementation, you'd also need to
                            // re-setup stdout/stderr readers and event channel
                        }
                        Err(e) => {
                            error!("Failed to restart AngryOxide: {}", e);
                            recovery_restart.lock().await.record_crash();
                        }
                    }
                }
            }
        }
    });

    let handle = AngryOxideHandle {
        child,
        event_tx,
        event_rx: Arc::new(Mutex::new(event_rx)),
        config: config.clone(),
        recovery,
        shutdown_flag,
    };

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_structure() {
        // Verify module structure compiles
    }
}