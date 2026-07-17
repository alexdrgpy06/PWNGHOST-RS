use mac_addr::MacAddr;

#[derive(Debug, Clone)]
pub struct Peer {
    pub mac: MacAddr,
    pub last_seen: u64,
}

pub struct MeshManager {
    peers: Vec<Peer>,
}

impl MeshManager {
    pub fn new() -> Self {
        Self { peers: Vec::new() }
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.peers.push(peer);
    }

    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }
}

impl Default for MeshManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_mesh_add_peer() {
        let mut mesh = MeshManager::new();
        let mac = MacAddr::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        mesh.add_peer(Peer { mac, last_seen: 123 });
        assert_eq!(mesh.peers().len(), 1);
    }
}
