//! Small persistence helpers for the interactive toggles.
//!
//! Written to keep the modal actions out of `config.rs` (which is one giant
//! serializable struct) and to give the TUI a single place to call into.

use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    config::{home_dir, Config},
    discovery::{overrides_path, user_mcp_path, McpServer, McpTransport},
};

/// Toggle a skill in the persisted UI config. Returns the fresh disabled list
/// so the caller can update in-memory state without re-reading from disk.
pub fn toggle_skill(config: &mut Config, name: &str) -> Result<Vec<String>> {
    let name = name.to_ascii_lowercase();
    let list = &mut config.ui.disabled_skills;
    if let Some(pos) = list.iter().position(|n| n == &name) {
        list.remove(pos);
    } else {
        list.push(name);
    }
    config.save().context("saving config after skill toggle")?;
    Ok(config.ui.disabled_skills.clone())
}

/// Read the MCP override file. Missing file returns an empty map (not an
/// error) so first-run is silent.
pub fn read_overrides() -> BTreeMap<String, McpOverride> {
    let path = overrides_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Write the MCP override file atomically.
fn write_overrides(map: &BTreeMap<String, McpOverride>) -> Result<PathBuf> {
    let path = overrides_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let text = serde_json::to_string_pretty(map)?;
    atomic_write(&path, text.as_bytes())?;
    Ok(path)
}

/// Set the enabled flag for one MCP server. Persists to the overlay file so
/// the source config (which might be `~/.claude/mcp.json` or Codex's TOML) is
/// never mutated.
pub fn set_mcp_enabled(name: &str, enabled: bool) -> Result<()> {
    let mut map = read_overrides();
    map.entry(name.to_string())
        .and_modify(|e| e.enabled = Some(enabled))
        .or_insert(McpOverride {
            enabled: Some(enabled),
        });
    write_overrides(&map)?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Payload for the "Add MCP" popup. Kept loose (strings) so validation lives
/// close to the form rather than in the deserializer.
#[derive(Debug, Clone, Default)]
pub struct McpDraft {
    pub name: String,
    pub command: String,
    /// Comma-separated in the form; split on write.
    pub args: String,
    /// Newline-separated `KEY=VAL`.
    pub env: String,
    pub transport: McpTransport,
}

pub struct AddMcpError {
    pub message: String,
}

impl std::fmt::Display for AddMcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::fmt::Debug for AddMcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AddMcpError {}

/// Add or replace a user-scope MCP server in `~/.enx/mcp.json`.
///
/// * `existing` is the current discovery result so name collisions can be
///   caught before writing.
pub fn add_user_mcp(draft: &McpDraft, existing: &[McpServer]) -> Result<PathBuf, AddMcpError> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err(AddMcpError {
            message: "name is required".into(),
        });
    }
    let command = draft.command.trim();
    if command.is_empty() {
        return Err(AddMcpError {
            message: "command or URL is required".into(),
        });
    }
    if existing
        .iter()
        .any(|s| s.name.eq_ignore_ascii_case(name) && s.source != user_mcp_path())
    {
        return Err(AddMcpError {
            message: format!("`{name}` already exists from another source"),
        });
    }

    let path = user_mcp_path();
    let mut root: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({ "mcpServers": {} }));

    let map = root
        .as_object_mut()
        .and_then(|m| {
            m.entry("mcpServers".to_string())
                .or_insert(json!({}))
                .as_object_mut()
        })
        .ok_or_else(|| AddMcpError {
            message: format!("{} is not a JSON object with `mcpServers`", path.display()),
        })?;

    let args: Vec<String> = draft
        .args
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let mut env = serde_json::Map::new();
    for line in draft.env.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(AddMcpError {
                message: format!("env line must be KEY=VAL: `{line}`"),
            });
        };
        env.insert(k.trim().to_string(), Value::String(v.trim().to_string()));
    }

    let mut entry = serde_json::Map::new();
    match draft.transport {
        McpTransport::Stdio => {
            entry.insert("command".into(), Value::String(command.to_string()));
            if !args.is_empty() {
                entry.insert(
                    "args".into(),
                    Value::Array(args.into_iter().map(Value::String).collect()),
                );
            }
            if !env.is_empty() {
                entry.insert("env".into(), Value::Object(env));
            }
        }
        McpTransport::Http | McpTransport::Sse => {
            entry.insert("url".into(), Value::String(command.to_string()));
            entry.insert(
                "transport".into(),
                Value::String(
                    match draft.transport {
                        McpTransport::Http => "http",
                        McpTransport::Sse => "sse",
                        _ => unreachable!(),
                    }
                    .to_string(),
                ),
            );
        }
    }
    map.insert(name.to_string(), Value::Object(entry));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let text = serde_json::to_string_pretty(&root).map_err(|e| AddMcpError {
        message: format!("serialize mcp.json: {e}"),
    })?;
    atomic_write(&path, text.as_bytes()).map_err(|e| AddMcpError {
        message: format!("write {}: {e}", path.display()),
    })?;
    Ok(path)
}

/// Rewrite in place with a tmp-file + rename so a crash mid-write never leaves
/// a half-written config that a fresh process would parse as empty.
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

// Silence the unused import when built without the modal tests.
#[allow(dead_code)]
fn _keep_home_dir_referenced() -> PathBuf {
    home_dir()
}
