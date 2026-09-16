//! Discover MCP server declarations from the config files each major tool
//! uses. Project sources are consulted first so a project entry (including an
//! explicit `enabled: false`) claims the name before a user-level definition.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

use serde_json::Value;

use super::{user_home, walk_up, Discovery, McpServer, McpTransport, SkillScope};

pub fn collect(workspace: &Path, discovery: &mut Discovery) {
    let mut claimed: HashSet<String> = HashSet::new();

    for base in walk_up(workspace) {
        for suffix in [
            ".mcp.json",
            ".cursor/mcp.json",
            ".vscode/mcp.json",
            ".claude/mcp.json",
            ".gemini/settings.json",
        ] {
            read_json_map(
                &base.join(suffix),
                SkillScope::Project,
                discovery,
                &mut claimed,
            );
        }
    }

    if let Some(home) = user_home() {
        for path in [
            home.join(".claude/mcp.json"),
            home.join(".cursor/mcp.json"),
            home.join(".gemini/settings.json"),
        ] {
            read_json_map(&path, SkillScope::User, discovery, &mut claimed);
        }
        read_codex_toml(&home.join(".codex/config.toml"), discovery, &mut claimed);
    }

    // User's own additions, kept in a dedicated file so add-new never mutates
    // another tool's config.
    read_json_map(
        &super::user_mcp_path(),
        SkillScope::User,
        discovery,
        &mut claimed,
    );

    // Overlay disables/re-enables discovered servers without touching source.
    apply_overrides(discovery);
}

fn apply_overrides(discovery: &mut Discovery) {
    let path = super::overrides_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(value): Result<Value, _> = serde_json::from_str(&text) else {
        discovery.warnings.push(format!(
            "mcp overrides: {} is not valid JSON",
            path.display()
        ));
        return;
    };
    let Some(map) = value.as_object() else {
        return;
    };
    for server in &mut discovery.mcp_servers {
        if let Some(entry) = map.get(&server.name).and_then(Value::as_object) {
            if let Some(enabled) = entry.get("enabled").and_then(Value::as_bool) {
                server.enabled = enabled;
            }
        }
    }
}

fn read_json_map(
    path: &Path,
    scope: SkillScope,
    discovery: &mut Discovery,
    claimed: &mut HashSet<String>,
) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let Ok(value): Result<Value, _> = serde_json::from_str(&text) else {
        discovery
            .warnings
            .push(format!("invalid JSON in {}", path.display()));
        return;
    };
    // Every popular tool nests servers under `mcpServers`; some drop the
    // wrapper (bare map). Accept both instead of failing on layout drift.
    let map = value
        .get("mcpServers")
        .and_then(Value::as_object)
        .or_else(|| value.as_object());
    let Some(map) = map else { return };
    for (name, entry) in map {
        // A false `enabled` still claims the name so a user-level server with
        // the same name stays disabled, mirroring Codex/Claude Code behaviour.
        if !claimed.insert(name.clone()) {
            continue;
        }
        let enabled = entry
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let command = entry
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let url = entry.get("url").and_then(Value::as_str).map(str::to_owned);
        let args = entry
            .get("args")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let env = entry
            .get("env")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|value| (k.clone(), value.to_owned())))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let (command_or_url, transport) = match (command, url) {
            (Some(cmd), _) => (cmd, McpTransport::Stdio),
            (None, Some(url)) => {
                let transport = if url.contains("/sse") {
                    McpTransport::Sse
                } else {
                    McpTransport::Http
                };
                (url, transport)
            }
            _ => continue,
        };
        discovery.mcp_servers.push(McpServer {
            name: name.clone(),
            scope,
            command_or_url,
            args,
            env,
            transport,
            source: path.to_path_buf(),
            enabled,
        });
    }
}

/// Codex stores MCP entries in `~/.codex/config.toml` under `[mcp_servers.<name>]`.
/// We stay format-neutral by scanning line-by-line; parsing a full TOML tree is
/// overkill for a handful of scalar keys.
fn read_codex_toml(path: &Path, discovery: &mut Discovery, claimed: &mut HashSet<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let mut current: Option<String> = None;
    let mut command: Option<String> = None;
    let mut args: Vec<String> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut enabled = true;
    for raw in text.lines().chain(std::iter::once("[__end__]")) {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Some(name) = current.take() {
                if let Some(cmd) = command.take() {
                    if claimed.insert(name.clone()) {
                        discovery.mcp_servers.push(McpServer {
                            name,
                            scope: SkillScope::User,
                            command_or_url: cmd,
                            args: std::mem::take(&mut args),
                            env: std::mem::take(&mut env),
                            transport: McpTransport::Stdio,
                            source: path.to_path_buf(),
                            enabled,
                        });
                    }
                }
                enabled = true;
            }
            if let Some(name) = rest.strip_prefix("mcp_servers.") {
                current = Some(name.trim().trim_matches('"').to_owned());
            } else {
                current = None;
            }
            continue;
        }
        if current.is_none() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let raw_value = value.trim().trim_start_matches('"').trim_end_matches('"');
        match key {
            "command" => command = Some(raw_value.to_owned()),
            "enabled" => enabled = !matches!(raw_value, "false" | "0"),
            "args" => {
                if let Some(inner) = value
                    .trim()
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                {
                    args = inner
                        .split(',')
                        .map(|token| token.trim().trim_matches('"').to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            _ if key.starts_with("env.") => {
                env.insert(
                    key.trim_start_matches("env.").to_owned(),
                    raw_value.to_owned(),
                );
            }
            _ => {}
        }
    }
}
