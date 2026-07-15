//! Lua plugin system

use anyhow::Result;
use mlua::{Lua, Table, Value};
use pwncore::EpochState;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Plugin API exposed to Lua
pub struct PluginApi {
    pub agent: Arc<RwLock<AgentRef>>,
}

pub struct AgentRef {
    pub current_epoch: u64,
    pub current_channel: u8,
    pub aps_count: usize,
    pub handshakes: u32,
    pub mood: String,
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerInfo {
    pub mac: String,
    pub name: String,
    pub channel: u8,
    pub mood: String,
    pub level: u32,
}

/// Lua plugin wrapper
pub struct LuaPlugin {
    name: String,
    lua: Lua,
    code: String,
}

impl LuaPlugin {
    pub fn new(name: &str, code: &str) -> Result<Self> {
        let lua = Lua::new();
        // Register API
        let api = lua.create_table()?;
        // Would register API functions here
        
        Ok(Self {
            name: name.to_string(),
            lua,
            code: code.to_string(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn execute(&self, func: &str, args: &[Value]) -> Result<()> {
        let func = self.lua.globals().get::<_, mlua::Function>(func)?;
        func.call(args)?;
        Ok(())
    }
}

/// Plugin manager
pub struct PluginManager {
    plugins: HashMap<String, LuaPlugin>,
    plugin_dir: std::path::PathBuf,
}

impl PluginManager {
    pub async fn new(config: &crate::config::PwnConfig) -> Result<Self> {
        let mut mgr = Self {
            plugins: HashMap::new(),
            plugin_dir: std::path::PathBuf::from(&config.main.custom_plugins),
        };

        mgr.load_plugins().await?;
        Ok(mgr)
    }

    async fn load_plugins(&mut self) -> Result<()> {
        if !self.plugin_dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().map_or(false, |e| e == "lua") {
                let name = entry.file_name().to_string_lossy().to_string();
                let code = tokio::fs::read_to_string(entry.path()).await?;
                
                match LuaPlugin::new(&name, &code) {
                    Ok(plugin) => {
                        self.plugins.insert(name.clone(), plugin);
                        info!("Loaded plugin: {}", name);
                    }
                    Err(e) => {
                        warn!("Failed to load plugin {}: {}", name, e);
                    }
                }
            }
        }

        // Load built-in plugins
        self.load_builtin_plugins()?;
        Ok(())
    }

    fn load_builtin_plugins(&mut self) -> Result<()> {
        // Built-in plugins as strings
        let builtins = [
            ("auto_tune", include_str!("../../lua/auto_tune.lua")),
            ("auto_backup", include_str!("../../lua/auto_backup.lua")),
            ("auto_update", include_str!("../../lua/auto_update.lua")),
            ("bt_tether", include_str!("../../lua/bt_tether.lua")),
            ("cache", include_str!("../../lua/cache.lua")),
            ("fix_services", include_str!("../../lua/fix_services.lua")),
            ("gpio_buttons", include_str!("../../lua/gpio_buttons.lua")),
            ("gps", include_str!("../../lua/gps.lua")),
            ("grid", include_str!("../../lua/grid.lua")),
            ("logtail", include_str!("../../lua/logtail.lua")),
            ("memtemp", include_str!("../../lua/memtemp.lua")),
            ("ohcapi", include_str!("../../lua/ohcapi.lua")),
            ("pisugarx", include_str!("../../lua/pisugarx.lua")),
            ("pwncrack", include_str!("../../lua/pwncrack.lua")),
            ("session_stats", include_str!("../../lua/session_stats.lua")),
            ("ups_lite", include_str!("../../lua/ups_lite.lua")),
            ("webcfg", include_str!("../../lua/webcfg.lua")),
            ("wigle", include_str!("../../lua/wigle.lua")),
            ("wpa_sec", include_str!("../../lua/wpa_sec.lua")),
        ];

        for (name, code) in builtins {
            if let Ok(plugin) = LuaPlugin::new(name, code) {
                self.plugins.insert(name.to_string(), plugin);
            }
        }

        Ok(())
    }

    pub fn get_plugin(&self, name: &str) -> Option<&LuaPlugin> {
        self.plugins.get(name)
    }

    pub async fn on_epoch(&self, epoch: u64, status: &crate::EpochStatus) -> Result<()> {
        // Call on_epoch for all plugins
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.execute("on_epoch", &[serde_json::to_value(epoch)?, serde_json::to_value(status)?]) {
                warn!("Plugin {} on_epoch error: {}", name, e);
            }
        }
        Ok(())
    }

    pub fn list_plugins(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager() {
        let config = crate::config::PwnConfig::default();
        let mgr = PluginManager::new(&config).await.unwrap();
        assert!(mgr.list_plugins().len() > 0);
    }
}