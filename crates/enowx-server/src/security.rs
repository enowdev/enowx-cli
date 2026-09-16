use super::*;

// Prevent another website from driving local filesystem/shell tools, including DNS rebinding.
pub(super) async fn local_origin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let authorities = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
        format!("[::1]:{}", state.port),
    ];
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !authorities.iter().any(|allowed| allowed == host) {
        return (StatusCode::FORBIDDEN, "invalid local host").into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        let same = authorities
            .iter()
            .any(|host| origin == format!("http://{host}"));
        let dev = origin == "http://localhost:5173" || origin == "http://127.0.0.1:5173";
        if !same && !dev {
            return (StatusCode::FORBIDDEN, "cross-origin access refused").into_response();
        }
    }
    next.run(request).await
}
