//! On-disk configuration. One file, `~/.enx/config.toml`, plus environment
//! overrides so a container can run without writing anything.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// Where the agent keeps configuration, sessions, and logs.
pub fn home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ENX_HOME") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".enx")
}

pub fn config_path() -> PathBuf {
    home_dir().join("config.toml")
}

pub fn sessions_dir() -> PathBuf {
    home_dir().join("sessions")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: ModelConfig,
    pub provider: ProviderConfig,
    pub server: ServerConfig,
    pub agent: AgentConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub show_sidebar: bool,
    /// Skills the user has toggled off via `/skills`.
    pub disabled_skills: Vec<String>,
    /// 3-letter ISO code shown next to prices. Defaults to `USD`.
    pub currency: String,
    /// Multiplier applied to USD prices before display (e.g. `15800.0` for
    /// IDR). `0` or `1.0` mean "no conversion, show as USD".
    pub currency_rate: f64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "obsidian_ice".to_string(),
            show_sidebar: true,
            disabled_skills: Vec::new(),
            currency: "USD".to_string(),
            currency_rate: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    /// Model id sent to the provider, e.g. `anthropic/claude-sonnet-4.5`.
    pub default: String,
    /// Sampling temperature. `None` keeps the provider default.
    pub temperature: Option<f32>,
    /// Context window in tokens, used for the fill gauge in the UI.
    pub context_window: u32,
    /// USD per 1M input tokens. 0 means unknown; sidebar shows the running
    /// cost as `$0.00` when unknown rather than hiding it.
    #[serde(default)]
    pub price_input: f64,
    /// USD per 1M output tokens.
    #[serde(default)]
    pub price_output: f64,
    /// USD per 1M cached-read tokens (prompt cache). Optional.
    #[serde(default)]
    pub price_cache_read: f64,
    /// True when the model accepts image inputs; drives whether `/attach`
    /// warns the user.
    #[serde(default)]
    pub vision: bool,
    /// True when the model can call tools.
    #[serde(default = "default_true")]
    pub tool_call: bool,
    /// True when the model produces a reasoning trace.
    #[serde(default)]
    pub reasoning: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            default: String::new(),
            temperature: None,
            context_window: 128_000,
            price_input: 0.0,
            price_output: 0.0,
            price_cache_read: 0.0,
            vision: false,
            tool_call: true,
            reasoning: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    /// Display name of the active provider.
    pub name: String,
    /// Preset id this provider came from, or `custom` for a hand-entered endpoint.
    pub preset: String,
    /// OpenAI-compatible base URL, without the trailing `/chat/completions`.
    pub base_url: String,
    /// Exact endpoint used by model auto-detection, e.g. `https://host/v1/models`.
    pub models_url: String,
    /// API key. `ENX_API_KEY` overrides this.
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8787,
            host: "127.0.0.1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Hard cap on model round-trips in a single turn, so a tool loop cannot spin forever.
    pub max_steps: u32,
    /// Working directory the file and shell tools are rooted in.
    pub workspace: Option<PathBuf>,
    /// Seconds a single shell command may run before it is killed.
    pub shell_timeout_secs: u64,
    /// Trigger auto-compact when this fraction of the context window is used
    /// (0.0 disables). Default 0.85: at 85% the agent summarises older turns
    /// before the next user request runs, so a long session never crashes on
    /// a full context.
    pub auto_compact_at: f32,
    /// Whether the auto-compact trigger fires. Users can still run `/compact`
    /// manually with this off.
    pub auto_compact: bool,
    /// Number of trailing turns to keep verbatim during compact. Older turns
    /// get folded into a single summary assistant turn.
    pub compact_keep_last: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 32,
            workspace: None,
            shell_timeout_secs: 120,
            auto_compact_at: 0.85,
            auto_compact: true,
            compact_keep_last: 4,
        }
    }
}

impl Config {
    /// Load `~/.enx/config.toml`, falling back to defaults when it is absent,
    /// then apply environment overrides.
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut config = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?
        } else {
            Self::default()
        };
        config.apply_env();
        // Fill missing metadata (context window, pricing, capabilities) from
        // the cached models.dev catalog. Never overrides fields the user or
        // provider already set.
        config.fill_from_catalog();
        config.validate()?;
        Ok(config)
    }

    /// Look up the current model in the cached catalog and fill any field
    /// the user did not set. Fetching a fresh catalog is a separate async
    /// step wired by the runtime; here we only read what is on disk.
    pub fn fill_from_catalog(&mut self) {
        if self.model.default.trim().is_empty() {
            return;
        }
        let catalog = crate::catalog::Catalog::load_cached();
        let Some(entry) = catalog.lookup(&self.model.default) else {
            return;
        };
        if (self.model.context_window == 0 || self.model.context_window == 128_000)
            && entry.limit.context > 0
        {
            self.model.context_window = entry.limit.context;
        }
        if self.model.price_input == 0.0 && entry.cost.input > 0.0 {
            self.model.price_input = entry.cost.input;
        }
        if self.model.price_output == 0.0 && entry.cost.output > 0.0 {
            self.model.price_output = entry.cost.output;
        }
        if self.model.price_cache_read == 0.0 {
            if let Some(cr) = entry.cost.cache_read {
                self.model.price_cache_read = cr;
            }
        }
        if !self.model.vision && entry.modalities.supports_vision() {
            self.model.vision = true;
        }
        if !self.model.reasoning && entry.reasoning {
            self.model.reasoning = true;
        }
        // `tool_call` defaults to true; only set false when catalog says so
        // and user has not explicitly enabled it (we cannot distinguish, so
        // leave alone).
    }

    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.agent.max_steps > 0 && self.agent.max_steps <= 256,
            "agent.max_steps must be 1..=256"
        );
        anyhow::ensure!(
            self.agent.shell_timeout_secs > 0 && self.agent.shell_timeout_secs <= 86_400,
            "agent.shell_timeout_secs must be 1..=86400"
        );
        anyhow::ensure!(
            self.model.context_window > 0,
            "model.context_window must be greater than zero"
        );
        if !self.provider.base_url.trim().is_empty() {
            validate_http_url(&self.provider.base_url, "provider.base_url")?;
        }
        if !self.provider.models_url.trim().is_empty() {
            validate_http_url(&self.provider.models_url, "provider.models_url")?;
        }
        Ok(())
    }

    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("ENX_API_KEY") {
            self.provider.api_key = v;
        }
        if let Ok(v) = std::env::var("ENX_BASE_URL") {
            self.provider.base_url = v;
            if self.provider.name.trim().is_empty() {
                self.provider.name = reqwest::Url::parse(&self.provider.base_url)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_owned))
                    .unwrap_or_default();
            }
        }
        if let Ok(v) = std::env::var("ENX_MODEL") {
            self.model.default = v;
        }
        if let Ok(v) = std::env::var("ENX_PORT") {
            if let Ok(port) = v.parse() {
                self.server.port = port;
            }
        }
        if let Ok(v) = std::env::var("ENX_THEME") {
            self.ui.theme = v;
        }
    }

    pub fn save(&self) -> Result<PathBuf> {
        self.validate()?;
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        atomic_write(&path, text.as_bytes())?;
        Ok(path)
    }

    /// The directory tools operate in: configured workspace, else the process cwd.
    pub fn workspace(&self) -> PathBuf {
        self.agent
            .workspace
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Whether a provider has been explicitly configured and can own models.
    pub fn provider_active(&self) -> bool {
        !self.provider.name.trim().is_empty() && !self.provider.base_url.trim().is_empty()
    }

    /// Whether the active provider has everything required for a model call.
    pub fn is_ready(&self) -> bool {
        let local = reqwest::Url::parse(&self.provider.base_url)
            .ok()
            .is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
        self.provider_active()
            && !self.model.default.trim().is_empty()
            && (local || !self.provider.api_key.is_empty())
    }

    /// Read a dotted key. Used by `enx config get`.
    pub fn get(&self, key: &str) -> Option<String> {
        // Never echo the key: `enx config get` output lands in shell history,
        // terminal scrollback, and pasted bug reports.
        let mut value = serde_json::to_value(self).ok()?;
        value["provider"]["api_key"] = serde_json::Value::String(
            if self.provider.api_key.is_empty() {
                "(unset)"
            } else {
                "(redacted)"
            }
            .into(),
        );
        let mut cursor = &value;
        for part in key.split('.') {
            cursor = cursor.get(part)?;
        }
        Some(match cursor {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }

    /// Write a dotted key. Numbers and booleans are parsed so `server.port=9000`
    /// stays an integer in the file.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<()> {
        let mut value = serde_json::to_value(&*self)?;
        let parts: Vec<&str> = key.split('.').collect();
        let mut cursor = &mut value;
        for part in &parts[..parts.len() - 1] {
            cursor = cursor
                .get_mut(*part)
                .ok_or_else(|| anyhow::anyhow!("unknown config section: {part}"))?;
        }
        let last = parts[parts.len() - 1];
        let slot = cursor
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("{key} is not a settable field"))?;
        if !slot.contains_key(last) {
            anyhow::bail!("unknown config key: {key}");
        }
        let parsed = if slot[last].is_string() || key == "agent.workspace" {
            serde_json::Value::String(raw.to_string())
        } else if raw == "null" {
            serde_json::Value::Null
        } else {
            parse_scalar(raw)
        };
        slot.insert(last.to_string(), parsed);
        let next: Self =
            serde_json::from_value(value).with_context(|| format!("invalid value for {key}"))?;
        next.validate()?;
        *self = next;
        Ok(())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        if let Ok(metadata) = std::fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.with_context(|| format!("writing {}", path.display()))
}

fn parse_scalar(raw: &str) -> serde_json::Value {
    if let Ok(b) = raw.parse::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(n) = raw.parse::<u64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(n) = raw.parse::<f64>() {
        return serde_json::Value::from(n);
    }
    serde_json::Value::String(raw.to_string())
}

fn validate_http_url(raw: &str, field: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw).with_context(|| format!("invalid {field}"))?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "{field} must be HTTP(S) without credentials"
    );
    Ok(())
}

/// Resolve a possibly-relative path against the workspace root, rejecting
/// escapes so a tool call cannot read outside the workspace.
pub fn resolve_in_workspace(root: &Path, candidate: &str) -> Result<PathBuf> {
    let joined = if Path::new(candidate).is_absolute() {
        PathBuf::from(candidate)
    } else {
        root.join(candidate)
    };
    let normalized = normalize(&joined);
    let root = normalize(root);
    if !normalized.starts_with(&root) {
        anyhow::bail!("path escapes the workspace: {candidate}");
    }
    Ok(normalized)
}

/// Lexical normalization: `..` and `.` are resolved without touching the disk,
/// so a path that does not exist yet (a file about to be written) still checks out.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_keeps_scalar_types() {
        let mut config = Config::default();
        config.set("server.port", "9100").unwrap();
        assert_eq!(config.server.port, 9100);
        config.set("model.default", "zai/glm-4.6").unwrap();
        assert_eq!(config.model.default, "zai/glm-4.6");
        assert!(config.set("model.nope", "x").is_err());
        config.provider.api_key = "secret".into();
        assert_eq!(
            config.get("provider.api_key").as_deref(),
            Some("(redacted)")
        );
    }

    #[test]
    fn models_require_an_active_provider() {
        let mut config = Config::default();
        assert!(!config.provider_active());
        assert!(!config.is_ready());

        config.provider.name = "fixture".into();
        config.provider.base_url = "http://127.0.0.1:8913".into();
        assert!(config.provider_active());
        assert!(!config.is_ready());

        config.model.default = "fixture/model".into();
        assert!(config.is_ready());
    }

    #[test]
    fn workspace_escape_is_rejected() {
        let root = PathBuf::from("/tmp/enx-root");
        assert!(resolve_in_workspace(&root, "src/main.rs").is_ok());
        assert!(resolve_in_workspace(&root, "../etc/passwd").is_err());
        assert!(resolve_in_workspace(&root, "/etc/passwd").is_err());
    }
}
