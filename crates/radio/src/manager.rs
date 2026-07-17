use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMode {
    Rage,
    Bt,
    Safe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioState {
    Idle,
    Transitioning { from: RadioMode, to: RadioMode },
    Active(RadioMode),
    Failed { mode: RadioMode, error: String },
}

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

    pub fn switch_to(
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

        teardown_all(&self.interface);

        match mode {
            RadioMode::Rage => {
                if let Err(e) = bringup_rage(&self.interface) {
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
                if let Err(e) = bringup_bt(address, chip) {
                    teardown_rage(&self.interface);
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
                if let Err(e) = bringup_safe(&self.interface, ssid, pass) {
                    teardown_rage(&self.interface);
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

    pub fn reset(&mut self) {
        teardown_all(&self.interface);
        self.mode = RadioMode::Rage;
        self.state = RadioState::Idle;
    }
}

impl Default for RadioManager {
    fn default() -> Self {
        Self::new("wlan0".to_string())
    }
}

fn teardown_rage(iface: &str) {
    let _ = crate::wifi::set_monitor_mode(iface, false);
}

fn teardown_bt() {
    let _ = crate::bluetooth::disconnect_pan();
}

fn teardown_safe() {
    let _ = crate::safe::disconnect_wifi();
}

fn teardown_all(iface: &str) {
    teardown_rage(iface);
    teardown_bt();
    teardown_safe();
}

fn bringup_rage(iface: &str) -> Result<()> {
    crate::wifi::set_monitor_mode(iface, true)?;
    Ok(())
}

fn bringup_bt(address: &str, chip: &str) -> Result<()> {
    crate::patchram::load_patchram(chip)?;
    crate::bluetooth::connect_pan(address)?;
    Ok(())
}

fn bringup_safe(_iface: &str, ssid: &str, pass: &str) -> Result<()> {
    crate::safe::connect_known_wifi(ssid, pass)?;
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
    fn test_switch_to_rage() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
        assert_eq!(result.unwrap(), RadioMode::Rage);
    }

    #[test]
    fn test_switch_to_rage_idempotent() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
    }

    #[test]
    fn test_switch_rage_to_bt() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        let result = mgr.switch_to(
            RadioMode::Bt,
            Some("00:11:22:33:44:55"),
            Some("bcm43436b0"),
            None,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Bt));
        assert_eq!(result.unwrap(), RadioMode::Bt);
    }

    #[test]
    fn test_switch_bt_to_safe() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let _ = mgr.switch_to(
            RadioMode::Bt,
            Some("00:11:22:33:44:55"),
            Some("bcm43436b0"),
            None,
            None,
        );
        let result = mgr.switch_to(
            RadioMode::Safe,
            None,
            None,
            Some("MyWiFi"),
            Some("password123"),
        );
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Safe));
        assert_eq!(result.unwrap(), RadioMode::Safe);
    }

    #[test]
    fn test_switch_back_to_rage() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let _ = mgr.switch_to(
            RadioMode::Safe,
            None,
            None,
            Some("MyWiFi"),
            Some("password123"),
        );
        let result = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        assert!(result.is_ok());
        assert_eq!(mgr.state, RadioState::Active(RadioMode::Rage));
        assert_eq!(result.unwrap(), RadioMode::Rage);
    }

    #[test]
    fn test_reset_tears_down() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        let _ = mgr.switch_to(
            RadioMode::Safe,
            None,
            None,
            Some("MyWiFi"),
            Some("password123"),
        );
        mgr.reset();
        assert_eq!(mgr.state, RadioState::Idle);
        assert_eq!(mgr.mode, RadioMode::Rage);
    }

    #[test]
    fn test_current_mode_after_switch() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
        let _ = mgr.switch_to(
            RadioMode::Bt,
            Some("00:11:22:33:44:55"),
            Some("bcm43436b0"),
            None,
            None,
        );
        assert_eq!(mgr.current_mode(), RadioMode::Bt);
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        assert_eq!(mgr.current_mode(), RadioMode::Rage);
    }

    #[test]
    fn test_is_transitioning() {
        let mut mgr = RadioManager::new("wlan0".to_string());
        assert!(!mgr.is_transitioning());
        let _ = mgr.switch_to(RadioMode::Rage, None, None, None, None);
        assert!(!mgr.is_transitioning());
    }
}
