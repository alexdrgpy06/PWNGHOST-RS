//! Handshake types and file handling

use crate::{Channel, EncryptionType};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::MacAddr;
use uuid::Uuid;

/// Captured handshake (validated .22000 + .pcapng pair)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Handshake {
    pub id: Uuid,
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: Channel,
    pub handshake_type: HandshakeType,
    pub pcapng_path: String,
    pub hashcat_path: String,
    pub captured_at: DateTime<Utc>,
    pub validated: bool,
    pub gps: Option<GpsData>,
}

/// Type of handshake captured
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HandshakeType {
    WPA2,  // 4-way handshake
    WPA3,  // SAE handshake
    PMKID, // PMKID attack
}

impl HandshakeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            HandshakeType::WPA2 => "wpa2",
            HandshakeType::WPA3 => "wpa3",
            HandshakeType::PMKID => "pmkid",
        }
    }
}

/// Handshake file pair info (for capture pipeline)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeFile {
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: Channel,
    pub handshake_type: HandshakeType,
    pub pcapng_path: String,
    pub hashcat_path: String,
    pub file_size: u64,
    pub captured_at: DateTime<Utc>,
    pub validated: bool,
}

/// GPS coordinates for wardriving
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpsData {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub accuracy: f64,
    pub timestamp: DateTime<Utc>,
}

/// Validate a handshake file pair
pub fn validate_handshake_pair(pcapng_path: &str, hashcat_path: &str) -> Result<bool> {
    use std::path::Path;

    let pcapng = Path::new(pcapng_path);
    let hashcat = Path::new(hashcat_path);

    if !pcapng.exists() || !hashcat.exists() {
        return Ok(false);
    }

    // Check file sizes > 0
    let pcapng_meta = std::fs::metadata(pcapng)?;
    let hashcat_meta = std::fs::metadata(hashcat)?;

    if pcapng_meta.len() == 0 || hashcat_meta.len() == 0 {
        return Ok(false);
    }

    // TODO: Add hcxpcapngtool validation for .22000 format
    // For now, basic size check
    Ok(true)
}

/// Generate hashcat (.22000) filename from pcapng path
pub fn hashcat_filename(pcapng_path: &str) -> String {
    use std::path::Path;
    let path = Path::new(pcapng_path);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    format!("{}.22000", stem)
}

/// Generate pcapng filename from components
pub fn pcapng_filename(bssid: &MacAddr, ssid: Option<&str>, timestamp: DateTime<Utc>) -> String {
    let ssid_part = ssid
        .map(|s| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .unwrap_or_else(|| "unknown".to_string());
    let bssid_str = bssid.to_string().replace(':', "");
    format!("{}_{}_{}.pcapng", ssid_part, bssid_str, timestamp.format("%Y%m%d-%H%M%S"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::MacAddr;

    #[test]
    fn test_handshake_type_as_str() {
        assert_eq!(HandshakeType::WPA2.as_str(), "wpa2");
        assert_eq!(HandshakeType::WPA3.as_str(), "wpa3");
        assert_eq!(HandshakeType::PMKID.as_str(), "pmkid");
    }

    #[test]
    fn test_hashcat_filename() {
        let path = "/tmp/handshake.pcapng";
        assert_eq!(hashcat_filename(path), "handshake.22000");
    }

    #[test]
    fn test_pcapng_filename() {
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ts = DateTime::from_timestamp(1700000000, 0).unwrap();
        let name = pcapng_filename(&bssid, Some("TestAP"), ts);
        assert!(name.contains("aabbccddeeff"));
        assert!(name.contains("TestAP"));
        assert!(name.ends_with(".pcapng"));
    }
}