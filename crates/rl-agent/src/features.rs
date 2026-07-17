use pwncore::ap::AccessPoint;
use pwncore::epoch::Epoch;
use pwncore::peer::Peer;
use pwncore::personality::Personality;

#[derive(Debug, Clone)]
pub struct Features {
    pub ap_histogram: [f32; 13],
    pub sta_histogram: [f32; 13],
    pub peer_histogram: [f32; 13],
    pub epoch_stats: [f32; 10],
}

impl Features {
    pub fn new() -> Self {
        Self {
            ap_histogram: [0.0; 13],
            sta_histogram: [0.0; 13],
            peer_histogram: [0.0; 13],
            epoch_stats: [0.0; 10],
        }
    }

    pub fn as_slice(&self) -> [f32; 49] {
        let mut flat = [0.0f32; 49];
        let mut idx = 0;
        for &v in &self.ap_histogram {
            flat[idx] = v;
            idx += 1;
        }
        for &v in &self.sta_histogram {
            flat[idx] = v;
            idx += 1;
        }
        for &v in &self.peer_histogram {
            flat[idx] = v;
            idx += 1;
        }
        for &v in &self.epoch_stats {
            flat[idx] = v;
            idx += 1;
        }
        flat
    }

    /// Extract 49-dim feature vector from current epoch state
    pub fn from_epoch(
        epoch: &Epoch,
        aps: &[AccessPoint],
        peers: &[Peer],
        personality: &Personality,
    ) -> Self {
        let mut f = Self::new();

        // AP histogram (13): count APs per channel 1-13
        // STA histogram (13): count stations per channel 1-13
        for ap in aps {
            let ch = ap.channel;
            if ch >= 1 && ch <= 13 {
                let idx = (ch as usize) - 1;
                f.ap_histogram[idx] += 1.0;
                f.sta_histogram[idx] += ap.clients.len() as f32;
            }
        }

        // Peer histogram (13): count peers per channel 1-13
        for peer in peers {
            let ch = peer.last_channel;
            if ch >= 1 && ch <= 13 {
                let idx = (ch as usize) - 1;
                f.peer_histogram[idx] += 1.0;
            }
        }

        // Epoch stats (10):
        let es = &mut f.epoch_stats;
        // [0] inactive_epochs (normalized to 0-1 by dividing by 100)
        es[0] = (epoch.inactive_epochs as f32 / 100.0).min(1.0);
        // [1] active_epochs (normalized)
        es[1] = (epoch.active_epochs as f32 / 100.0).min(1.0);
        // [2] blind_epochs (normalized by max_inactive_scale)
        es[2] = (epoch.blind_epochs as f32 / personality.max_inactive_scale as f32).min(1.0);
        // [3] aps_seen (normalized by 20)
        es[3] = (epoch.aps_seen as f32 / 20.0).min(1.0);
        // [4] handshakes_captured (normalized by 10)
        es[4] = (epoch.handshakes_captured as f32 / 10.0).min(1.0);
        // [5] deauths_sent (normalized by 10)
        es[5] = (epoch.deauths_sent as f32 / 10.0).min(1.0);
        // [6] total_bond_factor (normalized by 20000)
        es[6] = (epoch.total_bond_factor as f32 / 20000.0).min(1.0);
        // [7] any_activity (binary)
        es[7] = if epoch.any_activity { 1.0 } else { 0.0 };
        // [8] channel_hops (normalized by 20)
        es[8] = (epoch.channel_hops as f32 / 20.0).min(1.0);
        // [9] cpu_load (clamped 0-1, it's already a fraction)
        es[9] = epoch.cpu_load.max(0.0).min(1.0);

        f
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwncore::epoch::Epoch;
    use pwncore::personality::Personality;

    #[test]
    fn test_features_dimension() {
        let f = Features::new();
        assert_eq!(f.as_slice().len(), 49);
    }

    #[test]
    fn test_from_epoch_produces_49() {
        let epoch = Epoch::new(1);
        let aps = vec![];
        let peers = vec![];
        let p = Personality::default();
        let f = Features::from_epoch(&epoch, &aps, &peers, &p);
        assert_eq!(f.as_slice().len(), 49);
    }
}
