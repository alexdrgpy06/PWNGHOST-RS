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
        if blacklist.contains(&self.bssid) {
            return false;
        }
        // Real pwnagotchi's `main.whitelist` protects listed SSIDs/BSSIDs
        // from ever being attacked (its own docs: useful so you don't
        // deauth your own network, or a neighbor's, constantly) -- an
        // exclude-list, not a restrict-targeting-to-only-these allow-scope.
        // A previous version of this function had that backwards ("a
        // non-empty whitelist restricts targets to listed BSSIDs"), which
        // -- once actually wired into `Agent::find_target` -- would have
        // inverted the real safety guarantee: whitelisting your own
        // network would have made it the *only* thing ever attacked.
        if whitelist.contains(&self.bssid) {
            return false;
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
    /// Every candidate kaomoji for this mood, in real jayofelony/pwnagotchi's
    /// own order.
    ///
    /// **Single source of truth** for faces across the whole workspace --
    /// `agent::faces::face_for_mood` and `ui/display`'s `face_for_mood` both
    /// delegate to [`Mood::face`], which picks randomly from this list.
    /// Corrected from an earlier version of this table that claimed upstream
    /// faces were "exactly one per mood, not randomized" -- that was wrong:
    /// it was checked against `pwnagotchi/ui/faces.py`'s bare Python
    /// fallback constants, but real pwnagotchi always loads `default.toml`'s
    /// `[ui.faces]` section on boot (`faces.load_from_config`, and
    /// `default.toml` is regenerated every restart, so this always applies),
    /// which overrides every one of those constants with a list, and
    /// `view.py::_get_random_face` does `random.choice()` whenever it gets a
    /// list. Verified directly against a real device's `default.toml`, not
    /// re-derived from faces.py alone this time.
    pub fn face_variants(&self) -> &'static [&'static str] {
        match self {
            Mood::LookR => &["( ⚆_⚆)"],
            Mood::LookL => &["(☉_☉ )"],
            Mood::LookRHappy => &["( ◕‿◕)", "( ≧◡≦)"],
            Mood::LookLHappy => &["(◕‿◕ )", "(≧◡≦ )"],
            Mood::Sleep => &["(⇀‿‿↼)", "(≖‿‿≖)", "(－_－)"],
            Mood::Awake => &["(◕‿‿◕)"],
            Mood::Bored => &["(-__-)", "(—__—)"],
            Mood::Intense => &["(°▃▃°)", "(°ロ°)"],
            Mood::Cool => &["(⌐■_■)", "(단__단)"],
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

    /// A random face for this mood, matching real pwnagotchi's own
    /// `_get_random_face` behavior. See [`Mood::face_variants`] for why this
    /// is randomized rather than a fixed single string.
    pub fn face(&self) -> &'static str {
        use rand::seq::SliceRandom;
        let variants = self.face_variants();
        variants
            .choose(&mut rand::thread_rng())
            .copied()
            .unwrap_or(variants[0])
    }

    /// Every candidate status-line phrase for this mood, ported verbatim
    /// (English source strings, not the gettext-wrapped originals -- this
    /// project has no i18n layer) from real jayofelony/pwnagotchi's
    /// `pwnagotchi/voice.py`. Real pwnagotchi's own `on_bored`/`on_sad`/
    /// `on_angry`/etc. each return `random.choice()` over a pool like this;
    /// a previous version of this table (in `Personality::get_phrase`) had
    /// exactly one fixed, emoji-decorated string per mood with no variety
    /// and no connection to real pwnagotchi's actual phrasing at all --
    /// this is the real content, matching `face_variants`' precedent for
    /// how this project ports real per-mood variety.
    ///
    /// Not every real `voice.py` method maps to a `Mood` (several take
    /// event data -- a peer name, a MAC, a handshake count -- that doesn't
    /// fit a pure mood lookup); those are wired as standalone event-based
    /// calls at their real trigger sites instead (see
    /// `crates/pwnghost-rs/src/main.rs`'s handshake-capture/deauth/associate
    /// call sites), not here.
    pub fn voice_lines(&self) -> &'static [&'static str] {
        match self {
            Mood::Bored => &["I'm bored ...", "Let's go for a walk!"],
            Mood::Sad => &[
                "I'm extremely bored ...",
                "I'm very sad ...",
                "I'm sad",
                "I'm so happy ...",
                "Life? Don't talk to me about life.",
                "...",
            ],
            Mood::Angry => &["...", "Leave me alone ...", "I'm mad at you!"],
            Mood::Excited => &[
                "I'm living the life!",
                "I pwn therefore I am.",
                "So many networks!!!",
                "I'm having so much fun!",
                "It's a Wi-Fi system! I know this!",
                "My crime is that of curiosity ...",
            ],
            Mood::Grateful => &["Good friends are a blessing!", "I love my friends!"],
            Mood::Lonely => &[
                "Nobody wants to play with me ...",
                "I feel so alone ...",
                "Let's find friends",
                "Where's everybody?!",
            ],
            // real pwnagotchi's `on_awakening` (waking from sleep)
            Mood::Awake => &["...", "!", "Hello World!", "I dreamed of electric sheep"],
            // real pwnagotchi's `on_shutdown`
            Mood::Sleep => &["Good night.", "Zzz"],
            // real pwnagotchi's `on_waiting` (the idle look-around loop,
            // matching this project's Recon-mode LookR/LookL alternation)
            Mood::LookR | Mood::LookL => &["...", "Looking around ..."],
            Mood::LookRHappy | Mood::LookLHappy => &["...", "Looking around ..."],
            // real pwnagotchi's `on_motivated`/`on_demotivated`
            Mood::Motivated => &[
                "This is the best day of my life!",
                "All your base are belong to us",
                "Fascinating!",
            ],
            Mood::Demotivated => &["Shitty day :/"],
            // real pwnagotchi's `on_rebooting`
            Mood::Broken => &[
                "Oops, something went wrong ... Rebooting ...",
                "Well, this is awkward.",
                "Tell my packets I love them.",
                "Have you tried turning it off and on again?",
                "I'm afraid Dave",
                "I'm dead, Jim!",
                "I have a bad feeling about this",
                "You did this.",
            ],
            // No direct real-pwnagotchi mood-level equivalent for these
            // (real pwnagotchi sets Happy/friend text dynamically per-event
            // instead -- see `on_handshakes`/`on_new_peer`/`on_assoc` at
            // their real call sites) -- kept close in spirit rather than
            // inventing unrelated flavor text.
            // Lines containing `{name}`/`{ap}`/`{sta}` are templates that
            // `voice_line_with_context` interpolates at runtime.
            Mood::Happy => &[
                "Cool, we got a new handshake!",
                "Yes! Captured {ap}!",
                "Got ya {ap}!",
            ],
            Mood::Friend => &[
                "Yo! Sup?",
                "Hey, how are you doing?",
                "Hey I know {name}!",
                "Hello {name}, you're new here!",
            ],
            Mood::Intense => &[
                "Associating ...",
                "Yo!",
                "Associating with {ap} ...",
            ],
            Mood::Cool => &[
                "Deauthenticating ...",
                "No more Wi-Fi for you!",
                "Deauthing {sta} from {ap}",
                "Bye bye {sta}!",
            ],
            Mood::Smart => &[
                "Hey, a free channel! Your AP will say thanks.",
                "This channel is all mine!",
            ],
            Mood::Upload => &["Uploading data ...", "Beam me up!"],
        }
    }

    /// A random status-line phrase for this mood. See [`Mood::voice_lines`].
    pub fn voice_line(&self) -> &'static str {
        use rand::seq::SliceRandom;
        let variants = self.voice_lines();
        variants
            .choose(&mut rand::thread_rng())
            .copied()
            .unwrap_or(variants[0])
    }

    /// Like [`Mood::voice_line`] but substitutes `{name}`, `{ap}`, and
    /// `{sta}` placeholders with runtime context.  If a placeholder is
    /// present in the chosen template but no value is provided, it is
    /// replaced with the empty string.  This mirrors real pwnagotchi's
    /// voice interpolation system where mood-triggering events carry
    /// AP/station/peer info into the displayed line.
    pub fn voice_line_with_context(
        &self,
        name: Option<&str>,
        ap: Option<&str>,
        sta: Option<&str>,
    ) -> String {
        let template = self.voice_line();
        let mut s = template
            .replace("{name}", name.unwrap_or(""))
            .replace("{ap}", ap.unwrap_or(""))
            .replace("{sta}", sta.unwrap_or(""));
        // Collapse double spaces left by empty substitutions (e.g.
        // "Deauthing  from " -> "Deauthing  "), then trim trailing
        // whitespace so an empty trailing field still reads well.
        while s.contains("  ") {
            s = s.replace("  ", " ");
        }
        s.trim().to_string()
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
    fn test_mood_face() {
        // face() picks randomly among the mood's real variants (matching
        // upstream default.toml + view.py's random.choice behavior), so
        // check membership rather than a single fixed value.
        for _ in 0..50 {
            assert!(Mood::Happy.face_variants().contains(&Mood::Happy.face()));
            assert!(Mood::Angry.face_variants().contains(&Mood::Angry.face()));
            assert!(Mood::Lonely.face_variants().contains(&Mood::Lonely.face()));
        }
    }

    #[test]
    fn test_mood_face_single_variant_moods_are_stable() {
        // Moods with exactly one real variant (awake, grateful, smart,
        // broken, look_r, look_l) should always return that one value.
        assert_eq!(Mood::Awake.face(), "(◕‿‿◕)");
        assert_eq!(Mood::Grateful.face(), "(^‿‿^)");
        assert_eq!(Mood::Broken.face(), "(☓‿‿☓)");
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

        // Whitelist match = protected, NOT a target (an exclude-list,
        // matching real pwnagotchi's actual "never attack this" semantic)
        assert!(!ap.is_target(&["aa:bb:cc:dd:ee:ff".parse().unwrap()], &[]));

        // A whitelist entry for a *different* BSSID doesn't protect this one
        assert!(ap.is_target(&["11:22:33:44:55:66".parse().unwrap()], &[]));

        // Blacklist match = not target
        assert!(!ap.is_target(&[], &["aa:bb:cc:dd:ee:ff".parse().unwrap()]));

        // Both lists agreeing still excludes (no real conflict now that
        // whitelist and blacklist both mean "exclude")
        assert!(!ap.is_target(
            &["aa:bb:cc:dd:ee:ff".parse().unwrap()],
            &["aa:bb:cc:dd:ee:ff".parse().unwrap()]
        ));
    }
}
