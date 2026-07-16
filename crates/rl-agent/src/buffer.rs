//! Rollout buffer for RL training

use crate::features::Features;
use crate::policy::RlAction;

/// Transition stored in rollout buffer
#[derive(Debug, Clone)]
pub struct Transition {
    pub features: Features,
    pub action: RlAction,
    pub reward: f32,
    pub value: f32,
    pub log_prob: f32,
    pub done: bool,
    /// GAE advantage estimate (filled by `compute_advantages`).
    pub advantage: f32,
    /// Discounted return target (filled by `compute_advantages`).
    pub returns: f32,
}

/// Rollout buffer for PPO/A2C
pub struct RolloutBuffer {
    transitions: Vec<Transition>,
    capacity: usize,
}

impl RolloutBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            transitions: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a transition, returning `true` if the buffer has now reached
    /// capacity (the caller should run `compute_advantages` + train, then
    /// `clear`). On-policy rollouts must stay contiguous, so once full this
    /// keeps accepting rather than silently dropping/overwriting data; it's
    /// the caller's responsibility to drain the buffer promptly.
    pub fn push(&mut self, features: Features, action: RlAction, reward: f32, done: bool) -> bool {
        let transition = Transition {
            features,
            action,
            reward,
            value: 0.0, // Will be filled during GAE computation
            log_prob: 0.0,
            done,
            advantage: 0.0,
            returns: 0.0,
        };

        self.transitions.push(transition);
        self.is_full()
    }

    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Whether the buffer has reached (or exceeded) its target capacity.
    pub fn is_full(&self) -> bool {
        self.transitions.len() >= self.capacity
    }

    pub fn clear(&mut self) {
        self.transitions.clear();
    }

    /// Get all transitions for training
    pub fn get_transitions(&self) -> &[Transition] {
        &self.transitions
    }

    /// Compute advantages and return targets using Generalized Advantage
    /// Estimation (GAE), storing the results on each transition.
    pub fn compute_advantages(&mut self, gamma: f32, gae_lambda: f32, last_value: f32) {
        let mut next_value = last_value;
        let mut next_advantage = 0.0f32;

        for transition in self.transitions.iter_mut().rev() {
            let non_terminal = 1.0 - transition.done as u8 as f32;
            let delta = transition.reward + gamma * next_value * non_terminal - transition.value;
            next_advantage = delta + gamma * gae_lambda * non_terminal * next_advantage;

            transition.advantage = next_advantage;
            transition.returns = next_advantage + transition.value;

            next_value = transition.value;
        }
    }
}

impl Default for RolloutBuffer {
    fn default() -> Self {
        Self::new(2048)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_new() {
        let buffer = RolloutBuffer::new(100);
        assert_eq!(buffer.capacity, 100);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_push() {
        let mut buffer = RolloutBuffer::new(10);
        let features = crate::features::Features::new();
        let full = buffer.push(features, crate::policy::RlAction::Wait, 1.0, false);
        assert_eq!(buffer.len(), 1);
        assert!(!full);
    }

    #[test]
    fn test_buffer_reports_full() {
        let mut buffer = RolloutBuffer::new(2);
        assert!(!buffer.is_full());

        let full = buffer.push(
            crate::features::Features::new(),
            crate::policy::RlAction::Wait,
            1.0,
            false,
        );
        assert!(!full);
        assert!(!buffer.is_full());

        let full = buffer.push(
            crate::features::Features::new(),
            crate::policy::RlAction::Wait,
            1.0,
            false,
        );
        assert!(full);
        assert!(buffer.is_full());
    }

    #[test]
    fn test_buffer_clear() {
        let mut buffer = RolloutBuffer::new(10);
        let features = crate::features::Features::new();
        buffer.push(features, crate::policy::RlAction::Wait, 1.0, false);
        buffer.clear();
        assert!(buffer.is_empty());
    }
}
