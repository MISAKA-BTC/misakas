//! **RFC-0006 × G14: the non-seat cell watcher's targets** (agent SHARD, `docs/design/palw/shard-rfc6-10.md` §3) — pure over the
//! tip state; node policy, nothing folds.
//!
//! G14 for a sharded claim is one bonded verifier OUTSIDE the seats reaching a conviction (or a correctly classified default) from
//! public material only. Consensus already admits it — `TirShardCourtAccused` takes any Active bond at the floor, and a
//! `TirStepRun` demand any Active bond within the non-seat DA budget — so what a watcher needs from the chain is only WHAT to
//! check: [`palw_tir_shard_watch_duties_v1`] lists, for a bond, every shard of every live claim drawn per shard (lane A's or the
//! permissionless Panel's — both write the same per-shard record) that the bond does not seat and did not produce, as a
//! **watch duty**: the shard's outsider span (its whole shard, every segment), the watcher's bond, the claim's committed roots.
//!
//! A watch duty is the seat duty's shape so the seat's own cell verifier (`palw_tir_shard_outcome_v1`, node) and accusation builder
//! run on it unchanged, but it is NO seat: `seat_index` is `u8::MAX` and meaningless, and a watcher signs no receipt from it — it
//! accuses (its bond the accuser) or demands, nothing else.

use crate::palw_producer_v2::{PalwSeatDutyV2, PalwTirShardDutyV1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwStateParamsV2};
use crate::palw_tir_shard_v1::{palw_tir_panel_shard_slice_v1, palw_tir_shard_outsider_mask_v1};

/// A watch duty carries no seat: this index names none.
pub const PALW_TIR_SHARD_WATCH_NO_SEAT_V1: u8 = u8::MAX;

/// **Every shard `watcher` may watch at the tip**, as watch duties, in claim then shard order: a live claim drawn per shard
/// (`PanelBound`, or `ReceiptLicensed` while it is not `Final`), not produced by `watcher`, and each shard of it whose slice does
/// not seat `watcher` (a seat's own duty covers its shard). `receipt_deadline` is the claim's challenge horizon for a watcher — the
/// bound plus the receipt window, then the licence's challenge window — after which a finding is late; the node files within it.
pub fn palw_tir_shard_watch_duties_v1(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    watcher: &PalwBondKeyV2,
) -> Vec<PalwSeatDutyV2> {
    let mut out = Vec::new();
    for (claim_id, record) in state.tir_shard_claims_iter() {
        let Some(claim) = state.claim(claim_id) else { continue };
        let horizon = match claim.phase {
            PalwClaimPhaseV2::PanelBound { bound_daa } => bound_daa.saturating_add(state_params.window_receipt()),
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => {
                licensed_daa.saturating_add(state_params.window_challenge_at(licensed_daa))
            }
            _ => continue,
        };
        if claim.bond == *watcher {
            continue;
        }
        let Some(panel) = state.panel(claim_id) else { continue };
        let Some(artifact_root) = state.class(&claim.class_id).map(|class| class.artifact_root) else { continue };
        for shard in 0..record.s_l {
            let Some(slice) = palw_tir_panel_shard_slice_v1(&panel.seats, record.s_l, record.outsider, shard) else { continue };
            if slice.iter().any(|seat| seat.bond == *watcher) {
                continue;
            }
            out.push(PalwSeatDutyV2 {
                accepted_block: claim.accepted_block,
                claim_id: *claim_id,
                class_id: claim.class_id,
                artifact_root,
                seat_bond: *watcher,
                executor_bond: claim.bond,
                execution_root: claim.execution_root,
                trace_root: claim.trace_root,
                output_root: claim.output_root,
                bound_daa: panel.bound_daa,
                receipt_deadline: horizon,
                panel_anchor: panel.anchor,
                seat_index: PALW_TIR_SHARD_WATCH_NO_SEAT_V1,
                panel_seat_count: panel.seats.len() as u16,
                pwu: claim.pwu,
                quanta: match claim.source {
                    PalwClaimSourceV2::FreePrompt { quanta, .. } => quanta,
                    _ => 0,
                },
                free_prompt: matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }),
                work_leaves: claim.work_leaves,
                job_identity: claim.job_identity,
                // The outsider's span: the whole shard, every segment.
                tir_shard: Some(PalwTirShardDutyV1 {
                    shard,
                    s_l: record.s_l,
                    s_p: record.s_p,
                    outsider: true,
                    slice_index: 0,
                    segments: palw_tir_shard_outsider_mask_v1(record.s_p),
                }),
            });
        }
    }
    out
}
