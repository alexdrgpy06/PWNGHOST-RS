use anyhow::Result;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// WL_REG_ON GPIO pin for BCM43436B0 (Pi Zero 2W)
/// This is GPIO 22 on Pi Zero 2W (same as Pi 3B+)
const WL_REG_ON_PIN: u8 = 22;

/// Power cycle delay (ms)
const POWER_CYCLE_DELAY_MS: u64 = 500;

/// Post-power-cycle wait for firmware reload (ms)
const POST_CYCLE_WAIT_MS: u64 = 2000;

#[cfg(target_os = "linux")]
mod linux_gpio {
    use super::*;
    use rppal::gpio::{Gpio, Level, OutputPin};
    use anyhow::bail;

    /// GPIO controller for WiFi chip power management
    pub struct WifiPowerControl {
        pin: OutputPin,
    }

    impl WifiPowerControl {
        /// Initialize GPIO control for WL_REG_ON
        pub fn new() -> Result<Self> {
            let gpio = Gpio::new()
                .context("Failed to initialize GPIO")?;
            
            let pin = gpio.get(WL_REG_ON_PIN)
                .with_context(|| format!("Failed to get GPIO {}", WL_REG_ON_PIN))?
                .into_output_high(); // Start with WiFi enabled (high)
            
            info!("Initialized WL_REG_ON on GPIO {}", WL_REG_ON_PIN);
            
            Ok(WifiPowerControl { pin })
        }

        /// Power cycle the WiFi chip (toggle WL_REG_ON low then high)
        pub async fn power_cycle(&mut self) -> Result<()> {
            warn!("Power cycling WiFi chip via WL_REG_ON (GPIO {})", WL_REG_ON_PIN);
            
            // Drive low (power off)
            self.pin.set_low();
            info!("WL_REG_ON driven LOW (WiFi chip powered off)");
            
            sleep(Duration::from_millis(POWER_CYCLE_DELAY_MS)).await;
            
            // Drive high (power on)
            self.pin.set_high();
            info!("WL_REG_ON driven HIGH (WiFi chip powered on)");
            
            // Wait for firmware to reload and interface to appear
            sleep(Duration::from_millis(POST_CYCLE_WAIT_MS)).await;
            
            info!("Power cycle complete, waiting for brcmfmac reload");
            Ok(())
        }

        /// Force power off (drive low)
        pub fn power_off(&mut self) -> Result<()> {
            self.pin.set_low();
            info!("WL_REG_ON driven LOW (WiFi chip powered off)");
            Ok(())
        }

        /// Force power on (drive high)
        pub fn power_on(&mut self) -> Result<()> {
            self.pin.set_high();
            info!("WL_REG_ON driven HIGH (WiFi chip powered on)");
            Ok(())
        }

        /// Get current pin state
        pub fn is_powered(&self) -> bool {
            self.pin.is_set_high()
        }
    }

    /// Perform full WiFi recovery: unload module, power cycle, reload module
    pub async fn full_wifi_recovery() -> Result<()> {
        warn!("Starting full WiFi recovery sequence");
        
        // 1. Stop services that might use WiFi
        stop_wifi_services().await?;
        
        // 2. Unload brcmfmac module
        unload_brcmfmac().await?;
        
        // 3. Power cycle the chip
        let mut power_ctrl = WifiPowerControl::new()?;
        power_ctrl.power_cycle().await?;
        
        // 4. Reload brcmfmac module
        load_brcmfmac().await?;
        
        // 5. Wait for interface to appear
        wait_for_wlan_interface().await?;
        
        // 6. Restart services
        start_wifi_services().await?;
        
        info!("Full WiFi recovery completed successfully");
        Ok(())
    }

    /// Stop services that use WiFi
    async fn stop_wifi_services() -> Result<()> {
        let services = ["pwnagotchi-rs", "wlan_keepalive", "hostapd", "wpa_supplicant"];
        
        for svc in services {
            let _ = tokio::process::Command::new("systemctl")
                .args(["stop", svc])
                .output()
                .await;
            debug!("Stopped service: {}", svc);
        }
        
        Ok(())
    }

    /// Unload brcmfmac kernel module
    async fn unload_brcmfmac() -> Result<()> {
        info!("Unloading brcmfmac module");
        
        let output = tokio::process::Command::new("modprobe")
            .args(["-r", "brcmfmac"])
            .output()
            .await
            .context("Failed to run modprobe -r brcmfmac")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("modprobe -r brcmfmac failed (may not be loaded): {}", stderr);
        } else {
            info!("brcmfmac module unloaded");
        }
        
        // Also remove dependent modules
        let _ = tokio::process::Command::new("modprobe")
            .args(["-r", "brcmutil", "cfg80211", "rfkill"])
            .output()
            .await;
        
        sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    /// Load brcmfmac kernel module
    async fn load_brcmfmac() -> Result<()> {
        info!("Loading brcmfmac module");
        
        let output = tokio::process::Command::new("modprobe")
            .args(["brcmfmac"])
            .output()
            .await
            .context("Failed to run modprobe brcmfmac")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("modprobe brcmfmac failed: {}", stderr);
            bail!("Failed to load brcmfmac: {}", stderr);
        }
        
        info!("brcmfmac module loaded");
        sleep(Duration::from_millis(1000)).await;
        Ok(())
    }

    /// Wait for wlan0 interface to appear
    async fn wait_for_wlan_interface() -> Result<()> {
        info!("Waiting for wlan0 interface");
        
        for attempt in 1..=30 {
            if std::path::Path::new("/sys/class/net/wlan0").exists() {
                info!("wlan0 interface appeared after {} attempts", attempt);
                return Ok(());
            }
            
            debug!("wlan0 not ready, attempt {}/30", attempt);
            sleep(Duration::from_millis(1000)).await;
        }
        
        bail!("Timeout waiting for wlan0 interface")
    }

    /// Restart WiFi services
    async fn start_wifi_services() -> Result<()> {
        let services = ["wlan_keepalive", "pwnagotchi-rs"];
        
        for svc in services {
            let _ = tokio::process::Command::new("systemctl")
                .args(["start", svc])
                .output()
                .await;
            debug!("Started service: {}", svc);
        }
        
        Ok(())
    }

    /// Check if WiFi firmware has crashed (by examining dmesg)
    pub fn check_firmware_crash() -> bool {
        let output = std::process::Command::new("dmesg")
            .args(["-T", "-k", "-l", "err,crit,alert,emerg"])
            .output();
        
        match output {
            Ok(out) => {
                let dmesg = String::from_utf8_lossy(&out.stdout).to_lowercase();
                dmesg.contains("brcmfmac") && (
                    dmesg.contains("firmware has halted") ||
                    dmesg.contains("sdio error") ||
                    dmesg.contains("firmware crashed") ||
                    dmesg.contains("chip crash")
                )
            }
            Err(_) => false,
        }
    }

    /// Get WiFi chip status from debugfs
    pub fn get_wifi_chip_status() -> Result<String> {
        let status_path = "/sys/kernel/debug/brcmfmac/sdio_status";
        
        if std::path::Path::new(status_path).exists() {
            let content = std::fs::read_to_string(status_path)?;
            Ok(content)
        } else {
            Ok("debugfs not available".to_string())
        }
    }
}

// Mock implementation for non-Linux platforms (Windows, macOS)
#[cfg(not(target_os = "linux"))]
mod mock_gpio {
    use super::*;
    

    /// Mock GPIO controller for WiFi chip power management (non-Linux)
    pub struct WifiPowerControl {
        powered: bool,
    }

    impl WifiPowerControl {
        /// Initialize mock GPIO control for WL_REG_ON
        pub fn new() -> Result<Self> {
            info!("Mock: Initialized WL_REG_ON on GPIO {} (mock)", WL_REG_ON_PIN);
            Ok(WifiPowerControl { powered: true })
        }

        /// Mock power cycle the WiFi chip
        pub async fn power_cycle(&mut self) -> Result<()> {
            warn!("Mock: Power cycling WiFi chip via WL_REG_ON (GPIO {})", WL_REG_ON_PIN);
            
            // Drive low (power off)
            self.powered = false;
            info!("Mock: WL_REG_ON driven LOW (WiFi chip powered off)");
            
            sleep(Duration::from_millis(POWER_CYCLE_DELAY_MS)).await;
            
            // Drive high (power on)
            self.powered = true;
            info!("Mock: WL_REG_ON driven HIGH (WiFi chip powered on)");
            
            // Wait for firmware to reload and interface to appear
            sleep(Duration::from_millis(POST_CYCLE_WAIT_MS)).await;
            
            info!("Mock: Power cycle complete, waiting for brcmfmac reload");
            Ok(())
        }

        /// Mock force power off
        pub fn power_off(&mut self) -> Result<()> {
            self.powered = false;
            info!("Mock: WL_REG_ON driven LOW (WiFi chip powered off)");
            Ok(())
        }

        /// Mock force power on
        pub fn power_on(&mut self) -> Result<()> {
            self.powered = true;
            info!("Mock: WL_REG_ON driven HIGH (WiFi chip powered on)");
            Ok(())
        }

        /// Get current pin state
        pub fn is_powered(&self) -> bool {
            self.powered
        }
    }

    /// Mock full WiFi recovery
    pub async fn full_wifi_recovery() -> Result<()> {
        warn!("Mock: Starting full WiFi recovery sequence");
        
        // 1. Stop services that might use WiFi
        stop_wifi_services().await?;
        
        // 2. Unload brcmfmac module (mock)
        unload_brcmfmac().await?;
        
        // 3. Power cycle the chip
        let mut power_ctrl = WifiPowerControl::new()?;
        power_ctrl.power_cycle().await?;
        
        // 4. Reload brcmfmac module (mock)
        load_brcmfmac().await?;
        
        // 5. Wait for interface to appear (mock)
        wait_for_wlan_interface().await?;
        
        // 6. Restart services
        start_wifi_services().await?;
        
        info!("Mock: Full WiFi recovery completed successfully");
        Ok(())
    }

    async fn stop_wifi_services() -> Result<()> {
        let services = ["pwnagotchi-rs", "wlan_keepalive", "hostapd", "wpa_supplicant"];
        
        for svc in services {
            debug!("Mock: Stopped service: {}", svc);
        }
        Ok(())
    }

    async fn unload_brcmfmac() -> Result<()> {
        info!("Mock: Unloading brcmfmac module");
        sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    async fn load_brcmfmac() -> Result<()> {
        info!("Mock: Loading brcmfmac module");
        sleep(Duration::from_millis(1000)).await;
        Ok(())
    }

    async fn wait_for_wlan_interface() -> Result<()> {
        info!("Mock: Waiting for wlan0 interface");
        sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    async fn start_wifi_services() -> Result<()> {
        let services = ["wlan_keepalive", "pwnagotchi-rs"];
        
        for svc in services {
            debug!("Mock: Started service: {}", svc);
        }
        
        Ok(())
    }

    /// Mock check if WiFi firmware has crashed
    pub fn check_firmware_crash() -> bool {
        false // Mock: no crash on non-Linux
    }

    /// Mock get WiFi chip status
    pub fn get_wifi_chip_status() -> Result<String> {
        Ok("mock: debugfs not available".to_string())
    }
}

// Re-export the platform-appropriate implementation
#[cfg(target_os = "linux")]
pub use linux_gpio::*;

#[cfg(not(target_os = "linux"))]
pub use mock_gpio::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_firmware_crash_no_dmesg() {
        // This will just run and return false if dmesg not available
        let _ = check_firmware_crash();
    }

    #[tokio::test]
    async fn test_wifi_power_control() {
        let mut ctrl = WifiPowerControl::new().unwrap();
        assert!(ctrl.is_powered());
        
        ctrl.power_off().unwrap();
        assert!(!ctrl.is_powered());
        
        ctrl.power_on().unwrap();
        assert!(ctrl.is_powered());
    }
}