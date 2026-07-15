//! BCM43436B0 patchram loader for Bluetooth firmware

use anyhow::Result;
use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{info, warn};

/// Load BCM43436B0 patchram firmware
pub async fn load_patchram(chip: &str) -> Result<()> {
    info!("Loading patchram for chip: {}", chip);

    // Find patchram binary
    let patchram_bin = find_patchram_binary().await?;

    // Determine firmware file
    let firmware = match chip {
        "bcm43436b0" | "43436B0" => "/lib/firmware/brcm/BCM43436B0.hcd",
        _ => "/lib/firmware/brcm/BCM43436B0.hcd",
    };

    // Check if firmware exists
    if !tokio::fs::metadata(firmware).await.is_ok() {
        warn!("Firmware not found at {}, trying alternative", firmware);
    }

    // Run patchram
    let status = AsyncCommand::new(&patchram_bin)
        .args([
            "--patchram", firmware,
            "--enable_hci",
            "--no2bytes",
            "--tosleep", "1000",
            "--baudrate", "3000000",
            "/dev/ttyAMA0", // UART device for Pi Zero 2W
        ])
        .status()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to execute patchram: {}", e))?;

    if !status.success() {
        anyhow::bail!("patchram failed with exit code: {:?}", status.code());
    }

    info!("Patchram loaded successfully for {}", chip);
    Ok(())
}

async fn find_patchram_binary() -> Result<String> {
    let candidates = [
        "/usr/local/bin/brcm_patchram_plus",
        "/usr/bin/brcm_patchram_plus",
        "/usr/sbin/brcm_patchram_plus",
    ];

    for bin in &candidates {
        if tokio::fs::metadata(bin).await.is_ok() {
            return Ok(bin.to_string());
        }
    }

    // Try which
    let output = AsyncCommand::new("which")
        .arg("brcm_patchram_plus")
        .output()
        .await?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(path);
        }
    }

    Err(anyhow::anyhow!("brcm_patchram_plus not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patchram_module_structure() {
        // Just verify module compiles
    }
}