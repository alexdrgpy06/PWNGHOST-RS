//! Kaomoji faces for moods

use pwncore::Mood;

/// Get a random face for the given mood
pub fn face_for_mood(mood: Mood) -> &'static str {
    let faces: &[&'static str] = match mood {
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
        Mood::Demotivated => &["(≖__≖)", "(￣ヘ￣)", "(¬_¬)"],
        Mood::Smart => &["(✜‿‿✜)"],
        Mood::Lonely => &["(ب__ب)", "(｡•́︿•̀｡)", "(︶︹︺)"],
        Mood::Sad => &["(╥☁╥ )", "(╥﹏╥)", "(ಥ﹏ಥ)"],
        Mood::Angry => &["(-_-')", "(⇀__⇀)", "(`___´)"],
        Mood::Friend => &["(♥‿‿♥)", "(♡‿‿♡)", "(♥‿♥ )", "(♥ω♥ )"],
        Mood::Broken => &["(☓‿‿☓)"],
        Mood::Upload => &["(1__0)", "(1__1)", "(0__1)"],
    };

    let idx = rand::random::<usize>() % faces.len();
    faces[idx]
}

/// Get all faces for a mood
pub fn faces_for_mood(mood: Mood) -> &'static [&'static str] {
    match mood {
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
        Mood::Demotivated => &["(≖__≖)", "(￣ヘ￣)", "(¬_¬)"],
        Mood::Smart => &["(✜‿‿✜)"],
        Mood::Lonely => &["(ب__ب)", "(｡•́︿•̀｡)", "(︶︹︺)"],
        Mood::Sad => &["(╥☁╥ )", "(╥﹏╥)", "(ಥ﹏ಥ)"],
        Mood::Angry => &["(-_-')", "(⇀__⇀)", "(`___´)"],
        Mood::Friend => &["(♥‿‿♥)", "(♡‿‿♡)", "(♥‿♥ )", "(♥ω♥ )"],
        Mood::Broken => &["(☓‿‿☓)"],
        Mood::Upload => &["(1__0)", "(1__1)", "(0__1)"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwncore::Mood;

    #[test]
    fn test_face_for_mood() {
        let face = face_for_mood(Mood::Happy);
        assert!(!face.is_empty());
    }

    #[test]
    fn test_faces_for_mood() {
        let faces = faces_for_mood(Mood::Sleep);
        assert!(!faces.is_empty());
        assert!(faces.len() >= 3);
    }
}
