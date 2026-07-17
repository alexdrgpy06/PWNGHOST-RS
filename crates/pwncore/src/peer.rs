//! Peer types for mesh communication

use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::handshake::GpsData;

/// Peer pwnagotchi unit (mesh network)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Peer {
    pub id: Uuid,
    pub mac: MacAddr,
    pub name: String,
    pub last_seen: DateTime<Utc>,
    pub last_channel: u8,
    pub encounters: u32,
    pub bond_factor: f64,
    pub firmware_version: Option<String>,
    pub capabilities: Vec<String>,
    pub gps: Option<GpsData>,
}

impl Peer {
    pub fn new(mac: MacAddr, name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            mac,
            name,
            last_seen: Utc::now(),
            last_channel: 0,
            encounters: 1,
            bond_factor: 1.0,
            firmware_version: None,
            capabilities: Vec::new(),
            gps: None,
        }
    }

    pub fn encounter(&mut self, channel: u8) {
        self.encounters += 1;
        self.last_seen = Utc::now();
        self.last_channel = channel;
        self.bond_factor = (self.encounters as f64).sqrt(); // Simple bond calc
    }

    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        Utc::now().signed_duration_since(self.last_seen) > max_age
    }

    pub fn display_name(&self) -> String {
        if self.name.is_empty() {
            self.mac.to_string()
        } else {
            self.name.clone()
        }
    }
}

/// Peer manager for mesh network
#[derive(Debug, Default)]
pub struct PeerManager {
    pub peers: Vec<Peer>,
    pub max_peers: usize,
    pub ttl: chrono::Duration,
}

impl PeerManager {
    pub fn new(max_peers: usize, ttl_seconds: u64) -> Self {
        Self {
            peers: Vec::new(),
            max_peers,
            ttl: chrono::Duration::seconds(ttl_seconds as i64),
        }
    }

    pub fn add_or_update(&mut self, peer: Peer) {
        // Remove stale peers first
        self.prune_stale();

        // Check if peer exists
        if let Some(existing) = self.peers.iter_mut().find(|p| p.mac == peer.mac) {
            existing.encounter(peer.last_channel);
            existing.last_seen = peer.last_seen;
            existing.firmware_version = peer.firmware_version.or(existing.firmware_version.clone());
            existing.capabilities = peer.capabilities;
            existing.gps = peer.gps.or(existing.gps.clone());
        } else {
            // Add new peer
            if self.peers.len() >= self.max_peers {
                // Remove oldest stale peer
                if let Some(oldest_idx) = self.peers.iter().enumerate()
                    .max_by_key(|(_, p)| p.last_seen)
                    .map(|(i, _)| i) 
                {
                    self.peers.remove(oldest_idx);
                }
            }
            self.peers.push(peer);
        }
    }

    pub fn prune_stale(&mut self) {
        let now = Utc::now();
        self.peers.retain(|p| {
            now.signed_duration_since(p.last_seen) <= self.ttl
        });
    }

    pub fn get_peer(&self, mac: &mac_addr::MacAddr) -> Option<&Peer> {
        self.peers.iter().find(|p| p.mac == *mac)
    }

    pub fn get_peer_mut(&mut self, mac: &mac_addr::MacAddr) -> Option<&mut Peer> {
        self.peers.iter_mut().find(|p| p.mac == *mac)
    }

    pub fn total_bond_factor(&self) -> f64 {
        self.peers.iter().map(|p| p.bond_factor).sum()
    }

    pub fn avg_bond_factor(&self) -> f64 {
        if self.peers.is_empty() {
            0.0
        } else {
            self.total_bond_factor() / self.peers.len() as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_creation() {
        let mac = mac_addr::MacAddr::from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let peer = Peer::new(mac, "TestPeer".to_string());
        
        assert_eq!(peer.mac, mac);
        assert_eq!(peer.name, "TestPeer");
        assert_eq!(peer.encounters, 1);
    }

    #[test]
    fn test_peer_manager() {
        let mut mgr = PeerManager::new(10, 300);
        let mac = mac_addr::MacAddr::from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let peer = Peer::new(mac, "Peer1".to_string());
        
        mgr.add_or_update(peer);
        assert_eq!(mgr.peers.len(), 1);
        assert_eq!(mgr.total_bond_factor(), 1.0);
    }
}