//! **The model preflight** (RFC-0002 Part II §II.2, requirement R1): for a model nobody has downloaded yet, can
//! it be registered, and what is missing?
//!
//! `palw-class preflight <input>` and `misaka model preflight <input>` print this report, human or `--json`.
//! It is a composition of existing, tested parts and invents no rule:
//!
//! * [`source`] reads the headers (a safetensors shard's, a GGUF's tensor table), never the data;
//! * [`model`] runs the generic frontend over them — features, scope, storage and its descriptors, the tensor
//!   check against the program's parameters, the artifact estimate — and judges the **convert** stage;
//! * [`chain`] asks the chain's own functions at a height — `tir_admit_v1`, admission v10 (typed), the court
//!   window, the canonical job, the fences, the registry's derived profile — and judges the **register** and
//!   **mine** stages;
//! * [`residency`] reads the program's tiers as a node holds the class (ADR-0112 for IR classes): the pinned, routed
//!   and gathered bytes, the RAM floor, the default budget, and what one replay reads from storage — reported, never
//!   judged (a seat's resources gate its readiness, not a class's admission).
//!
//! Every verdict is `ok`, `blocked` or `unknown` (a depth too shallow to say), and a blocker is
//! `{ stage, code, what, evidence, have, need, safe_paths }` with a code that is stable once published. The
//! JSON form carries no timestamp and no path: the same inputs give the same bytes.

pub mod chain;
pub mod full;
pub mod kernel;
pub mod model;
pub mod node;
pub mod pipeline;
pub mod remote;
pub mod render;
pub mod residency;
pub mod source;

use serde::Serialize;
use std::path::{Path, PathBuf};

pub use source::{InputKind, detect};

/// The schema id of the JSON report.
pub const PREFLIGHT_SCHEMA_V1: &str = "misaka.palw.preflight.v1";

/// How far a preflight goes (ADR-0108's depths, renamed for a model).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Depth {
    /// Architecture, scope, storage, the tensor checks, the artifact estimate: the convert stage's verdict.
    Headers,
    /// Adds the shape-only lowering's admission, the chain's conditions at a height, the seat, the forecast:
    /// the register and mine verdicts. Needs a network.
    Shape,
    /// The artifact is built (or supplied) and verified. Needs the weights and a pack.
    Full,
}

impl Depth {
    pub fn parse(s: &str) -> Option<Depth> {
        match s {
            "headers" => Some(Depth::Headers),
            "shape" => Some(Depth::Shape),
            "full" => Some(Depth::Full),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Depth::Headers => "headers",
            Depth::Shape => "shape",
            Depth::Full => "full",
        }
    }
}

/// The three stages, in the order a model meets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Convert,
    Register,
    Mine,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Convert => "convert",
            Stage::Register => "register",
            Stage::Mine => "mine",
        }
    }
}

/// One thing that stops a stage. Codes are stable once published (RFC-0002 §II.2.4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Blocker {
    pub stage: Stage,
    pub code: String,
    /// The code's argument (`ARCH_NEEDS_FEATURE(<feature>)`, `FENCE_NOT_ARMED(<name>)`), when it has one.
    pub arg: Option<String>,
    pub what: String,
    pub evidence: Vec<String>,
    /// A count or a size the condition compares (what the model has, what the rule needs), with its unit.
    pub have: Option<u64>,
    pub need: Option<u64>,
    pub unit: Option<String>,
    /// What exists instead, taken from data.
    pub safe_paths: Vec<String>,
}

impl Blocker {
    pub fn new(stage: Stage, code: impl Into<String>, what: impl Into<String>) -> Blocker {
        Blocker {
            stage,
            code: code.into(),
            arg: None,
            what: what.into(),
            evidence: Vec::new(),
            have: None,
            need: None,
            unit: None,
            safe_paths: Vec::new(),
        }
    }
    pub fn arg(mut self, a: impl Into<String>) -> Blocker {
        self.arg = Some(a.into());
        self
    }
    pub fn evidence(mut self, e: impl IntoIterator<Item = String>) -> Blocker {
        self.evidence.extend(e);
        self
    }
    pub fn numbers(mut self, have: u64, need: u64, unit: &str) -> Blocker {
        self.have = Some(have);
        self.need = Some(need);
        self.unit = Some(unit.to_string());
        self
    }
    pub fn safe(mut self, p: impl IntoIterator<Item = String>) -> Blocker {
        self.safe_paths.extend(p);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Ok,
    Blocked,
    /// The depth reached cannot say.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageVerdict {
    pub status: StageStatus,
    pub blockers: Vec<Blocker>,
    /// Why it is `unknown`.
    pub unknown_because: Option<String>,
}

impl StageVerdict {
    pub fn of(blockers: Vec<Blocker>) -> StageVerdict {
        StageVerdict {
            status: if blockers.is_empty() { StageStatus::Ok } else { StageStatus::Blocked },
            blockers,
            unknown_because: None,
        }
    }
    pub fn unknown(why: impl Into<String>) -> StageVerdict {
        StageVerdict { status: StageStatus::Unknown, blockers: Vec::new(), unknown_because: Some(why.into()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub convert: StageVerdict,
    pub register: StageVerdict,
    pub mine: StageVerdict,
}

/// One condition of the chain, as needed against limit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Condition {
    pub id: String,
    pub what: String,
    pub needed: Option<u64>,
    pub limit: Option<u64>,
    pub unit: String,
    /// `None`: not computed (an earlier wall stopped it, or the depth is too shallow).
    pub ok: Option<bool>,
    /// The function or rule that answers it.
    pub source: String,
}

/// What a preflight is asked.
#[derive(Clone, Debug)]
pub struct Options {
    pub depth: Depth,
    /// A network preset id (`testnet-12`); `None` stops at the headers depth.
    pub network: Option<String>,
    pub height: Option<u64>,
    /// Quant-format descriptor files (`--quant-format`).
    pub quant_formats: Vec<PathBuf>,
    /// A data adapter file (`--adapter`).
    pub adapter: Option<PathBuf>,
    /// For a lone `config.json`: a directory of `*.safetensors` files (or header prefixes of them).
    pub headers: Option<PathBuf>,
    pub max_context: Option<u32>,
    pub tile_len: u32,
    pub h_chunk: u32,
    /// The program's history bound is the held one (`--held`).
    pub held: bool,
    /// The memory a seat has, in GiB; `None` is the reference seat.
    pub seat_shares: Vec<chain::SeatShare>,
    /// The tier rule the residency is read under: a row-addressed param under this many bytes is pinned (the node's
    /// default, `TIR_PIN_BELOW_BYTES_V1`).
    pub residency_pin_below_bytes: u64,
    /// What a live node said (`--node`, [`node::NodeFacts`]): the tip is the default height, the registry's reading of the classes it
    /// holds is what a class already on the chain is judged by, and the base population is what the forecast's independence is read
    /// against.
    pub node: Option<node::NodeFacts>,
    /// The `full` depth's inputs: the runtime pack and the artifact ([`full`]).
    pub full: full::FullInputs,
    /// A PEFT LoRA adapter over the model (RFC-0004: a candidate is a parent plus an adapter): its `adapter_config.json` and the
    /// tensor names of its `adapter_model.safetensors`, attached to the model's spec before the shape-only lowering.
    pub lora: Option<LoraInput>,
    /// **Judge a data-route class by the pipeline admission** (RFC-0003; [`pipeline`]): an encoder–decoder is declared shape-only at
    /// the context and asked of the node's own gate (`palw_gen_registration_preflight_at_v1`) at the judged height, and a route this
    /// build cannot declare is the blocker `PIPELINE_CLASS_UNDECLARED`. Off, the route's stage programs are admitted one by one and the
    /// register and mine stages stay `unknown` — the behaviour the corpus pins (`tests/golden/corpus_preflight_v1.json`, hashed by the
    /// RFC-0011 coverage audit) record for `Options::default()`. The CLIs and the census turn it on.
    pub pipeline_admission: bool,
}

/// An adapter the preflight attaches to the model it reads ([`Options::lora`]).
#[derive(Clone, Debug)]
pub struct LoraInput {
    pub config: String,
    pub tensors: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            depth: Depth::Shape,
            network: None,
            height: None,
            quant_formats: Vec::new(),
            adapter: None,
            headers: None,
            max_context: None,
            tile_len: crate::check_architecture::IR_DEFAULT_TILE_LEN_V1,
            h_chunk: crate::check_architecture::IR_DEFAULT_H_CHUNK_V1,
            held: false,
            seat_shares: Vec::new(),
            residency_pin_below_bytes: misaka_palw_tir_exec::tiers::TIR_PIN_BELOW_BYTES_V1,
            node: None,
            full: full::FullInputs::default(),
            lora: None,
            pipeline_admission: false,
        }
    }
}

/// The identities of the registries a verdict was computed with, so it is attributable to a build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Registries {
    pub adapter_pack: String,
    pub feature_registry: String,
    pub quant_registry: String,
    pub lowering: String,
    pub tool_version: String,
}

pub fn registries(reg: &misaka_palw_tir_lower::quantfmt::QuantRegistry) -> Registries {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/quant-registry/v1").to_state();
    let mut rows: Vec<(String, String)> = reg.all().iter().map(|f| (f.name().to_string(), f.digest_hex())).collect();
    rows.sort();
    for (n, d) in rows {
        st.update(n.as_bytes());
        st.update(&[0]);
        st.update(d.as_bytes());
        st.update(b"\n");
    }
    Registries {
        adapter_pack: misaka_palw_tir_lower::adapter::builtin::pack_hash(),
        feature_registry: misaka_palw_tir_lower::model::registry_digest(),
        quant_registry: source::hex(st.finalize().as_bytes()),
        lowering: crate::runtime_pack::manifest::LOWERING_VERSION_V1.to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InputInfo {
    pub kind: InputKind,
    pub label: String,
    pub bytes_read: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DepthInfo {
    pub requested: Depth,
    pub reached: Depth,
    pub stopped_at: Option<String>,
}

/// What the files said.
#[derive(Clone, Debug, Serialize)]
pub struct SourceInfo {
    pub files: Vec<source::FileInfo>,
    pub config_sha256: Option<String>,
    pub shards: Vec<ShardInfo>,
    pub missing_shards: Vec<String>,
    /// The checkpoint's weight bytes as the headers (or the index's total) declare them.
    pub weight_bytes: Option<u64>,
    pub index_total_size: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShardInfo {
    pub file: String,
    pub header_bytes: u64,
    pub declared_data_bytes: u64,
    pub file_bytes: u64,
    pub complete: bool,
    pub header_sha256: String,
    pub tensors: usize,
}

/// What the node said, as the report states it.
#[derive(Clone, Debug, Serialize)]
pub struct NodeInfo {
    pub network: String,
    pub tip_daa: u64,
    pub classes: usize,
    /// The largest base population any class's seating reads, if the node serves seating (past `palw_class_seating`).
    pub base_operators: Option<u32>,
}

/// The whole report.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub mode: &'static str,
    pub input: InputInfo,
    pub depth: DepthInfo,
    pub network: Option<chain::NetworkInfo>,
    pub source: SourceInfo,
    pub model: Option<model::ModelInfo>,
    pub scope: Option<misaka_palw_tir_lower::model::FeatureScope>,
    pub storage: model::StorageInfo,
    pub tensors: model::TensorsInfo,
    pub artifact: Option<model::ArtifactInfo>,
    /// The class's residency as a node holds it, read off the program (reported, not judged).
    pub residency: Option<residency::ResidencyInfo>,
    /// ADR-0172: the K2-TIR-v1 kernel's static route of the program, shipped and hypothetically armed (reported, not judged).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<kernel::KernelRouteInfo>,
    pub admission: Option<chain::AdmissionInfo>,
    /// The pipeline class judged by the generative admission ([`pipeline`]); absent for an IR class (and in the JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<pipeline::PipelineInfo>,
    pub chain: Vec<Condition>,
    pub seat: Option<chain::SeatInfo>,
    pub forecast: Option<chain::Forecast>,
    /// The `full` depth's findings (the pack's verification, the artifact root, the chain's reading of it); absent below it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<full::FullInfo>,
    /// What the node was asked (`--node`): its network, tip and the classes it holds; absent without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeInfo>,
    pub verdict: Verdict,
    pub notes: Vec<String>,
    pub registries: Registries,
}

impl Report {
    /// Whether every stage the depth reached is `ok` (the exit code of the command).
    ///
    /// **A stage the requested depth should have judged and could not is not a pass**: at the `shape` depth or deeper a `register` or
    /// `mine` verdict of `unknown` (a route whose class was not judged, no network named) makes the model not registrable, so the
    /// command's exit status is never 0 on a registration nobody asked the chain about. At the `headers` depth they are `unknown`
    /// by design.
    pub fn registrable(&self) -> bool {
        let judged = self.depth.requested >= Depth::Shape;
        [&self.verdict.convert, &self.verdict.register, &self.verdict.mine].iter().all(|v| v.status != StageStatus::Blocked)
            && self.verdict.convert.status == StageStatus::Ok
            && (!judged || (self.verdict.register.status == StageStatus::Ok && self.verdict.mine.status == StageStatus::Ok))
    }

    /// The blockers of every stage, in stage order.
    pub fn blockers(&self) -> Vec<&Blocker> {
        self.verdict.convert.blockers.iter().chain(&self.verdict.register.blockers).chain(&self.verdict.mine.blockers).collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Run a model preflight on `path`.
pub fn run(path: &Path, opts: &Options) -> Result<Report, String> {
    let kind = detect(path)?;
    if kind == InputKind::Artifact {
        return Err(
            "a .palwtir artifact is judged by the artifact admission (palw-class preflight on it), not by the model preflight".into(),
        );
    }
    run_input(path, kind, InputInfoOverride::default(), opts)
}

/// **Run a model preflight on a repository read by HTTP ranges** (RFC-0002 Part II §II.2.1): `base` is the repository's URL prefix
/// (`<base>/config.json` resolves it; [`remote::hf_base_url`] spells the Hugging Face one), `fetcher` the transport
/// ([`remote::HttpRangeFetcher`] for `http://`, a mirror or a fixture server; the lowerer's `curl` fetcher for `https://`). Only the
/// configuration, the index and the shard headers are fetched; the report is the local directory's but for the input's kind and label.
pub fn run_remote(base: &str, fetcher: &dyn misaka_palw_tir_lower::weights::RangeFetcher, opts: &Options) -> Result<Report, String> {
    let scratch = remote::Scratch::new()?;
    let stats = remote::materialize_headers(fetcher, base, &scratch.0)?;
    run_input(
        &scratch.0,
        InputKind::Remote,
        InputInfoOverride { label: Some(base.to_string()), bytes_read: Some(stats.fetched_bytes) },
        opts,
    )
}

/// What a remote input says about itself in place of the scratch snapshot's.
#[derive(Default)]
struct InputInfoOverride {
    label: Option<String>,
    bytes_read: Option<u64>,
}

fn run_input(path: &Path, kind: InputKind, over: InputInfoOverride, opts: &Options) -> Result<Report, String> {
    let mut files: Vec<std::path::PathBuf> = opts.quant_formats.clone();
    files.sort();
    let reg_owned;
    let reg: &misaka_palw_tir_lower::quantfmt::QuantRegistry = if files.is_empty() {
        misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin()
    } else {
        reg_owned = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin()
            .with(
                files
                    .iter()
                    .map(|p| {
                        let text = std::fs::read_to_string(p).map_err(|e| format!("--quant-format {}: {e}", p.display()))?;
                        misaka_palw_tir_lower::quantfmt::QuantFormat::from_json(&text)
                            .map_err(|e| format!("--quant-format {}: {e}", p.display()))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            )
            .map_err(|e| format!("--quant-format: {e}"))?;
        &reg_owned
    };
    let adapter_text = match &opts.adapter {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| format!("--adapter {}: {e}", p.display()))?),
        None => None,
    };
    let src = source::open(path, kind, opts.headers.as_deref(), reg)?;
    run_on_source(&src, Some(path), kind, over, opts, reg, adapter_text.as_deref())
}

/// **The preflight of a source already read** — the Hugging Face census (RFC-0002 §II.10, [`crate::census`]) builds its [`source::Source`]
/// from a header store (the files' sizes are the Hub's, the shards are their headers) and judges it here, with the built-in registries.
/// `cache` shares the chain's judgment between sources whose shape-only programs are the same bytes ([`JudgeCache`]).
pub fn run_census_source(
    src: &source::Source,
    label: &str,
    bytes_read: u64,
    opts: &Options,
    cache: Option<&JudgeCache>,
) -> Result<Report, String> {
    run_on_source_cached(
        src,
        None,
        InputKind::Remote,
        InputInfoOverride { label: Some(label.to_string()), bytes_read: Some(bytes_read) },
        opts,
        misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
        None,
        cache,
    )
}

/// **The chain's judgment, shared by identical programs.** [`chain::judge`] is a function of the shape-only program, the
/// options, and two figures of the convert stage it reads (the parameters' bytes and the inventory's leaf estimate, both functions
/// of the program); a census meets the same program in every fine-tune of one configuration, and one judgment runs admission v10
/// many times (the layout search). The key is all of those inputs, so a hit is the same computation, not an approximation. (The
/// tokenizer's bytes, which differ between fine-tunes that edit a chat template, are not read by the judgment and not keyed.)
#[derive(Default)]
pub struct JudgeCache {
    map: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::OnceLock<chain::ChainOutput>>>>,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
    /// A file the judgments are appended to and read back from (one `{"key", "chain"}` per line), so a census that restarts does not
    /// judge a program twice. The caller names it per build: a judgment is a function of the build's consensus code.
    file: Option<std::sync::Mutex<std::fs::File>>,
}

impl JudgeCache {
    pub fn key(program: &misaka_palw_tir::TirProgramV1, analysis: &model::Analysis, opts: &Options) -> String {
        let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/preflight-judge-cache/v1").to_state();
        st.update(&program.encode());
        let a = analysis.artifact.as_ref().map(|a| (a.params_bytes, a.inventory_leaves_estimate));
        st.update(
            format!(
                "|{a:?}|{:?}|{:?}|{:?}|{}|{}|{}|{:?}|{}|{}",
                opts.network,
                opts.height,
                opts.max_context,
                opts.tile_len,
                opts.h_chunk,
                opts.held,
                opts.seat_shares.iter().map(|s| (s.name.clone(), s.bytes)).collect::<Vec<_>>(),
                opts.residency_pin_below_bytes,
                opts.node.is_some()
            )
            .as_bytes(),
        );
        source::hex(st.finalize().as_bytes())
    }

    /// The key of a pipeline class's judgment: its programs' bytes, its lengths, its template and every option the judgment reads.
    pub fn pipeline_key(routed: &model::RoutedClass, opts: &Options) -> String {
        let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/preflight-judge-cache/pipeline/v1").to_state();
        match routed {
            model::RoutedClass::EncDec(e) => {
                st.update(&e.encoder.encode());
                st.update(&e.decoder.encode());
                st.update(
                    format!("|{}|{}|{:?}|{}|{}|{}", e.source_len, e.target_len, e.eos, e.pad, e.decoder_start, e.vocab).as_bytes(),
                );
            }
            model::RoutedClass::Undeclarable { kind, adapter, why } => {
                st.update(format!("undeclarable|{kind}|{adapter}|{why}").as_bytes());
            }
        }
        st.update(
            format!(
                "|{:?}|{:?}|{:?}|{:?}|{}",
                opts.network,
                opts.height,
                opts.seat_shares.iter().map(|s| (s.name.clone(), s.bytes)).collect::<Vec<_>>(),
                opts.node.is_some(),
                opts.pipeline_admission
            )
            .as_bytes(),
        );
        source::hex(st.finalize().as_bytes())
    }

    /// A cache backed by `path`: its judgments are loaded, and every new one is appended.
    pub fn with_file(path: &Path) -> Result<JudgeCache, String> {
        let c = JudgeCache::default();
        let mut n = 0;
        if let Ok(text) = std::fs::read_to_string(path) {
            let mut m = c.map.lock().map_err(|_| "poisoned")?;
            for line in text.lines() {
                // A line cut short by a stopped process is skipped.
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                let (Some(k), Some(ch)) = (v.get("key").and_then(|k| k.as_str()), v.get("chain")) else { continue };
                let Ok(out) = serde_json::from_value::<chain::ChainOutput>(ch.clone()) else { continue };
                let cell = std::sync::OnceLock::new();
                let _ = cell.set(out);
                m.insert(k.to_string(), std::sync::Arc::new(cell));
                n += 1;
            }
        }
        let f = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let _ = n;
        Ok(JudgeCache { file: Some(std::sync::Mutex::new(f)), ..c })
    }

    /// The judgments the cache holds.
    pub fn len(&self) -> usize {
        self.map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get_or_judge(&self, key: String, f: impl FnOnce() -> chain::ChainOutput) -> chain::ChainOutput {
        let key_for_file = key.clone();
        let cell = {
            let mut m = self.map.lock().unwrap_or_else(|p| p.into_inner());
            m.entry(key).or_default().clone()
        };
        let mut computed = false;
        let out = cell
            .get_or_init(|| {
                computed = true;
                f()
            })
            .clone();
        let counter = if computed { &self.misses } else { &self.hits };
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if computed
            && let Some(file) = &self.file
            && let Ok(line) = serde_json::to_string(&serde_json::json!({"key": key_for_file, "chain": out}))
            && let Ok(mut f) = file.lock()
        {
            use std::io::Write;
            let _ = writeln!(f, "{line}");
        }
        out
    }

    /// (hits, misses).
    pub fn stats(&self) -> (u64, u64) {
        (self.hits.load(std::sync::atomic::Ordering::Relaxed), self.misses.load(std::sync::atomic::Ordering::Relaxed))
    }
}

fn run_on_source(
    src: &source::Source,
    path: Option<&Path>,
    kind: InputKind,
    over: InputInfoOverride,
    opts: &Options,
    reg: &misaka_palw_tir_lower::quantfmt::QuantRegistry,
    adapter_text: Option<&str>,
) -> Result<Report, String> {
    run_on_source_cached(src, path, kind, over, opts, reg, adapter_text, None)
}

#[allow(clippy::too_many_arguments)]
fn run_on_source_cached(
    src: &source::Source,
    path: Option<&Path>,
    kind: InputKind,
    over: InputInfoOverride,
    opts: &Options,
    reg: &misaka_palw_tir_lower::quantfmt::QuantRegistry,
    adapter_text: Option<&str>,
    cache: Option<&JudgeCache>,
) -> Result<Report, String> {
    let analysis = model::analyze(src, opts, reg, adapter_text);

    // The depth reached: the shape depth needs a network and a program.
    let mut stopped_at: Option<String> = None;
    let mut reached = Depth::Headers;
    let mut network = None;
    let mut chain_out = chain::ChainOutput::default();
    if opts.depth >= Depth::Shape {
        match (&opts.network, &analysis.program) {
            (None, _) => stopped_at = Some("network not given (--network <id>)".into()),
            // A data-route class (RFC-0003) the convert stage lowered: the pipeline admission judges it.
            (Some(id), None) if opts.pipeline_admission && analysis.routed.is_some() => {
                let routed = analysis.routed.as_ref().expect("checked");
                let net = chain::PreflightNetwork::parse(id)?;
                if let Some(node) = &opts.node
                    && !node.network.is_empty()
                    && node.network != net.id
                {
                    return Err(format!(
                        "--node is on {} and --network is {}: the conditions would be judged on another chain",
                        node.network, net.id
                    ));
                }
                chain_out = match cache {
                    Some(c) => c.get_or_judge(JudgeCache::pipeline_key(routed, opts), || pipeline::judge_pipeline(&net, opts, routed)),
                    None => pipeline::judge_pipeline(&net, opts, routed),
                };
                network = Some(chain_out.network.clone());
                reached = Depth::Shape;
            }
            (Some(_), None) => stopped_at = Some("no program to judge: the convert stage is blocked".into()),
            (Some(id), Some(program)) => {
                let net = chain::PreflightNetwork::parse(id)?;
                if let Some(node) = &opts.node
                    && !node.network.is_empty()
                    && node.network != net.id
                {
                    return Err(format!("--node is on {} and --network is {}: the conditions would be judged on another chain", node.network, net.id));
                }
                chain_out = match cache {
                    Some(c) => c.get_or_judge(JudgeCache::key(program, &analysis, opts), || chain::judge(&net, opts, program, &analysis, src)),
                    None => chain::judge(&net, opts, program, &analysis, src),
                };
                network = Some(chain_out.network.clone());
                reached = Depth::Shape;
            }
        }
    }
    // The full depth: the pack verified and the chain's reading of the artifact root and the declared classes.
    let mut full_info = None;
    let mut full_register: Vec<Blocker> = Vec::new();
    let mut full_mine: Vec<Blocker> = Vec::new();
    if opts.depth == Depth::Full && stopped_at.is_none() {
        let model_dir = path.filter(|_| kind == InputKind::HfDirectory);
        let (info, blockers) = full::judge_full(&opts.full, model_dir, network.as_ref().map(|n| n.id.as_str()), opts.node.as_ref());
        for b in blockers {
            match b.stage {
                Stage::Register => full_register.push(b),
                _ => full_mine.push(b),
            }
        }
        reached = Depth::Full;
        full_info = Some(info);
    }
    chain_out.register.extend(full_register);
    chain_out.mine.extend(full_mine);

    let mut convert_blockers = analysis.blockers.clone();
    convert_blockers.extend(chain_out.convert_extra.iter().cloned());
    let convert = StageVerdict::of(convert_blockers);
    let (register, mine) = if reached >= Depth::Shape {
        (StageVerdict::of(chain_out.register.clone()), chain_out.mine_verdict(&convert))
    } else {
        let why = stopped_at.clone().unwrap_or_else(|| "the depth reached is headers".into());
        (StageVerdict::unknown(why.clone()), StageVerdict::unknown(why))
    };
    let mut notes = analysis.notes.clone();
    notes.extend(chain_out.notes.iter().cloned());
    // The residency at the canonical job of the context the shape depth chose (one forward pass without one).
    let canonical = chain_out
        .admission
        .as_ref()
        .and_then(|a| a.layout.as_ref())
        .and_then(|l| kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(l.max_context));
    let rules = misaka_palw_tir_exec::tiers::TirTierRulesV1 { pin_below_bytes: opts.residency_pin_below_bytes };
    let residency = analysis.program.as_ref().map(|p| residency::residency_of(p, rules, canonical));
    let kernel_positions =
        chain_out.admission.as_ref().and_then(|a| a.layout.as_ref()).map(|l| l.max_context).or(opts.max_context).unwrap_or(u32::MAX);
    let kernel = analysis
        .program
        .as_ref()
        .map(|p| kernel::kernel_route_of(p, kernel_positions, network.as_ref().map(|n: &chain::NetworkInfo| n.daa).unwrap_or(0)));
    Ok(Report {
        schema: PREFLIGHT_SCHEMA_V1,
        mode: "model",
        input: InputInfo {
            kind,
            label: over.label.unwrap_or_else(|| src.label.clone()),
            bytes_read: over.bytes_read.unwrap_or(src.bytes_read),
        },
        depth: DepthInfo { requested: opts.depth, reached, stopped_at },
        network,
        source: model::source_info(src),
        model: analysis.model.clone(),
        scope: analysis.scope.clone(),
        storage: analysis.storage.clone(),
        tensors: analysis.tensors.clone(),
        artifact: analysis.artifact.clone(),
        residency,
        kernel,
        admission: chain_out.admission.clone(),
        pipeline: chain_out.pipeline.clone(),
        chain: chain_out.conditions.clone(),
        seat: chain_out.seat.clone(),
        forecast: chain_out.forecast.clone(),
        full: full_info,
        node: opts.node.as_ref().map(|n| NodeInfo {
            network: n.network.clone(),
            tip_daa: n.tip_daa,
            classes: n.classes.len(),
            base_operators: n.base_operators(),
        }),
        verdict: Verdict { convert, register, mine },
        notes,
        registries: registries(reg),
    })
}
