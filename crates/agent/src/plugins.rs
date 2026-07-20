//! Lua plugin system

use crate::epoch::EpochState;
use anyhow::Result;
use mlua::Lua;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

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
}

impl LuaPlugin {
    pub fn new(name: &str, code: &str) -> Result<Self> {
        let lua = Lua::new();
        // Load the plugin source so its hook functions become globals.
        lua.load(code).exec()?;
        Ok(Self {
            name: name.to_string(),
            lua,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Expose per-epoch context to the plugin via Lua globals.
    pub fn set_context(&self, epoch: u64, status: &EpochState) -> Result<()> {
        let globals = self.lua.globals();
        globals.set("epoch", epoch)?;
        globals.set("status_json", serde_json::to_string(status)?)?;
        Ok(())
    }

    /// Call a global Lua function by name if it exists.
    pub fn execute(&self, func: &str) -> Result<()> {
        if let Ok(f) = self.lua.globals().get::<mlua::Function>(func) {
            f.call::<()>(())?;
        }
        Ok(())
    }
}

/// Plugin manager
pub struct PluginManager {
    plugins: HashMap<String, LuaPlugin>,
    plugin_dir: std::path::PathBuf,
}

impl PluginManager {
    /// Create an empty plugin manager (no plugins loaded yet).
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_dir: std::path::PathBuf::new(),
        }
    }

    /// Create a plugin manager and load plugins described by `config`.
    pub async fn load(config: &config::PwnConfig) -> Result<Self> {
        let mut mgr = Self {
            plugins: HashMap::new(),
            plugin_dir: std::path::PathBuf::from(&config.main.custom_plugins),
        };

        mgr.load_plugins().await?;
        Ok(mgr)
    }

    async fn load_plugins(&mut self) -> Result<()> {
        // Load user plugins from disk if the directory exists.
        if self.plugin_dir.exists() {
            let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.path().extension().is_some_and(|e| e == "lua") {
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
        }

        // Load built-in plugins.
        self.load_builtin_plugins();
        Ok(())
    }

    fn load_builtin_plugins(&mut self) {
        // Built-in plugin sources, embedded at compile time.
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
            match LuaPlugin::new(name, code) {
                Ok(plugin) => {
                    self.plugins.entry(name.to_string()).or_insert(plugin);
                }
                Err(e) => warn!("Failed to load builtin plugin {}: {}", name, e),
            }
        }
    }

    pub fn get_plugin(&self, name: &str) -> Option<&LuaPlugin> {
        self.plugins.get(name)
    }

    /// Invoke the `on_epoch` hook of every loaded plugin.
    pub async fn on_epoch(&self, epoch: u64, status: &EpochState) -> Result<()> {
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin
                .set_context(epoch, status)
                .and_then(|_| plugin.execute("on_epoch"))
            {
                warn!("Plugin {} on_epoch error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Invoke the `on_ready` hook of every loaded plugin, once, after the
    /// agent/display/web/AngryOxide are all up. Matches real pwnagotchi's
    /// `on_starting`/plugin `on_ready` convention -- plugins that need
    /// one-time setup (e.g. `grid` announcing this unit, `webcfg` priming
    /// its own state) previously had no hook fired at startup at all,
    /// only per-epoch.
    pub async fn on_ready(&self) -> Result<()> {
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.execute("on_ready") {
                warn!("Plugin {} on_ready error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Invoke the `on_handshake` hook of every loaded plugin with the
    /// real captured BSSID/SSID/file path as Lua globals -- previously
    /// plugins had no way to react to an actual handshake capture at
    /// all (`wpa_sec`/`wigle`/`grid`/`pwncrack` all need this to upload
    /// or log the real file, not just observe the epoch counter go up).
    pub async fn on_handshake(&self, bssid: &str, ssid: &str, path: &str) -> Result<()> {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            if let Err(e) = globals
                .set("handshake_bssid", bssid)
                .and_then(|_| globals.set("handshake_ssid", ssid))
                .and_then(|_| globals.set("handshake_path", path))
            {
                warn!("Plugin {} on_handshake context error: {}", name, e);
                continue;
            }
            if let Err(e) = plugin.execute("on_handshake") {
                warn!("Plugin {} on_handshake error: {}", name, e);
            }
        }
        Ok(())
    }

    pub fn list_plugins(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager() {
        let config = config::PwnConfig::default();
        let mgr = PluginManager::load(&config).await.unwrap();
        assert!(!mgr.list_plugins().is_empty());
    }
}
