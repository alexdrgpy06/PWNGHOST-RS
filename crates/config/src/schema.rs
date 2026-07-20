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
    pub oxigotchi: OxigotchiConfig,

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
            oxigotchi: OxigotchiConfig::default(),
            plugins: default_plugins(),
        }
    }
}

/// Runtime/loop tuning for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxigotchiConfig {
    /// Duration of one agent epoch, in seconds.
    #[serde(default = "default_epoch_duration")]
    pub epoch_duration: u64,
}

fn default_epoch_duration() -> u64 {
    15
}

impl Default for OxigotchiConfig {
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
            tokio::fs::create_dir_all(dir).await?;
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
    for name in [
        "auto_tune",
        "auto_backup",
        "auto_update",
        "bt_tether",
        "cache",
        "fix_services",
        "gpio_buttons",
        "gps",
        "grid",
        "logtail",
        "memtemp",
        "ohcapi",
        "pisugarx",
        "pwncrack",
        "session_stats",
        "ups_lite",
        "webcfg",
        "pwnstore_ui",
        "webgpsmap",
        "wigle",
        "wpa_sec",
    ] {
        plugins.insert(
            name.to_string(),
            PluginConfig {
                enabled: true,
                options: HashMap::new(),
            },
        );
    }
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
    "wlan0".to_string()
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

    // Faces
    #[serde(default)]
    pub faces: FaceConfig,
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
            // APs to force handshakes. AngryOxide does the deauthing
            // autonomously unless told --disable-deauth.
            deauth: true,
            associate: true,
            min_rssi: default_min_rssi(),
            position_x: 0,
            position_y: 34,
            frame_padding: default_frame_padding(),
            frame_padding_min_bytes: default_frame_padding_min(),
            faces: FaceConfig::default(),
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

/// Face configuration - kaomoji strings per mood (matches pwnagotchi personality.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceConfig {
    #[serde(default)]
    pub look_r: Vec<String>,
    #[serde(default)]
    pub look_l: Vec<String>,
    #[serde(default)]
    pub look_r_happy: Vec<String>,
    #[serde(default)]
    pub look_l_happy: Vec<String>,
    #[serde(default)]
    pub sleep: Vec<String>,
    #[serde(default)]
    pub awake: Vec<String>,
    #[serde(default)]
    pub bored: Vec<String>,
    #[serde(default)]
    pub intense: Vec<String>,
    #[serde(default)]
    pub cool: Vec<String>,
    #[serde(default)]
    pub happy: Vec<String>,
    #[serde(default)]
    pub excited: Vec<String>,
    #[serde(default)]
    pub grateful: Vec<String>,
    #[serde(default)]
    pub motivated: Vec<String>,
    #[serde(default)]
    pub demotivated: Vec<String>,
    #[serde(default)]
    pub smart: Vec<String>,
    #[serde(default)]
    pub lonely: Vec<String>,
    #[serde(default)]
    pub sad: Vec<String>,
    #[serde(default)]
    pub angry: Vec<String>,
    #[serde(default)]
    pub friend: Vec<String>,
    #[serde(default)]
    pub broken: Vec<String>,
    #[serde(default)]
    pub upload: Vec<String>,
    #[serde(default)]
    pub png: bool,
}

impl Default for FaceConfig {
    fn default() -> Self {
        Self {
            look_r: vec!["( ⚆_⚆)".to_string()],
            look_l: vec!["(☉_☉ )".to_string()],
            look_r_happy: vec!["( ◕‿◕)".to_string(), "( ≧◡≦)".to_string()],
            look_l_happy: vec!["(◕‿◕ )".to_string(), "(≧◡≦ )".to_string()],
            sleep: vec![
                "(⇀‿‿↼)".to_string(),
                "(≖‿‿≖)".to_string(),
                "(－_－)".to_string(),
            ],
            awake: vec!["(◕‿‿◕)".to_string()],
            bored: vec!["(-__-)".to_string(), "(—__—)".to_string()],
            intense: vec!["(°▃▃°)".to_string(), "(°ロ°)".to_string()],
            cool: vec!["(⌐■_■)".to_string(), "(单__单)".to_string()],
            happy: vec![
                "(•‿‿•)".to_string(),
                "(^‿‿^)".to_string(),
                "(^◡◡^)".to_string(),
            ],
            excited: vec!["(ᵔ◡◡ᵔ)".to_string(), "(✜‿‿✜)".to_string()],
            grateful: vec!["(^‿‿^)".to_string()],
            motivated: vec![
                "(☼‿‿☼)".to_string(),
                "(★‿★)".to_string(),
                "(•̀ᴗ•́)".to_string(),
            ],
            demotivated: vec![
                "(≖__≖)".to_string(),
                "(￣ヘ￣)".to_string(),
                "(¬､¬)".to_string(),
            ],
            smart: vec!["(✜‿‿✜)".to_string()],
            lonely: vec![
                "(ب__ب)".to_string(),
                "(｡•́︿•̀｡)".to_string(),
                "(︶︹︺)".to_string(),
            ],
            sad: vec![
                "(╥☁╥ )".to_string(),
                "(╥﹏╥)".to_string(),
                "(ಥ﹏ಥ)".to_string(),
            ],
            angry: vec![
                "(-_-')".to_string(),
                "(⇀__⇀)".to_string(),
                "(`___´)".to_string(),
            ],
            friend: vec![
                "(♥‿‿♥)".to_string(),
                "(♡‿‿♡)".to_string(),
                "(♥‿♥ )".to_string(),
                "(♥ω♥ )".to_string(),
            ],
            broken: vec!["(☓‿‿☓)".to_string()],
            upload: vec![
                "(1__0)".to_string(),
                "(1__1)".to_string(),
                "(0__1)".to_string(),
            ],
            png: false,
        }
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
            position_y: 16,
            face_paths: HashMap::new(),
        }
    }
}

/// Bettercap configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BettercapConfig {
    #[serde(default = "default_handshakes_path")]
    pub handshakes: String,

    #[serde(default = "default_silence")]
    pub silence: Vec<String>,
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
}
