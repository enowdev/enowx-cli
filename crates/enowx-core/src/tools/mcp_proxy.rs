use super::{Tool, ToolCtx, ToolOutput};
use crate::mcp::{McpClient, McpTool};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// Wraps one remote MCP tool as a built-in-shaped `Tool`. Names and
/// descriptions live inside the value (owned strings); `Tool::name` and
/// `Tool::description` return `&str` so no static-lifetime hack is needed.
pub struct McpProxyTool {
    name: String,
    description: String,
    schema: Value,
    tool_name: String,
    client: Arc<McpClient>,
}

impl McpProxyTool {
    pub fn new(client: Arc<McpClient>, tool: &McpTool) -> Self {
        let description = if tool.description.is_empty() {
            format!("MCP tool from `{}`", tool.server)
        } else {
            tool.description.clone()
        };
        // Ensure the schema is an object type: some servers advertise only a
        // bare `properties` map, which the harness validator would reject.
        let mut schema = tool.input_schema.clone();
        if schema.get("type").is_none() {
            schema = json!({
                "type": "object",
                "properties": schema,
                "additionalProperties": true,
            });
        }
        Self {
            name: tool.qualified(),
            description,
            schema,
            tool_name: tool.name.clone(),
            client,
        }
    }
}

#[async_trait]
impl Tool for McpProxyTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        self.schema.clone()
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let text = self.client.call_tool(&self.tool_name, args).await?;
        Ok(ToolOutput::ok(text))
    }
}
