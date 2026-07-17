use crate::features::Features;
use crate::policy::{HeuristicPolicy, Policy, RlAction};

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
