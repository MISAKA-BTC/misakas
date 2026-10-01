//! **The architecture report** — what `palw-class check-architecture <hf dir | config.json>` prints.
//!
//! It names the model's `model_type` (for information only: nothing is selected by it), the features
//! the model needs, each `SUPPORTED` or `MISSING` with the capability that would close the gap, the
//! support [`Level`], and whether the protocol would need a new primitive or a new court kernel. The
//! same data serialises as JSON (`--json`) for the tools that wrap it (header-only preflight, the t12
//! admission conditions).

use super::features::{Area, FeatureInfo, FeatureUse, Lowering, Requirement, feature_info};
use crate::hf_schema::{AdapterSource, Level, MissingItem, ReadOptions, TensorIndex, read_model};
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
    pub level: Level,
    pub features: Vec<FeatureReport>,
    pub assumed_defaults: Vec<String>,
    pub unmapped_config_keys: Vec<String>,
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

/// Read a configuration (and, if given, its tensor names) and report what it needs.
pub fn analyze(config: &Value, tensors: Option<&TensorIndex>, opts: &ReadOptions) -> ArchitectureReport {
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
                level,
                features,
                assumed_defaults: read.assumed_defaults,
                unmapped_config_keys: Vec::new(),
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
                level: Level::C,
                features: Vec::new(),
                assumed_defaults: Vec::new(),
                unmapped_config_keys: f.unmapped_config_keys,
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
        let _ = writeln!(o, "level           {}", self.level);
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
                let _ = writeln!(o, "result          LOWERABLE (Level {})", self.level);
            }
            ReportResult::NotLowerable { reason } => {
                let _ = writeln!(o, "result          NOT_LOWERABLE (Level {}): {reason}", self.level);
            }
        }
        o
    }
}
