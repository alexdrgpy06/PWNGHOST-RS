use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "pwnagotchi-rs", about = "Self-driving WiFi penetration testing tool")]
struct Args {
    #[arg(short, long, default_value = "/etc/pwnagotchi/config.toml")]
    config: String,

    #[arg(long)]
    mock: bool,

    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // Load configuration — warn on failure, continue with defaults
    let _pwn_config = match config::schema::load_config(&args.config) {
        Ok(cfg) => {
            tracing::info!("Loaded config from {}", args.config);
            Some(cfg)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to load config from {} ({}); using defaults",
                args.config,
                e
            );
            None
        }
    };

    // Create agent with default personality
    let personality = pwncore::personality::Personality::default();
    let mut agent = agent::agent::Agent::new(personality);

    // Create radio manager
    let iface = "wlan0mon";
    let mut _radio = radio::manager::RadioManager::new(iface.to_string());

    // Start agent
    agent.start();
    tracing::info!("Agent started");

    // Set up AngryOxide (or mock)
    let ao_manager: Option<Arc<angryoxide::AngryOxideManager>>;
    let mut event_rx: mpsc::Receiver<angryoxide::parser::AoEvent>;
    let mut monitor_handle: Option<tokio::task::JoinHandle<()>> = None;

    if args.mock {
        tracing::info!("Running in mock mode (no AngryOxide process)");
        let config = Arc::new(angryoxide::AoConfig::default());
        let (mgr, rx) = angryoxide::AngryOxideManager::new(config);
        ao_manager = Some(Arc::new(mgr));
        event_rx = rx;
    } else {
        tracing::info!("Starting AngryOxide on {}", iface);
        let config = Arc::new(angryoxide::AoConfig {
            interface: iface.to_string(),
            ..Default::default()
        });
        let (mgr, rx) = angryoxide::AngryOxideManager::new(config);
        let mgr_arc = Arc::new(mgr);

        mgr_arc.start().await.context("Failed to start AngryOxide")?;

        let mgr_for_monitor = mgr_arc.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = mgr_for_monitor.monitor().await {
                let msg = e.to_string();
                if !msg.contains("Max restart attempts") {
                    tracing::error!("AngryOxide monitor error: {}", e);
                }
            }
        });

        ao_manager = Some(mgr_arc);
        event_rx = rx;
        monitor_handle = Some(handle);
    }

    // Signal handling
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown requested (Ctrl-C)");
        let _ = shutdown_tx.send(true);
    });

    // Epoch tick interval
    let mut tick_interval = tokio::time::interval(std::time::Duration::from_secs(30));
    let mut epoch_events: Vec<angryoxide::parser::AoEvent> = Vec::new();

    // Main event loop
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                tracing::info!("Shutting down...");
                agent.stop();
                break;
            }
            _ = tick_interval.tick() => {
                // Flush accumulated events
                if !epoch_events.is_empty() {
                    agent.handle_events(&epoch_events);
                    epoch_events.clear();
                }

                // Heartbeat
                agent.report_alive();

                // Tick
                let (face, action) = agent.tick();
                tracing::info!(
                    "Epoch {} | Channel {} | Mood {:?} | Face {} | Action {:?}",
                    agent.total_epochs(),
                    agent.current_channel(),
                    agent.current_mood(),
                    face,
                    action,
                );

                // Check healer
                let healing = agent.check_healing();
                match healing {
                    agent::healing::HealingAction::None => {}
                    other => tracing::warn!("Healing action: {:?}", other),
                }
            }
            recv = event_rx.recv() => {
                match recv {
                    Some(event) => epoch_events.push(event),
                    None => {
                        tracing::warn!("AngryOxide event channel closed");
                        // If the channel closed unexpectedly, keep running on ticks
                    }
                }
            }
        }
    }

    // Graceful shutdown
    if let Some(handle) = monitor_handle.take() {
        handle.abort();
    }
    if let Some(mgr) = ao_manager {
        if let Err(e) = mgr.shutdown().await {
            tracing::error!("AngryOxide shutdown error: {}", e);
        }
    }

    tracing::info!("Goodbye");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parse() {
        let args =
            Args::try_parse_from(["pwnagotchi-rs", "--config", "/tmp/test.toml", "--mock"])
                .unwrap();
        assert_eq!(args.config, "/tmp/test.toml");
        assert!(args.mock);
    }

    #[test]
    fn test_args_defaults() {
        let args = Args::try_parse_from(["pwnagotchi-rs"]).unwrap();
        assert_eq!(args.config, "/etc/pwnagotchi/config.toml");
        assert!(!args.mock);
        assert_eq!(args.port, 8080);
    }

    #[test]
    fn test_args_with_port() {
        let args =
            Args::try_parse_from(["pwnagotchi-rs", "--port", "9090"]).unwrap();
        assert_eq!(args.port, 9090);
        assert!(!args.mock);
    }
}
