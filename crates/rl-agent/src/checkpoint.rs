//! Model checkpoint management

use crate::model::ActorCritic;
use anyhow::Result;
use std::path::Path;
use tracing::info;

/// Checkpoint manager for saving/loading models
pub struct CheckpointManager {
    model_dir: std::path::PathBuf,
    keep_best: usize,
}

impl CheckpointManager {
    pub fn new<P: AsRef<Path>>(model_dir: P, keep_best: usize) -> Result<Self> {
        let model_dir = model_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&model_dir)?;

        Ok(Self {
            model_dir,
            keep_best,
        })
    }

    /// Save a model checkpoint for a given training step.
    pub fn save_checkpoint(&self, model: &ActorCritic, step: usize) -> Result<()> {
        let filename = format!("checkpoint_step_{}.safetensors", step);
        let path = self.model_dir.join(&filename);

        info!("Saving checkpoint to {:?}", path);
        model.save(&path)?;

        // Clean old checkpoints
        self.cleanup_old_checkpoints()?;

        Ok(())
    }

    /// Load the latest checkpoint into `model`, returning its step number.
    pub fn load_latest(&self, model: &mut ActorCritic) -> Result<Option<usize>> {
        // Find latest checkpoint
        let mut entries: Vec<_> = std::fs::read_dir(&self.model_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("checkpoint_step_")
                    && e.file_name().to_string_lossy().ends_with(".safetensors")
            })
            .collect();

        if entries.is_empty() {
            return Ok(None);
        }

        entries.sort_by_key(|e| e.file_name());
        let latest = entries.last().unwrap();
        let path = latest.path();

        // Extract step number
        let stem = path.file_stem().unwrap().to_string_lossy();
        let step_str = stem.strip_prefix("checkpoint_step_").unwrap_or("0");
        let step: usize = step_str.parse().unwrap_or(0);

        info!("Loading checkpoint from {:?}", path);
        model.load(&path)?;

        Ok(Some(step))
    }

    /// Load a specific checkpoint step into `model`.
    pub fn load_checkpoint(&self, step: usize, model: &mut ActorCritic) -> Result<()> {
        let filename = format!("checkpoint_step_{}.safetensors", step);
        let path = self.model_dir.join(&filename);

        if !path.exists() {
            anyhow::bail!("Checkpoint {} not found", step);
        }

        info!("Loading checkpoint step {}", step);
        model.load(&path)?;

        Ok(())
    }

    /// Clean up old checkpoints, keep only best N
    fn cleanup_old_checkpoints(&self) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.model_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("checkpoint_step_")
                    && e.file_name().to_string_lossy().ends_with(".safetensors")
            })
            .collect();

        if entries.len() <= self.keep_best {
            return Ok(());
        }

        entries.sort_by_key(|e| e.file_name());

        for entry in entries.iter().take(entries.len() - self.keep_best) {
            let _ = std::fs::remove_file(entry.path());
        }

        Ok(())
    }

    /// Save model with metrics for best model tracking
    pub fn save_best_model(
        &self,
        model: &ActorCritic,
        metric_name: &str,
        metric_value: f32,
    ) -> Result<()> {
        let filename = format!(
            "best_{}_{:.4}.safetensors",
            metric_name.replace('.', "_"),
            metric_value
        );
        let path = self.model_dir.join(&filename);

        info!("Saving best model for {}: {:.4}", metric_name, metric_value);
        model.save(&path)?;

        Ok(())
    }

    /// Get list of available checkpoints
    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointInfo>> {
        let mut checkpoints = Vec::new();

        for entry in std::fs::read_dir(&self.model_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with("checkpoint_step_") && name.ends_with(".safetensors") {
                let step_str = name
                    .strip_prefix("checkpoint_step_")
                    .unwrap()
                    .strip_suffix(".safetensors")
                    .unwrap();
                if let Ok(step) = step_str.parse::<usize>() {
                    let metadata = entry.metadata()?;
                    checkpoints.push(CheckpointInfo {
                        step,
                        path: entry.path(),
                        size: metadata.len(),
                        modified: metadata.modified().ok(),
                    });
                }
            }
        }

        checkpoints.sort_by_key(|c| c.step);
        Ok(checkpoints)
    }
}

/// Checkpoint metadata
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    pub step: usize,
    pub path: std::path::PathBuf,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_checkpoint_manager_new() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path(), 5).unwrap();
        assert_eq!(mgr.keep_best, 5);
    }

    #[test]
    fn test_list_checkpoints_empty() {
        let tmp = TempDir::new().unwrap();
        let mgr = CheckpointManager::new(tmp.path(), 5).unwrap();
        let checkpoints = mgr.list_checkpoints().unwrap();
        assert!(checkpoints.is_empty());
    }
}
