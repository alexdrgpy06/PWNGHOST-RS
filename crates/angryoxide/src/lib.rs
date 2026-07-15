//! AngryOxide subprocess manager and JSON parser

pub mod args;
pub mod parser;
pub mod recovery;
pub mod spawn;

pub use args::AngryOxideConfig;
pub use parser::{AoEvent, parse_json_line};
pub use recovery::RecoveryManager;
pub use spawn::{spawn_angryoxide, AngryOxideHandle};

use anyhow::Result;
use tracing::info;

/// Initialize AngryOxide with config
pub async fn init(config: &AngryOxideConfig) -> Result<AngryOxideHandle> {
    info!("Initializing AngryOxide subprocess...");
    let handle = spawn_angryoxide(config).await?;
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure() {
        // Verify the crate modules are accessible
        let _ = parser::AngryOxideEvent::Ap(parser::ApEvent {
            bssid: [0xaa; 6],
            ssid: None,
            channel: 1,
            rssi: -50,
            encryption: parser::EncryptionType::Wpa2,
            vendor: None,
            distance: None,
            clients: vec![],
            first_seen: 0,
            last_seen: 0,
            beacon: 0,
            beacon_interval: 100,
        });
    }

    #[test]
    fn test_ao_event_display() {
        let event = parser::AngryOxideEvent::Status(parser::StatusEvent {
            level: "info".to_string(),
            message: "AngryOxide started".to_string(),
            timestamp: 1000,
        });
        let _ = format!("{:?}", event);
    }
}