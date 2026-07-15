//! AngryOxide JSON line parser

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// AngryOxide event types parsed from JSON lines
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AngryOxideEvent {
    /// Access point discovered/updated
    Ap(ApEvent),
    /// Client station discovered/updated
    Client(ClientEvent),
    /// Handshake captured
    Handshake(HandshakeEvent),
    /// Statistics update
    Stats(StatsEvent),
    /// Channel hop
    Channel(ChannelEvent),
    /// Status/error message
    Status(StatusEvent),
}

/// Legacy alias for backward compatibility
pub type AoEvent = AngryOxideEvent;

/// Access point event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApEvent {
    pub bssid: String,
    pub ssid: Option<String>,
    pub channel: u8,
    pub rssi: i16,
    pub encryption: String,
    pub vendor: String,
    pub clients: Vec<ClientInfo>,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// Client station info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub mac: String,
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
}

/// Client station event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEvent {
    pub mac: String,
    pub vendor: String,
    pub bssid: String,
    pub channel: u8,
    pub rssi: i16,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// Handshake captured event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeEvent {
    pub bssid: String,
    pub station: String,
    pub file: String,
    pub handshake_type: String, // "PMKID" or "WPA" or "WPA2"
    pub timestamp: u64,
}

/// Statistics event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsEvent {
    pub aps_count: u32,
    pub clients_count: u32,
    pub handshakes_count: u32,
    pub channel: u8,
    pub uptime: u64,
}

/// Channel hop event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEvent {
    pub channel: u8,
    pub timestamp: u64,
}

/// Status/error message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEvent {
    pub level: String, // "info", "warn", "error"
    pub message: String,
    pub timestamp: u64,
}

/// Parse a single line of AngryOxide JSON output
pub fn parse_json_line(line: &str) -> Result<AngryOxideEvent> {
    let line = line.trim();
    if line.is_empty() {
        anyhow::bail!("Empty line");
    }

    // Try to parse as JSON
    let value: serde_json::Value = serde_json::from_str(line)
        .with_context(|| format!("Failed to parse JSON: {}", line))?;

    // Determine event type from structure
    if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
        match event_type {
            "ap" => parse_ap_event(value),
            "client" => parse_client_event(value),
            "handshake" => parse_handshake_event(value),
            "stats" => parse_stats_event(value),
            "channel" => parse_channel_event(value),
            "status" => parse_status_event(value),
            _ => anyhow::bail!("Unknown event type: {}", event_type),
        }
    } else {
        // Try to infer from fields
        infer_event_type(value)
    }
}

fn parse_ap_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let ap: ApEvent = serde_json::from_value(value)
        .context("Failed to parse AP event")?;
    Ok(AngryOxideEvent::Ap(ap))
}

fn parse_client_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let client: ClientEvent = serde_json::from_value(value)
        .context("Failed to parse client event")?;
    Ok(AngryOxideEvent::Client(client))
}

fn parse_handshake_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let hs: HandshakeEvent = serde_json::from_value(value)
        .context("Failed to parse handshake event")?;
    Ok(AngryOxideEvent::Handshake(hs))
}

fn parse_stats_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let stats: StatsEvent = serde_json::from_value(value)
        .context("Failed to parse stats event")?;
    Ok(AngryOxideEvent::Stats(stats))
}

fn parse_channel_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let ch: ChannelEvent = serde_json::from_value(value)
        .context("Failed to parse channel event")?;
    Ok(AngryOxideEvent::Channel(ch))
}

fn parse_status_event(value: serde_json::Value) -> Result<AngryOxideEvent> {
    let status: StatusEvent = serde_json::from_value(value)
        .context("Failed to parse status event")?;
    Ok(AngryOxideEvent::Status(status))
}

fn infer_event_type(value: serde_json::Value) -> Result<AngryOxideEvent> {
    // Infer from fields present
    if value.get("bssid").is_some() && value.get("ssid").is_some() {
        parse_ap_event(value)
    } else if value.get("bssid").is_some() && value.get("station").is_some() {
        parse_handshake_event(value)
    } else if value.get("aps_count").is_some() {
        parse_stats_event(value)
    } else if value.get("channel").is_some() && value.get("timestamp").is_some() {
        parse_channel_event(value)
    } else if value.get("message").is_some() {
        parse_status_event(value)
    } else {
        anyhow::bail!("Cannot infer event type from: {}", value);
    }
}

/// Internal event types for agent consumption
#[derive(Debug, Clone)]
pub enum InternalEvent {
    Ap(ApData),
    Client(ClientData),
    Handshake(HandshakeData),
    Stats(StatsData),
    Channel(ChannelData),
    Status(StatusData),
}

#[derive(Debug, Clone)]
pub struct ApData {
    pub bssid: [u8; 6],
    pub ssid: Option<String>,
    pub channel: u8,
    pub rssi: i16,
    pub encryption: EncryptionType,
    pub vendor: String,
    pub clients: Vec<ClientData>,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone)]
pub struct ClientData {
    pub mac: [u8; 6],
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
}

#[derive(Debug, Clone)]
pub struct HandshakeData {
    pub bssid: [u8; 6],
    pub station: [u8; 6],
    pub file: String,
    pub handshake_type: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct StatsData {
    pub aps_count: u32,
    pub clients_count: u32,
    pub handshakes_count: u32,
    pub channel: u8,
    pub uptime: u64,
}

#[derive(Debug, Clone)]
pub struct ChannelData {
    pub channel: u8,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct StatusData {
    pub level: String,
    pub message: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionType {
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Unknown,
}

/// Convert AoEvent to internal types
pub fn ao_event_to_internal(event: AngryOxideEvent) -> Result<InternalEvent> {
    match event {
        AngryOxideEvent::Ap(ap) => Ok(InternalEvent::Ap(ApData {
            bssid: parse_mac(&ap.bssid)?,
            ssid: ap.ssid,
            channel: ap.channel,
            rssi: ap.rssi,
            encryption: parse_encryption(&ap.encryption),
            vendor: ap.vendor,
            clients: ap.clients.into_iter().map(|c| ClientData {
                mac: parse_mac(&c.mac).unwrap_or([0; 6]),
                vendor: c.vendor,
                rssi: c.rssi,
                channel: c.channel,
            }).collect(),
            first_seen: ap.first_seen,
            last_seen: ap.last_seen,
        })),
        AngryOxideEvent::Client(client) => Ok(InternalEvent::Client(ClientData {
            mac: parse_mac(&client.mac)?,
            vendor: client.vendor,
            rssi: client.rssi,
            channel: client.channel,
        })),
        AngryOxideEvent::Handshake(hs) => Ok(InternalEvent::Handshake(HandshakeData {
            bssid: parse_mac(&hs.bssid)?,
            station: parse_mac(&hs.station)?,
            file: hs.file,
            handshake_type: hs.handshake_type,
            timestamp: hs.timestamp,
        })),
        AngryOxideEvent::Stats(stats) => Ok(InternalEvent::Stats(StatsData {
            aps_count: stats.aps_count,
            clients_count: stats.clients_count,
            handshakes_count: stats.handshakes_count,
            channel: stats.channel,
            uptime: stats.uptime,
        })),
        AngryOxideEvent::Channel(ch) => Ok(InternalEvent::Channel(ChannelData {
            channel: ch.channel,
            timestamp: ch.timestamp,
        })),
        AngryOxideEvent::Status(status) => Ok(InternalEvent::Status(StatusData {
            level: status.level,
            message: status.message,
            timestamp: status.timestamp,
        })),
    }
}

fn parse_mac(s: &str) -> Result<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        anyhow::bail!("Invalid MAC format: {}", s);
    }
    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .with_context(|| format!("Invalid MAC byte: {}", part))?;
    }
    Ok(bytes)
}

fn parse_encryption(s: &str) -> EncryptionType {
    match s.to_uppercase().as_str() {
        "OPEN" | "" => EncryptionType::Open,
        "WEP" => EncryptionType::Wep,
        "WPA" => EncryptionType::Wpa,
        "WPA2" | "WPA2-PSK" | "WPA2-CCMP" => EncryptionType::Wpa2,
        "WPA3" | "WPA3-SAE" => EncryptionType::Wpa3,
        _ => EncryptionType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ap_event() {
        let json = r#"{"type":"ap","bssid":"aa:bb:cc:dd:ee:ff","ssid":"TestAP","channel":6,"rssi":-45,"encryption":"WPA2","vendor":"TestVendor","clients":[],"first_seen":1000,"last_seen":2000}"#;
        let event = parse_json_line(json).unwrap();
        match event {
            AngryOxideEvent::Ap(ap) => {
                assert_eq!(ap.bssid, "aa:bb:cc:dd:ee:ff");
                assert_eq!(ap.ssid, Some("TestAP".to_string()));
                assert_eq!(ap.channel, 6);
                assert_eq!(ap.rssi, -45);
                assert_eq!(ap.encryption, "WPA2");
            }
            _ => panic!("Expected Ap event"),
        }
    }

    #[test]
    fn test_parse_handshake_event() {
        let json = r#"{"type":"handshake","bssid":"aa:bb:cc:dd:ee:ff","station":"11:22:33:44:55:66","file":"/tmp/handshake.pcapng","handshake_type":"WPA2","timestamp":1234567890}"#;
        let event = parse_json_line(json).unwrap();
        match event {
            AngryOxideEvent::Handshake(hs) => {
                assert_eq!(hs.bssid, "aa:bb:cc:dd:ee:ff");
                assert_eq!(hs.station, "11:22:33:44:55:66");
                assert_eq!(hs.handshake_type, "WPA2");
            }
            _ => panic!("Expected Handshake event"),
        }
    }

    #[test]
    fn test_parse_stats_event() {
        let json = r#"{"type":"stats","aps_count":10,"clients_count":5,"handshakes_count":2,"channel":6,"uptime":3600}"#;
        let event = parse_json_line(json).unwrap();
        match event {
            AngryOxideEvent::Stats(stats) => {
                assert_eq!(stats.aps_count, 10);
                assert_eq!(stats.clients_count, 5);
                assert_eq!(stats.handshakes_count, 2);
                assert_eq!(stats.channel, 6);
            }
            _ => panic!("Expected Stats event"),
        }
    }

    #[test]
    fn test_parse_mac() {
        let mac = parse_mac("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(mac, [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        assert_eq!(format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn test_parse_encryption() {
        assert_eq!(parse_encryption("WPA2"), EncryptionType::Wpa2);
        assert_eq!(parse_encryption("open"), EncryptionType::Open);
        assert_eq!(parse_encryption("WPA3"), EncryptionType::Wpa3);
        assert_eq!(parse_encryption("UNKNOWN"), EncryptionType::Unknown);
    }

    #[test]
    fn test_ao_event_to_internal() {
        let json = r#"{"type":"ap","bssid":"aa:bb:cc:dd:ee:ff","ssid":"TestAP","channel":6,"rssi":-45,"encryption":"WPA2","vendor":"TestVendor","clients":[],"first_seen":1000,"last_seen":2000}"#;
        let event = parse_json_line(json).unwrap();
        let internal = ao_event_to_internal(event).unwrap();

        match internal {
            InternalEvent::Ap(ap) => {
                assert_eq!(ap.bssid, [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
                assert_eq!(ap.ssid, Some("TestAP".to_string()));
                assert_eq!(ap.channel, 6);
                assert_eq!(ap.rssi, -45);
                assert_eq!(ap.encryption, EncryptionType::Wpa2);
            }
            _ => panic!("Expected Ap event"),
        }
    }
}