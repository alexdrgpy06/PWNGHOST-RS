//! Configuration loading for PWNGHOST-RS

pub mod migrate;
pub mod schema;

pub use migrate::migrate_config;
pub use schema::{MainConfig, PersonalityConfig, PwnConfig, UiConfig};

use anyhow::{Context, Result};
use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use std::path::Path;
use tokio::fs;

/// Load configuration from file with defaults and conf.d overlay
pub async fn load_config<P: AsRef<Path>>(path: P) -> Result<PwnConfig> {
    let config_path = path.as_ref();

    // Start with defaults
    let mut figment = Figment::from(Serialized::defaults(PwnConfig::default()))
        .merge(Env::prefixed("PWNGHOST").split("__"));

    // Load main config file if exists
    if config_path.exists() {
        figment = figment.merge(Toml::file(config_path));
    }

    // Load conf.d/*.toml files
    let conf_dir = config_path
        .parent()
        .unwrap_or(Path::new("/etc/pwnghost"))
        .join("conf.d");

    if conf_dir.exists() {
        let mut entries = Vec::new();
        let mut dir = fs::read_dir(&conf_dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.path().extension().is_some_and(|ext| ext == "toml") {
                entries.push(entry);
            }
        }
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            figment = figment.merge(Toml::file(entry.path()));
        }
    }

    let mut cfg: PwnConfig = figment
        .extract()
        .context("Failed to deserialize configuration")?;

    // Validate and fix up config
    cfg.validate_and_fix().await?;

    Ok(cfg)
}

/// Save configuration to file
pub async fn save_config<P: AsRef<Path>>(config: &PwnConfig, path: P) -> Result<()> {
    let content = toml::to_string_pretty(config)?;
    fs::write(path, content).await?;
    Ok(())
}

/// Generate default configuration
pub fn default_config() -> PwnConfig {
    PwnConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_config() {
        let config = PwnConfig::default();
        assert_eq!(config.main.name, "pwnghost");
        assert_eq!(config.main.iface, "wlan0");
        assert!(config.ui.web.enabled);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = PwnConfig::default();
        let toml = toml::to_string(&config).unwrap();
        let parsed: PwnConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.main.name, parsed.main.name);
    }
}
