//! Discover project and global agent-instruction files (AGENTS.md, CLAUDE.md,
//! GEMINI.md, .cursorrules, .github/copilot-instructions.md) and expand `@path`
//! imports up to the documented Claude Code depth.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::{
    read_capped, user_home, walk_up, Discovery, InstructionFile, SkillScope, MAX_AT_IMPORT_DEPTH,
};

/// Files searched in project walk-up (relative to each candidate directory).
const PROJECT_FILES: [&str; 5] = [
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".cursorrules",
    ".github/copilot-instructions.md",
];

pub fn collect(workspace: &Path, discovery: &mut Discovery) {
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut seen_hashes: HashSet<u64> = HashSet::new();

    for base in walk_up(workspace) {
        for name in PROJECT_FILES {
            push(
                &base.join(name),
                SkillScope::Project,
                discovery,
                &mut seen_paths,
                &mut seen_hashes,
            );
        }
    }

    if let Some(home) = user_home() {
        for candidate in [
            home.join(".agents/AGENTS.md"),
            home.join(".agent/AGENTS.md"),
            home.join(".claude/CLAUDE.md"),
            home.join(".codex/AGENTS.md"),
            home.join(".gemini/GEMINI.md"),
            home.join(".config/enx/AGENTS.md"),
        ] {
            push(
                &candidate,
                SkillScope::User,
                discovery,
                &mut seen_paths,
                &mut seen_hashes,
            );
        }
    }
}

fn push(
    path: &Path,
    scope: SkillScope,
    discovery: &mut Discovery,
    seen_paths: &mut HashSet<PathBuf>,
    seen_hashes: &mut HashSet<u64>,
) {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen_paths.insert(canonical.clone()) {
        return;
    }
    let Some((body, truncated)) = read_capped(path) else {
        return;
    };
    let mut visited = HashSet::new();
    visited.insert(canonical.clone());
    let mut imports = 0;
    let expanded = expand_at_imports(
        &body,
        path.parent().unwrap_or(Path::new(".")),
        &mut visited,
        &mut imports,
        0,
    );
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    expanded.hash(&mut fingerprint);
    let digest = fingerprint.finish();
    if !seen_hashes.insert(digest) {
        // Global and project copies with identical text (same tool installed both
        // places) collapse to one entry so the model does not read them twice.
        return;
    }
    discovery.instructions.push(InstructionFile {
        path: canonical,
        scope,
        body: expanded,
        imports,
        truncated,
    });
}

/// Expand `@relative/or/absolute` file references, capped at MAX_AT_IMPORT_DEPTH.
fn expand_at_imports(
    source: &str,
    base: &Path,
    visited: &mut HashSet<PathBuf>,
    imports: &mut usize,
    depth: u8,
) -> String {
    if depth >= MAX_AT_IMPORT_DEPTH {
        return source.to_owned();
    }
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        // Only bare `@path` tokens on their own line are treated as imports;
        // email addresses and inline handles are left untouched.
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            if !rest.is_empty() && !rest.contains(' ') {
                let candidate = if rest.starts_with('~') {
                    if let Some(home) = user_home() {
                        home.join(rest.trim_start_matches("~/"))
                    } else {
                        PathBuf::from(rest)
                    }
                } else if Path::new(rest).is_absolute() {
                    PathBuf::from(rest)
                } else {
                    base.join(rest)
                };
                let canonical = fs::canonicalize(&candidate).unwrap_or(candidate);
                if visited.insert(canonical.clone()) {
                    if let Some((body, _)) = read_capped(&canonical) {
                        *imports += 1;
                        let nested_base =
                            canonical.parent().unwrap_or(Path::new(".")).to_path_buf();
                        let expanded =
                            expand_at_imports(&body, &nested_base, visited, imports, depth + 1);
                        out.push_str(&format!("\n<!-- imported {} -->\n", canonical.display()));
                        out.push_str(&expanded);
                        out.push('\n');
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
