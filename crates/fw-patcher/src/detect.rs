//! BCM chip detection from dmesg and device tree

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tokio::process::Command;
use tracing::info;

/// Detected BCM chip type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BcmChip {
    Bcm43430,
    Bcm43436b0,
    Bcm4345c5,
    Unknown(String),
}

impl BcmChip {
    pub fn as_str(&self) -> &str {
        match self {
            BcmChip::Bcm43430 => "43430",
            BcmChip::Bcm43436b0 => "43436B0",
            BcmChip::Bcm4345c5 => "4345c5",
            BcmChip::Unknown(s) => s,
        }
    }

    pub fn requires_coderfx_patches(&self) -> bool {
        matches!(self, BcmChip::Bcm43436b0)
    }
}

/// Detect BCM chip from dmesg and device tree
pub async fn detect_chip() -> Result<String> {
    // Try device tree first (most reliable on Pi)
    if let Ok(chip) = detect_from_dt().await {
        info!("Detected BCM chip from DT: {}", chip);
        return Ok(chip);
    }

    // Fall back to dmesg
    if let Ok(chip) = detect_from_dmesg().await {
        info!("Detected BCM chip from dmesg: {}", chip);
        return Ok(chip);
    }

    // Try nexmon module info
    if let Ok(chip) = detect_from_nexmon().await {
        info!("Detected BCM chip from nexmon: {}", chip);
        return Ok(chip);
    }

    Err(anyhow::anyhow!("Could not detect BCM chip"))
}

/// Detect from device tree
async fn detect_from_dt() -> Result<String> {
    let dt_path = Path::new("/proc/device-tree/soc/");
    if !dt_path.exists() {
        return Err(anyhow::anyhow!("Device tree not accessible"));
    }

    // Look for brcmfmac compatible strings
    let compatible_paths = [
        "/proc/device-tree/soc/*/compatible",
        "/proc/device-tree/soc/*/*/compatible",
    ];

    for pattern in compatible_paths {
        let paths = glob::glob(pattern).context("Failed to glob device tree compatibles")?;

        for path in paths.flatten() {
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("brcm,bcm43436") || content.contains("brcm,bcm43430") {
                    if content.contains("bcm43436") {
                        return Ok("43436B0".to_string());
                    } else if content.contains("bcm43430") {
                        return Ok("43430".to_string());
                    }
                }
            }
        }
    }

    Err(anyhow::anyhow!("Chip not found in device tree"))
}

/// Detect from dmesg output
async fn detect_from_dmesg() -> Result<String> {
    let output = Command::new("dmesg")
        .args(["-T", "-k"])
        .output()
        .await
        .context("Failed to run dmesg")?;

    let output = String::from_utf8_lossy(&output.stdout);

    // Look for brcmfmac lines
    for line in output.lines() {
        if line.contains("brcmfmac") && line.contains("chip") {
            if let Some(chip) = extract_chip_from_line(line) {
                return Ok(chip);
            }
        }
    }

    Err(anyhow::anyhow!("Chip not found in dmesg"))
}

/// Extract chip string from dmesg line
fn extract_chip_from_line(line: &str) -> Option<String> {
    // Pattern: "brcmfmac: brcmf_fw_alloc_request: using brcm/brcmfmac43436-sdio for chip BCM43436/1"
    if let Some(pos) = line.find("chip ") {
        let after = &line[pos + 5..];
        let chip = after.split([' ', '/', ',']).next()?;
        return Some(chip.to_string());
    }

    // Pattern: "brcmfmac: brcmf_chip_attach: Chip ID: 0xa9a6, rev 0x1"
    if let Some(pos) = line.find("Chip ID:") {
        let after = &line[pos + 8..];
        let id = after.split(',').next()?.trim();
        return Some(id.to_string());
    }

    None
}

/// Detect from nexmon module info
async fn detect_from_nexmon() -> Result<String> {
    let output = Command::new("modinfo")
        .arg("nexmon")
        .output()
        .await
        .context("Failed to run modinfo nexmon")?;

    let output = String::from_utf8_lossy(&output.stdout);

    // Parse module parameters for chip info
    for line in output.lines() {
        if line.contains("chip") || line.contains("firmware") {
            if let Some(chip) = extract_chip_from_line(line) {
                return Ok(chip);
            }
        }
    }

    Err(anyhow::anyhow!("Chip not found in nexmon module info"))
}

/// Get detailed chip info
#[derive(Debug, Clone)]
pub struct ChipInfo {
    pub chip: String,
    pub revision: String,
    pub firmware_path: String,
    pub nvram_path: String,
}

pub async fn get_chip_info() -> Result<ChipInfo> {
    let chip = detect_chip().await?;
    let (fw, nvram) = match chip.as_str() {
        "43436B0" => (
            "/lib/firmware/brcm/brcmfmac43436-sdio.bin",
            "/lib/firmware/brcm/brcmfmac43436-sdio.txt",
        ),
        "43430" => (
            "/lib/firmware/brcm/brcmfmac43430-sdio.bin",
            "/lib/firmware/brcm/brcmfmac43430-sdio.txt",
        ),
        _ => (
            "/lib/firmware/brcm/brcmfmac43436-sdio.bin",
            "/lib/firmware/brcm/brcmfmac43436-sdio.txt",
        ),
    };

    // Try to get revision from dmesg
    let output = Command::new("dmesg").args(["-T", "-k"]).output().await?;
    let output = String::from_utf8_lossy(&output.stdout);

    let mut revision = "unknown".to_string();
    for line in output.lines() {
        if line.contains("brcmfmac") && line.contains("rev") {
            if let Some(pos) = line.find("rev ") {
                let after = &line[pos + 4..];
                revision = after
                    .split([' ', ','])
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                break;
            }
        }
    }

    Ok(ChipInfo {
        chip,
        revision,
        firmware_path: fw.to_string(),
        nvram_path: nvram.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_chip_from_line() {
        let line =
            "brcmfmac: brcmf_fw_alloc_request: using brcm/brcmfmac43436-sdio for chip BCM43436/1";
        let chip = extract_chip_from_line(line);
        assert_eq!(chip, Some("BCM43436".to_string()));

        let line = "brcmfmac: brcmf_chip_attach: Chip ID: 0xa9a6, rev 0x1";
        let chip = extract_chip_from_line(line);
        assert_eq!(chip, Some("0xa9a6".to_string()));
    }

    #[test]
    fn test_bcm_chip_requires_patches() {
        assert!(BcmChip::Bcm43436b0.requires_coderfx_patches());
        assert!(!BcmChip::Bcm43430.requires_coderfx_patches());
        assert!(!BcmChip::Unknown("other".to_string()).requires_coderfx_patches());
    }
}
