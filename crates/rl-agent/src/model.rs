//! Neural network model for the RL agent.
//!
//! This is a lightweight, dependency-free actor-critic MLP implemented on top
//! of `ndarray`. It performs real inference (a two-headed feed-forward pass)
//! and can persist/restore its weights via `safetensors`, keeping the crate
//! cross-compilable to the Raspberry Pi target without a heavy ML runtime.

use crate::policy::RlAction;
use anyhow::{Context, Result};
use ndarray::{Array1, Array2};
use safetensors::tensor::{Dtype, SafeTensors, TensorView};
use std::path::Path;

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

/// Actor-Critic network: a shared hidden layer feeding an action head (policy
/// logits) and a value head (state-value estimate).
pub struct ActorCritic {
    /// Shared layer: hidden_size x input_dim
    w_hidden: Array2<f32>,
    b_hidden: Array1<f32>,
    /// Actor head: action_dim x hidden_size
    w_actor: Array2<f32>,
    b_actor: Array1<f32>,
    /// Critic head: value_dim x hidden_size
    w_critic: Array2<f32>,
    b_critic: Array1<f32>,
    config: ModelConfig,
}

impl ActorCritic {
    /// Create a new network with small random weights (Xavier-ish uniform).
    pub fn new(config: ModelConfig) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut init = |rows: usize, cols: usize| {
            let bound = (6.0f32 / (rows + cols) as f32).sqrt();
            Array2::from_shape_fn((rows, cols), |_| rng.gen_range(-bound..bound))
        };

        let w_hidden = init(config.hidden_size, config.input_dim);
        let w_actor = init(config.action_dim, config.hidden_size);
        let w_critic = init(config.value_dim, config.hidden_size);

        Self {
            b_hidden: Array1::zeros(config.hidden_size),
            b_actor: Array1::zeros(config.action_dim),
            b_critic: Array1::zeros(config.value_dim),
            w_hidden,
            w_actor,
            w_critic,
            config,
        }
    }

    /// Forward pass. Returns `(action_logits, value)`.
    pub fn forward(&self, input: &[f32]) -> Result<(Vec<f32>, f32)> {
        anyhow::ensure!(
            input.len() == self.config.input_dim,
            "expected {} input features, got {}",
            self.config.input_dim,
            input.len()
        );
        let x = Array1::from_vec(input.to_vec());

        // Shared hidden layer with tanh activation.
        let hidden = (self.w_hidden.dot(&x) + &self.b_hidden).mapv(f32::tanh);

        let logits = self.w_actor.dot(&hidden) + &self.b_actor;
        let value = (self.w_critic.dot(&hidden) + &self.b_critic)[0];

        Ok((logits.to_vec(), value))
    }

    /// Greedy action selection: argmax over the policy logits.
    pub fn act(&self, input: &[f32]) -> Result<(RlAction, f32)> {
        let (logits, value) = self.forward(input)?;
        let best = logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);
        Ok((RlAction::from_index(best, self.config.action_dim), value))
    }

    pub fn config(&self) -> &ModelConfig {
        &self.config
    }

    /// Save all weights to a `.safetensors` file.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let tensors: [(&str, &Array2<f32>, &Array1<f32>); 3] = [
            ("hidden", &self.w_hidden, &self.b_hidden),
            ("actor", &self.w_actor, &self.b_actor),
            ("critic", &self.w_critic, &self.b_critic),
        ];

        // Materialize byte buffers first so the tensor views can borrow them.
        let mut buffers: Vec<(String, Vec<usize>, Vec<u8>)> = Vec::new();
        for (name, w, b) in tensors {
            buffers.push((
                format!("{name}.weight"),
                w.shape().to_vec(),
                f32_to_le_bytes(w.iter()),
            ));
            buffers.push((
                format!("{name}.bias"),
                b.shape().to_vec(),
                f32_to_le_bytes(b.iter()),
            ));
        }

        let views: Vec<(String, TensorView)> = buffers
            .iter()
            .map(|(name, shape, bytes)| {
                TensorView::new(Dtype::F32, shape.clone(), bytes)
                    .map(|v| (name.clone(), v))
                    .context("failed to build tensor view")
            })
            .collect::<Result<_>>()?;

        safetensors::tensor::serialize_to_file(views, &None, path.as_ref())
            .context("failed to write safetensors checkpoint")?;
        Ok(())
    }

    /// Load all weights from a `.safetensors` file, validating shapes.
    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let bytes = std::fs::read(path.as_ref())
            .with_context(|| format!("failed to read checkpoint {}", path.as_ref().display()))?;
        let st = SafeTensors::deserialize(&bytes).context("failed to parse safetensors")?;

        self.w_hidden = read_matrix(&st, "hidden.weight", self.w_hidden.dim())?;
        self.b_hidden = read_vector(&st, "hidden.bias", self.b_hidden.len())?;
        self.w_actor = read_matrix(&st, "actor.weight", self.w_actor.dim())?;
        self.b_actor = read_vector(&st, "actor.bias", self.b_actor.len())?;
        self.w_critic = read_matrix(&st, "critic.weight", self.w_critic.dim())?;
        self.b_critic = read_vector(&st, "critic.bias", self.b_critic.len())?;
        Ok(())
    }
}

fn f32_to_le_bytes<'a>(iter: impl Iterator<Item = &'a f32>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in iter {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn read_f32s(st: &SafeTensors, name: &str, expected: usize) -> Result<Vec<f32>> {
    let view = st
        .tensor(name)
        .with_context(|| format!("missing tensor `{name}` in checkpoint"))?;
    let data = view.data();
    anyhow::ensure!(
        data.len() == expected * 4,
        "tensor `{name}` has {} elements, expected {}",
        data.len() / 4,
        expected
    );
    Ok(data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn read_matrix(st: &SafeTensors, name: &str, dim: (usize, usize)) -> Result<Array2<f32>> {
    let values = read_f32s(st, name, dim.0 * dim.1)?;
    Array2::from_shape_vec(dim, values).context("shape mismatch loading matrix")
}

fn read_vector(st: &SafeTensors, name: &str, len: usize) -> Result<Array1<f32>> {
    let values = read_f32s(st, name, len)?;
    Ok(Array1::from_vec(values))
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
    fn test_forward_shapes() {
        let config = ModelConfig::default();
        let model = ActorCritic::new(config.clone());
        let input = vec![0.0f32; config.input_dim];
        let (logits, _value) = model.forward(&input).unwrap();
        assert_eq!(logits.len(), config.action_dim);
    }

    #[test]
    fn test_act_returns_valid_action() {
        let model = ActorCritic::new(ModelConfig::default());
        let input = vec![0.1f32; 49];
        let (action, _value) = model.act(&input).unwrap();
        let _ = action.to_index(16);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.safetensors");

        let model = ActorCritic::new(ModelConfig::default());
        let input = vec![0.3f32; 49];
        let (logits_before, value_before) = model.forward(&input).unwrap();
        model.save(&path).unwrap();

        let mut restored = ActorCritic::new(ModelConfig::default());
        restored.load(&path).unwrap();
        let (logits_after, value_after) = restored.forward(&input).unwrap();

        assert_eq!(logits_before, logits_after);
        assert_eq!(value_before, value_after);
    }
}
