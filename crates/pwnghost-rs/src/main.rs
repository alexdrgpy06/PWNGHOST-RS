//! PWNGHOST-RS Main Binary

use agent::Agent;
use angryoxide::init as init_angryoxide;
use clap::Parser;
use config::load_config;
use fw_patcher::apply_on_first_boot;
use radio::RadioManager;
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use ui::display::Display;
use ui::web::WebServer;

#[derive(Parser, Debug)]
#[command(
    name = "pwnghost-rs",
    version,
    about = "PWNGHOST-RS - Rust Pwnagotchi Implementation"
)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "/etc/pwnghost/config.toml")]
    config: PathBuf,

    /// Run in mock mode (no hardware)
    #[arg(long)]
    mock: bool,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Run firmware patcher on first boot
    #[arg(long)]
    patch_firmware: bool,

    /// Interface to use
    #[arg(short, long)]
    interface: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&args.log_level))
        .init();

    info!("Starting PWNGHOST-RS v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let mut config = load_config(&args.config).await?;
    if let Some(iface) = args.interface {
        config.main.iface = iface;
    }

    // Apply firmware patches on first boot
    if args.patch_firmware {
        info!("Applying firmware patches...");
        apply_on_first_boot(&PathBuf::from("/lib/firmware/brcm")).await?;
    }

    // Initialize firmware monitor
    // fw_patcher::run_monitor_task().await;

    // Initialize AngryOxide
    let ao_config = angryoxide::args::AngryOxideConfig::default();
    let ao_handle = init_angryoxide(&ao_config).await?;

    // Initialize radio manager
    let mut radio = RadioManager::new(config.main.iface.clone());

    // Initialize display if enabled
    let display = if config.ui.display.enabled {
        let disp_cfg = ui::display::DisplayConfig {
            rotation: match config.ui.display.rotation {
                90 => ui::display::DisplayRotation::Rotate90,
                180 => ui::display::DisplayRotation::Rotate180,
                270 => ui::display::DisplayRotation::Rotate270,
                _ => ui::display::DisplayRotation::Rotate0,
            },
            display_type: ui::display::DisplayType::from_config_str(
                &config.ui.display.display_type,
            ),
            ..Default::default()
        };
        Some(Display::new(disp_cfg)?)
    } else {
        None
    };

    // Initialize web server if enabled
    let web_server = if config.ui.web.enabled {
        Some(WebServer::new(ui::web::WebConfig::default()))
    } else {
        None
    };

    // Create agent
    let mut agent = Agent::new(agent::Personality::new(config.personality.clone().into()));

    // Start web server in background
    if let Some(web) = web_server {
        let addr: std::net::SocketAddr =
            format!("{}:{}", config.ui.web.address, config.ui.web.port).parse()?;
        tokio::spawn(async move {
            if let Err(e) = web.serve(addr).await {
                error!("Web server error: {}", e);
            }
        });
    }

    // Initialize display
    if let Some(ref d) = display {
        d.init().await?;
    }

    // Main loop
    info!("Entering main loop");
    agent.start();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
        config.oxigotchi.epoch_duration as u64,
    ));

    // Once the AngryOxide event channel closes for good, stop selecting on
    // it (recv() on a closed channel resolves immediately, which would
    // otherwise busy-loop that branch and starve the timer tick below).
    let mut ao_events_open = true;

    loop {
        tokio::select! {
            event = ao_handle.recv_event(), if ao_events_open => {
                match event {
                    Some(event) => agent.handle_event(&event),
                    None => {
                        warn!("AngryOxide event channel closed");
                        ao_events_open = false;
                    }
                }
            }
            _ = interval.tick() => {
                // Tick agent
                let (face, action) = agent.tick();

                // Update display with the current frame
                if let Some(ref d) = display {
                    let uptime = format!("{}s", agent.start.elapsed().as_secs());
                    let mode = format!("{:?}", agent.current_mood());
                    if let Err(e) = d
                        .draw_pwnagotchi_frame(
                            agent.current_channel(),
                            0,
                            false,
                            &uptime,
                            &config.main.name,
                            "",
                            face,
                            agent.epoch_tracker.current.handshakes_this_epoch,
                            0,
                            &mode,
                            None,
                            0,
                            0,
                        )
                        .await
                    {
                        warn!("Display draw failed: {}", e);
                    } else if let Err(e) = d.update(true).await {
                        warn!("Display update failed: {}", e);
                    }
                }

                // Execute action
                match action {
                    agent::AgentAction::Hop(ch) => {
                        info!("Hopping to channel {}", ch);
                        let _ = radio.switch_to(radio::RadioMode::Rage, None, None, None, None).await;
                    }
                    agent::AgentAction::Deauth(bssid) => {
                        info!("Deauthing {}", bssid);
                        // Send deauth via AO
                    }
                    agent::AgentAction::Associate(bssid) => {
                        info!("Associating with {}", bssid);
                    }
                    agent::AgentAction::Sleep(secs) => {
                        info!("Sleeping for {}s", secs);
                        tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                    }
                    agent::AgentAction::Stay => {}
                    agent::AgentAction::Wait => {}
                }

                // Check healing
                let healing_action = agent.check_healing();
                match healing_action {
                    agent::HealingAction::RestartAo => {
                        warn!("Healer: Restarting AngryOxide");
                        if let Err(e) = ao_handle.restart().await {
                            error!("Failed to restart AngryOxide: {}", e);
                        }
                    }
                    agent::HealingAction::PowerCycleGpio => {
                        error!("Healer: Power cycling WiFi chip");
                        // fw_patcher::power_cycle_wl_reg_on().await?;
                    }
                    agent::HealingAction::EnterSafeMode => {
                        error!("Healer: Entering safe mode");
                        let _ = radio.switch_to(radio::RadioMode::Safe, None, None, Some("fallback"), Some("fallback")).await;
                    }
                    agent::HealingAction::EnableUsbLifeline => {
                        error!("Healer: Enabling USB lifeline");
                    }
                    agent::HealingAction::None => {}
                }
            }
            _ = signal::ctrl_c() => {
                info!("Shutdown signal received");
                break;
            }
        }
    }

    // Shutdown
    info!("Shutting down...");
    agent.stop();
    ao_handle.stop().await?;

    if let Some(d) = display {
        d.show_shutdown().await?;
        d.sleep().await?;
    }

    info!("PWNGHOST-RS stopped");
    Ok(())
}
