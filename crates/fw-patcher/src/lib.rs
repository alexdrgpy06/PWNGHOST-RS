//! Firmware patcher stub — full implementation deferred
//!
//! This crate provides firmware patching for BCM43436B0 (CoderFX 8-layer patches).
//! Full implementation is deferred; this stub ensures the workspace compiles.

use anyhow::Result;
use std::path::Path;
use tracing::info;

/// Apply firmware patches on first boot (no-op stub)
pub async fn apply_on_first_boot(_firmware_dir: &Path) -> Result<bool> {
    info!("fw-patcher stub: skipping (not implemented)");
    Ok(false)
}

/// Run firmware health monitor in background (no-op stub)
pub async fn run_monitor(_firmware_dir: &Path) -> Result<()> {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}
