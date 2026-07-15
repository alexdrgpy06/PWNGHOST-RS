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
use pwncore::PwnConfig;
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
        // Verify exports exist
        let _ = AngryOxideEvent::AccessPointFound { bssid: "aa:bb:cc:dd:ee:ff".parse().unwrap(), ssid: None, channel: 1, rssi: -50, encryption: "WPA2".to_string(), vendor: String::new() };
    }
}