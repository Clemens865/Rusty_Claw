//! Tailscale Funnel integration.
//!
//! Optionally exposes the gateway to the public internet via Tailscale Funnel.

use tracing::{error, info, warn};

/// Set up Tailscale Funnel to expose a local port.
/// Returns the public URL on success.
pub async fn setup_tailscale_funnel(port: u16) -> anyhow::Result<Option<String>> {
    // Check if tailscale CLI is available
    let check = tokio::process::Command::new("tailscale")
        .arg("version")
        .output()
        .await;

    match check {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            info!(version = %version.trim(), "Tailscale CLI found");
        }
        _ => {
            warn!("Tailscale CLI not found. Install it from https://tailscale.com/download");
            return Ok(None);
        }
    }

    // Enable funnel on the port
    info!(port, "Setting up Tailscale Funnel");
    let funnel = tokio::process::Command::new("tailscale")
        .args(["funnel", &port.to_string()])
        .output()
        .await;

    match funnel {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!("Tailscale Funnel enabled: {}", stdout.trim());

            // Try to get the funnel URL
            let status = tokio::process::Command::new("tailscale")
                .args(["funnel", "status"])
                .output()
                .await;

            if let Ok(status_output) = status {
                let status_text = String::from_utf8_lossy(&status_output.stdout);
                // Extract URL from status output (simple heuristic)
                for line in status_text.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("https://") {
                        return Ok(Some(trimmed.to_string()));
                    }
                }
            }

            Ok(Some(format!("https://<your-machine>.ts.net:{port}")))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(%stderr, "Tailscale Funnel setup failed");
            Err(anyhow::anyhow!("Tailscale Funnel failed: {stderr}"))
        }
        Err(e) => {
            error!(%e, "Failed to run tailscale funnel command");
            Err(anyhow::anyhow!("Failed to run tailscale: {e}"))
        }
    }
}
