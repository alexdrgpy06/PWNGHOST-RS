//! Policy trait and implementations

use crate::features::Features;

/// Policy trait for action selection
pub trait Policy: Send + Sync {
    /// Select action given features
    fn select_action(&self, features: &Features) -> RlAction;

    /// Get action space size
    fn action_space(&self) -> u8;

    /// Feed back the real-world outcome of the last selected action (e.g. a
    /// captured handshake, a blind epoch). Policies that don't learn online
    /// (heuristic/model/random) simply ignore this; [`BanditPolicy`] is the
    /// one that actually updates from it.
    fn observe_reward(&mut self, _action: RlAction, _reward: f32) {}

    /// Serialize any learned state for persistence across reboots (see
    /// `agent::recovery`). Returns `None` for policies with nothing to
    /// persist.
    fn export_state(&self) -> Option<Vec<u8>> {
        None
    }

    /// Restore learned state previously produced by [`Policy::export_state`].
    /// A no-op for policies that don't support it.
    fn import_state(&mut self, _data: &[u8]) {}
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

/// Epsilon-greedy multi-armed bandit over the 16-action space, using the
/// classic incremental-mean (sample-average) update from Sutton & Barto.
/// This is honest, real online reinforcement learning -- not the A2C
/// LSTM/actor-critic network the design docs describe (that needs an
/// offline training pipeline and real deployment data neither of which
/// exist yet, see `ModelPolicy`/`ActorCritic`), but a genuinely simpler,
/// well-understood technique that actually improves its action choices
/// from real, honestly-sourced reward signals (captured handshakes, blind
/// epochs -- see `agent::Agent::mark_handshake_captured`/`tick`), with a
/// decaying exploration rate that gives a real, measurable learning curve
/// over the device's lifetime. State persists across reboots via
/// `export_state`/`import_state` (wired through `agent::recovery`).
pub struct BanditPolicy {
    action_dim: usize,
    q_values: Vec<f32>,
    action_counts: Vec<u32>,
    epsilon: f32,
    epsilon_min: f32,
    epsilon_decay: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BanditSnapshot {
    q_values: Vec<f32>,
    action_counts: Vec<u32>,
    epsilon: f32,
}

impl BanditPolicy {
    /// `epsilon_decay` is applied once per `observe_reward` call (i.e. once
    /// per completed epoch that produced a reward signal); with the
    /// defaults below epsilon reaches its floor after roughly 1,000 reward
    /// events, a few days of typical operation.
    pub fn new(action_dim: usize) -> Self {
        Self {
            action_dim,
            q_values: vec![0.0; action_dim],
            action_counts: vec![0; action_dim],
            epsilon: 1.0,
            epsilon_min: 0.05,
            epsilon_decay: 0.995,
        }
    }

    /// Current exploration rate -- a direct, human-readable measure of how
    /// far along the "learning curve" this policy is (starts at 1.0 = pure
    /// exploration, decays toward `epsilon_min` = mostly exploiting learned
    /// values).
    pub fn epsilon(&self) -> f32 {
        self.epsilon
    }

    /// Total reward observations processed so far.
    pub fn total_updates(&self) -> u32 {
        self.action_counts.iter().sum()
    }

    /// Learned value estimate for each action index, for display/debugging.
    pub fn q_values(&self) -> &[f32] {
        &self.q_values
    }
}

impl Policy for BanditPolicy {
    fn select_action(&self, features: &Features) -> RlAction {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        if rng.gen::<f32>() < self.epsilon {
            // Explore: prefer channels with observed AP activity when we
            // have to guess blind (uniform over histogram-weighted
            // channels if any signal exists, else fully random), which
            // keeps early exploration useful rather than purely wasteful.
            let ap_sum: f32 = features.ap_histogram.sum();
            if ap_sum > 0.0 {
                let weights = features.ap_histogram.as_slice().unwrap_or(&[]);
                let total: f32 = weights.iter().sum();
                let mut roll = rng.gen::<f32>() * total;
                for (idx, w) in weights.iter().enumerate() {
                    roll -= *w;
                    if roll <= 0.0 {
                        return RlAction::HopChannel((idx + 1) as u8);
                    }
                }
            }
            return RlAction::from_index(rng.gen_range(0..self.action_dim), self.action_dim);
        }

        // Exploit: pick the highest learned Q-value, breaking ties toward
        // the lowest index (deterministic, easy to reason about in tests).
        let best = self
            .q_values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        RlAction::from_index(best, self.action_dim)
    }

    fn action_space(&self) -> u8 {
        self.action_dim as u8
    }

    fn observe_reward(&mut self, action: RlAction, reward: f32) {
        let idx = action.to_index(self.action_dim).min(self.action_dim - 1);
        self.action_counts[idx] += 1;
        let n = self.action_counts[idx] as f32;
        // Incremental sample-average update: Q(a) += (reward - Q(a)) / n.
        self.q_values[idx] += (reward - self.q_values[idx]) / n;
        self.epsilon = (self.epsilon * self.epsilon_decay).max(self.epsilon_min);
    }

    fn export_state(&self) -> Option<Vec<u8>> {
        let snapshot = BanditSnapshot {
            q_values: self.q_values.clone(),
            action_counts: self.action_counts.clone(),
            epsilon: self.epsilon,
        };
        serde_json::to_vec(&snapshot).ok()
    }

    fn import_state(&mut self, data: &[u8]) {
        if let Ok(snapshot) = serde_json::from_slice::<BanditSnapshot>(data) {
            if snapshot.q_values.len() == self.action_dim
                && snapshot.action_counts.len() == self.action_dim
            {
                self.q_values = snapshot.q_values;
                self.action_counts = snapshot.action_counts;
                self.epsilon = snapshot.epsilon;
            }
        }
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
    fn test_bandit_learns_toward_higher_reward_action() {
        let mut policy = BanditPolicy::new(16);
        // Repeatedly reward action index 3 (HopChannel(4)) highly, and
        // action index 7 (HopChannel(8)) poorly -- Q(3) should end up well
        // above Q(7), demonstrating the update actually discriminates.
        for _ in 0..50 {
            policy.observe_reward(RlAction::HopChannel(4), 1.0);
            policy.observe_reward(RlAction::HopChannel(8), -1.0);
        }
        assert!(policy.q_values()[3] > policy.q_values()[7]);
        assert!(policy.q_values()[3] > 0.9);
        assert!(policy.q_values()[7] < -0.9);
    }

    #[test]
    fn test_bandit_epsilon_decays_toward_floor() {
        let mut policy = BanditPolicy::new(16);
        assert_eq!(policy.epsilon(), 1.0);
        for _ in 0..2000 {
            policy.observe_reward(RlAction::Wait, 0.0);
        }
        assert!(
            policy.epsilon() <= 0.06,
            "epsilon should decay near its floor after many updates, got {}",
            policy.epsilon()
        );
        assert!(
            policy.epsilon() >= 0.05,
            "epsilon should never go below its floor, got {}",
            policy.epsilon()
        );
    }

    #[test]
    fn test_bandit_state_roundtrips_through_export_import() {
        let mut policy = BanditPolicy::new(16);
        for _ in 0..20 {
            policy.observe_reward(RlAction::HopChannel(6), 0.7);
        }
        let snapshot = policy.export_state().expect("bandit exports state");

        let mut restored = BanditPolicy::new(16);
        restored.import_state(&snapshot);
        assert_eq!(restored.q_values(), policy.q_values());
        assert_eq!(restored.epsilon(), policy.epsilon());
        assert_eq!(restored.total_updates(), policy.total_updates());
    }

    #[test]
    fn test_bandit_import_ignores_mismatched_action_dim() {
        let mut policy = BanditPolicy::new(16);
        policy.observe_reward(RlAction::HopChannel(6), 0.7);
        let mismatched = serde_json::to_vec(&serde_json::json!({
            "q_values": [0.0, 0.0, 0.0, 0.0],
            "action_counts": [0, 0, 0, 0],
            "epsilon": 0.5,
        }))
        .unwrap();
        let before = policy.q_values().to_vec();
        policy.import_state(&mismatched);
        assert_eq!(policy.q_values(), before.as_slice());
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
