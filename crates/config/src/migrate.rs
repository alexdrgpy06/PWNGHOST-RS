//! Configuration migration from legacy formats

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Legacy config structure for migration
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyConfig {
    pub main: Option<LegacyMainConfig>,
    pub personality: Option<LegacyPersonalityConfig>,
    pub ui: Option<LegacyUiConfig>,
    pub bettercap: Option<LegacyBettercapConfig>,
    pub plugins: Option<HashMap<String, LegacyPluginConfig>>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyMainConfig {
    pub name: Option<String>,
    pub lang: Option<String>,
    pub iface: Option<String>,
    pub mon_start_cmd: Option<String>,
    pub mon_stop_cmd: Option<String>,
    pub mon_max_blind_epochs: Option<u32>,
    pub no_restart: Option<bool>,
    pub whitelist: Option<Vec<String>>,
    pub confd: Option<String>,
    pub custom_plugin_repos: Option<Vec<String>>,
    pub custom_plugins: Option<String>,
    pub plugins: Option<HashMap<String, LegacyPluginConfig>>,
    pub log: Option<LegacyLogConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyPersonalityConfig {
    pub advertise: Option<bool>,
    pub look_r: Option<Vec<String>>,
    pub look_l: Option<Vec<String>>,
    pub look_r_happy: Option<Vec<String>>,
    pub look_l_happy: Option<Vec<String>>,
    pub sleep: Option<Vec<String>>,
    pub awake: Option<Vec<String>>,
    pub bored: Option<Vec<String>>,
    pub intense: Option<Vec<String>>,
    pub cool: Option<Vec<String>>,
    pub happy: Option<Vec<String>>,
    pub excited: Option<Vec<String>>,
    pub grateful: Option<Vec<String>>,
    pub motivated: Option<Vec<String>>,
    pub demotivated: Option<Vec<String>>,
    pub smart: Option<Vec<String>>,
    pub lonely: Option<Vec<String>>,
    pub sad: Option<Vec<String>>,
    pub angry: Option<Vec<String>>,
    pub friend: Option<Vec<String>>,
    pub broken: Option<Vec<String>>,
    pub debug: Option<Vec<String>>,
    pub upload: Option<Vec<String>>,
    pub png: Option<bool>,
    pub position_x: Option<i32>,
    pub position_y: Option<i32>,
    pub frame_padding: Option<bool>,
    pub frame_padding_min_bytes: Option<usize>,
    pub deauth: Option<bool>,
    pub associate: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyUiConfig {
    pub web: Option<LegacyWebConfig>,
    pub display: Option<LegacyDisplayConfig>,
    pub faces: Option<LegacyFacesConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyWebConfig {
    pub enabled: Option<bool>,
    pub address: Option<String>,
    pub auth: Option<bool>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub origin: Option<String>,
    pub port: Option<u16>,
    pub on_frame: Option<String>,
    pub theme: Option<LegacyWebThemeConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyWebThemeConfig {
    pub accent_r: Option<u8>,
    pub accent_g: Option<u8>,
    pub accent_b: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyDisplayConfig {
    pub enabled: Option<bool>,
    pub rotation: Option<u16>,
    pub display_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyFacesConfig {
    pub png: Option<bool>,
    pub position_x: Option<i32>,
    pub position_y: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyBettercapConfig {
    pub handshakes: Option<String>,
    pub silence: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyLogConfig {
    pub path: Option<String>,
    pub path_debug: Option<String>,
    pub rotation: Option<LegacyLogRotationConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyLogRotationConfig {
    pub enabled: Option<bool>,
    pub size: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LegacyPluginConfig {
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Migrate legacy config to new schema
pub fn migrate_config(legacy: LegacyConfig) -> super::schema::PwnConfig {
    let mut config = super::schema::PwnConfig::default();

    // Migrate main
    if let Some(m) = legacy.main {
        config.main.name = m.name.unwrap_or_else(|| "pwnghost".to_string());
        config.main.lang = m.lang.unwrap_or_else(|| "en".to_string());
        config.main.iface = m.iface.unwrap_or_else(|| "wlan0".to_string());
        config.main.mon_start_cmd = m
            .mon_start_cmd
            .unwrap_or_else(|| "/usr/bin/monstart".to_string());
        config.main.mon_stop_cmd = m
            .mon_stop_cmd
            .unwrap_or_else(|| "/usr/bin/monstop".to_string());
        // Real pwnagotchi's own default is 50 -- 5 was 10x more
        // trigger-happy, real restart-loop risk in an ordinary dead wifi
        // zone (matches `schema::default_max_blind_epochs`).
        config.main.mon_max_blind_epochs = m.mon_max_blind_epochs.unwrap_or(50);
        config.main.no_restart = m.no_restart.unwrap_or(false);
        config.main.whitelist = m.whitelist.unwrap_or_default();
        config.main.confd = m
            .confd
            .unwrap_or_else(|| "/etc/pwnghost/conf.d/".to_string());
        config.main.custom_plugin_repos = m.custom_plugin_repos.unwrap_or_default();
        config.main.custom_plugins = m
            .custom_plugins
            .unwrap_or_else(|| "/usr/local/share/pwnghost/custom-plugins/".to_string());

        if let Some(log) = m.log {
            config.main.log.path = log
                .path
                .unwrap_or_else(|| "/etc/pwnghost/log/pwnghost.log".to_string());
            config.main.log.path_debug = log
                .path_debug
                .unwrap_or_else(|| "/etc/pwnghost/log/pwnghost-debug.log".to_string());
            if let Some(rot) = log.rotation {
                config.main.log.rotation.enabled = rot.enabled.unwrap_or(true);
                config.main.log.rotation.size = rot.size.unwrap_or_else(|| "10M".to_string());
            }
        }
    }

    // Migrate personality
    if let Some(p) = legacy.personality {
        config.personality.bored_num_epochs = 50; // default
        config.personality.sad_num_epochs = 100;
        config.personality.angry_num_epochs = 200;
        config.personality.lonely_num_epochs = 150;
        config.personality.bond_encounters_factor = 1.0;
        config.personality.max_interactions = 10;
        config.personality.throttle = 30;
        config.personality.reward_handshake = 100;
        config.personality.reward_new_ap = 10;
        config.personality.reward_association = 5;
        config.personality.penalty_missed = -10;
        config.personality.penalty_reboot = -50;
        config.personality.min_recon_time = 5;
        config.personality.max_recon_time = 30;
        config.personality.hop_recon_time = 10;
        // Real pwnagotchi defaults deauth on; a migrated config that
        // didn't set it should inherit that, not silently go passive.
        config.personality.deauth = p.deauth.unwrap_or(true);
        config.personality.associate = p.associate.unwrap_or(true);
        // Real pwnagotchi's own default is -200, effectively unfiltered
        // (matches `schema::default_min_rssi`).
        config.personality.min_rssi = -200;
        config.personality.position_x = p.position_x.unwrap_or(0);
        config.personality.position_y = p.position_y.unwrap_or(34);
        config.personality.frame_padding = p.frame_padding.unwrap_or(true);
        config.personality.frame_padding_min_bytes = p.frame_padding_min_bytes.unwrap_or(650);
        // Per-mood face overrides are no longer part of our config: faces come
        // from the single canonical table in `pwncore::Mood::face()`, verified
        // against upstream faces.py. Any legacy `look_r`/`sleep`/... keys in an
        // old personality.toml are accepted and ignored.
    }

    // Migrate UI
    if let Some(ui) = legacy.ui {
        if let Some(web) = ui.web {
            config.ui.web.enabled = web.enabled.unwrap_or(true);
            config.ui.web.address = web.address.unwrap_or_else(|| "0.0.0.0".to_string());
            config.ui.web.auth = web.auth.unwrap_or(false);
            config.ui.web.username = web.username.unwrap_or_else(|| "changeme".to_string());
            config.ui.web.password = web.password.unwrap_or_else(|| "changeme".to_string());
            config.ui.web.origin = web.origin.unwrap_or_default();
            config.ui.web.port = web.port.unwrap_or(8080);
            config.ui.web.on_frame = web.on_frame.unwrap_or_default();
            if let Some(theme) = web.theme {
                config.ui.web.theme.accent_r = theme.accent_r.unwrap_or(76);
                config.ui.web.theme.accent_g = theme.accent_g.unwrap_or(175);
                config.ui.web.theme.accent_b = theme.accent_b.unwrap_or(80);
            }
        }

        if let Some(disp) = ui.display {
            config.ui.display.enabled = disp.enabled.unwrap_or(true);
            config.ui.display.rotation = disp.rotation.unwrap_or(180);
            config.ui.display.display_type = disp
                .display_type
                .unwrap_or_else(|| "waveshare_v4".to_string());
        }

        if let Some(faces) = ui.faces {
            config.ui.faces.png = faces.png.unwrap_or(true);
            config.ui.faces.position_x = faces.position_x.unwrap_or(0);
            config.ui.faces.position_y = faces.position_y.unwrap_or(34);
        }
    }

    // Migrate bettercap
    if let Some(bc) = legacy.bettercap {
        config.bettercap.handshakes = bc
            .handshakes
            .unwrap_or_else(|| "/etc/pwnghost/handshakes".to_string());
        config.bettercap.silence = bc.silence.unwrap_or_else(super::schema::default_silence);
    }

    // Migrate plugins
    if let Some(plugins) = legacy.plugins {
        for (name, plugin) in plugins {
            config.plugins.insert(
                name,
                super::schema::PluginConfig {
                    enabled: plugin.enabled.unwrap_or(true),
                    options: plugin.options,
                },
            );
        }
    }

    config
}

/// Load and migrate config from file
pub async fn load_and_migrate<P: AsRef<Path>>(path: P) -> Result<super::schema::PwnConfig> {
    let content = tokio::fs::read_to_string(&path).await?;
    let legacy: LegacyConfig = toml::from_str(&content)?;
    Ok(migrate_config(legacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_legacy_config() {
        let legacy = LegacyConfig {
            main: Some(LegacyMainConfig {
                name: Some("test".to_string()),
                lang: Some("en".to_string()),
                iface: Some("wlan0mon".to_string()),
                mon_start_cmd: Some("/usr/bin/monstart".to_string()),
                mon_stop_cmd: Some("/usr/bin/monstop".to_string()),
                mon_max_blind_epochs: Some(10),
                no_restart: Some(false),
                whitelist: Some(vec!["aa:bb:cc:dd:ee:ff".to_string()]),
                confd: Some("/etc/pwnghost/conf.d/".to_string()),
                custom_plugin_repos: Some(vec![]),
                custom_plugins: Some("/custom/plugins".to_string()),
                plugins: None,
                log: None,
            }),
            personality: Some(LegacyPersonalityConfig {
                deauth: Some(true),
                associate: Some(false),
                position_x: Some(10),
                position_y: Some(20),
                ..Default::default()
            }),
            ui: Some(LegacyUiConfig {
                web: Some(LegacyWebConfig {
                    enabled: Some(true),
                    address: Some("0.0.0.0".to_string()),
                    auth: Some(true),
                    username: Some("admin".to_string()),
                    password: Some("secret".to_string()),
                    port: Some(9000),
                    ..Default::default()
                }),
                display: Some(LegacyDisplayConfig {
                    rotation: Some(90),
                    display_type: Some("waveshare_v3".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            bettercap: Some(LegacyBettercapConfig {
                handshakes: Some("/custom/handshakes".to_string()),
                silence: Some(vec!["custom.silence".to_string()]),
            }),
            plugins: Some({
                let mut m = HashMap::new();
                m.insert(
                    "test_plugin".to_string(),
                    LegacyPluginConfig {
                        enabled: Some(false),
                        options: HashMap::new(),
                    },
                );
                m
            }),
        };

        let config = migrate_config(legacy);

        assert_eq!(config.main.name, "test");
        assert_eq!(config.main.iface, "wlan0mon");
        assert_eq!(config.main.mon_max_blind_epochs, 10);
        assert_eq!(config.main.whitelist.len(), 1);
        assert!(config.personality.deauth);
        assert_eq!(config.personality.position_x, 10);
        assert_eq!(config.personality.position_y, 20);
        assert!(config.ui.web.enabled);
        assert!(config.ui.web.auth);
        assert_eq!(config.ui.web.username, "admin");
        assert_eq!(config.ui.web.port, 9000);
        assert_eq!(config.ui.display.rotation, 90);
        assert_eq!(config.bettercap.handshakes, "/custom/handshakes");
        assert!(config.plugins.contains_key("test_plugin"));
        assert!(!config.plugins["test_plugin"].enabled);
    }
}
