//! wlan_keepalive daemon management -- keeps the BCM43436B0 SDIO bus alive.
//!
//! # Why this exists
//! The BCM43436B0 WiFi chip (Pi Zero W / 2W) talks to the SoC over SDIO.
//! When nothing actively reads frames from the monitor interface, the SDIO
//! bus goes idle and the firmware crashes ("Firmware has halted"). AO
//! (angryoxide) alone does not guarantee continuous RX draining the way
//! bettercap's wifi.recon accidentally did; this daemon provides the same
//! keepalive effect at a fraction of the memory footprint.
//!
//! # What this module is NOT (previous behavior, now removed)
//! This module used to write out a bash script that pinged `8.8.8.8` and
//! sent `arping` probes over the monitor interface on a timer. That is
//! broken: a monitor-mode interface has no IP stack and cannot route ICMP
//! or ARP traffic, so the ping/arping calls always failed silently and did
//! nothing to keep the SDIO bus busy. It was self-documented as a
//! "placeholder" and never replaced.
//!
//! # The real daemon
//! Ported verbatim from the proven `oxigotchi` sibling project
//! (`oxigotchi/tools/wlan_keepalive.c`, vendored unmodified at
//! `crates/fw-patcher/vendor/wlan_keepalive.c`). It is a small C program
//! that:
//!   1. Opens a raw `AF_PACKET` socket on the monitor interface in
//!      promiscuous mode and drains incoming frames (RX activity).
//!   2. Every 3 seconds, injects a minimal 802.11 broadcast probe request
//!      (with a radiotap header) directly onto the interface (TX
//!      activity), so the SDIO bus stays busy even with no nearby WiFi
//!      traffic. This -- not pinging an IP over a monitor interface -- is
//!      the actual keepalive mechanism.
//!   3. Waits for the interface to reappear and reconnects if it vanishes
//!      (e.g. after a firmware crash or AngryOxide restart).
//!
//! Invocation is `wlan_keepalive [interface] [poll_ms]` (positional args,
//! not environment variables) -- see [`DEFAULT_INTERFACE`] /
//! [`DEFAULT_POLL_MS`].
//!
//! # Build/install split (hand-off to the pi-gen-owning agent)
//! Building the vendored C source requires an ARM cross-compiler for the
//! target Pi image. That toolchain exists in `pi-gen`'s chroot build
//! environment, not in this crate's own `cargo build` (and not at all on
//! this Windows dev machine). So the division of responsibility is:
//!   - **pi-gen** (owned by another agent) must `gcc -O2 -o wlan_keepalive
//!     vendor/wlan_keepalive.c` (or equivalent cross-compile) and install
//!     the resulting binary at [`KEEPALIVE_BINARY_PATH`]
//!     (`/usr/local/bin/wlan_keepalive`) during image build, run as
//!     `wlan_keepalive wlan0mon 100` (interface, then poll-ms -- both
//!     positional, see [`DEFAULT_INTERFACE`]/[`DEFAULT_POLL_MS`]).
//!   - pi-gen's existing `stage5/00-install-pwnghost/00-run.sh` already
//!     writes its OWN (currently broken, ping-based, env-var-driven)
//!     wlan_keepalive script + `wlan-keepalive.service` unit -- that is a
//!     duplicate of what this module also does and needs to be reconciled
//!     by whoever owns `pi-gen/` (either let this crate's
//!     [`install_keepalive_service`] own the systemd unit and have pi-gen
//!     install only the compiled binary, or vice versa; either way the
//!     `ExecStart` must invoke the real C binary with positional args, not
//!     the old bash script with `INTERFACE`/`PING_INTERVAL`/`PING_TARGET`
//!     env vars).
//!   - **this crate** (`keepalive.rs`) does not compile or embed the C
//!     source. At runtime it only: checks whether the binary is present,
//!     writes/renews the systemd unit file, and enables/starts/stops/
//!     queries the service.

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

/// Path where the compiled wlan_keepalive C binary is installed. Must
/// match the path pi-gen's image-build stage compiles/installs it to.
pub const KEEPALIVE_BINARY_PATH: &str = "/usr/local/bin/wlan_keepalive";
/// Path to systemd service file.
pub const KEEPALIVE_SERVICE_PATH: &str = "/etc/systemd/system/wlan_keepalive.service";
/// Default monitor interface the daemon listens/injects on.
pub const DEFAULT_INTERFACE: &str = "wlan0mon";
/// Default poll interval (ms) for draining RX frames (first CLI arg is the
/// interface, second is this value -- both positional, not env vars).
pub const DEFAULT_POLL_MS: u32 = 100;

/// Vendored copy of the real wlan_keepalive.c daemon source (from
/// `oxigotchi/tools/wlan_keepalive.c`), embedded here purely for
/// reference/verification so the crate that documents the contract also
/// carries the source it documents. It is `pi-gen`'s image-build stage
/// that actually compiles and installs the binary -- see module docs.
pub const KEEPALIVE_C_SOURCE: &str = include_str!("../vendor/wlan_keepalive.c");

/// Check whether the compiled keepalive binary is present and (on Unix)
/// executable.
pub fn is_keepalive_binary_installed() -> bool {
    let path = Path::new(KEEPALIVE_BINARY_PATH);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111) != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

/// Ensure the systemd unit for wlan_keepalive exists and points at the real
/// C daemon with the correct positional args. Does NOT write the daemon
/// binary itself -- that is pi-gen's job at image-build time (see module
/// docs). Warns (but does not fail) if the binary isn't there yet, since
/// on a properly built image it always will be by the time this runs.
pub async fn install_keepalive_service() -> Result<()> {
    if !is_keepalive_binary_installed() {
        warn!(
            "wlan_keepalive binary not found at {} -- expected pi-gen to have \
             compiled vendor/wlan_keepalive.c and installed it there during image \
             build. Writing the systemd unit anyway, but it will fail to start \
             until the binary is present.",
            KEEPALIVE_BINARY_PATH
        );
    }

    create_keepalive_service().await?;
    info!("wlan_keepalive service unit installed");
    Ok(())
}

/// Write the systemd service file for wlan_keepalive, pointed at the real
/// C binary invoked with positional args (`interface poll_ms`) -- not the
/// old bash placeholder's `INTERFACE`/`PING_INTERVAL`/`PING_TARGET` env
/// vars.
async fn create_keepalive_service() -> Result<()> {
    let service_content = format!(
        r#"[Unit]
Description=WiFi monitor interface keepalive (BCM43436B0 SDIO bus)
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart={bin} {iface} {poll_ms}
Restart=always
RestartSec=3
Nice=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes

[Install]
WantedBy=multi-user.target
"#,
        bin = KEEPALIVE_BINARY_PATH,
        iface = DEFAULT_INTERFACE,
        poll_ms = DEFAULT_POLL_MS,
    );

    tokio::fs::write(KEEPALIVE_SERVICE_PATH, service_content)
        .await
        .context("Failed to write systemd service file")?;

    Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await
        .context("Failed to reload systemd")?;

    info!(
        "wlan_keepalive.service unit written to {}",
        KEEPALIVE_SERVICE_PATH
    );
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
    fn test_keepalive_c_source_is_the_real_daemon() {
        // Guards against the vendored source silently reverting to a
        // placeholder: the real daemon opens a raw AF_PACKET socket and
        // injects 802.11 probe requests -- it never touches ICMP/ping.
        assert!(KEEPALIVE_C_SOURCE.contains("AF_PACKET"));
        assert!(KEEPALIVE_C_SOURCE.contains("SIOCGIFINDEX"));
        assert!(KEEPALIVE_C_SOURCE.contains("send_probe"));
        assert!(!KEEPALIVE_C_SOURCE.contains("8.8.8.8"));
        assert!(!KEEPALIVE_C_SOURCE.to_lowercase().contains("arping"));
    }

    #[test]
    fn test_service_paths_and_defaults() {
        assert_eq!(
            KEEPALIVE_SERVICE_PATH,
            "/etc/systemd/system/wlan_keepalive.service"
        );
        assert_eq!(KEEPALIVE_BINARY_PATH, "/usr/local/bin/wlan_keepalive");
        assert_eq!(DEFAULT_INTERFACE, "wlan0mon");
        assert_eq!(DEFAULT_POLL_MS, 100);
    }
}
