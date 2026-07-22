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
    pub total_handshakes: u32,
    pub mood: String,
    pub peers: Vec<PeerInfo>,
    pub level: u32,
    pub xp: u32,
    pub uptime: u64,
    pub name: String,
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

    /// The canonical set of built-in plugins, embedded at compile time.
    /// Exposed so the web layer can enumerate every built-in (loaded or not)
    /// for the plugins page. Keep this list and `default_plugins()` in
    /// `config/src/schema.rs` in sync.
    pub const BUILTINS: &'static [(&'static str, &'static str)] = &[
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

    /// The names of every built-in plugin (see [`Self::BUILTINS`]).
    pub fn builtin_names() -> impl Iterator<Item = &'static str> {
        Self::BUILTINS.iter().map(|(n, _)| *n)
    }

    /// Whether a plugin should be loaded, per its `[plugins.<name>].enabled`
    /// flag. Unlisted plugins default to enabled (matches `PluginConfig`'s
    /// own default), but every built-in is listed in `defaults.toml`, so in
    /// practice this reads the real per-plugin flag.
    fn plugin_enabled(
        plugins_cfg: &HashMap<String, config::schema::PluginConfig>,
        name: &str,
    ) -> bool {
        plugins_cfg.get(name).map(|p| p.enabled).unwrap_or(true)
    }

    /// Create a plugin manager and load plugins described by `config`.
    pub async fn load(config: &config::PwnConfig) -> Result<Self> {
        let mut mgr = Self {
            plugins: HashMap::new(),
            plugin_dir: std::path::PathBuf::from(&config.main.custom_plugins),
        };

        mgr.load_plugins(&config.plugins).await?;
        Ok(mgr)
    }

    async fn load_plugins(
        &mut self,
        plugins_cfg: &HashMap<String, config::schema::PluginConfig>,
    ) -> Result<()> {
        // Load user plugins from disk if the directory exists, gated by their
        // `[plugins.<name>].enabled` flag (the plugin's key is its filename
        // without the `.lua` extension).
        if self.plugin_dir.exists() {
            let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.path().extension().is_some_and(|e| e == "lua") {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let key = name.strip_suffix(".lua").unwrap_or(&name);
                    if !Self::plugin_enabled(plugins_cfg, key) {
                        info!("Skipping disabled plugin: {}", key);
                        continue;
                    }
                    let code = tokio::fs::read_to_string(entry.path()).await?;

                    match LuaPlugin::new(key, &code) {
                        Ok(plugin) => {
                            if let Err(e) = plugin.execute("on_loaded") {
                                warn!("Plugin {} on_loaded error: {}", key, e);
                            }
                            self.plugins.insert(key.to_string(), plugin);
                            info!("Loaded plugin: {}", key);
                        }
                        Err(e) => {
                            warn!("Failed to load plugin {}: {}", key, e);
                        }
                    }
                }
            }
        }

        // Load built-in plugins, each gated by its enabled flag so the config
        // toggle actually controls what runs (previously every built-in loaded
        // unconditionally and the flag did nothing).
        self.load_builtin_plugins(plugins_cfg);
        Ok(())
    }

    fn load_builtin_plugins(
        &mut self,
        plugins_cfg: &HashMap<String, config::schema::PluginConfig>,
    ) {
        for &(name, code) in Self::BUILTINS {
            if !Self::plugin_enabled(plugins_cfg, name) {
                continue;
            }
            match LuaPlugin::new(name, code) {
                Ok(plugin) => {
                    if let Err(e) = plugin.execute("on_loaded") {
                        warn!("Plugin {} on_loaded error: {}", name, e);
                    }
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
    /// real captured BSSID/SSID/file paths as Lua globals -- previously
    /// plugins had no way to react to an actual handshake capture at
    /// all (`wpa_sec`/`wigle`/`grid`/`pwncrack` all need this to upload
    /// or log the real file, not just observe the epoch counter go up).
    ///
    /// `hashcat_path` is the validated `.hc22000` hash file; `pcap_path` is
    /// the raw `.pcapng` capture. Both are exposed as globals
    /// (`handshake_path` / `handshake_pcap_path`) because different services
    /// want different formats -- wpa-sec/OnlineHashCrack want the raw pcap,
    /// local hashcat wants the `.hc22000`.
    pub async fn on_handshake(
        &self,
        bssid: &str,
        ssid: &str,
        hashcat_path: &str,
        pcap_path: &str,
    ) -> Result<()> {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            if let Err(e) = globals
                .set("handshake_bssid", bssid)
                .and_then(|_| globals.set("handshake_ssid", ssid))
                .and_then(|_| globals.set("handshake_path", hashcat_path))
                .and_then(|_| globals.set("handshake_pcap_path", pcap_path))
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

    /// Invoke the `on_association` hook of every loaded plugin.
    /// Sets `assoc_bssid` and `assoc_ssid` Lua globals before calling.
    /// Matches real pwnagotchi's plugin hook of the same name.
    pub async fn on_association(&self, bssid: &str, ssid: &str) -> Result<()> {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            if let Err(e) = globals
                .set("assoc_bssid", bssid)
                .and_then(|_| globals.set("assoc_ssid", ssid))
            {
                warn!("Plugin {} on_association context error: {}", name, e);
                continue;
            }
            if let Err(e) = plugin.execute("on_association") {
                warn!("Plugin {} on_association error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Invoke the `on_deauthentication` hook of every loaded plugin.
    /// Sets `deauth_bssid`, `deauth_ssid`, and `deauth_sta` Lua globals.
    /// Matches real pwnagotchi's plugin hook of the same name.
    pub async fn on_deauthentication(
        &self,
        bssid: &str,
        ssid: &str,
        sta: &str,
    ) -> Result<()> {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            if let Err(e) = globals
                .set("deauth_bssid", bssid)
                .and_then(|_| globals.set("deauth_ssid", ssid))
                .and_then(|_| globals.set("deauth_sta", sta))
            {
                warn!("Plugin {} on_deauthentication context error: {}", name, e);
                continue;
            }
            if let Err(e) = plugin.execute("on_deauthentication") {
                warn!("Plugin {} on_deauthentication error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Invoke the `on_channel_hop` hook of every loaded plugin.
    /// Sets `old_channel` and `new_channel` Lua globals.
    /// Matches real pwnagotchi's plugin hook of the same name.
    pub async fn on_channel_hop(&self, old_ch: u8, new_ch: u8) -> Result<()> {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            if let Err(e) = globals
                .set("old_channel", old_ch)
                .and_then(|_| globals.set("new_channel", new_ch))
            {
                warn!("Plugin {} on_channel_hop context error: {}", name, e);
                continue;
            }
            if let Err(e) = plugin.execute("on_channel_hop") {
                warn!("Plugin {} on_channel_hop error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Invoke the `on_internet_available` hook of every loaded plugin.
    /// No additional Lua globals are set (the plugin can use the existing
    /// `epoch`/`status_json` context). Matches real pwnagotchi's plugin
    /// hook of the same name, used by `bt_tether` to know when it can
    /// sync, and by `grid` to announce via the web API.
    pub async fn on_internet_available(&self) -> Result<()> {
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.execute("on_internet_available") {
                warn!("Plugin {} on_internet_available error: {}", name, e);
            }
        }
        Ok(())
    }

    /// Set the `agent` Lua global table on every loaded plugin.
    /// Fields mirror the OG pwnagotchi `agent` object that plugin
    /// hook callbacks receive — plugins reference `agent.mood`,
    /// `agent.channel`, etc.  Called at the start of every hook
    /// invocation so plugins always see fresh state.
    pub fn set_agent_globals(&self, agent_ref: &AgentRef) {
        for (name, plugin) in &self.plugins {
            let globals = plugin.lua.globals();
            let table = match plugin.lua.create_table() {
                Ok(t) => t,
                Err(e) => {
                    warn!("Plugin {} create_table error: {}", name, e);
                    continue;
                }
            };

            if let Err(e) = table.set("mood", agent_ref.mood.clone()) {
                warn!("Plugin {} set agent.mood error: {}", name, e);
            }
            if let Err(e) = table.set("channel", agent_ref.current_channel as u64) {
                warn!("Plugin {} set agent.channel error: {}", name, e);
            }
            if let Err(e) = table.set("aps_count", agent_ref.aps_count as u64) {
                warn!("Plugin {} set agent.aps_count error: {}", name, e);
            }
            if let Err(e) = table.set("handshakes", agent_ref.handshakes) {
                warn!("Plugin {} set agent.handshakes error: {}", name, e);
            }
            if let Err(e) = table.set("total_handshakes", agent_ref.total_handshakes) {
                warn!("Plugin {} set agent.total_handshakes error: {}", name, e);
            }
            if let Err(e) = table.set("epoch", agent_ref.current_epoch) {
                warn!("Plugin {} set agent.epoch error: {}", name, e);
            }
            if let Err(e) = table.set("level", agent_ref.level) {
                warn!("Plugin {} set agent.level error: {}", name, e);
            }
            if let Err(e) = table.set("xp", agent_ref.xp) {
                warn!("Plugin {} set agent.xp error: {}", name, e);
            }
            if let Err(e) = table.set("uptime", agent_ref.uptime) {
                warn!("Plugin {} set agent.uptime error: {}", name, e);
            }
            if let Err(e) = table.set("name", agent_ref.name.clone()) {
                warn!("Plugin {} set agent.name error: {}", name, e);
            }

            // Peers as array of tables
            let peers_table = match plugin.lua.create_table() {
                Ok(t) => t,
                Err(e) => {
                    warn!("Plugin {} create peers_table error: {}", name, e);
                    continue;
                }
            };
            for (i, peer) in agent_ref.peers.iter().enumerate() {
                let peer_table = match plugin.lua.create_table() {
                    Ok(t) => t,
                    Err(e) => {
                        warn!("Plugin {} create peer_table error: {}", name, e);
                        continue;
                    }
                };
                if let Err(e) = peer_table.set("mac", peer.mac.clone()) {
                    warn!("Plugin {} set peer.mac error: {}", name, e);
                }
                if let Err(e) = peer_table.set("name", peer.name.clone()) {
                    warn!("Plugin {} set peer.name error: {}", name, e);
                }
                if let Err(e) = peer_table.set("channel", peer.channel as u64) {
                    warn!("Plugin {} set peer.channel error: {}", name, e);
                }
                if let Err(e) = peer_table.set("mood", peer.mood.clone()) {
                    warn!("Plugin {} set peer.mood error: {}", name, e);
                }
                if let Err(e) = peer_table.set("level", peer.level) {
                    warn!("Plugin {} set peer.level error: {}", name, e);
                }
                if let Err(e) = peers_table.set(i + 1, peer_table) {
                    warn!("Plugin {} set peers[{}] error: {}", name, i + 1, e);
                }
            }
            if let Err(e) = table.set("peers", peers_table) {
                warn!("Plugin {} set agent.peers error: {}", name, e);
            }

            if let Err(e) = globals.set("agent", table) {
                warn!("Plugin {} set_agent_globals error: {}", name, e);
            }
        }
    }

    /// Invoke a mood-transition hook on every loaded plugin.
    /// Maps each `Mood` variant to its corresponding Lua hook name
    /// (e.g. `on_grateful`, `on_lonely`) and calls it with fresh
    /// `agent` globals.  Unknown/unmapped moods are silently ignored.
    pub fn fire_mood_hook(&self, mood: &pwncore::Mood, agent_ref: &AgentRef) {
        let hook = match mood {
            pwncore::Mood::Grateful => "on_grateful",
            pwncore::Mood::Lonely => "on_lonely",
            pwncore::Mood::Bored => "on_bored",
            pwncore::Mood::Sad => "on_sad",
            pwncore::Mood::Angry => "on_angry",
            pwncore::Mood::Excited => "on_excited",
            pwncore::Mood::Motivated => "on_motivated",
            pwncore::Mood::Demotivated => "on_demotivated",
            pwncore::Mood::Broken => "on_rebooting",
            pwncore::Mood::Sleep => "on_sleep",
            _ => return,
        };

        self.set_agent_globals(agent_ref);
        for (name, plugin) in &self.plugins {
            if let Err(e) = plugin.execute(hook) {
                warn!("Plugin {} {} error: {}", name, hook, e);
            }
        }
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
