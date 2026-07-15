//! Peer pwnagotchi (mesh networking)

use crate::Mood;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::MacAddr;
use std::time::Duration;

/// Peer pwnagotchi unit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Peer {
    pub mac: MacAddr,
    pub name: String,
    pub last_seen: DateTime<Utc>,
    pub epochs_since_seen: u64,
    pub handshakes_shared: u32,
    pub mood: Mood,
    pub channel: u8,
    pub signal: i16,
    pub version: String,
}

impl Peer {
    pub fn new(mac: MacAddr, name: String, channel: u8, signal: i16) -> Self {
        Self {
            mac,
            name,
            last_seen: Utc::now(),
            epochs_since_seen: 0,
            handshakes_shared: 0,
            mood: Mood::Friend,
            channel,
            signal,
            version: String::new(),
        }
    }

    pub fn update_seen(&mut self) {
        self.last_seen = Utc::now();
        self.epochs_since_seen = 0;
    }

    pub fn increment_epoch(&mut self) {
        self.epochs_since_seen += 1;
    }

    pub fn is_stale(&self, max_epochs: u64) -> bool {
        self.epochs_since_seen > max_epochs
    }

    pub fn age(&self) -> Duration {
        (Utc::now() - self.last_seen).to_std().unwrap_or_default()
    }
}

/// Peer manager for mesh networking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerManager {
    peers: HashMap<MacAddr, Peer>,
    max_stale_epochs: u64,
}

impl PeerManager {
    pub fn new(max_stale_epochs: u64) -> Self {
        Self {
            peers: HashMap::new(),
            max_stale_epochs,
        }
    }

    pub fn add_or_update(&mut self, peer: Peer) {
        self.peers.insert(peer.mac, peer);
    }

    pub fn remove(&mut self, mac: &MacAddr) -> Option<Peer> {
        self.peers.remove(mac)
    }

    pub fn get(&self, mac: &MacAddr) -> Option<&Peer> {
        self.peers.get(mac)
    }

    pub fn get_mut(&mut self, mac: &MacAddr) -> Option<&mut Peer> {
        self.peers.get_mut(mac)
    }

    pub fn all(&self) -> Vec<&Peer> {
        self.peers.values().collect()
    }

    pub fn active(&self) -> Vec<&Peer> {
        self.peers.values().filter(|p| !p.is_stale(self.max_stale_epochs)).collect()
    }

    pub fn count(&self) -> usize {
        self.peers.len()
    }

    pub fn active_count(&self) -> usize {
        self.active().len()
    }

    pub fn increment_epochs(&mut self) {
        for peer in self.peers.values_mut() {
            peer.increment_epoch();
        }
    }

    pub fn cleanup_stale(&mut self) -> Vec<Peer> {
        let stale: Vec<_> = self
            .peers
            .drain_filter(|_, p| p.is_stale(self.max_stale_epochs))
            .map(|(_, p)| p)
            .collect();
        stale
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new(50) // 50 epochs ~ 25 minutes at 30s/epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_peer_new() {
        let mac = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let peer = Peer::new(mac, "TestPeer".to_string(), 6, -50);

        assert_eq!(peer.mac, mac);
        assert_eq!(peer.name, "TestPeer");
        assert_eq!(peer.channel, 6);
        assert_eq!(peer.signal, -50);
        assert_eq!(peer.mood, Mood::Friend);
        assert_eq!(peer.epochs_since_seen, 0);
    }

    #[test]
    fn test_peer_stale() {
        let mac = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let mut peer = Peer::new(mac, "TestPeer".to_string(), 6, -50);

        assert!(!peer.is_stale(10));

        for _ in 0..11 {
            peer.increment_epoch();
        }

        assert!(peer.is_stale(10));
    }

    #[test]
    fn test_peer_manager() {
        let mut mgr = PeerManager::new(10);
        let mac1 = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        let mac2 = MacAddr::from_str("11:22:33:44:55:66").unwrap();

        let peer1 = Peer::new(mac1, "Peer1".to_string(), 6, -50);
        let peer2 = Peer::new(mac2, "Peer2".to_string(), 1, -60);

        mgr.add_or_update(peer1);
        mgr.add_or_update(peer2);

        assert_eq!(mgr.count(), 2);
        assert_eq!(mgr.active_count(), 2);

        mgr.get_mut(&mac1).unwrap().increment_epoch();
        for _ in 0..10 {
            mgr.increment_epochs();
        }

        assert_eq!(mgr.active_count(), 1); // peer1 should be stale
    }
}