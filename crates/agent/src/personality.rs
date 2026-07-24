//! Personality configuration and behavior

use crate::epoch::EpochState;
use chrono::{DateTime, Utc};
use pwncore::{AgentMode, Mood, Peer};
use std::collections::HashMap;

/// Personality configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersonalityConfig {
    // Mood thresholds (epochs)
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
    /// Missed-interaction threshold (pwnagotchi's `max_misses_for_recon`,
    /// default 5): after this many epochs of seeing APs but failing to engage
    /// (no valid target / no capture), the agent goes lonely -- and angry at
    /// 2x. Distinct from `blind_epochs` (no APs at all).
    pub max_misses_for_recon: u32,

    // Attack settings
    pub deauth: bool,
    pub associate: bool,
    pub min_rssi: i16,

    // Display
    pub position_x: i32,
    pub position_y: i32,
    pub frame_padding: bool,
    pub frame_padding_min_bytes: usize,
}

impl From<config::PersonalityConfig> for PersonalityConfig {
    fn from(c: config::PersonalityConfig) -> Self {
        Self {
            bored_num_epochs: c.bored_num_epochs,
            sad_num_epochs: c.sad_num_epochs,
            angry_num_epochs: c.angry_num_epochs,
            lonely_num_epochs: c.lonely_num_epochs,
            bond_encounters_factor: c.bond_encounters_factor,
            max_interactions: c.max_interactions,
            throttle: c.throttle,
            reward_handshake: c.reward_handshake,
            reward_new_ap: c.reward_new_ap,
            reward_association: c.reward_association,
            penalty_missed: c.penalty_missed,
            penalty_reboot: c.penalty_reboot,
            min_recon_time: c.min_recon_time,
            max_recon_time: c.max_recon_time,
            hop_recon_time: c.hop_recon_time,
            max_misses_for_recon: c.max_misses_for_recon,
            deauth: c.deauth,
            associate: c.associate,
            min_rssi: c.min_rssi,
            position_x: c.position_x,
            position_y: c.position_y,
            frame_padding: c.frame_padding,
            frame_padding_min_bytes: c.frame_padding_min_bytes,
        }
    }
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            // Match real pwnagotchi defaults (see config/defaults.toml note).
            bored_num_epochs: 15,
            sad_num_epochs: 25,
            angry_num_epochs: 200,
            lonely_num_epochs: 150,
            bond_encounters_factor: 20000.0,
            max_interactions: 3,
            throttle: 30,
            reward_handshake: 100,
            reward_new_ap: 10,
            reward_association: 5,
            penalty_missed: -10,
            penalty_reboot: -50,
            min_recon_time: 5,
            max_recon_time: 30,
            hop_recon_time: 10,
            max_misses_for_recon: 5,
            deauth: true,
            associate: true,
            min_rssi: -200, // match pwnagotchi's default (no practical floor)
            position_x: 0,
            position_y: 34,
            frame_padding: true,
            frame_padding_min_bytes: 650,
        }
    }
}

/// Personality engine
pub struct Personality {
    config: PersonalityConfig,
    xp: u32,
    level: u32,
    handshakes: u32,
    pmkids: u32,
    encounters: HashMap<[u8; 6], u32>,
    last_handshake: Option<DateTime<Utc>>,
    last_reboot: Option<DateTime<Utc>>,
}

impl Personality {
    pub fn new(config: PersonalityConfig) -> Self {
        Self {
            config,
            xp: 0,
            level: 0,
            handshakes: 0,
            pmkids: 0,
            encounters: HashMap::new(),
            last_handshake: None,
            last_reboot: None,
        }
    }

    pub fn config(&self) -> &PersonalityConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut PersonalityConfig {
        &mut self.config
    }

    /// Restore previously persisted progress (xp/level/handshake+pmkid
    /// counts/per-AP bond encounters) onto a freshly constructed
    /// `Personality`, so a device's progression survives a reboot instead
    /// of resetting to zero every power cycle. See `agent::recovery`.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        &mut self,
        xp: u32,
        level: u32,
        handshakes: u32,
        pmkids: u32,
        encounters: HashMap<[u8; 6], u32>,
    ) {
        self.xp = xp;
        self.level = level;
        self.handshakes = handshakes;
        self.pmkids = pmkids;
        self.encounters = encounters;
    }

    /// Update on handshake captured
    pub fn update_on_handshake(&mut self, ap_bssid: [u8; 6]) {
        self.handshakes += 1;
        self.xp += self.config.reward_handshake as u32;
        self.last_handshake = Some(Utc::now());
        *self.encounters.entry(ap_bssid).or_insert(0) += 1;
        self.check_level_up();
    }

    /// Number of times a handshake was captured from a given AP.
    pub fn encounters_for(&self, ap_bssid: &[u8; 6]) -> u32 {
        self.encounters.get(ap_bssid).copied().unwrap_or(0)
    }

    /// All per-AP bond encounter counts, for persistence (see
    /// `agent::recovery`).
    pub fn encounters(&self) -> &HashMap<[u8; 6], u32> {
        &self.encounters
    }

    /// Update on new AP seen
    pub fn update_on_new_ap(&mut self) {
        self.xp += self.config.reward_new_ap as u32;
        self.check_level_up();
    }

    /// Update on association
    pub fn update_on_association(&mut self) {
        self.xp += self.config.reward_association as u32;
        self.check_level_up();
    }

    /// Update on missed opportunity
    pub fn update_on_missed(&mut self) {
        self.xp = self.xp.saturating_sub((-self.config.penalty_missed) as u32);
    }

    /// Update on reboot
    pub fn update_on_reboot(&mut self) {
        self.last_reboot = Some(Utc::now());
        self.xp = self.xp.saturating_sub((-self.config.penalty_reboot) as u32);
    }

    /// Check and update level
    fn check_level_up(&mut self) {
        // Simple XP curve: level = xp / 1000
        let new_level = self.xp / 1000;
        if new_level > self.level {
            self.level = new_level;
        }
    }

    /// Compute mood from epoch state, matching real pwnagotchi's precedence.
    /// Severity ranking for the negative-mood cascade (higher = worse), used to
    /// keep the worse of the blind-epoch and missed-interaction signals.
    fn mood_severity(m: Mood) -> u8 {
        match m {
            Mood::Angry => 4,
            Mood::Lonely => 3,
            Mood::Sad => 2,
            Mood::Bored => 1,
            _ => 0,
        }
    }

    pub fn compute_mood(&self, epoch: &EpochState, peers: &[Peer]) -> Mood {
        // Handshakes captured this epoch trump everything.
        if epoch.handshakes_this_epoch > 0 {
            if epoch.handshakes_this_epoch > 1 {
                return Mood::Excited;
            }
            if self.handshakes == epoch.handshakes_this_epoch {
                return Mood::Grateful; // First ever handshake
            }
            return Mood::Happy;
        }

        // Blind-epoch negative-mood cascade, checked worst-first *by
        // threshold value* so every band is actually reachable. Real
        // pwnagotchi's order is angry > lonely > sad > bored, matching
        // `lonely_num_epochs` (150) sitting between `sad_num_epochs` (100)
        // and `angry_num_epochs` (200). The old code checked sad *before*
        // lonely, so with those defaults `Lonely` could never fire.
        let negative = if epoch.blind_epochs >= self.config.angry_num_epochs {
            Some(Mood::Angry)
        } else if epoch.blind_epochs >= self.config.lonely_num_epochs {
            Some(Mood::Lonely)
        } else if epoch.blind_epochs >= self.config.sad_num_epochs {
            Some(Mood::Sad)
        } else if epoch.blind_epochs >= self.config.bored_num_epochs {
            Some(Mood::Bored)
        } else {
            None
        };

        // Missed-interaction backoff (pwnagotchi's `is_stale` -> lonely/angry):
        // seeing APs but repeatedly failing to engage/capture makes the agent
        // lonely once `num_missed` crosses `max_misses_for_recon`, and angry at
        // twice that. This is independent of the blind-epoch cascade above
        // (which covers "no APs at all"); we keep whichever signal is worse.
        let missed = if self.config.max_misses_for_recon > 0 {
            if epoch.num_missed >= self.config.max_misses_for_recon.saturating_mul(2) {
                Some(Mood::Angry)
            } else if epoch.num_missed >= self.config.max_misses_for_recon {
                Some(Mood::Lonely)
            } else {
                None
            }
        } else {
            None
        };
        let negative = match (negative, missed) {
            (Some(a), Some(b)) => Some(if Self::mood_severity(a) >= Self::mood_severity(b) {
                a
            } else {
                b
            }),
            (a, b) => a.or(b),
        };

        if let Some(neg) = negative {
            // Peer-bond override: a unit with a support network nearby is
            // grateful *instead of* the negative mood it would otherwise
            // show (real pwnagotchi's `_has_support_network_for`). This
            // replaces the old bug where any peer short-circuited straight
            // to `Motivated` *before* the negative cascade even ran,
            // hiding Bored/Sad/Lonely/Angry entirely whenever a peer was
            // present.
            if !peers.is_empty() {
                return Mood::Grateful;
            }
            return neg;
        }

        // Not in a negative state: peers nearby are motivating; otherwise
        // fall back to a mode-appropriate look.
        if !peers.is_empty() {
            return Mood::Motivated;
        }

        match epoch.mode {
            // Real pwnagotchi's view.py alternates LOOK_R/LOOK_L (its
            // `wait()` step toggle) while idle/exploring rather than
            // freezing on one -- this previously always returned a fixed
            // `Mood::LookR` for every Recon epoch, so the face never
            // varied for as long as the agent stayed in Recon (the common
            // case: no handshakes yet, blind_epochs below the Bored
            // threshold, no peers). Confirmed on real hardware: face
            // stuck on "( ⚆_⚆)" the entire session despite 14 real APs
            // and active channel-hopping. Alternate by epoch parity so
            // consecutive epochs actually differ.
            AgentMode::Recon => {
                if epoch.epoch.is_multiple_of(2) {
                    Mood::LookR
                } else {
                    Mood::LookL
                }
            }
            AgentMode::Attack => Mood::Intense,
            AgentMode::Hop => Mood::LookL,
            AgentMode::Sleep => Mood::Sleep,
        }
    }

    /// Calculate recon time for current epoch
    pub fn calc_recon_time(&self, epoch: &EpochState) -> u64 {
        let base = self.config.min_recon_time;
        let max = self.config.max_recon_time;
        let ap_bonus = (epoch.aps_found as u64 * 2).min(10);
        (base + ap_bonus).clamp(base, max)
    }

    /// Calculate hop time for current epoch
    pub fn calc_hop_time(&self, epoch: &EpochState) -> u64 {
        let base = self.config.hop_recon_time;

        if epoch.aps_found == 0 {
            return base / 2;
        }

        let elapsed = epoch.duration().num_seconds() as u64;
        if elapsed >= base {
            return 0;
        }

        base - elapsed
    }


    /// Stats for display
    pub fn stats(&self) -> PersonalityStats {
        PersonalityStats {
            level: self.level,
            xp: self.xp,
            handshakes: self.handshakes,
            pmkids: self.pmkids,
        }
    }
}

/// Personality stats for display
#[derive(Debug, Clone, serde::Serialize)]
pub struct PersonalityStats {
    pub level: u32,
    pub xp: u32,
    pub handshakes: u32,
    pub pmkids: u32,
}

impl Default for Personality {
    fn default() -> Self {
        Self::new(PersonalityConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwncore::Channel;

    #[test]
    fn test_personality_new() {
        let p = Personality::default();
        assert_eq!(p.level, 0);
        assert_eq!(p.xp, 0);
    }

    #[test]
    fn test_restore_applies_persisted_progress() {
        let mut p = Personality::default();
        let mut encounters = HashMap::new();
        encounters.insert([1, 2, 3, 4, 5, 6], 3u32);
        p.restore(2500, 2, 7, 1, encounters.clone());
        assert_eq!(p.xp, 2500);
        assert_eq!(p.level, 2);
        assert_eq!(p.handshakes, 7);
        assert_eq!(p.pmkids, 1);
        assert_eq!(p.encounters_for(&[1, 2, 3, 4, 5, 6]), 3);
    }

    #[test]
    fn test_handshake_xp() {
        let mut p = Personality::default();
        p.update_on_handshake([0; 6]);
        assert_eq!(p.handshakes, 1);
        assert_eq!(p.xp, 100);
    }

    #[test]
    fn test_level_up() {
        let mut p = Personality::default();
        // 10 handshakes = 1000 xp = level 1
        for _ in 0..10 {
            p.update_on_handshake([0; 6]);
        }
        assert_eq!(p.level, 1);
    }

    #[test]
    fn test_mood_computation() {
        let p = Personality::default();

        // No APs, no peers, Recon mode -> alternates LookR/LookL by epoch
        // parity (matching real pwnagotchi's look-around idle animation,
        // rather than freezing on a single fixed face for the whole
        // session).
        let even_epoch = EpochState::new(0, Channel::new(1).unwrap());
        assert_eq!(p.compute_mood(&even_epoch, &[]), Mood::LookR);

        let odd_epoch = EpochState::new(1, Channel::new(1).unwrap());
        assert_eq!(p.compute_mood(&odd_epoch, &[]), Mood::LookL);
    }

    #[test]
    fn test_calc_recon_time() {
        let p = Personality::default();
        let mut epoch = EpochState::new(1, Channel::new(1).unwrap());
        epoch.aps_found = 5;

        let time = p.calc_recon_time(&epoch);
        assert!(time >= 5);
        assert!(time <= 30);
    }

    #[test]
    fn test_missed_interactions_trigger_lonely_then_angry() {
        // A-2 recon backoff (pwnagotchi's `max_misses_for_recon`, default 5):
        // seeing APs but repeatedly failing to engage -> lonely, then angry at
        // 2x. Independent of the blind-epoch (no-AP) cascade.
        let p = Personality::default();
        let mut epoch = EpochState::new(1, Channel::new(1).unwrap());

        epoch.num_missed = 4; // below threshold -> still an idle recon look
        assert!(matches!(
            p.compute_mood(&epoch, &[]),
            Mood::LookR | Mood::LookL
        ));
        epoch.num_missed = 5; // == threshold -> lonely
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Lonely);
        epoch.num_missed = 10; // 2x threshold -> angry
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Angry);
    }

    #[test]
    fn test_blind_cascade_lonely_is_reachable() {
        // AC5 regression: with pwnagotchi-matching defaults bored=15 < sad=25 <
        // lonely=150 < angry=200, each band must be reachable. The old
        // worst-first order checked sad before lonely, so Lonely never fired.
        let p = Personality::default();
        let mut epoch = EpochState::new(1, Channel::new(1).unwrap());

        epoch.blind_epochs = 18; // [15, 25) -> Bored
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Bored);
        epoch.blind_epochs = 60; // [25, 150) -> Sad
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Sad);
        epoch.blind_epochs = 160; // [150, 200) -> Lonely
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Lonely);
        epoch.blind_epochs = 220; // [200, inf) -> Angry
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::Angry);
    }

    #[test]
    fn test_peers_do_not_short_circuit_negative_moods() {
        // AC5 regression: a peer being present must NOT hide the negative
        // cascade behind Motivated. Instead, the support network converts
        // the negative mood to Grateful (real pwnagotchi behavior).
        let p = Personality::default();
        let mut epoch = EpochState::new(1, Channel::new(1).unwrap());
        epoch.blind_epochs = 220; // would be Angry with no peers
        let peers = [pwncore::Peer::new(
            pwncore::MacAddr::default(),
            "buddy".to_string(),
            6,
            -60,
        )];
        assert_eq!(p.compute_mood(&epoch, &peers), Mood::Grateful);
    }

    #[test]
    fn test_peers_motivate_when_not_blind() {
        // Peers present and no negative state -> Motivated.
        let p = Personality::default();
        let epoch = EpochState::new(1, Channel::new(1).unwrap());
        let peers = [pwncore::Peer::new(
            pwncore::MacAddr::default(),
            "buddy".to_string(),
            6,
            -60,
        )];
        assert_eq!(p.compute_mood(&epoch, &peers), Mood::Motivated);
    }

    #[test]
    fn test_phrase_selection() {
        // Phrase pools now live on `Mood` itself (`voice_lines`/
        // `voice_line`), ported from real pwnagotchi's `voice.py`, and are
        // shared with the face-table precedent (`Mood::face_variants`) --
        // `Personality::get_phrase` (a fixed, non-upstream, emoji-decorated
        // 1:1 table) was removed.
        assert!(Mood::Sleep.voice_lines().contains(&"Zzz"));
        for _ in 0..20 {
            assert!(Mood::Happy.voice_lines().contains(&Mood::Happy.voice_line()));
        }
    }
}
