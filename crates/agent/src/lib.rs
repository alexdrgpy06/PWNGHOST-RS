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
pub use plugins::{AgentRef, LuaPlugin, PeerInfo, PluginApi, PluginManager};

use chrono::{DateTime, Utc};
use mac_addr::MacAddr;
use pwncore::{AccessPoint, Channel, Mood, Peer as CorePeer};
use std::cell::Cell;
use std::collections::HashMap;
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
    /// The status-line phrase for `current_mood`, re-rolled only when the
    /// mood actually transitions (see `tick()`) rather than every call --
    /// real pwnagotchi picks a phrase once per mood-transition event (each
    /// `Automata::set_X()` calls `View.on_X()` exactly once), not on every
    /// render tick. Re-rolling every tick (this project's display refreshes
    /// at ~1Hz) would make the status line flicker between random phrases
    /// every second instead of holding steady until something actually
    /// changes.
    current_phrase: String,
    aps: Vec<AccessPoint>,
    peers: Vec<CorePeer>,
    current_channel: u8,

    /// SSIDs and/or MAC addresses (real pwnagotchi's `main.whitelist` mixes
    /// both) that `find_target` must never select as a deauth/associate
    /// target. Set once from `config.main.whitelist` after construction --
    /// see `is_whitelisted` for why this wasn't consulted at all before.
    pub whitelist: Vec<String>,

    /// Per-BSSID interaction counter, mirroring real pwnagotchi's
    /// `Agent._history`/`_should_interact`: `personality.max_interactions`
    /// caps how many times the same AP can be offered as a deauth/associate
    /// target before `find_target` stops selecting it. Without this, an AP
    /// that never yields a handshake (WPA3-only, deauth-resistant client,
    /// etc.) could monopolize every future action slot forever, since
    /// nothing else made `find_target` move on to a different candidate.
    /// Was fully configurable (`PersonalityConfig::max_interactions`,
    /// present in schema/migrate/defaults.toml) but never actually
    /// consulted anywhere -- another instance of the "config exists, never
    /// wired up" pattern found repeatedly this session.
    interaction_history: HashMap<MacAddr, u32>,

    /// Timestamp of the last deauth command sent, used to enforce
    /// `personality.throttle` (minimum seconds between deauths to
    /// prevent Broadcom firmware freeze from rapid-fire injection).
    /// OG pwnagotchi uses the same throttle mechanism to protect the
    /// BCM43430/BCM43436 chips from Nexmon injection lockups.
    /// `Cell` provides interior mutability so the check can live in
    /// the otherwise-immutable `select_action_heuristic`.
    last_deauth: Cell<Option<Instant>>,

    /// Timestamp of the last association command sent, same throttle
    /// protection as deauth.
    last_assoc: Cell<Option<Instant>>,

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
            current_phrase: Mood::Awake.voice_line().to_string(),
            aps: Vec::new(),
            peers: Vec::new(),
            current_channel: 1,
            whitelist: Vec::new(),
            interaction_history: HashMap::new(),
            last_deauth: Cell::new(None),
            last_assoc: Cell::new(None),
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
        let new_mood = self
            .personality
            .compute_mood(&self.epoch_tracker.current, &self.peers);
        if new_mood != self.current_mood {
            let agent_ref = self.build_agent_ref();
            self.plugins.fire_mood_hook(&new_mood, &agent_ref);

            // Provide runtime context for voice-line interpolation
            // ({name}, {ap}, {sta} placeholders).  Peer name is the
            // closest/most-relevant peer; AP name is the SSID of the
            // first visible access point.
            let peer_name = self.peers.first().map(|p| p.name.as_str());
            let ap_name = self
                .aps
                .first()
                .and_then(|ap| ap.ssid.as_deref());
            self.current_phrase = new_mood.voice_line_with_context(peer_name, ap_name, None);
        }
        self.current_mood = new_mood;

        // Select action based on state
        let action = self.select_action();

        // Record the interaction against the per-target cap now that an
        // action was actually chosen (see `interaction_history`'s field doc
        // comment) -- `find_target` only *checks* the cap; this is what
        // increments it. Also update rate-limit timestamps so the throttle
        // starts counting from now for the next tick.
        match &action {
            AgentAction::Deauth(bssid) => {
                if let Ok(mac) = bssid.parse::<MacAddr>() {
                    self.record_interaction(mac);
                }
                self.last_deauth.set(Some(Instant::now()));
            }
            AgentAction::Associate(bssid) => {
                if let Ok(mac) = bssid.parse::<MacAddr>() {
                    self.record_interaction(mac);
                }
                self.last_assoc.set(Some(Instant::now()));
            }
            _ => {}
        }

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

    /// Build an `AgentRef` snapshot of the current agent state for
    /// plugin hook invocation.  Mirrors the fields real pwnagotchi's
    /// Lua plugins expect on the `agent` global table.
    pub fn build_agent_ref(&self) -> plugins::AgentRef {
        plugins::AgentRef {
            current_epoch: self.total_epochs(),
            current_channel: self.current_channel(),
            aps_count: self.aps.len() as usize,
            handshakes: self.epoch_tracker.current.handshakes_this_epoch,
            total_handshakes: self.epoch_tracker.current.total_handshakes as u32,
            mood: format!("{:?}", self.current_mood),
            peers: self
                .peers
                .iter()
                .map(|p| plugins::PeerInfo {
                    mac: p.mac.to_string(),
                    name: p.name.clone(),
                    channel: p.channel,
                    mood: format!("{:?}", p.mood),
                    level: p.level,
                })
                .collect(),
            level: self.personality.stats().level,
            xp: self.personality.stats().xp,
            uptime: self.start.elapsed().as_secs(),
            name: String::new(),
        }
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

        let not_deauth_throttled = !Self::is_throttled(&self.last_deauth, p.throttle);
        let not_assoc_throttled = !Self::is_throttled(&self.last_assoc, p.throttle);

        match guard.select_action(&features) {
            rl_agent::RlAction::HopChannel(ch) => Some(AgentAction::Hop(ch.clamp(1, 13))),
            rl_agent::RlAction::Deauth if p.deauth && not_deauth_throttled => self
                .find_target(true)
                .map(|t| AgentAction::Deauth(t.bssid.to_string())),
            rl_agent::RlAction::Associate if p.associate && not_assoc_throttled => self
                .find_target(false)
                .map(|t| AgentAction::Associate(t.bssid.to_string())),
            rl_agent::RlAction::Wait => Some(AgentAction::Stay),
            rl_agent::RlAction::Sleep(secs) => Some(AgentAction::Sleep(secs as u64)),
            // Deauth/Associate requested but disabled or throttled.
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

    /// Check whether a rate-limited action is still on cooldown.
    /// `last` is the timestamp of the last action (deauth or assoc),
    /// and `throttle` is the minimum interval from `personality.throttle` (u32 seconds).
    fn is_throttled(last: &Cell<Option<Instant>>, throttle: u32) -> bool {
        match last.get() {
            None => false,
            Some(t) => t.elapsed().as_secs() < throttle as u64,
        }
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
                // Deauth and associate now look for independently-eligible
                // targets rather than sharing one `find_target()` call:
                // a deauth candidate must have a detected client (see
                // `find_target`'s doc comment), but the AP first in
                // iteration order might not have one yet while a later,
                // client-bearing AP does -- sharing one lookup meant a
                // clientless AP could block both actions for the whole
                // epoch even though associate never needed a client at all.
                // Rate-liming: skip deauth if throttle hasn't elapsed since
                // last one -- prevents Broadcom firmware freeze on Nexmon.
                if p.deauth
                    && !epoch.did_deauth
                    && !Self::is_throttled(&self.last_deauth, p.throttle)
                {
                    if let Some(target) = self.find_target(true) {
                        return AgentAction::Deauth(target.bssid.to_string());
                    }
                }
                if p.associate
                    && !epoch.did_associate
                    && !Self::is_throttled(&self.last_assoc, p.throttle)
                {

                    if let Some(target) = self.find_target(false) {
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

    /// Find best target AP on current channel. `requires_clients` gates
    /// deauth-eligibility: bettercap's `wifi.deauth <BSSID>` collects every
    /// currently-known client of that AP and deauths each one (confirmed
    /// directly from bettercap's Go source, `modules/wifi/wifi_deauth.go`'s
    /// `startDeauth`) -- if the AP has *no* detected clients yet, that same
    /// source returns a hard error ("doesn't have detected clients") instead
    /// of quietly doing nothing. Without this check, `find_target` could
    /// repeatedly hand a clientless AP to the deauth branch, burning that
    /// epoch's one deauth slot on a command that can never succeed while a
    /// real deauth-capable target sits later in `self.aps`. Associate has no
    /// such requirement (a PMKID can be captured from the AP directly, no
    /// client needed), so callers pass `false` there.
    fn find_target(&self, requires_clients: bool) -> Option<&AccessPoint> {
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
            if requires_clients && ap.clients.is_empty() {
                continue;
            }
            if !self.is_targetable(ap) {
                continue;
            }
            if !self.interactions_remaining(ap.bssid) {
                continue;
            }
            return Some(ap);
        }
        None
    }

    /// Whether `bssid` may still be selected as a deauth/associate target,
    /// per `personality.max_interactions`. See `interaction_history`'s
    /// field doc comment.
    fn interactions_remaining(&self, bssid: MacAddr) -> bool {
        let count = self.interaction_history.get(&bssid).copied().unwrap_or(0);
        count < self.personality.config().max_interactions
    }

    /// Record one deauth/associate interaction with `bssid`. See
    /// `interaction_history`'s field doc comment.
    fn record_interaction(&mut self, bssid: MacAddr) {
        *self.interaction_history.entry(bssid).or_insert(0) += 1;
    }

    /// Whether `ap` is safe to target (i.e. is NOT protected by
    /// `main.whitelist`). Named to avoid the ambiguity that "whitelist"
    /// itself invites: despite the name, real pwnagotchi's `main.whitelist`
    /// is a *protect-from-attack* exclude-list (its own docs: useful so you
    /// don't deauth your own network, or a neighbor's, constantly), not an
    /// allow-scope restricting targeting to only listed entries.
    ///
    /// Previously this wasn't checked at all: `AccessPoint::is_target`
    /// existed, was unit-tested, and was never called from `find_target` --
    /// the sole target-selection function for both the heuristic and RL
    /// action-selection paths -- so a network a user explicitly whitelisted
    /// would still get deauthed/associated. (A first attempt at this fix
    /// also had the exclude-vs-allow-scope direction backwards, matching an
    /// equally-backwards docstring `is_target` had at the time -- both are
    /// now corrected together, with regression tests on both sides.)
    ///
    /// Real pwnagotchi's own `main.whitelist` mixes SSIDs and MAC addresses
    /// in the same list (its own example config:
    /// `["MyHomeNetwork", "aa:bb:cc:dd:ee:ff"]`), so this checks the SSID as
    /// a plain string first, then falls back to `AccessPoint::is_target` for
    /// whichever entries parse as a MAC address. It deliberately does NOT
    /// call `is_target` when zero entries parse as a MAC: `is_target` treats
    /// an *empty* whitelist slice as "nothing protected," which would
    /// silently allow everything if every configured entry happened to be
    /// an SSID -- the opposite of what a non-empty `main.whitelist` means.
    fn is_targetable(&self, ap: &AccessPoint) -> bool {
        if self.whitelist.is_empty() {
            return true;
        }
        if let Some(ssid) = ap.ssid.as_deref() {
            if self.whitelist.iter().any(|w| w == ssid) {
                return false;
            }
        }
        let whitelist_macs: Vec<MacAddr> = self
            .whitelist
            .iter()
            .filter_map(|w| w.parse().ok())
            .collect();
        if !whitelist_macs.is_empty() {
            return ap.is_target(&whitelist_macs, &[]);
        }
        true
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

    /// Current status-line phrase, held steady since the last mood
    /// transition (see the `current_phrase` field doc comment).
    pub fn current_phrase(&self) -> &str {
        &self.current_phrase
    }

    /// Get current face
    pub fn current_face(&self) -> &'static str {
        face_for_mood(self.current_mood)
    }

    /// Update AP list from a fresh bettercap poll, merging each observation
    /// via `add_or_update_ap` and awarding `reward_new_ap` XP for every
    /// genuinely new BSSID. Returns the APs that were newly discovered this
    /// call, so the caller can surface them (e.g. the WebUI's live activity
    /// feed).
    ///
    /// Previously this did a wholesale `self.aps = aps` replace, which never
    /// distinguished a new AP from an already-known one -- `add_or_update_ap`
    /// (which returns that distinction) existed and was unit-tested, but sat
    /// marked `#[allow(dead_code)]`, never called from anywhere. Confirmed
    /// on real hardware after the nexmon monitor-mode fix started producing
    /// real AP data: `aps` correctly reported 14 real access points, but
    /// `xp`/`level` stayed at 0 the whole time, since nothing ever called
    /// `Personality::update_on_new_ap`.
    pub fn update_aps(&mut self, aps: Vec<AccessPoint>) -> Vec<AccessPoint> {
        let mut new_aps = Vec::new();
        for ap in aps {
            if self.add_or_update_ap(ap.clone()) {
                self.personality.update_on_new_ap();
                new_aps.push(ap);
            }
        }
        new_aps
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

    /// Access the current AP list (for plugin hooks / web UI / status).
    pub fn aps(&self) -> &[AccessPoint] {
        &self.aps
    }

    /// Merge one AP observation into `self.aps`, updating in place if we've
    /// already seen this BSSID and returning whether it was new. Called from
    /// `update_aps`, which is fed real data by `bettercap::WifiSession::
    /// to_pwncore` via `pwnghost-rs`'s main loop.
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
        // Matches real pwnagotchi's `Voice.on_handshakes` -- a captured
        // handshake gets its own celebratory line immediately, overriding
        // whatever the current mood's generic phrase would say, rather than
        // waiting for the next mood transition to (eventually) pick a
        // Happy/Excited/Grateful line from the generic pool.
        self.current_phrase = "Cool, we got a new handshake!".to_string();
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
    fn test_find_target_skips_whitelisted_bssid() {
        // Regression test: `find_target` previously never consulted
        // `whitelist` at all -- a whitelisted AP would still get selected
        // as a deauth/associate target.
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];
        agent.whitelist = vec!["aa:bb:cc:dd:ee:ff".to_string()];

        assert!(agent.find_target(false).is_none());
    }

    #[test]
    fn test_find_target_skips_whitelisted_ssid() {
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let mut ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        ap.ssid = Some("MyHomeNetwork".to_string());
        agent.aps = vec![ap];
        agent.whitelist = vec!["MyHomeNetwork".to_string()];

        assert!(agent.find_target(false).is_none());
    }

    #[test]
    fn test_find_target_allows_non_whitelisted_ap_when_whitelist_set() {
        // The whitelist only protects *listed* APs -- an unrelated AP
        // (not in the list) must remain a perfectly valid target.
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "11:22:33:44:55:66".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];
        agent.whitelist = vec!["aa:bb:cc:dd:ee:ff".to_string()];

        assert!(agent.find_target(false).is_some());
    }

    #[test]
    fn test_find_target_allows_all_when_whitelist_empty() {
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];

        assert!(agent.find_target(false).is_some());
    }

    #[test]
    fn test_find_target_requires_clients_when_gated_for_deauth() {
        // bettercap's `wifi.deauth <BSSID>` errors ("doesn't have detected
        // clients") when the AP has none -- `find_target(true)` (the deauth
        // path) must skip clientless APs so that error can't burn the
        // epoch's one deauth slot on a target that could never succeed.
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];

        assert!(agent.find_target(true).is_none());
        // Associate has no client requirement (a PMKID can be captured
        // straight from the AP), so the same clientless AP is still a
        // valid associate target.
        assert!(agent.find_target(false).is_some());
    }

    #[test]
    fn test_find_target_selects_ap_with_clients_when_gated_for_deauth() {
        let mut agent = Agent::default();
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let mut ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        let sta_mac: MacAddr = "11:22:33:44:55:66".parse().unwrap();
        ap.add_client(pwncore::Station::new(sta_mac, "test".into(), -50, 1));
        agent.aps = vec![ap];

        let target = agent.find_target(true);
        assert!(target.is_some());
        assert_eq!(target.unwrap().bssid, bssid);
    }

    #[test]
    fn test_find_target_stops_selecting_ap_after_max_interactions() {
        // Regression test: `max_interactions` was fully configurable
        // (schema, migrate, defaults.toml) but never consulted anywhere --
        // without this cap, a single AP that never yields a handshake
        // could monopolize every future deauth/associate slot forever.
        let p = crate::personality::PersonalityConfig {
            max_interactions: 2,
            ..Default::default()
        };
        let mut agent = Agent::new(Personality::new(p));
        agent.start();
        let bssid: MacAddr = "aa:bb:cc:dd:ee:ff".parse().unwrap();
        let ap = AccessPoint::new(bssid, 1, -50, pwncore::EncryptionType::Wpa2, "test".into());
        agent.aps = vec![ap];

        assert!(agent.find_target(false).is_some());
        agent.record_interaction(bssid);
        assert!(agent.find_target(false).is_some());
        agent.record_interaction(bssid);
        // Cap reached (2/2) -- no longer offered as a target.
        assert!(agent.find_target(false).is_none());
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
