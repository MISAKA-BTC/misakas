//! The Studio's own API: models on disk, the catalog, downloads, hardware, settings, provenance.
//!
//! Everything the UI needs that OpenAI has no vocabulary for. Two conventions run through it:
//!
//! * **A model is never returned as a bare file record.** Every model view carries its memory
//!   requirement at the context it would actually load with, and the verdict for *this* machine.
//!   The UI should not be re-deriving "will this fit" from a size in bytes; the answer is
//!   arithmetic over hardware facts, and it belongs next to the facts.
//! * **Long-running work is a resource, not a request.** Downloads and metrics are started or
//!   subscribed to, then observed over SSE. A ten-minute HTTP request is a ten-minute chance to
//!   lose everything to one dropped connection.

use crate::backend::Availability;
use crate::download::DownloadProgress;
use crate::records::StoredRecord;
use crate::state::{AppState, RuntimeStatus};
use crate::{Error, Result};
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use misaka_studio_core::HardwareSnapshot;
use misaka_studio_core::model::{FitVerdict, LocalModel, ModelRequirements};
use misaka_studio_core::provenance::ModelIdentity;
use misaka_studio_core::settings::Settings;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/system", get(system))
        .route("/metrics", get(metrics_now))
        .route("/metrics/stream", get(metrics_stream))
        .route("/models", get(list_models))
        .route("/models/refresh", post(refresh_models))
        .route("/models/unload", post(unload))
        .route("/models/{id}", get(get_model).delete(delete_model))
        .route("/models/{id}/load", post(load_model))
        .route("/models/{id}/hash", post(hash_model))
        .route("/runtime", get(runtime_status))
        .route("/runtime/backends", get(backends))
        .route("/catalog/search", get(search))
        .route("/catalog/repo/{*repo}", get(repo))
        .route("/downloads", get(list_downloads).post(start_download))
        .route("/downloads/stream", get(downloads_stream))
        .route("/downloads/{*id}", delete(cancel_download))
        .route("/settings", get(get_settings).route_layer(axum::middleware::from_fn(pass)).put(put_settings))
        .route("/settings/reset", post(reset_settings))
        .route("/records", get(list_records))
        .route("/records/{id}", get(get_record))
}

/// A no-op layer, present so the settings route reads the same as the others.
async fn pass(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    next.run(request).await
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
    name: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok", version: env!("CARGO_PKG_VERSION"), name: "misaka-studio-runtime" })
}

#[derive(Serialize)]
struct SystemInfo {
    hardware: HardwareSnapshot,
    data_dir: String,
    models_dir: String,
    records_path: String,
    /// The endpoint the catalog talks to — worth surfacing, because a mirror or a blocked host is
    /// the most common reason search returns nothing.
    catalog_endpoint: String,
}

async fn system(State(state): State<Arc<AppState>>) -> Json<SystemInfo> {
    let settings = state.settings.read().await;
    Json(SystemInfo {
        hardware: state.hardware.clone(),
        data_dir: state.data_dir.display().to_string(),
        models_dir: settings.models_dir.display().to_string(),
        records_path: state.records.read().await.path().display().to_string(),
        catalog_endpoint: settings.huggingface.endpoint.clone(),
    })
}

async fn metrics_now(State(state): State<Arc<AppState>>) -> Json<crate::metrics::RuntimeSample> {
    Json(state.metrics.sample().await)
}

/// Live metrics over SSE.
async fn metrics_stream(State(state): State<Arc<AppState>>) -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.metrics.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|sample| async move {
        // A lagged receiver has missed samples. Skipping them is right for a gauge: the next
        // tick is a quarter of a second away and carries the current value.
        let sample = sample.ok()?;
        Some(Ok(Event::default().data(serde_json::to_string(&sample).ok()?)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// A model, with what it needs and whether it fits.
#[derive(Serialize)]
pub struct ModelView {
    #[serde(flatten)]
    pub model: LocalModel,
    pub recommended_context: u64,
    pub requirements: ModelRequirements,
    pub fit: FitVerdict,
    pub fit_summary: String,
    /// Present once the file has been hashed.
    pub identity: Option<ModelIdentity>,
}

fn view(model: &LocalModel, hardware: &HardwareSnapshot) -> ModelView {
    let recommended_context = model.recommended_context(hardware);
    let requirements = model.requirements(recommended_context);
    let fit = FitVerdict::assess(&requirements, hardware);
    ModelView {
        recommended_context,
        requirements,
        fit_summary: fit.summary(),
        fit,
        identity: model.identity(),
        model: model.clone(),
    }
}

async fn list_models(State(state): State<Arc<AppState>>) -> Json<Vec<ModelView>> {
    let models = state.store.list().await;
    Json(models.iter().map(|m| view(m, &state.hardware)).collect())
}

async fn refresh_models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<ModelView>>> {
    let models = state.store.refresh().await?;
    Ok(Json(models.iter().map(|m| view(m, &state.hardware)).collect()))
}

async fn get_model(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<ModelView>> {
    let model = state.store.require(&id).await?;
    Ok(Json(view(&model, &state.hardware)))
}

async fn delete_model(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    // Unload first when it is the loaded one: deleting a file an engine has mapped leaves the
    // engine holding a deleted inode on Unix and fails outright on Windows.
    if state.loaded().await.map(|s| s.model.id) == Some(id.clone()) {
        state.unload().await?;
    }
    state.store.delete(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[derive(Deserialize)]
struct LoadBody {
    #[serde(default)]
    context_size: Option<u32>,
}

async fn load_model(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<LoadBody>>,
) -> Result<Json<RuntimeStatus>> {
    let context = body.and_then(|Json(b)| b.context_size);
    Ok(Json(state.load(&id, context).await?))
}

async fn unload(State(state): State<Arc<AppState>>) -> Result<Json<RuntimeStatus>> {
    state.unload().await?;
    Ok(Json(state.status().await))
}

/// Hash a model and return its chain-compatible identity.
///
/// Explicit, and its own endpoint, because it reads the entire file: on a 40 GB model this is a
/// minute of disk, and it must be something the user asks for rather than something a model list
/// does forty times.
async fn hash_model(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<ModelView>> {
    let model = state.store.ensure_hashed(&id).await?;
    if state.loaded().await.map(|s| s.model.id) == Some(id) {
        state.resolve_identity().await?;
    }
    Ok(Json(view(&model, &state.hardware)))
}

async fn runtime_status(State(state): State<Arc<AppState>>) -> Json<RuntimeStatus> {
    Json(state.status().await)
}

#[derive(Serialize)]
struct BackendInfo {
    name: String,
    selected: bool,
    availability: Availability,
}

/// Which engines this machine could use. What the Settings screen lists.
async fn backends(State(state): State<Arc<AppState>>) -> Json<Vec<BackendInfo>> {
    let settings = state.settings.read().await.clone();
    let selected = state.backend().await;
    let mut out = Vec::new();
    for backend in [
        crate::state::build_backend(&Settings { backend: misaka_studio_core::settings::BackendSettings { kind: misaka_studio_core::settings::BackendKind::LlamaCpp, ..settings.backend.clone() }, ..settings.clone() }, &state.hardware),
        crate::state::build_backend(&Settings { backend: misaka_studio_core::settings::BackendSettings { kind: misaka_studio_core::settings::BackendKind::Mlx, ..settings.backend.clone() }, ..settings.clone() }, &state.hardware),
        crate::state::build_backend(&Settings { backend: misaka_studio_core::settings::BackendSettings { kind: misaka_studio_core::settings::BackendKind::Mock, ..settings.backend.clone() }, ..settings.clone() }, &state.hardware),
    ] {
        out.push(BackendInfo {
            selected: backend.name() == selected.name(),
            availability: backend.availability().await,
            name: backend.name().to_string(),
        });
    }
    Json(out)
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

async fn search(State(state): State<Arc<AppState>>, Query(query): Query<SearchQuery>) -> Result<Json<Vec<crate::catalog::CatalogEntry>>> {
    let catalog = state.catalog().await;
    Ok(Json(catalog.search(&query.q, query.limit).await?))
}

#[derive(Deserialize)]
struct RevisionQuery {
    #[serde(default)]
    revision: Option<String>,
}

async fn repo(
    State(state): State<Arc<AppState>>,
    Path(repo): Path<String>,
    Query(query): Query<RevisionQuery>,
) -> Result<Json<crate::catalog::CatalogRepo>> {
    let catalog = state.catalog().await;
    Ok(Json(catalog.repo(&repo, query.revision.as_deref()).await?))
}

async fn list_downloads(State(state): State<Arc<AppState>>) -> Json<Vec<DownloadProgress>> {
    Json(state.downloads.list().await)
}

#[derive(Deserialize)]
struct StartDownload {
    repo: String,
    #[serde(default)]
    revision: Option<String>,
    file: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    base_model: Option<String>,
}

async fn start_download(State(state): State<Arc<AppState>>, Json(body): Json<StartDownload>) -> Result<Json<DownloadProgress>> {
    let settings = state.settings.read().await.clone();
    let catalog = state.catalog().await;

    // Resolve the revision when the caller did not pin one: a download recorded against `main`
    // names a branch that moves, and `h_M` derived from it would not identify the bytes.
    let (revision, sha256, size, base_model) = match (&body.revision, &body.sha256) {
        (Some(rev), Some(sha)) => (rev.clone(), Some(sha.clone()), body.size, body.base_model.clone()),
        _ => {
            let info = catalog.repo(&body.repo, body.revision.as_deref()).await?;
            let file = info.files.iter().find(|f| f.path == body.file);
            (
                info.revision.clone().or_else(|| body.revision.clone()).unwrap_or_else(|| "main".into()),
                body.sha256.clone().or_else(|| file.and_then(|f| f.sha256.clone())),
                body.size.or_else(|| file.and_then(|f| f.size)),
                body.base_model.clone().or(info.base_model),
            )
        }
    };

    let progress = state
        .downloads
        .start(
            &catalog,
            state.store.clone(),
            settings.models_dir.clone(),
            body.repo,
            revision,
            body.file,
            sha256,
            size,
            base_model,
        )
        .await?;
    Ok(Json(progress))
}

async fn cancel_download(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    state.downloads.cancel(&id).await?;
    Ok(Json(serde_json::json!({ "cancelling": id })))
}

async fn downloads_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.downloads.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|progress| async move {
        let progress = progress.ok()?;
        Some(Ok(Event::default().data(serde_json::to_string(&progress).ok()?)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_settings(State(state): State<Arc<AppState>>) -> Json<Settings> {
    Json(state.settings.read().await.clone())
}

async fn put_settings(State(state): State<Arc<AppState>>, Json(new): Json<Settings>) -> Result<Json<Settings>> {
    // The one setting that can lock the user out of their own app: a public bind with no key
    // would serve unauthenticated inference to the network on the next start.
    if new.server.requires_api_key() {
        return Err(Error::bad_request(
            "binding to a non-loopback address requires server.api_key — otherwise anyone on the network can use this model",
        ));
    }
    Ok(Json(state.apply_settings(new).await?))
}

async fn reset_settings(State(state): State<Arc<AppState>>) -> Result<Json<Settings>> {
    Ok(Json(state.apply_settings(Settings::default()).await?))
}

#[derive(Deserialize)]
struct RecordsQuery {
    #[serde(default = "default_records_limit")]
    limit: usize,
}

fn default_records_limit() -> usize {
    50
}

async fn list_records(State(state): State<Arc<AppState>>, Query(query): Query<RecordsQuery>) -> Json<Vec<StoredRecord>> {
    let records = state.records.read().await.clone();
    Json(records.list(query.limit).await)
}

async fn get_record(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<StoredRecord>> {
    let records = state.records.read().await.clone();
    records.get(&id).await.map(Json).ok_or_else(|| Error::bad_request(format!("no record with id '{id}'")))
}
