use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tracing::{debug, error, info};

/// Schema version of the manifest - must be 1
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Trusted manifest hashes embedded at compile time
/// These are the SHA256 hashes of known-good manifest.json files
/// The installer validates the manifest against these before trusting any values
const TRUSTED_MANIFEST_HASHES: &[&str] = &[
    // CoderFX pwnagotchi-pi-zero-2w-bcm43436b0-firmware-fix v0.1.0
    "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456",
    // Add more as new firmware versions are supported
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub repo_version: String,
    pub patches: Vec<PatchEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchEntry {
    pub name: String,
    pub input_sha256: String,
    pub input_size: u64,
    pub patch_file: String,
    pub patch_sha256: String,
    pub output_sha256: String,
    pub output_size: u64,
    pub userspace: UserspaceBinaries,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserspaceBinaries {
    pub aarch64_sha256: String,
    pub armhf_sha256: String,
}

impl Manifest {
    /// Load and validate manifest from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;

        let manifest: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse manifest JSON: {}", path.display()))?;

        manifest.validate()?;
        manifest.verify_trusted()?;

        info!("Manifest loaded and validated: {}", path.display());
        Ok(manifest)
    }

    /// Validate manifest structure and internal consistency
    fn validate(&self) -> Result<()> {
        // Check schema version
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            bail!(
                "Unsupported manifest schema version: {} (expected {})",
                self.schema_version,
                MANIFEST_SCHEMA_VERSION
            );
        }

        // Must have at least one patch
        if self.patches.is_empty() {
            bail!("Manifest contains no patches");
        }

        // Validate each patch entry
        for (i, patch) in self.patches.iter().enumerate() {
            patch.validate().with_context(|| format!("Patch entry {} invalid", i))?;
        }

        Ok(())
    }

    /// Verify manifest hash against embedded trusted hashes
    fn verify_trusted(&self) -> Result<()> {
        // Compute hash of the manifest file as it would be on disk
        // (We need to re-serialize to get canonical form)
        let canonical = serde_json::to_vec(self)
            .context("Failed to serialize manifest for hashing")?;
        let hash = Sha256::digest(&canonical);
        let hash_hex = hex::encode(hash);

        if !TRUSTED_MANIFEST_HASHES.iter().any(|&h| h == hash_hex) {
            error!(
                "Manifest hash {} not in trusted list! Manifest may be tampered.",
                hash_hex
            );
            bail!("Manifest hash verification failed: {}", hash_hex);
        }

        info!("Manifest hash verified against trusted list: {}", hash_hex);
        Ok(())
    }

    /// Get patch entry by name
    pub fn get_patch(&self, name: &str) -> Option<&PatchEntry> {
        self.patches.iter().find(|p| p.name == name)
    }

    /// Get the appropriate patch for the detected firmware
    pub fn find_matching_patch(&self, firmware_sha256: &str) -> Option<&PatchEntry> {
        self.patches
            .iter()
            .find(|p| p.input_sha256 == firmware_sha256)
    }
}

impl PatchEntry {
    /// Validate patch entry fields
    fn validate(&self) -> Result<()> {
        // Validate SHA256 format (64 hex chars)
        validate_sha256(&self.input_sha256, "input_sha256")?;
        validate_sha256(&self.patch_sha256, "patch_sha256")?;
        validate_sha256(&self.output_sha256, "output_sha256")?;
        validate_sha256(&self.userspace.aarch64_sha256, "userspace.aarch64_sha256")?;
        validate_sha256(&self.userspace.armhf_sha256, "userspace.armhf_sha256")?;

        // Validate sizes match expectations
        if self.input_size == 0 {
            bail!("input_size must be > 0");
        }
        if self.output_size == 0 {
            bail!("output_size must be > 0");
        }
        if self.input_size != self.output_size {
            bail!("input_size ({}) != output_size ({}) - patch must not change file size", 
                  self.input_size, self.output_size);
        }

        // Patch file must be specified
        if self.patch_file.is_empty() {
            bail!("patch_file must not be empty");
        }

        Ok(())
    }

    /// Verify patch file hash matches manifest
    pub fn verify_patch_file<P: AsRef<Path>>(&self, patch_path: P) -> Result<()> {
        let path = patch_path.as_ref();
        let content = fs::read(path)
            .with_context(|| format!("Failed to read patch file: {}", path.display()))?;
        
        let hash = Sha256::digest(&content);
        let hash_hex = hex::encode(hash);

        if hash_hex != self.patch_sha256 {
            bail!(
                "Patch file hash mismatch: expected {}, got {}",
                self.patch_sha256,
                hash_hex
            );
        }

        debug!("Patch file hash verified: {}", path.display());
        Ok(())
    }

    /// Verify userspace binary hash
    pub fn verify_userspace_binary<P: AsRef<Path>>(
        &self,
        binary_path: P,
        arch: &str,
    ) -> Result<()> {
        let path = binary_path.as_ref();
        let content = fs::read(path)
            .with_context(|| format!("Failed to read userspace binary: {}", path.display()))?;
        
        let hash = Sha256::digest(&content);
        let hash_hex = hex::encode(hash);

        let expected = match arch {
            "aarch64" => &self.userspace.aarch64_sha256,
            "armhf" | "armv7" => &self.userspace.armhf_sha256,
            _ => bail!("Unknown architecture: {}", arch),
        };

        if hash_hex != *expected {
            bail!(
                "Userspace binary hash mismatch ({}): expected {}, got {}",
                arch,
                expected,
                hash_hex
            );
        }

        debug!("Userspace binary hash verified ({}): {}", arch, path.display());
        Ok(())
    }
}

fn validate_sha256(hash: &str, field: &str) -> Result<()> {
    if hash.len() != 64 {
        bail!("{} must be 64 hex chars, got {}", field, hash.len());
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("{} contains non-hex characters", field);
    }
    Ok(())
}

/// Compute SHA256 of a file
pub fn file_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let content = fs::read(path)
        .with_context(|| format!("Failed to read file for hashing: {}", path.display()))?;
    let hash = Sha256::digest(&content);
    Ok(hex::encode(hash))
}

/// Compute SHA256 of bytes
pub fn bytes_sha256(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    fn create_test_manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            repo_version: "0.1.0".to_string(),
            patches: vec![PatchEntry {
                name: "v7".to_string(),
                input_sha256: "a".repeat(64),
                input_size: 414696,
                patch_file: "patches/inplace-v7.txt".to_string(),
                patch_sha256: "b".repeat(64),
                output_sha256: "c".repeat(64),
                output_size: 414696,
                userspace: UserspaceBinaries {
                    aarch64_sha256: "d".repeat(64),
                    armhf_sha256: "e".repeat(64),
                },
                description: "BCM43436B0 v7 stability patch".to_string(),
            }],
        }
    }

    #[test]
    fn test_manifest_validation_passes() {
        let manifest = create_test_manifest();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation_fails_on_wrong_schema() {
        let mut manifest = create_test_manifest();
        manifest.schema_version = 99;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_fails_on_empty_patches() {
        let mut manifest = create_test_manifest();
        manifest.patches.clear();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_sha256() {
        assert!(validate_sha256(&"a".repeat(64), "test").is_ok());
        assert!(validate_sha256(&"a".repeat(63), "test").is_err());
        assert!(validate_sha256(&"g".repeat(64), "test").is_err());
    }

    #[test]
    fn test_bytes_sha256() {
        let data = b"test data";
        let hash = bytes_sha256(data);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_file_sha256() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"test content").unwrap();
        f.flush().unwrap();
        
        let hash = file_sha256(f.path()).unwrap();
        assert_eq!(hash.len(), 64);
    }
}