//! AngryOxide configuration and CLI argument building

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

/// Configuration for AngryOxide subprocess
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngryOxideConfig {
    /// Path to the angryoxide binary
    pub binary: String,

    /// Interface to use (e.g., "wlan0"). AngryOxide manages monitor mode
    /// itself via netlink, so this must be a normal interface name - NOT a
    /// pre-existing `<iface>mon` monitor-mode interface.
    pub interface: String,

    /// Channels to scan (comma-separated)
    pub channels: Option<String>,

    /// Band to scan (2, 5, 6, 60)
    pub band: Option<u8>,

    /// Output file prefix
    pub output: Option<PathBuf>,

    /// Attack rate (1, 2, 3)
    pub rate: u8,

    /// Target MAC/SSID entries
    pub targets: Vec<String>,

    /// Whitelist MAC/SSID entries
    pub whitelist: Vec<String>,

    /// Target list file
    pub target_list: Option<PathBuf>,

    /// Whitelist file
    pub whitelist_file: Option<PathBuf>,

    /// Headless mode (no TUI)
    pub headless: bool,

    /// Auto-exit when all targets have handshake
    pub auto_exit: bool,

    /// Passive only (no transmit)
    pub no_transmit: bool,

    /// Don't tar output files
    pub no_tar: bool,

    /// Disable mouse capture
    pub no_mouse: bool,

    /// Dwell time in seconds
    pub dwell: u8,

    /// GPSD host:port
    pub gpsd: Option<String>,

    /// Auto-hunt channels with targets
    pub auto_hunt: bool,

    /// Rogue MAC for rogue attacks
    pub rogue_mac: Option<String>,

    /// Disable specific attacks
    pub disable_deauth: bool,
    pub disable_pmkid: bool,
    pub disable_anon: bool,
    pub disable_csa: bool,
    pub disable_disassoc: bool,
    pub disable_roguem2: bool,

    /// Combine all hc22000 files
    pub combine: bool,

    /// Geofencing options
    pub geofence: bool,
    pub geofence_center: Option<String>,
    pub geofence_distance: Option<u32>,
    pub geofence_timeout: Option<u32>,
}

impl Default for AngryOxideConfig {
    fn default() -> Self {
        Self {
            binary: "/usr/local/bin/angryoxide".to_string(),
            interface: "wlan0".to_string(),
            // "1,6,11" are AP-*planning* non-overlapping channels, not a
            // hardware limit -- restricting a monitor/attack tool to
            // just those three leaves it blind to every real AP sitting
            // on 2/3/4/5/7/8/9/10/12/13 (routers auto-select across the
            // whole band). This project's target hardware (BCM43430 on
            // Pi Zero W, BCM43436B0 on Pi Zero 2W) is 2.4GHz-only, so
            // `-b 2` ("scan the whole 2.4GHz band") is the correct,
            // hardware-honest default -- AngryOxide queries the
            // interface's own actual supported/regulatory-allowed
            // channel list for that band instead of us guessing one.
            channels: None,
            band: Some(2),
            output: None,
            rate: 2,
            targets: Vec::new(),
            whitelist: Vec::new(),
            target_list: None,
            whitelist_file: None,
            headless: true,
            auto_exit: false,
            no_transmit: false,
            no_tar: true,
            no_mouse: true,
            dwell: 2,
            gpsd: Some("127.0.0.1:2947".to_string()),
            auto_hunt: false,
            rogue_mac: None,
            disable_deauth: false,
            disable_pmkid: false,
            disable_anon: false,
            disable_csa: false,
            disable_disassoc: false,
            disable_roguem2: false,
            combine: false,
            geofence: false,
            geofence_center: None,
            geofence_distance: None,
            geofence_timeout: None,
        }
    }
}

/// Build AngryOxide command line arguments
pub fn build_args(config: &AngryOxideConfig) -> Result<Vec<String>> {
    let mut args = Vec::new();

    // Interface (required)
    args.push("-i".to_string());
    args.push(config.interface.clone());

    // Channels
    if let Some(ch) = &config.channels {
        args.push("-c".to_string());
        args.push(ch.clone());
    } else if let Some(b) = config.band {
        args.push("-b".to_string());
        args.push(b.to_string());
    }

    // Output file
    if let Some(out) = &config.output {
        args.push("-o".to_string());
        args.push(out.to_string_lossy().to_string());
    }

    // Rate
    args.push("-r".to_string());
    args.push(config.rate.to_string());

    // Targets
    for target in &config.targets {
        args.push("-t".to_string());
        args.push(target.clone());
    }

    // Whitelist
    for wl in &config.whitelist {
        args.push("-w".to_string());
        args.push(wl.clone());
    }

    // Target list file
    if let Some(tl) = &config.target_list {
        args.push("--targetlist".to_string());
        args.push(tl.to_string_lossy().to_string());
    }

    // Whitelist file
    if let Some(wl) = &config.whitelist_file {
        args.push("--whitelist".to_string());
        args.push(wl.to_string_lossy().to_string());
    }

    // Headless mode
    if config.headless {
        args.push("--headless".to_string());
    }

    // Auto exit
    if config.auto_exit {
        args.push("--autoexit".to_string());
    }

    // No transmit
    if config.no_transmit {
        args.push("--notransmit".to_string());
    }

    // No tar
    if config.no_tar {
        args.push("--notar".to_string());
    }

    // No mouse
    if config.no_mouse {
        args.push("--disablemouse".to_string());
    }

    // Dwell time
    args.push("--dwell".to_string());
    args.push(config.dwell.to_string());

    // GPSD
    if let Some(gpsd) = &config.gpsd {
        args.push("--gpsd".to_string());
        args.push(gpsd.clone());
    }

    // Auto hunt
    if config.auto_hunt {
        args.push("--autohunt".to_string());
    }

    // Rogue MAC
    if let Some(mac) = &config.rogue_mac {
        args.push("--rogue".to_string());
        args.push(mac.clone());
    }

    // Disable attacks
    if config.disable_deauth {
        args.push("--disable-deauth".to_string());
    }
    if config.disable_pmkid {
        args.push("--disable-pmkid".to_string());
    }
    if config.disable_anon {
        args.push("--disable-anon".to_string());
    }
    if config.disable_csa {
        args.push("--disable-csa".to_string());
    }
    if config.disable_disassoc {
        args.push("--disable-disassoc".to_string());
    }
    if config.disable_roguem2 {
        args.push("--disable-roguem2".to_string());
    }

    // Combine
    if config.combine {
        args.push("--combine".to_string());
    }

    // Geofencing
    if config.geofence {
        args.push("--geofence".to_string());
        if let Some(center) = &config.geofence_center {
            args.push("--center".to_string());
            args.push(center.clone());
        }
        if let Some(dist) = config.geofence_distance {
            args.push("--distance".to_string());
            args.push(dist.to_string());
        }
        if let Some(timeout) = config.geofence_timeout {
            args.push("--geofence-timeout".to_string());
            args.push(timeout.to_string());
        }
    }

    debug!("Built AO args: {:?}", args);
    Ok(args)
}

/// Generate args for a specific personality/profile
pub fn build_personality_args(
    personality: &str,
    base_config: &AngryOxideConfig,
) -> Result<Vec<String>> {
    let mut config = base_config.clone();

    match personality {
        "aggressive" => {
            config.rate = 3;
            config.dwell = 1;
            config.auto_hunt = true;
        }
        "stealth" => {
            config.rate = 1;
            config.dwell = 5;
            config.disable_deauth = true;
            config.disable_disassoc = true;
        }
        "pmkid_only" => {
            config.disable_deauth = true;
            config.disable_anon = true;
            config.disable_csa = true;
            config.disable_disassoc = true;
            config.disable_roguem2 = true;
        }
        "handshake_only" => {
            config.disable_pmkid = true;
        }
        "passive" => {
            config.no_transmit = true;
        }
        _ => {} // "balanced" - use defaults
    }

    build_args(&config)
}

/// Validate configuration
pub fn validate_config(config: &AngryOxideConfig) -> Result<()> {
    if config.interface.is_empty() {
        anyhow::bail!("Interface must not be empty");
    }

    if config.rate > 3 {
        anyhow::bail!("Rate must be 1, 2, or 3");
    }

    if config.dwell == 0 {
        anyhow::bail!("Dwell time must be > 0");
    }

    if config.geofence {
        if config.geofence_center.is_none() {
            anyhow::bail!("Geofence requires --center");
        }
        if config.geofence_distance.is_none() {
            anyhow::bail!("Geofence requires --distance");
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_build_basic_args() {
        let config = AngryOxideConfig::default();
        let args = build_args(&config).unwrap();

        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"wlan0".to_string()));
        // Default scans the whole 2.4GHz band (`-b 2`), not a hardcoded
        // 3-channel subset -- see the doc comment on
        // `AngryOxideConfig::default()`'s `band` field.
        assert!(args.contains(&"-b".to_string()));
        assert!(args.contains(&"2".to_string()));
        assert!(!args.contains(&"-c".to_string()));
        assert!(args.contains(&"--headless".to_string()));
    }

    #[test]
    fn test_explicit_channels_take_precedence_over_band() {
        let mut config = AngryOxideConfig::default();
        config.channels = Some("1,6,11".to_string());
        let args = build_args(&config).unwrap();

        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"1,6,11".to_string()));
        assert!(!args.contains(&"-b".to_string()));
    }

    #[test]
    fn test_build_with_targets() {
        let mut config = AngryOxideConfig::default();
        config.targets = vec!["aa:bb:cc:dd:ee:ff".to_string()];
        config.whitelist = vec!["MyNetwork".to_string()];

        let args = build_args(&config).unwrap();

        assert!(args.contains(&"-t".to_string()));
        assert!(args.contains(&"aa:bb:cc:dd:ee:ff".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"MyNetwork".to_string()));
    }

    #[test]
    fn test_validate_config() {
        let config = AngryOxideConfig::default();
        assert!(validate_config(&config).is_ok());

        let mut config = AngryOxideConfig::default();
        config.interface = "".to_string();
        assert!(validate_config(&config).is_err());

        let mut config = AngryOxideConfig::default();
        config.rate = 5;
        assert!(validate_config(&config).is_err());

        let mut config = AngryOxideConfig::default();
        config.dwell = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_personality_args() {
        let config = AngryOxideConfig::default();

        let aggressive = build_personality_args("aggressive", &config).unwrap();
        assert!(aggressive.contains(&"-r".to_string()));
        assert!(aggressive.contains(&"3".to_string()));
        assert!(aggressive.contains(&"--autohunt".to_string()));

        let stealth = build_personality_args("stealth", &config).unwrap();
        assert!(stealth.contains(&"--disable-deauth".to_string()));
        assert!(stealth.contains(&"--disable-disassoc".to_string()));

        let passive = build_personality_args("passive", &config).unwrap();
        assert!(passive.contains(&"--notransmit".to_string()));
    }
}
