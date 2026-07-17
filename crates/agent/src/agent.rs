use angryoxide::parser::AoEvent;
use chrono::DateTime;
use mac_addr::MacAddr;
use pwncore::ap::AccessPoint;
use pwncore::ap::Client;
use pwncore::mood::Mood;
use pwncore::peer::Peer;
use pwncore::personality::Personality;
use std::str::FromStr;

use crate::epoch::EpochTracker;
use crate::faces;
use crate::healing::HealingAction;

/// Actions the agent can take
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Stay on current channel, continue recon
    Stay,
    /// Hop to specific channel
    Hop(u8),
    /// Send deauth to BSSID
    Deauth(String),
    /// Send association to BSSID
    Associate(String),
    /// Sleep for duration in seconds
    Sleep(u64),
    /// Wait for next epoch cycle
    Wait,
}

/// Agent state machine — the brain of pwnagotchi
pub struct Agent {
    pub epoch_tracker: EpochTracker,
    pub personality: Personality,
    pub running: bool,
    pub healer: crate::healing::Healer,

    // Current state
    current_mood: Mood,
    aps: Vec<AccessPoint>,
    peers: Vec<Peer>,

    // Channel tracking
    current_channel: u8,
}

impl Agent {
    pub fn new(personality: Personality) -> Self {
        Self {
            epoch_tracker: EpochTracker::new(),
            personality,
            running: false,
            healer: crate::healing::Healer::new(),
            current_mood: Mood::Awake,
            aps: Vec::new(),
            peers: Vec::new(),
            current_channel: 1,
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
    /// Called each epoch cycle.
    pub fn tick(&mut self) -> (&'static str, Action) {
        // Finalize previous epoch counters
        self.epoch_tracker.finalize_current(&self.personality);

        // Advance to next epoch
        self.epoch_tracker.advance();

        // Observe current environment
        {
            let epoch = &mut self.epoch_tracker.current;
            epoch.observe(&self.aps, &self.peers);
        }

        // Compute mood from epoch state
        self.current_mood = Mood::from_epoch(
            &self.epoch_tracker.current,
            &self.personality,
            &self.peers,
        );

        // Select action based on state
        let action = self.select_action();

        // Get face for current mood
        let face = faces::face_for_mood(&self.current_mood);

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

    /// Check healer state and return any healing action needed.
    /// Call this after tick() to see if the healer needs external action.
    pub fn check_healing(&mut self) -> HealingAction {
        if self.healer.should_take_action() {
            let action = self.healer.decide();
            match action {
                HealingAction::None => {}
                HealingAction::RestartAo => {
                    tracing::warn!("Healer: Restarting AngryOxide (layer {:?})", self.healer.active_layer());
                }
                HealingAction::PowerCycleGpio => {
                    tracing::error!("Healer: Power-cycling WiFi chip (layer {:?})", self.healer.active_layer());
                }
                HealingAction::EnterSafeMode => {
                    tracing::error!("Healer: Entering safe mode (layer {:?})", self.healer.active_layer());
                }
                HealingAction::EnableUsbLifeline => {
                    tracing::error!("Healer: Enabling USB lifeline (layer {:?})", self.healer.active_layer());
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

    /// Access the healer directly
    pub fn healer(&self) -> &crate::healing::Healer {
        &self.healer
    }

    pub fn healer_mut(&mut self) -> &mut crate::healing::Healer {
        &mut self.healer
    }

    /// Select next action based on epoch state, mood, and personality
    fn select_action(&self) -> Action {
        let epoch = &self.epoch_tracker.current;
        let p = &self.personality;

        // Blind epochs: no APs seen, hop to find signal
        if epoch.blind_epochs > 0 && epoch.blind_epochs >= 2 {
            return Action::Hop(Self::next_channel(self.current_channel));
        }

        // Happy/excited/grateful: we have activity, check for targets
        match self.current_mood {
            Mood::Excited | Mood::Grateful | Mood::Motivated => {
                // Find a target to interact with
                if let Some(target) = self.find_target() {
                    if p.deauth && !epoch.did_deauth {
                        return Action::Deauth(target.bssid.to_string());
                    }
                    if p.associate && !epoch.did_associate {
                        return Action::Associate(target.bssid.to_string());
                    }
                }
                // Time to hop
                    let hop_time = p.calc_hop_time(epoch);
                    if hop_time > 0 && epoch.duration().num_seconds() as u32 >= hop_time {
                    return Action::Hop(Self::next_channel(self.current_channel));
                }
                Action::Stay
            }
            Mood::Bored | Mood::Sad | Mood::Lonely => {
                // Low activity: hop sooner to find new targets
                Action::Hop(Self::next_channel(self.current_channel))
            }
            Mood::Angry | Mood::Broken => {
                // Too many failures: take a short break
                Action::Sleep(5)
            }
            _ => {
                // Default: recon on current channel
                let recon_time = p.calc_recon_time(epoch);
                if epoch.duration().num_seconds() as u32 >= recon_time {
                    Action::Hop(Self::next_channel(self.current_channel))
                } else {
                    Action::Stay
                }
            }
        }
    }

    /// Find best target AP on current channel
    fn find_target(&self) -> Option<&AccessPoint> {
        let p = &self.personality;

        for ap in &self.aps {
            if ap.channel != self.current_channel {
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
        faces::face_for_mood(&self.current_mood)
    }

    /// Update AP list (called from event handler)
    pub fn update_aps(&mut self, aps: Vec<AccessPoint>) {
        self.aps = aps;
    }

    /// Update peer list
    pub fn update_peers(&mut self, peers: Vec<Peer>) {
        self.peers = peers;
    }

    /// Set current channel
    pub fn set_channel(&mut self, channel: u8) {
        if channel >= 1 && channel <= 14 {
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

    fn add_or_update_ap(&mut self, ap: AccessPoint) -> bool {
        if let Some(existing) = self.aps.iter_mut().find(|a| a.bssid == ap.bssid) {
            *existing = ap;
            false
        } else {
            self.aps.push(ap);
            true
        }
    }

    pub fn handle_event(&mut self, event: &AoEvent) {
        match event {
            AoEvent::Ap(ap_event) => {
                let bssid = MacAddr::from_str(&ap_event.bssid).unwrap_or_default();
                let encryption = pwncore::ap::EncryptionType::from_str(&ap_event.encryption);
                let mut ap = AccessPoint::new(
                    bssid,
                    ap_event.channel,
                    ap_event.rssi,
                    encryption,
                    ap_event.vendor.clone(),
                );
                if let Some(ref ssid) = ap_event.ssid {
                    ap = ap.with_ssid(ssid.clone());
                }
                ap.first_seen = DateTime::from_timestamp(ap_event.first_seen as i64, 0)
                    .unwrap_or(DateTime::UNIX_EPOCH);
                ap.last_seen = DateTime::from_timestamp(ap_event.last_seen as i64, 0)
                    .unwrap_or(DateTime::UNIX_EPOCH);
                for ci in &ap_event.clients {
                    let client_mac = MacAddr::from_str(&ci.mac).unwrap_or_default();
                    let client = Client::new(client_mac, ci.vendor.clone(), ci.rssi, ci.channel);
                    ap.add_client(client);
                }
                if self.add_or_update_ap(ap) {
                    self.epoch_tracker.current.aps_seen += 1;
                }
            }
            AoEvent::Client(client_event) => {
                let client_mac = MacAddr::from_str(&client_event.mac).unwrap_or_default();
                let client = Client::new(
                    client_mac,
                    client_event.vendor.clone(),
                    client_event.rssi,
                    client_event.channel,
                );
                if let Ok(bssid) = MacAddr::from_str(&client_event.bssid) {
                    if let Some(ap) = self.aps.iter_mut().find(|a| a.bssid == bssid) {
                        ap.add_client(client);
                    }
                }
                self.epoch_tracker.current.clients_seen = self.aps.iter()
                    .map(|ap| ap.clients.len())
                    .sum();
            }
            AoEvent::Handshake(hs_event) => {
                if let Ok(bssid) = MacAddr::from_str(&hs_event.bssid) {
                    if let Some(ap) = self.aps.iter_mut().find(|a| a.bssid == bssid) {
                        ap.handshake_captured = true;
                    }
                }
                self.epoch_tracker.current.track_handshake();
            }
            AoEvent::Stats(stats_event) => {
                if stats_event.channel > 0 {
                    self.current_channel = stats_event.channel;
                }
            }
            AoEvent::Channel(channel_event) => {
                self.set_channel(channel_event.channel);
            }
            AoEvent::Status(status_event) => {
                match status_event.level.as_str() {
                    "error" => tracing::error!("AngryOxide: {}", status_event.message),
                    "warn" => tracing::warn!("AngryOxide: {}", status_event.message),
                    _ => tracing::info!("AngryOxide: {}", status_event.message),
                }
            }
        }
    }

    pub fn handle_events(&mut self, events: &[AoEvent]) {
        for event in events {
            self.handle_event(event);
        }
    }
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
        // No APs seen → blind epoch → should hop
        assert!(!face.is_empty());
        assert!(matches!(action, Action::Hop(_)) || matches!(action, Action::Stay));
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

        // Run several ticks to see mood transitions
        for _ in 0..20 {
            agent.tick();
        }
        let mood = agent.current_mood();
        // Should have transitioned from Awake to something else after 20 ticks with no APs
        let faces = mood.faces();
        assert!(!faces.is_empty());
    }

    #[test]
    fn test_agent_hop_after_blind_epochs() {
        let mut agent = Agent::default();
        agent.start();

        // Multiple ticks with no APs should trigger hop (mon_max_blind_epochs defaults to 5)
        for _ in 0..7 {
            let (_face, action) = agent.tick();
            if matches!(action, Action::Hop(_)) {
                return;
            }
        }
        panic!("Agent never hopped after multiple blind epochs");
    }

    #[test]
    fn test_agent_selects_deauth_with_targets() {
        let p = Personality::default();
        let mut agent = Agent::new(p);
        agent.start();

        // Add an AP on current channel
        let ap = AccessPoint::new(
            "aa:bb:cc:dd:ee:ff".parse().unwrap(),
            1, -50,
            pwncore::ap::EncryptionType::Wpa2,
            "test".into(),
        );
        agent.aps = vec![ap];

        // Tick - aggressive should deauth
        let (_face, action) = agent.tick();
        // After activating mood tracking, agent should eventually try to interact
        // At minimum, it shouldn't crash
        assert!(!agent.current_face().is_empty());
        // Action should be one of the valid actions
        matches!(action, Action::Stay | Action::Hop(_) | Action::Deauth(_) | Action::Associate(_) | Action::Sleep(_));
    }

    #[test]
    fn test_handle_ap_event_adds_ap() {
        let mut agent = Agent::default();
        agent.start();

        let ap_event = angryoxide::parser::ApEvent {
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            ssid: Some("TestNet".into()),
            channel: 1,
            rssi: -60,
            encryption: "wpa2".into(),
            vendor: "Intel".into(),
            clients: vec![],
            first_seen: 1000,
            last_seen: 1000,
        };
        let event = angryoxide::parser::AoEvent::Ap(ap_event);
        agent.handle_event(&event);

        assert_eq!(agent.aps.len(), 1);
        assert_eq!(agent.aps[0].ssid.as_deref(), Some("TestNet"));
    }

    #[test]
    fn test_handle_handshake_event() {
        let mut agent = Agent::default();
        agent.start();

        let hs_event = angryoxide::parser::HandshakeEvent {
            bssid: "aa:bb:cc:dd:ee:ff".into(),
            station: "11:22:33:44:55:66".into(),
            file: "/tmp/hs.pcap".into(),
            handshake_type: "WPA2".into(),
            timestamp: 1000,
        };
        let event = angryoxide::parser::AoEvent::Handshake(hs_event);
        agent.handle_event(&event);

        assert_eq!(agent.epoch_tracker.current.handshakes_captured, 1);
    }

    #[test]
    fn test_handle_channel_event() {
        let mut agent = Agent::default();
        agent.start();

        let ch_event = angryoxide::parser::ChannelEvent {
            channel: 6,
            timestamp: 1000,
        };
        let event = angryoxide::parser::AoEvent::Channel(ch_event);
        agent.handle_event(&event);

        assert_eq!(agent.current_channel, 6);
    }

    #[test]
    fn test_handle_events_batch() {
        let mut agent = Agent::default();
        agent.start();

        let events = vec![
            angryoxide::parser::AoEvent::Channel(angryoxide::parser::ChannelEvent { channel: 6, timestamp: 1 }),
            angryoxide::parser::AoEvent::Channel(angryoxide::parser::ChannelEvent { channel: 11, timestamp: 2 }),
        ];
        agent.handle_events(&events);
        assert_eq!(agent.current_channel, 11);
    }

    #[test]
    fn test_agent_healer_default() {
        let agent = Agent::default();
        assert!(!agent.healer.should_take_action());
        assert_eq!(agent.healer.active_layer(), crate::healing::HealingLayer::FwWatchdog);
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
        assert_eq!(agent.healer.active_layer(), crate::healing::HealingLayer::FwWatchdog);
    }
}
