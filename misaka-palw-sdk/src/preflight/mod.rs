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
pub mod model;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    pub admission: Option<chain::AdmissionInfo>,
    pub chain: Vec<Condition>,
    pub seat: Option<chain::SeatInfo>,
    pub forecast: Option<chain::Forecast>,
    pub verdict: Verdict,
    pub notes: Vec<String>,
    pub registries: Registries,
}

impl Report {
    /// Whether every stage the depth reached is `ok` (the exit code of the command).
    pub fn registrable(&self) -> bool {
        [&self.verdict.convert, &self.verdict.register, &self.verdict.mine].iter().all(|v| v.status != StageStatus::Blocked)
            && self.verdict.convert.status == StageStatus::Ok
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
    let analysis = model::analyze(&src, opts, reg, adapter_text.as_deref());

    // The depth reached: the shape depth needs a network and a program.
    let mut stopped_at: Option<String> = None;
    let mut reached = Depth::Headers;
    let mut network = None;
    let mut chain_out = chain::ChainOutput::default();
    if opts.depth >= Depth::Shape {
        match (&opts.network, &analysis.program) {
            (None, _) => stopped_at = Some("network not given (--network <id>)".into()),
            (Some(_), None) => stopped_at = Some("no program to judge: the convert stage is blocked".into()),
            (Some(id), Some(program)) => {
                let net = chain::PreflightNetwork::parse(id)?;
                chain_out = chain::judge(&net, opts, program, &analysis, &src);
                network = Some(chain_out.network.clone());
                reached = Depth::Shape;
            }
        }
    }
    if opts.depth == Depth::Full && stopped_at.is_none() {
        stopped_at = Some(
            "full depth needs the weights and a pack: palw-tir-fidelity, palw-class pack build, then palw-class pack verify".into(),
        );
    }

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
    Ok(Report {
        schema: PREFLIGHT_SCHEMA_V1,
        mode: "model",
        input: InputInfo { kind, label: src.label.clone(), bytes_read: src.bytes_read },
        depth: DepthInfo { requested: opts.depth, reached, stopped_at },
        network,
        source: model::source_info(&src),
        model: analysis.model.clone(),
        scope: analysis.scope.clone(),
        storage: analysis.storage.clone(),
        tensors: analysis.tensors.clone(),
        artifact: analysis.artifact.clone(),
        residency,
        admission: chain_out.admission.clone(),
        chain: chain_out.conditions.clone(),
        seat: chain_out.seat.clone(),
        forecast: chain_out.forecast.clone(),
        verdict: Verdict { convert, register, mine },
        notes,
        registries: registries(reg),
    })
}
