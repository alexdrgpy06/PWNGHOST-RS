//! REST API endpoints for Web UI

use axum::{
    extract::{Path, State},
    http::{Method, StatusCode},
    Json,
};
use axum::body::Bytes;
use pwncore::AccessPoint;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

fn default_source_local() -> String {
    "local".to_string()
}

#[derive(Serialize, serde::Deserialize)]
pub struct CrackedPassword {
    pub bssid: String,
    pub ssid: String,
    pub password: String,
    #[serde(default)]
    pub cracked_at: i64,
    /// Where this result came from: "local" (on-device hashcat via
    /// `pwncrack.lua`) or "wpa-sec" (the remote potfile). Lets the UI show
    /// both sources together and dedup by BSSID.
    #[serde(default = "default_source_local")]
    pub source: String,
    // Richer local-hashcat metadata (C4b). All optional so the older
    // `<bssid>.json` files `pwncrack.lua` wrote (bssid/ssid/password/
    // cracked_at only) still deserialize cleanly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wordlist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_type: Option<String>,
}

/// Real pwnagotchi has no built-in equivalent view for *locally* cracked
/// passwords (only wpa-sec's remote cracked-potfile download), but the
/// underlying capability -- and the user's own explicit ask -- mirrors
/// third-party plugins like Sniffleupagus's `display-password.py`. This is
/// the lightweight slice of that: a read-only list of what `pwncrack.lua`
/// has written (one `<bssid>.json` file per cracked handshake, see that
/// plugin's doc comment), no on-device QR/captive-portal UI (those need the
/// plugin host API + `on_webhook` this project doesn't have yet -- see
/// REWORK_PLAN.md Workstream D).
pub async fn get_cracked(State(state): State<Arc<RwLock<AppState>>>) -> Json<Vec<CrackedPassword>> {
    let dir = state.read().await.cracked_dir.clone();
    let mut out = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return Json(out);
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(cracked) = serde_json::from_str::<CrackedPassword>(&content) {
                    out.push(cracked);
                }
            }
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.cracked_at));
    Json(out)
}

/// Format a bare 12-hex-char MAC (as wpa-sec's potfile stores it) into the
/// familiar `aa:bb:cc:dd:ee:ff` form. Returns the input unchanged if it isn't
/// exactly 12 hex chars.
fn format_mac_hex(hex: &str) -> String {
    if hex.len() == 12 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        hex.as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(":")
    } else {
        hex.to_string()
    }
}

/// The wpa-sec **potfile view**: passwords recovered by the remote
/// wpa-sec.stanev.org cracking service. `wpa_sec.lua` downloads the account's
/// potfile to `wpa_sec_potfile` (see that plugin); this parses it. Kept
/// *alongside* the local-hashcat `/api/cracked` view, not as a replacement --
/// the UI shows both, deduped by BSSID.
///
/// wpa-sec's `?api&dl=1` returns colon-separated
/// `bssid:clientmac:ssid:password` lines, with MACs as bare 12-hex-char
/// strings. Password may itself contain `:`, so only the first three colons
/// are treated as field separators.
pub async fn get_wpa_sec_cracked(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<Vec<CrackedPassword>> {
    let path = state.read().await.wpa_sec_potfile.clone();
    let mut out = Vec::new();
    let Ok(content) = tokio::fs::read_to_string(&path).await else {
        return Json(out);
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 4 {
            continue;
        }
        out.push(CrackedPassword {
            bssid: format_mac_hex(parts[0]),
            ssid: parts[2].to_string(),
            password: parts[3].to_string(),
            cracked_at: 0,
            source: "wpa-sec".to_string(),
            duration_secs: None,
            attack_mode: None,
            wordlist: None,
            hash_type: None,
        });
    }
    Json(out)
}

#[derive(Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub enabled: bool,
    /// The plugin's `[plugins.<name>].options` table verbatim (e.g. wpa_sec's
    /// `api_key`, pwncrack's `wordlist`) -- exposed so the UI can render and
    /// edit real per-plugin settings instead of only enable/disable.
    pub options: std::collections::HashMap<String, serde_json::Value>,
}

/// List every configured plugin, whether it's enabled, and its options.
/// Enabled state is the real `[plugins.<name>].enabled` flag that now
/// actually gates loading (`PluginManager::load_builtin_plugins`), so the
/// toggle no longer lies.
pub async fn get_plugins(State(state): State<Arc<RwLock<AppState>>>) -> Json<Vec<PluginInfo>> {
    let state = state.read().await;
    let mut list: Vec<PluginInfo> = state
        .config
        .plugins
        .iter()
        .map(|(name, cfg)| PluginInfo {
            name: name.clone(),
            enabled: cfg.enabled,
            options: cfg.options.clone(),
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Json(list)
}

/// Persist a plugin's `options` map (merged with its existing options, so
/// setting one key never clobbers another) through the same deep-merge +
/// validate + atomic-write path as `update_config`.
pub async fn update_plugin_options(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(name): Path<String>,
    Json(options): Json<std::collections::HashMap<String, serde_json::Value>>,
) -> Json<serde_json::Value> {
    let mut state = state.write().await;
    // `PluginConfig::options` is `#[serde(flatten)]` (schema.rs), so on the
    // wire a plugin's options are SIBLING keys next to `enabled`, not
    // nested under an "options" key -- the patch must mirror that flattened
    // shape, or `merge_json` parks them under a literal "options" key
    // instead of merging them into the flattened set.
    let plugin_patch: serde_json::Map<String, serde_json::Value> = options.into_iter().collect();
    let mut plugins_patch = serde_json::Map::new();
    plugins_patch.insert(name.clone(), serde_json::Value::Object(plugin_patch));
    let mut patch = serde_json::Map::new();
    patch.insert(
        "plugins".to_string(),
        serde_json::Value::Object(plugins_patch),
    );
    let patch = serde_json::Value::Object(patch);

    let path = state.config_path.clone();
    let mut merged = match config::apply_config_patch(&state.config, &patch) {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
        }
    };
    if let Err(e) = merged.validate_and_fix().await {
        return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
    }
    match config::save_config(&merged, &path).await {
        Ok(()) => {
            state.config = merged;
            Json(serde_json::json!({"status": "ok", "name": name}))
        }
        Err(e) => Json(serde_json::json!({"status": "error", "message": e.to_string()})),
    }
}

/// Toggle a plugin's `enabled` flag and persist it through the same safe
/// deep-merge + validate + atomic-write path as `update_config`, so nothing
/// else in the config is disturbed. Takes effect on the next restart (plugins
/// are loaded once at startup).
pub async fn toggle_plugin(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(name): Path<String>,
) -> Json<serde_json::Value> {
    let mut state = state.write().await;
    let current = state
        .config
        .plugins
        .get(&name)
        .map(|p| p.enabled)
        .unwrap_or(true);
    let new_enabled = !current;
    let patch = serde_json::json!({ "plugins": { name.clone(): { "enabled": new_enabled } } });

    let path = state.config_path.clone();
    let mut merged = match config::apply_config_patch(&state.config, &patch) {
        Ok(c) => c,
        Err(e) => {
            return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
        }
    };
    if let Err(e) = merged.validate_and_fix().await {
        return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
    }
    match config::save_config(&merged, &path).await {
        Ok(()) => {
            state.config = merged;
            Json(serde_json::json!({"status": "ok", "name": name, "enabled": new_enabled}))
        }
        Err(e) => Json(serde_json::json!({"status": "error", "message": e.to_string()})),
    }
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub epoch: u64,
    pub uptime: u64,
    pub aps: usize,
    pub handshakes: u32,
    pub channel: u8,
    pub mood: String,
    pub face: String,
    pub phrase: String,
    pub level: u32,
    pub xp: u32,
    pub peers: usize,
}

pub async fn get_session(State(state): State<Arc<RwLock<AppState>>>) -> Json<SessionResponse> {
    let state = state.read().await;
    Json(SessionResponse {
        epoch: state.epoch,
        uptime: state.uptime,
        aps: state.aps.len(),
        handshakes: state.handshakes,
        channel: state.current_channel,
        mood: format!("{:?}", state.mood),
        face: state.face.clone(),
        phrase: state.phrase.clone(),
        level: state.level,
        xp: state.xp,
        peers: state.peers.len(),
    })
}

/// Return the **entire** config as JSON. Previously this returned only
/// `main`/`personality`/`ui`, omitting `bettercap`/`fs`/`agent`/
/// `plugins`; combined with a whole-object POST that was a silent data-loss
/// trap (an edit-and-save round-trip wiped the omitted sections). Now the
/// full config is exposed so every section is visible and editable, and the
/// POST path deep-merges so nothing is lost either way.
pub async fn get_config(State(state): State<Arc<RwLock<AppState>>>) -> Json<serde_json::Value> {
    let state = state.read().await;
    Json(serde_json::to_value(&state.config).unwrap_or(serde_json::Value::Null))
}

/// Persist a **partial** config patch by deep-merging it onto the current
/// config, so sections the client didn't send are preserved (mirrors real
/// pwnagotchi's webcfg "merge-save"). Accepts an arbitrary JSON object; only
/// the keys present are changed.
pub async fn update_config(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(patch): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let mut state = state.write().await;
    let path = state.config_path.clone();
    let mut merged = match config::apply_config_patch(&state.config, &patch) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Config patch rejected: {}", e);
            return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
        }
    };
    // apply_config_patch only proves the merged JSON deserializes into a
    // PwnConfig (right shapes/types) -- it says nothing about whether the
    // *values* make sense. Without this, a type-valid but semantically
    // broken patch (web.port=0, min_recon_time > max_recon_time, etc.)
    // would be written straight to disk by save_config below, and since
    // main.rs's startup path calls load_config -> validate_and_fix and
    // exits via `?` on failure, that broken config would then fail to
    // load on the *next* boot -- before the web server (the only way to
    // fix it without SD-card/SSH access) even starts. Same class of
    // self-inflicted lockout as the SD-corruption crash-loop diagnosed
    // this session, just via a bad config write instead of a bad
    // process restart.
    if let Err(e) = merged.validate_and_fix().await {
        tracing::warn!("Config patch rejected by validation: {}", e);
        return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
    }
    match config::save_config(&merged, &path).await {
        Ok(()) => {
            state.config = merged;
            Json(serde_json::json!({"status": "ok"}))
        }
        Err(e) => {
            tracing::warn!("Failed to persist config to {:?}: {}", path, e);
            Json(serde_json::json!({"status": "error", "message": e.to_string()}))
        }
    }
}

#[derive(Serialize)]
pub struct PeerResponse {
    pub mac: String,
    pub name: String,
    pub channel: u8,
    pub mood: String,
    pub level: u32,
    pub signal: i16,
    pub handshakes_shared: u32,
    pub last_seen: i64,
}

pub async fn get_peers(State(state): State<Arc<RwLock<AppState>>>) -> Json<Vec<PeerResponse>> {
    let state = state.read().await;
    Json(
        state
            .peers
            .iter()
            .map(|p| PeerResponse {
                mac: p.mac.to_string(),
                name: p.name.clone(),
                channel: p.channel,
                mood: format!("{:?}", p.mood),
                level: p.level,
                signal: p.signal,
                handshakes_shared: p.handshakes_shared,
                last_seen: p.last_seen.timestamp(),
            })
            .collect(),
    )
}

#[derive(Serialize)]
pub struct HandshakeResponse {
    pub id: String,
    pub bssid: String,
    pub ssid: Option<String>,
    pub channel: u8,
    pub handshake_type: String,
    pub captured_at: i64,
    pub file: String,
}

pub async fn get_handshakes(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<Vec<HandshakeResponse>> {
    let state = state.read().await;
    Json(
        state
            .handshakes_list
            .iter()
            .map(|h| HandshakeResponse {
                id: h.id.to_string(),
                bssid: h.bssid.to_string(),
                ssid: h.ssid.clone(),
                channel: h.channel.value(),
                handshake_type: format!("{:?}", h.handshake_type),
                captured_at: h.captured_at.timestamp(),
                file: h.pcapng_path.clone(),
            })
            .collect(),
    )
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub uptime: u64,
    pub epoch: u64,
    pub mood: String,
    pub face: String,
    pub phrase: String,
    pub channel: u8,
    pub aps: usize,
    pub handshakes: u32,
    pub level: u32,
    pub xp: u32,
    pub peers: usize,
    pub cpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub battery: Option<u8>,
    pub charging: bool,
}

pub async fn get_status(State(state): State<Arc<RwLock<AppState>>>) -> Json<StatusResponse> {
    let state = state.read().await;
    Json(StatusResponse {
        uptime: state.uptime,
        epoch: state.epoch,
        mood: format!("{:?}", state.mood),
        face: state.face.clone(),
        phrase: state.phrase.clone(),
        channel: state.current_channel,
        aps: state.aps.len(),
        handshakes: state.handshakes,
        level: state.level,
        xp: state.xp,
        peers: state.peers.len(),
        cpu_temp: state.cpu_temp,
        ram_used: state.ram_used,
        ram_total: state.ram_total,
        battery: state.battery,
        charging: state.charging,
    })
}

/// Reboots the device via `systemctl reboot` -- goes through the same
/// systemd shutdown path (and this project's own system-shutdown hook,
/// `safe-shutdown.sh`, that flushes the zram-backed log/data mounts first)
/// as any other reboot, rather than calling the `reboot(2)` syscall
/// directly, which pwnghost-rs.service's CapabilityBoundingSet doesn't
/// grant (no CAP_SYS_BOOT) -- systemd itself performs the actual syscall
/// on our behalf over D-Bus, so this works despite that restriction.
pub async fn reboot_system() -> Json<serde_json::Value> {
    match tokio::process::Command::new("systemctl")
        .arg("reboot")
        .spawn()
    {
        Ok(_) => Json(serde_json::json!({"status": "ok", "message": "rebooting"})),
        Err(e) => Json(serde_json::json!({"status": "error", "message": e.to_string()})),
    }
}

/// Request sent from the webhook route to the agent's main loop.
/// The agent task processes this on its own thread (where `PluginManager`
/// lives) and sends the response back via `reply`.
pub struct WebhookRequest {
    pub plugin_name: String,
    pub path: String,
    pub method: String,
    pub body: String,
    pub reply: tokio::sync::oneshot::Sender<(u16, String)>,
}

// Shared application state
pub struct AppState {
    pub epoch: u64,
    pub uptime: u64,
    pub aps: Vec<AccessPoint>,
    pub handshakes: u32,
    pub current_channel: u8,
    pub mood: pwncore::Mood,
    pub face: String,
    /// Current status-line phrase (`Agent::current_phrase()`), re-rolled
    /// only on mood transitions -- see that method's doc comment. Real
    /// pwnagotchi surfaces this same text on both the e-ink display and the
    /// web UI; this field is the WebUI's copy of it.
    pub phrase: String,
    pub level: u32,
    pub xp: u32,
    pub peers: Vec<pwncore::Peer>,
    pub handshakes_list: Vec<pwncore::Handshake>,
    pub config: config::PwnConfig,
    pub cpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub battery: Option<u8>,
    pub charging: bool,
    pub ws_manager: Arc<crate::ws::WebSocketManager>,
    /// Where `config` was loaded from -- `update_config` writes back here.
    /// Defaults to `/etc/pwnghost/config.toml` (the real path this
    /// project's systemd unit passes via `--config`), but `main.rs` sets
    /// this to whatever path was actually used at startup.
    pub config_path: std::path::PathBuf,
    /// Most recent rendered e-ink frame, PNG-encoded, for the `/ui` live
    /// view (real pwnagotchi serves the same thing). `main.rs` refreshes
    /// this on every ~1s display tick via `Display::frame_png`. Empty until
    /// the first frame is drawn.
    pub frame_png: Vec<u8>,
    /// Directory `pwncrack.lua` writes cracked-password JSON files to (see
    /// `get_cracked`). Defaults to the real path that plugin uses; tests
    /// override it with a tempdir.
    pub cracked_dir: std::path::PathBuf,
    /// Path `wpa_sec.lua` downloads the remote cracked potfile to (see
    /// `get_wpa_sec_cracked`). Defaults to the real path that plugin uses.
    pub wpa_sec_potfile: std::path::PathBuf,
    /// Channel sender for dispatching webhook requests to the agent's main
    /// loop. Set by `pwnghost-rs` main.rs to bridge the web server and the
    /// Lua plugin manager without a direct crate dependency (avoids a
    /// circular dep: `agent` → `ui` → `ui-web`).
    pub webhook_tx: Option<tokio::sync::mpsc::Sender<WebhookRequest>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            epoch: 0,
            uptime: 0,
            aps: Vec::new(),
            handshakes: 0,
            current_channel: 1,
            mood: pwncore::Mood::Awake,
            face: "(◕‿‿◕)".to_string(),
            phrase: String::new(),
            level: 0,
            xp: 0,
            peers: Vec::new(),
            handshakes_list: Vec::new(),
            config: config::PwnConfig::default(),
            cpu_temp: None,
            ram_used: 0,
            ram_total: 0,
            battery: None,
            charging: false,
            ws_manager: Arc::new(crate::ws::WebSocketManager::new()),
            config_path: std::path::PathBuf::from("/etc/pwnghost/config.toml"),
            frame_png: Vec::new(),
            cracked_dir: std::path::PathBuf::from("/var/tmp/pwnghost/pwncrack"),
            wpa_sec_potfile: std::path::PathBuf::from("/var/tmp/pwnghost/wpa-sec/potfile"),
            webhook_tx: None,
        }
    }
}

/// Serve the most recent rendered e-ink frame as a PNG (the live display
/// view). Mirrors real pwnagotchi's `/ui` route. Returns 503 until the first
/// frame has been rendered.
pub async fn get_ui_frame(State(state): State<Arc<RwLock<AppState>>>) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    let png = state.read().await.frame_png.clone();
    if png.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "display not ready").into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png,
    )
        .into_response()
}

/// Route an HTTP request to a Lua plugin's `on_webhook` handler.
/// Dispatches the request through a channel to the agent's main loop,
/// which processes it on the same thread where `PluginManager` lives.
pub async fn plugin_webhook(
    Path((name, path)): Path<(String, std::path::PathBuf)>,
    State(state): State<Arc<RwLock<AppState>>>,
    method: Method,
    body: Bytes,
) -> impl axum::response::IntoResponse {
    use axum::response::IntoResponse;
    use std::time::Duration;
    use tokio::time::timeout;

    let body_str = String::from_utf8_lossy(&body).to_string();
    let state = state.read().await;
    let Some(tx) = &state.webhook_tx else {
        return (StatusCode::NOT_FOUND, "Webhook not configured".to_string()).into_response();
    };
    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    let req = WebhookRequest {
        plugin_name: name,
        path: path.to_string_lossy().to_string(),
        method: method.as_str().to_string(),
        body: body_str,
        reply: reply_tx,
    };
    if tx.send(req).await.is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Webhook channel closed".to_string(),
        )
            .into_response();
    }
    // Timeout after 30s to prevent hung requests if plugin misbehaves
    match timeout(Duration::from_secs(30), reply_rx).await {
        Ok(Ok((status, response))) => {
            (StatusCode::from_u16(status).unwrap_or(StatusCode::OK), response).into_response()
        }
        Ok(Err(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Webhook reply cancelled".to_string(),
        )
            .into_response(),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            "Webhook handler timeout (30s)".to_string(),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_cracked_reads_json_files_sorted_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("aa.json"),
            r#"{"bssid":"AA:BB:CC:DD:EE:FF","ssid":"Older","password":"pw1","cracked_at":100}"#,
        )
        .await
        .unwrap();
        tokio::fs::write(
            dir.path().join("bb.json"),
            r#"{"bssid":"11:22:33:44:55:66","ssid":"Newer","password":"pw2","cracked_at":200}"#,
        )
        .await
        .unwrap();
        // A non-JSON file in the same directory must be ignored, not error.
        tokio::fs::write(dir.path().join("notes.txt"), "irrelevant")
            .await
            .unwrap();

        let state = Arc::new(RwLock::new(AppState {
            cracked_dir: dir.path().to_path_buf(),
            ..AppState::default()
        }));

        let Json(cracked) = get_cracked(State(state)).await;
        assert_eq!(cracked.len(), 2);
        assert_eq!(cracked[0].ssid, "Newer", "expected newest-first ordering");
        assert_eq!(cracked[1].ssid, "Older");
    }

    #[tokio::test]
    async fn test_get_cracked_missing_dir_returns_empty_not_error() {
        let state = Arc::new(RwLock::new(AppState {
            cracked_dir: std::path::PathBuf::from("/nonexistent-dir-xyz/pwncrack"),
            ..AppState::default()
        }));
        let Json(cracked) = get_cracked(State(state)).await;
        assert!(cracked.is_empty());
    }

    #[tokio::test]
    async fn test_update_config_persists_patch_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut state = AppState {
            config_path: path.clone(),
            ..AppState::default()
        };
        state.config.main.name = "test-persisted-name".to_string();
        let state = Arc::new(RwLock::new(state));

        // Send only the changed field, as the real editor does.
        let patch = serde_json::json!({ "main": { "name": "renamed-via-api" } });
        let response = update_config(State(state.clone()), Json(patch)).await;
        assert_eq!(response.0["status"], "ok");

        let on_disk = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(on_disk.contains("renamed-via-api"));

        let reloaded = config::load_config(&path).await.unwrap();
        assert_eq!(reloaded.main.name, "renamed-via-api");
        assert_eq!(state.read().await.config.main.name, "renamed-via-api");
    }

    #[tokio::test]
    async fn test_update_config_patch_preserves_unspecified_sections() {
        // The data-loss regression: a patch touching only `main.name` must
        // NOT wipe a plugin api_key the client never sent. Before the
        // deep-merge fix, POSTing a whole config with `[plugins]` omitted
        // silently reset every plugin to defaults.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut state = AppState {
            config_path: path.clone(),
            ..AppState::default()
        };
        // Seed a plugin config with a secret the user set earlier.
        state.config.plugins.insert(
            "wpa_sec".to_string(),
            config::schema::PluginConfig {
                enabled: true,
                options: {
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "api_key".to_string(),
                        serde_json::Value::String("SECRET-KEY-123".to_string()),
                    );
                    m
                },
            },
        );
        let state = Arc::new(RwLock::new(state));

        // Patch only the name -- nothing about plugins.
        let patch = serde_json::json!({ "main": { "name": "new-name" } });
        let response = update_config(State(state.clone()), Json(patch)).await;
        assert_eq!(response.0["status"], "ok");

        // The api_key must still be there, in memory and on disk.
        let guard = state.read().await;
        let key = guard
            .config
            .plugins
            .get("wpa_sec")
            .and_then(|p| p.options.get("api_key"))
            .and_then(|v| v.as_str());
        assert_eq!(
            key,
            Some("SECRET-KEY-123"),
            "patch wiped the plugin api_key"
        );
        assert_eq!(guard.config.main.name, "new-name");
        drop(guard);

        let reloaded = config::load_config(&path).await.unwrap();
        assert_eq!(
            reloaded
                .plugins
                .get("wpa_sec")
                .and_then(|p| p.options.get("api_key"))
                .and_then(|v| v.as_str()),
            Some("SECRET-KEY-123"),
            "reloaded config lost the plugin api_key"
        );
    }

    #[tokio::test]
    async fn test_update_config_reports_error_on_unwritable_path() {
        // A path whose parent directory doesn't exist can't be written --
        // this must surface as a real error, not a false "ok".
        let state = AppState {
            config_path: std::path::PathBuf::from("/nonexistent-dir-xyz/config.toml"),
            ..AppState::default()
        };
        let state = Arc::new(RwLock::new(state));

        let patch = serde_json::json!({ "main": { "name": "x" } });
        let response = update_config(State(state), Json(patch)).await;
        assert_eq!(response.0["status"], "error");
    }

    #[tokio::test]
    async fn test_get_wpa_sec_cracked_parses_potfile() {
        let dir = tempfile::tempdir().unwrap();
        let potfile = dir.path().join("potfile");
        // wpa-sec format: bssid:clientmac:ssid:password (MACs bare hex).
        // Second line's password contains a colon on purpose.
        tokio::fs::write(
            &potfile,
            "aabbccddeeff:112233445566:HomeNet:hunter2\n\
             001122334455:665544332211:Cafe:p@ss:word\n\
             malformed-line-no-colons\n",
        )
        .await
        .unwrap();

        let state = Arc::new(RwLock::new(AppState {
            wpa_sec_potfile: potfile,
            ..AppState::default()
        }));

        let Json(list) = get_wpa_sec_cracked(State(state)).await;
        assert_eq!(list.len(), 2, "malformed line must be skipped");
        assert_eq!(list[0].bssid, "aa:bb:cc:dd:ee:ff");
        assert_eq!(list[0].ssid, "HomeNet");
        assert_eq!(list[0].password, "hunter2");
        assert_eq!(list[0].source, "wpa-sec");
        // Password with an embedded colon is preserved intact.
        assert_eq!(list[1].password, "p@ss:word");
    }

    #[tokio::test]
    async fn test_get_wpa_sec_cracked_missing_file_is_empty() {
        let state = Arc::new(RwLock::new(AppState {
            wpa_sec_potfile: std::path::PathBuf::from("/nonexistent-xyz/potfile"),
            ..AppState::default()
        }));
        let Json(list) = get_wpa_sec_cracked(State(state)).await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_toggle_plugin_flips_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut state = AppState {
            config_path: path.clone(),
            ..AppState::default()
        };
        state.config.plugins.insert(
            "wpa_sec".to_string(),
            config::schema::PluginConfig {
                enabled: false,
                options: std::collections::HashMap::new(),
            },
        );
        let state = Arc::new(RwLock::new(state));

        let resp = toggle_plugin(State(state.clone()), Path("wpa_sec".to_string())).await;
        assert_eq!(resp.0["status"], "ok");
        assert_eq!(resp.0["enabled"], true);
        assert!(state.read().await.config.plugins["wpa_sec"].enabled);

        // Persisted to disk and reloadable.
        let reloaded = config::load_config(&path).await.unwrap();
        assert!(reloaded.plugins["wpa_sec"].enabled);
    }

    #[tokio::test]
    async fn test_update_plugin_options_merges_not_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut state = AppState {
            config_path: path.clone(),
            ..AppState::default()
        };
        let mut existing = std::collections::HashMap::new();
        existing.insert(
            "api_key".to_string(),
            serde_json::Value::String("OLD-KEY".to_string()),
        );
        state.config.plugins.insert(
            "wpa_sec".to_string(),
            config::schema::PluginConfig {
                enabled: true,
                options: existing,
            },
        );
        let state = Arc::new(RwLock::new(state));

        // Only send api_url -- api_key must survive untouched.
        let mut patch = std::collections::HashMap::new();
        patch.insert(
            "api_url".to_string(),
            serde_json::Value::String("https://example.test/".to_string()),
        );
        let resp = update_plugin_options(
            State(state.clone()),
            Path("wpa_sec".to_string()),
            Json(patch),
        )
        .await;
        assert_eq!(resp.0["status"], "ok");

        let guard = state.read().await;
        let opts = &guard.config.plugins["wpa_sec"].options;
        assert_eq!(opts["api_key"], "OLD-KEY", "existing option was clobbered");
        assert_eq!(opts["api_url"], "https://example.test/");
        drop(guard);

        let reloaded = config::load_config(&path).await.unwrap();
        assert_eq!(reloaded.plugins["wpa_sec"].options["api_key"], "OLD-KEY");
        assert_eq!(
            reloaded.plugins["wpa_sec"].options["api_url"],
            "https://example.test/"
        );
    }

    #[tokio::test]
    async fn test_get_plugins_lists_sorted() {
        let mut state = AppState::default();
        state.config.plugins.clear();
        for (n, en) in [("zeta", true), ("alpha", false)] {
            state.config.plugins.insert(
                n.to_string(),
                config::schema::PluginConfig {
                    enabled: en,
                    options: std::collections::HashMap::new(),
                },
            );
        }
        let state = Arc::new(RwLock::new(state));
        let Json(list) = get_plugins(State(state)).await;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha", "expected name-sorted");
        assert!(!list[0].enabled);
        assert_eq!(list[1].name, "zeta");
        assert!(list[1].enabled);
    }
}
