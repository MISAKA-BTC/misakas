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
    PalwTransitionExtrasV1 { panel_economy_active: true, ..door_extras(true) }
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
    let (s1, _) = apply_door(&s0, &p, &funded, &[], Some(&env), &xx()).expect("the claim lands");
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
    // The extra executors stand behind the root: the claim's own reservation, in the one committed ledger.
    let reserved = s0.claim(&claim).unwrap().reserved;
    assert!(reserved > 0);
    assert_eq!(root.executor_exposure, reserved);
    assert_eq!(s1.reserved_exposure(&bond_key(20)), exposure_20 + reserved);
    assert_eq!(s1.reserved_exposure(&bond_key(21)), exposure_20 + reserved);
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
    assert_eq!(admit(&good), Ok(PalwSliceAdmissionV2 { work: 240, verified_at: None }));
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
        Ok(PalwSliceAdmissionV2 { work: 240, verified_at: None }),
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
fn run_to_terminal(p: &PalwStateParamsV2, mut s: PalwChainStateV2, claim: Hash64, mut word: u64, mut daa: u64) -> PalwChainStateV2 {
    for _ in 0..400 {
        if s.claim(&claim).is_none_or(|c| c.phase.is_terminal()) {
            return s;
        }
        let at = deadline_of(&s, claim).unwrap_or(daa + 1).max(daa + 1);
        daa = at;
        word += 1;
        s = step(&s, p, word, daa, &[]).expect("an empty block").0;
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
    let payee_of = |bond: PalwBondKeyV2| s.bond(&bond).unwrap().payout_payload;
    let paid_to = |bond: PalwBondKeyV2| {
        s.pending_payouts_iter().filter(|(_, row)| row.payload == payee_of(bond)).map(|(_, row)| row.amount).sum::<u64>()
    };
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
    assert!(s.reserved_exposure(&a) > 0 && s.reserved_exposure(&b) > 0);
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
    assert_eq!(retired.exec_v2_counts_v1(), (0, 0, 0), "and its session, its slices and its job tombstone with it");
    assert!(retired.exec_v2_state_v1().is_empty());
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
        x.reserved_exposure.remove(&bond_key(21));
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
    use crate::palw_exec_v2_verify::{palw_exec_v2_kernel_key_v1, palw_exec_v2_slice_job_nonce_v1, palw_exec_v2_token_state_root_v1};
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

    fn bound_opened() -> (PalwStateParamsV2, PalwChainStateV2, Hash64) {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let s = open(&p, &s, decl(claim)).expect("a session on a kernel-bound class opens");
        (p, s, claim)
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
            s: &PalwChainStateV2,
            root: Hash64,
            index: u32,
            executor: PalwBondKeyV2,
            prompt: &[u32],
            generated: &[u32],
            reserved: u64,
        ) -> Self {
            let mut covered = slice_of(s, root, index, executor);
            let job = KernelJobV1 {
                class_binding_id: digest(h64(KERNEL_CLASS)),
                prompt: prompt.to_vec(),
                max_new_tokens: generated.len() as u32,
                decode: DecodeRuleV1::Greedy,
                nonce: digest(palw_exec_v2_slice_job_nonce_v1(&covered.slice)),
            };
            let kernel_bond = palw_kernel_bond_id_v1(&executor);
            let claim = KernelClaimV1 {
                job_id: job.id(),
                producer_bond: kernel_bond,
                generated: generated.to_vec(),
                evidence_root: digest(h64(0xE0 + index as u64)),
            };
            let row = ClaimRowV1 {
                producer: kernel_bond,
                class_binding_id: digest(h64(KERNEL_CLASS)),
                job_id: job.id(),
                body: ClaimBodyV1::Program { claim: claim.clone(), evidence: evidence(), commitments: Vec::new() },
                committed_daa: 100,
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
            slice.result_state_root = palw_exec_v2_token_state_root_v1(&[prompt, generated]);
            slice.output_root = palw_exec_v2_token_state_root_v1(&[generated]);
            slice.da_root = Hash64::from_bytes(claim.evidence_root);
            KernelSlice { covered, claim_id, job, row }
        }

        /// Put the job and the claim (as it now stands) into the route — state surgery for the setup, outside any block.
        fn install(&self, s: &mut PalwChainStateV2) {
            let route = s.kernel_route.as_mut().expect("a route");
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

    /// The token stream of the session: the prompt and each slice's run.
    const PROMPT: [u32; 3] = [11, 12, 13];
    const RUNS: [[u32; 2]; 3] = [[21, 22], [31, 32], [41, 42]];

    fn prompt_of(index: usize) -> Vec<u32> {
        let mut prompt = PROMPT.to_vec();
        for run in RUNS.iter().take(index) {
            prompt.extend_from_slice(run);
        }
        prompt
    }

    /// The three slices of the session, chained through one token stream, executed by 20, 21, 20, each claim holding `reserved`.
    /// The root's initial boundary must be the prompt's token state: the declaration says so.
    fn session(reserved: u64) -> (PalwStateParamsV2, PalwChainStateV2, Hash64, Vec<KernelSlice>) {
        let (p, mut s, claim) = world(Some(0));
        with_route(&mut s, claim, Some(PLAN));
        let mut d = decl(claim);
        d.initial_state_root = palw_exec_v2_token_state_root_v1(&[&PROMPT]);
        let mut s = open(&p, &s, d).expect("the session opens");
        let executors = [bond_key(20), bond_key(21), bond_key(20)];
        let mut slices = Vec::new();
        for (i, executor) in executors.iter().enumerate() {
            let k = KernelSlice::new(&s, claim, i as u32, *executor, &prompt_of(i), &RUNS[i], reserved);
            k.install(&mut s);
            slices.push(k);
        }
        // Chain: slice i+1's predecessor is slice i's result, so `slice_of`'s placeholder predecessor is replaced by the token state.
        for (i, k) in slices.iter().enumerate() {
            assert_eq!(k.covered.slice.predecessor_state_root, palw_exec_v2_token_state_root_v1(&[&prompt_of(i)]));
        }
        (p, s, claim, slices)
    }

    fn carry_all(p: &PalwStateParamsV2, s: &PalwChainStateV2, slices: &[KernelSlice]) -> PalwChainStateV2 {
        fold_slices(p, s, 50, 120, slices.iter().map(|k| k.covered.clone()).collect())
    }

    fn kernel_block(
        p: &PalwStateParamsV2,
        s: &PalwChainStateV2,
        word: u64,
        daa: u64,
        rows: Vec<(u8, Vec<u8>, Option<Vec<u8>>)>,
    ) -> PalwChainStateV2 {
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
        open(&p, &s, decl(claim)).expect("bound, under its plan");
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
        assert!(matches!(s.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Verified { verified_daa: 130 }));
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
    fn a_slice_leg_is_capped_at_what_the_route_still_holds_on_its_claims() {
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
        let payee_of = |bond: PalwBondKeyV2| s.bond(&bond).unwrap().payout_payload;
        let paid_to = |bond: PalwBondKeyV2| {
            s.pending_payouts_iter().filter(|(_, row)| row.payload == payee_of(bond)).map(|(_, row)| row.amount).sum::<u64>()
        };
        let producer = s.vesting_row(&claim).expect("the vested row").producer.amount;
        let (leg_a, leg_b) = (paid_to(bond_key(20)), paid_to(bond_key(21)));
        assert_eq!(leg_a, 14, "capped at the collateral the route holds on 20's two claims");
        // The twin's whole producer leg is conserved: what 20 could not be paid stayed with the root executor.
        let (pt, st, claim_t) = world(Some(0));
        let twin = run_to_terminal(&pt, license(&pt, &st, claim_t, 45, 111), claim_t, 70, 111);
        let twin_leg = twin.vesting_row(&claim_t).unwrap().producer.amount;
        assert_eq!(producer + leg_a + leg_b, twin_leg, "nothing minted, nothing lost");
        assert_eq!(leg_b, (twin_leg as u128 * 240 / 800) as u64, "21's leg is under its cap");
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
        slices[0].install(&mut s);
        // Slice 0 (Final already) and a copy of slice 0 by the wrong executor in the same block.
        let mut wrong = slices[0].covered.clone();
        wrong.carrier = h64(0x5FFF);
        wrong.slice.executor_bond = bond_key(21);
        wrong.pubkey = s.bond(&bond_key(21)).unwrap().pubkey.clone();
        let (after, delta) = step_with(&s, &p, 50, 120, &[], &xx_slices(vec![slices[0].covered.clone(), wrong.clone()])).unwrap();
        assert!(matches!(after.exec_v2_slice_v1(&claim, 0).unwrap().stage, PalwWorkSliceStageV2::Verified { verified_daa: 120 }));
        let root = after.exec_v2_root_v1(&claim).unwrap();
        assert_eq!((root.pending, root.verified_work), (0, 400 - PWU), "verified at once: never pending");
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
        ];
        let codes: std::collections::BTreeSet<u8> = all.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), all.len());
        assert!(!codes.contains(&0), "0 is an admitted slice");
        for r in &all {
            assert_ne!(PalwSliceRefusalV2::name_of_code(r.code()), "unknown", "{r:?}");
        }
    }
}
