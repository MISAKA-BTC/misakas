//! End-to-end against a **real** `llama-server`.
//!
//! Everything else in this crate's tests runs on the mock backend, which is the right default: CI
//! has no GPU, no engine and no model. But the mock cannot tell you whether the llama.cpp backend
//! actually starts a process, survives the health wait, parses that engine's SSE framing, or reads
//! a version banner that a real binary printed — and llama.cpp is the backend the Studio ships
//! with.
//!
//! So this test exists and skips itself unless pointed at both halves:
//!
//! ```sh
//! # An engine (any recent build):
//! #   cmake -B build -DCMAKE_BUILD_TYPE=Release -DLLAMA_CURL=OFF && cmake --build build --target llama-server
//! # A model — a ~50 kB fixture is enough, and this repository generates one:
//! python3 misaka-studio/testing/make_tiny_gguf.py /tmp/models/tiny-llama-F32.gguf
//!
//! MISAKA_TEST_LLAMA_SERVER=/path/to/llama-server \
//! MISAKA_TEST_MODELS_DIR=/tmp/models \
//!   cargo test -p misaka-studio-runtime --test llamacpp_e2e -- --nocapture
//! ```
//!
//! The fixture model generates nonsense — one layer, 32 dimensions, random weights. Nothing here
//! tests what a model *says*; it tests that the Studio can drive one.

use futures_util::StreamExt;
use misaka_studio_core::provenance::SamplingCommitment;
use misaka_studio_core::settings::{BackendKind, BackendSettings, FlashAttention, GenerationDefaults, Settings};
use misaka_studio_runtime::AppState;
use misaka_studio_runtime::backend::StreamEvent;
use std::path::PathBuf;

/// The two environment variables, or `None` — in which case the test prints why it skipped.
///
/// A skip that says nothing is a test that silently stops running the day someone renames the
/// variable.
fn environment() -> Option<(PathBuf, PathBuf)> {
    let engine = std::env::var("MISAKA_TEST_LLAMA_SERVER").ok().map(PathBuf::from);
    let models = std::env::var("MISAKA_TEST_MODELS_DIR").ok().map(PathBuf::from);
    match (engine, models) {
        (Some(engine), Some(models)) if engine.is_file() && models.is_dir() => Some((engine, models)),
        (engine, models) => {
            eprintln!(
                "skipping the llama.cpp end-to-end test: MISAKA_TEST_LLAMA_SERVER={:?} MISAKA_TEST_MODELS_DIR={:?} \
                 (both must be set, and exist)",
                engine, models
            );
            None
        }
    }
}

async fn studio(engine: PathBuf, models: PathBuf) -> (std::sync::Arc<AppState>, tempfile::TempDir) {
    let data = tempfile::tempdir().expect("tempdir");
    let settings = Settings {
        models_dir: models,
        backend: BackendSettings {
            kind: BackendKind::LlamaCpp,
            llama_server_path: Some(engine),
            // The fixture is tiny; a long wait would only hide a hang.
            startup_timeout_secs: 90,
            // Left at Auto — the default, which passes no flag at all. An earlier version of this
            // test set flash attention off explicitly, and that is exactly why it did not catch
            // the bare `-fa` flag failing on a current engine.
            flash_attention: FlashAttention::Auto,
            ..Default::default()
        },
        generation: GenerationDefaults { context_size: Some(512), max_tokens: 16, ..Default::default() },
        ..Default::default()
    };

    let state = AppState::new(settings, data.path().join("settings.json"), data.path().to_path_buf()).await;
    (state, data)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_engine_loads_streams_and_identifies_itself() {
    let Some((engine, models)) = environment() else { return };
    let (state, _data) = studio(engine, models).await;

    let listed = state.store.list().await;
    let model = listed.first().expect("the models directory has at least one GGUF").clone();
    eprintln!("model: {} ({} bytes)", model.id, model.size_bytes);

    // 1. It loads — which means the process started, bound a port, and answered /health.
    let status = state.load(&model.id, Some(512)).await.expect("the engine loads the model");
    assert_eq!(status.model_id.as_deref(), Some(model.id.as_str()));
    assert_eq!(status.backend, "llamacpp");
    eprintln!("loaded in {} ms", status.load_ms.unwrap_or(0));

    // 2. It identifies itself. A real binary prints `version: <build> (<commit>)`, and the whole
    //    point of `h_R` is that it comes from what the engine says rather than from a guess.
    let descriptor = status.descriptor.expect("a runtime descriptor");
    eprintln!("engine: commit={} build={}", descriptor.engine_commit, descriptor.engine_build_number);
    assert_ne!(descriptor.engine_commit, "unknown", "the engine's version banner should have parsed");
    assert!(descriptor.engine_build_number > 0, "a real build number");
    assert!(status.runtime_hash.is_some() && status.runtime_class_id.is_some());

    // 3. It streams. Deltas arrive, then exactly one Done carrying usage from the engine.
    let params = SamplingCommitment { temperature: 0.0, max_tokens: 16, ..Default::default() };
    let mut stream = state
        .generate(vec![misaka_studio_runtime::backend::ChatMessage::new("user", "a b c")], None, params, Vec::new())
        .await
        .expect("generation starts");

    let mut text = String::new();
    let mut done = None;
    while let Some(event) = stream.next().await {
        match event.expect("no stream error") {
            StreamEvent::Delta(delta) => text.push_str(&delta),
            StreamEvent::Done { usage, finish_reason } => done = Some((usage, finish_reason)),
        }
    }
    let (usage, finish_reason) = done.expect("the stream ends with a Done event");
    eprintln!("generated {:?} ({} tokens, {finish_reason})", text, usage.completion_tokens);
    assert!(usage.completion_tokens > 0, "the engine reported completion tokens");
    assert!(usage.prompt_tokens > 0, "the engine reported prompt tokens");

    // 4. It recorded the run. This is the claim the whole provenance layer makes, checked against
    //    a real engine rather than the mock.
    let records = state.records.read().await.clone();
    let recorded = records.list(1).await;
    let record = recorded.first().expect("a record was written").record.clone();
    assert_eq!(record.completion_tokens, usage.completion_tokens);
    assert_eq!(record.runtime.descriptor.engine_commit, descriptor.engine_commit);
    assert!(record.runtime.verify(), "the runtime identity re-derives");
    assert!(record.tokens_per_second > 0.0);

    // 5. It unloads, and the child process goes with it.
    state.unload().await.expect("unloads");
    assert!(state.loaded().await.is_none());
}

/// A model path that does not exist must fail as a load error naming the engine, not hang until
/// the startup timeout.
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_model_fails_fast_with_the_engines_own_words() {
    let Some((engine, models)) = environment() else { return };
    let (state, _data) = studio(engine, models).await;

    let error = state.load("no-such-model", None).await.unwrap_err();
    assert!(matches!(error, misaka_studio_runtime::Error::ModelNotFound { .. }), "got {error}");
}
