//! Embedded SPA (built by `build.rs` from `frontend/dist`) with history-API fallback.

use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$OUT_DIR/web"]
struct Web;

pub fn is_embedded() -> bool {
    env!("TRIAD_WEB_EMBEDDED") == "1"
}

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Web::get(path) {
        Some(file) => respond(path, file.data.into_owned(), path.starts_with("assets/")),
        None => match Web::get("index.html") {
            Some(index) => respond("index.html", index.data.into_owned(), false),
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        },
    }
}

fn respond(path: &str, data: Vec<u8>, immutable: bool) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if immutable { "public, max-age=31536000, immutable" } else { "no-cache" };
    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(data))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
