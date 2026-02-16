//! Control UI — embedded static SPA assets served by the gateway.
//!
//! Uses `rust-embed` to bake the `ui/` directory into the binary.
//! In debug mode (`debug-embed` feature), files are read from disk
//! so you can edit JS/CSS and just refresh the browser.

use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/"]
struct UiAssets;

/// Build an axum `Router` that serves the embedded Control UI.
///
/// Register this **after** `/ws` and `/health` so those routes take priority
/// over the SPA catch-all.
pub fn ui_router() -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/{*path}", get(static_handler))
}

async fn index_handler() -> impl IntoResponse {
    serve_file("index.html")
}

async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    // Try the exact path first, then fall back to index.html for SPA routing
    if let Some(resp) = try_serve_file(&path) {
        resp
    } else {
        serve_file("index.html")
    }
}

fn try_serve_file(path: &str) -> Option<Response> {
    let asset = UiAssets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref())],
            asset.data.into_owned(),
        )
            .into_response(),
    )
}

fn serve_file(path: &str) -> Response {
    match UiAssets::get(path) {
        Some(asset) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                asset.data.into_owned(),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, Html("<h1>404</h1>")).into_response(),
    }
}
