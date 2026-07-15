//! wlan_keepalive daemon installation and management

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

/// Path where keepalive binary is installed
pub const KEEPALIVE_BINARY_PATH: &str = "/usr/local/bin/wlan_keepalive";
/// Path to systemd service file
pub const KEEPALIVE_SERVICE_PATH: &str = "/etc/systemd/system/wlan_keepalive.service";

/// Embedded wlan_keepalive binary (built separately and embedded via include_bytes!)
/// For now, we'll create a placeholder that installs a script version
pub const KEEPALIVE_SCRIPT: &str = r#"#!/bin/bash
# wlan_keepalive - WiFi keepalive daemon for BCM43436B0
# This script maintains WiFi link to prevent firmware crashes

set -euo pipefail

INTERFACE="${INTERFACE:-wlan0mon}"
PING_INTERVAL="${PING_INTERVAL:-30}"
PING_TARGET="${PING_TARGET:-8.8.8.8}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

# Check if interface exists and is up
check_interface() {
    if ! ip link show "$INTERFACE" &>/dev/null; then
        log "Interface $INTERFACE does not exist"
        return 1
    fi

    if ! ip link show "$INTERFACE" | grep -q "UP"; then
        log "Interface $INTERFACE is not UP"
        return 1
    fi

    return 0
}

# Send keepalive packet
send_keepalive() {
    # Send ARP probe to keep link active
    arping -c 1 -I "$INTERFACE" -q "$PING_TARGET" &>/dev/null || true

    # Also try ping
    ping -c 1 -W 1 -I "$INTERFACE" "$PING_TARGET" &>/dev/null || true
}

main() {
    log "Starting wlan_keepalive for $INTERFACE"

    while true; do
        if check_interface; then
            send_keepalive
            log "Keepalive sent to $PING_TARGET via $INTERFACE"
        else
            log "Interface $INTERFACE not ready, waiting..."
        fi

        sleep "$PING_INTERVAL"
    done
}

main "$@"
"#;

/// Install keepalive binary as a script (for development)
pub async fn install_keepalive_script() -> Result<()> {
    info!("Installing wlan_keepalive script");

    // Write the script
    fs::write(KEEPALIVE_BINARY_PATH, KEEPALIVE_SCRIPT)
        .context("Failed to write keepalive script")?;

    // Make executable
    let mut perms = fs::metadata(KEEPALIVE_BINARY_PATH)
        .context("Failed to get script metadata")?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(KEEPALIVE_BINARY_PATH, perms)
        .context("Failed to set script permissions")?;

    // Create systemd service
    create_keepalive_service().await?;

    info!("wlan_keepalive installed successfully");
    Ok(())
}

/// Create systemd service for keepalive
async fn create_keepalive_service() -> Result<()> {
    let service_content = r#"[Unit]
Description=WiFi Keepalive Daemon for BCM43436B0
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wlan_keepalive
Environment=INTERFACE=wlan0mon
Environment=PING_INTERVAL=30
Environment=PING_TARGET=8.8.8.8
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/run/wlan_keepalive

[Install]
WantedBy=multi-user.target
"#;

    fs::write(KEEPALIVE_SERVICE_PATH, service_content)
        .context("Failed to write systemd service file")?;

    // Reload systemd
    Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await
        .context("Failed to reload systemd")?;

    info!("wlan_keepalive service created");
    Ok(())
}

/// Enable and start keepalive service
pub async fn enable_keepalive() -> Result<()> {
    info!("Enabling wlan_keepalive service");

    Command::new("systemctl")
        .args(["enable", "wlan_keepalive.service"])
        .output()
        .await
        .context("Failed to enable wlan_keepalive service")?;

    Command::new("systemctl")
        .args(["start", "wlan_keepalive.service"])
        .output()
        .await
        .context("Failed to start wlan_keepalive service")?;

    info!("wlan_keepalive service started");
    Ok(())
}

/// Disable keepalive service
pub async fn disable_keepalive() -> Result<()> {
    info!("Disabling wlan_keepalive service");

    let _ = Command::new("systemctl")
        .args(["stop", "wlan_keepalive.service"])
        .output()
        .await;

    let _ = Command::new("systemctl")
        .args(["disable", "wlan_keepalive.service"])
        .output()
        .await;

    Ok(())
}

/// Check if keepalive is running
pub async fn is_keepalive_active() -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", "wlan_keepalive.service"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if keepalive is enabled
pub async fn is_keepalive_enabled() -> bool {
    Command::new("systemctl")
        .args(["is-enabled", "--quiet", "wlan_keepalive.service"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keepalive_script_content() {
        assert!(KEEPALIVE_SCRIPT.contains("#!/bin/bash"));
        assert!(KEEPALIVE_SCRIPT.contains("INTERFACE"));
        assert!(KEEPALIVE_SCRIPT.contains("arping"));
        assert!(KEEPALIVE_SCRIPT.contains("ping"));
    }

    #[test]
    fn test_service_path() {
        assert_eq!(KEEPALIVE_SERVICE_PATH, "/etc/systemd/system/wlan_keepalive.service");
    }
}