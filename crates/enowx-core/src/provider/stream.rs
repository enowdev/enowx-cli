use super::{Chunk, Completion};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

/// Decode after framing, not per network chunk: UTF-8 characters can span packets.
#[derive(Default)]
pub struct SseDecoder {
    bytes: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > 8 * 1024 * 1024 {
            bail!("SSE frame exceeds 8 MiB");
        }
        let mut frames = Vec::new();
        let mut consumed = 0;
        while let Some(index) = self.bytes[consumed..].iter().position(|b| *b == b'\n') {
            let end = consumed + index;
            let line = std::str::from_utf8(&self.bytes[consumed..end])
                .context("invalid UTF-8 in SSE")?
                .trim_end_matches('\r');
            consumed = end + 1;
            if line.is_empty() {
                if !self.data.is_empty() {
                    frames.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        self.bytes.drain(..consumed);
        Ok(frames)
    }
}

#[derive(Default)]
pub(super) struct PartialCall {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) arguments: String,
}

pub(super) async fn apply_frame(
    data: &str,
    result: &mut Completion,
    calls: &mut BTreeMap<usize, PartialCall>,
    sink: &mpsc::Sender<Chunk>,
) -> Result<()> {
    let frame: Frame = serde_json::from_str(data).context("invalid provider SSE JSON")?;
    if let Some(error) = frame.error {
        bail!("provider stream error: {error}");
    }
    if let Some(usage) = frame.usage {
        result.usage.input_tokens = usage.prompt_tokens;
        result.usage.output_tokens = usage.completion_tokens;
    }
    let Some(choice) = frame.choices.into_iter().next() else {
        return Ok(());
    };
    if let Some(reason) = choice.finish_reason {
        result.finish_reason = reason;
    }
    if let Some(text) = choice.delta.content.filter(|s| !s.is_empty()) {
        result.text.push_str(&text);
        sink.send(Chunk::Text(text))
            .await
            .context("stream consumer closed")?;
    }
    if let Some(text) = choice
        .delta
        .reasoning
        .or(choice.delta.reasoning_content)
        .filter(|s| !s.is_empty())
    {
        result.reasoning.push_str(&text);
        sink.send(Chunk::Reasoning(text))
            .await
            .context("stream consumer closed")?;
    }
    for fragment in choice.delta.tool_calls {
        if fragment.index >= 128 {
            bail!("provider tool index exceeds 127");
        }
        let call = calls.entry(fragment.index).or_default();
        if let Some(id) = fragment.id {
            call.id = id;
        }
        if let Some(name) = fragment.function.name {
            call.name.push_str(&name);
        }
        if let Some(arguments) = fragment.function.arguments {
            call.arguments.push_str(&arguments);
        }
        if call.arguments.len() > 8 * 1024 * 1024 {
            bail!("tool arguments exceed 8 MiB");
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct Frame {
    #[serde(default)]
    choices: Vec<Choice>,
    usage: Option<WireUsage>,
    error: Option<Value>,
}
#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}
#[derive(Default, Deserialize)]
struct Delta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<CallFragment>,
}
#[derive(Deserialize)]
struct CallFragment {
    index: usize,
    id: Option<String>,
    #[serde(default)]
    function: FunctionFragment,
}
#[derive(Default, Deserialize)]
struct FunctionFragment {
    name: Option<String>,
    arguments: Option<String>,
}
#[derive(Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}
