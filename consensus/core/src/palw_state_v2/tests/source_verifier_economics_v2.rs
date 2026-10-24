//! Synthetic native mechanism fixtures, not signed native campaign evidence.
use super::*;
use crate::palw_bond_budget_v1::{PalwBondBudgetMirrorV1, PalwBondBudgetPolicyV1};
use crate::palw_kernel_route_v1::*;
use crate::palw_source_bootstrap_v1::*;
#[path = "../../../../../misaka/palw/kernel/tests/common/mod.rs"]
mod fixture;
fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([n; 64]), index: 0 })
}
fn ctx(n: u8, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(n), daa_score: daa, blue_score: daa, subsidy: 0 }
}
fn source_put<T: borsh::BorshSerialize>(s: &mut PalwChainStateV2, k: Vec<u8>, v: &T) {
    s.kernel_route.as_mut().unwrap().aux.insert((PALW_SELECTED_SOURCE_BOOTSTRAP_TABLE_V1, k), borsh::to_vec(v).unwrap());
}
fn econ_put<T: borsh::BorshSerialize>(s: &mut PalwChainStateV2, k: Vec<u8>, v: &T) {
    s.kernel_route.as_mut().unwrap().aux.insert((SOURCE_VERIFIER_ECONOMICS_TABLE_V2, k), borsh::to_vec(v).unwrap());
}
struct Fx {
    state: PalwChainStateV2,
    p: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
    epoch: PalwSourceBootstrapEpochRowV1,
    work: PalwSourceBootstrapWorkV1,
    terms: PalwSourceVerifierFeeTermsV2,
}
fn setup(r: bool) -> Fx {
    use fixture::{
        chain::{T, block_of, salted, seals_for},
        ledger_world::World,
    };
    let producer = bond(1);
    let producer_id = palw_kernel_bond_id_v1(&producer);
    let policy = palw_kernel_route_policy_v1(h(9), h(3));
    let mut opv = fixture::opv_world::opv_example();
    // Derive a valid synthetic reservation under the actual native policy;
    // these values remain mechanism inputs, never network acceptance.
    opv.economics.reservation_per_claim = u64::try_from(opv.required_reservation(&policy)).unwrap().checked_add(1).unwrap();
    opv.validate(&policy).unwrap();
    let mut route = PalwKernelRouteStateV1::new(policy, Some(opv), Default::default());
    let mut w = World::with_opv(policy, opv);
    let mut l = route.template();
    let txs = vec![
        T::RegisterBond { bond: producer_id, collateral: 1_000_000_000_000_000 },
        T::RegisterBond { bond: fixture::chain::POSTER, collateral: 1_000_000_000_000_000 },
    ];
    let mut registration = block_of(1, txs, producer_id);
    registration.txs.extend(fixture::opv_world::register_in(
        &w,
        misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification,
        producer_id,
    ));
    w.class = l
        .apply_block(&registration)
        .iter()
        .find_map(|e| if let misaka_palw_kernel::ledger::LedgerEventV1::ClassRegistered { class } = e { Some(*class) } else { None })
        .unwrap();
    let job = misaka_palw_kernel::job::KernelJobV1 {
        class_binding_id: w.class,
        prompt: vec![3],
        max_new_tokens: 1,
        decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
        nonce: [7; 64],
    };
    l.apply_block(&block_of(31, vec![T::PostJob { job: job.clone() }], producer_id));
    w.l = l;
    let produced = w.produce(&job, producer_id, w.greedy(&w.params, &job.prompt, 1), &w.params, |_| {});
    let claim = Hash64::from_bytes(produced.claim.id());
    let txs = salted(&w.l, vec![produced.tx]);
    w.l.apply_block(&block_of(33, seals_for(&txs), producer_id));
    w.l.apply_block(&block_of(34, txs, producer_id));
    assert!(w.l.claims.contains_key(&claim.as_bytes()), "actual OPV source claim");
    let class = w.l.classes.get(&w.class).unwrap();
    let header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: class.network_domain,
        ruleset_digest: class.ruleset_digest,
        class_binding_id: w.class,
        program_root: misaka_palw_kernel::public::program_root_v1(&class.program_bytes),
        artifact_root: class.param_commitments.root(),
        plan_root: hash_serialized(misaka_palw_kernel::plan::PLAN_ROOT_DOMAIN_V1, &class.plan),
    };
    let source = PalwSelectedSourceBootstrapPolicyV1 {
        version: 1,
        chain_genesis: h(3),
        network: Hash64::from_bytes(policy.network_domain),
        ruleset: Hash64::from_bytes(policy.ruleset_digest),
        epoch: 1,
        freeze_cutoff_daa: 10,
        anchor_cutoff_daa: 20,
        starts_at_daa: 30,
        ends_at_daa: 100,
        stake_release_daa: 2000,
        selection: misaka_palw_challenge::bootstrap_epoch_v1::BootstrapPolicyV1 {
            version: 1,
            seats: 2,
            quorum: 2,
            maximum_population: 2,
            maximum_draw_attempts: 100,
            maximum_source_works: 2,
        },
        source_profile: Hash64::from_bytes(w.class),
        checker_scope: Hash64::from_bytes(hash_serialized(b"MISAKA/PALW/SOURCE-COMPLETE-CHECKER/V1", &header)),
        maximum_claim_bytes: u64::MAX,
        maximum_check_work: u64::MAX,
        maximum_history_headers: 100,
        security_review: h(8),
        economic_review: h(9),
    };
    let mut population = vec![
        PalwSourceBootstrapMemberV1 { bond: bond(2), locked_weight: 1 },
        PalwSourceBootstrapMemberV1 { bond: bond(3), locked_weight: 3 },
    ];
    population.sort_unstable_by_key(|m| m.candidate().bond);
    let context = PalwSourceBootstrapOwnContextV1 {
        policy: source,
        frozen_block: h(10),
        frozen_state_root: h(11),
        frozen_block_daa: 10,
        population,
        population_refused: false,
        anchor: Some((h(12), 20)),
    };
    let candidates = context.population.iter().map(|m| m.candidate()).collect::<Vec<_>>();
    let committee = misaka_palw_challenge::bootstrap_epoch_v1::select_bootstrap_committee_v1(
        &source.selection,
        &context.epoch().unwrap(),
        &candidates,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    let mut epoch = PalwSourceBootstrapEpochRowV1 {
        context: context.clone(),
        cohort: committee.seats().to_vec(),
        draw_refused: false,
        source_works: 0,
    };
    route.header.scalars = w.l.scalars();
    route.rows = w.l.to_rows();
    for actor in (1..=3).map(bond) {
        route
            .aux
            .insert((PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, palw_kernel_bond_id_v1(&actor).to_vec()), borsh::to_vec(&actor).unwrap());
    }
    let mut state = PalwChainStateV2::genesis();
    state.kernel_route = Some(route);
    state.last_point = Some(ctx(34, 34));
    for n in 1..=3 {
        state.bonds.insert(
            bond(n),
            PalwBondStateV2 {
                pubkey: vec![n],
                operator_id: h(n),
                collateral: 1_000_000_000_000_000,
                slashed: 0,
                status: PalwBondStatusV2::Active,
                registered_daa: 0,
                payout_payload: h(n + 70),
                capable_classes: Default::default(),
            },
        );
    }
    source_put(&mut state, vec![0], &epoch);
    let work = state.source_bootstrap_prepare_work_v1(claim).unwrap();
    let economics = PalwSourceVerifierEconomicsPolicyV2 {
        version: 2,
        network: source.network,
        bootstrap_policy: source.id(),
        exposure_per_work_sompi: 7,
        minimum_fee_per_checker_sompi: 5,
        maximum_funded_works: 2,
        maximum_liabilities: 4,
    };
    let mut p = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
    p.selected_source_bootstrap_v1 = Some((0, source));
    p.selected_source_economics_v2 = Some((0, economics));
    if r {
        p = p.with_bond_budget(Some(PalwBondBudgetMirrorV1 {
            from_daa: 1,
            policy: PalwBondBudgetPolicyV1::unapproved_probe_v1(),
            allocation: None,
        }));
    }
    let mut extras = PalwTransitionExtrasV1::default();
    extras.kernel_route = Some(PalwKernelRouteExtrasV1 {
        network_domain: source.network,
        ruleset_digest: source.ruleset,
        opv: Some(PalwKernelOpvExtrasV1 {
            policy: opv,
            denied_classes: vec![],
            min_effective_bits: 0,
            sampled_conformance_gates_reward: false,
            test_eligible: vec![],
            reward_gate: None,
            budget_required: false,
        }),
        ..Default::default()
    });
    let bytes = borsh::to_vec(&context).unwrap();
    extras.selected_source_bootstrap_v1 = Some(std::sync::Arc::new(
        crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(&bytes, || borsh::from_slice(&bytes)).unwrap(),
    ));
    {
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        prepare(&mut b, &ctx(35, 35)).unwrap();
        state = b.state;
    }
    epoch.source_works = 1;
    source_put(&mut state, vec![0], &epoch);
    source_put(&mut state, key_v2(1, work.id()).to_vec(), &PalwSourceBootstrapWorkRowV1 { work, approvals: vec![], settlement: None });
    source_put(&mut state, key_v2(2, work.canonical_work).to_vec(), &work.id());
    let terms = PalwSourceVerifierFeeTermsV2 {
        version: 2,
        policy: economics.id(),
        work: work.id(),
        sponsor: producer,
        fee_per_checker_sompi: 5,
    };
    Fx { state, p, extras, epoch, work, terms }
}
fn funding(b: &TransitionBuilder<'_>, work: Hash64) -> PalwSourceVerifierFundingV2 {
    *read_v2::<PalwSourceVerifierFundingV2, 65>(route(b).unwrap(), &key_v2(1, work)).unwrap().unwrap()
}
fn duty(b: &TransitionBuilder<'_>, actor: PalwBondKeyV2, work: Hash64) -> PalwSourceVerifierLiabilityV2 {
    *read_v2::<PalwSourceVerifierLiabilityV2, 133>(route(b).unwrap(), &liability_key_v2(&actor, work)).unwrap().unwrap()
}
fn all_approvals(b: &mut TransitionBuilder<'_>, f: &Fx) {
    for (slot, seat) in f.epoch.cohort.iter().enumerate() {
        let actor = (2..=3).map(bond).find(|a| palw_kernel_bond_id_v1(a) == seat.bond).unwrap();
        super::super::source_bootstrap_v1::apply(
            b,
            &ctx(36, 36),
            actor,
            &PalwSourceBootstrapActionV1::ApproveWithLiabilityV2 {
                work: f.work.id(),
                slot: slot as u16,
                statement: f.work.id(),
                signed_daa: 36,
                funding_id: f.terms.id(),
            },
        )
        .unwrap();
    }
}
fn fund_action(b: &mut TransitionBuilder<'_>, f: &Fx) {
    super::super::source_bootstrap_v1::apply(
        b,
        &ctx(35, 35),
        f.terms.sponsor,
        &PalwSourceBootstrapActionV1::FundVerifierEconomicsV2 { terms: f.terms },
    )
    .unwrap();
}
fn common_prefix(b: &mut TransitionBuilder<'_>, c: &PalwBlockContextV2) {
    let keys = b.state.pending_payouts.keys().copied().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect::<Vec<_>>();
    for k in keys {
        b.write_payout(k, None).unwrap();
    }
    acknowledge_consumed_prefix(b, c).unwrap();
}
#[test]
fn no_approvals_refund_uses_real_debit_horizon_and_journal_ack_without_r() {
    let f = setup(false);
    let mut b = TransitionBuilder::new(&f.state, &f.p, false, false, false, false, &f.extras);
    let before = b.state.bond(&f.terms.sponsor).unwrap().collateral;
    fund_action(&mut b, &f);
    let paid = funding(&b, f.work.id());
    assert!(paid.conserves());
    assert_eq!(before - b.state.bond(&f.terms.sponsor).unwrap().collateral, paid.prepaid);
    assert!(fund(&mut b, &ctx(36, 36), f.terms.sponsor, &f.epoch, &f.terms).is_err());
    sweep(&mut b, &ctx(37, 2000)).unwrap();
    assert!(b.state.pending_payouts.is_empty(), "inclusive H retains escrow");
    sweep(&mut b, &ctx(38, 2001)).unwrap();
    let queued = funding(&b, f.work.id());
    assert_eq!(queued.refunded, queued.prepaid);
    assert_eq!(queued.queued, 0);
    assert!(queued.refund_acknowledged_block.is_none());
    assert!(b.state.bond_budget.is_none(), "principal is never R income");
    let parent = b.state.clone();
    let journal_start = b.entries.len();
    common_prefix(&mut b, &ctx(39, 2001));
    let ack = funding(&b, f.work.id());
    assert_eq!(ack.refund_acknowledged_block, Some(h(39)));
    assert_eq!(ack.refund_acknowledged_daa, Some(2001));
    assert_eq!(ack.refunded, queued.refunded);
    assert!(ack.conserves());
    let delta = PalwStateDeltaV2 { point: ctx(39, 2001), entries: b.entries[journal_start..].to_vec() };
    let replay = apply_delta_v2(&parent, &delta, &f.p).unwrap();
    assert_eq!(replay.state_root(), b.state.state_root());
    let rollback = revert_delta_v2(&replay, &delta, &f.p).unwrap();
    assert_eq!(rollback.state_root(), parent.state_root());
    let bytes = borsh::to_vec(&delta).unwrap();
    let cold: PalwStateDeltaV2 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(apply_delta_v2(&parent, &cold, &f.p).unwrap().state_root(), replay.state_root());
    sweep(&mut b, &ctx(40, 2002)).unwrap();
    assert!(b.state.pending_payouts.is_empty(), "acknowledged principal never queues again");
    b.state.last_point = Some(ctx(40, 2002));
    b.state.assert_source_verifier_economics_consistency_v2(&f.p).unwrap();
}
#[test]
fn selected_slots_share_one_actor_hold_and_objective_court_is_once_inclusive_h() {
    let f = setup(true);
    let mut b = TransitionBuilder::new(&f.state, &f.p, false, false, false, false, &f.extras);
    fund_action(&mut b, &f);
    let before = b.state.clone();
    let rooms = (2..=3).map(bond).map(|a| (a, b.gate_room(&a, 36, PalwRcoreGateV1::Work))).collect::<BTreeMap<_, _>>();
    let journal_start = b.entries.len();
    all_approvals(&mut b, &f);
    let actors = f.epoch.cohort.iter().map(|s| s.bond).collect::<BTreeSet<_>>();
    for actor in (2..=3).map(bond).filter(|a| actors.contains(&palw_kernel_bond_id_v1(a))) {
        assert_eq!(b.state.reserved_exposure(&actor) - before.reserved_exposure(&actor), 7);
        assert_eq!(
            rooms[&actor] - b.gate_room(&actor, 36, PalwRcoreGateV1::Work),
            7,
            "common gate consumes source hold exactly once without aux reads"
        );
    }
    let after = b.state.state_root();
    let entries = b.entries.len();
    let first_actor = (2..=3).map(bond).find(|a| palw_kernel_bond_id_v1(a) == f.epoch.cohort[0].bond).unwrap();
    assert!(
        super::super::source_bootstrap_v1::apply(
            &mut b,
            &ctx(36, 36),
            first_actor,
            &PalwSourceBootstrapActionV1::ApproveWithLiabilityV2 {
                work: f.work.id(),
                slot: 0,
                statement: f.work.id(),
                signed_daa: 36,
                funding_id: f.terms.id()
            }
        )
        .is_err()
    );
    assert_eq!(b.state.state_root(), after);
    assert_eq!(b.entries.len(), entries, "duplicate slots create neither primary nor aux exposure journal");
    let delta = PalwStateDeltaV2 { point: ctx(36, 36), entries: b.entries[journal_start..].to_vec() };
    assert_eq!(apply_delta_v2(&before, &delta, &f.p).unwrap().state_root(), after);
    assert_eq!(revert_delta_v2(&b.state, &delta, &f.p).unwrap().state_root(), before.state_root());
    let mut derived = BTreeMap::new();
    let _lease = b.state.add_source_verifier_exposures_v2(&mut derived).unwrap();
    assert_eq!(derived, b.state.reserved_exposure);
    let mut full = b.state.clone();
    full.last_point = Some(ctx(36, 36));
    full.assert_internal_consistency_v3(&f.p, false, None).unwrap();
    let carriage = PalwStateCarriageV2::from_state(&full);
    let bytes = borsh::to_vec(&carriage).unwrap();
    let decoded: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    let cold = decoded.into_state_v3(&f.p, Some(full.state_root()), false, None).unwrap();
    assert_eq!(cold.state_root(), full.state_root());
    let mut corrupt_primary = full.clone();
    *corrupt_primary.reserved_exposure.get_mut(&first_actor).unwrap() += 1;
    assert!(
        corrupt_primary.assert_internal_consistency_v3(&f.p, false, None).is_err(),
        "native primary ledger must match aux146 duties"
    );
    let corrupt_carriage = PalwStateCarriageV2::from_state(&corrupt_primary);
    assert!(
        corrupt_carriage.into_state_v3(&f.p, None, false, None).is_err(),
        "cold restore cannot accept a primary-only source hold edit"
    );

    let g = *read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route(&b).unwrap(), &[0]).unwrap().unwrap();
    assert_eq!(g.liabilities as usize, actors.len());
    for actor in (2..=3).map(bond).filter(|a| actors.contains(&palw_kernel_bond_id_v1(a))) {
        assert_eq!(b.state.source_verifier_reserved_v2(&actor, 9999), 7, "failed tick cannot silently release hold");
    }
    // The bridge consumes only actual native Convicted events. This fixture
    // supplies that deterministic event; it does not claim an external court campaign.
    let event = misaka_palw_kernel::ledger::LedgerEventV1::Convicted {
        claim: f.work.claim.as_bytes(),
        accuser: [99; 64],
        slashed: 1,
        accuser_reward: 0,
        post_final: true,
    };
    court_events(&mut b, &ctx(41, 2000), &[event.clone()]).unwrap();
    let once = funding(&b, f.work.id());
    assert_eq!(once.forfeited, 5 * actors.len() as u64);
    assert!(once.conserves());
    let snapshot = b.state.state_root();
    court_events(&mut b, &ctx(42, 2000), &[event]).unwrap();
    assert_eq!(snapshot, b.state.state_root());
    for actor in (2..=3).map(bond).filter(|a| actors.contains(&palw_kernel_bond_id_v1(a))) {
        assert_eq!(b.state.source_verifier_reserved_v2(&actor, 2000), 0);
        assert_eq!(b.state.reserved_exposure(&actor), 0, "conviction releases primary exposure once");
        assert!(matches!(
            duty(&b, actor, f.work.id()).status,
            PalwSourceVerifierDutyStatusV2::Convicted { daa: 2000, collected: 7, .. }
        ));
    }
    sweep(&mut b, &ctx(43, 2001)).unwrap();
    let end = funding(&b, f.work.id());
    assert!(end.closed);
    assert_eq!(end.refunded, 0);
    assert!(end.conserves());
    b.state.last_point = Some(ctx(43, 2001));
    b.state.assert_source_verifier_economics_consistency_v2(&f.p).unwrap();
    let actor = (2..=3).map(bond).find(|a| actors.contains(&palw_kernel_bond_id_v1(a))).unwrap();
    let original = duty(&b, actor, f.work.id());
    let mut d = original;
    let mut c = *read_v2::<PalwSourceVerifierCourtOutcomeV2, 65>(route(&b).unwrap(), &key_v2(4, d.id())).unwrap().unwrap();
    let mut corrupt = b.state.clone();
    d.status = PalwSourceVerifierDutyStatusV2::Convicted { block: c.block, daa: c.daa, collected: d.exposure + 1 };
    c.collected = d.exposure + 1;
    econ_put(&mut corrupt, liability_key_v2(&actor, f.work.id()).to_vec(), &d);
    econ_put(&mut corrupt, key_v2(4, d.id()).to_vec(), &c);
    assert!(
        corrupt.assert_source_verifier_economics_consistency_v2(&f.p).is_err(),
        "matching receipt cannot collect more than exposure"
    );
    let mut d = original;
    d.entitlement = PalwSourceVerifierEntitlementV2::Forfeited { block: h(99) };
    let mut corrupt = b.state.clone();
    econ_put(&mut corrupt, liability_key_v2(&actor, f.work.id()).to_vec(), &d);
    assert!(
        corrupt.assert_source_verifier_economics_consistency_v2(&f.p).is_err(),
        "forfeiture block must equal native conviction and receipt"
    );
}
#[test]
fn settled_fee_queue_saturation_then_same_daa_next_block_ack_is_exactly_once() {
    let f = setup(true);
    let mut b = TransitionBuilder::new(&f.state, &f.p, false, false, false, false, &f.extras);
    fund_action(&mut b, &f);
    all_approvals(&mut b, &f);
    // A real native closing tick establishes native settlement and the source
    // journal, rather than setting a synthetic settlement flag.
    super::super::palw_kernel_route_fold_v1::tick_kernel_route_v1(&mut b, &ctx(44, 100)).unwrap();
    super::super::palw_kernel_route_fold_v1::tick_kernel_route_v1(&mut b, &ctx(44, 1000)).unwrap();
    super::super::source_bootstrap_v1::settle(&mut b, &ctx(44, 1000)).unwrap();
    assert!(b.state.source_bootstrap_work_v1(f.work.id()).unwrap().unwrap().settlement.is_some());
    let actor = (2..=3).map(bond).find(|a| palw_kernel_bond_id_v1(a) == f.epoch.cohort[0].bond).unwrap();
    let payload = b.state.bond(&actor).unwrap().payout_payload;
    let queue = palw_kernel_payout_key_v1(&payload);
    // Aggregate overflow simulates unavailable queue room without allocating
    // the global maximum number of unrelated payouts.
    b.write_payout(queue, Some(PalwPayoutV2 { payload, amount: u64::MAX })).unwrap();
    sweep(&mut b, &ctx(45, 2001)).unwrap();
    assert!(matches!(duty(&b, actor, f.work.id()).entitlement, PalwSourceVerifierEntitlementV2::Held));
    assert_eq!(b.state.source_verifier_reserved_v2(&actor, 2001), 0, "explicit release survives deferred fee");
    b.write_payout(queue, None).unwrap();
    sweep(&mut b, &ctx(46, 2001)).unwrap();
    let allocated = funding(&b, f.work.id());
    assert_eq!(allocated.queued, allocated.prepaid);
    assert_eq!(allocated.held, 0);
    assert!(allocated.closed);
    assert!(allocated.conserves());
    // Use a new builder: only this next block's common prefix is observed.
    let mut next = TransitionBuilder::new(&b.state, &f.p, false, false, false, false, &f.extras);
    common_prefix(&mut next, &ctx(47, 2001));
    let ack = duty(&next, actor, f.work.id());
    assert!(matches!(ack.entitlement, PalwSourceVerifierEntitlementV2::QueueAcknowledged { acknowledged_daa: 2001, amount: 5, .. }));
    assert_eq!(funding(&next, f.work.id()).queued, allocated.queued, "cumulative allocation is not pending amount");
    sweep(&mut next, &ctx(48, 2002)).unwrap();
    assert!(next.state.pending_payouts.is_empty());
    next.state.last_point = Some(ctx(48, 2002));
    next.state.assert_source_verifier_economics_consistency_v2(&f.p).unwrap();
}
#[test]
fn restore_rejects_counter_hold_and_pending_receipt_tampering() {
    let f = setup(true);
    let mut b = TransitionBuilder::new(&f.state, &f.p, false, false, false, false, &f.extras);
    fund_action(&mut b, &f);
    all_approvals(&mut b, &f);
    b.state.last_point = Some(ctx(36, 36));
    b.state.assert_source_verifier_economics_consistency_v2(&f.p).unwrap();
    let mut absent = b.state.clone();
    absent.kernel_route.as_mut().unwrap().aux.retain(|(table, _), _| *table != SOURCE_VERIFIER_ECONOMICS_TABLE_V2);
    assert!(
        absent.assert_source_verifier_economics_consistency_v2(&f.p).is_err(),
        "anchored active epoch cannot erase its whole governor"
    );
    let mut pre_anchor = absent.clone();
    let mut epoch = f.epoch.clone();
    epoch.context.anchor = None;
    source_put(&mut pre_anchor, vec![0], &epoch);
    pre_anchor.assert_source_verifier_economics_consistency_v2(&f.p).unwrap();
    let mut before_activation = absent.clone();
    before_activation.last_point = Some(ctx(0, 0));
    let mut future = f.p.clone();
    future.selected_source_economics_v2.as_mut().unwrap().0 = 1;
    before_activation.assert_source_verifier_economics_consistency_v2(&future).unwrap();
    let mut invalid = funding(&b, f.work.id());
    invalid.distinct_payees += 1;
    // Preserve conservation and product so this specifically checks actual
    // native cohort cardinality, rather than merely arithmetic framing.
    invalid.prepaid += invalid.terms.fee_per_checker_sompi;
    invalid.held += invalid.terms.fee_per_checker_sompi;
    let mut bad_count = b.state.clone();
    econ_put(&mut bad_count, key_v2(1, f.work.id()).to_vec(), &invalid);
    assert!(bad_count.assert_source_verifier_economics_consistency_v2(&f.p).is_err());
    let mut invalid = funding(&b, f.work.id());
    invalid.funded_daa = f.work.accepted_daa - 1;
    let mut bad_clock = b.state.clone();
    econ_put(&mut bad_clock, key_v2(1, f.work.id()).to_vec(), &invalid);
    assert!(bad_clock.assert_source_verifier_economics_consistency_v2(&f.p).is_err());
    let mut corrupt = b.state.clone();
    let mut g = *read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route(&b).unwrap(), &[0]).unwrap().unwrap();
    g.liabilities += 1;
    econ_put(&mut corrupt, vec![0], &g);
    assert!(corrupt.assert_source_verifier_economics_consistency_v2(&f.p).is_err());
    let actor = (2..=3).map(bond).find(|a| palw_kernel_bond_id_v1(a) == f.epoch.cohort[0].bond).unwrap();
    let key = actor_key_v2(3, &actor);
    let mut hold = *read_v2::<PalwSourceVerifierBondHoldV2, 69>(route(&b).unwrap(), &key).unwrap().unwrap();
    hold.reserved -= 1;
    let mut corrupt = b.state.clone();
    econ_put(&mut corrupt, key.to_vec(), &hold);
    assert!(corrupt.assert_source_verifier_economics_consistency_v2(&f.p).is_err());
    let empty = setup(false);
    let mut q = TransitionBuilder::new(&empty.state, &empty.p, false, false, false, false, &empty.extras);
    fund_action(&mut q, &empty);
    sweep(&mut q, &ctx(38, 2001)).unwrap();
    q.state.last_point = Some(ctx(38, 2001));
    q.state.assert_source_verifier_economics_consistency_v2(&empty.p).unwrap();
    let (key, receipt) = first_v2::<PalwSourceVerifierQueueV2, 129>(route(&q).unwrap(), &[8], None).unwrap().unwrap();
    let mut corrupt = q.state.clone();
    corrupt.kernel_route.as_mut().unwrap().aux.remove(&(SOURCE_VERIFIER_ECONOMICS_TABLE_V2, key.to_vec()));
    assert!(corrupt.assert_source_verifier_economics_consistency_v2(&empty.p).is_err());
    let mut wrong_clock = *receipt;
    wrong_clock.queued_daa = 2000;
    let mut corrupt = q.state.clone();
    econ_put(&mut corrupt, key.to_vec(), &wrong_clock);
    assert!(corrupt.assert_source_verifier_economics_consistency_v2(&empty.p).is_err(), "queue cannot predate inclusive H");
    let mut wrong = *receipt;
    wrong.amount += 1;
    let mut corrupt = q.state.clone();
    econ_put(&mut corrupt, key.to_vec(), &wrong);
    assert!(corrupt.assert_source_verifier_economics_consistency_v2(&empty.p).is_err());
}
#[test]
fn dormant_public_payout_without_route_needs_no_economics_context() {
    let mut p = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
    p.selected_source_economics_v2 = None;
    let mut state = PalwChainStateV2::genesis();
    state.pending_payouts.insert(h(60), PalwPayoutV2 { payload: h(61), amount: 9 });
    let extras = PalwTransitionExtrasV1::default();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    b.write_payout(h(60), None).unwrap();
    acknowledge_consumed_prefix(&mut b, &ctx(60, 16001)).unwrap();
    assert!(b.state.kernel_route().is_none());
    assert_eq!(b.state.source_verifier_reserved_v2(&bond(2), 16001), 0);
    let mut route_state = state;
    route_state.kernel_route = Some(PalwKernelRouteStateV1::new(fixture::ledger_world::policy(), None, Default::default()));
    assert_eq!(route_state.source_verifier_reserved_v2(&bond(2), 16001), 0);
}

#[test]
fn public_none_strict_private_refusal_mirror_and_identity_contract() {
    use crate::config::params::{DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_base_params};
    for p in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert!(p.palw_source_verifier_economics_v2.is_none());
        p.validate_palw_source_verifier_economics_v2().unwrap();
    }
    let policy = PalwSourceVerifierEconomicsPolicyV2 {
        version: 2,
        network: h(9),
        bootstrap_policy: h(10),
        exposure_per_work_sompi: 7,
        minimum_fee_per_checker_sompi: 5,
        maximum_funded_works: 2,
        maximum_liabilities: 4,
    };
    let base = palw_t12_base_params();
    let mut p = base.clone();
    p.palw_source_verifier_economics_v2 = Some(PalwSourceVerifierEconomicsFenceV2 { activation: ForkActivation::new(1), policy });
    assert!(p.validate_palw_source_verifier_economics_v2().is_err(), "missing runtime mirror refuses");
    p.sync_palw_source_verifier_economics_v2();
    assert!(p.validate_palw_source_verifier_economics_v2().is_err(), "well-formed private terms cannot arm normal activation");
    assert_ne!(p.consensus_params_id(), base.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), base.consensus_schedule_id());
    let mut changed = p.clone();
    changed.palw_source_verifier_economics_v2.as_mut().unwrap().policy.minimum_fee_per_checker_sompi += 1;
    changed.sync_palw_source_verifier_economics_v2();
    assert_ne!(p.consensus_params_id(), changed.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), changed.consensus_schedule_id());
    p.palw_source_verifier_economics_v2.as_mut().unwrap().activation = ForkActivation::never();
    p.sync_palw_source_verifier_economics_v2();
    p.validate_palw_source_verifier_economics_v2().unwrap();
    assert_eq!(
        p.consensus_identity_id(),
        base.consensus_identity_id(),
        "scheduled/never whole policy normalizes only in handshake identity"
    );
    assert_ne!(p.consensus_params_id(), base.consensus_params_id(), "raw params deliberately retain explicit schedule inputs");
    assert_ne!(p.consensus_schedule_id(), base.consensus_schedule_id());
}

#[test]
fn source_hold_resource_classification_child() {
    if std::env::var("MISAKA_SOURCE_HOLD_RESOURCE_CHILD").ok().as_deref() != Some("1") {
        return;
    }
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    let f = setup(true);
    let mut b = TransitionBuilder::new(&f.state, &f.p, false, false, false, false, &f.extras);
    fund_action(&mut b, &f);
    all_approvals(&mut b, &f);
    let actor = (2..=3).map(bond).find(|a| palw_kernel_bond_id_v1(a) == f.epoch.cohort[0].bond).unwrap();
    let mut dormant = b.state.clone();
    dormant.kernel_route.as_mut().unwrap().aux.retain(|(table, _), _| *table != SOURCE_VERIFIER_ECONOMICS_TABLE_V2);
    let memory = 32 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    assert!(
        matches!(b.state.source_verifier_reserved_checked_v2(&actor, 36), Err(NativeDecodeErrorV1::Resource(_))),
        "native checked reader preserves local capacity"
    );
    assert_eq!(b.state.source_verifier_reserved_v2(&actor, 36), u128::MAX, "only legacy scalar view uses conservative MAX");
    assert_eq!(
        dormant.source_verifier_reserved_checked_v2(&actor, 36).unwrap(),
        0,
        "absent146 needs no allocation even with quota exhausted"
    );
    drop(blocker);
    assert_eq!(b.state.source_verifier_reserved_checked_v2(&actor, 36).unwrap(), 7);
    assert!(reserve_native_requests_v1(memory).is_ok(), "reader returns all temporary ownership");
}
#[test]
fn native_hold_resource_and_dormant_zero_allocation_are_isolated() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "palw_state_v2::palw_source_verifier_economics_fold_v2::tests::source_hold_resource_classification_child",
            "--nocapture",
        ])
        .env("MISAKA_SOURCE_HOLD_RESOURCE_CHILD", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
