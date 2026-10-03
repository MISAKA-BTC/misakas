//! **RFC-0003 §I.4 in the fold: the tensor claim** — dormant behind `Params::palw_fp_job_v5` over
//! `palw_gen_v1`. A child module of `palw_state_v2`, as the evaluation lane's fold is, so it reads the
//! builder and the state's tables directly and writes them only through their one writers.
//!
//! The commitment ([`apply_gen_tensor_commitment_v1`], reached by the `GenTensorCommitted` object the
//! acceptance walk builds from a version-10 payload): the claim is an FP claim with no quanta, no pwu, no
//! receipt rights and no weight — a weightless claim, as RFC-0004's evaluation claim is — and it holds
//! collateral as any claim of that work does. Everything else is derived and checked, in the order the
//! RFC states (§I.4.5): the fences, the bond, the class (a generative class of a tensor profile, not
//! frozen, admitting its claims), the job against the class, the work (`work_leaves` is the chain's count
//! of the job's step space, never the executor's word), the class's capacity (the fence's cap on claims in
//! flight), the work identity, the reservation and the executor's exposure ceiling.
//!
//! From the claim's creation the lane is the lane's: panel at the bind, receipts, licence, `Final`, the
//! court (a generative close against the claim's execution root and the class's row), and the slash on a
//! conviction. Nothing here pays: weight for tensor claims is a later fence (RFC-0003 §I.4.6).

use super::*;
use crate::palw_gen_claim_v1::{
    PalwGenClaimErrorV1, palw_gen_commitment_execution_root_v1, palw_gen_ids_within_bounds_v1, palw_gen_job_step_leaves_v1,
    palw_gen_split_ids_v1, palw_gen_work_id_v1,
};
use crate::palw_gen_job_v1::{PalwGenJobV1, palw_gen_job_resolve_class_v1};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::GenClaimRefused(why.into())
}

/// **What the object hands the tensor branch** — the fields of `GenTensorCommitted` it reads.
pub(super) struct PalwGenCommitV1<'a> {
    pub claim_id: &'a Hash64,
    pub class_id: &'a Hash64,
    pub bond: &'a PalwBondKeyV2,
    pub executor_pubkey: &'a [u8],
    pub work_leaves: u64,
    pub prompt_token_ids: &'a [u32],
    pub trace_root: &'a Hash64,
    pub output_root: &'a Hash64,
    pub execution_root: &'a Hash64,
    pub job_pin: &'a Hash64,
    pub job: &'a PalwGenJobV1,
}

impl PalwFoldReadV1<'_> {
    /// **How many of a class's claims are in flight on the free-prompt lane** (RFC-0003 §I.4.8): the live
    /// (non-terminal) claims of the class, whatever their quanta — the registry's in-flight index counts
    /// a weightless claim as one. The index files a free-prompt claim of ZERO quanta under its `eval_claims` lane
    /// (RFC-0004 A6's lane for the claims that earn nothing: no quanta to round into whole jobs), and a tensor
    /// claim is exactly such a claim, so the class's live free-prompt claims are both lanes'.
    pub(super) fn class_inflight_free_prompt_claims_v1(&self, class_id: &Hash64) -> u64 {
        self.with_inflight_index(|index| {
            index.get(class_id).map(|tally| tally.free_prompts.saturating_add(tally.eval_claims)).unwrap_or(0)
        })
    }
}

/// **The tensor claim's commitment** (RFC-0003 §I.4.5): see the module doc.
pub(super) fn apply_gen_tensor_commitment_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    c: PalwGenCommitV1<'_>,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    // 1. The fences: the second lock behind the acceptance walk's drop.
    if !builder.params.gen_active_at(daa) || !builder.params.fp_job_v5_active_at(daa) {
        return Err(refused("a tensor claim below palw_fp_job_v5 over palw_gen_v1 (RFC-0003 §I.4)"));
    }
    if builder.state.claims.contains_key(c.claim_id) {
        return Err(PalwStateV2Error::DuplicateClaim(*c.claim_id));
    }
    // The bond, as the free-prompt arm checks it: it exists, its key is the signer's, it is not retiring,
    // frozen or below the producer floor.
    let bond_record = builder.state.bonds.get(c.bond).ok_or(PalwStateV2Error::MissingBond(*c.bond))?;
    if bond_record.pubkey != c.executor_pubkey {
        return Err(PalwStateV2Error::BondKeyMismatch(*c.bond));
    }
    if let PalwBondStatusV2::Retiring { .. } = bond_record.status {
        return Err(PalwStateV2Error::RetiringBond(*c.bond));
    }
    if let Some(shortfall) = palw_bond_producer_floor_shortfall_v1(&builder.state, builder.params, c.bond, daa) {
        let floor = builder.params.min_collateral_sompi();
        return Err(PalwStateV2Error::ProducerBelowFloor { bond: *c.bond, collateral: floor.saturating_sub(shortfall), floor });
    }
    if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&builder.state, c.bond) {
        return Err(PalwStateV2Error::ProducerFrozen { bond: *c.bond });
    }
    // 2. The class: a registry row that is Active (not frozen, not awaiting its activation, not reclaimed), a
    // generative class of a tensor profile (a text class takes V4/V5). **The model registry's lifecycle does
    // not gate it**: a generative class's graph derives no priced work (the registry's `Registered` row, which
    // never admits, is all a pipeline class ever gets — its ramp is the attempt lane's probes, which a
    // pipeline has none of), so the lane's own bounds do the gating — the per-class cap on claims in flight
    // below, the executor's reservation against its collateral, and the panel's readiness.
    let class = builder.state.classes.get(c.class_id).ok_or(PalwStateV2Error::MissingClass(*c.class_id))?;
    match class.status {
        PalwClassStatusV2::Active => {}
        PalwClassStatusV2::Frozen { .. } => return Err(PalwStateV2Error::FrozenClass(*c.class_id)),
        ref other => return Err(refused(format!("the class is {other:?}, not Active"))),
    }
    let slash_value_per_pwu = class.slash_value_per_pwu;
    let row = builder.state.gen_classes.get(c.class_id).cloned().ok_or_else(|| refused("the claim's class is no generative class"))?;
    // **Readiness: a claim flows only once seats can replay it** (the coordinator's decision of 2026-10-01).
    // The registry's lifecycle would have asked this (a class leaves `Prefetching` only on ready seats) and
    // is not asked for a pipeline, so the lane asks it itself: at least a panel's worth of DISTINCT operators —
    // never the executor's — hold the class with a fresh possession proof (the registry's own five-clause
    // predicate, `model_registry_seat_is_ready`: active, above the floor, a fresh readiness V2 row, free
    // collateral for the readiness multiple). Without it a claim is accepted that no panel can be drawn for:
    // it holds its executor's reservation and one of the class's slots for the whole bind window and voids.
    // Where there is no registry there are no possession proofs to count, and the lane refuses.
    //
    // **The one function every class kind asks** (lane F's Proposal A, approved 2026-10-01): the possession
    // floor is `class_possession_v1`'s, shared with the IR and composite lanes (`palw_class_seating_v1`); the
    // independence floor is added there under the fence `palw_class_seating` and asked just below.
    let Some(fold) = builder.model_registry_fold().cloned() else {
        return Err(refused("no model registry is in force: a class's seats prove possession through it"));
    };
    let (ready, needed) = builder
        .read()
        .class_possession_v1(c.class_id, c.bond, daa, &fold)
        .ok_or(PalwStateV2Error::MissingBond(*c.bond))?;
    if ready < needed {
        return Err(PalwStateV2Error::GenClassNotReady { class: *c.class_id, ready, needed });
    }
    // Past `palw_class_seating` the same function asks the independence floor too (the possession floor above is its first
    // condition, kept under this lane's own name): a refusal names `ClassNotSeated`.
    builder.read().check_class_seated_v1(c.class_id, c.bond, daa)?;
    builder.check_class_verify_admits_v1(
        c.class_id,
        daa,
        crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::FreePrompt { work_leaves: c.work_leaves },
        true,
    )?;
    // 3. The job against the class (the version, the profile, the modes, the seed rule, every parameter in
    // the class's offers, one image reference per slot at its size), and — where the ids ride — their count
    // and the class's token bound (their hashes were the walk's).
    let accepted = palw_gen_job_resolve_class_v1(c.job, &row).map_err(|e| refused(format!("the job is not the class's: {e}")))?;
    if !matches!(accepted.profile, crate::palw_gen_v1::PalwGenProfileV1::Image | crate::palw_gen_v1::PalwGenProfileV1::Embedding) {
        return Err(refused("a tensor claim's class is an image or an embedding class"));
    }
    if accepted.public_da {
        let (prompt, negative) = palw_gen_split_ids_v1(c.prompt_token_ids, accepted.prompt_tokens);
        palw_gen_ids_within_bounds_v1(&row, &accepted, prompt, negative).map_err(|e| refused(format!("the carried ids: {e}")))?;
    }
    // The commitment's own roots, again (the walk's check, as the second lock): the execution root is the
    // tensor execution root of the claim's own parts.
    if *c.execution_root == Hash64::default() {
        return Err(PalwStateV2Error::UnadjudicableCommitment(*c.claim_id));
    }
    if crate::palw_gen_close_v1::palw_gen_tensor_execution_root_v1(
        &crate::palw_gen_job_v1::palw_gen_job_id_v1(c.job),
        c.class_id,
        c.work_leaves,
        c.trace_root,
        c.output_root,
    ) != *c.execution_root
    {
        return Err(refused("the execution root is not the tensor execution root of the commitment's own parts"));
    }
    // 4. The work: the chain's count of the job's step space (the court's own count), never the executor's
    // word. Refused, not corrected.
    let leaves = palw_gen_job_step_leaves_v1(&row, &accepted).map_err(|e: PalwGenClaimErrorV1| refused(e.to_string()))?;
    if leaves != c.work_leaves {
        return Err(refused("the claim's work_leaves are not the chain's count of its job's step space"));
    }
    // 5. The class's capacity: a generative class has no registry lifecycle row, hence no panel-room
    // budget, so the cap on claims in flight is the fence's.
    let cap = builder.params.gen_max_inflight_claims(accepted.profile);
    let inflight = builder.read().class_inflight_free_prompt_claims_v1(c.class_id);
    if inflight >= cap as u64 {
        return Err(PalwStateV2Error::GenClassInflightCapped { class: *c.class_id, inflight, cap });
    }
    // T-2(a)'s rule, over the lane's own cap (the registry's `c_class` is a lifecycle row's, which a pipeline
    // class never has): one bond holds at most half the class's slots (`⌈cap / 2⌉`) among its claims not yet
    // licensed, so one executor cannot take every slot of a class and refuse the others' claims.
    let share = (cap as u64).div_ceil(2);
    let unlicensed = builder.read().bond_class_unlicensed(c.class_id, c.bond);
    if (unlicensed as u64).saturating_add(1) > share {
        return Err(PalwStateV2Error::BondClassShareExceeded { bond: *c.bond, class: *c.class_id, unlicensed, share });
    }
    // 6. The work identity: one inference, one claim per bond.
    let work_id = palw_gen_work_id_v1(c.class_id, &c.job.tail(), &c.bond.0);
    if let Some(holder) = builder.state.work_ids.get(&work_id) {
        return Err(PalwStateV2Error::DuplicateWork { work_id, claim: *holder });
    }
    // 7. The reservation: the lane's stage-1 rule on the claim's leaves at the class's registered slash
    // value, under the executor's exposure ceiling as the free-prompt arm asks it.
    let raw = (c.work_leaves as u128).saturating_mul(slash_value_per_pwu as u128);
    let reserved = crate::palw_weight_cap_v1::palw_claim_weight_reservation_of_v1(builder.params, 0, 0, raw, daa);
    let bond_record = builder.state.bonds.get(c.bond).cloned().ok_or(PalwStateV2Error::MissingBond(*c.bond))?;
    let declared = if builder.capability_bound { palw_bond_capability_exposure_v1(&bond_record) } else { 0 };
    let own = if builder.params.rcore_plus_active_at(daa) {
        builder.committed_at(c.bond, daa)
    } else {
        builder
            .state
            .reserved_exposure(c.bond)
            .checked_add(builder.state.registration_exposure(c.bond))
            .ok_or(PalwStateV2Error::Overflow("total exposure"))?
    };
    let backed = own.checked_add(declared).ok_or(PalwStateV2Error::Overflow("total exposure"))?;
    let ceiling = if builder.params.rcore_plus_active_at(daa) {
        own.saturating_add(builder.gate_room(c.bond, daa, PalwRcoreGateV1::Work))
    } else {
        (bond_record.collateral as u128)
            .checked_mul(builder.params.fp_max_exposure_ratio_permille as u128)
            .ok_or(PalwStateV2Error::Overflow("exposure ceiling"))?
            / 1000
    };
    let would_reserve = backed.checked_add(reserved).ok_or(PalwStateV2Error::Overflow("reserved exposure"))?;
    if would_reserve > ceiling {
        return Err(PalwStateV2Error::FreePromptExposureCeiling { bond: *c.bond, backed, claim: reserved, ceiling });
    }
    // 8. The claim: no quanta, no pwu, no receipt rights, no weight, no escrow; the roots as committed;
    // the data-availability obligation the chain's (one chunk, retention from acceptance).
    let claim = PalwClaimStateV2 {
        source: PalwClaimSourceV2::FreePrompt { quanta: 0, spent: BTreeSet::new() },
        class_id: *c.class_id,
        bond: *c.bond,
        pwu: 0,
        accepted_daa: daa,
        rebound_daa: None,
        accepted_blue_score: ctx.blue_score,
        accepted_block: ctx.block,
        trace_root: *c.trace_root,
        output_root: *c.output_root,
        execution_root: *c.execution_root,
        trace_chunk_count: 1,
        trace_retention_daa: daa.saturating_add(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(builder.params)),
        reserved,
        immature_contribution: 0,
        escrowed_reward: 0,
        work_leaves: c.work_leaves,
        work_id: Some(work_id),
        phase: PalwClaimPhaseV2::Provisional,
        rights_reserved: 0,
        job_identity: if builder.extras.offence_attribution_active { *c.job_pin } else { Hash64::default() },
        rcore: PalwClaimRcoreV1::default(),
    };
    builder.reserve_for_claim(&claim)?;
    builder.write_claim(*c.claim_id, Some(claim));
    let deadline = daa.checked_add(builder.params.window_bind).ok_or(PalwStateV2Error::Overflow("bind deadline"))?;
    builder.arm_deadline(deadline, *c.claim_id);
    Ok(())
}

// A tensor claim's commitment root the fold recomputes, for a caller that holds the commitment.
#[allow(dead_code)]
fn palw_gen_commitment_root_is_its_parts_v1(commitment: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3, job: &PalwGenJobV1) -> bool {
    palw_gen_commitment_execution_root_v1(commitment, job) == commitment.execution_root
}
