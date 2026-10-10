//! **The convert stage of a preflight** (RFC-0002 Part II §II.2.3 items 1–5): the generic frontend run over
//! the headers — features, scope, storage and its descriptors, the tensor check against the program's
//! parameters, the artifact estimate — and the blockers each of them raises.
//!
//! Nothing here reads tensor data except the few small role tensors a described format reads for its own
//! parameters (`DescribedSource::new`), and only when they are on disk.

use super::source::{HeaderSource, InputKind, Source};
use super::{Blocker, Options, Stage};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_lower::fidelity::{Prepared, prepare_spec};
use misaka_palw_tir_lower::gguf::GgufFile;
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, MissingItem, ReadOptions, TensorIndex, read_model_with};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::model::{
    ArchitectureReport, FeatureReport, FeatureScope, FeatureStatus, ReportResult, analyze_headers_with, spec_digest,
};
use misaka_palw_tir_lower::quantfmt::{QuantRegistry, known_undescribed_method};
use misaka_palw_tir_lower::weights::{check_names, check_weights};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// What the model is, as the frontend read it.
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    /// Informational only: nothing is selected by it.
    pub model_type: Option<String>,
    pub architectures: Vec<String>,
    /// `A` (the standard keys), `B` (a data adapter maps the rest), `C` (a capability is missing).
    pub level: String,
    pub adapter: misaka_palw_tir_lower::hf_schema::AdapterSource,
    pub features: Vec<FeatureReport>,
    pub assumed_defaults: Vec<String>,
    pub unmapped_config_keys: Vec<String>,
    pub missing: Vec<MissingItem>,
    pub new_consensus_primitive_required: bool,
    pub new_court_kernel_required: bool,
    pub lowerable: bool,
    pub reason: Option<String>,
    /// The identity of the spec the lowering starts from (equal digests are the same function whatever the names).
    pub spec_digest: Option<String>,
    /// The history bound the program was lowered with.
    pub history_bound: u32,
}

/// One storage type of the checkpoint and what reads it.
#[derive(Clone, Debug, Serialize)]
pub struct StorageRow {
    /// A safetensors dtype (`BF16`, `I32`, `F8_E4M3`) or a GGUF type (`Q4_K`, `type40`).
    pub storage: String,
    pub tensors: usize,
    pub bytes: u64,
    /// `float`, `described`, `no_descriptor`, `known_undescribed`, `refused` or `other` (not a weight type).
    pub status: String,
    pub descriptor: Option<DescriptorRef>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DescriptorRef {
    pub name: String,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StorageInfo {
    /// The `quantization_config` the checkpoint announces, `quant_method` and all (HF), when it has one.
    pub quant_method: Option<String>,
    /// The descriptor that reads it.
    pub quant_descriptor: Option<DescriptorRef>,
    pub rows: Vec<StorageRow>,
}

/// The checkpoint's tensors against the program's parameters.
#[derive(Clone, Debug, Serialize)]
pub struct TensorsInfo {
    /// `shapes` (every header read), `names` (an index only), `none`, or why it could not run.
    pub checked: String,
    pub bound: usize,
    pub missing: Vec<String>,
    pub missing_total: usize,
    pub shape_mismatch: Vec<String>,
    pub shape_mismatch_total: usize,
    /// Tensors whose shape a described format derives from the data of a small role tensor (at most 4 KiB) that is not
    /// on disk: the shape could not be checked, and is not reported as wrong. The names are the role tensors to fetch.
    pub unverified: Vec<String>,
    pub unverified_total: usize,
    pub unused: Vec<String>,
    pub unused_total: usize,
    pub unused_bytes: u64,
}

/// What converting the model would produce and fetch.
#[derive(Clone, Debug, Serialize)]
pub struct ArtifactInfo {
    /// The program's parameters as the lowering stores them (integers, per-row scales, tables).
    pub params_bytes: u64,
    pub program_bytes: u64,
    pub tokenizer_bytes: u64,
    pub estimate_bytes: u64,
    /// The inventory's leaves (32 KiB at most each), estimated: the depth of the root's paths.
    pub inventory_leaves_estimate: u64,
    /// The bytes of the tensors the class reads (what a download must fetch), when every header was read.
    pub download_bytes_needed: Option<u64>,
    /// The checkpoint's weight bytes in all.
    pub download_bytes_total: Option<u64>,
    /// The bytes of tensors the feature scope leaves out (a vision tower): need not be fetched.
    pub left_out_bytes: u64,
    pub note: String,
}

/// **A data-route class** (RFC-0003) the convert stage lowered: what the pipeline admission is asked of ([`super::pipeline`]).
#[derive(Clone, Debug)]
pub enum RoutedClass {
    /// An encoder–decoder whose two stages lowered at the declared source and target lengths.
    EncDec(Box<misaka_palw_tir_lower::model::route::EncDecStagesV1>),
    /// A route whose class this build cannot declare shape-only, and why (never a pass).
    Undeclarable { kind: String, adapter: String, why: String },
    /// **A bidirectional encoder's class** (HFX 2026-10-08): an embedding, a sequence classifier or a token head, lowered shape-only to
    /// its one-stage pipeline and judged by the generative lane's admission. An embedding is an `Embedding`-profile class (RFC-0003
    /// §II.3, `palw_gen_v1`); a head's task profile is the dormant `Head` profile's (`task-heads-profile-v1.md`), judged as that
    /// profile would judge it and refused by name while its fence is not armed.
    Encoder(Box<misaka_palw_tir_lower::model::route::BidirClassShapeV1>),
    /// **An image classifier's class** (HFX 2026-10-10): a vision tower or a convolutional network ending in its classifier, lowered
    /// shape-only to a one-stage pipeline over the job's canonical image.
    Image(Box<misaka_palw_tir_lower::model::route::ImageClassShapeV1>),
}

/// **Why a bidirectional encoder's class did not lower at `lmax` padded positions, as a blocker.** The IR's element cap (no node holds
/// more than 2^28 elements: an 8,192-position encoder's attention scores) is a SIZE limit of the declared context, `SHAPE_OVER_CAP`
/// (the census's `CONTEXT_BOUND`, a resource refusal) — not the architecture's; any other failure is `ARCH_REFUSED`.
pub fn encoder_lowering_blocker(error: &str, lmax: u32) -> Blocker {
    if error.contains("more than 2^28 elements") {
        return Blocker::new(
            Stage::Convert,
            "SHAPE_OVER_CAP",
            "the bidirectional encoder's class at the declared context has a node over the IR's 2^28-element cap",
        )
        .arg("2^28 elements")
        .evidence([format!("{lmax} padded positions: {}", short(error))])
        .safe([
            "a narrower context (--max-context) lowers; the class then declares that context, not the model's".to_string(),
            "a tiled encoder court (one close per tile of rows) would not hold the whole score matrix in one node".to_string(),
        ]);
    }
    Blocker::new(Stage::Convert, "ARCH_REFUSED", "the bidirectional encoder's class cannot be lowered at the declared context")
        .evidence([format!("{lmax} padded positions: {}", short(error))])
}

/// Everything the convert stage learned.
pub struct Analysis {
    pub model: Option<ModelInfo>,
    pub scope: Option<FeatureScope>,
    pub storage: StorageInfo,
    pub tensors: TensorsInfo,
    pub artifact: Option<ArtifactInfo>,
    pub blockers: Vec<Blocker>,
    pub notes: Vec<String>,
    /// The shape-only lowering, when the convert stage got that far.
    pub program: Option<TirProgramV1>,
    /// The tokenizer is present (or its absence is not known).
    pub tokenizer_known: Option<bool>,
    /// The data-route class lowered for the pipeline admission (only when `Options::pipeline_admission` is set).
    pub routed: Option<RoutedClass>,
}

const CAP: usize = 24;

fn capped(mut v: Vec<String>) -> Vec<String> {
    v.truncate(CAP);
    v
}

/// The quantisation block of a configuration, as the lowering reads it (`misaka_palw_tir_lower::prequant::quant_block`: a
/// `quantization_config`, or MLX's `quantization` block named `quant_method: "mlx"`), else the text decoder's
/// `quantization_config` when the model nests it. A block the lowering refuses (a `quantization` that is not MLX's, two that
/// disagree) is not a storage description: the lowering's own refusal names it.
fn quantization_config(config: &serde_json::Value) -> Option<serde_json::Value> {
    let root = config.as_object()?;
    match misaka_palw_tir_lower::prequant::quant_block(root) {
        Ok(Some(q)) => Some(q),
        Ok(None) => config.get("text_config").and_then(|t| t.get("quantization_config")).filter(|q| !q.is_null()).cloned(),
        Err(_) => None,
    }
}

fn float_dtype(d: &str) -> bool {
    matches!(d, "F32" | "F16" | "BF16" | "F64")
}

fn descriptor_ref(f: &misaka_palw_tir_lower::quantfmt::QuantFormat) -> DescriptorRef {
    DescriptorRef { name: f.name().to_string(), digest: f.digest_hex() }
}

fn dtype_descriptor(dtype: &str, reg: &QuantRegistry) -> Option<DescriptorRef> {
    reg.all().iter().find(|f| f.name().eq_ignore_ascii_case(dtype)).map(|f| descriptor_ref(f))
}

/// The cap of a message in evidence.
fn short(s: &str) -> String {
    const N: usize = 600;
    if s.len() <= N { s.to_string() } else { format!("{}…", s.chars().take(N).collect::<String>()) }
}

/// Run the convert stage over a source.
pub fn analyze(src: &Source, opts: &Options, reg: &QuantRegistry, adapter_text: Option<&str>) -> Analysis {
    let mut blockers: Vec<Blocker> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // **The head a task reads** (HFX 2026-10-10): a `…ForMaskedLM` checkpoint is read as its encoder (a sentence embedder) unless the
    // task is `fill-mask`, which reads the checkpoint's own head (`hf_schema::masked_lm_adapter_for`). An adapter the caller supplies wins.
    let masked_lm = (adapter_text.is_none() && opts.task.as_deref() == Some("fill-mask"))
        .then(|| src.config.as_ref().and_then(misaka_palw_tir_lower::hf_schema::masked_lm_adapter_for))
        .flatten();
    let read_opts = ReadOptions {
        adapter: match (adapter_text, masked_lm) {
            (Some(t), _) => AdapterChoice::Text(t.to_string()),
            (None, Some(id)) => AdapterChoice::BuiltIn(id.to_string()),
            (None, None) => AdapterChoice::default(),
        },
    };
    let history_bound =
        if opts.held { misaka_palw_tir::program::HISTORY_BOUND_V1_HELD } else { misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL };
    // **A declared context bounds the history window** (`LowerOpts::max_window`, the runtime pack's `max_window`): every job of a
    // class at C positions reads at most C rows, so a window of C computes what the model's own (wider) window computes on every
    // job the class admits, and its state and per-position work are those of C, not of the history bound. Admission checks the
    // window covers the declared context (`tir_window_covers_context_v1`). With no declared context the model's windows are kept.
    let lopts = LowerOpts { history_bound, max_window: opts.max_context, ..Default::default() };

    // ---- completeness of the source -------------------------------------------------------------------------------
    if !src.missing_shards.is_empty() {
        notes.push(format!(
            "{} shard(s) are not in the directory ({}): their tensors are known by name from the index, not by shape",
            src.missing_shards.len(),
            src.missing_shards.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    let partial: Vec<&str> = src.shards.iter().filter(|s| !s.complete()).map(|s| s.file.as_str()).collect();
    if !partial.is_empty() {
        notes.push(format!(
            "{} shard file(s) hold a header and not all of their data ({}): a preflight reads headers, and a conversion needs the whole files",
            partial.len(),
            partial.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if src.gguf_truncated {
        notes.push(
            "the GGUF holds its header and not all of its data: a preflight reads the header, and a conversion needs the whole file"
                .into(),
        );
    }

    // Tensors whose values the mapping reads and a header-only view cannot hold (a GGUF's `rope_freqs.weight`): the program's structure
    // — every bound a static admission checks — does not depend on them, so the verdict stands; the values are read at conversion.
    if let Some(g) = &src.gguf_model {
        for p in g.pending_tensor_data() {
            notes.push(format!(
                "{p}: its values are read at conversion; the program's structure does not depend on them (ROPE_FREQ_FACTORS_V1), so this verdict stands for any table"
            ));
        }
        if !g.inert_keys().is_empty() {
            notes.push(format!(
                "GGUF_INERT_PROVENANCE_V1: {} metadata key(s) ignored as a publisher's bookkeeping ({}); the registry's digest {}",
                g.inert_keys().len(),
                g.inert_keys().join(", "),
                &misaka_palw_tir_lower::gguf::gguf_inert_registry_digest_v1()[..16]
            ));
        }
    }

    // A weight file the frontend refuses by its FORM (a PyTorch pickle that is not a plain state dict, a global off the allowlist, a
    // legacy serialization): named, with the safe path — never "the tensors are missing".
    if let Some(why) = &src.weights_refusal {
        blockers.push(
            Blocker::new(
                Stage::Convert,
                "FORMAT_UNSUPPORTED",
                "the weights are a file form this build does not read (a PyTorch pickle is interpreted, never run)",
            )
            .evidence([short(why)])
            .safe([
                "re-save the checkpoint with safetensors (`model.save_pretrained(..., safe_serialization=True)`)".to_string(),
                "a plain `torch.save(model.state_dict())` of contiguous tensors is read; a training checkpoint, a strided view or the legacy format is not".to_string(),
            ]),
        );
    }

    // ---- the configuration, the features, the scope -----------------------------------------------------------------
    let mut model: Option<ModelInfo> = None;
    let mut scope: Option<FeatureScope> = None;
    let mut lowerable = false;
    let mut report: Option<ArchitectureReport> = None;
    let tindex: Option<TensorIndex> = src.tensor_index();
    match (&src.config, &src.config_error) {
        (Some(config), _) => {
            let r = analyze_headers_with(config, tindex.as_ref(), &read_opts, reg, &src.file_names());
            lowerable = r.result == ReportResult::Lowerable;
            scope = Some(r.scope.clone());
            report = Some(r);
        }
        (None, why) => {
            let why = why.clone().unwrap_or_else(|| "no configuration".into());
            if src.kind == InputKind::Gguf && why.contains("GGUF LoRA adapter") {
                // The file is not a model: it is an adapter over one (llama.cpp's `convert_lora_to_gguf`). Composing a GGUF adapter with its
                // base is not built; it is not an architecture the frontend failed to map.
                blockers.push(
                    Blocker::new(Stage::Convert, "ADAPTER_REFUSED", "the GGUF is a LoRA adapter, which this build does not compose with a base")
                        .evidence([short(&why)])
                        .safe([
                            "a PEFT adapter (adapter_config.json and adapter_model.safetensors) over its Hugging Face base is composed".to_string(),
                        ]),
                );
            } else if src.kind == InputKind::Gguf {
                blockers.push(
                    Blocker::new(
                        Stage::Convert,
                        "ARCH_REFUSED",
                        "the GGUF's architecture has no mapping to a Hugging Face configuration",
                    )
                    .evidence([short(&why)])
                    .safe([
                        "convert the model's original Hugging Face checkpoint instead".to_string(),
                        "supply a data adapter (--adapter) when the architecture is a combination of known features".to_string(),
                    ]),
                );
            } else {
                blockers.push(Blocker::new(Stage::Convert, "CONFIG_INVALID", "config.json cannot be read").evidence([short(&why)]));
            }
        }
    }

    // ---- the storage, and its descriptors ------------------------------------------------------------------------------
    let (storage, quant_blockers) = storage_of(src, reg);
    blockers.extend(quant_blockers);

    // ---- a kind the decoder pipeline does not run: its RFC-0003 route (a vision tower, a convolutional network, an encoder–decoder, a
    // diffusers component) is read, lowered to its version-2 programs and admitted, and that is the convert stage's verdict ------------
    let mut routed_ok = false;
    let mut routed: Option<RoutedClass> = None;
    let is_route = src.gguf_model.is_none() && src.config.as_ref().is_some_and(misaka_palw_tir_lower::model::route::is_data_route);
    if let (Some(config), None) = (&src.config, &src.gguf_model)
        && misaka_palw_tir_lower::model::route::is_data_route(config)
    {
        let dir = (src.kind == InputKind::HfDirectory).then(|| std::path::PathBuf::from(&src.label)).filter(|d| d.is_dir());
        let probe = misaka_palw_tir_lower::model::route::probe_data_route(config, tindex.as_ref(), dir.as_deref());
        if probe["ok"] == serde_json::json!(true) {
            routed_ok = true;
            let programs = probe["programs"].as_array().cloned().unwrap_or_default();
            notes.push(format!(
                "a {} class (adapter {}): lowered to {} RFC-0003 program(s), each admitted ({}); it registers as a pipeline class, {}",
                probe["kind"].as_str().unwrap_or("?"),
                probe["adapter"].as_str().unwrap_or("?"),
                programs.len(),
                programs
                    .iter()
                    .map(|p| format!("{} nodes, {:.0} MACs", p["nodes"], p["macs"].as_f64().unwrap_or(0.0)))
                    .collect::<Vec<_>>()
                    .join("; "),
                if opts.pipeline_admission {
                    "judged below by the generative lane's admission (RFC-0003)"
                } else {
                    "whose rules are the generative lane's (RFC-0003) and are not judged here"
                }
            ));
            if opts.pipeline_admission {
                let kind = probe["kind"].as_str().unwrap_or("?").to_string();
                let adapter = probe["adapter"].as_str().unwrap_or("?").to_string();
                if kind == "encdec" {
                    let ctx = opts.max_context.unwrap_or_else(|| pipeline_default_context(config));
                    match misaka_palw_tir_lower::model::route::lower_encdec_stages_v1(config, tindex.as_ref(), ctx, ctx) {
                        Ok(misaka_palw_tir_lower::model::route::EncDecShapeV1::Stages(st)) => routed = Some(RoutedClass::EncDec(st)),
                        Ok(misaka_palw_tir_lower::model::route::EncDecShapeV1::NotDeclarable(why)) => {
                            routed = Some(RoutedClass::Undeclarable { kind, adapter, why })
                        }
                        Err(e) => blockers.push(
                            Blocker::new(
                                Stage::Convert,
                                "ARCH_REFUSED",
                                "the model's data route cannot be lowered at the declared context",
                            )
                            .evidence([format!("source and target of {ctx} positions: {}", short(&e))]),
                        ),
                    }
                } else if kind == "vision" || kind == "cnn" {
                    // An image classifier is declared shape-only (HFX 2026-10-10); a backbone is not.
                    routed = Some(match misaka_palw_tir_lower::model::route::lower_image_class_shape_v1(config) {
                        Ok(misaka_palw_tir_lower::model::route::ImageShapeV1::Class(shape)) => RoutedClass::Image(shape),
                        Ok(misaka_palw_tir_lower::model::route::ImageShapeV1::NotDeclarable(why)) => {
                            RoutedClass::Undeclarable { why, kind, adapter }
                        }
                        Err(why) => RoutedClass::Undeclarable { why: format!("the image class cannot be lowered shape-only: {why}"), kind, adapter },
                    });
                } else {
                    routed = Some(RoutedClass::Undeclarable {
                        why: format!(
                            "a {kind} route declares an Embedding- or Image-profile class, which lowers from the checkpoint's weights (calibration); this build declares no such class from headers"
                        ),
                        kind,
                        adapter,
                    });
                }
            }
        } else {
            blockers.push(
                Blocker::new(Stage::Convert, "ARCH_REFUSED", "the model's data route (RFC-0003 programs) refuses it")
                    .evidence([short(probe["error"].as_str().unwrap_or("no reason given"))]),
            );
            routed_ok = false;
        }
        // Either way the decoder pipeline's own refusal is not the verdict of a kind it does not run.
        report = report.map(|mut r| {
            r.result = ReportResult::Lowerable;
            r.missing.clear();
            r
        });
        lowerable = false;
    }

    // ---- the frontend's verdict ------------------------------------------------------------------------------------------
    if let Some(r) = &report {
        if let ReportResult::NotLowerable { reason } = &r.result {
            for m in &r.missing {
                blockers.push(
                    Blocker::new(
                        Stage::Convert,
                        "ARCH_NEEDS_FEATURE",
                        format!("the model needs `{}`, which the generic lowerer does not lower yet", m.what),
                    )
                    .arg(m.what.clone())
                    .evidence([m.why.clone()])
                    .safe(
                        m.general_primitive.iter().map(|g| format!("the smallest general addition that closes it: {g}")).chain(
                            std::iter::once(
                                "a data adapter (--adapter) can map a missing key or tensor name, never a missing computation"
                                    .to_string(),
                            ),
                        ),
                    ),
                );
            }
            for k in &r.unmapped_config_keys {
                blockers.push(
                    Blocker::new(
                        Stage::Convert,
                        "CONFIG_KEY_UNREAD",
                        "a configuration key that might change the math has no rule (refused, never ignored)",
                    )
                    .arg(k.clone())
                    .safe(["a data adapter (--adapter) that maps the key, if it is a renaming".to_string()]),
                );
            }
            if r.missing.is_empty() && r.unmapped_config_keys.is_empty() && !quant_refusal_in_reason(reason) {
                let (code, what) = if reason.contains("bad config") {
                    ("CONFIG_INVALID", "the configuration is malformed")
                } else if reason.contains("remote code") || reason.contains("trust_remote_code") || reason.contains("auto_map") {
                    ("REMOTE_CODE", "the architecture is defined by remote code")
                } else {
                    ("ARCH_REFUSED", "the generic frontend refuses this architecture")
                };
                blockers.push(Blocker::new(Stage::Convert, code, what).evidence([short(reason)]));
            }
        }
        model = Some(ModelInfo {
            model_type: r.model_type.clone(),
            architectures: r.architectures.clone(),
            level: r.level.to_string(),
            adapter: r.adapter.clone(),
            features: r.features.clone(),
            assumed_defaults: r.assumed_defaults.clone(),
            unmapped_config_keys: r.unmapped_config_keys.clone(),
            missing: r.missing.clone(),
            new_consensus_primitive_required: r.new_consensus_primitive_required,
            new_court_kernel_required: r.new_court_kernel_required,
            lowerable,
            reason: match &r.result {
                ReportResult::NotLowerable { reason } => Some(short(reason)),
                ReportResult::Lowerable => None,
            },
            spec_digest: None,
            history_bound,
        });
        for f in r.features.iter().filter(|f| f.status == FeatureStatus::Missing && !is_route) {
            // Reported as blockers above through `missing`; a feature without a `missing` row still blocks.
            if !r.missing.iter().any(|m| m.what == f.id)
                && !blockers.iter().any(|b| b.code == "ARCH_NEEDS_FEATURE" && b.arg.as_deref() == Some(&f.id))
            {
                blockers.push(
                    Blocker::new(
                        Stage::Convert,
                        "ARCH_NEEDS_FEATURE",
                        format!("the model needs `{}`, which the generic lowerer does not lower yet", f.id),
                    )
                    .arg(f.id.clone())
                    .evidence(f.capability.iter().cloned()),
                );
            }
        }
        if let Some(sc) = &scope
            && sc.text_only
        {
            {
                notes.push(format!(
                    "text stage only on testnet-12: {} — a vision-language or audio model registers as its text decoder; the other parts need RFC-0003's generative class",
                    sc.modalities_left_out().join(", ")
                ));
            }
        }
    }

    // ---- the shape-only lowering and the tensor check ------------------------------------------------------------------------------
    let mut prepared: Option<Prepared> = None;
    let mut digest: Option<String> = None;
    if lowerable && let Some(config) = &src.config {
        let prep: Result<Prepared, String> = match &src.gguf_model {
            Some(m) => m.prepare(&lopts).map_err(|e| e.to_string()),
            None => read_model_with(config, tindex.as_ref(), &read_opts, reg).map_err(|f| f.to_string()).and_then(|mut read| {
                // A LoRA adapter over the model (RFC-0004): attached unmerged, every adapter tensor accounted for.
                if let Some(ad) = &opts.lora {
                    misaka_palw_tir_lower::lora::attach(&mut read.spec, &ad.config).map_err(|e| e.to_string())?;
                    misaka_palw_tir_lower::lora::check_adapter_tensors(&read.spec, &ad.tensors).map_err(|e| e.to_string())?;
                }
                digest = Some(spec_digest(&read.spec));
                prepare_spec(read.spec, &lopts).map_err(|e| e.to_string())
            }),
        };
        match prep {
            Ok(p) => prepared = Some(p),
            Err(e) if e.contains("LoRA adapter:") => blockers.push(
                Blocker::new(Stage::Convert, "ADAPTER_REFUSED", "the LoRA adapter is not one the lowering attaches")
                    .evidence([short(&e)]),
            ),
            Err(e) => {
                if !quant_refusal_in_reason(&e) {
                    blockers
                        .push(Blocker::new(Stage::Convert, "ARCH_REFUSED", "the lowering refuses this model").evidence([short(&e)]));
                }
            }
        }
    }
    if let Some(m) = model.as_mut() {
        m.spec_digest = digest.clone();
        if prepared.is_none() && lowerable {
            m.lowerable = false;
        }
    }
    let (tensors, unused_set, ignored) = match &prepared {
        Some(p) => tensor_check(src, p, reg, &mut blockers, &mut notes),
        None => (
            TensorsInfo {
                checked: "none (no program to bind the tensors to)".into(),
                bound: 0,
                missing: vec![],
                missing_total: 0,
                shape_mismatch: vec![],
                shape_mismatch_total: 0,
                unverified: vec![],
                unverified_total: 0,
                unused: vec![],
                unused_total: 0,
                unused_bytes: 0,
            },
            BTreeSet::new(),
            Vec::new(),
        ),
    };
    // **A key ignored because the transformers class never reads it must not be a sign of a different model.** The reader ignored these
    // keys on the strength of the reference being that class (`hf_schema::hf_keys`); a checkpoint that carries tensors the class does
    // not have (a `q_norm` beside `use_qk_norm`, an expert stack beside `moe_intermediate_size`) says its author's model is not that
    // class, so the ignoring is withdrawn: the key is refused as it always was.
    let ignored_keys: Vec<String> = model
        .as_ref()
        .map(|m| m.assumed_defaults.iter().filter_map(|a| a.strip_prefix("ignored `").and_then(|r| r.split('`').next()).map(str::to_string)).collect())
        .unwrap_or_default();
    if !ignored_keys.is_empty() && tensors.unused_total > 0 {
        blockers.push(
            Blocker::new(
                Stage::Convert,
                "CONFIG_KEY_UNREAD",
                "a configuration key the transformers class never reads was ignored, and the checkpoint carries tensors the class does not have: it is not that class",
            )
            .arg(ignored_keys[0].clone())
            .evidence(
                std::iter::once(format!("ignored keys: {}", ignored_keys.join(", ")))
                    .chain(std::iter::once(format!("{} tensor(s) no parameter of the program reads, the first {}", tensors.unused_total, tensors.unused.first().cloned().unwrap_or_default()))),
            )
            .safe(["a data adapter (--adapter) that models the key and its tensors".to_string()]),
        );
    }
    let mut storage = storage;
    // A weight stored as a type no descriptor claims: the bound tensors of an `other` row.
    if prepared.is_some() && tensors.checked != "none" {
        for row in storage.rows.iter_mut().filter(|r| r.status == "other") {
            let bound = bound_in_dtype(src, &row.storage, &unused_set, &ignored);
            if bound > 0 {
                row.status = "no_descriptor".into();
                row.note = Some(format!(
                    "{bound} tensor(s) the class reads are stored as {} and no quantization_config describes it",
                    row.storage
                ));
                blockers.push(
                    Blocker::new(
                        Stage::Convert,
                        "QUANT_NO_DESCRIPTOR",
                        format!("{bound} tensor(s) the class reads are stored as {}, and no descriptor reads that type", row.storage),
                    )
                    .arg(format!("safetensors/{}", row.storage))
                    .safe([
                        "the model's original (float) checkpoint, if it publishes one".to_string(),
                        "a descriptor file for the format (--quant-format <file.json>): a data change, no code".to_string(),
                    ]),
                );
            }
        }
    }

    // ---- the tokenizer ------------------------------------------------------------------------------------------------------------
    let tokenizer_known = match src.kind {
        InputKind::HfDirectory => {
            Some(src.files.iter().any(|f| misaka_palw_tir_lower::artifact::is_tokenizer_file(&f.name)))
        }
        InputKind::Gguf => src.gguf_file.as_ref().map(|g| g.meta.contains_key("tokenizer.ggml.tokens")),
        _ => None,
    };
    let needs_tokenizer = !is_route || src.config.as_ref().is_some_and(misaka_palw_tir_lower::hf_schema::is_encoder_decoder);
    // A tokenizer the pinned base supplies (the class commits to the bytes of that file), accepted only for the same vocabulary.
    let declared_vocab = src.config.as_ref().and_then(|c| {
        c.get("vocab_size").or_else(|| c.get("text_config").and_then(|t| t.get("vocab_size"))).and_then(serde_json::Value::as_u64)
    });
    let base_tokenizer = opts
        .base_tokenizer
        .as_ref()
        .filter(|bt| src.kind == InputKind::HfDirectory && declared_vocab == Some(bt.vocab_size));
    if tokenizer_known == Some(false) && needs_tokenizer && base_tokenizer.is_some() {
        let bt = base_tokenizer.expect("checked");
        notes.push(format!(
            "TOKENIZER_BOUND_FROM_BASE: no tokenizer file beside the checkpoint; the class binds the tokenizer of its pinned base `{}` (its configuration declares vocab_size {} = this model's): the registrant commits to the bytes of that one file",
            bt.base, bt.vocab_size
        ));
    } else if tokenizer_known == Some(false) && needs_tokenizer {
        blockers.push(
            Blocker::new(
                Stage::Convert,
                "TOKENIZER_MISSING",
                "no tokenizer file beside the checkpoint: the class commits to a tokenizer id",
            )
            .safe([
                "download tokenizer.json (a few MB) from the model repository".to_string(),
                "or bind the tokenizer of the base model it was fine-tuned from: the class commits to the bytes of one tokenizer file".to_string(),
            ]),
        );
    }

    // ---- a bidirectional encoder (an embedding, a sequence classifier, a token head): an RFC-0003 pipeline class --------------------
    // Its class is the one-stage pipeline over the padded token axis, not the per-position program above: it is declared shape-only and
    // judged by the generative lane's admission (`super::pipeline`), as an encoder–decoder is.
    let mut encoder_routed = false;
    if opts.pipeline_admission
        && routed.is_none()
        && let (Some(p), Some(config)) = (&prepared, &src.config)
        && misaka_palw_tir_lower::model::route::bidir_head_of(&p.spec).is_some()
        && p.spec.features().iter().any(|f| f.id.0 == "ENC_BIDIR_V1")
    {
        let lmax = opts.max_context.unwrap_or_else(|| pipeline_default_context(config));
        // A sentence-transformers repository says its pooling and normalisation; without its files, the mean (the costlier mode).
        let dir = (src.kind == InputKind::HfDirectory).then(|| std::path::PathBuf::from(&src.label)).filter(|d| d.is_dir());
        let st = dir.as_deref().and_then(|d| misaka_palw_tir_lower::encoder::sentence_transformers(d).ok().flatten());
        let mean = st.as_ref().is_none_or(|s| s.pooling == misaka_palw_tir_lower::encoder::StPooling::Mean);
        let normalize = st.as_ref().is_some_and(|s| s.normalize);
        match misaka_palw_tir_lower::model::route::lower_bidir_class_shape_v1(&p.spec, &p.hl, config, lmax, mean, normalize) {
            Ok(shape) => {
                notes.push(format!(
                    "a bidirectional encoder class ({} head, [{}, {}] output at {} padded positions): lowered to 1 RFC-0003 program; it registers as a pipeline class, {}{}",
                    shape.head.name(),
                    shape.rows,
                    shape.width,
                    shape.lmax,
                    "judged below by the generative lane's admission (RFC-0003)",
                    shape.pair_sep.map_or(String::new(), |s| format!(
                        "; pair segments ({}): the segment ids are computed in the program from the job's ids (separator id {s})",
                        misaka_palw_tir_lower::model::route::ENC_PAIR_SEGMENTS_V1
                    ))
                ));
                routed = Some(RoutedClass::Encoder(Box::new(shape)));
                encoder_routed = true;
            }
            Err(e) => blockers.push(encoder_lowering_blocker(&e.to_string(), lmax)),
        }
    }

    // ---- the artifact estimate ------------------------------------------------------------------------------------------------------
    let program = prepared.as_ref().filter(|_| !encoder_routed).map(|p| p.lowered.program.clone());
    let artifact = program.as_ref().map(|p| artifact_of(src, p, &scope, &tensors, &unused_set, &ignored));
    Analysis { model, scope, storage, tensors, artifact, blockers, notes, program, tokenizer_known, routed }
}

/// The declared context of a pipeline class when none is asked for: the model's declared positions, at most 1,024 (an encoder–decoder's
/// attention is quadratic in it), at least 16; 512 when the configuration declares none.
fn pipeline_default_context(config: &serde_json::Value) -> u32 {
    crate::census::gates::declared_positions(config).map(|(v, _)| v.min(1_024) as u32).unwrap_or(512).max(16)
}

/// Whether a refusal's text is the quantisation's (already raised as its own blocker).
fn quant_refusal_in_reason(reason: &str) -> bool {
    reason.contains("quant-format descriptor") || reason.contains("quantization_config") || reason.contains("pre-quantized checkpoint")
}

/// The storage histogram and the quantisation blockers.
fn storage_of(src: &Source, reg: &QuantRegistry) -> (StorageInfo, Vec<Blocker>) {
    let mut blockers = Vec::new();
    let mut quant_method = None;
    let mut quant_descriptor = None;
    let mut rows: Vec<StorageRow> = Vec::new();

    if let Some(g) = &src.gguf_file {
        rows = gguf_rows(g);
        for n in g.needs_descriptors() {
            let known = reg.names();
            let id = n.id.map(|i| i.to_string()).unwrap_or_default();
            blockers.push(
                Blocker::new(
                    Stage::Convert,
                    "QUANT_NO_DESCRIPTOR",
                    format!(
                        "GGUF tensor type {id}{} has no quant-format descriptor: {} tensor(s) need it",
                        n.name.as_ref().map(|x| format!(" ({x})")).unwrap_or_default(),
                        n.tensors.len()
                    ),
                )
                .arg(format!("ggml/{id}"))
                .evidence(capped(n.tensors.clone()))
                .safe([
                    "the model's original safetensors checkpoint, converted by this tool".to_string(),
                    format!("re-quantise it to a described type ({})", known.join(", ")),
                    "a descriptor file for the type (--quant-format <file.json>) once its layout is specified: a data change, no code"
                        .to_string(),
                ]),
            );
        }
    } else {
        // Safetensors dtypes.
        let mut by: BTreeMap<String, (usize, u64)> = BTreeMap::new();
        for s in &src.shards {
            for e in s.entries.values() {
                let r = by.entry(e.dtype.clone()).or_default();
                r.0 += 1;
                r.1 += e.bytes;
            }
        }
        let q = src.config.as_ref().and_then(quantization_config);
        let mut described: Option<DescriptorRef> = None;
        if let Some(q) = q.as_ref() {
            let method = q.get("quant_method").and_then(|m| m.as_str()).unwrap_or("unknown").to_ascii_lowercase();
            let name = match q.get("format").and_then(|f| f.as_str()) {
                Some(f) => format!("{method}/{f}"),
                None => method.clone(),
            };
            quant_method = Some(name.clone());
            let cfg = src.config.as_ref().unwrap();
            let arch = cfg.get("architectures").and_then(|a| a.get(0)).and_then(|a| a.as_str()).unwrap_or("?").to_string();
            let model_type = cfg.get("model_type").and_then(|a| a.as_str()).unwrap_or("").to_string();
            match misaka_palw_tir_lower::prequant::parse_quant_config_with(q, &arch, &model_type, reg) {
                Ok(c) => {
                    if let Some((f, _)) = c.fmt.binding() {
                        described = Some(descriptor_ref(&f));
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("has no quant-format descriptor") {
                        if let Some(k) = known_undescribed_method(&method) {
                            blockers.push(
                                Blocker::new(
                                    Stage::Convert,
                                    "QUANT_KNOWN_UNDESCRIBED",
                                    format!("`{}` ({}) is known and not yet described: {}", k.method, k.title, k.note),
                                )
                                .arg(method.clone())
                                .evidence([format!("status {}", k.status)])
                                .safe([
                                    "the model's original (float) checkpoint, if it publishes one".to_string(),
                                    format!("re-quantise it to a described type ({})", reg.names().join(", ")),
                                ]),
                            );
                        } else {
                            blockers.push(
                                Blocker::new(
                                    Stage::Convert,
                                    "QUANT_NO_DESCRIPTOR",
                                    format!("quantization_config quant_method={name} has no quant-format descriptor"),
                                )
                                .arg(format!("config/{name}"))
                                .evidence([format!("described methods: {}", reg.config_methods().join(", "))])
                                .safe([
                                    "the model's original (float) checkpoint, if it publishes one".to_string(),
                                    "a descriptor file for the format (--quant-format <file.json>): a data change, no code"
                                        .to_string(),
                                ]),
                            );
                        }
                    } else {
                        // A descriptor read it and refused: its own rule, by name.
                        let by = reg.config_for(&method, q).map(|f| f.name().to_string()).unwrap_or_else(|| method.clone());
                        blockers.push(
                            Blocker::new(Stage::Convert, "QUANT_REFUSED", format!("the descriptor `{by}` refuses this configuration"))
                                .arg(by)
                                .evidence([short(&msg)])
                                .safe(["the model's original (float) checkpoint, if it publishes one".to_string()]),
                        );
                    }
                }
            }
            quant_descriptor = described.clone();
        }
        for (dtype, (tensors, bytes)) in by {
            let (status, descriptor, note) = if float_dtype(&dtype) {
                ("float".to_string(), dtype_descriptor(&dtype, reg), None)
            } else if let Some(d) = &described {
                (
                    "described".to_string(),
                    Some(d.clone()),
                    Some(format!("read through the descriptor of quantization_config ({})", quant_method.clone().unwrap_or_default())),
                )
            } else if q.is_some() {
                ("no_descriptor".to_string(), None, Some("the quantization_config has no descriptor".to_string()))
            } else {
                ("other".to_string(), None, None)
            };
            rows.push(StorageRow { storage: dtype, tensors, bytes, status, descriptor, note });
        }
    }
    (StorageInfo { quant_method, quant_descriptor, rows }, blockers)
}

fn gguf_rows(g: &GgufFile) -> Vec<StorageRow> {
    let mut by: BTreeMap<u32, (usize, u64, Option<DescriptorRef>, String)> = BTreeMap::new();
    for t in g.tensors.values() {
        let e = by.entry(t.ty.id).or_insert_with(|| (0, 0, t.ty.fmt.as_ref().map(|f| descriptor_ref(f)), t.ty.name()));
        e.0 += 1;
        e.1 += g.stored_bytes(&t.name).unwrap_or(0);
    }
    let need: BTreeMap<u32, ()> = g.needs_descriptors().into_iter().filter_map(|n| n.id.map(|i| (i, ()))).collect();
    by.into_iter()
        .map(|(id, (tensors, bytes, descriptor, name))| {
            let undescribed = need.contains_key(&id);
            let status = if undescribed {
                "no_descriptor"
            } else if descriptor.as_ref().is_some_and(|d| matches!(d.name.as_str(), "F32" | "F16" | "BF16" | "F64")) {
                "float"
            } else {
                "described"
            };
            StorageRow {
                storage: if undescribed { format!("type{id}") } else { name },
                tensors,
                bytes,
                status: status.into(),
                descriptor,
                note: undescribed.then(|| format!("ggml type id {id}")),
            }
        })
        .collect()
}

/// The tensors the class reads against the program's parameters: shapes where every header was read, names where
/// only the index was.
fn tensor_check(
    src: &Source,
    prep: &Prepared,
    reg: &QuantRegistry,
    blockers: &mut Vec<Blocker>,
    notes: &mut Vec<String>,
) -> (TensorsInfo, BTreeSet<String>, Vec<String>) {
    let _ = reg;
    let ignored = prep.binding.ignored_prefixes.clone();
    let mut info = TensorsInfo {
        checked: "none".into(),
        bound: 0,
        missing: vec![],
        missing_total: 0,
        shape_mismatch: vec![],
        shape_mismatch_total: 0,
        unverified: vec![],
        unverified_total: 0,
        unused: vec![],
        unused_total: 0,
        unused_bytes: 0,
    };
    let rep = if let Some(m) = &src.gguf_model {
        info.checked = "shapes".into();
        Some(check_weights(&prep.hl, &prep.binding, m))
    } else if src.shapes_known() {
        let hs = HeaderSource::new(&src.shards);
        let no_entries = BTreeMap::new();
        let entries = prep.spec.hf.quant.as_ref().map_or(&no_entries, |q| &q.module_params);
        match prep.spec.hf.quant.as_ref().filter(|q| q.fmt.is_virtual()).and_then(|q| q.fmt.binding()) {
            Some((f, params)) => match misaka_palw_tir_lower::weights::described::DescribedSource::new_with(Box::new(hs), f, params, entries) {
                Ok(d) => {
                    info.checked = "shapes".into();
                    Some(check_weights(&prep.hl, &prep.binding, &d))
                }
                Err(e) => {
                    info.checked = format!("skipped: {}", short(&e.to_string()));
                    notes.push(format!(
                        "the packed tensors' shapes could not be derived from the headers alone ({}): the tensor check ran when their small role tensors are on disk",
                        short(&e.to_string())
                    ));
                    None
                }
            },
            None => {
                info.checked = "shapes".into();
                Some(check_weights(&prep.hl, &prep.binding, &hs))
            }
        }
    } else if let Some(names) = checkpoint_names(src) {
        info.checked = "names".into();
        Some(check_names(&prep.hl, &prep.binding, &names))
    } else {
        None
    };
    let mut unused_set = BTreeSet::new();
    if let Some(rep) = rep {
        info.bound = rep.bound;
        let (mut miss, mut shp, mut unv) = (Vec::new(), Vec::new(), Vec::new());
        for e in &rep.errors {
            if e.contains("its data is not in this file") {
                // The role tensor named last: the one whose few bytes the shape needs.
                unv.push(last_name(e));
            } else if e.contains("checkpoint gives") {
                shp.push(e.clone());
            } else {
                miss.push(e.clone());
            }
        }
        unv.sort();
        unv.dedup();
        info.missing_total = miss.len();
        info.shape_mismatch_total = shp.len();
        info.unverified_total = unv.len();
        if !unv.is_empty() {
            info.checked = format!("shapes ({} role tensor(s) whose data is not here)", unv.len());
            notes.push(format!(
                "{} tensor shape(s) a described format reads from small role tensors (at most 4 KiB each) could not be checked: fetch {}",
                unv.len(),
                unv.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        info.unverified = capped(unv);
        if !miss.is_empty() {
            blockers.push(
                Blocker::new(
                    Stage::Convert,
                    "TENSOR_MISSING",
                    format!("{} tensor(s) the program reads are not in the checkpoint", miss.len()),
                )
                .arg(first_name(&miss[0]))
                .evidence(capped(miss.clone()))
                .numbers(rep.bound as u64, (rep.bound + miss.len() + shp.len() + info.unverified_total) as u64, "tensors")
                .safe(["a data adapter (--adapter) when the checkpoint names the tensor differently".to_string()]),
            );
        }
        if !shp.is_empty() {
            blockers.push(
                Blocker::new(
                    Stage::Convert,
                    "TENSOR_SHAPE",
                    format!("{} tensor(s) have a shape the program does not read", shp.len()),
                )
                .arg(first_name(&shp[0]))
                .evidence(capped(shp.clone()))
                .safe(["a data adapter (--adapter) when the checkpoint stores the tensor transposed or reshaped".to_string()]),
            );
        }
        info.missing = capped(miss);
        info.shape_mismatch = capped(shp);
        info.unused_total = rep.unused.len();
        let bytes = unused_bytes(src, &rep.unused);
        info.unused_bytes = bytes;
        unused_set = rep.unused.iter().cloned().collect();
        info.unused = capped(rep.unused);
    }
    (info, unused_set, ignored)
}

/// The last backquoted name of a message.
fn last_name(e: &str) -> String {
    let parts: Vec<&str> = e.split('`').collect();
    // `a `x` b `y` c` splits into [a, x, b, y, c]: the names are the odd positions.
    parts.iter().enumerate().filter(|(i, _)| i % 2 == 1).map(|(_, p)| *p).next_back().unwrap_or(e).to_string()
}

/// The first backquoted name of a message (`param `X` ...` is `X`).
fn first_name(e: &str) -> String {
    let mut parts = e.split('`');
    parts.next();
    match parts.next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => e.chars().take(60).collect(),
    }
}

fn checkpoint_names(src: &Source) -> Option<BTreeSet<String>> {
    if src.shards.is_empty() && src.index_names.is_none() {
        return None;
    }
    let mut n: BTreeSet<String> = src.shards.iter().flat_map(|s| s.entries.keys().cloned()).collect();
    if let Some(i) = &src.index_names {
        n.extend(i.iter().cloned());
    }
    Some(n)
}

fn unused_bytes(src: &Source, unused: &[String]) -> u64 {
    let set: BTreeSet<&str> = unused.iter().map(String::as_str).collect();
    src.shards.iter().flat_map(|s| s.entries.iter()).filter(|(n, _)| set.contains(n.as_str())).map(|(_, e)| e.bytes).sum()
}

fn bound_in_dtype(src: &Source, dtype: &str, unused: &BTreeSet<String>, ignored: &[String]) -> usize {
    src.shards
        .iter()
        .flat_map(|s| s.entries.iter())
        .filter(|(n, e)| {
            e.dtype == dtype
                && !unused.contains(*n)
                && !ignored.iter().any(|p| n.starts_with(p.as_str()))
                // A module BUFFER (`position_ids`, `inv_freq`, …) older checkpoints saved is neither "unused" (the check skips it) nor
                // bound: it is no weight of the class. Counting it made every BERT checkpoint with an `embeddings.position_ids` (I64)
                // buffer a `QUANT_NO_DESCRIPTOR(safetensors/I64)` — the MiniLM / MPNet sentence-transformers among them.
                && !misaka_palw_tir_lower::weights::is_module_buffer(n)
        })
        .count()
}

fn artifact_of(
    src: &Source,
    program: &TirProgramV1,
    scope: &Option<FeatureScope>,
    tensors: &TensorsInfo,
    unused: &BTreeSet<String>,
    ignored: &[String],
) -> ArtifactInfo {
    let params_bytes = kaspa_consensus_core::palw_tir_work_v1::palw_tir_work_shape_v1(program)
        .map(|s| s.param_bytes().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    let program_bytes = program.encode().len() as u64;
    let tokenizer_bytes: u64 = src
        .files
        .iter()
        .filter(|f| {
            misaka_palw_tir_lower::artifact::is_tokenizer_file(&f.name)
                || matches!(f.name.as_str(), "tokenizer_config.json" | "merges.txt" | "special_tokens_map.json" | "added_tokens.json")
        })
        .map(|f| f.bytes)
        .sum();
    let left_out_bytes = scope.as_ref().map(|s| s.excluded.iter().filter_map(|e| e.bytes).sum()).unwrap_or(0);
    let total = src.declared_weight_bytes();
    let needed = if src.gguf_file.is_some() {
        total
    } else if src.shapes_known() && tensors.checked == "shapes" {
        Some(
            src.shards
                .iter()
                .flat_map(|s| s.entries.iter())
                .filter(|(n, _)| !unused.contains(*n) && !ignored.iter().any(|p| n.starts_with(p.as_str())))
                .map(|(_, e)| e.bytes)
                .sum(),
        )
    } else {
        None
    };
    ArtifactInfo {
        params_bytes,
        program_bytes,
        tokenizer_bytes,
        estimate_bytes: params_bytes + program_bytes + tokenizer_bytes,
        inventory_leaves_estimate: params_bytes.div_ceil(32 << 10) + program.params.len() as u64,
        download_bytes_needed: needed,
        download_bytes_total: total,
        left_out_bytes,
        note: if src.gguf_file.is_some() {
            "a GGUF is one file: it is fetched whole".into()
        } else if needed.is_none() {
            "the shards' headers are not all here: the size of the tensors the class reads is known only from the index's total".into()
        } else {
            "the size of the tensors the class reads; tensors the scope leaves out and tensors nothing reads need not be fetched"
                .into()
        },
    }
}

/// What `Report::source` carries.
pub fn source_info(src: &Source) -> super::SourceInfo {
    super::SourceInfo {
        files: src.files.clone(),
        config_sha256: src.config_sha256.clone(),
        shards: src
            .shards
            .iter()
            .map(|s| super::ShardInfo {
                file: s.file.clone(),
                header_bytes: s.header_bytes,
                declared_data_bytes: s.declared_data_bytes,
                file_bytes: s.file_bytes,
                complete: s.complete(),
                header_sha256: s.header_sha256.clone(),
                tensors: s.entries.len(),
            })
            .collect(),
        missing_shards: src.missing_shards.clone(),
        weight_bytes: src.declared_weight_bytes(),
        index_total_size: src.index_total_size,
    }
}
