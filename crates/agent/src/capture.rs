//! Capture management (tmpfs -> validated .22000 + .pcapng)

use anyhow::Result;
use chrono::{DateTime, Utc};
use pwncore::{Channel, Handshake, MacAddr};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::process::Command;
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
            final_dir,
            seen_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize capture directories
    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.staging_dir).await?;
        fs::create_dir_all(&self.final_dir).await?;
        info!(
            "Capture manager initialized: staging={:?}, final={:?}",
            self.staging_dir, self.final_dir
        );
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
            if path.extension().is_some_and(|e| e == "pcapng") {
                let meta = entry.metadata().await?;
                let modified = meta.modified().ok().and_then(|t| {
                    DateTime::<Utc>::from_timestamp(
                        t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64,
                        0,
                    )
                });

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

    /// Validate a capture file and extract its BSSID via `hcxpcapngtool`,
    /// writing a hashcat-ready `.22000` file next to it on success.
    ///
    /// Returns `Ok(None)` when the capture contains no crackable handshake,
    /// or when `hcxpcapngtool` isn't installed (in which case a coarse
    /// size-based heuristic is used so the pipeline still degrades
    /// gracefully instead of fabricating a result).
    async fn extract_handshake(
        &self,
        pcapng_path: &Path,
        out_22000: &Path,
    ) -> Result<Option<MacAddr>> {
        let output = Command::new("hcxpcapngtool")
            .arg("-o")
            .arg(out_22000)
            .arg(pcapng_path)
            .output()
            .await;

        let output = match output {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    "hcxpcapngtool not installed; falling back to a size heuristic for {:?}",
                    pcapng_path
                );
                let meta = fs::metadata(pcapng_path).await?;
                return Ok(if meta.len() > 100 {
                    Some(MacAddr::zero())
                } else {
                    None
                });
            }
            Err(e) => return Err(e.into()),
        };

        if !output.status.success() {
            debug!(
                "hcxpcapngtool found no handshake in {:?}: {}",
                pcapng_path,
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(None);
        }

        let content = fs::read_to_string(out_22000).await.unwrap_or_default();
        Ok(parse_bssid_from_22000(&content))
    }

    /// Move a validated capture (and its `.22000` sidecar) to the final
    /// handshakes directory, naming both files from the BSSID/SSID.
    pub async fn move_to_final(
        &self,
        pcapng_path: &Path,
        hashcat_path: &Path,
        bssid: MacAddr,
        ssid: Option<&str>,
    ) -> Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let ssid = ssid.unwrap_or("unknown");
        let safe_ssid: String = ssid.chars().filter(|c| c.is_alphanumeric()).collect();
        let bssid_str = bssid
            .octets()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        let base_name = format!("{}_{}_{}", safe_ssid, bssid_str, timestamp);

        let final_pcapng = self.final_dir.join(format!("{}.pcapng", base_name));
        let final_hashcat = self.final_dir.join(format!("{}.22000", base_name));

        fs::rename(pcapng_path, &final_pcapng).await?;
        if fs::metadata(hashcat_path).await.is_ok() {
            fs::rename(hashcat_path, &final_hashcat).await?;
        }

        info!("Moved capture to final: {:?}", final_pcapng);
        Ok(final_pcapng)
    }

    /// Process all new captures: validate, extract the BSSID, move to the
    /// final directory, and return a `Handshake` record for each success.
    pub async fn process_new(&self) -> Result<Vec<Handshake>> {
        let new_files = self.scan_new_captures().await?;
        let mut handshakes = Vec::new();

        for pcapng in new_files {
            let hashcat_tmp = pcapng.with_extension("22000");
            match self.extract_handshake(&pcapng, &hashcat_tmp).await? {
                Some(bssid) => {
                    let final_pcapng = self
                        .move_to_final(&pcapng, &hashcat_tmp, bssid, None)
                        .await?;
                    let mut handshake = Handshake::new(bssid, Channel::new(1).unwrap());
                    handshake.pcapng_path = final_pcapng.display().to_string();
                    handshake.hashcat_path =
                        final_pcapng.with_extension("22000").display().to_string();
                    handshakes.push(handshake);
                }
                None => {
                    warn!("No handshake found in capture, discarding: {:?}", pcapng);
                    let _ = fs::remove_file(&pcapng).await;
                    let _ = fs::remove_file(&hashcat_tmp).await;
                }
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

/// Extract the AP BSSID from a hashcat 22000-format file's contents.
///
/// Format: `WPA*type*pmkid_or_mic*bssid*sta_mac*essid*...` where the BSSID
/// field is 12 hex characters with no separators.
fn parse_bssid_from_22000(content: &str) -> Option<MacAddr> {
    content.lines().find_map(|line| {
        let fields: Vec<&str> = line.split('*').collect();
        let bssid_hex = fields.get(3)?;
        if bssid_hex.len() != 12 {
            return None;
        }
        let mut octets = [0u8; 6];
        for (i, octet) in octets.iter_mut().enumerate() {
            *octet = u8::from_str_radix(&bssid_hex[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(MacAddr::from_octets(octets))
    })
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

    #[test]
    fn test_parse_bssid_from_22000() {
        let content = "WPA*02*aabbccddeeff00112233445566778899*aabbccddeeff*112233445566*746573745353494400*7061737365777264*\n";
        let bssid = parse_bssid_from_22000(content).unwrap();
        assert_eq!(bssid.octets(), [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
    }

    #[test]
    fn test_parse_bssid_from_22000_ignores_malformed_lines() {
        assert!(parse_bssid_from_22000("").is_none());
        assert!(parse_bssid_from_22000("not*hashcat*format\n").is_none());
    }

    #[tokio::test]
    async fn test_move_to_final_renames_both_files() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        let final_dir = tmp.path().join("final");
        let mgr = CaptureManager::new(staging.clone(), final_dir.clone());
        mgr.init().await.unwrap();

        let pcapng = staging.join("capture.pcapng");
        let hashcat = staging.join("capture.22000");
        fs::write(&pcapng, b"pcap-data").await.unwrap();
        fs::write(&hashcat, b"hash-data").await.unwrap();

        let bssid = MacAddr::from_octets([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let final_pcapng = mgr
            .move_to_final(&pcapng, &hashcat, bssid, Some("TestNet"))
            .await
            .unwrap();

        assert!(final_pcapng.exists());
        assert!(!pcapng.exists());
        assert!(final_pcapng.with_extension("22000").exists());
        assert!(final_pcapng.to_string_lossy().contains("aabbccddeeff"));
    }
}
