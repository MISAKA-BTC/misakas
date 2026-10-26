//! Private mechanics regression: real kernel claim, native paid Lock/Receipt folds, then op247 reader.
use super::*;
use crate::palw_claim_challenge_v2::*;
use crate::palw_kernel_route_v1::*;
use crate::palw_paid_claim_read_v1::palw_paid_claim_read_v1;
use misaka_palw_kernel::{claim_paid_v2::*, hash::object_id};
#[path = "../../../../../misaka/palw/kernel/tests/common/mod.rs"]
mod fixture;

fn setup() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2) {
    setup_mode(false)
}
fn setup_mode(fs: bool) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2) {
    setup_mode_contract(fs, false)
}
fn setup_mode_contract(fs: bool, staged: bool) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2) {
    setup_mode_contract_opv(fs, staged, None)
}
/// Same fixture with the route folded from genesis under an OPV policy (the
/// kernel's source-fee gate requires `optimistic_allowed`). The policy binds
/// the stored route, so it cannot be added to the extras afterwards.
fn setup_mode_contract_opv(
    fs: bool,
    staged: bool,
    opv: Option<misaka_palw_kernel::opv::OpvPolicyV1>,
) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2) {
    use fixture::{
        chain::{T, block_of, salted, seals_for},
        ledger_world::World,
    };
    let producer = bond_key(300);
    let producer_id = palw_kernel_bond_id_v1(&producer);
    let network = h64(1);
    let rules = h64(2);
    let policy = palw_kernel_route_policy_v1(network, rules);
    let mut route = PalwKernelRouteStateV1::new(policy, opv, Default::default());
    let mut l = route.template();
    let mut w = World::new();
    let candidates: Vec<_> =
        (301..304).map(|i| PalwPaidCheckerCandidateV2 { bond: bond_key(i), operator: h64(i), weight: 1 }).collect();
    let mut txs = vec![
        T::RegisterBond { bond: producer_id, collateral: 1_000_000_000_000_000 },
        T::RegisterBond { bond: fixture::chain::POSTER, collateral: 1_000_000_000_000_000 },
        w.register(),
    ];
    for c in &candidates {
        txs.push(T::RegisterBond { bond: palw_kernel_bond_id_v1(&c.bond), collateral: 1_000_000_000_000_000 });
    }
    let events = l.apply_block(&block_of(1, txs, producer_id));
    w.class = events
        .iter()
        .find_map(|e| if let misaka_palw_kernel::ledger::LedgerEventV1::ClassRegistered { class } = e { Some(*class) } else { None })
        .expect("real registered class");
    let base_job = misaka_palw_kernel::job::KernelJobV1 {
        class_binding_id: w.class,
        prompt: vec![3],
        max_new_tokens: 1,
        decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
        nonce: [7; 64],
    };
    l.apply_block(&block_of(2, vec![T::PostJob { job: base_job.clone() }], producer_id));
    w.l = l;
    let produced = w.produce(&base_job, producer_id, w.greedy(&w.params, &base_job.prompt, 1), &w.params, |_| {});
    let base = produced.claim.id();
    let txs = salted(&w.l, vec![produced.tx]);
    w.l.apply_block(&block_of(3, seals_for(&txs), producer_id));
    w.l.apply_block(&block_of(4, txs, producer_id));
    assert!(w.l.claims.contains_key(&base), "real claim commit");
    let mut class = PaidClaimClassV2 {
        version: 2,
        registrant: producer_id,
        base_class: w.class,
        target: PaidClaimTargetV2::Program,
        checking: PaidClaimCheckingV2::Panel,
        checker_contract: checker_contract_v2(),
        source_policy: misaka_palw_challenge::policy::paid_opt_in_source_policy_v5(1, 1, 10, 1, 2),
        check_window_daa: 1000,
        challenge_window_daa: 100,
        court_and_grace_daa: 100,
        liability_daa: 1000,
        maximum_stream_blocks: 64,
        maximum_rounds: 1,
        maximum_commit_delay_daa: 10,
        review_scope: None,
        interactive_plan: None,
    };
    if fs {
        let mode = if staged { misaka_palw_challenge::InteractiveModeV1::StagedBeacon } else { misaka_palw_challenge::InteractiveModeV1::TranscriptBoundFiatShamir };
        class.maximum_rounds = 2;
        class.source_policy.interactive_mode = mode;
        class.source_policy.transcript_transform_id = if staged { misaka_palw_challenge::ImplementedV1::transcript_staged_beacon() } else { misaka_palw_challenge::ImplementedV1::transcript_fiat_shamir() };
        class.interactive_plan = Some(
            misaka_palw_kernel::claim_interactive_v3::CompiledClaimProofPlanV3::compile(
                class.target,
                mode,
                2,
                4096,
                class.maximum_stream_blocks,
            )
            .unwrap(),
        );
    }
    let job = PaidClaimJobV2 {
        version: 2,
        class: class.id(),
        base_job: base_job.id(),
        payer: fixture::chain::POSTER,
        nonce: [8; 64],
        last_commit_daa: 100,
        maximum_consumers: 1,
    };
    let enrollment = PaidClaimEnrollmentV2 {
        version: 2,
        class: class.id(),
        job: job.id(),
        base_claim: base,
        producer: producer_id,
        commit_root: w.l.claim_challenge_subject(&base).unwrap().commitment_root,
        lifetime: PaidClaimLifetimeV2::of(&class, 4).unwrap(),
        accepted_until_daa: 14,
        material_consent: [10; 64],
        round: 0,
        prior_transcript: [0; 64],
    };
    let subject = paid_subject_with_checker_snapshot_v2(
        &w.l.claim_challenge_subject(&base).unwrap(),
        &class,
        &enrollment,
        paid_checker_snapshot_root_v2(&candidates),
    )
    .unwrap();
    let context = paid_context_v2(&subject, &class, &enrollment, [[11; 64]].into_iter().collect(), 4);
    let source_policy = crate::palw_beacon_source_opt_in_v1::PalwBeaconSourcePolicyV1 {
        deposit_sompi: 100,
        max_per_producer: 8,
        max_total: 64,
        source_final_slack_daa: 120,
    };
    let source_target = claim_source_target_v2(source_policy, context.clone(), enrollment.lifetime, &policy).unwrap();
    let a = PalwPaidClaimAuthorityV2 {
        version: 2,
        enrollment,
        class,
        job,
        subject,
        context,
        source_target,
        source_policy,
        authorized_daa: 4,
        producer,
        sponsor: None,
        checker_candidates: candidates,
    };
    a.validate(&w.l).unwrap();
    route.header.scalars = w.l.scalars();
    route.rows = w.l.to_rows();
    for (prefix, owner, kind) in [
        (PAID_CLAIM_BASE_INDEX_PREFIX_V3, base, PalwPaidClaimIndexKindV3::BaseClaim),
        (PAID_CLAIM_ID_INDEX_PREFIX_V3, a.enrollment.id(), PalwPaidClaimIndexKindV3::Enrollment),
    ] {
        route.aux.insert(
            (PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(prefix, &owner)),
            borsh::to_vec(&PalwPaidClaimIndexV3 { version: 3, kind, owner, target: a.context.id() }).unwrap(),
        );
    }
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec()), borsh::to_vec(&a).unwrap());
    for bond in std::iter::once(&producer).chain(a.checker_candidates.iter().map(|c| &c.bond)) {
        route.aux.insert((PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, palw_kernel_bond_id_v1(bond).to_vec()), borsh::to_vec(bond).unwrap());
    }
    let mut state = PalwChainStateV2::genesis();
    state.kernel_route = Some(route);
    for c in &a.checker_candidates {
        state.bonds.insert(
            c.bond,
            PalwBondStateV2 {
                pubkey: vec![1; 4],
                operator_id: c.operator,
                collateral: 1_000_000_000_000_000,
                slashed: 0,
                status: PalwBondStatusV2::Active,
                registered_daa: 0,
                payout_payload: h64(90),
                capable_classes: Default::default(),
            },
        );
    }
    state.bonds.insert(
        producer,
        PalwBondStateV2 {
            pubkey: vec![1; 4],
            operator_id: h64(300),
            collateral: 1_000_000_000_000_000,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h64(90),
            capable_classes: Default::default(),
        },
    );
    let mut p = params();
    let mut fence = PalwClaimChallengeFenceV2::unapproved_probe_at(crate::config::params::ForkActivation::always());
    fence.policy.bootstrap_seed = Some([12; 64]);
    p.palw_claim_challenge_v2 = Some(fence);
    let extras = PalwTransitionExtrasV1 {
        kernel_route: Some(PalwKernelRouteExtrasV1 {
            network_domain: network,
            ruleset_digest: rules,
            opv: opv.map(|policy| crate::palw_kernel_route_v1::PalwKernelOpvExtrasV1 {
                policy,
                denied_classes: vec![],
                min_effective_bits: 0,
                sampled_conformance_gates_reward: false,
                test_eligible: vec![],
                reward_gate: None,
                budget_required: false,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    (state, p, extras, a)
}

#[test]
fn native_lock_and_complete_receipts_are_visible_and_stale_receipts_refused() {
    let (state, p, extras, a) = setup();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let enrollment = a.enrollment.id();
    let base = a.enrollment.base_claim;
    super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(
        &mut b,
        &ctx(5, 5, 5),
        &a.producer,
        &PaidClaimObjectV2::Lock { enrollment },
    )
    .unwrap();
    super::super::palw_kernel_route_fold_v1::tick_kernel_route_v1(&mut b, &ctx(5, 5, 5)).unwrap();
    let locked = palw_paid_claim_read_v1(b.state.kernel_route().unwrap(), &base, 0).unwrap().unwrap();
    assert_eq!(locked.state, "round-locked");
    assert_eq!(locked.quorum, 3);
    assert_eq!(locked.seats.len(), 3);
    let lock = b.state.kernel_route().unwrap().paid_claim_effective_lock_v6(&a).unwrap().unwrap();
    for seat in &lock.seats {
        let receipt = PaidClaimReceiptV2 {
            version: 2,
            enrollment,
            verification_class: a.class.id(),
            assignment: lock.assignment,
            source_context: lock.context,
            sample_seed: lock.seed,
            evidence_root: a.enrollment.commit_root,
            transcript_root: a.enrollment.prior_transcript,
            signer: palw_kernel_bond_id_v1(seat),
            signed_daa: 6,
            complete: true,
            round: 0,
            message_root: [0; 64],
        };
        super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(
            &mut b,
            &ctx(6, 6, 6),
            seat,
            &PaidClaimObjectV2::Receipt { receipt },
        )
        .unwrap();
    }
    let route = b.state.kernel_route.as_ref().unwrap();
    let complete = palw_paid_claim_read_v1(route, &base, 0).unwrap().unwrap();
    assert_eq!(complete.state, "round-complete");
    assert_eq!(complete.receipts.len(), 3);
    assert_eq!(palw_paid_claim_read_v1(route, &base, 1).unwrap().unwrap().quorum, 0);
    let first_receipt = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, palw_kernel_bond_id_v1(&lock.seats[0])));
    for change in 0..5 {
        let mut corrupt = route.clone();
        let mut receipt: PaidClaimReceiptV2 =
            borsh::from_slice(corrupt.aux.get(&(PAID_CLAIM_RECEIPTS_V2, first_receipt.to_vec())).unwrap()).unwrap();
        match change {
            0 => receipt.enrollment = [88; 64],
            1 => receipt.round = 1,
            2 => receipt.signer = [77; 64],
            3 => receipt.signed_daa = lock.locked_daa - 1,
            _ => receipt.signed_daa = 7,
        }
        corrupt.aux.insert((PAID_CLAIM_RECEIPTS_V2, first_receipt.to_vec()), borsh::to_vec(&receipt).unwrap());
        assert!(palw_paid_claim_read_v1(&corrupt, &base, 0).is_err(), "receipt substitution {change}");
    }
    let mut wrong_lock = route.clone();
    let mut wrong = (*lock).clone();
    wrong.enrollment = [66; 64];
    wrong_lock.aux.insert((PAID_CLAIM_LOCKS_V2, enrollment.to_vec()), borsh::to_vec(&wrong).unwrap());
    assert!(palw_paid_claim_read_v1(&wrong_lock, &base, 0).is_err());
    let mut bad_index = route.clone();
    bad_index.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &base)), vec![255]);
    assert!(palw_paid_claim_read_v1(&bad_index, &base, 0).is_err());
    let mut stale = route.clone();
    let mut changed = (*lock).clone();
    changed.seed = [99; 64];
    changed.assignment =
        object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &(enrollment, changed.seed, &changed.seats, changed.quorum));
    stale.aux.insert((PAID_CLAIM_LOCKS_V2, enrollment.to_vec()), borsh::to_vec(&changed).unwrap());
    assert!(palw_paid_claim_read_v1(&stale, &base, 0).is_err());
    for (table, key) in [
        (PAID_CLAIM_FINALS_V2, enrollment.to_vec()),
        (PAID_CLAIM_LOCKS_V2, enrollment.to_vec()),
        (
            PAID_CLAIM_RECEIPTS_V2,
            object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, palw_kernel_bond_id_v1(&lock.seats[0]))).to_vec(),
        ),
    ] {
        let mut malformed = route.clone();
        malformed.aux.insert((table, key), vec![255]);
        assert!(palw_paid_claim_read_v1(&malformed, &base, 0).is_err(), "table {table}");
    }
    let now = a.enrollment.lifetime.dispute_until;
    super::super::palw_kernel_route_fold_v1::tick_kernel_route_v1(&mut b, &ctx(now, now, now)).unwrap();
    let final_route = b.state.kernel_route().unwrap();
    let final_view = palw_paid_claim_read_v1(final_route, &base, 0).unwrap().unwrap();
    assert_eq!(final_view.state, "final");
    assert_eq!(final_view.final_daa, Some(now));
    assert_eq!(palw_paid_claim_read_v1(final_route, &base, 1).unwrap().unwrap().state, "final");
    assert_broken_finals(final_route, &a);
}

#[test]
fn fs_projection_keeps_initial_source_lock_separate_from_round_approval() {
    // Projection compatibility control, not a funded source or native StageMessage campaign.
    let (mut state, p, extras, a) = setup_mode(true);
    let enrollment = a.enrollment.id();
    let base = a.enrollment.base_claim;
    let mut ranked: Vec<_> = a
        .checker_candidates
        .iter()
        .map(|c| (palw_kernel_interim_key_v1(&Hash64::from_bytes([20; 64]), &c.operator, 1), c.bond))
        .collect();
    ranked.sort();
    let seats: Vec<_> = ranked.into_iter().map(|(_, bond)| bond).collect();
    let initial = PalwPaidClaimLockV2 {
        enrollment,
        context: a.context.id(),
        anchor: [19; 64],
        seed: [20; 64],
        locked_daa: 5,
        assignment: object_id(b"MISAKA/PALW/PAID-CLAIM-ASSIGNMENT/V2", &(enrollment, [20u8; 64], &seats, 3u32)),
        seats: seats.clone(),
        quorum: 3,
        source_valid: true,
    };
    let route = state.kernel_route.as_mut().unwrap();
    route.header.scalars.daa = 10;
    route.aux.insert((PAID_CLAIM_LOCKS_V2, enrollment.to_vec()), borsh::to_vec(&initial).unwrap());
    let initial_only = palw_paid_claim_read_v1(route, &base, 0).unwrap().unwrap();
    assert_eq!(initial_only.quorum, 0);
    assert!(!initial_only.round_found);
    let mut row = crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 {
        enrollment,
        round: 0,
        prior_transcript: [0; 64],
        message_root: [22; 64],
        accepted_daa: 6,
        subject: a.subject.clone(),
        context: a.context.clone(),
        target: a.source_target.clone(),
        sponsor: a.sponsor,
        lock: None,
        transcript_root: [0; 64],
        receipts: vec![],
    };
    let key = misaka_palw_kernel::claim_interactive_v3::round_id_v3(enrollment, 0).to_vec();
    route.aux.insert((PAID_CLAIM_ROUNDS_V2, key.clone()), borsh::to_vec(&row).unwrap());
    assert_eq!(palw_paid_claim_read_v1(route, &base, 0).unwrap().unwrap().state, "round-open");
    let mut round_lock = initial.clone();
    round_lock.seed = [23; 64];
    round_lock.locked_daa = 7;
    round_lock.assignment = object_id(b"MISAKA/PALW/CLAIM-PROOF-ROUND-ASSIGNMENT/V3", &(row.id(), round_lock.seed, &seats, 3u32));
    row.transcript_root = misaka_palw_kernel::claim_interactive_v3::transcript_root_v3(
        a.class.interactive_plan.as_ref().unwrap(),
        enrollment,
        0,
        row.prior_transcript,
        row.message_root,
        round_lock.context,
        round_lock.seed,
    );
    row.lock = Some(round_lock);
    route.aux.insert((PAID_CLAIM_ROUNDS_V2, key), borsh::to_vec(&row).unwrap());
    let observed = palw_paid_claim_read_v1(route, &base, 0).unwrap().unwrap();
    assert_eq!(observed.state, "round-locked");
    assert_eq!(observed.seats, seats);
    assert!(observed.receipts.is_empty());
    let signer = palw_kernel_bond_id_v1(&initial.seats[0]);
    let receipt = PaidClaimReceiptV2 {
        version: 2,
        enrollment,
        verification_class: a.class.id(),
        assignment: initial.assignment,
        source_context: initial.context,
        sample_seed: initial.seed,
        evidence_root: a.enrollment.commit_root,
        transcript_root: a.enrollment.prior_transcript,
        signer,
        signed_daa: 8,
        complete: true,
        round: 0,
        message_root: [0; 64],
    };
    route.aux.insert(
        (PAID_CLAIM_RECEIPTS_V2, object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, signer)).to_vec()),
        borsh::to_vec(&receipt).unwrap(),
    );
    assert!(palw_paid_claim_read_v1(route, &base, 0).is_err());
    route.aux.remove(&(PAID_CLAIM_RECEIPTS_V2, object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(enrollment, signer)).to_vec()));
    // Canonical completed round projections only: no source collection or RoundReceipt acceptance is claimed.
    let mut prior = [0; 64];
    for round in 0..2 {
        row.round = round;
        row.prior_transcript = prior;
        row.message_root = [30 + round as u8; 64];
        let mut lock = initial.clone();
        lock.seed = [40 + round as u8; 64];
        lock.locked_daa = 7;
        lock.assignment = object_id(b"MISAKA/PALW/CLAIM-PROOF-ROUND-ASSIGNMENT/V3", &(row.id(), lock.seed, &seats, 3u32));
        row.transcript_root = misaka_palw_kernel::claim_interactive_v3::transcript_root_v3(
            a.class.interactive_plan.as_ref().unwrap(),
            enrollment,
            round,
            prior,
            row.message_root,
            lock.context,
            lock.seed,
        );
        row.receipts = seats
            .iter()
            .map(|seat| PaidClaimReceiptV2 {
                version: 2,
                enrollment,
                verification_class: a.class.id(),
                assignment: lock.assignment,
                source_context: lock.context,
                sample_seed: lock.seed,
                evidence_root: a.enrollment.commit_root,
                transcript_root: row.transcript_root,
                signer: palw_kernel_bond_id_v1(seat),
                signed_daa: 8,
                complete: true,
                round,
                message_root: row.message_root,
            })
            .collect();
        row.lock = Some(lock);
        prior = row.transcript_root;
        route.aux.insert((PAID_CLAIM_ROUNDS_V2, row.id().to_vec()), borsh::to_vec(&row).unwrap());
    }
    let now = a.enrollment.lifetime.dispute_until;
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let context = ctx(now, now, now);
    // The kernel clock and actual Final projection writer are exercised; the synthetic FS source rows
    // deliberately do not stand in for a native funded-source/StageMessage campaign or settlement run.
    let mut ledger = super::super::palw_kernel_route_fold_v1::load_ledger(&mut b, &context).unwrap();
    let before = super::super::palw_kernel_route_fold_v1::loaded_rows(&mut b, &ledger).unwrap();
    ledger.tick_reserved_v1().unwrap();
    super::super::palw_kernel_route_fold_v1::flush(&mut b, &ledger, &before).unwrap();
    super::super::palw_claim_challenge_fold_v2::tick_paid_claim_final_projection_v2(&mut b, &context).unwrap();
    let final_route = b.state.kernel_route().unwrap();
    assert_eq!(palw_paid_claim_read_v1(final_route, &base, 0).unwrap().unwrap().state, "final");
    assert_broken_finals(final_route, &a);
    let last_key = (PAID_CLAIM_ROUNDS_V2, misaka_palw_kernel::claim_interactive_v3::round_id_v3(enrollment, 1).to_vec());
    for change in 0..3 {
        let mut future = final_route.clone();
        let mut last: crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 =
            borsh::from_slice(future.aux.get(&last_key).unwrap()).unwrap();
        match change {
            0 => last.accepted_daa = now + 1,
            1 => last.lock.as_mut().unwrap().locked_daa = now + 1,
            _ => last.receipts[0].signed_daa = now + 1,
        }
        future.aux.insert(last_key.clone(), borsh::to_vec(&last).unwrap());
        assert!(palw_paid_claim_read_v1(&future, &base, 0).is_err(), "future last-round clock {change}");
    }
    let mut wrong = final_route.clone();
    let final_key = (PAID_CLAIM_FINALS_V2, enrollment.to_vec());
    let mut final_row: PalwPaidClaimFinalV2 = borsh::from_slice(wrong.aux.get(&final_key).unwrap()).unwrap();
    let first: crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3 = borsh::from_slice(
        wrong.aux.get(&(PAID_CLAIM_ROUNDS_V2, misaka_palw_kernel::claim_interactive_v3::round_id_v3(enrollment, 0).to_vec())).unwrap(),
    )
    .unwrap();
    final_row.sample_seed = first.lock.unwrap().seed;
    final_row.transcript_root = first.transcript_root;
    wrong.aux.insert(final_key, borsh::to_vec(&final_row).unwrap());
    assert!(palw_paid_claim_read_v1(&wrong, &base, 0).is_err(), "requested first round cannot authenticate last-round Final");
}

fn assert_broken_finals(route: &PalwKernelRouteStateV1, a: &PalwPaidClaimAuthorityV2) {
    let enrollment = a.enrollment.id();
    let base = a.enrollment.base_claim;
    let original: PalwPaidClaimFinalV2 =
        borsh::from_slice(route.aux.get(&(PAID_CLAIM_FINALS_V2, enrollment.to_vec())).unwrap()).unwrap();
    for change in 0..10 {
        let mut bad = route.clone();
        let mut row = original.clone();
        match change {
            0 => row.enrollment = [55; 64],
            1 => row.base_claim = [56; 64],
            2 => row.verification_class = [57; 64],
            3 => row.sample_seed = [58; 64],
            4 => row.source_context = [59; 64],
            5 => row.signed_lifetime.checking_until += 1,
            6 => row.complete_receipts = 0,
            7 => row.logical_final_daa += 1,
            8 => row.completed_rounds += 1,
            _ => row.transcript_root = [60; 64],
        }
        bad.aux.insert((PAID_CLAIM_FINALS_V2, enrollment.to_vec()), borsh::to_vec(&row).unwrap());
        assert!(palw_paid_claim_read_v1(&bad, &base, 0).is_err(), "Final substitution {change}");
    }
}

#[path = "paid_claim_bootstrap_fail_closed_v6.rs"]
mod bootstrap_fail_closed_v6;

#[path = "paid_trap_checked_v1.rs"]
mod paid_trap_checked_v1;

#[path = "paid_check_authority_resource_v2.rs"]
mod paid_check_authority_resource_v2;

#[path = "rfc07_normal_source_owned_tests.rs"]
mod rfc07_normal_source_owned_tests;
