use anyhow::{Context, Result};
use crate::parser::{parse_ao_line, AoEvent};
use crate::recovery::{BackoffRecovery, HealthStatus, check_process_health};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Managed AngryOxide process
pub struct AngryOxideProcess {
    child: Option<Child>,
    config: Arc<crate::args::AoConfig>,
    recovery: BackoffRecovery,
    event_tx: mpsc::Sender<AoEvent>,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl AngryOxideProcess {
    /// Create new managed process
    pub fn new(
        config: Arc<crate::args::AoConfig>,
        event_tx: mpsc::Sender<AoEvent>,
    ) -> Self {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        
        Self {
            child: None,
            config,
            recovery: BackoffRecovery::new(),
            event_tx,
            shutdown_tx,
        }
    }

    /// Start the process
    pub async fn start(&mut self) -> Result<()> {
        if self.child.is_some() {
            return Ok(()); // Already running
        }

        let args = crate::args::build_args(&self.config)?;
        let ao_binary = self.find_ao_binary()?;

        info!("Starting AngryOxide: {} {:?}", ao_binary.display(), args);

        let mut cmd = Command::new(&ao_binary);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()
            .context("Failed to spawn AngryOxide process")?;

        let pid = child.id().unwrap_or(0);
        info!("AngryOxide started with PID {}", pid);

        // Take stdout/stderr
        let stdout = child.stdout.take()
            .context("Failed to capture stdout")?;
        let stderr = child.stderr.take()
            .context("Failed to capture stderr")?;

        self.recovery.record_start();
        
        // Spawn stdout reader
        let event_tx = self.event_tx.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => break,
                    line = reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                if let Err(e) = Self::handle_stdout_line(&line, &event_tx).await {
                                    debug!("Stdout parse error: {}", e);
                                }
                            }
                            Ok(None) => break, // EOF
                            Err(e) => {
                                error!("Stdout read error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Spawn stderr reader (just log)
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if !line.trim().is_empty() {
                    debug!("AO stderr: {}", line);
                }
            }
        });

        self.child = Some(child);
        Ok(())
    }

    /// Find AngryOxide binary
    fn find_ao_binary(&self) -> Result<std::path::PathBuf> {
        let candidates = [
            "/usr/local/bin/angryoxide",
            "/usr/bin/angryoxide",
            "/opt/angryoxide/angryoxide",
            "./angryoxide",
        ];

        for path in candidates {
            let p = std::path::Path::new(path);
            if p.exists() {
                return Ok(p.to_path_buf());
            }
        }

        // Try PATH
        if let Ok(paths) = std::env::var("PATH") {
            for dir in std::env::split_paths(&paths) {
                let p = dir.join("angryoxide");
                if p.exists() {
                    return Ok(p);
                }
            }
        }

        anyhow::bail!("AngryOxide binary not found. Install it or specify path in config.");
    }

    /// Handle a line from stdout
    async fn handle_stdout_line(line: &str, event_tx: &mpsc::Sender<AoEvent>) -> Result<()> {
        if line.trim().is_empty() {
            return Ok(());
        }

        // Parse JSON line
        match parse_ao_line(line) {
            Ok(event) => {
                event_tx.send(event).await
                    .context("Failed to send event")?;
            }
            Err(e) => {
                debug!("Failed to parse AO line: {} - {}", line, e);
            }
        }
        Ok(())
    }

    /// Monitor process health and restart if needed
    pub async fn monitor(&mut self) -> Result<()> {
        let mut check_interval = interval(Duration::from_secs(10));
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => break,
                _ = check_interval.tick() => {
                    let is_running = self.is_running().await;
                    let health = check_process_health(&self.recovery, is_running);

                    match health {
                        HealthStatus::Healthy => {
                            self.recovery.record_alive();
                        }
                        HealthStatus::Stalled => {
                            warn!("AngryOxide appears stalled, restarting");
                            self.restart().await?;
                        }
                        HealthStatus::Dead => {
                            warn!("AngryOxide process dead, restarting");
                            self.restart().await?;
                        }
                        HealthStatus::Recovering => {
                            debug!("AngryOxide recovering...");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if process is still running
    pub async fn is_running(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Process exited
                    self.child = None;
                    false
                }
                Ok(None) => true, // Still running
                Err(_) => false,  // Error checking
            }
        } else {
            false
        }
    }

    /// Restart the process with backoff
    async fn restart(&mut self) -> Result<()> {
        if !self.recovery.should_retry() {
            anyhow::bail!("Max restart attempts reached");
        }

        // Kill existing process if any
        self.kill().await?;

        // Wait for backoff delay
        let delay = self.recovery.next_delay();
        warn!("Waiting {:?} before restart (attempt {})", delay, self.recovery.attempt_count() + 1);
        tokio::time::sleep(delay).await;

        // Record restart and start again
        self.recovery.record_restart();
        self.start().await?;

        info!("AngryOxide restarted successfully");
        Ok(())
    }

    /// Kill the process
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            info!("Killing AngryOxide process");
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }

    /// Shutdown gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        let _ = self.shutdown_tx.send(true);
        self.kill().await
    }

    /// Get current PID
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|c| c.id())
    }

    /// Get recovery stats
    pub fn recovery_stats(&self) -> (u32, Duration) {
        (self.recovery.attempt_count(), self.recovery.next_delay())
    }
}

/// High-level manager that handles process lifecycle
pub struct AngryOxideManager {
    process: Arc<Mutex<AngryOxideProcess>>,
}

impl AngryOxideManager {
    /// Create new manager
    pub fn new(config: Arc<crate::args::AoConfig>) -> (Self, mpsc::Receiver<AoEvent>) {
        let (event_tx, event_rx) = mpsc::channel(100);
        let process = AngryOxideProcess::new(config, event_tx);
        
        let manager = Self {
            process: Arc::new(Mutex::new(process)),
        };

        (manager, event_rx)
    }

    /// Start the process
    pub async fn start(&self) -> Result<()> {
        let mut process = self.process.lock().await;
        process.start().await
    }

    /// Run monitor loop (call in background task)
    pub async fn monitor(&self) -> Result<()> {
        let mut process = self.process.lock().await;
        process.monitor().await
    }

    /// Shutdown
    pub async fn shutdown(&self) -> Result<()> {
        let mut process = self.process.lock().await;
        process.shutdown().await
    }

    /// Get current PID
    pub async fn pid(&self) -> Option<u32> {
        let process = self.process.lock().await;
        process.pid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_create_manager() {
        let config = Arc::new(crate::args::AoConfig::default());
        let (manager, _rx) = AngryOxideManager::new(config);
        
        assert_eq!(manager.pid().await, None);
    }
}