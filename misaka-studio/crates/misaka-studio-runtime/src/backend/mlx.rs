//! The MLX backend — Apple Silicon's native path.
//!
//! MLX runs on the unified memory directly and, for the models it supports, beats llama.cpp on
//! Apple hardware. It is a second backend rather than a replacement because it takes a different
//! artifact: MLX weights are a **directory** (safetensors plus a tokenizer), not a GGUF. Nothing
//! converts one to the other at load time, so the Studio treats them as two kinds of model and
//! routes each to the engine that can read it.
//!
//! Availability is genuinely conditional — `mlx_lm` is a Python package on a Mac — so this
//! backend reports itself unavailable everywhere else rather than failing at load time. Being
//! told "MLX needs macOS on Apple Silicon" while choosing a backend is useful; being told it
//! after a download and a click is not.

use super::openai_child::{ChildEngine, ChildEngineConfig};
use super::{Availability, GenerationRequest, InferenceBackend, LoadRequest, LoadedModel, StreamEvent};
use crate::Result;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use misaka_studio_core::provenance::RuntimeDescriptor;
use std::path::PathBuf;
use std::time::Duration;

pub struct MlxBackend {
    engine: ChildEngine,
}

impl MlxBackend {
    pub fn new(configured: Option<PathBuf>, startup_timeout: Duration) -> Self {
        // `mlx_lm.server` is installed as a console script by the `mlx-lm` package.
        let program = configured.unwrap_or_else(|| PathBuf::from("mlx_lm.server"));
        MlxBackend {
            engine: ChildEngine::new(ChildEngineConfig {
                name: "mlx",
                program,
                args: Box::new(build_args),
                // MLX's server has no health endpoint; /v1/models is the cheapest thing it
                // answers once it is up, and it answers nothing before that.
                health_path: "/v1/models",
                startup_timeout,
                env: Vec::new(),
            }),
        }
    }

    /// Whether this machine could run MLX at all.
    pub fn platform_supported() -> bool {
        cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
    }
}

fn build_args(request: &LoadRequest, port: u16) -> Vec<String> {
    let mut args = vec![
        "--model".into(),
        request.model_path.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
    ];
    args.extend(request.extra_args.iter().cloned());
    args
}

impl InferenceBackend for MlxBackend {
    fn name(&self) -> &'static str {
        "mlx"
    }

    fn descriptor(&self) -> BoxFuture<'_, RuntimeDescriptor> {
        // "metal" always: MLX has no other target.
        Box::pin(async move { self.engine.descriptor("metal").await })
    }

    fn availability(&self) -> BoxFuture<'_, Availability> {
        Box::pin(async {
            if !Self::platform_supported() {
                return Availability::Unavailable {
                    reason: "MLX runs only on macOS with Apple Silicon".into(),
                    remedy: "Use the llama.cpp backend on this machine.".into(),
                };
            }
            self.engine.availability("Install it with `pip install mlx-lm`, or set backend.mlx_server_path to the script.").await
        })
    }

    fn load(&self, request: LoadRequest) -> BoxFuture<'_, Result<LoadedModel>> {
        Box::pin(async move { self.engine.load(request).await })
    }

    fn unload(&self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { self.engine.unload().await })
    }

    fn loaded(&self) -> BoxFuture<'_, Option<LoadedModel>> {
        Box::pin(async move { self.engine.loaded().await })
    }

    fn generate(&self, request: GenerationRequest) -> BoxFuture<'_, Result<BoxStream<'static, Result<StreamEvent>>>> {
        Box::pin(async move { self.engine.generate(request).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn off_apple_silicon_it_says_so_before_anything_is_downloaded() {
        let backend = MlxBackend::new(None, Duration::from_secs(1));
        let availability = backend.availability().await;
        if !MlxBackend::platform_supported() {
            match availability {
                Availability::Unavailable { reason, remedy } => {
                    assert!(reason.contains("Apple Silicon"));
                    assert!(remedy.contains("llama.cpp"));
                }
                other => panic!("expected unavailable, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_command_line_points_at_the_model_directory() {
        let request = LoadRequest {
            model_id: "Qwen3-4B-mlx".into(),
            model_path: PathBuf::from("/models/Qwen3-4B-mlx"),
            context_size: 8192,
            gpu_layers: None,
            threads: None,
            flash_attention: misaka_studio_core::settings::FlashAttention::Auto,
            use_mmap: true,
            use_mlock: false,
            needs_default_chat_template: false,
            extra_args: Vec::new(),
        };
        let joined = build_args(&request, 7000).join(" ");
        assert!(joined.contains("--model /models/Qwen3-4B-mlx"));
        assert!(joined.contains("--port 7000"));
        assert!(joined.contains("--host 127.0.0.1"));
    }
}
