use pwncore::mood::Mood;

pub fn face_for_mood(mood: &Mood) -> &'static str {
    mood.random_face()
}

pub fn default_face() -> &'static str {
    Mood::Awake.random_face()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_face_for_happy() {
        let face = face_for_mood(&Mood::Happy);
        assert!(Mood::Happy.faces().contains(&face));
    }

    #[test]
    fn test_default_face() {
        let face = default_face();
        assert!(Mood::Awake.faces().contains(&face));
    }
}
