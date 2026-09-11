//! **The report** (ADR-0108 Decisions 2 and 3): every answer names its tier, says which depth it
//! reached, lists every check by name, and carries what it recomputed. There is no bare pass.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::manifest::{PalwExtensionError, PalwExtensionKindV1};

/// The three depths (Decision 3), ordered: a deeper depth includes the shallower ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PalwExtensionDepthV1 {
    /// Any machine: canonical form, bounds, the id, the kind's identity recomputed from what the
    /// manifest carries inline.
    Structural,
    /// The declared vectors re-run with this build.
    Vectors,
    /// The same computation the chain's transition applies, over the bytes the person holds.
    Full,
}

impl PalwExtensionDepthV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Vectors => "vectors",
            Self::Full => "full",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "structural" => Some(Self::Structural),
            "vectors" => Some(Self::Vectors),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

impl std::fmt::Display for PalwExtensionDepthV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Decision 8: whether SERVING a class on this build needs the chain-class arm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionServingV1 {
    /// The class is a row of this build's SDK ledger.
    pub in_build_table: bool,
    /// `!in_build_table`: the class would be served from its registration only once an operator
    /// arms `with_chain_classes_v1()` (ADR-0067 Decision 5).
    pub needs_chain_classes_arm: bool,
}

/// One fence a ruleset candidate moves, or a fence a kind's refusal points at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwFenceDeltaV1 {
    pub name: String,
    /// What the manifest asked for (`genesis`, a height, or `?` where the height is the
    /// person's to fill).
    pub requested: String,
    /// This build's value: `genesis`, `never`, a height, or `absent` for a name this build lacks.
    pub this_build: String,
}

/// Decision 6: what the arming build would print, beside what this build prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwWouldPrintV1 {
    pub params_id: String,
    pub identity_id: String,
    pub schedule_id: String,
    pub fence_schedule: Vec<u64>,
}

/// Decision 2: exactly one of four. Serialized with a `tier` tag so a script can branch on it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tier", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PalwExtensionClassificationV1 {
    /// The chain has the object; this build recomputed it.
    Expressible {
        /// Which object it would be (`ClassRegistered`, `FamilyCertified`, `ClassLaneCertified`,
        /// or `none` for a transformer, whose derivations ride per claim).
        admission_object: String,
        /// A field-naming reason the chain would still refuse it (already registered, a lane of a
        /// class the chain does not hold yet …), or `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        would_be_refused: Option<String>,
        /// Decision 8, for a class.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        serving: Option<PalwExtensionServingV1>,
        /// ADR-0069 Decision 5: no end-to-end certified family covers the class's kernels, so it
        /// registers at share 0 and earns nothing until one does.
        #[serde(default)]
        weightless: bool,
        /// The exact (class, root) is already in the network's genesis or its terms.
        #[serde(default)]
        already_registered: bool,
    },
    /// The chain never sees it; this build lacks the code to say more than "unverifiable here".
    NodeExtension {
        /// What this build lacks, by name — a transformer, a lineage, a kernel, a file.
        missing: Vec<String>,
    },
    /// A fence — a release, never a manifest.
    RulesetChange {
        fences: Vec<PalwFenceDeltaV1>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        would_print: Option<PalwWouldPrintV1>,
        flag_day: bool,
        reason: String,
    },
    /// The manifest is wrong about itself.
    Refused { field: String, reason: String },
}

impl PalwExtensionClassificationV1 {
    pub fn tier(&self) -> &'static str {
        match self {
            Self::Expressible { .. } => "expressible",
            Self::NodeExtension { .. } => "node-extension",
            Self::RulesetChange { .. } => "ruleset-change",
            Self::Refused { .. } => "refused",
        }
    }

    pub fn refused(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Refused { field: field.into(), reason: reason.into() }
    }

    pub fn node_extension(missing: impl Into<String>) -> Self {
        Self::NodeExtension { missing: vec![missing.into()] }
    }

    /// One line a script or a person reads first.
    pub fn summary(&self) -> String {
        match self {
            // A kind nothing rides for (a transformer, whose derivations ride per claim) says so
            // rather than reading "expressible now as none".
            Self::Expressible { admission_object, would_be_refused: None, .. } if admission_object == "none" => {
                "expressible now — no chain object rides for this kind".to_string()
            }
            Self::Expressible { admission_object, would_be_refused: None, weightless, .. } => {
                format!("expressible now as {admission_object}{}", if *weightless { " (weightless)" } else { "" })
            }
            Self::Expressible { admission_object, would_be_refused: Some(why), .. } if admission_object == "none" => {
                format!("expressible, but would be refused: {why}")
            }
            Self::Expressible { admission_object, would_be_refused: Some(why), .. } => {
                format!("expressible as {admission_object}, but would be refused: {why}")
            }
            Self::NodeExtension { missing } => format!("node extension — unverifiable here; this build lacks: {}", missing.join("; ")),
            Self::RulesetChange { reason, flag_day, .. } => {
                format!("ruleset change{} — {reason}", if *flag_day { " (flag day)" } else { "" })
            }
            Self::Refused { field, reason } => format!("refused — {field}: {reason}"),
        }
    }
}

/// One check, by name: `pass`, `fail` with a reason, or `skipped` with a reason (Decision 4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "reason", rename_all = "lowercase", deny_unknown_fields)]
pub enum PalwExtensionOutcomeV1 {
    Pass,
    Fail(String),
    Skipped(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionCheckV1 {
    pub name: String,
    #[serde(flatten)]
    pub outcome: PalwExtensionOutcomeV1,
}

/// The report (Decision 2). Canonical-JSON-able: every field is a string, an integer, a list or a
/// map, so `canonical_json` never meets a float.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PalwExtensionReportV1 {
    /// 128 hex — `extension_id_v1` of the manifest's canonical bytes.
    pub extension_id: String,
    pub kind: PalwExtensionKindV1,
    /// The network the report answers for (the manifest's, which the verifier's env must match).
    pub network: String,
    pub classification: PalwExtensionClassificationV1,
    pub depth_requested: PalwExtensionDepthV1,
    pub depth_reached: PalwExtensionDepthV1,
    /// Why a requested depth was not reached — the bytes this machine lacks, by field. Not a
    /// failure (Decision 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    pub checks: Vec<PalwExtensionCheckV1>,
    /// Everything recomputed, by name, as hex or a decimal string.
    pub recomputed: BTreeMap<String, String>,
    /// 64 hex — this build's `consensus_params_id` for the network.
    pub ruleset_id_this_build: String,
    /// The manifest's `requires.ruleset_id`, echoed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset_id_manifest: Option<String>,
    /// ADR-0108 §8: which registration terms the verdict was judged against.
    pub chain_terms: String,
    /// The DAA score the shape was resolved at (`0` = the genesis shape).
    pub daa_score: u64,
}

impl PalwExtensionReportV1 {
    /// The report's RFC 8785 bytes — what a receipt signs over.
    pub fn canonical_json(&self) -> Result<Vec<u8>, PalwExtensionError> {
        let raw = serde_json::to_vec(self).map_err(|e| PalwExtensionError::Internal(format!("the report does not serialize: {e}")))?;
        crate::manifest::canonical_json(&raw)
    }

    /// Whether the person asked for more than this machine could do.
    pub fn depth_not_reached(&self) -> bool {
        self.depth_reached < self.depth_requested
    }

    pub fn passed(&self) -> usize {
        self.checks.iter().filter(|c| matches!(c.outcome, PalwExtensionOutcomeV1::Pass)).count()
    }

    pub fn failed(&self) -> usize {
        self.checks.iter().filter(|c| matches!(c.outcome, PalwExtensionOutcomeV1::Fail(_))).count()
    }

    pub fn skipped(&self) -> usize {
        self.checks.iter().filter(|c| matches!(c.outcome, PalwExtensionOutcomeV1::Skipped(_))).count()
    }
}
