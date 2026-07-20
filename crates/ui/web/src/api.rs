//! REST API endpoints for Web UI

use axum::{extract::State, Json};
use pwncore::AccessPoint;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Serialize)]
pub struct SessionResponse {
    pub epoch: u64,
    pub uptime: u64,
    pub aps: usize,
    pub handshakes: u32,
    pub channel: u8,
    pub mood: String,
    pub face: String,
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
        level: state.level,
        xp: state.xp,
        peers: state.peers.len(),
    })
}

/// Return the **entire** config as JSON. Previously this returned only
/// `main`/`personality`/`ui`, omitting `bettercap`/`fs`/`oxigotchi`/
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
    let merged = match config::apply_config_patch(&state.config, &patch) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Config patch rejected: {}", e);
            return Json(serde_json::json!({"status": "error", "message": e.to_string()}));
        }
    };
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

// Shared application state
pub struct AppState {
    pub epoch: u64,
    pub uptime: u64,
    pub aps: Vec<AccessPoint>,
    pub handshakes: u32,
    pub current_channel: u8,
    pub mood: pwncore::Mood,
    pub face: String,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(key, Some("SECRET-KEY-123"), "patch wiped the plugin api_key");
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
}
