use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PwnagotchiConfig {
    pub main: MainConfig,
    #[serde(default)]
    pub personality: PersonalityConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

impl Default for PwnagotchiConfig {
    fn default() -> Self {
        Self {
            main: MainConfig::default(),
            personality: PersonalityConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    pub name: String,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub lang: String,
}

impl Default for MainConfig {
    fn default() -> Self {
        Self {
            name: "pwnagotchi-rs".into(),
            whitelist: Vec::new(),
            lang: "en".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    #[serde(default)]
    pub aggressive: bool,
    #[serde(default)]
    pub recon_time: u32,
    #[serde(default)]
    pub hop_recon_time: u32,
    #[serde(default)]
    pub min_recon_time: u32,
    #[serde(default)]
    pub max_interactions: u32,
    #[serde(default)]
    pub deauth: bool,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            aggressive: false,
            recon_time: 30,
            hop_recon_time: 10,
            min_recon_time: 5,
            max_interactions: 10,
            deauth: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub display: DisplayConfig,
    pub web: WebConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            web: WebConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub enabled: bool,
    pub r#type: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            // preconfigured for Waveshare V4 in the default image
            enabled: true,
            r#type: "waveshare_4".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    pub enabled: bool,
    pub port: u16,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8080,
        }
    }
}

pub fn load_config(path: &str) -> anyhow::Result<PwnagotchiConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: PwnagotchiConfig = toml::from_str(&content)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PwnagotchiConfig::default();
        assert_eq!(config.main.name, "pwnagotchi-rs");
        assert_eq!(config.main.lang, "en");
    }

    #[test]
    fn test_config_roundtrip() {
        let config = PwnagotchiConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: PwnagotchiConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.main.name, config.main.name);
    }

    #[test]
    fn test_load_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = PwnagotchiConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        std::fs::write(&path, &toml_str).unwrap();
        let loaded = load_config(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.main.name, "pwnagotchi-rs");
    }
}
