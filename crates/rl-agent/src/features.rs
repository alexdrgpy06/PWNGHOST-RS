//! Feature extraction for RL agent

use anyhow::Result;
use ndarray::Array1;
use crate::policy::RlAction;

/// Observation features (49-dimensional)
#[derive(Debug, Clone)]
pub struct Features {
    /// AP histogram across 13 channels
    pub ap_histogram: Array1<f32>,

    /// Station histogram across 13 channels
    pub sta_histogram: Array1<f32>,

    /// Peer histogram across 13 channels
    pub peer_histogram: Array1<f32>,

    /// Epoch statistics (10 dims)
    pub epoch_stats: Array1<f32>,
}

impl Features {
    pub fn new() -> Self {
        Self {
            ap_histogram: Array1::zeros(13),
            sta_histogram: Array1::zeros(13),
            peer_histogram: Array1::zeros(13),
            epoch_stats: Array1::zeros(10),
        }
    }

    /// Convert to tensor for model input
    pub fn to_tensor(&self) -> Result<candle_core::Tensor> {
        let mut data = Vec::with_capacity(49);
        data.extend_from_slice(self.ap_histogram.as_slice().unwrap());
        data.extend_from_slice(self.sta_histogram.as_slice().unwrap());
        data.extend_from_slice(self.peer_histogram.as_slice().unwrap());
        data.extend_from_slice(self.epoch_stats.as_slice().unwrap());

        let tensor = candle_core::Tensor::from_slice(&data, (1, 49), &candle_core::Device::Cpu)?;
        Ok(tensor)
    }

    /// Update from agent state
    pub fn update_from_agent(&mut self, agent: &crate::agent::Agent) {
        // Update histograms
        for ap in &agent.aps {
            let ch = ap.channel.value().saturating_sub(1) as usize;
            if ch < 13 {
                self.ap_histogram[ch] += 1.0;
            }
            for _client in &ap.clients {
                self.sta_histogram[ch] += 1.0;
            }
        }

        // Update peer histogram
        for peer in &agent.peers {
            if peer.channel >= 1 && peer.channel <= 13 {
                self.peer_histogram[peer.channel as usize - 1] += 1.0;
            }
        }

        // Normalize
        let max_ap = self.ap_histogram.iter().cloned().fold(0.0, f32::max);
        if max_ap > 0.0 {
            self.ap_histogram /= max_ap;
        }

        let max_sta = self.sta_histogram.iter().cloned().fold(0.0, f32::max);
        if max_sta > 0.0 {
            self.sta_histogram /= max_sta;
        }

        let max_peer = self.peer_histogram.iter().cloned().fold(0.0, f32::max);
        if max_peer > 0.0 {
            self.peer_histogram /= max_peer;
        }

        // Update epoch stats
        let epoch = &agent.epoch_tracker.current;
        self.epoch_stats[0] = epoch.aps_found as f32 / 50.0;
        self.epoch_stats[1] = epoch.handshakes_this_epoch as f32 / 10.0;
        self.epoch_stats[2] = epoch.deauths_sent as f32 / 20.0;
        self.epoch_stats[3] = epoch.assoc_attempts as f32 / 20.0;
        self.epoch_stats[4] = epoch.duration().as_secs() as f32 / 60.0;
        self.epoch_stats[5] = epoch.blind_epochs as f32 / 10.0;
        self.epoch_stats[6] = agent.current_channel as f32 / 14.0;
        self.epoch_stats[7] = agent.total_epochs() as f32 / 1000.0;
        self.epoch_stats[8] = agent.peers.len() as f32 / 10.0;
        self.epoch_stats[9] = match agent.current_mood {
            pwncore::Mood::Happy | pwncore::Mood::Excited => 1.0,
            pwncore::Mood::Sad | pwncore::Mood::Angry => -1.0,
            _ => 0.0,
        };
    }

    /// Get feature vector as flat array
    pub fn as_flat(&self) -> Vec<f32> {
        let mut data = Vec::with_capacity(49);
        data.extend_from_slice(self.ap_histogram.as_slice().unwrap());
        data.extend_from_slice(self.sta_histogram.as_slice().unwrap());
        data.extend_from_slice(self.peer_histogram.as_slice().unwrap());
        data.extend_from_slice(self.epoch_stats.as_slice().unwrap());
        data
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract features from agent state for RL
pub fn extract_features(agent: &crate::agent::Agent) -> Features {
    let mut features = Features::new();
    features.update_from_agent(agent);
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_features_new() {
        let f = Features::new();
        assert_eq!(f.ap_histogram.len(), 13);
        assert_eq!(f.sta_histogram.len(), 13);
        assert_eq!(f.peer_histogram.len(), 13);
        assert_eq!(f.epoch_stats.len(), 10);
    }

    #[test]
    fn test_features_to_tensor() {
        let f = Features::new();
        let tensor = f.to_tensor().unwrap();
        assert_eq!(tensor.dims(), &[1, 49]);
    }

    #[test]
    fn test_features_as_flat() {
        let f = Features::new();
        let flat = f.as_flat();
        assert_eq!(flat.len(), 49);
    }
}