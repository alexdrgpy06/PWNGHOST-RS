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

/// Recursively merge JSON `patch` into `base`, in place. Objects are merged
/// key-by-key (nested objects recurse); every other value (scalars, arrays)
/// in `patch` replaces the corresponding value in `base`. Keys present in
/// `base` but absent from `patch` are left untouched.
///
/// This is how a config-editor save must work: the web UI sends only the
/// fields the user changed, and unspecified sections (`bettercap`, `fs`,
/// `oxigotchi`, `plugins`, plugin `api_key`s, ...) must survive rather than
/// being reset to defaults. Mirrors real pwnagotchi's `utils.merge_config`
/// (its webcfg "merge-save" path), which exists for exactly this reason.
pub fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                merge_json(base_map.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
        (base_slot, patch_val) => {
            *base_slot = patch_val.clone();
        }
    }
}

/// Apply a partial config `patch` (JSON) onto an existing `PwnConfig` by
/// deep-merging, returning the merged config. Unspecified sections are
/// preserved. Fails if the merged result no longer deserializes into a
/// valid `PwnConfig` (e.g. the patch set a field to the wrong type).
pub fn apply_config_patch(
    current: &PwnConfig,
    patch: &serde_json::Value,
) -> Result<PwnConfig> {
    let mut merged = serde_json::to_value(current)
        .context("serializing current config for merge")?;
    merge_json(&mut merged, patch);
    let cfg: PwnConfig =
        serde_json::from_value(merged).context("merged config is not a valid PwnConfig")?;
    Ok(cfg)
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

    #[test]
    fn test_merge_json_deep_merges_objects() {
        let mut base = serde_json::json!({
            "a": { "x": 1, "y": 2 },
            "b": "keep-me",
        });
        let patch = serde_json::json!({ "a": { "y": 99, "z": 3 } });
        merge_json(&mut base, &patch);
        assert_eq!(
            base,
            serde_json::json!({
                "a": { "x": 1, "y": 99, "z": 3 },  // x untouched, y replaced, z added
                "b": "keep-me",                    // sibling section preserved
            })
        );
    }

    #[test]
    fn test_merge_json_patch_replaces_arrays_wholesale() {
        // Arrays are replaced, not element-merged (e.g. a whitelist edit
        // fully replaces the list).
        let mut base = serde_json::json!({ "list": [1, 2, 3] });
        merge_json(&mut base, &serde_json::json!({ "list": [9] }));
        assert_eq!(base, serde_json::json!({ "list": [9] }));
    }

    #[test]
    fn test_apply_config_patch_preserves_other_sections() {
        let mut current = PwnConfig::default();
        current.main.name = "original".to_string();
        current.main.iface = "wlan0mon".to_string();
        let patch = serde_json::json!({ "main": { "name": "patched" } });
        let merged = apply_config_patch(&current, &patch).unwrap();
        assert_eq!(merged.main.name, "patched");
        // iface (same section, unspecified key) must survive.
        assert_eq!(merged.main.iface, "wlan0mon");
    }
}
