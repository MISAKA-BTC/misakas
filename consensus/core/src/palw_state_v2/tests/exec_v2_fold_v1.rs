//! **RFC-0008 v2 through the fold** — the work-session root, the six-rule slice admission, the accounting invariants, the `Final`
//! hold, the one settlement, the void and the retirement, on the R-core+ door fixtures (a 160-pwu claim bound to five sybil seats,
//! testnet-12's windows). Fence: `exec_v2_from_daa` at DAA 0 unless a test says otherwise.
//!
//! The fold does not verify signatures — the header stage verified every carrier's, and the acceptance walk the root declaration's —
//! so a declaration here carries a dummy signature. **`Verified` has no production door** (the verification route is a gate of spec
//! section 9): the tests that need a verified slice use the `#[cfg(test)]` writer `mark_slice_verified_for_tests`, which is
//! reachable by no node.

use super::*;
use crate::palw_exec_v2::*;
use crate::palw_work_slice_v2::*;

const COLLATERAL: u64 = 1_000_000_000;
/// The claim's pwu (`door_claim_bound`'s attempt).
const PWU: u64 = 160;
/// A job identity for the claim (the fold records it only under the attribution fence; the fixture writes it).
const JOB: u64 = 0x10B;

fn xp(fence: Option<u64>) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 600, 120, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_worker_carve_permille(620)
        .unwrap()
        .with_exec_v2_from_daa(fence)
}

fn xx() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { panel_economy_active: true, exec_v2_test_origin_daa: Some(100), ..door_extras(true) }
}

/// The extras of a block that declares a root: the chain-derived canonical work is in force (the root's prefix is the claim's).
fn xx_declare() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { canonical_work_daa: Some(0), ..xx() }
}

fn xx_slices(covered: Vec<PalwExecV2CoveredSliceV1>) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { exec_v2_covered: covered, ..xx() }
}

fn step_with(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    word: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    extras: &PalwTransitionExtrasV1,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    let applied =
        apply_palw_transition_v2_with_extras(parent, p, &ctx(word, daa, word), objects, None, true, false, false, false, extras)?;
    applied.0.assert_internal_consistency(p).expect("internal consistency after apply");
    applied.0.assert_deadline_consistency(p).expect("deadline consistency after apply");
    // The delta reproduces the fold and reverts to the parent (a reorg across this block is exact).
    assert_eq!(apply_delta_v2(parent, &applied.1, p).unwrap().state_root(), applied.0.state_root(), "the delta reproduces the fold");
    assert_eq!(
        revert_delta_v2(&applied.0, &applied.1, p).unwrap().state_root(),
        parent.state_root(),
        "the delta reverts to the parent"
    );
    // The carriage reproduces the state (a restart or an IBD from a pruning snapshot).
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&applied.0)).unwrap();
    let back =
        borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(applied.0.state_root())).expect("a restart");
    assert_eq!(back.state_root(), applied.0.state_root(), "the carriage round-trips");
    assert_eq!(back.exec_v2_counts_v1(), applied.0.exec_v2_counts_v1());
    Ok(applied)
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    word: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
    step_with(parent, p, word, daa, objects, &xx())
}

/// The world: a funded 160-pwu claim bound to the five sybil seats (the door fixture, in a 1,000,000-sompi block so the claim
/// escrows a reward), two spare bonds (20, 21) registered as potential executors, and the claim's job identity recorded.
/// Returns `(params, state, claim)`.
fn world(fence: Option<u64>) -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    world_with(xp(fence))
}

fn world_with(p: PalwStateParamsV2) -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    world_with_panel(p, true)
}

fn world_with_panel(p: PalwStateParamsV2, bind_panel: bool) -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    let mut objects = register_class_and_bond();
    // The producer's bond posts enough for more than one claim (the second-claim tests).
    if let Some(PalwConsensusObjectV2::BondRegistered { collateral, .. }) = objects.last_mut() {
        *collateral = COLLATERAL;
    }
    objects.extend((2..=6).map(|n| seat_bond_reg(n, COLLATERAL)));
    objects.extend([20, 21].map(|n| seat_bond_reg(n, COLLATERAL)));
    let (s0, _) = apply_door(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None, &xx()).expect("the registry");
    let env = attempt(PWU, 1);
    let claim = attempt_id_v2(&env.attempt);
    let funded = PalwBlockContextV2 { subsidy: 1_000_000, ..ctx(2, 101, 2) };
    let (mut s1, _) = apply_door(&s0, &p, &funded, &[], Some(&env), &xx()).expect("the claim lands");
    if !bind_panel {
        assert!(matches!(s1.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional));
        s1.claims.get_mut(&claim).unwrap().job_identity = h64(JOB);
        return (p, s1, claim);
    }
    let (mut s2, _) = apply_door(
        &s1,
        &p,
        &ctx(3, 102, 3),
        &[PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats: sybil_seats() }],
        None,
        &xx(),
    )
    .expect("the panel binds");
    assert!(matches!(s2.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
    assert!(s2.claim(&claim).unwrap().escrowed_reward > 0, "the claim escrows a reward");
    // Surgery: the fixture's attempt records no job identity (that is the attribution fence's), so write the one the session binds.
    s2.claims.get_mut(&claim).expect("the claim").job_identity = h64(JOB);
    (p, s2, claim)
}

#[test]
fn exec_root_freezes_rider_reallocation_without_disabling_riders_before_open() {
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_TRACE_CHUNKS, attempt_trace_manifest_root_v1};
    use crate::palw_capacity_s567_v1::palw_rider_challenge_v1;
    let (p, native, lead) = world_with_panel(xp(Some(0)).with_capacity_s567_mirror(None, Some(0), None), false);
    let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(1000).unwrap();
    let extras = xx();
    let context = ctx(5, 104, 5);
    let mut rider = attempt(PWU, 2);
    rider.attempt.challenge = palw_rider_challenge_v1(&lead, 0);
    rider.attempt.trace_root = h64(0x52);
    rider.attempt.trace_chunk_count = PALW_ATTEMPT_V2_TRACE_CHUNKS;
    rider.attempt.trace_manifest_root = attempt_trace_manifest_root_v1(rider.attempt.trace_root, rider.attempt.trace_chunk_count);
    rider.attempt.trace_retention_daa = native.claim(&lead).unwrap().trace_retention_daa;
    let riders = [rider];
    let escrow = native.claim(&lead).unwrap().escrowed_reward;
    assert!(escrow > 0);

    // Positive control: an admissible rider really takes half of this lead's escrow before an EXEC root exists.
    riders_cheap_refusal_v1(&native, &context, &lead, &riders).expect("ordinary rider reader");
    let mut before = TransitionBuilder::new(&native, &p, false, false, false, false, &extras);
    attach_riders_v1(&mut before, &context, &p, Some(&admission), &lead, &riders).expect("ordinary rider writer");
    assert_eq!(before.state.claim(&lead).unwrap().escrowed_reward, escrow / 2);
    assert_eq!(before.state.claim(&attempt_id_v2(&riders[0].attempt)).unwrap().escrowed_reward, escrow / 2);

    // Open while the native claim is still Provisional and within the same rider window.
    let (rooted, _) = step_with(&native, &p, 4, 103, &[open_object(decl(lead))], &xx_declare()).expect("root opens");
    let refusal = riders_cheap_refusal_v1(&rooted, &context, &lead, &riders).expect_err("root allocation is frozen");
    assert!(refusal.to_string().contains("EXEC root"), "{refusal}");
    let mut after = TransitionBuilder::new(&rooted, &p, false, false, false, false, &extras);
    let refusal = attach_riders_v1(&mut after, &context, &p, Some(&admission), &lead, &riders)
        .expect_err("full writer must enforce the same allocation freeze");
    assert!(refusal.to_string().contains("EXEC root"), "{refusal}");
    assert!(after.entries.is_empty(), "refusal precedes every financial write");
    assert_eq!(after.state.state_root(), rooted.state_root());
    assert_eq!(after.state.claim(&lead).unwrap().escrowed_reward, escrow);

    // Objects are queued before rider admission. Neither order in one block may split an allocation that block opened.
    // Exercise both the cheap-reader profile and the checkpoint/restore profile through the actual full fold.
    for cheap_first in [false, true] {
        let p = p.clone().with_audit_1004_from_daa(cheap_first.then_some(0));
        let extras = xx_declare();
        let fold = |objects: &[PalwConsensusObjectV2]| {
            apply_palw_transition_v7(
                &native,
                &p,
                Some(&admission),
                &context,
                objects,
                PalwBlockWorkV3::None,
                &[],
                Hash64::default(),
                false,
                false,
                false,
                false,
                &extras,
            )
            .expect("a refused rider does not invalidate its carrier block")
        };
        let root = open_object(decl(lead));
        let rider_object = PalwConsensusObjectV2::AttemptRidersV1 { lead, riders: riders.to_vec() };
        let (control, _, control_skips) = fold(&[root.clone()]);
        assert!(control_skips.is_empty(), "{control_skips:?}");
        for objects in [[root.clone(), rider_object.clone()], [rider_object.clone(), root.clone()]] {
            let (state, delta, skips) = fold(&objects);
            assert_eq!(skips.len(), 1, "{skips:?}");
            assert!(skips[0].1.contains("EXEC root"), "{skips:?}");
            assert_eq!(state.state_root(), control.state_root(), "the refused rider changes no root, payout or capacity right");
            state.assert_internal_consistency(&p).unwrap();
            state.assert_deadline_consistency(&p).unwrap();
            assert_eq!(apply_delta_v2(&native, &delta, &p).unwrap().state_root(), state.state_root());
            assert_eq!(revert_delta_v2(&state, &delta, &p).unwrap().state_root(), native.state_root());
            let carriage = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
            let restored =
                borsh::from_slice::<PalwStateCarriageV2>(&carriage).unwrap().into_state(&p, Some(state.state_root())).unwrap();
            assert_eq!(restored.state_root(), state.state_root());
        }
    }
}

#[test]
fn rfc0012_an_open_computation_court_excludes_work_and_holds_native_settlement() {
    use crate::palw_native_settlement_v1::*;
    let (p, mut s, claim) = world(None);
    let c = s.claims.get_mut(&claim).unwrap();
    c.phase = PalwClaimPhaseV2::Final { final_daa: 110 };
    c.trace_retention_daa = 120;
    c.work_id = Some(h64(0xBEEF));
    let mut recorded = c.clone();
    recorded.class_id = h64(0xABC); // non-floor evidence
    let evidence = NativeDeltaEvidenceV1 { finalized_attempts: vec![(claim, recorded)], ..Default::default() };
    let check = |state: &PalwChainStateV2| {
        let sessions = native_open_sessions_v1(state);
        let open = sessions.iter().map(|(claim, _)| *claim).collect();
        let rules = NativeFactRulesV1 {
            state,
            params: &p,
            canonical_work_daa: Some(0),
            quantum_maturity_daa: 0,
            claims_with_open_disputes: &open,
        };
        let facts = native_facts_of_block_v1(&rules, &evidence, &BTreeSet::new(), (101, 2));
        let from = native_open_from_v1(
            state.claims_iter().map(|(_, c)| (c.accepted_blue_score, c.phase.clone(), c.trace_retention_daa)),
            sessions.iter().map(|(id, _)| state.claim(id).map(|c| c.accepted_blue_score)),
            10_000,
        );
        (facts.len(), from)
    };
    assert_eq!(check(&s), (1, u64::MAX), "control: retention has lapsed, no dispute");
    let ladder = crate::palw_bisect::PalwBisectLadderV1::open(
        &h64(1),
        &h64(2),
        &h64(3),
        &h64(4),
        crate::palw_bisect::PalwBisectSpaceV1::TraceEvents,
        2,
        121,
        20_000,
    )
    .unwrap();
    s.court_sessions.insert(
        h64(0xC0),
        PalwCourtSessionStateV2 {
            claim,
            challenger_bond: bond_key(20),
            opened_daa: 121,
            deadline_daa: 20_000,
            ladder,
            dissection: None,
        },
    );
    assert_eq!(native_open_sessions_v1(&s), vec![(claim, 20_000)]);
    assert_eq!(check(&s), (0, s.claim(&claim).unwrap().accepted_blue_score));
    s.court_sessions.clear();
    assert_eq!(check(&s), (1, u64::MAX), "clearing the court restores the evidence");
}

fn decl(claim: Hash64) -> PalwWorkRootDeclarationV2 {
    PalwWorkRootDeclarationV2 {
        root_claim_id: claim,
        canonical_job_id: h64(JOB),
        input_root: h64(0x31),
        kernel_version: 1,
        plan_root: h64(0x32),
        total_work: 800,
        boundaries: vec![PWU, 400, 640, 800],
        initial_state_root: h64(0x33),
        prefix_claim: h64(0x3F),
        evidence_policy_root: h64(0x34),
        extra_executors: vec![bond_key(20), bond_key(21)],
        expiry_daa: 5_000,
        signature: vec![0; 8],
    }
}

fn open_object(d: PalwWorkRootDeclarationV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ExecWorkRootOpenedV2 { declaration: Box::new(d) }
}

fn open(p: &PalwStateParamsV2, s: &PalwChainStateV2, d: PalwWorkRootDeclarationV2) -> Result<PalwChainStateV2, PalwStateV2Error> {
    step_with(s, p, 40, 110, &[open_object(d)], &xx_declare()).map(|(state, _)| state)
}

fn opened() -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    let (p, s, claim) = world(Some(0));
    let s = open(&p, &s, decl(claim)).expect("the session opens");
    (p, s, claim)
}

fn held_exec_to(s: &PalwChainStateV2, claim: Hash64, bond: PalwBondKeyV2) -> u64 {
    s.exec_v2_reward_vesting_v1(&claim)
        .map(|row| row.legs.iter().filter(|((who, _), _)| *who == bond).map(|(_, leg)| leg.amount).sum())
        .unwrap_or(0)
}

#[test]
fn budget_and_exec_ledgers_coexist_in_the_state_root_delta_and_restart_carriage() {
    use crate::palw_bond_budget_v1::{PalwBondBudgetMirrorV1, PalwBondBudgetPolicyV1};
    let (p, s, claim) = opened();
    // A TEST policy and fence: boot the budget beside a pre-existing EXEC root.
    let p = p.with_bond_budget(Some(PalwBondBudgetMirrorV1 {
        from_daa: 111,
        policy: PalwBondBudgetPolicyV1::unapproved_probe_v1(),
        allocation: None,
    }));
    let (after, delta) = step(&s, &p, 41, 111, &[]).expect("both ledgers fold, revert and restart together");
    assert!(after.bond_budget().is_some());
    assert_eq!(after.exec_v2_root_v1(&claim), s.exec_v2_root_v1(&claim));
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::BondBudgetHeader { old: None, new: Some(_) })));
    let preimage = after.state_root_preimage();
    assert!(preimage.windows(b"bond_budget/v1".len()).any(|w| w == b"bond_budget/v1"));
    assert!(preimage.windows(b"exec_v2/v1".len()).any(|w| w == b"exec_v2/v1"));
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&borsh::to_vec(&PalwStateCarriageV2::from_state(&after)).unwrap()).unwrap();
    assert_eq!(carriage.bond_budget.as_ref(), after.bond_budget());
    assert_eq!(&carriage.exec_v2, after.exec_v2_state_v1());
}

/// The slice a root plans at `index`, executed by `executor`, chained from the root's current boundary.
fn slice_of(s: &PalwChainStateV2, claim: Hash64, index: u32, executor: PalwBondKeyV2) -> PalwExecV2CoveredSliceV1 {
    let root = s.exec_v2_root_v1(&claim).expect("the root");
    let range = palw_work_plan_range_v2(&root.boundaries, index).expect("a planned slice");
    let predecessor = if index == 0 {
        root.initial_state_root
    } else {
        s.exec_v2_slice_v1(&claim, index - 1).map(|row| row.result_state_root).unwrap_or(h64(0xDEAD))
    };
    PalwExecV2CoveredSliceV1 {
        carrier: h64(0x5000 + index as u64),
        pubkey: s.bond(&executor).expect("a registered executor").pubkey.clone(),
        slice: PalwWorkSliceV1 {
            root_claim_id: claim,
            slice_index: index,
            class_id: root.class_id,
            canonical_job_id: root.canonical_job_id,
            kernel_version: root.kernel_version,
            plan_root: root.plan_root,
            canonical_range: range,
            predecessor_state_root: predecessor,
            result_state_root: h64(0x6000 + index as u64),
            input_root: h64(0x31),
            output_root: h64(0x7000 + index as u64),
            evidence_root: h64(0x8000 + index as u64),
            da_root: h64(0x9000 + index as u64),
            executor_bond: executor,
        },
    }
}

fn fold_slices(
    p: &PalwStateParamsV2,
    s: &PalwChainStateV2,
    word: u64,
    daa: u64,
    covered: Vec<PalwExecV2CoveredSliceV1>,
) -> PalwChainStateV2 {
    step_with(s, p, word, daa, &[], &xx_slices(covered)).expect("the block folds").0
}

/// Mark slices verified at `daa` through the test-only door (one journaled block).
fn verify(p: &PalwStateParamsV2, s: &PalwChainStateV2, word: u64, daa: u64, which: &[(Hash64, u32)]) -> PalwChainStateV2 {
    let extras = PalwTransitionExtrasV1 { exec_v2_test_verified: which.to_vec(), ..xx() };
    step_with(s, p, word, daa, &[], &extras).expect("the verification block folds").0
}

fn verdicts_of(
    p: &PalwStateParamsV2,
    s: &PalwChainStateV2,
    daa: u64,
    covered: &[PalwExecV2CoveredSliceV1],
) -> Vec<Result<(), PalwSliceRefusalV2>> {
    // The same sequential judgement the fold makes: each carrier against the state the earlier ones left.
    let mut state = s.clone();
    let mut out = Vec::new();
    for (n, carrier) in covered.iter().enumerate() {
        match state.exec_v2_admit_slice_v1(p, daa, carrier, 0) {
            Ok(_) => {
                out.push(Ok(()));
                state = fold_slices(p, &state, 1_000 + n as u64, daa, vec![carrier.clone()]);
            }
            Err(e) => out.push(Err(e)),
        }
    }
    out
}

fn refusal(p: &PalwStateParamsV2, s: &PalwChainStateV2, covered: PalwExecV2CoveredSliceV1) -> PalwSliceRefusalV2 {
    s.exec_v2_admit_slice_v1(p, 120, &covered, 0).expect_err("refused")
}

// =============================================================================================
// The root declaration
// =============================================================================================

#[test]
fn a_root_opens_on_an_accepted_claim_and_earns_nothing() {
    let (p, s0, claim) = world(Some(0));
    let weight = (s0.safe_weight, s0.bounded_immature);
    let payouts = s0.pending_payouts_iter().count();
    let exposure_20 = s0.reserved_exposure(&bond_key(20));
    let s1 = open(&p, &s0, decl(claim)).expect("the session opens");
    let root = s1.exec_v2_root_v1(&claim).expect("a root");
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Open);
    assert_eq!(root.prefix_work(), PWU, "the prefix is the claim's admitted canonical work");
    assert_eq!(root.slice_count(), 3);
    assert_eq!(root.root_bond, bond_key(1));
    assert_eq!(root.class_id, s0.claim(&claim).unwrap().class_id);
    assert_eq!(root.last_state_root, h64(0x33));
    assert_eq!((root.next_index, root.accepted_work, root.verified_work, root.pending), (0, 0, 0, 0));
    assert_eq!(s1.exec_v2_job_holder_v1(&root.job_work_id), Some(&claim));
    // A declaration earns no credit, no weight, no payout, no clock term.
    assert_eq!((s1.safe_weight, s1.bounded_immature), weight, "no weight");
    assert_eq!(s1.pending_payouts_iter().count(), payouts, "no payout");
    assert_eq!(s1.claim(&claim).unwrap().phase, s0.claim(&claim).unwrap().phase, "the claim is untouched");
    // Listing a worker cannot reserve its collateral; its first signed slice is the consent.
    let reserved = s0.claim(&claim).unwrap().reserved;
    assert!(reserved > 0);
    assert_eq!(root.executor_exposure, reserved);
    assert_eq!(s1.reserved_exposure(&bond_key(20)), exposure_20);
    assert_eq!(s1.reserved_exposure(&bond_key(21)), s0.reserved_exposure(&bond_key(21)));
}

#[test]
fn a_listed_executor_reserves_nothing_until_its_first_signed_slice_and_is_reserved_once() {
    let (p, mut initial, claim) = world(Some(0));
    let collateral = initial.bonds[&bond_key(20)].collateral;
    initial.bonds.get_mut(&bond_key(20)).unwrap().collateral = 1;
    let s = open(&p, &initial, decl(claim)).expect("an allow-list cannot bind another worker's collateral");
    let exposure = s.exec_v2_root_v1(&claim).unwrap().executor_exposure;
    assert!(exposure > 1);
    assert_eq!(s.reserved_exposure(&bond_key(20)), 0);
    assert_eq!(s.reserved_exposure(&bond_key(21)), 0);
    let slice = slice_of(&s, claim, 0, bond_key(20));
    assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &slice, 0), Err(PalwSliceRefusalV2::ExecutorUnfunded));
    let (refused, delta) = step_with(&s, &p, 50, 120, &[], &xx_slices(vec![slice.clone()])).unwrap();
    assert!(delta.entries.iter().any(|entry| matches!(entry, PalwDeltaEntryV2::ExecV2Verdict { code: 32, .. })));
    assert_eq!(refused.exec_v2_root_v1(&claim).unwrap().next_index, 0);
    assert_eq!(refused.reserved_exposure(&bond_key(20)), 0);
    assert!(refused.pending_payouts_iter().next().is_none());
    let mut funded = refused;
    funded.bonds.get_mut(&bond_key(20)).unwrap().collateral = collateral;
    let first = fold_slices(&p, &funded, 51, 121, vec![slice]);
    assert_eq!(first.reserved_exposure(&bond_key(20)), exposure);
    assert_eq!(first.reserved_exposure(&bond_key(21)), 0, "unused workers hold nothing");
    let second = fold_slices(&p, &first, 52, 122, vec![slice_of(&first, claim, 1, bond_key(21))]);
    let third = fold_slices(&p, &second, 53, 123, vec![slice_of(&second, claim, 2, bond_key(20))]);
    assert_eq!(third.reserved_exposure(&bond_key(20)), exposure, "a second signed slice does not reserve twice");
    assert_eq!(third.reserved_exposure(&bond_key(21)), exposure);
    let (expired, _) = step(&third, &p, 60, third.exec_v2_root_v1(&claim).unwrap().expiry_daa, &[]).unwrap();
    assert_eq!(expired.reserved_exposure(&bond_key(20)), 0);
    assert_eq!(expired.reserved_exposure(&bond_key(21)), 0);
    assert!(expired.pending_payouts_iter().next().is_none());
}

#[test]
fn every_refused_declaration_is_named_and_writes_nothing() {
    let (p, s0, claim) = world(Some(0));
    let root0 = s0.state_root();
    let refuse = |d: PalwWorkRootDeclarationV2| -> String {
        let err = step_with(&s0, &p, 40, 110, &[open_object(d)], &xx_declare()).expect_err("refused");
        assert!(matches!(err, PalwStateV2Error::ExecV2Refused(_)), "{err:?}");
        err.to_string()
    };
    // No claim; the prefix is not the claim's admitted work; a wrong job; a plan that is not a partition.
    let mut d = decl(claim);
    d.root_claim_id = h64(0xBAD);
    assert!(refuse(d).contains("unknown"));
    let mut d = decl(claim);
    d.boundaries[0] = PWU + 1;
    assert!(refuse(d).contains("not the root claim's admitted canonical work"));
    let mut d = decl(claim);
    d.canonical_job_id = h64(JOB + 1);
    assert!(refuse(d).contains("not the claim's"));
    let mut d = decl(claim);
    d.boundaries = vec![PWU, 300, 300, 800];
    assert!(refuse(d).contains("not a partition"));
    let mut d = decl(claim);
    d.total_work = 799;
    assert!(refuse(d).contains("not a partition"));
    // Expiry: not in the past, not beyond the lifetime bound.
    let mut d = decl(claim);
    d.expiry_daa = 110;
    assert!(refuse(d).contains("expiry"));
    let mut d = decl(claim);
    d.expiry_daa = 110 + PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA + 1;
    assert!(refuse(d).contains("expiry"));
    // An executor that is not registered cannot stand behind the root; the root bond is not an extra.
    let mut d = decl(claim);
    d.extra_executors = vec![bond_key(20), bond_key(77)];
    assert!(refuse(d).contains("unknown, inactive or cannot fund"));
    let mut d = decl(claim);
    d.extra_executors = vec![bond_key(1)];
    assert!(refuse(d).contains("strictly ascending"));
    // Nothing was written by any refusal: the parent state is the parent state.
    assert_eq!(s0.state_root(), root0);
    // A declaration on a claim that already left `Provisional`/`PanelBound` is refused.
    let (pl, sl, claim_l) = {
        let (p, s, claim) = world(Some(0));
        let seats: Vec<PalwBondKeyV2> = sybil_seats().iter().map(|seat| seat.bond).collect();
        let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(h64(77), claim, 5);
        let receipts: Vec<_> = seats
            .iter()
            .enumerate()
            .map(|(i, seat)| crate::palw_panel_v2::PalwSeatReceiptV3 {
                receipt: valid_receipt(claim, *seat, 104),
                segments: assignment.mask_of(i as u16),
            })
            .collect();
        let (licensed, _) = step(&s, &p, 30, 105, &[PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts }]).expect("licensed");
        assert!(matches!(licensed.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        (p, licensed, claim)
    };
    let err = step_with(&sl, &pl, 40, 110, &[open_object(decl(claim_l))], &xx_declare()).expect_err("licensed claims open no session");
    assert!(err.to_string().contains("not a live REAL attempt in a phase that may open a session"), "{err}");
}

#[test]
fn a_session_opens_once_and_a_job_is_worked_once() {
    let (p, s0, claim) = world(Some(0));
    let s1 = open(&p, &s0, decl(claim)).unwrap();
    let again = step_with(&s1, &p, 41, 111, &[open_object(decl(claim))], &xx_declare()).expect_err("a second session on one claim");
    assert!(again.to_string().contains("already has a session"), "{again}");
    // A second claim whose declaration would carry the same job work identity is refused: copying a job into a new root, with
    // another claim and other executors, cannot earn the same work twice.
    let env = attempt(PWU, 2);
    let claim2 = attempt_id_v2(&env.attempt);
    let (s2, _) = apply_door(&s1, &p, &ctx(41, 111, 41), &[], Some(&env), &xx()).expect("a second claim lands");
    let mut s2 = s2;
    // The second claim runs the SAME job (the surgery stands for a copied job identity).
    s2.claims.get_mut(&claim2).expect("claim 2").job_identity = h64(JOB);
    let mut copy = decl(claim2);
    copy.extra_executors = vec![bond_key(21)];
    let refused = step_with(&s2, &p, 42, 112, &[open_object(copy)], &xx_declare()).expect_err("the job's work was already used");
    assert!(refused.to_string().contains("already used by another root"), "{refused}");
    // The identity differs only if the job does: a claim with its own job identity is its own work.
    s2.claims.get_mut(&claim2).unwrap().job_identity = h64(JOB + 1);
    let mut own = decl(claim2);
    own.canonical_job_id = h64(JOB + 1);
    own.extra_executors = vec![bond_key(21)];
    step_with(&s2, &p, 42, 112, &[open_object(own)], &xx_declare()).expect("a different job is different work");
}

#[test]
fn below_the_fence_the_declaration_is_the_folds_refusal_by_name_and_the_slices_are_ignored() {
    let (p, s, claim) = world(None);
    let root_before = s.state_root();
    let err = step_with(&s, &p, 40, 110, &[open_object(decl(claim))], &xx_declare()).expect_err("dormant");
    assert!(err.to_string().contains("not in force"), "{err}");
    assert!(palw_object_is_exec_v2(&open_object(decl(claim))));
    // A covered slice below the fence writes nothing and moves no root.
    let phantom = PalwExecV2CoveredSliceV1 {
        carrier: h64(1),
        pubkey: vec![1],
        slice: PalwWorkSliceV1 {
            root_claim_id: claim,
            slice_index: 0,
            class_id: h64(1),
            canonical_job_id: h64(JOB),
            kernel_version: 1,
            plan_root: h64(2),
            canonical_range: PalwWorkRangeV1 { start: 1, end: 2 },
            predecessor_state_root: h64(3),
            result_state_root: h64(4),
            input_root: h64(5),
            output_root: h64(6),
            evidence_root: h64(7),
            da_root: h64(8),
            executor_bond: bond_key(1),
        },
    };
    let (after, delta) = step_with(&s, &p, 41, 111, &[], &xx_slices(vec![phantom.clone()])).unwrap();
    assert_eq!(after.exec_v2_counts_v1(), (0, 0, 0));
    assert!(delta.entries.iter().all(|entry| !matches!(entry, PalwDeltaEntryV2::ExecV2Row { .. })));
    assert_eq!(phantom_verdict(&p, &s), PalwSliceRefusalV2::Dormant);
    // The state root of a chain that never saw the lane is the one it had: an empty table is neither rooted nor carried.
    let (plain, _) = step(&s, &p, 41, 111, &[]).unwrap();
    assert_eq!(after.state_root(), plain.state_root());
    assert_ne!(root_before, plain.state_root(), "(control: a block does move the root)");
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&after)).unwrap();
    assert!(!carriage_has_tail(&bytes, 0xEE), "no exec v2 tail is written when the table is empty");
}

fn phantom_verdict(p: &PalwStateParamsV2, s: &PalwChainStateV2) -> PalwSliceRefusalV2 {
    let carrier = PalwExecV2CoveredSliceV1 {
        carrier: h64(1),
        pubkey: vec![1],
        slice: PalwWorkSliceV1 {
            root_claim_id: h64(1),
            slice_index: 0,
            class_id: h64(1),
            canonical_job_id: h64(1),
            kernel_version: 1,
            plan_root: h64(2),
            canonical_range: PalwWorkRangeV1 { start: 1, end: 2 },
            predecessor_state_root: h64(3),
            result_state_root: h64(4),
            input_root: h64(5),
            output_root: h64(6),
            evidence_root: h64(7),
            da_root: h64(8),
            executor_bond: bond_key(1),
        },
    };
    s.exec_v2_admit_slice_v1(p, 5_000, &carrier, 0).expect_err("dormant")
}

/// Does the carriage's byte stream contain the tail tag `tag` at a tail position? (A coarse check: the tag byte followed by a
/// well-formed table is how it is written; absence of the byte as a tail is what the empty-table test needs, so the check scans the
/// decoded structure instead of the raw bytes.)
fn carriage_has_tail(bytes: &[u8], tag: u8) -> bool {
    let carriage = borsh::from_slice::<PalwStateCarriageV2>(bytes).expect("a carriage");
    match tag {
        0xEE => !carriage.exec_v2.is_empty(),
        _ => unreachable!("only the exec v2 tail is asked"),
    }
}

// =============================================================================================
// Slice admission: the six rules, in the spec's order
// =============================================================================================

#[test]
fn an_honest_session_accepts_its_slices_in_order_and_credits_each_range_once() {
    let (p, s0, claim) = opened();
    let a = bond_key(20);
    let b = bond_key(21);
    let s1 = fold_slices(&p, &s0, 50, 120, vec![slice_of(&s0, claim, 0, a)]);
    let root = s1.exec_v2_root_v1(&claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending, root.verified_work), (1, 240, 1, 0));
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Open);
    assert_eq!(root.last_state_root, h64(0x6000), "the boundary moved to the slice's result");
    let row = s1.exec_v2_slice_v1(&claim, 0).expect("the row");
    assert_eq!(
        (row.range, row.executor, row.stage, row.carrier),
        (PalwWorkRangeV1 { start: PWU, end: 400 }, a, PalwWorkSliceStageV2::Pending, h64(0x5000))
    );
    // The next slice chains from the previous one, by another authorised executor.
    let s2 = fold_slices(&p, &s1, 51, 121, vec![slice_of(&s1, claim, 1, b)]);
    // The last slice completes the root (accepted, not verified, not Final).
    let s3 = fold_slices(&p, &s2, 52, 122, vec![slice_of(&s2, claim, 2, bond_key(1))]);
    let root = s3.exec_v2_root_v1(&claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending, root.verified_work), (3, 640, 3, 0));
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Complete);
    assert_eq!(root.prefix_work() + root.accepted_work, root.total_work, "prefix + slices == the plan");
    assert!(!root.ready_for_final(), "complete is not Final: nothing is verified");
    // No carrier subsidy, no weight, no payout, no permit: the lane's accepting a slice moves none of them.
    assert_eq!((s3.safe_weight, s3.bounded_immature), (s0.safe_weight, s0.bounded_immature));
    assert_eq!(s3.pending_payouts_iter().count(), s0.pending_payouts_iter().count());
    assert_eq!(s3.round_permits_used.len(), s0.round_permits_used.len(), "a slice spends no round permit");
}

#[test]
fn two_slices_in_one_block_see_each_other_in_order_and_a_wrong_order_is_the_skip_rule() {
    let (p, s0, claim) = opened();
    let a = bond_key(20);
    // Slice 1 cannot be built from state before slice 0: build both from the plan with the predecessor chain written by hand.
    let first = slice_of(&s0, claim, 0, a);
    let mut second = slice_of(&s0, claim, 1, bond_key(21));
    second.slice.predecessor_state_root = first.slice.result_state_root;
    let both = fold_slices(&p, &s0, 50, 120, vec![first.clone(), second.clone()]);
    assert_eq!(both.exec_v2_root_v1(&claim).unwrap().next_index, 2, "the second sees the first, in the same block");
    // Reverse order: slice 1 first skips ahead; slice 0 then lands; slice 1 is not retried (a verdict, not an error).
    let reversed = fold_slices(&p, &s0, 50, 120, vec![second.clone(), first.clone()]);
    assert_eq!(reversed.exec_v2_root_v1(&claim).unwrap().next_index, 1, "only the in-order slice lands");
    assert_eq!(verdicts_of(&p, &s0, 120, &[second.clone(), first.clone()])[0], Err(PalwSliceRefusalV2::SkippedSlice));
    // The same carrier twice in one block: the second is a used index.
    let twice = fold_slices(&p, &s0, 50, 120, vec![first.clone(), first.clone()]);
    assert_eq!(twice.exec_v2_root_v1(&claim).unwrap().next_index, 1, "credited once");
    assert_eq!(twice.exec_v2_root_v1(&claim).unwrap().accepted_work, 240);
}

#[test]
fn each_admission_rule_refuses_by_name_in_the_specs_order_and_a_refusal_writes_nothing() {
    use PalwSliceRefusalV2 as R;
    let (p, s, claim) = opened();
    let a = bond_key(20);
    let good = slice_of(&s, claim, 0, a);
    let admit = |c: &PalwExecV2CoveredSliceV1| s.exec_v2_admit_slice_v1(&p, 120, c, 0);
    assert_eq!(admit(&good), Ok(PalwSliceAdmissionV2 { work: 240, verified_at: None, leg_cap: 0, route_reward: 0 }));
    let mutated = |f: &dyn Fn(&mut PalwExecV2CoveredSliceV1)| {
        let mut c = good.clone();
        f(&mut c);
        admit(&c)
    };
    // ---- 1: the root ----
    assert_eq!(mutated(&|c| c.slice.root_claim_id = h64(0xBAD)), Err(R::NoRoot));
    assert_eq!(s.exec_v2_admit_slice_v1(&p, 5_000, &good, 0), Err(R::RootExpired), "at and past the expiry");
    assert_eq!(
        s.exec_v2_admit_slice_v1(&p, 4_999, &good, 0),
        Ok(PalwSliceAdmissionV2 { work: 240, verified_at: None, leg_cap: 0, route_reward: 0 }),
        "the last DAA before it"
    );
    // ---- 2: the bond ----
    assert_eq!(
        mutated(&|c| c.slice.executor_bond = bond_key(2)),
        Err(R::ExecutorNotAuthorized),
        "a registered bond the root did not name"
    );
    assert_eq!(mutated(&|c| c.pubkey = vec![0xFF; 4]), Err(R::ExecutorKeyMismatch));
    let mut retiring = s.clone();
    retiring.bonds.get_mut(&a).unwrap().status = PalwBondStatusV2::Retiring { since_daa: 100, settled_at_since: 0 };
    assert_eq!(retiring.exec_v2_admit_slice_v1(&p, 120, &good, 0), Err(R::ExecutorNotActive));
    // ---- 3: the index and the range ----
    assert_eq!(mutated(&|c| c.slice.slice_index = 3), Err(R::IndexPastPlan));
    assert_eq!(mutated(&|c| c.slice.slice_index = 2), Err(R::SkippedSlice));
    assert_eq!(mutated(&|c| c.slice.canonical_range.end = 401), Err(R::RangeNotPlan));
    assert_eq!(
        mutated(&|c| c.slice.canonical_range = PalwWorkRangeV1 { start: 0, end: 240 }),
        Err(R::RangeOverlap),
        "overlaps the root prefix"
    );
    // ---- 4: the predecessor ----
    assert_eq!(mutated(&|c| c.slice.predecessor_state_root = h64(0xBAD)), Err(R::PredecessorMismatch));
    // ---- 5: the bindings ----
    assert_eq!(mutated(&|c| c.slice.class_id = h64(0xBAD)), Err(R::ClassMismatch));
    assert_eq!(mutated(&|c| c.slice.canonical_job_id = h64(0xBAD)), Err(R::JobMismatch));
    assert_eq!(mutated(&|c| c.slice.kernel_version = 9), Err(R::KernelMismatch));
    assert_eq!(mutated(&|c| c.slice.plan_root = h64(0xBAD)), Err(R::PlanMismatch));
    // ---- the order: a slice breaking rules 3 and 5 is named for rule 3 ----
    assert_eq!(
        mutated(&|c| {
            c.slice.slice_index = 2;
            c.slice.class_id = h64(0xBAD);
        }),
        Err(R::SkippedSlice)
    );
    // ---- 6: the limits ----
    assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &good, PALW_EXEC_V2_MAX_SLICES_PER_BLOCK), Err(R::BlockQuota));
    // A refusal wrote nothing: the fold of a refused carrier is the fold of no carrier.
    let mut bad = good.clone();
    bad.slice.predecessor_state_root = h64(0xBAD);
    let (after, delta) = step_with(&s, &p, 50, 120, &[], &xx_slices(vec![bad])).unwrap();
    assert!(delta.entries.iter().all(|e| !matches!(e, PalwDeltaEntryV2::ExecV2Row { .. })), "no row was written");
    assert_eq!(after.exec_v2_state_v1(), s.exec_v2_state_v1());
}

#[test]
fn a_used_index_a_skip_an_overlap_and_a_replay_credit_nothing() {
    use PalwSliceRefusalV2 as R;
    let (p, s0, claim) = opened();
    let a = bond_key(20);
    let first = slice_of(&s0, claim, 0, a);
    let s1 = fold_slices(&p, &s0, 50, 120, vec![first.clone()]);
    // The same slice again, from a different carrier block: the (root, index) is used.
    let mut replay = first.clone();
    replay.carrier = h64(0xCAFE);
    assert_eq!(refusal(&p, &s1, replay), R::IndexUsed);
    // The same index with different roots (an equivocating executor): the first accepted stands.
    let mut equivocation = first.clone();
    equivocation.slice.result_state_root = h64(0xFA15E);
    equivocation.carrier = h64(0xCAFF);
    assert_eq!(refusal(&p, &s1, equivocation), R::IndexUsed);
    assert_eq!(s1.exec_v2_slice_v1(&claim, 0).unwrap().result_state_root, h64(0x6000));
    // A range that overlaps the accepted slice (shifted back into it) is an overlap, not a plan range.
    let mut overlap = slice_of(&s1, claim, 1, a);
    overlap.slice.canonical_range = PalwWorkRangeV1 { start: 300, end: 640 };
    assert_eq!(refusal(&p, &s1, overlap), R::RangeOverlap);
    // The sum never exceeds the plan: after every slice, accepted + prefix == total and nothing more can land.
    let s2 = fold_slices(&p, &s1, 51, 121, vec![slice_of(&s1, claim, 1, a)]);
    let s3 = fold_slices(&p, &s2, 52, 122, vec![slice_of(&s2, claim, 2, a)]);
    let root = s3.exec_v2_root_v1(&claim).unwrap();
    assert_eq!(root.prefix_work() + root.accepted_work, root.total_work);
    let mut extra = slice_of(&s3, claim, 2, a);
    extra.slice.slice_index = 3;
    assert!(matches!(refusal(&p, &s3, extra), R::RootNotOpen), "a complete root accepts nothing more");
    let again = slice_of(&s3, claim, 2, a);
    assert_eq!(refusal(&p, &s3, again), R::RootNotOpen);
}

#[test]
fn a_correct_slice_borrowed_from_another_root_or_job_is_refused_by_binding() {
    use PalwSliceRefusalV2 as R;
    // Two roots: the borrowed slice is correct for root B and presented to root A.
    let (p, s0, claim_a) = opened();
    let env = attempt(PWU, 3);
    let claim_b = attempt_id_v2(&env.attempt);
    let (s1, _) = apply_door(&s0, &p, &ctx(41, 111, 41), &[], Some(&env), &xx()).unwrap();
    let mut s1 = s1;
    s1.claims.get_mut(&claim_b).unwrap().job_identity = h64(JOB + 7);
    let mut d = decl(claim_b);
    d.canonical_job_id = h64(JOB + 7);
    d.plan_root = h64(0x42);
    d.input_root = h64(0x43);
    d.initial_state_root = h64(0x44);
    d.extra_executors = vec![bond_key(21)];
    let s2 = step_with(&s1, &p, 42, 112, &[open_object(d)], &xx_declare()).unwrap().0;
    let for_b = slice_of(&s2, claim_b, 0, bond_key(1));
    // Valid for B...
    assert!(s2.exec_v2_admit_slice_v1(&p, 120, &for_b, 0).is_ok());
    // ...but re-addressed to root A (whose root bond is also bond 1, so the bond rule passes), every binding is A's to refuse, one rule at
    // a time and in the spec's order: the boundary state, then the job, then the verification plan. (A header-stage signature over the
    // slice identity would already have failed on any of these edits; the fold's checks are the second lock.)
    let mut borrowed = for_b.clone();
    borrowed.slice.root_claim_id = claim_a;
    assert_eq!(refusal(&p, &s2, borrowed.clone()), R::PredecessorMismatch, "B's boundary is not A's");
    borrowed.slice.predecessor_state_root = h64(0x33);
    assert_eq!(refusal(&p, &s2, borrowed.clone()), R::JobMismatch, "B's job is not A's");
    borrowed.slice.canonical_job_id = h64(JOB);
    assert_eq!(refusal(&p, &s2, borrowed.clone()), R::PlanMismatch, "B's verification plan is not A's");
    borrowed.slice.plan_root = h64(0x32);
    assert!(s2.exec_v2_admit_slice_v1(&p, 120, &borrowed, 0).is_ok(), "only a slice carrying EVERY root field of A is A's slice");
    // The slice's own job/plan fields must be the root's, whatever the predecessor: a slice that copies A's boundary but B's job.
    let mut mixed = slice_of(&s2, claim_a, 0, bond_key(20));
    mixed.slice.canonical_job_id = h64(JOB + 7);
    assert_eq!(refusal(&p, &s2, mixed), R::JobMismatch);
    let mut mixed = slice_of(&s2, claim_a, 0, bond_key(20));
    mixed.slice.plan_root = h64(0x42);
    assert_eq!(refusal(&p, &s2, mixed), R::PlanMismatch);
}

#[test]
fn the_pending_depth_and_the_per_bond_quota_bound_what_is_accepted_ahead_of_verification() {
    use PalwSliceRefusalV2 as R;
    // A root with more slices than the pending depth: slices accept ahead of verification up to the depth, then wait.
    let (p, s0, claim) = world(Some(0));
    let mut d = decl(claim);
    d.total_work = PWU + 8 * 100;
    d.boundaries = std::iter::once(PWU).chain((1..=8).map(|i| PWU + i * 100)).collect();
    let mut s = step_with(&s0, &p, 40, 110, &[open_object(d)], &xx_declare()).unwrap().0;
    for i in 0..PALW_EXEC_V2_MAX_PENDING_DEPTH {
        s = fold_slices(&p, &s, 50 + i as u64, 120 + i as u64, vec![slice_of(&s, claim, i, bond_key(20))]);
    }
    assert_eq!(s.exec_v2_root_v1(&claim).unwrap().pending, PALW_EXEC_V2_MAX_PENDING_DEPTH);
    let next = slice_of(&s, claim, PALW_EXEC_V2_MAX_PENDING_DEPTH, bond_key(20));
    assert_eq!(s.exec_v2_admit_slice_v1(&p, 130, &next, 0), Err(R::RootPendingDepth), "pending does not mean verified");
    // Verifying the oldest frees a place (the test-only door).
    let s = verify(&p, &s, 60, 131, &[(claim, 0)]);
    assert_eq!(s.exec_v2_root_v1(&claim).unwrap().pending, PALW_EXEC_V2_MAX_PENDING_DEPTH - 1);
    assert!(s.exec_v2_admit_slice_v1(&p, 132, &next, 0).is_ok(), "the next slice may follow the verification of an earlier one");
}

// =============================================================================================
// The `Final` hold, the one settlement, the void and the retirement
// =============================================================================================

/// License the claim by the receipt path (the five sybil seats) at DAA `daa`.
fn license(p: &PalwStateParamsV2, s: &PalwChainStateV2, claim: Hash64, word: u64, daa: u64) -> PalwChainStateV2 {
    let seats: Vec<PalwBondKeyV2> = sybil_seats().iter().map(|seat| seat.bond).collect();
    let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(h64(77), claim, 5);
    let receipts: Vec<_> = seats
        .iter()
        .enumerate()
        .map(|(i, seat)| crate::palw_panel_v2::PalwSeatReceiptV3 {
            receipt: valid_receipt(claim, *seat, 104),
            segments: assignment.mask_of(i as u16),
        })
        .collect();
    let licensed =
        step(s, p, word, daa, &[PalwConsensusObjectV2::ReceiptLicensedV2 { claim, receipts }]).expect("the receipts license").0;
    assert!(matches!(licensed.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    licensed
}

fn deadline_of(s: &PalwChainStateV2, claim: Hash64) -> Option<u64> {
    s.deadlines.iter().find(|(_, id)| *id == claim).map(|(at, _)| *at)
}

/// Advance with empty blocks until `claim` is `Final` or voided (or give up).
fn run_to_terminal(p: &PalwStateParamsV2, s: PalwChainStateV2, claim: Hash64, word: u64, daa: u64) -> PalwChainStateV2 {
    run_to_terminal_with_delta(p, s, claim, word, daa).0
}

fn run_to_terminal_with_delta(
    p: &PalwStateParamsV2,
    mut s: PalwChainStateV2,
    claim: Hash64,
    mut word: u64,
    mut daa: u64,
) -> (PalwChainStateV2, Option<PalwStateDeltaV2>) {
    let mut last = None;
    for _ in 0..400 {
        if s.claim(&claim).is_none_or(|c| c.phase.is_terminal()) {
            return (s, last);
        }
        let at = deadline_of(&s, claim).unwrap_or(daa + 1).max(daa + 1);
        daa = at;
        word += 1;
        let (next, delta) = step(&s, p, word, daa, &[]).expect("an empty block");
        s = next;
        last = Some(delta);
    }
    panic!("the claim never reached a terminal phase");
}

/// A session with all three slices accepted (by bonds 20, 21, 20) and the claim licensed; nothing verified.
fn complete_and_licensed() -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
    let (p, s, claim) = opened();
    let s = license(&p, &s, claim, 45, 111);
    let s = fold_slices(&p, &s, 50, 120, vec![slice_of(&s, claim, 0, bond_key(20))]);
    let s = fold_slices(&p, &s, 51, 121, vec![slice_of(&s, claim, 1, bond_key(21))]);
    let s = fold_slices(&p, &s, 52, 122, vec![slice_of(&s, claim, 2, bond_key(20))]);
    (p, s, claim)
}

#[test]
fn a_licensed_claim_with_an_unready_session_owes_no_final_deadline_and_a_ready_one_is_re_armed() {
    let (p, s, claim) = opened();
    let licensed = license(&p, &s, claim, 45, 111);
    assert!(deadline_of(&licensed, claim).is_none(), "the licence armed no Final deadline: the session is not ready");
    assert!(licensed.exec_v2_holds_final_v1(&claim));
    // The twin without a session is armed by the same licence.
    let (pt, st, claim_t) = world(Some(0));
    let twin = license(&pt, &st, claim_t, 45, 111);
    assert!(deadline_of(&twin, claim_t).is_some(), "(control: the same licence arms a Final deadline when there is no session)");
    // Slices accepted, then verified: the claim waits until the LAST one is verified, then the deadline is re-derived.
    let (p, s, claim) = complete_and_licensed();
    assert!(deadline_of(&s, claim).is_none(), "complete but unverified: still waiting");
    let s = verify(&p, &s, 60, 130, &[(claim, 0), (claim, 1)]);
    assert!(deadline_of(&s, claim).is_none(), "two of three verified: still waiting");
    assert!(s.exec_v2_holds_final_v1(&claim));
    let s = verify(&p, &s, 61, 131, &[(claim, 2)]);
    let root = s.exec_v2_root_v1(&claim).unwrap();
    assert!(root.ready_for_final() && root.pending == 0 && root.verified_work == root.accepted_work);
    assert!(!s.exec_v2_holds_final_v1(&claim));
    assert!(deadline_of(&s, claim).is_some_and(|at| at >= 131), "the hold released: the deadline is re-derived, never in the past");
}

#[test]
fn the_root_settles_once_at_final_and_the_allocation_is_conserved_against_a_session_free_twin() {
    // The twin: the same claim licensed and finalized with no session — the producer's whole leg.
    let (pt, st, claim_t) = world(Some(0));
    let twin = run_to_terminal(&pt, license(&pt, &st, claim_t, 45, 111), claim_t, 70, 111);
    assert!(matches!(twin.claim(&claim_t).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    let twin_leg = twin.vesting_row(&claim_t).expect("the vested row").producer.amount;
    assert!(twin_leg > 0);

    let (p, s, claim) = complete_and_licensed();
    let a = bond_key(20);
    let b = bond_key(21);
    let before_a = s.reserved_exposure(&a);
    assert!(before_a > 0, "the open reserved the claim's stake on its extra executors");
    let s = verify(&p, &s, 60, 130, &[(claim, 0), (claim, 1), (claim, 2)]);
    let s = run_to_terminal(&p, s, claim, 70, 131);
    assert!(
        matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }),
        "the claim finalized once the session was ready"
    );
    // ---- one settlement: the root is Settled, its executors' exposure is back, its rows stay until the claim retires ----
    let root = s.exec_v2_root_v1(&claim).expect("the root is retained");
    assert!(matches!(root.phase, PalwWorkRootPhaseV2::Settled { .. }));
    assert_eq!(s.reserved_exposure(&a), 0);
    assert_eq!(s.reserved_exposure(&b), 0);
    // ---- conservation: the producer's vested leg plus the slice legs is exactly the twin's producer leg ----
    let producer_leg = s.vesting_row(&claim).expect("the vested row").producer.amount;
    let paid_to = |bond: PalwBondKeyV2| held_exec_to(&s, claim, bond);
    let (leg_a, leg_b) = (paid_to(a), paid_to(b));
    assert_eq!(producer_leg + leg_a + leg_b, twin_leg, "nothing minted, nothing lost");
    // work shares of 800: a executed 240 + 160, b executed 240 (the prefix 160 is the root's)
    assert_eq!(leg_a, (twin_leg as u128 * 400 / 800) as u64);
    assert_eq!(leg_b, (twin_leg as u128 * 240 / 800) as u64);
    assert!(producer_leg >= (twin_leg as u128 * 160 / 800) as u64, "the root keeps at least its prefix share (plus the remainder)");
    // ---- N slices are not N rewards: the credit does not depend on how the work was cut ----
    assert!(leg_a + leg_b <= twin_leg - (twin_leg as u128 * 160 / 800) as u64 + 1);
}

#[test]
fn an_unfinished_session_voids_its_claim_uncharged_at_its_expiry() {
    let (p, s, claim) = opened();
    let s = license(&p, &s, claim, 45, 111);
    let s = fold_slices(&p, &s, 50, 120, vec![slice_of(&s, claim, 0, bond_key(20))]);
    let collateral: Vec<(PalwBondKeyV2, u64)> = s.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
    let (a, b) = (bond_key(20), bond_key(21));
    assert!(s.reserved_exposure(&a) > 0);
    assert_eq!(s.reserved_exposure(&b), 0, "a listed worker who signed no slice was never reserved");
    // Before the expiry nothing happens (the claim waits); at it, the claim voids.
    let waiting = step(&s, &p, 70, 4_999, &[]).unwrap().0;
    assert!(!waiting.claim(&claim).unwrap().phase.is_terminal(), "a session within its expiry holds the claim, not voids it");
    let voided = step(&s, &p, 71, 5_000, &[]).unwrap().0;
    assert!(
        matches!(
            voided.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkRootExpired, voided_daa: 5_000 }
        ),
        "{:?}",
        voided.claim(&claim).unwrap().phase
    );
    // Uncharged: no bond lost a sompi; the executors' exposure returned; no payout; the rows record the void.
    let after: Vec<(PalwBondKeyV2, u64)> = voided.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
    assert_eq!(after, collateral, "a timeout is not a conviction: nothing is slashed");
    assert_eq!(voided.reserved_exposure(&a), 0);
    assert_eq!(voided.reserved_exposure(&b), 0);
    let root = voided.exec_v2_root_v1(&claim).unwrap();
    assert!(matches!(root.phase, PalwWorkRootPhaseV2::Voided { from_index: 0, voided_daa: 5_000 }), "{:?}", root.phase);
    assert!(matches!(voided.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Voided { voided_daa: 5_000 }));
    assert!(voided.vesting_row(&claim).is_none(), "no reward for a session that did not finish");
    // The job's one-use tombstone outlives the void: the same job under a new claim is still refused.
    assert!(voided.exec_v2_job_holder_v1(&root.job_work_id).is_some());
    // A voided root accepts nothing.
    assert_eq!(
        voided.exec_v2_admit_slice_v1(&p, 5_001, &slice_of(&voided, claim, 1, bond_key(21)), 0),
        Err(PalwSliceRefusalV2::RootNotOpen)
    );
}

#[test]
fn the_rows_retire_with_their_claim_and_the_state_returns_to_the_session_free_root() {
    let (p, s, claim) = world_with(xp(Some(0)).with_claim_retirement_daa(700).unwrap());
    let s = open(&p, &s, decl(claim)).unwrap();
    let s = fold_slices(&p, &s, 50, 120, vec![slice_of(&s, claim, 0, bond_key(20))]);
    assert_eq!(s.exec_v2_counts_v1(), (1, 1, 1));
    let voided = step(&s, &p, 71, 5_000, &[]).unwrap().0;
    assert_eq!(voided.exec_v2_counts_v1(), (1, 1, 1), "voided rows are retained (the tombstone) until the claim retires");
    let retired = run_to_retirement(&p, voided, claim);
    assert!(retired.claim(&claim).is_none(), "the claim retired");
    assert_eq!(retired.exec_v2_counts_v1(), (0, 0, 0), "and its active session, slices and job index with it");
    assert!(retired.exec_v2_state_v1().is_empty());
    assert_eq!(retired.exec_v2_job_use_count_v1(), 1, "authorization history survives the retired claim and its active indexes");
}

#[test]
fn exec_v2_root_authorization_survives_header_loss_and_ignores_late_clock_inputs() {
    let (p, s, claim) = world(Some(0));
    let origin = s.exec_v2_origin_v1(&claim).unwrap();
    assert_eq!(origin.issued_daa, 100);
    assert_eq!(origin.source_block, s.claim(&claim).unwrap().accepted_block);
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    let restarted = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(s.state_root())).unwrap();
    let mut pruned = xx_declare();
    pruned.exec_v2_test_origin_daa = None;
    let mut full = pruned.clone();
    full.exec_v2_origin_headers.insert(origin.source_block, vec![0xFF]);
    full.exec_v2_test_origin_daa = Some(109); // even the test-only native seam cannot renew root authority
    let objects = [open_object(decl(claim))];
    let (a, da) = step_with(&s, &p, 10, 110, &objects, &full).unwrap();
    let (b, db) = step_with(&restarted, &p, 10, 110, &objects, &pruned).unwrap();
    assert_eq!(a.state_root(), b.state_root());
    assert_eq!(borsh::to_vec(&da).unwrap(), borsh::to_vec(&db).unwrap());
    assert_eq!(a.exec_v2_job_uses.values().next().unwrap().issued_daa, 100);
    // An already opened F2 root remains recoverable without the later F3 provenance extension.
    let mut old_opened = PalwStateCarriageV2::from_state(&a);
    old_opened.exec_v2_origins.clear();
    let old_root = old_opened.clone().into_state(&p, None).unwrap().state_root();
    let restored_old = borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&old_opened).unwrap())
        .unwrap()
        .into_state(&p, Some(old_root))
        .unwrap();
    assert_eq!(restored_old.exec_v2_root_v1(&claim), a.exec_v2_root_v1(&claim));
    assert_eq!(restored_old.exec_v2_job_uses, a.exec_v2_job_uses);
    assert!(restored_old.exec_v2_origin_v1(&claim).is_none());
    let mut legacy = s.clone();
    legacy.exec_v2_origins.clear();
    for extras in [&full, &pruned] {
        let error = step_with(&legacy, &p, 10, 110, &objects, extras).unwrap_err();
        assert!(
            error.to_string().contains("no recorded original"),
            "missing recorded authority cannot be backfilled at root admission: {error}"
        );
    }
}

// Arithmetic provenance fixture: an accepted native row is pinned to a complete header; this is not native signature evidence.
fn legacy_origin_world() -> (PalwStateParamsV2, PalwChainStateV2, Hash64, crate::palw_work_slice_v2::PalwExecOriginWitnessV1) {
    legacy_origin_world_at(100)
}

fn legacy_origin_world_at(
    source_daa: u64,
) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, crate::palw_work_slice_v2::PalwExecOriginWitnessV1) {
    let (p, mut s, id) = world(Some(0));
    let envelope = attempt(PWU, 1);
    let mut header = crate::header::Header::from_precomputed_hash(h64(0xBAD_CACE), vec![]);
    header.pow_algo_id = crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3;
    header.daa_score = source_daa;
    header.nonce = 17;
    header.palw_commitment = envelope.encode_wire();
    let claim = s.claims.get_mut(&id).unwrap();
    claim.accepted_block = crate::hashing::header::hash(&header);
    claim.job_identity = crate::palw_attempt_v2::execution_anchor_v3(
        envelope.attempt.network_domain,
        crate::hashing::header::pre_pow_hash_64(&header),
        claim.class_id,
        &claim.bond.0,
        header.nonce,
    );
    s.exec_v2_origins.clear();
    s.assert_internal_consistency(&p).unwrap();
    let witness = crate::palw_work_slice_v2::PalwExecOriginWitnessV1 { claim: id, header: borsh::to_vec(&header).unwrap() };
    (p, s, id, witness)
}

/// The per-block work budget counts refused decodes/hashes and is never a reusable authentication.
#[test]
fn exec_v2_origin_budget_is_spent_before_verification_and_not_refunded_by_restore() {
    let (p, state, id, witness) = legacy_origin_world();
    let extras = xx_declare();
    let point = ctx(21, 110, 21);
    let mut builder = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let checkpoint = builder.checkpoint();
    let mut borrowed = witness.clone();
    borrowed.claim = h64(0xBAD);
    assert!(apply_object(&mut builder, &point, &PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(borrowed) }).is_err());
    assert_eq!(builder.exec_origin_budget.charged(), 0, "missing claims refuse before header work");
    let mut header: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    header.nonce += 1;
    let fake = PalwExecOriginWitnessV1 { claim: id, header: borsh::to_vec(&header).unwrap() };
    assert!(state.exec_v2_origin_witness_metadata_admissible_v1(&p, point.daa_score, &fake).is_ok());
    let fake = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(fake) };
    for charged in 1..=PALW_EXEC_ORIGIN_VERIFICATIONS_PER_BLOCK_V1 {
        let error = apply_object(&mut builder, &point, &fake).unwrap_err();
        assert_eq!(error.to_string(), PalwStateV2Error::ExecV2Refused(PalwWorkRootRefusalV2::AuthorizationOrigin.to_string()).to_string());
        assert_eq!(builder.exec_origin_budget.charged(), charged);
        assert!(builder.entries.is_empty());
        assert_eq!(builder.state.state_root(), state.state_root());
    }
    builder.restore(checkpoint);
    let good = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness) };
    let error = apply_object(&mut builder, &point, &good).unwrap_err();
    assert_eq!(error.to_string(), PalwStateV2Error::ExecV2Refused(PalwWorkRootRefusalV2::OriginVerificationBudget.to_string()).to_string());
    assert_eq!(builder.exec_origin_budget.charged(), PALW_EXEC_ORIGIN_VERIFICATIONS_PER_BLOCK_V1);
    assert!(builder.state.exec_v2_origin_v1(&id).is_none());
    // A new block has a fresh budget, but it must still do the full proof check before writing.
    let migrated = step_with(&state, &p, 22, 111, std::slice::from_ref(&good), &extras).unwrap().0;
    assert_eq!(migrated.exec_v2_origin_v1(&id).unwrap().issued_daa, 100);
    let dormant = p.clone().with_exec_v2_from_daa(None);
    let mut builder = TransitionBuilder::new(&state, &dormant, false, false, false, false, &extras);
    assert!(apply_object(&mut builder, &point, &good).is_err());
    assert_eq!(builder.exec_origin_budget.charged(), 0);
}

/// Compare the clone-free query with the old acceptance-reader + one-object writer oracle.
/// The verdict is scoped to this exact state/point/proof and never installs an authority row.
#[test]
fn exec_v2_origin_rehearsal_matches_the_writer_and_rechecks_changed_inputs() {
    use crate::palw_producer_v2::PalwObjectRehearsalV1 as Verdict;
    use std::cell::Cell;
    let (p, original, id, witness) = legacy_origin_world();
    let header: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    for case in 0..17 {
        let mut state = original.clone();
        let mut params = p.clone();
        let mut proof = witness.clone();
        let mut changed = header.clone();
        let point = ctx(21, 110, 21);
        match case {
            0 => {}
            1 => {
                changed.nonce += 1;
                proof.header = borsh::to_vec(&changed).unwrap();
            }
            2 => {
                changed.daa_score += 1;
                proof.header = borsh::to_vec(&changed).unwrap();
            }
            3 => {
                changed.hash = h64(0xCA_CE);
                proof.header = borsh::to_vec(&changed).unwrap();
            }
            4 => {
                proof.claim = h64(0xBAD);
            }
            5 => {
                state.claims.get_mut(&id).unwrap().class_id = h64(0xBAD);
            }
            6 => {
                state.claims.get_mut(&id).unwrap().bond = bond_key(99);
            }
            7 => {
                state.claims.get_mut(&id).unwrap().job_identity = h64(0xBAD);
            }
            8 => {
                state.claims.get_mut(&id).unwrap().source = PalwClaimSourceV2::FreePrompt { quanta: 1, spent: BTreeSet::new() };
            }
            9 => {
                state.claims.remove(&id);
            }
            10 => {
                state.exec_v2_origins.insert(id, state.exec_v2_origin_witness_admissible_v1(&p, point.daa_score, &witness).unwrap());
            }
            11 => {
                params = params.with_exec_v2_from_daa(None);
            }
            12 => {
                params = params.with_exec_v2_from_daa(Some(point.daa_score + 1));
            }
            13 => {
                params = params.with_exec_v2_from_daa(Some(point.daa_score));
            }
            14 => {
                proof.header.clear();
            }
            15 => {
                proof.header.pop();
            }
            16 => {
                changed.pow_algo_id = crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
                proof.header = borsh::to_vec(&changed).unwrap();
            }
            _ => unreachable!(),
        }
        let object = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(proof.clone()) };
        let root = state.state_root();
        let origins = state.exec_v2_origin_count_v1();
        for flags in 0..32 {
            let mut extras = xx_declare();
            extras.model_immutable_active = flags & 16 != 0;
            let accepted = state.exec_v2_origin_witness_admissible_v1(&params, point.daa_score, &proof);
            let expected = match &accepted {
                Err(error) => Verdict::NotAccepted(error.to_string()),
                Ok(_) => match palw_v2_apply_one_object_v1(
                    &state,
                    &params,
                    &point,
                    &object,
                    flags & 1 != 0,
                    flags & 2 != 0,
                    flags & 4 != 0,
                    flags & 8 != 0,
                    &extras,
                ) {
                    Ok(written) => {
                        assert_eq!(written.exec_v2_origin_v1(&id), accepted.as_ref().ok());
                        Verdict::Accepted
                    }
                    Err(error) => Verdict::Refused(error),
                },
            };
            let calls = Cell::new(0);
            let actual = palw_v2_origin_rehearsal_v1(&state, &params, &point, &object, || {
                calls.set(calls.get() + 1);
                extras.clone()
            });
            assert_eq!(actual, expected, "case {case}, flags {flags}");
            assert_eq!(calls.get(), usize::from(accepted.is_ok()), "no extras for a refused proof");
            assert_eq!(state.state_root(), root, "the query is read-only");
            assert_eq!(state.exec_v2_origin_count_v1(), origins);
        }
    }
    let object = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness) };
    let dormant = p.clone().with_exec_v2_from_daa(Some(110));
    for (daa, ok) in [(109, false), (110, true), (111, true), (109, false)] {
        let verdict = palw_v2_origin_rehearsal_v1(&original, &dormant, &ctx(21, daa, 21), &object, xx);
        assert_eq!(verdict == Verdict::Accepted, ok, "a previous verdict never authorizes a new point");
    }
    let other = register_class_and_bond().pop().unwrap();
    let calls = Cell::new(0);
    assert!(matches!(
        palw_v2_origin_rehearsal_v1(&original, &p, &ctx(21, 110, 21), &other, || {
            calls.set(calls.get() + 1);
            xx()
        }),
        Verdict::NotAccepted(_)
    ));
    assert_eq!(calls.get(), 0, "this query never substitutes for another object arm");
}

#[test]
fn exec_v2_authorization_orders_both_clocks_against_opening_not_against_each_other() {
    use crate::palw_state_v2::palw_exec_v2_history_v1::authorize;
    // Arithmetic provenance, not evidence that this DAA ordering occurs in a live DAG. Both clocks are authenticated data;
    // neither possible ordering should create a fresh lease or forbid an otherwise eligible root after both clocks are reached.
    let (p, s, id, witness) = legacy_origin_world_at(112);
    assert_eq!(s.claim(&id).unwrap().accepted_daa, 101);
    let mut extras = xx_declare();
    extras.exec_v2_test_origin_daa = None;
    let (migrated, _) =
        step_with(&s, &p, 21, 110, &[PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness) }], &extras).unwrap();
    let mut d = decl(id);
    d.canonical_job_id = migrated.claim(&id).unwrap().job_identity;
    assert!(step_with(&migrated, &p, 22, 111, &[open_object(d.clone())], &extras).is_err(), "source clock not reached");
    let (opened, _) = step_with(&migrated, &p, 23, 112, &[open_object(d.clone())], &extras)
        .expect("both clocks reached even though source DAA exceeds native acceptance DAA");
    let key = d.job_work_id(&opened.claim(&id).unwrap().class_id);
    let history = opened.exec_v2_job_use_v1(&key).unwrap();
    assert_eq!((history.issued_daa, history.expires_daa), (112, 100_112));
    let claim = opened.claim(&id).unwrap();
    assert!(authorize(&opened, &d, claim, 100_111).is_ok());
    assert!(authorize(&opened, &d, claim, 100_112).unwrap_err().to_string().contains("expired"));
    let mut future_acceptance = claim.clone();
    future_acceptance.accepted_daa = 113;
    assert!(authorize(&opened, &d, &future_acceptance, 112).unwrap_err().to_string().contains("header"));
    // A late state point cannot conceal that a root was opened before its original authorization clock.
    let mut early_root = opened.clone();
    early_root.exec_v2.roots.get_mut(&id).unwrap().opened_daa = 111;
    assert!(early_root.assert_internal_consistency(&p).is_err());
    assert!(PalwStateCarriageV2::from_state(&early_root).into_state(&p, None).is_err());
    // Changing the native acceptance clock alone never renews or alters the authenticated source deadline.
    for accepted in [100, 112, 113] {
        let mut reordered = claim.clone();
        reordered.accepted_daa = accepted;
        assert_eq!(authorize(&opened, &d, &reordered, 114).unwrap().expires_daa, 100_112);
    }
}

#[test]
fn exec_v2_legacy_clock_migrates_from_carried_proof_without_income_or_lease_renewal() {
    let (p, s, id, witness) = legacy_origin_world();
    let object = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness.clone()) };
    let mut extras = xx_declare();
    extras.exec_v2_origin_headers.clear();
    extras.exec_v2_test_origin_daa = None;
    let (migrated, delta) = step_with(&s, &p, 21, 110, &[object.clone()], &extras).unwrap();
    let control = step_with(&s, &p, 21, 110, &[], &extras).unwrap().0;
    assert_eq!(migrated.claim(&id), control.claim(&id));
    assert_eq!(migrated.safe_weight, control.safe_weight);
    assert_eq!(migrated.reserved_exposure, control.reserved_exposure);
    assert_eq!(migrated.pending_payouts, control.pending_payouts);
    assert_eq!(migrated.exec_v2_origin_v1(&id).unwrap().issued_daa, 100);
    assert_ne!(migrated.state_root(), control.state_root());
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::ExecV2Row { table: PALW_EXEC_V2_TABLE_ORIGINS_V1, .. })));
    let mut d = decl(id);
    d.canonical_job_id = s.claim(&id).unwrap().job_identity;
    assert!(step_with(&s, &p, 22, 111, &[open_object(d.clone())], &extras).is_err());
    let opened = step_with(&migrated, &p, 22, 111, &[open_object(d.clone())], &extras).unwrap().0;
    assert_eq!(opened.exec_v2_job_use_v1(&d.job_work_id(&s.claim(&id).unwrap().class_id)).unwrap().expires_daa, 100_100);
    let repeated = step_with(&migrated, &p, 23, 112, &[object.clone()], &extras).unwrap_err();
    assert!(repeated.to_string().contains("already has a recorded"));
    assert!(step_with(&s, &p.clone().with_exec_v2_from_daa(None), 21, 110, &[object], &extras).is_err());
    // A late carried proof records the same original clock; it does not mint a lease from its migration block.
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    apply_object(&mut builder, &ctx(25, 100_101, 25), &PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness) })
        .unwrap();
    assert_eq!(builder.state.exec_v2_origin_v1(&id).unwrap().issued_daa, 100);
    let error =
        super::super::palw_exec_v2_history_v1::authorize(&builder.state, &d, builder.state.claim(&id).unwrap(), 100_100).unwrap_err();
    assert!(error.to_string().contains("expired"));
}

#[test]
fn exec_v2_origin_migration_chunks_are_atomic_fenced_and_cannot_contradict_open_history() {
    let (p, s, id, witness) = legacy_origin_world();
    let object = PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(witness.clone()) };
    assert_eq!(borsh::to_vec(&object).unwrap()[0], 131);
    let cap = borsh::to_vec(&object).unwrap().len().div_ceil(4);
    let chunks = palw_object_chunks_with_cap_v1(&object, cap).unwrap().unwrap();
    assert_eq!(chunks.len(), 4);
    let mut state = s.clone();
    let mut dormant = s.clone();
    let mut extras = xx_declare();
    extras.exec_v2_test_origin_daa = None;
    for (index, chunk) in chunks.iter().enumerate() {
        state = step_with(&state, &p, 31 + index as u64, 110 + index as u64, std::slice::from_ref(chunk), &extras).unwrap().0;
        if index < 3 {
            assert!(state.exec_v2_origin_v1(&id).is_none(), "partial carriage is never authorization");
            dormant = step_with(
                &dormant,
                &p.clone().with_exec_v2_from_daa(None),
                31 + index as u64,
                110 + index as u64,
                std::slice::from_ref(chunk),
                &extras,
            )
            .unwrap()
            .0;
        } else {
            let error = step_with(&dormant, &p.clone().with_exec_v2_from_daa(None), 34, 113, std::slice::from_ref(chunk), &extras)
                .unwrap_err();
            assert!(
                matches!(error, PalwStateV2Error::ChunkedObjectUndecodable(_)),
                "below the fence this is the old build's unknown payload: {error}"
            );
        }
    }
    assert!(state.pending_chunks.is_empty());
    assert_eq!(state.exec_v2_origin_v1(&id).unwrap().issued_daa, 100);
    let mut d = decl(id);
    d.canonical_job_id = state.claim(&id).unwrap().job_identity;
    let mut opened = step_with(&state, &p, 35, 114, &[open_object(d.clone())], &extras).unwrap().0;
    opened.exec_v2_origins.clear();
    assert!(opened.exec_v2_origin_witness_admissible_v1(&p, 115, &witness).is_ok());
    let history = opened.exec_v2_job_uses.get_mut(&d.job_work_id(&s.claim(&id).unwrap().class_id)).unwrap();
    history.issued_daa += 1;
    history.expires_daa += 1;
    assert!(
        opened.exec_v2_origin_witness_admissible_v1(&p, 115, &witness).is_err(),
        "a proof cannot install a clock contradicting authenticated F2 history"
    );
}

#[test]
fn exec_v2_origin_proof_codec_rejects_hostile_parent_runs_and_substitution_before_writes() {
    use crate::palw_work_slice_v2::{PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1, PalwExecOriginWitnessV1};
    let (p, s, id, witness) = legacy_origin_world();
    let mut encoded = id.as_byte_slice().to_vec();
    encoded.extend_from_slice(&((PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1 + 1) as u32).to_le_bytes());
    assert!(borsh::from_slice::<PalwExecOriginWitnessV1>(&encoded).unwrap_err().to_string().contains("carriage budget"));
    let bytes = borsh::to_vec(&witness).unwrap();
    assert_eq!(borsh::from_slice::<PalwExecOriginWitnessV1>(&bytes).unwrap(), witness);
    assert!(borsh::from_slice::<PalwExecOriginWitnessV1>(&bytes[..bytes.len() - 1]).is_err());
    let original: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed.daa_score += 1;
    variants.push(borsh::to_vec(&changed).unwrap());
    let mut changed = original.clone();
    changed.nonce += 1;
    variants.push(borsh::to_vec(&changed).unwrap());
    for runs in [vec![(0u8, vec![h64(1)])], vec![(2, vec![h64(1)]), (1, vec![h64(2)])], vec![(1, vec![h64(1)]), (2, vec![h64(1)])]] {
        let mut changed = original.clone();
        changed.parents_by_level = borsh::from_slice(&borsh::to_vec(&runs).unwrap()).unwrap();
        variants.push(borsh::to_vec(&changed).unwrap());
    }
    let mut count = witness.header.clone();
    count[66..70].copy_from_slice(&u32::MAX.to_le_bytes());
    variants.push(count);
    for header in variants {
        let forged = PalwExecOriginWitnessV1 { claim: id, header };
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.exec_v2_origin_witness_admissible_v1(&p, 110, &forged)));
        assert!(outcome.unwrap().is_err(), "hostile preimages refuse instead of panicking");
        let extras = xx();
        let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
        assert!(
            apply_object(&mut builder, &ctx(21, 110, 21), &PalwConsensusObjectV2::ExecOriginRecordedV1 { witness: Box::new(forged) })
                .is_err()
        );
        assert!(builder.entries.is_empty());
        assert_eq!(builder.state.state_root(), s.state_root());
    }
    let mut borrowed = witness;
    borrowed.claim = h64(0xBAD);
    assert!(s.exec_v2_origin_witness_admissible_v1(&p, 110, &borrowed).is_err());
}

#[test]
fn exec_v2_origin_verification_rejects_cheap_mismatches_before_expanded_hashing_with_same_verdicts() {
    use crate::palw_state_v2::palw_exec_v2_origin_v1::verify_header_bytes_with_hashers;
    use std::cell::Cell;
    let (_, s, id, witness) = legacy_origin_world();
    let original: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    let original_claim = s.claim(&id).unwrap();
    for case in 0..13 {
        let mut header = original.clone();
        let mut claim = original_claim.clone();
        let mut claim_id = id;
        let (expected_calls, expected_ok) = match case {
            0 => ((1, 1), true),
            1 => {
                claim_id = h64(0xBAD);
                ((0, 0), false)
            }
            2 => {
                claim.class_id = h64(0xBAD);
                ((0, 0), false)
            }
            3 => {
                claim.bond = bond_key(99);
                ((0, 0), false)
            }
            4 => {
                header.pow_algo_id = crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
                ((0, 0), false)
            }
            5 => {
                header.palw_commitment = vec![0xFF];
                ((0, 0), false)
            }
            6 => {
                header.nonce += 1;
                ((1, 0), false)
            }
            7 => {
                header.palw_commitment.push(0);
                ((0, 0), false)
            }
            8 => {
                let mut envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).unwrap();
                envelope.signature.push(1);
                header.palw_commitment = envelope.encode_wire();
                ((1, 0), false)
            }
            9 => {
                claim.job_identity = h64(0xBAD);
                ((1, 1), false)
            }
            10 => {
                header.hash = h64(0xBAD);
                ((1, 1), true)
            }
            11 => {
                claim.job_identity = Hash64::default();
                ((1, 1), true)
            }
            _ => {
                let mut envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).unwrap();
                envelope.attempt.class_id = h64(0xBAD);
                claim.class_id = envelope.attempt.class_id;
                header.palw_commitment = envelope.encode_wire();
                ((0, 0), false)
            }
        };
        // The previous predicate, using the ordinary codec and ordinary header digests as an independent verdict oracle.
        let old_ok = (|| {
            if crate::hashing::header::hash(&header) != claim.accepted_block
                || !crate::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id)
            {
                return false;
            }
            let Ok(envelope) = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment) else {
                return false;
            };
            let anchor = crate::palw_attempt_v2::execution_anchor_v3(
                envelope.attempt.network_domain,
                crate::hashing::header::pre_pow_hash_64(&header),
                envelope.attempt.class_id,
                &envelope.attempt.executor_bond,
                header.nonce,
            );
            attempt_id_v2(&envelope.attempt) == claim_id
                && envelope.attempt.class_id == claim.class_id
                && envelope.attempt.executor_bond == claim.bond.0
                && (claim.job_identity == Hash64::default() || anchor == claim.job_identity)
        })();
        let (identity_calls, pre_pow_calls) = (Cell::new(0), Cell::new(0));
        let result = verify_header_bytes_with_hashers(
            claim_id,
            &claim,
            &borsh::to_vec(&header).unwrap(),
            |h| {
                identity_calls.set(identity_calls.get() + 1);
                crate::hashing::header::hash_with_parent_run_cache(h)
            },
            |h| {
                pre_pow_calls.set(pre_pow_calls.get() + 1);
                crate::hashing::header::pre_pow_hash_with_parent_run_cache(h)
            },
        );
        assert_eq!(old_ok, expected_ok, "old verdict case {case}");
        assert_eq!(result.is_ok(), old_ok, "unchanged verdict case {case}");
        assert_eq!((identity_calls.get(), pre_pow_calls.get()), expected_calls, "digest calls case {case}");
        if let Ok(issued) = result {
            assert_eq!(issued, header.daa_score);
        }
    }
}

#[test]
fn exec_v2_origin_large_parent_run_with_wrong_class_never_enters_either_header_digest() {
    use crate::palw_state_v2::palw_exec_v2_origin_v1::verify_header_bytes_with_hashers;
    let (_, s, id, witness) = legacy_origin_world();
    let mut header: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    let cap = PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1;
    let count = (cap - witness.header.len() - 5) / kaspa_hashes::HASH64_SIZE;
    header.parents_by_level =
        crate::header::CompressedParents::try_from(vec![(255, (0..count).map(|i| h64(i as u64)).collect())]).unwrap();
    let bytes = borsh::to_vec(&header).unwrap();
    assert!(bytes.len() <= cap);
    assert!(count * 255 * kaspa_hashes::HASH64_SIZE > 400_000_000);
    let mut wrong = s.claim(&id).unwrap().clone();
    wrong.class_id = h64(0xBAD);
    let result = verify_header_bytes_with_hashers(
        id,
        &wrong,
        &bytes,
        |_| panic!("wrong class must refuse before identity hashing"),
        |_| panic!("wrong class must refuse before pre-PoW hashing"),
    );
    assert!(result.is_err());
}

#[test]
#[ignore = "explicit per-process resource measurement, not a latency/RSS approval threshold"]
fn exec_v2_origin_resource_probe() {
    use std::hint::black_box;
    use std::time::Instant;
    let mode = std::env::var("MISAKA_EXEC_ORIGIN_RESOURCE_CASE").unwrap_or_else(|_| "matching-fake".into());
    assert!(["baseline", "wrong-class", "matching-fake", "pinned-identity", "eight-refusals", "one-parent"].contains(&mode.as_str()));
    let (p, mut s, id, witness) = legacy_origin_world();
    let mut header: crate::header::Header = borsh::from_slice(&witness.header).unwrap();
    let cap = PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1;
    let count = if mode == "one-parent" { 1 } else { (cap - witness.header.len() - 5) / kaspa_hashes::HASH64_SIZE };
    header.parents_by_level =
        crate::header::CompressedParents::try_from(vec![(255, (0..count).map(|i| h64(i as u64)).collect())]).unwrap();
    let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).unwrap();
    // Arithmetic authentication control only: this large direct-parent set is not native admission evidence.
    // Use the ordinary writers to pin the fixture, independently of the origin verifier's optimized writers.
    let claim = s.claims.get_mut(&id).unwrap();
    claim.accepted_block = crate::hashing::header::hash(&header);
    claim.job_identity = crate::palw_attempt_v2::execution_anchor_v3(
        envelope.attempt.network_domain,
        crate::hashing::header::pre_pow_hash_64(&header),
        claim.class_id,
        &claim.bond.0,
        header.nonce,
    );
    s.assert_internal_consistency(&p).unwrap();
    if mode == "wrong-class" {
        let mut wrong = envelope;
        wrong.attempt.class_id = h64(0xBAD);
        header.palw_commitment = wrong.encode_wire();
    } else if mode == "matching-fake" || mode == "eight-refusals" {
        header.nonce += 1; // Public attempt id/class/bond still match; full identity must reject the substitution.
    }
    let encoded = borsh::to_vec(&header).unwrap();
    assert!(encoded.len() <= cap);
    let issued = header.daa_score;
    drop(header); // Measurement retains the state and carriage bytes, not a producer's decoded copy.
    let witness = PalwExecOriginWitnessV1 { claim: id, header: encoded };
    let repetitions = if mode == "eight-refusals" { PALW_OBJECT_CHUNK_MAX_GROUPS } else { 1 };
    let expected_ok = mode == "pinned-identity" || mode == "one-parent";
    let verify = || {
        let result = black_box(s.exec_v2_origin_witness_admissible_v1(black_box(&p), 110, black_box(&witness)));
        assert_eq!(result.is_ok(), expected_ok);
        match result {
            Ok(origin) => assert_eq!(origin.issued_daa, issued),
            Err(error) => assert_eq!(
                error.to_string(),
                PalwStateV2Error::ExecV2Refused(PalwWorkRootRefusalV2::AuthorizationOrigin.to_string()).to_string()
            ),
        }
    };
    let mut samples = Vec::new();
    if mode != "baseline" {
        verify();
        for _ in 0..6 {
            let started = Instant::now();
            for _ in 0..repetitions {
                verify();
            }
            samples.push(started.elapsed());
        }
        samples.sort();
    }
    assert_eq!(s.exec_v2_origin_count_v1(), 0, "reader is read-only even for a successful provenance control");
    eprintln!(
        "[EXEC origin resource] case={mode} parents={count} levels=255 encoded_bytes={} parent_bytes_per_digest={} repetitions={repetitions} samples={} median_batch={:?} median_per_reader={:?}",
        witness.header.len(),
        count * 255 * kaspa_hashes::HASH64_SIZE,
        samples.len(),
        samples.get(3),
        samples.get(3).map(|duration| *duration / repetitions as u32)
    );
}

#[test]
fn exec_v2_origin_capture_cannot_cap_or_reprice_native_weight_or_income() {
    let (p, state, _) = world(Some(0));
    let env = attempt(PWU, 2);
    let id = attempt_id_v2(&env.attempt);
    let point = ctx(20, 111, 20);
    // Original source clocks are data, not a second native admission clock. Root eligibility is checked separately.
    let extras = PalwTransitionExtrasV1 { exec_v2_test_origin_daa: Some(112), ..xx() };
    let (active, _) = apply_door(&state, &p, &point, &[], Some(&env), &extras).unwrap();
    let (inactive, _) = apply_door(&state, &p.clone().with_exec_v2_from_daa(None), &point, &[], Some(&env), &extras).unwrap();
    assert_eq!(active.claim(&id), inactive.claim(&id));
    assert!(active.claim(&id).is_some(), "ordinary native work remains admitted");
    assert_eq!(active.safe_weight, inactive.safe_weight);
    assert_eq!(active.bounded_immature, inactive.bounded_immature);
    assert_eq!(active.pending_payouts, inactive.pending_payouts);
    assert_eq!(active.reserved_exposure, inactive.reserved_exposure);
    assert_eq!(active.exec_v2_origin_v1(&id).unwrap().issued_daa, 112);
    assert!(inactive.exec_v2_origin_v1(&id).is_none());
}

#[test]
fn exec_v2_origin_is_rooted_bounded_by_live_claims_and_retires_through_the_same_journal() {
    let (p, s, claim) = world_with(xp(Some(0)).with_claim_retirement_daa(700).unwrap());
    let before = PalwStateCarriageV2::from_state(&s);
    let mut changed = s.clone();
    changed.exec_v2_origins.get_mut(&claim).unwrap().issued_daa += 1;
    assert_ne!(changed.state_root(), s.state_root());
    let mut bad = before.clone();
    bad.exec_v2_origins.get_mut(&claim).unwrap().source_block = h64(0xBAD);
    assert!(bad.into_state(&p, None).is_err());
    let mut orphan = before.clone();
    orphan.exec_v2_origins.insert(h64(0xBAD), orphan.exec_v2_origins[&claim].clone());
    assert!(orphan.into_state(&p, None).is_err());
    let mut invalid_time = before.clone();
    invalid_time.exec_v2_origins.get_mut(&claim).unwrap().issued_daa = s.claim(&claim).unwrap().accepted_daa + 1;
    assert!(invalid_time.into_state(&p, Some(s.state_root())).is_err(), "original clock is authenticated by the snapshot root");
    let bytes = borsh::to_vec(&before).unwrap();
    let tail = borsh::to_vec(&s.exec_v2_origins).unwrap();
    assert_eq!(bytes[bytes.len() - tail.len() - 1], PALW_CARRIAGE_EXEC_V2_ORIGINS_TAIL_V1);
    let mut repeated = bytes.clone();
    repeated.push(PALW_CARRIAGE_EXEC_V2_ORIGINS_TAIL_V1);
    repeated.extend_from_slice(&tail);
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&repeated).is_err());
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes[..bytes.len() - 1]).is_err());
    let mut excess = bytes[..bytes.len() - tail.len()].to_vec();
    excess.extend_from_slice(&(s.claims.len() as u32 + 1).to_le_bytes());
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&excess).unwrap_err().to_string().contains("exceeds live claims"));
    let extras = xx();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    // Removal uses the native claim's sole writer; rollback restores both claim and immutable origin.
    builder.write_claim(claim, None);
    assert!(builder.state.exec_v2_origin_v1(&claim).is_none());
    assert!(
        builder
            .entries
            .iter()
            .any(|e| matches!(e, PalwDeltaEntryV2::ExecV2Row { table: PALW_EXEC_V2_TABLE_ORIGINS_V1, new: None, .. }))
    );
    let opened = open(&p, &s, decl(claim)).unwrap();
    let voided = step(&opened, &p, 71, 5_000, &[]).unwrap().0;
    let retired = run_to_retirement(&p, voided, claim);
    assert!(retired.exec_v2_origin_v1(&claim).is_none());
    assert_eq!(retired.exec_v2_job_use_count_v1(), 1);
    assert_eq!(PALW_CARRIAGE_EXEC_V2_ORIGINS_TAIL_V1, 0xF3);
    assert_eq!(PALW_EXEC_V2_TABLE_ORIGINS_V1, 8);
}

#[test]
fn exec_v2_authorization_reads_the_original_header_and_refuses_substitution_or_renewal() {
    use crate::palw_attempt_v2::execution_anchor_v3;
    use crate::palw_state_v2::palw_exec_v2_history_v1::authorize;
    use crate::palw_state_v2::palw_exec_v2_origin_v1::source_time;
    let (_, mut state, id) = world(Some(0));
    let envelope = attempt(PWU, 1);
    assert_eq!(attempt_id_v2(&envelope.attempt), id);
    let mut header = crate::header::Header::from_precomputed_hash(Hash64::default(), vec![]);
    header.pow_algo_id = crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3;
    header.daa_score = 100;
    header.nonce = 17;
    header.palw_commitment = envelope.encode_wire();
    let anchor = |h: &crate::header::Header| {
        execution_anchor_v3(
            envelope.attempt.network_domain,
            crate::hashing::header::pre_pow_hash_64(h),
            envelope.attempt.class_id,
            &envelope.attempt.executor_bond,
            h.nonce,
        )
    };
    let mut claim = state.claim(&id).unwrap().clone();
    claim.accepted_block = crate::hashing::header::hash(&header);
    claim.job_identity = anchor(&header);
    let mut declaration = decl(id);
    declaration.canonical_job_id = claim.job_identity;
    let mut extras = PalwTransitionExtrasV1::default();
    extras.exec_v2_origin_headers.insert(claim.accepted_block, borsh::to_vec(&header).unwrap());
    let issued = source_time(&extras, id, &claim).unwrap();
    assert_eq!(issued, 100);
    state.exec_v2_origins.insert(id, PalwExecOriginV1 { source_block: claim.accepted_block, issued_daa: issued });
    let row = authorize(&state, &declaration, &claim, 110).unwrap();
    assert_eq!((row.issued_daa, row.expires_daa), (100, 100_100));
    assert!(authorize(&state, &declaration, &claim, 100_099).is_ok());
    assert!(authorize(&state, &declaration, &claim, 100_100).unwrap_err().to_string().contains("expired"));
    let mut late = claim.clone();
    late.accepted_daa = 100_100;
    assert!(authorize(&state, &declaration, &late, 100_101).unwrap_err().to_string().contains("expired"));
    assert!(source_time(&PalwTransitionExtrasV1::default(), id, &claim).unwrap_err().to_string().contains("header"));
    for field in 0..4 {
        let mut substituted = header.clone();
        match field {
            0 => substituted.daa_score += 1,
            1 => substituted.nonce += 1,
            2 => substituted.timestamp += 1,
            _ => substituted.pow_algo_id = crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
        }
        let mut wrong = extras.clone();
        wrong.exec_v2_origin_headers.insert(claim.accepted_block, borsh::to_vec(&substituted).unwrap());
        assert!(source_time(&wrong, id, &claim).is_err(), "cached header hash cannot authorize substituted bytes");
    }
    for field in 0..3 {
        let mut wrong = claim.clone();
        match field {
            0 => wrong.job_identity = h64(99),
            1 => wrong.class_id = h64(99),
            _ => wrong.bond = bond_key(99),
        }
        assert!(source_time(&extras, id, &wrong).is_err());
    }
    let mut wrong = declaration.clone();
    wrong.root_claim_id = h64(99);
    assert!(source_time(&extras, wrong.root_claim_id, &claim).is_err());
    // Genuine header binding with an unrepresentable finite authorization is still refused.
    header.daa_score = u64::MAX - 99_999;
    claim.accepted_daa = header.daa_score;
    claim.accepted_block = crate::hashing::header::hash(&header);
    claim.job_identity = anchor(&header);
    declaration.canonical_job_id = claim.job_identity;
    extras.exec_v2_origin_headers.clear();
    extras.exec_v2_origin_headers.insert(claim.accepted_block, borsh::to_vec(&header).unwrap());
    let issued = source_time(&extras, id, &claim).unwrap();
    state.exec_v2_origins.insert(id, PalwExecOriginV1 { source_block: claim.accepted_block, issued_daa: issued });
    assert!(authorize(&state, &declaration, &claim, header.daa_score).unwrap_err().to_string().contains("expired"));
}

#[test]
fn exec_v2_authorization_history_survives_retirement_and_cannot_be_renewed_after_expiry() {
    use crate::palw_work_slice_v2::PALW_EXEC_V2_JOB_AUTHORIZATION_DAA_V1;
    let (p, s, old) = world_with(xp(Some(0)).with_claim_retirement_daa(700).unwrap());
    let s = open(&p, &s, decl(old)).unwrap();
    let key = s.exec_v2_root_v1(&old).unwrap().job_work_id;
    let use_row = s.exec_v2_job_use_v1(&key).unwrap().clone();
    assert_eq!(use_row.expires_daa, 100 + PALW_EXEC_V2_JOB_AUTHORIZATION_DAA_V1);
    let voided = step(&s, &p, 71, 5_000, &[]).unwrap().0;
    let retired = run_to_retirement(&p, voided, old);
    assert!(retired.claim(&old).is_none());
    assert!(retired.exec_v2_state_v1().is_empty());
    assert_eq!(retired.exec_v2_job_use_v1(&key), Some(&use_row));

    // A new native claim/root id is not fresh authorization. These are the arithmetic header/attribution fixtures.
    let add_claim = |state: &PalwChainStateV2, daa, nonce, job, issued| {
        let word = state.last_point.unwrap().blue_score + 1;
        let env = attempt(PWU, nonce);
        let id = attempt_id_v2(&env.attempt);
        let (mut next, _) = apply_door(
            state,
            &p,
            &ctx(word, daa, word),
            &[],
            Some(&env),
            &PalwTransitionExtrasV1 { exec_v2_test_origin_daa: Some(issued), ..xx() },
        )
        .unwrap();
        next.claims.get_mut(&id).unwrap().job_identity = job;
        (next, id)
    };
    let (copied, copy) = add_claim(&retired, retired.last_point.unwrap().daa_score + 1, 2, h64(JOB), 100);
    let point = copied.last_point.unwrap();
    let mut declaration = decl(copy);
    declaration.expiry_daa = point.daa_score + 5_000;
    let rejected =
        step_with(&copied, &p, point.blue_score + 1, point.daa_score, &[open_object(declaration)], &xx_declare()).unwrap_err();
    assert!(rejected.to_string().contains("already"), "{rejected}");

    // Half-open deadline: deletion at the exact expiry never licenses reuse of the old source.
    let after = step(&retired, &p, retired.last_point.unwrap().blue_score + 1, use_row.expires_daa, &[]).unwrap().0;
    assert_eq!(after.exec_v2_job_use_count_v1(), 0);
    let (expired, copy) = add_claim(&after, use_row.expires_daa, 3, h64(JOB), 100);
    let point = expired.last_point.unwrap();
    let mut declaration = decl(copy);
    declaration.expiry_daa = point.daa_score + 5_000;
    let rejected =
        step_with(&expired, &p, point.blue_score + 1, point.daa_score, &[open_object(declaration)], &xx_declare()).unwrap_err();
    assert!(rejected.to_string().contains("authorization has expired"), "{rejected}");

    // Fresh authorization may repeat legitimate inputs. Give its actual fresh origin time, not the old source a new TTL.
    let (fresh, claim) = add_claim(&after, use_row.expires_daa, 4, h64(JOB + 1), use_row.expires_daa);
    let point = fresh.last_point.unwrap();
    let mut declaration = decl(claim);
    declaration.canonical_job_id = h64(JOB + 1);
    declaration.expiry_daa = point.daa_score + 5_000;
    let extras = PalwTransitionExtrasV1 { exec_v2_test_origin_daa: Some(point.daa_score), ..xx_declare() };
    let (accepted, _) = step_with(&fresh, &p, point.blue_score + 1, point.daa_score, &[open_object(declaration)], &extras).unwrap();
    assert_eq!(accepted.exec_v2_job_use_count_v1(), 1);
    assert!(accepted.exec_v2_root_v1(&claim).is_some());
}

#[test]
fn exec_v2_history_tail_is_authenticated_and_missing_live_history_is_refused() {
    let (p, s, claim) = opened();
    let key = s.exec_v2_root_v1(&claim).unwrap().job_work_id;
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&s);
    carriage.exec_v2_job_uses.clear();
    assert_ne!(borsh::to_vec(&carriage).unwrap(), bytes);
    assert!(carriage.into_state(&p, None).unwrap_err().to_string().contains("history"));
    let mut carriage = PalwStateCarriageV2::from_state(&s);
    carriage.exec_v2_job_uses.get_mut(&key).unwrap().expires_daa += 1;
    assert!(carriage.into_state(&p, None).is_err());
    let mut bad = s.clone();
    bad.exec_v2_job_uses.get_mut(&key).unwrap().source_block = h64(0xBAD);
    assert_ne!(bad.state_root(), s.state_root());
    assert!(bad.assert_internal_consistency(&p).is_err());
    let mut duplicate_owner = PalwStateCarriageV2::from_state(&s);
    let row = PalwExecJobUseV1 { canonical_job_id: h64(0xBAD), ..s.exec_v2_job_uses[&key].clone() };
    duplicate_owner.exec_v2_job_uses.insert(row.work_id(), row);
    assert!(duplicate_owner.into_state(&p, None).is_err(), "a retained root cannot own a second work key outside its active index");
    let tail = borsh::to_vec(&s.exec_v2_job_uses).unwrap();
    let tail_at = bytes.len() - tail.len() - 1;
    assert_eq!(bytes[tail_at], PALW_CARRIAGE_EXEC_V2_JOB_USES_TAIL_V1);
    let mut duplicate = bytes.clone();
    duplicate.push(PALW_CARRIAGE_EXEC_V2_JOB_USES_TAIL_V1);
    duplicate.extend_from_slice(&tail);
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&duplicate).is_err());
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes[..bytes.len() - 1]).is_err());
    let mut excessive = bytes[..tail_at + 1].to_vec();
    excessive.extend_from_slice(&(PALW_EXEC_V2_MAX_JOB_USES_V1 as u32 + 1).to_le_bytes());
    let error = borsh::from_slice::<PalwStateCarriageV2>(&excessive).unwrap_err();
    assert!(error.to_string().contains("exceeds its cap"), "reject length before requesting any rows: {error}");
    let mut repeated = bytes[..tail_at + 1].to_vec();
    repeated.extend_from_slice(&2u32.to_le_bytes());
    repeated.extend_from_slice(&tail[4..]);
    repeated.extend_from_slice(&tail[4..]);
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&repeated).unwrap_err().to_string().contains("strictly ordered"));
    assert_eq!(PALW_CARRIAGE_EXEC_V2_JOB_USES_TAIL_V1, 0xF2);
    assert_eq!(PALW_EXEC_V2_TABLE_JOB_USES_V1, 7);
}

#[test]
fn exec_v2_history_capacity_never_evicts_live_authorization_and_the_expiry_work_is_bounded() {
    use crate::palw_work_slice_v2::{PALW_EXEC_V2_JOB_USE_SWEEP_V1, PALW_EXEC_V2_MAX_JOB_USES_V1};
    let (p, mut s, claim) = opened();
    let key = s.exec_v2_root_v1(&claim).unwrap().job_work_id;
    let base = s.exec_v2_job_use_v1(&key).unwrap().clone();
    // Structural worst population, not a public 100k-node/release latency claim.
    for i in 0..PALW_EXEC_V2_MAX_JOB_USES_V1 - 1 {
        let row = PalwExecJobUseV1 { owner_root: h64(900_000 + i as u64), canonical_job_id: h64(10_000 + i as u64), ..base.clone() };
        s.exec_v2_job_uses.insert(row.work_id(), row);
    }
    palw_exec_v2_history_v1::rebuild(&mut s);
    s.assert_internal_consistency(&p).unwrap();
    let env = attempt(PWU, 2);
    let second = attempt_id_v2(&env.attempt);
    let point = s.last_point.unwrap();
    let (mut s, _) = apply_door(&s, &p, &ctx(point.blue_score + 1, 111, point.blue_score + 1), &[], Some(&env), &xx()).unwrap();
    s.claims.get_mut(&second).unwrap().job_identity = h64(JOB + 1);
    let mut declaration = decl(second);
    declaration.canonical_job_id = h64(JOB + 1);
    let extras = xx_declare();
    let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
    let context = ctx(s.last_point.unwrap().blue_score + 1, 112, s.last_point.unwrap().blue_score + 1);
    let refusal = palw_exec_v2_fold::apply_root_declared_v2(&mut builder, &context, &declaration).unwrap_err();
    assert!(refusal.to_string().contains("history is full"), "{refusal}");
    assert!(builder.entries.is_empty());
    assert_eq!(builder.state.state_root(), s.state_root());
    let (after, delta) = step(&s, &p, context.blue_score, base.expires_daa, &[]).unwrap();
    let removed = delta
        .entries
        .iter()
        .filter(|entry| matches!(entry, PalwDeltaEntryV2::ExecV2Row { table: PALW_EXEC_V2_TABLE_JOB_USES_V1, new: None, .. }))
        .count();
    assert_eq!(removed, PALW_EXEC_V2_JOB_USE_SWEEP_V1);
    assert_eq!(after.exec_v2_job_use_count_v1(), PALW_EXEC_V2_MAX_JOB_USES_V1 - removed);
    assert!(after.exec_v2_job_use_v1(&key).is_some(), "a retained root keeps its history even past authorization expiry");
}

fn run_to_retirement(p: &PalwStateParamsV2, mut s: PalwChainStateV2, claim: Hash64) -> PalwChainStateV2 {
    let mut word = 100;
    let mut daa = s.last_point.map(|point| point.daa_score).unwrap_or(0);
    for _ in 0..400 {
        if s.claim(&claim).is_none() {
            return s;
        }
        let at = deadline_of(&s, claim).expect("a terminal claim owes its retirement").max(daa + 1);
        word += 1;
        daa = at;
        s = step(&s, p, word, daa, &[]).unwrap().0;
    }
    panic!("the claim never retired");
}

// =============================================================================================
// Consistency, branches, the carriage tail
// =============================================================================================

#[test]
fn executor_funds_partially_drain_with_the_market_reserve_and_the_plan_matches_the_fold() {
    let (p, mut s, _) = world(Some(0));
    let root = h64(0xDEAD);
    let legs: BTreeMap<_, _> = s
        .bonds
        .iter()
        .flat_map(|(bond, row)| {
            [PalwExecV2RewardFundV1::NativeSlice, PalwExecV2RewardFundV1::KernelFee]
                .map(|fund| ((*bond, fund), PalwPayoutV2 { payload: row.payout_payload, amount: 100 }))
        })
        .collect();
    assert_eq!(legs.len(), 16);
    let total = legs.len() as u128 * 100;
    s.exec_v2_rewards
        .insert(root, PalwExecV2RewardRowV1 { final_daa: 101, liability_until: 101, allocation_sompi: total as u64, legs });
    s.vesting_counters.created += total;
    s.rebuild_vesting_indices();
    // Eight old non-market payments drain at step 1b, leaving the two market keys to reserve width at 3d.
    for i in 0..10 {
        let mut bytes = [0u8; 64];
        if i >= 8 {
            bytes[0] = PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX;
        }
        bytes[63] = i + 1;
        s.pending_payouts.insert(Hash64::from_bytes(bytes), PalwPayoutV2 { payload: h64(0xD000 + i as u64), amount: 1 });
    }
    s.assert_internal_consistency(&p).unwrap();
    assert_eq!(s.exec_v2_reward_release_keys_v1(103, true), 6);
    let first = step(&s, &p, 10, 103, &[]).unwrap().0;
    assert_eq!(first.exec_v2_reward_vesting_v1(&root).unwrap().legs.len(), 10);
    assert_eq!(first.pending_payouts_iter().count(), 8);
    assert_eq!(first.vesting_counters().moved, 600);
    let mut s = first;
    for i in 1..=3 {
        let predicted = s.exec_v2_reward_release_keys_v1(103 + i, true);
        let next = step(&s, &p, 10 + i, 103 + i, &[]).unwrap().0;
        assert_eq!(next.pending_payouts_iter().count(), predicted);
        assert!(predicted <= 8);
        s = next;
    }
    assert!(s.exec_v2_reward_vesting_v1(&root).is_none());
    assert!(s.pending_payouts_iter().next().is_none());
    assert_eq!(s.vesting_counters().moved, total);
    assert_eq!(s.vesting_counters().burned, 0);
}

#[test]
fn closed_unpaid_receipts_reserve_slots_before_a_new_root_can_commit_to_final() {
    for count in [PALW_EXEC_V2_MAX_OPEN_ROOTS - 1, PALW_EXEC_V2_MAX_OPEN_ROOTS] {
        let (p, mut s, claim) = world(Some(0));
        let past = s.last_point.as_ref().unwrap().daa_score - 1;
        // Isolate the quota using closed, zero-value receipts awaiting the next step 3d cleanup.
        for n in 0..count {
            s.exec_v2_rewards.insert(
                h64(0xF000 + n as u64),
                PalwExecV2RewardRowV1 { final_daa: past, liability_until: past, allocation_sompi: 0, legs: BTreeMap::new() },
            );
        }
        s.assert_internal_consistency(&p).unwrap();
        if count == PALW_EXEC_V2_MAX_OPEN_ROOTS {
            assert!(
                matches!(open(&p, &s, decl(claim)).unwrap_err(), PalwStateV2Error::ExecV2Refused(reason) if reason == PalwWorkRootRefusalV2::RootQuota.to_string())
            );
        } else {
            open(&p, &s, decl(claim)).expect("the last reserved execution slot can be admitted");
        }
    }
}

#[test]
fn a_corrupted_ledger_is_caught_by_the_consistency_check() {
    let (p, s, claim) = opened();
    let s = fold_slices(&p, &s, 50, 120, vec![slice_of(&s, claim, 0, bond_key(20))]);
    s.assert_internal_consistency(&p).expect("(control)");
    let corrupt = |f: &dyn Fn(&mut PalwChainStateV2)| {
        let mut bad = s.clone();
        f(&mut bad);
        bad.assert_internal_consistency(&p).expect_err("the corruption is caught")
    };
    // A counter that disagrees with the rows.
    corrupt(&|x| x.exec_v2.roots.get_mut(&claim).unwrap().accepted_work += 1);
    corrupt(&|x| x.exec_v2.roots.get_mut(&claim).unwrap().pending = 0);
    corrupt(&|x| x.exec_v2.roots.get_mut(&claim).unwrap().next_index = 2);
    // A slice row that is not the plan's range, or has no root, or whose boundary is not the root's.
    corrupt(&|x| x.exec_v2.slices.get_mut(&(claim, 0)).unwrap().range.end = 399);
    corrupt(&|x| {
        let row = x.exec_v2.slices.get(&(claim, 0)).unwrap().clone();
        x.exec_v2.slices.insert((h64(0xBAD), 0), row);
    });
    corrupt(&|x| x.exec_v2.roots.get_mut(&claim).unwrap().last_state_root = h64(0xBAD));
    // A job row that does not point back, and a root that outlives its claim.
    corrupt(&|x| {
        let job = x.exec_v2.roots.get(&claim).unwrap().job_work_id;
        x.exec_v2.jobs.insert(job, h64(0xBAD));
    });
    corrupt(&|x| {
        x.claims.remove(&claim);
    });
    // An exposure the rows do not explain (the ledger is re-derived from them).
    corrupt(&|x| {
        x.reserved_exposure.insert(bond_key(20), 1);
    });
    corrupt(&|x| {
        x.reserved_exposure.remove(&bond_key(20));
    });
}

#[test]
fn two_branches_hold_independent_ledgers_and_each_credits_a_slice_once() {
    let (p, s0, claim) = opened();
    let carrier_x = slice_of(&s0, claim, 0, bond_key(20));
    let mut carrier_y = carrier_x.clone();
    carrier_y.carrier = h64(0x5FFF);
    carrier_y.slice.result_state_root = h64(0x61FF);
    let on_x = fold_slices(&p, &s0, 50, 120, vec![carrier_x.clone()]);
    let on_y = fold_slices(&p, &s0, 50, 120, vec![carrier_y.clone()]);
    assert_ne!(on_x.state_root(), on_y.state_root());
    assert_eq!(on_x.exec_v2_slice_v1(&claim, 0).unwrap().carrier, h64(0x5000));
    assert_eq!(on_y.exec_v2_slice_v1(&claim, 0).unwrap().carrier, h64(0x5FFF));
    // The same slice re-accepted on the other branch is credited once THERE (a ledger is branch-local).
    let x_then = fold_slices(&p, &on_x, 51, 121, vec![slice_of(&on_x, claim, 1, bond_key(21))]);
    assert_eq!(x_then.exec_v2_root_v1(&claim).unwrap().accepted_work, 480);
    assert_eq!(on_y.exec_v2_root_v1(&claim).unwrap().accepted_work, 240, "the other branch is untouched");
    // Reorg: revert the sibling's delta and apply the winner's — bit-identical to building the winner fresh.
    let (_, delta_x) = step_with(&s0, &p, 50, 120, &[], &xx_slices(vec![carrier_x])).unwrap();
    let (_, delta_y) = step_with(&s0, &p, 50, 120, &[], &xx_slices(vec![carrier_y])).unwrap();
    let back = revert_delta_v2(&on_y, &delta_y, &p).unwrap();
    assert_eq!(back.state_root(), s0.state_root(), "the losing branch reverts exactly");
    assert_eq!(apply_delta_v2(&back, &delta_x, &p).unwrap().state_root(), on_x.state_root(), "and the winner applies exactly");
}

#[test]
fn the_exec_v2_tail_is_pinned_at_0xee_and_the_empty_carriage_is_unchanged() {
    assert_eq!(PALW_CARRIAGE_EXEC_V2_TAIL_V1, 0xEE);
    let taken = [
        PALW_CARRIAGE_MODEL_TAIL_V1,
        PALW_CARRIAGE_CLASS_COURT_WINDOWS_TAIL_V1,
        PALW_CARRIAGE_VERTEX_TAIL_V1,
        PALW_CARRIAGE_TIR_SHARD_TAIL_V1,
        PALW_CARRIAGE_FLOOR_STATE_TAIL_V1,
        PALW_CARRIAGE_SEAT_AVAILABILITY_TAIL_V1,
        PALW_CARRIAGE_SEAT_ROOT_READINESS_TAIL_V1,
        PALW_CARRIAGE_IMPROVEMENT_EARNINGS_TAIL_V1,
    ];
    assert!(!taken.contains(&PALW_CARRIAGE_EXEC_V2_TAIL_V1));
    // A fresh chain's carriage carries no exec tail, so its bytes are the bytes it had before the field existed.
    let (_, s, _) = world(Some(0));
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
    assert!(!carriage_has_tail(&bytes, 0xEE));
    // With a session the tail is the last thing written and the carriage reloads.
    let (p, opened, _) = opened();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&opened)).unwrap();
    assert!(carriage_has_tail(&bytes, 0xEE));
    let back = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&p, Some(opened.state_root())).unwrap();
    assert_eq!(back.exec_v2_state_v1(), opened.exec_v2_state_v1());
    // A repeated or truncated exec tail is not a carriage.
    let mut twice = bytes.clone();
    let mut tail = vec![0xEEu8];
    tail.extend(borsh::to_vec(opened.exec_v2_state_v1()).unwrap());
    twice.extend(tail);
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&twice).is_err(), "a second 0xEE tail is refused");
    assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes[..bytes.len() - 3]).is_err(), "a truncated tail is refused");
}

/// **A tag-130 carrier is judged at admission exactly as the build without RFC-0008 v2 judges its bytes** (the X8R review). That build —
/// the live testnet-12 release — cannot decode tag 130: it tolerates the payload where the ruleset tolerates undecodable lifecycle
/// payloads (testnet-12 declares the audit fence) and refuses it where it does not. A well-formed and a malformed declaration alike: a
/// ride-time shape check would refuse a block the older build accepts, a split before the fence. Modelled here by the same payload with
/// its object tag replaced by one no build decodes.
#[test]
fn a_tag_130_carrier_rides_exactly_as_an_undecodable_payload() {
    use crate::palw_lifecycle_objects_v2::{
        PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxError, PalwLifecycleTxPayloadV2, palw_lifecycle_object_may_ride_v2,
        validate_palw_lifecycle_tx,
    };
    let mut malformed = decl(h64(1));
    malformed.boundaries.clear();
    assert!(malformed.validate_shape().is_err(), "a declaration with no plan is malformed");
    let tag_at = borsh::to_vec(&PALW_LIFECYCLE_TX_VERSION_V2).unwrap().len();
    for declaration in [decl(h64(1)), malformed] {
        let object = open_object(declaration);
        assert_eq!(palw_lifecycle_object_may_ride_v2(&object), Ok(()), "rides unjudged");
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        assert_eq!(payload[tag_at], 130);
        let mut unknown = payload.clone();
        unknown[tag_at] = 0xFF;
        assert!(borsh::from_slice::<PalwLifecycleTxPayloadV2>(&unknown).is_err(), "an object tag no build decodes");
        for tolerate in [true, false] {
            assert_eq!(
                validate_palw_lifecycle_tx(&payload, tolerate),
                validate_palw_lifecycle_tx(&unknown, tolerate),
                "tolerate = {tolerate}: the older build's verdict"
            );
        }
        assert_eq!(validate_palw_lifecycle_tx(&payload, true), Ok(()));
        assert_eq!(validate_palw_lifecycle_tx(&payload, false), Err(PalwLifecycleTxError::Undecodable));
    }
}

#[test]
fn the_object_and_delta_numbers_are_the_allocated_ones() {
    let declaration = decl(h64(1));
    let object = open_object(declaration);
    assert_eq!(borsh::to_vec(&object).unwrap()[0], 130, "tag 130 (the lead's allocation 130-139)");
    let entry = PalwDeltaEntryV2::ExecV2Row { table: PALW_EXEC_V2_TABLE_ROOTS_V1, key: Vec::new(), old: None, new: None };
    assert_eq!(borsh::to_vec(&entry).unwrap()[0], 180, "delta 180 (the lead's allocation 180-189)");
    assert_eq!(borsh::to_vec(&PalwVoidReasonV2::WorkRootExpired).unwrap(), vec![130], "void reason 130, explicit (the range 130-139)");
    assert_eq!(borsh::to_vec(&PalwVoidReasonV2::PanelUnavailable).unwrap(), vec![10], "(the neighbour did not move)");
}

#[test]
fn a_root_declaration_is_signed_by_the_claims_bond_for_this_network_and_by_no_other_key() {
    let (_, s, claim) = world(Some(0));
    let network = h64(999);
    let bond = s.bond(&s.claim(&claim).unwrap().bond).unwrap().clone();
    let mut d = decl(claim);
    // A mock signer: the message in the first 64 bytes, valid under one key and the root context only.
    let sign = |d: &PalwWorkRootDeclarationV2, net: Hash64| {
        let mut signature = vec![0u8; 80];
        signature[..64].copy_from_slice(d.signing_message(net).as_byte_slice());
        signature
    };
    let key = bond.pubkey.clone();
    let verify = move |k: &[u8], m: &[u8], sig: &[u8], ctx: &[u8]| {
        k == key.as_slice() && ctx == PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT && sig.len() >= 64 && &sig[..64] == m
    };
    d.signature = sign(&d, network);
    assert_eq!(palw_work_root_verify_signature_v2(&s, network, &d, &verify), Ok(()));
    // Another network, a changed field, another claim's bond, an unknown claim: each is not a valid declaration.
    assert_eq!(palw_work_root_verify_signature_v2(&s, h64(1000), &d, &verify), Err(PalwWorkRootRefusalV2::NotSigned));
    let mut tampered = d.clone();
    tampered.total_work += 0;
    tampered.expiry_daa += 1;
    assert_eq!(palw_work_root_verify_signature_v2(&s, network, &tampered, &verify), Err(PalwWorkRootRefusalV2::NotSigned));
    let wrong_key = |_: &[u8], m: &[u8], sig: &[u8], ctx: &[u8]| ctx == PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT && &sig[..64] == m && false;
    assert_eq!(palw_work_root_verify_signature_v2(&s, network, &d, &wrong_key), Err(PalwWorkRootRefusalV2::NotSigned));
    let mut unknown = d.clone();
    unknown.root_claim_id = h64(0xBAD);
    assert_eq!(palw_work_root_verify_signature_v2(&s, network, &unknown, &verify), Err(PalwWorkRootRefusalV2::NoClaim));
    // The signature is under the root's own ML-DSA context, not the permit's or the slice's.
    assert_ne!(PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, PALW_EXEC_V2_SLICE_MLDSA87_CONTEXT);
    assert_ne!(PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, PALW_EXEC_V2_TX_MLDSA87_CONTEXT);
    assert_ne!(PALW_EXEC_V2_ROOT_SIGNING_DOMAIN, PALW_EXEC_V2_ROOT_DECL_DOMAIN);
}

// =============================================================================================
// The anchored set
// =============================================================================================

fn anchor_fold(span_now: u64, members: &[(u64, u64)]) -> crate::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1 {
    crate::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1 {
        members: members.iter().map(|(b, span)| (h64(*b), *span)).collect(),
        span_now,
    }
}

fn anchor_step(
    p: &PalwStateParamsV2,
    s: &PalwChainStateV2,
    word: u64,
    daa: u64,
    fold: crate::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let extras = PalwTransitionExtrasV1 { exec_v2_anchor: Some(fold), ..xx() };
    step_with(s, p, word, daa, &[], &extras).map(|(state, _)| state)
}

#[test]
fn an_anchor_records_what_it_covered_once_and_the_window_drops_the_old() {
    let (p, s0, _) = world(Some(0));
    let s1 = anchor_step(&p, &s0, 50, 120, anchor_fold(5, &[(0xA1, 4), (0xA2, 5)])).unwrap();
    assert!(s1.exec_v2_anchored_v1(&h64(0xA1)) && s1.exec_v2_anchored_v1(&h64(0xA2)));
    assert!(!s1.exec_v2_state_v1().is_empty(), "an anchored block is a row: the table is rooted and carried");
    // A block covered by an earlier anchor cannot be covered again.
    let again = anchor_step(&p, &s1, 51, 121, anchor_fold(5, &[(0xA1, 4)])).expect_err("covered once");
    assert!(again.to_string().contains("earlier anchor"), "{again}");
    // A block outside the window is refused by the ledger's own guard (the closure never offers one).
    assert!(
        anchor_step(&p, &s1, 51, 121, anchor_fold(8, &[(0xA3, 5)]))
            .expect_err("outside the window")
            .to_string()
            .contains("outside the window")
    );
    // Two spans on, the older entries are dropped by the next anchoring block; the newer one stays inside the window.
    let s2 = anchor_step(&p, &s1, 51, 130, anchor_fold(6, &[(0xA4, 6)])).unwrap();
    assert!(!s2.exec_v2_anchored_v1(&h64(0xA1)), "span 4 is outside the window of span 6");
    assert!(s2.exec_v2_anchored_v1(&h64(0xA2)), "span 5 is the span before");
    assert!(s2.exec_v2_anchored_v1(&h64(0xA4)));
    // Below the fence an anchor is a fold error by name, and no row is written.
    let (pd, sd, _) = world(None);
    let err = anchor_step(&pd, &sd, 50, 120, anchor_fold(5, &[(0xA1, 5)])).expect_err("dormant");
    assert!(err.to_string().contains("before palw_exec_payload_v2"), "{err}");
}

// =============================================================================================
// Amendment 1 (spec §10): the verification route is the G14 kernel route
// =============================================================================================

mod amendment_1 {
    use super::*;
    use crate::palw_attempt_rules_v1::palw_attempt_output_root_v1;
    use crate::palw_exec_v2_verify::{
        palw_exec_v2_kernel_key_v1, palw_exec_v2_prefix_job_nonce_v1, palw_exec_v2_real_job_v1, palw_exec_v2_retention_terms_v1,
        palw_exec_v2_slice_job_nonce_v2, palw_exec_v2_token_state_root_v1,
    };
    use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_policy_v1};
    use crate::palw_onboarding_v1::{KernelBindingRowV1, PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1};
    use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
    use misaka_palw_kernel::ledger::{ClaimBodyV1, ClaimRowV1};
    use misaka_palw_kernel::lifecycle::{ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
    use misaka_palw_kernel::rows::{TABLE_CLAIMS_V1, TABLE_JOBS_V1};

    /// The kernel class the claim's V2 class is bound to (onboarding tag 106).
    const KERNEL_CLASS: u64 = 0x4C1A55;
    /// The plan root of that binding: `decl`'s.
    const PLAN: u64 = 0x32;

    /// The route state, with (`bind`) or without the claim's class bound to [`KERNEL_CLASS`] under `plan`.
    fn with_route(s: &mut PalwChainStateV2, claim: Hash64, bind: Option<u64>) {
        let class = s.claim(&claim).unwrap().class_id;
        let mut route = PalwKernelRouteStateV1::new(palw_kernel_route_policy_v1(h64(0xE7), h64(0xE8)), None, Default::default());
        if let Some(plan) = bind {
            let binding = KernelBindingRowV1 {
                kernel_class: h64(KERNEL_CLASS),
                kernel_param_root: h64(0xE9),
                plan_root: h64(plan),
                challenge_policy_id: h64(0xEA),
                binder: bond_key(20),
                bound_daa: 100,
            };
            route
                .aux
                .insert((PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1, borsh::to_vec(&class).unwrap()), borsh::to_vec(&binding).unwrap());
        }
        s.kernel_route = Some(route);
    }

    /// GAP-62: the REAL claim's run that the root bond's prefix claim carries.
    const PREFIX_RUN: [u32; 1] = [5];

    /// **GAP-62: the root bond's prefix claim** — the root claim's class stood in as an IR class (`test_row_v1`), the claim's output
    /// root the commitment of [`PREFIX_RUN`] under its anchored job, and the prefix claim itself (the job carrying the prefix job nonce
    /// and the anchored prompt, the root bond's run) in the route.
    struct Prefix {
        claim_id: Hash64,
        job: KernelJobV1,
        row: ClaimRowV1,
        /// The session's stream so far: the anchored prompt and the run.
        stream: Vec<u32>,
        boundaries: Vec<u64>,
        input_root: Hash64,
    }

    impl Prefix {
        fn new(s: &mut PalwChainStateV2, claim: Hash64, state: ClaimStateV1, rewarded: bool) -> Self {
            let class = s.claim(&claim).unwrap().class_id;
            // A registry seam, but with actual canonical IR bytes and their commitment:
            // every block below also tests the production carriage's restart validation.
            let mut record = crate::palw_tir_admission_v1::PalwTirClassRecordV1::test_row_v1(class);
            record.program = std::sync::Arc::new(misaka_palw_tir_sketch::fixture::dense_moe_v1(1).program.encode());
            record.program_bytes = record.program.len().try_into().unwrap();
            record.graph_ir_root = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&record.program);
            s.test_insert_tir_class_v1(class, record);
            let facts = s.tir_class_v1(&class).unwrap().facts.clone();
            let root_claim = s.claim(&claim).unwrap().clone();
            let (prompt, context) = palw_exec_v2_real_job_v1(&facts, &root_claim.job_identity, xx_declare().prompt_ids_form_v1())
                .expect("the anchored job");
            s.claims.get_mut(&claim).unwrap().output_root = palw_attempt_output_root_v1(&context, &PREFIX_RUN);
            let mut d = decl(claim);
            let program = misaka_palw_tir::TirProgramV1::decode_canonical(&s.tir_class_v1(&class).unwrap().program).unwrap();
            let plan = crate::palw_exec_work_plan_v1::palw_exec_work_plan_v1(
                &program,
                crate::palw_exec_work_plan_v1::PalwExecWorkCostV1::AllNodesV1,
                facts.max_context,
                prompt.len() as u32,
                context.exact_decode_tokens,
                &[3, 5, 7],
            )
            .unwrap();
            d.boundaries = plan.boundaries().to_vec();
            d.total_work = plan.total_work();
            let input_root = palw_exec_v2_token_state_root_v1(&[&prompt]);
            let job = KernelJobV1 {
                class_binding_id: digest(h64(KERNEL_CLASS)),
                prompt: prompt.clone(),
                max_new_tokens: PREFIX_RUN.len() as u32,
                decode: DecodeRuleV1::Greedy,
                nonce: digest(palw_exec_v2_prefix_job_nonce_v1(&claim, d.boundaries[0], &d.canonical_job_id, &d.plan_root)),
            };
            let kernel_bond = palw_kernel_bond_id_v1(&root_claim.bond);
            let (evidence, commitments) = provider_material_fixture(PREFIX_RUN.len(), 0xDF);
            let kclaim = KernelClaimV1 {
                job_id: job.id(),
                producer_bond: kernel_bond,
                generated: PREFIX_RUN.to_vec(),
                evidence_root: evidence.root(),
            };
            let liability_until = match &state {
                ClaimStateV1::Final { final_daa } => Some(final_daa + s.kernel_route.as_ref().unwrap().header.policy.liability_daa),
                _ => None,
            };
            let row = ClaimRowV1 {
                producer: kernel_bond,
                class_binding_id: digest(h64(KERNEL_CLASS)),
                job_id: job.id(),
                body: ClaimBodyV1::Program { claim: kclaim.clone(), evidence, commitments },
                committed_daa: 100,
                sealed_daa: 100,
                life: ClaimLifecycleV1 {
                    policy: LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 },
                    state,
                    retention_met: true,
                    final_hold_until: 0,
                },
                reserved: 1_000,
                liability_until,
                convicted: false,
                rewarded,
            };
            let mut stream = prompt;
            stream.extend_from_slice(&PREFIX_RUN);
            let prefix = Prefix { claim_id: Hash64::from_bytes(kclaim.id()), job, row, stream, boundaries: d.boundaries, input_root };
            prefix.install(s);
            prefix
        }

        /// Put the job and the claim into the route (setup surgery).
        fn install(&self, s: &mut PalwChainStateV2) {
            let route = s.kernel_route.as_mut().expect("a route");
            route.ledger_cache.clear();
            route.rows.insert((TABLE_JOBS_V1, borsh::to_vec(&self.job.id()).unwrap()), borsh::to_vec(&self.job).unwrap());
            route.rows.insert((TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&self.claim_id)), borsh::to_vec(&self.row).unwrap());
        }

        /// The claim row with `state` (and `convicted`), as a kernel row write of a block.
        fn write(&self, state: ClaimStateV1, convicted: bool) -> (u8, Vec<u8>, Option<Vec<u8>>) {
            let mut row = self.row.clone();
            row.life.state = state;
            row.convicted = convicted;
            (TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&self.claim_id), Some(borsh::to_vec(&row).unwrap()))
        }

        /// The declaration that opens on this prefix: it names the claim and starts from `token_state(prompt ‖ run)`.
        fn decl(&self, claim: Hash64) -> PalwWorkRootDeclarationV2 {
            let mut d = decl(claim);
            d.boundaries = self.boundaries.clone();
            d.total_work = *self.boundaries.last().unwrap();
            d.input_root = self.input_root;
            d.prefix_claim = self.claim_id;
            d.initial_state_root = palw_exec_v2_token_state_root_v1(&[&self.stream]);
            d
        }
    }

    fn digest(h: Hash64) -> [u8; 64] {
        h.as_bytes()
    }

    /// One slice's kernel claim, as the route would hold it: the job (the slice's nonce, `prompt`), the program claim by `executor`'s
    /// kernel bond generating `generated`, in `state`, holding `reserved`. Returns the covered slice (bound to it) and the two rows.
    struct KernelSlice {
        covered: PalwExecV2CoveredSliceV1,
        claim_id: Hash64,
        job: KernelJobV1,
        row: ClaimRowV1,
    }

    impl KernelSlice {
        fn new(
            p: &PalwStateParamsV2,
            s: &PalwChainStateV2,
            root: Hash64,
            index: u32,
            executor: PalwBondKeyV2,
            prompt: &[u32],
            generated: &[u32],
            reserved: u64,
        ) -> Self {
            let mut covered = slice_of(s, root, index, executor);
            let terms = palw_exec_v2_retention_terms_v1(
                s.exec_v2_root_v1(&root).unwrap().expiry_daa,
                s.exec_v2_root_v1(&root).unwrap().executor_exposure,
                p.window_court(),
                &s.kernel_route.as_ref().unwrap().header.policy,
            )
            .unwrap();
            let job = KernelJobV1 {
                class_binding_id: digest(h64(KERNEL_CLASS)),
                prompt: prompt.to_vec(),
                max_new_tokens: generated.len() as u32,
                decode: DecodeRuleV1::Greedy,
                nonce: digest(palw_exec_v2_slice_job_nonce_v2(&covered.slice, &terms)),
            };
            let kernel_bond = palw_kernel_bond_id_v1(&executor);
            let (evidence, commitments) = provider_material_fixture(generated.len(), 0xE0 + index as u64);
            let claim = KernelClaimV1 {
                job_id: job.id(),
                producer_bond: kernel_bond,
                generated: generated.to_vec(),
                evidence_root: evidence.root(),
            };
            let row = ClaimRowV1 {
                producer: kernel_bond,
                class_binding_id: digest(h64(KERNEL_CLASS)),
                job_id: job.id(),
                body: ClaimBodyV1::Program { claim: claim.clone(), evidence, commitments },
                committed_daa: 100,
                sealed_daa: 100,
                life: ClaimLifecycleV1 {
                    policy: LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 },
                    state: ClaimStateV1::Challengeable { since_daa: 100, window_end_daa: 200 },
                    retention_met: true,
                    final_hold_until: 0,
                },
                reserved,
                liability_until: None,
                convicted: false,
                rewarded: false,
            };
            let claim_id = Hash64::from_bytes(claim.id());
            let slice = &mut covered.slice;
            slice.evidence_root = claim_id;
            slice.predecessor_state_root = palw_exec_v2_token_state_root_v1(&[prompt]);
            slice.input_root = slice.predecessor_state_root;
            slice.result_state_root = palw_exec_v2_token_state_root_v1(&[prompt, generated]);
            slice.output_root = palw_exec_v2_token_state_root_v1(&[generated]);
            slice.da_root = Hash64::from_bytes(claim.evidence_root);
            KernelSlice { covered, claim_id, job, row }
        }

        /// Put the job and the claim (as it now stands) into the route — state surgery for the setup, outside any block.
        fn install(&self, s: &mut PalwChainStateV2) {
            let route = s.kernel_route.as_mut().expect("a route");
            route.ledger_cache.clear();
            route.rows.insert((TABLE_JOBS_V1, borsh::to_vec(&self.job.id()).unwrap()), borsh::to_vec(&self.job).unwrap());
            route.rows.insert((TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&self.claim_id)), borsh::to_vec(&self.row).unwrap());
        }

        /// The claim row with `state` (and `convicted`), as a kernel row write of a block.
        fn write(&self, state: ClaimStateV1, convicted: bool) -> (u8, Vec<u8>, Option<Vec<u8>>) {
            self.write_with(|row| {
                row.life.state = state;
                row.convicted = convicted;
            })
        }

        /// The claim row as `edit` leaves it, as a kernel row write of a block.
        fn write_with(&self, edit: impl FnOnce(&mut ClaimRowV1)) -> (u8, Vec<u8>, Option<Vec<u8>>) {
            let mut row = self.row.clone();
            edit(&mut row);
            (TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&self.claim_id), Some(borsh::to_vec(&row).unwrap()))
        }
    }

    fn evidence() -> misaka_palw_kernel::evidence::VerificationEvidenceV1 {
        use misaka_palw_kernel::evidence::{EvidenceHeaderV1, SuiteParamsV1, VerificationEvidenceV1};
        VerificationEvidenceV1 {
            version: 1,
            header: EvidenceHeaderV1 {
                network_domain: [1; 64],
                ruleset_digest: [2; 64],
                class_binding_id: digest(h64(KERNEL_CLASS)),
                program_root: [3; 64],
                artifact_root: [4; 64],
                plan_root: digest(h64(PLAN)),
            },
            job_input_root: [5; 64],
            positions: 1,
            initial_state_root: [6; 64],
            final_state_root: [7; 64],
            trace_root: [8; 64],
            output_root: [9; 64],
            segments: Vec::new(),
            suite: SuiteParamsV1 {
                descriptor_digest: [10; 64],
                checker_suite_id: 1,
                challenge_policy_id: 1,
                repetitions: 1,
                target_bits: 2,
                field_bits: 61,
            },
        }
    }

    /// Material commitments for the seeded kernel rows. These bind actual tensor bytes
    /// before the root opens, so a public provider challenge names an existing position.
    /// Computation admission/Final and signatures remain the documented fold fixture seams;
    /// this helper is not an end-to-end execution or honest-trace proof.
    fn provider_material_fixture(
        positions: usize,
        tag: u64,
    ) -> (misaka_palw_kernel::evidence::VerificationEvidenceV1, Vec<Vec<Vec<[u8; 64]>>>) {
        let tensor = misaka_palw_tir::Tensor::new(misaka_palw_tir::DType::I64, vec![1], vec![tag as i128]).unwrap();
        let commitments = vec![vec![vec![misaka_palw_kernel::trace::tensor_commitment(&tensor)]]; positions];
        let mut ev = evidence();
        ev.positions = positions as u32;
        ev.trace_root = misaka_palw_kernel::trace::EvidenceV1::new(commitments.clone()).root();
        (ev, commitments)
    }

    /// Each slice's run of the session's token stream.
    const RUNS: [[u32; 2]; 3] = [[1, 2], [3, 4], [6, 7]];

    /// Slice `index`'s prompt: the stream after the prefix (`base`, the anchored prompt and the REAL claim's run) and the runs before it.
    fn prompt_of(base: &[u32], index: usize) -> Vec<u32> {
        let mut prompt = base.to_vec();
        for run in RUNS.iter().take(index) {
            prompt.extend_from_slice(run);
        }
        prompt
    }

    /// The three slices of the session, chained through one token stream, executed by 20, 21, 20, each claim holding `reserved`.
    /// GAP-62: the root's initial boundary is the prefix claim's stream (the REAL claim's anchored prompt and committed run); the
    /// prefix claim is Final (verified at the declaration) and unrewarded.
    fn session(reserved: u64) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>) {
        let (p, s, claim, slices, _) = session_with(reserved, ClaimStateV1::Final { final_daa: 105 }, false, None);
        (p, s, claim, slices)
    }

    /// [`session`] with the prefix claim in `prefix` (rewarded if `rewarded`) and, if given, the route's `Final` reward set to
    /// `claim_reward` (setup surgery on the route's policy).
    fn session_with(
        reserved: u64,
        prefix: ClaimStateV1,
        rewarded: bool,
        claim_reward: Option<u64>,
    ) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>, Prefix) {
        session_with_params(xp(Some(0)), reserved, prefix, rewarded, claim_reward)
    }

    fn session_with_params(
        params: PalwStateParamsV2,
        reserved: u64,
        prefix: ClaimStateV1,
        rewarded: bool,
        claim_reward: Option<u64>,
    ) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>, Prefix) {
        let (p, mut s, claim) = world_with(params);
        with_route(&mut s, claim, Some(PLAN));
        if let Some(reward) = claim_reward {
            s.kernel_route.as_mut().unwrap().header.policy.claim_reward = reward;
        }
        let pre = Prefix::new(&mut s, claim, prefix, rewarded);
        let mut s = open(&p, &s, pre.decl(claim)).expect("the session opens");
        let executors = [bond_key(20), bond_key(21), bond_key(20)];
        let mut slices = Vec::new();
        for (i, executor) in executors.iter().enumerate() {
            let k = KernelSlice::new(&p, &s, claim, i as u32, *executor, &prompt_of(&pre.stream, i), &RUNS[i], reserved);
            k.install(&mut s);
            slices.push(k);
        }
        // Chain: slice i+1's predecessor is slice i's result, so `slice_of`'s placeholder predecessor is replaced by the token state.
        for (i, k) in slices.iter().enumerate() {
            assert_eq!(k.covered.slice.predecessor_state_root, palw_exec_v2_token_state_root_v1(&[&prompt_of(&pre.stream, i)]));
        }
        (p, s, claim, slices, pre)
    }

    fn carry_all(p: &PalwStateParamsV2, s: &PalwChainStateV2, slices: &[KernelSlice]) -> PalwChainStateV2 {
        fold_slices(p, s, 50, 120, slices.iter().map(|k| k.covered.clone()).collect())
    }

    #[test]
    fn exec_slice_signed_for_one_retention_term_cannot_attach_to_a_longer_fork() {
        let (p, original, native, slices, _) = session_with(1_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        let old = &slices[0];
        let (_, mut fork, same_native) = world_with(p.clone());
        assert_eq!(native, same_native);
        with_route(&mut fork, native, Some(PLAN));
        let pre = Prefix::new(&mut fork, native, ClaimStateV1::Final { final_daa: 105 }, false);
        let mut declaration = pre.decl(native);
        declaration.expiry_daa = original.exec_v2_root_v1(&native).unwrap().expiry_daa + 1;
        let mut fork = open(&p, &fork, declaration).unwrap();
        old.install(&mut fork);
        let job_before = fork.kernel_route.as_ref().unwrap().rows.clone();
        assert_eq!(
            fork.exec_v2_admit_slice_v1(&p, 120, &old.covered, 0),
            Err(PalwSliceRefusalV2::VerificationClaimNotBound("job nonce"))
        );
        assert!(
            crate::palw_exec_v2_verify::palw_exec_v2_slice_statement_v1(
                &fork,
                &p,
                native,
                0,
                old.claim_id,
                old.covered.slice.executor_bond
            )
            .unwrap_err()
            .contains("job nonce")
        );
        let refused = fold_slices(&p, &fork, 50, 120, vec![old.covered.clone()]);
        assert_eq!(refused.exec_v2_root_v1(&native).unwrap().next_index, 0);
        assert_eq!(refused.kernel_route.as_ref().unwrap().rows, job_before, "refusal extends no reservation or fee hold");
        let new = KernelSlice::new(&p, &refused, native, 0, old.covered.slice.executor_bond, &old.job.prompt, &RUNS[0], 1_000);
        assert_ne!(new.job.id(), old.job.id());
        assert_ne!(new.claim_id, old.claim_id, "the worker must sign a new kernel claim");
        let mut renewed = refused;
        new.install(&mut renewed);
        let admitted = fold_slices(&p, &renewed, 51, 121, vec![new.covered.clone()]);
        assert_eq!(admitted.exec_v2_root_v1(&native).unwrap().next_index, 1);
        assert!(admitted.pending_payouts_iter().next().is_none());
        let prepared = crate::palw_exec_v2_verify::palw_exec_v2_slice_job_nonce_at_root_v2(&fork, &p, &native, 0).unwrap();
        assert_eq!(new.job.nonce, prepared.as_bytes());
        let mut stale = admitted.clone();
        // A different native window changes the policy commitment as well as the horizon;
        // a carried old job is not consent to a new network policy on restoration.
        stale.kernel_route.as_mut().unwrap().header.policy.claim_reward += 1;
        assert!(stale.assert_exec_v2_consistency_v1(&p).unwrap_err().to_string().contains("did not authorize its retention terms"));
        assert!(PalwStateCarriageV2::from_state(&stale).into_state(&p, None).is_err());
    }

    #[test]
    fn exec_worker_preflight_preserves_a_second_clock_lock_until_the_actual_depth_is_known() {
        use crate::palw_exec_v2_verify::{palw_exec_v2_slice_statement_at_depth_v2, palw_exec_v2_slice_statement_v1};
        let (p, mut s, native, slices) = session(1_000);
        let worker = slices[0].covered.slice.executor_bond;
        let now = s.last_point().unwrap().daa_score;
        let previous = h64(0xC10C);
        // An old native lock has spent its DAA term but not its settlement count.
        // This read-only fixture isolates the gate; no root/slice consent exists yet.
        s.slashable_locks.insert(
            (worker, previous),
            crate::palw_panel_var_v1::PalwSlashableLockV1 {
                claim: previous,
                amount: u128::from(s.bonds[&worker].collateral),
                expiry_daa: now - 1,
                settled_at_final: 0,
                attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                segments: 0,
            },
        );
        s.settled_attempt_finals = 1;
        s.recent_anchor_daas = vec![now];
        assert_eq!(s.reserved_exposure(&worker), 0);
        assert_eq!(
            s.exec_v2_admit_slice_at_depth_v2(&p, now, &slices[0].covered, 0, Some(5)),
            Err(PalwSliceRefusalV2::ExecutorUnfunded)
        );
        assert!(
            palw_exec_v2_slice_statement_at_depth_v2(&s, &p, native, 0, slices[0].claim_id, worker, Some(5))
                .unwrap_err()
                .contains("lacks Work room")
        );
        assert!(
            palw_exec_v2_slice_statement_v1(&s, &p, native, 0, slices[0].claim_id, worker).unwrap_err().contains("lacks Work room")
        );
        assert!(
            s.exec_v2_admit_slice_at_depth_v2(&p, now, &slices[0].covered, 0, None).is_ok(),
            "known DAA-only clock releases the old lock"
        );
        assert_eq!(
            palw_exec_v2_slice_statement_at_depth_v2(&s, &p, native, 0, slices[0].claim_id, worker, None).unwrap(),
            slices[0].covered.slice
        );
        assert_eq!(s.exec_v2_root_v1(&native).unwrap().next_index, 0, "preflight writes no consent or exposure");
        assert_eq!(s.reserved_exposure(&worker), 0);
    }

    #[test]
    fn exec_retention_read_exposes_the_actual_worker_nonce_and_never_invents_missing_terms() {
        use crate::palw_exec_v2_verify::{palw_exec_v2_observation_v1, palw_exec_v2_observation_with_retention_v2};
        let (p, s, native, slices, _) = session_with(1_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        let read = palw_exec_v2_observation_with_retention_v2(&s, Some(0), h64(1), 110, &[native], None, None, Some(p.window_court()));
        let terms = read.roots[0].retention_terms.unwrap();
        assert_eq!(terms.expiry_daa, s.exec_v2_root_v1(&native).unwrap().expiry_daa);
        assert_eq!(read.roots[0].next_job_nonce.as_deref(), Some(Hash64::from_bytes(slices[0].job.nonce).to_string().as_str()));
        let json: serde_json::Value = serde_json::from_str(&read.to_json()).unwrap();
        assert_eq!(read.job_use_history_total, 1);
        assert_eq!(json["jobUseHistoryTotal"], 1);
        assert_eq!(json["authorizationOriginsTotal"], 1);
        assert_eq!(json["roots"][0]["retentionTerms"]["liabilityUntil"], terms.liability_until);
        assert_eq!(json["roots"][0]["retentionTerms"]["policyRoot"], terms.policy_root.to_string());
        assert_eq!(json["roots"][0]["retentionTerms"]["executorExposureSompi"], terms.executor_exposure_sompi.to_string());
        let unknown = palw_exec_v2_observation_v1(&s, Some(0), h64(1), 110, &[native], None, None);
        assert!(unknown.roots[0].retention_terms.is_none());
        assert!(unknown.roots[0].next_job_nonce.is_none());
    }

    #[test]
    fn a_self_consistent_tiny_kernel_run_cannot_claim_a_larger_ir_execution_cell() {
        let (p, mut s, claim, honest) = session(1_000);
        let tiny = KernelSlice::new(&p, &s, claim, 0, honest[0].covered.slice.executor_bond, &honest[0].job.prompt, &[1], 1_000);
        tiny.install(&mut s);
        let binding = s.kernel_route.as_ref().unwrap().kernel_binding_v1(&tiny.covered.slice.class_id).unwrap();
        // The old token/nonce/class/evidence binding accepts this self-consistent statement.
        // Its committed run is shorter than the root's registered two-token cell.
        crate::palw_exec_v2_verify::palw_exec_v2_verification_binding_v2(
            &tiny.covered.slice,
            &binding,
            &tiny.row,
            Some(&tiny.job),
            &palw_kernel_bond_id_v1(&tiny.covered.slice.executor_bond),
            120,
            &palw_exec_v2_retention_terms_v1(
                s.exec_v2_root_v1(&claim).unwrap().expiry_daa,
                s.exec_v2_root_v1(&claim).unwrap().executor_exposure,
                p.window_court(),
                &s.kernel_route.as_ref().unwrap().header.policy,
            )
            .unwrap(),
        )
        .unwrap();
        let before = s.state_root();
        assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &tiny.covered, 0), Err(PalwSliceRefusalV2::ExecutionScopeMismatch));
        assert!(
            crate::palw_exec_v2_verify::palw_exec_v2_slice_statement_v1(
                &s,
                &p,
                claim,
                0,
                tiny.claim_id,
                tiny.covered.slice.executor_bond
            )
            .is_err(),
            "the producer uses the same execution scope"
        );
        assert_eq!(s.state_root(), before, "no accepted work or payout on refusal");
        assert!(s.exec_v2_admit_slice_v1(&p, 120, &honest[0].covered, 0).is_ok(), "the full cell is the control");
    }

    #[test]
    fn a_root_rejects_fabricated_ir_cost_and_an_input_hash_unrelated_to_its_real_job() {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let prefix = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, false);
        let honest = prefix.decl(claim);
        let mut wrong_cost = honest.clone();
        wrong_cost.boundaries[1] += 1;
        assert!(open(&p, &s, wrong_cost).err().unwrap().to_string().contains("execution work cannot be derived"));
        let mut wrong_input = honest.clone();
        wrong_input.input_root = h64(0xBAD);
        assert!(open(&p, &s, wrong_input).err().unwrap().to_string().contains("canonical job"));
        let opened = open(&p, &s, honest).unwrap();
        assert_eq!(opened.claim(&claim).unwrap().pwu, PWU, "IR execution cost never changes REAL lottery weight");
        assert_ne!(opened.exec_v2_root_v1(&claim).unwrap().prefix_work(), PWU, "the two units are separate");
    }

    #[test]
    fn restart_rederives_the_real_binding_cost_key_and_retained_slice_execution_scope() {
        let (p, s, claim, slices) = session(1_000);
        let s = carry_all(&p, &s, &slices);
        let refuses = |bad: PalwChainStateV2, expected: &str| {
            let carriage = PalwStateCarriageV2::from_state(&bad);
            let error = carriage.into_state(&p, Some(bad.state_root())).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        };
        let mut bad = s.clone();
        bad.exec_v2.roots.get_mut(&claim).unwrap().canonical_job_id = h64(0xBAD);
        refuses(bad, "REAL binding");
        let mut bad = s.clone();
        let old = bad.exec_v2.roots.get(&claim).unwrap().job_work_id;
        bad.exec_v2.jobs.remove(&old);
        bad.exec_v2.jobs.insert(h64(0xBAD), claim);
        bad.exec_v2.roots.get_mut(&claim).unwrap().job_work_id = h64(0xBAD);
        refuses(bad, "useful-work key");
        let mut bad = s.clone();
        let kernel = bad.kernel_route.as_mut().unwrap();
        kernel.ledger_cache.clear();
        let mut job = slices[0].job.clone();
        job.prompt.pop();
        kernel.rows.insert(
            (misaka_palw_kernel::rows::TABLE_JOBS_V1, borsh::to_vec(&slices[0].row.job_id).unwrap()),
            borsh::to_vec(&job).unwrap(),
        );
        refuses(bad, "execution scope");
        let mut bad = s.clone();
        let kernel = bad.kernel_route.as_mut().unwrap();
        kernel.ledger_cache.clear();
        let prefix = s.exec_v2_root_v1(&claim).unwrap().prefix_claim;
        let key = (TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&prefix));
        let mut row: ClaimRowV1 = borsh::from_slice(&kernel.rows[&key]).unwrap();
        row.liability_until = Some(s.exec_v2_root_v1(&claim).unwrap().expiry_daa + kernel.header.policy.liability_daa);
        kernel.rows.insert(key, borsh::to_vec(&row).unwrap());
        refuses(bad, "root window");
    }

    fn kernel_block(
        p: &PalwStateParamsV2,
        s: &PalwChainStateV2,
        word: u64,
        daa: u64,
        rows: Vec<(u8, Vec<u8>, Option<Vec<u8>>)>,
    ) -> PalwChainStateV2 {
        // A kernel court/lifecycle update owns its verdict, not the consumer's already-frozen liability horizon.
        let rows = rows
            .into_iter()
            .map(|(table, key, bytes)| {
                let bytes = if table == TABLE_CLAIMS_V1 {
                    bytes.map(|bytes| {
                        let mut row: ClaimRowV1 = borsh::from_slice(&bytes).unwrap();
                        let old: Option<ClaimRowV1> = s
                            .kernel_route
                            .as_ref()
                            .unwrap()
                            .rows
                            .get(&(table, key.clone()))
                            .and_then(|bytes| borsh::from_slice(bytes).ok());
                        row.liability_until = row.liability_until.max(old.and_then(|row| row.liability_until));
                        borsh::to_vec(&row).unwrap()
                    })
                } else {
                    bytes
                };
                (table, key, bytes)
            })
            .collect();
        let extras = PalwTransitionExtrasV1 { exec_v2_test_kernel_rows: rows, ..xx() };
        step_with(s, p, word, daa, &[], &extras).expect("the block folds").0
    }

    #[test]
    fn a_session_opens_only_on_a_kernel_bound_class_under_its_plan() {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, None);
        let refused = open(&p, &s, decl(claim)).expect_err("no kernel binding");
        assert!(refused.to_string().contains("not bound to a kernel class"), "{refused}");
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(0x99));
        let refused = open(&p, &s, decl(claim)).expect_err("another plan");
        assert!(refused.to_string().contains("kernel verification plan"), "{refused}");
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let prefix = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, false);
        open(&p, &s, prefix.decl(claim)).expect("bound, under its plan, from its prefix");
        // (control) with no kernel route the pre-amendment rule stands: the fixture chains of the other tests open as before.
        let (p, s, claim) = world(Some(0));
        open(&p, &s, decl(claim)).expect("no route: the binding is not asked");
    }

    #[test]
    fn a_slice_must_name_a_kernel_claim_that_binds_it_and_each_mismatch_is_named() {
        let (p, s, claim, slices) = session(1_000);
        let k = &slices[0];
        assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &k.covered, 0).map(|a| a.verified_at), Ok(None), "the honest slice, pending");
        let named = |edit: &dyn Fn(&mut PalwExecV2CoveredSliceV1)| {
            let mut covered = k.covered.clone();
            edit(&mut covered);
            s.exec_v2_admit_slice_v1(&p, 120, &covered, 0).expect_err("refused")
        };
        use PalwSliceRefusalV2::*;
        assert_eq!(named(&|c| c.slice.evidence_root = h64(0xBAD)), VerificationClaimMissing);
        assert_eq!(named(&|c| c.slice.result_state_root = h64(0xBAD)), VerificationClaimNotBound("result"));
        assert_eq!(named(&|c| c.slice.output_root = h64(0xBAD)), VerificationClaimNotBound("output"));
        assert_eq!(named(&|c| c.slice.da_root = h64(0xBAD)), VerificationClaimNotBound("evidence"));
        // Another executor's claim: the slice names bond 21, the claim is 20's.
        let mut other = k.covered.clone();
        other.slice.executor_bond = bond_key(21);
        other.pubkey = s.bond(&bond_key(21)).unwrap().pubkey.clone();
        assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &other, 0), Err(VerificationClaimNotBound("executor")));
        // A claim of slice 1's job named by slice 0: its nonce is another slice's.
        let mut borrowed = k.covered.clone();
        borrowed.slice.evidence_root = slices[1].claim_id;
        borrowed.slice.executor_bond = bond_key(21);
        borrowed.pubkey = s.bond(&bond_key(21)).unwrap().pubkey.clone();
        assert_eq!(s.exec_v2_admit_slice_v1(&p, 120, &borrowed, 0), Err(VerificationClaimNotBound("job nonce")));
        // A predecessor that is not the root's boundary is rule 4's, before the binding.
        assert_eq!(named(&|c| c.slice.predecessor_state_root = h64(0xBAD)), PredecessorMismatch);
        // A failed claim backs nothing.
        let mut failed = s.clone();
        let mut row = k.row.clone();
        row.convicted = true;
        failed
            .kernel_route
            .as_mut()
            .unwrap()
            .rows
            .insert((TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&k.claim_id)), borsh::to_vec(&row).unwrap());
        assert_eq!(failed.exec_v2_admit_slice_v1(&p, 120, &k.covered, 0), Err(VerificationClaimFailed));
        // A claim of another kernel class.
        let mut foreign = s.clone();
        let mut row = k.row.clone();
        row.class_binding_id = digest(h64(0xF0));
        foreign
            .kernel_route
            .as_mut()
            .unwrap()
            .rows
            .insert((TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&k.claim_id)), borsh::to_vec(&row).unwrap());
        assert_eq!(foreign.exec_v2_admit_slice_v1(&p, 120, &k.covered, 0), Err(VerificationClaimNotBound("class")));
        let _ = claim;
    }

    #[test]
    fn a_prefix_claim_cannot_be_reused_by_a_second_root() {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let prefix = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, false);
        let other = h64(0xC1A2);
        s.claims.insert(other, s.claim(&claim).unwrap().clone());
        let before = s.state_root();
        let e = open(&p, &s, prefix.decl(other)).expect_err("another root cannot reuse the prefix job");
        assert!(e.to_string().contains("job nonce"), "{e}");
        assert_eq!(s.state_root(), before);
        assert_eq!(s.exec_v2_counts_v1(), (0, 0, 0));
    }

    #[test]
    fn a_repeated_kernel_final_credits_a_slice_once() {
        let (p, s, claim, slices) = session(1_000);
        let s = carry_all(&p, &s, &slices);
        let row = slices[0].write(ClaimStateV1::Final { final_daa: 130 }, false);
        let once = kernel_block(&p, &s, 60, 130, vec![row.clone()]);
        let twice = kernel_block(&p, &once, 61, 131, vec![row]);
        assert_eq!(once.exec_v2_root_v1(&claim), twice.exec_v2_root_v1(&claim));
        assert_eq!(once.exec_v2_slice_v1(&claim, 0), twice.exec_v2_slice_v1(&claim, 0));
        assert_eq!(twice.exec_v2_root_v1(&claim).unwrap().pending, 2);
        assert_eq!(twice.exec_v2_root_v1(&claim).unwrap().verified_work, slices[0].covered.slice.canonical_range.work().unwrap());
    }

    #[test]
    fn a_final_kernel_claim_verifies_its_slice_and_the_root_settles_once_with_capped_legs() {
        let (p, s, claim, slices) = session(1_000_000_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let root = s.exec_v2_root_v1(&claim).unwrap();
        assert_eq!((root.next_index, root.pending, root.verified_work), (3, 3, 0), "three pending, none verified");
        assert!(deadline_of(&s, claim).is_none(), "the claim's Final is held");
        // The route finalizes slices 0 and 1 in one block: both verified, the root still waits.
        let s = kernel_block(
            &p,
            &s,
            60,
            130,
            vec![
                slices[0].write(ClaimStateV1::Final { final_daa: 130 }, false),
                slices[1].write(ClaimStateV1::Final { final_daa: 130 }, false),
            ],
        );
        assert!(matches!(
            s.exec_v2_slice_v1(&claim, 0).unwrap().stage,
            PalwWorkSliceStageV2::Verified { verified_daa: 130, leg_cap: 1_000_000_000, route_reward: 0 }
        ));
        assert!(matches!(s.exec_v2_slice_v1(&claim, 1).unwrap().stage, PalwWorkSliceStageV2::Verified { .. }));
        assert!(matches!(s.exec_v2_slice_v1(&claim, 2).unwrap().stage, PalwWorkSliceStageV2::Pending));
        assert!(s.exec_v2_holds_final_v1(&claim));
        // A block that writes no claim row moves nothing (the sync is event-driven).
        let quiet = step(&s, &p, 61, 131, &[]).unwrap().0;
        assert_eq!(quiet.exec_v2_root_v1(&claim), s.exec_v2_root_v1(&claim));
        // The last one: the root is ready, the hold released, and the claim reaches Final and settles once.
        let s = kernel_block(&p, &quiet, 62, 132, vec![slices[2].write(ClaimStateV1::Final { final_daa: 132 }, false)]);
        let root = s.exec_v2_root_v1(&claim).unwrap();
        assert!(root.ready_for_final(), "{root:?}");
        assert!(deadline_of(&s, claim).is_some_and(|at| at >= 132));
        let s = run_to_terminal(&p, s, claim, 70, 133);
        assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Settled { .. }));
    }

    #[test]
    fn a_slice_leg_is_capped_at_what_its_claims_held_when_they_verified() {
        // Bond 20's two claims hold 7 each, bond 21's a billion: 20's leg is capped at 14 and the rest stays with the root executor.
        let (p, s, claim, mut slices) = session(1_000_000_000);
        for k in slices.iter_mut().filter(|k| k.covered.slice.executor_bond == bond_key(20)) {
            k.row.reserved = 7;
        }
        let mut s = s;
        for k in &slices {
            k.install(&mut s);
        }
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let finals = slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect();
        let s = kernel_block(&p, &s, 60, 130, finals);
        let s = run_to_terminal(&p, s, claim, 70, 131);
        let paid_to = |bond: PalwBondKeyV2| held_exec_to(&s, claim, bond);
        let producer = s.vesting_row(&claim).expect("the vested row").producer.amount;
        let (leg_a, leg_b) = (paid_to(bond_key(20)), paid_to(bond_key(21)));
        assert_eq!(leg_a, 14, "capped at the collateral the route holds on 20's two claims");
        // The twin's whole producer leg is conserved: what 20 could not be paid stayed with the root executor.
        let (pt, st, claim_t) = world(Some(0));
        let twin = run_to_terminal(&pt, license(&pt, &st, claim_t, 45, 111), claim_t, 70, 111);
        let twin_leg = twin.vesting_row(&claim_t).unwrap().producer.amount;
        assert_eq!(producer + leg_a + leg_b, twin_leg, "nothing minted, nothing lost");
        let root = s.exec_v2_root_v1(&claim).unwrap();
        let work_b = s.exec_v2_slice_v1(&claim, 1).unwrap().range.work().unwrap();
        assert_eq!(leg_b, (twin_leg as u128 * work_b as u128 / root.total_work as u128) as u64, "21's leg is under its cap");
    }

    #[test]
    fn a_convicted_slice_voids_its_suffix_and_the_root_and_charges_the_root_nothing() {
        let (p, s, claim, slices) = session(1_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let s = kernel_block(&p, &s, 60, 130, vec![slices[0].write(ClaimStateV1::Final { final_daa: 130 }, false)]);
        let collateral: Vec<(PalwBondKeyV2, u64)> = s.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
        assert!(s.reserved_exposure(&bond_key(20)) > 0);
        // An outsider's proof convicted slice 1's claim (the route's own G14 case): the slice is proven false, 2 depends on it.
        let s = kernel_block(&p, &s, 61, 131, vec![slices[1].write(ClaimStateV1::Convicted { daa: 131 }, true)]);
        assert!(
            matches!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Verified { .. }),
            "the prefix of the suffix stands"
        );
        assert_eq!(s.exec_v2_slice_v1(&claim, 1).unwrap().stage, PalwWorkSliceStageV2::ProvenFalse { daa: 131 });
        assert_eq!(s.exec_v2_slice_v1(&claim, 2).unwrap().stage, PalwWorkSliceStageV2::Voided { voided_daa: 131 });
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, voided_daa: 131 }));
        assert!(
            matches!(
                s.claim(&claim).unwrap().phase,
                PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceProvenFalse, voided_daa: 131 }
            ),
            "{:?}",
            s.claim(&claim).unwrap().phase
        );
        let after: Vec<(PalwBondKeyV2, u64)> = s.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
        assert_eq!(after, collateral, "no V2 charge: the evidence-bound slash is the route's");
        assert_eq!(s.reserved_exposure(&bond_key(20)), 0, "the extra executors' exposure returns");
        assert!(s.vesting_row(&claim).is_none(), "no prefix or partial payment");
        // Later Finals of the void suffix change nothing.
        let s2 = kernel_block(&p, &s, 62, 132, vec![slices[2].write(ClaimStateV1::Final { final_daa: 132 }, false)]);
        assert_eq!(s2.exec_v2_slice_v1(&claim, 2), s.exec_v2_slice_v1(&claim, 2));
    }

    #[test]
    fn a_defaulted_slice_voids_its_suffix_as_a_default_and_a_verified_one_convicted_later_still_voids_an_unsettled_root() {
        let (p, s, claim, slices) = session(1_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let defaulted = kernel_block(
            &p,
            &s,
            60,
            130,
            vec![slices[0].write(ClaimStateV1::Unavailable { daa: 130, producer_defaulted: true }, false)],
        );
        assert_eq!(defaulted.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Defaulted { daa: 130 });
        assert!(matches!(
            defaulted.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceDefaulted, .. }
        ));
        // Verified, then convicted inside the route's liability horizon, before the root settled: proven false.
        let s = kernel_block(
            &p,
            &s,
            61,
            131,
            slices.iter().take(2).map(|k| k.write(ClaimStateV1::Final { final_daa: 131 }, false)).collect(),
        );
        assert!(matches!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Verified { .. }));
        let s = kernel_block(&p, &s, 62, 132, vec![slices[0].write(ClaimStateV1::Final { final_daa: 131 }, true)]);
        assert_eq!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::ProvenFalse { daa: 132 });
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 0, .. }));
    }

    #[test]
    fn a_slice_whose_claim_is_final_at_admission_is_verified_at_once_and_the_refusal_record_names_every_carrier() {
        let (p, mut s, claim, mut slices) = session(1_000);
        slices[0].row.life.state = ClaimStateV1::Final { final_daa: 115 };
        slices[0].row.liability_until = Some(115 + s.kernel_route.as_ref().unwrap().header.policy.liability_daa);
        slices[0].install(&mut s);
        // Slice 0 (Final already) and a copy of slice 0 by the wrong executor in the same block.
        let mut wrong = slices[0].covered.clone();
        wrong.carrier = h64(0x5FFF);
        wrong.slice.executor_bond = bond_key(21);
        wrong.pubkey = s.bond(&bond_key(21)).unwrap().pubkey.clone();
        let (after, delta) = step_with(&s, &p, 50, 120, &[], &xx_slices(vec![slices[0].covered.clone(), wrong.clone()])).unwrap();
        assert!(matches!(
            after.exec_v2_slice_v1(&claim, 0).unwrap().stage,
            PalwWorkSliceStageV2::Verified { verified_daa: 120, leg_cap: 1_000, route_reward: 0 }
        ));
        let root = after.exec_v2_root_v1(&claim).unwrap();
        assert_eq!(
            (root.pending, root.verified_work),
            (0, slices[0].covered.slice.canonical_range.work().unwrap()),
            "verified at once: never pending"
        );
        let record: Vec<(Hash64, u8)> = delta
            .entries
            .iter()
            .filter_map(|e| match e {
                PalwDeltaEntryV2::ExecV2Verdict { carrier, code } => Some((*carrier, *code)),
                _ => None,
            })
            .collect();
        assert_eq!(
            record,
            vec![(slices[0].covered.carrier, 0), (wrong.carrier, PalwSliceRefusalV2::IndexUsed.code())],
            "every covered carrier, in the fold's order, admitted or refused by name"
        );
        assert_eq!(PalwSliceRefusalV2::name_of_code(PalwSliceRefusalV2::IndexUsed.code()), "index_used");
    }

    /// The legs `run_to_terminal` paid: `(root executor's vested producer leg, bond 20's, bond 21's)`, and the session-free twin's
    /// producer leg — what the three must add up to.
    fn paid_legs(s: &PalwChainStateV2, claim: Hash64) -> (u64, u64, u64, u64) {
        let paid_to = |bond: PalwBondKeyV2| held_exec_to(s, claim, bond);
        let producer = s.vesting_row(&claim).expect("the vested row").producer.amount;
        let (pt, st, claim_t) = world(Some(0));
        let twin = run_to_terminal(&pt, license(&pt, &st, claim_t, 45, 111), claim_t, 70, 111);
        let twin_leg = twin.vesting_row(&claim_t).unwrap().producer.amount;
        (producer, paid_to(bond_key(20)), paid_to(bond_key(21)), twin_leg)
    }

    /// A consumer cannot trust a stale frozen cap when its dependency reservation disappears early.
    #[test]
    fn a_reservation_lost_before_the_root_horizon_defaults_the_root_without_a_payout() {
        let (p, s, claim, slices) = session(1_000_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let s = kernel_block(
            &p,
            &s,
            60,
            130,
            slices.iter().take(2).map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect(),
        );
        let s = kernel_block(
            &p,
            &s,
            61,
            141,
            vec![slices[0].write_with(|row| {
                row.life.state = ClaimStateV1::Final { final_daa: 130 };
                row.liability_until = Some(140);
                row.reserved = 0;
            })],
        );
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
        assert!(matches!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Defaulted { .. }));
        assert!(s.vesting_row(&claim).is_none());
        assert_eq!(s.reserved_exposure(&bond_key(20)), 0);
        assert_eq!(s.reserved_exposure(&bond_key(21)), 0);
    }

    /// Computation Final is unrewarded: the entire held reservation caps the executor's gross share.
    #[test]
    fn a_bound_leg_uses_the_held_reservation_without_a_policy_sized_advance() {
        let (p, s, claim, slices) = session(1_000_000_000_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let finals = slices
            .iter()
            .map(|k| {
                k.write_with(|row| {
                    row.life.state = ClaimStateV1::Final { final_daa: 130 };
                    row.liability_until = Some(330);
                    if k.covered.slice.executor_bond == bond_key(20) {
                        row.reserved = 7;
                    }
                })
            })
            .collect();
        let s = kernel_block(&p, &s, 60, 130, finals);
        assert!(matches!(
            s.exec_v2_slice_v1(&claim, 0).unwrap().stage,
            PalwWorkSliceStageV2::Verified { leg_cap: 7, route_reward: 0, .. }
        ));
        let s = run_to_terminal(&p, s, claim, 70, 131);
        let (producer, leg_a, leg_b, allocation) = paid_legs(&s, claim);
        assert_eq!(leg_a, 14);
        assert_eq!(producer + leg_a + leg_b, allocation);
    }

    /// **A verified slice whose kernel claim forfeits its reservation after Final is defaulted** (the X8R round-2 review): the route
    /// answers a demand left unserved inside the liability horizon by taking the whole reservation and leaving the state `Final`
    /// (`PostFinalDefault`). The material was withheld, so — as a conviction after Final does — it voids the slice's suffix and the
    /// unsettled root (`WorkSliceDefaulted`, uncharged here). A release PAST the horizon is not that (the test above).
    #[test]
    fn a_verified_slice_whose_claim_forfeits_after_final_defaults_and_voids_an_unsettled_root() {
        let (p, s, claim, slices) = session(1_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let finals = slices
            .iter()
            .take(2)
            .map(|k| {
                k.write_with(|row| {
                    row.life.state = ClaimStateV1::Final { final_daa: 130 };
                    row.liability_until = Some(330);
                })
            })
            .collect();
        let s = kernel_block(&p, &s, 60, 130, finals);
        assert!(matches!(s.exec_v2_slice_v1(&claim, 1).unwrap().stage, PalwWorkSliceStageV2::Verified { .. }));
        let forfeit = slices[1].write_with(|row| {
            row.life.state = ClaimStateV1::Final { final_daa: 130 };
            row.liability_until = Some(330);
            row.reserved = 0;
        });
        let s = kernel_block(&p, &s, 61, 131, vec![forfeit]);
        assert!(matches!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Verified { .. }), "the prefix stands");
        assert_eq!(s.exec_v2_slice_v1(&claim, 1).unwrap().stage, PalwWorkSliceStageV2::Defaulted { daa: 131 });
        assert_eq!(s.exec_v2_slice_v1(&claim, 2).unwrap().stage, PalwWorkSliceStageV2::Voided { voided_daa: 131 });
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, voided_daa: 131 }));
        assert!(matches!(
            s.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceDefaulted, voided_daa: 131 }
        ));
        // The admission refuses a forfeited claim as it refuses a failed one.
        let mut row = slices[2].row.clone();
        row.life.state = ClaimStateV1::Final { final_daa: 130 };
        row.liability_until = Some(330);
        row.reserved = 0;
        assert_eq!(
            crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&row, 200),
            crate::palw_exec_v2_verify::PalwSliceVerificationV1::Defaulted
        );
        assert_eq!(
            crate::palw_exec_v2_verify::palw_exec_v2_claim_outcome_v1(&row, 331),
            crate::palw_exec_v2_verify::PalwSliceVerificationV1::Verified { final_daa: 130 },
            "past the horizon a reservation of 0 is the release"
        );
    }

    /// **GAP-62 (X8R round 3): the initial boundary is the REAL claim's output, carried by the root bond's prefix claim.** On a
    /// kernel-bound class a session opens only when the declaration names a kernel claim, by the root bond, of the job carrying the
    /// prefix job nonce, whose prompt is the one the REAL claim's anchor names and whose run is the REAL claim's committed output; the
    /// initial boundary is then `token_state(prompt ‖ run)` and nothing else. Each mismatch is refused by name and writes nothing.
    #[test]
    fn a_bound_session_starts_from_the_real_claims_output_and_each_prefix_mismatch_is_refused_by_name() {
        let setup = || {
            let (p, mut s, claim) = world(Some(0));
            with_route(&mut s, claim, Some(PLAN));
            let prefix = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, false);
            (p, s, claim, prefix)
        };
        let refused = |p: &PalwStateParamsV2, s: &PalwChainStateV2, d: PalwWorkRootDeclarationV2, want: &str| {
            let e = open(p, s, d).expect_err(want).to_string();
            assert!(e.contains(want), "expected `{want}`, got `{e}`");
        };
        // The honest declaration opens, verified at once (its prefix claim is Final), from the prefix's stream.
        let (p, s, claim, prefix) = setup();
        let opened = open(&p, &s, prefix.decl(claim)).expect("the honest prefix binds");
        let root = opened.exec_v2_root_v1(&claim).unwrap();
        assert_eq!(root.prefix_claim, prefix.claim_id);
        assert_eq!(root.prefix, PalwWorkPrefixStageV2::Verified { verified_daa: 110, route_reward: 0 });
        assert_eq!(root.last_state_root, palw_exec_v2_token_state_root_v1(&[&prefix.stream]), "slice 0 continues the REAL output");
        assert_eq!(opened.exec_v2_root_of_kernel_claim_v1(&prefix.claim_id), Some(claim), "BUDGET's H-3a read");
        // No such claim.
        let mut d = prefix.decl(claim);
        d.prefix_claim = h64(0xBAD);
        refused(&p, &s, d, "prefix claim is not held");
        // A boundary the root bond chose: the prompt without the REAL claim's run, or any other root.
        let mut d = prefix.decl(claim);
        d.initial_state_root = palw_exec_v2_token_state_root_v1(&[&prefix.job.prompt]);
        refused(&p, &s, d, "initial boundary is not the token state");
        let mut d = prefix.decl(claim);
        d.initial_state_root = h64(0x33);
        refused(&p, &s, d, "initial boundary is not the token state");
        // The REAL claim committed another output: the prefix claim does not carry its run.
        let mut other = s.clone();
        other.claims.get_mut(&claim).unwrap().output_root = h64(0x0BAD);
        refused(&p, &other, prefix.decl(claim), "its output differs");
        // A prefix claim of another bond.
        let mut other = s.clone();
        let mut row = prefix.row.clone();
        row.producer = palw_kernel_bond_id_v1(&bond_key(20));
        if let ClaimBodyV1::Program { claim: c, .. } = &mut row.body {
            c.producer_bond = row.producer;
        }
        Prefix { row, ..prefix_clone(&prefix) }.install(&mut other);
        refused(&p, &other, prefix.decl(claim), "its executor differs");
        // A job whose prompt is not the anchored one (its nonce is right): the job and the claim are re-made around it.
        let mut other = s.clone();
        let mut job = prefix.job.clone();
        job.prompt[0] ^= 1;
        let rebuilt = rebuild(&prefix, job);
        rebuilt.install(&mut other);
        let mut d = prefix.decl(claim);
        d.prefix_claim = rebuilt.claim_id;
        refused(&p, &other, d, "its prompt differs");
        // A job of another root's prefix (the nonce binds the root, its prefix work, the job and the plan).
        let mut other = s.clone();
        let mut job = prefix.job.clone();
        job.nonce = digest(palw_exec_v2_prefix_job_nonce_v1(&h64(0x5150), PWU, &h64(JOB), &h64(PLAN)));
        let rebuilt = rebuild(&prefix, job);
        rebuilt.install(&mut other);
        let mut d = prefix.decl(claim);
        d.prefix_claim = rebuilt.claim_id;
        refused(&p, &other, d, "its job nonce differs");
        // A failed prefix claim binds nothing.
        let mut other = s.clone();
        let mut row = prefix.row.clone();
        row.convicted = true;
        Prefix { row, ..prefix_clone(&prefix) }.install(&mut other);
        refused(&p, &other, prefix.decl(claim), "prefix claim has failed");
        // A class with no IR record: the REAL claim's job cannot be derived.
        let (p2, mut s2, claim2) = world(Some(0));
        with_route(&mut s2, claim2, Some(PLAN));
        refused(&p2, &s2, prefix.decl(claim2), "cannot be derived");
        // (control) every refusal above wrote nothing: the state is the parent's.
        assert!(s.exec_v2_root_v1(&claim).is_none());
    }

    #[test]
    fn a_prefix_cannot_change_the_real_claims_exact_decode_length_even_with_a_matching_output_commitment() {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let mut prefix = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, false);
        let real = s.claim(&claim).unwrap();
        let facts = &s.tir_class_v1(&real.class_id).unwrap().facts;
        let (_, context) = palw_exec_v2_real_job_v1(facts, &real.job_identity, xx_declare().prompt_ids_form_v1()).unwrap();
        assert_eq!(context.exact_decode_tokens, 1);

        // A self-consistent two-token kernel job and its output hash under the ONE-token REAL context.
        // The output-root function hashes its inputs; it is not an execution-shape validator.
        let generated = vec![5, 6];
        prefix.job.max_new_tokens = generated.len() as u32;
        prefix.row.job_id = prefix.job.id();
        let ClaimBodyV1::Program { claim: kernel_claim, .. } = &mut prefix.row.body else { unreachable!() };
        kernel_claim.job_id = prefix.job.id();
        kernel_claim.generated = generated.clone();
        prefix.claim_id = Hash64::from_bytes(kernel_claim.id());
        prefix.stream = prefix.job.prompt.clone();
        prefix.stream.extend_from_slice(&generated);
        s.claims.get_mut(&claim).unwrap().output_root = palw_attempt_output_root_v1(&context, &generated);
        prefix.install(&mut s);

        let err = open(&p, &s, prefix.decl(claim)).err().expect("a matching hash cannot enlarge the REAL decode").to_string();
        assert!(err.contains("its decode differs"), "{err}");
        assert!(s.exec_v2_root_v1(&claim).is_none());
    }

    fn prefix_clone(p: &Prefix) -> Prefix {
        Prefix {
            claim_id: p.claim_id,
            job: p.job.clone(),
            row: p.row.clone(),
            stream: p.stream.clone(),
            boundaries: p.boundaries.clone(),
            input_root: p.input_root,
        }
    }

    /// The prefix's claim re-made around `job` (same run, same producer).
    fn rebuild(p: &Prefix, job: KernelJobV1) -> Prefix {
        let mut row = p.row.clone();
        row.job_id = job.id();
        let ClaimBodyV1::Program { claim, .. } = &mut row.body else { unreachable!() };
        claim.job_id = job.id();
        let claim_id = Hash64::from_bytes(claim.id());
        Prefix { claim_id, job, row, stream: p.stream.clone(), boundaries: p.boundaries.clone(), input_root: p.input_root }
    }

    /// **GAP-62: a pending prefix holds the root's `Final`; its conviction (an outsider's proof of a lie in the REAL claim's run) voids
    /// the whole session, verified slices included; a default of its material does the same as a default; its `Final` releases the
    /// hold.** Nothing is charged at V2: the evidence-bound slash is the route's, on the root bond's kernel reservation.
    #[test]
    fn a_pending_prefix_holds_the_roots_final_and_its_conviction_or_default_voids_the_whole_session() {
        let (p, s, claim, slices, prefix) =
            session_with(1_000, ClaimStateV1::Challengeable { since_daa: 100, window_end_daa: 200 }, false, None);
        assert_eq!(s.exec_v2_root_v1(&claim).unwrap().prefix, PalwWorkPrefixStageV2::Pending);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let finals = slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect();
        let s = kernel_block(&p, &s, 60, 130, finals);
        let root = s.exec_v2_root_v1(&claim).unwrap();
        assert_eq!((root.pending, root.verified_work), (0, root.total_work - root.prefix_work()), "every slice verified");
        assert!(!root.ready_for_final() && s.exec_v2_holds_final_v1(&claim), "the pending prefix holds the REAL claim's Final");
        assert!(deadline_of(&s, claim).is_none());
        // (a) the prefix verifies: the hold is released.
        let ok = kernel_block(&p, &s, 61, 131, vec![prefix.write(ClaimStateV1::Final { final_daa: 131 }, false)]);
        assert_eq!(ok.exec_v2_root_v1(&claim).unwrap().prefix, PalwWorkPrefixStageV2::Verified { verified_daa: 131, route_reward: 0 });
        assert!(ok.exec_v2_root_v1(&claim).unwrap().ready_for_final() && !ok.exec_v2_holds_final_v1(&claim));
        assert!(deadline_of(&ok, claim).is_some_and(|at| at >= 131));
        // (b) an outsider convicted the prefix claim: the REAL claim's run is false, every slice and the root are void.
        let collateral: Vec<(PalwBondKeyV2, u64)> = s.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
        let convicted = kernel_block(&p, &s, 61, 131, vec![prefix.write(ClaimStateV1::Convicted { daa: 131 }, true)]);
        let root = convicted.exec_v2_root_v1(&claim).unwrap();
        assert_eq!(root.prefix, PalwWorkPrefixStageV2::ProvenFalse { daa: 131 });
        assert_eq!(root.phase, PalwWorkRootPhaseV2::Voided { from_index: 0, voided_daa: 131 });
        for index in 0..3 {
            assert_eq!(convicted.exec_v2_slice_v1(&claim, index).unwrap().stage, PalwWorkSliceStageV2::Voided { voided_daa: 131 });
        }
        assert!(matches!(
            convicted.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceProvenFalse, voided_daa: 131 }
        ));
        let after: Vec<(PalwBondKeyV2, u64)> = convicted.bonds.iter().map(|(k, b)| (*k, b.collateral)).collect();
        assert_eq!(after, collateral, "no V2 charge");
        assert_eq!(convicted.reserved_exposure(&bond_key(20)), 0, "the extra executors' exposure returns");
        assert!(convicted.vesting_row(&claim).is_none(), "nobody is paid");
        // (c) the prefix claim's material was withheld: a default, the same void.
        let defaulted =
            kernel_block(&p, &s, 61, 131, vec![prefix.write(ClaimStateV1::Unavailable { daa: 131, producer_defaulted: true }, false)]);
        assert_eq!(defaulted.exec_v2_root_v1(&claim).unwrap().prefix, PalwWorkPrefixStageV2::Defaulted { daa: 131 });
        assert!(matches!(
            defaulted.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceDefaulted, .. }
        ));
    }

    /// Install real kernel reservations and their consumer bond mappings. These rows are setup,
    /// and all subsequent Final/void debits, refunds and rewards use the production fold.
    fn fund_jobs(s: &mut PalwChainStateV2, pre: &Prefix, slices: &[KernelSlice], amount: u64) {
        use crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1;
        use misaka_palw_kernel::ledger::{BondRowV1, JobEscrowRowV1};
        use misaka_palw_kernel::rows::{TABLE_BONDS_V1, TABLE_JOB_ESCROWS_V1};
        let mut jobs = vec![pre.job.id()];
        jobs.extend(slices.iter().map(|k| k.job.id()));
        let mut reservations = std::collections::BTreeMap::new();
        reservations.insert(s.claims.values().next().unwrap().bond, pre.row.reserved);
        for slice in slices {
            *reservations.entry(slice.covered.slice.executor_bond).or_insert(0) += slice.row.reserved;
        }
        let poster = bond_key(6);
        *reservations.entry(poster).or_insert(0) += amount * jobs.len() as u64;
        let route = s.kernel_route.as_mut().unwrap();
        route.ledger_cache.clear();
        for (bond, reserved) in reservations {
            assert!(reserved <= COLLATERAL);
            let id = palw_kernel_bond_id_v1(&bond);
            route.aux.insert((PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, borsh::to_vec(&id).unwrap()), borsh::to_vec(&bond).unwrap());
            route.rows.insert(
                (TABLE_BONDS_V1, borsh::to_vec(&id).unwrap()),
                borsh::to_vec(&BondRowV1 { collateral: COLLATERAL, reserved, exit_requested: None }).unwrap(),
            );
        }
        for job in jobs {
            route.rows.insert(
                (TABLE_JOB_ESCROWS_V1, borsh::to_vec(&job).unwrap()),
                borsh::to_vec(&JobEscrowRowV1 { poster: palw_kernel_bond_id_v1(&poster), amount, posted_daa: 105 }).unwrap(),
            );
        }
    }

    #[test]
    fn root_final_funds_vested_job_fees_within_one_allocation_and_refunds_unused_escrow() {
        use misaka_palw_kernel::rows::{TABLE_EXEC_REWARD_HOLDS_V1, TABLE_JOB_ESCROWS_V1};
        for fee in [0, 1_000, 200_000] {
            let (p, mut s, claim, slices, pre) = session_with(1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, Some(fee));
            fund_jobs(&mut s, &pre, &slices, fee);
            let s = license(&p, &s, claim, 45, 111);
            let s = carry_all(&p, &s, &slices);
            let s =
                kernel_block(&p, &s, 60, 130, slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect());
            let before = s.bonds[&bond_key(6)].collateral;
            assert_eq!(before, COLLATERAL, "no poster debit before root Final");
            assert_eq!(s.kernel_route.as_ref().unwrap().rows.keys().filter(|(t, _)| *t == TABLE_JOB_ESCROWS_V1).count(), 4);
            assert_eq!(s.exec_v2_root_v1(&claim).unwrap().prefix.route_reward(), 0);
            let s = run_to_terminal(&p, s, claim, 70, 131);
            let (producer, total_a, total_b, allocation) = paid_legs(&s, claim);
            let root = s.exec_v2_root_v1(&claim).unwrap();
            let mut route_total = root.prefix.route_reward();
            let mut fee_a = 0;
            let mut fee_b = 0;
            for index in 0..3 {
                if let PalwWorkSliceStageV2::Verified { route_reward, .. } = s.exec_v2_slice_v1(&claim, index).unwrap().stage {
                    route_total += route_reward;
                    if index == 1 {
                        fee_b += route_reward;
                    } else {
                        fee_a += route_reward;
                    }
                }
            }
            let work = |index| s.exec_v2_slice_v1(&claim, index).unwrap().range.work().unwrap();
            let share = |work: u64| (allocation as u128 * work as u128 / root.total_work as u128) as u64;
            assert_eq!(total_a, share(work(0) + work(2)));
            assert_eq!(total_b, share(work(1)));
            assert_eq!(producer + root.prefix.route_reward() + total_a + total_b, allocation);
            assert!(fee_a <= total_a && fee_b <= total_b);
            assert_eq!(before - s.bonds[&bond_key(6)].collateral, route_total, "actual debit funds exactly the fee entitlement");
            assert!(s.pending_payouts_iter().next().is_none(), "Final creates no immediately spendable executor or Panel reward");
            assert!(route_total <= 4 * fee);
            assert!(
                s.kernel_route
                    .as_ref()
                    .unwrap()
                    .rows
                    .keys()
                    .all(|(t, _)| *t != TABLE_JOB_ESCROWS_V1 && *t != TABLE_EXEC_REWARD_HOLDS_V1)
            );
        }
    }

    fn funded_final(retirement: u64) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>, Prefix) {
        let (p, s, claim, slices, pre, _) = funded_final_with_delta(retirement);
        (p, s, claim, slices, pre)
    }

    fn provider_terms(
        p: &PalwStateParamsV2,
        s: &PalwChainStateV2,
        claim: Hash64,
        producer: PalwBondKeyV2,
        daa: u64,
        until: u64,
        first_provider: u64,
        transfer: bool,
    ) -> PalwChainStateV2 {
        use crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1;
        use crate::palw_provider_court_v1::{PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1 as MIN, ProviderSubjectV1};
        let mut objects = Vec::new();
        for provider in first_provider..first_provider + 2 {
            objects.push(seat_bond_reg(provider, 2 * MIN));
            objects.push(PalwConsensusObjectV2::ProviderLeaseV1 {
                subject: ProviderSubjectV1::KernelClaim { claim },
                reserved: MIN,
                serve_until_daa: until,
                provider: bond_key(provider),
                signature: vec![9; 8],
            });
        }
        if transfer {
            objects.push(PalwConsensusObjectV2::DaTransferV1 { claim, producer, signature: vec![9; 8] });
        }
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(PalwKernelRouteExtrasV1 {
                network_domain: h64(0xE7),
                ruleset_digest: h64(0xE8),
                provider_court: Some(0),
                ..Default::default()
            }),
            ..xx()
        };
        step_with(s, p, s.last_point.unwrap().blue_score + 1, daa, &objects, &extras).unwrap().0
    }

    fn provider_extras() -> PalwTransitionExtrasV1 {
        use crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1;
        PalwTransitionExtrasV1 {
            kernel_route: Some(PalwKernelRouteExtrasV1 {
                network_domain: h64(0xE7),
                ruleset_digest: h64(0xE8),
                provider_court: Some(0),
                ..Default::default()
            }),
            ..xx()
        }
    }

    #[test]
    fn exec_transferred_post_final_provider_court_defaults_the_root_without_charging_its_workers() {
        use crate::palw_provider_court_v1::{
            PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1 as B, PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1 as MIN, ProviderSubjectV1,
        };
        use crate::palw_public_material_v1::PublicUnitV1;
        for prefix in [true, false] {
            let (p, s, native, slices, pre) = funded_final(0);
            let dependency = if prefix { pre.claim_id } else { slices[0].covered.slice.evidence_root };
            let producer = if prefix { bond_key(1) } else { slices[0].covered.slice.executor_bond };
            let lease_at = s.last_point.unwrap().daa_score + 1;
            let challenge_at = lease_at + 1;
            let deadline = challenge_at + 20;
            let until = s.exec_v2_rewards[&native].liability_until;
            let s = provider_terms(&p, &s, dependency, producer, lease_at, until, 30, true);
            let subject = ProviderSubjectV1::KernelClaim { claim: dependency };
            let mut objects = vec![seat_bond_reg(34, 2 * B)];
            for provider in 30..32 {
                objects.push(PalwConsensusObjectV2::ProviderChallengeV1 {
                    subject,
                    provider: bond_key(provider),
                    unit: PublicUnitV1::ClaimPosition { stage: 0, position: 0 },
                    valid_until_daa: challenge_at + 10,
                    challenger: bond_key(34),
                    signature: vec![9; 8],
                });
            }
            let (challenged, _) =
                step_with(&s, &p, s.last_point.unwrap().blue_score + 1, challenge_at, &objects, &provider_extras()).unwrap();
            let money =
                challenged.vesting_row(&native).unwrap().total_sompi_u128() + challenged.exec_v2_rewards[&native].total_sompi_u128();
            let worker_collateral: Vec<_> =
                [bond_key(1), bond_key(20), bond_key(21)].into_iter().map(|bond| (bond, challenged.bonds[&bond].collateral)).collect();
            let (last, _) =
                step_with(&challenged, &p, challenged.last_point.unwrap().blue_score + 1, deadline, &[], &provider_extras()).unwrap();
            assert!(
                matches!(last.exec_v2_root_v1(&native).unwrap().phase, PalwWorkRootPhaseV2::Settled { .. }),
                "inclusive last response day"
            );
            let (defaulted, delta) =
                step_with(&last, &p, last.last_point.unwrap().blue_score + 1, deadline + 1, &[], &provider_extras()).unwrap();
            assert!(matches!(defaulted.exec_v2_root_v1(&native).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
            if prefix {
                assert!(
                    matches!(defaulted.exec_v2_root_v1(&native).unwrap().prefix, PalwWorkPrefixStageV2::Defaulted { daa } if daa == deadline + 1)
                );
            } else {
                assert!(
                    matches!(defaulted.exec_v2_slice_v1(&native, 0).unwrap().stage, PalwWorkSliceStageV2::Defaulted { daa } if daa == deadline + 1)
                );
            }
            assert!(defaulted.vesting_row(&native).is_none());
            assert!(defaulted.exec_v2_rewards.get(&native).is_none());
            assert_eq!(defaulted.vesting_counters.burned - last.vesting_counters.burned, money);
            assert_eq!(defaulted.vesting_counters.moved, last.vesting_counters.moved);
            assert_eq!(defaulted.safe_weight(), 0);
            assert_eq!(crate::palw_native_settlement_v1::native_delta_evidence_v1(&delta).voided, vec![native]);
            for (bond, collateral) in worker_collateral {
                assert_eq!(defaulted.bonds[&bond].collateral, collateral, "provider failure does not charge a worker");
            }
            for provider in 30..32 {
                assert_eq!(defaulted.bonds[&bond_key(provider)].collateral, MIN, "only the signed provider reservation was charged");
            }
            let kernel_row: ClaimRowV1 = borsh::from_slice(
                &defaulted.kernel_route.as_ref().unwrap().rows[&(TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&dependency))],
            )
            .unwrap();
            assert!(matches!(kernel_row.life.state, ClaimStateV1::Final { .. }));
            assert!(
                kernel_row.reserved > 0,
                "the transferred default is recorded in the provider journal, not by slashing the worker"
            );
            assert!(defaulted.pending_payouts_iter().next().is_none());
            let mut stale = last.clone();
            stale.kernel_route.as_mut().unwrap().aux = defaulted.kernel_route.as_ref().unwrap().aux.clone();
            assert!(
                PalwStateCarriageV2::from_state(&stale).into_state(&p, None).is_err(),
                "restore refuses an unpropagated provider lapse"
            );
            let (again, _) =
                step_with(&defaulted, &p, defaulted.last_point.unwrap().blue_score + 1, deadline + 2, &[], &provider_extras())
                    .unwrap();
            assert_eq!(again.vesting_counters, defaulted.vesting_counters, "default and burn settle once");
            assert_eq!(again.safe_weight(), 0);
        }
    }

    #[test]
    fn exec_transferred_provider_lease_expiry_after_held_horizon_is_lawful() {
        use crate::palw_provider_court_v1::{PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1 as MIN, ProviderSubjectV1};
        let (p, s, native, _, pre) = funded_final(0);
        let until = s.exec_v2_rewards[&native].liability_until;
        let s = provider_terms(&p, &s, pre.claim_id, bond_key(1), s.last_point.unwrap().daa_score + 1, until, 30, true);
        let subject = ProviderSubjectV1::KernelClaim { claim: pre.claim_id };
        let (last, _) = step_with(&s, &p, s.last_point.unwrap().blue_score + 1, until, &[], &provider_extras()).unwrap();
        assert_eq!(last.kernel_route.as_ref().unwrap().provider_leases_of_v1(&subject).len(), 2);
        let (expired, _) = step_with(&last, &p, last.last_point.unwrap().blue_score + 1, until + 1, &[], &provider_extras()).unwrap();
        let route = expired.kernel_route.as_ref().unwrap();
        assert!(route.provider_leases_of_v1(&subject).is_empty());
        assert_eq!(route.provider_subject_v1(&subject).unwrap().lapsed_daa, None);
        assert!(matches!(expired.exec_v2_root_v1(&native).unwrap().phase, PalwWorkRootPhaseV2::Settled { .. }));
        for provider in 30..32 {
            assert_eq!(expired.bonds[&bond_key(provider)].collateral, 2 * MIN);
        }
    }

    #[test]
    fn exec_transferred_pending_claim_without_live_provider_lapses_without_an_expiry_charge() {
        use crate::palw_provider_court_v1::{
            PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1 as MIN, ProviderSubjectV1, palw_kernel_claim_horizon_bound_v1,
        };
        let (p, mut s, native) = world(Some(0));
        with_route(&mut s, native, Some(PLAN));
        // Seed a pending lifecycle that lasts beyond its original material term. The
        // transfer, signed terms, expiry tick, journal and restoration are real folds.
        let pre = Prefix::new(&mut s, native, ClaimStateV1::Challengeable { since_daa: 100, window_end_daa: 1_000_000 }, false);
        fund_jobs(&mut s, &pre, &[], 1);
        let until = palw_kernel_claim_horizon_bound_v1(&pre.row, &s.kernel_route.as_ref().unwrap().header.policy, None);
        assert!(until < 1_000_000);
        let s = provider_terms(&p, &s, pre.claim_id, bond_key(1), 106, until, 30, true);
        let subject = ProviderSubjectV1::KernelClaim { claim: pre.claim_id };
        let (expired, _) = step_with(&s, &p, s.last_point.unwrap().blue_score + 1, until + 1, &[], &provider_extras()).unwrap();
        let route = expired.kernel_route.as_ref().unwrap();
        assert!(route.provider_leases_of_v1(&subject).is_empty());
        assert_eq!(route.provider_subject_v1(&subject).unwrap().lapsed_daa, Some(until + 1));
        assert!(matches!(route.kernel_claim_row_v1(&pre.claim_id).unwrap().life.state, ClaimStateV1::Unavailable { .. }));
        for provider in 30..32 {
            assert_eq!(expired.bonds[&bond_key(provider)].collateral, 2 * MIN);
        }
        assert!(expired.pending_payouts_iter().next().is_none());
    }

    #[test]
    fn exec_transferred_prefix_requires_the_providers_signed_consumer_horizon() {
        use crate::palw_provider_court_v1::{ProviderSubjectV1, palw_kernel_claim_horizon_bound_v1};
        let (p, mut s, native) = world(Some(0));
        with_route(&mut s, native, Some(PLAN));
        let pre = Prefix::new(&mut s, native, ClaimStateV1::Final { final_daa: 105 }, false);
        fund_jobs(&mut s, &pre, &[], 1);
        let d = pre.decl(native);
        let policy = &s.kernel_route.as_ref().unwrap().header.policy;
        let old = palw_kernel_claim_horizon_bound_v1(&pre.row, policy, None);
        let until = super::palw_kernel_route_fold_v1::exec_liability_floor_v1(d.expiry_daa, p.window_court(), policy).unwrap();
        assert!(old < until);
        let short = provider_terms(&p, &s, pre.claim_id, bond_key(1), 106, old, 30, true);
        let subject = ProviderSubjectV1::KernelClaim { claim: pre.claim_id };
        assert!(short.kernel_route.as_ref().unwrap().provider_subject_v1(&subject).unwrap().transferred_daa.is_some());
        let rows = short.kernel_route.as_ref().unwrap().rows.clone();
        let aux = short.kernel_route.as_ref().unwrap().aux.clone();
        let rejected = open(&p, &short, d.clone()).unwrap_err();
        assert!(matches!(rejected, PalwStateV2Error::KernelRouteRefused(_)));
        assert_eq!(short.kernel_route.as_ref().unwrap().rows, rows);
        assert_eq!(short.kernel_route.as_ref().unwrap().aux, aux, "no implicit provider lease extension");
        assert!(short.exec_v2_root_v1(&native).is_none());
        let exact = provider_terms(&p, &s, pre.claim_id, bond_key(1), 106, until, 30, true);
        let leases = exact.kernel_route.as_ref().unwrap().provider_leases_of_v1(&subject);
        let admitted = open(&p, &exact, d).unwrap();
        assert_eq!(admitted.kernel_route.as_ref().unwrap().provider_leases_of_v1(&subject), leases);
        assert_eq!(admitted.exec_v2_liability_until_v1(&p, &native).unwrap(), Some(until));
        assert_eq!(admitted.exec_v2_root_v1(&native).unwrap().prefix_claim, pre.claim_id);
        assert!(admitted.pending_payouts_iter().next().is_none());
    }

    #[test]
    fn exec_transferred_slice_is_a_named_lane_refusal_and_can_recover_with_new_signed_terms() {
        use crate::palw_provider_court_v1::{ProviderSubjectV1, palw_kernel_claim_horizon_bound_v1};
        let (p, mut s, native, mut slices, pre) = session_with(1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        slices[0].row.life.state = ClaimStateV1::Final { final_daa: 105 };
        slices[0].row.liability_until = Some(105 + s.kernel_route.as_ref().unwrap().header.policy.liability_daa);
        slices[0].install(&mut s);
        fund_jobs(&mut s, &pre, &slices, 1);
        let k = &slices[0];
        let policy = &s.kernel_route.as_ref().unwrap().header.policy;
        let old = palw_kernel_claim_horizon_bound_v1(&k.row, policy, None);
        let until = s.exec_v2_liability_until_v1(&p, &native).unwrap().unwrap();
        let short = provider_terms(&p, &s, k.covered.slice.evidence_root, k.covered.slice.executor_bond, 111, old, 30, true);
        assert_eq!(refusal(&p, &short, k.covered.clone()), PalwSliceRefusalV2::VerificationProviderHorizon);
        let (refused, delta) =
            step_with(&short, &p, 200, 120, &[], &xx_slices(vec![k.covered.clone(), slices[1].covered.clone()])).unwrap();
        let codes: Vec<_> = delta
            .entries
            .iter()
            .filter_map(|entry| if let PalwDeltaEntryV2::ExecV2Verdict { code, .. } = entry { Some(*code) } else { None })
            .collect();
        assert_eq!(codes, vec![31, PalwSliceRefusalV2::SkippedSlice.code()]);
        assert_eq!(refused.exec_v2_root_v1(&native).unwrap().next_index, 0);
        assert!(refused.exec_v2_slice_v1(&native, 0).is_none());
        let recovered =
            provider_terms(&p, &refused, k.covered.slice.evidence_root, k.covered.slice.executor_bond, 121, until, 32, false);
        let subject = ProviderSubjectV1::KernelClaim { claim: k.covered.slice.evidence_root };
        let leases = recovered.kernel_route.as_ref().unwrap().provider_leases_of_v1(&subject);
        let admitted = fold_slices(&p, &recovered, recovered.last_point.unwrap().blue_score + 1, 122, vec![k.covered.clone()]);
        assert_eq!(admitted.exec_v2_root_v1(&native).unwrap().next_index, 1);
        assert_eq!(admitted.kernel_route.as_ref().unwrap().provider_leases_of_v1(&subject), leases);
        assert_eq!(admitted.exec_v2_slice_v1(&native, 0).unwrap().evidence_root, k.covered.slice.evidence_root);
        assert!(admitted.pending_payouts_iter().next().is_none());
    }

    fn funded_final_with_delta(
        retirement: u64,
    ) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>, Prefix, PalwStateDeltaV2) {
        funded_final_with_params(xp(Some(0)), retirement)
    }

    fn funded_final_with_params(
        params: PalwStateParamsV2,
        retirement: u64,
    ) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>, Prefix, PalwStateDeltaV2) {
        let (p, mut s, claim, slices, pre) =
            session_with_params(params, 1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        let p = p.with_claim_retirement_daa(retirement).unwrap();
        fund_jobs(&mut s, &pre, &slices, 1_000);
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        let s = kernel_block(&p, &s, 60, 130, slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect());
        let (s, delta) = run_to_terminal_with_delta(&p, s, claim, 70, 131);
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Settled { .. }));
        assert!(s.exec_v2_reward_vesting_v1(&claim).unwrap().total_sompi_u128() > 4_000);
        (p, s, claim, slices, pre, delta.unwrap())
    }

    fn native_dispute_step(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        daa: u64,
        object: PalwConsensusObjectV2,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let (next, delta) =
            apply_palw_transition_v2_with_extras(s, p, &ctx(10_000, daa, 10_000), &[object], None, true, false, false, true, &xx())?;
        next.assert_internal_consistency(p).unwrap();
        next.assert_deadline_consistency(p).unwrap();
        assert_eq!(apply_delta_v2(s, &delta, p).unwrap().state_root(), next.state_root());
        assert_eq!(revert_delta_v2(&next, &delta, p).unwrap().state_root(), s.state_root());
        let carriage = PalwStateCarriageV2::from_state(&next);
        assert_eq!(carriage.into_state(p, Some(next.state_root())).unwrap().state_root(), next.state_root());
        Ok(next)
    }

    #[test]
    fn exec_native_da_accepts_the_last_funded_window_and_refuses_one_daa_more() {
        let params = xp(Some(0)).with_fp_exposure_ceiling(500).unwrap();
        let (p, s, claim, _, _, _) = funded_final_with_params(params, 0);
        let bound = super::palw_exec_v2_rewards_v1::horizon(&s, &p, &claim).unwrap().unwrap();
        let now = bound - palw_da_disclose_window_daa_v1(&p) - p.window_challenge_at(bound);
        let accused =
            || PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: 0, accuser: bond_key(2), signature: vec![9; 8] };
        let admission = palw_da_accusation_admissible_v2(&s, &p, &xx(), &claim, &bond_key(2), now).unwrap();
        assert_eq!(admission.deadline_daa + p.window_challenge_at(now), bound);
        let opened = native_dispute_step(&s, &p, now, accused()).unwrap();
        assert_eq!(opened.da_sessions[&(claim, bond_key(2))].deadline_daa, admission.deadline_daa);
        assert_eq!(opened.kernel_route.as_ref().unwrap().rows, s.kernel_route.as_ref().unwrap().rows);
        assert_eq!(opened.exec_v2_rewards[&claim], s.exec_v2_rewards[&claim]);
        assert_eq!(opened.claim(&claim).unwrap().trace_retention_daa, s.claim(&claim).unwrap().trace_retention_daa);
        assert!(opened.pending_payouts_iter().next().is_none());
        let before = s.state_root();
        assert!(matches!(
            palw_da_accusation_admissible_v2(&s, &p, &xx(), &claim, &bond_key(2), now + 1),
            Err(PalwStateV2Error::ExecV2Refused(_))
        ));
        assert!(matches!(native_dispute_step(&s, &p, now + 1, accused()), Err(PalwStateV2Error::ExecV2Refused(_))));
        assert_eq!(s.state_root(), before, "a refused accusation cannot change the parent");
        let mut bad = opened.clone();
        bad.da_sessions.get_mut(&(claim, bond_key(2))).unwrap().deadline_daa += 1;
        assert!(matches!(super::palw_exec_v2_rewards_v1::consistent(&bad, &p), Err(PalwStateV2Error::ExecV2Refused(_))));
        assert!(PalwStateCarriageV2::from_state(&bad).into_state(&p, None).is_err(), "restore refuses an overlong session");
    }

    #[test]
    fn exec_native_session_cannot_borrow_another_dependency_longer_liability() {
        let (p, mut s, claim, slices, _) = funded_final(0);
        let until = s.exec_v2_rewards[&claim].liability_until;
        let kernel = s.kernel_route.as_mut().unwrap();
        let key = (TABLE_CLAIMS_V1, borsh::to_vec(&slices[0].covered.slice.evidence_root.as_bytes()).unwrap());
        let mut row: ClaimRowV1 = borsh::from_slice(&kernel.rows[&key]).unwrap();
        row.liability_until = Some(until - 1);
        kernel.rows.insert(key, borsh::to_vec(&row).unwrap());
        assert_eq!(super::palw_exec_v2_rewards_v1::horizon(&s, &p, &claim).unwrap(), Some(until));
        assert!(matches!(
            super::palw_exec_v2_rewards_v1::require_native_session_window(&s, &p, &claim, until),
            Err(PalwStateV2Error::ExecV2Refused(_))
        ));
        assert!(super::palw_exec_v2_rewards_v1::require_native_session_window(&s, &p, &claim, until - 1).is_ok());
    }

    #[test]
    fn exec_native_model_court_cannot_extend_past_the_frozen_kernel_window() {
        let params = xp(Some(0)).with_fp_exposure_ceiling(500).unwrap();
        let (p, s, claim, _, _) = session_with_params(params, 1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        let bound = super::palw_exec_v2_rewards_v1::horizon(&s, &p, &claim).unwrap().unwrap();
        let c = s.claim(&claim).unwrap();
        let court = court_open(claim, c.trace_root, c.bond, bond_key(20));
        let mut exact = s.clone();
        exact.class_court_windows.insert(c.class_id, bound - 130);
        let opened = native_dispute_step(&exact, &p, 130, court.clone()).unwrap();
        assert_eq!(opened.court_sessions.values().next().unwrap().deadline_daa, bound);
        assert_eq!(opened.kernel_route.as_ref().unwrap().rows, exact.kernel_route.as_ref().unwrap().rows);
        let mut late = exact.clone();
        *late.class_court_windows.get_mut(&c.class_id).unwrap() += 1;
        assert!(matches!(native_dispute_step(&late, &p, 130, court), Err(PalwStateV2Error::ExecV2Refused(_))));
    }

    #[test]
    fn native_da_admission_without_exec_preserves_its_existing_retention_rule() {
        let (p, s, claim) = world_with(xp(None).with_fp_exposure_ceiling(500).unwrap());
        let now = 20_000;
        assert!(palw_da_accusation_admissible_v2(&s, &p, &xx(), &claim, &bond_key(2), now).is_ok());
        assert!(super::palw_exec_v2_rewards_v1::require_native_da_window(&s, &p, &claim, u64::MAX, u64::MAX).is_ok());
    }

    #[test]
    fn complete_reward_reader_conserves_all_funds_and_pages_each_leg_without_changing_totals() {
        use crate::palw_reward_vesting_read_v2::palw_reward_vesting_read_v2 as read;
        use crate::palw_vesting_read_v1::{PalwVestingPayeeV1 as Payee, PalwVestingQueryV1 as Q};
        let (p, s, claim, _, _) = funded_final(0);
        let daa = s.last_point.unwrap().daa_score + 1;
        let chain = read(&s, &p, daa, None, &Q::Chain, 500, None, None);
        let auxiliary = s.exec_v2_rewards[&claim].total_sompi_u128();
        assert_eq!(chain.outstanding_sompi, chain.native.live_sompi + auxiliary);
        assert_eq!(chain.query_outstanding_sompi, chain.outstanding_sompi);
        assert_eq!(chain.exec_slice_sompi + chain.exec_kernel_fee_sompi, auxiliary);
        assert_eq!(chain.outstanding_sompi + s.vesting_counters.moved + s.vesting_counters.burned, s.vesting_counters.created);
        assert!(chain.native.has_exec_rewards);
        assert_eq!(chain.state_root, s.state_root());
        let mut cursor = None;
        let mut legs = Vec::new();
        loop {
            let page = read(&s, &p, daa, None, &Q::Chain, 1, None, cursor);
            assert_eq!(
                (page.query_outstanding_sompi, page.locked_sources, page.exec_rows_total),
                (chain.query_outstanding_sompi, chain.locked_sources, chain.exec_rows_total)
            );
            legs.extend(page.exec_rows);
            match page.exec_next_after {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(legs, chain.exec_rows, "same-bond funds require distinct cursor positions");
        let own = read(&s, &p, daa, None, &Q::Payee(Payee::Bond(bond_key(20))), 1, None, None);
        let expected: u128 = s.exec_v2_rewards[&claim]
            .legs
            .iter()
            .filter(|((who, _), _)| *who == bond_key(20))
            .map(|(_, leg)| u128::from(leg.amount))
            .sum();
        assert_eq!(own.native.rows_total, 0);
        assert_eq!(own.exec_rows_total, 2);
        assert_eq!(own.locked_sources, 1, "two funds of one actor share a holding source");
        assert_eq!(own.query_outstanding_sompi, expected);
        let address = read(&s, &p, daa, None, &Q::Payee(Payee::Payload(s.bonds[&bond_key(20)].payout_payload)), 500, None, None);
        assert_eq!(address.query_outstanding_sompi, expected);
        let root = read(&s, &p, daa, None, &Q::Claim(claim), 500, None, None);
        assert_eq!(root.query_outstanding_sompi, chain.outstanding_sompi);
        let absent = read(&s, &p, daa, None, &Q::Claim(h64(0xFA)), 1, None, None);
        assert_eq!((absent.query_outstanding_sompi, absent.locked_sources, absent.exec_rows_total), (0, 0, 0));
        let off = p.clone().with_rcore_plus_mirrors(None, 0, Vec::new());
        let forced = read(&s, &off, daa, None, &Q::Chain, 500, None, None);
        assert!(!forced.native.rcore_plus_active && forced.native.live_rows > 0);
        assert_eq!(forced.outstanding_sompi, chain.outstanding_sompi, "mandatory EXEC native vesting remains visible below R-core+");
    }

    #[test]
    fn complete_reward_reader_names_actor_hold_and_exact_release_after_native_moves() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        use crate::palw_reward_vesting_read_v2::palw_reward_vesting_read_v2 as read;
        use crate::palw_vesting_read_v1::{PalwClaimVestingStageV1 as Stage, PalwVestingPayeeV1 as Payee, PalwVestingQueryV1 as Q};
        let (p, s, claim, _, _) = funded_final(0);
        let p = aggregate_params(&p);
        let until = s.exec_v2_rewards[&claim].liability_until;
        let lift = until + p.window_court();
        let (s, _) = aggregate_exec_actor(&s, &p, bond_key(20), until, O::Equivocation);
        let word = s.last_point.unwrap().blue_score + 1;
        let s = step(&s, &p, word, until + 1, &[]).unwrap().0;
        let s = step(&s, &p, word + 1, until + 2, &[]).unwrap().0;
        assert!(s.vesting_row(&claim).is_none());
        let query = Q::Payee(Payee::Bond(bond_key(20)));
        let held = read(&s, &p, lift - 1, None, &query, 1, None, None);
        assert_eq!((held.exec_rows_total, held.locked_sources, held.last_lock_daa), (2, 1, Some(lift)));
        assert_eq!(held.query_exec_ready_sompi, 0);
        let root = read(&s, &p, lift - 1, None, &Q::Claim(claim), 500, None, None);
        assert_eq!(root.native.claim_stage, Some(Stage::ExecPending));
        let ready = read(&s, &p, lift, None, &query, 500, None, None);
        assert_eq!(ready.query_exec_pending_sompi, 0);
        assert_eq!(ready.query_exec_ready_sompi, held.query_exec_pending_sompi);
        assert_eq!((ready.locked_sources, ready.last_lock_daa), (0, None));
        assert!(ready.exec_rows.iter().all(|leg| leg.ready && leg.in_next_block && !leg.native_pending && !leg.lock_live));
        let (after, _) = step(&s, &p, word + 2, lift, &[]).unwrap();
        assert_eq!(after.vesting_counters.moved - s.vesting_counters.moved, ready.query_exec_ready_sompi);
        let paid = read(&after, &p, lift + 1, None, &Q::Claim(claim), 500, None, None);
        assert_eq!(paid.native.claim_stage, Some(Stage::Moved));
        assert_eq!((paid.outstanding_sompi, paid.query_outstanding_sompi, paid.exec_rows_total), (0, 0, 0));
        assert!(!paid.native.has_exec_rewards);
    }

    fn aggregate_params(p: &PalwStateParamsV2) -> PalwStateParamsV2 {
        use crate::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
        p.clone().with_capacity_liability(Some(PalwCapacityLiabilityV1 {
            activation: crate::config::params::ForkActivation::new(0),
            steps: vec![PalwCapacityStepV1 { from_daa: 0, rho: 10, q_credit_permille: 0 }],
        }))
    }

    // Tests the existing conviction funnel after an actual funded composite Final. Objective court admission
    // is a separate acceptance gate; this helper does not claim to prove a public native/kernel offence.
    fn aggregate_exec_actor(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        bond: PalwBondKeyV2,
        daa: u64,
        offence: crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1,
    ) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let extras = xx();
        let mut b = TransitionBuilder::new(s, p, false, false, false, false, &extras);
        b.aggregate_on_conviction_v1(daa, h64(0xFA17), crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted, &[(bond, offence)])
            .unwrap();
        let point = ctx(s.last_point.unwrap().blue_score + 1, daa, s.last_point.unwrap().blue_score + 1);
        b.entries.push(PalwDeltaEntryV2::LastPoint { old: s.last_point, new: Some(point) });
        b.state.last_point = Some(point);
        let delta = PalwStateDeltaV2 { point, entries: b.entries };
        let replay = apply_delta_v2(s, &delta, p).unwrap();
        assert_eq!(replay, b.state, "actor and derived index replay are exact");
        assert_eq!(revert_delta_v2(&replay, &delta, p).unwrap(), *s);
        replay.assert_internal_consistency(p).unwrap();
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&replay)).unwrap();
        let back = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(replay.state_root())).unwrap();
        assert_eq!(back, replay, "holds and payee index survive restart");
        (replay, delta)
    }

    #[test]
    fn aggregate_intent_burns_only_the_executor_own_unpaid_funds_even_after_maturity_and_is_fenced() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        use crate::palw_vesting_v1::{PalwVestingNoteV1, palw_vesting_burned_by_delta_v1, palw_vesting_notes_of_delta_v1};
        for latched in [false, true] {
            let (p, mut s, claim, _, _) = funded_final(0);
            if latched {
                s.vesting.get_mut(&claim).unwrap().matured_at = Some(s.last_point.unwrap().daa_score);
                s.rebuild_vesting_indices();
            }
            let original = s.exec_v2_rewards[&claim].clone();
            let native = s.vesting[&claim].clone();
            let bond = bond_key(20);
            let own: Vec<_> = original.legs.iter().filter(|((who, _), _)| *who == bond).collect();
            assert_eq!(own.len(), 2, "both source funds must be exercised");
            let expected: u128 = own.iter().map(|(_, leg)| u128::from(leg.amount)).sum();
            let daa = if latched { original.liability_until + 2 } else { s.last_point.unwrap().daa_score + 1 };
            let (off, _) = aggregate_exec_actor(&s, &p, bond, daa, O::Contradiction { tag: 9 });
            assert_eq!(off.exec_v2_rewards, s.exec_v2_rewards, "the dormant aggregate fence cannot burn");
            assert_eq!(off.vesting_counters, s.vesting_counters);
            let p = aggregate_params(&p);
            let (after, delta) = aggregate_exec_actor(&s, &p, bond, daa, O::Contradiction { tag: 9 });
            assert_eq!(after.vesting[&claim], native, "an unrelated conviction must keep the root producer, Panel and reserve");
            let mut kept = original.legs.clone();
            kept.retain(|(who, _), _| *who != bond);
            assert_eq!(after.exec_v2_rewards[&claim].legs, kept);
            assert_eq!(after.vesting_counters.burned - s.vesting_counters.burned, expected);
            assert_eq!(after.bond_freezes[&bond].forfeited_sompi as u128, u128::from(s.bonds[&bond].collateral) + expected);
            assert_eq!(palw_vesting_burned_by_delta_v1(&delta) as u128, expected);
            assert_eq!(
                palw_vesting_notes_of_delta_v1(&delta)
                    .filter(|note| matches!(note,
                PalwVestingNoteV1::ExecReward { action: PalwExecV2RewardActionV1::Forfeited { offence_id }, bond: who, .. }
                if *offence_id == h64(0xFA17) && *who == bond))
                    .count(),
                2
            );
            let (again, _) = aggregate_exec_actor(&after, &p, bond, daa + 1, O::Contradiction { tag: 9 });
            assert_eq!(again.vesting_counters, after.vesting_counters, "duplicate aggregate forfeiture cannot burn twice");
            assert_eq!(again.exec_v2_rewards, after.exec_v2_rewards);
        }
    }

    #[test]
    fn executor_tier_freeze_holds_only_its_funds_through_restart_and_releases_at_its_lift() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        let (p, s, claim, _, _) = funded_final(700);
        let p = aggregate_params(&p);
        let until = s.exec_v2_rewards[&claim].liability_until;
        let lift = until + p.window_court();
        let own: u128 = s.exec_v2_rewards[&claim]
            .legs
            .iter()
            .filter(|((who, _), _)| *who == bond_key(20))
            .map(|(_, leg)| u128::from(leg.amount))
            .sum();
        let (s, _) = aggregate_exec_actor(&s, &p, bond_key(20), until, O::Equivocation);
        assert_eq!(s.exec_v2_reward_holds.get(&(claim, bond_key(20))), Some(&lift));
        assert!(!s.exec_v2_reward_holds.contains_key(&(claim, bond_key(21))));
        let word = s.last_point.unwrap().blue_score + 1;
        let s = step(&s, &p, word, until + 1, &[]).unwrap().0;
        let s = step(&s, &p, word + 1, until + 2, &[]).unwrap().0;
        assert!(s.vesting_row(&claim).is_none());
        assert!(
            s.exec_v2_rewards[&claim].legs.keys().all(|(who, _)| *who == bond_key(20)),
            "other executor funds must move on their original schedule"
        );
        assert_eq!(s.exec_v2_rewards[&claim].total_sompi_u128(), own);
        assert!(palw_bond_is_payee_of_unmatured_row_v1(&s, &p, &bond_key(20), lift - 1, None));
        assert!(!palw_bond_is_payee_of_unmatured_row_v1(&s, &p, &bond_key(21), lift - 1, None));
        let s = step(&s, &p, word + 2, lift - 1, &[]).unwrap().0;
        assert_eq!(s.exec_v2_rewards[&claim].total_sompi_u128(), own);
        let s = step(&s, &p, word + 3, lift, &[]).unwrap().0;
        assert!(s.exec_v2_reward_vesting_v1(&claim).is_none());
        assert!(s.exec_v2_reward_holds.is_empty() && s.exec_v2_reward_payees.is_empty());
        assert_eq!(s.vesting_counters.moved, s.vesting_counters.created);
        assert_eq!(s.vesting_counters.burned, 0);
    }

    #[test]
    fn executor_b3_tracks_its_clocks_and_forced_native_vesting_even_below_rcore_without_rearming_queue_waiters() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        let (p, s, claim, _, _) = funded_final(0);
        let until = s.exec_v2_rewards[&claim].liability_until;
        let off = p.clone().with_rcore_plus_mirrors(None, 0, Vec::new());
        for params in [&p, &off] {
            for who in [bond_key(20), s.vesting[&claim].producer_bond] {
                let mut retiring = s.bonds[&who].clone();
                retiring.status = PalwBondStatusV2::Retiring { since_daa: 0, settled_at_since: 0 };
                assert!(palw_bond_collateral_is_locked_v6(
                    &s,
                    params,
                    &who,
                    &retiring,
                    until,
                    params.withdrawal_delay_daa(),
                    None,
                    false
                ));
                assert!(
                    !palw_bond_collateral_is_locked_v6(
                        &s,
                        params,
                        &who,
                        &retiring,
                        until + 1,
                        params.withdrawal_delay_daa(),
                        None,
                        false
                    ),
                    "spent clocks must not lock only because a receipt waits in the payout queue"
                );
            }
        }
        // The shared second clock holds an executor after the DAA horizon, but a native latch cannot be rearmed.
        let mut clocked = s.clone();
        clocked.recent_anchor_daas = vec![until + 1];
        assert!(palw_bond_is_payee_of_unmatured_row_v1(&clocked, &p, &bond_key(20), until + 1, Some(100)));
        assert!(
            !palw_bond_is_payee_of_unmatured_row_v1(&clocked, &p, &bond_key(20), until + 1 + 2 * p.window_court(), Some(100)),
            "the shared clock has a finite per-obligation bound"
        );
        clocked.vesting.get_mut(&claim).unwrap().matured_at = Some(until + 1);
        clocked.rebuild_vesting_indices();
        assert!(!palw_bond_is_payee_of_unmatured_row_v1(&clocked, &p, &bond_key(20), until + 1, Some(100)));
        let p = aggregate_params(&p);
        let (s, _) = aggregate_exec_actor(&s, &p, bond_key(20), until + 2, O::Equivocation);
        assert!(s.exec_v2_reward_holds.is_empty(), "a tier conviction cannot rearm already matured executor funds");
    }

    #[test]
    fn a_later_final_cannot_recreate_a_forfeited_executor_fee_or_slice_reward() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        let (p, mut s, claim, slices, pre) = session_with(1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
        let p = aggregate_params(&p);
        fund_jobs(&mut s, &pre, &slices, 1_000);
        let own = bond_key(20);
        let collateral = s.bonds[&own].collateral;
        let s = license(&p, &s, claim, 45, 111);
        let s = carry_all(&p, &s, &slices);
        // Work was signed/admitted before the freeze. A previously unconsenting
        // worker cannot create a first Work reservation after full confiscation.
        let (s, _) = aggregate_exec_actor(&s, &p, own, 121, O::Contradiction { tag: 9 });
        assert_eq!(s.bond_freezes[&own].forfeited_sompi, collateral);
        let s = kernel_block(&p, &s, 60, 130, slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect());
        let (s, delta) = run_to_terminal_with_delta(&p, s, claim, 70, 131);
        assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Settled { .. }));
        assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
        assert!(s.exec_v2_rewards[&claim].legs.keys().all(|(who, _)| *who != own));
        assert!(s.exec_v2_reward_hold_after_v1(&claim, &own).is_none());
        let burned = crate::palw_vesting_v1::palw_vesting_burned_by_delta_v1(&delta.unwrap());
        assert!(burned > 1_000, "both a debited fee and the later native slice leg are forfeited");
        assert_eq!(s.bond_freezes[&own].forfeited_sompi, collateral + burned);
        assert_eq!(s.vesting_counters.burned, u128::from(burned));
        crate::palw_vesting_v1::palw_vesting_counters_consistent_v1(&s).unwrap();
    }

    #[test]
    fn executor_hold_tail_is_optional_and_restoration_rejects_orphan_duplicate_and_truncated_holds() {
        use crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1 as O;
        let (p, s, claim, _, _) = funded_final(0);
        let old_bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
        let old = borsh::from_slice::<PalwStateCarriageV2>(&old_bytes).unwrap();
        assert!(old.exec_v2_reward_holds.is_empty(), "the previous F0 layout needs no actor tail");
        assert_eq!(old.into_state(&p, Some(s.state_root())).unwrap(), s);
        let p = aggregate_params(&p);
        let (s, _) = aggregate_exec_actor(&s, &p, bond_key(20), s.exec_v2_rewards[&claim].liability_until, O::Equivocation);
        let carriage = PalwStateCarriageV2::from_state(&s);
        let bytes = borsh::to_vec(&carriage).unwrap();
        let tail = borsh::to_vec(&s.exec_v2_reward_holds).unwrap();
        let history_len = if s.exec_v2_job_uses.is_empty() { 0 } else { 1 + borsh::to_vec(&s.exec_v2_job_uses).unwrap().len() };
        let origin_len = if s.exec_v2_origins.is_empty() { 0 } else { 1 + borsh::to_vec(&s.exec_v2_origins).unwrap().len() };
        let tail_end = bytes.len() - history_len - origin_len;
        assert_eq!(bytes[tail_end - tail.len() - 1], PALW_CARRIAGE_EXEC_V2_REWARD_HOLDS_TAIL_V1);
        assert_eq!(PALW_CARRIAGE_EXEC_V2_REWARD_HOLDS_TAIL_V1, 0xF1);
        let mut twice = bytes.clone();
        twice.push(PALW_CARRIAGE_EXEC_V2_REWARD_HOLDS_TAIL_V1);
        twice.extend_from_slice(&tail);
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&twice).is_err());
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes[..tail_end - 1]).is_err());
        let non_executor = *s
            .bonds
            .keys()
            .find(|bond| !s.exec_v2_rewards[&claim].legs.keys().any(|(who, _)| who == *bond))
            .expect("a registered bond with no executor fund");
        for who in [non_executor, bond_key(99)] {
            let mut bad = carriage.clone();
            bad.exec_v2_reward_holds.insert((claim, who), u64::MAX);
            assert!(bad.into_state(&p, None).is_err(), "a hold without this actor's executor fund must fail restore");
        }
        let mut shortened = carriage.clone();
        shortened.exec_v2_reward_holds.clear();
        assert!(
            shortened.into_state(&p, Some(s.state_root())).is_err(),
            "omitting a rooted extension cannot pass the committed snapshot root"
        );
        let mut bad = carriage.clone();
        bad.exec_v2_reward_holds.insert((claim, bond_key(20)), s.exec_v2_rewards[&claim].liability_until);
        assert!(bad.into_state(&p, None).is_err());
        let mut phantom = carriage;
        phantom.exec_v2_reward_holds.insert((h64(0xFA11), bond_key(20)), u64::MAX);
        assert!(phantom.into_state(&p, None).is_err());
        let action = PalwExecV2RewardActionV1::Forfeited { offence_id: h64(0xFA17) };
        assert_eq!(borsh::to_vec(&action).unwrap()[0], 3);
        for (tag, old) in [PalwExecV2RewardActionV1::Created, PalwExecV2RewardActionV1::Moved, PalwExecV2RewardActionV1::Burned]
            .into_iter()
            .enumerate()
        {
            assert_eq!(borsh::to_vec(&old).unwrap(), vec![tag as u8]);
        }
    }

    #[test]
    fn final_journal_freezes_the_consumer_window_for_native_maturity_and_readiness_after_retirement() {
        use crate::palw_native_settlement_v1::*;
        let (p, s, claim, _, _, delta) = funded_final_with_delta(700);
        let until = s.exec_v2_reward_vesting_v1(&claim).unwrap().liability_until;
        let evidence = native_delta_evidence_v1(&delta);
        assert_eq!(evidence.exec_liability_until.get(&claim), Some(&until));
        let original = &evidence.finalized_attempts.iter().find(|(id, _)| *id == claim).unwrap().1;
        assert_eq!(native_effective_retention_v1(&s, &p, &claim, original).unwrap(), original.trace_retention_daa.max(until + 1));
        let open = |state: &PalwChainStateV2, daa| {
            native_open_from_v1(
                state.claims_iter().map(|(id, c)| {
                    (c.accepted_blue_score, c.phase.clone(), native_effective_retention_v1(state, &p, id, c).unwrap_or(u64::MAX))
                }),
                std::iter::empty(),
                daa,
            )
        };
        assert_eq!(open(&s, until), original.accepted_blue_score, "the inclusive dependency window is still open");
        let after = original.trace_retention_daa.max(until + 1);
        assert_eq!(open(&s, after), u64::MAX, "readiness closes on the same effective clock");
        // A non-floor, priced evidence record lets the converter exercise maturity independently of the base-class exclusion.
        let mut priced = evidence.clone();
        priced.finalized_attempts[0].1.class_id = h64(0xABC);
        priced.finalized_attempts[0].1.work_id = Some(h64(0xBEEF));
        let facts = |state: &PalwChainStateV2| {
            native_facts_of_block_v1(
                &NativeFactRulesV1 {
                    state,
                    params: &p,
                    canonical_work_daa: Some(0),
                    quantum_maturity_daa: 0,
                    claims_with_open_disputes: &BTreeSet::new(),
                },
                &priced,
                &BTreeSet::new(),
                (131, 71),
            )
        };
        assert!(facts(&s).iter().all(|fact| fact.matured_daa > until));
        assert_eq!(facts(&s).len(), 1);
        let retired = run_to_retirement(&p, s, claim);
        assert!(retired.exec_v2_root_v1(&claim).is_none());
        let preserved = facts(&retired);
        assert_eq!(preserved.len(), 1);
        assert!(preserved[0].matured_daa > until, "frozen Final evidence survives removal of the live root");
    }

    #[test]
    fn a_composite_final_vests_even_when_rcore_is_off_and_zero_slice_rounding_keeps_the_release_receipt() {
        for escrow in [1, 1_000_000] {
            let params = xp(Some(0)).with_rcore_plus_mirrors(None, 0, Vec::new());
            let (p, mut s, claim, slices, _) =
                session_with_params(params, 1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
            // Arithmetic control for a root whose slice shares all round to zero.
            s.claims.get_mut(&claim).unwrap().escrowed_reward = escrow;
            let s = license(&p, &s, claim, 45, 111);
            let s = carry_all(&p, &s, &slices);
            let s =
                kernel_block(&p, &s, 60, 130, slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect());
            let s = run_to_terminal(&p, s, claim, 70, 131);
            assert!(!p.rcore_plus_active_at(131));
            assert!(s.pending_payouts_iter().next().is_none());
            let native = s.vesting_row(&claim).expect("composite Final always vests its native reward");
            let receipt = s.exec_v2_reward_vesting_v1(&claim).expect("receipt retains the forced vesting path");
            if escrow == 1 {
                assert!(receipt.legs.is_empty());
            }
            let daa = native.expiry_daa;
            let word = s.last_point.as_ref().unwrap().blue_score + 1;
            let released = step(&s, &p, word, daa, &[]).unwrap().0;
            assert!(released.vesting_row(&claim).is_none(), "the force-vested row cannot be stranded by the off fence");
            let done = step(&released, &p, word + 1, daa + 1, &[]).unwrap().0;
            assert!(done.exec_v2_reward_vesting_v1(&claim).is_none());
            assert_eq!(done.vesting_counters().created, done.vesting_counters().moved);
        }
    }

    #[test]
    fn native_final_reversal_closes_the_settled_exec_root_without_returning_exposure_twice() {
        for reason in [PalwVoidReasonV2::CourtFraud, PalwVoidReasonV2::ProducerWithholding] {
            let (p, s, claim, _, _) = funded_final(700);
            let amount = s.exec_v2_reward_vesting_v1(&claim).unwrap().total_sompi_u128();
            let before = s.vesting_counters();
            let extras = xx();
            let mut builder = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
            let daa = s.last_point.as_ref().unwrap().daa_score + 1;
            let context = ctx(0x1234, daa, s.last_point.as_ref().unwrap().blue_score + 1);
            builder.reverse_convicted_final(&context, claim, reason).unwrap();
            assert!(matches!(builder.state.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
            assert!(
                builder.state.exec_v2_reward_vesting_v1(&claim).unwrap().legs.is_empty(),
                "a zero-value receipt preserves any actor-specific native legs until they finish"
            );
            assert_eq!(builder.state.vesting_counters().burned - before.burned, amount);
            assert_eq!(builder.state.reserved_exposure(&bond_key(20)), 0);
            assert_eq!(builder.state.reserved_exposure(&bond_key(21)), 0);
            builder.state.assert_internal_consistency(&p).unwrap();
            let root = builder.state.state_root();
            builder.reverse_convicted_final(&context, claim, reason).unwrap();
            assert_eq!(builder.state.state_root(), root, "duplicate reversal is idempotent");
            let native = builder.state.vesting_row(&claim).unwrap().total_sompi_u128();
            builder.burn_vesting_row(claim, h64(0xF0), crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction).unwrap();
            assert!(builder.state.exec_v2_reward_vesting_v1(&claim).is_none());
            assert_eq!(builder.state.vesting_counters().burned - before.burned, native + amount);
            crate::palw_vesting_v1::palw_vesting_consistency_v1(&builder.state).unwrap();
            // The whole-native burn can arrive first too: neither counter write may overwrite the other fund's burn.
            let mut other = TransitionBuilder::new(&s, &p, false, false, false, false, &extras);
            other.burn_vesting_row(claim, h64(0xF0), crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction).unwrap();
            assert!(other.state.exec_v2_reward_vesting_v1(&claim).is_none());
            assert_eq!(other.state.vesting_counters().burned - before.burned, native + amount);
            crate::palw_vesting_v1::palw_vesting_counters_consistent_v1(&other.state).unwrap();
        }
    }

    #[test]
    fn a_settled_prefix_or_middle_slice_verdict_reverses_native_weight_evidence_and_every_unreleased_leg_once() {
        for prefix in [false, true] {
            for fraud in [false, true] {
                let (p, mut s, claim, slices, pre) = funded_final(700);
                // An independent history's weight must survive this one root's reversal.
                s.safe_weight += 999;
                s.retired_safe_weight += 999;
                s.assert_internal_consistency(&p).unwrap();
                let native = s.vesting_row(&claim).unwrap().total_sompi_u128();
                let extra = s.exec_v2_reward_vesting_v1(&claim).unwrap().total_sompi_u128();
                let before = s.vesting_counters();
                let balances: Vec<_> = s.bonds.iter().map(|(key, row)| (*key, row.collateral)).collect();
                let dependency = if prefix { pre.claim_id } else { slices[1].claim_id };
                let key = palw_exec_v2_kernel_key_v1(&dependency);
                let mut row: ClaimRowV1 =
                    borsh::from_slice(&s.kernel_route.as_ref().unwrap().rows[&(TABLE_CLAIMS_V1, key.clone())]).unwrap();
                row.convicted = fraud;
                row.reserved = 0;
                let word = s.last_point.as_ref().unwrap().blue_score + 1;
                let daa = s.last_point.as_ref().unwrap().daa_score + 1;
                let extras = PalwTransitionExtrasV1 {
                    exec_v2_test_kernel_rows: vec![(TABLE_CLAIMS_V1, key, Some(borsh::to_vec(&row).unwrap()))],
                    ..xx()
                };
                let (after, delta) = step_with(&s, &p, word, daa, &[], &extras).unwrap();
                let reason = if fraud { PalwVoidReasonV2::WorkSliceProvenFalse } else { PalwVoidReasonV2::WorkSliceDefaulted };
                assert!(
                    matches!(after.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Voided { reason: actual, .. } if actual == reason)
                );
                assert!(matches!(after.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
                assert_eq!(after.safe_weight(), 999, "only this root's exact Final contribution leaves");
                assert_eq!(crate::palw_native_settlement_v1::native_delta_evidence_v1(&delta).voided, vec![claim]);
                assert!(after.vesting_row(&claim).is_none() && after.exec_v2_reward_vesting_v1(&claim).is_none());
                assert_eq!(after.vesting_counters().created, before.created);
                assert_eq!(after.vesting_counters().moved, before.moved);
                assert_eq!(after.vesting_counters().burned - before.burned, native + extra);
                assert_eq!(u128::from(crate::palw_vesting_v1::palw_vesting_burned_by_delta_v1(&delta)), native + extra);
                assert_eq!(
                    after.bonds.iter().map(|(key, row)| (*key, row.collateral)).collect::<Vec<_>>(),
                    balances,
                    "the consumer adds no collateral punishment"
                );
                assert_eq!(after.reserved_exposure(&bond_key(20)), 0);
                assert_eq!(after.reserved_exposure(&bond_key(21)), 0, "no second release of Final's executor exposures");
                assert!(after.pending_payouts_iter().next().is_none());
                let again = step_with(&after, &p, word + 1, daa + 1, &[], &extras).unwrap().0;
                assert_eq!(again.safe_weight(), 999);
                assert_eq!(again.vesting_counters(), after.vesting_counters(), "a duplicate verdict cannot burn twice");
            }
        }
    }

    #[test]
    fn all_executor_rewards_wait_through_the_inclusive_horizon_and_release_finitely_with_retirement_and_queue_width() {
        let (p, s, claim, _, _) = funded_final(700);
        let until = s.exec_v2_reward_vesting_v1(&claim).unwrap().liability_until;
        let native = s.vesting_row(&claim).unwrap();
        assert_eq!(native.expiry_daa, until + 1);
        assert!(deadline_of(&s, claim).unwrap() > until, "retirement cannot strand actionable dependency weight");
        let created = s.vesting_counters().created;
        assert_eq!(created, native.total_sompi_u128() + s.exec_v2_reward_vesting_v1(&claim).unwrap().total_sompi_u128());
        assert!(s.pending_payouts_iter().next().is_none());
        let word = s.last_point.as_ref().unwrap().blue_score + 1;
        let at_last = step(&s, &p, word, until, &[]).unwrap().0;
        assert!(at_last.claim(&claim).is_some());
        assert!(at_last.vesting_row(&claim).is_some() && at_last.exec_v2_reward_vesting_v1(&claim).is_some());
        assert!(at_last.pending_payouts_iter().next().is_none(), "last-day proofs remain actionable before any payout");
        let first_release = step(&at_last, &p, word + 1, until + 1, &[]).unwrap().0;
        assert!(first_release.vesting_row(&claim).is_none());
        assert!(first_release.exec_v2_reward_vesting_v1(&claim).is_some(), "additional legs follow the native clock's release");
        let mut state = first_release;
        for i in 2..6 {
            let predicted_keys = state.exec_v2_reward_release_keys_v1(until + i, true);
            let predicted_native = crate::palw_vesting_v1::palw_vesting_next_block_plan_v1(&state, &p, until + i, None);
            state = step(&state, &p, word + i, until + i, &[]).unwrap().0;
            assert!(state.pending_payouts_iter().count() <= PALW_V2_MAX_PAYOUTS_PER_BLOCK);
            assert!(predicted_keys + predicted_native.new_keys <= PALW_V2_MAX_PAYOUTS_PER_BLOCK);
            if state.exec_v2_reward_vesting_v1(&claim).is_none() {
                break;
            }
        }
        assert!(state.claim(&claim).is_none() && state.exec_v2_root_v1(&claim).is_none());
        assert!(state.exec_v2_reward_vesting_v1(&claim).is_none());
        assert_eq!(state.vesting_counters().moved, created);
        assert_eq!(state.vesting_counters().burned, 0);
        crate::palw_vesting_v1::palw_vesting_counters_consistent_v1(&state).unwrap();
    }

    #[test]
    fn executor_vesting_has_a_separate_optional_rooted_tail_and_refuses_shortened_or_overallocated_restore() {
        let (p, s, claim, _, _) = funded_final(0);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&s)).unwrap();
        let tail = borsh::to_vec(&s.exec_v2_rewards).unwrap();
        let history_len = if s.exec_v2_job_uses.is_empty() { 0 } else { 1 + borsh::to_vec(&s.exec_v2_job_uses).unwrap().len() };
        let origin_len = if s.exec_v2_origins.is_empty() { 0 } else { 1 + borsh::to_vec(&s.exec_v2_origins).unwrap().len() };
        let tail_end = bytes.len() - history_len - origin_len;
        assert_eq!(bytes[tail_end - tail.len() - 1], PALW_CARRIAGE_EXEC_V2_REWARDS_TAIL_V1);
        let mut twice = bytes.clone();
        twice.push(PALW_CARRIAGE_EXEC_V2_REWARDS_TAIL_V1);
        twice.extend(tail);
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&twice).is_err());
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes[..tail_end - 1]).is_err());
        for shortened in [false, true] {
            let mut bad = s.clone();
            let row = bad.exec_v2_rewards.get_mut(&claim).unwrap();
            if shortened {
                row.liability_until = row.final_daa;
            } else {
                row.allocation_sompi = 0;
            }
            assert_ne!(bad.state_root(), s.state_root());
            assert_ne!(borsh::to_vec(&PalwStateCarriageV2::from_state(&bad)).unwrap(), bytes);
            assert!(bad.assert_internal_consistency(&p).is_err());
            assert!(PalwStateCarriageV2::from_state(&bad).into_state(&p, None).is_err());
        }
        let mut short_native = PalwStateCarriageV2::from_state(&s);
        short_native.vesting.get_mut(&claim).unwrap().expiry_daa = s.exec_v2_reward_vesting_v1(&claim).unwrap().liability_until;
        assert!(
            matches!(short_native.into_state(&p, None), Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason.contains("native and additional vesting disagree"))
        );
    }

    #[test]
    fn void_and_ready_root_expiry_release_all_escrows_without_a_reward() {
        use misaka_palw_kernel::rows::{TABLE_EXEC_REWARD_HOLDS_V1, TABLE_JOB_ESCROWS_V1};
        for ready_at_expiry in [false, true] {
            let (p, mut s, claim, slices, pre) = session_with(1_000_000, ClaimStateV1::Final { final_daa: 105 }, false, None);
            fund_jobs(&mut s, &pre, &slices, 1_000);
            let s = license(&p, &s, claim, 45, 111);
            let s = carry_all(&p, &s, &slices);
            let before = s.bonds[&bond_key(6)].collateral;
            let s = if ready_at_expiry {
                let s = kernel_block(
                    &p,
                    &s,
                    60,
                    130,
                    slices.iter().map(|k| k.write(ClaimStateV1::Final { final_daa: 130 }, false)).collect(),
                );
                assert!(s.exec_v2_root_v1(&claim).unwrap().ready_for_final());
                let expiry = s.exec_v2_root_v1(&claim).unwrap().expiry_daa;
                step(&s, &p, 61, expiry, &[]).unwrap().0
            } else {
                kernel_block(
                    &p,
                    &s,
                    60,
                    130,
                    vec![slices[1].write(ClaimStateV1::Unavailable { daa: 130, producer_defaulted: true }, false)],
                )
            };
            assert!(matches!(s.exec_v2_root_v1(&claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
            assert_eq!(s.bonds[&bond_key(6)].collateral, before);
            assert!(s.vesting_row(&claim).is_none());
            assert!(s.pending_payouts_iter().all(|(_, row)| row.payload != s.bond(&bond_key(20)).unwrap().payout_payload
                && row.payload != s.bond(&bond_key(21)).unwrap().payout_payload));
            assert!(
                s.kernel_route
                    .as_ref()
                    .unwrap()
                    .rows
                    .keys()
                    .all(|(t, _)| *t != TABLE_JOB_ESCROWS_V1 && *t != TABLE_EXEC_REWARD_HOLDS_V1)
            );
        }
    }

    #[test]
    fn paid_prefix_and_paid_slice_cannot_attach_to_an_exec_root() {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let pre = Prefix::new(&mut s, claim, ClaimStateV1::Final { final_daa: 105 }, true);
        assert!(matches!(open(&p, &s, pre.decl(claim)), Err(PalwStateV2Error::ExecV2Refused(_))));
        let (p, s, claim, slices) = session(1000);
        let s = kernel_block(
            &p,
            &s,
            45,
            111,
            vec![slices[0].write_with(|row| {
                row.life.state = ClaimStateV1::Final { final_daa: 111 };
                row.rewarded = true;
            })],
        );
        assert_eq!(s.exec_v2_admit_slice_v1(&p, 112, &slices[0].covered, 0), Err(PalwSliceRefusalV2::VerificationAlreadyPaid));
        assert_eq!(s.exec_v2_root_v1(&claim).unwrap().next_index, 0);
    }

    #[test]
    fn refusal_codes_are_distinct_and_named() {
        use PalwSliceRefusalV2::*;
        let all = [
            Dormant,
            NoRoot,
            RootNotOpen,
            RootExpired,
            RootClaimNotLive,
            ExecutorNotAuthorized,
            ExecutorNotActive,
            ExecutorKeyMismatch,
            IndexUsed,
            SkippedSlice,
            IndexPastPlan,
            RangeOverlap,
            RangeNotPlan,
            PredecessorMismatch,
            ClassMismatch,
            JobMismatch,
            KernelMismatch,
            PlanMismatch,
            RootPendingDepth,
            BondPendingDepth,
            BlockQuota,
            Overflow,
            NotKernelBound,
            VerificationClaimMissing,
            VerificationKindUnsupported,
            VerificationClaimNotBound("x"),
            VerificationClaimFailed,
            ExecutionScopeMismatch,
            VerificationAlreadyPaid,
            VerificationRewardBound,
            VerificationProviderHorizon,
            ExecutorUnfunded,
        ];
        let codes: std::collections::BTreeSet<u8> = all.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), all.len());
        assert!(!codes.contains(&0), "0 is an admitted slice");
        for r in &all {
            assert_ne!(PalwSliceRefusalV2::name_of_code(r.code()), "unknown", "{r:?}");
        }
        assert_eq!(ExecutorUnfunded.code(), 32);
        assert_eq!(PalwSliceRefusalV2::name_of_code(32), "executor_unfunded");
        assert_eq!(VerificationProviderHorizon.code(), 31);
        assert_eq!(PalwSliceRefusalV2::name_of_code(31), "verification_provider_horizon");
    }
}
