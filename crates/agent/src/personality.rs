use pwncore::personality::{Personality, PersonalityProfile};

pub fn apply_personality_profile(profile: PersonalityProfile, personality: &mut Personality) {
    profile.apply(personality);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_aggressive() {
        let mut p = Personality::default();
        apply_personality_profile(PersonalityProfile::Aggressive, &mut p);
        assert_eq!(p.recon_time, 15);
    }
}
