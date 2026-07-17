//! Handshake types for WiFi handshake captures

use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// GPS data for wardriving
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpsData {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub accuracy: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

/// Handshake type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HandshakeType {
    Pmkid,
    Wpa,
    Wpa2,
    Wpa3,
}

/// Handshake capture
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Handshake {
    pub id: Uuid,
    pub bssid: MacAddr,
    pub station: MacAddr,
    pub file_path: String,
    pub handshake_type: HandshakeType,
    pub captured_at: DateTime<Utc>,
    pub ssid: Option<String>,
    pub gps: Option<GpsData>,
    pub validated: bool,
    pub signal_strength: Option<i16>,
    pub noise_floor: Option<i16>,
}

impl Handshake {
    pub fn new(
        bssid: MacAddr,
        station: MacAddr,
        file_path: String,
        handshake_type: HandshakeType,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bssid,
            station,
            file_path,
            handshake_type,
            captured_at: Utc::now(),
            ssid: None,
            gps: None,
            validated: false,
            signal_strength: None,
            noise_floor: None,
        }
    }

    pub fn display_key(&self) -> String {
        format!("{} -> {}", self.station, self.bssid)
    }

    pub fn hashcat_hashline(&self) -> String {
        format!("WPA*{bssid}*{station}*", bssid = self.bssid, station = self.station)
    }
}

/// Handshake file info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeFile {
    pub path: String,
    pub handshakes: Vec<Handshake>,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_creation() {
        let bssid = MacAddr::from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let station = MacAddr::from([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        
        let hs = Handshake::new(bssid, station, "/tmp/test.pcapng".to_string(), HandshakeType::Wpa2);
        
        assert_eq!(hs.bssid, bssid);
        assert_eq!(hs.station, station);
        assert_eq!(hs.handshake_type, HandshakeType::Wpa2);
    }
}