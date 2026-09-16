use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderValue, StatusCode, Uri},
    middleware::{self, Next},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use enowx_core::{
    Agent, Config, Event, Role, RunRequest, Session, SessionStore, ToolRegistry, ROLES,
};
use futures::Stream;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

mod assets;
mod configuration;
mod security;
mod sessions;
use assets::static_asset;
use configuration::*;
use security::local_origin;
use sessions::*;

#[derive(Clone)]
struct AppState {
    config: Arc<RwLock<Config>>,
    store: SessionStore,
    runs: Arc<Mutex<HashMap<String, CancellationToken>>>,
    port: u16,
}

pub async fn serve(mut config: Config) -> Result<()> {
    let address: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .context("invalid server address")?;
    anyhow::ensure!(address.ip().is_loopback(), "This local tool server must bind to a loopback address; remote authentication is not configured.");
    config.agent.workspace =
        Some(std::fs::canonicalize(config.workspace()).context("resolving workspace")?);
    let state = AppState {
        port: config.server.port,
        config: Arc::new(RwLock::new(config)),
        store: SessionStore::default(),
        runs: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/config", get(get_config).put(set_config))
        .route("/api/providers", get(providers))
        .route("/api/provider", post(connect_provider))
        .route("/api/models", get(models).post(discover_models))
        .route("/api/model", post(select_model))
        .route("/api/roles", get(roles))
        .route("/api/tools", get(tools))
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session).delete(delete_session))
        .route("/api/chat", post(chat))
        .route("/api/chat/:id/interrupt", post(interrupt))
        .fallback(static_asset)
        .layer(middleware::from_fn_with_state(state.clone(), local_origin))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("binding {address}"))?;
    println!("enowx-cli ready at http://{address}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
struct ApiError(StatusCode, String);
type ApiResult<T> = std::result::Result<T, ApiError>;
impl ApiError {
    fn bad(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(error: E) -> Self {
        let error = error.into();
        let status = if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        Self(status, format!("{error:#}"))
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
