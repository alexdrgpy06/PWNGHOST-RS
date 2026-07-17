use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use serde::{Deserialize, Serialize};

/// Access Point (WiFi network)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessPoint {
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: u8,
    pub rssi: i16,
    pub encryption: EncryptionType,
    pub vendor: String,
    pub clients: Vec<Client>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
}

impl AccessPoint {
    pub fn new(bssid: MacAddr, channel: u8, rssi: i16, encryption: EncryptionType, vendor: String) -> Self {
        let now = Utc::now();
        Self {
            bssid,
            ssid: None,
            channel,
            rssi,
            encryption,
            vendor,
            clients: Vec::new(),
            first_seen: now,
            last_seen: now,
            handshake_captured: false,
        }
    }

    pub fn with_ssid(mut self, ssid: String) -> Self {
        self.ssid = Some(ssid);
        self
    }

    pub fn add_client(&mut self, client: Client) {
        // Check if client already exists
        if !self.clients.iter().any(|c| c.mac == client.mac) {
            self.clients.push(client);
        }
    }

    pub fn update_seen(&mut self) {
        self.last_seen = Utc::now();
    }

    pub fn is_on_channel(&self, channel: u8) -> bool {
        self.channel == channel
    }

    pub fn is_encrypted(&self) -> bool {
        !matches!(self.encryption, EncryptionType::Open)
    }

    pub fn display_name(&self) -> String {
        self.ssid.as_deref().unwrap_or(&self.bssid.to_string()).to_string()
    }
}

/// Client station (device connected to AP)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Client {
    pub mac: MacAddr,
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub associated_ap: Option<MacAddr>,
}

impl Client {
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
        }
    }

    pub fn update_seen(&mut self) {
        self.last_seen = Utc::now();
    }

    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        Utc::now().signed_duration_since(self.last_seen) > max_age
    }
}

/// Encryption type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum EncryptionType {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Unknown,
}

impl EncryptionType {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "OPEN" | "" => EncryptionType::Open,
            "WEP" => EncryptionType::Wep,
            "WPA" => EncryptionType::Wpa,
            "WPA2" | "WPA2-PSK" | "WPA2-CCMP" => EncryptionType::Wpa2,
            "WPA3" | "WPA3-SAE" => EncryptionType::Wpa3,
            _ => EncryptionType::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EncryptionType::Open => "OPEN",
            EncryptionType::Wep => "WEP",
            EncryptionType::Wpa => "WPA",
            EncryptionType::Wpa2 => "WPA2",
            EncryptionType::Wpa3 => "WPA3",
            EncryptionType::Unknown => "UNKNOWN",
        }
    }

    pub fn is_crackable(&self) -> bool {
        matches!(self, EncryptionType::Wep | EncryptionType::Wpa | EncryptionType::Wpa2 | EncryptionType::Wpa3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_from_str() {
        assert_eq!(EncryptionType::from_str("WPA2"), EncryptionType::Wpa2);
        assert_eq!(EncryptionType::from_str("open"), EncryptionType::Open);
        assert_eq!(EncryptionType::from_str("UNKNOWN"), EncryptionType::Unknown);
    }

    #[test]
    fn test_ap_display_name() {
        let mac = MacAddr::from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let ap = AccessPoint::new(mac, 6, -50, EncryptionType::Wpa2, "TestVendor".to_string());
        assert_eq!(ap.display_name(), "aa:bb:cc:dd:ee:ff");
        
        let ap_with_ssid = ap.with_ssid("MyNetwork".to_string());
        assert_eq!(ap_with_ssid.display_name(), "MyNetwork");
    }
}