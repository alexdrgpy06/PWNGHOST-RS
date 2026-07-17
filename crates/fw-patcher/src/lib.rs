//! Firmware stability support for BCM43436B0 (Raspberry Pi Zero W / 2 W).
//!
//! # History: the "CoderFX 8-layer patch" engine is unused
//! This crate originally shipped a binary firmware-patcher (see the
//! `patch`/`manifest` modules, now gated behind the off-by-default
//! `legacy-binary-patch` feature) that parsed an `inplace-v7.txt`-style
//! patch file plus a `manifest.json`, byte-patched a firmware `.bin`, and
//! verified hashes throughout, attributed to "CoderFX's BCM43436B0 8-layer
//! firmware patches". A thorough search of this entire workspace found
//! **no such patch data anywhere** -- no firmware blobs, no
//! `inplace*.txt`, no `manifest.json`. The engine had nothing to operate
//! on, so it is no longer wired into [`apply_on_first_boot`].
//!
//! The real, working fix for BCM43436B0 SDIO-bus crashes is a SOURCE-level
//! patch applied to nexmon *before* it is compiled (disabling a
//! `reload_brcm` call that otherwise kills the SDIO bus), applied at
//! image-build time as part of `pi-gen`'s nexmon kernel-build stage (owned
//! by a different agent/workstream) -- see `oxigotchi/tools/apply_patches.sh`
//! for the reference mechanism. By the time this crate's binary runs on a
//! deployed device, nexmon/brcmfmac is already compiled and flashed into
//! the image, so a runtime Rust binary cannot perform that fix itself.
//!
//! What this crate provides at runtime instead:
//! - [`detect`]: identify the BCM chip (device tree / dmesg / nexmon
//!   modinfo), normalized to one canonical string form so every gate in
//!   this crate (and consumers like `crates/radio`) agrees on it.
//! - [`monitor`]: poll nexmon's SDIO RAMRW netlink interface for firmware
//!   crash counters and preventive PSM/DPC/RSSI watchdog counters, cross-
//!   checked against the sibling `oxigotchi` implementation.
//! - [`keepalive`]: manage the wlan_keepalive daemon that keeps the SDIO
//!   bus from going idle -- the actual mechanism that prevents "Firmware
//!   has halted" crashes on BCM43436B0 (see `vendor/wlan_keepalive.c`).
//! - [`gpio`]: WL_REG_ON control, retained for manual/diagnostic use only.
//!   **Do not** wire this into automatic recovery for BCM43436B0: toggling
//!   WL_REG_ON reverts the chip to stock (non-nexmon) firmware, losing
//!   monitor-mode support until a physical power cycle -- a hard-won lesson
//!   from the `oxigotchi` sibling project (see `gpio.rs` doc comment).

pub mod detect;
pub mod gpio;
pub mod keepalive;
pub mod monitor;

/// Legacy CoderFX binary-patch engine. Unused by default -- see module docs
/// on `patch` and `manifest` for why, and where the real fix lives instead.
#[cfg(feature = "legacy-binary-patch")]
pub mod manifest;
#[cfg(feature = "legacy-binary-patch")]
pub mod patch;

use anyhow::Result;
use std::path::Path;
use tracing::{info, warn};

/// First-boot firmware setup for BCM43436B0.
///
/// This no longer applies a binary firmware patch (see crate-level docs) --
/// BCM43436B0 stability comes from pi-gen's nexmon source patch (applied at
/// image-build time) plus this crate's runtime keepalive daemon and crash
/// monitor. What this function actually does on first boot:
///   1. Detect the BCM chip.
///   2. For BCM43436B0, ensure the wlan_keepalive systemd unit is installed
///      (see `keepalive::install_keepalive_service`).
///   3. Write a marker file so this only runs once.
///
/// Deliberately does NOT power-cycle WL_REG_ON (see `gpio` module doc) --
/// that GPIO toggle reverts BCM43436B0 to stock firmware and would undo
/// whatever nexmon patch pi-gen baked into the image.
///
/// Returns `Ok(true)` if BCM43436B0-specific setup ran, `Ok(false)` if
/// setup was already done or the detected chip doesn't need it.
pub async fn apply_on_first_boot(firmware_dir: &Path) -> Result<bool> {
    // Check if already set up
    let marker = firmware_dir.join(".patched");
    if marker.exists() {
        info!("Firmware first-boot setup already done, skipping");
        return Ok(false);
    }

    // Detect BCM chip (canonical form, see detect.rs)
    let chip = detect::detect_chip().await?;
    info!("Detected BCM chip: {}", chip);

    if let Err(e) = tokio::fs::create_dir_all(firmware_dir).await {
        warn!(
            "Could not create firmware dir {}: {}",
            firmware_dir.display(),
            e
        );
    }

    if chip == detect::CANONICAL_43436B0 {
        keepalive::install_keepalive_service().await?;

        tokio::fs::write(&marker, b"setup-complete").await?;
        info!("BCM43436B0 first-boot setup complete");
        Ok(true)
    } else {
        info!("Chip {} does not require BCM43436B0-specific setup", chip);
        tokio::fs::write(&marker, b"no-setup-needed").await?;
        Ok(false)
    }
}

/// Run firmware health monitor in background (polls crash counters and
/// resets preventive watchdog counters forever -- see `monitor` module).
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
