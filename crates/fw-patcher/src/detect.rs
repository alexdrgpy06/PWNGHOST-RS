use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

/// BCM WiFi chip variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BcmChip {
    /// BCM43436B0 - Raspberry Pi Zero 2W (needs firmware patch)
    BCM43436B0,
    /// BCM43430 - Raspberry Pi Zero W (original, no patch needed)
    BCM43430,
    /// BCM43455 - Raspberry Pi 3B+/4/5 (different chip, not supported by this patcher)
    BCM43455,
    /// Unknown/unsupported chip
    Unknown,
}

impl BcmChip {
    pub fn as_str(&self) -> &'static str {
        match self {
            BcmChip::BCM43436B0 => "BCM43436B0",
            BcmChip::BCM43430 => "BCM43430",
            BcmChip::BCM43455 => "BCM43455",
            BcmChip::Unknown => "UNKNOWN",
        }
    }

    pub fn needs_patch(&self) -> bool {
        matches!(self, BcmChip::BCM43436B0)
    }

    pub fn is_pi_zero_w(&self) -> bool {
        matches!(self, BcmChip::BCM43430)
    }

    pub fn is_pi_zero_2w(&self) -> bool {
        matches!(self, BcmChip::BCM43436B0)
    }
}

/// Firmware file information
#[derive(Debug, Clone)]
pub struct FirmwareInfo {
    pub path: std::path::PathBuf,
    pub sha256: String,
    pub size: u64,
}

impl FirmwareInfo {
    /// Detect firmware file in a directory
    pub fn detect<P: AsRef<Path>>(firmware_dir: P) -> Result<Option<Self>> {
        let firmware_dir = firmware_dir.as_ref();
        
        let candidates = [
            "brcmfmac43436-sdio.bin",
            "brcmfmac43430-sdio.bin",
            "brcmfmac43455-sdio.bin",
        ];

        for name in candidates {
            let path = firmware_dir.join(name);
            if path.exists() {
                let metadata = fs::metadata(&path)?;
                let sha256 = file_sha256(&path)?;
                return Ok(Some(FirmwareInfo {
                    path: path.clone(),
                    sha256,
                    size: metadata.len(),
                }));
            }
        }

        Ok(None)
    }
}

/// Compute SHA256 of a file
pub fn file_sha256<P: AsRef<std::path::Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file for hashing: {}", path.display()))?;
    let hash = Sha256::digest(&content);
    Ok(hex::encode(hash))
}

/// Detect BCM chip from device tree
pub fn detect_from_dtb() -> Result<BcmChip> {
    // Try to read from device tree
    let dt_paths = [
        "/proc/device-tree/compatible",
        "/proc/device-tree/model",
    ];

    for path in dt_paths {
        if let Ok(content) = fs::read_to_string(path) {
            let content = content.to_lowercase();
            debug!("DT {}: {}", path, content);

            if content.contains("bcm43436") || content.contains("pi zero 2 w") {
                return Ok(BcmChip::BCM43436B0);
            }
            if content.contains("bcm43430") || content.contains("pi zero w") {
                return Ok(BcmChip::BCM43430);
            }
            if content.contains("bcm43455") {
                return Ok(BcmChip::BCM43455);
            }
        }
    }

    Ok(BcmChip::Unknown)
}

/// Detect BCM chip from dmesg (brcmfmac messages)
pub fn detect_from_dmesg() -> Result<BcmChip> {
    let output = std::process::Command::new("dmesg")
        .args(["-T", "-k", "-l", "info"])
        .output()
        .context("Failed to run dmesg")?;

    let dmesg = String::from_utf8_lossy(&output.stdout).to_lowercase();
    
    // Look for brcmfmac firmware messages
    for line in dmesg.lines() {
        if line.contains("brcmfmac") && (line.contains("chip") || line.contains("firmware")) {
            debug!("dmesg: {}", line.trim());
            
            if line.contains("bcm43436") {
                return Ok(BcmChip::BCM43436B0);
            }
            if line.contains("bcm43430") || line.contains("bcm43431") {
                return Ok(BcmChip::BCM43430);
            }
            if line.contains("bcm43455") {
                return Ok(BcmChip::BCM43455);
            }
        }
    }

    Ok(BcmChip::Unknown)
}

/// Detect BCM chip from firmware file name/size
pub fn detect_from_firmware<P: AsRef<Path>>(firmware_dir: P) -> Result<BcmChip> {
    let firmware_dir = firmware_dir.as_ref();
    
    let candidates = [
        ("brcmfmac43436-sdio.bin", BcmChip::BCM43436B0),
        ("brcmfmac43430-sdio.bin", BcmChip::BCM43430),
        ("brcmfmac43455-sdio.bin", BcmChip::BCM43455),
    ];

    for (name, chip) in candidates {
        let path = firmware_dir.join(name);
        if path.exists() {
            let metadata = fs::metadata(&path)?;
            info!("Found firmware: {} ({} bytes)", name, metadata.len());
            return Ok(chip);
        }
    }

    Ok(BcmChip::Unknown)
}

/// Detect BCM chip using multiple methods
pub fn detect_chip<P: AsRef<Path>>(firmware_dir: P) -> Result<BcmChip> {
    // Priority: device tree > dmesg > firmware file
    if let Ok(chip) = detect_from_dtb() {
        if chip != BcmChip::Unknown {
            info!("Detected chip via device tree: {}", chip.as_str());
            return Ok(chip);
        }
    }

    if let Ok(chip) = detect_from_dmesg() {
        if chip != BcmChip::Unknown {
            info!("Detected chip via dmesg: {}", chip.as_str());
            return Ok(chip);
        }
    }

    if let Ok(chip) = detect_from_firmware(firmware_dir.as_ref()) {
        if chip != BcmChip::Unknown {
            info!("Detected chip via firmware file: {}", chip.as_str());
            return Ok(chip);
        }
    }

    warn!("Could not detect BCM chip, assuming BCM43436B0 (Pi Zero 2W)");
    Ok(BcmChip::BCM43436B0)
}

/// Check if running on supported hardware
pub fn check_hardware_support() -> Result<()> {
    // Read model
    let model = fs::read_to_string("/proc/device-tree/model")
        .unwrap_or_default()
        .trim_end_matches('\0')
        .to_string();

    info!("Hardware model: {}", model);

    let supported = [
        "Raspberry Pi Zero 2 W",
        "Raspberry Pi Zero W",
        "Raspberry Pi Zero",
    ];

    let is_supported = supported.iter().any(|s| model.contains(s));

    if !is_supported {
        warn!("Unsupported hardware: {}. This tool is designed for Pi Zero W / Zero 2W.", model);
        // Don't fail - just warn
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chip_properties() {
        assert!(BcmChip::BCM43436B0.needs_patch());
        assert!(!BcmChip::BCM43430.needs_patch());
        assert!(!BcmChip::BCM43455.needs_patch());

        assert!(BcmChip::BCM43430.is_pi_zero_w());
        assert!(!BcmChip::BCM43436B0.is_pi_zero_w());

        assert!(BcmChip::BCM43436B0.is_pi_zero_2w());
        assert!(!BcmChip::BCM43430.is_pi_zero_2w());
    }

    #[test]
    fn test_as_str() {
        assert_eq!(BcmChip::BCM43436B0.as_str(), "BCM43436B0");
        assert_eq!(BcmChip::BCM43430.as_str(), "BCM43430");
        assert_eq!(BcmChip::BCM43455.as_str(), "BCM43455");
        assert_eq!(BcmChip::Unknown.as_str(), "UNKNOWN");
    }
}