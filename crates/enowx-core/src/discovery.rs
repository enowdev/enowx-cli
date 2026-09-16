//! Discover skills, agent instructions, and MCP server configs from the
//! standard on-disk layouts used by Claude Code, Codex, Cursor, Gemini,
//! OpenCode, and the shared `.agents/` convention.
//!
//! Project sources win over user-level sources; within each scope, the first
//! entry for a given name wins so a project override cleanly shadows a global.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub mod instructions;
pub mod mcp;
pub mod skills;

/// Byte caps that keep discovered material from overwhelming the context.
pub const MAX_INSTRUCTION_BYTES: usize = 8 * 1024;
pub const MAX_INSTRUCTION_TOTAL_BYTES: usize = 32 * 1024;
pub const MAX_SKILL_ENTRIES: usize = 200;
pub const MAX_AT_IMPORT_DEPTH: u8 = 5;

/// Per-user overlay file toggling MCP servers on or off without touching the
/// tool-owned source files (`.mcp.json`, `~/.codex/config.toml`, etc.).
pub fn overrides_path() -> std::path::PathBuf {
    crate::config::home_dir().join("mcp-overrides.json")
}

/// User-added MCP servers live in a dedicated file so add-new never mutates a
/// project or another tool's config without consent.
pub fn user_mcp_path() -> std::path::PathBuf {
    crate::config::home_dir().join("mcp.json")
}
/// Everywhere skills may live. Order encodes precedence: project first, then
/// user home. Directory candidates within each scope are checked in listed order.
#[derive(Debug, Clone, Copy)]
pub enum Scope {
    Project,
    User,
}

fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Walk up from `start` toward the filesystem root, yielding every candidate
/// directory. Stops at a directory containing `.git` or when the root is hit
/// so a nested workspace does not read the parent repo's configuration.
fn walk_up(start: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut current = Some(start.to_path_buf());
    let mut seen = HashSet::new();
    while let Some(dir) = current {
        if !seen.insert(dir.clone()) {
            break;
        }
        out.push(dir.clone());
        if dir.join(".git").exists() {
            break;
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    out
}

/// Read a file, capped to `MAX_INSTRUCTION_BYTES` so an accidental huge file
/// cannot break the context window budget.
fn read_capped(path: &Path) -> Option<(String, bool)> {
    let bytes = fs::read(path).ok()?;
    let truncated = bytes.len() > MAX_INSTRUCTION_BYTES;
    let slice = if truncated {
        &bytes[..MAX_INSTRUCTION_BYTES]
    } else {
        &bytes[..]
    };
    let text = String::from_utf8_lossy(slice).into_owned();
    Some((text, truncated))
}

/// One discovered skill entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    /// Absolute path to `SKILL.md`.
    pub path: PathBuf,
    /// The scope this skill was found in.
    pub scope: SkillScope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    Project,
    User,
}

/// One agent-instruction file. Body is already trimmed to `MAX_INSTRUCTION_BYTES`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionFile {
    pub path: PathBuf,
    pub scope: SkillScope,
    pub body: String,
    /// Number of `@import` files pulled into `body`.
    pub imports: usize,
    /// Whether the on-disk source was longer than the per-file cap.
    pub truncated: bool,
}

/// One MCP server declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub scope: SkillScope,
    /// Stdio: absolute or PATH-resolved executable. HTTP: server URL.
    pub command_or_url: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub transport: McpTransport,
    /// Origin config file so telemetry can point back at it.
    pub source: PathBuf,
    /// Explicitly disabled entries are kept: they still "claim" the name so a
    /// user-level server with the same name stays disabled, mirroring how
    /// Claude Code and Codex resolve the same situation.
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
    Sse,
}

/// Aggregate discovery result, ready to feed the interface and the agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Discovery {
    pub skills: Vec<SkillEntry>,
    /// Skills dropped because a higher-precedence entry took the name. Reported
    /// so the sidebar can surface conflicts without silently discarding files.
    pub shadowed_skills: Vec<(String, PathBuf)>,
    pub instructions: Vec<InstructionFile>,
    pub mcp_servers: Vec<McpServer>,
    /// Warnings collected during parsing (malformed frontmatter, unreadable
    /// files, etc.). Surfaced in the sidebar; never fatal.
    pub warnings: Vec<String>,
}

impl Discovery {
    pub fn run(workspace: &Path) -> Self {
        let mut result = Discovery::default();
        skills::collect(workspace, &mut result);
        instructions::collect(workspace, &mut result);
        mcp::collect(workspace, &mut result);
        result
    }

    /// System-prompt block combining every collected instruction, bounded by
    /// the aggregate cap. Skills are advertised as an inventory only so the
    /// model can request specific ones via the `skill_read` tool.
    pub fn system_prompt_supplement(&self) -> Option<String> {
        self.system_prompt_with_disabled(&[])
    }

    /// System-prompt block that skips any skill whose name is in `disabled`.
    pub fn system_prompt_with_disabled(&self, disabled: &[String]) -> Option<String> {
        let active_skills: Vec<&SkillEntry> = self
            .skills
            .iter()
            .filter(|s| !disabled.iter().any(|d| d == &s.name))
            .collect();
        if self.instructions.is_empty() && active_skills.is_empty() {
            return None;
        }
        let mut out = String::new();
        if !self.instructions.is_empty() {
            let mut used = 0usize;
            for file in &self.instructions {
                let header = format!(
                    "\n## {} ({})\n",
                    file.path.display(),
                    match file.scope {
                        SkillScope::Project => "project",
                        SkillScope::User => "user",
                    }
                );
                let cost = header.len() + file.body.len() + 1;
                if used + cost > MAX_INSTRUCTION_TOTAL_BYTES {
                    out.push_str("\n[additional instruction files omitted for context budget]\n");
                    break;
                }
                out.push_str(&header);
                out.push_str(&file.body);
                if file.truncated {
                    out.push_str("\n[truncated]\n");
                }
                used += cost;
            }
        }
        if !active_skills.is_empty() {
            out.push_str("\n## Available skills\n");
            out.push_str(
                "Read a skill on demand with the `skill_read` tool before doing work it covers.\n",
            );
            for skill in &active_skills {
                let one_liner = skill.description.split('\n').next().unwrap_or("");
                out.push_str(&format!("- `{}` — {}\n", skill.name, one_liner));
            }
        }
        Some(out)
    }
}

/// Parse the leading YAML-style frontmatter block used by every Skills format.
/// We do not need a full YAML parser: the fields we consume are simple strings,
/// possibly quoted, and unknown keys are ignored. Returns `(map, body)` where
/// `body` has the frontmatter stripped.
pub(crate) fn parse_frontmatter(source: &str) -> (BTreeMap<String, String>, String) {
    let mut map = BTreeMap::new();
    let trimmed = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Some(rest) = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
    else {
        return (map, source.to_owned());
    };
    let Some(end) = rest.find("\n---") else {
        return (map, source.to_owned());
    };
    let header = &rest[..end];
    for line in header.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        if !key.is_empty() && !value.is_empty() {
            map.insert(key, value);
        }
    }
    let body_start = end + "\n---".len();
    let body = rest[body_start..]
        .trim_start_matches('\n')
        .trim_start_matches('\r')
        .to_owned();
    (map, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_reads_name_description_and_allowed_tools() {
        let source = "---\nname: demo\ndescription: \"quoted line\"\nallowed-tools: Read Write\n---\n\n# body\n";
        let (map, body) = parse_frontmatter(source);
        assert_eq!(map["name"], "demo");
        assert_eq!(map["description"], "quoted line");
        assert_eq!(map["allowed-tools"], "Read Write");
        assert_eq!(body, "# body\n");
    }

    #[test]
    fn missing_frontmatter_returns_original_source() {
        let source = "# heading\ncontent\n";
        let (map, body) = parse_frontmatter(source);
        assert!(map.is_empty());
        assert_eq!(body, source);
    }
}
