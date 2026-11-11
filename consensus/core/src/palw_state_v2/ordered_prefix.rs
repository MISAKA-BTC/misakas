//! Full accepted-prefix replay shares initialization and ordered application
//! with the canonical fold. It never runs per-object post-maintenance or
//! labels a partial state/delta as a completed block transition.
use super::*;

/// Read-only rehearsal of a complete candidate accepted list. Every native
/// writer and its journal stay under one construction reservation. This is
/// NOT a finalized state, an accepted block or permission to publish a delta.
/// If candidate filtering changes any input, discard this owner and replay
/// the entire updated list, including raw-action positions and fold inputs.
pub struct PalwOrderedPrefixV1<'a> {
    pub(super) builder: TransitionBuilder<'a>,
    pub(super) ncp_at_draw: BTreeSet<Hash64>,
    pub(super) thin_at_draw: BTreeSet<Hash64>,
}
impl PalwOrderedPrefixV1<'_> {
    pub fn state_v1(&self) -> &PalwChainStateV2 {
        &self.builder.state
    }
    pub fn journal_len_v1(&self) -> usize {
        self.builder.entries.len()
    }
}

/// Local retry envelope, not a consensus tariff or a bound on the work of
/// one court operation. Existing per-operation work/IO/heap limits still apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOrderedReplayLimitsV1 {
    pub passes: u64,
    /// Sum of ordinary objects, native handoffs and raw actions over all
    /// attempted lists, including refused passes. Charged before preparation.
    pub candidate_visits: u64,
}

/// Resolve these switches once from the canonical accepting point. Per-pass
/// extras still require the same canonical policy and rebuilt candidate facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOrderedReplayRulesV1 {
    pub unavailable_abstains: bool,
    pub capability_bound: bool,
    pub uncertified_weightless: bool,
    pub da_court: bool,
}

/// One owner across retries, pinned to an immutable parent, rules and point.
/// Not Clone: a failed pass does not refund its amplification allowance.
pub struct PalwOrderedReplayBudgetV1<'a> {
    parent: &'a PalwChainStateV2,
    params: &'a PalwStateParamsV2,
    point: PalwBlockContextV2,
    remaining: PalwOrderedReplayLimitsV1,
    rules: PalwOrderedReplayRulesV1,
}
impl<'a> PalwOrderedReplayBudgetV1<'a> {
    pub fn new_v1(
        parent: &'a PalwChainStateV2,
        params: &'a PalwStateParamsV2,
        point: PalwBlockContextV2,
        rules: PalwOrderedReplayRulesV1,
        limits: PalwOrderedReplayLimitsV1,
    ) -> Result<Self, PalwStateV2Error> {
        if point.block == crate::blockhash::ORIGIN || point.block == Hash64::default() {
            return Err(PalwStateV2Error::CarriageInconsistent("ordered history replay requires a concrete accepting block".into()));
        }
        if limits.passes == 0 {
            return Err(replay_capacity_v1("no replay pass admitted"));
        }
        Ok(Self { parent, params, point, remaining: limits, rules })
    }
    pub fn remaining_v1(&self) -> PalwOrderedReplayLimitsV1 {
        self.remaining
    }
    fn begin_v1(&mut self, objects: usize, extras: &PalwTransitionExtrasV1) -> Result<(), PalwStateV2Error> {
        let counts = [
            objects,
            extras.batch_court_carriers_v1.len(),
            extras.tokenizer_registration_carriers_v1.len(),
            extras.positive_assignment_actions.len(),
        ];
        let visits = counts
            .into_iter()
            .try_fold(0u64, |sum, n| u64::try_from(n).ok().and_then(|n| sum.checked_add(n)))
            .ok_or_else(|| replay_capacity_v1("candidate visit count overflow"))?;
        let passes = self.remaining.passes.checked_sub(1).ok_or_else(|| replay_capacity_v1("replay pass allowance exhausted"))?;
        let candidate_visits = self
            .remaining
            .candidate_visits
            .checked_sub(visits)
            .ok_or_else(|| replay_capacity_v1("cumulative candidate visit allowance exhausted"))?;
        self.remaining = PalwOrderedReplayLimitsV1 { passes, candidate_visits };
        Ok(())
    }
}
fn replay_capacity_v1(detail: &str) -> PalwStateV2Error {
    PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwOrderedReplayLimitsV1>(
        "ordered prefix replay",
        detail,
    ))
}

/// Position in the exact input list supplied for this pass, never a wire
/// ordinal invented from a filtered ordinary-object index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwOrderedReplayLocationV1 {
    Budget,
    Preparation,
    Order,
    Ordinary { object_index: usize },
    Batch { accepted_tx_ordinal: usize },
    Tokenizer { accepted_tx_ordinal: usize },
    RawAssignment { action_index: usize, accepted_tx_ordinal: Option<u32> },
}
#[derive(Debug)]
pub struct PalwOrderedReplayFailureV1 {
    pub location: PalwOrderedReplayLocationV1,
    pub error: PalwStateV2Error,
}
impl PalwOrderedReplayFailureV1 {
    /// Explicit duplicates and the unsupported public profile may be
    /// considered for filtering. Unknown/new errors remain hard, including
    /// arithmetic/state faults. Future active TFO3 semantic refusals need
    /// their own typed category; generic carriage errors stay hard.
    pub fn ordinary_refusal_index_v1(&self) -> Option<usize> {
        if !matches!(
            self.error,
            PalwStateV2Error::DuplicateClass(_)
                | PalwStateV2Error::DuplicateBond(_)
                | PalwStateV2Error::DuplicateBondKey(_)
                | PalwStateV2Error::DuplicateClaim(_)
                | PalwStateV2Error::DuplicateExecution(_)
                | PalwStateV2Error::DuplicateSession(_)
                | PalwStateV2Error::PublicOriginalProfileUnavailable { .. }
        ) {
            return None;
        }
        match self.location {
            PalwOrderedReplayLocationV1::Ordinary { object_index } => Some(object_index),
            _ => None,
        }
    }
}

/// Uses the actual fold's complete input basis. No unmetered production
/// entrypoint exists: all retries share one budget, charged before any fold
/// initialization. This bounds repeated visits, not each operation's CPU work.
/// Errors discard the whole candidate. Rebuild all input/position maps after
/// filtering; never concatenate singleton journals or reuse stale extras.
#[allow(clippy::too_many_arguments)]
pub fn palw_v2_replay_accepted_prefix_v1<'a>(
    budget: &mut PalwOrderedReplayBudgetV1<'a>,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
    extras: &'a PalwTransitionExtrasV1,
) -> Result<PalwOrderedPrefixV1<'a>, PalwOrderedReplayFailureV1> {
    use PalwOrderedReplayLocationV1 as At;
    budget.begin_v1(accepted_objects.len(), extras).map_err(|error| PalwOrderedReplayFailureV1 { location: At::Budget, error })?;
    let parent = budget.parent;
    let params = budget.params;
    let ctx = &budget.point;
    let _observations = crate::palw_court_observation_v1::CourtObservationGuardV1::enter(ctx.block);
    crate::palw_attempt_rules_v1::palw_with_prompt_root_block_memo_v2(ctx.block, || {
        let mut prefix = prepare_ordered_prefix_v1(
            parent,
            params,
            ctx,
            accepted_objects,
            block_work,
            merged_work,
            own_execution_key,
            budget.rules.unavailable_abstains,
            budget.rules.capability_bound,
            budget.rules.uncertified_weightless,
            budget.rules.da_court,
            extras,
        )
        .map_err(|error| PalwOrderedReplayFailureV1 { location: At::Preparation, error })?;
        let mut location = At::Order;
        apply_ordered_prefix_located_v1(&mut prefix.builder, parent, ctx, accepted_objects, &mut location)
            .map_err(|error| PalwOrderedReplayFailureV1 { location, error })?;
        Ok(prefix)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_ordered_prefix_v1<'a>(
    parent: &'a PalwChainStateV2,
    params: &'a PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
    unavailable_abstains: bool,
    capability_bound: bool,
    uncertified_weightless: bool,
    da_court: bool,
    extras: &'a PalwTransitionExtrasV1,
) -> Result<PalwOrderedPrefixV1<'a>, PalwStateV2Error> {
    // 1. Context monotonicity: blue score strictly increases along a chain, DAA never decreases.
    if let Some(last) = &parent.last_point {
        if ctx.blue_score <= last.blue_score {
            return Err(PalwStateV2Error::NonMonotonicContext("blue_score must strictly increase along a chain"));
        }
        if ctx.daa_score < last.daa_score {
            return Err(PalwStateV2Error::NonMonotonicContext("daa_score must not decrease along a chain"));
        }
    }

    let mut builder =
        TransitionBuilder::try_new(parent, params, unavailable_abstains, capability_bound, uncertified_weightless, da_court, extras)?;
    builder = builder.with_native_original_parent_v19(parent);
    builder.claim_local_lock_point_v6 = params.claim_local_settlement_active_at(ctx.daa_score).then_some(*ctx);
    // Data-only observations must begin before object/deadline execution:
    // retirement may compact a completed Pending into A5 in this same fold.
    builder.claim_local_capture_retired_exec_events_v24 = builder.claim_local_lock_point_v6.is_some();
    builder.claim_local_input_basis_v12 = if params.claim_local_settlement_active_at(ctx.daa_score)
        && palw_tir_public_fold_v1::native_input_measurement_enabled_v12(extras)
    {
        Some(palw_tir_public_fold_v1::NativeFoldInputBasisV12::from_native_fold_v12(
            ctx,
            accepted_objects,
            block_work,
            merged_work,
            own_execution_key,
        )?)
    } else {
        None
    };
    builder.claim_local_gc_defer_v3 = true;
    palw_claim_local_execution_cohort_census_v15::prepare_first_active_boundary_v15(&mut builder, parent, ctx)?;

    builder.native_reservation.reserve_output_owners_v1(ctx).map_err(PalwStateV2Error::NativeResource)?;
    builder.reserve_assignment_material_slots_v2(extras.positive_assignment_actions.len())?;

    // Prospective custody is authenticated against the pre-object parent. A same-block
    // enrollment/replacement never authorizes this block's capture. A carriage that does not
    // authenticate under THIS parent (a merged blue captured against its own, different prior
    // view; a consumed or replaced session) records nothing: its attempt then meets
    // `plan_native_ready`'s refusal and is skipped like any refused own/merged attempt. It never
    // fails the merging block's whole transition, which a third party's blue could otherwise force.
    if params.monetary_material_at_v1(ctx.daa_score).is_some() {
        for bytes in extras.exec_v2_origin_headers.values() {
            let Ok(header) = borsh::from_slice::<crate::header::Header>(bytes) else { continue };
            if !crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::is_carriage(&header.palw_commitment) {
                continue;
            }
            let Ok(ready) = crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::decode_wire(&header.palw_commitment)
            else {
                continue;
            };
            let a = &ready.base.attempt;
            let anchor = crate::palw_attempt_v2::execution_anchor_v3(
                a.network_domain,
                crate::hashing::header::pre_pow_hash_64(&header),
                a.class_id,
                &a.executor_bond,
                header.nonce,
            );
            match ready.verify_prior_with_recovery_typed_v4(
                parent,
                params,
                anchor,
                header.daa_score,
                crate::palw_attempt_material_v3::verify_ready_signature_v3,
            ) {
                Ok(()) => {
                    builder.monetary_native_ready.insert(ready.ready.binding.claim, (ready.ready.session, ready.ready.binding));
                }
                Err(crate::palw_attempt_material_v3::PalwNativeReadyVerificationErrorV4::LocalResource(why)) => {
                    return Err(PalwStateV2Error::NativeResource(why));
                }
                Err(crate::palw_attempt_material_v3::PalwNativeReadyVerificationErrorV4::NativeContext(why)) => {
                    return Err(PalwStateV2Error::NativeContextUnavailable(why));
                }
                Err(crate::palw_attempt_material_v3::PalwNativeReadyVerificationErrorV4::Invalid(_)) => {}
            }
        }
    }
    // 1b. Drain the payout queue THIS block's coinbase paid. It must happen before the sweeps,
    //     because the sweeps are what refill it: a claim finalized by this block is paid by the
    //     NEXT one, and clearing after would erase it. Every node applying this block clears the
    //     same set, because the set is the parent state's — already committed, already hashed
    //     into the root this block's header names.
    //
    //     **At most `PALW_V2_MAX_PAYOUTS_PER_BLOCK`, and the coinbase builder takes the same
    //     prefix.** Draining the whole queue made the coinbase's output count a function of how
    //     many claims happened to finalize at once, which is unbounded — while a coinbase may
    //     carry at most `ghostdag_k + 2 + PALW_V2_COINBASE_EXTRA_OUTPUTS`. On testnet-11 the very
    //     first `Final` took the count to 4 against a limit of 3 and the producer's own chain
    //     refused every block it built, 112 in a row, until production stopped entirely. A cap
    //     nothing enforces is a cap that decides, later, that the network is over.
    //
    //     The prefix is the first N in `BTreeMap` key order, which is the same set on every node
    //     and the same set `palw_v2_payout_outputs` pays. A backlog drains at N per block against
    //     at most one new claim per block, so the queue cannot grow.
    //
    //     **The second writer** (mainnet audit 2026-09-06, M-10): ADR-0087's market writes fee legs
    //     into this same map from two lanes, so "at most one new claim per block" is no longer the
    //     whole story. What holds the sentence up now is `PALW_V2_MAX_PENDING_PAYOUTS` — every
    //     market move is measured against it and refused by it, and past the 2026-09-23 audit fence
    //     so is every carrier refund (P-B1) — and market keys are minted in the top 1/256 of the key
    //     space, so this prefix still belongs to the claims it was sized for.
    block_preparation::prepare_block_objects_v1(
        &mut builder,
        parent,
        ctx,
        extras.block_facts(),
        extras.rules_at(ctx),
        block_preparation::ArtifactMigrationOrderV1::AfterMeshSweep,
    )?;

    // 3. The block's accepted objects, in consensus acceptance order.
    //
    //    Past the 2026-09-23 audit fence the class gate holds room for the block's OWN attempt
    //    while the objects fold (`TransitionBuilder::own_attempt_class`, the audit review of #11):
    //    a free-prompt commitment that would leave the attempt no room is the object refused, not
    //    the attempt at step 4. Released before step 4, where the attempt counts itself.
    //
    //    The reservation is held for whatever attempt the header carries, including one step 4
    //    then SKIPS at its exposure ceiling (finding 17) — so at a class's last slot such an attempt
    //    costs this block the commitments it refuses, of its own class and, past audit #4's review,
    //    of any other class whose object would take its room. That is the reservation's price, and
    //    it is accepted: the ceiling is `apply_attempt`'s verdict on the state step 3 leaves, the
    //    acceptance rehearsal knows only the header's class, and a rehearsal that reserved less
    //    than this fold would let through an object the fold refuses — which fails the block. An
    //    honest producer's headroom pre-check does not attach such an attempt; one that does gains
    //    no more than an attempt its bond can back already takes, the last slot for its own work.
    // 2e. **Lane F1 (the 2026-09-25 sweep's V03(1)): the claims this block anchors whose class could
    //     not seat a panel on the state the draw reads** — the pre-object base, which is fixed before
    //     anything this block carries (the processor derives the block's bindings on exactly this
    //     state, `palw_v2_pre_object_base_v1`). Only these may be re-anchored at step 4c, so no object,
    //     merged work or attempt the anchor producer chooses can turn a draw it did not like into a
    //     retry: where the class COULD seat a panel here, a claim left unbound voids as SW-8 always
    //     voided it. Empty below the registry-resilience fence and off anchor blocks.
    let ncp_at_draw: BTreeSet<Hash64> = match extras.sw8_anchor_delay {
        Some(anchor_delay) if builder.params.registry_resilience_active_at(ctx.daa_score) => {
            palw_claims_provisional_past_their_anchor_slot_v1(&builder.state, ctx.daa_score, anchor_delay)
                .into_iter()
                .filter(|claim_id| {
                    builder.state.claims.get(claim_id).is_some_and(|claim| builder.class_cannot_seat_a_panel_v1(claim, ctx))
                })
                .collect()
        }
        _ => BTreeSet::new(),
    };
    // 2e′. **Lane F2 (`Params::palw_floor_refusal_retry`): the claims this block anchors whose stake draw
    //      refuses for ELIGIBILITY at every seed on the same pre-object base** — SW-10's floor, fewer
    //      eligible operators than seats after the load filters, or no eligible outsider, with the
    //      draw's own inputs at each claim's anchor fact (`extras.sw8_draw`). Taken here, before
    //      anything this block carries, for V03(1)'s reason: no object, merged work or attempt the
    //      anchor producer chooses can turn a draw into a retry, and a verdict that holds at every seed
    //      left no panel to discard. Empty below the fence (and off anchor blocks), where every claim
    //      step 4c reaches voids as before.
    let thin_at_draw: BTreeSet<Hash64> = match extras.sw8_anchor_delay {
        Some(anchor_delay) if extras.sw8_draw.is_some() && builder.params.floor_refusal_retry_active_at(ctx.daa_score) => {
            let reach = extras.sw8_anchor_reach.map_or(ctx.daa_score, |reach| reach.min(ctx.daa_score));
            palw_sw8_thin_draws_v1(&builder.state, builder.params, extras, ctx.daa_score, anchor_delay, reach)?
        }
        _ => BTreeSet::new(),
    };
    if extras.audit_2026_09_23_active
        && let PalwBlockWorkV3::Attempt(envelope) = &block_work
    {
        // The attempt's room, read before any object of this block moves the demand (audit #4
        // review, item 2): step 4 asks it no room question again.
        builder.own_attempt_fit =
            builder.check_class_admits_claim(&envelope.attempt.class_id, ctx.daa_score, PalwGatedClaimV1::Attempt).is_ok();
        builder.own_attempt_class = Some(envelope.attempt.class_id);
    }
    source_bootstrap_v1::prepare(&mut builder, ctx)?;
    Ok(PalwOrderedPrefixV1 { builder, ncp_at_draw, thin_at_draw })
}

pub(super) fn apply_ordered_prefix_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
) -> Result<(), PalwStateV2Error> {
    apply_ordered_prefix_located_v1(builder, parent, ctx, accepted_objects, &mut PalwOrderedReplayLocationV1::Order)
}

fn apply_ordered_prefix_located_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    location: &mut PalwOrderedReplayLocationV1,
) -> Result<(), PalwStateV2Error> {
    let extras = builder.extras;
    use PalwOrderedReplayLocationV1 as At;
    *location = At::Order;
    use crate::palw_batch_court_v1::fold_order::{AcceptedNativeCourtItemV1, in_accepted_native_order_v1};
    // Only this whole ordered fold can name positions in its persistent delta.
    // Single-object rehearsal builders deliberately keep this scope absent.
    builder.tfp1_history_journal_parent = parent.last_point().map(|point| point.block);
    let ordered = in_accepted_native_order_v1(
        accepted_objects.iter(),
        &extras.batch_court_carriers_v1,
        &extras.tokenizer_registration_carriers_v1,
        |carrier| carrier.accepted_ordinal_v1(),
        |carrier| carrier.accepted_ordinal_v1(),
    )
    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("native carriers do not follow accepted lifecycle order".into()))?;
    if extras.tokenizer_registration_expected_fees_v1.len() != extras.tokenizer_registration_carriers_v1.len() {
        return Err(PalwStateV2Error::CarriageInconsistent("tokenizer fee projection and carrier counts differ".into()));
    }
    let tokenizer_parent_root = (!extras.tokenizer_registration_carriers_v1.is_empty()).then(|| parent.state_root());
    let mut raw_cursor = 0;
    let mut previous_raw_ordinal = None;
    let mut index = 0;
    for item in ordered {
        let before = match &item {
            AcceptedNativeCourtItemV1::Ordinary(_) => None,
            AcceptedNativeCourtItemV1::Batch(carrier) => Some(carrier.accepted_ordinal_v1()),
            AcceptedNativeCourtItemV1::Tokenizer(carrier) => Some(carrier.accepted_ordinal_v1()),
        };
        apply_raw_located_v1(builder, ctx, index, before, &mut raw_cursor, &mut previous_raw_ordinal, location)?;
        match item {
            AcceptedNativeCourtItemV1::Ordinary(object) => {
                *location = At::Ordinary { object_index: index };
                apply_object(builder, ctx, object)?;
                index += 1;
            }
            AcceptedNativeCourtItemV1::Batch(carrier) => {
                *location = At::Batch { accepted_tx_ordinal: carrier.accepted_ordinal_v1() };
                builder.apply_batch_court_carrier_v1(ctx, carrier)?;
            }
            AcceptedNativeCourtItemV1::Tokenizer(carrier) => {
                *location = At::Tokenizer { accepted_tx_ordinal: carrier.accepted_ordinal_v1() };
                let (fee, _refusal) = builder.apply_tokenizer_registration_at_v1(
                    parent,
                    tokenizer_parent_root.expect("carrier requires parent root"),
                    ctx,
                    carrier,
                )?;
                *location = At::Order;
                if extras.tokenizer_registration_expected_fees_v1.get(&carrier.carrier_v1()) != Some(&fee) {
                    return Err(PalwStateV2Error::CarriageInconsistent(
                        "tokenizer fee projection differs on actual accepted prefix".into(),
                    ));
                }
            }
        }
    }
    apply_raw_located_v1(builder, ctx, index, None, &mut raw_cursor, &mut previous_raw_ordinal, location)?;
    builder.tfp1_history_journal_parent = None;
    *location = At::Order;
    if raw_cursor != extras.positive_assignment_actions.len() {
        return Err(PalwStateV2Error::DeltaMismatch("Assignment action points beyond accepted object prefix"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_raw_located_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    index: usize,
    before: Option<usize>,
    cursor: &mut usize,
    previous: &mut Option<u32>,
    location: &mut PalwOrderedReplayLocationV1,
) -> Result<(), PalwStateV2Error> {
    positive_assignment::apply_raw_before_object_v2(builder, ctx, index, before, cursor, previous).map_err(|error| {
        *location = PalwOrderedReplayLocationV1::RawAssignment {
            action_index: *cursor,
            accepted_tx_ordinal: builder
                .extras
                .positive_assignment_actions
                .get(*cursor)
                .map(|scheduled| scheduled.action.origin().accepted_tx_ordinal),
        };
        error
    })
}
