//! The llama.cpp backend — `llama-server`, supervised.
//!
//! This is the default engine on every platform the Studio targets: CUDA on Windows and Linux,
//! Metal on Apple Silicon, plain CPU everywhere else. It is also the one that already appears in
//! this repository's PALW work, which matters for the long path — the artifacts a validator
//! pins are llama.cpp GGUFs under a pinned llama.cpp build.
//!
//! # Finding the binary
//!
//! Three places, in order, because each is right for a different kind of user:
//!
//! 1. **The configured path** — someone who built llama.cpp with flags they care about.
//! 2. **Next to the Studio executable** — the packaged desktop app ships an engine beside itself.
//! 3. **`PATH`** — a developer with `llama-server` installed system-wide.
//!
//! When none of them has it, the backend reports unavailable *with the remedy*, and the app
//! keeps running on the mock backend rather than failing to start. A local-LLM app that refuses
//! to open because an engine is missing has made the user's first problem unsolvable from inside
//! the app.

use super::openai_child::{ChildEngine, ChildEngineConfig};
use super::{Availability, GenerationRequest, InferenceBackend, LoadRequest, LoadedModel, StreamEvent};
use crate::Result;
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use misaka_studio_core::provenance::RuntimeDescriptor;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct LlamaCppBackend {
    engine: ChildEngine,
    accelerator_tag: String,
}

impl LlamaCppBackend {
    /// `configured` is `backend.llama_server_path`; `accelerator_tag` is `cuda`, `metal`, `rocm`
    /// or `cpu` and becomes part of the determinism class, because the same source built for a
    /// different accelerator is different arithmetic.
    pub fn new(configured: Option<PathBuf>, accelerator_tag: impl Into<String>, startup_timeout: Duration) -> Self {
        let program = resolve_program(configured);
        LlamaCppBackend {
            accelerator_tag: accelerator_tag.into(),
            engine: ChildEngine::new(ChildEngineConfig {
                name: "llamacpp",
                program,
                args: Box::new(build_args),
                // llama-server answers /health with 503 while the model loads and 200 once it is
                // ready, which is exactly the signal a supervisor needs.
                health_path: "/health",
                startup_timeout,
                env: Vec::new(),
            }),
        }
    }

    pub fn recent_log(&self) -> Vec<String> {
        self.engine.recent_log()
    }
}

/// Where the engine binary is.
pub fn resolve_program(configured: Option<PathBuf>) -> PathBuf {
    let exe_name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };

    if let Some(path) = configured {
        return path;
    }
    // Beside the Studio's own executable: how the packaged app ships an engine.
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        for candidate in [dir.join(exe_name), dir.join("engines").join(exe_name)] {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    if let Some(found) = which(exe_name) {
        return found;
    }
    // Not found: return the bare name so the error names the thing that is missing rather than
    // an absolute path that never existed.
    PathBuf::from(exe_name)
}

/// A minimal `which`, to avoid a dependency for eleven lines.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(name)).find(|c| c.is_file())
}

/// The command line.
///
/// Conservative on purpose: only flags `llama-server` has accepted for years, because a user's
/// engine build may be any age and an unknown flag makes it exit with a usage message instead of
/// loading. Anything newer goes through `backend.extra_args`, where the user owns the risk.
fn build_args(request: &LoadRequest, port: u16) -> Vec<String> {
    let mut args = vec![
        "--model".into(),
        request.model_path.display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--ctx-size".into(),
        request.context_size.to_string(),
        // So the engine's own /v1/models reports the id the Studio uses.
        "--alias".into(),
        request.model_id.clone(),
    ];
    if let Some(layers) = request.gpu_layers {
        args.push("--n-gpu-layers".into());
        args.push(layers.to_string());
    }
    if let Some(threads) = request.threads {
        args.push("--threads".into());
        args.push(threads.to_string());
    }
    if request.flash_attention {
        args.push("-fa".into());
    }
    if !request.use_mmap {
        args.push("--no-mmap".into());
    }
    if request.use_mlock {
        args.push("--mlock".into());
    }
    args.extend(request.extra_args.iter().cloned());
    args
}

impl InferenceBackend for LlamaCppBackend {
    fn name(&self) -> &'static str {
        "llamacpp"
    }

    fn descriptor(&self) -> BoxFuture<'_, RuntimeDescriptor> {
        Box::pin(async move { self.engine.descriptor(&self.accelerator_tag).await })
    }

    fn availability(&self) -> BoxFuture<'_, Availability> {
        Box::pin(async {
            self.engine
                .availability(
                    "Install llama.cpp (its `llama-server` binary), or set backend.llama_server_path in Settings \
                     to a build you already have.",
                )
                .await
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

/// Which accelerator tag this machine should use, from what the hardware probe found.
pub fn accelerator_tag(hardware: &misaka_studio_core::HardwareSnapshot) -> &'static str {
    use misaka_studio_core::hardware::AcceleratorKind;
    match hardware.accelerators.iter().map(|a| a.kind).find(|k| *k != AcceleratorKind::Cpu) {
        Some(AcceleratorKind::Cuda) => "cuda",
        Some(AcceleratorKind::AppleUnified) => "metal",
        Some(AcceleratorKind::Rocm) => "rocm",
        Some(AcceleratorKind::Vulkan) => "vulkan",
        _ => "cpu",
    }
}

/// True when `path` looks like a directory of MLX weights rather than a GGUF.
pub fn is_gguf(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> LoadRequest {
        LoadRequest {
            model_id: "Qwen3-4B-Q4_K_M".into(),
            model_path: PathBuf::from("/models/Qwen3-4B-Q4_K_M.gguf"),
            context_size: 8192,
            gpu_layers: Some(33),
            threads: Some(8),
            flash_attention: true,
            use_mmap: true,
            use_mlock: false,
            extra_args: vec!["--verbose".into()],
        }
    }

    #[test]
    fn the_command_line_carries_the_load_request() {
        let args = build_args(&request(), 5599);
        let joined = args.join(" ");
        assert!(joined.contains("--model /models/Qwen3-4B-Q4_K_M.gguf"));
        assert!(joined.contains("--port 5599"));
        assert!(joined.contains("--ctx-size 8192"));
        assert!(joined.contains("--n-gpu-layers 33"));
        assert!(joined.contains("--threads 8"));
        assert!(joined.contains("-fa"));
        assert!(joined.ends_with("--verbose"), "extra args go last so they can override");
        assert!(!joined.contains("--no-mmap"));
    }

    /// The engine must never be told to listen anywhere but loopback: it has no authentication
    /// of its own, and the Studio's API key check happens in front of it.
    #[test]
    fn the_engine_only_ever_binds_loopback() {
        let args = build_args(&request(), 1234);
        let host = args.iter().position(|a| a == "--host").map(|i| args[i + 1].clone());
        assert_eq!(host.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn absent_options_are_absent_flags() {
        let mut req = request();
        req.gpu_layers = None;
        req.threads = None;
        req.flash_attention = false;
        req.extra_args.clear();
        let joined = build_args(&req, 1).join(" ");
        assert!(!joined.contains("--n-gpu-layers"));
        assert!(!joined.contains("--threads"));
        assert!(!joined.contains("-fa"));
    }

    #[test]
    fn mlock_and_no_mmap_are_passed_when_asked_for() {
        let mut req = request();
        req.use_mmap = false;
        req.use_mlock = true;
        let joined = build_args(&req, 1).join(" ");
        assert!(joined.contains("--no-mmap"));
        assert!(joined.contains("--mlock"));
    }

    #[test]
    fn a_configured_path_wins() {
        let configured = PathBuf::from("/opt/llama/llama-server");
        assert_eq!(resolve_program(Some(configured.clone())), configured);
    }
}
