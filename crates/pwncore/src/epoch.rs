use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::ap::AccessPoint;
use crate::peer::Peer;
use crate::personality::Personality;

/// Epoch tracking - one cycle of recon/attack/hop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epoch {
    pub number: u64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    
    // Activity tracking
    pub aps_seen: usize,
    pub clients_seen: usize,
    pub handshakes_captured: usize,
    pub deauths_sent: usize,
    pub associations_sent: usize,
    pub channel_hops: usize,
    pub sleep_time: Duration,
    
    // Mood-related
    pub inactive_epochs: u32,
    pub active_epochs: u32,
    pub bored_epochs: u32,
    pub sad_epochs: u32,
    pub blind_epochs: u32,
    
    // Current activity
    pub did_deauth: bool,
    pub did_associate: bool,
    pub any_activity: bool,
    
    // Peer bonding
    pub peer_encounters: u32,
    pub total_bond_factor: f64,
    pub avg_bond_factor: f64,
    
    // System stats
    pub cpu_load: f32,
    pub memory_usage: f32,
    pub temperature: f32,
}

impl Epoch {
    pub fn new(number: u64) -> Self {
        Self {
            number,
            started_at: Utc::now(),
            ended_at: None,
            aps_seen: 0,
            clients_seen: 0,
            handshakes_captured: 0,
            deauths_sent: 0,
            associations_sent: 0,
            channel_hops: 0,
            sleep_time: Duration::zero(),
            inactive_epochs: 0,
            active_epochs: 0,
            bored_epochs: 0,
            sad_epochs: 0,
            blind_epochs: 0,
            did_deauth: false,
            did_associate: false,
            any_activity: false,
            peer_encounters: 0,
            total_bond_factor: 0.0,
            avg_bond_factor: 0.0,
            cpu_load: 0.0,
            memory_usage: 0.0,
            temperature: 0.0,
        }
    }

    pub fn end(&mut self) {
        self.ended_at = Some(Utc::now());
    }

    pub fn duration(&self) -> Duration {
        let end = self.ended_at.unwrap_or_else(Utc::now);
        end.signed_duration_since(self.started_at)
    }

    pub fn observe(&mut self, aps: &[AccessPoint], peers: &[Peer]) {
        self.aps_seen = aps.len();
        self.clients_seen = aps.iter().map(|ap| ap.clients.len()).sum();
        
        // Track peer bonding
        self.peer_encounters = peers.len() as u32;
        let total_encounters: u32 = peers.iter().map(|p| p.encounters).sum();
        self.total_bond_factor = total_encounters as f64;
        self.avg_bond_factor = if !peers.is_empty() {
            total_encounters as f64 / peers.len() as f64
        } else {
            0.0
        };

        // Track blind epochs
        if aps.is_empty() {
            self.blind_epochs += 1;
        } else {
            self.blind_epochs = 0;
        }
    }

    pub fn track_deauth(&mut self) {
        self.deauths_sent += 1;
        self.did_deauth = true;
        self.any_activity = true;
    }

    pub fn track_assoc(&mut self) {
        self.associations_sent += 1;
        self.did_associate = true;
        self.any_activity = true;
    }

    pub fn track_handshake(&mut self) {
        self.handshakes_captured += 1;
        self.any_activity = true;
    }

    pub fn track_hop(&mut self) {
        self.channel_hops += 1;
        // Reset channel-specific activity flags
        self.did_deauth = false;
        self.did_associate = false;
    }

    pub fn track_sleep(&mut self, duration: Duration) {
        self.sleep_time += duration;
    }

    pub fn finalize(&mut self, config: &Personality) {
        self.end();
        
        // Update mood counters
        if !self.any_activity && self.handshakes_captured == 0 {
            self.inactive_epochs += 1;
            self.active_epochs = 0;
            
            if self.inactive_epochs >= config.sad_epochs {
                self.sad_epochs += 1;
                self.bored_epochs = 0;
            } else if self.inactive_epochs >= config.bored_epochs {
                self.bored_epochs += 1;
                self.sad_epochs = 0;
            }
        } else {
            self.active_epochs += 1;
            self.inactive_epochs = 0;
            self.bored_epochs = 0;
            self.sad_epochs = 0;
        }
    }
}

/// Epoch history for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochHistory {
    pub epochs: Vec<Epoch>,
    pub max_epochs: usize,
}

impl EpochHistory {
    pub fn new(max_epochs: usize) -> Self {
        Self {
            epochs: Vec::new(),
            max_epochs,
        }
    }

    pub fn add(&mut self, epoch: Epoch) {
        self.epochs.push(epoch);
        if self.epochs.len() > self.max_epochs {
            self.epochs.remove(0);
        }
    }

    pub fn recent(&self, count: usize) -> &[Epoch] {
        let start = self.epochs.len().saturating_sub(count);
        &self.epochs[start..]
    }

    pub fn total_handshakes(&self) -> u64 {
        self.epochs.iter().map(|e| e.handshakes_captured as u64).sum()
    }

    pub fn total_deauths(&self) -> u64 {
        self.epochs.iter().map(|e| e.deauths_sent as u64).sum()
    }

    pub fn total_associations(&self) -> u64 {
        self.epochs.iter().map(|e| e.associations_sent as u64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_creation() {
        let epoch = Epoch::new(1);
        assert_eq!(epoch.number, 1);
        assert!(epoch.aps_seen == 0);
    }

    #[test]
    fn test_epoch_tracking() {
        let mut epoch = Epoch::new(1);
        epoch.track_deauth();
        epoch.track_assoc();
        epoch.track_handshake();
        
        assert_eq!(epoch.deauths_sent, 1);
        assert_eq!(epoch.associations_sent, 1);
        assert_eq!(epoch.handshakes_captured, 1);
        assert!(epoch.any_activity);
    }

}