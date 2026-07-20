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

/// CPU temperature in Celsius, for the display's resource footer. `None`
/// if unreadable (no thermal zone, or a non-Linux dev environment).
fn read_cpu_temp() -> Option<f32> {
    std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp")
        .ok()
        .and_then(|raw| parse_cpu_temp_millidegrees(&raw))
}

fn parse_cpu_temp_millidegrees(raw: &str) -> Option<f32> {
    raw.trim().parse::<f32>().ok().map(|milli_c| milli_c / 1000.0)
}

/// (used_mb, total_mb) RAM, for the display's resource footer. `(0, 0)` if
/// `/proc/meminfo` is unreadable.
fn read_ram_usage_mb() -> (u64, u64) {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .map(|raw| parse_ram_usage_mb(&raw))
        .unwrap_or((0, 0))
}

fn parse_ram_usage_mb(meminfo: &str) -> (u64, u64) {
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in meminfo.lines() {
        if let Some(kb) = parse_meminfo_field_kb(line, "MemTotal:") {
            total_kb = kb;
        } else if let Some(kb) = parse_meminfo_field_kb(line, "MemAvailable:") {
            avail_kb = kb;
        }
    }
    ((total_kb.saturating_sub(avail_kb)) / 1024, total_kb / 1024)
}

fn parse_meminfo_field_kb(line: &str, prefix: &str) -> Option<u64> {
    line.strip_prefix(prefix)?
        .trim()
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// (face, line) for the display's "friend" fields: the strongest-signal
/// active mesh peer's own mood face, plus signal bars + name +
/// handshakes shared -- matches real pwnagotchi's
/// `View.set_closest_peer`/`friend_face`/`friend_name` (same RSSI
/// thresholds for the 1-4 signal bars, same "peer's own face" idea via
/// `peer.face()`).
fn closest_peer_face_and_line(peers: &[pwncore::Peer]) -> Option<(String, String)> {
    let peer = peers.iter().max_by_key(|p| p.signal)?;
    let num_bars = if peer.signal >= -67 {
        4
    } else if peer.signal >= -70 {
        3
    } else if peer.signal >= -80 {
        2
    } else {
        1
    };
    let bars: String = "|".repeat(num_bars) + &".".repeat(4 - num_bars);
    let face = agent::faces::face_for_mood(peer.mood).to_string();
    let line = format!("{bars} {} {}", peer.name, peer.handshakes_shared);
    Some((face, line))
}

/// Choose the face to draw on a given ~1s display tick, animating the gaze
/// during passive recon the way real pwnagotchi does in its `ui/view.py`
/// `wait()` loop (alternating LOOK_R/LOOK_L, using the happy gaze variants
/// when in a good mood). This is most of what makes the idle screen feel
/// alive; the agent's own epoch/mood cycle runs far slower than 1s, so
/// without animating here the face would freeze between epochs.
///
/// Only the passive/neutral recon moods animate. Genuine event moods -- a
/// capture (Happy/Excited/Grateful), Sad/Angry/Lonely/Bored, uploading, a
/// friend greeting, sleeping -- are returned unchanged (as `base_face`,
/// the real mood face cached from the last epoch tick) because they carry
/// meaning the scan animation must not paint over.
fn animated_face(
    mood: pwncore::Mood,
    base_face: &'static str,
    tick: u64,
    good_mood: bool,
) -> &'static str {
    use pwncore::Mood;
    match mood {
        // Neutral recon/awake states: sweep the gaze left/right each tick.
        Mood::LookR | Mood::LookL | Mood::Awake => {
            let look = match (tick % 2 == 0, good_mood) {
                (true, false) => Mood::LookR,
                (false, false) => Mood::LookL,
                (true, true) => Mood::LookRHappy,
                (false, true) => Mood::LookLHappy,
            };
            agent::faces::face_for_mood(look)
        }
        _ => base_face,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Config has to load before logging is initialized -- the log file
    // path comes from `config.toml`'s `[main.log] path`. Previously
    // logging was initialized first, unconditionally to stdout only, so
    // that config value was silently never honored: no file was ever
    // created there, no matter what config.toml said.
    let mut config = load_config(&args.config).await?;
    if let Some(iface) = args.interface {
        config.main.iface = iface;
    }

    // Non-rotating (exact filename, no date suffix) so this matches the
    // literal path config.toml (and other tooling expecting to find it
    // there, e.g. the `logtail` Lua plugin) specifies -- tracing-
    // appender's rotating writers append a date suffix to the filename
    // instead, which would silently break anything reading the plain
    // path. `config.toml`'s `[main.log.rotation]` size-based rotation
    // isn't implemented here (a real, separate follow-up); logging to a
    // real file that actually exists is strictly better than the
    // previous stdout-only behavior even without it.
    let log_path = std::path::Path::new(&config.main.log.path);
    let log_dir = log_path.parent().filter(|p| !p.as_os_str().is_empty());
    let log_file_name = log_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("pwnghost.log");
    if let Some(dir) = log_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("warning: could not create log directory {dir:?}: {e}");
        }
    }
    let file_appender =
        tracing_appender::rolling::never(log_dir.unwrap_or_else(|| std::path::Path::new(".")), log_file_name);
    let (non_blocking_file, _log_guard) = tracing_appender::non_blocking(file_appender);

    // Initialize logging: journal (stdout, systemd captures this via
    // StandardOutput=journal) and the real log file, together.
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&args.log_level))
        .with_writer(std::io::stdout.and(non_blocking_file))
        .init();

    info!("Starting PWNGHOST-RS v{}", env!("CARGO_PKG_VERSION"));

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
    // `whitelist`/`personality.deauth` are real, user-configurable
    // settings (`config.toml`'s `[main] whitelist = []` and
    // `[personality] deauth = false`) that previously never reached
    // AngryOxide at all -- it was always spawned with an empty whitelist
    // and deauth left enabled regardless of what the user configured.
    let ao_config = angryoxide::args::AngryOxideConfig {
        interface: config.main.iface.clone(),
        output: Some(staging_dir.join("session")),
        whitelist: config.main.whitelist.clone(),
        disable_deauth: !config.personality.deauth,
        ..angryoxide::args::AngryOxideConfig::default()
    };
    let ao_handle = init_angryoxide(&ao_config).await?;

    let capture_manager = agent::CaptureManager::new(staging_dir.clone(), handshakes_dir);
    if let Err(e) = capture_manager.init().await {
        warn!("Failed to initialize capture manager directories: {}", e);
    }

    // Initialize radio manager
    let mut radio = RadioManager::new(config.main.iface.clone());

    // Initialize display if enabled. A display/hardware-wiring problem (wrong
    // SPI path, wrong GPIO chip, a pin that doesn't match the physical HAT)
    // must never take down the rest of the agent -- WiFi scanning, capture,
    // healing, and the web UI are all independent of whether the e-ink panel
    // is working, and a device that's otherwise fully functional shouldn't
    // crash-loop just because its screen doesn't light up.
    let mut display = if config.ui.display.enabled {
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
        match Display::new(disp_cfg) {
            Ok(d) => Some(d),
            Err(e) => {
                error!(
                    "Failed to construct display driver, continuing without a display: {}",
                    e
                );
                None
            }
        }
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
    // `AppState::default()` starts with `PwnConfig::default()`, not the
    // config actually loaded above -- `/api/config` (and the web UI's
    // config viewer) would otherwise always show defaults regardless of
    // what's really in config.toml. `config_path` is what `update_config`
    // writes back to.
    if let Some(ref state_arc) = web_state {
        let mut state = state_arc.write().await;
        state.config = config.clone();
        state.config_path = args.config.clone();
    }

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

    // Initialize display. See the comment above: a real hardware/wiring
    // failure here (never verified against real silicon -- see
    // ui::display::hardware's module docs) must disable the display, not
    // crash the whole agent. Check `journalctl -u pwnghost-rs` for the exact
    // error if the e-ink panel doesn't light up.
    if let Some(d) = display.as_ref() {
        if let Err(e) = d.init().await {
            error!(
                "Display hardware init failed, continuing without a display: {:?}",
                e
            );
            display = None;
        }
    }

    // Main loop
    info!("Entering main loop");
    agent.start();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
        config.oxigotchi.epoch_duration as u64,
    ));
    // Display refresh, decoupled from the (much slower, `epoch_duration`-
    // paced) agent tick above -- confirmed on real hardware that tying
    // the display to the epoch interval directly made it feel static
    // compared to real pwnagotchi, which redraws roughly once a second
    // (`ui.fps=1` is the common e-ink default there) regardless of how
    // often the underlying recon/decision cycle actually runs. 1s
    // matches that convention; this project has no `ui.fps` config knob
    // yet to make it tunable. `latest_face` is the only piece of drawn
    // state that can't be safely recomputed outside `agent.tick()`
    // (calling it again here would double-advance epoch state) --
    // everything else the fast tick draws (stats, phrase, aps, mode,
    // friend) is cheap and side-effect-free to recompute every second.
    let mut display_interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    let mut latest_face: &str = agent::faces::face_for_mood(agent.current_mood());
    // Counter driving the per-second face "looking around" animation (see
    // `animated_face`). Real pwnagotchi's idle screen feels alive because
    // the face alternates gaze direction roughly every second during recon;
    // our epoch tick is far slower than 1s, so without animating here the
    // face would sit frozen between epochs.
    let mut display_tick: u64 = 0;
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

    // Real pwnagotchi fires plugins' `on_ready` once at startup, after
    // everything else is up -- previously the only hook ever invoked was
    // `on_epoch`, so plugins needing one-time setup (grid announcing this
    // unit, webcfg priming state, etc.) never got a chance to run it.
    if let Err(e) = agent.plugins.on_ready().await {
        warn!("Plugin on_ready hook failed: {}", e);
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

                        // AO's real-time activity narration (channel hops,
                        // associations, deauths, etc.) was previously only
                        // ever logged via `agent.handle_event` -- reaching
                        // journalctl but nowhere in the UI, which is why
                        // recon/pwning activity looked invisible even
                        // though AO was genuinely doing it in the
                        // background. Surface it to the web dashboard too.
                        if let angryoxide::parser::AoEvent::StatusLine { level, message } = &event {
                            if let Some(ref state_arc) = web_state {
                                let ws = state_arc.read().await.ws_manager.clone();
                                ws.broadcast_ao_activity(format!("{level:?}"), message.clone());
                            }
                        }

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

                                            if let Err(e) = agent
                                                .plugins
                                                .on_handshake(
                                                    &hs.bssid.to_string(),
                                                    hs.ssid.as_deref().unwrap_or(""),
                                                    &hs.hashcat_path,
                                                )
                                                .await
                                            {
                                                warn!("Plugin on_handshake hook failed: {}", e);
                                            }

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

                // Real stats, shared by both the web dashboard below and
                // (recomputed fresh every second) the display-refresh
                // tick further down.
                let stats = agent.personality.stats();
                let mood_str = format!("{:?}", agent.current_mood());
                let aps_count = agent.aps_count();
                let cpu_temp = read_cpu_temp();
                let (ram_used_mb, ram_total_mb) = read_ram_usage_mb();
                // Cache the face for the faster, decoupled display-refresh
                // tick below -- `agent.tick()` (which produced it) also
                // advances epoch state, so it can't be safely re-derived
                // outside this slower interval the way the rest of the
                // drawn state can.
                latest_face = face;

                // Push live state to the web UI so the dashboard isn't static.
                if let Some(ref state_arc) = web_state {
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
                        state.cpu_temp = cpu_temp;
                        state.ram_used = ram_used_mb;
                        state.ram_total = ram_total_mb;
                    }

                    let ws = state_arc.read().await.ws_manager.clone();
                    ws.broadcast_session(
                        agent.total_epochs(),
                        agent.start.elapsed().as_secs(),
                        aps_count,
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
            _ = display_interval.tick() => {
                // Redraws every second regardless of the (much slower)
                // agent/epoch cadence above -- matches real pwnagotchi's
                // `ui.fps=1` e-ink convention. Only `latest_face` is
                // cached from the epoch tick; everything else here is
                // cheap and side-effect-free to recompute fresh, so the
                // uptime clock and current stats visibly move every
                // second instead of only when the (much slower) agent
                // tick happens to fire.
                if let Some(ref d) = display {
                    display_tick = display_tick.wrapping_add(1);
                    let uptime = format!("{}s", agent.start.elapsed().as_secs());
                    let stats = agent.personality.stats();
                    let phrase = agent.personality.get_phrase(agent.current_mood());
                    let aps_count = agent.aps_count();
                    let peers: Vec<pwncore::Peer> = mesh_manager
                        .active_peers()
                        .await
                        .into_iter()
                        .map(|mp| mp.peer)
                        .collect();
                    let friend = closest_peer_face_and_line(&peers);
                    // Animate the gaze during passive recon so the idle
                    // screen moves every second like real pwnagotchi,
                    // instead of freezing on `latest_face` until the next
                    // (much slower) epoch tick. Event moods (a capture,
                    // sad/angry/bored, uploading, a friend) are shown as-is
                    // -- they carry real meaning and shouldn't be
                    // overridden by the scan animation. "Good mood" (happy
                    // gaze variant) when peers are nearby or we just caught
                    // something, mirroring real pwnagotchi's
                    // `in_good_mood` gate in its `wait()` loop.
                    let good_mood = !peers.is_empty()
                        || agent.epoch_tracker.current.handshakes_this_epoch > 0;
                    let face = animated_face(
                        agent.current_mood(),
                        latest_face,
                        display_tick,
                        good_mood,
                    );
                    // Blinking name cursor, matching real pwnagotchi's
                    // `_refresh_handler` trailing block that toggles at fps
                    // -- a small always-moving element so the screen never
                    // looks hung even when nothing else changed this second.
                    let name = if display_tick % 2 == 0 {
                        format!("{}", config.main.name)
                    } else {
                        format!("{}\u{2588}", config.main.name)
                    };
                    if let Err(e) = d
                        .draw_pwnagotchi_frame(
                            agent.current_channel(),
                            aps_count,
                            &uptime,
                            &name,
                            &phrase,
                            face,
                            agent.epoch_tracker.current.handshakes_this_epoch,
                            stats.handshakes,
                            stats.level,
                            stats.xp,
                            "AUTO",
                            friend
                                .as_ref()
                                .map(|(face, line)| (face.as_str(), line.as_str())),
                        )
                        .await
                    {
                        warn!("Display draw failed: {}", e);
                    } else {
                        // Publish the freshly-drawn frame to the web UI's
                        // live view (`/ui`) before pushing it to the panel,
                        // so the browser mirrors exactly what the e-ink shows
                        // -- the same model as real pwnagotchi's PNG frame.
                        if let Some(ref state_arc) = web_state {
                            if let Ok(png) = d.frame_png().await {
                                state_arc.write().await.frame_png = png;
                            }
                        }
                        if let Err(e) = d.update(true).await {
                            warn!("Display update failed: {}", e);
                        }
                    }
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
        if let Err(e) = d.show_shutdown().await {
            warn!("Display show_shutdown failed: {}", e);
        }
        if let Err(e) = d.sleep().await {
            warn!("Display sleep failed: {}", e);
        }
    }

    info!("PWNGHOST-RS stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cpu_temp_millidegrees() {
        assert_eq!(parse_cpu_temp_millidegrees("45123\n"), Some(45.123));
        assert_eq!(parse_cpu_temp_millidegrees("not a number"), None);
        assert_eq!(parse_cpu_temp_millidegrees(""), None);
    }

    #[test]
    fn test_closest_peer_face_and_line_none_when_no_peers() {
        assert_eq!(closest_peer_face_and_line(&[]), None);
    }

    #[test]
    fn test_closest_peer_face_and_line_picks_strongest_signal() {
        let mut weak = pwncore::Peer::new(pwncore::MacAddr::default(), "far".to_string(), 6, -85);
        weak.handshakes_shared = 1;
        let mut strong =
            pwncore::Peer::new(pwncore::MacAddr::default(), "close".to_string(), 6, -60);
        strong.handshakes_shared = 5;
        let (_face, line) = closest_peer_face_and_line(&[weak, strong]).unwrap();
        assert!(line.contains("close"));
        assert!(line.contains('5'));
        assert!(line.starts_with("||||"), "strong signal should show 4 bars: {line}");
    }

    #[test]
    fn test_closest_peer_face_and_line_weak_signal_shows_one_bar() {
        let peer = pwncore::Peer::new(pwncore::MacAddr::default(), "far".to_string(), 1, -90);
        let (_face, line) = closest_peer_face_and_line(&[peer]).unwrap();
        assert!(line.starts_with("|..."), "weak signal should show 1 bar: {line}");
    }

    #[test]
    fn test_parse_meminfo_field_kb() {
        assert_eq!(
            parse_meminfo_field_kb("MemTotal:         474088 kB", "MemTotal:"),
            Some(474088)
        );
        assert_eq!(
            parse_meminfo_field_kb("MemAvailable:     356792 kB", "MemAvailable:"),
            Some(356792)
        );
        assert_eq!(parse_meminfo_field_kb("MemTotal:         474088 kB", "Cached:"), None);
    }

    #[test]
    fn test_parse_ram_usage_mb() {
        let meminfo = "\
MemTotal:         474088 kB
MemFree:          200000 kB
MemAvailable:     356792 kB
Buffers:           10000 kB
";
        let (used_mb, total_mb) = parse_ram_usage_mb(meminfo);
        // 474088 - 356792 = 117296 kB -> 114 MB (integer division)
        assert_eq!(used_mb, 114);
        // 474088 kB / 1024 = 462 MB (integer division)
        assert_eq!(total_mb, 462);
    }

    #[test]
    fn test_parse_ram_usage_mb_missing_fields_yields_zero() {
        let (used_mb, total_mb) = parse_ram_usage_mb("SomeOtherField: 123 kB\n");
        assert_eq!(used_mb, 0);
        assert_eq!(total_mb, 0);
    }
}
