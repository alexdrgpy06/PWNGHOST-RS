//! PWNGHOST-RS Main Binary

use agent::Agent;
use angryoxide::init as init_angryoxide;
use clap::Parser;
use config::load_config;
use fw_patcher::apply_on_first_boot;
use radio::RadioManager;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use ui::display::Display;
use ui::web::WebServer;

/// Minimal `sd_notify(3)` client.
///
/// systemd `Type=notify` units expect the service to `sendto` a datagram
/// on the `AF_UNIX` socket named in `$NOTIFY_SOCKET` once startup is
/// complete (`READY=1`), and optionally ping a watchdog (`WATCHDOG=1`) if
/// the unit sets `WatchdogSec=`. This is the entire protocol, so it's
/// hand-rolled here rather than pulling in the `sd-notify` crate for a
/// single `sendto` call.
///
/// The real target is always Linux (Raspberry Pi), but this crate is
/// sometimes type-checked from non-Linux dev machines, so the actual
/// `AF_UNIX` datagram logic is `#[cfg(unix)]`-gated with a harmless no-op
/// fallback elsewhere rather than making the whole binary fail to compile
/// off-target.
mod sd_notify {
    #[cfg(unix)]
    pub fn notify(state: &str) -> std::io::Result<()> {
        use std::os::unix::net::UnixDatagram;

        let Ok(socket_path) = std::env::var("NOTIFY_SOCKET") else {
            return Ok(());
        };
        if socket_path.is_empty() {
            return Ok(());
        }

        let socket = UnixDatagram::unbound()?;
        socket.send_to(state.as_bytes(), &socket_path)?;
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn notify(_state: &str) -> std::io::Result<()> {
        Ok(())
    }

    /// Tell systemd the service finished starting up.
    pub fn ready() -> std::io::Result<()> {
        notify("READY=1")
    }

    /// Ping the systemd watchdog. Harmless no-op if the unit doesn't set
    /// `WatchdogSec=`.
    pub fn watchdog() -> std::io::Result<()> {
        notify("WATCHDOG=1")
    }
}

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

/// Read the MAC address of `iface` from sysfs, for use as our mesh identity.
/// Returns the zero address if it can't be read (interface not up yet,
/// non-Linux dev environment, etc.) - mesh IE encoding still works, it just
/// advertises an all-zero MAC until the interface is available.
fn read_iface_mac(iface: &str) -> pwncore::MacAddr {
    std::fs::read_to_string(format!("/sys/class/net/{iface}/address"))
        .ok()
        .and_then(|s| pwncore::MacAddr::from_str(s.trim()).ok())
        .unwrap_or_default()
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

    // Capture pipeline directories: AngryOxide writes its raw output
    // (pcapng/hc22000/kismetdb/tarball) directly into the staging dir via
    // `-o`; `CaptureManager` validates candidates with `hcxpcapngtool` and
    // moves confirmed handshakes into the final handshakes directory.
    let staging_dir = PathBuf::from("/var/tmp/pwnghost/ao-output");
    let handshakes_dir = config.main.handshakes_dir();

    // Initialize AngryOxide. The interface comes from config (`main.iface`)
    // rather than a hardcoded `wlan0mon` - AO manages monitor mode itself via
    // netlink, so it must be handed a plain interface name. The output
    // prefix points into the capture staging dir so the filesystem watcher
    // and the capture pipeline agree on where files show up.
    let ao_config = angryoxide::args::AngryOxideConfig {
        interface: config.main.iface.clone(),
        output: Some(staging_dir.join("session")),
        ..angryoxide::args::AngryOxideConfig::default()
    };
    let ao_handle = init_angryoxide(&ao_config).await?;

    let capture_manager = agent::CaptureManager::new(staging_dir.clone(), handshakes_dir);
    if let Err(e) = capture_manager.init().await {
        warn!("Failed to initialize capture manager directories: {}", e);
    }

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
    // Keep a handle to the shared app state before `web_server` is consumed
    // by `serve()` below, so the tick loop can push live updates to it.
    let web_state = web_server.as_ref().map(|w| w.state());

    // Create agent
    let mut agent = Agent::new(agent::Personality::new(config.personality.clone().into()));
    agent.capture_manager = Some(Arc::new(capture_manager));

    // Load Lua plugins: built-ins (embedded via `include_str!`) plus any
    // user plugins found under `config.main.custom_plugins`.
    match agent::PluginManager::load(&config).await {
        Ok(mgr) => {
            info!("Loaded {} Lua plugin(s)", mgr.list_plugins().len());
            agent.plugins = mgr;
        }
        Err(e) => warn!("Failed to load plugins, continuing without them: {}", e),
    }

    // Load the RL agent: uses a trained model if one is present on disk,
    // otherwise falls back to the heuristic policy. `rl_agent::init_agent`
    // already implements that fallback - the only thing missing before was
    // ever calling it, which left `agent.rl_agent` permanently `None`.
    let model_path = PathBuf::from("/etc/pwnghost/models/rl_model.safetensors");
    let using_model = model_path.exists();
    let rl_config = rl_agent::RlAgentConfig {
        model_path: if using_model {
            Some(model_path.to_string_lossy().to_string())
        } else {
            None
        },
        ..rl_agent::RlAgentConfig::default()
    };
    match rl_agent::init_agent(&rl_config) {
        Ok(rl) => {
            agent.rl_agent = Some(Arc::new(RwLock::new(rl)));
            info!(
                "RL agent ready ({})",
                if using_model {
                    "trained model"
                } else {
                    "heuristic policy"
                }
            );
        }
        Err(e) => warn!(
            "Failed to initialize RL agent, continuing with heuristic-only decisions: {}",
            e
        ),
    }

    // Recovery: restore progress (xp/level/handshake+pmkid counts, per-AP
    // bond encounters, the RL policy's learned Q-values) from a prior run,
    // so a device's progression survives a reboot instead of resetting to
    // zero every power cycle. `RecoveryManager`/`RecoveryState` and the RL
    // policy's `export_state`/`import_state` were all fully implemented and
    // unit-tested but never actually wired up before - this is the fix.
    // Lives under /var/lib/pwnghost, not the zram-backed /var/log or
    // /var/tmp (see SPEC.md's SD-card-longevity design) since this is
    // exactly the state we want to survive a reboot/power loss.
    let mut recovery_manager =
        agent::recovery::RecoveryManager::new("/var/lib/pwnghost/recovery.json", 300);
    if let Err(e) = recovery_manager.load().await {
        warn!(
            "Failed to load recovery state (starting fresh, this is expected on first boot): {}",
            e
        );
    }
    recovery_manager.apply_to_agent(&mut agent);
    agent.personality.update_on_reboot();

    // Mesh manager: advertises our own state via `build_mesh_ie` (for
    // whatever embeds it into beacons/probe responses) and prunes stale
    // peers each tick. NOTE: nothing calls `update_peer()` with real data
    // yet - that requires parsing vendor IEs out of raw beacon/probe-
    // response frames, and AngryOxide's CLI/stdout interface (see
    // `angryoxide::parser` docs) exposes neither raw frames nor any
    // structured peer data. `MeshManager` is fully implemented and
    // unit-tested; wiring a real IE-capture source later just means calling
    // `update_peer()` from that source.
    let our_mac = read_iface_mac(&config.main.iface);
    let mesh_manager = agent::MeshManager::new(our_mac, config.main.name.clone());

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
    // systemd watchdog ping. Harmless if the unit doesn't set
    // `WatchdogSec=`; ready to use as soon as it does.
    let mut watchdog_interval = tokio::time::interval(tokio::time::Duration::from_secs(15));

    // Periodic recovery save, throttled to `recovery_manager.save_interval()`
    // (real wall-clock time, independent of the epoch tick rate) rather than
    // every tick, since /var/lib/pwnghost lives on the SD card, not zram.
    let mut recovery_save_interval = tokio::time::interval(recovery_manager.save_interval());
    recovery_save_interval.tick().await; // first tick fires immediately; skip it

    // Once the AngryOxide event channel closes for good, stop selecting on
    // it (recv() on a closed channel resolves immediately, which would
    // otherwise busy-loop that branch and starve the timer tick below).
    let mut ao_events_open = true;

    // Tell systemd (Type=notify) that startup is complete, now that the
    // agent/AO/display/web are all initialized and we're about to enter the
    // main loop. No-op if not running under systemd.
    if let Err(e) = sd_notify::ready() {
        warn!("sd_notify READY=1 failed: {}", e);
    }

    loop {
        tokio::select! {
            event = ao_handle.recv_event(), if ao_events_open => {
                match event {
                    Some(event) => {
                        let is_handshake_file = matches!(
                            event,
                            angryoxide::parser::AoEvent::HandshakeFileWritten(_)
                        );
                        agent.handle_event(&event);

                        // A handshake-shaped file showed up in AO's output
                        // dir: run the capture pipeline (hcxpcapngtool
                        // validation + move-to-final) and attribute the
                        // result to the real AP it extracts the BSSID for.
                        if is_handshake_file {
                            if let Some(cm) = agent.capture_manager.clone() {
                                match cm.process_new().await {
                                    Ok(handshakes) => {
                                        for hs in handshakes {
                                            info!(
                                                "Captured handshake: {} ({:?})",
                                                hs.bssid, hs.handshake_type
                                            );
                                            agent.mark_handshake_captured(hs.bssid);

                                            if let Some(ref state_arc) = web_state {
                                                let ws = state_arc.read().await.ws_manager.clone();
                                                ws.broadcast_handshake(
                                                    hs.id.to_string(),
                                                    hs.bssid.to_string(),
                                                    hs.ssid.clone(),
                                                    hs.channel.value(),
                                                    format!("{:?}", hs.handshake_type),
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => warn!("Capture processing failed: {}", e),
                                }
                            }
                        }
                    }
                    None => {
                        warn!("AngryOxide event channel closed");
                        ao_events_open = false;
                    }
                }
            }
            _ = interval.tick() => {
                let previous_mood = agent.current_mood();

                // Tick agent
                let (face, action) = agent.tick();

                // Mesh: prune stale peers and feed whatever's left into the
                // personality engine (peers affect mood - see
                // `Personality::compute_mood`).
                mesh_manager.cleanup_stale().await;
                let peers: Vec<pwncore::Peer> = mesh_manager
                    .active_peers()
                    .await
                    .into_iter()
                    .map(|mp| mp.peer)
                    .collect();
                agent.update_peers(peers.clone());

                // Run the on_epoch hook for every loaded Lua plugin.
                if let Err(e) = agent
                    .plugins
                    .on_epoch(agent.total_epochs(), &agent.epoch_tracker.current)
                    .await
                {
                    warn!("Plugin on_epoch hook failed: {}", e);
                }

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

                // Push live state to the web UI so the dashboard isn't static.
                if let Some(ref state_arc) = web_state {
                    let stats = agent.personality.stats();
                    let mood_str = format!("{:?}", agent.current_mood());
                    let mood_changed = previous_mood != agent.current_mood();

                    {
                        let mut state = state_arc.write().await;
                        state.epoch = agent.total_epochs();
                        state.uptime = agent.start.elapsed().as_secs();
                        state.current_channel = agent.current_channel();
                        state.mood = agent.current_mood();
                        state.face = face.to_string();
                        state.handshakes = agent.epoch_tracker.current.handshakes_this_epoch;
                        state.level = stats.level;
                        state.xp = stats.xp;
                        state.peers = peers.clone();
                    }

                    let ws = state_arc.read().await.ws_manager.clone();
                    ws.broadcast_session(
                        agent.total_epochs(),
                        agent.start.elapsed().as_secs(),
                        agent.aps_count(),
                        agent.epoch_tracker.current.handshakes_this_epoch,
                        agent.current_channel(),
                        mood_str.clone(),
                        face.to_string(),
                        stats.level,
                        stats.xp,
                        peers.len(),
                    );

                    if mood_changed {
                        ws.broadcast_mood_change(mood_str, face.to_string());
                    }
                }

                // Execute action
                match action {
                    agent::AgentAction::Hop(ch) => {
                        // NOTE: AngryOxide owns channel hopping internally
                        // (via `-c`/`-b`/`--autohunt` plus its own
                        // netlink-based monitor-mode management). This used
                        // to call `radio.switch_to(RadioMode::Rage, ...)`,
                        // but that toggles the RAGE/BT/SAFE radio *mode*
                        // state machine (interface teardown + monitor-mode
                        // bringup) - unrelated to WiFi channel selection,
                        // and actively harmful here since it would flap
                        // monitor mode out from under AO on every epoch
                        // hop. We still track the intended channel locally
                        // (feeds mood/epoch state and the web UI); we just
                        // don't tell the radio manager to do anything.
                        info!("Agent wants channel {} (informational; AO manages hopping)", ch);
                        agent.set_channel(ch);
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
                        // `fw_patcher::gpio` is always callable: it's a real
                        // `rppal`-backed implementation when built with
                        // `--features linux-gpio` (see this crate's
                        // Cargo.toml), and a stub that logs+errors
                        // otherwise, so this never needs a `#[cfg]` here.
                        if let Err(e) = fw_patcher::gpio::power_cycle_wl_reg_on().await {
                            error!("GPIO power-cycle failed: {}", e);
                        }
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
            _ = watchdog_interval.tick() => {
                if let Err(e) = sd_notify::watchdog() {
                    warn!("sd_notify WATCHDOG=1 failed: {}", e);
                }
            }
            _ = recovery_save_interval.tick() => {
                recovery_manager.update_from_agent(&agent);
                if let Err(e) = recovery_manager.save().await {
                    warn!("Failed to save recovery state: {}", e);
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

    // Final recovery save so a clean shutdown never loses progress made
    // since the last periodic save.
    recovery_manager.update_from_agent(&agent);
    if let Err(e) = recovery_manager.save().await {
        warn!("Failed to save recovery state on shutdown: {}", e);
    }

    if let Some(d) = display {
        d.show_shutdown().await?;
        d.sleep().await?;
    }

    info!("PWNGHOST-RS stopped");
    Ok(())
}
