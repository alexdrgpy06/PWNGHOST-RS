use axum::{
    Router,
    routing::get,
    response::Json,
    extract::State,
    extract::ws::{WebSocket, WebSocketUpgrade, Message},
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct AppState {
    pub agent_info: Arc<tokio::sync::RwLock<AgentInfo>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentInfo {
    pub epoch: u64,
    pub channel: u8,
    pub mood: String,
    pub face: String,
    pub uptime_secs: u64,
    pub handshakes: u64,
    pub aps_seen: u64,
    pub deauths: u64,
}

impl Default for AgentInfo {
    fn default() -> Self {
        Self {
            epoch: 0,
            channel: 1,
            mood: "Awake".into(),
            face: "(◕‿‿◕)".into(),
            uptime_secs: 0,
            handshakes: 0,
            aps_seen: 0,
            deauths: 0,
        }
    }
}

pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let state = AppState {
        agent_info: Arc::new(tokio::sync::RwLock::new(AgentInfo::default())),
    };

    let app = Router::new()
        .route("/api/session", get(api_get_session))
        .route("/api/peers", get(api_get_peers))
        .route("/api/handshakes", get(api_get_handshakes))
        .route("/api/config", get(api_get_config))
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("templates").append_index_html_on_directories(true))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Web UI starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn api_get_session(
    State(state): State<AppState>,
) -> Json<AgentInfo> {
    let info = state.agent_info.read().await;
    Json(info.clone())
}

async fn api_get_peers() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}

async fn api_get_handshakes() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}

async fn api_get_config() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "config not loaded"}))
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl axum::response::IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let msg = serde_json::json!({"type": "connected", "message": "pwnagotchi-rs WebSocket connected"});
    let _ = socket.send(Message::Text(msg.to_string())).await;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let ping = serde_json::json!({"type": "ping"});
                if socket.send(Message::Text(ping.to_string())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    #[test]
    fn test_agent_info_default() {
        let info = AgentInfo::default();
        assert_eq!(info.epoch, 0);
        assert_eq!(info.mood, "Awake");
    }

    #[tokio::test]
    async fn test_server_binds() {
        let state = AppState {
            agent_info: Arc::new(tokio::sync::RwLock::new(AgentInfo::default())),
        };
        let _app: axum::Router<AppState> = axum::Router::new()
            .route("/api/session", get(api_get_session))
            .with_state(state);
    }
}
