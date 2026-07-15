//! Access Point types

use crate::{Channel, EncryptionType, Station};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::MacAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessPoint {
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: Channel,
    pub rssi: i16,
    pub encryption: EncryptionType,
    pub vendor: String,
    pub clients: Vec<Station>,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
    pub pmkid_captured: bool,
    pub first_seen: DateTime<Utc>,
}

impl AccessPoint {
    pub fn new(bssid: MacAddr, channel: Channel, rssi: i16, encryption: EncryptionType, vendor: String) -> Self {
        let now = Utc::now();
        Self {
            bssid,
            ssid: None,
            channel,
            rssi,
            encryption,
            vendor,
            clients: Vec::new(),
            last_seen: now,
            handshake_captured: false,
            pmkid_captured: false,
            first_seen: now,
        }
    }

    pub fn with_ssid(mut self, ssid: String) -> Self {
        self.ssid = Some(ssid);
        self
    }

    pub fn add_client(&mut self, client: Station) {
        if !self.clients.iter().any(|c| c.mac == client.mac) {
            self.clients.push(client);
        }
    }

    pub fn update_rssi(&mut self, rssi: i16) {
        self.rssi = rssi;
        self.last_seen = Utc::now();
    }

    pub fn mark_handshake(&mut self) {
        self.handshake_captured = true;
    }

    pub fn mark_pmkid(&mut self) {
        self.pmkid_captured = true;
    }

    pub fn is_target(&self, whitelist: &[MacAddr], blacklist: &[MacAddr]) -> bool {
        if whitelist.is_empty() && blacklist.is_empty() {
            return true;
        }
        if whitelist.contains(&self.bssid) {
            return true;
        }
        !blacklist.contains(&self.bssid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_ap_new() {
        let bssid = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let channel = Channel::new(6).unwrap();
        let ap = AccessPoint::new(bssid, channel, -50, EncryptionType::Wpa2, "TestVendor".into());

        assert_eq!(ap.bssid, bssid);
        assert_eq!(ap.channel, channel);
        assert_eq!(ap.rssi, -50);
        assert_eq!(ap.encryption, EncryptionType::Wpa2);
        assert_eq!(ap.vendor, "TestVendor");
        assert!(!ap.handshake_captured);
    }

    #[test]
    fn test_ap_add_client() {
        let bssid = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let channel = Channel::new(6).unwrap();
        let mut ap = AccessPoint::new(bssid, channel, -50, EncryptionType::Wpa2, "TestVendor".into());

        let client_mac = MacAddr::from_str("11:22:33:44:55:66").unwrap();
        let client = Station::new(client_mac, "ClientVendor".into(), -60, 6);
        ap.add_client(client.clone());

        assert_eq!(ap.clients.len(), 1);
        assert_eq!(ap.clients[0].mac, client_mac);

        // Adding same client again should not duplicate
        ap.add_client(client);
        assert_eq!(ap.clients.len(), 1);
    }

    #[test]
    fn test_ap_target_logic() {
        let bssid = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let channel = Channel::new(6).unwrap();
        let ap = AccessPoint::new(bssid, channel, -50, EncryptionType::Wpa2, "TestVendor".into());

        assert!(ap.is_target(&[], &[]));
        assert!(ap.is_target(&[bssid], &[]));
        assert!(!ap.is_target(&[], &[bssid]));
        assert!(!ap.is_target(&[bssid], &[bssid]));
    }
}