//! Image generation tool using OpenAI DALL-E or Stability AI.

use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use crate::{Tool, ToolContext, ToolOutput};

pub struct ImageGenerationTool;

/// Generate a unique output filename.
fn output_filename() -> std::path::PathBuf {
    let dir = rusty_claw_core::config::data_dir().join("images");
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let id = uuid::Uuid::new_v4().simple().to_string();
    dir.join(format!("image_{ts}_{}.png", &id[..8]))
}

/// Parse a size string like "1024x1024" into (width, height).
fn parse_size(size: &str) -> (u32, u32) {
    let parts: Vec<&str> = size.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().unwrap_or(1024);
        let h = parts[1].parse().unwrap_or(1024);
        (w, h)
    } else {
        (1024, 1024)
    }
}

#[async_trait]
impl Tool for ImageGenerationTool {
    fn name(&self) -> &str {
        "generate_image"
    }

    fn description(&self) -> &str {
        "Generate an image from a text prompt using DALL-E or Stability AI. Returns the path to the saved image."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text description of the image to generate"
                },
                "size": {
                    "type": "string",
                    "description": "Image size (e.g. '1024x1024', '1024x1792'). Default: 1024x1024"
                },
                "model": {
                    "type": "string",
                    "description": "Model name (e.g. 'dall-e-3', 'stable-diffusion-xl-1024-v1-0')"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of images to generate (default: 1)"
                },
                "quality": {
                    "type": "string",
                    "description": "Image quality ('standard' or 'hd', OpenAI only)"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'prompt' parameter"))?;

        let img_config = context
            .config
            .tools
            .as_ref()
            .and_then(|t| t.image_generation.as_ref());

        let provider = img_config
            .map(|c| c.provider.as_str())
            .unwrap_or("openai");

        let api_key = img_config
            .and_then(|c| c.resolve_api_key())
            .or_else(|| {
                let env_var = match provider {
                    "stability" => "STABILITY_API_KEY",
                    _ => "OPENAI_API_KEY",
                };
                std::env::var(env_var).ok().filter(|v| !v.is_empty())
            });

        let api_key = match api_key {
            Some(key) => key,
            None => {
                return Ok(ToolOutput {
                    content: format!(
                        "Image generation not configured. Set tools.image_generation.api_key in config or {} environment variable.",
                        if provider == "stability" { "STABILITY_API_KEY" } else { "OPENAI_API_KEY" }
                    ),
                    is_error: true,
                    media: None,
                });
            }
        };

        let size = params
            .get("size")
            .and_then(|v| v.as_str())
            .or_else(|| img_config.and_then(|c| c.default_size.as_deref()))
            .unwrap_or("1024x1024");

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| img_config.and_then(|c| c.default_model.as_deref()));

        let quality = params
            .get("quality")
            .and_then(|v| v.as_str())
            .or_else(|| img_config.and_then(|c| c.default_quality.as_deref()))
            .unwrap_or("standard");

        match provider {
            "stability" => {
                self.generate_stability(&api_key, prompt, size, model)
                    .await
            }
            _ => {
                self.generate_openai(&api_key, prompt, size, model, quality)
                    .await
            }
        }
    }
}

impl ImageGenerationTool {
    async fn generate_openai(
        &self,
        api_key: &str,
        prompt: &str,
        size: &str,
        model: Option<&str>,
        quality: &str,
    ) -> anyhow::Result<ToolOutput> {
        let model = model.unwrap_or("dall-e-3");
        let client = reqwest::Client::new();

        let resp = client
            .post("https://api.openai.com/v1/images/generations")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&json!({
                "model": model,
                "prompt": prompt,
                "n": 1,
                "size": size,
                "quality": quality,
                "response_format": "b64_json",
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok(ToolOutput {
                content: format!("OpenAI API error ({status}): {body}"),
                is_error: true,
                media: None,
            });
        }

        let body: serde_json::Value = resp.json().await?;
        let b64 = body["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No image data in response"))?;

        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let file_path = output_filename();

        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, &bytes).await?;

        let size_kb = bytes.len() / 1024;
        info!(
            path = %file_path.display(),
            size_kb,
            model,
            "Image generated (OpenAI)"
        );

        Ok(ToolOutput {
            content: format!(
                "Image saved to: {}\nSize: {}KB\nModel: {}\nPrompt: {}",
                file_path.display(),
                size_kb,
                model,
                prompt,
            ),
            is_error: false,
            media: None,
        })
    }

    async fn generate_stability(
        &self,
        api_key: &str,
        prompt: &str,
        size: &str,
        model: Option<&str>,
    ) -> anyhow::Result<ToolOutput> {
        let engine = model.unwrap_or("stable-diffusion-xl-1024-v1-0");
        let (width, height) = parse_size(size);
        let client = reqwest::Client::new();

        let url = format!("https://api.stability.ai/v1/generation/{engine}/text-to-image");
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&json!({
                "text_prompts": [{"text": prompt, "weight": 1.0}],
                "cfg_scale": 7,
                "width": width,
                "height": height,
                "samples": 1,
                "steps": 30,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Ok(ToolOutput {
                content: format!("Stability AI API error ({status}): {body}"),
                is_error: true,
                media: None,
            });
        }

        let body: serde_json::Value = resp.json().await?;
        let b64 = body["artifacts"][0]["base64"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No image data in response"))?;

        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
        let file_path = output_filename();

        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file_path, &bytes).await?;

        let size_kb = bytes.len() / 1024;
        info!(
            path = %file_path.display(),
            size_kb,
            engine,
            "Image generated (Stability AI)"
        );

        Ok(ToolOutput {
            content: format!(
                "Image saved to: {}\nSize: {}KB\nEngine: {}\nPrompt: {}",
                file_path.display(),
                size_kb,
                engine,
                prompt,
            ),
            is_error: false,
            media: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameters_schema_has_required_prompt() {
        let tool = ImageGenerationTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("prompt")));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1024x1024"), (1024, 1024));
        assert_eq!(parse_size("512x768"), (512, 768));
        assert_eq!(parse_size("invalid"), (1024, 1024));
        assert_eq!(parse_size(""), (1024, 1024));
    }

    #[tokio::test]
    async fn test_missing_config_returns_error() {
        let ctx = ToolContext {
            session_key: "test".into(),
            workspace: std::env::temp_dir(),
            config: std::sync::Arc::new(rusty_claw_core::config::Config::default()),
            restrict_to_workspace: false,
            sandbox_mode: rusty_claw_core::config::SandboxMode::default(),
        };

        let saved = std::env::var("OPENAI_API_KEY").ok();
        unsafe { std::env::remove_var("OPENAI_API_KEY") };

        let result = ImageGenerationTool
            .execute(json!({"prompt": "a cat"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));

        if let Some(val) = saved {
            unsafe { std::env::set_var("OPENAI_API_KEY", val) };
        }
    }
}
