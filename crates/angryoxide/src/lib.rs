//! AngryOxide subprocess manager and JSON parser

pub mod args;
pub mod parser;
pub mod recovery;
pub mod spawn;

pub use args::AngryOxideConfig;
pub use parser::{parse_status_line, watch_output_dir, AoEvent, StatusLevel};
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
        // Verify the public parser API round-trips a real AngryOxide status
        // line (the honest event vocabulary - no fabricated JSON protocol).
        let line = "2024-01-01 00:00:00 UTC |  Status  | Starting interface wlan0";
        let event: AoEvent = parse_status_line(line).unwrap();
        assert!(matches!(event, AoEvent::StatusLine { .. }));
    }
}
