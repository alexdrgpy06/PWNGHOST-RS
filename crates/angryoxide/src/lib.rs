//! AngryOxide subprocess manager and JSON parser

pub mod args;
pub mod parser;
pub mod recovery;
pub mod spawn;

pub use args::AngryOxideConfig;
pub use parser::{parse_json_line, AoEvent};
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
        // Verify the public parser API round-trips a real AngryOxide event.
        let json = r#"{"type":"ap","bssid":"aa:bb:cc:dd:ee:ff","ssid":"TestAP","channel":1,"rssi":-50,"encryption":"WPA2","vendor":"","clients":[],"first_seen":0,"last_seen":0}"#;
        let event: AoEvent = parse_json_line(json).unwrap();
        assert!(matches!(event, AoEvent::Ap(_)));
    }
}
