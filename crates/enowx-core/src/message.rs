//! The wire-neutral message model the loop keeps in memory, plus its conversion
//! to the OpenAI-compatible chat payload the provider expects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments as the model produced them.
    pub arguments: String,
}

/// An image the user attached to a message, already encoded for the provider as
/// a `data:` URL so a replayed session needs no access to the original file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// File name shown in the interface.
    pub name: String,
    /// `data:image/png;base64,…`
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    /// Provider reasoning summary, kept out of the replay payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Images attached to a user message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Set on tool results: which call this answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::plain(Role::Assistant, content)
    }

    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: Some(call_id.into()),
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        }
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<Attachment>) -> Self {
        self.attachments = attachments;
        self
    }

    /// The provider payload for this message. Reasoning is dropped: replaying a
    /// summary back to the model wastes context and some providers reject it.
    pub fn to_wire(&self) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        out.insert(
            "role".into(),
            serde_json::Value::String(
                match self.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string(),
            ),
        );
        // Text-only messages keep the plain string form: some OpenAI-compatible
        // endpoints reject the content array when no image is present.
        if self.attachments.is_empty() {
            out.insert(
                "content".into(),
                serde_json::Value::String(self.content.clone()),
            );
        } else {
            let mut parts = Vec::with_capacity(self.attachments.len() + 1);
            if !self.content.is_empty() {
                parts.push(serde_json::json!({"type": "text", "text": self.content}));
            }
            for image in &self.attachments {
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": image.data_url},
                }));
            }
            out.insert("content".into(), serde_json::Value::Array(parts));
        }
        if let Some(id) = &self.tool_call_id {
            out.insert("tool_call_id".into(), serde_json::Value::String(id.clone()));
        }
        if !self.tool_calls.is_empty() {
            let calls: Vec<serde_json::Value> = self
                .tool_calls
                .iter()
                .map(|call| {
                    serde_json::json!({
                        "id": call.id,
                        "type": "function",
                        "function": { "name": call.name, "arguments": call.arguments },
                    })
                })
                .collect();
            out.insert("tool_calls".into(), serde_json::Value::Array(calls));
        }
        serde_json::Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_tool_calls_reach_the_wire_and_reasoning_does_not() {
        let message = Message {
            role: Role::Assistant,
            content: "checking".into(),
            reasoning: Some("private".into()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "read".into(),
                arguments: "{\"path\":\"a.rs\"}".into(),
            }],
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        };
        let wire = message.to_wire();
        assert_eq!(wire["role"], "assistant");
        assert_eq!(wire["tool_calls"][0]["function"]["name"], "read");
        assert!(wire.get("reasoning").is_none());
    }
}
