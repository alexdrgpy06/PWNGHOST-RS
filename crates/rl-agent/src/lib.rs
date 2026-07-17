//! A2C RL Agent for PWNGHOST-RS

pub mod agent;
pub mod buffer;
pub mod checkpoint;
pub mod features;
pub mod model;
pub mod policy;

pub use agent::RlAgent;
pub use buffer::RolloutBuffer;
pub use checkpoint::CheckpointManager;
pub use features::Features;
pub use model::{ActorCritic, ModelConfig};
pub use policy::{BanditPolicy, HeuristicPolicy, Policy, RlAction};

use anyhow::Result;
use tracing::info;

/// RL Agent configuration
#[derive(Debug, Clone)]
pub struct RlAgentConfig {
    pub model_path: Option<String>,
    pub use_heuristic_fallback: bool,
    pub observation_dim: usize,
    pub action_dim: usize,
    pub hidden_size: usize,
    pub lstm_layers: usize,
    pub lr: f64,
}

impl Default for RlAgentConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            use_heuristic_fallback: true,
            observation_dim: 49,
            action_dim: 16,
            hidden_size: 128,
            lstm_layers: 2,
            lr: 3e-4,
        }
    }
}

/// Initialize RL agent from config
pub fn init_agent(config: &RlAgentConfig) -> Result<RlAgent> {
    use anyhow::Context;

    if let Some(path) = &config.model_path {
        let mut model = ActorCritic::new(ModelConfig {
            input_dim: config.observation_dim,
            hidden_size: config.hidden_size,
            lstm_layers: config.lstm_layers,
            action_dim: config.action_dim,
            value_dim: 1,
        });
        model
            .load(path)
            .with_context(|| format!("Failed to load RL model from {path}"))?;
        info!("Loaded RL model from {path}");
        Ok(RlAgent::with_policy(Box::new(policy::ModelPolicy::new(
            model,
        ))))
    } else if config.use_heuristic_fallback {
        // BanditPolicy replaces the old static HeuristicPolicy here: it's a
        // strict upgrade for a device with no trained model on it (no A2C
        // checkpoint exists yet -- that needs an offline training pipeline
        // and real deployment data, neither of which exist). It behaves
        // similarly to the heuristic early on (high initial exploration,
        // biased toward channels with observed AP activity) and then
        // actually improves its channel/action choices from real reward
        // signals over the device's lifetime -- see `BanditPolicy`'s docs.
        info!("Using online-learning bandit policy (no trained model loaded)");
        Ok(RlAgent::with_policy(Box::new(policy::BanditPolicy::new(
            config.action_dim,
        ))))
    } else {
        anyhow::bail!("No model path provided and heuristic fallback disabled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = RlAgentConfig::default();
        assert_eq!(config.observation_dim, 49);
        assert_eq!(config.action_dim, 16);
        assert!(config.use_heuristic_fallback);
    }
}
