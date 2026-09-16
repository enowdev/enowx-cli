use super::{string_arg, Tool, ToolCtx, ToolOutput};
use crate::discovery::{Discovery, SkillEntry};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// Reads a discovered skill's `SKILL.md` on demand so the model does not carry
/// every skill body in context, only the inventory advertised in the system prompt.
pub struct SkillReadTool {
    discovery: Arc<Discovery>,
    disabled: Arc<Vec<String>>,
}

impl SkillReadTool {
    pub fn new(discovery: Arc<Discovery>) -> Self {
        Self {
            discovery,
            disabled: Arc::new(Vec::new()),
        }
    }

    pub fn with_disabled(discovery: Arc<Discovery>, disabled: Vec<String>) -> Self {
        Self {
            discovery,
            disabled: Arc::new(disabled),
        }
    }

    fn find(&self, name: &str) -> Option<&SkillEntry> {
        let needle = name.to_ascii_lowercase();
        if self.disabled.iter().any(|d| d == &needle) {
            return None;
        }
        self.discovery.skills.iter().find(|s| s.name == needle)
    }
}

#[async_trait]
impl Tool for SkillReadTool {
    fn name(&self) -> &str {
        "skill_read"
    }
    fn description(&self) -> &str {
        "Read a discovered skill's SKILL.md by name. Use before doing work that a listed skill covers."
    }
    fn parameters(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"name":{"type":"string","description":"Skill name as listed in the system prompt"}},
            "required":["name"],
            "additionalProperties":false
        })
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let name = string_arg(&args, "name")?;
        let Some(entry) = self.find(name) else {
            return Ok(ToolOutput::error(format!(
                "no skill named `{name}` was discovered"
            )));
        };
        let body = std::fs::read_to_string(&entry.path)?;
        Ok(ToolOutput::ok(format!(
            "# {} ({})\n{}",
            entry.name,
            entry.path.display(),
            body
        )))
    }
}
