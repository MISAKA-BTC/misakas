//! Everything the server holds, and the two decisions it makes on the user's behalf:
//! **which backend** and **how many layers on the GPU**.
//!
//! Both are "Auto" by default, and both are the sort of automatic that has to be explainable —
//! the app reports what it chose and why, because a person whose model is unexpectedly slow
//! needs to see "23 of 33 layers offloaded, VRAM was the limit" rather than a spinner.

use crate::backend::llamacpp::{LlamaCppBackend, accelerator_tag};
use crate::backend::misaka::MisakaBackend;
use crate::backend::mlx::MlxBackend;
use crate::backend::mock::MockBackend;
use crate::backend::{ChatMessage, GenerationRequest, LoadRequest, LoadedModel, SharedBackend, StreamEvent, Usage};
use crate::catalog::Catalog;
use crate::download::DownloadManager;
use crate::metrics::MetricsHub;
use crate::records::{RecordStore, StoredRecord};
use crate::store::ModelStore;
use crate::{Error, Result};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use misaka_studio_core::HardwareSnapshot;
use misaka_studio_core::model::LocalModel;
use misaka_studio_core::provenance::{
    InferenceInputs, InferenceRecord, ModelIdentity, RuntimeIdentity, SamplingCommitment, canonical_prompt_bytes,
    canonical_raw_prompt_bytes,
};
use misaka_studio_core::settings::{BackendKind, GpuLayers, Settings};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// A model that is loaded, with everything provenance needs about it.
#[derive(Clone)]
pub struct LoadedState {
    pub model: LocalModel,
    pub loaded: LoadedModel,
    pub runtime: RuntimeIdentity,
    /// `None` until the file has been hashed.
    pub identity: Option<ModelIdentity>,
    pub backend: String,
}

/// What the UI shows about the current runtime.
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatus {
    pub backend: String,
    pub backend_available: bool,
    pub model_id: Option<String>,
    pub context_size: Option<u32>,
    pub gpu_layers: Option<u32>,
    pub load_ms: Option<u64>,
    pub runtime_hash: Option<String>,
    pub runtime_class_id: Option<String>,
    pub model_hash: Option<String>,
    pub descriptor: Option<misaka_studio_core::provenance::RuntimeDescriptor>,
}

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub data_dir: PathBuf,
    pub hardware: HardwareSnapshot,
    pub store: Arc<ModelStore>,
    pub downloads: Arc<DownloadManager>,
    pub metrics: Arc<MetricsHub>,
    pub records: RwLock<Arc<RecordStore>>,
    catalog: RwLock<Arc<Catalog>>,
    backend: RwLock<SharedBackend>,
    loaded: RwLock<Option<LoadedState>>,
}

impl AppState {
    pub async fn new(settings: Settings, settings_path: PathBuf, data_dir: PathBuf) -> Arc<Self> {
        let hardware = HardwareSnapshot::probe();
        let store = Arc::new(ModelStore::new(vec![settings.models_dir.clone()]));
        if let Err(e) = store.refresh().await {
            tracing::warn!("initial model scan failed: {e}");
        }
        let records = RecordStore::open(
            data_dir.join("inference-records.jsonl"),
            settings.provenance.max_records,
            settings.provenance.record_inferences,
        )
        .await;
        if let Err(e) = records.trim().await {
            tracing::warn!("could not trim the record log: {e}");
        }
        let catalog = Arc::new(Catalog::new(settings.huggingface.endpoint.clone(), settings.huggingface.token.clone()));
        let backend = build_backend(&settings, &hardware);
        let metrics = MetricsHub::new(&hardware);

        Arc::new(AppState {
            settings: RwLock::new(settings),
            settings_path,
            data_dir,
            hardware,
            store,
            downloads: Arc::new(DownloadManager::new()),
            metrics,
            records: RwLock::new(records),
            catalog: RwLock::new(catalog),
            backend: RwLock::new(backend),
            loaded: RwLock::new(None),
        })
    }

    pub async fn catalog(&self) -> Arc<Catalog> {
        self.catalog.read().await.clone()
    }

    pub async fn backend(&self) -> SharedBackend {
        self.backend.read().await.clone()
    }

    pub async fn loaded(&self) -> Option<LoadedState> {
        self.loaded.read().await.clone()
    }

    /// Apply new settings: persist them, then rebuild whatever they changed.
    ///
    /// Changing the backend or the model directory unloads the current model. That is the honest
    /// behaviour — the loaded model may not exist under the new directory, and it certainly is
    /// not loaded in the new engine — and it is stated in the API response rather than left for
    /// the user to discover when generation fails.
    pub async fn apply_settings(&self, new: Settings) -> Result<Settings> {
        let old = self.settings.read().await.clone();
        new.save(&self.settings_path)?;

        let backend_changed = new.backend.kind != old.backend.kind
            || new.backend.llama_server_path != old.backend.llama_server_path
            || new.backend.mlx_server_path != old.backend.mlx_server_path;
        let models_dir_changed = new.models_dir != old.models_dir;
        let hub_changed = new.huggingface.endpoint != old.huggingface.endpoint || new.huggingface.token != old.huggingface.token;
        let recording_changed = new.provenance.record_inferences != old.provenance.record_inferences
            || new.provenance.max_records != old.provenance.max_records;

        if backend_changed {
            self.unload().await?;
            *self.backend.write().await = build_backend(&new, &self.hardware);
        }
        if models_dir_changed {
            self.store.set_roots(vec![new.models_dir.clone()]).await?;
        }
        if hub_changed {
            *self.catalog.write().await = Arc::new(Catalog::new(new.huggingface.endpoint.clone(), new.huggingface.token.clone()));
        }
        if recording_changed {
            *self.records.write().await = RecordStore::open(
                self.data_dir.join("inference-records.jsonl"),
                new.provenance.max_records,
                new.provenance.record_inferences,
            )
            .await;
        }

        *self.settings.write().await = new.clone();
        Ok(new)
    }

    /// Load a model into the current backend.
    pub async fn load(&self, model_id: &str, context_override: Option<u32>) -> Result<RuntimeStatus> {
        let model = self.store.require(model_id).await?;
        let settings = self.settings.read().await.clone();
        let backend = self.backend().await;

        let availability = backend.availability().await;
        if let crate::backend::Availability::Unavailable { reason, remedy } = availability {
            return Err(Error::BackendUnavailable { backend: backend.name().to_string(), reason, remedy });
        }

        let context_size =
            context_override.or(settings.generation.context_size).unwrap_or_else(|| model.recommended_context(&self.hardware) as u32);
        let gpu_layers = plan_gpu_layers(&model, &self.hardware, context_size as u64, settings.backend.gpu_layers);

        let loaded = backend
            .load(LoadRequest {
                model_id: model.id.clone(),
                model_path: model.path.clone(),
                context_size,
                gpu_layers,
                threads: settings.backend.threads,
                flash_attention: settings.backend.flash_attention,
                use_mmap: settings.backend.use_mmap,
                use_mlock: settings.backend.use_mlock,
                // The header already told us; the engine has no way to guess.
                needs_default_chat_template: !model.has_chat_template,
                extra_args: settings.backend.extra_args.clone(),
            })
            .await?;

        let runtime = RuntimeIdentity::derive(backend.descriptor().await);
        // Hashing is deliberately not done here: it would add a minute to every load of a large
        // model. The identity fills in the first time provenance is asked for.
        let identity = model.identity();
        let state = LoadedState { model, loaded, runtime, identity, backend: backend.name().to_string() };
        *self.loaded.write().await = Some(state.clone());
        Ok(self.status_from(Some(&state), true).await)
    }

    pub async fn unload(&self) -> Result<()> {
        let backend = self.backend().await;
        backend.unload().await?;
        *self.loaded.write().await = None;
        Ok(())
    }

    /// Compute and cache the model identity for the loaded model, hashing the file if needed.
    pub async fn resolve_identity(&self) -> Result<Option<ModelIdentity>> {
        let Some(state) = self.loaded().await else { return Ok(None) };
        if let Some(identity) = state.identity {
            return Ok(Some(identity));
        }
        let hashed = self.store.ensure_hashed(&state.model.id).await?;
        let identity = hashed.identity();
        if let Some(slot) = self.loaded.write().await.as_mut() {
            slot.model = hashed;
            slot.identity = identity.clone();
        }
        Ok(identity)
    }

    pub async fn status(&self) -> RuntimeStatus {
        let loaded = self.loaded().await;
        let backend = self.backend().await;
        let available = backend.availability().await.is_available();
        self.status_from(loaded.as_ref(), available).await
    }

    async fn status_from(&self, state: Option<&LoadedState>, available: bool) -> RuntimeStatus {
        let backend = self.backend().await;
        match state {
            Some(s) => RuntimeStatus {
                backend: s.backend.clone(),
                backend_available: available,
                model_id: Some(s.model.id.clone()),
                context_size: Some(s.loaded.context_size),
                gpu_layers: s.loaded.gpu_layers,
                load_ms: Some(s.loaded.load_ms),
                runtime_hash: Some(s.runtime.h_r.to_hex()),
                runtime_class_id: Some(s.runtime.class_id.to_hex()),
                model_hash: s.identity.as_ref().map(|i| i.h_m.to_hex()),
                descriptor: Some(s.runtime.descriptor.clone()),
            },
            None => RuntimeStatus {
                backend: backend.name().to_string(),
                backend_available: available,
                model_id: None,
                context_size: None,
                gpu_layers: None,
                load_ms: None,
                runtime_hash: None,
                runtime_class_id: None,
                model_hash: None,
                descriptor: None,
            },
        }
    }

    /// Generate, with metrics and provenance attached.
    ///
    /// The returned stream is the backend's, wrapped: text is accumulated as it passes so the
    /// completion can be committed to, and the record is written when the stream ends. Wrapping
    /// rather than buffering matters — the user sees tokens as they arrive, and the record still
    /// covers the whole answer.
    pub async fn generate(
        self: &Arc<Self>,
        messages: Vec<ChatMessage>,
        prompt: Option<String>,
        params: SamplingCommitment,
        stop: Vec<String>,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let state = self.loaded().await.ok_or(Error::NoModelLoaded)?;
        let backend = self.backend().await;

        // The bytes the record commits to. Canonical and length-prefixed — see
        // `canonical_prompt_bytes`, which exists because the obvious `role: content` flattening
        // lets two different conversations produce the same commitment.
        let prompt_bytes = match &prompt {
            Some(raw) => canonical_raw_prompt_bytes(raw),
            None => {
                let pairs: Vec<(&str, &str)> = messages.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
                canonical_prompt_bytes(&pairs)
            }
        };

        let request = GenerationRequest { model: state.model.id.clone(), messages, prompt, params, stop };

        self.metrics.generation_started();
        let inner = match backend.generate(request).await {
            Ok(stream) => stream,
            Err(e) => {
                // The counter must come back down on the failure path too, or "1 generation
                // active" sticks forever after one bad request.
                self.metrics.generation_finished(0, 0.0, 0);
                return Err(e);
            }
        };

        let app = self.clone();
        let started = Instant::now();
        let started_at_unix_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            let mut inner = inner;
            let mut text = String::new();
            let mut first_token: Option<Duration> = None;
            let mut usage = Usage::default();

            while let Some(event) = inner.next().await {
                match &event {
                    Ok(StreamEvent::Delta(delta)) => {
                        if first_token.is_none() {
                            first_token = Some(started.elapsed());
                        }
                        text.push_str(delta);
                    }
                    Ok(StreamEvent::Done { usage: u, .. }) => usage = *u,
                    Err(_) => {}
                }
                if tx.send(event).await.is_err() {
                    break; // client hung up
                }
            }

            let duration_ms = started.elapsed().as_millis() as u64;
            let ttft = first_token.map(|d| d.as_millis() as u64);
            let tps = if duration_ms > 0 { usage.completion_tokens as f64 * 1000.0 / duration_ms as f64 } else { 0.0 };
            app.metrics.generation_finished(usage.completion_tokens, tps, ttft.unwrap_or(0));
            app.record(&state, &prompt_bytes, &text, usage, started_at_unix_ms, duration_ms, ttft, params).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(
        &self,
        state: &LoadedState,
        prompt: &[u8],
        completion: &str,
        usage: Usage,
        started_at_unix_ms: u64,
        duration_ms: u64,
        time_to_first_token_ms: Option<u64>,
        params: SamplingCommitment,
    ) {
        let records = self.records.read().await.clone();
        if !records.is_enabled() {
            return;
        }
        let keep_transcripts = self.settings.read().await.provenance.keep_transcripts;
        // Use the identity if it is already known; do not hash a 40 GB file on the completion
        // path. `model: None` then says plainly that this run is not attributed to an artifact.
        let identity = state.identity.clone();
        let record = InferenceRecord::new(
            uuid::Uuid::new_v4().to_string(),
            InferenceInputs {
                model: identity.as_ref(),
                runtime: &state.runtime,
                params,
                prompt,
                output: completion.as_bytes(),
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                started_at_unix_ms,
                duration_ms,
                time_to_first_token_ms,
            },
        );
        records
            .append(StoredRecord {
                record,
                // The transcript is the readable text, not the canonical commitment bytes:
                // a person auditing this log wants the conversation, and the commitment is
                // already the hash beside it.
                prompt: keep_transcripts.then(|| String::from_utf8_lossy(prompt).into_owned()),
                completion: keep_transcripts.then(|| completion.to_string()),
                model_id: Some(state.model.id.clone()),
            })
            .await;
    }
}

/// Build the backend a settings value asks for.
pub fn build_backend(settings: &Settings, hardware: &HardwareSnapshot) -> SharedBackend {
    let timeout = Duration::from_secs(settings.backend.startup_timeout_secs);
    let tag = accelerator_tag(hardware);
    match settings.backend.kind {
        BackendKind::Mock => Arc::new(MockBackend::default()),
        BackendKind::Mlx => Arc::new(MlxBackend::new(settings.backend.mlx_server_path.clone(), timeout)),
        BackendKind::LlamaCpp => Arc::new(LlamaCppBackend::new(settings.backend.llama_server_path.clone(), tag, timeout)),
        // Reserved, and refused rather than silently substituted: a user who selected the MISAKA
        // runtime must not be given llama.cpp under a record that names MISAKA.
        BackendKind::Misaka => Arc::new(MisakaBackend),
        // Auto: MLX where it can run, llama.cpp everywhere else. MLX is chosen only on Apple
        // Silicon, and only when its server is actually installed — the check happens at load,
        // where a missing engine is reported with a remedy.
        BackendKind::Auto => {
            if MlxBackend::platform_supported() && settings.backend.mlx_server_path.is_some() {
                Arc::new(MlxBackend::new(settings.backend.mlx_server_path.clone(), timeout))
            } else {
                Arc::new(LlamaCppBackend::new(settings.backend.llama_server_path.clone(), tag, timeout))
            }
        }
    }
}

/// How many layers to put on the accelerator.
///
/// The arithmetic is the same as the fit estimate: the accelerator's budget less the KV cache and
/// the compute overhead, divided by the per-layer weight size. What is left is what can be
/// offloaded, and offloading one layer more is an out-of-memory error at load time — the failure
/// this function exists to avoid.
pub fn plan_gpu_layers(model: &LocalModel, hardware: &HardwareSnapshot, context: u64, setting: GpuLayers) -> Option<u32> {
    let total_layers = model.block_count.unwrap_or(0) as u32;
    match setting {
        GpuLayers::None => return Some(0),
        // 999 is llama.cpp's idiom for "all of them", and it is right even when the layer count
        // is unknown.
        GpuLayers::All => return Some(if total_layers > 0 { total_layers + 1 } else { 999 }),
        GpuLayers::Fixed { layers } => return Some(layers),
        GpuLayers::Auto => {}
    }

    if !hardware.has_gpu() {
        return Some(0);
    }
    let budget = hardware
        .accelerators
        .iter()
        .filter(|a| a.kind != misaka_studio_core::hardware::AcceleratorKind::Cpu)
        .filter_map(|a| a.usable_memory)
        .max()?;

    let requirements = model.requirements(context);
    if requirements.total_bytes <= budget {
        return Some(if total_layers > 0 { total_layers + 1 } else { 999 });
    }
    if total_layers == 0 {
        // No layer count and it does not all fit: let the engine decide rather than guess a
        // number that could be far too high.
        return None;
    }
    let per_layer = (requirements.weights_bytes / total_layers as u64).max(1);
    let for_weights = budget.saturating_sub(requirements.kv_cache_bytes).saturating_sub(requirements.overhead_bytes);
    Some(((for_weights / per_layer) as u32).min(total_layers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_studio_core::hardware::{Accelerator, AcceleratorKind};
    use misaka_studio_core::model::ModelSource;

    fn model(size_gb: u64, layers: u64) -> LocalModel {
        LocalModel {
            id: "m".into(),
            name: "m".into(),
            path: PathBuf::from("/models/m.gguf"),
            size_bytes: size_gb << 30,
            quantization: None,
            architecture: Some("llama".into()),
            parameter_count: None,
            context_length: Some(32768),
            block_count: Some(layers),
            expert_count: None,
            kv_cache_bytes_per_token: Some(128 << 10),
            has_chat_template: true,
            source: ModelSource::default(),
            sha256: None,
            modified_at: None,
        }
    }

    fn machine(ram_gb: u64, vram_gb: Option<u64>) -> HardwareSnapshot {
        HardwareSnapshot {
            os: "test".into(),
            arch: "x86_64".into(),
            cpu_name: "cpu".into(),
            physical_cores: Some(8),
            logical_cores: 16,
            total_memory: ram_gb << 30,
            available_memory: ram_gb << 30,
            accelerators: vram_gb
                .map(|v| Accelerator {
                    kind: AcceleratorKind::Cuda,
                    name: "GPU".into(),
                    total_memory: Some(v << 30),
                    free_memory: Some(v << 30),
                    usable_memory: Some(v << 30),
                    driver: None,
                    index: 0,
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn a_model_that_fits_is_fully_offloaded() {
        let layers = plan_gpu_layers(&model(8, 32), &machine(64, Some(24)), 4096, GpuLayers::Auto);
        assert_eq!(layers, Some(33), "every layer plus the output tensor");
    }

    /// The case the whole function exists for: too big for the card, so some layers stay on the
    /// CPU. Offloading them all would be an out-of-memory error at load.
    #[test]
    fn a_model_that_does_not_fit_is_split() {
        let layers = plan_gpu_layers(&model(40, 60), &machine(128, Some(24)), 4096, GpuLayers::Auto).expect("a plan");
        assert!(layers > 0 && layers < 60, "expected a partial offload, got {layers}");
    }

    #[test]
    fn without_a_gpu_nothing_is_offloaded() {
        assert_eq!(plan_gpu_layers(&model(8, 32), &machine(32, None), 4096, GpuLayers::Auto), Some(0));
    }

    #[test]
    fn explicit_settings_win_over_the_estimate() {
        let m = model(40, 60);
        let h = machine(128, Some(24));
        assert_eq!(plan_gpu_layers(&m, &h, 4096, GpuLayers::All), Some(61));
        assert_eq!(plan_gpu_layers(&m, &h, 4096, GpuLayers::None), Some(0));
        assert_eq!(plan_gpu_layers(&m, &h, 4096, GpuLayers::Fixed { layers: 7 }), Some(7));
    }

    /// A long context eats the offload budget: the same model and card must offload fewer layers
    /// at 128 k than at 4 k.
    #[test]
    fn context_length_takes_layers_off_the_gpu() {
        let m = model(20, 48);
        let h = machine(128, Some(24));
        let short = plan_gpu_layers(&m, &h, 4096, GpuLayers::Auto).expect("a plan");
        let long = plan_gpu_layers(&m, &h, 131_072, GpuLayers::Auto).expect("a plan");
        assert!(long < short, "short={short} long={long}");
    }
}
