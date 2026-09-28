//! **The node's pipeline-class loops** (RFC-0003 §II.2.1): a seat's receipt on a V5 claim and a
//! party's court objects, over `misaka_palw_base0::gen_worker` — dormant until the free-prompt lane
//! opens for pipeline classes ([`palw_gen_lane_open_v1`]: `palw_fp_job_v5` over `palw_gen_v1`).
//!
//! * **The seat.** A V5 claim is judged by replaying its job from the material the panel received
//!   (`gen_seat_judge_v1`). A matching execution root files `Valid`; a difference files NOTHING —
//!   the fault is the court's question, and a sampled verdict never slashes (the free-prompt seat's
//!   rule, ADR-0077 W10); material that is not the job's is not judged.
//! * **The court.** At the narrowed leaf a party builds its moves from the accused's capture
//!   (`gen_court_candidates_v1`) and files the one that wins its side: the challenger a conviction,
//!   the responder an acquittal or, at a dissected leaf, its root claim. Each close's declared
//!   verdict is the one the consensus check itself derives from it.
//!
//! **Logging (ADR-0079 SA-7):** nothing here logs prompt ids or image bytes; a refusal names the
//! rule.

use kaspa_consensus_core::config::params::Params;

/// The loops' bodies, from the node's worker crate (tested there): the held class and its V5 runs,
/// the seat's judgment and receipt, the capture, and a party's court moves and the objects it files.
pub use misaka_palw_base0::gen_worker::{
    GenCaptureV1, GenCourtMoveV1, GenHeldClassV1, GenSeatJudgmentV1, GenWorkV1, gen_court_candidates_v1, gen_court_object_v1,
    gen_first_divergence_v1, gen_seat_judge_v1, gen_seat_receipt_v1, gen_worker_answer_v1,
};

/// **Is the lane open for pipeline classes at `daa_score`?** Both fences in force: `palw_gen_v1`
/// (the registry, the court) and `palw_fp_job_v5` (the lane's V5 jobs). Below it no V5 claim can
/// reach a block, and these loops do not run.
pub fn palw_gen_lane_open_v1(params: &Params, daa_score: u64) -> bool {
    params.palw_gen_v1_active_at(daa_score) && params.palw_fp_job_v5_active_at(daa_score)
}
