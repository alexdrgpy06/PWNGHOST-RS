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
pub use features::{extract_features, Features};
pub use model::{ActorCritic, ModelConfig};
pub use policy::{Policy, HeuristicPolicy, RlAction};

use anyhow::Result;

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
    if let Some(path) = &config.model_path {
        // TODO: Load model from path when ML dependencies are available
        todo!("Model loading not implemented yet")
    } else if config.use_heuristic_fallback {
        info!("Using heuristic policy (no model loaded)");
        Ok(RlAgent::new())
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