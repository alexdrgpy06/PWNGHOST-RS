//! Managed WiFi (SAFE mode)

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::info;

/// Connect to known WiFi network
pub async fn connect_known_wifi(ssid: &str, password: &str) -> Result<()> {
    info!("Connecting to WiFi: {}", ssid);

    // Use nmcli for connection
    let status = Command::new("nmcli")
        .args(["device", "wifi", "connect", ssid, "password", password])
        .status()
        .await
        .context("Failed to run nmcli")?;

    if !status.success() {
        // Try wpa_supplicant directly
        connect_wpa_supplicant(ssid, password).await?;
    }

    // Wait for connection
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Verify connection
    if verify_connection().await? {
        info!("Connected to WiFi: {}", ssid);
        Ok(())
    } else {
        anyhow::bail!("Failed to connect to {}", ssid)
    }
}

/// Connect using wpa_supplicant directly
async fn connect_wpa_supplicant(ssid: &str, password: &str) -> Result<()> {
    // Create temporary wpa_supplicant config
    let config = format!(
        r#"network={{
    ssid="{}"
    psk="{}"
    key_mgmt=WPA-PSK
}}"#,
        ssid, password
    );

    let config_path = "/tmp/wpa_supplicant_pwnghost.conf";
    tokio::fs::write(config_path, config).await?;

    // Run wpa_supplicant
    let status = Command::new("wpa_supplicant")
        .args(["-B", "-i", "wlan0", "-c", config_path, "-D", "nl80211"])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("wpa_supplicant failed");
    }

    // Request DHCP
    let _ = Command::new("dhclient").args(["wlan0"]).status().await;

    Ok(())
}

/// Disconnect from WiFi
pub async fn disconnect_wifi() -> Result<()> {
    info!("Disconnecting from WiFi");

    let _ = Command::new("nmcli")
        .args(["device", "disconnect", "wlan0"])
        .status()
        .await;

    let _ = Command::new("wpa_cli").args(["disconnect"]).status().await;

    Ok(())
}

/// Verify internet connectivity
async fn verify_connection() -> Result<bool> {
    // Try to reach a known host
    let output = Command::new("ping")
        .args(["-c", "1", "-W", "2", "8.8.8.8"])
        .output()
        .await?;

    Ok(output.status.success())
}

/// Get current WiFi status
pub async fn get_wifi_status() -> Result<WifiStatus> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "ACTIVE,SSID,SIGNAL", "device", "wifi"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut connected_ssid = None;
    let mut signal = 0;

    for line in stdout.lines() {
        if line.starts_with("yes:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                connected_ssid = Some(parts[1].to_string());
                signal = parts[2].parse().unwrap_or(0);
            }
        }
    }

    Ok(WifiStatus {
        connected: connected_ssid.is_some(),
        ssid: connected_ssid,
        signal,
    })
}

/// WiFi connection status
#[derive(Debug, Clone)]
pub struct WifiStatus {
    pub connected: bool,
    pub ssid: Option<String>,
    pub signal: i32,
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_safe_module() {
        // Module structure test
    }
}
