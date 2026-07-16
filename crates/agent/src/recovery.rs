//! Recovery and persistence for agent state

use anyhow::Result;
use chrono::{DateTime, Utc};
use pwncore::Mood;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, info, warn};

/// Recovery state for persistence across reboots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub epoch: u64,
    pub total_epochs: u64,
    pub total_handshakes: u32,
    pub total_pmkids: u32,
    pub xp: u32,
    pub level: u32,
    pub last_channel: u8,
    pub last_mood: Mood,
    pub last_face: String,
    pub uptime_seconds: u64,
    pub started_at: DateTime<Utc>,
    pub last_saved: DateTime<Utc>,
    pub encounters: std::collections::HashMap<[u8; 6], u32>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            epoch: 0,
            total_epochs: 0,
            total_handshakes: 0,
            total_pmkids: 0,
            xp: 0,
            level: 0,
            last_channel: 1,
            last_mood: Mood::Awake,
            last_face: "(◕‿‿◕)".to_string(),
            uptime_seconds: 0,
            started_at: Utc::now(),
            last_saved: Utc::now(),
            encounters: std::collections::HashMap::new(),
        }
    }
}

/// Recovery manager for persisting agent state
pub struct RecoveryManager {
    state: RecoveryState,
    path: std::path::PathBuf,
    save_interval: std::time::Duration,
}

impl RecoveryManager {
    pub fn new(path: impl AsRef<Path>, save_interval_secs: u64) -> Self {
        Self {
            state: RecoveryState::default(),
            path: path.as_ref().to_path_buf(),
            save_interval: std::time::Duration::from_secs(save_interval_secs),
        }
    }

    /// Load recovery state from disk
    pub async fn load(&mut self) -> Result<()> {
        if self.path.exists() {
            let content = fs::read_to_string(&self.path).await?;
            self.state = serde_json::from_str(&content)?;
            info!(
                "Loaded recovery state from {:?} (epoch {})",
                self.path, self.state.epoch
            );
        }
        Ok(())
    }

    /// Save recovery state to disk
    pub async fn save(&mut self) -> Result<()> {
        self.state.last_saved = Utc::now();
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.path, content).await?;
        debug!("Saved recovery state to {:?}", self.path);
        Ok(())
    }

    /// Get current recovery state
    pub fn state(&self) -> &RecoveryState {
        &self.state
    }

    /// Get mutable recovery state
    pub fn state_mut(&mut self) -> &mut RecoveryState {
        &mut self.state
    }

    /// Update from agent
    pub fn update_from_agent(&mut self, agent: &crate::Agent) {
        self.state.epoch = agent.total_epochs();
        self.state.total_epochs = agent.epoch_tracker.total_epochs;
        self.state.total_handshakes = agent.personality.stats().handshakes;
        self.state.total_pmkids = agent.personality.stats().pmkids;
        self.state.xp = agent.personality.stats().xp;
        self.state.level = agent.personality.stats().level;
        self.state.last_channel = agent.current_channel();
        self.state.last_mood = agent.current_mood();
        self.state.last_face = agent.current_face().to_string();
        self.state.uptime_seconds = agent.start.elapsed().as_secs();
        self.state.started_at = agent.started_at;
    }
}

/// Run periodic save task
pub async fn run_save_task(mut manager: RecoveryManager) {
    let mut interval = tokio::time::interval(manager.save_interval);
    loop {
        interval.tick().await;
        if let Err(e) = manager.save().await {
            warn!("Failed to save recovery state: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recovery_manager() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("recovery.json");

        let mut mgr = RecoveryManager::new(&path, 60);
        assert_eq!(mgr.state.epoch, 0);

        mgr.state.epoch = 42;
        mgr.save().await.unwrap();

        let mut mgr2 = RecoveryManager::new(&path, 60);
        mgr2.load().await.unwrap();
        assert_eq!(mgr2.state.epoch, 42);
    }
}
