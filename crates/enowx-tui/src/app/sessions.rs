use super::*;

impl App {
    /// Open the modal picker for saved sessions in this workspace.
    pub(crate) fn open_sessions(&mut self) -> Result<()> {
        let workspace = std::fs::canonicalize(self.config.workspace())
            .unwrap_or_else(|_| self.config.workspace());
        let mut items = Vec::new();
        for meta in self.store.list(200)? {
            // A session belongs to the workspace it was created in. Loading
            // the header is cheap because `list` already parsed the file.
            let owned = self
                .store
                .load(&meta.id)
                .map(|session| session.workspace == workspace)
                .unwrap_or(false);
            if owned {
                items.push((
                    meta.id.clone(),
                    format!(
                        "{} · {} · {} messages",
                        if meta.title.is_empty() {
                            "Untitled".into()
                        } else {
                            meta.title
                        },
                        meta.role.label(),
                        meta.message_count
                    ),
                ));
            }
        }
        if items.is_empty() {
            self.push(
                TranscriptKind::System,
                "No saved sessions in this workspace.",
            );
            return Ok(());
        }
        self.modal_items = items;
        self.modal = Modal::Sessions;
        self.modal_cursor = 0;
        Ok(())
    }

    pub(crate) fn resume(&mut self, id: &str) -> Result<()> {
        let session = self.store.load(id)?;
        let workspace = std::fs::canonicalize(self.config.workspace())?;
        anyhow::ensure!(
            session.workspace == workspace,
            "Session belongs to {}",
            session.workspace.display()
        );
        self.tokens_in = 0;
        self.tokens_out = 0;
        self.context_tokens = 0;
        self.tool_counts.clear();
        self.session_id = Some(session.id.clone());
        self.title = session.title;
        self.role = session.role;
        self.blocks.clear();
        self.events = None;
        self.auto_scroll = true;
        let mut tool_blocks: HashMap<String, usize> = HashMap::new();
        for turn in session.turns {
            match turn.message.role {
                MessageRole::User => {
                    let mut display = turn.message.content.clone();
                    if !turn.message.attachments.is_empty() {
                        let mut names: Vec<String> = turn
                            .message
                            .attachments
                            .iter()
                            .map(|a| format!("📎 {}", a.name))
                            .collect();
                        names.sort();
                        if !display.is_empty() {
                            display.push_str("\n\n");
                        }
                        display.push_str(&names.join("  "));
                    }
                    self.push(TranscriptKind::User, display);
                }
                MessageRole::Assistant => {
                    if let Some(reasoning) = turn.message.reasoning {
                        self.push(TranscriptKind::Reasoning, reasoning);
                    }
                    if !turn.message.content.is_empty() {
                        self.push(TranscriptKind::Assistant, turn.message.content);
                    }
                    for call in turn.message.tool_calls {
                        *self.tool_counts.entry(call.name.clone()).or_insert(0) += 1;
                        let index = self.blocks.len();
                        tool_blocks.insert(call.id.clone(), index);
                        self.push(
                            TranscriptKind::Tool {
                                id: call.id,
                                name: call.name,
                                args: call.arguments,
                                result: String::new(),
                                running: true,
                                error: false,
                            },
                            String::new(),
                        );
                    }
                    if let Some(error) = turn.message.error {
                        self.push(TranscriptKind::Error, error);
                    }
                }
                MessageRole::Tool => {
                    if let Some(id) = turn.message.tool_call_id {
                        if let Some(index) = tool_blocks.get(&id).copied() {
                            if let TranscriptKind::Tool {
                                result,
                                running,
                                error,
                                ..
                            } = &mut self.blocks[index].kind
                            {
                                *result = turn.message.content;
                                *running = false;
                                *error = turn.message.error.is_some();
                            }
                        }
                    }
                }
                MessageRole::System => self.push(TranscriptKind::System, turn.message.content),
            }
        }
        self.status = format!("resumed {}", &session.id[..8]);
        Ok(())
    }
}
