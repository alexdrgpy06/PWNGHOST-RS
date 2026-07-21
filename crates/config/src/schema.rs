//! Configuration schema for PWNGHOST-RS

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwnConfig {
    #[serde(default)]
    pub main: MainConfig,

    #[serde(default)]
    pub personality: PersonalityConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub bettercap: BettercapConfig,

    #[serde(default)]
    pub fs: FsConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

impl Default for PwnConfig {
    fn default() -> Self {
        Self {
            main: MainConfig::default(),
            personality: PersonalityConfig::default(),
            ui: UiConfig::default(),
            bettercap: BettercapConfig::default(),
            fs: FsConfig::default(),
            agent: AgentConfig::default(),
            plugins: default_plugins(),
        }
    }
}

/// Runtime/loop tuning for the agent -- this is **our own** config section,
/// not related to the sibling `oxigotchi` Rust project (studied only as
/// prior-art reference, never integrated; see `REWORK_PLAN.md`). It was
/// previously misnamed `OxigotchiConfig`/`[oxigotchi]`, which read as
/// leftover config from a project this codebase doesn't use -- renamed for
/// clarity, no behavior change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Duration of one agent epoch, in seconds.
    #[serde(default = "default_epoch_duration")]
    pub epoch_duration: u64,
}

fn default_epoch_duration() -> u64 {
    15
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            epoch_duration: default_epoch_duration(),
        }
    }
}

impl PwnConfig {
    /// Validate and fix up configuration
    pub async fn validate_and_fix(&mut self) -> Result<()> {
        // Ensure required directories exist
        let dirs = vec![
            self.main.handshakes_dir(),
            self.main.log_dir(),
            self.main.backup_dir(),
            self.main.sessions_dir(),
        ];

        for dir in &dirs {
            if let Err(e) = tokio::fs::create_dir_all(dir).await {
                if e.kind() != std::io::ErrorKind::PermissionDenied {
                    return Err(e.into());
                }
            }
        }

        // Validate personality config
        self.personality.validate()?;

        // Validate UI config
        self.ui.validate()?;

        Ok(())
    }
}

fn default_plugins() -> HashMap<String, PluginConfig> {
    let mut plugins = HashMap::new();
    // Safe-by-default, no external accounts/credentials or optional
    // hardware required: on out of the box.
    for name in [
        "auto_tune",
        "auto_backup",
        "auto_update",
        "cache",
        "fix_services",
        "grid",
        "webcfg",
        "pwnstore_ui",
    ] {
        plugins.insert(
            name.to_string(),
            PluginConfig {
                enabled: true,
                options: HashMap::new(),
            },
        );
    }
    // Opt-in: either uploads to a third-party service and needs a
    // credential the user hasn't set yet (wpa_sec, wigle, ohcapi), or
    // depends on optional hardware/tooling not every install has
    // (bt_tether, gpio_buttons, gps, memtemp, pisugarx, ups_lite,
    // webgpsmap), or is otherwise not something every user wants running
    // by default (logtail, pwncrack, session_stats). **Previously this
    // whole function set every plugin to `enabled: true` unconditionally,
    // silently shipping upload plugins active with no credential set and
    // every optional-hardware plugin polling for hardware that usually
    // isn't there -- this list is what `defaults.toml` already documented
    // as the intended defaults, which this function had drifted out of
    // sync with (that file itself is reference documentation only; it is
    // never parsed at runtime -- `PwnConfig::default()` here is the real
    // source of truth).**
    for name in [
        "bt_tether",
        "gps",
        "logtail",
        "memtemp",
        "ohcapi",
        "pisugarx",
        "pwncrack",
        "session_stats",
        "ups_lite",
        "webgpsmap",
        "wigle",
        "wpa_sec",
    ] {
        plugins.insert(
            name.to_string(),
            PluginConfig {
                enabled: false,
                options: HashMap::new(),
            },
        );
    }
    // gpio_buttons: opt-in (needs a physical button wired up), with the
    // PiSugar S button's GPIO3 as the default pin -- see gpio_buttons.lua's
    // doc comment for the hardware caveat (shares I2C1 SCL).
    let mut gpio_buttons_options = HashMap::new();
    gpio_buttons_options.insert("pin".to_string(), serde_json::Value::from(3));
    gpio_buttons_options.insert("long_press_secs".to_string(), serde_json::Value::from(3));
    plugins.insert(
        "gpio_buttons".to_string(),
        PluginConfig {
            enabled: false,
            options: gpio_buttons_options,
        },
    );
    plugins
}

/// Main configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    #[serde(default = "default_name")]
    pub name: String,

    #[serde(default = "default_lang")]
    pub lang: String,

    #[serde(default = "default_iface")]
    pub iface: String,

    #[serde(default = "default_mon_start_cmd")]
    pub mon_start_cmd: String,

    #[serde(default = "default_mon_stop_cmd")]
    pub mon_stop_cmd: String,

    #[serde(default = "default_max_blind_epochs")]
    pub mon_max_blind_epochs: u32,

    #[serde(default)]
    pub no_restart: bool,

    #[serde(default)]
    pub whitelist: Vec<String>,

    #[serde(default = "default_confd")]
    pub confd: String,

    #[serde(default)]
    pub custom_plugin_repos: Vec<String>,

    #[serde(default = "default_custom_plugins")]
    pub custom_plugins: String,

    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,

    #[serde(default)]
    pub log: LogConfig,
}

fn default_name() -> String {
    "pwnghost".to_string()
}
fn default_lang() -> String {
    "en".to_string()
}
fn default_iface() -> String {
    // bettercap (Phase 1) needs the real nexmon monitor-mode interface,
    // brought up by `monstart` (mon_start_cmd) -- matches real pwnagotchi's
    // own default (`pwnagotchi/defaults.toml`: `iface = "wlan0mon"`).
    "wlan0mon".to_string()
}
fn default_mon_start_cmd() -> String {
    "/usr/bin/monstart".to_string()
}
fn default_mon_stop_cmd() -> String {
    "/usr/bin/monstop".to_string()
}
fn default_max_blind_epochs() -> u32 {
    5
}
fn default_confd() -> String {
    "/etc/pwnghost/conf.d/".to_string()
}
fn default_custom_plugins() -> String {
    "/usr/local/share/pwnghost/custom-plugins/".to_string()
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            lang: default_lang(),
            iface: default_iface(),
            mon_start_cmd: default_mon_start_cmd(),
            mon_stop_cmd: default_mon_stop_cmd(),
            mon_max_blind_epochs: default_max_blind_epochs(),
            no_restart: false,
            whitelist: vec![],
            confd: default_confd(),
            custom_plugin_repos: vec![],
            custom_plugins: default_custom_plugins(),
            plugins: HashMap::new(),
            log: LogConfig::default(),
        }
    }
}

impl MainConfig {
    pub fn handshakes_dir(&self) -> PathBuf {
        PathBuf::from("/etc/pwnghost/handshakes")
    }
    pub fn log_dir(&self) -> PathBuf {
        PathBuf::from("/etc/pwnghost/log")
    }
    pub fn backup_dir(&self) -> PathBuf {
        PathBuf::from("/etc/pwnghost/backups")
    }
    pub fn sessions_dir(&self) -> PathBuf {
        PathBuf::from("/etc/pwnghost/sessions")
    }
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_log_path")]
    pub path: String,

    #[serde(default = "default_log_debug_path")]
    pub path_debug: String,

    #[serde(default)]
    pub rotation: LogRotationConfig,
}

fn default_log_path() -> String {
    "/etc/pwnghost/log/pwnghost.log".to_string()
}
fn default_log_debug_path() -> String {
    "/etc/pwnghost/log/pwnghost-debug.log".to_string()
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            path: default_log_path(),
            path_debug: default_log_debug_path(),
            rotation: LogRotationConfig::default(),
        }
    }
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_log_size")]
    pub size: String,
}

fn default_log_size() -> String {
    "10M".to_string()
}

impl Default for LogRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size: default_log_size(),
        }
    }
}

/// Personality configuration (matches pwnagotchi personality.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    // Mood thresholds
    #[serde(default = "default_bored_epochs")]
    pub bored_num_epochs: u64,

    #[serde(default = "default_sad_epochs")]
    pub sad_num_epochs: u64,

    #[serde(default = "default_angry_epochs")]
    pub angry_num_epochs: u64,

    #[serde(default = "default_lonely_epochs")]
    pub lonely_num_epochs: u64,

    // Activity factors
    #[serde(default = "default_bond_factor")]
    pub bond_encounters_factor: f32,

    #[serde(default = "default_max_interactions")]
    pub max_interactions: u32,

    #[serde(default = "default_throttle")]
    pub throttle: u32,

    // Rewards
    #[serde(default = "default_reward_handshake")]
    pub reward_handshake: i32,

    #[serde(default = "default_reward_new_ap")]
    pub reward_new_ap: i32,

    #[serde(default = "default_reward_assoc")]
    pub reward_association: i32,

    #[serde(default = "default_penalty_missed")]
    pub penalty_missed: i32,

    #[serde(default = "default_penalty_reboot")]
    pub penalty_reboot: i32,

    // Behavior
    #[serde(default = "default_min_recon")]
    pub min_recon_time: u64,

    #[serde(default = "default_max_recon")]
    pub max_recon_time: u64,

    #[serde(default = "default_hop_recon")]
    pub hop_recon_time: u64,

    // Attack settings
    #[serde(default)]
    pub deauth: bool,

    #[serde(default)]
    pub associate: bool,

    #[serde(default = "default_min_rssi")]
    pub min_rssi: i16,

    // Display
    #[serde(default)]
    pub position_x: i32,

    #[serde(default)]
    pub position_y: i32,

    #[serde(default = "default_frame_padding")]
    pub frame_padding: bool,

    #[serde(default = "default_frame_padding_min")]
    pub frame_padding_min_bytes: usize,
}

fn default_bored_epochs() -> u64 {
    50
}
fn default_sad_epochs() -> u64 {
    100
}
fn default_angry_epochs() -> u64 {
    200
}
fn default_lonely_epochs() -> u64 {
    150
}
fn default_bond_factor() -> f32 {
    1.0
}
fn default_max_interactions() -> u32 {
    10
}
fn default_throttle() -> u32 {
    30
}
fn default_reward_handshake() -> i32 {
    100
}
fn default_reward_new_ap() -> i32 {
    10
}
fn default_reward_assoc() -> i32 {
    5
}
fn default_penalty_missed() -> i32 {
    -10
}
fn default_penalty_reboot() -> i32 {
    -50
}
fn default_min_recon() -> u64 {
    5
}
fn default_max_recon() -> u64 {
    30
}
fn default_hop_recon() -> u64 {
    10
}
fn default_min_rssi() -> i16 {
    -80
}
fn default_frame_padding() -> bool {
    true
}
fn default_frame_padding_min() -> usize {
    650
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            bored_num_epochs: default_bored_epochs(),
            sad_num_epochs: default_sad_epochs(),
            angry_num_epochs: default_angry_epochs(),
            lonely_num_epochs: default_lonely_epochs(),
            bond_encounters_factor: default_bond_factor(),
            max_interactions: default_max_interactions(),
            throttle: default_throttle(),
            reward_handshake: default_reward_handshake(),
            reward_new_ap: default_reward_new_ap(),
            reward_association: default_reward_assoc(),
            penalty_missed: default_penalty_missed(),
            penalty_reboot: default_penalty_reboot(),
            min_recon_time: default_min_recon(),
            max_recon_time: default_max_recon(),
            hop_recon_time: default_hop_recon(),
            // Real pwnagotchi's core behavior: actively deauth discovered
            // APs to force handshakes. When enabled, the agent issues
            // deauth/assoc against targeted APs via the bettercap backend.
            deauth: true,
            associate: true,
            min_rssi: default_min_rssi(),
            position_x: 0,
            position_y: 34,
            frame_padding: default_frame_padding(),
            frame_padding_min_bytes: default_frame_padding_min(),
        }
    }
}

impl PersonalityConfig {
    pub fn validate(&self) -> Result<()> {
        if self.min_recon_time > self.max_recon_time {
            anyhow::bail!("min_recon_time must be <= max_recon_time");
        }
        if self.hop_recon_time > self.max_recon_time {
            anyhow::bail!("hop_recon_time must be <= max_recon_time");
        }
        Ok(())
    }

    /// Calculate recon time based on epoch state
    pub fn calc_recon_time(&self, epoch: &pwncore::EpochState) -> u64 {
        let base = self.min_recon_time;
        let max = self.max_recon_time;
        let ap_bonus = (epoch.aps_found as u64 * 2).min(10);
        (base + ap_bonus).clamp(base, max)
    }

    /// Calculate hop time based on epoch state
    pub fn calc_hop_time(&self, epoch: &pwncore::EpochState) -> u64 {
        let base = self.hop_recon_time;
        if epoch.aps_found == 0 {
            return base / 2;
        }
        let elapsed = epoch.duration().as_secs();
        if elapsed >= base {
            return 0;
        }
        base - elapsed
    }
}

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default)]
    pub web: WebUiConfig,

    #[serde(default)]
    pub display: DisplayUiConfig,

    #[serde(default)]
    pub faces: FacesConfig,
}

impl UiConfig {
    pub fn validate(&self) -> Result<()> {
        self.web.validate()?;
        self.display.validate()?;
        Ok(())
    }
}

/// Web UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_web_address")]
    pub address: String,

    #[serde(default)]
    pub auth: bool,

    #[serde(default = "default_web_user")]
    pub username: String,

    #[serde(default = "default_web_pass")]
    pub password: String,

    #[serde(default)]
    pub origin: String,

    #[serde(default = "default_web_port")]
    pub port: u16,

    #[serde(default)]
    pub on_frame: String,

    #[serde(default)]
    pub theme: WebThemeConfig,
}

fn default_web_address() -> String {
    "0.0.0.0".to_string()
}
fn default_web_user() -> String {
    "changeme".to_string()
}
fn default_web_pass() -> String {
    "changeme".to_string()
}
fn default_web_port() -> u16 {
    8080
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            address: default_web_address(),
            auth: false,
            username: default_web_user(),
            password: default_web_pass(),
            origin: String::new(),
            port: default_web_port(),
            on_frame: String::new(),
            theme: WebThemeConfig::default(),
        }
    }
}

impl WebUiConfig {
    pub fn validate(&self) -> Result<()> {
        if self.port == 0 {
            anyhow::bail!("Web UI port must be > 0");
        }
        Ok(())
    }
}

/// Web UI theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebThemeConfig {
    #[serde(default = "default_accent_r")]
    pub accent_r: u8,

    #[serde(default = "default_accent_g")]
    pub accent_g: u8,

    #[serde(default = "default_accent_b")]
    pub accent_b: u8,
}

fn default_accent_r() -> u8 {
    76
}
fn default_accent_g() -> u8 {
    175
}
fn default_accent_b() -> u8 {
    80
}

impl Default for WebThemeConfig {
    fn default() -> Self {
        Self {
            accent_r: default_accent_r(),
            accent_g: default_accent_g(),
            accent_b: default_accent_b(),
        }
    }
}

/// Display UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayUiConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_rotation")]
    pub rotation: u16,

    #[serde(default = "default_display_type")]
    pub display_type: String,
}

fn default_rotation() -> u16 {
    180
}
fn default_display_type() -> String {
    "waveshare_v4".to_string()
}

impl Default for DisplayUiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rotation: default_rotation(),
            display_type: default_display_type(),
        }
    }
}

impl DisplayUiConfig {
    pub fn validate(&self) -> Result<()> {
        if self.rotation > 360 {
            anyhow::bail!("Display rotation must be <= 360");
        }
        Ok(())
    }
}

/// Faces configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacesConfig {
    #[serde(default)]
    pub png: bool,

    #[serde(default)]
    pub position_x: i32,

    #[serde(default)]
    pub position_y: i32,

    #[serde(skip)]
    pub face_paths: HashMap<String, String>,
}

impl Default for FacesConfig {
    fn default() -> Self {
        Self {
            png: true,
            position_x: 0,
            // Matches real pwnagotchi's `ui.faces.position_y` default
            // (pwnagotchi/ui/view.py: face position comes directly from
            // this config value, not the per-panel layout dict) --
            // confirmed against a real jayofelony v2.8.9 device's
            // /usr/local/lib/python3.9/dist-packages/pwnagotchi/ui/view.py.
            // Previously 16, which rendered the face noticeably higher
            // than the original.
            position_y: 34,
            face_paths: HashMap::new(),
        }
    }
}

/// Bettercap configuration -- connection settings for the real bettercap
/// REST API this agent now drives directly (`crates/bettercap`). bettercap
/// runs as a separate process/systemd unit (same architecture as real
/// pwnagotchi: `pwnagotchi/bettercap.py`'s `Client` talks to bettercap over
/// this exact REST API), bound to loopback only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BettercapConfig {
    #[serde(default = "default_handshakes_path")]
    pub handshakes: String,

    #[serde(default = "default_silence")]
    pub silence: Vec<String>,

    /// bettercap's REST API host. Always loopback in this project's setup
    /// (`api.rest.address 127.0.0.1` in the bettercap systemd unit) -- never
    /// exposed to the network.
    #[serde(default = "default_bettercap_hostname")]
    pub hostname: String,

    #[serde(default = "default_bettercap_port")]
    pub port: u16,

    #[serde(default = "default_bettercap_username")]
    pub username: String,

    #[serde(default = "default_bettercap_password")]
    pub password: String,
}

fn default_bettercap_hostname() -> String {
    "127.0.0.1".to_string()
}
fn default_bettercap_port() -> u16 {
    8081
}
fn default_bettercap_username() -> String {
    "pwnghost".to_string()
}
fn default_bettercap_password() -> String {
    "pwnghost".to_string()
}

fn default_handshakes_path() -> String {
    "/etc/pwnghost/handshakes".to_string()
}
pub fn default_silence() -> Vec<String> {
    vec![
        "ble.device.new".to_string(),
        "ble.device.lost".to_string(),
        "ble.device.service.discovered".to_string(),
        "ble.device.characteristic.discovered".to_string(),
        "ble.device.disconnected".to_string(),
        "ble.device.connected".to_string(),
        "ble.connection.timeout".to_string(),
        "wifi.client.new".to_string(),
        "wifi.client.lost".to_string(),
        "wifi.client.probe".to_string(),
        "wifi.ap.new".to_string(),
        "wifi.ap.lost".to_string(),
        "mod.started".to_string(),
    ]
}

impl Default for BettercapConfig {
    fn default() -> Self {
        Self {
            handshakes: default_handshakes_path(),
            silence: default_silence(),
            hostname: default_bettercap_hostname(),
            port: default_bettercap_port(),
            username: default_bettercap_username(),
            password: default_bettercap_password(),
        }
    }
}

/// Filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default)]
    pub mounts: HashMap<String, FsMountConfig>,
}

impl Default for FsConfig {
    fn default() -> Self {
        let mut mounts = HashMap::new();
        mounts.insert(
            "log".to_string(),
            FsMountConfig {
                enabled: true,
                mount: "/etc/pwnghost/log/".to_string(),
                size: "50M".to_string(),
                sync: 60,
                zram: true,
                rsync: true,
            },
        );
        mounts.insert(
            "data".to_string(),
            FsMountConfig {
                enabled: true,
                mount: "/var/tmp/pwnghost".to_string(),
                size: "10M".to_string(),
                sync: 3600,
                zram: true,
                rsync: true,
            },
        );
        Self {
            enabled: true,
            mounts,
        }
    }
}

/// Filesystem mount configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsMountConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    pub mount: String,

    pub size: String,

    #[serde(default = "default_sync")]
    pub sync: u32,

    #[serde(default = "default_true")]
    pub zram: bool,

    #[serde(default = "default_true")]
    pub rsync: bool,
}

fn default_sync() -> u32 {
    60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personality_validate() {
        let mut p = PersonalityConfig::default();
        assert!(p.validate().is_ok());

        p.min_recon_time = 30;
        p.max_recon_time = 10;
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_calc_recon_time() {
        let p = PersonalityConfig::default();
        let epoch = pwncore::EpochState {
            epoch: 1,
            channel: pwncore::Channel::new(1).unwrap(),
            mode: pwncore::AgentMode::Recon,
            aps_found: 5,
            handshakes_this_epoch: 0,
            deauths_sent: 0,
            assoc_attempts: 0,
            mood: pwncore::Mood::LookR,
            timestamp: chrono::Utc::now(),
            started_at: chrono::Utc::now(),
            ended_at: None,
        };

        let time = p.calc_recon_time(&epoch);
        assert!(time >= p.min_recon_time);
        assert!(time <= p.max_recon_time);
    }

    #[test]
    fn test_default_plugins() {
        let config = PwnConfig::default();
        assert!(config.plugins.contains_key("auto_tune"));
        assert!(config.plugins.contains_key("webcfg"));
        assert!(config.plugins["auto_tune"].enabled);
    }

    #[test]
    fn test_default_plugins_opt_in_ones_are_off() {
        // Regression: `default_plugins()` used to enable every plugin
        // unconditionally, silently shipping upload plugins (wpa_sec/
        // wigle/ohcapi) active with no credential set, and every
        // optional-hardware plugin polling for hardware most installs
        // don't have.
        let config = PwnConfig::default();
        for name in [
            "wpa_sec",
            "wigle",
            "ohcapi",
            "bt_tether",
            "gpio_buttons",
            "gps",
            "pisugarx",
            "ups_lite",
        ] {
            assert!(
                !config.plugins[name].enabled,
                "{name} should default to disabled (opt-in)"
            );
        }
    }

    #[test]
    fn test_default_gpio_buttons_targets_pisugar_s_button() {
        let config = PwnConfig::default();
        let opts = &config.plugins["gpio_buttons"].options;
        assert_eq!(opts["pin"], 3);
        assert_eq!(opts["long_press_secs"], 3);
    }
}
