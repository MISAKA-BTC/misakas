//! **L5 stress test 1 — claim flood through the real fold.**
//!
//! Junk attempts (every root a made-up hash; see `dos_l5_common::junk_attempt`) folded through
//! `apply_palw_transition_v7` on testnet-12's own genesis state, bundle params and fences
//! (registry fold included, so only the floor class admits — the two held model rows are
//! `Prefetching` until seven seats prove readiness, which needs the real artifact).
//!
//! 1a. **What survives.** N junk floor claims, one per chain block; nobody binds a panel; the
//!     windows close (`window_bind` voids them, `claim_retirement_daa` retires them). Measured:
//!     rooted carriage bytes at peak and after every window has closed, and which rows remain.
//!     **Converted to the fix (2026-09-24 DoS audit #12 (a)):** a `BindTimeout` no `Valid` signed
//!     writes no liability row past the audit fence, so a voided, retired junk claim leaves nothing
//!     rooted behind (asserted; at the audit's commit it left 499.3 B each for ever).
//! 1b. **What the survivors cost every node per block.** `state_root()` re-hashes every
//!     collection from scratch (palw_state_v2.rs:7282, `collection_root` :8118), and the fold
//!     clones the parent (`TransitionBuilder::new`, :8621); both run per chain block
//!     (palw_state_v2_sync.rs:314). Timed at the residue size, extrapolated per row.
//! 1c. **Reservations on OTHER parties.** A junk claim that IS bound puts every seat on duty at
//!     the ADR-0130 floor `max(3 x claim.reserved, lambda x escrow share)`. The producer withholds
//!     (seats cannot sign Valid on junk), the receipt window lapses, the claim is redrawn ONCE onto
//!     a fresh panel, lapses again and voids `ReceiptTimeout`. **Converted to the fix (#9/#10,
//!     b38356fe):** the producer is sized to what #9 admits for one floor attempt (read off the
//!     runtime, `admitted_per_attempt`; the 1,000 MSK producer of the audit can no longer post one),
//!     and past the fence the second `ReceiptTimeout` goes through `void_and_slash`, forfeiting
//!     weight + escrow. Asserted: the charge covers the collateral the claim pinned on others.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l5_1_claim_flood -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_seat_exposure_v1, palw_seat_has_headroom_v1};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};
use std::time::Instant;

const N_JUNK: u64 = 1_200;
const ATTACKER: u64 = 1;
/// 1a/1b's flooder: one junk attempt per block, each live for `window_bind` + 1 DAA before it
/// voids, so ~601 of them are in flight at once. #9 refuses an attempt past the bond's exposure
/// ceiling on the live state, so the flooder is sized for that concurrency from the runtime:
/// `admitted_per_attempt` (6,401.69 MSK per concurrent floor attempt at 500 permille) x
/// (`window_bind` + 2), asserted against the measured reservation below.
fn flooder_collateral(p: &kaspa_consensus_core::config::params::Params, floor: Hash64, pwu: u64) -> u64 {
    let per = admitted_per_attempt(p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    per.collateral * (bundle(p).state.window_bind() + 2)
}

fn time_per_call<F: FnMut()>(iters: u32, mut f: F) -> f64 {
    let t = Instant::now();
    for _ in 0..iters {
        f();
    }
    t.elapsed().as_secs_f64() / iters as f64
}

#[test]
fn dos_l5_1a_1b_junk_floor_flood_leaves_no_rooted_rows() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let flooder = flooder_collateral(&p, floor, pwu);

    let g = genesis_state(&p);
    let (mut s, _, _) = fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, flooder)], PalwBlockWorkV3::None, Hash64::default()).unwrap();
    let baseline_bytes = carriage_bytes(&s);
    let baseline_root_s = time_per_call(20, || {
        let _ = s.state_root();
    });

    // ---- the flood: one junk floor attempt per chain block ------------------------------------
    let mut ids = Vec::with_capacity(N_JUNK as usize);
    let mut daa = 1_000u64;
    let mut blue = 1u64;
    let mut reserved_one = 0u128;
    let t_flood = Instant::now();
    for i in 0..N_JUNK {
        daa += 1;
        blue += 1;
        let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, i + 1, 0x5EED_0000 + i);
        let (next, _, skips) = fold(&p, &sp, &s, &ctx(0x1000 + i, daa, blue, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap_or_else(|e| panic!("junk #{i} folds: {e:?}"));
        assert!(skips.is_empty());
        s = next;
        if i == 0 {
            reserved_one = s.claim(&id).unwrap().reserved;
        }
        ids.push(id);
    }
    let flood_s = t_flood.elapsed().as_secs_f64();
    let peak_bytes = carriage_bytes(&s);
    let peak_claims = s.claims_iter().count();
    let peak_reserved = s.reserved_exposure(&bond_key(ATTACKER));
    let peak_live = s.claims_iter().filter(|(_, c)| !c.phase.is_terminal()).count();

    // ---- the windows close: bind timeout, then retirement -------------------------------------
    let bind = sp.window_bind();
    let retire = sp.claim_retirement_daa();
    for (j, jump) in [daa + bind + 1, daa + bind + 1 + retire + 1].into_iter().enumerate() {
        blue += 1;
        let (next, _, _) = fold(&p, &sp, &s, &ctx(0x9000 + j as u64, jump, blue, 0), &[], PalwBlockWorkV3::None, Hash64::default()).unwrap();
        s = next;
        daa = jump;
    }
    let after_bytes = carriage_bytes(&s);
    let after_claims = s.claims_iter().count();
    let after_liabilities = ids.iter().filter(|id| s.panel_liability(id).is_some()).count();
    let after_reserved = s.reserved_exposure(&bond_key(ATTACKER));
    let attacker_after = s.bond(&bond_key(ATTACKER)).unwrap().collateral;
    let voided_reason = s.panel_liability(&ids[0]).map(|r| (r.void_reason, r.valid_signers.len(), r.expiry_daa));

    // ---- 1b: what the residue costs per block --------------------------------------------------
    let root_s = time_per_call(20, || {
        let _ = s.state_root();
    });
    let s_for_fold = s.clone();
    let mut bb = blue;
    let fold_s = time_per_call(20, || {
        bb += 1;
        let _ = fold(&p, &sp, &s_for_fold, &ctx(0xF000 + bb, daa + 1, bb, 0), &[], PalwBlockWorkV3::None, Hash64::default()).unwrap();
    });
    let residue = after_bytes.saturating_sub(baseline_bytes);
    let residue_per_claim = residue as f64 / N_JUNK as f64;
    let root_s_per_row = (root_s - baseline_root_s).max(0.0) / N_JUNK as f64;

    // ---- the attacker's side -------------------------------------------------------------------
    let draws = palw_expected_attempts_v1(target);
    let lock_sompi_daa = reserved_one * bind as u128;
    let blocks_per_year = 365 * 24 * 3600 * 1000 / p.target_time_per_block();

    println!("=== 1a: {N_JUNK} junk floor claims through the real fold ===");
    println!("floor: expected draws/win = {draws}  (each draw = 1 BLAKE2b over a made-up execution_root, NOT an inference)");
    println!("reserved per junk claim   = {reserved_one} sompi ({:.6} MSK), held {bind} DAA", msk(reserved_one));
    println!("fold time for the flood   = {flood_s:.2} s debug ({:.2} ms/block)", flood_s * 1e3 / N_JUNK as f64);
    println!("carriage bytes: baseline {baseline_bytes} -> peak {peak_bytes} ({peak_claims} claims, attacker reserved {peak_reserved})");
    println!("after bind+retirement windows: bytes {after_bytes}, claims {after_claims}, attacker reserved {after_reserved}, attacker collateral {attacker_after} (posted {flooder})");
    println!("panel_liabilities rows left by voided junk = {after_liabilities}/{N_JUNK}  (first: reason/signers/expiry = {voided_reason:?})");
    println!("residue                    = {residue} bytes = {residue_per_claim:.1} bytes per junk claim");
    println!("  (#12 (a): a signer-less BindTimeout writes no liability past the audit fence)");
    println!();
    println!("=== 1b: per-block cost of the residue (debug build) ===");
    println!("state_root(): baseline {:.3} ms -> {:.3} ms with {after_liabilities} residue rows => {:.3} us per row per call", baseline_root_s * 1e3, root_s * 1e3, root_s_per_row * 1e6);
    println!("claimless fold on the residue state = {:.3} ms", fold_s * 1e3);
    println!("--- extrapolation (formula: rows x bytes_per_row; root_time = rows x us_per_row) ---");
    for (label, rows) in [
        ("honest t12, one attempt claim per block, 1 year", blocks_per_year as f64),
        ("attacker, 180 merged junk attempts per chain block, 1 day", 180.0 * 720.0),
    ] {
        println!(
            "{label:<58}: {rows:>9.0} rows = {:>8.1} MB rooted forever, state_root +{:.1} ms per call (debug)",
            rows * residue_per_claim / 1e6,
            rows * root_s_per_row * 1e3
        );
    }
    println!("--- the ratio ---");
    println!("attacker: 0 sompi fee, 0 sompi slashed, {lock_sompi_daa} sompi*DAA locked per claim, {draws} ticket hashes per claim");
    println!("defender: {residue_per_claim:.0} rooted bytes per claim on every node for the chain's life + a re-hash of them on every block");

    // The flooder was sized for its own concurrency (#9 refuses past the ceiling on the live state).
    assert!(
        peak_reserved <= flooder as u128 * u128::from(sp.fp_max_exposure_ratio_permille()) / 1000,
        "the flood fits under the ceiling it is refused past"
    );
    let per_live = peak_reserved / peak_live.max(1) as u128;
    println!(
        "live exposure per concurrent junk attempt = {:.1} MSK -> collateral per attempt at the 50 % ceiling = {:.1} MSK ({peak_live} live at peak)",
        msk(per_live),
        msk(per_live * 1000 / u128::from(sp.fp_max_exposure_ratio_permille()))
    );
    // The claims themselves are pruned — that part of launch-blocker §8 holds.
    assert_eq!(after_claims, 0, "every junk claim voided and retired");
    assert_eq!(after_reserved, 0, "every reservation released");
    assert_eq!(attacker_after, flooder, "and the attacker lost nothing");
    assert_eq!(after_liabilities, 0, "#12 (a): no BindTimeout left a liability row");
    // **The bound this lane asserts: once every window a claim can be read in has closed, the claim
    // leaves nothing rooted behind.** It failed at the audit's commit (499.3 B per claim, for ever);
    // past #12 (a) it holds.
    assert!(
        residue_per_claim < 1.0,
        "a voided, retired junk claim must leave no rooted residue; measured {residue_per_claim:.1} bytes/claim permanent \
         ({after_liabilities} panel_liabilities rows for {N_JUNK} junk claims)"
    );
}

/// The 2M dense row's per-claim reservation as the registry-armed runtime computes it
/// (`t12_collateral_terms.rs`, the same arithmetic): derived MAC-eq per draw renormalised by the
/// floor's leaves-per-MAC-eq, times slash 5, times attempts (= 1 at this target).
fn dense_2m_reserved_sompi() -> u128 {
    let draw = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, c: (u32, u32)| -> u128 {
        let d = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).unwrap();
        let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, c.0, c.1);
        palw_canonical_draw_work_v1(&d, &j, true).unwrap().provisional_scalar_v1()
    };
    let floor_p = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
    let floor_draw = draw(&floor_p, kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    let d_p = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        n_ctx: kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX,
        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
    })
    .unwrap();
    let d_draw = draw(&d_p, kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX));
    palw_exposure_unit_pwu_v1(d_draw, 7_708, floor_draw) as u128 * 5
}

#[test]
fn dos_l5_1c_bound_junk_claim_reserves_on_other_parties_and_forfeits_at_the_second_timeout() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let bonds = genesis_bonds(&p);
    let seats_a: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let seats_b: Vec<(PalwBondKeyV2, Hash64)> = [bonds[6], bonds[7], bonds[1], bonds[2], bonds[3]].iter().map(|(k, o, _)| (*k, *o)).collect();
    // The least a producer can post and still get ONE floor attempt past #9 (runtime, not a guess).
    let per = admitted_per_attempt(&p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let attacker_collateral = per.collateral;

    let g = genesis_state(&p);
    let mut s: PalwChainStateV2 = fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, attacker_collateral)], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 1, 0x5EED);
    let mut daa = 1_001u64;
    let mut blue = 2u64;
    let step = |s: &PalwChainStateV2, daa: u64, blue: u64, objs: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, sub: u64| {
        fold(&p, &sp, s, &ctx(0x7000 + blue, daa, blue, sub), objs, work, key).unwrap_or_else(|e| panic!("DAA {daa}: {e:?}")).0
    };
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    let reserved = s.claim(&id).expect("an attacker sized by #9 gets its claim").reserved;
    let escrow = s.claim(&id).unwrap().escrowed_reward;

    let others = |s: &PalwChainStateV2| -> u128 { bonds.iter().map(|(k, _, _)| s.reserved_exposure(k)).sum() };
    let mut integral_other: u128 = 0; // sompi x DAA reserved on OTHER parties
    let mut integral_attacker: u128 = 0;
    let mut last = daa;
    let account = |s: &PalwChainStateV2, now: u64, last: &mut u64, io: &mut u128, ia: &mut u128| {
        let dt = (now - *last) as u128;
        *io += others(s) * dt;
        *ia += s.reserved_exposure(&bond_key(ATTACKER)) * dt;
        *last = now;
    };

    // Panel #1 binds; the producer serves nothing; seats cannot sign Valid on junk.
    daa += 1;
    blue += 1;
    account(&s, daa, &mut last, &mut integral_other, &mut integral_attacker);
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA1), seats: seats_of(&seats_a) }], PalwBlockWorkV3::None, Hash64::default(), 0);
    let other_after_bind = others(&s);
    let receipt_window = sp.receipt_window_for_claim_v1(&s, &floor, daa);
    // Receipt window lapses -> redraw (claim back to Provisional).
    daa += receipt_window + 1;
    blue += 1;
    account(&s, daa, &mut last, &mut integral_other, &mut integral_attacker);
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let phase_after_first = format!("{:?}", s.claim(&id).unwrap().phase);
    // Panel #2 binds on a fresh draw.
    daa += 1;
    blue += 1;
    account(&s, daa, &mut last, &mut integral_other, &mut integral_attacker);
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA2), seats: seats_of(&seats_b) }], PalwBlockWorkV3::None, Hash64::default(), 0);
    let other_after_rebind = others(&s);
    daa += receipt_window + 1;
    blue += 1;
    account(&s, daa, &mut last, &mut integral_other, &mut integral_attacker);
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let final_phase = format!("{:?}", s.claim(&id).map(|c| c.phase.clone()));
    let attacker_collateral_after = s.bond(&bond_key(ATTACKER)).unwrap().collateral;
    let charge = (attacker_collateral - attacker_collateral_after) as u128;
    let seats_collateral: u64 = bonds.iter().map(|(k, _, _)| s.bond(k).unwrap().collateral).sum();
    let seats_posted: u64 = bonds.iter().map(|(_, _, c)| *c).sum();

    // ---- the 2M row at the registry-armed reservation (analytic, same arithmetic) --------------
    // Option A: the 2M claim's bond carries its escrow too; #10 forfeits reserved + escrow.
    let r2m = dense_2m_reserved_sompi();
    let ratio = b.state.fp_max_exposure_ratio_permille();
    let seat2m = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_seat_exposure_v1(
        r2m,
        escrow,
        b.panel.seat_count() as usize,
        extras(&p, daa).panel_reward_multiple_permille,
    );
    let res2m = r2m + escrow as u128;
    let genesis_collateral = bonds[0].2;
    let seats_per_bond = (genesis_collateral as u128 * ratio as u128 / 1000) / seat2m;
    let honest_bonds = bonds.len() as u128 - 1; // the executor is excluded from its own panel
    let fits_second = palw_seat_has_headroom_v1(genesis_collateral, seat2m, seat2m, ratio);
    let fits_third = palw_seat_has_headroom_v1(genesis_collateral, 2 * seat2m, seat2m, ratio);

    println!("=== 1c: one BOUND junk claim, producer withholds (FIXED: #9 + #10) ===");
    println!(
        "#9 admits one floor attempt at {:.2} MSK posted: reserved {reserved} + escrow {escrow} = {:.2} MSK at {ratio} permille (before: 1,000 MSK)",
        msk(attacker_collateral as u128),
        msk(per.reservation)
    );
    println!("seat duty per seat (ADR-0130 floor) = {} sompi; 3 x reserved = {}", other_after_bind / b.panel.seat_count() as u128, palw_seat_exposure_v1(reserved));
    println!("reserved on OTHER bonds after bind #1 = {other_after_bind} ({:.2} MSK)", msk(other_after_bind));
    println!("after first receipt window: phase = {phase_after_first}");
    println!("reserved on OTHER bonds after redraw bind #2 = {other_after_rebind}");
    println!("after second receipt window: phase = {final_phase}");
    println!("producer collateral {attacker_collateral} -> {attacker_collateral_after}  (charged {charge} = {:.2} MSK; before: 0)", msk(charge));
    println!("seats' collateral {seats_posted} -> {seats_collateral}");
    println!(
        "sompi x DAA reserved: OTHER parties {integral_other}, producer {integral_attacker}  => ratio {:.4} (before: 3,316,585)",
        integral_other as f64 / integral_attacker.max(1) as f64
    );
    println!("peak pinned on others / charge = {:.4}", other_after_bind as f64 / charge.max(1) as f64);
    println!();
    println!("=== 1c': the 2M dense row at the registry-armed reservation (analytic, t12_collateral_terms arithmetic) ===");
    println!("expected draws per 2M win     = {}  (the lottery needs no inference at all)", palw_expected_attempts_v1(genesis_classes(&p)[2].2));
    println!("reserved per 2M claim         = {r2m} sompi ({:.2} MSK); + escrow = {:.2} MSK on the producer's bond", msk(r2m), msk(res2m));
    println!("seat duty per seat            = {seat2m} sompi ({:.2} MSK)", msk(seat2m));
    println!("genesis bond ceiling          = {:.2} MSK  -> 2M seats one bond can hold at once = {seats_per_bond}", msk(genesis_collateral as u128 * ratio as u128 / 1000));
    println!("a second 2M seat fits on a bond already seated? {fits_second}; a third? {fits_third}");
    println!("honest bonds eligible for a panel = {honest_bonds}; seats per panel = {}", b.panel.seat_count());
    println!(
        "attacker per 2M junk claim: {:.2} MSK posted (#9), {:.2} MSK forfeited at the second ReceiptTimeout (#10)",
        msk(res2m * 1000 / ratio as u128),
        msk(res2m)
    );
    println!(
        "defender pinned per 2M junk claim: {} seats x {:.2} MSK = {:.2} MSK -> pinned / charge = {:.2} (dos_repro_3d folds it)",
        b.panel.seat_count(),
        msk(seat2m),
        msk(seat2m * b.panel.seat_count() as u128),
        (seat2m * b.panel.seat_count() as u128) as f64 / res2m as f64
    );

    assert!(final_phase.contains("ReceiptTimeout"), "the junk claim voids at the second receipt timeout");
    assert!(phase_after_first.contains("Provisional"), "the first lapse redraws");
    assert_eq!(seats_collateral, seats_posted, "no seat was charged");
    assert_eq!(other_after_rebind, other_after_bind, "the redraw pins the same duty again");
    assert_eq!(charge, per.reservation, "#10: the second ReceiptTimeout forfeits weight + escrow");
    // **The bound this lane asserts: a claim that withheld its material through two panels must
    // cost its producer at least what it pinned on others.** At the audit's commit it cost 0;
    // past #10 it costs the escrow-inclusive reservation.
    assert!(
        charge >= other_after_bind,
        "a producer whose claim withheld through two panels must be charged at least what it pinned: charge {:.2} MSK, pinned {:.2} MSK",
        msk(charge),
        msk(other_after_bind)
    );
    assert!(integral_attacker >= integral_other, "and it locks at least the capital x time it pins");
    assert_eq!(seats_per_bond, 2, "option A's genesis bond holds two 2M seat duties");
    assert!(fits_second && !fits_third);
}
