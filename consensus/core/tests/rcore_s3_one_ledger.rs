//! **ADR-0152 v3.1 R-core+, S-3: one ledger at every gate, on testnet-12's own fold** — T08, T15,
//! T17 (U2), T33, T77, T78, T84 and A-4 (S-SPEC §3.1, §3.3–§3.5, §10), beside fence-off twins where
//! the rule has a pre-fence form; and where S-3 meets the other two halves of the integration
//! (rcore/int-1): the vesting row a partial quorum writes (T13 under SR-6's door) and the stake draw
//! under the seat filter (SR-7 × SW-1).
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves and is
//! checked by [`Chain::step`]: the delta re-applies and reverts, and the carriage reloads (the ledger
//! re-derived from the claims' commitments — without the court term past the fence — R-core+'s load
//! invariants and DL-1's deadlines exactly). Collateral is moved through the carriage where a test
//! needs a bond at an exact size: collateral is primary data the load never re-derives, and the one
//! ledger reads it as it finds it.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_s3_one_ledger

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionV2Error, PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
use kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1;
use kaspa_consensus_core::palw_panel_economy_v1::{PalwSeatEconomyV1, palw_panel_collateral_floor_v1};
use kaspa_consensus_core::palw_panel_v2::{
    PalwPanelDrawPolicyV1, PalwPanelIndependenceV1, PalwPanelStakeDrawV1, PalwPanelV2Error, PalwPanelValidLockV1,
    PalwRcoreSeatFilterV1, derive_panel_v2_with_policy,
};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v4;
use kaspa_consensus_core::palw_state_v2::{
    PALW_RCORE_VESTING_ROWS_LANDED_V1, PalwBlockContextV2, PalwStateV2Error, palw_accuser_exposure_v1,
    palw_bond_collateral_is_locked_v6, palw_bond_committed_raw_v1, palw_bond_committed_v1, palw_bond_is_payee_of_unmatured_row_v1,
    palw_panel_valid_lock_required_v1, palw_rcore_bind_prices_v1, palw_rcore_duty_bind_v1, palw_rcore_lock_unvested_v1,
    palw_rcore_lock_vested_at_cap_v1, palw_rcore_lock_vested_v1, palw_rcore_seat_lock_v1, palw_second_clock_depth_of_v1,
    palw_second_clock_depth_v1, palw_v2_licence_backed_seats_v1, palw_v2_object_licenses_claim_v1,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use std::collections::BTreeSet;

const MSK: u128 = 100_000_000;

fn msk_of(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

/// `s` with `bond`'s posted collateral set to `collateral` (through the carriage: a load reads it).
fn with_collateral(sp: &PalwStateParamsV2, s: &PalwChainStateV2, bond: PalwBondKeyV2, collateral: u64) -> PalwChainStateV2 {
    edited(sp, s, |c| c.bonds.get_mut(&bond).expect("the bond").collateral = collateral)
}

/// The raw second-clock depth the processor resolves at `daa` (`palw_settled_anchor_depth_at`).
fn raw_depth(p: &Params, daa: u64) -> Option<u64> {
    extras(p, daa).settled_anchor_depth
}

/// The admission fences the processor resolves at `daa` (`palw_epoch_budget_fences_at`), as far as
/// the stateful half reads them.
fn fences(p: &Params, daa: u64) -> PalwEpochBudgetFencesV1 {
    let fold = registry_fold(p, daa).expect("the registry");
    PalwEpochBudgetFencesV1 {
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        canonical_work_daa: p.palw_canonical_work_daa(),
        base_known_draw: fold.genesis_works.get(&bundle(p).base_class_id).map(|w| w.economic_ccu_per_claim),
        settled_anchor_depth: raw_depth(p, daa),
        ..Default::default()
    }
}

/// A floor attempt by rich bond `n`, with the floor's registered artifact root (so admission reads
/// it), priced for `subsidy`.
fn floor_attempt(c: &Chain, n: u64, seed: u64) -> (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let pwu = c.floor_pwu(c.daa + 1);
    let (mut env, _, _) = junk_attempt(floor, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x10C0 + seed);
    env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
    let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x10C0 + seed), floor, &bond_key(n).0, 7);
    let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
    let id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
    (env, key, id)
}

/// A floor chain with rich bond 1 registered and seated on the genesis producer's licensed and
/// finalized claim (so it holds a lock with no duty), then producing two claims of its own.
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

/// **T08: the fold, admission, the producer's facts (v4) and the draw's filter read one committed
/// number, to the sompi**, at one `(state, DAA, raw depth)`: bond 1's own two claims (w + E each),
/// and its lock on a finalized claim (no duty left). With collateral `X = 2 · (committed + R)` the
/// attempt fits exactly — admission admits, the fold records it — and with `X − 2` (a ceiling one
/// sompi lower) admission refuses and the fold skips it, both naming `committed` as what the bond
/// backs; the draw admits a seat iff `committed + eligibility ≤ X / 2`.
#[test]
fn t08_fold_admission_producer_facts_and_draw_read_one_committed_number() {
    let (c, t) = t08_chain();
    let p = c.p.clone();
    let b = bundle(&p);
    let raw = raw_depth(&p, t);
    assert!(raw.is_some(), "the premise: testnet-12 runs the second clock");
    let committed = palw_bond_committed_raw_v1(&c.s, &c.sp, &bond_key(1), t, raw);
    let lock = c.s.slashable_locks_of(&bond_key(1)).map(|(_, l)| l.amount).sum::<u128>();
    assert_eq!(committed, c.s.reserved_exposure(&bond_key(1)) + lock, "own commitments + the lock alone (its duty left at Final)");
    assert!(committed > 6_500 * MSK, "the premise: two own claims keep X above the producer floor");

    // The producer's facts, v4, with the raw depth: the same number.
    let (floor, _, _, _) = genesis_classes(&p)[0];
    let facts = palw_producer_facts_v4(
        &c.s,
        &c.sp,
        &b.admission,
        kaspa_consensus_core::BlockHash::from_u64_word(1),
        t,
        floor,
        Some(&bond_key(1)),
        None,
        p.palw_canonical_work_daa(),
        fences(&p, t).base_known_draw,
        true,
        0,
        raw,
    )
    .expect("the floor has facts");
    assert_eq!(facts.bond.as_ref().unwrap().committed, committed, "PROD v4 reads the one ledger");

    // What one more attempt reserves (no subsidy: no escrow term), read off the fold on the rich bond.
    let (env, key, id) = floor_attempt(&c, 1, 0x0804);
    let ctx_t = PalwBlockContextV2 { block: h(0x0800_0000 + t), daa_score: t, blue_score: t, subsidy: 0 };
    let probe = c.try_fold(&c.s, &ctx_t, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the rich bond takes it").0;
    let r = probe.claim(&id).expect("recorded").reserved;
    let x = u64::try_from(2 * (committed + r)).expect("fits");

    for (collateral, fits) in [(x, true), (x - 2, false)] {
        let s = with_collateral(&c.sp, &c.s, bond_key(1), collateral);
        assert_eq!(palw_bond_committed_raw_v1(&s, &c.sp, &bond_key(1), t, raw), committed, "collateral moves no commitment");
        let adm = check_palw_attempt_admission_v2(&s, &c.sp, &b.admission, &ctx_t, &env, fences(&p, t));
        let (next, _, skips) = c.try_fold(&s, &ctx_t, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands either way");
        if fits {
            adm.expect("admission admits at the exact ceiling");
            assert!(skips.is_empty() && next.claim(&id).is_some(), "the fold records it: {skips:?}");
        } else {
            match adm {
                Err(PalwAdmissionV2Error::ExposureCeilingExceeded { reserved, claim, .. }) => {
                    assert_eq!((reserved, claim), (committed, r), "admission names the one ledger")
                }
                other => panic!("admission must refuse one sompi over: {other:?}"),
            }
            assert!(next.claim(&id).is_none(), "the fold records nothing");
            assert_eq!(skips.len(), 1, "the own attempt is skipped, the block stands");
            assert!(skips[0].1.contains(&format!("{committed}")), "the fold's refusal names the same backing: {}", skips[0].1);
        }
    }

    // The draw's filter, the bind's own question, to the sompi.
    let s = with_collateral(&c.sp, &c.s, bond_key(1), x);
    let escaped = palw_second_clock_depth_v1(raw, s.recent_anchor_daas(), t, c.sp.window_court());
    let filter = |eligibility| PalwPanelValidLockV1 {
        required: u128::MAX,
        now_daa: t,
        settled_anchor_depth: escaped,
        window_court: c.sp.window_court(),
        rcore: Some(PalwRcoreSeatFilterV1 { eligibility, ceiling_permille: c.sp.fp_max_exposure_ratio_permille() }),
    };
    let room = u128::from(x) * u128::from(c.sp.fp_max_exposure_ratio_permille()) / 1000 - committed;
    assert!(filter(room).admits(&s, &bond_key(1)), "committed + eligibility = ceiling is drawn");
    assert!(!filter(room + 1).admits(&s, &bond_key(1)), "one sompi more is not");
    println!(
        "T08: committed {:.8} MSK = fold = admission = PROD v4 = draw at DAA {t} (raw depth {raw:?}, escaped {escaped:?})",
        msk_of(committed)
    );
}

/// The draw policy the processor resolves for `claim` bound at `daa`, rebuilt from core on `c.s`:
/// `palw_panel_draw_policy_at` at the anchor and `palw_panel_valid_lock_of_v1` at the binding block
/// — one block under SW-8, so both are read at `daa`.
fn draw_policy(c: &Chain, id: &Hash64, claim: &PalwClaimStateV2, daa: u64) -> PalwPanelDrawPolicyV1 {
    let (p, sp) = (&c.p, &c.sp);
    let e = extras(p, daa);
    let seat_count = bundle(p).panel.seat_count() as usize;
    let economy = p.palw_panel_economy_active_at(daa).then(|| PalwSeatEconomyV1 {
        panel_floor_sompi: palw_panel_collateral_floor_v1(sp.min_collateral_sompi()),
        max_exposure_ratio_permille: sp.fp_max_exposure_ratio_permille(),
        reward_multiple_permille: p.palw_panel_reward_multiple_permille_at(daa),
    });
    let readiness = registry_fold(p, daa).filter(|fold| fold.governs_at(daa)).map(|fold| {
        PalwReadinessPolicyV1::at(
            &fold,
            daa,
            sp.base_class_id(),
            p.palw_audit_2026_09_23_active_at(daa) && p.palw_readiness_v2_at(daa),
        )
    });
    let independence = p.palw_admission_independence_daa().map(|from_daa| PalwPanelIndependenceV1 {
        from_daa,
        base_class_id: sp.base_class_id(),
        anchor_daa: daa,
    });
    let valid_lock = (e.audit_2026_09_23_active && e.objective_offence_at(daa)).then(|| PalwPanelValidLockV1 {
        required: palw_panel_valid_lock_required_v1(&c.s, sp, &e, claim),
        now_daa: daa,
        settled_anchor_depth: palw_second_clock_depth_of_v1(&c.s, sp, &e, daa),
        window_court: sp.window_court(),
        rcore: sp.rcore_plus_active_at(daa).then(|| PalwRcoreSeatFilterV1 {
            eligibility: palw_rcore_bind_prices_v1(&c.s, sp, &e, id, claim, seat_count, daa).eligibility,
            ceiling_permille: sp.fp_max_exposure_ratio_permille(),
        }),
    });
    PalwPanelDrawPolicyV1 {
        weighted: p.palw_audit_2026_09_11_deep_active_at(daa),
        economy,
        readiness,
        independence,
        valid_lock,
        stake: p.palw_rcore_plus_active_at(daa).then_some(PalwPanelStakeDrawV1::V1),
    }
}

/// `s` with a live lock on `bond` worth half its posted collateral — its whole 500‰ ceiling on the
/// one ledger (A-1 reads a lock), so S-3's seat filter refuses it on any claim.
fn loaded(sp: &PalwStateParamsV2, s: &PalwChainStateV2, bond: PalwBondKeyV2, lock_claim: Hash64) -> PalwChainStateV2 {
    let amount = s.bond(&bond).expect("the bond").collateral as u128 / 2;
    edited(sp, s, |carriage| {
        carriage.slashable_locks.insert(
            (bond, lock_claim),
            PalwSlashableLockV1 {
                claim: lock_claim,
                amount,
                expiry_daa: 10_000_000,
                settled_at_final: 0,
                attested: PalwSegmentMaskV2::NONE,
                segments: 0,
            },
        );
    })
}

/// **SR-7 × SW-1: the stake draw seats only what the bind binds, on testnet-12's own state** (the
/// integration's review, 2026-09-24). Under SW-8 a panel binds in its anchor block or the claim
/// voids there, and past the 2026-09-23 audit a `PanelBound` the bind refuses is inert (C-3) — so
/// a draw that seated a bond L-4b refuses would void the claim with nothing logged. With the policy
/// resolved as the processor resolves it (the stake draw, the economy, readiness, independence and
/// S-3's seat filter) and genesis seat 3 loaded to its ceiling by a live lock:
///
/// * the filter refuses seat 3 and admits every other genesis seat;
/// * over eight anchors the race never seats seat 3 or the executor, and always binds (7 of 8
///   eligible with SW-10's executor term, exactly 875‰), and the fold binds every panel it drew;
/// * the floor's own panel, seat 3 on it, is inert at the fold: the claim stays `Provisional`;
/// * with seat 4 loaded too the draw refuses under SW-10 (6 of 8 < 875‰), the base still counting
///   both loaded seats.
#[test]
fn sr7_the_stake_draw_seats_only_what_the_bind_binds() {
    let mut c = Chain::new(t12());
    let id = c.floor_claim(0x5701);
    let genesis = genesis_bonds(&c.p);
    let (producer, _, _) = floor_producer(&c.p);
    assert_eq!(genesis[0].0, producer, "the premise: the producer is the first genesis bond");
    let seat3 = genesis[3].0;
    let unloaded = c.s.clone();
    c.s = loaded(&c.sp, &c.s, seat3, h(0x57C3));
    let daa = c.daa + 1;
    let claim = c.claim(&id);
    let policy = draw_policy(&c, &id, &claim, daa);
    let lock = policy.valid_lock.expect("the Valid lock is armed on testnet-12");
    assert!(lock.rcore.is_some() && policy.stake.is_some(), "the premise: S-3's filter and the stake draw are both in force");
    assert!(!lock.admits(&c.s, &seat3), "the filter refuses the loaded seat");
    for (k, _, _) in genesis.iter().filter(|(k, _, _)| *k != seat3) {
        assert!(lock.admits(&c.s, k), "every other genesis seat has room");
    }

    let panel = bundle(&c.p).panel;
    let floor = c.sp.min_collateral_sompi();
    let bind = |s: &PalwChainStateV2, anchor: Hash64, seats: Vec<kaspa_consensus_core::palw_state_v2::PalwPanelSeatV2>| {
        fold(
            &c.p,
            &c.sp,
            s,
            &ctx(0xCA_0000 + daa, daa, daa, 0),
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor, seats }],
            PalwBlockWorkV3::None,
            Hash64::default(),
        )
        .expect("the block stands")
    };
    for i in 0..8u64 {
        let anchor = h(0x57_A000 + i);
        let seats = derive_panel_v2_with_policy(&c.s, &panel, &id, anchor, floor, None, c.p.palw_capability_bound_at(daa), policy)
            .unwrap_or_else(|e: PalwPanelV2Error| panic!("anchor {i}: 7 of 8 eligible with the executor term binds, got {e:?}"));
        assert_eq!(seats.len(), 5);
        assert!(
            seats.iter().all(|s| s.bond != seat3 && s.bond != producer),
            "anchor {i}: neither the loaded seat nor the executor sits"
        );
        let (child, _, skips) = bind(&c.s, anchor, seats);
        assert!(skips.is_empty(), "anchor {i}: {skips:?}");
        assert!(
            matches!(child.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
            "anchor {i}: the fold binds the drawn panel"
        );
    }

    // The floor's own panel (genesis seats 1–5) sits the loaded seat. Before the lock it binds;
    // with it the bind refuses it and, past the audit fence, the object is inert — no panel, the
    // claim `Provisional`, the block standing.
    let with_seat3 = c.floor_seats();
    assert!(with_seat3.iter().any(|(k, _)| *k == seat3));
    let (control, _, _) = bind(&unloaded, h(0x57_AFFF), seats_of(&with_seat3));
    assert!(matches!(control.claim(&id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the control: unloaded, it binds");
    let (child, _, skips) = bind(&c.s, h(0x57_AFFF), seats_of(&with_seat3));
    assert!(skips.is_empty());
    assert!(child.panel(&id).is_none(), "no panel is written");
    assert!(
        matches!(child.claim(&id).unwrap().phase, PalwClaimPhaseV2::Provisional),
        "the bind refuses the seat the draw's filter refuses"
    );

    // Two loaded: 6 of 8 < 875‰, and the base keeps both.
    let seat4 = genesis[4].0;
    let two = Chain { s: loaded(&c.sp, &c.s, seat4, h(0x57C4)), p: c.p.clone(), sp: c.sp.clone(), daa: c.daa, room: false, attribution: c.attribution };
    let policy = draw_policy(&two, &id, &claim, daa);
    let raced =
        derive_panel_v2_with_policy(&two.s, &panel, &id, h(0x57_A000), floor, None, two.p.palw_capability_bound_at(daa), policy);
    match raced {
        Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }) => {
            assert_eq!(eligible * 8, base * 6, "six of eight equal genesis seats are eligible: {eligible} of {base}")
        }
        other => panic!("SW-10 refuses over the one-ledger cut: {other:?}"),
    }
}

/// **T15: the lock each counted signer posts is L-1's `lock_{max(k,2)}`** — the V1 quorum door
/// (basis 3) at `lock_3`, the coverage door (basis 2) at `lock_2`, S2 (basis 1) at `lock_2` for the
/// full seat and the partial alike (`lock_1` is gone) — each with its attested mask and the cut.
///
/// This line has the vesting rows (`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`: every post-Final
/// conviction burns them through S-4's funnel), so the fold prices every lock on the RESIDUAL at the
/// buyback cap (`palw_rcore_lock_vested_at_cap_v1`: `s = 5% · E` whatever the pair; the floor has no
/// market): `k′` locks out-value `G_res + s_cap` with the margin. The ADR §2 residual prices at
/// `s = 0`, 106.74 / 160.11 MSK, and the whole-gain prices a build without rows posted (the S
/// review's H1), 1,173.69 / 1,760.53 MSK, are pinned from the same `G_res`. All read the processor's
/// extras: the execution lane's quantum counts the execution rights `R` (the S review's L1).
#[test]
fn t15_every_door_locks_l1s_price_with_the_attested_mask() {
    let seats5 = |c: &Chain| c.floor_seats();
    let mut table = Vec::new();
    for (door, seed) in [("quorum", 0x1501u64), ("coverage", 0x1502), ("S2", 0x1503)] {
        let mut c = Chain::new(t12());
        let id = c.floor_claim(seed);
        let seats = seats5(&c);
        let bound = c.bind(id, &seats);
        let a = palw_segment_assignment_v2(c.anchor(&id), id, 5);
        let full = a.full_seat as usize;
        let object = match door {
            "quorum" => PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
            },
            "coverage" => PalwConsensusObjectV2::ReceiptLicensedV2 {
                claim: id,
                receipts: covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound),
            },
            _ => PalwConsensusObjectV2::OptimisticLicensed {
                claim: id,
                receipts: covered(id, c.anchor(&id), &seats, &[full, (full + 1) % 5], bound),
            },
        };
        let claim = c.claim(&id);
        let extras_now = c.extras_at(c.daa + 1);
        let g_res = palw_rcore_bind_prices_v1(&c.s, &c.sp, &extras_now, &id, &claim, 5, c.daa + 1).g_res;
        c.step(&[object]);
        let licensed = c.claim(&id);
        let k = licensed.rcore.basis_k;
        let price = palw_rcore_seat_lock_v1(&c.s, &c.sp, &extras_now, &id, &claim, k);
        let signers: Vec<_> = seats.iter().enumerate().filter(|(i, _)| licensed.rcore.served_mask & (1 << i) != 0).collect();
        assert!(!signers.is_empty());
        for (i, (seat, _)) in &signers {
            let lock = c.s.slashable_lock(*seat, id).expect("every counted signer locks");
            assert_eq!(lock.amount, price, "{door}: seat {i} locks lock_{{max({k},2)}}");
            assert_eq!(lock.segments, 4, "{door}: the cut is recorded");
            let want = if door == "quorum" {
                kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::full(4)
            } else {
                a.mask_of(*i as u16)
            };
            assert_eq!(lock.attested, want, "{door}: seat {i} records the mask it attested (a V2 receipt the full cut)");
        }
        table.push((door, k, price, g_res, claim.escrowed_reward));
    }
    assert_eq!((table[0].1, table[1].1, table[2].1), (3, 2, 1), "the recounts");
    assert_eq!(table[2].2, table[1].2, "S2 prices at lock_2: lock_1 is gone");
    assert!(table[1].2 > table[0].2, "fewer colluders, a dearer lock");
    assert!(PALW_RCORE_VESTING_ROWS_LANDED_V1, "the vesting rows are on this line");
    let near = |got: u128, adr: f64| (msk_of(got) - adr).abs() < 0.02;
    for (door, k, price, g_res, e) in &table {
        assert_eq!(*price, palw_rcore_lock_vested_at_cap_v1(*g_res, *e, 0, *k), "{door}: the residual at the buyback cap");
        let residual = g_res + u128::from(kaspa_consensus_core::palw_model_market_v1::palw_model_buyback_slice_v1(*e));
        let colluders = u128::from((*k).max(2));
        assert!(
            colluders * price > residual + residual / 10 - colluders,
            "{door}: {colluders} locks out-value the residual G_res + s_cap with the margin"
        );
    }
    let (lock3, lock2, g_res, e) = (table[0].2, table[1].2, table[0].3, table[0].4);
    let whole = g_res + u128::from(e);
    let (whole3, whole2) = (palw_rcore_lock_unvested_v1(whole, 3), palw_rcore_lock_unvested_v1(whole, 2));
    assert!(
        near(whole3, 1_173.69) && near(whole2, 1_760.53),
        "floor whole-gain lock_3 / lock_2 on G = {:.4} / {:.4} MSK (1,173.69 / 1,760.53, a build without rows)",
        msk_of(whole3),
        msk_of(whole2)
    );
    assert!(lock3 < whole3 && lock2 < whole2, "the residual prices below the whole gain");
    let (vested3, vested2) = (palw_rcore_lock_vested_v1(g_res, e, 0, 3), palw_rcore_lock_vested_v1(g_res, e, 0, 2));
    assert!(
        near(vested3, 106.74) && near(vested2, 160.11),
        "floor residual lock_3 / lock_2 = {:.4} / {:.4} MSK (ADR 106.74 / 160.11)",
        msk_of(vested3),
        msk_of(vested2)
    );
    println!(
        "T15 floor: G {:.4} MSK (G_res {:.4}); lock_3 {:.4}, lock_2 {:.4} at the buyback cap; residual at s = 0 {:.4} / {:.4}",
        msk_of(g_res + u128::from(e)),
        msk_of(g_res),
        msk_of(lock3),
        msk_of(lock2),
        msk_of(vested3),
        msk_of(vested2)
    );
}

/// **T77: `duty_bind` per class and the ≤ 1 bound** — the duty the bind writes is L-4's
/// `min(max(λ, lock_2), commitment / 5)`, and `5 · duty ≤ commitment` on every class. This line has
/// the vesting rows, so `lock_2` is the residual at the buyback cap and each class binds on ADR §2's
/// term: the floor on λ, the 8k row on `lock_2`, the 2M row on the cap (a build without rows priced
/// the whole gain, `lock_2 > commitment / 5` everywhere, and the cap bound every class). ADR §2's
/// residual duties at `s = 0` — the floor 256.07 MSK, the 8k row 459.12, the 2M row 12,588.76 — are
/// pinned through the same formula (processor extras, L1).
#[test]
fn t77_duty_bind_per_class_and_the_amplification_bound() {
    let p = t12();
    let (short, id2m) = model_classes(&p);
    let mut rows = Vec::new();
    for (name, class) in [("floor", None), ("8k", Some(short)), ("2M", Some(id2m))] {
        let (mut c, id, seats) = match class {
            None => {
                let mut c = Chain::new(p.clone());
                let id = c.floor_claim(0x7701);
                let seats = c.floor_seats();
                (c, id, seats)
            }
            Some(class) => {
                let mut c = model_chain(p.clone(), class, 1);
                let id = model_claim(&mut c, class, 1, 0x7710 + rows.len() as u64);
                c.s = readied(&c.sp, &c.s, &honest(&c.p), class, c.daa);
                (c, id, honest_seats(&p, 5))
            }
        };
        let claim = c.claim(&id);
        let prices = palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(c.daa + 1), &id, &claim, 5, c.daa + 1);
        c.bind(id, &seats);
        let duty = c.s.panel_duty_row_of(&id).expect("a duty row").seat_exposure;
        assert_eq!(duty, prices.duty_bind, "{name}: the row stores duty_bind");
        assert!(5 * duty <= prices.commitment, "{name}: n·duty ≤ commitment");
        assert!(PALW_RCORE_VESTING_ROWS_LANDED_V1, "the vesting rows are on this line");
        assert_eq!(prices.lock_2, palw_rcore_lock_vested_at_cap_v1(prices.g_res, claim.escrowed_reward, 0, 2), "{name}: lock_2 at the cap");
        let binds_now = match name {
            "floor" => prices.lambda_term,
            "8k" => prices.lock_2,
            _ => prices.commitment / 5,
        };
        assert_eq!(duty, binds_now, "{name}: L-4 binds on the ADR's term");
        let residual_lock_2 = palw_rcore_lock_vested_v1(prices.g_res, claim.escrowed_reward, 0, 2);
        let residual = palw_rcore_duty_bind_v1(prices.lambda_term, residual_lock_2, prices.commitment, 5);
        let (adr, binds) = match name {
            "floor" => (256.07, prices.lambda_term),
            "8k" => (459.12, residual_lock_2),
            _ => (12_588.76, prices.commitment / 5),
        };
        assert_eq!(residual, binds, "{name}: the ADR's binding term at s = 0");
        assert!((msk_of(residual) - adr).abs() < 0.02, "{name}: residual duty {:.4} MSK (ADR {adr})", msk_of(residual));
        rows.push((name, duty, residual, (5 * duty) as f64 / prices.commitment as f64));
    }
    for (name, duty, residual, amp) in &rows {
        assert!(*amp <= 1.0, "{name}: amplification within the bound: {amp:.4}");
        if *name == "2M" {
            assert!(*amp > 0.9999, "2M: the cap binds, amplification 1.00: {amp:.4}");
        }
        println!(
            "T77 {name}: duty_bind {:.4} MSK (amplification {amp:.4}); ADR residual at s = 0 {:.4}",
            msk_of(*duty),
            msk_of(*residual)
        );
    }
}

/// **T78 (2M): the licence takes the top-up above the duty from the seat's room, and the draw's
/// eligibility is `lock_2`** (L-4b generalized: `max(duty_bind, lock_2)`). The bind reserves the
/// capped duty (12,588.76 MSK); the V1 licence locks `lock_3` and the committed ledger grows by
/// exactly `lock_3 − duty`; the draw's filter at `eligibility = lock_2` admits a seat iff
/// `committed + lock_2` fits. The locks are on the whole gain (H1); the residual `lock_3 / lock_2`
/// the vesting build will price are ADR §2's 22,252.74 / 33,379.11 MSK (processor extras, L1).
#[test]
fn t78_the_2m_top_up_and_the_lock_2_eligibility() {
    let p = t12();
    let (_, id2m) = model_classes(&p);
    let mut c = model_chain(p.clone(), id2m, 1);
    let id = model_claim(&mut c, id2m, 1, 0x7801);
    let claim = c.claim(&id);
    let prices = palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(c.daa + 1), &id, &claim, 5, c.daa + 1);
    assert!(prices.lock_2 > prices.duty_bind, "the premise: on 2M the lock exceeds the capped duty");
    assert_eq!(prices.eligibility, prices.lock_2, "L-4b: eligibility is lock_2");
    assert!((msk_of(prices.duty_bind) - 12_588.76).abs() < 0.02, "2M duty {:.4} MSK (ADR 12,588.76)", msk_of(prices.duty_bind));
    let residual = |k| palw_rcore_lock_vested_v1(prices.g_res, claim.escrowed_reward, 0, k);
    assert!(
        (msk_of(residual(3)) - 22_252.74).abs() < 0.02 && (msk_of(residual(2)) - 33_379.11).abs() < 0.02,
        "2M residual lock_3 / lock_2 = {:.4} / {:.4} MSK (ADR 22,252.74 / 33,379.11)",
        msk_of(residual(3)),
        msk_of(residual(2))
    );
    let seats = honest_seats(&c.p, 5);
    let seat = seats[0].0;
    let committed_at = |c: &Chain, at: u64| palw_bond_committed_v1(&c.s, &seat, at, None, c.sp.window_court());
    let at = c.daa + 1;
    let before_bind = committed_at(&c, at);
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    let bound = c.bind(id, &seats);
    assert_eq!(committed_at(&c, bound) - before_bind, prices.duty_bind, "the bind reserves the duty");
    // The draw's filter at the eligibility, to the sompi, on this seat's own ledger.
    let committed = committed_at(&c, bound);
    let collateral = c.s.bond(&seat).unwrap().collateral as u128;
    let ceiling = collateral * u128::from(c.sp.fp_max_exposure_ratio_permille()) / 1000;
    let filter = |eligibility| PalwPanelValidLockV1 {
        required: u128::MAX,
        now_daa: bound,
        settled_anchor_depth: None,
        window_court: c.sp.window_court(),
        rcore: Some(PalwRcoreSeatFilterV1 { eligibility, ceiling_permille: c.sp.fp_max_exposure_ratio_permille() }),
    };
    assert_eq!(filter(prices.eligibility).admits(&c.s, &seat), committed + prices.lock_2 <= ceiling);
    assert!(filter(ceiling - committed).admits(&c.s, &seat) && !filter(ceiling - committed + 1).admits(&c.s, &seat));
    // The licence: every seat locks lock_3 and its ledger grows by the top-up alone.
    c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed {
        claim: id,
        receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
    }]);
    let lock = c.s.slashable_lock(seat, id).expect("locked").amount;
    assert_eq!(lock, palw_rcore_seat_lock_v1(&c.s, &c.sp, &c.extras_at(c.daa), &id, &claim, 3), "lock_3");
    assert_eq!(committed_at(&c, c.daa) - committed, lock - prices.duty_bind, "the top-up is the lock's excess over the duty");
    println!(
        "T78 2M: duty {:.2} MSK, lock_3 {:.2}, top-up {:.2}, lock_2 eligibility {:.2}",
        msk_of(prices.duty_bind),
        msk_of(lock),
        msk_of(lock - prices.duty_bind),
        msk_of(prices.lock_2)
    );
}

/// **The S re-review's N1, closed by the residual price: on the floor and the 8k row a licence needs
/// no top-up, so it is never inert for want of room.** With the vesting rows in, L-1's `lock_3` and
/// `lock_2` sit within the duty the bind already reserved (the floor's duty is λ, the 8k row's is
/// `lock_2`), so `max(duty, lock) − duty = 0` for every door: a seat whose room the bind used up to
/// the last sompi is still backed. Five `Valid`s license through the V1 door — every seat locked,
/// credited and served — on both classes. (SR-6's unbacked path bites where the lock tops the duty
/// up: the 2M row, T33.)
#[test]
fn n1_on_the_floor_and_the_8k_row_the_licence_needs_no_top_up_and_five_valids_license() {
    let p = t12();
    let (short, _) = model_classes(&p);
    for (name, class) in [("floor", None), ("8k", Some(short))] {
        let (mut c, id, seats) = match class {
            None => {
                let mut c = Chain::new(p.clone());
                let id = c.floor_claim(0x4E01);
                let seats = c.floor_seats();
                (c, id, seats)
            }
            Some(class) => {
                let mut c = model_chain(p.clone(), class, 1);
                let id = model_claim(&mut c, class, 1, 0x4E02);
                c.s = readied(&c.sp, &c.s, &honest(&c.p), class, c.daa);
                (c, id, honest_seats(&p, 5))
            }
        };
        let claim = c.claim(&id);
        let e = c.extras_at(c.daa + 1);
        let prices = palw_rcore_bind_prices_v1(&c.s, &c.sp, &e, &id, &claim, 5, c.daa + 1);
        for k in [3u8, 2] {
            assert!(
                palw_rcore_seat_lock_v1(&c.s, &c.sp, &e, &id, &claim, k) <= prices.duty_bind,
                "{name}: lock_{k} within the duty — no top-up at licence"
            );
        }
        let bound = c.bind(id, &seats);
        // The victim's room gone after the bind: its ceiling one sompi under what it already backs.
        let victim = seats[1].0;
        let at = c.daa + 1;
        let committed = palw_bond_committed_v1(&c.s, &victim, at, raw_depth(&p, at), c.sp.window_court());
        c.s = with_collateral(&c.sp, &c.s, victim, u64::try_from(2 * committed - 2).unwrap());
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
        let after = c.claim(&id);
        assert!(matches!(after.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{name}: five Valids license");
        assert_eq!(after.rcore.served_mask.count_ones(), 5, "{name}: every seat served, the squeezed one included");
        assert!(c.s.slashable_lock(victim, id).is_some(), "{name}: the squeezed seat is backed and locks");
        let row = c.s.panel_duty_row_of(&id).expect("duty row");
        assert!(row.seats.get(&victim).is_some_and(|at| *at != 0), "{name}: and is credited");
    }
}

/// **T33 (SR-6): a licence on the backed subset.** One seat is unbacked at the licence (its room
/// gone after the bind). On the V1 door the other four license: the unbacked one takes no lock, no
/// credit and no served bit (so the escrow is held), and the assemblers' predicate agrees — as does
/// the subset they offer (Phase 2 P2-5, `palw_v2_licence_backed_seats_v1`: the four). On the
/// coverage door the backed four do not cover testnet-12's cut (every segment needs its unique
/// partial holder), so the object is inert and the predicate says so. Fence off, the same set is
/// inert on both doors (today's all-or-nothing rule).
///
/// **Past the fence the armed cases run on the 2M row.** With the vesting rows in, L-1 prices the
/// residual, which on the floor and the 8k row sits within the seat's duty (λ, resp. `lock_2`, binds
/// it): the licence needs no top-up there, so every seat is backed whatever its room — a seat cannot
/// be unbacked on those classes. The 2M row's duty is capped below its lock, so its licence takes the
/// top-up from the seat's room, and that is where SR-6 bites. The fence-off twin keeps the floor.
#[test]
fn t33_a_licence_is_the_backed_subsets_and_the_predicate_agrees() {
    for (door, armed) in [("quorum", true), ("coverage", true), ("quorum", false)] {
        let p = if armed { t12() } else { twin(&t12()) };
        let (mut c, id, seats) = if armed {
            let (_, id2m) = model_classes(&p);
            let mut c = model_chain(p.clone(), id2m, 1);
            let id = model_claim(&mut c, id2m, 1, 0x3301 + (door == "coverage") as u64);
            c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
            (c, id, honest_seats(&p, 5))
        } else {
            let mut c = Chain::new(p.clone());
            let id = c.floor_claim(0x3303);
            let seats = c.floor_seats();
            (c, id, seats)
        };
        let bound = c.bind(id, &seats);
        let a = palw_segment_assignment_v2(c.anchor(&id), id, 5);
        let victim_index = (a.full_seat as usize + 1) % 5;
        let victim = seats[victim_index].0;
        let at = c.daa + 1;
        let committed = palw_bond_committed_v1(&c.s, &victim, at, raw_depth(&p, at), c.sp.window_court());
        // Its ceiling one sompi under what it already backs (the fence-off twin reads its locks at 100%:
        // posted under the lock it would take).
        let squeezed = if armed { u64::try_from(2 * committed - 2).unwrap() } else { 1 };
        c.s = with_collateral(&c.sp, &c.s, victim, squeezed);
        let object = match door {
            "quorum" => PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
            },
            _ => PalwConsensusObjectV2::ReceiptLicensedV2 {
                claim: id,
                receipts: covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound),
            },
        };
        let ctx_at = PalwBlockContextV2 { block: h(0xCA_0000 + at), daa_score: at, blue_score: at, subsidy: 0 };
        let f = flags(&c.p, at);
        let predicate = palw_v2_object_licenses_claim_v1(
            &c.s,
            &c.sp,
            &ctx_at,
            &object,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &c.extras_at(at),
        );
        // Phase 2 P2-5: the backed subset the V1 and coverage assemblers offer, read off the same fold.
        let subset = palw_v2_licence_backed_seats_v1(
            &c.s,
            &c.sp,
            &ctx_at,
            &object,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &c.extras_at(at),
        );
        c.step(&[object]);
        let after = c.claim(&id);
        let licensed = matches!(after.phase, PalwClaimPhaseV2::ReceiptLicensed { .. });
        assert_eq!(predicate, licensed, "{door}/armed={armed}: the assemblers' predicate is the fold's");
        match (door, armed) {
            ("quorum", true) => {
                assert!(licensed, "four backed Valids are a quorum");
                let backed: Vec<_> = seats.iter().enumerate().filter(|(i, _)| *i != victim_index).map(|(_, (k, _))| *k).collect();
                assert_eq!(subset, Some(backed), "the assemblers' subset: the four seats the fold locked, in carried order");
                assert!(c.s.slashable_lock(victim, id).is_none(), "the unbacked Valid takes no lock");
                assert_eq!(after.rcore.served_mask & (1 << victim_index), 0, "…and no served bit");
                assert_eq!(after.rcore.served_mask.count_ones(), 4);
                assert!(!after.rcore.escrow_released, "not every seat served: E held");
                let row = c.s.panel_duty_row_of(&id).expect("duty row");
                assert_eq!(row.seats.get(&victim), Some(&0), "…and no credit");
            }
            _ => {
                assert!(!licensed, "{door}/armed={armed}: inert");
                assert_eq!(subset, None, "{door}/armed={armed}: no subset to offer (below the fence none is read)");
            }
        }
    }
}

/// **T13 under SR-6's door: three of five license, and the row vests only the credited signers**
/// (the integration's review, 2026-09-24). The vesting fold's own fixtures (`vesting_fold_v1`) sit
/// three seats, and past `palw_rcore_plus` a three-seat panel licenses only when all three sign, so
/// the silent seat that suite once left off the row is pinned here, on testnet-12's own five-seat
/// floor panel: three backed `Valid`, two seats silent. At `Final` nothing enters
/// `pending_payouts`; the row's seat legs are exactly the three signers; the row copies the door and
/// `basis_k` S-3 recorded on the claim at the licence (`Quorum`, 3); and B-3's payee clause — the
/// body v6 reads by name — holds the producer and the three signers and neither silent seat.
#[test]
fn t13_three_of_five_license_and_the_row_vests_only_the_credited_signers() {
    let mut c = Chain::new(t12());
    let id = c.floor_claim(0x1301);
    let seats = c.floor_seats();
    assert_eq!(seats.len(), 5, "the premise: testnet-12's floor panel sits five");
    let bound = c.bind(id, &seats);
    let (signers, silent) = seats.split_at(3);
    let receipts: Vec<_> = signers.iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let licensed = c.claim(&id);
    assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "three backed Valid of five license (SR-6)");
    assert_eq!((licensed.rcore.licence_door, licensed.rcore.basis_k), (Some(PalwLicenceDoorTagV1::Quorum), 3));
    c.finalize(id);

    let row = c.s.vesting_row(&id).expect("a Final past palw_rcore_plus names a row").clone();
    assert_eq!(
        (Some(row.licence_door), row.basis_k),
        (licensed.rcore.licence_door, licensed.rcore.basis_k),
        "the row copies S-3's door and recount"
    );
    let legs: BTreeSet<PalwBondKeyV2> = row.seats.iter().map(|(k, _)| *k).collect();
    let credited: BTreeSet<PalwBondKeyV2> = signers.iter().map(|(k, _)| *k).collect();
    assert_eq!(legs, credited, "the seat legs are exactly the credited signers; a silent seat is not a leg");
    let queued: BTreeSet<Hash64> = c.s.pending_payouts_iter().map(|(k, _)| *k).collect();
    assert!(row.legs().all(|leg| leg.queue_key.is_none_or(|key| !queued.contains(&key))), "nothing entered pending_payouts at Final");

    let raw = raw_depth(&c.p, c.daa);
    let (producer, _, _) = floor_producer(&c.p);
    let payee = |bond: &PalwBondKeyV2| palw_bond_is_payee_of_unmatured_row_v1(&c.s, &c.sp, bond, c.daa, raw);
    assert!(payee(&producer), "the producer is held");
    for (k, _) in signers {
        assert!(payee(k), "a credited signer is held");
        assert_eq!(c.s.vesting_rows_of_payee(k).count(), 1);
    }
    for (k, _) in silent {
        assert!(!payee(k), "a silent seat is not a payee");
        assert_eq!(c.s.vesting_rows_of_payee(k).count(), 0, "…and no row is keyed to it");
    }
}

/// **A-4: the duty is not slashable on testnet-12** — a licence carrying two `Unavailable`s and the
/// quorum's three `Valid`s charges no seat (`Withheld` abstains; `slash_silent_seats` is a no-op).
#[test]
fn a4_no_seat_is_charged_at_a_licence() {
    let mut c = Chain::new(t12());
    let id = c.floor_claim(0xA401);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let before: Vec<u64> = seats.iter().map(|(k, _)| c.s.bond(k).unwrap().collateral).collect();
    let mut receipts: Vec<_> = seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect();
    receipts.extend(seats[3..].iter().map(|(k, _)| unavailable(id, *k, bound)));
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let after: Vec<u64> = seats.iter().map(|(k, _)| c.s.bond(k).unwrap().collateral).collect();
    assert_eq!(before, after, "no seat is debited");
}

/// **T84 (A-6): a challenger whose 500‰ is full can still open a court on its free half.** Bond 1
/// backs five claims of its own and posts exactly twice that, so its 500‰ ceiling is spent. Past the
/// fence the court opens (`committed + accuser + reserved ≤ collateral`), writes nothing to
/// `reserved_exposure`, is read by the accuser ledger (re-derived on load) and holds the challenger's
/// exit (B-3); one sompi short of the free half it is refused. The fence-off twin refuses the same
/// court under the 500‰ ceiling it always reserved against.
#[test]
fn t84_a_court_challenger_accuses_on_its_free_half() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p.clone());
        c.step(&[bond_obj(1, RICH)]);
        let challenger = bond_key(1);
        let id = c.floor_claim(0x8401);
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed {
            claim: id,
            receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect(),
        }]);
        let claim = c.claim(&id);
        for seed in 0..5u64 {
            let (env, key, _) = floor_attempt(&c, 1, 0x8410 + seed);
            c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        }
        let at = c.daa + 1;
        let committed = palw_bond_committed_v1(&c.s, &challenger, at, None, c.sp.window_court());
        assert!(committed > u128::from(c.sp.min_collateral_sompi()), "the premise: above the floor on its own claims");
        let open = || {
            const SPACE: kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1 =
                kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves;
            PalwConsensusObjectV2::CourtOpened {
                session_id: kaspa_consensus_core::palw_court_v2::court_session_id_v2(
                    &id,
                    &claim.trace_root,
                    &claim.bond,
                    &challenger,
                    SPACE,
                    16,
                ),
                claim: id,
                challenger_bond: challenger,
                space: SPACE,
                space_size: 16,
                signature: Vec::new(),
            }
        };
        let ctx_at = PalwBlockContextV2 { block: h(0x8400_0000 + at), daa_score: at, blue_score: at, subsidy: 0 };
        let full = u64::try_from(2 * committed).unwrap();
        let short = u64::try_from(committed + claim.reserved - 1).unwrap();
        for (collateral, label) in [(full, "500‰ full"), (short, "one sompi short of the free half")] {
            let s = with_collateral(&c.sp, &c.s, challenger, collateral);
            let reserved_before = s.reserved_exposure(&challenger);
            let result = c.try_fold(&s, &ctx_at, &[open()], PalwBlockWorkV3::None, Hash64::default());
            if armed && collateral == full {
                let (next, _, _) = result.expect("the court opens on the free half");
                assert_eq!(next.reserved_exposure(&challenger), reserved_before, "A-6: nothing enters reserved_exposure");
                assert_eq!(palw_accuser_exposure_v1(&next, &challenger), claim.reserved, "the accuser ledger holds it");
                let record = next.bond(&challenger).unwrap().clone();
                assert!(
                    palw_bond_collateral_is_locked_v6(
                        &next,
                        &c.sp,
                        &challenger,
                        &record,
                        at,
                        c.sp.withdrawal_delay_daa(),
                        raw_depth(&c.p, at),
                        true
                    ),
                    "B-3: the open court holds exit"
                );
                let reloaded = PalwStateCarriageV2::from_state(&next).into_state(&c.sp, Some(next.state_root())).expect("reloads");
                assert_eq!(palw_accuser_exposure_v1(&reloaded, &challenger), claim.reserved, "re-derived on load");
            } else {
                assert!(
                    matches!(result, Err(PalwStateV2Error::AccusationExposureCeiling { .. })),
                    "armed={armed}, {label}: refused by the accuser ceiling: {result:?}"
                );
            }
        }
    }
}

/// **T17 / U2 (S-SPEC §10): the producer floor gate.** A 13,000 MSK producer takes one S0′ (its
/// claim's second panel times out: `w + E` forfeited) and falls below the floor. Past the fence its
/// own attempt is skipped (`ProducerBelowFloor`) with the block standing, and admission refuses it by
/// name; a re-registration at the floor (a new key and operator, ADR §9.3 Q11) produces again. The
/// fence-off twin has no floor gate: the same producer's attempt is recorded.
#[test]
fn t17_u2_a_producer_below_the_floor_after_s0_prime_is_refused_until_it_re_registers() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p.clone());
        let floor = c.sp.min_collateral_sompi();
        c.step(&[bond_obj(1, floor)]);
        // One S0′: two panels bound, neither answers.
        let (env, key, id) = floor_attempt(&c, 1, 0x1701);
        c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        let accepted = c.claim(&id);
        let forfeit = accepted.reserved + escrow(&c.sp, &accepted) + accepted.rights_reserved;
        let seats = c.floor_seats();
        for _ in 0..2 {
            let bound = c.bind(id, &seats);
            let rw = c.sp.receipt_window_for_claim_v1(&c.s, &accepted.class_id, bound);
            c.step_at(bound + rw + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        }
        assert!(
            matches!(
                c.claim(&id).phase,
                PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::ReceiptTimeout, .. }
            ),
            "the premise: the second panel's timeout voids it"
        );
        let posted = c.s.bond(&bond_key(1)).unwrap().collateral;
        assert_eq!(u128::from(floor - posted), forfeit, "S0′ forfeits w + E + rr");
        let at = c.daa + 1;
        let shortfall = kaspa_consensus_core::palw_state_v2::palw_bond_producer_floor_shortfall_v1(&c.s, &c.sp, &bond_key(1), at);
        assert_eq!(shortfall, armed.then_some(floor - posted), "armed={armed}: the shortfall");
        // The own attempt: skipped past the fence, the block standing; recorded below it.
        let (env, key, id2) = floor_attempt(&c, 1, 0x1702);
        let ctx_at = PalwBlockContextV2 { block: h(0x1700_0000 + at), daa_score: at, blue_score: at, subsidy: 0 };
        let (next, _, skips) = c.try_fold(&c.s, &ctx_at, &[], PalwBlockWorkV3::Attempt(&env), key).expect("the block stands");
        assert_eq!(next.claim(&id2).is_some(), !armed, "armed={armed}");
        if armed {
            assert!(skips.len() == 1 && skips[0].1.contains("producer floor"), "{skips:?}");
            let refused = check_palw_attempt_admission_v2(&c.s, &c.sp, &bundle(&p).admission, &ctx_at, &env, fences(&p, at));
            assert!(
                matches!(refused, Err(PalwAdmissionV2Error::ProducerBelowFloor { collateral, floor: f, .. }) if collateral == posted && f == floor),
                "{refused:?}"
            );
            // Re-registration at the floor under a new key and operator produces.
            c.step(&[bond_obj(2, floor)]);
            let (env, key, id3) = floor_attempt(&c, 2, 0x1703);
            c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, 0);
            assert!(c.s.claim(&id3).is_some(), "the re-registered producer's attempt is recorded");
        }
    }
}
