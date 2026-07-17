//! Channel and handshake types

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// WiFi channel (2.4GHz: 1-14)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Channel(pub u8);

impl Channel {
    pub fn new(channel: u8) -> Result<Self> {
        if (1..=14).contains(&channel) {
            Ok(Channel(channel))
        } else {
            anyhow::bail!("Invalid 2.4GHz channel: {}", channel)
        }
    }

    pub fn is_non_overlapping(&self) -> bool {
        matches!(self.0, 1 | 6 | 11)
    }

    pub fn frequency_mhz(&self) -> u32 {
        match self.0 {
            1..=13 => 2412 + (self.0 - 1) as u32 * 5,
            14 => 2484,
            _ => 0,
        }
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u8> for Channel {
    fn from(c: u8) -> Self {
        Channel(c)
    }
}

impl From<Channel> for u8 {
    fn from(c: Channel) -> Self {
        c.0
    }
}

/// All 2.4GHz channels
pub const ALL_CHANNELS: [Channel; 13] = [
    Channel(1), Channel(2), Channel(3), Channel(4), Channel(5),
    Channel(6), Channel(7), Channel(8), Channel(9), Channel(10),
    Channel(11), Channel(12), Channel(13),
];

/// Non-overlapping channels (1, 6, 11)
pub const NON_OVERLAPPING: [Channel; 3] = [Channel(1), Channel(6), Channel(11)];

/// Channel set for scanning
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelSet {
    pub channels: Vec<Channel>,
    pub current: Option<Channel>,
    pub dwell_time: u64,
}

impl ChannelSet {
    pub fn new(channels: Vec<Channel>) -> Self {
        Self {
            channels,
            current: None,
            dwell_time: 2000,
        }
    }

    pub fn non_overlapping() -> Self {
        Self::new(NON_OVERLAPPING.to_vec())
    }

    pub fn all() -> Self {
        Self::new(ALL_CHANNELS.to_vec())
    }

    pub fn with_dwell(mut self, ms: u64) -> Self {
        self.dwell_time = ms;
        self
    }

    pub fn next_channel(&mut self) -> Option<Channel> {
        let current_idx = self.current.and_then(|c| {
            self.channels.iter().position(|&ch| ch == c)
        });

        let next_idx = match current_idx {
            Some(idx) if idx + 1 < self.channels.len() => idx + 1,
            _ => 0,
        };

        if next_idx < self.channels.len() {
            self.current = Some(self.channels[next_idx]);
            self.current
        } else {
            None
        }
    }

    pub fn set_current(&mut self, channel: Channel) {
        if self.channels.contains(&channel) {
            self.current = Some(channel);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_frequency() {
        assert_eq!(Channel(1).frequency_mhz(), 2412);
        assert_eq!(Channel(6).frequency_mhz(), 2437);
        assert_eq!(Channel(11).frequency_mhz(), 2462);
        assert_eq!(Channel(14).frequency_mhz(), 2484);
    }

    #[test]
    fn test_non_overlapping() {
        assert!(Channel(1).is_non_overlapping());
        assert!(Channel(6).is_non_overlapping());
        assert!(Channel(11).is_non_overlapping());
        assert!(!Channel(3).is_non_overlapping());
    }

    #[test]
    fn test_channel_set() {
        let mut set = ChannelSet::non_overlapping();
        assert_eq!(set.channels.len(), 3);
        
        let next = set.next_channel();
        assert_eq!(next, Some(Channel(1)));
        
        let next = set.next_channel();
        assert_eq!(next, Some(Channel(6)));
    }
}