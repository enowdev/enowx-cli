//! Atomic JSONL session snapshots, including tool calls and results for replay.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::sessions_dir;
use crate::message::{Message, Role as MessageRole};
use crate::role::Role;

/// One persisted transcript entry. Tool calls and results are kept so a resumed
/// session replays with the same context the model originally saw.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurn {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub workspace: PathBuf,
    pub turns: Vec<StoredTurn>,
}

impl Session {
    pub fn new(role: Role) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: String::new(),
            role,
            created_at: now,
            updated_at: now,
            workspace: PathBuf::new(),
            turns: Vec::new(),
        }
    }

    pub fn meta(&self) -> SessionMeta {
        SessionMeta {
            id: self.id.clone(),
            title: self.title.clone(),
            role: self.role,
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.turns.len(),
        }
    }

    pub fn push(&mut self, message: Message) {
        if self.title.is_empty() && message.role == MessageRole::User {
            self.title = derive_title(&message.content);
        }
        self.updated_at = Utc::now();
        self.turns.push(StoredTurn {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: self.updated_at,
            message,
        });
    }

    /// Repair calls left without results by a process exit. Never rerun a
    /// possibly completed side effect automatically when resuming.
    pub fn replay(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        for turn in &self.turns {
            let message = &turn.message;
            if message.interrupted {
                continue;
            }
            if message.role == MessageRole::Tool {
                if let Some(index) = pending
                    .iter()
                    .position(|id| Some(id) == message.tool_call_id.as_ref())
                {
                    pending.remove(index);
                    messages.push(message.clone());
                }
                continue;
            }
            for id in pending.drain(..) {
                messages.push(Message::tool_result(
                    id,
                    "Result unavailable after interruption; inspect state before retrying.",
                ));
            }
            pending.extend(message.tool_calls.iter().map(|call| call.id.clone()));
            messages.push(message.clone());
        }
        for id in pending {
            messages.push(Message::tool_result(
                id,
                "Result unavailable after interruption; inspect state before retrying.",
            ));
        }
        messages
    }
}

/// First line of the prompt, trimmed to something a sidebar can show.
fn derive_title(prompt: &str) -> String {
    let first = prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut title: String = first.chars().take(60).collect();
    if first.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        "New conversation".to_string()
    } else {
        title
    }
}

/// JSONL-backed store. The header line holds metadata; every later line is a turn.
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Header {
    id: String,
    title: String,
    role: Role,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    workspace: PathBuf,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(sessions_dir())
    }
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        uuid::Uuid::parse_str(id).context("invalid session id")?;
        Ok(self.root.join(format!("{id}.jsonl")))
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let mut out = String::new();
        let header = Header {
            id: session.id.clone(),
            title: session.title.clone(),
            role: session.role,
            created_at: session.created_at,
            updated_at: session.updated_at,
            workspace: session.workspace.clone(),
        };
        out.push_str(&serde_json::to_string(&header)?);
        out.push('\n');
        for turn in &session.turns {
            out.push_str(&serde_json::to_string(turn)?);
            out.push('\n');
        }
        let path = self.path_for(&session.id)?;
        crate::config::atomic_write(&path, out.as_bytes())?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.path_for(id)?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut lines = text.lines();
        let header: Header = lines
            .next()
            .map(serde_json::from_str)
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("session {id} is empty"))?;
        let mut turns = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            turns.push(serde_json::from_str(line)?);
        }
        Ok(Session {
            id: header.id,
            title: header.title,
            role: header.role,
            created_at: header.created_at,
            updated_at: header.updated_at,
            workspace: header.workspace,
            turns,
        })
    }

    /// Newest first; malformed files are reported instead of silently hidden.
    pub fn list(&self, limit: usize) -> Result<Vec<SessionMeta>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(err) => return Err(err).context("listing sessions"),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let mut lines = text.lines();
            let header: Header = serde_json::from_str(lines.next().unwrap_or(""))
                .with_context(|| format!("parsing {}", path.display()))?;
            let message_count = lines.filter(|line| !line.trim().is_empty()).count();
            out.push(SessionMeta {
                id: header.id,
                title: header.title,
                role: header.role,
                created_at: header.created_at,
                updated_at: header.updated_at,
                message_count,
            });
        }
        out.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        out.truncate(limit);
        Ok(out)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path_for(id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("removing {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (SessionStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("enx-sessions-{}", uuid::Uuid::new_v4()));
        (SessionStore::new(dir.clone()), dir)
    }

    #[test]
    fn round_trip_preserves_tool_calls_and_title() {
        let (store, dir) = temp_store();
        let mut session = Session::new(Role::Researcher);
        session.push(Message::user("Where is the config parsed?\nsecond line"));
        session.push(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: None,
            tool_calls: vec![crate::message::ToolCall {
                id: "call_1".into(),
                name: "grep".into(),
                arguments: "{}".into(),
            }],
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        });
        session.push(Message::tool_result("call_1", "hit"));
        store.save(&session).unwrap();

        let loaded = store.load(&session.id).unwrap();
        assert_eq!(loaded.role, Role::Researcher);
        assert_eq!(loaded.title, "Where is the config parsed?");
        assert_eq!(loaded.turns.len(), 3);
        assert_eq!(loaded.turns[1].message.tool_calls[0].name, "grep");
        assert_eq!(
            loaded.turns[2].message.tool_call_id.as_deref(),
            Some("call_1")
        );

        let listed = store.list(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].message_count, 3);

        store.delete(&session.id).unwrap();
        assert!(store.list(10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A crash between a tool call and its result leaves the transcript unpaired.
    /// Replay must synthesize the missing result rather than send an assistant
    /// tool call the provider will reject, and never silently rerun the tool.
    #[test]
    fn replay_pairs_calls_left_without_results() {
        let mut session = Session::new(Role::Orchestrator);
        session.push(Message::user("write the file"));
        session.push(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: None,
            tool_calls: vec![crate::message::ToolCall {
                id: "call_1".into(),
                name: "write".into(),
                arguments: "{}".into(),
            }],
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        });
        session.push(Message {
            role: MessageRole::Assistant,
            content: "partial".into(),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: true,
            error: Some("Interrupted by user.".into()),
            model: None,
            message_id: None,
        });

        let replay = session.replay();
        // The interrupted partial reply stays out of provider context.
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[2].role, MessageRole::Tool);
        assert_eq!(replay[2].tool_call_id.as_deref(), Some("call_1"));
        assert!(replay[2].content.contains("Result unavailable"));
        // The partial text is still visible in stored history for the user.
        assert_eq!(session.turns.len(), 3);
    }
}
