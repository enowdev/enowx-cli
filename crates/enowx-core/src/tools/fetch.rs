use super::{string_arg, truncate_output, Tool, ToolCtx, ToolOutput};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Default)]
pub(super) struct FetchTool {
    client: reqwest::Client,
}

#[async_trait]
impl Tool for FetchTool {
    fn name(&self) -> &str {
        "fetch"
    }
    fn description(&self) -> &str {
        "Fetch an HTTP or HTTPS URL and return readable text, capped at 100 KB."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"],"additionalProperties":false})
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let url = string_arg(&args, "url")?;
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            anyhow::bail!("fetch accepts only http:// or https:// URLs");
        }
        let mut response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2_000_000 {
                anyhow::bail!("fetched document exceeds 2 MB");
            }
            bytes.extend_from_slice(&chunk);
        }
        let mut text = String::from_utf8(bytes).context("URL did not return UTF-8 text")?;
        if content_type.contains("text/html") {
            text = html_to_text(&text);
        }
        truncate_output(&mut text, 100_000);
        Ok(ToolOutput::ok(format!("URL: {url}\n\n{text}")))
    }
}

fn html_to_text(html: &str) -> String {
    use std::sync::LazyLock;
    static BLOCKS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>|<style\b[^>]*>.*?</style\s*>|<noscript\b[^>]*>.*?</noscript\s*>").expect("static regex")
    });
    static BREAKS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)</?(p|div|section|article|h[1-6]|li|br|tr)\b[^>]*>").expect("static regex")
    });
    static TAGS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<[^>]+>").expect("static regex"));
    let clean = BLOCKS.replace_all(html, "");
    let lines = BREAKS.replace_all(&clean, "\n");
    TAGS.replace_all(&lines, "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}
