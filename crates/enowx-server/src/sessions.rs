use super::*;

pub(super) async fn roles() -> Json<Value> {
    Json(
        json!({"roles":ROLES.iter().map(|role| json!({"name":role.id(),"title":role.label(),"summary":role.summary(),"tools":role.allowed_tools()})).collect::<Vec<_>>()}),
    )
}

pub(super) async fn tools() -> Json<Value> {
    Json(json!({"tools":ToolRegistry::default().schemas(Role::Orchestrator, None)}))
}

pub(super) async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"sessions":state.store.list(100)?})))
}

pub(super) async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    validate_id(&id)?;
    let session = state.store.load(&id)?;
    let running = state.runs.lock().await.contains_key(&id);
    Ok(Json(json!({"session":session,"running":running})))
}

pub(super) async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    validate_id(&id)?;
    let runs = state.runs.lock().await;
    if runs.contains_key(&id) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "stop this session before deleting it".into(),
        ));
    }
    state.store.delete(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChatRequest {
    session_id: Option<String>,
    message: String,
    #[serde(default)]
    role: Role,
    model: Option<String>,
}

pub(super) async fn chat(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> ApiResult<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>> {
    if request.message.trim().is_empty() {
        return Err(ApiError::bad("message is empty"));
    }
    let mut config = state.config.read().await.clone();
    if let Some(model) = request.model.filter(|model| !model.trim().is_empty()) {
        config.model.default = model;
    }
    if !config.is_ready() {
        return Err(ApiError::bad(
            "Configure a provider and API key in Providers before sending a message.",
        ));
    }
    let cancel = CancellationToken::new();
    let mut runs = state.runs.lock().await;
    let id = if let Some(id) = request.session_id {
        validate_id(&id)?;
        if runs.contains_key(&id) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "session is already running".into(),
            ));
        }
        state.store.load(&id)?;
        id
    } else {
        let mut session = Session::new(request.role);
        session.workspace = config.workspace();
        state.store.save(&session)?;
        session.id
    };
    runs.insert(id.clone(), cancel.clone());
    drop(runs);
    let (tx, rx) = mpsc::channel(256);
    let agent = Agent::with_store(config, state.store.clone());
    tokio::spawn(async move {
        let request = RunRequest {
            session_id: Some(id.clone()),
            prompt: request.message,
            role: request.role,
            attachments: Vec::new(),
        };
        if let Err(error) = agent.run(request, tx, cancel).await {
            tracing::warn!(%error, "turn failed");
        }
        state.runs.lock().await.remove(&id);
    });
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        let event: Event = rx.recv().await?;
        Some((
            Ok(SseEvent::default()
                .json_data(event)
                .expect("serializable event")),
            rx,
        ))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub(super) async fn interrupt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    validate_id(&id)?;
    let runs = state.runs.lock().await;
    if let Some(token) = runs.get(&id) {
        token.cancel();
    }
    Ok(Json(json!({"interrupted":runs.contains_key(&id)})))
}

fn validate_id(id: &str) -> ApiResult<()> {
    uuid::Uuid::parse_str(id).map_err(|_| ApiError::bad("invalid session id"))?;
    Ok(())
}
