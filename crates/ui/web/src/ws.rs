//! WebSocket manager for live updates

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

/// Live update message types
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum LiveUpdate {
    Session {
        epoch: u64,
        uptime: u64,
        aps: usize,
        handshakes: u32,
        channel: u8,
        mood: String,
        face: String,
        phrase: String,
        level: u32,
        xp: u32,
        peers: usize,
    },
    Handshake {
        id: String,
        bssid: String,
        ssid: Option<String>,
        channel: u8,
        handshake_type: String,
    },
    Peer {
        mac: String,
        name: String,
        channel: u8,
        mood: String,
        level: u32,
    },
    PeerLost {
        mac: String,
    },
    ChannelChange {
        channel: u8,
    },
    MoodChange {
        mood: String,
        face: String,
        phrase: String,
    },
    Status {
        cpu_temp: Option<f32>,
        ram_used: u64,
        ram_total: u64,
        battery: Option<u8>,
        charging: bool,
    },
    /// A real-time status line from the agent's capture/recon activity
    /// (level + free-text message, ANSI-stripped). This is the live signal
    /// of scanning/capture activity (channel hops, associations, deauths,
    /// etc.) available in the UI, so the web dashboard surfaces it directly
    /// as a scrolling feed instead of that activity being invisible outside
    /// `journalctl`.
    Activity {
        level: String,
        message: String,
    },
}

/// WebSocket manager for broadcasting updates
pub struct WebSocketManager {
    sender: broadcast::Sender<LiveUpdate>,
}

impl WebSocketManager {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self { sender }
    }

    /// Broadcast update to all connected clients
    pub fn broadcast(&self, update: LiveUpdate) {
        let _ = self.sender.send(update);
    }

    /// Subscribe to updates
    pub fn subscribe(&self) -> broadcast::Receiver<LiveUpdate> {
        self.sender.subscribe()
    }

    /// Handle a WebSocket upgrade for this manager.
    pub fn handle_upgrade(self: Arc<Self>, ws: WebSocketUpgrade) -> Response {
        ws.on_upgrade(move |socket| handle_socket(socket, self))
    }
}

/// Handle individual WebSocket connection
async fn handle_socket(socket: WebSocket, manager: Arc<WebSocketManager>) {
    let mut broadcast_rx = manager.subscribe();
    let (mut sender, mut receiver_ws) = socket.split();

    // Spawn task to forward broadcasts to WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(update) = broadcast_rx.recv().await {
            if let Ok(msg) = serde_json::to_string(&update) {
                if sender.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Drain incoming messages until the client closes (axum auto-answers pings).
    while let Some(msg) = receiver_ws.next().await {
        if matches!(msg, Ok(Message::Close(_)) | Err(_)) {
            break;
        }
    }

    send_task.abort();
    debug!("WebSocket connection closed");
}

/// Helper functions for broadcasting updates
impl WebSocketManager {
    #[allow(clippy::too_many_arguments)]
    pub fn broadcast_session(
        &self,
        epoch: u64,
        uptime: u64,
        aps: usize,
        handshakes: u32,
        channel: u8,
        mood: String,
        face: String,
        phrase: String,
        level: u32,
        xp: u32,
        peers: usize,
    ) {
        self.broadcast(LiveUpdate::Session {
            epoch,
            uptime,
            aps,
            handshakes,
            channel,
            mood,
            face,
            phrase,
            level,
            xp,
            peers,
        });
    }

    pub fn broadcast_handshake(
        &self,
        id: String,
        bssid: String,
        ssid: Option<String>,
        channel: u8,
        handshake_type: String,
    ) {
        self.broadcast(LiveUpdate::Handshake {
            id,
            bssid,
            ssid,
            channel,
            handshake_type,
        });
    }

    pub fn broadcast_peer(&self, mac: String, name: String, channel: u8, mood: String, level: u32) {
        self.broadcast(LiveUpdate::Peer {
            mac,
            name,
            channel,
            mood,
            level,
        });
    }

    pub fn broadcast_peer_lost(&self, mac: String) {
        self.broadcast(LiveUpdate::PeerLost { mac });
    }

    pub fn broadcast_channel_change(&self, channel: u8) {
        self.broadcast(LiveUpdate::ChannelChange { channel });
    }

    pub fn broadcast_mood_change(&self, mood: String, face: String, phrase: String) {
        self.broadcast(LiveUpdate::MoodChange { mood, face, phrase });
    }

    pub fn broadcast_status(
        &self,
        cpu_temp: Option<f32>,
        ram_used: u64,
        ram_total: u64,
        battery: Option<u8>,
        charging: bool,
    ) {
        self.broadcast(LiveUpdate::Status {
            cpu_temp,
            ram_used,
            ram_total,
            battery,
            charging,
        });
    }

    pub fn broadcast_activity(&self, level: String, message: String) {
        self.broadcast(LiveUpdate::Activity { level, message });
    }
}

impl Default for WebSocketManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_manager() {
        let manager = WebSocketManager::new();
        let _rx = manager.subscribe();
        // Just verify it creates
    }
}
