use std::{collections::BTreeMap, time::Duration};
mod models;
mod stream;
use models::parse_models;
pub use stream::SseDecoder;
use stream::{apply_frame, PartialCall};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    config::Config,
    message::{Message, ToolCall},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub models_url: &'static str,
    pub key_url: &'static str,
}
pub const PROVIDER_PRESETS: [ProviderPreset; 6] = [
    ProviderPreset {
        id: "enxapi",
        name: "enxapi",
        base_url: "https://enxapi.id/v1",
        models_url: "https://enxapi.id/v1/models",
        key_url: "https://enxapi.id",
    },
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        models_url: "https://api.openai.com/v1/models",
        key_url: "https://platform.openai.com/api-keys",
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        models_url: "https://openrouter.ai/api/v1/models",
        key_url: "https://openrouter.ai/settings/keys",
    },
    ProviderPreset {
        id: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        models_url: "https://api.groq.com/openai/v1/models",
        key_url: "https://console.groq.com/keys",
    },
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        models_url: "https://api.deepseek.com/models",
        key_url: "https://platform.deepseek.com/api_keys",
    },
    ProviderPreset {
        id: "custom",
        name: "Custom OpenAI-compatible",
        base_url: "",
        models_url: "",
        key_url: "",
    },
];

pub fn provider_preset(id: &str) -> Option<ProviderPreset> {
    PROVIDER_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id)
}

#[derive(Debug, Default)]
pub struct Completion {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug)]
pub enum Chunk {
    Text(String),
    Reasoning(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl ModelInfo {
    /// One line of detected facts. Absent values read as `unknown` rather than
    /// a guessed number, so nothing looks detected that the endpoint never sent.
    pub fn summary(&self) -> String {
        let context = self
            .context_window
            .map_or_else(|| "unknown".into(), |n| n.to_string());
        let output = self
            .max_output_tokens
            .map_or_else(|| "unknown".into(), |n| n.to_string());
        let mut summary = String::new();
        if let Some(name) = self.name.as_deref().filter(|name| *name != self.id) {
            summary.push_str(name);
            summary.push_str(" · ");
        }
        summary.push_str(&format!("context {context} · output {output}"));
        summary
    }
}

pub struct Provider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    temperature: Option<f32>,
    active: bool,
}

impl Provider {
    pub fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(600))
                .build()?,
            base_url: config.provider.base_url.trim_end_matches('/').to_owned(),
            api_key: config.provider.api_key.clone(),
            model: config.model.default.clone(),
            temperature: config.model.temperature,
            active: config.provider_active(),
        })
    }

    /// Detect model metadata from the exact URL supplied by the user.
    pub async fn models(&self, url: &str) -> Result<Vec<ModelInfo>> {
        anyhow::ensure!(
            self.active,
            "Set an active provider before detecting models"
        );
        let url = reqwest::Url::parse(url.trim()).context("Invalid model-list URL")?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "Model-list URL must be HTTP(S) without embedded credentials"
        );
        let base = reqwest::Url::parse(&self.base_url).context("Invalid provider base URL")?;
        anyhow::ensure!(
            matches!(base.scheme(), "http" | "https")
                && base.host_str().is_some()
                && base.username().is_empty()
                && base.password().is_none(),
            "Invalid provider base URL"
        );
        // A user-supplied catalogue may be hosted elsewhere; never send it the provider key.
        let same_origin = url.origin() == base.origin();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()?;
        let mut request = client.get(url);
        if same_origin && !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        let response = request.send().await.context("Fetching provider models")?;
        anyhow::ensure!(
            response.status().is_success(),
            "Model discovery returned {}",
            response.status()
        );
        let value: Value = response
            .json()
            .await
            .context("Model-list URL must return JSON")?;
        parse_models(&value)
    }
}

/// Notice callback used by the retry loop to surface `Notice` events to the
/// UI while the request is being retried. `None` disables notifications.
pub type NoticeSink = Option<Box<dyn Fn(String) + Send + Sync>>;

impl Provider {
    pub async fn complete(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
    ) -> Result<Completion> {
        self.complete_with_notice(messages, tools, sink, &None)
            .await
    }

    /// Same as `complete` but calls `notice` on every retry so the UI can
    /// show progress. Retries transient failures (network, 5xx, 429, stream
    /// disconnect) up to `MAX_RETRIES` with exponential backoff.
    pub async fn complete_with_notice(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
        notice: &NoticeSink,
    ) -> Result<Completion> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 1..=MAX_RETRIES {
            match self.complete_once(messages, tools, sink).await {
                Ok(c) => return Ok(c),
                Err(e) => {
                    let transient = is_transient(&e);
                    if !transient || attempt == MAX_RETRIES {
                        return Err(e);
                    }
                    let delay = backoff_ms(attempt);
                    if let Some(cb) = notice {
                        cb(format!(
                            "upstream error: {e}. retry {}/{} in {}ms",
                            attempt + 1,
                            MAX_RETRIES,
                            delay
                        ));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    last_err = Some(e);
                }
            }
        }
        // Loop always returns before reaching here; keep this fallback.
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("retry loop exhausted")))
    }

    async fn complete_once(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
    ) -> Result<Completion> {
        let mut payload = json!({
            "model": self.model,
            "messages": messages.iter().map(Message::to_wire).collect::<Vec<_>>(),
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(temperature) = self.temperature {
            payload["temperature"] = json!(temperature);
        }
        if !tools.is_empty() {
            payload["tools"] = json!(tools);
            payload["tool_choice"] = json!("auto");
        }
        let mut request = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&payload);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        let response = request.send().await.context("calling the provider")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.context("reading provider error")?;
            bail!(
                "provider returned {status}: {}",
                body.chars().take(2000).collect::<String>()
            );
        }
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut result = Completion::default();
        let mut calls: BTreeMap<usize, PartialCall> = BTreeMap::new();
        let mut done = false;
        while let Some(chunk) = stream.next().await {
            for data in decoder.push(&chunk.context("reading provider stream")?)? {
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                apply_frame(&data, &mut result, &mut calls, sink).await?;
            }
            if done {
                break;
            }
        }
        if result.finish_reason.is_empty() {
            bail!("provider stream ended without a finish reason; no tools executed");
        }
        for (_, call) in calls {
            if call.id.is_empty() || call.name.is_empty() {
                bail!("provider returned an incomplete tool call");
            }
            result.tool_calls.push(ToolCall {
                id: call.id,
                name: call.name,
                arguments: call.arguments,
            });
        }
        Ok(result)
    }
}

/// Hard cap on how many times a single completion is retried. Ten attempts
/// with the backoff schedule below tops out around 30 seconds of waiting
/// before giving up, which covers most transient upstream issues without
/// making the user sit forever.
const MAX_RETRIES: u32 = 10;

fn backoff_ms(attempt: u32) -> u64 {
    // 200, 400, 800, 1600, 3200, 6400, 8000, 8000, 8000
    let base = 200u64 << attempt.min(6);
    base.min(8_000)
}

/// True when an error is worth retrying: connection/timeout/5xx/429.
/// Non-transient (4xx auth, malformed request) returns immediately.
fn is_transient(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}").to_ascii_lowercase();
    // Server-side or transport failures.
    if msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("reset")
        || msg.contains("broken pipe")
        || msg.contains("eof")
        || msg.contains("finish reason")
        || msg.contains("provider returned 5")
        || msg.contains("provider returned 429")
        || msg.contains("provider returned 408")
        || msg.contains("service unavailable")
        || msg.contains("bad gateway")
        || msg.contains("gateway timeout")
    {
        return true;
    }
    // Auth, bad request, forbidden, not-found: never retry.
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_fragments_preserve_unicode_and_multiline_events() {
        let bytes = "data: héllo\r\ndata: 世界\r\n\r\n".as_bytes();
        let mut decoder = SseDecoder::default();
        let mut frames = Vec::new();
        for byte in bytes {
            frames.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(frames, ["héllo\n世界"]);
    }

    /// Catalogues differ per provider: ids may sit under `data`, `models`, or a
    /// bare array, and limits arrive under several names. Absent limits stay
    /// `None` so the UI never presents a guessed context window as detected.
    #[test]
    fn model_catalogues_expose_available_limits_only() {
        let openai_style = serde_json::json!({"data":[
            {"id":"vendor/b","context_length":128000,"top_provider":{"max_completion_tokens":8192}},
            {"id":"vendor/a"},
            {"id":"  "},
        ]});
        let models = parse_models(&openai_style).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "vendor/a");
        assert_eq!(models[0].context_window, None);
        assert_eq!(models[1].context_window, Some(128_000));
        assert_eq!(models[1].max_output_tokens, Some(8192));

        let gemini_style = serde_json::json!({"models":[
            {"name":"models/gemini","displayName":"Gemini","inputTokenLimit":"1048576","outputTokenLimit":65536},
        ]});
        let models = parse_models(&gemini_style).unwrap();
        assert_eq!(models[0].context_window, Some(1_048_576));
        assert_eq!(models[0].max_output_tokens, Some(65_536));

        assert!(parse_models(&serde_json::json!({"data":[]})).is_err());
        assert!(parse_models(&serde_json::json!({"error":"nope"})).is_err());
    }
    #[tokio::test]
    async fn interleaved_tool_arguments_remain_paired() {
        let (tx, _rx) = mpsc::channel(8);
        let mut result = Completion::default();
        let mut calls = BTreeMap::new();
        for data in [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"read","arguments":"{\"path\":"}},{"index":0,"id":"a","function":{"name":"glob","arguments":"{\"pattern\":\"*.rs\"}"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"\"lib.rs\"}"}}]},"finish_reason":"tool_calls"}]}"#,
        ] {
            apply_frame(data, &mut result, &mut calls, &tx)
                .await
                .unwrap();
        }
        assert_eq!(calls[&1].arguments, r#"{"path":"lib.rs"}"#);
        assert_eq!(calls[&0].arguments, r#"{"pattern":"*.rs"}"#);
        assert!(apply_frame(
            r#"{"error":{"message":"rate limit"}}"#,
            &mut result,
            &mut calls,
            &tx
        )
        .await
        .is_err());
    }
}
