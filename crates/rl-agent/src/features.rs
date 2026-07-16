//! Feature extraction for the RL agent.
//!
//! `Features` is the 49-dimensional observation the policy consumes. It is a
//! plain data container so the `agent` crate (which owns the live wifi state)
//! can populate it without creating a dependency cycle back into this crate.

use ndarray::Array1;

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

    /// Normalize each histogram in place to the [0, 1] range by its max.
    pub fn normalize(&mut self) {
        for hist in [
            &mut self.ap_histogram,
            &mut self.sta_histogram,
            &mut self.peer_histogram,
        ] {
            let max = hist.iter().cloned().fold(0.0, f32::max);
            if max > 0.0 {
                *hist /= max;
            }
        }
    }

    /// Get the feature vector as a flat 49-element array.
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
    fn test_features_as_flat() {
        let f = Features::new();
        let flat = f.as_flat();
        assert_eq!(flat.len(), 49);
    }

    #[test]
    fn test_normalize() {
        let mut f = Features::new();
        f.ap_histogram[0] = 2.0;
        f.ap_histogram[1] = 4.0;
        f.normalize();
        assert_eq!(f.ap_histogram[1], 1.0);
        assert_eq!(f.ap_histogram[0], 0.5);
    }
}
