//! Capture management (tmpfs -> validated .22000 + .pcapng)

use anyhow::Result;
use chrono::{DateTime, Utc};
use pwncore::{AccessPoint, Channel, Handshake, HandshakeType, MacAddr};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Capture manager for handshake files
pub struct CaptureManager {
    staging_dir: PathBuf,
    final_dir: PathBuf,
    seen_files: Arc<RwLock<HashMap<PathBuf, DateTime<Utc>>>>,
}

impl CaptureManager {
    pub fn new(staging: PathBuf, final_dir: PathBuf) -> Self {
        Self {
            staging_dir: staging,
            final_dir: final_dir,
            seen_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize capture directories
    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.staging_dir).await?;
        fs::create_dir_all(&self.final_dir).await?;
        info!("Capture manager initialized: staging={:?}, final={:?}", self.staging_dir, self.final_dir);
        Ok(())
    }

    /// Scan for new captures in staging directory
    pub async fn scan_new_captures(&self) -> Result<Vec<PathBuf>> {
        let mut new_files = Vec::new();
        let mut seen = self.seen_files.write().await;

        if !self.staging_dir.exists() {
            return Ok(new_files);
        }

        let mut entries = fs::read_dir(&self.staging_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "pcapng") {
                let meta = entry.metadata().await?;
                let modified = meta.modified().ok().and_then(|t| DateTime::<Utc>::from_timestamp(t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64, 0));

                if let Some(mod_time) = modified {
                    if !seen.contains_key(&path) || seen[&path] < mod_time {
                        new_files.push(path.clone());
                        seen.insert(path, mod_time);
                    }
                }
            }
        }

        Ok(new_files)
    }

    /// Validate a capture file (check for valid handshake)
    pub async fn validate_capture(&self, pcapng_path: &Path) -> Result<bool> {
        // In real implementation, would use hcxpcapngtool or similar
        // For now, just check file size
        let meta = fs::metadata(pcapng_path).await?;
        Ok(meta.len() > 100)
    }

    /// Move validated capture to final directory
    pub async fn move_to_final(&self, pcapng_path: &Path, ap: &AccessPoint) -> Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let ssid = ap.ssid.as_deref().unwrap_or("unknown");
        let safe_ssid: String = ssid.chars().filter(|c| c.is_alphanumeric()).collect();
        let bssid_str = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}", 
            ap.bssid.octets()[0], ap.bssid.octets()[1], ap.bssid.octets()[2],
            ap.bssid.octets()[3], ap.bssid.octets()[4], ap.bssid.octets()[5]);

        let base_name = format!("{}_{}_{}", safe_ssid, bssid_str, timestamp);
        
        let final_pcapng = self.final_dir.join(format!("{}.pcapng", base_name));
        let final_hashcat = self.final_dir.join(format!("{}.22000", base_name));

        // Move pcapng
        fs::rename(pcapng_path, &final_pcapng).await?;

        // Generate hashcat file (placeholder - would use hcxpcapngtool)
        // For now, create empty file
        fs::write(&final_hashcat, b"").await?;

        info!("Moved capture to final: {:?}", final_pcapng);
        Ok(final_pcapng)
    }

    /// Process all new captures
    pub async fn process_new(&self) -> Result<Vec<Handshake>> {
        let new_files = self.scan_new_captures().await?;
        let mut handshakes = Vec::new();

        for pcapng in new_files {
            if self.validate_capture(&pcapng).await? {
                // In real implementation, would parse AP info from pcapng
                // For now, create placeholder
                let handshake = Handshake::new(
                    [0; 6].into(),
                    Channel::new(1).unwrap(),
                );
                handshakes.push(handshake);

                // Move to final
                // Would need AP info here
            } else {
                warn!("Invalid capture: {:?}", pcapng);
                fs::remove_file(&pcapng).await?;
            }
        }

        Ok(handshakes)
    }

    /// Clean up old staging files
    pub async fn cleanup_old(&self, max_age_hours: u64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::hours(max_age_hours as i64);
        let mut removed = 0;

        let mut entries = fs::read_dir(&self.staging_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Ok(meta) = entry.metadata().await {
                if let Ok(modified) = meta.modified() {
                    let mod_time: DateTime<Utc> = modified.into();
                    if mod_time < cutoff {
                        fs::remove_file(&path).await?;
                        removed += 1;
                    }
                }
            }
        }

        Ok(removed)
    }
}

impl Default for CaptureManager {
    fn default() -> Self {
        Self::new(
            PathBuf::from("/var/tmp/pwnghost/staging"),
            PathBuf::from("/etc/pwnghost/handshakes"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_capture_manager_init() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        let final_dir = tmp.path().join("final");

        let mgr = CaptureManager::new(staging, final_dir);
        mgr.init().await.unwrap();

        assert!(mgr.staging_dir.exists());
        assert!(mgr.final_dir.exists());
    }
}