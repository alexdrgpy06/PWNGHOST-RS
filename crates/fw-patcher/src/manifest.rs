//! Firmware manifest parsing and hash validation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Firmware manifest from CoderFX
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub chip: String,
    pub firmware: FirmwareInfo,
    pub patches: PatchInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareInfo {
    pub filename: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchInfo {
    pub filename: String,
    pub sha256: String,
    pub size: u64,
    pub layers: u8,
}

impl Manifest {
    /// Load manifest from JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.as_ref().display()))?;
        let manifest: Manifest = serde_json::from_str(&content)
            .with_context(|| "Failed to parse manifest JSON")?;
        Ok(manifest)
    }

    /// Verify firmware file matches manifest
    pub fn verify_firmware<P: AsRef<Path>>(&self, firmware_path: P) -> Result<bool> {
        let path = firmware_path.as_ref();
        let actual_hash = file_sha256(path)?;
        let expected = self.firmware.sha256.to_lowercase();

        if actual_hash != expected {
            tracing::warn!(
                "Firmware hash mismatch: expected {}, got {}",
                expected,
                actual_hash
            );
            return Ok(false);
        }

        // Verify size
        let meta = fs::metadata(path)?;
        if meta.len() != self.firmware.size {
            tracing::warn!(
                "Firmware size mismatch: expected {}, got {}",
                self.firmware.size,
                meta.len()
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Verify patch file matches manifest
    pub fn verify_patches<P: AsRef<Path>>(&self, patches_path: P) -> Result<bool> {
        let path = patches_path.as_ref();
        let actual_hash = file_sha256(path)?;
        let expected = self.patches.sha256.to_lowercase();

        if actual_hash != expected {
            tracing::warn!(
                "Patches hash mismatch: expected {}, got {}",
                expected,
                actual_hash
            );
            return Ok(false);
        }

        let meta = fs::metadata(path)?;
        if meta.len() != self.patches.size {
            tracing::warn!(
                "Patches size mismatch: expected {}, got {}",
                self.patches.size,
                meta.len()
            );
            return Ok(false);
        }

        Ok(true)
    }
}

/// Compute SHA256 of a file
pub fn file_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)
        .with_context(|| format!("Failed to open file for hashing: {}", path.display()))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let n = std::io::Read::read(&mut file, &mut buffer)
            .with_context(|| format!("Read failed during hashing: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Compute SHA256 of bytes
pub fn bytes_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_bytes_sha256() {
        let data = b"hello world";
        let hash = bytes_sha256(data);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_file_sha256() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"test content").unwrap();
        f.flush().unwrap();

        let hash = file_sha256(f.path()).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_manifest_load() {
        let json = r#"{
            "version": "1.0",
            "chip": "43436B0",
            "firmware": {
                "filename": "brcmfmac43436-sdio.bin",
                "sha256": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
                "size": 12345
            },
            "patches": {
                "filename": "inplace-v7.txt",
                "sha256": "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321",
                "size": 54321,
                "layers": 8
            }
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.chip, "43436B0");
        assert_eq!(manifest.firmware.filename, "brcmfmac43436-sdio.bin");
        assert_eq!(manifest.patches.layers, 8);
    }
}