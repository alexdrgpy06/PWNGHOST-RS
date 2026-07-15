//! Channel types and constants

use crate::Channel;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Set of channels for scanning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelSet {
    channels: HashSet<u8>,
}

impl ChannelSet {
    pub fn new(channels: &[u8]) -> Result<Self> {
        let mut set = HashSet::new();
        for ch in channels {
            anyhow::ensure!((1..=14).contains(ch), "Invalid channel: {}", ch);
            set.insert(*ch);
        }
        Ok(Self { channels: set })
    }

    pub fn all() -> Self {
        Self::new(&(1..=14).collect::<Vec<_>>()).unwrap()
    }

    pub fn non_overlapping() -> Self {
        Self::new(&[1, 6, 11]).unwrap()
    }

    pub fn contains(&self, ch: u8) -> bool {
        self.channels.contains(&ch)
    }

    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        self.channels.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}

impl Default for ChannelSet {
    fn default() -> Self {
        Self::non_overlapping()
    }
}

/// All 2.4 GHz channels
pub const ALL_CHANNELS: [Channel; 14] = [
    Channel(1), Channel(2), Channel(3), Channel(4), Channel(5), Channel(6),
    Channel(7), Channel(8), Channel(9), Channel(10), Channel(11), Channel(12),
    Channel(13), Channel(14),
];

/// Non-overlapping 2.4 GHz channels (1, 6, 11)
pub const NON_OVERLAPPING: [Channel; 3] = [Channel(1), Channel(6), Channel(11)];

/// Channel hopping sequence (prioritizes non-overlapping, then fills gaps)
pub const HOP_SEQUENCE: [u8; 13] = [1, 6, 11, 2, 7, 3, 8, 4, 9, 5, 10, 12, 13];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_set_new() {
        let set = ChannelSet::new(&[1, 6, 11]).unwrap();
        assert!(set.contains(1));
        assert!(set.contains(6));
        assert!(set.contains(11));
        assert!(!set.contains(2));
    }

    #[test]
    fn test_channel_set_invalid() {
        assert!(ChannelSet::new(&[0]).is_err());
        assert!(ChannelSet::new(&[15]).is_err());
    }

    #[test]
    fn test_non_overlapping() {
        let set = ChannelSet::non_overlapping();
        assert_eq!(set.len(), 3);
        assert!(set.contains(1));
        assert!(set.contains(6));
        assert!(set.contains(11));
    }

    #[test]
    fn test_hop_sequence() {
        // First 3 should be non-overlapping
        assert_eq!(&HOP_SEQUENCE[0..3], &[1, 6, 11]);
        // All channels 1-13 should be present
        let mut seen = [false; 14];
        for ch in HOP_SEQUENCE {
            seen[ch as usize] = true;
        }
        for ch in 1..=13 {
            assert!(seen[ch], "Channel {} missing from hop sequence", ch);
        }
    }
}