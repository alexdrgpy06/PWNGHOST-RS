//! Personality configuration and behavior

use crate::{Channel, EncryptionType, Mood};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::MacAddr;

/// Personality configuration (matches pwnagotchi personality.toml)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Personality {
    // Mood thresholds (epochs)
    pub bored_num_epochs: u64,
    pub sad_num_epochs: u64,
    pub angry_num_epochs: u64,
    pub lonely_num_epochs: u64,

    // Activity factors
    pub bond_encounters_factor: f32,
    pub max_interactions: u32,
    pub throttle: u32,

    // Rewards/penalties
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

    // Face images (for PNG mode)
    pub faces: FaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl Default for Personality {
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
            sleep: vec!["(⇀‿‿↼)".to_string(), "(≖‿‿≖)".to_string(), "(－_－)".to_string()],
            awake: vec!["(◕‿‿◕)".to_string()],
            bored: vec!["(-__-)".to_string(), "(—__—)".to_string()],
            intense: vec!["(°▃▃°)".to_string(), "(°ロ°)".to_string()],
            cool: vec!["(⌐■_■)".to_string(), "(单__单)".to_string()],
            happy: vec!["(•‿‿•)".to_string(), "(^‿‿^)".to_string(), "(^◡◡^)".to_string()],
            excited: vec!["(ᵔ◡◡ᵔ)".to_string(), "(✜‿‿✜)".to_string()],
            grateful: vec!["(^‿‿^)".to_string()],
            motivated: vec!["(☼‿‿☼)".to_string(), "(★‿★)".to_string(), "(•̀ᴗ•́)".to_string()],
            demotivated: vec!["(≖__≖)".to_string(), "(￣ヘ￣)".to_string(), "(¬､¬)".to_string()],
            smart: vec!["(✜‿‿✜)".to_string()],
            lonely: vec!["(ب__ب)".to_string(), "(｡•́︿•̀｡)".to_string(), "(︶︹︺)".to_string()],
            sad: vec!["(╥☁╥ )".to_string(), "(╥﹏╥)".to_string(), "(ಥ﹏ಥ)".to_string()],
            angry: vec!["(-_-')".to_string(), "(⇀__⇀)".to_string(), "(`___´)".to_string()],
            friend: vec!["(♥‿‿♥)".to_string(), "(♡‿‿♡)".to_string(), "(♥‿♥ )".to_string(), "(♥ω♥ )".to_string()],
            broken: vec!["(☓‿‿☓)".to_string()],
            upload: vec!["(1__0)".to_string(), "(1__1)".to_string(), "(0__1)".to_string()],
            png: false,
        }
    }
}

impl Personality {
    /// Calculate recon time based on current epoch state
    pub fn calc_recon_time(&self, epoch: &crate::Epoch) -> u64 {
        let base = self.min_recon_time;
        let max = self.max_recon_time;

        // Increase recon time if we're finding APs
        let ap_bonus = (epoch.aps_found as u64 * 2).min(10);
        let time = base + ap_bonus;

        time.clamp(base, max)
    }

    /// Calculate hop time based on epoch state
    pub fn calc_hop_time(&self, epoch: &crate::Epoch) -> u64 {
        let base = self.hop_recon_time;

        // Hop sooner if no APs found (blind epochs)
        if epoch.aps_found == 0 {
            return base / 2;
        }

        // Hop sooner if we've been on this channel long
        let elapsed = epoch.duration().as_secs();
        if elapsed >= base {
            return 0; // Immediate hop
        }

        base - elapsed
    }

    /// Compute mood from epoch stats (simplified - actual uses personality.toml params)
    pub fn compute_mood(&self, epoch: &crate::Epoch, peers: &[crate::Peer], total_handshakes: u32) -> Mood {
        // If we captured handshakes this epoch
        if epoch.handshakes_this_epoch > 0 {
            if total_handshakes == epoch.handshakes_this_epoch {
                return Mood::Grateful; // First ever
            }
            if epoch.handshakes_this_epoch > 1 {
                return Mood::Excited;
            }
            return Mood::Happy;
        }

        // If we have active peers
        if !peers.is_empty() {
            return Mood::Motivated;
        }

        // Check blind epochs (no APs seen)
        if epoch.aps_found == 0 {
            // This would be tracked across epochs
            // Simplified: check current epoch
            return Mood::LookR; // Default looking
        }

        // Based on attack activity
        if epoch.deauths_sent > 0 || epoch.assoc_attempts > 0 {
            return Mood::Intense;
        }

        // Default based on agent mode
        match epoch.mode {
            crate::AgentMode::Recon => Mood::LookR,
            crate::AgentMode::Attack => Mood::Intense,
            crate::AgentMode::Hop => Mood::LookL,
            crate::AgentMode::Sleep => Mood::Sleep,
        }
    }

    /// Get face for mood
    pub fn get_face(&self, mood: Mood) -> String {
        let faces = match mood {
            Mood::LookR => &self.faces.look_r,
            Mood::LookL => &self.faces.look_l,
            Mood::LookRHappy => &self.faces.look_r_happy,
            Mood::LookLHappy => &self.faces.look_l_happy,
            Mood::Sleep => &self.faces.sleep,
            Mood::Awake => &self.faces.awake,
            Mood::Bored => &self.faces.bored,
            Mood::Intense => &self.faces.intense,
            Mood::Cool => &self.faces.cool,
            Mood::Happy => &self.faces.happy,
            Mood::Excited => &self.faces.excited,
            Mood::Grateful => &self.faces.grateful,
            Mood::Motivated => &self.faces.motivated,
            Mood::Demotivated => &self.faces.demotivated,
            Mood::Smart => &self.faces.smart,
            Mood::Lonely => &self.faces.lonely,
            Mood::Sad => &self.faces.sad,
            Mood::Angry => &self.faces.angry,
            Mood::Friend => &self.faces.friend,
            Mood::Broken => &self.faces.broken,
            Mood::Upload => &self.faces.upload,
        };

        if faces.is_empty() {
            mood.random_face().to_string()
        } else {
            let idx = rand::random::<usize>() % faces.len();
            faces[idx].clone()
        }
    }

    /// Get a motivational phrase for current mood
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
}

/// Personality statistics for display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersonalityStats {
    pub level: u32,
    pub xp: u32,
    pub handshakes: u32,
    pub pmkids: u32,
    pub epochs: u64,
    pub uptime_secs: u64,
    pub mood: Mood,
    pub best_channel: Option<u8>,
    pub peers_met: u32,
}

impl Default for PersonalityStats {
    fn default() -> Self {
        Self {
            level: 0,
            xp: 0,
            handshakes: 0,
            pmkids: 0,
            epochs: 0,
            uptime_secs: 0,
            mood: Mood::Awake,
            best_channel: None,
            peers_met: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personality_default() {
        let p = Personality::default();
        assert_eq!(p.bored_num_epochs, 50);
        assert_eq!(p.reward_handshake, 100);
        assert!(p.deauth == false);
    }

    #[test]
    fn test_calc_recon_time() {
        let p = Personality::default();
        let mut epoch = crate::Epoch::new(1, Channel::new(1).unwrap());
        epoch.aps_found = 5;

        let time = p.calc_recon_time(&epoch);
        assert!(time >= p.min_recon_time);
        assert!(time <= p.max_recon_time);
    }

    #[test]
    fn test_calc_hop_time() {
        let p = Personality::default();
        let mut epoch = crate::Epoch::new(1, Channel::new(1).unwrap());
        epoch.aps_found = 0; // Blind epoch

        let time = p.calc_hop_time(&epoch);
        assert_eq!(time, p.hop_recon_time / 2);
    }

    #[test]
    fn test_get_face() {
        let p = Personality::default();
        let face = p.get_face(Mood::Happy);
        assert!(!face.is_empty());
        assert!(face.contains("•") || face.contains("^"));
    }

    #[test]
    fn test_get_phrase() {
        let p = Personality::default();
        assert!(p.get_phrase(Mood::Happy).contains("Got one"));
        assert!(p.get_phrase(Mood::Sleep).contains("Zzz"));
        assert!(p.get_phrase(Mood::Intense).contains("ATTACKING"));
    }
}