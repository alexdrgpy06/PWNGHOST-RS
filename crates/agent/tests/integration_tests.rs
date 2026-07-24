use agent::{Agent, AgentAction};
use pwncore::Mood;

#[tokio::test]
async fn test_agent_integration_state_machine_flow() {
    let mut agent = Agent::default();
    agent.start();

    // Check initial state
    assert_eq!(agent.current_mood(), Mood::Awake);
    assert_eq!(agent.aps_count(), 0);

    // Populate simulated AP
    let ap = pwncore::AccessPoint::new(
        "aa:bb:cc:dd:ee:ff".parse().unwrap(),
        1,
        -60,
        pwncore::EncryptionType::Wpa2,
        "Vendor".to_string(),
    ).with_ssid("TestSSID".to_string());
    agent.update_aps(vec![ap]);
    assert_eq!(agent.aps_count(), 1);

    // Execute tick
    let (_face, action) = agent.tick();
    match action {
        AgentAction::Associate(_) | AgentAction::Deauth(_) | AgentAction::Hop(_) | AgentAction::Stay => {
            // Valid action dispatched
        }
        _ => panic!("Unexpected action on active AP"),
    }
}

#[tokio::test]
async fn test_recovery_manager_roundtrip() {
    let mut recovery = agent::recovery::RecoveryManager::new("/tmp/pwnghost_test_rec.json", 300);
    let mut agent = Agent::default();
    
    // Simulate progression
    agent.personality.update_on_association();
    agent.personality.update_on_handshake([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);

    recovery.update_from_agent(&agent);
    let result = recovery.save().await;
    assert!(result.is_ok());

    let mut new_agent = Agent::default();
    let load_result = recovery.load().await;
    assert!(load_result.is_ok());

    recovery.apply_to_agent(&mut new_agent);
    assert_eq!(new_agent.personality.stats().handshakes, agent.personality.stats().handshakes);
}

#[tokio::test]
async fn test_per_bssid_multi_target_selection() {
    let mut agent = Agent::default();
    agent.start();

    // Create 2 APs on channel 1
    let ap1 = pwncore::AccessPoint::new(
        "11:22:33:44:55:66".parse().unwrap(),
        1,
        -50,
        pwncore::EncryptionType::Wpa2,
        "Vendor1".to_string(),
    ).with_ssid("AP1".to_string());

    let ap2 = pwncore::AccessPoint::new(
        "aa:bb:cc:dd:ee:ff".parse().unwrap(),
        1,
        -55,
        pwncore::EncryptionType::Wpa2,
        "Vendor2".to_string(),
    ).with_ssid("AP2".to_string());

    agent.update_aps(vec![ap1, ap2]);

    // Tick 1: should select first AP
    let (_face1, action1) = agent.tick();
    match action1 {
        AgentAction::Associate(target) | AgentAction::Deauth(target) => {
            assert_eq!(target, "11:22:33:44:55:66");
        }
        other => panic!("Expected target action on AP1, got {:?}", other),
    }

    // Tick 2: should select second AP (AP2) rather than being throttled globally!
    let (_face2, action2) = agent.tick();
    match action2 {
        AgentAction::Associate(target) | AgentAction::Deauth(target) => {
            assert_eq!(target, "aa:bb:cc:dd:ee:ff");
        }
        other => panic!("Expected target action on AP2, got {:?}", other),
    }
}

