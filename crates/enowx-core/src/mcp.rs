//! Minimal MCP (Model Context Protocol) stdio client: spawn a server subprocess,
//! speak JSON-RPC 2.0 line-by-line, list its tools, and invoke them so the model
//! can call them like any built-in tool.
//!
//! This is a deliberately small subset of the MCP spec. It handles the parts
//! we need for a coding agent (initialize, tools/list, tools/call) and leaves
//! resources/prompts/notifications for a later pass.

use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, Mutex},
    task::JoinHandle,
    time::timeout,
};

use crate::discovery::{McpServer, McpTransport};

/// One MCP tool the server exposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub server: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl McpTool {
    /// Namespaced name the agent uses so tools from different MCP servers cannot
    /// collide with each other or with built-in tools.
    pub fn qualified(&self) -> String {
        format!("mcp__{}__{}", sanitize(&self.server), sanitize(&self.name))
    }
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Handle to a running MCP server. Dropping the client shuts the child down.
pub struct McpClient {
    server: String,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
    reader_handle: Mutex<Option<JoinHandle<()>>>,
    child: Mutex<Option<Child>>,
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Kill the child if it is still alive; the reader task exits when the
        // pipe closes, so it needs no separate signal.
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

impl McpClient {
    /// Spawn a stdio MCP server and complete the `initialize` handshake.
    pub async fn spawn(server: &McpServer) -> Result<Self> {
        anyhow::ensure!(
            matches!(server.transport, McpTransport::Stdio),
            "only stdio MCP transports are supported"
        );
        let mut command = Command::new(&server.command_or_url);
        command.args(&server.args);
        for (key, value) in &server.env {
            command.env(key, value);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("spawning MCP server `{}`", server.name))?;
        let stdin = child.stdin.take().context("MCP child stdin missing")?;
        let stdout = child.stdout.take().context("MCP child stdout missing")?;
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        let reader_pending = pending.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(value): Result<Value, _> = serde_json::from_str(&line) else {
                    continue;
                };
                let Some(id) = value.get("id").and_then(Value::as_u64) else {
                    // Notifications and log lines have no id; ignore them.
                    continue;
                };
                if let Some(sender) = reader_pending.lock().await.remove(&id) {
                    let _ = sender.send(value);
                }
            }
        });
        let client = Self {
            server: server.name.clone(),
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            reader_handle: Mutex::new(Some(reader)),
            child: Mutex::new(Some(child)),
        };
        // Initialize handshake. Every MCP server rejects tool calls without it.
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "enx", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
            .await?;
        client
            .notify("notifications/initialized", json!({}))
            .await?;
        Ok(client)
    }

    /// Fetch every tool the server advertises.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let response = self.request("tools/list", json!({})).await?;
        let tools = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(tools.len());
        for tool in tools {
            let Some(name) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let input_schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type":"object"}));
            out.push(McpTool {
                server: self.server.clone(),
                name: name.to_owned(),
                description,
                input_schema,
            });
        }
        Ok(out)
    }

    /// Invoke a named tool and return its concatenated text output.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<String> {
        let response = self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await?;
        if let Some(error) = response.get("error") {
            return Err(anyhow!("mcp error: {error}"));
        }
        let content = response
            .get("result")
            .and_then(|r| r.get("content"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut out = String::new();
        for part in content {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        Ok(out)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_frame(&frame).await?;
        // A missing response for 30s means the server is stuck; drop the pending
        // slot so a later reply cannot deliver into a leaked channel.
        match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&id);
                Err(anyhow!("mcp channel closed before reply"))
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(anyhow!("mcp `{method}` timed out"))
            }
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let frame = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.write_frame(&frame).await
    }

    async fn write_frame(&self, frame: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        let text = serde_json::to_string(frame)?;
        stdin.write_all(text.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        if let Some(handle) = self.reader_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
    }
}
