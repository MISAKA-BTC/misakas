//! **ADR-0108 — a seat may demand the committed leaf it needs to judge.** The pure half: which
//! intervals a seat was assigned (a function of chain facts alone), which interval owns a leaf, and
//! so whether a demand for a leaf's evidence is one the chain assigned the demanding seat.
//!
//! The seat's draw (`palw_fp_interval_draw_v1`, ADR-0077 Decision 8) is not a consensus object: what
//! a seat checks before it signs a receipt is its own duty. A DEMAND is different. It obliges the
//! executor to put a leaf's evidence on chain or lose the claim, so the leaf it names must be one the
//! chain told that seat to check — never one the seat chose, never one an executor's worst case was
//! picked from (ADR-0108 Decision 3). This module is that bound, spelled from the binding the demand
//! carries (authenticated against the claim's `execution_root` by the held DA court before this is
//! asked), so the acceptance layer and every seat compute one answer.
//!
//! **The geometry is the seat's, in both units.** A class under the held map samples intervals of
//! POSITIONS over the whole job, prefill included (ADR-0103 Decision 2); every other class samples
//! intervals of decode CALLS, interval 0 holding the prefill. The widths are chain facts: the held
//! width is `palw_held_interval_positions_v1` of the class, the call width is the binding's own
//! `checkpoint_interval`.

use crate::Hash64;
use crate::palw_fp_interval_v1::{PALW_FP_SEAT_INTERVAL_SAMPLES_V1, palw_fp_interval_draw_v1};
use crate::palw_step::canonical_step_coordinates;
use crate::palw_step_leg::PalwStepBindingV2;

/// **How long a seat waits for the fast path before it demands on chain**, in DAA (ADR-0108
/// Decision 6). A node-side patience, not a consensus number: the demand's own window is ADR-0062's
/// `W_disclose`, and nothing on chain reads this. Long enough for one signed round trip on the
/// interval lane and one interval's replay at the executor; short against every receipt window.
pub const PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1: u64 = 6;

/// **A leaf-evidence request rides the interval lane under bit 29** (ADR-0108 Decision 2). Bits 31
/// and 30 clear (they are the block-leaves and resume requests'), bit 29 set, the leaf's interval
/// below it; the leaf itself rides the request's `leafIndex` field, which the signature binds. A
/// plain interval index never reaches bit 29 — a held job at 2M positions has about a thousand
/// intervals — so the four request kinds cannot collide.
pub const PALW_LEAF_EVIDENCE_REQUEST_BIT_V1: u32 = 1 << 29;

/// The request index a seat asks for leaf evidence under, for a leaf in `interval`.
pub fn palw_leaf_evidence_request_index_v1(interval: u32) -> Option<u32> {
    (interval < PALW_LEAF_EVIDENCE_REQUEST_BIT_V1).then_some(PALW_LEAF_EVIDENCE_REQUEST_BIT_V1 | interval)
}

/// `Some(interval)` for a leaf-evidence request index, `None` for any other kind.
pub fn palw_leaf_evidence_request_decode_v1(index: u32) -> Option<u32> {
    (index & (3 << 30) == 0 && index & PALW_LEAF_EVIDENCE_REQUEST_BIT_V1 != 0).then_some(index & !PALW_LEAF_EVIDENCE_REQUEST_BIT_V1)
}

/// The seat's sampling unit for one job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSeatIntervalUnitV1 {
    /// Intervals of decode calls, `width` calls each; interval 0 holds the prefill (call 0) and the
    /// first `width` decode calls.
    Calls { width: u32 },
    /// Intervals of positions over the whole job, prefill included (ADR-0103 Decision 2).
    Positions { width: u32 },
}

/// **The intervals of one claim's job, as a seat draws over them** — derived from the binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSeatIntervalGeometryV1 {
    pub prefill: u32,
    /// `exact_decode_tokens − 1`.
    pub decode_calls: u32,
    pub unit: PalwSeatIntervalUnitV1,
    pub count: u32,
}

impl PalwSeatIntervalGeometryV1 {
    /// From the binding alone: the job's two counts, the class's map and the leg's cadence.
    pub fn from_binding_v1(binding: &PalwStepBindingV2) -> Option<Self> {
        Self::from_parts_v1(&binding.shape_profile, &binding.job_context, binding.checkpoint_profile.checkpoint_interval)
    }

    /// From the class, the job's context and the leg's checkpoint interval — the form a family that
    /// holds no binding (the executor opening an interval, a seat before any opening arrives) asks.
    pub fn from_parts_v1(
        profile: &crate::palw_step::PalwShapeProfileV3,
        ctx: &crate::palw_v2::PalwJobContextV2,
        checkpoint_interval: u32,
    ) -> Option<Self> {
        let prefill = ctx.declared_prefill_tokens;
        let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
        if crate::palw_state_chunk_map::palw_profile_is_held_v4(profile) {
            let width = crate::palw_held_context_v1::palw_held_interval_positions_v1(profile);
            let held =
                crate::palw_held_context_v1::PalwHeldSeatIntervalsV1::from_chain_facts_v1(prefill, ctx.exact_decode_tokens, width)?;
            return Some(Self { prefill, decode_calls, unit: PalwSeatIntervalUnitV1::Positions { width }, count: held.count });
        }
        if checkpoint_interval == 0 {
            return None;
        }
        let count = decode_calls.div_ceil(checkpoint_interval).max(1);
        Some(Self { prefill, decode_calls, unit: PalwSeatIntervalUnitV1::Calls { width: checkpoint_interval }, count })
    }

    /// **The interval that owns step leaf `leaf`** — by its coordinate's step (a prefill position
    /// `p` is step `p + 1`, decode call `c` is step `prefill + c`), in the unit the seat samples.
    pub fn interval_of_leaf_v1(&self, binding: &PalwStepBindingV2, leaf: u64) -> Option<u32> {
        self.interval_of_leaf_in_v1(&binding.shape_profile, &binding.job_context, leaf)
    }

    /// [`Self::interval_of_leaf_v1`] from the class and the context.
    pub fn interval_of_leaf_in_v1(
        &self,
        profile: &crate::palw_step::PalwShapeProfileV3,
        ctx: &crate::palw_v2::PalwJobContextV2,
        leaf: u64,
    ) -> Option<u32> {
        let coord = canonical_step_coordinates(profile, ctx, leaf)?;
        let interval = match self.unit {
            PalwSeatIntervalUnitV1::Positions { width } => {
                let step = if coord.call_index == 0 {
                    u64::from(coord.position) + 1
                } else {
                    u64::from(self.prefill) + u64::from(coord.call_index)
                };
                (step.saturating_sub(1) / u64::from(width.max(1))) as u32
            }
            PalwSeatIntervalUnitV1::Calls { width } => {
                if coord.call_index == 0 {
                    0
                } else {
                    (coord.call_index - 1) / width.max(1)
                }
            }
        };
        (interval < self.count).then_some(interval)
    }
}

/// Why a demand is not one the chain assigned the demanding seat.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwLeafDemandRefusalV1 {
    #[error("the job's intervals are not derivable from the binding")]
    NoGeometry,
    #[error("leaf {leaf} has no coordinates in this job")]
    LeafHasNoCoordinates { leaf: u64 },
    #[error("leaf {leaf} is in interval {interval}, and seat {seat_index}'s draw assigned {drawn:?}")]
    NotTheSeatsInterval { leaf: u64, interval: u32, seat_index: u8, drawn: Vec<u32> },
}

/// **Is `leaf` one the chain assigned seat `seat_index` to check?** The draw is the seat's own —
/// the panel's anchor, the claim, the seat's index and the job's interval count, `k` of them — and
/// the leaf must lie in one of the drawn intervals. Returns the interval.
pub fn palw_leaf_demand_is_the_seats_v1(
    network_domain: &Hash64,
    panel_anchor: &Hash64,
    claim_id: &Hash64,
    seat_index: u8,
    binding: &PalwStepBindingV2,
    leaf: u64,
) -> Result<u32, PalwLeafDemandRefusalV1> {
    let geometry = PalwSeatIntervalGeometryV1::from_binding_v1(binding).ok_or(PalwLeafDemandRefusalV1::NoGeometry)?;
    let interval = geometry.interval_of_leaf_v1(binding, leaf).ok_or(PalwLeafDemandRefusalV1::LeafHasNoCoordinates { leaf })?;
    let drawn =
        palw_fp_interval_draw_v1(network_domain, panel_anchor, claim_id, seat_index, PALW_FP_SEAT_INTERVAL_SAMPLES_V1, geometry.count);
    if !drawn.contains(&interval) {
        return Err(PalwLeafDemandRefusalV1::NotTheSeatsInterval { leaf, interval, seat_index, drawn });
    }
    Ok(interval)
}

/// **A leaf's evidence, built by the executor from its own retention** (ADR-0108 Decisions 1 and
/// 2) — one builder for the fast path's answer and the slow path's disclosure, so the two carry the
/// same bytes for one `(claim, leaf)`.
///
/// ADR-0085's path first: the leaf's interval opened with the close annex for that one leaf, and
/// the refutation assembled from it (`refutation_from_served_intervals`) — a replay of one interval
/// from its anchor, never of the job. The whole-capture prover is the fallback for a family or a
/// retention the annex path cannot serve; ADR-0085 X1 makes the two the same object. Then the
/// artifact rows the court will read (`operand_openings_for`, which records them under the carriage
/// the chain reads) and the prompt carriage of the network's form (ADR-0103 §10.3 item 11).
#[allow(clippy::too_many_arguments)]
pub fn palw_leaf_evidence_from_capture_v1(
    backend: &dyn crate::palw_backend::PalwExecutionBackendV1,
    capture: &[u8],
    prompt_token_ids: &[u32],
    claim: crate::palw_backend::PalwClaimRootsV1,
    work_leaves: u64,
    leaf: u64,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<crate::palw_shard_court_v1::PalwLeafEvidenceV1, String> {
    let shape = backend.capture_shape(capture).ok_or("the capture has no shape this family reads")?;
    let generated = backend.fp_committed_output_ids(capture).unwrap_or_default();
    let annexed = backend
        .fp_interval_of_leaf_v1(&shape.job_context, leaf)
        .ok_or_else(|| format!("leaf {leaf} is in no interval"))
        .and_then(|interval| {
            let opened = backend.open_fp_interval_with_close(capture, interval, prompt_token_ids, &[leaf])?;
            backend.refutation_from_served_intervals(&[(interval, opened)], claim, prompt_token_ids, &generated, work_leaves, leaf)
        });
    let refutation = match annexed {
        Ok(refutation) => refutation,
        Err(_) => backend.refutation_for_free_prompt_index(capture, leaf, prompt_token_ids)?,
    };
    let artifact_openings = backend.operand_openings_for(&refutation)?;
    let (refutation, prompt_ids_opening) = crate::palw_step_refute::palw_refutation_prompt_carriage_v1(form, refutation)
        .map_err(|e| format!("the prompt tile does not open: {e}"))?;
    Ok(crate::palw_shard_court_v1::PalwLeafEvidenceV1 { refutation, artifact_openings, prompt_ids_opening })
}
