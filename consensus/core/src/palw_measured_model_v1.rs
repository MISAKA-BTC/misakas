//! **ADR-0099 Decision 1 — the Measured Model Artifact: the adder measures, the chain recomputes
//! what it can, and drills the rest.**
//!
//! Anyone adding a model hands the network a MANIFEST — the family and the public numbers of the
//! geometry — and the network's own generator turns it into a signed document: the artifact's
//! size, the state a seat holds at each context, every wall of ADR-0097 at each context, and the
//! shard plans a seat budget allows (ADR-0099 Decision 2). Every one of those is a pure function
//! of the manifest and the ruleset, so any node recomputes it and compares
//! ([`palw_verify_measured_model_v1`]): a document that disagrees with the recomputation is refused
//! by the field that disagrees. What is NOT recomputable — the adder's measured replay rate on the
//! adder's host — is carried as a self-report, labelled so, and admits nothing: the chain's own
//! certification drill (ADR-0075 Decision 7) measures a seat's rate on the network's hosts, and
//! `window_receipt × rate` is what admission bounds (ADR-0082 Decision 9).
//!
//! The shipped classes are manifests too ([`PalwModelManifestV1::from_dense`] /
//! [`PalwModelManifestV1::from_hybrid`]), and their manifests reproduce the registered rows' own
//! class ids — the test that pins it is what makes "a manifest is enough to name the class" a
//! measured statement.
//!
//! Serialised two ways on purpose: JSON for the person and the CLI (serde), borsh for the id and
//! the signature ([`palw_measured_model_id_v1`], over everything but the signature).

use crate::Hash64;
use crate::palw_class_admission_v2::PalwKaryCourtV1;
use crate::palw_mode_v2::{PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS, PalwConsensusParamsV2};
use crate::palw_model_fit_v1::palw_model_fit_v1;
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v5};
use crate::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_artifact_row_profile_v5};
use crate::palw_shard_plan_v1::{
    PalwArtifactBytesV1, palw_qwen25_artifact_bytes_v1, palw_qwen36_artifact_bytes_v1, palw_shard_plan_for_seat_v1,
};
use crate::palw_step::{PalwShapeProfileV3, PalwStepError};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

pub const PALW_MEASURED_MODEL_SCHEMA_V1: &str = "misaka.palw.measured-model.v1";
pub const PALW_MEASURED_MODEL_DOMAIN_ID_V1: &[u8] = b"misaka-palw/measured-model/id/v1";
/// The ML-DSA-87 context the adder signs the id under. Not yet in the bundle's registry; it
/// enters it with `Params::palw_shard_court`'s ruleset move, and until then the signature is a
/// statement to a person, not to the chain.
pub const PALW_MEASURED_MODEL_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/measured-model/sign/v1";

/// The families this tree can build a graph for. A model of another architecture needs a
/// converter and kernels first — ADR-0075's route — and is refused at the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PalwModelFamilyV1 {
    /// Qwen2.5-shaped dense attention with a gated MLP, on the A16 integer engine.
    DenseA16,
    /// Qwen3.6-shaped hybrid: gated delta-rule layers with periodic full attention, a routed
    /// mixture with a shared expert. The K3 stand-in is written in this family.
    HybridQwen36,
}

/// **The model definition an adder hands in.** The public numbers of a geometry; the fields a
/// family does not use stay zero. `total_parameters` is a card's stated count, to which the
/// formula's per-layer split is scaled when the two disagree (ADR-0099 U-01 is the reconciliation
/// the adder owes when they do).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwModelManifestV1 {
    pub name: String,
    pub family: PalwModelFamilyV1,
    pub layer_count: u16,
    pub hidden_dim: u32,
    pub vocab_size: u32,
    pub attn_heads: u16,
    pub attn_kv_heads: u16,
    pub attn_head_dim: u32,
    #[serde(default)]
    pub ffn_dim: u32,
    #[serde(default)]
    pub full_attention_interval: u16,
    #[serde(default)]
    pub rope_dims: u16,
    #[serde(default)]
    pub gdn_k_heads: u16,
    #[serde(default)]
    pub gdn_v_heads: u16,
    #[serde(default)]
    pub gdn_head_dim: u32,
    #[serde(default)]
    pub gdn_conv_kernel: u16,
    #[serde(default)]
    pub n_experts: u32,
    #[serde(default)]
    pub experts_per_token: u32,
    #[serde(default)]
    pub moe_dim: u32,
    #[serde(default)]
    pub shared_dim: u32,
    #[serde(default)]
    pub attn_output_gate: u8,
    #[serde(default)]
    pub total_parameters: Option<u64>,
}

impl PalwModelManifestV1 {
    /// A dense manifest from the family's measured geometry.
    pub fn from_dense(name: impl Into<String>, g: &PalwQwen25GeometryV1) -> Self {
        PalwModelManifestV1 {
            name: name.into(),
            family: PalwModelFamilyV1::DenseA16,
            layer_count: g.layer_count,
            hidden_dim: g.hidden_dim,
            vocab_size: g.vocab_size,
            attn_heads: g.attn_heads,
            attn_kv_heads: g.attn_kv_heads,
            attn_head_dim: g.attn_head_dim,
            ffn_dim: g.ffn_dim,
            full_attention_interval: 0,
            rope_dims: 0,
            gdn_k_heads: 0,
            gdn_v_heads: 0,
            gdn_head_dim: 0,
            gdn_conv_kernel: 0,
            n_experts: 0,
            experts_per_token: 0,
            moe_dim: 0,
            shared_dim: 0,
            attn_output_gate: 0,
            total_parameters: None,
        }
    }

    /// A hybrid manifest from the family's geometry, with a card's total when one is stated.
    pub fn from_hybrid(name: impl Into<String>, g: &PalwQwen36GeometryV1, total_parameters: Option<u64>) -> Self {
        PalwModelManifestV1 {
            name: name.into(),
            family: PalwModelFamilyV1::HybridQwen36,
            layer_count: g.layer_count,
            hidden_dim: g.hidden_dim,
            vocab_size: g.vocab_size,
            attn_heads: g.attn_heads,
            attn_kv_heads: g.attn_kv_heads,
            attn_head_dim: g.attn_head_dim,
            ffn_dim: 0,
            full_attention_interval: g.full_attention_interval,
            rope_dims: g.rope_dims,
            gdn_k_heads: g.gdn_k_heads,
            gdn_v_heads: g.gdn_v_heads,
            gdn_head_dim: g.gdn_head_dim,
            gdn_conv_kernel: g.gdn_conv_kernel,
            n_experts: g.n_experts,
            experts_per_token: g.experts_per_token,
            moe_dim: g.moe_dim,
            shared_dim: g.shared_dim,
            attn_output_gate: g.attn_output_gate,
            total_parameters,
        }
    }

    /// The dense geometry at `n_ctx`; `None` for another family. The engine constants the
    /// manifest does not carry (threads, the integer epsilon, the tile) are the family's own.
    pub fn dense_geometry(&self, n_ctx: u32) -> Option<PalwQwen25GeometryV1> {
        (self.family == PalwModelFamilyV1::DenseA16).then_some(PalwQwen25GeometryV1 {
            layer_count: self.layer_count,
            hidden_dim: self.hidden_dim,
            ffn_dim: self.ffn_dim,
            attn_heads: self.attn_heads,
            attn_kv_heads: self.attn_kv_heads,
            attn_head_dim: self.attn_head_dim,
            vocab_size: self.vocab_size,
            n_ctx,
            ..QWEN25_1_5B
        })
    }

    /// The hybrid geometry at `n_ctx`; `None` for another family. The rotary base, the epsilon,
    /// the thread count and the tile are the family's own.
    pub fn hybrid_geometry(&self, n_ctx: u32) -> Option<PalwQwen36GeometryV1> {
        (self.family == PalwModelFamilyV1::HybridQwen36).then_some(PalwQwen36GeometryV1 {
            layer_count: self.layer_count,
            full_attention_interval: self.full_attention_interval,
            hidden_dim: self.hidden_dim,
            attn_heads: self.attn_heads,
            attn_kv_heads: self.attn_kv_heads,
            attn_head_dim: self.attn_head_dim,
            rope_dims: self.rope_dims,
            gdn_k_heads: self.gdn_k_heads,
            gdn_v_heads: self.gdn_v_heads,
            gdn_head_dim: self.gdn_head_dim,
            gdn_conv_kernel: self.gdn_conv_kernel,
            n_experts: self.n_experts,
            experts_per_token: self.experts_per_token,
            moe_dim: self.moe_dim,
            shared_dim: self.shared_dim,
            attn_output_gate: self.attn_output_gate,
            vocab_size: self.vocab_size,
            n_ctx,
            ..QWEN36_35B_A3B
        })
    }

    /// **The class's graph at `n_ctx`** — the family's graph-v5 row over the artifact's epsilon,
    /// the same projection the registered rows are built through, so a manifest of a shipped class
    /// names the shipped class id. Refused past the geometry ceiling like any row.
    pub fn profile(&self, n_ctx: u32) -> Result<PalwShapeProfileV3, PalwStepError> {
        match self.family {
            PalwModelFamilyV1::DenseA16 => {
                qwen25_a16_artifact_row_profile_v5(self.dense_geometry(n_ctx).expect("the family is dense"))
            }
            PalwModelFamilyV1::HybridQwen36 => {
                qwen36_artifact_row_profile_v5(self.hybrid_geometry(n_ctx).expect("the family is hybrid"))
            }
        }
    }

    /// The artifact's bytes by the family formula, scaled to the card's total when one is stated.
    pub fn artifact_bytes(&self) -> PalwArtifactBytesV1 {
        let formula = match self.family {
            PalwModelFamilyV1::DenseA16 => palw_qwen25_artifact_bytes_v1(&self.dense_geometry(1).expect("dense")),
            PalwModelFamilyV1::HybridQwen36 => palw_qwen36_artifact_bytes_v1(&self.hybrid_geometry(1).expect("hybrid")),
        };
        match self.total_parameters {
            Some(total) => formula.scaled_to_total(total),
            None => formula,
        }
    }
}

/// One seat budget's answer at one context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwMeasuredPlanV1 {
    pub seat_budget_bytes: u64,
    /// `None` when no plan of up to the search's shard bound fits the budget.
    pub shard_count: Option<u32>,
    pub widest_seat_bytes: Option<u64>,
    pub boundary_bytes_per_job: Option<u64>,
}

/// Everything recomputable at one context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwMeasuredRowV1 {
    pub n_ctx: u32,
    /// The class id at this context, or `None` past the geometry ceiling.
    pub shape_profile_id_hex: Option<String>,
    pub fit_admitted: bool,
    pub refusing_walls: Vec<String>,
    pub kv_row_bytes: u64,
    pub kv_cache_bytes: u64,
    pub recurrent_state_bytes: u64,
    pub boundary_row_bytes: u64,
    pub plans: Vec<PalwMeasuredPlanV1>,
}

/// The half the chain recomputes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwMeasuredDeterministicV1 {
    pub artifact_bytes: u64,
    pub artifact_basis: String,
    pub rows: Vec<PalwMeasuredRowV1>,
}

/// The half the chain cannot recompute and does not believe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwSelfReportedV1 {
    /// The adder's measured replay rate, on the adder's host (`palw-certify`'s number).
    pub replay_ms_per_position: Option<u64>,
    pub measured_on: String,
    /// Derived from the self-report at the ruleset's cadence: `window_receipt × 120 000 / ms`
    /// positions — what ADR-0082 Decision 9 would admit IF the rate held on the network's hosts.
    pub positions_within_window_receipt: Option<u64>,
    /// The sentence that travels with every self-reported number.
    pub verified_by: String,
}

pub const PALW_SELF_REPORT_VERIFIED_BY_V1: &str = "the certification drill (ADR-0075 Decision 7) measures a seat's rate on the network's own hosts, and window_receipt × rate is what admission bounds (ADR-0082 Decision 9); a self-reported number admits nothing";

/// **The document.**
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwMeasuredModelV1 {
    pub schema: String,
    pub manifest: PalwModelManifestV1,
    /// The ruleset the deterministic half was evaluated on, by name and by fingerprint.
    pub ruleset: String,
    pub ruleset_fingerprint_hex: String,
    pub deterministic: PalwMeasuredDeterministicV1,
    pub self_reported: PalwSelfReportedV1,
    /// The adder's ML-DSA-87 public key and its signature over [`palw_measured_model_id_v1`], hex;
    /// empty until signed.
    pub adder_pubkey_hex: String,
    pub signature_hex: String,
}

/// The id: borsh over the document with the signature cleared, under the domain.
pub fn palw_measured_model_id_v1(doc: &PalwMeasuredModelV1) -> Hash64 {
    let mut unsigned = doc.clone();
    unsigned.signature_hex.clear();
    let bytes = borsh::to_vec(&unsigned).expect("the document is borsh-serializable");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_MEASURED_MODEL_DOMAIN_ID_V1).to_state();
    s.update(&bytes);
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// What the generator is handed beside the manifest.
#[derive(Clone, Copy, Debug)]
pub struct PalwMeasureInputsV1<'a> {
    pub ruleset: &'a str,
    pub ruleset_fingerprint_hex: &'a str,
    pub bundle: &'a PalwConsensusParamsV2,
    pub contexts: &'a [u32],
    pub seat_budgets: &'a [u64],
    pub max_shards: u32,
}

/// The court a profile is judged under — the caller's reading of the fences
/// (`palw_admission_shape_at_v1`), handed in as a function because it needs the `Params`.
pub type PalwCourtForV1<'a> = &'a dyn Fn(&PalwShapeProfileV3) -> (Option<PalwKaryCourtV1>, PalwPromptIdsFormV1);

fn hex(h: Hash64) -> String {
    h.as_byte_slice().iter().map(|b| format!("{b:02x}")).collect()
}

/// **Measure.** Every deterministic field from the manifest and the ruleset; the self-report as
/// given, with its derived window figure and its sentence.
pub fn palw_measure_model_v1(
    manifest: &PalwModelManifestV1,
    inputs: PalwMeasureInputsV1<'_>,
    court_for: PalwCourtForV1<'_>,
    replay_ms_per_position: Option<u64>,
    measured_on: &str,
) -> PalwMeasuredModelV1 {
    let artifact = manifest.artifact_bytes();
    let rows = inputs
        .contexts
        .iter()
        .map(|&n_ctx| match manifest.profile(n_ctx) {
            Ok(profile) => {
                let (court, form) = court_for(&profile);
                let fit = palw_model_fit_v1(&profile, inputs.bundle, court, form);
                let plans = inputs
                    .seat_budgets
                    .iter()
                    .map(|&budget| match palw_shard_plan_for_seat_v1(&profile, &artifact, budget, inputs.max_shards) {
                        Ok(plan) => PalwMeasuredPlanV1 {
                            seat_budget_bytes: budget,
                            shard_count: Some(plan.shard_count),
                            widest_seat_bytes: Some(plan.widest_seat_bytes),
                            boundary_bytes_per_job: Some(plan.boundary_bytes_per_job(u64::from(n_ctx))),
                        },
                        Err(_) => PalwMeasuredPlanV1 {
                            seat_budget_bytes: budget,
                            shard_count: None,
                            widest_seat_bytes: None,
                            boundary_bytes_per_job: None,
                        },
                    })
                    .collect();
                PalwMeasuredRowV1 {
                    n_ctx,
                    shape_profile_id_hex: Some(hex(profile.shape_profile_id())),
                    fit_admitted: fit.admitted(),
                    refusing_walls: fit.refusing_walls().iter().map(|w| w.name().to_string()).collect(),
                    kv_row_bytes: fit.seat.kv_row_bytes,
                    kv_cache_bytes: fit.seat.kv_cache_bytes,
                    recurrent_state_bytes: fit.seat.recurrent_state_bytes,
                    boundary_row_bytes: u64::from(profile.hidden_dim) * 4,
                    plans,
                }
            }
            Err(_) => PalwMeasuredRowV1 {
                n_ctx,
                shape_profile_id_hex: None,
                fit_admitted: false,
                refusing_walls: vec![crate::palw_model_fit_v1::PalwFitWallV1::GeometryCeiling.name().to_string()],
                kv_row_bytes: 0,
                kv_cache_bytes: 0,
                recurrent_state_bytes: 0,
                boundary_row_bytes: u64::from(manifest.hidden_dim) * 4,
                plans: Vec::new(),
            },
        })
        .collect();
    let positions_within_window_receipt = replay_ms_per_position
        .filter(|ms| *ms > 0)
        .map(|ms| inputs.bundle.state.window_receipt().saturating_mul(PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS / ms));
    PalwMeasuredModelV1 {
        schema: PALW_MEASURED_MODEL_SCHEMA_V1.to_string(),
        manifest: manifest.clone(),
        ruleset: inputs.ruleset.to_string(),
        ruleset_fingerprint_hex: inputs.ruleset_fingerprint_hex.to_string(),
        deterministic: PalwMeasuredDeterministicV1 {
            artifact_bytes: artifact.total(),
            artifact_basis: artifact.basis.to_string(),
            rows,
        },
        self_reported: PalwSelfReportedV1 {
            replay_ms_per_position,
            measured_on: measured_on.to_string(),
            positions_within_window_receipt,
            verified_by: PALW_SELF_REPORT_VERIFIED_BY_V1.to_string(),
        },
        adder_pubkey_hex: String::new(),
        signature_hex: String::new(),
    }
}

/// One field's verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwMeasuredCheckV1 {
    /// Recomputed and equal.
    Recomputed { field: String },
    /// Recomputed and different: the document is refused by this field.
    Mismatch { field: String, expected: String, got: String },
    /// Not recomputable; carried, and what verifies it instead.
    SelfReported { field: String, verified_by: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMeasuredVerificationV1 {
    pub checks: Vec<PalwMeasuredCheckV1>,
}

impl PalwMeasuredVerificationV1 {
    /// No recomputed field disagrees.
    pub fn deterministic_ok(&self) -> bool {
        !self.checks.iter().any(|c| matches!(c, PalwMeasuredCheckV1::Mismatch { .. }))
    }

    pub fn mismatches(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter_map(|c| match c {
                PalwMeasuredCheckV1::Mismatch { field, .. } => Some(field.clone()),
                _ => None,
            })
            .collect()
    }
}

/// **Verify by recomputation.** The document's manifest is measured again on the given ruleset and
/// every deterministic field is compared with what it says; the self-reported fields are listed
/// with what verifies them instead.
pub fn palw_verify_measured_model_v1(
    doc: &PalwMeasuredModelV1,
    inputs: PalwMeasureInputsV1<'_>,
    court_for: PalwCourtForV1<'_>,
) -> PalwMeasuredVerificationV1 {
    let contexts: Vec<u32> = doc.deterministic.rows.iter().map(|r| r.n_ctx).collect();
    let budgets: Vec<u64> =
        doc.deterministic.rows.first().map(|r| r.plans.iter().map(|p| p.seat_budget_bytes).collect()).unwrap_or_default();
    let again = palw_measure_model_v1(
        &doc.manifest,
        PalwMeasureInputsV1 { contexts: &contexts, seat_budgets: &budgets, ..inputs },
        court_for,
        doc.self_reported.replay_ms_per_position,
        &doc.self_reported.measured_on,
    );
    let mut checks = Vec::new();
    let mut check = |field: &str, expected: String, got: String| {
        checks.push(if expected == got {
            PalwMeasuredCheckV1::Recomputed { field: field.to_string() }
        } else {
            PalwMeasuredCheckV1::Mismatch { field: field.to_string(), expected, got }
        });
    };
    check("schema", PALW_MEASURED_MODEL_SCHEMA_V1.to_string(), doc.schema.clone());
    check("ruleset", inputs.ruleset.to_string(), doc.ruleset.clone());
    check("ruleset_fingerprint_hex", inputs.ruleset_fingerprint_hex.to_string(), doc.ruleset_fingerprint_hex.clone());
    check("artifact_bytes", again.deterministic.artifact_bytes.to_string(), doc.deterministic.artifact_bytes.to_string());
    check("artifact_basis", again.deterministic.artifact_basis.clone(), doc.deterministic.artifact_basis.clone());
    for (expected, got) in again.deterministic.rows.iter().zip(doc.deterministic.rows.iter()) {
        let at = |f: &str| format!("rows[{}].{f}", got.n_ctx);
        check(&at("shape_profile_id_hex"), format!("{:?}", expected.shape_profile_id_hex), format!("{:?}", got.shape_profile_id_hex));
        check(&at("fit_admitted"), expected.fit_admitted.to_string(), got.fit_admitted.to_string());
        check(&at("refusing_walls"), format!("{:?}", expected.refusing_walls), format!("{:?}", got.refusing_walls));
        check(&at("kv_row_bytes"), expected.kv_row_bytes.to_string(), got.kv_row_bytes.to_string());
        check(&at("kv_cache_bytes"), expected.kv_cache_bytes.to_string(), got.kv_cache_bytes.to_string());
        check(&at("recurrent_state_bytes"), expected.recurrent_state_bytes.to_string(), got.recurrent_state_bytes.to_string());
        check(&at("boundary_row_bytes"), expected.boundary_row_bytes.to_string(), got.boundary_row_bytes.to_string());
        check(&at("plans"), format!("{:?}", expected.plans), format!("{:?}", got.plans));
    }
    check("rows.len", again.deterministic.rows.len().to_string(), doc.deterministic.rows.len().to_string());
    check(
        "self_reported.positions_within_window_receipt",
        format!("{:?}", again.self_reported.positions_within_window_receipt),
        format!("{:?}", doc.self_reported.positions_within_window_receipt),
    );
    check("self_reported.verified_by", PALW_SELF_REPORT_VERIFIED_BY_V1.to_string(), doc.self_reported.verified_by.clone());
    checks.push(PalwMeasuredCheckV1::SelfReported {
        field: "self_reported.replay_ms_per_position".into(),
        verified_by: PALW_SELF_REPORT_VERIFIED_BY_V1.into(),
    });
    checks
        .push(PalwMeasuredCheckV1::SelfReported { field: "self_reported.measured_on".into(), verified_by: "nobody: a label".into() });
    PalwMeasuredVerificationV1 { checks }
}
