//! Firmware patcher for BCM43436B0 (CoderFX 8-layer patches)

pub mod detect;
pub mod gpio;
pub mod keepalive;
pub mod manifest;
pub mod monitor;
pub mod patch;

use anyhow::Result;
use std::path::Path;
use tracing::info;

/// Apply firmware patches on first boot
pub async fn apply_on_first_boot(firmware_dir: &Path) -> Result<bool> {
    // Check if already patched
    let marker = firmware_dir.join(".patched");
    if marker.exists() {
        info!("Firmware already patched, skipping");
        return Ok(false);
    }

    // Detect BCM chip
    let chip = detect::detect_chip().await?;
    info!("Detected BCM chip: {}", chip);

    if chip == "43436B0" {
        // Apply CoderFX patches
        patch::apply_patches(firmware_dir).await?;
        gpio::power_cycle_wl_reg_on_default().await?;
        keepalive::install_keepalive_script().await?;

        // Write marker
        tokio::fs::write(&marker, b"patched").await?;
        info!("Firmware patching complete");
        Ok(true)
    } else {
        info!("Chip {} does not require CoderFX patches", chip);
        tokio::fs::write(&marker, b"no-patch-needed").await?;
        Ok(false)
    }
}

/// Run firmware health monitor in background
pub async fn run_monitor(firmware_dir: &Path) -> Result<()> {
    let mut monitor = monitor::FirmwareMonitor::new(firmware_dir.to_path_buf());
    monitor.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fw_patcher_structure() {
        // Verify crate compiles and modules exist
    }
}