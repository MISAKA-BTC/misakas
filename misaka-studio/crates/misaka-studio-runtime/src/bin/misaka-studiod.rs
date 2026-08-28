//! `misaka-studiod` — the MISAKA Runtime as a process.
//!
//! Run on its own it is a headless local inference server; run by the desktop shell it is the
//! sidecar the window talks to. Same binary, same API, so anything that works in the app works
//! from a script, and the UI cannot grow a private channel the API does not have.
//!
//! Command-line flags override the settings file for this run without rewriting it — the right
//! behaviour for `--port 9000` on a one-off, and the wrong behaviour to persist silently.

use clap::Parser;
use misaka_studio_core::settings::{BackendKind, Settings, default_data_dir, default_settings_path};
use misaka_studio_runtime::{AppState, api, locate_ui};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "misaka-studiod", version, about = "MISAKA Studio runtime: local LLM inference with an OpenAI-compatible API")]
struct Args {
    /// Address to bind. Anything but a loopback address requires --api-key.
    #[arg(long, env = "MISAKA_STUDIO_HOST")]
    host: Option<String>,

    #[arg(long, env = "MISAKA_STUDIO_PORT")]
    port: Option<u16>,

    /// Directory holding GGUF models.
    #[arg(long, env = "MISAKA_STUDIO_MODELS_DIR")]
    models_dir: Option<PathBuf>,

    /// Directory for settings, the record log and other state.
    #[arg(long, env = "MISAKA_STUDIO_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Settings file. Defaults to `<data dir>/settings.json`.
    #[arg(long, env = "MISAKA_STUDIO_SETTINGS")]
    settings: Option<PathBuf>,

    /// Directory of built UI assets to serve at `/`.
    #[arg(long, env = "MISAKA_STUDIO_UI_DIR")]
    ui_dir: Option<PathBuf>,

    /// Engine to use.
    #[arg(long, value_enum)]
    backend: Option<BackendArg>,

    /// Bearer token required on every request.
    #[arg(long, env = "MISAKA_STUDIO_API_KEY")]
    api_key: Option<String>,

    /// Path to `llama-server`.
    #[arg(long, env = "MISAKA_STUDIO_LLAMA_SERVER")]
    llama_server: Option<PathBuf>,

    /// Log filter, in `tracing` syntax.
    #[arg(long, env = "MISAKA_STUDIO_LOG", default_value = "info")]
    log: String,

    /// Print the resolved configuration and exit. Cheap way to see which settings file and model
    /// directory are actually in play before starting anything.
    #[arg(long)]
    check: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum BackendArg {
    Auto,
    Llamacpp,
    Mlx,
    Mock,
}

impl From<BackendArg> for BackendKind {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Auto => BackendKind::Auto,
            BackendArg::Llamacpp => BackendKind::LlamaCpp,
            BackendArg::Mlx => BackendKind::Mlx,
            BackendArg::Mock => BackendKind::Mock,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&args.log))
        .with_target(false)
        .init();

    let data_dir = args.data_dir.clone().unwrap_or_else(default_data_dir);
    let settings_path = args.settings.clone().unwrap_or_else(|| {
        if args.data_dir.is_some() { data_dir.join("settings.json") } else { default_settings_path() }
    });

    let mut settings = Settings::load(&settings_path)?;
    if let Some(host) = args.host.clone() {
        settings.server.host = host;
    }
    if let Some(port) = args.port {
        settings.server.port = port;
    }
    if let Some(dir) = args.models_dir.clone() {
        settings.models_dir = dir;
    }
    if let Some(kind) = args.backend {
        settings.backend.kind = kind.into();
    }
    if let Some(key) = args.api_key.clone() {
        settings.server.api_key = Some(key);
    }
    if let Some(path) = args.llama_server.clone() {
        settings.backend.llama_server_path = Some(path);
    }

    // Refused, not warned about. The failure this prevents — an unauthenticated inference
    // endpoint on a shared network — is silent, and by the time it is noticed it has been open
    // for a week.
    if settings.server.requires_api_key() {
        anyhow::bail!(
            "binding to {} would serve unauthenticated inference to the network. \
             Pass --api-key, or bind 127.0.0.1.",
            settings.server.host
        );
    }

    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&settings.models_dir)?;

    let ui_dir = locate_ui(args.ui_dir.clone());
    let addr: SocketAddr = format!("{}:{}", settings.server.host, settings.server.port).parse()?;

    if args.check {
        println!("settings file : {}", settings_path.display());
        println!("data dir      : {}", data_dir.display());
        println!("models dir    : {}", settings.models_dir.display());
        println!("bind          : {addr}");
        println!("backend       : {:?}", settings.backend.kind);
        println!("ui            : {}", ui_dir.map(|d| d.display().to_string()).unwrap_or_else(|| "none (API only)".into()));
        println!("api key       : {}", if settings.server.api_key.is_some() { "set" } else { "not set" });
        return Ok(());
    }

    let cors_origins = settings.server.cors_origins.clone();
    let state = AppState::new(settings, settings_path, data_dir).await;
    tokio::spawn(state.metrics.clone().run());

    let models = state.store.list().await.len();
    let app = api::router(state.clone(), ui_dir.clone(), cors_origins);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    tracing::info!("MISAKA Runtime listening on http://{bound}");
    tracing::info!("  OpenAI-compatible API : http://{bound}/v1");
    tracing::info!("  Studio API            : http://{bound}/api/v1");
    tracing::info!("  models found          : {models}");
    if ui_dir.is_none() {
        tracing::info!("  UI                    : not bundled; serving the API only");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            // Both signals, because a desktop shell stopping its sidecar sends SIGTERM and a
            // developer in a terminal sends SIGINT — and an engine left running holds the GPU.
            #[cfg(unix)]
            {
                let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                let _ = tokio::signal::ctrl_c().await;
            }
            tracing::info!("shutting down");
        })
        .await?;

    // The engine is a child process; stopping it here is what keeps a restart from finding its
    // VRAM already taken.
    if let Err(e) = state.unload().await {
        tracing::warn!("could not stop the engine cleanly: {e}");
    }
    Ok(())
}
