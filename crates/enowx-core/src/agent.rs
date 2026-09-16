//! The core agent loop: stream a model call, persist it, execute every tool call,
//! append paired results, then continue until the model yields or a cap stops it.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    event::Event,
    message::{Attachment, Message, Role as MessageRole},
    provider::{Chunk, Provider},
    role::Role,
    session::{Session, SessionStore},
    tools::{mcp_proxy::McpProxyTool, skill::SkillReadTool, ToolCtx, ToolOutput, ToolRegistry},
};

use crate::{discovery::Discovery, mcp::McpClient};

/// Build the tool registry with skill discovery. MCP servers are spawned lazily
/// via `Agent::warm_mcp` so a synchronous `Agent::new` cannot deadlock the
/// tokio runtime with a nested `block_on`.
fn build_registry(discovery: Arc<Discovery>, disabled_skills: Vec<String>) -> ToolRegistry {
    let mut registry = ToolRegistry::default();
    let active = discovery
        .skills
        .iter()
        .filter(|s| !disabled_skills.iter().any(|d| d == &s.name))
        .count();
    if active > 0 {
        registry.register(SkillReadTool::with_disabled(
            discovery.clone(),
            disabled_skills,
        ));
    }
    registry
}
async fn warm_mcp(registry: &mut ToolRegistry, discovery: &Discovery) -> Vec<Arc<McpClient>> {
    let mut clients = Vec::new();
    for server in discovery.mcp_servers.iter().filter(|s| s.enabled) {
        let client = match McpClient::spawn(server).await {
            Ok(client) => Arc::new(client),
            Err(_) => continue,
        };
        // A slow server should never freeze startup; skip it instead.
        let tools =
            match tokio::time::timeout(std::time::Duration::from_secs(10), client.list_tools())
                .await
            {
                Ok(Ok(tools)) => tools,
                _ => Vec::new(),
            };
        for tool in tools {
            registry.register(McpProxyTool::new(client.clone(), &tool));
        }
        clients.push(client);
    }
    clients
}
pub struct RunRequest {
    pub session_id: Option<String>,
    pub prompt: String,
    pub role: Role,
    /// Images the user attached to this message.
    pub attachments: Vec<Attachment>,
}

/// Summary row shown by the TUI for one proxied MCP tool.
#[derive(Debug, Clone)]
pub struct McpToolSummary {
    pub name: String,
    pub description: String,
}

fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Rough char-based token estimate for the whole session content. Not an
/// exact tokenizer, but stable enough for an auto-compact threshold: ~4
/// chars per token averages out across natural prose and code.
fn estimate_session_tokens(session: &crate::session::Session) -> f32 {
    let mut chars: usize = 0;
    for t in &session.turns {
        chars += t.message.content.len();
        if let Some(r) = &t.message.reasoning {
            chars += r.len();
        }
        for tc in &t.message.tool_calls {
            chars += tc.arguments.len();
        }
    }
    (chars as f32) / 4.0
}

pub struct Agent {
    config: Config,
    tools: Arc<tokio::sync::RwLock<ToolRegistry>>,
    store: SessionStore,
    discovery: Arc<Discovery>,
    /// Kept alive so child processes survive for the agent's lifetime; set
    /// once by `Agent::warm` and never mutated afterward.
    #[allow(dead_code)]
    mcp_clients: Arc<tokio::sync::Mutex<Vec<Arc<McpClient>>>>,
    mcp_warmed: Arc<std::sync::atomic::AtomicBool>,
}

impl Agent {
    pub fn new(config: Config) -> Self {
        Self::assemble(config, SessionStore::default())
    }

    pub fn with_store(config: Config, store: SessionStore) -> Self {
        Self::assemble(config, store)
    }

    fn assemble(config: Config, store: SessionStore) -> Self {
        let discovery = Arc::new(Discovery::run(&config.workspace()));
        let disabled = config.ui.disabled_skills.clone();
        let registry = build_registry(discovery.clone(), disabled);
        Self {
            config,
            tools: Arc::new(tokio::sync::RwLock::new(registry)),
            store,
            discovery,
            mcp_clients: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            mcp_warmed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Spawn every discovered MCP server on the current tokio runtime. Safe to
    /// call more than once: the second call is a no-op. Called on the first
    /// turn so `Agent::new` stays sync-friendly.
    async fn warm_mcp_servers(&self) {
        use std::sync::atomic::Ordering;
        if self.mcp_warmed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut registry = self.tools.write().await;
        let clients = warm_mcp(&mut registry, &self.discovery).await;
        *self.mcp_clients.lock().await = clients;
    }

    pub fn discovery(&self) -> Arc<Discovery> {
        self.discovery.clone()
    }

    /// Snapshot of every proxy tool the registry currently exposes for one
    /// MCP server. Returns `(name, description)` pairs, stripped of the
    /// `mcp__<server>__` prefix.
    pub fn mcp_tools(&self, server: &str) -> Vec<crate::agent::McpToolSummary> {
        let prefix = format!("mcp__{}__", sanitize(server));
        let Ok(registry) = self.tools.try_read() else {
            return Vec::new();
        };
        registry
            .keys()
            .filter_map(|(name, desc)| {
                name.strip_prefix(&prefix).map(|short| McpToolSummary {
                    name: short.to_string(),
                    description: desc.clone(),
                })
            })
            .collect()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    /// Manually compact a session: fold older turns into a summary. Returns
    /// the summary text when work was done; `None` when the session was too
    /// short to compact or the summarizer produced nothing.
    pub async fn compact(&self, session_id: &str) -> Result<Option<String>> {
        let mut session = self.store.load(session_id)?;
        let provider = Provider::from_config(&self.config)?;
        let keep = self.config.agent.compact_keep_last.max(1);
        let summary = crate::compact::compact(&mut session, &provider, keep).await?;
        if summary.is_some() {
            self.store.save(&session)?;
        }
        Ok(summary)
    }

    pub async fn run(
        &self,
        request: RunRequest,
        events: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<()> {
        match self.run_inner(request, &events, cancel).await {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = events
                    .send(Event::Error {
                        message: format!("{error:#}"),
                    })
                    .await;
                Err(error)
            }
        }
    }

    async fn run_inner(
        &self,
        request: RunRequest,
        events: &mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let prompt = request.prompt.trim();
        if prompt.is_empty() {
            anyhow::bail!("the message is empty");
        }
        anyhow::ensure!(
            self.config.agent.max_steps > 0,
            "agent.max_steps must be greater than zero"
        );
        anyhow::ensure!(
            self.config.is_ready(),
            "Configure a provider with /provider, or set ENX_BASE_URL, ENX_MODEL and ENX_API_KEY."
        );
        let workspace = std::fs::canonicalize(self.config.workspace())
            .context("resolving the configured workspace")?;
        let mut session = match &request.session_id {
            Some(id) => {
                let session = self.store.load(id)?;
                if !session.workspace.as_os_str().is_empty() && session.workspace != workspace {
                    anyhow::bail!(
                        "session {} belongs to {}, but this server serves {}",
                        session.id,
                        session.workspace.display(),
                        workspace.display()
                    );
                }
                session
            }
            None => {
                let mut session = Session::new(request.role);
                session.workspace = workspace.clone();
                session
            }
        };
        session.role = request.role;
        session.push(Message::user(prompt).with_attachments(request.attachments.clone()));
        self.store.save(&session)?;

        // Auto-compact: if the previous turn's context usage was near the
        // window, fold older turns into a summary before we spend another
        // round-trip. Cheap heuristic: peek at the last stored Usage-shaped
        // hint via the message count and trigger conservatively.
        if self.config.agent.auto_compact
            && self.config.agent.auto_compact_at > 0.0
            && session.turns.len() > self.config.agent.compact_keep_last + 2
        {
            let window = self.config.model.context_window.max(1) as f32;
            let approx_tokens = estimate_session_tokens(&session);
            let ratio = approx_tokens / window;
            if ratio >= self.config.agent.auto_compact_at {
                let _ = events
                    .send(Event::Notice {
                        message: format!(
                            "auto-compacting session ({:.0}% of context used)…",
                            ratio * 100.0
                        ),
                    })
                    .await;
                let provider_pre = Provider::from_config(&self.config)?;
                let keep = self.config.agent.compact_keep_last.max(1);
                let summary = crate::compact::compact(&mut session, &provider_pre, keep).await?;
                if summary.is_some() {
                    self.store.save(&session)?;
                    let _ = events
                        .send(Event::Notice {
                            message: "compact done; continuing".into(),
                        })
                        .await;
                }
            }
        }
        let _ = events
            .send(Event::Session {
                id: session.id.clone(),
                title: session.title.clone(),
            })
            .await;

        let provider = Provider::from_config(&self.config)?;
        let provider_model = self.config.model.default.clone();
        let role = session.role;
        self.warm_mcp_servers().await;
        let tools_registry = self.tools.read().await;
        let schemas = tools_registry.schemas(role, Some(&self.discovery));
        let mut prompt = role.system_prompt(&workspace.to_string_lossy());
        if let Some(extra) = self
            .discovery
            .system_prompt_with_disabled(&self.config.ui.disabled_skills)
        {
            prompt.push('\n');
            prompt.push_str(&extra);
        }
        let system = Message::system(prompt);
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<(String, String)>(64);
        // Forward every tool progress delta to the UI event stream so long
        // running tools (write, bash) reveal output while they work instead
        // of dropping it in one lump at the end.
        let events_forward = events.clone();
        tokio::spawn(async move {
            while let Some((id, delta)) = progress_rx.recv().await {
                let _ = events_forward.send(Event::ToolProgress { id, delta }).await;
            }
        });
        let tool_ctx = ToolCtx {
            workspace,
            shell_timeout: Duration::from_secs(self.config.agent.shell_timeout_secs),
            cancel: cancel.clone(),
            progress: Some(progress_tx),
            call_id: String::new(),
        };

        let mut stop_reason = "stop".to_string();
        for step in 0..self.config.agent.max_steps {
            if cancel.is_cancelled() {
                stop_reason = "aborted".into();
                break;
            }
            let mut wire = Vec::with_capacity(session.turns.len() + 1);
            wire.push(system.clone());
            wire.extend(session.replay());

            let assistant_id = uuid::Uuid::new_v4().to_string();
            let _ = events
                .send(Event::MessageStart {
                    id: assistant_id.clone(),
                })
                .await;
            let (chunk_tx, mut chunk_rx) = mpsc::channel(256);
            let text = Arc::new(Mutex::new(String::new()));
            let reasoning = Arc::new(Mutex::new(String::new()));
            let event_sink = events.clone();
            let text_capture = text.clone();
            let reasoning_capture = reasoning.clone();
            let forward = tokio::spawn(async move {
                while let Some(chunk) = chunk_rx.recv().await {
                    let event = match chunk {
                        Chunk::Text(delta) => {
                            text_capture.lock().expect("text capture").push_str(&delta);
                            Event::Text { delta }
                        }
                        Chunk::Reasoning(delta) => {
                            reasoning_capture
                                .lock()
                                .expect("reasoning capture")
                                .push_str(&delta);
                            Event::Reasoning { delta }
                        }
                    };
                    if event_sink.send(event).await.is_err() {
                        break;
                    }
                }
            });

            // Bind the retry-notice sink to a local so its lifetime spans
            // the entire select! await; passing an inline `Some(Box::new…)`
            // is a temporary that drops before `provider.complete_with_notice`
            // can await it.
            let notice_sink: crate::provider::NoticeSink = {
                let events = events.clone();
                Some(Box::new(move |msg: String| {
                    let events = events.clone();
                    tokio::spawn(async move {
                        let _ = events.send(Event::Notice { message: msg }).await;
                    });
                }) as Box<dyn Fn(String) + Send + Sync>)
            };
            let completion = tokio::select! {
                result = provider.complete_with_notice(&wire, &schemas, &chunk_tx, &notice_sink) => {
                    drop(chunk_tx);
                    let _ = forward.await;
                    match result {
                        Ok(completion) => completion,
                        Err(error) => {
                            persist_interrupted(&mut session, &self.store, &assistant_id, &provider_model, &text, &reasoning, format!("{error:#}"))?;
                            return Err(error);
                        }
                    }
                }
                _ = cancel.cancelled() => {
                    drop(chunk_tx);
                    let _ = forward.await;
                    persist_interrupted(&mut session, &self.store, &assistant_id, &provider_model, &text, &reasoning, "Interrupted by user.".into())?;
                    stop_reason = "aborted".into();
                    break;
                }
            };

            let _ = events
                .send(Event::Usage {
                    input_tokens: completion.usage.input_tokens,
                    output_tokens: completion.usage.output_tokens,
                    context_tokens: completion.usage.input_tokens,
                    context_window: self.config.model.context_window,
                })
                .await;
            session.push(Message {
                role: MessageRole::Assistant,
                content: completion.text,
                reasoning: (!completion.reasoning.is_empty()).then_some(completion.reasoning),
                tool_calls: completion.tool_calls.clone(),
                attachments: Vec::new(),
                tool_call_id: None,
                interrupted: false,
                error: None,
                model: Some(provider_model.clone()),
                message_id: Some(assistant_id),
            });
            self.store.save(&session)?;
            if completion.tool_calls.is_empty() {
                stop_reason = completion.finish_reason;
                break;
            }

            for call in &completion.tool_calls {
                let _ = events
                    .send(Event::ToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    })
                    .await;
                let output = if cancel.is_cancelled() {
                    ToolOutput::error("Cancelled before execution.")
                } else if !matches!(completion.finish_reason.as_str(), "stop" | "tool_calls") {
                    ToolOutput::error(format!(
                        "Not executed: provider stopped with {} (possibly truncated arguments).",
                        completion.finish_reason
                    ))
                } else {
                    match serde_json::from_str::<serde_json::Value>(&call.arguments) {
                        Ok(args @ serde_json::Value::Object(_)) => {
                            let mut ctx = tool_ctx.clone();
                            ctx.call_id = call.id.clone();
                            tools_registry.execute(role, &ctx, &call.name, args).await
                        }
                        Ok(_) => ToolOutput::error("tool arguments must be a JSON object"),
                        Err(error) => {
                            ToolOutput::error(format!("tool arguments are not valid JSON: {error}"))
                        }
                    }
                };
                let _ = events
                    .send(Event::ToolResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        content: output.content.clone(),
                        is_error: output.is_error,
                    })
                    .await;
                let mut result = Message::tool_result(&call.id, output.content);
                if output.is_error {
                    result.error = Some("Tool execution failed".into());
                }
                session.push(result);
                self.store.save(&session)?;
            }
            self.store.save(&session)?;
            if !matches!(completion.finish_reason.as_str(), "stop" | "tool_calls") {
                stop_reason = completion.finish_reason;
                break;
            }
            if cancel.is_cancelled() {
                stop_reason = "aborted".into();
                break;
            }
            if step + 1 == self.config.agent.max_steps {
                stop_reason = "step_limit".into();
                let _ = events
                    .send(Event::Notice {
                        message: format!(
                            "Stopped after {} model calls. The turn is incomplete.",
                            self.config.agent.max_steps
                        ),
                    })
                    .await;
            }
        }

        self.store.save(&session)?;
        let _ = events.send(Event::Done { stop_reason }).await;
        Ok(())
    }
}

fn persist_interrupted(
    session: &mut Session,
    store: &SessionStore,
    message_id: &str,
    model: &str,
    text: &Mutex<String>,
    reasoning: &Mutex<String>,
    error: String,
) -> Result<()> {
    let content = text.lock().expect("text capture").clone();
    let reasoning = reasoning.lock().expect("reasoning capture").clone();
    session.push(Message {
        role: MessageRole::Assistant,
        content,
        reasoning: (!reasoning.is_empty()).then_some(reasoning),
        tool_calls: Vec::new(),
        attachments: Vec::new(),
        tool_call_id: None,
        interrupted: true,
        error: Some(error),
        model: Some(model.to_string()),
        message_id: Some(message_id.to_string()),
    });
    store.save(session)
}
