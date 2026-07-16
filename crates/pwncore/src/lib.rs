//! Core domain types for PwnGhost-RS
//!
//! This crate defines the fundamental types used across the workspace:
//! - AccessPoint, Station, Handshake
//! - Channel, EncryptionType
//! - Epoch, Mood, Personality types

use anyhow::Result;
use chrono::{DateTime, Utc};
pub use mac_addr::MacAddr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 802.11 encryption types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum EncryptionType {
    Wpa,
    Wpa2,
    Wpa3,
    Wep,
    Open,
    Unknown,
}

impl EncryptionType {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "WPA" => EncryptionType::Wpa,
            "WPA2" | "WPA2-PSK" | "WPA2-CCMP" => EncryptionType::Wpa2,
            "WPA3" | "WPA3-SAE" => EncryptionType::Wpa3,
            "WEP" => EncryptionType::Wep,
            "OPEN" | "" => EncryptionType::Open,
            _ => EncryptionType::Unknown,
        }
    }
}

/// IEEE 802.11 channel (1-14 for 2.4 GHz)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Channel(pub u8);

impl Channel {
    pub fn new(ch: u8) -> Result<Self> {
        anyhow::ensure!((1..=14).contains(&ch), "Invalid channel: {}", ch);
        Ok(Self(ch))
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    /// Non-overlapping 2.4 GHz channels
    pub const NON_OVERLAPPING: [Channel; 3] = [Channel(1), Channel(6), Channel(11)];
}

/// Access Point (BSSID + SSID + metadata)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessPoint {
    pub bssid: MacAddr,
    pub ssid: Option<String>,
    pub channel: Channel,
    pub rssi: i16,
    pub encryption: EncryptionType,
    pub vendor: String,
    pub clients: Vec<Station>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
    pub pmkid_captured: bool,
}

impl AccessPoint {
    /// Create an access point observed on `channel` (1-14).
    pub fn new(
        bssid: MacAddr,
        channel: u8,
        rssi: i16,
        encryption: EncryptionType,
        vendor: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            bssid,
            ssid: None,
            channel: Channel::new(channel).unwrap_or(Channel(1)),
            rssi,
            encryption,
            vendor,
            clients: Vec::new(),
            first_seen: now,
            last_seen: now,
            handshake_captured: false,
            pmkid_captured: false,
        }
    }

    /// Builder-style helper to set the SSID.
    pub fn with_ssid(mut self, ssid: String) -> Self {
        self.ssid = Some(ssid);
        self
    }

    /// Add (or replace) a client station associated with this AP.
    pub fn add_client(&mut self, client: Station) {
        if let Some(existing) = self.clients.iter_mut().find(|c| c.mac == client.mac) {
            *existing = client;
        } else {
            self.clients.push(client);
        }
    }

    pub fn is_target(&self, whitelist: &[MacAddr], blacklist: &[MacAddr]) -> bool {
        // Blacklist always wins, even over an explicit whitelist entry.
        if blacklist.contains(&self.bssid) {
            return false;
        }
        // A non-empty whitelist restricts targets to listed BSSIDs.
        if !whitelist.is_empty() {
            return whitelist.contains(&self.bssid);
        }
        true
    }
}

/// Client Station (associated with an AP)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Station {
    pub mac: MacAddr,
    pub vendor: String,
    pub rssi: i16,
    pub channel: u8,
    pub last_seen: DateTime<Utc>,
    pub handshake_captured: bool,
}

impl Station {
    /// Create a client station observed on `channel`.
    pub fn new(mac: MacAddr, vendor: String, rssi: i16, channel: u8) -> Self {
        Self {
            mac,
            vendor,
            rssi,
            channel,
            last_seen: Utc::now(),
            handshake_captured: false,
        }
    }
}

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

impl Handshake {
    /// Create a new (unvalidated) handshake record for `bssid` on `channel`.
    pub fn new(bssid: MacAddr, channel: Channel) -> Self {
        Self {
            id: Uuid::new_v4(),
            bssid,
            ssid: None,
            channel,
            handshake_type: HandshakeType::WPA2,
            pcapng_path: String::new(),
            hashcat_path: String::new(),
            captured_at: Utc::now(),
            validated: false,
            gps: None,
        }
    }
}

/// Type of handshake captured
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HandshakeType {
    WPA2,  // 4-way handshake
    WPA3,  // SAE handshake
    PMKID, // PMKID attack
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

/// Epoch state (replaces Python's epoch tracking)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochState {
    pub epoch: u64,
    pub channel: Channel,
    pub mode: AgentMode,
    pub aps_found: usize,
    pub handshakes_this_epoch: u32,
    pub deauths_sent: u32,
    pub assoc_attempts: u32,
    pub mood: Mood,
    pub timestamp: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub ended_at: Option<DateTime<Utc>>,
}

impl EpochState {
    /// Create a new epoch state for the given epoch number and channel.
    pub fn new(epoch: u64, channel: Channel) -> Self {
        let now = Utc::now();
        Self {
            epoch,
            channel,
            mode: AgentMode::Recon,
            aps_found: 0,
            handshakes_this_epoch: 0,
            deauths_sent: 0,
            assoc_attempts: 0,
            mood: Mood::Awake,
            timestamp: now,
            started_at: now,
            ended_at: None,
        }
    }

    /// Wall-clock duration of this epoch (uses `ended_at` if finalized, else now).
    pub fn duration(&self) -> std::time::Duration {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        (end - self.started_at).to_std().unwrap_or_default()
    }

    /// Mark this epoch as finished.
    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());
    }
}

/// Agent operating mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AgentMode {
    Recon,
    Attack,
    Hop,
    Sleep,
}

/// Classic pwnagotchi moods (21 moods)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    LookR,
    LookL,
    LookRHappy,
    LookLHappy,
    Sleep,
    Awake,
    Bored,
    Intense,
    Cool,
    Happy,
    Excited,
    Grateful,
    Motivated,
    Demotivated,
    Smart,
    Lonely,
    Sad,
    Angry,
    Friend,
    Broken,
    Upload,
}

impl Mood {
    /// Get kaomoji faces for this mood
    pub fn faces(&self) -> &'static [&'static str] {
        match self {
            Mood::LookR => &["( ⚆_⚆)", "(☉_☉ )"],
            Mood::LookL => &["(☉_☉ )", "( ⚆_⚆)"],
            Mood::LookRHappy => &["( ◕‿◕)", "( ≧◡≦)"],
            Mood::LookLHappy => &["(◕‿◕ )", "(≧◡≦ )"],
            Mood::Sleep => &["(⇀‿‿↼)", "(≖‿‿≖)", "(－_－)"],
            Mood::Awake => &["(◕‿‿◕)"],
            Mood::Bored => &["(-__-)", "(—__—)"],
            Mood::Intense => &["(°▃▃°)", "(°ロ°)"],
            Mood::Cool => &["(⌐■_■)", "(单__单)"],
            Mood::Happy => &["(•‿‿•)", "(^‿‿^)", "(^◡◡^)"],
            Mood::Excited => &["(ᵔ◡◡ᵔ)", "(✜‿‿✜)"],
            Mood::Grateful => &["(^‿‿^)"],
            Mood::Motivated => &["(☼‿‿☼)", "(★‿★)", "(•̀ᴗ•́)"],
            Mood::Demotivated => &["(≖__≖)", "(￣ヘ￣)", "(¬､¬)"],
            Mood::Smart => &["(✜‿‿✜)"],
            Mood::Lonely => &["(ب__ب)", "(｡•́︿•̀｡)", "(︶︹︺)"],
            Mood::Sad => &["(╥☁╥ )", "(╥﹏╥)", "(ಥ﹏ಥ)"],
            Mood::Angry => &["(-_-')", "(⇀__⇀)", "(`___´)"],
            Mood::Friend => &["(♥‿‿♥)", "(♡‿‿♡)", "(♥‿♥ )", "(♥ω♥ )"],
            Mood::Broken => &["(☓‿‿☓)"],
            Mood::Upload => &["(1__0)", "(1__1)", "(0__1)"],
        }
    }

    /// Get a random face for this mood
    pub fn random_face(&self) -> &'static str {
        let faces = self.faces();
        let idx = rand::random::<usize>() % faces.len();
        faces[idx]
    }
}

/// Personality configuration (matches pwnagotchi personality.toml)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersonalityConfig {
    // Mood thresholds
    pub bored_num_epochs: u64,
    pub sad_num_epochs: u64,
    pub angry_num_epochs: u64,
    pub lonely_num_epochs: u64,

    // Activity factors
    pub bond_encounters_factor: f32,
    pub max_interactions: u32,
    pub throttle: u32,

    // Rewards
    pub reward_handshake: i32,
    pub reward_new_ap: i32,
    pub reward_association: i32,
    pub penalty_missed: i32,
    pub penalty_reboot: i32,

    // Behavior
    pub min_recon_time: u64,
    pub max_recon_time: u64,
    pub hop_recon_time: u64,

    // Attack settings
    pub deauth: bool,
    pub associate: bool,
    pub min_rssi: i16,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            bored_num_epochs: 50,
            sad_num_epochs: 100,
            angry_num_epochs: 200,
            lonely_num_epochs: 150,
            bond_encounters_factor: 1.0,
            max_interactions: 10,
            throttle: 30,
            reward_handshake: 100,
            reward_new_ap: 10,
            reward_association: 5,
            penalty_missed: -10,
            penalty_reboot: -50,
            min_recon_time: 5,
            max_recon_time: 30,
            hop_recon_time: 10,
            deauth: false,
            associate: false,
            min_rssi: -80,
        }
    }
}

/// Statistics for the web dashboard
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionStats {
    pub epoch: u64,
    pub uptime_secs: u64,
    pub total_aps: usize,
    pub total_handshakes: u32,
    pub total_pmkids: u32,
    pub current_channel: Channel,
    pub current_mood: Mood,
    pub current_face: String,
    pub level: u32,
    pub xp: u32,
    pub peers_seen: usize,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub cpu_temp: Option<f32>,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
}

/// Peer pwnagotchi (mesh networking)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Peer {
    pub mac: MacAddr,
    pub name: String,
    pub last_seen: DateTime<Utc>,
    pub epochs_since_seen: u64,
    pub handshakes_shared: u32,
    pub mood: Mood,
    pub channel: u8,
    pub signal: i16,
    pub level: u32,
    pub version: String,
}

impl Peer {
    /// Create a new peer seen on `channel` at signal strength `signal`.
    pub fn new(mac: MacAddr, name: String, channel: u8, signal: i16) -> Self {
        Self {
            mac,
            name,
            last_seen: Utc::now(),
            epochs_since_seen: 0,
            handshakes_shared: 0,
            mood: Mood::Friend,
            channel,
            signal,
            level: 0,
            version: String::new(),
        }
    }

    /// Mark the peer as seen right now, resetting its staleness counter.
    pub fn update_seen(&mut self) {
        self.last_seen = Utc::now();
        self.epochs_since_seen = 0;
    }

    /// Whether the peer has not been seen for more than `max_epochs` epochs.
    pub fn is_stale(&self, max_epochs: u64) -> bool {
        self.epochs_since_seen > max_epochs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_validation() {
        assert!(Channel::new(1).is_ok());
        assert!(Channel::new(14).is_ok());
        assert!(Channel::new(0).is_err());
        assert!(Channel::new(15).is_err());
    }

    #[test]
    fn test_encryption_from_str() {
        assert_eq!(EncryptionType::from_str("WPA2"), EncryptionType::Wpa2);
        assert_eq!(EncryptionType::from_str("wpa2"), EncryptionType::Wpa2);
        assert_eq!(EncryptionType::from_str("OPEN"), EncryptionType::Open);
        assert_eq!(EncryptionType::from_str("garbage"), EncryptionType::Unknown);
    }

    #[test]
    fn test_mood_faces() {
        assert!(!Mood::Happy.faces().is_empty());
        let face = Mood::Happy.random_face();
        assert!(Mood::Happy.faces().contains(&face));
    }

    #[test]
    fn test_ap_target_logic() {
        let ap = AccessPoint::new(
            "aa:bb:cc:dd:ee:ff".parse().unwrap(),
            6,
            -50,
            EncryptionType::Wpa2,
            String::new(),
        );

        // No filters = target
        assert!(ap.is_target(&[], &[]));

        // Whitelist match = target
        assert!(ap.is_target(&["aa:bb:cc:dd:ee:ff".parse().unwrap()], &[]));

        // Blacklist match = not target
        assert!(!ap.is_target(&[], &["aa:bb:cc:dd:ee:ff".parse().unwrap()]));

        // Blacklist overrides whitelist
        assert!(!ap.is_target(
            &["aa:bb:cc:dd:ee:ff".parse().unwrap()],
            &["aa:bb:cc:dd:ee:ff".parse().unwrap()]
        ));
    }
}
