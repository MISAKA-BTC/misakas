//! **ADR-0152 P6 on testnet-12's own fold — T08's node half, with nothing written by hand.**
//!
//! `palw_producer`'s `p6_tests` pin the pre-check on a small fixture, where a seat lock can only be
//! put into `committed` by editing the facts: the lock ledger is written only through a bound
//! panel. Here the consensus-core `rcore_s3` T08 chain does it — testnet-12's `Params`, bundle,
//! genesis fold and extras (`consensus/core/tests/rcore_common.rs`, the fixture every `rcore_s*`
//! suite shares): a bond seated on a claim that has gone Final still holds its `Valid` lock, so its
//! committed ledger exceeds what it reserves. The candidate is priced with the real block subsidy
//! (option A's escrow term — most of one claim's exposure on testnet-12) and the second clock's raw
//! depth, as the processor resolves both; and the node's decision is compared with admission and
//! the fold at the same state and DAA.
//!
//! The fixture is consensus-core's and is included by path, so a change to its API breaks this
//! build rather than drifting from it. `#[rustfmt::skip]` keeps a `rustfmt` of this crate from
//! reformatting consensus-core's test files through the include.
#![allow(dead_code, unused_imports)]

#[rustfmt::skip]
#[path = "../../consensus/core/tests/rcore_common.rs"]
mod common;
use common::*;

use crate::palw_producer::{PalwProducerHoldV1, PalwRcorePlusReadsV1, palw_producer_ready_v1, palw_rcore_plus_producer_floor_v1};
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
use kaspa_consensus_core::palw_producer_v2::{PALW_NOT_READY_EXPOSURE_FULL_V2, PalwProducerFactsV2, palw_producer_facts_v4};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, palw_bond_committed_raw_v1, palw_claim_escrow_v1};

fn with_collateral(sp: &PalwStateParamsV2, s: &PalwChainStateV2, bond: PalwBondKeyV2, collateral: u64) -> PalwChainStateV2 {
    edited(sp, s, |c| c.bonds.get_mut(&bond).expect("the bond").collateral = collateral)
}

fn raw_depth(p: &Params, daa: u64) -> Option<u64> {
    extras(p, daa).settled_anchor_depth
}

/// The admission fences the processor resolves at `daa`, escrow carve included (admission prices
/// the own attempt's escrow from `ctx.subsidy` under it).
fn fences(p: &Params, daa: u64) -> PalwEpochBudgetFencesV1 {
    let fold = registry_fold(p, daa).expect("the registry");
    PalwEpochBudgetFencesV1 {
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        canonical_work_daa: p.palw_canonical_work_daa(),
        base_known_draw: fold.genesis_works.get(&bundle(p).base_class_id).map(|w| w.economic_ccu_per_claim),
        settled_anchor_depth: raw_depth(p, daa),
        escrow_carve: extras(p, daa).escrow_carve,
        ..Default::default()
    }
}

/// What the worker hands the pre-check at candidate `t` — `palw_rcore_plus_reads_v1` without its
/// session, whose one session read (the SW-10 seam) answers `None` on every build today.
fn reads(p: &Params, t: u64) -> Option<PalwRcorePlusReadsV1> {
    palw_rcore_plus_producer_floor_v1(p, t)
        .map(|producer_floor| PalwRcorePlusReadsV1 { producer_floor, eligible_stake_at_floor: None })
}

fn floor_attempt(c: &Chain, n: u64, seed: u64) -> (PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let pwu = c.floor_pwu(c.daa + 1);
    let (mut env, _, _) = junk_attempt(floor, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x10C0 + seed);
    env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
    let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x10C0 + seed), floor, &bond_key(n).0, 7);
    let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
    let id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
    (env, key, id)
}

/// `rcore_s3_one_ledger`'s T08 chain: bond 1 (rich) seated on a finalized floor claim — a lock
/// with no duty left under it — and then two claims of its own.
fn t08_chain() -> (Chain, u64) {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, RICH)]);
    let id = c.floor_claim(0x0801);
    let mut seats = vec![(bond_key(1), kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(1)))];
    seats.extend(genesis_bonds(&c.p)[1..5].iter().map(|(k, o, _)| (*k, *o)));
    let bound = c.bind(id, &seats);
    let receipts: Vec<_> = seats.iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    c.finalize(id);
    assert!(c.s.slashable_lock(bond_key(1), id).is_some(), "the premise: bond 1 holds a lock past the Final");
    for seed in [0x0802u64, 0x0803] {
        let (env, key, _) = floor_attempt(&c, 1, seed);
        c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    }
    let t = c.daa + 1;
    (c, t)
}

/// PROD v4 exactly as `palw_producer_facts_v2_impl` calls it at candidate `t` (the work-target floor
/// left `None` as the S3 suite does — the floor class is exempt from the budget either way).
fn facts_at(c: &Chain, s: &PalwChainStateV2, t: u64, subsidy: u64) -> PalwProducerFactsV2 {
    let p = &c.p;
    let (floor, _, _, _) = genesis_classes(p)[0];
    palw_producer_facts_v4(
        s,
        &c.sp,
        &bundle(p).admission,
        kaspa_consensus_core::BlockHash::from_u64_word(1),
        t,
        floor,
        Some(&bond_key(1)),
        None,
        p.palw_canonical_work_daa(),
        fences(p, t).base_known_draw,
        p.palw_audit_2026_09_23_active_at(t),
        palw_claim_escrow_v1(&c.sp, subsidy, extras(p, t).escrow_carve),
        raw_depth(p, t),
    )
    .expect("the floor has facts")
}

/// **The node holds exactly when admission and the fold refuse, at the ceiling, with a live lock,
/// the real escrow and the raw depth** — and one or two sompi under the ceiling the OLD pre-check
/// still says ready, because the lock is committed and never reserved: the switch, proven on
/// testnet-12's own ledger.
#[test]
fn the_node_holds_where_the_t12_fold_refuses_at_the_ceiling_with_a_live_lock() {
    let (c, t) = t08_chain();
    let p = c.p.clone();
    assert!(p.palw_rcore_plus_active_at(t), "testnet-12 arms R-core+");
    assert_eq!(c.sp.fp_max_exposure_ratio_permille(), 500, "the arithmetic below assumes the 500‰ ceiling");
    let subsidy = T12_BLOCK_SUBSIDY_SOMPI;
    let raw = raw_depth(&p, t);
    assert!(raw.is_some(), "the premise: the second clock runs");

    let (env, key, id) = floor_attempt(&c, 1, 0x0804);
    let ctx_t = PalwBlockContextV2 { block: h(0x0800_0000 + t), daa_score: t, blue_score: t, subsidy };
    let probe = fold(&p, &c.sp, &c.s, &ctx_t, &[], PalwBlockWorkV3::Attempt(&env), key).expect("folds").0;
    let adding = palw_claim_commitment_v1(&c.sp, probe.claim(&id).expect("the rich bond takes it"), t).expect("no overflow");

    let f0 = facts_at(&c, &c.s, t, subsidy);
    let b0 = f0.bond.as_ref().expect("bond 1");
    assert_eq!(f0.pwu, env.attempt.pwu, "the facts' pwu is the attempt's");
    assert_eq!(b0.claim_exposure, adding, "the node prices one claim at exactly the fold's commitment, escrow term included");
    let committed = palw_bond_committed_raw_v1(&c.s, &c.sp, &bond_key(1), t, raw);
    assert_eq!(b0.committed, committed, "PROD v4 reads the one ledger at the raw depth");
    let lock_excess = committed - c.s.reserved_exposure(&bond_key(1)) - c.s.registration_exposure(&bond_key(1));
    assert!(lock_excess > 0, "the premise: a live lock sits in committed and not in reserved");

    // Collateral whose 500‰ ceiling is exactly `committed + one claim`, then one and two sompi less.
    let x = u64::try_from(2 * (committed + adding)).expect("fits");
    let floor = c.sp.min_collateral_sompi();
    assert!(x > floor, "the premise: the exact-ceiling bond meets the producer floor");
    let mut held_where_old_passed = 0;
    for collateral in [x, x - 1, x - 2, RICH] {
        let s = with_collateral(&c.sp, &c.s, bond_key(1), collateral);
        let f = facts_at(&c, &s, t, subsidy);
        let node = palw_producer_ready_v1(&f, &pubkey_of(1), reads(&p, t));
        let old = f.ready_to_produce(&pubkey_of(1));
        let adm = check_palw_attempt_admission_v2(&s, &c.sp, &bundle(&p).admission, &ctx_t, &env, fences(&p, t));
        let (next, _, skips) = fold(&p, &c.sp, &s, &ctx_t, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
        let folded = next.claim(&id).is_some();
        let case = format!("collateral {collateral}: node={node:?} old={old:?} adm={adm:?} fold={folded} skips={skips:?}");
        assert_eq!(node.is_ok(), folded, "{case}");
        assert_eq!(adm.is_ok(), folded, "{case}");
        if !folded {
            assert_eq!(node, Err(PalwProducerHoldV1::NotReady(PALW_NOT_READY_EXPOSURE_FULL_V2)), "{case}");
            assert!(matches!(adm, Err(PalwAdmissionV2Error::ExposureCeilingExceeded { .. })), "{case}");
            if old.is_ok() {
                held_where_old_passed += 1;
            }
        }
    }
    assert_eq!(held_where_old_passed, 2, "x−1 and x−2: the old ledger saw room the chain does not grant");
}

/// **Under testnet-12's producer floor the node gives the chain's reason, first, with the chain's
/// numbers**: a bond under the floor is also over its ceiling here (its committed ledger is above
/// half the floor), and admission and the fold both ask the floor first — so does the node, with
/// the shortfall the fold computes and the floor it measures against.
#[test]
fn under_the_t12_floor_the_node_and_the_fold_both_name_the_floor() {
    let (c, t) = t08_chain();
    let p = c.p.clone();
    let subsidy = T12_BLOCK_SUBSIDY_SOMPI;
    let floor = c.sp.min_collateral_sompi();
    assert_eq!(reads(&p, t).map(|r| r.producer_floor), Some(floor), "the node's floor is the fold's");
    let (env, key, id) = floor_attempt(&c, 1, 0x0805);
    let ctx_t = PalwBlockContextV2 { block: h(0x0800_0000 + t), daa_score: t, blue_score: t, subsidy };
    for collateral in [floor - 1, floor / 2, 1] {
        let s = with_collateral(&c.sp, &c.s, bond_key(1), collateral);
        let f = facts_at(&c, &s, t, subsidy);
        let node = palw_producer_ready_v1(&f, &pubkey_of(1), reads(&p, t));
        let adm = check_palw_attempt_admission_v2(&s, &c.sp, &bundle(&p).admission, &ctx_t, &env, fences(&p, t));
        let (next, _, skips) = fold(&p, &c.sp, &s, &ctx_t, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
        let case = format!("collateral {collateral}: node={node:?} adm={adm:?} skips={skips:?}");
        assert!(next.claim(&id).is_none(), "{case}");
        assert_eq!(node, Err(PalwProducerHoldV1::BelowProducerFloor { shortfall: floor - collateral, floor }), "{case}");
        assert!(
            matches!(adm, Err(PalwAdmissionV2Error::ProducerBelowFloor { collateral: posted, floor: chain_floor, .. })
                if posted == collateral && chain_floor == floor),
            "{case}"
        );
        assert_eq!(skips.len(), 1, "{case}");
        assert!(skips[0].1.contains("producer floor"), "the fold skips it for the floor: {case}");
    }
}
