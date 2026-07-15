//! Classic pwnagotchi moods (21 moods)

use serde::{Deserialize, Serialize};

/// Classic pwnagotchi moods
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

    /// Get a descriptive name for the mood
    pub fn display_name(&self) -> &'static str {
        match self {
            Mood::LookR => "Looking Right",
            Mood::LookL => "Looking Left",
            Mood::LookRHappy => "Happy (Right)",
            Mood::LookLHappy => "Happy (Left)",
            Mood::Sleep => "Sleeping",
            Mood::Awake => "Awake",
            Mood::Bored => "Bored",
            Mood::Intense => "Intense",
            Mood::Cool => "Cool",
            Mood::Happy => "Happy",
            Mood::Excited => "Excited",
            Mood::Grateful => "Grateful",
            Mood::Motivated => "Motivated",
            Mood::Demotivated => "Demotivated",
            Mood::Smart => "Smart",
            Mood::Lonely => "Lonely",
            Mood::Sad => "Sad",
            Mood::Angry => "Angry",
            Mood::Friend => "Friend",
            Mood::Broken => "Broken",
            Mood::Upload => "Uploading",
        }
    }

    /// Check if this is a "positive" mood
    pub fn is_positive(&self) -> bool {
        matches!(
            self,
            Mood::Happy
                | Mood::Excited
                | Mood::Grateful
                | Mood::Motivated
                | Mood::LookRHappy
                | Mood::LookLHappy
                | Mood::Friend
                | Mood::Awake
        )
    }

    /// Check if this is a "negative" mood
    pub fn is_negative(&self) -> bool {
        matches!(
            self,
            Mood::Bored
                | Mood::Lonely
                | Mood::Sad
                | Mood::Angry
                | Mood::Demotivated
                | Mood::Broken
        )
    }

    /// Check if this is an "active" mood (attacking, interacting)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Mood::Intense
                | Mood::Cool
                | Mood::Happy
                | Mood::Excited
                | Mood::Motivated
                | Mood::Smart
        )
    }
}

impl Default for Mood {
    fn default() -> Self {
        Mood::LookR
    }
}

impl std::fmt::Display for Mood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mood_faces() {
        assert!(!Mood::Happy.faces().is_empty());
        assert!(Mood::Happy.random_face().contains("•"));
    }

    #[test]
    fn test_mood_properties() {
        assert!(Mood::Happy.is_positive());
        assert!(Mood::Sad.is_negative());
        assert!(Mood::Intense.is_active());
        assert!(!Mood::Sleep.is_active());
    }

    #[test]
    fn test_mood_display_name() {
        assert_eq!(Mood::Happy.display_name(), "Happy");
        assert_eq!(Mood::Sleep.display_name(), "Sleeping");
        assert_eq!(Mood::Friend.display_name(), "Friend");
    }

    #[test]
    fn test_mood_default() {
        let mood = Mood::default();
        assert_eq!(mood, Mood::LookR);
    }
}