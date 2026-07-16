//! Web UI for PWNGHOST-RS

pub mod api;
pub mod server;
pub mod ws;

pub use api::AppState;
pub use server::{create_router, serve};
pub use ws::{LiveUpdate, WebSocketManager};

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Web server configuration.
#[derive(Debug, Clone, Default)]
pub struct WebConfig {
    /// Optional directory of static assets to serve under `/static`.
    pub static_dir: Option<String>,
}

/// High-level web server handle wrapping the shared [`AppState`].
pub struct WebServer {
    state: Arc<RwLock<AppState>>,
}

impl WebServer {
    /// Create a new web server with default state.
    pub fn new(_config: WebConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(AppState::default())),
        }
    }

    /// Access the shared application state (for pushing live updates).
    pub fn state(&self) -> Arc<RwLock<AppState>> {
        self.state.clone()
    }

    /// Bind and serve the web UI on `addr` until the process exits.
    pub async fn serve(self, addr: SocketAddr) -> anyhow::Result<()> {
        let app = create_router(self.state);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("Web server listening on {}", addr);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_server_new() {
        let server = WebServer::new(WebConfig::default());
        let _state = server.state();
    }
}
