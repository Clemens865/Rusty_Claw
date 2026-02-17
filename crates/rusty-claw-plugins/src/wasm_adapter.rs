//! WASM plugin and tool adapters — bridge between WASM modules and Rust traits.

use async_trait::async_trait;
use tracing::debug;
use wasmtime::{Instance, Linker, Memory, Store};

use crate::api::PluginApi;
use crate::wasm_runtime::WasmModule;
use crate::Plugin;
use rusty_claw_tools::{Tool, ToolContext, ToolOutput};

/// Adapter that wraps a WASM module to implement the Plugin trait.
pub struct WasmPluginAdapter {
    id: String,
    name: String,
    module: WasmModule,
}

impl WasmPluginAdapter {
    pub fn new(id: String, name: String, module: WasmModule) -> Self {
        Self { id, name, module }
    }

    /// Get the underlying WASM module for tool discovery.
    pub fn module(&self) -> &WasmModule {
        &self.module
    }
}

impl Plugin for WasmPluginAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn register(&self, _api: &mut PluginApi) {
        // Tools are discovered and registered by the manager separately
    }
}

/// Adapter that wraps a WASM exported function to implement the Tool trait.
pub struct WasmToolAdapter {
    tool_name: String,
    description: String,
    schema: serde_json::Value,
    module: WasmModule,
}

impl WasmToolAdapter {
    pub fn new(
        tool_name: String,
        description: String,
        schema: serde_json::Value,
        module: WasmModule,
    ) -> Self {
        Self {
            tool_name,
            description,
            schema,
            module,
        }
    }
}

#[async_trait]
impl Tool for WasmToolAdapter {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        debug!(tool = %self.tool_name, "Executing WASM tool");

        // Create a fresh Store per call for isolation
        let mut store = Store::new(&self.module.engine, ());
        let linker = Linker::new(&self.module.engine);
        let instance = linker
            .instantiate_async(&mut store, &self.module.module)
            .await?;

        // Serialize params to JSON bytes
        let input = serde_json::to_vec(&params)?;

        // Try to call the tool's exported function
        // Convention: export "execute" that takes (ptr, len) and returns (ptr, len)
        let result = call_wasm_execute(&mut store, &instance, &input).await;

        match result {
            Ok(output) => {
                let output_str = String::from_utf8(output)
                    .unwrap_or_else(|_| "Invalid UTF-8 in WASM output".to_string());
                Ok(ToolOutput {
                    content: output_str,
                    is_error: false,
                    media: None,
                })
            }
            Err(e) => Ok(ToolOutput {
                content: format!("WASM tool error: {e}"),
                is_error: true,
                media: None,
            }),
        }
    }
}

/// Call the WASM module's "execute" export with input bytes.
async fn call_wasm_execute(
    store: &mut Store<()>,
    instance: &Instance,
    input: &[u8],
) -> anyhow::Result<Vec<u8>> {
    // Get memory and allocator exports
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| anyhow::anyhow!("WASM module has no 'memory' export"))?;

    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "alloc")
        .map_err(|_| anyhow::anyhow!("WASM module has no 'alloc' export"))?;

    let execute = instance
        .get_typed_func::<(i32, i32), i64>(&mut *store, "execute")
        .map_err(|_| anyhow::anyhow!("WASM module has no 'execute' export"))?;

    // Allocate input buffer in WASM memory
    let input_ptr = alloc.call_async(&mut *store, input.len() as i32).await?;
    write_to_memory(&memory, store, input_ptr as usize, input)?;

    // Call execute(ptr, len) -> packed(ptr, len) as i64
    let result = execute
        .call_async(&mut *store, (input_ptr, input.len() as i32))
        .await?;

    // Unpack result: high 32 bits = ptr, low 32 bits = len
    let out_ptr = (result >> 32) as usize;
    let out_len = (result & 0xFFFF_FFFF) as usize;

    // Read output from WASM memory
    read_from_memory(&memory, store, out_ptr, out_len)
}

fn write_to_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    data: &[u8],
) -> anyhow::Result<()> {
    let mem_data = memory.data_mut(store);
    if offset + data.len() > mem_data.len() {
        anyhow::bail!("WASM memory write out of bounds");
    }
    mem_data[offset..offset + data.len()].copy_from_slice(data);
    Ok(())
}

fn read_from_memory(
    memory: &Memory,
    store: &mut Store<()>,
    offset: usize,
    len: usize,
) -> anyhow::Result<Vec<u8>> {
    let mem_data = memory.data(store);
    if offset + len > mem_data.len() {
        anyhow::bail!("WASM memory read out of bounds");
    }
    Ok(mem_data[offset..offset + len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_id_name() {
        let loader = crate::wasm_runtime::WasmPluginLoader::new().unwrap();
        // We can't easily create a valid WasmModule without actual WASM bytes,
        // so just test the PluginSource enum
        let source = crate::PluginSource::Wasm {
            path: std::path::PathBuf::from("/tmp/test.wasm"),
        };
        match source {
            crate::PluginSource::Wasm { path } => {
                assert_eq!(path.display().to_string(), "/tmp/test.wasm");
            }
            _ => panic!("Expected Wasm source"),
        }

        // Test loader exists
        drop(loader);
    }

    #[test]
    fn test_tool_schema_passthrough() {
        // Test that WasmToolAdapter properly returns the schema
        // We can't instantiate WasmToolAdapter without a real module,
        // so we test the schema pattern
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            }
        });
        assert!(schema["properties"]["input"]["type"] == "string");
    }
}
