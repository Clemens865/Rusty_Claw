//! Canvas operation and event types.

use serde::{Deserialize, Serialize};

/// Operations the agent can perform on a canvas session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CanvasOperation {
    /// Push HTML content to the canvas.
    Push { html: String },
    /// Clear all canvas content.
    Reset,
    /// Evaluate JavaScript in the canvas context.
    Eval { js: String },
    /// Request a snapshot of the current canvas state.
    Snapshot,
}

/// Events sent to connected canvas clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CanvasEvent {
    /// A new HTML component was added.
    ComponentAdded { index: usize, html: String },
    /// Canvas was reset.
    Reset,
    /// JavaScript evaluation request.
    Eval { js: String },
    /// Full snapshot of all components.
    Snapshot { components: Vec<String> },
}
