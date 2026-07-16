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

    // Attack settings
    pub deauth: bool,
    pub associate: bool,
    pub min_rssi: i16,

    // Display
    pub position_x: i32,
    pub position_y: i32,
    pub frame_padding: bool,
    pub frame_padding_min_bytes: usize,

    // Faces
    pub faces: FaceConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FaceConfig {
    pub look_r: Vec<String>,
    pub look_l: Vec<String>,
    pub look_r_happy: Vec<String>,
    pub look_l_happy: Vec<String>,
    pub sleep: Vec<String>,
    pub awake: Vec<String>,
    pub bored: Vec<String>,
    pub intense: Vec<String>,
    pub cool: Vec<String>,
    pub happy: Vec<String>,
    pub excited: Vec<String>,
    pub grateful: Vec<String>,
    pub motivated: Vec<String>,
    pub demotivated: Vec<String>,
    pub smart: Vec<String>,
    pub lonely: Vec<String>,
    pub sad: Vec<String>,
    pub angry: Vec<String>,
    pub friend: Vec<String>,
    pub broken: Vec<String>,
    pub upload: Vec<String>,
    pub png: bool,
}

impl From<config::PersonalityConfig> for PersonalityConfig {
    fn from(c: config::PersonalityConfig) -> Self {
        let f = c.faces;
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
            deauth: c.deauth,
            associate: c.associate,
            min_rssi: c.min_rssi,
            position_x: c.position_x,
            position_y: c.position_y,
            frame_padding: c.frame_padding,
            frame_padding_min_bytes: c.frame_padding_min_bytes,
            faces: FaceConfig {
                look_r: f.look_r,
                look_l: f.look_l,
                look_r_happy: f.look_r_happy,
                look_l_happy: f.look_l_happy,
                sleep: f.sleep,
                awake: f.awake,
                bored: f.bored,
                intense: f.intense,
                cool: f.cool,
                happy: f.happy,
                excited: f.excited,
                grateful: f.grateful,
                motivated: f.motivated,
                demotivated: f.demotivated,
                smart: f.smart,
                lonely: f.lonely,
                sad: f.sad,
                angry: f.angry,
                friend: f.friend,
                broken: f.broken,
                upload: f.upload,
                png: f.png,
            },
        }
    }
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
            position_x: 0,
            position_y: 34,
            frame_padding: true,
            frame_padding_min_bytes: 650,
            faces: FaceConfig::default(),
        }
    }
}

impl Default for FaceConfig {
    fn default() -> Self {
        Self {
            look_r: vec!["( ⚆_⚆)".to_string()],
            look_l: vec!["(☉_☉ )".to_string()],
            look_r_happy: vec!["( ◕‿◕)".to_string(), "( ≧◡≦)".to_string()],
            look_l_happy: vec!["(◕‿◕ )".to_string(), "(≧◡≦ )".to_string()],
            sleep: vec![
                "(⇀‿‿↼)".to_string(),
                "(≖‿‿≖)".to_string(),
                "(－_－)".to_string(),
            ],
            awake: vec!["(◕‿‿◕)".to_string()],
            bored: vec!["(-__-)".to_string(), "(—__—)".to_string()],
            intense: vec!["(°▃▃°)".to_string(), "(°ロ°)".to_string()],
            cool: vec!["(⌐■_■)".to_string(), "(单__单)".to_string()],
            happy: vec![
                "(•‿‿•)".to_string(),
                "(^‿‿^)".to_string(),
                "(^◡◡^)".to_string(),
            ],
            excited: vec!["(ᵔ◡◡ᵔ)".to_string(), "(✜‿‿✜)".to_string()],
            grateful: vec!["(^‿‿^)".to_string()],
            motivated: vec![
                "(☼‿‿☼)".to_string(),
                "(★‿★)".to_string(),
                "(•̀ᴗ•́)".to_string(),
            ],
            demotivated: vec![
                "(≖__≖)".to_string(),
                "(￣ヘ￣)".to_string(),
                "(¬_¬)".to_string(),
            ],
            smart: vec!["(✜‿‿✜)".to_string()],
            lonely: vec![
                "(ب__ب)".to_string(),
                "(｡•́︿•̀｡)".to_string(),
                "(︶︹︺)".to_string(),
            ],
            sad: vec![
                "(╥☁╥ )".to_string(),
                "(╥﹏╥)".to_string(),
                "(ಥ﹏ಥ)".to_string(),
            ],
            angry: vec![
                "(-_-')".to_string(),
                "(⇀__⇀)".to_string(),
                "(`___´)".to_string(),
            ],
            friend: vec![
                "(♥‿‿♥)".to_string(),
                "(♡‿‿♡)".to_string(),
                "(♥‿♥ )".to_string(),
                "(♥ω♥ )".to_string(),
            ],
            broken: vec!["(☓‿‿☓)".to_string()],
            upload: vec![
                "(1__0)".to_string(),
                "(1__1)".to_string(),
                "(0__1)".to_string(),
            ],
            png: false,
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

    /// Compute mood from epoch state
    pub fn compute_mood(&self, epoch: &EpochState, peers: &[Peer]) -> Mood {
        // If handshakes this epoch
        if epoch.handshakes_this_epoch > 0 {
            if epoch.handshakes_this_epoch > 1 {
                return Mood::Excited;
            }
            if self.handshakes == epoch.handshakes_this_epoch {
                return Mood::Grateful; // First ever handshake
            }
            return Mood::Happy;
        }

        // If peers nearby
        if !peers.is_empty() {
            return Mood::Motivated;
        }

        // Check blind epochs
        if epoch.blind_epochs > 0 {
            if epoch.blind_epochs >= self.config.angry_num_epochs {
                return Mood::Angry;
            }
            if epoch.blind_epochs >= self.config.sad_num_epochs {
                return Mood::Sad;
            }
            if epoch.blind_epochs >= self.config.lonely_num_epochs {
                return Mood::Lonely;
            }
            if epoch.blind_epochs >= self.config.bored_num_epochs {
                return Mood::Bored;
            }
        }

        // Based on mode
        match epoch.mode {
            AgentMode::Recon => Mood::LookR,
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

    /// Get face for mood
    pub fn get_face(&self, mood: Mood) -> String {
        let faces = match mood {
            Mood::LookR => &self.config.faces.look_r,
            Mood::LookL => &self.config.faces.look_l,
            Mood::LookRHappy => &self.config.faces.look_r_happy,
            Mood::LookLHappy => &self.config.faces.look_l_happy,
            Mood::Sleep => &self.config.faces.sleep,
            Mood::Awake => &self.config.faces.awake,
            Mood::Bored => &self.config.faces.bored,
            Mood::Intense => &self.config.faces.intense,
            Mood::Cool => &self.config.faces.cool,
            Mood::Happy => &self.config.faces.happy,
            Mood::Excited => &self.config.faces.excited,
            Mood::Grateful => &self.config.faces.grateful,
            Mood::Motivated => &self.config.faces.motivated,
            Mood::Demotivated => &self.config.faces.demotivated,
            Mood::Smart => &self.config.faces.smart,
            Mood::Lonely => &self.config.faces.lonely,
            Mood::Sad => &self.config.faces.sad,
            Mood::Angry => &self.config.faces.angry,
            Mood::Friend => &self.config.faces.friend,
            Mood::Broken => &self.config.faces.broken,
            Mood::Upload => &self.config.faces.upload,
        };

        if faces.is_empty() {
            return mood.random_face().to_string();
        }

        let idx = rand::random::<usize>() % faces.len();
        faces[idx].clone()
    }

    /// Get motivational phrase for mood
    pub fn get_phrase(&self, mood: Mood) -> String {
        match mood {
            Mood::Happy => "Got one! ✨".to_string(),
            Mood::Excited => "On a roll! 🚀".to_string(),
            Mood::Grateful => "Thanks, friend! 🤝".to_string(),
            Mood::Motivated => "Let's go! 💪".to_string(),
            Mood::Bored => "Nothing happening... 😴".to_string(),
            Mood::Lonely => "Anyone there? 👻".to_string(),
            Mood::Sad => "So quiet... 😢".to_string(),
            Mood::Angry => "This is frustrating! 😤".to_string(),
            Mood::Intense => "ATTACKING! ⚡".to_string(),
            Mood::Cool => "Deauthing like a boss 😎".to_string(),
            Mood::Sleep => "Zzz... 💤".to_string(),
            Mood::Awake => "Good morning! ☀️".to_string(),
            Mood::LookR | Mood::LookL => "Scanning... 👀".to_string(),
            Mood::LookRHappy | Mood::LookLHappy => "Looking good! ✨".to_string(),
            Mood::Friend => "Hey buddy! 👋".to_string(),
            Mood::Broken => "Oops! 💥".to_string(),
            Mood::Upload => "Uploading... 📤".to_string(),
            Mood::Smart => "Thinking... 🤔".to_string(),
            Mood::Demotivated => "Why bother... 😔".to_string(),
        }
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
        let epoch = EpochState::new(1, Channel::new(1).unwrap());

        // No APs, no peers -> LookR (Recon)
        assert_eq!(p.compute_mood(&epoch, &[]), Mood::LookR);
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
    fn test_face_selection() {
        let p = Personality::default();
        let face = p.get_face(Mood::Happy);
        assert!(!face.is_empty());
        assert!(face.contains("•") || face.contains("^"));
    }

    #[test]
    fn test_phrase_selection() {
        let p = Personality::default();
        assert!(p.get_phrase(Mood::Happy).contains("Got one"));
        assert!(p.get_phrase(Mood::Sleep).contains("Zzz"));
    }
}
