//! Persistent unit identity (Workstream D1).
//!
//! Real pwnagotchi generates an RSA keypair on first boot
//! (`pwnagotchi/identity.py`) and uses it for grid identity and stable
//! naming. This project has no such identity at all today -- `grid.lua`'s
//! own doc comment notes "no way to hold a persistent signed identity from
//! Lua". This module gives the Rust agent a real persistent keypair and a
//! stable fingerprint derived from it, so a future grid/mesh integration
//! (Workstream E2) has real identity to build on, and so the agent has a
//! stable, non-MAC-address identifier today.
//!
//! ed25519 (via the pure-Rust `ed25519-dalek`, no C/asm dependency --
//! matches this project's `fontdue`-over-FreeType precedent for the same
//! ARMv6 cross-compile reason) rather than RSA-2048: smaller keys, faster
//! generation, and real pwnagotchi's own protocol dependency on RSA is tied
//! to the `pwngrid-peer` daemon this project doesn't have and hasn't
//! committed to reimplementing (Workstream E2 is still an open decision).
//! If real opwngrid interop is built later and requires RSA specifically,
//! this module is small and self-contained enough to swap.

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// A unit's persistent identity: an ed25519 keypair plus a stable
/// fingerprint (`sha256(pubkey)`, hex-encoded) derived from it.
pub struct Identity {
    signing_key: SigningKey,
    fingerprint: String,
}

impl Identity {
    /// Load the identity from `path` if it exists, otherwise generate a new
    /// one and persist it there. The fingerprint is stable across restarts
    /// as long as the file survives -- that's the entire point.
    pub async fn load_or_generate(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        match fs::read_to_string(path).await {
            Ok(content) => Self::from_hex_seed(content.trim())
                .with_context(|| format!("parsing identity file {path:?}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let identity = Self::generate();
                identity.persist(path).await?;
                Ok(identity)
            }
            Err(e) => Err(e).with_context(|| format!("reading identity file {path:?}")),
        }
    }

    /// Generate a fresh identity from the OS CSPRNG. Does not persist --
    /// callers that want it saved should call [`Self::persist`].
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed);
        Self::from_seed(seed)
    }

    fn from_seed(seed: [u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(&seed);
        let fingerprint = fingerprint_of(&signing_key);
        Self {
            signing_key,
            fingerprint,
        }
    }

    fn from_hex_seed(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str).context("identity file is not valid hex")?;
        let seed: [u8; 32] = bytes
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("expected 32-byte seed, got {} bytes", v.len()))?;
        Ok(Self::from_seed(seed))
    }

    /// Atomically persist the identity's secret seed (hex-encoded) to
    /// `path`, following this project's established crash-safety
    /// convention (temp file + fsync + rename + directory fsync -- mirrors
    /// `config::save_config`, written this session for exactly the same
    /// "never leave a torn write behind" reason).
    async fn persist(&self, path: &Path) -> Result<()> {
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        let dir = dir.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("creating identity directory {dir:?}"))?;

        let file_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("identity.key");
        let tmp_path = dir.join(format!(".{file_name}.tmp.{}", std::process::id()));

        let seed_hex = hex::encode(self.signing_key.to_bytes());
        let mut tmp_file = fs::File::create(&tmp_path)
            .await
            .with_context(|| format!("creating temp identity file {tmp_path:?}"))?;
        if let Err(e) = tmp_file.write_all(seed_hex.as_bytes()).await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(e).context("writing temp identity file");
        }
        if let Err(e) = tmp_file.sync_all().await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(e).context("fsyncing temp identity file");
        }
        drop(tmp_file);

        fs::rename(&tmp_path, path)
            .await
            .with_context(|| format!("renaming {tmp_path:?} to {path:?}"))?;

        if let Ok(dir_file) = std::fs::File::open(&dir) {
            let _ = dir_file.sync_all();
        }

        // Best-effort: restrict to owner read/write only (secret key
        // material). Not fatal on platforms/filesystems that don't support
        // Unix permission bits (e.g. this project's Windows dev loop).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(path, perms);
            }
        }

        Ok(())
    }

    /// The stable `sha256(pubkey)` fingerprint, hex-encoded.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// The raw 32-byte public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
}

fn fingerprint_of(signing_key: &SigningKey) -> String {
    let pubkey = signing_key.verifying_key().to_bytes();
    let mut hasher = Sha256::new();
    hasher.update(pubkey);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_produces_64_char_hex_fingerprint() {
        let id = Identity::generate();
        assert_eq!(id.fingerprint().len(), 64);
        assert!(id.fingerprint().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_same_seed_yields_same_fingerprint() {
        let a = Identity::from_seed([7u8; 32]);
        let b = Identity::from_seed([7u8; 32]);
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.public_key_bytes(), b.public_key_bytes());
    }

    #[test]
    fn test_different_seeds_yield_different_fingerprints() {
        let a = Identity::from_seed([1u8; 32]);
        let b = Identity::from_seed([2u8; 32]);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[tokio::test]
    async fn test_load_or_generate_creates_file_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("identity.key");
        assert!(!path.exists());

        let id = Identity::load_or_generate(&path).await.unwrap();
        assert!(path.exists(), "identity file should be created");
        assert_eq!(id.fingerprint().len(), 64);
    }

    #[tokio::test]
    async fn test_load_or_generate_is_stable_across_reloads() {
        // The whole point of D1: the fingerprint must survive a restart.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.key");

        let first = Identity::load_or_generate(&path).await.unwrap();
        let second = Identity::load_or_generate(&path).await.unwrap();

        assert_eq!(first.fingerprint(), second.fingerprint());
        assert_eq!(first.public_key_bytes(), second.public_key_bytes());
    }

    #[tokio::test]
    async fn test_load_or_generate_rejects_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity.key");
        tokio::fs::write(&path, "not valid hex at all!!")
            .await
            .unwrap();

        let result = Identity::load_or_generate(&path).await;
        assert!(result.is_err(), "corrupt identity file must not be silently regenerated");
    }
}
