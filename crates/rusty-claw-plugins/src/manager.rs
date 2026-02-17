//! Plugin manager — collects plugins and drives their initialization.

use std::collections::HashSet;
use std::sync::Arc;

use tracing::info;

use crate::api::PluginApi;
use crate::hooks::HookRegistry;
use crate::Plugin;
use rusty_claw_tools::Tool;

/// Manages plugin lifecycle: registration, initialization, and access.
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    hooks: Arc<HookRegistry>,
    plugin_ids: HashSet<String>,
}

/// Collected registrations from all plugins after initialization.
pub struct PluginRegistrations {
    pub tools: Vec<Box<dyn Tool>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            hooks: Arc::new(HookRegistry::new()),
            plugin_ids: HashSet::new(),
        }
    }

    /// Add a plugin. Returns an error if a plugin with the same ID is already registered.
    pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) -> anyhow::Result<()> {
        let id = plugin.id().to_string();
        if !self.plugin_ids.insert(id.clone()) {
            anyhow::bail!("Duplicate plugin ID: {id}");
        }
        info!(plugin_id = %id, plugin_name = %plugin.name(), "Plugin added");
        self.plugins.push(plugin);
        Ok(())
    }

    /// Initialize all plugins: call their `register` method and collect registrations.
    pub async fn initialize(&mut self) -> anyhow::Result<PluginRegistrations> {
        let mut all_tools: Vec<Box<dyn Tool>> = Vec::new();

        for plugin in &self.plugins {
            info!(plugin_id = %plugin.id(), "Initializing plugin");
            let mut api = PluginApi::new();
            plugin.register(&mut api);
            all_tools.extend(api.take_tools());

            // Apply collected hooks to the registry
            for (event, handler) in api.take_hooks() {
                self.hooks.register(event, handler).await;
            }
        }

        Ok(PluginRegistrations { tools: all_tools })
    }

    /// Get a reference to the shared hook registry.
    pub fn hooks(&self) -> Arc<HookRegistry> {
        self.hooks.clone()
    }

    /// Load and add a WASM plugin from a file path.
    #[cfg(feature = "wasm")]
    pub fn add_wasm_plugin(
        &mut self,
        path: &std::path::Path,
        loader: &crate::wasm_runtime::WasmPluginLoader,
    ) -> anyhow::Result<()> {
        let module = loader.load_module(path)?;
        let id = format!("wasm:{}", module.name);
        let name = module.name.clone();
        let adapter = crate::wasm_adapter::WasmPluginAdapter::new(id, name, module);
        self.add_plugin(Box::new(adapter))
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
    use crate::hooks::HookResult;

    struct TestPlugin {
        id: String,
    }

    impl Plugin for TestPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            "Test Plugin"
        }
        fn register(&self, api: &mut PluginApi) {
            api.register_hook(
                crate::HookEvent::BeforeAgentStart,
                Box::new(|_ctx, _data| Box::pin(async { Ok(HookResult::Continue) })),
            );
        }
    }

    #[tokio::test]
    async fn test_initialize_returns_registrations() {
        let mut mgr = PluginManager::new();
        mgr.add_plugin(Box::new(TestPlugin {
            id: "test-1".into(),
        }))
        .unwrap();
        let regs = mgr.initialize().await.unwrap();
        assert!(regs.tools.is_empty());
        assert_eq!(
            mgr.hooks().count(crate::HookEvent::BeforeAgentStart).await,
            1
        );
    }

    #[test]
    fn test_duplicate_plugin_id() {
        let mut mgr = PluginManager::new();
        mgr.add_plugin(Box::new(TestPlugin {
            id: "dup".into(),
        }))
        .unwrap();
        let result = mgr.add_plugin(Box::new(TestPlugin {
            id: "dup".into(),
        }));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate plugin ID"));
    }
}
