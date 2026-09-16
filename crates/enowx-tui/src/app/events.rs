use super::*;

impl App {
    pub(crate) fn start_turn(&mut self, prompt: String) {
        if self.busy {
            self.status = "Still working; Ctrl+C interrupts".into();
            return;
        }
        // Attachments already live as inline `[Image N]` chips inside the
        // prompt; the transcript replays the same string, and the payload sent
        // to the provider strips the chips so only real prose reaches the model.
        let attachments = std::mem::take(&mut self.attachments);
        self.attach_error = None;
        // Show attachments alongside the prose so the transcript reflects
        // what the model actually saw (and what a resumed session should
        // replay).
        let mut display = prompt.clone();
        if !attachments.is_empty() {
            let mut names: Vec<String> = attachments
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
        let clean_prompt = crate::attachments::strip_chips(&prompt);
        self.auto_scroll = true;
        self.busy = true;
        self.turn_started = Instant::now();
        self.set_activity(Activity::Waiting);
        self.status = "working".into();
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(256);
        self.cancel = Some(cancel.clone());
        self.events = Some(rx);
        let agent = self.agent.clone();
        let request = RunRequest {
            session_id: self.session_id.clone(),
            prompt: clean_prompt,
            role: self.role,
            attachments,
        };
        self.task = Some(tokio::spawn(async move {
            let _ = agent.run(request, tx, cancel).await;
        }));
    }

    pub(crate) fn interrupt(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
            self.set_activity(Activity::Tool("stopping".into()));
            self.status = "interrupting".into();
        } else {
            self.status = "nothing running".into();
        }
    }

    pub(crate) fn apply_event(&mut self, event: Event) {
        match event {
            Event::Session { id, title } => {
                self.session_id = Some(id);
                self.title = title;
            }
            Event::MessageStart { .. } => self.set_activity(Activity::Waiting),
            Event::Text { delta } => {
                self.set_activity(Activity::Writing);
                if let Some(last) = self
                    .blocks
                    .last_mut()
                    .filter(|block| matches!(block.kind, TranscriptKind::Assistant))
                {
                    last.text.push_str(&delta);
                } else {
                    self.push(TranscriptKind::Assistant, delta);
                }
            }
            Event::Reasoning { delta } => {
                self.set_activity(Activity::Thinking);
                if let Some(last) = self
                    .blocks
                    .last_mut()
                    .filter(|block| matches!(block.kind, TranscriptKind::Reasoning))
                {
                    last.text.push_str(&delta);
                } else {
                    self.push(TranscriptKind::Reasoning, delta);
                }
            }
            Event::ToolCall {
                id,
                name,
                arguments,
            } => {
                *self.tool_counts.entry(name.clone()).or_insert(0) += 1;
                self.set_activity(Activity::Tool(name.clone()));
                self.push(
                    TranscriptKind::Tool {
                        id,
                        name,
                        args: arguments,
                        result: String::new(),
                        running: true,
                        error: false,
                    },
                    String::new(),
                );
            }
            Event::ToolResult {
                id,
                content,
                is_error,
                ..
            } => {
                if let Some(block) = self.blocks.iter_mut().rev().find(|block| {
                    matches!(&block.kind, TranscriptKind::Tool { id: block_id, .. } if block_id == &id)
                }) {
                    if let TranscriptKind::Tool {
                        result,
                        running,
                        error,
                        ..
                    } = &mut block.kind
                    {
                        *result = content;
                        *running = false;
                        *error = is_error;
                    }
                }
                self.set_activity(Activity::Waiting);
            }
            Event::ToolProgress { id, delta } => {
                // Append the delta to the matching tool block's `result` so
                // the classify+render path picks up the growing preview on
                // the next frame. No new block gets created; if the tool is
                // already gone (rare race), the delta is silently dropped.
                if let Some(block) = self.blocks.iter_mut().rev().find(|block| {
                    matches!(
                        &block.kind,
                        TranscriptKind::Tool { id: bid, .. } if bid == &id
                    )
                }) {
                    if let TranscriptKind::Tool { result, .. } = &mut block.kind {
                        result.push_str(&delta);
                    }
                }
            }
            Event::FormatterMissing {
                language,
                bin,
                install_hint,
                install_cmd,
            } => {
                self.push(
                    TranscriptKind::Notice,
                    format!(
                        "Formatter `{bin}` for {language} not installed. {install_hint}: `{}`",
                        install_cmd.join(" ")
                    ),
                );
            }
            Event::Notice { message } => self.push(TranscriptKind::Notice, message),
            Event::Usage {
                input_tokens,
                output_tokens,
                context_tokens,
                context_window,
            } => {
                self.tokens_in = input_tokens;
                self.tokens_out = output_tokens;
                self.context_tokens = context_tokens;
                if context_window > 0 {
                    self.context_window = context_window;
                }
            }
            Event::Error { message } => {
                self.push(TranscriptKind::Error, message);
                self.busy = false;
                self.cancel = None;
                self.set_activity(Activity::Idle);
                self.status = "failed".into();
            }
            Event::Done { stop_reason } => {
                self.busy = false;
                self.cancel = None;
                self.set_activity(Activity::Idle);
                self.status = stop_reason;
            }
        }
    }

    pub(crate) fn drain_events(&mut self) {
        loop {
            let next = self.events.as_mut().map(mpsc::Receiver::try_recv);
            match next {
                Some(Ok(event)) => self.apply_event(event),
                Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                    if self.busy {
                        self.push(
                            TranscriptKind::Error,
                            "Agent stopped without a terminal event",
                        );
                    }
                    self.set_activity(Activity::Idle);
                    self.busy = false;
                    self.events = None;
                    break;
                }
                _ => break,
            }
        }
    }
}
