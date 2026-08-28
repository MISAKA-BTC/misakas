//! The MISAKA Runtime API.
//!
//! Two surfaces on one port, and the split is deliberate:
//!
//! * **`/v1/…`** — OpenAI-compatible. Anything written against the OpenAI SDK points at this and
//!   works: `chat/completions`, `completions`, `models`, streaming included. This is the contract
//!   the Studio's own UI uses too, so the app is a first-class client of its own public API and
//!   cannot quietly depend on a private one.
//! * **`/api/v1/…`** — everything OpenAI has no concept of: which models are on disk, what they
//!   need, downloads, hardware, settings, provenance records.
//!
//! # Authentication
//!
//! There is none by default, because the default bind is loopback and a key on a single-user
//! desktop app is friction with no threat model. Setting `server.api_key` turns on a bearer check
//! over both surfaces — and binding to a non-loopback address without setting one is refused at
//! startup rather than served insecurely.

use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

pub mod management;
pub mod openai;

/// Build the whole application router.
pub fn router(state: Arc<AppState>, ui_dir: Option<PathBuf>, cors_origins: Vec<String>) -> axum::Router {
    let api = axum::Router::new()
        .merge(openai::router())
        .nest("/api/v1", management::router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(cors_layer(cors_origins))
        .with_state(state);

    match ui_dir {
        Some(dir) => api.fallback(get(move |req: Request| serve_ui(dir.clone(), req))),
        None => api.fallback(get(no_ui)),
    }
}

/// CORS.
///
/// The UI is same-origin, so nothing needs this by default. It exists for the case the API is
/// really for: a web page or another app on the same machine calling the local endpoint, which
/// the browser blocks without it.
fn cors_layer(origins: Vec<String>) -> CorsLayer {
    let layer = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    if origins.is_empty() {
        return layer.allow_origin(Any);
    }
    let parsed: Vec<_> = origins.iter().filter_map(|o| o.parse::<header::HeaderValue>().ok()).collect();
    layer.allow_origin(parsed)
}

/// Bearer-token check, applied only when a key is configured.
async fn authenticate(State(state): State<Arc<AppState>>, headers: HeaderMap, request: Request, next: Next) -> Response {
    let key = state.settings.read().await.server.api_key.clone();
    let Some(key) = key.filter(|k| !k.is_empty()) else { return next.run(request).await };

    // Health has to answer without a key: it is what a supervisor, a container probe and the
    // desktop shell's own "is the sidecar up yet" loop call, none of which hold credentials.
    if request.uri().path() == "/api/v1/health" {
        return next.run(request).await;
    }

    let presented =
        headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::trim);

    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), key.as_bytes()) => next.run(request).await,
        _ => crate::Error::bad_request("missing or invalid API key").into_response(),
    }
}

/// Compare without an early exit.
///
/// A `==` on a secret leaks its length and its matching prefix through timing. The threat is
/// modest for a loopback desktop app and the fix is six lines, which is the wrong ratio to argue
/// about.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Serve the built UI, with SPA fallback.
async fn serve_ui(dir: PathBuf, request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/').to_string();
    let candidate = if path.is_empty() { dir.join("index.html") } else { dir.join(&path) };

    // Path traversal guard: the resolved file must still be inside the UI directory. `..` in a
    // URL is otherwise a file-read primitive over the whole disk.
    let inside = match (std::fs::canonicalize(&candidate), std::fs::canonicalize(&dir)) {
        (Ok(file), Ok(root)) => file.starts_with(&root),
        _ => false,
    };

    if inside && candidate.is_file() {
        return file_response(&candidate).await;
    }
    // Unknown path: hand back index.html so client-side routes work on a refresh.
    let index = dir.join("index.html");
    if index.is_file() {
        return file_response(&index).await;
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

async fn file_response(path: &std::path::Path) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], bytes).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{}: {e}", path.display())).into_response(),
    }
}

/// What `/` says when the runtime is running without a UI bundle — the headless case, which is
/// legitimate: the API is the product for anyone pointing another app at it.
async fn no_ui() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        concat!(
            "<!doctype html><meta charset=utf-8><title>MISAKA Runtime</title>",
            "<body style=\"font-family:ui-sans-serif,system-ui;max-width:40rem;margin:4rem auto;line-height:1.6\">",
            "<h1>MISAKA Runtime</h1>",
            "<p>The runtime is up. No UI bundle is being served — start it with <code>--ui-dir</code>, ",
            "or use the API directly.</p>",
            "<ul><li><code>GET /api/v1/health</code></li><li><code>GET /v1/models</code></li>",
            "<li><code>POST /v1/chat/completions</code></li></ul></body>"
        ),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison_still_compares() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
        assert!(constant_time_eq(b"", b""));
    }
}
