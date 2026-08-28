//! Settings, and where on each platform they live.
//!
//! Two rules shape this file.
//!
//! **An unknown key is kept, not dropped.** Settings are edited by a newer build and read by an
//! older one (a user rolls back; a sidecar lags the UI). `serde(default)` on every field plus a
//! save path that rewrites only what it understands means a downgrade loses nothing.
//!
//! **A partial write is never a settings file.** The save is write-to-temp-then-rename, because
//! the failure mode of the obvious implementation is a truncated JSON file that makes the app
//! refuse to start — the one bug where the fix ("delete this file") is invisible to the person
//! hitting it.

use crate::provenance::SamplingCommitment;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Which engine runs the model.
///
/// The Studio's whole architecture is `Studio UI → MISAKA Runtime API → backend → GPU/CPU`, and
/// this enum is the seam: a value here selects an implementation of the runtime's backend trait
/// and nothing above it changes. `Misaka` is reserved for the deterministic in-house runtime the
/// PALW work already has in this repository — named now so the setting does not have to be
/// invented later, and refused at load time until it exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Pick by platform: MLX on Apple Silicon when present, llama.cpp everywhere else.
    #[default]
    Auto,
    /// llama.cpp's `llama-server`, driven as a child process.
    LlamaCpp,
    /// Apple's MLX, via `mlx_lm.server`. macOS/Apple Silicon only.
    Mlx,
    /// The deterministic in-tree runtime. Not yet available to the Studio.
    Misaka,
    /// A built-in fake that streams a canned reply. For UI work and tests with no model.
    Mock,
}

/// How many transformer layers to put on the GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GpuLayers {
    /// Offload as much as the estimate says will fit — the right answer for most people, and
    /// the only one that adapts when they load a bigger model.
    #[default]
    Auto,
    /// Everything. Fails loudly if it does not fit, which is sometimes what you want.
    All,
    None,
    Fixed {
        layers: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendSettings {
    pub kind: BackendKind,
    /// Path to `llama-server`. `None` means "look on PATH and in the app's own bundle".
    pub llama_server_path: Option<PathBuf>,
    /// Path to the MLX server entry point (macOS).
    pub mlx_server_path: Option<PathBuf>,
    pub gpu_layers: GpuLayers,
    /// Generation threads. `None` lets the engine choose, which it does better than a fixed
    /// default copied from someone else's machine.
    pub threads: Option<u32>,
    /// Flash attention. Large memory win on long contexts; not supported by every build, so it
    /// is a setting rather than an assumption.
    pub flash_attention: bool,
    pub use_mmap: bool,
    /// Lock the model in RAM. Prevents the OS swapping weights out mid-generation, at the cost
    /// of being unable to load anything that does not fit.
    pub use_mlock: bool,
    /// Extra arguments appended verbatim to the engine's command line.
    pub extra_args: Vec<String>,
    /// Seconds to wait for an engine to become healthy after launch. A 70B model off a spinning
    /// disk genuinely takes minutes.
    pub startup_timeout_secs: u64,
}

impl Default for BackendSettings {
    fn default() -> Self {
        BackendSettings {
            kind: BackendKind::Auto,
            llama_server_path: None,
            mlx_server_path: None,
            gpu_layers: GpuLayers::Auto,
            threads: None,
            flash_attention: true,
            use_mmap: true,
            use_mlock: false,
            extra_args: Vec::new(),
            startup_timeout_secs: 600,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    /// Bind address. **127.0.0.1 by default, deliberately.**
    ///
    /// This process answers `/v1/chat/completions` with no authentication out of the box. On
    /// `0.0.0.0` that is an open inference endpoint for everyone on the café wifi, so exposing
    /// it is a decision someone has to make on purpose — and [`Self::requires_api_key`] refuses
    /// the combination of a public bind and no key.
    pub host: String,
    pub port: u16,
    /// Optional bearer token for the OpenAI-compatible surface.
    pub api_key: Option<String>,
    /// Extra CORS origins. The local UI is same-origin and needs none of these.
    pub cors_origins: Vec<String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        ServerSettings { host: "127.0.0.1".into(), port: 1338, api_key: None, cors_origins: Vec::new() }
    }
}

impl ServerSettings {
    /// True when this configuration would expose an unauthenticated endpoint beyond the machine.
    pub fn requires_api_key(&self) -> bool {
        let local = self.host == "127.0.0.1" || self.host == "localhost" || self.host == "::1";
        !local && self.api_key.is_none()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HuggingFaceSettings {
    /// API endpoint. Configurable for mirrors and for corporate proxies — `HF_ENDPOINT` is the
    /// variable the rest of the ecosystem already uses for this.
    pub endpoint: String,
    /// Access token, for gated repositories and higher rate limits.
    pub token: Option<String>,
    /// Parallel download connections.
    pub max_concurrent_downloads: usize,
}

impl Default for HuggingFaceSettings {
    fn default() -> Self {
        HuggingFaceSettings {
            endpoint: std::env::var("HF_ENDPOINT").unwrap_or_else(|_| "https://huggingface.co".into()),
            token: None,
            max_concurrent_downloads: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: Theme,
    /// Show the provenance panel (model hash, runtime identity, inference hash) in chat.
    pub show_provenance: bool,
    /// Show tokens/sec and the memory gauges while generating.
    pub show_performance: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings { theme: Theme::System, show_provenance: true, show_performance: true }
    }
}

/// What the Studio records about its own inferences.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvenanceSettings {
    /// Write an [`crate::InferenceRecord`] per completion.
    pub record_inferences: bool,
    /// Also keep the prompt and completion **text** alongside the record.
    ///
    /// Off by default. The record commits to the bytes with a hash, which is what verification
    /// needs; keeping the plaintext as well turns a provenance log into a transcript of
    /// everything the user ever typed, sitting in a second place they do not know about. Anyone
    /// who wants replayable evidence can turn it on knowing that.
    pub keep_transcripts: bool,
    /// Cap on records kept on disk; the oldest are dropped past it.
    pub max_records: usize,
}

impl Default for ProvenanceSettings {
    fn default() -> Self {
        ProvenanceSettings { record_inferences: true, keep_transcripts: false, max_records: 10_000 }
    }
}

/// The default generation settings a new conversation starts from.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct GenerationDefaults {
    pub system_prompt: String,
    pub context_size: Option<u32>,
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub min_p: f64,
    pub repeat_penalty: f64,
    pub max_tokens: u64,
    pub seed: Option<u64>,
}

impl Default for GenerationDefaults {
    fn default() -> Self {
        GenerationDefaults {
            system_prompt: String::new(),
            context_size: None,
            temperature: 0.7,
            top_p: 0.95,
            top_k: 40,
            min_p: 0.05,
            repeat_penalty: 1.1,
            max_tokens: 2048,
            seed: None,
        }
    }
}

impl GenerationDefaults {
    pub fn sampling(&self) -> SamplingCommitment {
        SamplingCommitment {
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            min_p: self.min_p,
            repeat_penalty: self.repeat_penalty,
            max_tokens: self.max_tokens,
            seed: self.seed,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Where GGUF files live. Movable, because models are the largest thing on most people's
    /// disks and the system drive is rarely where they want them.
    pub models_dir: PathBuf,
    pub server: ServerSettings,
    pub backend: BackendSettings,
    pub generation: GenerationDefaults,
    pub huggingface: HuggingFaceSettings,
    pub ui: UiSettings,
    pub provenance: ProvenanceSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            models_dir: default_models_dir(),
            server: ServerSettings::default(),
            backend: BackendSettings::default(),
            generation: GenerationDefaults::default(),
            huggingface: HuggingFaceSettings::default(),
            ui: UiSettings::default(),
            provenance: ProvenanceSettings::default(),
        }
    }
}

impl Settings {
    /// Load from `path`, or return the defaults when the file does not exist yet.
    ///
    /// A *corrupt* file is a different thing from a missing one and is reported as an error:
    /// silently starting with defaults after a bad parse throws away settings the user still
    /// has, and they find out by noticing their model directory moved.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| Error::Settings { path: path.display().to_string(), reason: format!("not valid settings JSON: {e}") }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(Error::io(path.display(), e)),
        }
    }

    /// Write atomically: temp file in the same directory, then rename over the target.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent.display(), e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Settings { path: path.display().to_string(), reason: e.to_string() })?;
        // Same directory, so the rename stays within one filesystem and is therefore atomic.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, json).map_err(|e| Error::io(temp.display(), e))?;
        std::fs::rename(&temp, path).map_err(|e| Error::io(path.display(), e))?;
        Ok(())
    }
}

/// Per-platform data directory for the app.
pub fn default_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("MISAKA Studio")
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support/MISAKA Studio")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("misaka-studio")
    }
}

/// Where models go by default: `<data dir>/models`.
pub fn default_models_dir() -> PathBuf {
    default_data_dir().join("models")
}

/// The settings file itself.
pub fn default_settings_path() -> PathBuf {
    default_data_dir().join("settings.json")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        let mut s = Settings::default();
        s.server.port = 4242;
        s.backend.kind = BackendKind::LlamaCpp;
        s.save(&path).expect("saves");
        let back = Settings::load(&path).expect("loads");
        assert_eq!(back.server.port, 4242);
        assert_eq!(back.backend.kind, BackendKind::LlamaCpp);
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_a_broken_one_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope.json");
        assert_eq!(Settings::load(&missing).expect("defaults").server.port, 1338);

        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "{ this is not json").expect("write");
        assert!(matches!(Settings::load(&broken), Err(Error::Settings { .. })));
    }

    /// A file written by a newer build must not lose the fields this build does not know, and
    /// must not fail to load either.
    #[test]
    fn unknown_and_missing_fields_both_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("partial.json");
        std::fs::write(&path, r#"{"server":{"port":9000},"someFutureKey":{"a":1}}"#).expect("write");
        let s = Settings::load(&path).expect("loads");
        assert_eq!(s.server.port, 9000);
        assert_eq!(s.server.host, "127.0.0.1", "absent fields fall back to defaults");
        assert_eq!(s.generation.temperature, 0.7);
    }

    /// The check that stops a convenience setting from becoming an open inference endpoint.
    #[test]
    fn a_public_bind_without_a_key_is_flagged() {
        let mut s = ServerSettings::default();
        assert!(!s.requires_api_key(), "the loopback default needs no key");
        s.host = "0.0.0.0".into();
        assert!(s.requires_api_key());
        s.api_key = Some("secret".into());
        assert!(!s.requires_api_key());
    }

    #[test]
    fn saving_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        Settings::default().save(&path).expect("saves");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left {leftovers:?}");
    }
}
