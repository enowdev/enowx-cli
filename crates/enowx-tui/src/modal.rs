//! Modal pickers and the provider/model settings draft they edit.

pub use enowx_core::persist::McpDraft;
use enowx_core::Config;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    Roles,
    Sessions,
    Providers,
    ProviderKey,
    ModelSource,
    ModelUrl,
    Models,
    Settings,
    Themes,
    Attach,
    Skills,
    Mcp,
    McpForm,
    /// Ctrl+C in an empty composer: confirm before quitting.
    QuitConfirm,
}

impl Modal {
    pub fn title(self) -> &'static str {
        match self {
            Modal::Roles => " AGENT ROLE ",
            Modal::Sessions => " RESUME SESSION ",
            Modal::Providers => " PROVIDER ",
            Modal::ModelSource => " ADD MODEL ",
            Modal::Models => " DETECTED MODELS ",
            Modal::Themes => " THEME ",
            Modal::Attach => " ATTACH IMAGE ",
            Modal::Skills => " SKILLS ",
            Modal::Mcp => " MCP SERVERS ",
            Modal::McpForm => " ADD MCP SERVER ",
            Modal::QuitConfirm => " QUIT ENX ",
            Modal::None | Modal::Settings | Modal::ModelUrl | Modal::ProviderKey => "",
        }
    }

    /// Text-field modals share one key handler and one renderer.
    pub fn is_form(self) -> bool {
        matches!(
            self,
            Modal::Settings | Modal::ModelUrl | Modal::ProviderKey | Modal::McpForm
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Provider,
    BaseUrl,
    ApiKey,
    ModelsUrl,
    Model,
    ContextWindow,
    Theme,
}

pub const SETTINGS_FIELDS: [SettingsField; 7] = [
    SettingsField::Provider,
    SettingsField::BaseUrl,
    SettingsField::ApiKey,
    SettingsField::ModelsUrl,
    SettingsField::Model,
    SettingsField::ContextWindow,
    SettingsField::Theme,
];

pub const SETTINGS_LABELS: [&str; 7] = [
    "Provider name",
    "Base URL",
    "API key",
    "Model-list URL",
    "Model ID",
    "Context window",
    "Theme",
];

#[derive(Clone)]
pub struct SettingsDraft {
    pub provider: String,
    pub preset: String,
    pub base_url: String,
    pub api_key: String,
    pub models_url: String,
    pub model: String,
    pub context_window: String,
    pub theme: String,
}

impl SettingsDraft {
    pub fn from_config(config: &Config) -> Self {
        Self {
            provider: config.provider.name.clone(),
            preset: config.provider.preset.clone(),
            base_url: config.provider.base_url.clone(),
            api_key: config.provider.api_key.clone(),
            models_url: config.provider.models_url.clone(),
            model: config.model.default.clone(),
            context_window: config.model.context_window.to_string(),
            theme: config.ui.theme.clone(),
        }
    }

    pub fn value(&self, field: SettingsField) -> &str {
        match field {
            SettingsField::Provider => &self.provider,
            SettingsField::BaseUrl => &self.base_url,
            SettingsField::ApiKey => &self.api_key,
            SettingsField::ModelsUrl => &self.models_url,
            SettingsField::Model => &self.model,
            SettingsField::ContextWindow => &self.context_window,
            SettingsField::Theme => &self.theme,
        }
    }

    pub fn value_mut(&mut self, field: SettingsField) -> &mut String {
        match field {
            SettingsField::Provider => &mut self.provider,
            SettingsField::BaseUrl => &mut self.base_url,
            SettingsField::ApiKey => &mut self.api_key,
            SettingsField::ModelsUrl => &mut self.models_url,
            SettingsField::Model => &mut self.model,
            SettingsField::ContextWindow => &mut self.context_window,
            SettingsField::Theme => &mut self.theme,
        }
    }

    pub fn provider_active(&self) -> bool {
        !self.provider.trim().is_empty() && !self.base_url.trim().is_empty()
    }
}

/// Fields in the `Add MCP` popup, tabbed through with Up/Down.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum McpFormField {
    Name,
    Command,
    Args,
    Env,
    Transport,
}

pub const MCP_FORM_FIELDS: [McpFormField; 5] = [
    McpFormField::Name,
    McpFormField::Command,
    McpFormField::Args,
    McpFormField::Env,
    McpFormField::Transport,
];

pub const MCP_FORM_LABELS: [&str; 5] = [
    "Name",
    "Command / URL",
    "Args (comma-separated)",
    "Env (KEY=VAL per line)",
    "Transport (stdio/http/sse)",
];

impl McpFormField {
    pub fn placeholder(self) -> &'static str {
        match self {
            McpFormField::Name => "e.g. github",
            McpFormField::Command => "e.g. npx or https://…",
            McpFormField::Args => "-y, @modelcontextprotocol/server-github",
            McpFormField::Env => "GITHUB_TOKEN=...",
            McpFormField::Transport => "stdio",
        }
    }
}
