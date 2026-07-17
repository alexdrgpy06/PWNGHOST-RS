use serde::{Deserialize, Serialize};

use crate::personality::Personality;

/// Mood states (matching pwnagotchi classic kaomoji faces)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mood {
    LookRight,
    LookLeft,
    LookRightHappy,
    LookLeftHappy,
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
    /// All kaomoji faces for this mood
    pub fn faces(&self) -> &'static [&'static str] {
        match self {
            Mood::LookRight => &["( ⚆_⚆)"],
            Mood::LookLeft => &["(☉_☉ )"],
            Mood::LookRightHappy => &["( ◕‿◕)", "( ≧◡≦)"],
            Mood::LookLeftHappy => &["(◕‿◕ )", "(≧◡≦ )"],
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

    /// Get a random face for this mood
    pub fn random_face(&self) -> &'static str {
        let faces = self.faces();
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as usize)
            % faces.len();
        faces[idx]
    }

    /// Determine mood from epoch state and personality (pwnagotchi compatible)
    pub fn from_epoch(epoch: &crate::epoch::Epoch, personality: &Personality, _peers: &[crate::peer::Peer]) -> Self {
        let was_stale = epoch.inactive_epochs > 0; // Would need missed count
        let bond_factor = epoch.total_bond_factor / personality.bond_encounters_factor as f64;
        let has_support = bond_factor >= 1.0;
        
        // After X misses during an epoch, set status to lonely or angry
        if was_stale {
            if has_support {
                return Mood::Grateful;
            }
            return Mood::Lonely;
        }
        
        // After X times being bored, the status is set to sad or angry
        if epoch.sad_epochs > 0 {
            let factor = epoch.inactive_epochs as f64 / personality.sad_epochs as f64;
            if has_support && factor < 2.0 {
                return Mood::Grateful;
            }
            if factor >= 2.0 {
                return Mood::Angry;
            }
            return Mood::Sad;
        }
        
        // After X times being inactive, the status is set to bored
        if epoch.bored_epochs > 0 {
            let inactive_ratio = epoch.inactive_epochs as f64 / personality.bored_epochs as f64;
            if has_support && inactive_ratio < 2.0 {
                return Mood::Grateful;
            }
            return Mood::Bored;
        }
        
        // After X times being active, the status is set to happy / excited
        if epoch.active_epochs >= personality.excited_epochs {
            return Mood::Excited;
        }
        
        // After X times being active with support
        if epoch.active_epochs >= 5 && bond_factor >= 5.0 {
            return Mood::Grateful;
        }
        
        // Default active
        if epoch.any_activity {
            return Mood::Intense;
        }
        
        Mood::Awake
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mood_faces() {
        assert!(!Mood::Happy.faces().is_empty());
        assert!(!Mood::Sad.faces().is_empty());
        assert!(!Mood::Sleep.faces().is_empty());
    }

    #[test]
    fn test_random_face() {
        let face = Mood::Happy.random_face();
        assert!(Mood::Happy.faces().contains(&face));
    }

    #[test]
    fn test_personality_default() {
        let p = Personality::default();
        assert_eq!(p.recon_time, 30);
        assert_eq!(p.max_interactions, 3);
        assert_eq!(p.bond_encounters_factor, 20000);
    }
}