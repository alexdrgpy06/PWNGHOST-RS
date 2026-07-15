//! Web UI for PWNGHOST-RS

pub mod api;
pub mod server;
pub mod ws;

pub use server::{create_router, serve, AppState};

use axum::{
    Router,
    routing::get,
    extract::State,
    response::Html,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Web server state
#[derive(Clone)]
pub struct WebState {
    // Web-specific state
}

impl Default for WebState {
    fn default() -> Self {
        Self {}
    }
}

/// Create web routes
pub fn web_routes() -> Router<Arc<RwLock<WebState>>> {
    Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health_handler))
}

/// Index page handler
async fn index_handler() -> Html<String> {
    Html(include_str!("../templates/index.html"))
}

/// Health check
async fn health_handler() -> Html<String> {
    Html("OK".to_string())
}

/// Start web server
pub async fn start_server(addr: &str, state: Arc<RwLock<WebState>>) -> anyhow::Result<()> {
    let app = web_routes().with_state(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Web server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_routes() {
        let state = Arc::new(RwLock::new(WebState::default()));
        let _router = web_routes().with_state(state);
    }
}