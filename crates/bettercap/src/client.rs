//! Blocking REST client for bettercap's `api.rest` module, matching real
//! pwnagotchi's `pwnagotchi/bettercap.py::Client` exactly (same URL shape,
//! same Basic Auth, same `{"cmd": "..."}`-over-POST command protocol --
//! confirmed by reading that file directly, not guessed).
//!
//! bettercap's REST API is loopback-only plain HTTP in this project's setup
//! (the bettercap systemd unit binds `api.rest.address` to `127.0.0.1`), so
//! a minimal blocking client (`ureq`, no TLS backend needed) wrapped in
//! `tokio::task::spawn_blocking` is simpler and far lighter to cross-compile
//! than an async+TLS HTTP stack for this one use -- the same pattern this
//! project already uses for the hardware display's blocking SPI/GPIO writes
//! (`crates/ui/display/src/driver.rs`).

use crate::session::WifiSession;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

/// bettercap's `POST /api/session` response shape
/// (`modules/api_rest/api_rest_controller.go::APIResponse`).
#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    success: bool,
    // bettercap's Go struct (`modules/api_rest/api_rest_controller.go::
    // APIResponse`) tags this field `json:"msg"`, not `message` -- without
    // the rename, serde silently left this at its `#[serde(default)]`
    // empty string on every real response, so `run_command`'s failure
    // messages never actually contained bettercap's reported reason.
    #[serde(default, rename = "msg")]
    message: String,
}

/// Blocking REST client for one bettercap instance.
#[derive(Clone)]
pub struct BettercapClient {
    base_url: String,
    username: String,
    password: String,
    agent: ureq::Agent,
}

impl BettercapClient {
    pub fn new(hostname: &str, port: u16, username: &str, password: &str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();
        Self {
            base_url: format!("http://{hostname}:{port}/api"),
            username: username.to_string(),
            password: password.to_string(),
            agent,
        }
    }

    /// Run one or more bettercap console commands (semicolon-separated, same
    /// as typing them into the bettercap console), e.g. `"wifi.recon on"` or
    /// `"set wifi.handshakes.file /path; set wifi.handshakes.aggregate false"`.
    pub fn run_command(&self, cmd: &str) -> Result<()> {
        let url = format!("{}/session", self.base_url);
        let resp = match self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .auth(&self.username, &self.password)
            .send_json(serde_json::json!({ "cmd": cmd }))
        {
            Ok(resp) => resp,
            // `ureq::Error::Status`'s own `Display` never includes the
            // response body, which is exactly where bettercap puts its
            // actual error text (e.g. "interface not in monitor mode") --
            // read it explicitly so real failure reasons reach the logs
            // instead of a bare "unexpected status code" being all that
            // survives up through `check_healing`'s warn!()s in main.rs.
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("POST {url} (cmd={cmd:?}) failed: HTTP {code}: {body}");
            }
            Err(e) => {
                return Err(e).with_context(|| format!("POST {url} (cmd={cmd:?})"));
            }
        };

        let parsed: ApiResponse = resp
            .into_json()
            .context("decoding bettercap command response")?;
        if !parsed.success {
            anyhow::bail!("bettercap command {cmd:?} failed: {}", parsed.message);
        }
        Ok(())
    }

    /// Fetch the current WiFi module state (`GET /api/session/wifi`): every
    /// AP bettercap has observed, with its clients and per-AP handshake
    /// status. Source-grounded shape -- see `crate::session`'s doc comment.
    pub fn wifi_session(&self) -> Result<WifiSession> {
        let url = format!("{}/session/wifi", self.base_url);
        let resp = match self.agent.get(&url).auth(&self.username, &self.password).call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::bail!("GET {url} failed: HTTP {code}: {body}");
            }
            Err(e) => {
                return Err(e).with_context(|| format!("GET {url}"));
            }
        };
        resp.into_json()
            .context("decoding bettercap wifi session response")
    }
}

trait RequestAuthExt {
    fn auth(self, username: &str, password: &str) -> Self;
}

impl RequestAuthExt for ureq::Request {
    fn auth(self, username: &str, password: &str) -> Self {
        // ureq has no built-in basic-auth helper; build the header directly
        // (`base64` is already a workspace dependency for exactly this kind
        // of use elsewhere in the project).
        use base64::Engine;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        self.set("Authorization", &format!("Basic {encoded}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builds_expected_urls() {
        let client = BettercapClient::new("127.0.0.1", 8081, "user", "pass");
        assert_eq!(client.base_url, "http://127.0.0.1:8081/api");
    }
}
