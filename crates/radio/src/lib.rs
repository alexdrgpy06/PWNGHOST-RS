//! Radio mode manager for PWNGHOST-RS (RAGE/BT/SAFE)

pub mod bluetooth;
pub mod patchram;
pub mod safe;
pub mod wifi;

use anyhow::Result;
use tracing::{info, warn};

/// Three mutually exclusive radio modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RadioMode {
    Rage,
    Bt,
    Safe,
}

/// Radio state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioState {
    Idle,
    Transitioning { from: RadioMode, to: RadioMode },
    Active(RadioMode),
    Failed { mode: RadioMode, error: String },
}

/// High-level radio manager
pub struct RadioManager {
    mode: RadioMode,
    state: RadioState,
    interface: String,
}

impl RadioManager {
    pub fn new(interface: String) -> Self {
        Self {
            mode: RadioMode::Rage,
            state: RadioState::Idle,
            interface,
        }
    }

    /// Switch radio mode atomically
    pub async fn switch_to(
        &mut self,
        mode: RadioMode,
        bt_address: Option<&str>,
        bt_chip: Option<&str>,
        wifi_ssid: Option<&str>,
        wifi_pass: Option<&str>,
    ) -> Result<RadioMode> {
        if self.state == RadioState::Active(mode) || (self.state != RadioState::Idle && self.mode == mode) {
            return Ok(self.mode);
        }

        if matches!(self.state, RadioState::Transitioning { .. }) {
            anyhow::bail!("Cannot switch modes while a transition is already in progress");
        }

        let from = self.mode;
        self.state = RadioState::Transitioning { from, to: mode };
        self.mode = mode;

        // Teardown current mode
        teardown_all(&self.interface).await;

        match mode {
            RadioMode::Rage => {
                if let Err(e) = bringup_rage(&self.interface).await {
                    self.state = RadioState::Failed {
                        mode,
                        error: format!("Failed to bring up RAGE mode: {}", e),
                    };
                    return Err(anyhow::anyhow!("Failed to bring up RAGE mode: {}", e));
                }
            }
            RadioMode::Bt => {
                let address = bt_address
                    .ok_or_else(|| anyhow::anyhow!("BT address required for Bt mode"))?;
                let chip = bt_chip
                    .ok_or_else(|| anyhow::anyhow!("BT chip required for Bt mode"))?;
                if let Err(e) = bringup_bt(address, chip).await {
                    teardown_rage(&self.interface).await;
                    self.state = RadioState::Failed {
                        mode,
                        error: format!("Failed to bring up BT mode: {}", e),
                    };
                    return Err(anyhow::anyhow!("Failed to bring up BT mode: {}", e));
                }
            }
            RadioMode::Safe => {
                let ssid = wifi_ssid
                    .ok_or_else(|| anyhow::anyhow!("SSID required for Safe mode"))?;
                let pass = wifi_pass
                    .ok_or_else(|| anyhow::anyhow!("Password required for Safe mode"))?;
                if let Err(e) = bringup_safe(&self.interface, ssid, pass).await {
                    teardown_rage(&self.interface).await;
                    self.state = RadioState::Failed {
                        mode,
                        error: format!("Failed to bring up SAFE mode: {}", e),
                    };
                    return Err(anyhow::anyhow!("Failed to bring up SAFE mode: {}", e));
                }
            }
        }

        self.state = RadioState::Active(mode);
        Ok(self.mode)
    }

    pub fn current_mode(&self) -> RadioMode {
        self.mode
    }

    pub fn state(&self) -> &RadioState {
        &self.state
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn is_transitioning(&self) -> bool {
        matches!(self.state, RadioState::Transitioning { .. })
    }

    pub async fn reset(&mut self) {
        teardown_all(&self.interface).await;
        self.mode = RadioMode::Rage;
        self.state = RadioState::Idle;
    }
}

async fn teardown_all(iface: &str) {
    teardown_rage(iface).await;
    teardown_bt().await;
    teardown_safe().await;
}

async fn teardown_rage(iface: &str) {
    let _ = crate::wifi::set_monitor_mode(iface, false).await;
}

async fn teardown_bt() {
    let _ = crate::bluetooth::disconnect_pan().await;
}

async fn teardown_safe() {
    let _ = crate::safe::disconnect_wifi().await;
}

async fn bringup_rage(iface: &str) -> Result<()> {
    crate::wifi::set_monitor_mode(iface, true).await?;
    Ok(())
}

async fn bringup_bt(address: &str, chip: &str) -> Result<()> {
    crate::patchram::load_patchram(chip).await?;
    crate::bluetooth::connect_pan(address).await?;
    Ok(())
}

async fn bringup_safe(_iface: &str, ssid: &str, pass: &str) -> Result<()> {
    crate::safe::connect_known_wifi(ssid, pass).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radio_manager_new() {
        let mgr = RadioManager::new("wlan0".to_string());
        assert_eq!(mgr.state, RadioState::Idle);
        assert_eq!(mgr.interface, "wlan0");
        assert_eq!(mgr.mode, RadioMode::Rage);
    }

    #[test]
    fn test_radio_mode_equality() {
        assert_eq!(RadioMode::Rage as u8, 0);
        assert_eq!(RadioMode::Bt as u8, 1);
        assert_eq!(RadioMode::Safe as u8, 2);
    }

    #[test]
    fn test_radio_state_equality() {
        assert_ne!(RadioState::Idle, RadioState::Active(RadioMode::Rage));
        assert_eq!(
            RadioState::Active(RadioMode::Rage),
            RadioState::Active(RadioMode::Rage)
        );
    }

    #[test]
    fn test_radio_state_transitioning() {
        let transitioning = RadioState::Transitioning {
            from: RadioMode::Rage,
            to: RadioMode::Bt,
        };
        assert!(matches!(transitioning, RadioState::Transitioning { .. }));
    }

    #[test]
    fn test_radio_mode_serde_roundtrip() {
        let modes = [RadioMode::Rage, RadioMode::Bt, RadioMode::Safe];
        for mode in &modes {
            let json = serde_json::to_string(mode).unwrap();
            let deserialized: RadioMode = serde_json::from_str(&json).unwrap();
            assert_eq!(*mode, deserialized);
        }
    }

    #[test]
    fn test_current_mode_default() {
        let mgr = RadioManager::new("wlan0".to_string());
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
    }

    #[test]
    fn test_state_default() {
        let mgr = RadioManager::new("wlan0".to_string());
        assert!(!mgr.is_transitioning());
        assert_eq!(mgr.state(), &RadioState::Idle);
    }

    #[test]
    fn test_interface_getter() {
        let mgr = RadioManager::new("mon0".to_string());
        assert_eq!(mgr.interface(), "mon0");
    }
}