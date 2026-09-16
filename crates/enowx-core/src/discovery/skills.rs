//! Discover `SKILL.md` files across every known layout and dedup them by name.

use std::{collections::HashSet, fs, path::Path};

use super::{
    parse_frontmatter, user_home, walk_up, Discovery, SkillEntry, SkillScope, MAX_SKILL_ENTRIES,
};

/// Directories that hold a `<skill-name>/SKILL.md` layout. Applies to both
/// project walk-up and user home candidates.
const SKILL_DIRS: [&str; 8] = [
    ".enx/skills",
    ".agent/skills",
    ".agents/skills",
    ".claude/skills",
    ".codex/skills",
    ".cursor/skills",
    ".gemini/skills",
    ".opencode/skills",
];

pub fn collect(workspace: &Path, discovery: &mut Discovery) {
    let mut seen: HashSet<String> = HashSet::new();

    for base in walk_up(workspace) {
        // A .git boundary already stopped `walk_up`; skip walking further out
        // through common junk directories like target/ and node_modules/.
        if base.join("target").exists()
            && discovery
                .skills
                .iter()
                .any(|s| matches!(s.scope, SkillScope::Project))
        {
            continue;
        }
        for suffix in SKILL_DIRS {
            scan_root(
                &base.join(suffix),
                SkillScope::Project,
                discovery,
                &mut seen,
            );
        }
        // Also accept a top-level `skills/` directly under the workspace root.
        if base == workspace {
            scan_root(
                &base.join("skills"),
                SkillScope::Project,
                discovery,
                &mut seen,
            );
        }
    }

    if let Some(home) = user_home() {
        for suffix in SKILL_DIRS {
            scan_root(&home.join(suffix), SkillScope::User, discovery, &mut seen);
        }
    }
}

fn scan_root(
    root: &Path,
    scope: SkillScope,
    discovery: &mut Discovery,
    seen: &mut HashSet<String>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if discovery.skills.len() >= MAX_SKILL_ENTRIES {
            discovery.warnings.push(format!(
                "skill limit reached ({MAX_SKILL_ENTRIES}); further entries skipped"
            ));
            return;
        }
        let path = entry.path();
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let Ok(source) = fs::read_to_string(&skill_file) else {
            discovery
                .warnings
                .push(format!("unreadable skill: {}", skill_file.display()));
            continue;
        };
        let (front, body) = parse_frontmatter(&source);
        let default_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = front
            .get("name")
            .cloned()
            .unwrap_or(default_name)
            .to_lowercase();
        if name.is_empty() {
            discovery
                .warnings
                .push(format!("skill without a name: {}", skill_file.display()));
            continue;
        }
        // The first entry for a name wins. `seen` is populated in scope order
        // (project first, then user), so a project skill legitimately shadows a
        // same-named user skill; the shadowed copy is surfaced separately.
        if !seen.insert(name.clone()) {
            discovery.shadowed_skills.push((name, skill_file));
            continue;
        }
        let description = front.get("description").cloned().unwrap_or_else(|| {
            body.lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_owned()
        });
        let allowed_tools = front
            .get("allowed-tools")
            .map(|value| {
                value
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_owned())
                    .collect()
            })
            .unwrap_or_default();
        discovery.skills.push(SkillEntry {
            name,
            description,
            allowed_tools,
            path: skill_file,
            scope,
        });
    }
}
