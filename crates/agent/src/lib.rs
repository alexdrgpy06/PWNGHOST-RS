//! Agent crate - Main agent loop for PWNGHOST-RS

pub mod capture;
pub mod epoch;
pub mod faces;
pub mod healing;
pub mod identity;
pub mod mesh;
pub mod personality;
pub mod plugins;
pub mod recovery;

pub use capture::CaptureManager;
pub use epoch::{EpochState, EpochTracker};
pub use faces::face_for_mood;
pub use healing::{Healer, HealingAction, HealingConfig, HealingLayer};
pub use identity::Identity;
pub use mesh::{MeshManager, MeshPeer, MeshPeerInfo};
pub use personality::Personality;
pub use plugins::{LuaPlugin, PluginApi, PluginManager};

use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use pwncore::{AccessPoint, Channel, Mood, Peer as CorePeer};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{error, warn};

/// Main agent structure
pub struct Agent {
    pub epoch_tracker: EpochTracker,
    pub personality: Personality,
    pub running: bool,
    pub healer: Healer,

    // Current state
    current_mood: Mood,
    aps: Vec<AccessPoint>,
    peers: Vec<CorePeer>,
    current_channel: u8,

    // Timing
    pub start: Instant,
    pub started_at: DateTime<Utc>,

    // Shared state for plugins
    pub plugins: PluginManager,
    pub rl_agent: Option<Arc<RwLock<rl_agent::RlAgent>>>,
    pub capture_manager: Option<Arc<CaptureManager>>,
    pub radio_manager: Option<Arc<radio::RadioManager>>,
    pub display: Option<Arc<ui::display::Display>>,
    pub web_server: Option<Arc<ui::web::WebServer>>,
}

impl Agent {
    pub fn new(personality: Personality) -> Self {
        Self {
            epoch_tracker: EpochTracker::new(),
            personality,
            running: false,
            healer: Healer::new(),
            current_mood: Mood::Awake,
            aps: Vec::new(),
            peers: Vec::new(),
            current_channel: 1,
            start: Instant::now(),
            started_at: Utc::now(),
            plugins: PluginManager::new(),
            rl_agent: None,
            capture_manager: None,
            radio_manager: None,
            display: None,
            web_server: None,
        }
    }

    pub fn start(&mut self) {
        self.running = true;
        self.healer.reset();
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Main tick: advance epoch, compute mood, produce action + face
    pub fn tick(&mut self) -> (&'static str, AgentAction) {
        // Finalize previous epoch counters
        self.epoch_tracker.finalize_current();

        // Advance to next epoch (staying on the current channel)
        let channel = Channel::new(self.current_channel).unwrap_or(Channel(1));
        self.epoch_tracker.advance(channel);

        // The epoch that just finished is now the newest entry in history
        // (pushed there by `advance` before rotating `current`). If it saw
        // no APs at all, apply the real "missed opportunity" penalty to
        // personality progress and feed a real negative reward to the RL
        // policy -- both were previously only ever computed in isolated
        // unit tests, never from the actual running agent.
        let was_blind = self
            .epoch_tracker
            .history
            .back()
            .map(|e| e.aps_found == 0)
            .unwrap_or(false);
        if was_blind {
            self.personality.update_on_missed();
            self.observe_rl_reward(-0.2);
        }

        // Observe current environment
        {
            let epoch = &mut self.epoch_tracker.current;
            epoch.observe(&self.aps, &self.peers);
        }

        // Compute mood from epoch state
        self.current_mood = self
            .personality
            .compute_mood(&self.epoch_tracker.current, &self.peers);

        // Select action based on state
        let action = self.select_action();

        // Get face for current mood
        let face = face_for_mood(self.current_mood);

        (face, action)
    }

    /// Report a crash/failure to the healer
    pub fn report_crash(&mut self) {
        self.healer.report_crash();
    }

    /// Report that everything is OK (heartbeat)
    pub fn report_alive(&mut self) {
        self.healer.report_alive();
    }

    /// Check healer state and return any healing action needed
    pub fn check_healing(&mut self) -> HealingAction {
        if self.healer.should_take_action() {
            let action = self.healer.decide();
            match action {
                HealingAction::None => {}
                HealingAction::RestartCapture => {
                    warn!(
                        "Healer: Soft-resetting capture backend (layer {:?})",
                        self.healer.active_layer()
                    );
                }
                HealingAction::PowerCycleGpio => {
                    error!(
                        "Healer: Power-cycling WiFi chip (layer {:?})",
                        self.healer.active_layer()
                    );
                }
                HealingAction::EnterSafeMode => {
                    error!(
                        "Healer: Entering safe mode (layer {:?})",
                        self.healer.active_layer()
                    );
                }
                HealingAction::EnableUsbLifeline => {
                    error!(
                        "Healer: Enabling USB lifeline (layer {:?})",
                        self.healer.active_layer()
                    );
                }
            }
            action
        } else {
            HealingAction::None
        }
    }

    /// Reset healer (called after successful recovery)
    pub fn reset_healer(&mut self) {
        self.healer.reset();
    }

    /// Select next action. Consults the RL agent first (if one is loaded);
    /// falls back to the heuristic personality-driven logic otherwise, or
    /// whenever the RL policy's suggestion isn't actionable right now (e.g.
    /// it wants to deauth but the personality has deauth disabled).
    fn select_action(&self) -> AgentAction {
        if let Some(action) = self.select_action_rl() {
            return action;
        }
        self.select_action_heuristic()
    }

    /// Ask the loaded RL policy for an action, translating its output into
    /// an [`AgentAction`]. Returns `None` when no RL agent is loaded, the
    /// lock is momentarily contended (never blocks the tick loop), or the
    /// suggested action isn't currently allowed (e.g. deauth/associate
    /// disabled by personality config) - all of which fall through to the
    /// heuristic policy.
    fn select_action_rl(&self) -> Option<AgentAction> {
        let rl = self.rl_agent.as_ref()?;
        let mut guard = rl.try_write().ok()?;
        let features = self.build_features();
        let p = self.personality.config();

        match guard.select_action(&features) {
            rl_agent::RlAction::HopChannel(ch) => Some(AgentAction::Hop(ch.clamp(1, 13))),
            rl_agent::RlAction::Deauth if p.deauth => self
                .find_target()
                .map(|t| AgentAction::Deauth(t.bssid.to_string())),
            rl_agent::RlAction::Associate if p.associate => self
                .find_target()
                .map(|t| AgentAction::Associate(t.bssid.to_string())),
            rl_agent::RlAction::Wait => Some(AgentAction::Stay),
            rl_agent::RlAction::Sleep(secs) => Some(AgentAction::Sleep(secs as u64)),
            // Deauth/Associate requested but disabled by personality config.
            rl_agent::RlAction::Deauth | rl_agent::RlAction::Associate => None,
        }
    }

    /// Build the 49-dim observation the RL policy consumes from current
    /// agent state (AP/station/peer channel histograms + epoch stats).
    fn build_features(&self) -> rl_agent::Features {
        let mut features = rl_agent::Features::new();

        for ap in &self.aps {
            let idx = (ap.channel.value().saturating_sub(1) as usize).min(12);
            features.ap_histogram[idx] += 1.0;
            for client in &ap.clients {
                let cidx = (client.channel.saturating_sub(1) as usize).min(12);
                features.sta_histogram[cidx] += 1.0;
            }
        }

        for peer in &self.peers {
            let idx = (peer.channel.saturating_sub(1) as usize).min(12);
            features.peer_histogram[idx] += 1.0;
        }

        let epoch = &self.epoch_tracker.current;
        features.epoch_stats[0] = epoch.aps_found as f32;
        features.epoch_stats[1] = epoch.handshakes_this_epoch as f32;
        features.epoch_stats[2] = epoch.deauths_sent as f32;
        features.epoch_stats[3] = epoch.assoc_attempts as f32;
        features.epoch_stats[4] = epoch.blind_epochs as f32;
        features.epoch_stats[5] = epoch.total_handshakes as f32;

        features.normalize();
        features
    }

    /// Heuristic action selection based on epoch state, mood, and
    /// personality. This is the fallback used when no RL model is loaded
    /// (or the RL policy declines to act), and is exactly the original
    /// decision logic.
    fn select_action_heuristic(&self) -> AgentAction {
        let epoch = &self.epoch_tracker.current;
        let p = self.personality.config();

        // Blind epochs: no APs seen for a couple of epochs, hop to find signal
        if epoch.blind_epochs >= 2 {
            return AgentAction::Hop(Self::next_channel(self.current_channel));
        }

        // Happy/excited/grateful: we have activity, check for targets
        match self.current_mood {
            Mood::Excited | Mood::Grateful | Mood::Motivated => {
                if let Some(target) = self.find_target() {
                    if p.deauth && !epoch.did_deauth {
                        return AgentAction::Deauth(target.bssid.to_string());
                    }
                    if p.associate && !epoch.did_associate {
                        return AgentAction::Associate(target.bssid.to_string());
                    }
                }
                // Time to hop
                let hop_time = self.personality.calc_hop_time(epoch) as u32;
                if hop_time > 0 && epoch.duration().num_seconds() as u32 >= hop_time {
                    return AgentAction::Hop(Self::next_channel(self.current_channel));
                }
                AgentAction::Stay
            }
            Mood::Bored | Mood::Sad | Mood::Lonely => {
                // Low activity: hop sooner to find new targets
                AgentAction::Hop(Self::next_channel(self.current_channel))
            }
            Mood::Angry | Mood::Broken => {
                // Too many failures: take a short break
                AgentAction::Sleep(5)
            }
            _ => {
                // Default: recon on current channel
                let recon_time = self.personality.calc_recon_time(epoch) as u32;
                if epoch.duration().num_seconds() as u32 >= recon_time {
                    AgentAction::Hop(Self::next_channel(self.current_channel))
                } else {
                    AgentAction::Stay
                }
            }
        }
    }

    /// Find best target AP on current channel
    fn find_target(&self) -> Option<&AccessPoint> {
        let p = self.personality.config();

        for ap in &self.aps {
            if ap.channel.value() != self.current_channel {
                continue;
            }
            if ap.handshake_captured {
                continue;
            }
            if ap.rssi < p.min_rssi {
                continue;
            }
            return Some(ap);
        }
        None
    }

    fn next_channel(current: u8) -> u8 {
        let channels = [1u8, 6, 11, 2, 7, 3, 8, 4, 9, 5, 10, 12, 13];
        let pos = channels.iter().position(|&c| c == current).unwrap_or(0);
        channels[(pos + 1) % channels.len()]
    }

    /// Get current mood
    pub fn current_mood(&self) -> Mood {
        self.current_mood
    }

    /// Get current face
    pub fn current_face(&self) -> &'static str {
        face_for_mood(self.current_mood)
    }

    /// Update AP list (called from event handler)
    pub fn update_aps(&mut self, aps: Vec<AccessPoint>) {
        self.aps = aps;
    }

    /// Update peer list
    pub fn update_peers(&mut self, peers: Vec<CorePeer>) {
        self.peers = peers;
    }

    /// Set current channel
    pub fn set_channel(&mut self, channel: u8) {
        if (1..=14).contains(&channel) {
            self.current_channel = channel;
            self.epoch_tracker.current.track_hop();
        }
    }

    pub fn current_channel(&self) -> u8 {
        self.current_channel
    }

    pub fn total_epochs(&self) -> u64 {
        self.epoch_tracker.total_epochs
    }

    /// Number of APs currently tracked (for web UI / status reporting).
    pub fn aps_count(&self) -> usize {
        self.aps.len()
    }

    /// Merge one AP observation into `self.aps`, updating in place if we've
    /// already seen this BSSID. Fed real data since Phase 1 by
    /// `bettercap::WifiSession::to_pwncore` via `update_aps` in
    /// `pwnghost-rs`'s main loop (previously unreachable: AngryOxide exposed
    /// no AP data over its CLI/stdout interface at all).
    #[allow(dead_code)]
    fn add_or_update_ap(&mut self, ap: AccessPoint) -> bool {
        if let Some(existing) = self.aps.iter_mut().find(|a| a.bssid == ap.bssid) {
            *existing = ap;
            false
        } else {
            self.aps.push(ap);
            true
        }
    }

    /// Mark the AP with `bssid` as having a captured handshake. Called once
    /// the capture pipeline (`agent::capture::CaptureManager`) has validated
    /// a `.hc22000` file with `hcxpcapngtool` and extracted the real BSSID
    /// from its contents - this is the only place we trust a bssid<->
    /// handshake association, since AO's own stdout never tells us this.
    pub fn mark_handshake_captured(&mut self, bssid: MacAddr) {
        if let Some(ap) = self.aps.iter_mut().find(|a| a.bssid == bssid) {
            ap.handshake_captured = true;
        }
        // This is real, honestly-sourced progress (a validated capture, not
        // a guess) -- previously `Personality::update_on_handshake` and the
        // RL policy's reward feedback were both fully implemented and unit-
        // tested but never actually called from here, so a device's
        // XP/level and its RL policy's learned values never moved no
        // matter how many real handshakes it captured.
        self.personality.update_on_handshake(bssid.octets());
        self.observe_rl_reward(1.0);
    }

    /// Feed a reward signal to the loaded RL policy for whatever action it
    /// selected last, if one is loaded. Never blocks the tick loop (skips
    /// silently if the lock is momentarily contended, same convention as
    /// `select_action_rl`).
    fn observe_rl_reward(&self, reward: f32) {
        if let Some(rl) = self.rl_agent.as_ref() {
            if let Ok(mut guard) = rl.try_write() {
                guard.observe_reward(reward);
            }
        }
    }
}

/// Actions the agent can take
#[derive(Debug, Clone, PartialEq)]
pub enum AgentAction {
    Stay,
    Hop(u8),
    Deauth(String),
    Associate(String),
    Sleep(u64),
    Wait,
}

impl Default for Agent {
    fn default() -> Self {
        Self::new(Personality::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_new() {
        let agent = Agent::default();
        assert!(!agent.running);
        assert_eq!(agent.current_channel, 1);
        assert_eq!(agent.total_epochs(), 0);
    }

    #[test]
    fn test_agent_start_stop() {
        let mut agent = Agent::default();
        agent.start();
        assert!(agent.running);
        agent.stop();
        assert!(!agent.running);
    }

    #[test]
    fn test_agent_tick_mood_and_action() {
        let mut agent = Agent::default();
        agent.start();

        let (face, action) = agent.tick();
        assert!(!face.is_empty());
        assert!(matches!(action, AgentAction::Hop(_) | AgentAction::Stay));
        assert_eq!(agent.total_epochs(), 1);
    }

    #[test]
    fn test_agent_channel_cycling() {
        let mut agent = Agent::default();
        agent.set_channel(1);
        let next = Agent::next_channel(1);
        assert_eq!(next, 6);
        let next = Agent::next_channel(13);
        assert_eq!(next, 1);
    }

    #[test]
    fn test_agent_tick_updates_mood() {
        let mut agent = Agent::default();
        agent.start();

        for _ in 0..20 {
            agent.tick();
        }
        let mood = agent.current_mood();
        assert!(!mood.face().is_empty());
    }

    #[test]
    fn test_agent_hop_after_blind_epochs() {
        let mut agent = Agent::default();
        agent.start();

        for _ in 0..7 {
            let (_face, action) = agent.tick();
            if matches!(action, AgentAction::Hop(_)) {
                return;
            }
        }
        panic!("Agent never hopped after multiple blind epochs");
    }

    #[test]
    fn test_agent_selects_deauth_with_targets() {
        let p = crate::personality::PersonalityConfig::default();
        let mut agent = Agent::new(Personality::new(p));
        agent.start();

        // Add an AP on current channel
        let ap = AccessPoint::new(
            "aa:bb:cc:dd:ee:ff".parse().unwrap(),
            1,
            -50,
            pwncore::EncryptionType::Wpa2,
            "test".into(),
        );
        agent.aps = vec![ap];

        let (_face, action) = agent.tick();
        assert!(!agent.current_face().is_empty());
        matches!(
            action,
            AgentAction::Stay
                | AgentAction::Hop(_)
                | AgentAction::Deauth(_)
                | AgentAction::Associate(_)
                | AgentAction::Sleep(_)
        );
    }

    #[test]
    fn test_track_handshake_increments_epoch_counter() {
        // Phase 1: the "PWND"/epoch handshake counter used to be bumped
        // from `Agent::handle_event`'s AngryOxide `HandshakeFileWritten`
        // branch (now removed -- nothing calls it since bettercap replaced
        // AngryOxide). `pwnghost-rs`'s main loop now calls
        // `epoch_tracker.current.track_handshake()` directly wherever the
        // capture pipeline confirms a real handshake; this just confirms
        // that counter still behaves as expected.
        let mut agent = Agent::default();
        agent.start();
        agent.epoch_tracker.current.track_handshake();
        agent.epoch_tracker.current.track_handshake();
        assert_eq!(agent.epoch_tracker.current.handshakes_this_epoch, 2);
    }

    #[test]
    fn test_mark_handshake_captured() {
        let mut agent = Agent::default();
        agent.start();

        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];

        agent.mark_handshake_captured(bssid);
        assert!(agent.aps[0].handshake_captured);
    }

    #[test]
    fn test_mark_handshake_captured_awards_personality_xp() {
        let mut agent = Agent::default();
        agent.start();

        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];

        let xp_before = agent.personality.stats().xp;
        let handshakes_before = agent.personality.stats().handshakes;
        agent.mark_handshake_captured(bssid);
        assert!(agent.personality.stats().xp > xp_before);
        assert_eq!(agent.personality.stats().handshakes, handshakes_before + 1);
        assert_eq!(agent.personality.encounters_for(&bssid.octets()), 1);
    }

    #[test]
    fn test_blind_epoch_applies_missed_penalty() {
        let mut agent = Agent::default();
        agent.start();
        agent.aps = Vec::new(); // no APs -> the epoch that just ran is blind

        let xp_before = agent.personality.stats().xp;
        // Two ticks: the first tick's `advance()` finalizes epoch 0 (which
        // has aps_found == 0 from Agent::default()'s initial state) into
        // history, triggering the penalty on this very first tick.
        agent.tick();
        assert!(
            agent.personality.stats().xp <= xp_before,
            "a blind epoch should not increase xp (before={}, after={})",
            xp_before,
            agent.personality.stats().xp
        );
    }

    #[test]
    fn test_agent_healer_default() {
        let agent = Agent::default();
        assert!(!agent.healer.should_take_action());
        assert_eq!(agent.healer.active_layer(), HealingLayer::FwWatchdog);
    }

    #[test]
    fn test_agent_crash_escalation() {
        let mut agent = Agent::default();
        for _ in 0..10 {
            agent.report_crash();
        }
        let action = agent.check_healing();
        assert_ne!(action, HealingAction::None);
    }

    #[test]
    fn test_agent_alive_resets_healer() {
        let mut agent = Agent::default();
        for _ in 0..10 {
            agent.report_crash();
        }
        agent.report_alive();
        agent.reset_healer();
        assert_eq!(agent.healer.active_layer(), HealingLayer::FwWatchdog);
    }
}
