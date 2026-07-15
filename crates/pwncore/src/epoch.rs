//! Epoch types for tracking agent state

use crate::{AgentMode, Channel, Mood};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Epoch state - tracks one cycle of the agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Epoch {
    pub epoch: u64,
    pub channel: Channel,
    pub mode: AgentMode,
    pub aps_found: usize,
    pub handshakes_this_epoch: u32,
    pub deauths_sent: u32,
    pub assoc_attempts: u32,
    pub mood: Mood,
    pub timestamp: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Epoch {
    pub fn new(epoch: u64, channel: Channel) -> Self {
        let now = Utc::now();
        Self {
            epoch,
            channel,
            mode: AgentMode::Recon,
            aps_found: 0,
            handshakes_this_epoch: 0,
            deauths_sent: 0,
            assoc_attempts: 0,
            mood: Mood::Awake,
            timestamp: now,
            started_at: now,
            ended_at: None,
        }
    }

    pub fn observe(&mut self, aps: &[crate::AccessPoint], peers: &[crate::Peer]) {
        self.aps_found = aps.len();
        // Mood will be computed by personality
    }

    pub fn track_handshake(&mut self) {
        self.handshakes_this_epoch += 1;
    }

    pub fn track_deauth(&mut self) {
        self.deauths_sent += 1;
    }

    pub fn track_assoc(&mut self) {
        self.assoc_attempts += 1;
    }

    pub fn track_hop(&mut self, new_channel: Channel) {
        self.channel = new_channel;
    }

    pub fn duration(&self) -> Duration {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        (end - self.started_at).to_std().unwrap_or_default()
    }

    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());
    }
}

/// Epoch tracker with history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpochTracker {
    pub total_epochs: u64,
    pub current: Epoch,
    pub history: Vec<Epoch>,
    pub max_history: usize,
}

impl EpochTracker {
    pub fn new() -> Self {
        Self {
            total_epochs: 0,
            current: Epoch::new(0, Channel::new(1).unwrap()),
            history: Vec::new(),
            max_history: 1000,
        }
    }

    pub fn advance(&mut self, new_channel: Channel) {
        self.current.finalize();
        self.history.push(self.current.clone());
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
        self.total_epochs += 1;
        self.current = Epoch::new(self.total_epochs, new_channel);
    }

    pub fn finalize_current(&mut self, personality: &crate::PersonalityConfig) {
        self.current.finalize();
        // Update mood based on epoch stats - delegated to personality
    }
}

impl Default for EpochTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_new() {
        let epoch = Epoch::new(1, Channel::new(6).unwrap());
        assert_eq!(epoch.epoch, 1);
        assert_eq!(epoch.channel.value(), 6);
        assert_eq!(epoch.mode, AgentMode::Recon);
        assert_eq!(epoch.mood, Mood::Awake);
    }

    #[test]
    fn test_epoch_tracker_advance() {
        let mut tracker = EpochTracker::new();
        assert_eq!(tracker.total_epochs, 0);

        tracker.advance(Channel::new(6).unwrap());
        assert_eq!(tracker.total_epochs, 1);
        assert_eq!(tracker.current.epoch, 1);
        assert_eq!(tracker.current.channel.value(), 6);
        assert_eq!(tracker.history.len(), 1);
        assert_eq!(tracker.history[0].epoch, 0);
    }

    #[test]
    fn test_epoch_tracker_history_limit() {
        let mut tracker = EpochTracker::new();
        tracker.max_history = 3;

        for i in 1..=5 {
            tracker.advance(Channel::new(i as u8).unwrap());
        }

        assert_eq!(tracker.history.len(), 3);
        assert_eq!(tracker.history[0].epoch, 2); // oldest dropped
        assert_eq!(tracker.history[2].epoch, 4); // newest
    }
}