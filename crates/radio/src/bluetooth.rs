//! Bluetooth PAN tethering management

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{info, warn};

/// Connect to Bluetooth PAN
pub async fn connect_pan(phone_mac: &str) -> Result<()> {
    info!("Connecting to Bluetooth PAN: {}", phone_mac);

    // Trust and pair if needed
    trust_device(phone_mac).await?;
    pair_device(phone_mac).await?;

    // Connect PAN
    let status = Command::new("bluetoothctl")
        .args(["connect", phone_mac])
        .status()
        .await
        .context("Failed to run bluetoothctl")?;

    if !status.success() {
        warn!("Bluetooth connection failed, trying alternative");
        // Try via dbus directly
        connect_pan_dbus(phone_mac).await?;
    }

    // Wait for PAN interface
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Request DHCP
    request_dhcp("bnep0").await?;

    info!("Bluetooth PAN connected: {}", phone_mac);
    Ok(())
}

/// Disconnect PAN
pub async fn disconnect_pan() -> Result<()> {
    info!("Disconnecting Bluetooth PAN");

    let _ = Command::new("bluetoothctl")
        .args(["disconnect"])
        .status()
        .await;

    // Bring down PAN interface
    let _ = Command::new("ip")
        .args(["link", "set", "bnep0", "down"])
        .status()
        .await;

    Ok(())
}

/// Trust a Bluetooth device
async fn trust_device(mac: &str) -> Result<()> {
    let status = Command::new("bluetoothctl")
        .args(["trust", mac])
        .status()
        .await?;

    if !status.success() {
        warn!("Failed to trust device {}", mac);
    }
    Ok(())
}

/// Pair with a Bluetooth device
async fn pair_device(mac: &str) -> Result<()> {
    let status = Command::new("bluetoothctl")
        .args(["pair", mac])
        .status()
        .await?;

    if !status.success() {
        warn!("Failed to pair with device {}", mac);
    }
    Ok(())
}

/// Connect PAN via D-Bus directly
async fn connect_pan_dbus(mac: &str) -> Result<()> {
    // Use D-Bus to connect PAN profile
    let status = Command::new("dbus-send")
        .args([
            "--system",
            "--dest=org.bluez",
            "--type=method_call",
            &format!("/org/bluez/hci0/dev_{}", mac.replace(':', "_")),
            "org.bluez.Network1.Connect",
            "string:nap",
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("D-Bus PAN connect failed");
    }
    Ok(())
}

/// Request DHCP on PAN interface
async fn request_dhcp(iface: &str) -> Result<()> {
    // Try dhclient
    let status = Command::new("dhclient")
        .args([iface])
        .status()
        .await;

    if let Ok(s) = status {
        if s.success() {
            return Ok(());
        }
    }

    // Try systemd-networkd
    let _ = Command::new("networkctl")
        .args(["reconfigure", iface])
        .status()
        .await;

    Ok(())
}

/// Scan for Bluetooth devices
pub async fn scan_devices(duration_secs: u64) -> Result<Vec<BluetoothDevice>> {
    info!("Scanning for Bluetooth devices for {}s", duration_secs);

    let output = Command::new("bluetoothctl")
        .args(["--timeout", &duration_secs.to_string(), "scan", "on"])
        .output()
        .await
        .context("Failed to start scan")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in stdout.lines() {
        if line.contains("Device ") {
            // Parse: "Device AA:BB:CC:DD:EE:FF DeviceName"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                devices.push(BluetoothDevice {
                    mac: parts[1].to_string(),
                    name: parts[2..].join(" "),
                    rssi: None,
                });
            }
        }
    }

    // Stop scan
    let _ = Command::new("bluetoothctl")
        .args(["scan", "off"])
        .status()
        .await;

    Ok(devices)
}

/// Bluetooth device info
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub mac: String,
    pub name: String,
    pub rssi: Option<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_module() {
        // Module structure test
    }
}