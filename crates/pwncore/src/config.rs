//! Core domain types and configuration for pwnagotchi-rs

pub mod ap;
pub mod channel;
pub mod client;
pub mod config;
pub mod epoch;
pub mod handshake;
pub mod mood;
pub mod peer;
pub mod personality;
pub mod plugin;

pub use ap::{AccessPoint, Client, EncryptionType};
pub use channel::Channel;
pub use client::Client;
pub use config::{PwnConfig, default_config};
pub use epoch::Epoch;
pub use handshake::{Handshake, HandshakeType, GpsData};
pub use mood::Mood;
pub use peer::Peer;
pub use personality::Personality;
pub use plugin::PluginConfig;

use anyhow::Result;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Full pwnagotchi configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwnConfig {
    pub main: MainConfig,
    pub personality: PersonalityConfig,
    pub ui: UiConfig,
    pub bettercap: BettercapConfig,
    pub fs: FsConfig,
    pub plugins: HashMap<String, PluginConfig>,
    pub log: LogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    pub name: String,
    pub lang: String,
    pub iface: String,
    pub mon_start_cmd: String,
    pub mon_stop_cmd: String,
    pub mon_max_blind_epochs: u32,
    pub no_restart: bool,
    pub whitelist: Vec<String>,
    pub confd: String,
    pub custom_plugin_repos: Vec<String>,
    pub custom_plugins: String,
    pub plugins: HashMap<String, PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    pub advertise: bool,
    pub deauth: bool,
    pub associate: bool,
    pub channels: Vec<u8>,
    pub min_rssi: i16,
    pub ap_ttl: u32,
    pub sta_ttl: u32,
    pub recon_time: u32,
    pub max_inactive_scale: u32,
    pub recon_inactive_multiplier: u32,
    pub hop_recon_time: u32,
    pub min_recon_time: u32,
    pub max_interactions: u32,
    pub max_misses_for_recon: u32,
    pub excited_num_epochs: u32,
    pub bored_num_epochs: u32,
    pub sad_num_epochs: u32,
    pub bond_encounters_factor: u32,
    pub throttle_a: f32,
    pub throttle_d: f32,
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
    pub rotation: u16,
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
    pub mounts: std::collections::HashMap<String, MemoryMount>,
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
    pub options: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub path: String,
    pub path_debug: String,
    pub rotation: LogRotationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    pub enabled: bool,
    pub size: String,
}

/// Default configuration matching pwnagotchi defaults.toml
pub fn default_config() -> PwnConfig {
    PwnConfig {
        main: MainConfig {
            name: "pwnagotchi".to_string(),
            lang: "en".to_string(),
            iface: "wlan0mon".to_string(),
            mon_start_cmd: "/usr/bin/monstart".to_string(),
            mon_stop_cmd: "/usr/bin/monstop".to_string(),
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
                "https://github.com/wpa-2/Pwnagotchi-Plugins/archive/main.zip".to_string(),
                "https://github.com/cyberartemio/wardriver-pwnagotchi-plugin/archive/main.zip".to_string(),
            ],
            custom_plugins: "/usr/local/share/pwnagotchi/custom-plugins/".to_string(),
            plugins: default_plugins(),
        },
        personality: PersonalityConfig {
            advertise: true,
            deauth: true,
            associate: true,
            channels: vec![],
            min_rssi: -200,
            ap_ttl: 120,
            sta_ttl: 300,
            recon_time: 30,
            max_inactive_scale: 2,
            recon_inactive_multiplier: 2,
            hop_recon_time: 10,
            min_recon_time: 5,
            max_interactions: 3,
            max_misses_for_recon: 5,
            excited_num_epochs: 10,
            bored_num_epochs: 15,
            sad_num_epochs: 25,
            bond_encounters_factor: 20000,
            throttle_a: 0.4,
            throttle_d: 0.9,
        },
        ui: UiConfig {
            invert: false,
            cursor: true,
            fps: 0.0,
            font: FontConfig {
                name: "DejaVuSansMono".to_string(),
                size_offset: 0,
            },
            faces: FacesConfig {
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
            },
            display: DisplayConfig {
                // Pre-enable display for Waveshare V4 by default for a 'preconfigured' image
                enabled: true,
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
                mounts: {
                    let mut m = std::collections::HashMap::new();
                    m.insert("log".to_string(), MemoryMount {
                        enabled: true,
                        mount: "/etc/pwnagotchi/log/".to_string(),
                        size: "50M".to_string(),
                        sync: 60,
                        zram: true,
                        rsync: true,
                    });
                    m.insert("data".to_string(), MemoryMount {
                        enabled: true,
                        mount: "/var/tmp/pwnagotchi".to_string(),
                        size: "10M".to_string(),
                        sync: 3600,
                        zram: true,
                        rsync: true,
                    });
                    m
                },
            },
        },
        plugins: default_plugins(),
        log: LogConfig {
            path: "/etc/pwnagotchi/log/pwnagotchi.log".to_string(),
            path_debug: "/etc/pwnagotchi/log/pwnagotchi-debug.log".to_string(),
            rotation: LogRotationConfig {
                enabled: true,
                size: "10M".to_string(),
            },
        },
    }
}

fn default_plugins() -> std::collections::HashMap<String, PluginConfig> {
    let mut plugins = std::collections::HashMap::new();
    
    plugins.insert("auto-tune".to_string(), PluginConfig {
        enabled: true,
        options: std::collections::HashMap::new(),
    });
    
    plugins.insert("auto_backup".to_string(), PluginConfig {
        enabled: true,
        options: {
            let mut o = std::collections::HashMap::new();
            o.insert("backup_location".to_string(), serde_json::json!("/etc/pwnagotchi/backups"));
            o
        },
    });
    
    plugins.insert("auto-update".to_string(), PluginConfig {
        enabled: true,
        options: {
            let mut o = std::collections::HashMap::new();
            o.insert("install".to_string(), serde_json::json!(true));
            o.insert("interval".to_string(), serde_json::json!(1));
            o.insert("token".to_string(), serde_json::json!(""));
            o
        },
    });
    
    plugins.insert("bt-tether".to_string(), PluginConfig {
        enabled: false,
        options: {
            let mut o = std::collections::HashMap::new();
            o.insert("auto_reconnect".to_string(), serde_json::json!(true));
            o.insert("show_on_screen".to_string(), serde_json::json!(true));
            o.insert("show_mini_status".to_string(), serde_json::json!(true));
            o.insert("mini_status_position".to_string(), serde_json::json!([110, 0]));
            o.insert("show_detailed_status".to_string(), serde_json::json!(true));
            o.insert("detailed_status_position".to_string(), serde_json::json!([0, 82]));
            o
        },
    });
    
    plugins.insert("fix_services".to_string(), PluginConfig {
        enabled: true,
        options: std::collections::HashMap::new(),
    });
    
    plugins.insert("cache".to_string(), PluginConfig {
        enabled: true,
        options: std::collections::HashMap::new(),
    });
    
    plugins.insert("gpio_buttons".to_string(), PluginConfig {
        enabled: false,
        options: std::collections::HashMap::new(),
    });
    
    // PiSugar battery/UPS plugin - preconfigured for PiSugar S/3 variants
    plugins.insert("pisugarx".to_string(), PluginConfig {
        enabled: true,
        options: {
            let mut o = std::collections::HashMap::new();
            // default I2C bus/address for PiSugar S-family (can be overridden in config.toml)
            o.insert("i2c_bus".to_string(), serde_json::json!(1));
            o.insert("i2c_addr".to_string(), serde_json::json!(0x36));
            o.insert("show_on_screen".to_string(), serde_json::json!(true));
            o.insert("report_interval".to_string(), serde_json::json!(60));
            o
        },
    });
    
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
        assert!(config.main.plugins.contains_key("auto-tune"));
        assert!(config.main.plugins.contains_key("auto_backup"));
    }

    #[test]
    fn test_config_serialization() {
        let config = default_config();
        let toml = toml::to_string(&config).unwrap();
        let parsed: PwnConfig = toml::from_str(&toml).unwrap();
        assert_eq!(parsed.main.name, config.main.name);
    }
}