//! REST API endpoints for Web UI

use axum::{Json, extract::State};
use pwncore::{AccessPoint, SessionStats};
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

pub async fn get_session(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<SessionResponse> {
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

#[derive(Serialize)]
pub struct ConfigResponse {
    pub main: crate::config::MainConfig,
    pub personality: crate::config::PersonalityConfig,
    pub ui: crate::config::UiConfig,
}

pub async fn get_config(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<ConfigResponse> {
    let state = state.read().await;
    Json(ConfigResponse {
        main: state.config.main.clone(),
        personality: state.config.personality.clone(),
        ui: state.config.ui.clone(),
    })
}

pub async fn update_config(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(config): Json<crate::config::PwnConfig>,
) -> Json<serde_json::Value> {
    let mut state = state.write().await;
    state.config = config;
    // Save to disk would happen here
    Json(serde_json::json!({"status": "ok"}))
}

#[derive(Serialize)]
pub struct PeerResponse {
    pub mac: String,
    pub name: String,
    pub channel: u8,
    pub mood: String,
    pub level: u32,
    pub last_seen: i64,
}

pub async fn get_peers(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<Vec<PeerResponse>> {
    let state = state.read().await;
    Json(state.peers.iter().map(|p| PeerResponse {
        mac: p.mac.to_string(),
        name: p.name.clone(),
        channel: p.channel,
        mood: format!("{:?}", p.mood),
        level: p.level,
        last_seen: p.last_seen.timestamp(),
    }).collect())
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
    Json(state.handshakes.iter().map(|h| HandshakeResponse {
        id: h.id.to_string(),
        bssid: h.bssid.to_string(),
        ssid: h.ssid.clone(),
        channel: h.channel.value(),
        handshake_type: format!("{:?}", h.handshake_type),
        captured_at: h.captured_at.timestamp(),
        file: h.pcapng_path.clone(),
    }).collect())
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

pub async fn get_status(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<StatusResponse> {
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
    pub handshakes_list: Vec<crate::pwncore::Handshake>,
    pub config: crate::config::PwnConfig,
    pub cpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub battery: Option<u8>,
    pub charging: bool,
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
            config: crate::config::PwnConfig::default(),
            cpu_temp: None,
            ram_used: 0,
            ram_total: 0,
            battery: None,
            charging: false,
        }
    }
}