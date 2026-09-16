use super::*;

impl App {
    pub(crate) fn command_matches(&self) -> Vec<(&'static str, &'static str)> {
        if !self.input.starts_with('/') || self.input.contains(char::is_whitespace) {
            return Vec::new();
        }
        COMMANDS
            .iter()
            .copied()
            .filter(|(name, _)| name.starts_with(&self.input[1..]))
            .collect()
    }

    pub(crate) fn run_command(&mut self, line: &str) -> Result<()> {
        let command = line.trim_start_matches('/');
        let (name, args) = command.split_once(' ').unwrap_or((command, ""));
        if self.busy && matches!(name, "new" | "resume" | "provider" | "role" | "model") {
            self.status = "Stop the current turn before changing session or configuration".into();
            return Ok(());
        }
        match name {
            "help" => {
                let mut text = String::from("Commands");
                for (name, summary) in COMMANDS { text.push_str(&format!("\n  /{name:<10} {summary}")); }
                text.push_str("\n\nKeys\n  Enter      Send message\n  Ctrl+J     Newline\n  Ctrl+R     Toggle reasoning\n  Ctrl+O     Toggle tool output\n  PgUp/PgDn  Scroll transcript\n  Esc        Clear input / close picker\n  Ctrl+C     Stop turn / quit");
                text.push_str("\n  F1–F5      Sidebar tabs\n  Alt+←/→    Sidebar pages\n  Ctrl+B     Toggle sidebar\n  /theme     Choose palette");
                self.push(TranscriptKind::System, text);
            }
            "new" => self.new_session(),
            "resume" => self.open_sessions()?,
            "role" if !args.trim().is_empty() => {
                self.role = Role::parse(args).ok_or_else(|| anyhow::anyhow!("Unknown role: {args}"))?;
                self.status = format!("role: {}", self.role.label());
            }
            "role" => self.open_roles(),
            "model" if !args.trim().is_empty() => {
                anyhow::ensure!(self.config.provider_active(), "Set a provider with /provider first.");
                let mut next = self.config.clone();
                next.model.default = args.trim().into();
                next.save()?;
                self.adopt(next);
                self.status = "model saved".into();
            }
            "model" => {
                self.open_settings();
                if self.settings.provider_active() && !self.settings.models_url.trim().is_empty() {
                    self.discover_models();
                } else {
                    self.open_model_source();
                }
            }
            "provider" => self.open_providers(),
            "attach" if !args.trim().is_empty() => {
                self.attach_from_path(std::path::Path::new(args.trim()));
                if let Some(error) = self.attach_error.clone() {
                    anyhow::bail!(error);
                }
            }
            "attach" => self.open_attach()?,
            "skills" => self.open_skills(),
            "mcp" => self.open_mcp(),
            "compact" => self.start_compact()?,
            "sidebar" => self.toggle_sidebar()?,
            "reasoning" => {
                self.show_reasoning = !self.show_reasoning;
                self.status = format!("reasoning {}", if self.show_reasoning { "on" } else { "off" });
            }
            "tools" => {
                self.show_tool_output = !self.show_tool_output;
                self.status = format!("tool output {}", if self.show_tool_output { "expanded" } else { "compact" });
            }
            "clear" => { self.blocks.clear(); self.status = "transcript cleared".into(); }
            "stop" => self.interrupt(),
            "status" => self.push(TranscriptKind::System, format!(
                "Model: {} · {}\nRole: {}\nWorkspace: {}\nSession: {}\nTokens: {} in / {} out\nTheme: {}\nReasoning: {}",
                self.config.model.default, self.config.provider.name, self.role.label(), self.config.workspace().display(),
                self.session_id.as_deref().unwrap_or("(new)"), self.tokens_in, self.tokens_out, self.theme.name,
                if self.show_reasoning { "on" } else { "off" },
            )),
            "quit" | "exit" => self.should_quit = true,
            "" => {}
            other => self.push(TranscriptKind::Error, format!("Unknown command /{other}. Try /help.")),
        }
        Ok(())
    }
    pub(crate) fn open_themes(&mut self) {
        self.modal = Modal::Themes;
        self.modal_cursor = THEMES
            .iter()
            .position(|t| t.name == self.theme.name)
            .unwrap_or(0);
        self.modal_items = THEMES
            .iter()
            .map(|t| (t.label.into(), format!("Palette id: {}", t.name)))
            .collect();
    }

    pub(crate) fn open_roles(&mut self) {
        self.modal = Modal::Roles;
        self.modal_cursor = ROLES
            .iter()
            .position(|role| *role == self.role)
            .unwrap_or(0);
        self.modal_items = ROLES
            .iter()
            .map(|role| (role.label().into(), role.summary().into()))
            .collect();
    }

    pub(crate) fn accept_modal(&mut self) -> Result<()> {
        match self.modal {
            Modal::Providers => self.select_provider(),
            Modal::ProviderKey => return self.connect_preset(),
            Modal::Roles => {
                if let Some(role) = ROLES.get(self.modal_cursor).copied() {
                    self.role = role;
                    self.status = format!("role: {}", role.label());
                }
                self.modal = Modal::None;
            }
            Modal::Sessions => {
                if let Some((id, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    self.resume(&id)?;
                }
                self.modal = Modal::None;
            }
            Modal::ModelSource => {
                if self.modal_cursor == 0 {
                    self.modal = Modal::ModelUrl;
                    self.modal_cursor = 3;
                    self.field_cursor = self.settings.models_url.len();
                } else {
                    self.modal = Modal::Settings;
                    self.modal_cursor = 4;
                    self.field_cursor = self.settings.model.len();
                }
            }
            Modal::Models => {
                if let Some(model) = self.models.get(self.modal_cursor).cloned() {
                    let mut next = self.config.clone();
                    next.provider.name = self.settings.provider.trim().into();
                    next.provider.preset = self.settings.preset.clone();
                    next.provider.base_url =
                        self.settings.base_url.trim().trim_end_matches('/').into();
                    next.provider.api_key = self.settings.api_key.trim().into();
                    next.provider.models_url = self.settings.models_url.trim().into();
                    next.model.default = model.id;
                    if let Some(context) = model.context_window {
                        next.model.context_window = context;
                    }
                    next.save()?;
                    self.adopt(next);
                    self.model_events = None;
                    self.discovering_models = false;
                    self.modal = Modal::None;
                    self.status = format!("model active: {}", self.config.model.default);
                }
            }
            Modal::Settings => return self.save_settings(),
            Modal::ModelUrl => self.discover_models(),
            Modal::Themes => {
                self.select_theme(self.modal_cursor)?;
                self.modal = Modal::None;
            }
            Modal::Attach => {
                if let Some((path, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    self.attach_from_path(std::path::Path::new(&path));
                }
                self.modal = Modal::None;
                if let Some(error) = self.attach_error.clone() {
                    anyhow::bail!(error);
                }
            }
            Modal::Skills => return self.read_selected_skill(),
            Modal::Mcp => return self.accept_mcp_row(),
            Modal::McpForm => return self.submit_mcp_form(),
            Modal::QuitConfirm => {
                self.should_quit = true;
            }
            Modal::None => {}
        }
        Ok(())
    }

    /// Fire off a manual compact for the current session. Runs off the UI
    /// thread so a slow summarizer never freezes the terminal; the result
    /// arrives via a Notice event.
    pub(crate) fn start_compact(&mut self) -> anyhow::Result<()> {
        let Some(id) = self.session_id.clone() else {
            self.push(
                crate::session::TranscriptKind::Notice,
                "no active session yet — send one message first",
            );
            return Ok(());
        };
        if self.busy {
            anyhow::bail!("a turn is already running; wait or interrupt first");
        }
        let agent = self.agent.clone();
        let (tx, rx) = tokio::sync::mpsc::channel::<enowx_core::Event>(64);
        self.events = Some(rx);
        self.status = "compacting…".into();
        self.busy = true;
        tokio::spawn(async move {
            let notice = match agent.compact(&id).await {
                Ok(Some(_)) => "compact done: older turns folded".to_string(),
                Ok(None) => "compact skipped: not enough history".to_string(),
                Err(e) => format!("compact failed: {e:#}"),
            };
            let _ = tx.send(enowx_core::Event::Notice { message: notice }).await;
            let _ = tx
                .send(enowx_core::Event::Done {
                    stop_reason: "compact".into(),
                })
                .await;
        });
        Ok(())
    }
}
