use super::*;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;
pub(super) async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"unknown API route"})),
        )
            .into_response();
    }
    let requested = if path.is_empty() { "index.html" } else { path };
    let (asset, mime) = if let Some(asset) = Assets::get(requested) {
        (
            Some(asset),
            mime_guess::from_path(requested)
                .first_or_octet_stream()
                .to_string(),
        )
    } else if !requested.contains('.') {
        (Assets::get("index.html"), "text/html; charset=utf-8".into())
    } else {
        (None, String::new())
    };
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = Response::new(Body::from(asset.data));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).expect("valid mime"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
