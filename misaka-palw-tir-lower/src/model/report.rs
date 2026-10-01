//! **The architecture report** — what `palw-class check-architecture <hf dir | config.json>` prints.
//!
//! It names the model's `model_type` (for information only: nothing is selected by it), the features
//! the model needs, each `SUPPORTED` or `MISSING` with the capability that would close the gap, the
//! support [`Level`], and whether the protocol would need a new primitive or a new court kernel. The
//! same data serialises as JSON (`--json`) for the tools that wrap it (header-only preflight, the t12
//! admission conditions).

use super::features::{Area, FeatureInfo, FeatureUse, Lowering, Requirement, encdec_features, feature_info, vision_features};
use crate::hf_schema::{AdapterSource, Level, MissingItem, ReadOptions, TensorIndex, is_encoder_decoder, is_vision_tower, read_encdec, read_model, read_vision};
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write;

/// The schema id of a serialised [`ArchitectureReport`].
pub const REPORT_SCHEMA_V1: &str = "misaka.palw.architecture-report.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FeatureStatus {
    Supported,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FeatureReport {
    pub id: String,
    pub title: String,
    pub area: Area,
    pub status: FeatureStatus,
    pub lowering: Lowering,
    /// The missing capability's id, when the status is `Missing`.
    pub capability: Option<String>,
    pub detail: String,
    /// The layers using it (empty: the model as a whole).
    pub layers: Vec<usize>,
    pub primitives: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum ReportResult {
    /// Every feature is supported and the config lowers; admission (`tir_admit_v1`) is judged
    /// separately, on the lowered program.
    Lowerable,
    NotLowerable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArchitectureReport {
    pub schema: &'static str,
    /// Informational only: no code or adapter is selected by it.
    pub model_type: Option<String>,
    pub architectures: Vec<String>,
    pub adapter: AdapterSource,
    /// The built-in refusal a user-supplied adapter overrode (FR-25), as `<architecture>: <why>`.
    pub overrides_refusal: Option<String>,
    pub level: Level,
    /// The level as the report prints it: Level A is `A (unconfirmed)` until a reference check passed (FR-26) —
    /// the standard template reads a class no adapter claims, and rope pairing, norm placement and MLP gating are
    /// class code, not configuration.
    pub level_label: String,
    /// A float-vs-transformers check on the same weights passed ([`ArchitectureReport::confirm_reference`]).
    pub reference_confirmed: bool,
    pub features: Vec<FeatureReport>,
    pub assumed_defaults: Vec<String>,
    pub unmapped_config_keys: Vec<String>,
    /// Checkpoint tensors no parameter of the reading reads, when a tensor index was given (FR-26): a feature the
    /// template or the adapter does not know. They are refused, never ignored.
    pub unread_tensors: Vec<String>,
    /// What the binder found wrong with the tensor index: a missing tensor, a shape the graph does not accept, a
    /// malformed weight expression.
    pub weight_errors: Vec<String>,
    pub missing: Vec<MissingItem>,
    pub new_consensus_primitive_required: bool,
    pub new_court_kernel_required: bool,
    pub result: ReportResult,
    pub notes: Vec<String>,
}

fn model_type_of(config: &Value) -> Option<String> {
    let own = config.get("model_type").and_then(Value::as_str).map(str::to_string);
    let text = config.get("text_config").and_then(|t| t.get("model_type")).and_then(Value::as_str).map(str::to_string);
    match (own, text) {
        (Some(a), Some(b)) if a != b => Some(format!("{a} (text decoder: {b})")),
        (Some(a), _) => Some(a),
        (None, b) => b,
    }
}

fn architectures_of(config: &Value) -> Vec<String> {
    config
        .get("architectures")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn feature_report(u: &FeatureUse) -> FeatureReport {
    let info: Option<&'static FeatureInfo> = feature_info(u.id.0);
    let (status, capability) = match info {
        Some(i) => match (i.lowering, i.protocol) {
            (_, Requirement::Capability { id, .. }) => (FeatureStatus::Missing, Some(id.to_string())),
            (Lowering::Implemented, Requirement::None) => (FeatureStatus::Supported, None),
            (_, Requirement::None) => (FeatureStatus::Missing, Some(format!("LOWERING_{}", i.id.0))),
        },
        None => (FeatureStatus::Missing, Some(format!("UNKNOWN_FEATURE_{}", u.id.0))),
    };
    FeatureReport {
        id: u.id.0.to_string(),
        title: info.map(|i| i.title.to_string()).unwrap_or_default(),
        area: info.map(|i| i.area).unwrap_or(Area::Mixer),
        status,
        lowering: info.map(|i| i.lowering).unwrap_or(Lowering::Missing),
        capability,
        detail: u.detail.clone(),
        layers: u.layers.clone(),
        primitives: info.map(|i| i.primitives.iter().map(|p| p.to_string()).collect()).unwrap_or_default(),
    }
}

/// `A (unconfirmed)` for a Level A nobody has checked against a reference; the letter otherwise.
fn label_of(level: Level, confirmed: bool) -> String {
    match level {
        Level::A if !confirmed => "A (unconfirmed)".to_string(),
        other => other.to_string(),
    }
}

/// Run the weight binding on the checkpoint's tensor names — and shapes, when the headers carry them — and
/// return `(tensors no param reads, binding errors)` (FR-26: this is the check that caught Arcee's gated MLP
/// and BitNet's sub-norms). An HL graph that cannot be built is the lowering's finding, not this one's.
fn weights_check(spec: &crate::spec::ModelSpec, t: &TensorIndex) -> (Vec<String>, Vec<String>) {
    let Ok(prog) = crate::hl::build_program(spec) else { return (Vec::new(), Vec::new()) };
    let binding = match crate::hf_weights::bind(spec, &prog) {
        Ok(b) => b,
        Err(e) => return (Vec::new(), vec![e.to_string()]),
    };
    let rep = if t.has_all_shapes() {
        crate::weights::check_weights(&prog, &binding, &crate::hf_schema::HeaderSource(t))
    } else {
        crate::weights::check_names(&prog, &binding, &t.names().map(str::to_string).collect())
    };
    // A tied head reads the embedding table; some checkpoints save the same matrix a second time as `lm_head.weight`
    // (HF loads both into the one tied parameter): that copy is not a feature the reading is missing.
    let tied_copy = |n: &String| {
        matches!(spec.output, crate::spec::OutputSpec::Logits) && spec.head.tied && (n == "lm_head.weight" || n.ends_with(".lm_head.weight"))
    };
    (rep.unused.into_iter().filter(|n| !tied_copy(n)).collect(), rep.errors)
}

fn short_list(v: &[String], n: usize) -> String {
    let mut s = v.iter().take(n).cloned().collect::<Vec<_>>().join(", ");
    if v.len() > n {
        s.push_str(&format!(", … ({} in all)", v.len()));
    }
    s
}

/// The encoder-decoder's weight binding run on the tensor index, as [`weights_check`] does for a decoder: both stages
/// bind from ONE checkpoint, so a tensor is unread only when NEITHER stage reads it. Returns `(unread, errors)`.
fn encdec_weights_check(spec: &crate::lower::encdec::EncDecSpec, t: &TensorIndex) -> (Vec<String>, Vec<String>) {
    use crate::lower::encdec::hl_programs;
    use crate::weights::{WeightReport, check_names, check_weights};
    let has = |n: &str| t.has(n);
    // The source length only sizes the carry of the encoder's program; the binding does not depend on it.
    let ((ehl, ebind), (dhl, dbind)) = match hl_programs(spec, 16, &has) {
        Ok(p) => p,
        Err(e) => return (Vec::new(), vec![e.to_string()]),
    };
    let check = |hl: &crate::hl::HlProgram, b: &crate::weights::Binding| -> WeightReport {
        if t.has_all_shapes() {
            check_weights(hl, b, &crate::hf_schema::HeaderSource(t))
        } else {
            check_names(hl, b, &t.names().map(str::to_string).collect())
        }
    };
    let (e, d) = (check(&ehl, &ebind), check(&dhl, &dbind));
    let unread: Vec<String> = e.unused.iter().filter(|n| d.unused.contains(n)).cloned().collect();
    let mut errors = e.errors;
    for x in d.errors {
        if !errors.contains(&x) {
            errors.push(x);
        }
    }
    (unread, errors)
}

/// [`analyze`] for an encoder-decoder (`ENCDEC_FROM_SPEC_V1`): the adapter of kind `encdec` builds the spec, the
/// features are listed, and the tensor index is run through both stages' bindings.
pub fn analyze_encdec(config: &Value, tensors: Option<&TensorIndex>, opts: &ReadOptions) -> ArchitectureReport {
    let model_type = model_type_of(config);
    let architectures = architectures_of(config);
    match read_encdec(config, opts) {
        Ok(read) => {
            let features: Vec<FeatureReport> = encdec_features(&read.spec).iter().map(feature_report).collect();
            let mut missing = Vec::new();
            let (unread_tensors, weight_errors) = match tensors {
                Some(t) => encdec_weights_check(&read.spec, t),
                None => (Vec::new(), Vec::new()),
            };
            if !unread_tensors.is_empty() {
                missing.push(MissingItem {
                    what: "checkpoint tensors the reading never uses".to_string(),
                    why: format!(
                        "{} tensor(s): {} — a feature this reading does not know; an adapter must account for every tensor (or list its prefix as ignored)",
                        unread_tensors.len(),
                        short_list(&unread_tensors, 6)
                    ),
                    general_primitive: None,
                });
            }
            if !weight_errors.is_empty() {
                missing.push(MissingItem {
                    what: "the checkpoint does not fit the reading".to_string(),
                    why: short_list(&weight_errors, 3),
                    general_primitive: None,
                });
            }
            let level = if missing.is_empty() { Level::B } else { Level::C };
            let result = if missing.is_empty() {
                ReportResult::Lowerable
            } else {
                ReportResult::NotLowerable {
                    reason: format!("missing: {}", missing.iter().map(|m| m.what.clone()).collect::<Vec<_>>().join(", ")),
                }
            };
            ArchitectureReport {
                schema: REPORT_SCHEMA_V1,
                model_type,
                architectures,
                adapter: read.adapter,
                overrides_refusal: None,
                level,
                level_label: label_of(level, false),
                reference_confirmed: false,
                features,
                assumed_defaults: read.assumed_defaults,
                unmapped_config_keys: Vec::new(),
                unread_tensors,
                weight_errors,
                missing,
                new_consensus_primitive_required: false,
                new_court_kernel_required: false,
                result,
                notes: vec!["an encoder–decoder lowers to two programs (the encoder over the padded source, then the decoder's text stage); admission is judged per stage".to_string()],
            }
        }
        Err(f) => ArchitectureReport {
            schema: REPORT_SCHEMA_V1,
            model_type,
            architectures,
            adapter: f.adapter,
            overrides_refusal: None,
            level: Level::C,
            level_label: label_of(Level::C, false),
            reference_confirmed: false,
            features: Vec::new(),
            assumed_defaults: Vec::new(),
            unmapped_config_keys: f.unmapped_config_keys,
            unread_tensors: Vec::new(),
            weight_errors: Vec::new(),
            missing: f.missing,
            new_consensus_primitive_required: false,
            new_court_kernel_required: false,
            result: ReportResult::NotLowerable { reason: f.error.to_string() },
            notes: Vec::new(),
        },
    }
}

/// [`analyze`] for a vision tower (`VISION_FROM_SPEC_V1`): the adapter of kind `vision` builds the spec, the features
/// are listed, and the tensor index is run through the tower's binding.
pub fn analyze_vision(config: &Value, tensors: Option<&TensorIndex>, opts: &ReadOptions) -> ArchitectureReport {
    let model_type = model_type_of(config);
    let architectures = architectures_of(config);
    match read_vision(config, opts) {
        Ok(read) => {
            let features: Vec<FeatureReport> = vision_features(&read.spec).iter().map(feature_report).collect();
            let mut missing = Vec::new();
            let (unread_tensors, weight_errors) = match tensors {
                Some(t) => match crate::lower::vision::hl_program(&read.spec) {
                    Ok((hl, b)) => {
                        let r = if t.has_all_shapes() {
                            crate::weights::check_weights(&hl, &b, &crate::hf_schema::HeaderSource(t))
                        } else {
                            crate::weights::check_names(&hl, &b, &t.names().map(str::to_string).collect())
                        };
                        (r.unused, r.errors)
                    }
                    Err(e) => (Vec::new(), vec![e.to_string()]),
                },
                None => (Vec::new(), Vec::new()),
            };
            if !unread_tensors.is_empty() {
                missing.push(MissingItem {
                    what: "checkpoint tensors the reading never uses".to_string(),
                    why: format!(
                        "{} tensor(s): {} — a feature this reading does not know; an adapter must account for every tensor (or list its prefix as ignored)",
                        unread_tensors.len(),
                        short_list(&unread_tensors, 6)
                    ),
                    general_primitive: None,
                });
            }
            if !weight_errors.is_empty() {
                missing.push(MissingItem { what: "the checkpoint does not fit the reading".to_string(), why: short_list(&weight_errors, 3), general_primitive: None });
            }
            let level = if missing.is_empty() { Level::B } else { Level::C };
            let result = if missing.is_empty() {
                ReportResult::Lowerable
            } else {
                ReportResult::NotLowerable { reason: format!("missing: {}", missing.iter().map(|m| m.what.clone()).collect::<Vec<_>>().join(", ")) }
            };
            ArchitectureReport {
                schema: REPORT_SCHEMA_V1,
                model_type,
                architectures,
                adapter: read.adapter,
                overrides_refusal: None,
                level,
                level_label: label_of(level, false),
                reference_confirmed: false,
                features,
                assumed_defaults: read.assumed_defaults,
                unmapped_config_keys: Vec::new(),
                unread_tensors,
                weight_errors,
                missing,
                new_consensus_primitive_required: false,
                new_court_kernel_required: false,
                result,
                notes: vec!["a vision tower takes a canonical u8 image at the class's declared size; the processor's normalisation is the adapter's default unless the class gives its own".to_string()],
            }
        }
        Err(f) => ArchitectureReport {
            schema: REPORT_SCHEMA_V1,
            model_type,
            architectures,
            adapter: f.adapter,
            overrides_refusal: None,
            level: Level::C,
            level_label: label_of(Level::C, false),
            reference_confirmed: false,
            features: Vec::new(),
            assumed_defaults: Vec::new(),
            unmapped_config_keys: f.unmapped_config_keys,
            unread_tensors: Vec::new(),
            weight_errors: Vec::new(),
            missing: f.missing,
            new_consensus_primitive_required: false,
            new_court_kernel_required: false,
            result: ReportResult::NotLowerable { reason: f.error.to_string() },
            notes: Vec::new(),
        },
    }
}

/// Read a configuration (and, if given, its tensor names) and report what it needs.
pub fn analyze(config: &Value, tensors: Option<&TensorIndex>, opts: &ReadOptions) -> ArchitectureReport {
    if is_encoder_decoder(config) {
        return analyze_encdec(config, tensors, opts);
    }
    if is_vision_tower(config) {
        return analyze_vision(config, tensors, opts);
    }
    let model_type = model_type_of(config);
    let architectures = architectures_of(config);
    match read_model(config, tensors, opts) {
        Ok(read) => {
            let uses = read.spec.features();
            let features: Vec<FeatureReport> = uses.iter().map(feature_report).collect();
            let mut missing = Vec::new();
            let (mut prim, mut kernel) = (false, false);
            for f in &features {
                if f.status != FeatureStatus::Missing {
                    continue;
                }
                let info = feature_info(&f.id);
                let (why, gp) = match info.map(|i| i.protocol) {
                    Some(Requirement::Capability { id, general_primitive, new_primitive, new_court_kernel }) => {
                        prim |= new_primitive;
                        kernel |= new_court_kernel;
                        (format!("capability {id}"), Some(general_primitive.to_string()))
                    }
                    _ => ("the generic lowerer does not lower this feature yet".to_string(), None),
                };
                missing.push(MissingItem { what: f.id.clone(), why, general_primitive: gp });
            }
            // The checkpoint, when its tensor index was given: every tensor must be read by some param, every
            // param must find its tensor with the shape the graph needs (FR-26).
            let (unread_tensors, weight_errors) = match tensors {
                Some(t) if missing.is_empty() => weights_check(&read.spec, t),
                _ => (Vec::new(), Vec::new()),
            };
            if !unread_tensors.is_empty() {
                missing.push(MissingItem {
                    what: "checkpoint tensors the reading never uses".to_string(),
                    why: format!(
                        "{} tensor(s): {} — a feature this reading does not know; an adapter must account for every tensor (or list its prefix as ignored)",
                        unread_tensors.len(),
                        short_list(&unread_tensors, 6)
                    ),
                    general_primitive: None,
                });
            }
            if !weight_errors.is_empty() {
                missing.push(MissingItem {
                    what: "the checkpoint does not fit the reading".to_string(),
                    why: short_list(&weight_errors, 3),
                    general_primitive: None,
                });
            }
            let level = if !missing.is_empty() {
                Level::C
            } else if matches!(read.adapter, AdapterSource::None) {
                Level::A
            } else {
                Level::B
            };
            let result = if missing.is_empty() {
                ReportResult::Lowerable
            } else {
                ReportResult::NotLowerable {
                    reason: format!("missing: {}", missing.iter().map(|m| m.what.clone()).collect::<Vec<_>>().join(", ")),
                }
            };
            ArchitectureReport {
                schema: REPORT_SCHEMA_V1,
                model_type,
                architectures,
                adapter: read.adapter,
                overrides_refusal: read.overrides_refusal,
                level,
                level_label: label_of(level, false),
                reference_confirmed: false,
                features,
                assumed_defaults: read.assumed_defaults,
                unmapped_config_keys: Vec::new(),
                unread_tensors,
                weight_errors,
                missing,
                new_consensus_primitive_required: prim,
                new_court_kernel_required: kernel,
                result,
                notes: read.spec.notes.clone(),
            }
        }
        Err(f) => {
            let reason = f.error.to_string();
            ArchitectureReport {
                schema: REPORT_SCHEMA_V1,
                model_type,
                architectures,
                adapter: f.adapter,
                overrides_refusal: None,
                level: Level::C,
                level_label: label_of(Level::C, false),
                reference_confirmed: false,
                features: Vec::new(),
                assumed_defaults: Vec::new(),
                unmapped_config_keys: f.unmapped_config_keys,
                unread_tensors: Vec::new(),
                weight_errors: Vec::new(),
                missing: f.missing,
                new_consensus_primitive_required: false,
                new_court_kernel_required: false,
                result: ReportResult::NotLowerable { reason },
                notes: Vec::new(),
            }
        }
    }
}

/// `0-3,5,7-9` for a sorted list of layer indices.
fn ranges(layers: &[usize]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < layers.len() {
        let mut j = i;
        while j + 1 < layers.len() && layers[j + 1] == layers[j] + 1 {
            j += 1;
        }
        out.push(if i == j { format!("{}", layers[i]) } else { format!("{}-{}", layers[i], layers[j]) });
        i = j + 1;
    }
    out.join(",")
}

impl ArchitectureReport {
    /// The human report.
    pub fn render(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "model_type      {}   (informational only: it selects no code)", self.model_type.as_deref().unwrap_or("(none)"));
        if !self.architectures.is_empty() {
            let _ = writeln!(o, "architectures   {}", self.architectures.join(", "));
        }
        let _ = writeln!(o, "adapter         {}", self.adapter.describe());
        if let Some(r) = &self.overrides_refusal {
            let _ = writeln!(o, "user adapter overrides built-in refusal: {r}");
            if !self.reference_confirmed {
                let _ = writeln!(
                    o,
                    "                the override passed the same validation as any adapter; whether the reading is RIGHT is not checked here — confirm with palw-tir-fidelity against the transformers class"
                );
            }
        }
        let _ = writeln!(o, "level           {}", self.level_label);
        if self.level == Level::A && !self.reference_confirmed {
            let _ = writeln!(
                o,
                "                the standard template is a reading of a class no adapter claims: rope pairing, norm placement and MLP gating are class code, not configuration — confirm with palw-tir-fidelity against the transformers class"
            );
        }
        if !self.features.is_empty() {
            let _ = writeln!(o, "features ({}):", self.features.len());
            let w = self.features.iter().map(|f| f.id.len()).max().unwrap_or(0);
            for f in &self.features {
                let status = match (&f.status, &f.capability) {
                    (FeatureStatus::Supported, _) => "SUPPORTED".to_string(),
                    (FeatureStatus::Missing, Some(c)) => format!("MISSING ({c})"),
                    (FeatureStatus::Missing, None) => "MISSING".to_string(),
                };
                let mut tail = f.detail.clone();
                if !f.layers.is_empty() {
                    if !tail.is_empty() {
                        tail.push_str("  ");
                    }
                    let _ = write!(tail, "layers {}", ranges(&f.layers));
                }
                let _ = writeln!(o, "  {:<w$}  {:<24} {}", f.id, status, tail, w = w);
            }
        }
        if !self.assumed_defaults.is_empty() {
            let _ = writeln!(o, "assumed class defaults (confirm against the class): {}", self.assumed_defaults.join(", "));
        }
        if !self.unmapped_config_keys.is_empty() {
            let _ = writeln!(o, "unmapped config keys (a key that might change the math is refused, not ignored): {}", self.unmapped_config_keys.join(", "));
        }
        for m in &self.missing {
            let _ = write!(o, "missing: {} — {}", m.what, m.why);
            if let Some(g) = &m.general_primitive {
                let _ = write!(o, "; the smallest general addition that closes it: {g}");
            }
            let _ = writeln!(o);
        }
        for t in self.unread_tensors.iter().take(20) {
            let _ = writeln!(o, "unread tensor: {t}");
        }
        for e in self.weight_errors.iter().take(20) {
            let _ = writeln!(o, "weights: {e}");
        }
        for n in &self.notes {
            let _ = writeln!(o, "note: {n}");
        }
        if matches!(self.result, ReportResult::Lowerable) || !self.missing.is_empty() || !self.features.is_empty() {
            let _ = writeln!(
                o,
                "{}",
                if self.new_consensus_primitive_required { "A new consensus primitive is required" } else { "No new consensus primitive required" }
            );
            let _ = writeln!(
                o,
                "{}",
                if self.new_court_kernel_required { "A new court kernel is required" } else { "No new court kernel required" }
            );
        }
        match &self.result {
            ReportResult::Lowerable => {
                let _ = writeln!(o, "result          LOWERABLE (Level {})", self.level_label);
            }
            ReportResult::NotLowerable { reason } => {
                let _ = writeln!(o, "result          NOT_LOWERABLE (Level {}): {reason}", self.level_label);
            }
        }
        o
    }

    /// Record that a reference check — the float reference against the transformers class on the same weights
    /// (`float_vs_hf`) — passed: a Level A reading is then plain `A` (FR-26).
    pub fn confirm_reference(mut self) -> Self {
        self.reference_confirmed = true;
        self.level_label = label_of(self.level, true);
        self
    }
}
