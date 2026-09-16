use super::{display_path, number_arg, resolve_existing, string_arg, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

pub(super) struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "List workspace paths matching a glob pattern, respecting .gitignore."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "pattern":{"type":"string","description":"Glob such as src/**/*.rs"},
            "limit":{"type":"integer","minimum":1,"maximum":1000}
        },"required":["pattern"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let pattern = string_arg(&args, "pattern")?;
        let limit = number_arg(&args, "limit", 200, 1000);
        let matcher = globset::Glob::new(pattern)?.compile_matcher();
        let mut out = Vec::new();
        for entry in WalkBuilder::new(&ctx.workspace)
            .hidden(false)
            .git_ignore(true)
            .build()
            .flatten()
        {
            let path = entry.path();
            if path == ctx.workspace {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&ctx.workspace) else {
                continue;
            };
            if matcher.is_match(relative) {
                let mut shown = relative.to_string_lossy().into_owned();
                if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    shown.push('/');
                }
                out.push(shown);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out.sort();
        if out.is_empty() {
            return Ok(ToolOutput::ok("No matches."));
        }
        Ok(ToolOutput::ok(out.join("\n")))
    }
}

pub(super) struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search UTF-8 workspace files with a Rust regular expression. Returns file, line, and matching text."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "pattern":{"type":"string"},"path":{"type":"string","description":"File or directory, default workspace root"},
            "case_sensitive":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":1000}
        },"required":["pattern"],"additionalProperties":false})
    }
    async fn execute(&self, ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let pattern = string_arg(&args, "pattern")?;
        let case = args
            .get("case_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case)
            .build()?;
        let raw = args.get("path").and_then(Value::as_str).unwrap_or(".");
        let root = resolve_existing(&ctx.workspace, raw)?;
        let limit = number_arg(&args, "limit", 200, 1000);
        let mut hits = Vec::new();
        if root.is_file() {
            scan_file(&ctx.workspace, &root, &regex, limit, &mut hits);
        } else {
            for entry in WalkBuilder::new(root)
                .hidden(false)
                .git_ignore(true)
                .build()
                .flatten()
            {
                if entry.file_type().is_some_and(|kind| kind.is_file()) {
                    scan_file(&ctx.workspace, entry.path(), &regex, limit, &mut hits);
                }
                if hits.len() >= limit {
                    break;
                }
            }
        }
        if hits.is_empty() {
            return Ok(ToolOutput::ok("No matches."));
        }
        Ok(ToolOutput::ok(hits.join("\n")))
    }
}

fn scan_file(root: &Path, path: &Path, regex: &Regex, limit: usize, hits: &mut Vec<String>) {
    if hits.len() >= limit {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for (index, line) in text.lines().enumerate() {
        if regex.is_match(line) {
            hits.push(format!(
                "{}:{}:{}",
                display_path(root, path),
                index + 1,
                line
            ));
            if hits.len() >= limit {
                break;
            }
        }
    }
}
