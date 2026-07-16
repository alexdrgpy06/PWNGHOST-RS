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
        // Load the CoderFX manifest describing the firmware + patch files
        let manifest_path = firmware_dir.join("manifest.json");
        let manifest = manifest::Manifest::load(&manifest_path)?;

        let firmware_path = firmware_dir.join(&manifest.firmware.filename);
        let patch_path = firmware_dir.join(&manifest.patches.filename);

        // Verify the inputs match the manifest before touching anything
        if !manifest.verify_firmware(&firmware_path)? {
            anyhow::bail!("Firmware file does not match manifest hash/size");
        }
        if !manifest.verify_patches(&patch_path)? {
            anyhow::bail!("Patch file does not match manifest hash/size");
        }

        // Apply CoderFX patches (verifies output hash when provided)
        patch::apply_patches(&firmware_path, &patch_path, &manifest.patches.output_sha256)?;
        gpio::power_cycle_wl_reg_on().await?;
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

/// Run firmware health monitor in background (polls crash counters forever)
pub async fn run_monitor(_firmware_dir: &Path) -> Result<()> {
    let monitor = monitor::FirmwareMonitor::new();
    monitor::run_monitor_task(monitor, 30).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fw_patcher_structure() {
        // Verify crate compiles and modules exist
    }
}
