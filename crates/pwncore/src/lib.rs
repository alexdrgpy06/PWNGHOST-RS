//! Core domain types for pwnagotchi-rs

pub mod ap;
pub mod channel;
pub mod epoch;
pub mod handshake;
pub mod mood;
pub mod peer;
pub mod station;
pub mod personality;

pub use ap::{AccessPoint, EncryptionType};
pub use channel::{Channel, ChannelSet, ALL_CHANNELS, NON_OVERLAPPING};
pub use epoch::{Epoch, EpochHistory};
pub use personality::Personality;
pub use handshake::{Handshake, HandshakeType, HandshakeFile, GpsData};
pub use mood::Mood;
pub use peer::{Peer, PeerManager};
pub use station::{Station, Client};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwnConfig {
    pub main: MainConfig,
    pub personality: Personality,
    pub ui: UiConfig,
    pub bettercap: BettercapConfig,
    pub fs: FsConfig,
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    pub name: String,
    pub lang: String,
    pub iface: String,
    pub mon_start_cmd: Option<String>,
    pub mon_stop_cmd: Option<String>,
    pub mon_max_blind_epochs: u32,
    pub no_restart: bool,
    pub whitelist: Vec<String>,
    pub confd: String,
    pub custom_plugin_repos: Vec<String>,
    pub custom_plugins: String,
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub invert: bool,
    pub cursor: bool,
    pub fps: f32,
    pub font: FontConfig,
    pub faces: FacesConfig,
    pub display: DisplayConfig,
    pub web: WebConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub name: String,
    pub size_offset: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacesConfig {
    pub look_r: Vec<String>,
    pub look_l: Vec<String>,
    pub look_r_happy: Vec<String>,
    pub look_l_happy: Vec<String>,
    pub sleep: Vec<String>,
    pub awake: Vec<String>,
    pub bored: Vec<String>,
    pub intense: Vec<String>,
    pub cool: Vec<String>,
    pub happy: Vec<String>,
    pub excited: Vec<String>,
    pub grateful: Vec<String>,
    pub motivated: Vec<String>,
    pub demotivated: Vec<String>,
    pub smart: Vec<String>,
    pub lonely: Vec<String>,
    pub sad: Vec<String>,
    pub angry: Vec<String>,
    pub friend: Vec<String>,
    pub broken: Vec<String>,
    pub upload: Vec<String>,
    pub png: bool,
    pub position_x: i32,
    pub position_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub enabled: bool,
    pub rotation: u8,
    pub display_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub enabled: bool,
    pub address: String,
    pub auth: bool,
    pub username: String,
    pub password: String,
    pub origin: String,
    pub port: u16,
    pub on_frame: String,
    pub theme: WebThemeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebThemeConfig {
    pub accent_r: u8,
    pub accent_g: u8,
    pub accent_b: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BettercapConfig {
    pub handshakes: String,
    pub silence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsConfig {
    pub memory: MemoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub mounts: HashMap<String, MemoryMount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMount {
    pub enabled: bool,
    pub mount: String,
    pub size: String,
    pub sync: u32,
    pub zram: bool,
    pub rsync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub enabled: bool,
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Load configuration from TOML
pub fn load_config<P: AsRef<std::path::Path>>(path: P) -> Result<PwnConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: PwnConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Save configuration to TOML
pub fn save_config<P: AsRef<std::path::Path>>(config: &PwnConfig, path: P) -> Result<()> {
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Generate default configuration
pub fn default_config() -> PwnConfig {
    PwnConfig {
        main: MainConfig {
            name: "pwnagotchi".to_string(),
            lang: "en".to_string(),
            iface: "wlan0mon".to_string(),
            mon_start_cmd: Some("/usr/bin/monstart".to_string()),
            mon_stop_cmd: Some("/usr/bin/monstop".to_string()),
            mon_max_blind_epochs: 5,
            no_restart: false,
            whitelist: vec![
                "EXAMPLE_NETWORK".to_string(),
                "ANOTHER_EXAMPLE_NETWORK".to_string(),
                "fo:od:ba:be:fo:od".to_string(),
                "fo:od:ba".to_string(),
            ],
            confd: "/etc/pwnagotchi/conf.d/".to_string(),
            custom_plugin_repos: vec![
                "https://github.com/jayofelony/pwnagotchi-torch-plugins/archive/master.zip".to_string(),
                "https://github.com/Sniffleupagus/pwnagotchi_plugins/archive/master.zip".to_string(),
                "https://github.com/NeonLightning/pwny/archive/master.zip".to_string(),
                "https://github.com/marbasec/UPSLite_Plugin_1_3/archive/master.zip".to_string(),
                "https://github.com/wpa-2/Pwnagotchi-Plugins/archive/master.zip".to_string(),
                "https://github.com/cyberartemio/wardriver-pwnagotchi-plugin/archive/main.zip".to_string(),
            ],
            custom_plugins: "/usr/local/share/pwnagotchi/custom-plugins/".to_string(),
            plugins: default_plugins(),
        },
        personality: Personality::default(),
        ui: UiConfig {
            invert: false,
            cursor: true,
            fps: 0.0,
            font: FontConfig {
                name: "DejaVuSansMono".to_string(),
                size_offset: 0,
            },
            faces: default_faces(),
            display: DisplayConfig {
                enabled: false,
                rotation: 180,
                display_type: "waveshare_4".to_string(),
            },
            web: WebConfig {
                enabled: true,
                address: "::".to_string(),
                auth: false,
                username: "changeme".to_string(),
                password: "changeme".to_string(),
                origin: "".to_string(),
                port: 8080,
                on_frame: "".to_string(),
                theme: WebThemeConfig {
                    accent_r: 76,
                    accent_g: 175,
                    accent_b: 80,
                },
            },
        },
        bettercap: BettercapConfig {
            handshakes: "/etc/pwnagotchi/handshakes".to_string(),
            silence: vec![
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
            ],
        },
        fs: FsConfig {
            memory: MemoryConfig {
                enabled: true,
                mounts: default_memory_mounts(),
            },
        },
        plugins: default_plugins(),
    }
}

fn default_faces() -> FacesConfig {
    FacesConfig {
        look_r: vec!["( ⚆_⚆)".to_string()],
        look_l: vec!["(☉_☉ )".to_string()],
        look_r_happy: vec!["( ◕‿◕)".to_string(), "( ≧◡≦)".to_string()],
        look_l_happy: vec!["(◕‿◕ )".to_string(), "(≧◡≦ )".to_string()],
        sleep: vec!["(⇀‿‿↼)".to_string(), "(≖‿‿≖)".to_string(), "(－_－)".to_string()],
        awake: vec!["(◕‿‿◕)".to_string()],
        bored: vec!["(-__-)".to_string(), "(—__—)".to_string()],
        intense: vec!["(°▃▃°)".to_string(), "(°ロ°)".to_string()],
        cool: vec!["(⌐■_■)".to_string(), "(단__단)".to_string()],
        happy: vec!["(•‿‿•)".to_string(), "(^‿‿^)".to_string(), "(^◡◡^)".to_string()],
        excited: vec!["(ᵔ◡◡ᵔ)".to_string(), "(✜‿‿✜)".to_string()],
        grateful: vec!["(^‿‿^)".to_string()],
        motivated: vec!["(☼‿‿☼)".to_string(), "(★‿★)".to_string(), "(•̀ᴗ•́)".to_string()],
        demotivated: vec!["(≖__≖)".to_string(), "(￣ヘ￣)".to_string(), "(¬､¬)".to_string()],
        smart: vec!["(✜‿‿✜)".to_string()],
        lonely: vec!["(ب__ب)".to_string(), "(｡•́︿•̀｡)".to_string(), "(︶︹︺)".to_string()],
        sad: vec!["(╥☁╥ )".to_string(), "(╥﹏╥)".to_string(), "(ಥ﹏ಥ)".to_string()],
        angry: vec!["(-_-')".to_string(), "(⇀__⇀)".to_string(), "(`___´)".to_string()],
        friend: vec!["(♥‿‿♥)".to_string(), "(♡‿‿♡)".to_string(), "(♥‿♥ )".to_string(), "(♥ω♥ )".to_string()],
        broken: vec!["(☓‿‿☓)".to_string()],
        upload: vec!["(1__0)".to_string(), "(1__1)".to_string(), "(0__1)".to_string()],
        png: false,
        position_x: 0,
        position_y: 34,
    }
}

fn default_memory_mounts() -> HashMap<String, MemoryMount> {
    let mut mounts = HashMap::new();
    mounts.insert("log".to_string(), MemoryMount {
        enabled: true,
        mount: "/etc/pwnagotchi/log/".to_string(),
        size: "50M".to_string(),
        sync: 60,
        zram: true,
        rsync: true,
    });
    mounts.insert("data".to_string(), MemoryMount {
        enabled: true,
        mount: "/var/tmp/pwnagotchi".to_string(),
        size: "10M".to_string(),
        sync: 3600,
        zram: true,
        rsync: true,
    });
    mounts
}

fn default_plugins() -> HashMap<String, PluginConfig> {
    let mut plugins = HashMap::new();
    plugins.insert("auto-tune".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("auto_backup".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("auto-update".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("bt-tether".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("cache".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("gpio_buttons".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("gps".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("grid".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("logtail".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("memtemp".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("ohcapi".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("pwncrack".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("session-stats".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("ups_hat_c".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("ups_lite".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("webcfg".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("pwnstore_ui".to_string(), PluginConfig { enabled: true, options: HashMap::new() });
    plugins.insert("webgpsmap".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("wigle".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins.insert("wpa-sec".to_string(), PluginConfig { enabled: false, options: HashMap::new() });
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert_eq!(config.main.name, "pwnagotchi");
        assert_eq!(config.main.iface, "wlan0mon");
        assert!(config.ui.web.enabled);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = default_config();
        let toml = toml::to_string(&config).unwrap();
        let parsed: PwnConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.main.name, parsed.main.name);
    }
}