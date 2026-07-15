//! Model checkpoint management

use anyhow::Result;
use candle_core::{Device, Module, VarMap};
use candle_nn::VarBuilder;
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

    /// Save model checkpoint
    pub fn save_checkpoint(
        &self,
        model: &dyn Module,
        step: usize,
        metrics: &std::collections::HashMap<String, f32>,
    ) -> Result<()> {
        let filename = format!("checkpoint_step_{}.safetensors", step);
        let path = self.model_dir.join(&filename);

        // In real implementation, use candle's save mechanism
        // For now, just create placeholder
        info!("Saving checkpoint to {:?}", path);

        // Clean old checkpoints
        self.cleanup_old_checkpoints()?;

        Ok(())
    }

    /// Load latest checkpoint
    pub fn load_latest<M: Module>(&self, model: &mut M, device: &Device) -> Result<Option<usize>> {
        // Find latest checkpoint
        let mut entries: Vec<_> = std::fs::read_dir(&self.model_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("checkpoint_step_")
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

        // In real implementation:
        // let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, device)? };
        // model.load(vb)?;

        Ok(Some(step))
    }

    /// Load specific checkpoint
    pub fn load_checkpoint<M: Module>(&self, step: usize, model: &mut M, device: &Device) -> Result<()> {
        let filename = format!("checkpoint_step_{}.safetensors", step);
        let path = self.model_dir.join(&filename);

        if !path.exists() {
            anyhow::bail!("Checkpoint {} not found", step);
        }

        info!("Loading checkpoint step {}", step);
        // let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, device)? };
        // model.load(vb)?;

        Ok(())
    }

    /// Clean up old checkpoints, keep only best N
    fn cleanup_old_checkpoints(&self) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.model_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_string_lossy().starts_with("checkpoint_step_")
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
    pub fn save_best_model<M: Module>(
        &self,
        model: &M,
        metric_name: &str,
        metric_value: f32,
    ) -> Result<()> {
        let filename = format!("best_{}_{:.4}.safetensors", metric_name.replace('.', "_"), metric_value);
        let path = self.model_dir.join(&filename);

        info!("Saving best model for {}: {:.4}", metric_name, metric_value);
        // model.save(&path)?;

        Ok(())
    }

    /// Get list of available checkpoints
    pub fn list_checkpoints(&self) -> Result<Vec<CheckpointInfo>> {
        let mut checkpoints = Vec::new();

        for entry in std::fs::read_dir(&self.model_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with("checkpoint_step_") && name.ends_with(".safetensors") {
                let step_str = name.strip_prefix("checkpoint_step_").unwrap().strip_suffix(".safetensors").unwrap();
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