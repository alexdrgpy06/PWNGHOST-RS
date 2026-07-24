//! Web server for PWNGHOST-RS

use axum::{
    extract::{DefaultBodyLimit, Request, State, WebSocketUpgrade},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
    Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use crate::api::{
    get_config, get_cracked, get_handshakes, get_peers, get_plugins, get_session, get_status,
    get_ui_frame, get_wpa_sec_cracked, plugin_webhook, reboot_system, toggle_plugin,
    update_config, update_plugin_options, AppState,
};

/// Create the web application router
pub fn create_router(state: Arc<RwLock<AppState>>) -> Router {
    // Routes that expose sensitive data (captured handshakes, cracked
    // passwords, full config) or control the device (config writes, reboot)
    // sit behind HTTP Basic auth, mirroring real pwnagotchi, which wraps every
    // web route in `with_auth` (`ui/web/handler.py`). Previously PWNGHOST-RS
    // enforced nothing, so anyone on the network could read captures and
    // reboot the unit -- a real security regression from the original.
    //
    // `/ws` (live activity) and `/static` (assets) stay open: a browser cannot
    // attach an `Authorization` header to a WebSocket handshake, and the
    // dashboard page that opens the socket is itself behind auth.
    let protected = Router::new()
        .route("/api/status", get(get_status))
        .route("/api/session", get(get_session))
        .route("/api/config", get(get_config).post(update_config))
        .route("/api/peers", get(get_peers))
        .route("/api/handshakes", get(get_handshakes))
        .route("/api/cracked", get(get_cracked))
        .route("/api/wpa-sec/cracked", get(get_wpa_sec_cracked))
        .route("/api/plugins", get(get_plugins))
        .route("/api/plugins/:name/toggle", post(toggle_plugin))
        .route("/api/plugins/:name/options", post(update_plugin_options))
        // Accept any HTTP method (GET, POST, etc.) so Lua plugins can
        // define REST-style handlers without method restrictions.
        .route("/api/plugins/:name/webhook/*path", any(plugin_webhook))
        .route("/api/reboot", post(reboot_system))
        // Live e-ink frame as PNG, polled ~1s by the dashboard -- the same
        // "live view" real pwnagotchi serves at `/ui`.
        .route("/ui", get(get_ui_frame))
        .route("/", get(index_handler))
        .layer(middleware::from_fn_with_state(state.clone(), basic_auth));

    let open = Router::new()
        .route("/ws", get(websocket_handler))
        .nest_service("/static", ServeDir::new("static"));

    protected
        .merge(open)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(DefaultBodyLimit::max(1_048_576)) // 1 MiB body limit
        .with_state(state)
}

/// HTTP Basic auth guard for the sensitive routes. Reads `ui.web.username` /
/// `ui.web.password` from the live config. An **empty username disables auth**
/// (explicit opt-out) so headless/kiosk setups can turn it off deliberately;
/// otherwise a missing/incorrect credential gets a `401` with a
/// `WWW-Authenticate: Basic` challenge (so a browser prompts for login).
async fn basic_auth(
    State(state): State<Arc<RwLock<AppState>>>,
    req: Request,
    next: Next,
) -> Response {
    let (auth_on, user, pass) = {
        let s = state.read().await;
        (
            s.config.ui.web.auth,
            s.config.ui.web.username.clone(),
            s.config.ui.web.password.clone(),
        )
    };
    // Gate on the explicit `ui.web.auth` flag (matches the config field and
    // real pwnagotchi's toggle). Auth off, or no username configured to check
    // against, => open. Otherwise require valid Basic credentials.
    if !auth_on || user.is_empty() || request_has_valid_basic_auth(&req, &user, &pass) {
        return next.run(req).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"pwnghost\"")],
        "401 Unauthorized",
    )
        .into_response()
}

fn request_has_valid_basic_auth(req: &Request, user: &str, pass: &str) -> bool {
    let Some(value) = req.headers().get(header::AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some(b64) = value.strip_prefix("Basic ") else {
        return false;
    };
    let Some(decoded) = base64_decode(b64.trim()) else {
        return false;
    };
    let Ok(creds) = String::from_utf8(decoded) else {
        return false;
    };
    match creds.split_once(':') {
        Some((u, p)) => u == user && p == pass,
        None => false,
    }
}

/// Minimal standard-alphabet base64 decoder for the `Authorization: Basic`
/// header, so the web crate needs no extra dependency. Returns `None` on any
/// invalid character; tolerates missing `=` padding.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = input.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &c in bytes {
        buf = (buf << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
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

    #[test]
    fn test_base64_decode() {
        assert_eq!(
            base64_decode("Y2hhbmdlbWU6Y2hhbmdlbWU=").unwrap(),
            b"changeme:changeme"
        );
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("not valid !!!").is_none());
    }

    #[tokio::test]
    async fn test_protected_route_requires_auth() {
        use axum::body::Body;
        use axum::http::Request as HttpRequest;
        use tower::ServiceExt;

        let state = Arc::new(RwLock::new(AppState::default()));
        {
            let mut s = state.write().await;
            s.config.ui.web.auth = true;
            s.config.ui.web.username = "changeme".to_string();
            s.config.ui.web.password = "changeme".to_string();
        }
        let router = create_router(state);

        // No credentials -> 401 with a Basic challenge.
        let resp = router
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().contains_key(header::WWW_AUTHENTICATE));

        // Correct credentials -> not 401.
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/status")
                    .header(header::AUTHORIZATION, "Basic Y2hhbmdlbWU6Y2hhbmdlbWU=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_off_by_default_is_open() {
        use axum::body::Body;
        use axum::http::Request as HttpRequest;
        use tower::ServiceExt;

        // Default config has `ui.web.auth = false` -> routes are open (matches
        // the shipped overlay config; the daemon must not lock the user out).
        let state = Arc::new(RwLock::new(AppState::default()));
        let router = create_router(state);
        let resp = router
            .oneshot(
                HttpRequest::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
