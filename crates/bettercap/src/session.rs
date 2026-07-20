//! Deserialization of bettercap's `GET /api/session/wifi` response and
//! conversion into `pwncore` types.
//!
//! The JSON shape here is not guessed: it was fetched and read directly
//! from bettercap's own Go source (`network/wifi.go`, `network/wifi_ap.go`,
//! `network/wifi_station.go`, `network/lan_endpoint.go` in
//! github.com/bettercap/bettercap). Key facts that shape this module:
//!
//! - `GET /api/session/wifi` returns `network.WiFi`'s `MarshalJSON`, which
//!   is exactly `{"aps": [...]}` (`wifiJSON` struct in `network/wifi.go`).
//! - Each AP is a Go struct that embeds `*Station` which itself embeds
//!   `*Endpoint`; Go flattens anonymous embedded fields into JSON, so every
//!   AP object has `Endpoint`'s fields (`mac`, `hostname`, `vendor`,
//!   `first_seen`, `last_seen`, ...) and `Station`'s fields (`channel`,
//!   `rssi`, `encryption`, ...) all at the top level, plus the AP-specific
//!   `clients` (array of the same flattened shape) and `handshake` (bool,
//!   true once bettercap has a complete handshake for this AP -- see
//!   `AccessPoint.MarshalJSON` in `network/wifi_ap.go`, which sets it from
//!   `ap.withKeyMaterial`).
//! - Every field this module doesn't need is simply not deserialized;
//!   `serde(default)` on optional/uncertain fields means an upstream schema
//!   change degrades gracefully (missing field -> default) rather than
//!   breaking parsing entirely.

use pwncore::{AccessPoint, EncryptionType, MacAddr, Station};
use serde::Deserialize;
use std::str::FromStr;

/// `GET /api/session/wifi` response body.
#[derive(Debug, Deserialize)]
pub struct WifiSession {
    #[serde(default, rename = "aps")]
    pub access_points: Vec<BettercapAp>,
}

/// One access point, as bettercap reports it (flattened Endpoint+Station+AP
/// fields -- see module doc for the source evidence).
#[derive(Debug, Clone, Deserialize)]
pub struct BettercapAp {
    #[serde(rename = "mac")]
    pub mac: String,
    #[serde(default, rename = "hostname")]
    pub hostname: String,
    #[serde(default, rename = "channel")]
    pub channel: i32,
    #[serde(default, rename = "rssi")]
    pub rssi: i8,
    #[serde(default, rename = "encryption")]
    pub encryption: String,
    #[serde(default, rename = "vendor")]
    pub vendor: String,
    #[serde(default, rename = "clients")]
    pub clients: Vec<BettercapStation>,
    /// True once bettercap holds a complete key-material capture for this
    /// AP (`AccessPoint.withKeyMaterial` in bettercap's Go source). We still
    /// treat the appearance of a real handshake *file* in
    /// `wifi.handshakes.file`'s directory as the authoritative capture
    /// signal (via the existing `agent::capture::CaptureManager` file-scan
    /// pipeline, unchanged from the AngryOxide era) -- this field is used
    /// only to mark an AP `handshake_captured` for display/targeting
    /// purposes, not as the trigger for the capture pipeline itself.
    #[serde(default, rename = "handshake")]
    pub handshake: bool,
}

/// One associated station/client, same flattened shape minus the AP-only
/// `clients`/`handshake` fields.
#[derive(Debug, Clone, Deserialize)]
pub struct BettercapStation {
    #[serde(rename = "mac")]
    pub mac: String,
    #[serde(default, rename = "vendor")]
    pub vendor: String,
    #[serde(default, rename = "rssi")]
    pub rssi: i8,
    #[serde(default, rename = "channel")]
    pub channel: i32,
}

fn parse_encryption(s: &str) -> EncryptionType {
    // bettercap reports whatever the 802.11 RSN/WPA IE parsing found, e.g.
    // "WPA2", "WPA3", "WPA", "WEP", or empty for open networks
    // (`Station.IsOpen()` in bettercap's Go source treats "" as open).
    match s.to_uppercase().as_str() {
        "" | "OPEN" => EncryptionType::Open,
        "WEP" => EncryptionType::Wep,
        "WPA3" | "WPA3-SAE" | "SAE" => EncryptionType::Wpa3,
        "WPA2" => EncryptionType::Wpa2,
        s if s.starts_with("WPA") => EncryptionType::Wpa,
        _ => EncryptionType::Unknown,
    }
}

/// Convert a bettercap-reported channel (already 802.11 channel number, per
/// `network.Dot11Freq2Chan`) into our `1..=14` domain, clamping anything
/// bettercap couldn't resolve (0, or an out-of-range 5GHz/6GHz channel we
/// don't track) to channel 1 rather than dropping the AP.
fn clamp_channel(ch: i32) -> u8 {
    if (1..=14).contains(&ch) {
        ch as u8
    } else {
        1
    }
}

impl BettercapStation {
    /// Convert to a `pwncore::Station`. Returns `None` if bettercap sent a
    /// MAC we can't parse (malformed/partial data), rather than fabricating
    /// a placeholder station.
    pub fn to_pwncore(&self) -> Option<Station> {
        let mac = MacAddr::from_str(&self.mac).ok()?;
        Some(Station::new(
            mac,
            self.vendor.clone(),
            self.rssi as i16,
            clamp_channel(self.channel),
        ))
    }
}

impl BettercapAp {
    /// Convert to a `pwncore::AccessPoint`, including its clients. Returns
    /// `None` if the BSSID doesn't parse.
    pub fn to_pwncore(&self) -> Option<AccessPoint> {
        let bssid = MacAddr::from_str(&self.mac).ok()?;
        let mut ap = AccessPoint::new(
            bssid,
            clamp_channel(self.channel),
            self.rssi as i16,
            parse_encryption(&self.encryption),
            self.vendor.clone(),
        );
        if !self.hostname.is_empty() {
            ap = ap.with_ssid(self.hostname.clone());
        }
        ap.handshake_captured = self.handshake;
        for client in &self.clients {
            if let Some(station) = client.to_pwncore() {
                ap.add_client(station);
            }
        }
        Some(ap)
    }
}

impl WifiSession {
    /// Convert every parseable AP into `pwncore::AccessPoint`s. APs with an
    /// unparseable MAC are skipped (logged by the caller), not fabricated.
    pub fn to_pwncore(&self) -> Vec<AccessPoint> {
        self.access_points
            .iter()
            .filter_map(BettercapAp::to_pwncore)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real shape confirmed against bettercap's network/wifi.go +
    // network/wifi_ap.go + network/wifi_station.go + network/lan_endpoint.go.
    const SAMPLE_SESSION: &str = r#"{
        "aps": [
            {
                "mac": "AA:BB:CC:DD:EE:FF",
                "hostname": "MyNetwork",
                "alias": "",
                "vendor": "TP-Link",
                "ipv4": "",
                "ipv6": "",
                "first_seen": "2024-01-01T00:00:00Z",
                "last_seen": "2024-01-01T00:01:00Z",
                "meta": {},
                "frequency": 2412,
                "channel": 1,
                "rssi": -45,
                "sent": 0,
                "received": 0,
                "encryption": "WPA2",
                "cipher": "CCMP",
                "authentication": "PSK",
                "wps": {},
                "clients": [
                    {
                        "mac": "11:22:33:44:55:66",
                        "hostname": "",
                        "vendor": "Apple",
                        "channel": 1,
                        "rssi": -60,
                        "frequency": 2412,
                        "sent": 0,
                        "received": 0,
                        "encryption": "",
                        "cipher": "",
                        "authentication": "",
                        "wps": {}
                    }
                ],
                "handshake": true
            },
            {
                "mac": "00:11:22:33:44:55",
                "hostname": "OpenGuest",
                "vendor": "",
                "channel": 6,
                "rssi": -70,
                "frequency": 2437,
                "encryption": "",
                "clients": [],
                "handshake": false
            }
        ]
    }"#;

    #[test]
    fn test_parse_real_bettercap_session_shape() {
        let session: WifiSession = serde_json::from_str(SAMPLE_SESSION).unwrap();
        assert_eq!(session.access_points.len(), 2);

        let ap0 = &session.access_points[0];
        assert_eq!(ap0.mac, "AA:BB:CC:DD:EE:FF");
        assert_eq!(ap0.hostname, "MyNetwork");
        assert_eq!(ap0.channel, 1);
        assert_eq!(ap0.rssi, -45);
        assert!(ap0.handshake);
        assert_eq!(ap0.clients.len(), 1);
        assert_eq!(ap0.clients[0].mac, "11:22:33:44:55:66");
    }

    #[test]
    fn test_to_pwncore_converts_aps_and_clients() {
        let session: WifiSession = serde_json::from_str(SAMPLE_SESSION).unwrap();
        let aps = session.to_pwncore();
        assert_eq!(aps.len(), 2);

        let mynetwork = aps
            .iter()
            .find(|ap| ap.ssid.as_deref() == Some("MyNetwork"))
            .expect("MyNetwork AP present");
        assert_eq!(mynetwork.channel.value(), 1);
        assert_eq!(mynetwork.rssi, -45);
        assert_eq!(mynetwork.encryption, EncryptionType::Wpa2);
        assert!(mynetwork.handshake_captured);
        assert_eq!(mynetwork.clients.len(), 1);

        let open = aps
            .iter()
            .find(|ap| ap.ssid.as_deref() == Some("OpenGuest"))
            .expect("OpenGuest AP present");
        assert_eq!(open.encryption, EncryptionType::Open);
        assert!(!open.handshake_captured);
    }

    #[test]
    fn test_empty_session_parses_to_no_aps() {
        let session: WifiSession = serde_json::from_str(r#"{"aps": []}"#).unwrap();
        assert!(session.to_pwncore().is_empty());
    }

    #[test]
    fn test_unparseable_mac_is_skipped_not_fabricated() {
        let ap = BettercapAp {
            mac: "not-a-mac".to_string(),
            hostname: "x".to_string(),
            channel: 1,
            rssi: -50,
            encryption: "WPA2".to_string(),
            vendor: String::new(),
            clients: vec![],
            handshake: false,
        };
        assert!(ap.to_pwncore().is_none());
    }

    #[test]
    fn test_out_of_range_channel_clamps_to_one() {
        assert_eq!(clamp_channel(0), 1);
        assert_eq!(clamp_channel(153), 1); // 5GHz channel we don't track
        assert_eq!(clamp_channel(11), 11);
    }

    #[test]
    fn test_parse_encryption_variants() {
        assert_eq!(parse_encryption(""), EncryptionType::Open);
        assert_eq!(parse_encryption("OPEN"), EncryptionType::Open);
        assert_eq!(parse_encryption("WEP"), EncryptionType::Wep);
        assert_eq!(parse_encryption("WPA2"), EncryptionType::Wpa2);
        assert_eq!(parse_encryption("WPA3"), EncryptionType::Wpa3);
        assert_eq!(parse_encryption("WPA"), EncryptionType::Wpa);
        assert_eq!(parse_encryption("something-else"), EncryptionType::Unknown);
    }
}
