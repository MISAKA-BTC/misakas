//! **An IR class's court close** (RFC-0002 Phase F, F6's node half).
//!
//! Once the ladder narrows a session to one step leaf, a legacy class closes with an arithmetic
//! refutation of that step (`refutation_for_index`, `operand_openings_for`). An IR class closes with
//! an IR proof instead (F5, appended to `PalwCourtVerdictProofV2`), built by the IR backend from the
//! ACCUSED capture — the same object whichever party builds it:
//!
//! * `TirCone` — the leaf's cone, demand-evaluated by the court over the units the close carries
//!   (either party: the court acquits an honest leaf and convicts a lying one, or convicts by
//!   PALW-TIR-33 an operand outside its proven interval);
//! * at a logits tile, for a challenger: `TirLogits` — the step tile against the same row's lanes in
//!   the committed trace, which convicts an executor that committed two different rows — and the
//!   decode-token door (`TirDecodeTokenTiled` / `TirDecodeToken`), which convicts a generated token
//!   that is not the greedy selection over its row, the challenger's own token naming the lane that
//!   beats it.
//!
//! A party files only the close that wins its side (the dissection's rule): the challenger a
//! conviction, the responder an acquittal. The other outcome needs no fee — the session's backstop
//! ends an unproven accusation on the challenger's side anyway.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
use kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
use misaka_palw_sdk::lineages::tir::{TirBackendV1, TirCaptureV1};

/// The IR closes a party may file at leaf `index`, in the order it tries them, each built or refused.
pub(crate) fn palw_tir_close_candidates_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    own: Option<&[u8]>,
    index: u64,
    rules: &PalwTirCourtRulesV1,
    challenger: bool,
) -> Vec<(&'static str, Result<PalwCourtVerdictProofV2, String>)> {
    let mut out = vec![("cone", tir.cone_close(accused, index, rules))];
    if !challenger {
        return out;
    }
    // At a logits tile the challenger also holds the two other doors. Its own execution names the
    // token it selected at that row — the lane that beats a wrongly committed one.
    let Ok(capture) = TirCaptureV1::decode(accused) else { return out };
    let ctx = &capture.binding.job_context;
    let space = tir.space();
    let Some(leaf) = space.leaf_at(ctx, index) else { return out };
    let post = (space.occurrences().len() - 1) as u32;
    let at_logits =
        matches!(leaf.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == space.program.logits);
    if !at_logits {
        return out;
    }
    out.push(("logits", tir.logits_close(accused, index)));
    let Some(row) = (leaf.position + 1).checked_sub(ctx.declared_prefill_tokens) else { return out };
    let own_token = own.and_then(|own| TirCaptureV1::decode(own).ok()).and_then(|mine| mine.generated.get(row as usize).copied());
    let committed = capture.generated.get(row as usize).copied();
    if let Some(beat) = own_token.filter(|t| Some(*t) != committed) {
        out.push(("decode token", tir.decode_token_close(accused, row, beat)));
    }
    out
}

/// Is `verdict` the side `i_am_responder` wins?
pub(crate) fn palw_tir_close_is_mine_v1(verdict: PalwCourtVerdictV2, i_am_responder: bool) -> bool {
    if i_am_responder { verdict == PalwCourtVerdictV2::ChallengerDefeated } else { verdict == PalwCourtVerdictV2::ExecutorGuilty }
}

/// The session a close is filed in, for the log.
pub(crate) fn palw_tir_close_note_v1(session_id: &Hash64, label: &str, verdict: PalwCourtVerdictV2, index: u64) -> String {
    format!("session {session_id} closes as {verdict:?} on IR step {index} ({label} close)")
}

/// **A seat's one-move case against an IR claim** (the input of an IR one-move accusation, on
/// chains whose court plays no bisection): the first step leaf at which the accused capture parts
/// from this seat's own execution of the job, and the IR closes that may convict there, in the
/// order a challenger tries them. `Ok(None)` when every step leaf agrees. (The object that carries
/// it, `TirShardCourtAccused`, is Phase F's next landing; until then only the tests call this.)
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn palw_tir_one_move_case_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    own: &[u8],
    rules: &PalwTirCourtRulesV1,
) -> Result<Option<(u64, Vec<(&'static str, Result<PalwCourtVerdictProofV2, String>)>)>, String> {
    let Some(leaf) = tir.first_divergent_leaf(accused, own)? else { return Ok(None) };
    Ok(Some((leaf, palw_tir_close_candidates_v1(tir, accused, Some(own), leaf, rules, true))))
}
