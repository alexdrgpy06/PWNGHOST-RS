//! Neural network models for RL agent

use anyhow::Result;
use candle_core::{Device, Tensor, DType};
use candle_nn::{linear, rnn, Linear, LSTM, Module, VarBuilder, VarMap};

/// Model configuration
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub input_dim: usize,
    pub hidden_size: usize,
    pub lstm_layers: usize,
    pub action_dim: usize,
    pub value_dim: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            input_dim: 49,
            hidden_size: 64,
            lstm_layers: 1,
            action_dim: 16,
            value_dim: 1,
        }
    }
}

/// Actor-Critic network with LSTM
pub struct ActorCritic {
    lstm: LSTM,
    actor: Linear,
    critic: Linear,
    config: ModelConfig,
    device: Device,
}

impl ActorCritic {
    pub fn new(config: ModelConfig, device: Device) -> Result<Self> {
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);

        let lstm = rnn::lstm(config.input_dim, config.hidden_size, config.lstm_layers, vb.pp("lstm"))?;
        let actor = linear(config.hidden_size, config.action_dim, vb.pp("actor"))?;
        let critic = linear(config.hidden_size, config.value_dim, vb.pp("critic"))?;

        Ok(Self {
            lstm,
            actor,
            critic,
            config,
            device,
        })
    }

    /// Forward pass: returns (action_logits, value)
    pub fn forward(&self, input: &Tensor, hidden: Option<(&Tensor, &Tensor)>) -> Result<(Tensor, Tensor, (Tensor, Tensor))> {
        let (output, (h_n, c_n)) = match hidden {
            Some((h, c)) => self.lstm.forward(input, Some((h, c)))?,
            None => self.lstm.forward(input, None)?,
        };

        // Use last timestep output
        let last_output = output.i((.., -1, ..))?;
        let action_logits = self.actor.forward(&last_output)?;
        let value = self.critic.forward(&last_output)?;

        Ok((action_logits, value, (h_n, c_n)))
    }

    /// Get action distribution and value
    pub fn act(&self, input: &Tensor, hidden: Option<(&Tensor, &Tensor)>) -> Result<(RlAction, f32, (Tensor, Tensor))> {
        let (logits, value, new_hidden) = self.forward(input, hidden)?;

        // Sample action from logits
        let probs = candle_nn::ops::softmax(&logits, 1)?;
        let action_idx = probs.argmax(1)?.to_scalar::<u32>()? as u8;
        let action = RlAction::from_index(action_idx, self.config.action_dim);

        let value_scalar = value.to_scalar::<f32>()?;

        Ok((action, value_scalar, new_hidden))
    }

    /// Get model parameters for saving
    pub fn parameters(&self) -> Vec<Tensor> {
        // In a real implementation, collect all parameters
        vec![]
    }

    pub fn config(&self) -> &ModelConfig {
        &self.config
    }
}

/// Initialize LSTM hidden state
pub fn init_hidden(batch_size: usize, config: &ModelConfig, device: &Device) -> Result<(Tensor, Tensor)> {
    let h = Tensor::zeros((config.lstm_layers, batch_size, config.hidden_size), DType::F32, device)?;
    let c = Tensor::zeros((config.lstm_layers, batch_size, config.hidden_size), DType::F32, device)?;
    Ok((h, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_default() {
        let config = ModelConfig::default();
        assert_eq!(config.input_dim, 49);
        assert_eq!(config.hidden_size, 64);
        assert_eq!(config.action_dim, 16);
    }

    #[test]
    fn test_init_hidden() {
        let config = ModelConfig::default();
        let device = Device::Cpu;
        let (h, c) = init_hidden(1, &config, &device).unwrap();
        assert_eq!(h.dims(), &[1, 1, 64]);
        assert_eq!(c.dims(), &[1, 1, 64]);
    }
}