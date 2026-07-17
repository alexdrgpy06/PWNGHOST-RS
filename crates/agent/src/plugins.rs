pub struct PluginManager;

impl PluginManager {
    pub fn new() -> Self {
        Self
    }

    pub fn load_all(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_new() {
        let mgr = PluginManager::new();
        let result = mgr.load_all();
        assert!(result.is_ok());
    }
}
