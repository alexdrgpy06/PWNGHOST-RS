//! Web server for PWNGHOST-RS

use axum::extract::ws::WebSocketUpgrade;
use axum::{extract::State, response::Html, routing::get, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use crate::api::{
    get_config, get_handshakes, get_peers, get_session, get_status, update_config, AppState,
};

/// Create the web application router
pub fn create_router(state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/session", get(get_session))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/peers", get(get_peers))
        .route("/api/handshakes", get(get_handshakes))
        .route("/ws", get(websocket_handler))
        .nest_service("/static", ServeDir::new("static"))
        .route("/", get(index_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

/// WebSocket upgrade handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<RwLock<AppState>>>,
) -> impl axum::response::IntoResponse {
    let manager = {
        let state = state.read().await;
        state.ws_manager.clone()
    };
    manager.handle_upgrade(ws)
}

/// Index page handler
async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../templates/index.html"))
}

/// Start the web server
pub async fn serve(addr: &str, state: Arc<RwLock<AppState>>) -> anyhow::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Web server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_router() {
        let state = Arc::new(RwLock::new(AppState::default()));
        let _router = create_router(state);
    }
}
