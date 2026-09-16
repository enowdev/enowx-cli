use super::*;

impl App {
    pub(crate) fn open_settings(&mut self) {
        self.settings = SettingsDraft::from_config(&self.config);
        self.modal = Modal::Settings;
        self.modal_cursor = 0;
        self.field_cursor = self.settings.value(SETTINGS_FIELDS[0]).len();
        self.modal_error.clear();
    }

    pub(crate) fn open_providers(&mut self) {
        self.modal = Modal::Providers;
        self.modal_cursor = 0;
        self.modal_error.clear();
        self.modal_items = PROVIDER_PRESETS
            .iter()
            .map(|preset| {
                (
                    preset.name.to_owned(),
                    if preset.id == "custom" {
                        "Set your own OpenAI-compatible endpoint".into()
                    } else {
                        "Connect with your API key".into()
                    },
                )
            })
            .collect();
        if self.config.provider_active() {
            self.modal_items.push((
                format!("Edit current · {}", self.config.provider.name),
                "Provider and model settings".into(),
            ));
        }
    }

    pub(crate) fn select_provider(&mut self) {
        let Some(preset) = PROVIDER_PRESETS.get(self.modal_cursor) else {
            self.open_settings();
            return;
        };
        if self.config.provider.preset == preset.id {
            self.settings = SettingsDraft::from_config(&self.config);
        } else {
            self.settings = SettingsDraft::from_config(&Config::default());
            self.settings.provider = if preset.id == "custom" {
                String::new()
            } else {
                preset.name.into()
            };
            self.settings.base_url = preset.base_url.into();
            self.settings.models_url = preset.models_url.into();
        }
        self.settings.preset = preset.id.into();
        self.models.clear();
        self.model_events = None;
        self.modal_error.clear();
        self.modal = if preset.id == "custom" {
            Modal::Settings
        } else {
            Modal::ProviderKey
        };
        self.modal_cursor = if preset.id == "custom" { 0 } else { 2 };
        self.field_cursor = self
            .settings
            .value(SETTINGS_FIELDS[self.modal_cursor])
            .len();
    }

    pub(crate) fn connect_preset(&mut self) -> Result<()> {
        anyhow::ensure!(
            !self.settings.api_key.trim().is_empty(),
            "Enter the provider API key."
        );
        let mut next = self.config.clone();
        next.provider.name = self.settings.provider.trim().into();
        next.provider.preset = self.settings.preset.clone();
        next.provider.base_url = self.settings.base_url.trim().into();
        next.provider.models_url = self.settings.models_url.trim().into();
        next.provider.api_key = self.settings.api_key.trim().into();
        if next.provider.preset != self.config.provider.preset
            || next.provider.base_url != self.config.provider.base_url
        {
            next.model.default.clear();
            next.model.context_window = 128_000;
        }
        next.save()?;
        self.adopt(next);
        self.status = "provider connected; add a model".into();
        self.open_model_source();
        Ok(())
    }

    pub(crate) fn save_settings(&mut self) -> Result<()> {
        let mut next = self.config.clone();
        next.provider.name = self.settings.provider.trim().to_owned();
        next.provider.preset = self.settings.preset.clone();
        next.provider.base_url = self
            .settings
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_owned();
        next.provider.models_url = self.settings.models_url.trim().to_owned();
        next.model.default = self.settings.model.trim().to_owned();
        next.provider.api_key = self.settings.api_key.trim().to_owned();
        next.ui.theme = self.settings.theme.clone();
        anyhow::ensure!(
            self.settings.provider_active(),
            "Provider name and Base URL are required."
        );
        let window = self.settings.context_window.trim();
        next.model.context_window = if window.is_empty() && next.model.default.is_empty() {
            128_000
        } else {
            window
                .parse()
                .map_err(|_| anyhow::anyhow!("Context window must be a whole number of tokens."))?
        };
        next.save()?;
        let needs_model = next.model.default.is_empty();
        let ready = next.is_ready();
        self.adopt(next);
        self.modal_error.clear();
        if needs_model {
            self.status = "provider saved; choose a model".into();
            self.open_model_source();
            return Ok(());
        }
        self.modal = Modal::None;
        self.status = if ready {
            "provider saved".into()
        } else {
            "saved; this provider still needs an API key".into()
        };
        Ok(())
    }

    pub(crate) fn open_model_source(&mut self) {
        self.models.clear();
        self.modal_items.clear();
        self.modal_error.clear();
        if !self.settings.provider_active() {
            self.open_providers();
            return;
        }
        self.modal = Modal::ModelSource;
        self.modal_cursor = 0;
        self.modal_items = vec![
            (
                "Auto detect".into(),
                "Enter a model-list URL and detect metadata".into(),
            ),
            (
                "Manual".into(),
                "Enter a model ID and context window yourself".into(),
            ),
        ];
    }

    pub(crate) fn discover_models(&mut self) {
        self.model_events = None;
        self.models.clear();
        self.modal_items.clear();
        self.modal_error.clear();
        if !self.settings.provider_active() {
            self.open_model_source();
            return;
        }
        if self.settings.models_url.trim().is_empty() {
            self.modal = Modal::ModelUrl;
            self.modal_error = "Enter the full model-list URL first.".into();
            return;
        }
        self.discovering_models = true;
        self.modal = Modal::Models;
        self.modal_cursor = 0;
        let mut config = self.config.clone();
        config.provider.name = self.settings.provider.trim().into();
        config.provider.base_url = self.settings.base_url.trim().into();
        config.provider.api_key = self.settings.api_key.trim().into();
        let url = self.settings.models_url.clone();
        let (tx, rx) = mpsc::channel(1);
        self.model_events = Some(rx);
        tokio::spawn(async move {
            tokio::select! {
                _ = tx.closed() => {}
                result = async { Provider::from_config(&config)?.models(&url).await } => {
                    let _ = tx.send(result.map_err(|error| format!("{error:#}"))).await;
                }
            }
        });
    }

    pub(crate) fn close_models(&mut self) {
        self.model_events = None;
        self.discovering_models = false;
        self.modal_error.clear();
        self.modal = Modal::Settings;
        self.modal_cursor = 4;
        self.field_cursor = self.settings.model.len();
    }

    pub(crate) fn drain_model_events(&mut self) {
        let next = self.model_events.as_mut().map(mpsc::Receiver::try_recv);
        match next {
            Some(Ok(Ok(models))) => {
                self.modal_items = models
                    .iter()
                    .map(|model| (model.id.clone(), model.summary()))
                    .collect();
                self.models = models;
                self.modal_cursor = self
                    .models
                    .iter()
                    .position(|model| model.id == self.settings.model)
                    .unwrap_or(0);
                self.modal_error.clear();
                self.discovering_models = false;
                self.model_events = None;
                self.status = format!("{} models detected", self.models.len());
            }
            Some(Ok(Err(error))) => {
                self.discovering_models = false;
                self.model_events = None;
                self.status = "model fetch failed".into();
                self.modal_error = format!("{error}. F5 retry; F2 manual ID.");
            }
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                self.discovering_models = false;
                self.model_events = None;
                self.status = "model fetch failed".into();
                self.modal_error = "Model request stopped. F5 retry; F2 manual ID.".into();
            }
            _ => {}
        }
    }
}
