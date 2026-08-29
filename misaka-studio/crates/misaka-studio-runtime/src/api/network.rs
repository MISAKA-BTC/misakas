//! `/api/v1/network` — participation in the MISAKA network, as an API.
//!
//! The shape mirrors the ladder: what the chain offers (`/classes` — the mining class list),
//! what this machine is doing about it (`/` — role, node status, activity), and the two verbs
//! that change that (`/node/start`, `/node/stop`). Everything a button does here is also a
//! visible command line, because a person putting a bonded key on the line must be able to
//! reproduce — and audit — what ran without this app.

use crate::node::NodeView;
use crate::state::AppState;
use crate::{Error, Result};
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use misaka_studio_core::palw::{PalwArtifactSource, PalwClassStatus, TESTNET11_CLASSES, assess_classes};
use misaka_studio_core::settings::{NetworkRole, NodeNetwork};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(overview))
        .route("/classes", get(classes))
        .route("/classes/{name}/download", post(download_artifact))
        .route("/node/start", post(start_node))
        .route("/node/stop", post(stop_node))
        .route("/node/log", get(node_log))
}

/// The whole network picture in one response — what the UI's Network tab renders.
#[derive(Serialize)]
struct NetworkOverview {
    role: NetworkRole,
    network: NodeNetwork,
    node: NodeView,
    classes: Vec<PalwClassStatus>,
    /// True when this build of the Studio found a node binary it could launch.
    kaspad_found: bool,
    kaspad_path: String,
}

/// Scan the models directory for PALW artifacts (`.palwq36`, `.palwart`).
///
/// The same directory models live in, on purpose: it is the one place users already know, and
/// the GGUF scanner ignores these extensions so the two lists cannot contaminate each other.
async fn artifact_scan(state: &AppState) -> Vec<(String, String, u64)> {
    let dir = state.settings.read().await.models_dir.clone();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else { return out };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.ends_with(".palwq36") || name.ends_with(".palwart")) {
                continue;
            }
            if let Ok(meta) = entry.metadata()
                && meta.is_file()
            {
                out.push((entry.path().display().to_string(), name, meta.len()));
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}

async fn overview(State(state): State<Arc<AppState>>) -> Result<Json<NetworkOverview>> {
    let settings = state.settings.read().await.clone();
    let node = state.node.view(&settings.node).await?;
    let artifacts = artifact_scan(&state).await;
    let classes = assess_classes(&artifacts, state.hardware.total_memory);
    let kaspad = crate::node::NodeManager::resolve_kaspad(settings.node.kaspad_path.as_ref());
    Ok(Json(NetworkOverview {
        role: settings.node.role,
        network: settings.node.network,
        node,
        classes,
        kaspad_found: kaspad.is_file(),
        kaspad_path: kaspad.display().to_string(),
    }))
}

async fn classes(State(state): State<Arc<AppState>>) -> Json<Vec<PalwClassStatus>> {
    let artifacts = artifact_scan(&state).await;
    Json(assess_classes(&artifacts, state.hardware.total_memory))
}

/// Download a class artifact into the models directory, verified against the chain-pinned digest.
///
/// Only the classes whose artifact is published as a file (QWEN36) can be downloaded; a
/// convert-locally class answers 400 carrying the conversion command instead — an error that
/// tells the user the actual next step.
async fn download_artifact(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<crate::download::DownloadProgress>> {
    let spec = TESTNET11_CLASSES
        .iter()
        .find(|class| class.name.eq_ignore_ascii_case(&name))
        .ok_or_else(|| Error::bad_request(format!("no PALW class named '{name}'")))?;

    match &spec.artifact {
        PalwArtifactSource::Download { filename, sha256, size_bytes, hf_repo, .. } => {
            let settings = state.settings.read().await.clone();
            let catalog = state.catalog().await;
            let progress = state
                .downloads
                .start(
                    &catalog,
                    state.store.clone(),
                    settings.models_dir.clone(),
                    hf_repo.to_string(),
                    // The artifact is pinned by content digest, so `main` is safe here in a way
                    // it is not for models: a moved branch cannot change what verifies.
                    "main".to_string(),
                    filename.to_string(),
                    Some(sha256.to_string()),
                    Some(*size_bytes),
                    None,
                )
                .await?;
            Ok(Json(progress))
        }
        PalwArtifactSource::ConvertLocally { convert_command, source_repo, .. } => Err(Error::bad_request(format!(
            "{} has no published download — convert it locally from {source_repo}: `{convert_command}` (in the misakas repository), then place the output in the models directory",
            spec.name
        ))),
        PalwArtifactSource::DerivedFromSeed => {
            Err(Error::bad_request(format!("{} needs no artifact — every node derives it from a seed", spec.name)))
        }
    }
}

#[derive(Deserialize)]
struct StartBody {
    /// Override the configured role for this launch, e.g. start as verifier while producer
    /// prerequisites are still being gathered.
    #[serde(default)]
    role: Option<NetworkRole>,
}

async fn start_node(State(state): State<Arc<AppState>>, body: Option<Json<StartBody>>) -> Result<Json<NodeView>> {
    let mut node_settings = state.settings.read().await.node.clone();
    if let Some(Json(StartBody { role: Some(role) })) = body {
        node_settings.role = role;
    }
    Ok(Json(state.node.start(&node_settings).await?))
}

async fn stop_node(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>> {
    state.node.stop().await?;
    Ok(Json(serde_json::json!({ "stopped": true })))
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default = "default_log_limit")]
    limit: usize,
}

fn default_log_limit() -> usize {
    200
}

async fn node_log(State(state): State<Arc<AppState>>, Query(query): Query<LogQuery>) -> Json<Vec<String>> {
    Json(state.node.recent_log(query.limit.min(600)))
}
