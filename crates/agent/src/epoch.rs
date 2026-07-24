//! Epoch tracking for the agent

use chrono::{DateTime, Utc};
use pwncore::{AccessPoint, AgentMode, Channel, Mood, Peer};
use std::collections::VecDeque;

/// Current epoch state
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EpochState {
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
    pub blind_epochs: u64,
    /// Running count of epochs where APs were visible but the agent failed to
    /// engage them (no valid target, no capture). pwnagotchi's `num_missed`;
    /// drives the lonely/angry "recon backoff" in `compute_mood`. Reset to 0 on
    /// any successful deauth/associate or capture. Distinct from `blind_epochs`
    /// (no APs at all).
    pub num_missed: u32,
    pub total_handshakes: u64,
    pub total_epochs: u64,
    pub aps_seen: usize,
    pub clients_seen: usize,
    pub did_deauth: bool,
    pub did_associate: bool,
}

impl EpochState {
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
            blind_epochs: 0,
            num_missed: 0,
            total_handshakes: 0,
            total_epochs: 0,
            aps_seen: 0,
            clients_seen: 0,
            did_deauth: false,
            did_associate: false,
        }
    }

    /// Advance to next epoch
    pub fn advance(&mut self, new_channel: Channel) {
        self.finalize();
        self.epoch += 1;
        self.channel = new_channel;
        self.mode = AgentMode::Recon;
        self.aps_found = 0;
        self.handshakes_this_epoch = 0;
        self.deauths_sent = 0;
        self.assoc_attempts = 0;
        self.timestamp = Utc::now();
        self.started_at = Utc::now();
        self.ended_at = None;
        self.blind_epochs = if self.aps_found == 0 {
            self.blind_epochs + 1
        } else {
            0
        };
    }

    /// Finalize current epoch
    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());
    }

    /// Track handshake captured
    pub fn track_handshake(&mut self) {
        self.handshakes_this_epoch += 1;
    }

    /// Track deauth sent
    pub fn track_deauth(&mut self) {
        self.deauths_sent += 1;
    }

    /// Track association attempt
    pub fn track_assoc(&mut self) {
        self.assoc_attempts += 1;
    }

    /// Track a channel hop (channel itself is updated by the agent).
    pub fn track_hop(&mut self) {
        self.mode = AgentMode::Hop;
    }

    /// Duration of current epoch
    pub fn duration(&self) -> chrono::Duration {
        self.ended_at.unwrap_or_else(Utc::now) - self.started_at
    }

    /// Update observation from current APs
    pub fn observe(&mut self, aps: &[AccessPoint], _peers: &[Peer]) {
        self.aps_found = aps.len();
    }
}

/// Epoch tracker with history
pub struct EpochTracker {
    pub current: EpochState,
    pub history: VecDeque<EpochState>,
    pub total_epochs: u64,
    max_history: usize,
}

impl EpochTracker {
    pub fn new() -> Self {
        Self {
            current: EpochState::new(0, Channel::new(1).unwrap()),
            history: VecDeque::with_capacity(1000),
            total_epochs: 0,
            max_history: 1000,
        }
    }

    /// Advance to next epoch
    pub fn advance(&mut self, new_channel: Channel) {
        self.current.finalize();

        // Carry the blind-epoch streak forward: increment when the epoch we
        // just finished saw no APs, otherwise reset to zero.
        let was_blind = self.current.aps_found == 0;
        let prev_blind = self.current.blind_epochs;

        // Missed-interaction streak (pwnagotchi's `num_missed`): the epoch we
        // just finished counts as a "miss" if it saw APs but never engaged them
        // (no deauth/associate) and captured nothing. A successful interaction
        // or capture resets the streak; a fully blind epoch (no APs) holds it,
        // since that case is handled by the separate `blind_epochs` cascade.
        let attacked = self.current.did_deauth || self.current.did_associate;
        let captured = self.current.handshakes_this_epoch > 0;
        let saw_aps = self.current.aps_found > 0;
        let prev_missed = self.current.num_missed;

        self.history.push_back(self.current.clone());
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }
        self.total_epochs += 1;

        let mut next = EpochState::new(self.total_epochs, new_channel);
        next.blind_epochs = if was_blind { prev_blind + 1 } else { 0 };
        next.num_missed = if attacked || captured {
            0
        } else if saw_aps {
            prev_missed + 1
        } else {
            prev_missed
        };
        next.total_epochs = self.total_epochs;
        self.current = next;
    }

    /// Finalize current epoch
    pub fn finalize_current(&mut self) {
        self.current.finalize();
    }

    /// Get current epoch reference
    pub fn current(&self) -> &EpochState {
        &self.current
    }

    /// The most recently finalized epoch's real counts (handshakes, deauths,
    /// associations, APs seen), i.e. the epoch `advance()` just pushed into
    /// `history`. `current` at this point is the *next* epoch's freshly
    /// zeroed state, not the one that just ended -- callers that want to
    /// report on "what just happened" (e.g. the `on_epoch` plugin hook) need
    /// this, not `current`.
    pub fn last_completed(&self) -> Option<&EpochState> {
        self.history.back()
    }

    /// Get mutable current epoch
    pub fn current_mut(&mut self) -> &mut EpochState {
        &mut self.current
    }

    /// Get history
    pub fn history(&self) -> &VecDeque<EpochState> {
        &self.history
    }

    /// Total epochs
    pub fn total_epochs(&self) -> u64 {
        self.total_epochs
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
    fn test_epoch_tracker_new() {
        let tracker = EpochTracker::new();
        assert_eq!(tracker.total_epochs, 0);
        assert_eq!(tracker.current.epoch, 0);
    }

    #[test]
    fn test_last_completed_has_finished_epoch_real_data_not_zeroed_current() {
        let mut tracker = EpochTracker::new();
        assert!(tracker.last_completed().is_none());

        tracker.current.aps_found = 5;
        tracker.current.handshakes_this_epoch = 2;
        tracker.advance(Channel::new(6).unwrap());

        // `current` is now the *next* epoch's freshly zeroed state ...
        assert_eq!(tracker.current.aps_found, 0);
        assert_eq!(tracker.current.handshakes_this_epoch, 0);
        // ... but `last_completed()` still has the real counts from the
        // epoch that just finished, which is what `on_epoch` plugin hooks
        // should observe.
        let finished = tracker.last_completed().unwrap();
        assert_eq!(finished.epoch, 0);
        assert_eq!(finished.aps_found, 5);
        assert_eq!(finished.handshakes_this_epoch, 2);
    }

    #[test]
    fn test_epoch_advance() {
        let mut tracker = EpochTracker::new();
        tracker.current.aps_found = 5;
        tracker.current.handshakes_this_epoch = 2;

        tracker.advance(Channel::new(6).unwrap());

        assert_eq!(tracker.total_epochs, 1);
        assert_eq!(tracker.current.epoch, 1);
        assert_eq!(tracker.current.channel.value(), 6);
        assert_eq!(tracker.current.aps_found, 0);
        assert_eq!(tracker.history.len(), 1);
        assert_eq!(tracker.history[0].aps_found, 5);
    }

    #[test]
    fn test_epoch_finalize() {
        let mut tracker = EpochTracker::new();
        tracker.current.aps_found = 3;
        tracker.finalize_current();
        assert!(tracker.current.ended_at.is_some());
    }

    #[test]
    fn test_epoch_duration() {
        let mut epoch = EpochState::new(1, Channel::new(1).unwrap());
        epoch.started_at = chrono::Utc::now();
        let duration = epoch.duration();
        assert!(duration.num_milliseconds() >= 0);
    }

    #[test]
    fn test_epoch_history_limit() {
        let mut tracker = EpochTracker::new();
        tracker.max_history = 3;

        for i in 1..=5 {
            tracker.advance(Channel::new(i as u8).unwrap());
        }

        assert_eq!(tracker.history.len(), 3);
        assert_eq!(tracker.history[0].epoch, 2); // oldest
        assert_eq!(tracker.history[2].epoch, 4); // newest
    }
}
