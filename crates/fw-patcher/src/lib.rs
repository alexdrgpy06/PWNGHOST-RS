//! Firmware patcher for BCM43436B0 (CoderFX 8-layer stability patches)
//! 
//! This crate handles:
//! - Parsing and validating CoderFX manifest.json
//! - Applying byte-level patches from inplace-v7.txt
//! - BCM chip detection (43436B0 vs 43430)
//! - GPIO power cycling via WL_REG_ON
//! - wlan_keepalive daemon installation

pub mod manifest;
pub mod patch;
pub mod detect;
pub mod gpio;
pub mod keepalive;

pub use manifest::{Manifest, PatchEntry, file_sha256, UserspaceBinaries};
pub use detect::FirmwareInfo;
pub use patch::{PatchLine, parse_patch_file, apply_patches, apply_patches_verified, backup_firmware, restore_firmware, verify_patches};
pub use detect::{BcmChip, detect_chip, check_hardware_support};
pub use gpio::{WifiPowerControl, full_wifi_recovery, check_firmware_crash, get_wifi_chip_status};
pub use keepalive::{install_keepalive, uninstall_keepalive, run_keepalive_daemon, verify_keepalive_binary};

use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

/// High-level firmware patching workflow
pub async fn patch_firmware<P: AsRef<Path>>(
    firmware_dir: P,
    patch_data_dir: P,
    backup_dir: P,
) -> Result<()> {
    let firmware_dir = firmware_dir.as_ref();
    let patch_data_dir = patch_data_dir.as_ref();
    let backup_dir = backup_dir.as_ref();

    info!("Starting firmware patching workflow");
    info!("Firmware dir: {}", firmware_dir.display());
    info!("Patch data dir: {}", patch_data_dir.display());
    info!("Backup dir: {}", backup_dir.display());

    // 1. Detect BCM chip
    let chip = detect_chip(firmware_dir)?;
    info!("Detected chip: {}", chip.as_str());

    if !chip.needs_patch() {
        info!("Chip {} does not require firmware patching", chip.as_str());
        return Ok(());
    }

    // 2. Find firmware file
    let firmware_info = FirmwareInfo::detect(firmware_dir)?
        .context("No supported firmware file found")?;
    
    info!("Found firmware: {} (SHA256: {}, size: {})", 
          firmware_info.path.display(), 
          &firmware_info.sha256[..16], 
          firmware_info.size);

    // 4. Load manifest
    let manifest_path = patch_data_dir.join("manifest.json");
    let manifest = Manifest::load(&manifest_path)?;
    
    // 5. Find matching patch
    let patch_entry = manifest.find_matching_patch(&firmware_info.sha256)
        .context("No patch entry matches firmware SHA256")?;
    
    info!("Found matching patch: {} ({})", patch_entry.name, patch_entry.description);

    // 6. Verify patch file
    let patch_file_path = patch_data_dir.join(&patch_entry.patch_file);
    patch_entry.verify_patch_file(&patch_file_path)?;

    // 7. Parse patch lines
    let patch_lines = parse_patch_file(&patch_file_path)?;
    info!("Parsed {} patch lines", patch_lines.len());

    // 8. Backup original firmware
    let backup_path = backup_firmware(firmware_info.path.as_path(), backup_dir)?;
    info!("Backed up to: {}", backup_path.display());

    // 9. Apply patches with verification
    apply_patches_verified(&firmware_info.path, &patch_lines, &patch_entry.output_sha256)?;

    // 10. Install keepalive daemon
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "armhf" };
    install_keepalive(Path::new("/usr/local/bin"), arch)?;

    info!("Firmware patching completed successfully");
    Ok(())
}

/// Check if firmware needs patching (for first-boot detection)
pub fn needs_patching<P: AsRef<Path>>(firmware_dir: P) -> Result<bool> {
    let chip = detect_chip(firmware_dir)?;
    Ok(chip.needs_patch())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_needs_patching() {
        let dir = tempdir().unwrap();
        // No firmware files, should default to needs patching
        let result = needs_patching(dir.path());
        assert!(result.is_ok());
    }
}