use serde_json::Value;
use std::sync::Arc;

pub type SharedState = Arc<tokio::sync::RwLock<crate::server::AgentInfo>>;

pub fn api_status() -> Value {
    serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_status_returns_ok() {
        let status = api_status();
        assert_eq!(status["status"], "ok");
    }
}
