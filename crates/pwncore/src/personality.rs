//! Personality configuration (matches pwnagotchi defaults.toml)

use serde::{Deserialize, Serialize};

/// Personality configuration matching pwnagotchi's defaults.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Personality {
    pub advertise: bool,
    pub deauth: bool,
    pub associate: bool,
    pub channels: Vec<u8>,
    pub min_rssi: i16,
    pub ap_ttl: u32,
    pub sta_ttl: u32,
    pub recon_time: u32,
    pub max_inactive_scale: u32,
    pub recon_inactive_multiplier: u32,
    pub hop_recon_time: u32,
    pub min_recon_time: u32,
    pub max_interactions: u32,
    pub max_misses_for_recon: u32,
    pub excited_epochs: u32,
    pub bored_epochs: u32,
    pub sad_epochs: u32,
    pub bond_encounters_factor: u32,
    pub throttle_a: f32,
    pub throttle_d: f32,
}

impl Default for Personality {
    fn default() -> Self {
        Self {
            advertise: true,
            deauth: true,
            associate: true,
            channels: vec![],
            min_rssi: -200,
            ap_ttl: 120,
            sta_ttl: 300,
            recon_time: 30,
            max_inactive_scale: 2,
            recon_inactive_multiplier: 2,
            hop_recon_time: 10,
            min_recon_time: 5,
            max_interactions: 3,
            max_misses_for_recon: 5,
            excited_epochs: 10,
            bored_epochs: 15,
            sad_epochs: 25,
            bond_encounters_factor: 20000,
            throttle_a: 0.4,
            throttle_d: 0.9,
        }
    }
}

impl Personality {
    /// Calculate recon time based on inactivity
    pub fn calc_recon_time(&self, epoch: &crate::epoch::Epoch) -> u32 {
        let mut recon = self.recon_time;
        if epoch.inactive_epochs >= self.max_inactive_scale {
            recon *= self.recon_inactive_multiplier;
        }
        recon
    }

    /// Calculate hop recon time
    pub fn calc_hop_time(&self, epoch: &crate::epoch::Epoch) -> u32 {
        if epoch.did_deauth {
            self.hop_recon_time
        } else if epoch.did_associate {
            self.min_recon_time
        } else {
            0
        }
    }
}

/// Personality profiles for different behaviors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonalityProfile {
    Balanced,
    Aggressive,
    Stealth,
    PmkidOnly,
    HandshakeOnly,
    Passive,
}

impl PersonalityProfile {
    pub fn apply(&self, base: &mut Personality) {
        match self {
            PersonalityProfile::Aggressive => {
                base.recon_time = 15;
                base.hop_recon_time = 5;
                base.min_recon_time = 2;
                base.max_interactions = 5;
                base.throttle_a = 0.2;
                base.throttle_d = 0.5;
            }
            PersonalityProfile::Stealth => {
                base.deauth = false;
                base.associate = false;
                base.recon_time = 60;
                base.hop_recon_time = 30;
            }
            PersonalityProfile::PmkidOnly => {
                base.deauth = false;
                base.associate = true;
            }
            PersonalityProfile::HandshakeOnly => {
                base.deauth = true;
                base.associate = false;
            }
            PersonalityProfile::Passive => {
                base.deauth = false;
                base.associate = false;
            }
            PersonalityProfile::Balanced => {} // Use defaults
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_personality() {
        let p = Personality::default();
        assert_eq!(p.recon_time, 30);
        assert_eq!(p.max_interactions, 3);
        assert_eq!(p.bond_encounters_factor, 20000);
    }

    #[test]
    fn test_personality_profiles() {
        let mut p = Personality::default();
        PersonalityProfile::Aggressive.apply(&mut p);
        assert_eq!(p.recon_time, 15);
        assert!(p.deauth);
        
        PersonalityProfile::Stealth.apply(&mut p);
        assert!(!p.deauth);
    }
}