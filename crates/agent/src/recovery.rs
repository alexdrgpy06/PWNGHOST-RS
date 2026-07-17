//! Recovery and persistence for agent state

use anyhow::Result;
use chrono::{DateTime, Utc};
use pwncore::Mood;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, info};

/// Recovery state for persistence across reboots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub epoch: u64,
    pub total_epochs: u64,
    pub total_handshakes: u32,
    pub total_pmkids: u32,
    pub xp: u32,
    pub level: u32,
    pub last_channel: u8,
    pub last_mood: Mood,
    pub last_face: String,
    pub uptime_seconds: u64,
    pub started_at: DateTime<Utc>,
    pub last_saved: DateTime<Utc>,
    /// Per-AP bond encounter counts, keyed by hex-encoded BSSID (e.g.
    /// "aabbccddeeff") rather than `[u8; 6]` directly -- JSON object keys
    /// must be strings, so `serde_json` cannot serialize a `HashMap` keyed
    /// by a byte array (confirmed the hard way: `save()` panicked with
    /// "key must be a string" the first time this map was ever actually
    /// populated before a save, since no prior code path exercised that).
    /// See `mac_to_hex`/`hex_to_mac` for the conversion to/from
    /// `Personality`'s own `HashMap<[u8; 6], u32>`.
    pub encounters: std::collections::HashMap<String, u32>,
    /// Opaque, policy-owned learned state (see `rl_agent::Policy::export_state`/
    /// `import_state`) -- e.g. the online-learning bandit's Q-values and
    /// exploration rate. `None` if no RL agent is loaded, or the loaded
    /// policy has nothing to persist (heuristic/model policies).
    /// `#[serde(default)]` so recovery files saved before this field
    /// existed still load correctly.
    #[serde(default)]
    pub rl_policy_state: Option<Vec<u8>>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            epoch: 0,
            total_epochs: 0,
            total_handshakes: 0,
            total_pmkids: 0,
            xp: 0,
            level: 0,
            last_channel: 1,
            last_mood: Mood::Awake,
            last_face: "(◕‿‿◕)".to_string(),
            uptime_seconds: 0,
            started_at: Utc::now(),
            last_saved: Utc::now(),
            encounters: std::collections::HashMap::new(),
            rl_policy_state: None,
        }
    }
}

/// Hex-encode a BSSID for use as a JSON-safe map key (e.g.
/// `[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]` -> `"aabbccddeeff"`).
fn mac_to_hex(bssid: [u8; 6]) -> String {
    bssid.iter().map(|b| format!("{b:02x}")).collect()
}

/// Inverse of [`mac_to_hex`]. Returns `None` for anything that isn't
/// exactly 12 valid hex characters (e.g. a hand-edited or corrupted
/// recovery file) rather than panicking.
fn hex_to_mac(hex: &str) -> Option<[u8; 6]> {
    if hex.len() != 12 {
        return None;
    }
    let mut out = [0u8; 6];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Recovery manager for persisting agent state
pub struct RecoveryManager {
    state: RecoveryState,
    path: std::path::PathBuf,
    save_interval: std::time::Duration,
}

impl RecoveryManager {
    pub fn new(path: impl AsRef<Path>, save_interval_secs: u64) -> Self {
        Self {
            state: RecoveryState::default(),
            path: path.as_ref().to_path_buf(),
            save_interval: std::time::Duration::from_secs(save_interval_secs),
        }
    }

    /// Load recovery state from disk
    pub async fn load(&mut self) -> Result<()> {
        if self.path.exists() {
            let content = fs::read_to_string(&self.path).await?;
            self.state = serde_json::from_str(&content)?;
            info!(
                "Loaded recovery state from {:?} (epoch {})",
                self.path, self.state.epoch
            );
        }
        Ok(())
    }

    /// Save recovery state to disk
    pub async fn save(&mut self) -> Result<()> {
        self.state.last_saved = Utc::now();
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.path, content).await?;
        debug!("Saved recovery state to {:?}", self.path);
        Ok(())
    }

    /// Get current recovery state
    pub fn state(&self) -> &RecoveryState {
        &self.state
    }

    /// Get mutable recovery state
    pub fn state_mut(&mut self) -> &mut RecoveryState {
        &mut self.state
    }

    /// Pull the latest progress out of a live agent, ready to be `save()`d.
    /// Call this before every save -- `save()` itself only serializes
    /// whatever is already in `self.state`.
    pub fn update_from_agent(&mut self, agent: &crate::Agent) {
        self.state.epoch = agent.total_epochs();
        self.state.total_epochs = agent.epoch_tracker.total_epochs;
        self.state.total_handshakes = agent.personality.stats().handshakes;
        self.state.total_pmkids = agent.personality.stats().pmkids;
        self.state.xp = agent.personality.stats().xp;
        self.state.level = agent.personality.stats().level;
        self.state.last_channel = agent.current_channel();
        self.state.last_mood = agent.current_mood();
        self.state.last_face = agent.current_face().to_string();
        self.state.uptime_seconds = agent.start.elapsed().as_secs();
        self.state.started_at = agent.started_at;
        self.state.encounters = agent
            .personality
            .encounters()
            .iter()
            .map(|(bssid, count)| (mac_to_hex(*bssid), *count))
            .collect();
        self.state.rl_policy_state = agent
            .rl_agent
            .as_ref()
            .and_then(|rl| rl.try_read().ok())
            .and_then(|guard| guard.export_policy_state());
    }

    /// Apply previously loaded state onto a freshly constructed agent, so
    /// progress (xp/level/handshake+pmkid counts, per-AP bond encounters,
    /// the RL policy's learned values) survives a reboot instead of
    /// resetting to zero every power cycle. Call once at startup, after
    /// `load()` and before the agent starts ticking.
    pub fn apply_to_agent(&self, agent: &mut crate::Agent) {
        agent.epoch_tracker.total_epochs = self.state.total_epochs;
        let encounters = self
            .state
            .encounters
            .iter()
            .filter_map(|(hex, count)| hex_to_mac(hex).map(|bssid| (bssid, *count)))
            .collect();
        agent.personality.restore(
            self.state.xp,
            self.state.level,
            self.state.total_handshakes,
            self.state.total_pmkids,
            encounters,
        );
        if let Some(data) = &self.state.rl_policy_state {
            if let Some(rl) = agent.rl_agent.as_ref() {
                if let Ok(mut guard) = rl.try_write() {
                    guard.import_policy_state(data);
                }
            }
        }
        info!(
            "Restored progress from recovery state: epoch {} xp {} level {}",
            self.state.total_epochs, self.state.xp, self.state.level
        );
    }

    /// The configured save interval, for callers that want to drive their
    /// own periodic-save loop (main.rs's tick loop already has the live
    /// `Agent` this crate deliberately doesn't hold a reference to).
    pub fn save_interval(&self) -> std::time::Duration {
        self.save_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_mac_hex_roundtrip() {
        let bssid = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        assert_eq!(mac_to_hex(bssid), "aabbccddeeff");
        assert_eq!(hex_to_mac("aabbccddeeff"), Some(bssid));
    }

    #[test]
    fn test_hex_to_mac_rejects_malformed_input() {
        assert_eq!(hex_to_mac("tooshort"), None);
        assert_eq!(hex_to_mac("not-hex-chars"), None);
        assert_eq!(hex_to_mac(""), None);
    }

    #[test]
    fn test_recovery_state_with_encounters_is_json_serializable() {
        // Regression test: serde_json can't serialize a HashMap keyed by a
        // non-string type ("key must be a string"), which is exactly what
        // broke here before encounters moved to hex-string keys -- and it
        // only ever surfaced once a real, non-empty encounters map was
        // serialized (an empty map has no keys to fail on).
        let mut state = RecoveryState::default();
        state.encounters.insert(mac_to_hex([1, 2, 3, 4, 5, 6]), 3);
        serde_json::to_string_pretty(&state)
            .expect("must serialize with a populated encounters map");
    }

    #[tokio::test]
    async fn test_recovery_manager() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("recovery.json");

        let mut mgr = RecoveryManager::new(&path, 60);
        assert_eq!(mgr.state.epoch, 0);

        mgr.state.epoch = 42;
        mgr.save().await.unwrap();

        let mut mgr2 = RecoveryManager::new(&path, 60);
        mgr2.load().await.unwrap();
        assert_eq!(mgr2.state.epoch, 42);
    }

    #[test]
    fn test_update_from_agent_then_apply_to_fresh_agent_restores_progress() {
        use crate::Agent;
        use mac_addr::MacAddr;

        // Build up real progress on one agent (a captured handshake, so
        // personality xp/handshakes/encounters and (if loaded) the RL
        // policy all have something real to persist).
        let mut source = Agent::default();
        source.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap =
            pwncore::AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        source.update_aps(vec![ap]);
        source.mark_handshake_captured(bssid);

        let mut mgr = RecoveryManager::new("unused-in-this-test.json", 60);
        mgr.update_from_agent(&source);
        assert!(mgr.state().xp > 0);
        assert_eq!(mgr.state().total_handshakes, 1);
        assert_eq!(
            *mgr.state()
                .encounters
                .get(&mac_to_hex(bssid.octets()))
                .unwrap(),
            1
        );

        // A brand new agent starts at zero...
        let mut restored = Agent::default();
        assert_eq!(restored.personality.stats().xp, 0);

        // ...but after applying the saved state, its progress matches what
        // was captured on the source agent.
        mgr.apply_to_agent(&mut restored);
        assert_eq!(restored.personality.stats().xp, mgr.state().xp);
        assert_eq!(restored.personality.stats().handshakes, 1);
        assert_eq!(restored.personality.encounters_for(&bssid.octets()), 1);
        assert_eq!(
            restored.epoch_tracker.total_epochs,
            mgr.state().total_epochs
        );
    }

    #[tokio::test]
    async fn test_full_save_load_apply_roundtrip() {
        use crate::Agent;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("recovery.json");

        let mut source = Agent::default();
        source.start();
        let bssid: mac_addr::MacAddr = "11:22:33:44:55:66".parse().unwrap();
        let ap = pwncore::AccessPoint::new(
            bssid,
            6,
            -40,
            pwncore::EncryptionType::Wpa2,
            "roundtrip".into(),
        );
        source.update_aps(vec![ap]);
        source.mark_handshake_captured(bssid);

        let mut save_mgr = RecoveryManager::new(&path, 60);
        save_mgr.update_from_agent(&source);
        save_mgr.save().await.unwrap();

        let mut load_mgr = RecoveryManager::new(&path, 60);
        load_mgr.load().await.unwrap();

        let mut restored = Agent::default();
        load_mgr.apply_to_agent(&mut restored);
        assert_eq!(
            restored.personality.stats().xp,
            source.personality.stats().xp
        );
        assert_eq!(restored.personality.encounters_for(&bssid.octets()), 1);
    }
}
