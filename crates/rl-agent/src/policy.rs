use crate::features::Features;

/// Actions the RL agent can select
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RlAction {
    /// Hop to a specific channel (1-14)
    HopChannel(u8),
    /// Send deauth on current channel
    Deauth,
    /// Associate with a target
    Associate,
    /// Wait / stay on current channel
    Wait,
    /// Sleep for N seconds
    Sleep(u8),
}

/// Policy trait — can be implemented by heuristic, candle model, etc.
pub trait Policy: Send {
    /// Select action given current features
    fn select_action(&self, features: &Features) -> RlAction;

    /// Number of possible actions
    fn action_space(&self) -> u8;
}

/// Heuristic policy — acts like a trained RL agent would.
/// Uses the feature vector to make decisions based on simple rules.
pub struct HeuristicPolicy {
    pub name: String,
}

impl HeuristicPolicy {
    pub fn new() -> Self {
        Self {
            name: "heuristic-v1".into(),
        }
    }
}

impl Default for HeuristicPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl Policy for HeuristicPolicy {
    fn select_action(&self, features: &Features) -> RlAction {
        let es = &features.epoch_stats;

        // Blind epochs: no APs seen → hop
        if es[2] > 0.0 && es[2] >= 0.5 {
            let channel = pick_channel_from_histogram(&features.ap_histogram);
            return RlAction::HopChannel(channel);
        }

        // Inactive: few APs, low activity → hop to explore
        if es[0] > 0.2 && es[3] < 0.3 {
            let channel = pick_channel_from_histogram(&features.ap_histogram);
            return RlAction::HopChannel(channel);
        }

        // Active: high AP density, good activity → deauth or associate
        if es[7] > 0.0 && es[4] < 0.5 {
            if features.ap_histogram.iter().any(|&c| c > 0.0) {
                return RlAction::Deauth;
            }
        }

        // Handshakes captured → associate or wait
        if es[4] > 0.0 {
            return RlAction::Associate;
        }

        // High bond factor → stay, interact with peers
        if es[6] > 0.5 {
            return RlAction::Wait;
        }

        // Default: explore by hopping
        let channel = pick_channel_from_histogram(&features.ap_histogram);
        RlAction::HopChannel(channel)
    }

    fn action_space(&self) -> u8 {
        17 // 14 channels + deauth + associate + wait
    }
}

/// Pick the channel with the fewest APs (load balancing).
/// Falls back to channel 1 when histogram is empty.
fn pick_channel_from_histogram(hist: &[f32; 13]) -> u8 {
    let mut min_count = f32::MAX;
    let mut best_ch = 1;
    for (i, &count) in hist.iter().enumerate() {
        if count < min_count {
            min_count = count;
            best_ch = (i + 1) as u8;
        }
    }
    best_ch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_policy_always_returns_valid_action() {
        let policy = HeuristicPolicy::new();
        let features = Features::new();
        let action = policy.select_action(&features);
        match action {
            RlAction::HopChannel(ch) => assert!((1..=14).contains(&ch)),
            RlAction::Deauth | RlAction::Associate | RlAction::Wait => {}
            RlAction::Sleep(_) => {}
        }
    }

    #[test]
    fn test_pick_channel_empty_histogram() {
        let hist = [0.0f32; 13];
        assert_eq!(pick_channel_from_histogram(&hist), 1);
    }

    #[test]
    fn test_pick_channel_skips_busy() {
        let mut hist = [0.0f32; 13];
        hist[0] = 5.0;  // channel 1 busy
        hist[1] = 3.0;  // channel 2 somewhat busy
        hist[5] = 10.0; // channel 6 very busy
        assert_eq!(pick_channel_from_histogram(&hist), 3);
    }

    #[test]
    fn test_action_space_17() {
        let policy = HeuristicPolicy::new();
        assert_eq!(policy.action_space(), 17);
    }
}
