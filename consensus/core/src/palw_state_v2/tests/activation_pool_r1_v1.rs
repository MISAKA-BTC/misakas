//! **R1 on the real cards** (ADR-0152-adjacent: Activation Pool, user decision 2026-09-25; the
//! review's §4 R1, the design's P1): silence reclamation past `Params::palw_activation_pool` spares
//! a class that cannot produce and a genesis row, and reclaims an idle class that can.
//!
//! The fixture is testnet-12 itself — its bundle's `PalwStateParamsV2`, its genesis object list
//! folded as `process_genesis` folds it, a bought registration of a floor variant (the
//! `review12_1_economic.rs` / `dos_l2_registration.rs` registration) — and the testnet-11 card for
//! the twin. Idle epochs are empty blocks half an epoch apart, so every closed epoch is folded once.

use super::*;
use crate::config::params::{Params, palw_rc_shipped_params, palw_t12_shipped_params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use crate::palw_model_registry_v1::PalwModelLifecycleV1;

const SUBSIDY: u64 = 444_562_014_000;
const REGISTRANT: u64 = 0xA77AC;

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("a ConsensusV2 card"),
    }
}

fn bctx(block: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h64(block), daa_score: daa, blue_score: block, subsidy: SUBSIDY }
}

/// `palw_model_registry_fold_at` rebuilt from core, as `review12_1_economic.rs` builds it: the
/// globals with the bundle's seat count, the lane's span, and the genesis classes' works. `None` on
/// a card without the registry.
fn registry_fold(p: &Params) -> Option<crate::palw_model_registry_v1::PalwModelRegistryFoldV1> {
    use crate::palw_model_registry_v1::*;
    let lane = p.palw_execution_lane?;
    p.palw_model_registry?;
    let b = bundle_of(p);
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let mut works = palw_genesis_model_works_v1(&b.genesis_objects);
    for (id, work) in palw_rc_typed_class_works_v1() {
        works.entry(id).or_insert(work);
    }
    let activation = p.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0);
    Some(PalwModelRegistryFoldV1 {
        globals,
        span_daa: lane.schedule_span_daa,
        genesis_works: works,
        grace_until_daa: PalwModelRegistryFoldV1::grace_until_v1(activation, lane.schedule_span_daa, &globals),
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    })
}

/// The extras a card's fold runs with for these rules: the registry, the audit fence and the
/// independence height as the card resolves them, and the pool's terms exactly as the processor
/// resolves them (`Params::palw_activation_pool_at`) — or forced off / on for a twin. No lane, so
/// the registry opens rows at registration and does not step: these tests are about reclamation.
fn card_extras(p: &Params, pool: Option<bool>) -> PalwTransitionExtrasV1 {
    let activation_pool = match pool {
        None => p.palw_activation_pool_at(0),
        Some(true) => Some(crate::palw_activation_pool_v1::PALW_ACTIVATION_POOL_TERMS_V1),
        Some(false) => None,
    };
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(0),
        settled_anchor_depth: if p.palw_audit_2026_09_23_active_at(0) { p.palw_settled_anchor_depth } else { None },
        admission_independence_daa: p.palw_admission_independence.map(|f| f.daa_score()),
        work_target_active: p.palw_work_target_at(0),
        model_registry: registry_fold(p),
        activation_pool,
        ..Default::default()
    }
}

fn fold_at(
    b: &PalwConsensusParamsV2,
    base: &PalwChainStateV2,
    block: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    extras: &PalwTransitionExtrasV1,
) -> PalwChainStateV2 {
    apply_palw_transition_v7(
        base,
        &b.state,
        None,
        &bctx(block, daa),
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        extras,
    )
    .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e:?}"))
    .0
}

fn registrant_bond(collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(REGISTRANT),
        pubkey: vec![0x5a; 8],
        operator_pubkey: vec![0xa5; 16],
        collateral,
        payout_payload: h64(REGISTRANT),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// The card's genesis, plus the registrant's bond on a card that has a registry to buy from.
fn genesis_of(b: &PalwConsensusParamsV2, with_registrant: bool, extras: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
    let mut objects = b.genesis_objects.clone();
    if with_registrant {
        objects.push(registrant_bond(51_642_979_663_480));
    }
    fold_at(b, &PalwChainStateV2::genesis(), 1, 0, &objects, extras)
}

/// A bought registration of a distinct floor variant (`n_threads = n`), registrant-signed shape.
fn bought(b: &PalwConsensusParamsV2, base: &PalwChainStateV2, n: u32) -> (Hash64, PalwConsensusObjectV2) {
    let floor = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("floor");
    let job = crate::palw_base0_profile::rc_job_context(
        &floor,
        crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
        crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
    );
    let mut prof = floor.clone();
    prof.n_threads = n;
    let mut job = job;
    job.shape_profile_id = prof.shape_profile_id();
    let leaves = crate::palw_step::step_leaf_count_capped_v1(&prof, &job, b.court.max_step_leaf_count()).unwrap_or(1);
    let class_id = prof.shape_profile_id();
    let object = PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root: Hash64::from_u64_word(0xF000_0000 + n as u64),
        slash_value_per_pwu: base.class(&b.base_class_id).unwrap().slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: base.class_target(&b.base_class_id).unwrap().target,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: prof,
            canonical: job,
            registrant_bond: bond_key(REGISTRANT),
            signature: vec![0u8; 4627],
        })),
    };
    (class_id, object)
}

/// `epochs + 1` closed epochs of silence from `daa`: empty blocks half an epoch apart.
fn idle(
    b: &PalwConsensusParamsV2,
    mut state: PalwChainStateV2,
    block: &mut u64,
    daa: &mut u64,
    epochs: u64,
    extras: &PalwTransitionExtrasV1,
) -> PalwChainStateV2 {
    let epoch = b.state.epoch_length();
    for _ in 0..(epochs + 1) * 2 {
        *daa += epoch / 2;
        *block += 1;
        state = fold_at(b, &state, *block, *daa, &[], extras);
    }
    state
}

fn is_dormant(state: &PalwChainStateV2, id: &Hash64) -> bool {
    matches!(state.class(id).map(|c| &c.status), Some(PalwClassStatusV2::Dormant { .. }))
}

/// **A Candidate is not Dormant after thirteen idle epochs; the pool-off twin shows what the old
/// rule did to it on the same card.** A bought class opens `Candidate` (ADR-0145 §7) with a share
/// row of zero and a budget of at least one block, so the old walk measured a class that could not
/// produce — the design's "Candidate → Dormant after ~17–18 days" reading, run here.
#[test]
fn r1_a_candidate_is_not_dormant_after_thirteen_epochs() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let armed = card_extras(&p, None);
    assert!(armed.activation_pool.is_some(), "testnet-12 arms the pool from genesis");
    let epochs = u64::from(b.state.reclaim_epochs()) + 1;
    assert_eq!(epochs, 13, "twelve reclaim epochs on testnet-12, plus one");
    let mut outcomes = Vec::new();
    for extras in [armed.clone(), card_extras(&p, Some(false))] {
        let s0 = genesis_of(&b, true, &extras);
        let (class_id, reg) = bought(&b, &s0, 71_001);
        let s1 = fold_at(&b, &s0, 2, 2, &[reg], &extras);
        assert_eq!(s1.model_lifecycle(&class_id).map(|row| row.state), Some(PalwModelLifecycleV1::Candidate), "opens Candidate");
        assert!(s1.class_share_permille(&class_id).is_some(), "the premise: a share row, which the old walk reads");
        let (mut block, mut daa) = (2u64, 2u64);
        let s = idle(&b, s1, &mut block, &mut daa, epochs, &extras);
        outcomes.push((is_dormant(&s, &class_id), s.registration_exposure(&bond_key(REGISTRANT))));
        println!(
            "pool {}: after {daa} DAA of silence the Candidate is {:?}",
            extras.activation_pool.is_some(),
            s.class(&class_id).map(|c| &c.status)
        );
    }
    assert!(!outcomes[0].0, "past the pool's fence a Candidate is never reclaimed for the network's absence");
    assert!(outcomes[0].1 > 0, "and its registration exposure stays reserved while it is listed");
    assert!(outcomes[1].0, "below it (the old rule, same card and blocks) the Candidate WAS reclaimed — the P1 defect");
}

/// **An idle class that CAN produce is still reclaimed**: the same bought class, its row set
/// `Active` (as ADR-0147's jury, probation and the stable steps would leave it), silent for
/// thirteen epochs, goes `Dormant` past the fence exactly as before it.
#[test]
fn r1_an_idle_active_bought_class_is_still_reclaimed() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let extras = card_extras(&p, None);
    let s0 = genesis_of(&b, true, &extras);
    let (class_id, reg) = bought(&b, &s0, 71_002);
    let mut s1 = fold_at(&b, &s0, 2, 2, &[reg], &extras);
    let mut row = s1.model_lifecycle(&class_id).cloned().expect("a row");
    row.state = PalwModelLifecycleV1::Active;
    s1.set_model_lifecycle_for_tests(class_id, row);
    let (mut block, mut daa) = (2u64, 2u64);
    let s = idle(&b, s1, &mut block, &mut daa, u64::from(b.state.reclaim_epochs()) + 1, &extras);
    assert!(
        is_dormant(&s, &class_id),
        "an Active class that produced nothing for twelve epochs is reclaimed: {:?}",
        s.class(&class_id)
    );
    assert_eq!(s.registration_exposure(&bond_key(REGISTRANT)), 0, "and its registrant's exposure comes back");
}

/// **testnet-12's genesis held rows (8k and 2M) are share-bearing, walked by the old rule, and
/// spared past the fence.** Answers the design's side finding (UNVERIFIED there): the genesis
/// assembly grants each held row a share, each has a budget of at least one block, so the old walk
/// measured them — and the pool-off twin shows it reclaiming an idle one.
#[test]
fn r1_testnet_12s_genesis_held_rows_are_share_bearing_and_never_reclaimed_past_the_fence() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let held = crate::config::params::palw_t12_genesis_held_class_ids_v1();
    assert_eq!(held.len(), 2, "the 8k and the 2M rows");
    let mut dormant_by_rule = Vec::new();
    for extras in [card_extras(&p, None), card_extras(&p, Some(false))] {
        let s0 = genesis_of(&b, false, &extras);
        for id in &held {
            let record = s0.class(id).expect("registered at genesis");
            assert!(record.registrant_bond.is_none(), "a genesis row: no registrant");
            assert!(matches!(record.status, PalwClassStatusV2::Active), "Active from genesis: {:?}", record.status);
            println!("genesis held row {id}: share {:?}", s0.class_share_permille(id));
            assert!(s0.class_share_permille(id).is_some(), "share-bearing: the old walk reads it");
        }
        let (mut block, mut daa) = (1u64, 0u64);
        let s = idle(&b, s0, &mut block, &mut daa, u64::from(b.state.reclaim_epochs()) + 1, &extras);
        let budgets = s.epoch_budgets().map(|t| held.iter().map(|id| t.budget_blocks.get(id).copied()).collect::<Vec<_>>());
        println!("pool {}: held rows' budgets {budgets:?}", extras.activation_pool.is_some());
        dormant_by_rule.push(held.iter().map(|id| is_dormant(&s, id)).collect::<Vec<_>>());
    }
    println!(
        "held rows Dormant after thirteen idle epochs — pool armed: {:?}; old rule: {:?}",
        dormant_by_rule[0], dormant_by_rule[1]
    );
    assert_eq!(dormant_by_rule[0], vec![false, false], "past the fence a genesis row is never reclaimed, like the floor");
    assert_eq!(dormant_by_rule[1], vec![true, true], "the old rule, on the same card and blocks, reclaimed both idle held rows");
}

/// **testnet-11 is untouched**: its card resolves no pool at any height, so its fold runs the old
/// walk by construction — and on that card the old walk reclaims an idle genesis row, which is why
/// R1 is fenced rather than applied everywhere (a live chain's replay would fork at the first
/// reclaim it spared).
#[test]
fn r1_testnet_11_resolves_no_pool_and_keeps_the_old_walk() {
    let p = palw_rc_shipped_params();
    assert_eq!(p.palw_activation_pool, None, "testnet-11 does not carry the fence");
    assert_eq!(p.palw_activation_pool_at(u64::MAX - 1), None);
    let b = bundle_of(&p);
    let t11 = card_extras(&p, None);
    assert!(t11.activation_pool.is_none());
    let genesis_rows: Vec<Hash64> = genesis_of(&b, false, &t11)
        .classes_iter()
        .filter(|(id, record)| **id != b.base_class_id && record.registrant_bond.is_none())
        .map(|(id, _)| *id)
        .collect();
    assert!(!genesis_rows.is_empty(), "testnet-11 has genesis model rows");
    let mut roots = Vec::new();
    let mut dormant = Vec::new();
    for extras in [t11.clone(), card_extras(&p, Some(true))] {
        let s0 = genesis_of(&b, false, &extras);
        let (mut block, mut daa) = (1u64, 0u64);
        let s = idle(&b, s0, &mut block, &mut daa, u64::from(b.state.reclaim_epochs()) + 1, &extras);
        roots.push(s.state_root());
        dormant.push(genesis_rows.iter().filter(|id| is_dormant(&s, id)).count());
    }
    println!("testnet-11 genesis rows Dormant after the idle run — as shipped: {}; with R1 forced: {}", dormant[0], dormant[1]);
    if dormant[0] > 0 {
        assert_ne!(roots[0], roots[1], "on testnet-11 the old walk reclaims a genesis row R1 would spare, so R1 must stay fenced");
    }
    assert_eq!(dormant[1], 0, "with R1 forced, no genesis row is reclaimed");
}

/// **The pool's fix round, L4: a Dormant class registered again re-enters as a registration.** R2
/// stops stepping a Dormant class's row, so the row a re-registration finds is the one its class held
/// when it went quiet — `Active` here. Past the fence the registration rewrites it to the entry state
/// (`Candidate`, the jury a registration owes), and keeps the class's pool; the pool-off twin keeps
/// the stale `Active` row, as the old rule always did.
#[test]
fn l4_a_dormant_class_registered_again_re_enters_as_a_candidate() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let mut rows = Vec::new();
    for extras in [card_extras(&p, None), card_extras(&p, Some(false))] {
        let s0 = genesis_of(&b, true, &extras);
        let (class_id, reg) = bought(&b, &s0, 71_003);
        let mut s1 = fold_at(&b, &s0, 2, 2, std::slice::from_ref(&reg), &extras);
        let mut row = s1.model_lifecycle(&class_id).cloned().expect("a row");
        row.state = PalwModelLifecycleV1::Active;
        s1.set_model_lifecycle_for_tests(class_id, row);
        let (mut block, mut daa) = (2u64, 2u64);
        let s = idle(&b, s1, &mut block, &mut daa, u64::from(b.state.reclaim_epochs()) + 1, &extras);
        assert!(is_dormant(&s, &class_id), "the premise: reclaimed");
        let again = fold_at(&b, &s, block + 1, daa + 1, &[reg], &extras);
        assert!(matches!(again.class(&class_id).map(|c| &c.status), Some(PalwClassStatusV2::Active)), "registered again");
        rows.push((again.model_lifecycle(&class_id).map(|row| row.state), again.activation_pool(&class_id).is_some()));
    }
    assert_eq!(rows[0], (Some(PalwModelLifecycleV1::Candidate), true), "past the fence: a Candidate again, its pool kept");
    assert_eq!(rows[1], (Some(PalwModelLifecycleV1::Active), false), "the old rule keeps the stale row");
}

/// A lifecycle carrier as the CLI builds one: the payer's change to its own P2PKH-ML-DSA-87 script
/// at output 0 (the payee P-B1 pays a refusal back to), then `sinks`, the object in the payload.
fn lifecycle_carrier(object: &PalwConsensusObjectV2, payer: Hash64, sinks: Vec<crate::tx::TransactionOutput>, nonce: u32) -> crate::tx::Transaction {
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::tx::{TransactionInput, TransactionOutpoint, TransactionOutput};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
    let mut outputs = vec![TransactionOutput::new(3 * 100_000_000, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payer.as_bytes()))];
    outputs.extend(sinks);
    crate::tx::Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(h64(0xF00D), nonce), vec![], 0, 1)],
        outputs,
        0,
        crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    )
}

/// One chain block accepting `carriers`, the way `VirtualStateProcessor` folds it: the lifecycle
/// walk in acceptance order, the rehearsal that drops each object the fold refuses and — past the
/// 2026-09-23 audit fence — names what its carrier is owed (P-B1), then the transition handed those
/// refunds. A reorg of the block must undo it. Returns the state and the refunds.
fn block_of_carriers(
    b: &PalwConsensusParamsV2,
    base: &PalwChainStateV2,
    block: u64,
    daa: u64,
    carriers: &[crate::tx::Transaction],
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, Vec<PalwCarrierRefundV1>) {
    use crate::palw_lifecycle_objects_v2::{palw_lifecycle_objects_from_accepted_txs_v2, palw_model_carrier_refund_v1};
    let ctx = bctx(block, daa);
    let walk = palw_lifecycle_objects_from_accepted_txs_v2(carriers);
    assert_eq!(walk.objects.len(), carriers.len(), "every carrier rides: {:?}", walk.skipped);
    let mut folded = palw_v2_pre_object_base_v1(base, &b.state, &ctx, false, false, false, false, extras).expect("the pre-object base");
    let (mut accepted, mut refunds) = (Vec::new(), Vec::new());
    for carried in walk.objects {
        let tx = carriers.iter().find(|tx| tx.id() == carried.carrier).expect("the carrier");
        match palw_v2_apply_one_object_v1(&folded, &b.state, &ctx, &carried.object, false, false, false, false, extras) {
            Ok(next) => {
                folded = next;
                accepted.push(carried.object);
            }
            Err(_) if extras.audit_2026_09_23_active => {
                refunds.extend(palw_model_carrier_refund_v1(tx, &carried.object));
            }
            Err(_) => {}
        }
    }
    let with_refunds = PalwTransitionExtrasV1 { carrier_market_refunds: refunds.clone(), ..extras.clone() };
    let (next, delta, _) = apply_palw_transition_v7(
        base,
        &b.state,
        None,
        &ctx,
        &accepted,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &with_refunds,
    )
    .expect("the block folds");
    assert_eq!(revert_delta_v2(&next, &delta, &b.state).unwrap().state_root(), base.state_root(), "a reorg undoes the block");
    (next, refunds)
}

/// **The pool's P4 (user decision 2026-09-25: the CLI sponsors a listing at its registration): a
/// top-up in its registration's own block is credited or paid back — never lost.** A lifecycle
/// carrier carries one object, so the sponsor is its own carrier; `misaka model add` files it once
/// the registration has folded (a node's mempool refuses a top-up of a class the tip does not
/// hold). Were the two mined into one block anyway, on testnet-12's own card: the fold applies the
/// block's objects in acceptance order against the running state, so a sponsor BEHIND its
/// registration finds the Candidate it just listed and is split by `α` into its pool; one AHEAD of
/// it finds no class, is refused `MissingClass`, and is paid back whole to its payer through P-B1.
/// Either way every sompi the activation sink took is in the pool or owed back.
#[test]
fn p4_a_sponsor_in_its_registrations_block_is_credited_behind_it_and_paid_back_ahead_of_it() {
    use crate::palw_activation_pool_v1::{PALW_ACTIVATION_POOL_TERMS_V1, palw_activation_sink_spk_v1};
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let extras = card_extras(&p, None);
    assert!(extras.activation_pool.is_some() && extras.audit_2026_09_23_active, "testnet-12 arms the pool and P-B1");
    let s0 = genesis_of(&b, true, &extras);
    let (class_id, reg) = bought(&b, &s0, 71_004);
    let sponsor = 500 * 100_000_000u64;
    let payer = h64(0x5B0);
    let registration = lifecycle_carrier(&reg, h64(REGISTRANT), Vec::new(), 1);
    let top_up = lifecycle_carrier(
        &PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount: sponsor, sink_index: 1 },
        payer,
        vec![crate::tx::TransactionOutput::new(sponsor, palw_activation_sink_spk_v1(&class_id))],
        2,
    );
    let owed = |state: &PalwChainStateV2| {
        let key = palw_model_refund_payout_key_v1(&top_up.id());
        state.pending_payouts_iter().find(|(k, _)| **k == key).map(|(_, row)| (row.payload, row.amount))
    };

    // Behind its registration: credited, split by α, owed nothing.
    let (behind, refunds) = block_of_carriers(&b, &s0, 2, 2, &[registration.clone(), top_up.clone()], &extras);
    assert!(refunds.is_empty(), "{refunds:?}");
    assert_eq!(behind.model_lifecycle(&class_id).map(|row| row.state), Some(PalwModelLifecycleV1::Candidate));
    let pool = behind.activation_pool(&class_id).cloned().expect("the listing's pool");
    let alpha = u64::from(PALW_ACTIVATION_POOL_TERMS_V1.prep_share_permille);
    assert_eq!((pool.funded_sompi, pool.prep_sompi, pool.bonus_sompi), (sponsor, sponsor * alpha / 1_000, sponsor - sponsor * alpha / 1_000));
    assert!(pool.is_balanced());
    assert_eq!(owed(&behind), None, "a folded sponsor is the pool's");

    // Ahead of it: refused, paid back whole to the change its payer signed; the listing still stands.
    let (ahead, refunds) = block_of_carriers(&b, &s0, 2, 2, &[top_up.clone(), registration.clone()], &extras);
    assert_eq!(refunds.len(), 1, "the sponsor is the one refused carrier");
    assert!(matches!(ahead.class(&class_id).map(|c| &c.status), Some(PalwClassStatusV2::Active)), "the registration folds");
    let pool = ahead.activation_pool(&class_id).cloned().expect("the listing's pool opens empty");
    assert_eq!(pool.funded_sompi, 0);
    assert_eq!(owed(&ahead), Some((payer, sponsor)), "paid back through P-B1");

    // Never lost: the sink's MSK is in the pool or owed back, in both orders.
    for (state, name) in [(&behind, "behind"), (&ahead, "ahead")] {
        let in_pool = state.activation_pool(&class_id).map(|pool| pool.funded_sompi).unwrap_or(0);
        assert_eq!(in_pool + owed(state).map(|(_, amount)| amount).unwrap_or(0), sponsor, "{name}");
    }
}
