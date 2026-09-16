use super::*;

pub(super) async fn providers() -> Json<Value> {
    Json(json!({"providers": enowx_core::PROVIDER_PRESETS}))
}

pub(super) async fn health(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.read().await;
    Json(
        json!({"ok":true,"configured":config.is_ready(),"model":config.model.default,"provider":config.provider.name,"workspace":config.workspace(),"context_window":config.model.context_window}),
    )
}

pub(super) async fn get_config(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.read().await;
    Json(
        json!({"provider":config.provider.name,"preset":config.provider.preset,"base_url":config.provider.base_url,"models_url":config.provider.models_url,"model":config.model.default,"has_api_key":!config.provider.api_key.is_empty(),"context_window":config.model.context_window}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProviderConnection {
    preset: String,
    provider: String,
    base_url: String,
    models_url: String,
    api_key: Option<String>,
}

pub(super) async fn connect_provider(
    State(state): State<AppState>,
    Json(update): Json<ProviderConnection>,
) -> ApiResult<Json<Value>> {
    let preset = enowx_core::provider_preset(&update.preset)
        .ok_or_else(|| ApiError::bad("Choose a supported provider"))?;
    if update.provider.trim().is_empty() || update.base_url.trim().is_empty() {
        return Err(ApiError::bad("Provider name and base URL are required"));
    }
    if preset.id != "custom"
        && (update.provider.trim() != preset.name
            || update.base_url.trim().trim_end_matches('/') != preset.base_url
            || update.models_url.trim().trim_end_matches('/') != preset.models_url)
    {
        return Err(ApiError::bad(
            "Built-in provider endpoints cannot be changed",
        ));
    }
    let mut config = state.config.write().await;
    let same_provider = config.provider.preset == preset.id
        && config.provider.name == update.provider.trim()
        && config.provider.base_url.trim_end_matches('/')
            == update.base_url.trim().trim_end_matches('/');
    if preset.id != "custom"
        && update
            .api_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
        && (!same_provider || config.provider.api_key.trim().is_empty())
    {
        return Err(ApiError::bad("Enter the provider API key"));
    }
    let mut next = config.clone();
    next.provider.preset = preset.id.into();
    next.provider.name = update.provider.trim().into();
    next.provider.base_url = update.base_url.trim().trim_end_matches('/').into();
    next.provider.models_url = update.models_url.trim().into();
    if let Some(key) = update.api_key {
        next.provider.api_key = key.trim().into();
    } else if !same_provider {
        next.provider.api_key.clear();
    }
    if !same_provider {
        next.model.default.clear();
        next.model.context_window = 128_000;
    }
    next.save()
        .map_err(|error| ApiError::bad(format!("{error:#}")))?;
    *config = next;
    Ok(Json(json!({"saved": true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfigUpdate {
    provider: String,
    preset: String,
    base_url: String,
    model: String,
    models_url: String,
    api_key: Option<String>,
    context_window: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelSelection {
    provider: String,
    base_url: String,
    model: String,
    context_window: Option<u32>,
    models_url: String,
}

pub(super) async fn select_model(
    State(state): State<AppState>,
    Json(update): Json<ModelSelection>,
) -> ApiResult<Json<Value>> {
    if update.model.trim().is_empty() {
        return Err(ApiError::bad("Select a model"));
    }
    let mut config = state.config.write().await;
    if !config.provider_active() {
        return Err(ApiError::bad("Connect a provider before selecting a model"));
    }
    if config.provider.name != update.provider.trim()
        || config.provider.base_url.trim_end_matches('/')
            != update.base_url.trim().trim_end_matches('/')
    {
        return Err(ApiError::bad("Provider changed; reload its model list"));
    }
    let mut next = config.clone();
    next.provider.models_url = update.models_url.trim().into();
    next.model.default = update.model.trim().into();
    if let Some(window) = update.context_window {
        if window == 0 {
            return Err(ApiError::bad("Context window must be greater than zero"));
        }
        next.model.context_window = window;
    }
    next.save()
        .map_err(|error| ApiError::bad(format!("{error:#}")))?;
    *config = next;
    Ok(Json(
        json!({"saved": true, "context_window": config.model.context_window}),
    ))
}

pub(super) async fn set_config(
    State(state): State<AppState>,
    Json(update): Json<ConfigUpdate>,
) -> ApiResult<Json<Value>> {
    if update.provider.trim().is_empty() || update.base_url.trim().is_empty() {
        return Err(ApiError::bad("Provider name and base URL are required"));
    }
    let preset = enowx_core::provider_preset(&update.preset)
        .ok_or_else(|| ApiError::bad("Choose a supported provider"))?;
    if preset.id != "custom"
        && (update.provider.trim() != preset.name
            || update.base_url.trim().trim_end_matches('/') != preset.base_url
            || update.models_url.trim().trim_end_matches('/') != preset.models_url)
    {
        return Err(ApiError::bad(
            "Built-in provider endpoints cannot be changed",
        ));
    }
    let mut config = state.config.write().await;
    if config.provider.preset != preset.id
        || config.provider.name != update.provider.trim()
        || config.provider.base_url.trim_end_matches('/')
            != update.base_url.trim().trim_end_matches('/')
    {
        return Err(ApiError::bad("Connect this provider before saving a model"));
    }
    if update.model.trim().is_empty() {
        return Err(ApiError::bad("Select a detected model or enter a model ID"));
    }
    let mut next = config.clone();
    next.provider.models_url = update.models_url.trim().into();
    next.model.default = update.model.trim().into();
    next.model.context_window = update.context_window;
    if let Some(key) = update.api_key.filter(|key| !key.trim().is_empty()) {
        next.provider.api_key = key.trim().into();
    }
    next.save()
        .map_err(|error| ApiError::bad(format!("{error:#}")))?;
    *config = next;
    Ok(Json(json!({"saved":true})))
}

pub(super) async fn models(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let config = state.config.read().await.clone();
    if !config.provider_active() {
        return Err(ApiError::bad(
            "Set an active provider before listing models",
        ));
    }
    if config.provider.models_url.trim().is_empty() {
        return Err(ApiError::bad(
            "Enter a model-list URL to auto-detect models",
        ));
    }
    let models = enowx_core::provider::Provider::from_config(&config)?
        .models(&config.provider.models_url)
        .await?;
    Ok(Json(json!({"models":models})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ModelDiscovery {
    provider: String,
    base_url: String,
    models_url: String,
    api_key: Option<String>,
    #[serde(default)]
    use_saved_key: bool,
}

pub(super) async fn discover_models(
    State(state): State<AppState>,
    Json(update): Json<ModelDiscovery>,
) -> ApiResult<Json<Value>> {
    if update.provider.trim().is_empty() || update.base_url.trim().is_empty() {
        return Err(ApiError::bad(
            "Set an active provider before detecting models",
        ));
    }
    if update.models_url.trim().is_empty() {
        return Err(ApiError::bad("Enter the model-list URL to detect models"));
    }
    let mut config = state.config.read().await.clone();
    let same_provider = config.provider.name == update.provider.trim()
        && config.provider.base_url.trim_end_matches('/')
            == update.base_url.trim().trim_end_matches('/');
    config.provider.name = update.provider.trim().into();
    config.provider.base_url = update.base_url.trim().trim_end_matches('/').into();
    config.provider.models_url = update.models_url.trim().into();
    if let Some(key) = update.api_key {
        config.provider.api_key = key;
    } else if !update.use_saved_key || !same_provider {
        config.provider.api_key.clear();
    }
    let models = enowx_core::provider::Provider::from_config(&config)?
        .models(&config.provider.models_url)
        .await
        .map_err(|error| ApiError::bad(format!("{error:#}")))?;
    Ok(Json(json!({"models": models})))
}
