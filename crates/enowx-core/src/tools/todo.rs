use super::{string_arg, Tool, ToolCtx, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

#[derive(Default)]
pub(super) struct TodoTool {
    items: Mutex<Vec<TodoItem>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TodoItem {
    text: String,
    done: bool,
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }
    fn description(&self) -> &str {
        "Replace or update the current turn's visible task checklist."
    }
    fn parameters(&self) -> Value {
        json!({"type":"object","properties":{
            "op":{"type":"string","enum":["set","done","view","clear"]},
            "items":{"type":"array","items":{"type":"string"}},
            "item":{"type":"string"}
        },"required":["op"],"additionalProperties":false})
    }
    async fn execute(&self, _ctx: &ToolCtx, args: Value) -> Result<ToolOutput> {
        let op = string_arg(&args, "op")?;
        let mut items = self.items.lock().await;
        match op {
            "set" => {
                let raw = args
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(|| anyhow::anyhow!("set requires items"))?;
                *items = raw
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|text| TodoItem {
                        text: text.to_string(),
                        done: false,
                    })
                    .collect();
            }
            "done" => {
                let target = string_arg(&args, "item")?;
                let item = items
                    .iter_mut()
                    .find(|item| item.text == target)
                    .ok_or_else(|| anyhow::anyhow!("unknown todo item: {target}"))?;
                item.done = true;
            }
            "clear" => items.clear(),
            "view" => {}
            other => anyhow::bail!("unknown todo operation: {other}"),
        }
        let open = items.iter().filter(|item| !item.done).count();
        let mut out = items
            .iter()
            .map(|item| format!("{} {}", if item.done { "[x]" } else { "[ ]" }, item.text))
            .collect::<Vec<_>>()
            .join("\n");
        if out.is_empty() {
            out = "No tasks.".to_string();
        }
        out.push_str(&format!("\n{open} remaining"));
        Ok(ToolOutput::ok(out))
    }
}
