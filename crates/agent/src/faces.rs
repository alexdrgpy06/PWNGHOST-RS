//! Kaomoji faces for moods.
//!
//! Exactly matches real jayofelony/pwnagotchi's `pwnagotchi/ui/faces.py`
//! (fetched from the live `noai` branch) -- one canonical face per mood,
//! not randomized. Real pwnagotchi's `View._get_random_face` only
//! actually randomizes when a *list* of faces is passed to `set('face',
//! ...)`, and grepping `view.py`'s own state-setting calls (`on_bored`,
//! `on_sad`, `on_angry`, `on_motivated`, etc.) shows every one of them
//! passes a single face constant, never a list -- the per-mood "extra
//! variety" this module used to invent (e.g. a second/third alternate
//! face per mood) doesn't exist upstream at all, and some of the
//! fabricated alternates even reused a *different* mood's real face
//! (e.g. the old `Happy` alt `"(^‿‿^)"` is actually real pwnagotchi's
//! `Grateful` face). The one genuine randomization upstream --
//! `on_new_peer` picking among a small set of faces for a first/known/
//! good encounter -- is a peer-greeting special case, not a general
//! per-mood behavior, and isn't part of this table.
use pwncore::Mood;

/// Get the face for the given mood.
pub fn face_for_mood(mood: Mood) -> &'static str {
    match mood {
        Mood::LookR => "( ⚆_⚆)",
        Mood::LookL => "(☉_☉ )",
        Mood::LookRHappy => "( ◕‿◕)",
        Mood::LookLHappy => "(◕‿◕ )",
        Mood::Sleep => "(⇀‿‿↼)",
        Mood::Awake => "(◕‿‿◕)",
        Mood::Bored => "(-__-)",
        Mood::Intense => "(°▃▃°)",
        Mood::Cool => "(⌐■_■)",
        Mood::Happy => "(•‿‿•)",
        Mood::Excited => "(ᵔ◡◡ᵔ)",
        Mood::Grateful => "(^‿‿^)",
        Mood::Motivated => "(☼‿‿☼)",
        Mood::Demotivated => "(≖__≖)",
        Mood::Smart => "(✜‿‿✜)",
        Mood::Lonely => "(ب__ب)",
        Mood::Sad => "(╥☁╥ )",
        Mood::Angry => "(-_-')",
        Mood::Friend => "(♥‿‿♥)",
        Mood::Broken => "(☓‿‿☓)",
        Mood::Upload => "(1__0)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwncore::Mood;

    #[test]
    fn test_face_for_mood() {
        assert_eq!(face_for_mood(Mood::Happy), "(•‿‿•)");
        assert_eq!(face_for_mood(Mood::Sleep), "(⇀‿‿↼)");
        assert_eq!(face_for_mood(Mood::Angry), "(-_-')");
    }

    #[test]
    fn test_face_for_mood_is_deterministic() {
        // Real pwnagotchi's per-mood faces are single constants, not a
        // randomized choice -- calling twice for the same mood must
        // yield the exact same string every time.
        for _ in 0..20 {
            assert_eq!(face_for_mood(Mood::Lonely), "(ب__ب)");
        }
    }
}
