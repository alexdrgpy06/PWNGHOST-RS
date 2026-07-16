//! Policy trait and implementations

use crate::features::Features;

/// Policy trait for action selection
pub trait Policy: Send + Sync {
    /// Select action given features
    fn select_action(&self, features: &Features) -> RlAction;

    /// Get action space size
    fn action_space(&self) -> u8;
}

/// RL actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RlAction {
    HopChannel(u8), // 1-13
    Deauth,         // Send deauth
    Associate,      // Send association
    Wait,           // Wait on current channel
    Sleep(u8),      // Sleep for N seconds
}

impl RlAction {
    pub fn from_index(idx: usize, _action_dim: usize) -> Self {
        match idx {
            0..=12 => RlAction::HopChannel((idx + 1) as u8),
            13 => RlAction::Deauth,
            14 => RlAction::Associate,
            15 => RlAction::Wait,
            _ => RlAction::Wait,
        }
    }

    pub fn to_index(&self, action_dim: usize) -> usize {
        match self {
            RlAction::HopChannel(ch) => (*ch as usize).min(13) - 1,
            RlAction::Deauth => 13,
            RlAction::Associate => 14,
            RlAction::Wait => 15,
            RlAction::Sleep(s) => 15 + (*s as usize).min(action_dim - 16),
        }
    }
}

/// Heuristic policy (fallback when no RL model)
pub struct HeuristicPolicy;

impl HeuristicPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HeuristicPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy for HeuristicPolicy {
    fn select_action(&self, features: &Features) -> RlAction {
        // Simple heuristic: if no APs seen, hop
        let ap_sum: f32 = features.ap_histogram.sum();
        if ap_sum < 0.1 {
            return RlAction::HopChannel(6); // Hop to channel 6
        }

        // If many APs, check for targets
        let sta_sum: f32 = features.sta_histogram.sum();
        if sta_sum > 0.5 {
            return RlAction::Deauth;
        }

        // Default: hop to non-overlapping channel
        RlAction::HopChannel(1)
    }

    fn action_space(&self) -> u8 {
        16
    }
}

/// Neural policy backed by a trained actor-critic network.
pub struct ModelPolicy {
    model: crate::model::ActorCritic,
}

impl ModelPolicy {
    pub fn new(model: crate::model::ActorCritic) -> Self {
        Self { model }
    }
}

impl Policy for ModelPolicy {
    fn select_action(&self, features: &Features) -> RlAction {
        match self.model.act(&features.as_flat()) {
            Ok((action, _value)) => action,
            Err(_) => RlAction::Wait,
        }
    }

    fn action_space(&self) -> u8 {
        self.model.config().action_dim as u8
    }
}

/// Random policy for exploration
pub struct RandomPolicy;

impl Policy for RandomPolicy {
    fn select_action(&self, _features: &Features) -> RlAction {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        match rng.gen_range(0..16) {
            0..=12 => RlAction::HopChannel(rng.gen_range(1..=13)),
            13 => RlAction::Deauth,
            14 => RlAction::Associate,
            _ => RlAction::Wait,
        }
    }

    fn action_space(&self) -> u8 {
        16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rl_action_from_index() {
        assert_eq!(RlAction::from_index(0, 16), RlAction::HopChannel(1));
        assert_eq!(RlAction::from_index(5, 16), RlAction::HopChannel(6));
        assert_eq!(RlAction::from_index(12, 16), RlAction::HopChannel(13));
        assert_eq!(RlAction::from_index(13, 16), RlAction::Deauth);
        assert_eq!(RlAction::from_index(14, 16), RlAction::Associate);
        assert_eq!(RlAction::from_index(15, 16), RlAction::Wait);
    }

    #[test]
    fn test_heuristic_policy() {
        let policy = HeuristicPolicy::new();
        let mut features = crate::features::Features::new();
        features.ap_histogram[5] = 1.0; // Channel 6

        let action = policy.select_action(&features);
        assert!(matches!(
            action,
            RlAction::HopChannel(_) | RlAction::Deauth | RlAction::Associate | RlAction::Wait
        ));
    }

    #[test]
    fn test_random_policy() {
        let policy = RandomPolicy;
        let features = crate::features::Features::new();
        let action = policy.select_action(&features);
        assert!(matches!(
            action,
            RlAction::HopChannel(_) | RlAction::Deauth | RlAction::Associate | RlAction::Wait
        ));
    }
}
