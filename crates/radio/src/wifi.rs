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

/// Bring a monitor-mode interface link up/down WITHOUT changing its `iw`
/// device type (unlike [`set_monitor_mode`], which tears down monitor mode
/// entirely and switches back to `managed`).
///
/// This is the lightweight pause primitive used for a brief BT scan/pairing
/// window on chips that share one radio between WiFi and BT (BCM43436B0):
/// AO stops, the monitor interface goes link-down for the scan's duration,
/// then link-up again -- monitor mode itself is never torn down, so
/// resuming is just bringing the link back up, no `iw ... set type
/// monitor` re-negotiation with the driver. Ported from oxigotchi's
/// `wifi::pause_for_bt`/`resume_from_pause` (confirmed via a fresh audit of
/// their `rust/src/wifi.rs`, 2026-07-18), which uses the identical
/// down/up-only approach specifically to avoid a full radio-mode
/// transition on every BT device-discovery scan.
pub async fn set_link_state(iface: &str, up: bool) -> Result<()> {
    let action = if up { "up" } else { "down" };
    info!("Setting {} link {}", iface, action);
    Command::new("ip")
        .args(["link", "set", iface, action])
        .status()
        .await
        .with_context(|| format!("Failed to bring {iface} link {action}"))?;
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
        // `iw scan` reports the channel as "DS Parameter set: channel 6".
        if let Some(idx) = line.find("channel ") {
            if let Ok(ch) = line[idx + "channel ".len()..].trim().parse::<u8>() {
                if (1..=14).contains(&ch) && !channels.contains(&ch) {
                    channels.push(ch);
                }
            }
        }
    }

    if channels.is_empty() {
        // Default to non-overlapping channels when nothing was parsed.
        channels = vec![1, 6, 11];
    }
    channels.sort_unstable();
    Ok(channels)
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
