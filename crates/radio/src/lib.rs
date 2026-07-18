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
    /// Set while a lightweight BT-scan pause (see [`RadioManager::pause_for_bt_scan`])
    /// has the monitor interface link-down. Distinct from `state`/`mode`,
    /// which keep reporting the real active mode throughout the pause --
    /// this is not a mode transition, just a brief radio-sharing courtesy.
    paused_for_bt: bool,
}

impl RadioManager {
    pub fn new(interface: String) -> Self {
        Self {
            mode: RadioMode::Rage,
            state: RadioState::Idle,
            interface,
            mock: false,
            paused_for_bt: false,
        }
    }

    /// Create a manager that never touches real hardware (state machine only).
    pub fn mock(interface: String) -> Self {
        Self {
            mode: RadioMode::Rage,
            state: RadioState::Idle,
            interface,
            mock: true,
            paused_for_bt: false,
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
        // A full mode transition supersedes any lightweight BT-scan pause.
        self.paused_for_bt = false;

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
        self.paused_for_bt = false;
    }

    /// Briefly quiesce the WiFi radio for a BT device-discovery/pairing scan,
    /// without doing a full RAGE<->BT mode transition. Only meaningful in
    /// RAGE mode (monitor interface active); a no-op everywhere else, since
    /// BT mode already owns the radio and SAFE mode's managed wlan0 doesn't
    /// contend with BT scanning the way monitor-mode TX/RX does.
    ///
    /// Ported from oxigotchi's `wifi::pause_for_bt` (see `radio::wifi::set_link_state`
    /// doc comment) -- the caller is expected to stop AO before calling this
    /// and restart it after [`RadioManager::resume_from_bt_scan`].
    pub async fn pause_for_bt_scan(&mut self) -> Result<()> {
        if self.mode != RadioMode::Rage || !matches!(self.state, RadioState::Active(RadioMode::Rage))
        {
            return Ok(());
        }
        if self.paused_for_bt {
            return Ok(());
        }
        if !self.mock {
            crate::wifi::set_link_state(&self.interface, false).await?;
        }
        self.paused_for_bt = true;
        Ok(())
    }

    /// Resume the WiFi radio after [`RadioManager::pause_for_bt_scan`]. A
    /// no-op if no pause is active.
    pub async fn resume_from_bt_scan(&mut self) -> Result<()> {
        if !self.paused_for_bt {
            return Ok(());
        }
        if !self.mock {
            crate::wifi::set_link_state(&self.interface, true).await?;
        }
        self.paused_for_bt = false;
        Ok(())
    }

    pub fn is_paused_for_bt(&self) -> bool {
        self.paused_for_bt
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

    #[tokio::test]
    async fn test_pause_and_resume_for_bt_scan_in_rage_mode() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        mgr.switch_to(RadioMode::Rage, None, None, None, None)
            .await
            .unwrap();
        assert!(!mgr.is_paused_for_bt());

        mgr.pause_for_bt_scan().await.unwrap();
        assert!(mgr.is_paused_for_bt());
        // Mode/state still report RAGE active -- this isn't a mode transition.
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
        assert_eq!(mgr.state(), &RadioState::Active(RadioMode::Rage));

        mgr.resume_from_bt_scan().await.unwrap();
        assert!(!mgr.is_paused_for_bt());
    }

    #[tokio::test]
    async fn test_pause_for_bt_scan_is_noop_outside_rage_mode() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        mgr.switch_to(
            RadioMode::Bt,
            Some("00:11:22:33:44:55"),
            Some("bcm43436b0"),
            None,
            None,
        )
        .await
        .unwrap();

        mgr.pause_for_bt_scan().await.unwrap();
        assert!(!mgr.is_paused_for_bt());
    }

    #[tokio::test]
    async fn test_resume_without_pause_is_noop() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        mgr.switch_to(RadioMode::Rage, None, None, None, None)
            .await
            .unwrap();
        assert!(mgr.resume_from_bt_scan().await.is_ok());
        assert!(!mgr.is_paused_for_bt());
    }

    #[tokio::test]
    async fn test_full_mode_switch_clears_pause_flag() {
        let mut mgr = RadioManager::mock("wlan0".to_string());
        mgr.switch_to(RadioMode::Rage, None, None, None, None)
            .await
            .unwrap();
        mgr.pause_for_bt_scan().await.unwrap();
        assert!(mgr.is_paused_for_bt());

        mgr.switch_to(
            RadioMode::Bt,
            Some("00:11:22:33:44:55"),
            Some("bcm43436b0"),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(!mgr.is_paused_for_bt());
    }
}
