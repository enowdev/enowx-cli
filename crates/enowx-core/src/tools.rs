//! Built-in tools rooted in the configured workspace.
use crate::{config::resolve_in_workspace, role::Role};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
mod fetch;
mod files;
pub mod mcp_proxy;
mod search;
mod shell;
pub mod skill;
mod todo;
use fetch::FetchTool;
use files::{EditTool, ReadTool, WriteTool};
use search::{GlobTool, GrepTool};
use shell::BashTool;
use todo::TodoTool;

use crate::discovery::Discovery;

#[derive(Clone)]
pub struct ToolCtx {
    pub workspace: PathBuf,
    pub shell_timeout: Duration,
    pub cancel: CancellationToken,
    /// Optional channel a tool can push progress deltas to. When set, the
    /// runtime forwards each delta as a `ToolProgress` event so the UI shows
    /// the tool's output as it runs.
    pub progress: Option<tokio::sync::mpsc::Sender<(String, String)>>,
    /// The current tool call id, so progress deltas can be tagged without
    /// threading it through every helper.
    pub call_id: String,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput>;

    fn wire_schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters(),
            }
        })
    }
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register(ReadTool);
        registry.register(WriteTool);
        registry.register(EditTool);
        registry.register(GlobTool);
        registry.register(GrepTool);
        registry.register(BashTool);
        registry.register(FetchTool::default());
        registry.register(TodoTool::default());
        registry
    }
}

impl ToolRegistry {
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_owned(), Arc::new(tool));
    }

    /// Iterator over `(qualified_name, description)` for every registered tool.
    /// Used by the TUI to render the MCP proxy inventory without exposing the
    /// full `Arc<dyn Tool>` map.
    pub fn keys(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.tools
            .values()
            .map(|t| (t.name().to_string(), t.description().to_string()))
    }

    pub fn schemas(&self, role: Role, discovery: Option<&Discovery>) -> Vec<Value> {
        let mut allowed: Vec<String> = role
            .allowed_tools()
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        // Discovered MCP tools are namespaced (`mcp__…`) and every role may
        // invoke them; the built-in role filter still gates the local tools.
        allowed.extend(
            self.tools
                .keys()
                .filter(|name| name.starts_with("mcp__"))
                .cloned(),
        );
        if discovery.is_some_and(|d| !d.skills.is_empty()) {
            allowed.push("skill_read".to_owned());
        }
        allowed
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|tool| tool.wire_schema())
            .collect()
    }

    pub async fn execute(&self, role: Role, ctx: &ToolCtx, name: &str, args: Value) -> ToolOutput {
        // Every role may reach discovered MCP tools and the on-demand skill
        // reader; the built-in tool set stays gated by the role's allowlist.
        let is_extension = name == "skill_read" || name.starts_with("mcp__");
        if !is_extension && !role.allowed_tools().contains(&name) {
            return ToolOutput::error(format!(
                "tool `{name}` is not available to the {} role",
                role.label()
            ));
        }
        let Some(tool) = self.tools.get(name) else {
            return ToolOutput::error(format!("unknown tool `{name}`"));
        };
        let schema = tool.parameters();
        if !args.is_object() {
            return ToolOutput::error("arguments must be an object");
        }
        for key in schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if args.get(key).is_none() {
                return ToolOutput::error(format!("missing required argument: {key}"));
            }
        }
        for (key, value) in args.as_object().expect("object checked") {
            let Some(property) = schema["properties"].get(key) else {
                return ToolOutput::error(format!("unknown argument: {key}"));
            };
            let valid = match property["type"].as_str() {
                Some("string") => value.is_string(),
                Some("integer") => value.is_u64(),
                Some("boolean") => value.is_boolean(),
                Some("array") => value
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string)),
                _ => true,
            };
            if !valid {
                return ToolOutput::error(format!("invalid type for argument: {key}"));
            }
            if let Some(choices) = property["enum"].as_array() {
                if !choices.contains(value) {
                    return ToolOutput::error(format!("invalid value for argument: {key}"));
                }
            }
            if let Some(n) = value.as_u64() {
                if property["minimum"].as_u64().is_some_and(|min| n < min)
                    || property["maximum"].as_u64().is_some_and(|max| n > max)
                {
                    return ToolOutput::error(format!("argument out of range: {key}"));
                }
            }
        }
        match tool.execute(ctx, args).await {
            Ok(output) => output,
            Err(error) => ToolOutput::error(format!("{error:#}")),
        }
    }
}

fn string_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string argument `{key}`"))
}

fn number_arg(args: &Value, key: &str, default: usize, max: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
        .min(max)
}
fn truncate_output(text: &mut String, limit: usize) {
    if text.len() > limit {
        let mut end = limit;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n[output truncated]");
    }
}
fn resolve_existing(root: &Path, raw: &str) -> Result<PathBuf> {
    let path = resolve_in_workspace(root, raw)?;
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolving workspace {}", root.display()))?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving {raw}"))?;
    if !canonical.starts_with(&canonical_root) {
        anyhow::bail!("path escapes the workspace through a symlink: {raw}");
    }
    Ok(canonical)
}

fn resolve_for_write(root: &Path, raw: &str) -> Result<PathBuf> {
    let path = resolve_in_workspace(root, raw)?;
    if std::fs::symlink_metadata(&path).is_ok() {
        return resolve_existing(root, raw);
    }
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolving workspace {}", root.display()))?;
    let mut parent = path.parent().unwrap_or(root);
    while !parent.exists() {
        parent = parent
            .parent()
            .ok_or_else(|| anyhow::anyhow!("cannot resolve parent for {raw}"))?;
    }
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_root) {
        anyhow::bail!("path escapes the workspace through a symlink: {raw}");
    }
    Ok(path)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ctx() -> (ToolCtx, PathBuf) {
        let root = std::env::temp_dir().join(format!("enx-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        (
            ToolCtx {
                workspace: root.clone(),
                shell_timeout: Duration::from_secs(5),
                cancel: CancellationToken::new(),
                progress: None,
                call_id: String::new(),
            },
            root,
        )
    }

    #[tokio::test]
    async fn write_edit_read_observe_real_file() {
        let (ctx, root) = temp_ctx();
        let registry = ToolRegistry::default();
        let written = registry
            .execute(
                Role::Orchestrator,
                &ctx,
                "write",
                json!({"path":"src/a.txt","content":"one\ntwo\n"}),
            )
            .await;
        assert!(!written.is_error);
        let edited = registry
            .execute(
                Role::Orchestrator,
                &ctx,
                "edit",
                json!({"path":"src/a.txt","old_text":"two","new_text":"second"}),
            )
            .await;
        assert!(!edited.is_error);
        let read = registry
            .execute(
                Role::Orchestrator,
                &ctx,
                "read",
                json!({"path":"src/a.txt"}),
            )
            .await;
        assert_eq!(read.content, "1:one\n2:second");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn role_boundary_blocks_mutation() {
        let (ctx, root) = temp_ctx();
        let registry = ToolRegistry::default();
        let output = registry
            .execute(
                Role::Researcher,
                &ctx,
                "write",
                json!({"path":"a","content":"x"}),
            )
            .await;
        assert!(output.is_error);
        assert!(!root.join("a").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn edit_refuses_ambiguous_match() {
        let (ctx, root) = temp_ctx();
        std::fs::write(root.join("a.txt"), "same same").unwrap();
        let output = ToolRegistry::default()
            .execute(
                Role::Writer,
                &ctx,
                "edit",
                json!({"path":"a.txt","old_text":"same","new_text":"x"}),
            )
            .await;
        assert!(output.is_error);
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "same same"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
