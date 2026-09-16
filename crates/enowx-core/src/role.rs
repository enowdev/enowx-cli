//! The three roles this build ships. Each is a system prompt plus the tool
//! surface it is allowed to touch, so a researcher cannot write files and a
//! writer cannot run shell commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[default]
    Orchestrator,
    Writer,
    Researcher,
}

/// Every shipped role, in the order the UI lists them.
pub const ROLES: [Role; 3] = [Role::Orchestrator, Role::Writer, Role::Researcher];

impl Role {
    pub fn id(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Writer => "writer",
            Self::Researcher => "researcher",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Orchestrator => "Orchestrator",
            Self::Writer => "Writer",
            Self::Researcher => "Researcher",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Orchestrator => "Plans the work, reads and edits the workspace, runs commands.",
            Self::Writer => "Turns findings into prose and writes files. No shell access.",
            Self::Researcher => "Reads the workspace and the web. Never modifies anything.",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "orchestrator" => Some(Self::Orchestrator),
            "writer" => Some(Self::Writer),
            "researcher" => Some(Self::Researcher),
            _ => None,
        }
    }

    /// Tools this role may call. A call to anything else is refused by the loop
    /// with an explanation the model can act on.
    pub fn allowed_tools(self) -> &'static [&'static str] {
        match self {
            Self::Orchestrator => &[
                "read", "write", "edit", "glob", "grep", "bash", "fetch", "todo",
            ],
            Self::Writer => &["read", "write", "edit", "glob", "grep", "todo"],
            Self::Researcher => &["read", "glob", "grep", "fetch", "todo"],
        }
    }

    pub fn system_prompt(self, workspace: &str) -> String {
        let shared = format!(
            "You are a tool-using engineering agent. Workspace root: {workspace}\n\
             \n\
             Rules that hold for every role:\n\
             - Use tools to establish facts. Never claim you read a file, ran a command, or fetched a page unless the tool result is in this conversation.\n\
             - Paths are relative to the workspace root. Reads and writes outside it are refused.\n\
             - Prefer one precise tool call over several speculative ones. Stop calling tools once you can answer.\n\
             - When a tool fails, read the error and change approach instead of repeating the same call.\n\
             - Answer in the user's language. Be concrete: exact paths, symbols, and commands.\n"
        );

        let specific = match self {
            Self::Orchestrator => {
                "Your role: Orchestrator.\n\
                 You own the whole task end to end: understand the request, inspect the workspace, make the change, and verify it.\n\
                 - Start from evidence: locate the relevant files with glob/grep and read the sections that matter, not whole files.\n\
                 - Edit surgically with `edit`; use `write` only for new files or a full rewrite.\n\
                 - Verify with `bash`: run the build, the specific test, or the command that exercises the change. Report the actual result.\n\
                 - Track multi-step work with `todo` so the user can see the plan and what is left.\n\
                 - Never delegate to another agent. There is none. Finish the work yourself."
            }
            Self::Writer => {
                "Your role: Writer.\n\
                 You produce prose and documentation: reports, READMEs, changelogs, comments, release notes.\n\
                 - Read the code or notes you are writing about before describing them. Do not invent behavior.\n\
                 - Write to files with `write`/`edit` when the user asks for a document; otherwise answer in the chat.\n\
                 - Structure follows the content: no filler sections, no marketing language, no invented statistics.\n\
                 - You have no shell access. If a claim needs a command run to verify it, say so instead of guessing."
            }
            Self::Researcher => {
                "Your role: Researcher.\n\
                 You investigate and report. You are read-only: you have no write, edit, or shell tools.\n\
                 - Map the ground first with `glob`/`grep`, then read the specific ranges that answer the question.\n\
                 - Use `fetch` for documentation, specs, and upstream sources; quote the URL you actually fetched.\n\
                 - Separate what you verified from what you inferred. Mark inferences plainly.\n\
                 - Deliver findings with file paths and line numbers so the next step is actionable."
            }
        };

        format!("{shared}\n{specific}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn researcher_cannot_reach_mutating_tools() {
        let allowed = Role::Researcher.allowed_tools();
        assert!(allowed.contains(&"read"));
        assert!(!allowed.contains(&"write"));
        assert!(!allowed.contains(&"edit"));
        assert!(!allowed.contains(&"bash"));
    }

    #[test]
    fn writer_has_no_shell() {
        assert!(!Role::Writer.allowed_tools().contains(&"bash"));
        assert!(Role::Writer.allowed_tools().contains(&"write"));
    }

    #[test]
    fn only_three_roles_ship() {
        let ids: Vec<&str> = ROLES.iter().map(|r| r.id()).collect();
        assert_eq!(ids, vec!["orchestrator", "writer", "researcher"]);
        assert_eq!(Role::parse("scout"), None);
    }
}
