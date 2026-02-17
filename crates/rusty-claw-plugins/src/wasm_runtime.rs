//! WASM plugin runtime — loads and manages WebAssembly plugin modules.

use std::path::Path;

use tracing::info;
use wasmtime::{Engine, Module};

/// Loads WASM plugin modules from disk or memory.
pub struct WasmPluginLoader {
    engine: Engine,
}

/// A loaded WASM module ready for instantiation.
pub struct WasmModule {
    pub engine: Engine,
    pub module: Module,
    pub name: String,
}

impl WasmPluginLoader {
    /// Create a new WASM plugin loader with an async-capable engine.
    pub fn new() -> anyhow::Result<Self> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    /// Load a WASM module from a file path.
    pub fn load_module(&self, path: &Path) -> anyhow::Result<WasmModule> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!(path = %path.display(), name = %name, "Loading WASM plugin module");
        let module = Module::from_file(&self.engine, path)?;

        Ok(WasmModule {
            engine: self.engine.clone(),
            module,
            name,
        })
    }

    /// Load a WASM module from bytes in memory.
    pub fn load_bytes(&self, name: &str, bytes: &[u8]) -> anyhow::Result<WasmModule> {
        let module = Module::new(&self.engine, bytes)?;
        Ok(WasmModule {
            engine: self.engine.clone(),
            module,
            name: name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = WasmPluginLoader::new();
        assert!(loader.is_ok());
    }

    #[test]
    fn test_invalid_bytes_error() {
        let loader = WasmPluginLoader::new().unwrap();
        let result = loader.load_bytes("bad", b"not valid wasm");
        assert!(result.is_err());
    }
}
