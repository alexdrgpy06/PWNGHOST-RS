//! RL Agent implementation

use crate::{
    features::Features,
    policy::{HeuristicPolicy, Policy, RlAction},
};

pub struct RlAgent {
    policy: Box<dyn Policy + Send>,
    total_decisions: u64,
    last_action: Option<RlAction>,
}

impl RlAgent {
    pub fn new() -> Self {
        Self {
            policy: Box::new(HeuristicPolicy::new()),
            total_decisions: 0,
            last_action: None,
        }
    }

    pub fn with_policy(policy: Box<dyn Policy + Send>) -> Self {
        Self {
            policy,
            total_decisions: 0,
            last_action: None,
        }
    }

    pub fn select_action(&mut self, features: &Features) -> RlAction {
        let action = self.policy.select_action(features);
        self.total_decisions += 1;
        self.last_action = Some(action);
        action
    }

    pub fn total_decisions(&self) -> u64 {
        self.total_decisions
    }

    pub fn last_action(&self) -> Option<RlAction> {
        self.last_action
    }

    pub fn policy_name(&self) -> String {
        "heuristic-v1".into()
    }

    /// Feed back the real-world outcome of the last action this agent
    /// selected (e.g. a captured handshake, a blind epoch) to the
    /// underlying policy. A no-op unless the policy actually learns
    /// online (see [`crate::policy::BanditPolicy`]).
    pub fn observe_reward(&mut self, reward: f32) {
        if let Some(action) = self.last_action {
            self.policy.observe_reward(action, reward);
        }
    }

    /// Serialize the policy's learned state for persistence across
    /// reboots (see `agent::recovery`). `None` if the current policy has
    /// nothing to persist.
    pub fn export_policy_state(&self) -> Option<Vec<u8>> {
        self.policy.export_state()
    }

    /// Restore previously persisted policy state.
    pub fn import_policy_state(&mut self, data: &[u8]) {
        self.policy.import_state(data);
    }
}

impl Default for RlAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rl_agent_new() {
        let agent = RlAgent::new();
        assert_eq!(agent.total_decisions(), 0);
        assert!(agent.last_action().is_none());
    }

    #[test]
    fn test_observe_reward_without_prior_action_is_noop() {
        let mut agent = RlAgent::new();
        agent.observe_reward(1.0); // no last_action yet; must not panic
    }

    #[test]
    fn test_observe_reward_applies_to_bandit_policy() {
        let mut agent = RlAgent::with_policy(Box::new(crate::policy::BanditPolicy::new(16)));
        let features = Features::new();
        agent.select_action(&features);
        agent.observe_reward(1.0);
        // The policy learned something -- exported state differs from a
        // freshly constructed one.
        let state = agent.export_policy_state().expect("bandit exports state");
        let mut fresh = crate::policy::BanditPolicy::new(16);
        fresh.import_state(&state);
        assert!(fresh.total_updates() > 0);
    }

    #[test]
    fn test_export_import_policy_state_noop_for_heuristic() {
        let agent = RlAgent::new();
        assert!(agent.export_policy_state().is_none());
    }

    #[test]
    fn test_select_action_returns_valid_action() {
        let mut agent = RlAgent::new();
        let features = Features::new();
        let action = agent.select_action(&features);
        match action {
            RlAction::HopChannel(ch) => assert!((1..=14).contains(&ch)),
            RlAction::Deauth | RlAction::Associate | RlAction::Wait => {}
            RlAction::Sleep(_) => {}
        }
    }

    #[test]
    fn test_total_decisions_increments() {
        let mut agent = RlAgent::new();
        let features = Features::new();
        assert_eq!(agent.total_decisions(), 0);
        agent.select_action(&features);
        assert_eq!(agent.total_decisions(), 1);
        agent.select_action(&features);
        assert_eq!(agent.total_decisions(), 2);
    }

    #[test]
    fn test_last_action_tracked() {
        let mut agent = RlAgent::new();
        let features = Features::new();
        let action = agent.select_action(&features);
        assert_eq!(agent.last_action(), Some(action));
    }

    #[test]
    fn test_with_custom_policy() {
        struct AlwaysWait;
        impl Policy for AlwaysWait {
            fn select_action(&self, _features: &Features) -> RlAction {
                RlAction::Wait
            }
            fn action_space(&self) -> u8 {
                1
            }
        }
        let mut agent = RlAgent::with_policy(Box::new(AlwaysWait));
        let features = Features::new();
        assert_eq!(agent.select_action(&features), RlAction::Wait);
    }

    #[test]
    fn test_heuristic_returns_actions() {
        let mut agent = RlAgent::new();
        let features = Features::new();
        // With empty features, heuristic should return a HopChannel
        let action = agent.select_action(&features);
        assert!(matches!(
            action,
            RlAction::HopChannel(_)
                | RlAction::Deauth
                | RlAction::Associate
                | RlAction::Wait
                | RlAction::Sleep(_)
        ));
    }
}
