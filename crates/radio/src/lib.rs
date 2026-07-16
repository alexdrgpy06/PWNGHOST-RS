//! Radio mode manager for PWNGHOST-RS (RAGE/BT/SAFE)

pub mod bluetooth;
pub mod patchram;
pub mod safe;
pub mod wifi;

use anyhow::Result;

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
    /// When true, hardware bring-up/teardown is skipped (for tests / dry runs).
    mock: bool,
}

impl RadioManager {
    pub fn new(interface: String) -> Self {
        Self {
            mode: RadioMode::Rage,
            state: RadioState::Idle,
            interface,
            mock: false,
        }
    }

    /// Create a manager that never touches real hardware (state machine only).
    pub fn mock(interface: String) -> Self {
        Self {
            mode: RadioMode::Rage,
            state: RadioState::Idle,
            interface,
            mock: true,
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
        if self.state == RadioState::Active(mode)
            || (self.state != RadioState::Idle && self.mode == mode)
        {
            return Ok(self.mode);
        }

        if matches!(self.state, RadioState::Transitioning { .. }) {
            anyhow::bail!("Cannot switch modes while a transition is already in progress");
        }

        let from = self.mode;
        self.state = RadioState::Transitioning { from, to: mode };
        self.mode = mode;

        // Teardown current mode
        if !self.mock {
            teardown_all(&self.interface).await;
        }

        match mode {
            RadioMode::Rage => {
                if !self.mock {
                    if let Err(e) = bringup_rage(&self.interface).await {
                        self.state = RadioState::Failed {
                            mode,
                            error: format!("Failed to bring up RAGE mode: {}", e),
                        };
                        return Err(anyhow::anyhow!("Failed to bring up RAGE mode: {}", e));
                    }
                }
            }
            RadioMode::Bt => {
                let address =
                    bt_address.ok_or_else(|| anyhow::anyhow!("BT address required for Bt mode"))?;
                let chip =
                    bt_chip.ok_or_else(|| anyhow::anyhow!("BT chip required for Bt mode"))?;
                if !self.mock {
                    if let Err(e) = bringup_bt(address, chip).await {
                        teardown_rage(&self.interface).await;
                        self.state = RadioState::Failed {
                            mode,
                            error: format!("Failed to bring up BT mode: {}", e),
                        };
                        return Err(anyhow::anyhow!("Failed to bring up BT mode: {}", e));
                    }
                }
            }
            RadioMode::Safe => {
                let ssid =
                    wifi_ssid.ok_or_else(|| anyhow::anyhow!("SSID required for Safe mode"))?;
                let pass =
                    wifi_pass.ok_or_else(|| anyhow::anyhow!("Password required for Safe mode"))?;
                if !self.mock {
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
        if !self.mock {
            teardown_all(&self.interface).await;
        }
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
        let mgr = RadioManager::mock("wlan0".to_string());
        assert_eq!(mgr.state, RadioState::Idle);
        assert_eq!(mgr.interface, "wlan0");
        assert_eq!(mgr.mode, RadioMode::Rage);
    }

    #[tokio::test]
    async fn test_switch_to_rage() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
        assert_eq!(result.unwrap(), RadioMode::Rage);
    }

    #[tokio::test]
    async fn test_switch_to_rage_idempotent() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
    }

    #[tokio::test]
    async fn test_switch_rage_to_bt() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        let result = mgr
            .switch_to(
                RadioMode::Bt,
                Some("00:11:22:33:44:55"),
                Some("bcm43436b0"),
                None,
                None,
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Bt));
        assert_eq!(result.unwrap(), RadioMode::Bt);
    }

    #[tokio::test]
    async fn test_switch_bt_to_safe() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let _ = mgr
            .switch_to(
                RadioMode::Bt,
                Some("00:11:22:33:44:55"),
                Some("bcm43436b0"),
                None,
                None,
            )
            .await;
        let result = mgr
            .switch_to(
                RadioMode::Safe,
                None,
                None,
                Some("MyWiFi"),
                Some("password123"),
            )
            .await;
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Safe));
        assert_eq!(result.unwrap(), RadioMode::Safe);
    }

    #[tokio::test]
    async fn test_switch_back_to_rage() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let _ = mgr
            .switch_to(
                RadioMode::Safe,
                None,
                None,
                Some("MyWiFi"),
                Some("password123"),
            )
            .await;
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
        assert_eq!(result.unwrap(), RadioMode::Rage);
    }

    #[tokio::test]
    async fn test_reset_tears_down() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        let _ = mgr
            .switch_to(
                RadioMode::Safe,
                None,
                None,
                Some("MyWiFi"),
                Some("password123"),
            )
            .await;
        mgr.reset().await;
        assert_eq!(mgr.state, RadioState::Idle);
        assert_eq!(mgr.mode, RadioMode::Rage);
    }

    #[tokio::test]
    async fn test_current_mode_after_switch() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
        let _ = mgr
            .switch_to(
                RadioMode::Bt,
                Some("00:11:22:33:44:55"),
                Some("bcm43436b0"),
                None,
                None,
            )
            .await;
        assert_eq!(mgr.current_mode(), RadioMode::Bt);
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
    }

    #[tokio::test]
    async fn test_is_transitioning() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        assert!(!mgr.is_transitioning());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None).await;
        assert!(!mgr.is_transitioning());
    }
}
