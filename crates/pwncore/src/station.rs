//! Station/client types

use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use serde::{Deserialize, Serialize};

/// Client station connected to an AP
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Station {
    pub mac: MacAddr,
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub associated_ap: Option<MacAddr>,
    pub probes: Vec<String>,
    pub data_rate: Option<String>,
    pub power_save: bool,
    pub wmm: bool,
    pub capabilities: u16,
}

impl Station {
    pub fn new(mac: MacAddr, vendor: String, rssi: i16, channel: u8) -> Self {
        let now = Utc::now();
        Self {
            mac,
            vendor,
            rssi,
            channel,
            first_seen: now,
            last_seen: now,
            associated_ap: None,
            probes: Vec::new(),
            data_rate: None,
            power_save: false,
            wmm: false,
            capabilities: 0,
        }
    }

    pub fn update_seen(&mut self) {
        self.last_seen = Utc::now();
    }

    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        Utc::now().signed_duration_since(self.last_seen) > max_age
    }

    pub fn add_probe(&mut self, ssid: String) {
        if !self.probes.contains(&ssid) {
            self.probes.push(ssid);
        }
    }
}

/// Lightweight client info (for AP clients list)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientInfo {
    pub mac: MacAddr,
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
}

/// Alias for backward compatibility
pub type Client = ClientInfo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_creation() {
        let mac = MacAddr::from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let station = Station::new(mac, "TestVendor".to_string(), -50, 6);
        
        assert_eq!(station.mac, mac);
        assert_eq!(station.vendor, "TestVendor");
        assert_eq!(station.rssi, -50);
        assert_eq!(station.channel, 6);
    }
}