//! Kaomoji faces for moods.
//!
//! Thin convenience wrapper over the canonical face table, which lives in
//! `pwncore::Mood::face()` (the single source of truth, verified against a
//! real device's `default.toml` -- randomly chosen among each mood's real
//! variants, matching upstream `view.py`'s `_get_random_face`). This module
//! used to carry its own hardcoded copy; it now delegates so there is
//! exactly one face table in the whole workspace.
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
        // face() picks randomly among a mood's real variants -- check
        // membership, not a single fixed value.
        assert!(Mood::Happy.face_variants().contains(&face_for_mood(Mood::Happy)));
        assert!(Mood::Sleep.face_variants().contains(&face_for_mood(Mood::Sleep)));
        assert!(Mood::Angry.face_variants().contains(&face_for_mood(Mood::Angry)));
    }

    #[test]
    fn test_face_for_mood_single_variant_is_stable() {
        // Moods with exactly one real variant should always return it --
        // multi-variant moods (like Lonely) are expected to vary, matching
        // real pwnagotchi's own `random.choice` behavior in view.py.
        for _ in 0..20 {
            assert_eq!(face_for_mood(Mood::Grateful), "(^‿‿^)");
        }
    }
}
