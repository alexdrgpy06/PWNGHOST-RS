//! Mesh networking for pwnagotchi peer communication

use anyhow::Result;
use pwncore::{Channel, MacAddr, Mood, Peer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Mesh peer information
#[derive(Debug, Clone)]
pub struct MeshPeer {
    pub peer: Peer,
    pub last_seen: Instant,
    pub signal_strength: i16,
    pub advertised_xp: u32,
    pub advertised_level: u32,
}

/// Mesh network manager
pub struct MeshManager {
    peers: Arc<RwLock<HashMap<MacAddr, MeshPeer>>>,
    max_age: Duration,
    our_mac: MacAddr,
    our_name: String,
}

impl MeshManager {
    pub fn new(our_mac: MacAddr, our_name: String) -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            max_age: Duration::from_secs(300), // 5 minutes
            our_mac,
            our_name,
        }
    }

    /// Update or add peer from mesh advertisement
    pub async fn update_peer(&self, peer: Peer, signal: i16, xp: u32, level: u32) {
        let mac = peer.mac;
        let mut peers = self.peers.write().await;
        
        peers.insert(mac, MeshPeer {
            peer,
            last_seen: Instant::now(),
            signal_strength: signal,
            advertised_xp: xp,
            advertised_level: level,
        });
    }

    /// Get all active peers
    pub async fn active_peers(&self) -> Vec<MeshPeer> {
        let peers = self.peers.read().await;
        let now = Instant::now();
        
        peers.values()
            .filter(|p| now.duration_since(p.last_seen) < self.max_age)
            .cloned()
            .collect()
    }

    /// Get peer count
    pub async fn peer_count(&self) -> usize {
        let peers = self.peers.read().await;
        let now = Instant::now();
        peers.values()
            .filter(|p| now.duration_since(p.last_seen) < self.max_age)
            .count()
    }

    /// Remove stale peers
    pub async fn cleanup_stale(&self) -> usize {
        let mut peers = self.peers.write().await;
        let now = Instant::now();
        let before = peers.len();
        
        peers.retain(|_, p| now.duration_since(p.last_seen) < self.max_age);
        
        before - peers.len()
    }

    /// Get peer by MAC
    pub async fn get_peer(&self, mac: MacAddr) -> Option<MeshPeer> {
        let peers = self.peers.read().await;
        peers.get(&mac).cloned()
    }

    /// Build mesh IE data for beacon/probe response
    pub fn build_mesh_ie(&self, epoch: u64, handshakes: u32, level: u32, xp: u32, mood: Mood, channel: Channel) -> Vec<u8> {
        // Mesh IE format:
        // Element ID: 221 (Vendor Specific)
        // Length: variable
        // OUI: 00:1A:2B (pwnagotchi OUI)
        // Type: 0x01 (mesh data)
        // Data: MAC(6) + Name(len+name) + Channel(1) + Mood(1) + Level(2) + XP(2) + Epoch(8) + Handshakes(2)
        
        let mut data = Vec::new();
        
        // Element ID
        data.push(221);
        
        // Length placeholder (will fill later)
        let len_pos = data.len();
        data.push(0);
        
        // OUI
        data.extend_from_slice(&[0x00, 0x1A, 0x2B]);
        
        // Type
        data.push(0x01);
        
        // MAC (6 bytes)
        // In real implementation, would get from interface
        data.extend_from_slice(&[0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);
        
        // Name length + name
        let name_bytes = self.our_name.as_bytes();
        if name_bytes.len() <= 32 {
            data.push(name_bytes.len() as u8);
            data.extend_from_slice(name_bytes);
        } else {
            data.push(32);
            data.extend_from_slice(&self.our_name.as_bytes()[..32]);
        }
        
        // Channel
        data.push(1); // placeholder
        
        // Mood
        data.push(mood as u8);
        
        // Level (2 bytes LE)
        data.extend_from_slice(&(level as u16).to_le_bytes());
        
        // XP (2 bytes LE)
        data.extend_from_slice(&(0u16).to_le_bytes()); // placeholder
        
        // Epoch (8 bytes LE)
        data.extend_from_slice(&0u64.to_le_bytes()); // placeholder
        
        // Handshakes (2 bytes LE)
        data.extend_from_slice(&(0u16).to_le_bytes()); // placeholder
        
        // Fix length
        data[len_pos] = (data.len() - len_pos - 1) as u8;
        
        data
    }

    /// Parse mesh IE from beacon/probe response
    pub fn parse_mesh_ie(data: &[u8]) -> Result<Option<MeshPeerInfo>> {
        if data.len() < 8 {
            return Ok(None);
        }

        // Check Element ID (221) and OUI (00:1A:2B)
        if data[0] != 221 {
            return Ok(None);
        }
        
        let len = data[1] as usize;
        if data.len() < 2 + len {
            return Ok(None);
        }
        
        if data[2..5] != [0x00, 0x1A, 0x2B] {
            return Ok(None);
        }
        
        // Check type
        if data[5] != 0x01 {
            return Ok(None);
        }
        
        let mut offset = 6;
        
        // MAC (6 bytes)
        if offset + 6 > data.len() {
            return Ok(None);
        }
        let mac = MacAddr::from_bytes(&data[offset..offset+6].try_into().unwrap());
        offset += 6;
        
        // Name length + name
        if offset >= data.len() {
            return Ok(None);
        }
        let name_len = data[offset] as usize;
        offset += 1;
        
        if offset + name_len > data.len() {
            return Ok(None);
        }
        let name = String::from_utf8_lossy(&data[offset..offset+name_len]).to_string();
        offset += name_len;
        
        // Channel
        if offset >= data.len() {
            return Ok(None);
        }
        let channel = data[offset];
        offset += 1;
        
        // Mood
        if offset >= data.len() {
            return Ok(None);
        }
        let mood = match data[offset] {
            0 => Mood::LookR,
            1 => Mood::LookL,
            2 => Mood::LookRHappy,
            3 => Mood::LookLHappy,
            4 => Mood::Sleep,
            5 => Mood::Awake,
            6 => Mood::Bored,
            7 => Mood::Intense,
            8 => Mood::Cool,
            8 => Mood::Happy,
            10 => Mood::Excited,
            11 => Mood::Grateful,
            12 => Mood::Motivated,
            13 => Mood::Demotivated,
            14 => Mood::Smart,
            15 => Mood::Lonely,
            16 => Mood::Sad,
            17 => Mood::Angry,
            18 => Mood::Friend,
            19 => Mood::Broken,
            20 => Mood::Upload,
            21 => Mood::Smart,
            _ => Mood::LookR,
        };
        offset += 1;
        
        // Level (2 bytes LE)
        if offset + 2 > data.len() {
            return Ok(None);
        }
        let level = u16::from_le_bytes([data[offset], data[offset+1]]) as u32;
        offset += 2;
        
        // XP (2 bytes LE)
        if offset + 2 > data.len() {
            return Ok(None);
        }
        let xp = u16::from_le_bytes([data[offset], data[offset+1]]) as u32;
        offset += 2;
        
        // Epoch (8 bytes LE)
        if offset + 8 > data.len() {
            return Ok(None);
        }
        let epoch = u64::from_le_bytes([
            data[offset], data[offset+1], data[offset+2], data[offset+3],
            data[offset+4], data[offset+5], data[offset+6], data[offset+7],
        ]);
        offset += 8;
        
        // Handshakes (2 bytes LE)
        if offset + 2 > data.len() {
            return Ok(None);
        }
        let handshakes = u16::from_le_bytes([data[offset], data[offset+1]]) as u32;

        Ok(Some(MeshPeerInfo {
            mac,
            name,
            channel,
            mood,
            level,
            xp,
            epoch,
            handshakes,
        }))
    }
}

/// Parsed mesh peer info
#[derive(Debug, Clone)]
pub struct MeshPeerInfo {
    pub mac: MacAddr,
    pub name: String,
    pub channel: u8,
    pub mood: Mood,
    pub level: u32,
    pub xp: u32,
    pub epoch: u64,
    pub handshakes: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mesh_manager_new() {
        let mac = MacAddr::new([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]);
        let mgr = MeshManager::new(mac, "test".to_string());
        // Just verify it creates
    }

    #[test]
    fn test_mesh_peer_info() {
        // Test MeshPeerInfo structure
    }
}