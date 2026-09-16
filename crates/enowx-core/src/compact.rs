//! Session compaction: fold older turns into a single summary assistant
//! message so a long conversation stays under the model's context window.
//!
//! Strategy:
//! - Keep the last `keep_last` turns verbatim (default 4).
//! - Ask the model to summarize the rest into a concise state (goal,
//!   decisions, files touched, key results). One extra API call per compact.
//! - Replace the old turns with one synthetic assistant turn carrying the
//!   summary. Session file on disk is preserved (append-only history stays).

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    message::{Message, Role as MessageRole},
    provider::{Chunk, Provider},
    session::{Session, StoredTurn},
};

const SUMMARY_PROMPT: &str = "\
You are compacting a coding-agent conversation to keep it under the model's \
context window. Produce a compact state note the agent can read on the next \
turn to keep working. Include ONLY:\n\
- Goal: what the user is trying to build.\n\
- Decisions: choices already made (libraries, patterns, file paths).\n\
- Files: which files were created, edited, or read, with a one-line purpose.\n\
- Open: what is not done yet or is blocked.\n\
Do NOT restate every tool call, do NOT include large code blocks, and do NOT \
apologize for summarizing. Write in tight, factual bullets.\n";

/// Perform a compact on `session` using `provider`. Returns the summary text
/// so callers can log or display it. Does nothing when the session has fewer
/// than `keep_last + 2` turns (nothing meaningful to fold).
pub async fn compact(
    session: &mut Session,
    provider: &Provider,
    keep_last: usize,
) -> Result<Option<String>> {
    if session.turns.len() < keep_last + 2 {
        return Ok(None);
    }
    let split = session.turns.len() - keep_last;
    let (older, keep): (Vec<StoredTurn>, Vec<StoredTurn>) = {
        let mut turns = std::mem::take(&mut session.turns);
        let keep = turns.split_off(split);
        (turns, keep)
    };

    // Build the summarization prompt from the older turns, stripped of tool
    // internals so the summarizer sees plain user/assistant prose.
    let transcript: String = older
        .iter()
        .filter_map(|t| match t.message.role {
            MessageRole::User => Some(format!("USER: {}\n", t.message.content.trim())),
            MessageRole::Assistant if !t.message.content.trim().is_empty() => {
                Some(format!("ASSISTANT: {}\n", t.message.content.trim()))
            }
            _ => None,
        })
        .collect();

    if transcript.trim().is_empty() {
        // Nothing to summarize (turns were all tool round-trips). Restore and
        // bail out cleanly.
        session.turns = older;
        session.turns.extend(keep);
        return Ok(None);
    }

    let messages = vec![Message::system(SUMMARY_PROMPT), Message::user(transcript)];
    let (tx, mut rx) = mpsc::channel::<Chunk>(64);
    // Drain the sink so the streaming client does not block on backpressure;
    // we do not surface partial chunks to the UI during compact.
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let completion = provider.complete(&messages, &[] as &[Value], &tx).await?;
    let summary = completion.text.trim().to_string();
    if summary.is_empty() {
        session.turns = older;
        session.turns.extend(keep);
        return Ok(None);
    }

    // Replace older turns with one synthetic assistant turn carrying the
    // summary. Keep the trailing `keep_last` turns as-is so the agent still
    // sees recent context verbatim.
    let now = Utc::now();
    let summary_turn = StoredTurn {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: now,
        message: Message {
            role: MessageRole::Assistant,
            content: format!("[COMPACTED SUMMARY]\n{summary}"),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        },
    };
    session.turns = vec![summary_turn];
    session.turns.extend(keep);
    session.updated_at = now;
    Ok(Some(summary))
}
