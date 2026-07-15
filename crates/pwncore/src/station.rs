//! Station/Client types

use crate::Channel;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::MacAddr;

/// Client station (associated with an AP)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Station {
    pub mac: MacAddr,
    pub ap_bssid: MacAddr,
    pub rssi: i16,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
    pub vendor: String,
    pub channel: Channel,
}

impl Station {
    pub fn new(mac: MacAddr, vendor: String, rssi: i16, channel: u8) -> Self {
        Self {
            mac,
            ap_bssid: MacAddr::from([0; 6]),
            rssi,
            last_seen: Utc::now(),
            handshake_captured: false,
            vendor,
            channel: Channel::new(channel).unwrap_or(Channel(1)),
        }
    }

    pub fn with_ap_bssid(mut self, bssid: MacAddr) -> Self {
        self.ap_bssid = bssid;
        self
    }

    pub fn update_rssi(&mut self, rssi: i16) {
        self.rssi = rssi;
        self.last_seen = Utc::now();
    }

    pub fn mark_handshake(&mut self) {
        self.handshake_captured = true;
    }
}

/// Alias for compatibility
pub type Client = Station;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_station_new() {
        let mac = "11:22:33:44:55:66".parse().unwrap();
        let station = Station::new(mac, "TestVendor".to_string(), -60, 6);

        assert_eq!(station.mac, mac);
        assert_eq!(station.rssi, -60);
        assert_eq!(station.channel.value(), 6);
        assert_eq!(station.vendor, "TestVendor");
        assert!(!station.handshake_captured);
    }

    #[test]
    fn test_station_update_rssi() {
        let mac = "11:22:33:44:55:66".parse().unwrap();
        let mut station = Station::new(mac, "TestVendor".to_string(), -60, 6);

        station.update_rssi(-50);
        assert_eq!(station.rssi, -50);
    }

    #[test]
    fn test_station_handshake() {
        let mac = "11:22:33:44:55:66".parse().unwrap();
        let mut station = Station::new(mac, "TestVendor".to_string(), -60, 6);

        assert!(!station.handshake_captured);
        station.mark_handshake();
        assert!(station.handshake_captured);
    }
}