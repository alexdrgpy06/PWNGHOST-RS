//! WiFi monitor mode management

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::info;

/// Enable monitor mode on interface
pub async fn set_monitor_mode(iface: &str, enable: bool) -> Result<()> {
    if enable {
        info!("Enabling monitor mode on {}", iface);

        // Bring down
        Command::new("ip")
            .args(["link", "set", iface, "down"])
            .status()
            .await
            .context("Failed to bring interface down")?;

        // Set monitor mode
        Command::new("iw")
            .args(["dev", iface, "set", "type", "monitor"])
            .status()
            .await
            .context("Failed to set monitor mode")?;

        // Bring up
        Command::new("ip")
            .args(["link", "set", iface, "up"])
            .status()
            .await
            .context("Failed to bring interface up")?;

        info!("Monitor mode enabled on {}", iface);
    } else {
        info!("Disabling monitor mode on {}", iface);

        Command::new("ip")
            .args(["link", "set", iface, "down"])
            .status()
            .await?;

        Command::new("iw")
            .args(["dev", iface, "set", "type", "managed"])
            .status()
            .await?;

        Command::new("ip")
            .args(["link", "set", iface, "up"])
            .status()
            .await?;

        info!("Monitor mode disabled on {}", iface);
    }

    Ok(())
}

/// Get monitor interface name
pub fn monitor_interface(iface: &str) -> String {
    if iface.ends_with("mon") {
        iface.to_string()
    } else {
        format!("{}mon", iface)
    }
}

/// Check if interface is in monitor mode
pub async fn is_monitor_mode(iface: &str) -> bool {
    let output = Command::new("iw")
        .args(["dev", iface, "info"])
        .output()
        .await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("type monitor")
        }
        Err(_) => false,
    }
}

/// Scan for available channels
pub async fn scan_channels(iface: &str) -> Result<Vec<u8>> {
    let output = Command::new("iw")
        .args(["dev", iface, "scan"])
        .output()
        .await
        .context("Failed to scan channels")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut channels: Vec<u8> = Vec::new();

    for line in stdout.lines() {
        if line.contains("Channel") || line.contains("channel") {
            // Parse channel from scan output
            // This is simplified - real implementation would parse properly
        }
    }

    // Default to non-overlapping channels
    Ok(vec![1, 6, 11])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_interface() {
        assert_eq!(monitor_interface("wlan0"), "wlan0mon");
        assert_eq!(monitor_interface("wlan0mon"), "wlan0mon");
    }
}