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

use crate::args::{build_args, AngryOxideConfig};
use crate::parser::{parse_status_line, watch_output_dir, AoEvent};
use crate::recovery::RecoveryManager;

type SharedChild = Arc<Mutex<Option<tokio::process::Child>>>;

/// Handle to a running AngryOxide process
pub struct AngryOxideHandle {
    child: SharedChild,
    event_rx: Arc<Mutex<mpsc::UnboundedReceiver<AoEvent>>>,
    event_tx: mpsc::UnboundedSender<AoEvent>,
    config: AngryOxideConfig,
    recovery: Arc<Mutex<RecoveryManager>>,
    shutdown_flag: Arc<Mutex<bool>>,
}

impl AngryOxideHandle {
    /// Receive the next parsed event from AngryOxide's stdout, or `None`
    /// once the process has shut down and no more events will arrive.
    ///
    /// Safe to call from a single consumer loop (e.g. inside a
    /// `tokio::select!`); the receiver is shared behind a mutex so it
    /// survives process restarts transparently.
    pub async fn recv_event(&self) -> Option<AoEvent> {
        self.event_rx.lock().await.recv().await
    }

    /// Check if AngryOxide is running
    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            c.try_wait()
                .unwrap_or(Some(std::process::ExitStatus::default()))
                .is_none()
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

    /// Force an immediate restart of AngryOxide, bypassing backoff.
    ///
    /// Unlike the automatic crash recovery (which waits out an exponential
    /// backoff before respawning), this is for a caller that already
    /// decided a restart is needed right now (e.g. the healing state
    /// machine). It kills the current process if any, then spawns a
    /// replacement and reattaches stdout/stderr readers and the crash
    /// monitor so events keep flowing through the same `recv_event`
    /// channel.
    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        sleep(Duration::from_secs(1)).await;

        *self.shutdown_flag.lock().await = false;
        spawn_process(
            &self.config,
            self.child.clone(),
            self.event_tx.clone(),
            self.shutdown_flag.clone(),
            self.recovery.clone(),
        )
        .await?;

        info!("AngryOxide restarted");
        Ok(())
    }
}

/// Spawn the AngryOxide child process and attach its stdout/stderr readers
/// plus a crash monitor. Used both for the initial launch and for restarts,
/// so every respawn keeps forwarding events into the same `event_tx`.
async fn spawn_process(
    config: &AngryOxideConfig,
    child_slot: SharedChild,
    event_tx: mpsc::UnboundedSender<AoEvent>,
    shutdown_flag: Arc<Mutex<bool>>,
    recovery: Arc<Mutex<RecoveryManager>>,
) -> Result<u32> {
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

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn AngryOxide: {}", binary))?;

    let pid = child.id().unwrap_or(0);
    info!("AngryOxide started with PID {}", pid);

    let stdout = child.stdout.take().context("Failed to take stdout")?;
    let stderr = child.stderr.take().context("Failed to take stderr")?;

    *child_slot.lock().await = Some(child);

    // Spawn stdout reader: best-effort parse of AO's headless status lines
    // (`{timestamp} | {type} | {content}`, ANSI-colored). This is purely
    // informational (see `parser` module docs) - the authoritative signal
    // for captures comes from the output-directory watcher below.
    let shutdown_stdout = shutdown_flag.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if *shutdown_stdout.lock().await {
                break;
            }

            match parse_status_line(&line) {
                Some(event) => {
                    if event_tx.send(event).is_err() {
                        break; // Receiver dropped
                    }
                }
                None => {
                    debug!("Unrecognized AO stdout line: {}", line);
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

    // Spawn crash monitor: watches for process exit and records it with
    // the recovery manager so the auto-restart loop can act on it.
    let child_crash = child_slot.clone();
    let recovery_crash = recovery.clone();
    let shutdown_crash = shutdown_flag.clone();

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

    Ok(pid)
}

/// Spawn AngryOxide subprocess with crash detection and stdout parsing
pub async fn spawn_angryoxide(config: &AngryOxideConfig) -> Result<AngryOxideHandle> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let recovery = Arc::new(Mutex::new(RecoveryManager::with_defaults()));
    let shutdown_flag = Arc::new(Mutex::new(false));
    let child: SharedChild = Arc::new(Mutex::new(None));

    spawn_process(
        config,
        child.clone(),
        event_tx.clone(),
        shutdown_flag.clone(),
        recovery.clone(),
    )
    .await?;

    // Watch AO's output directory (the parent of the `-o` prefix) for new
    // `.hc22000`/`.pcapng` files. This runs independently of the AO child
    // process's own lifecycle (it survives crash/restart cycles, since the
    // directory itself persists), and exits on its own once `event_tx`'s
    // receiver is dropped.
    if let Some(output) = &config.output {
        let watch_dir = output
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| output.clone());
        let watch_tx = event_tx.clone();
        tokio::spawn(async move {
            watch_output_dir(watch_dir, watch_tx).await;
        });
    } else {
        warn!("AngryOxide config has no output path set; capture-file watching is disabled");
    }

    // Auto-restart handler: whenever the recovery manager's backoff clears
    // after a crash, respawn the process (with fresh readers/monitor) so
    // the same event channel keeps delivering data to consumers.
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

            if recovery_restart.lock().await.should_restart()
                && recovery_restart.lock().await.try_auto_restart().await
            {
                info!("Attempting to restart AngryOxide...");
                if let Err(e) = spawn_process(
                    &config_restart,
                    child_restart.clone(),
                    event_tx_restart.clone(),
                    shutdown_restart.clone(),
                    recovery_restart.clone(),
                )
                .await
                {
                    error!("Failed to restart AngryOxide: {}", e);
                    recovery_restart.lock().await.record_crash();
                }
            }
        }
    });

    Ok(AngryOxideHandle {
        child,
        event_rx: Arc::new(Mutex::new(event_rx)),
        event_tx,
        config: config.clone(),
        recovery,
        shutdown_flag,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_spawn_structure() {
        // Verify module structure compiles
    }
}
