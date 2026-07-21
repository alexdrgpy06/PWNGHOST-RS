//! Kaomoji faces for moods.
//!
//! Thin convenience wrapper over the canonical face table, which lives in
//! `pwncore::Mood::face()` (the single source of truth, verified line-for-
//! line against real jayofelony/pwnagotchi's `pwnagotchi/ui/faces.py`).
//! This module used to carry its own hardcoded copy; it now delegates so
//! there is exactly one face table in the whole workspace.
use pwncore::Mood;

/// Get the face for the given mood. Delegates to [`Mood::face`].
pub fn face_for_mood(mood: Mood) -> &'static str {
    mood.face()
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
