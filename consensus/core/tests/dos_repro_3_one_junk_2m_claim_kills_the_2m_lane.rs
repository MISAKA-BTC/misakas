//! **DoS repro 3 — "one junk 2M claim kills the 2M lane", re-measured at option A and #9/#10.**
//!
//! Finding (L5, medium, amplification-oom, consensus; report §2 row 7): the genesis 2M dense row's
//! lottery needs a SINGLE draw (`palw_expected_attempts_v1 == 1`), so any ticket wins with one
//! BLAKE2b and no inference. At the audit's commit one bound junk 2M claim put each of a five-seat
//! panel on duty at `3 × claim.reserved` against a genesis bond ceiling that held exactly ONE such
//! duty, so the 2M lane's throughput went to zero for one hash plus a returnable 59,742.94 MSK lock
//! (honest 896,144.13 MSK pinned, 15×).
//!
//! **What changed.** Option A (K=64) re-sized the genesis bonds (938,888 MSK/seat): a genesis
//! bond's ceiling now holds TWO 2M seat duties, so one junk claim no longer kills the lane (3a
//! asserts the new capacity from the runtime and how many concurrent junk claims do). Option A
//! also puts the claim's escrow on the producer's bond; #9 (b38356fe) makes the producer post
//! `ceil((reserved + escrow) × 1000 / ratio)` per concurrent attempt on the live state; #10 makes
//! the second `ReceiptTimeout` forfeit weight + escrow (3c, on the floor). 3d folds the attack on
//! the 2M row itself (made Active through the carriage) and measures the charge against the
//! capital it pins.
//!
//! Everything below calls the REAL runtime:
//!   * `palw_expected_attempts_v1` — the class lottery's draw count;
//!   * the exact expression `apply_attempt` uses to write `claim.reserved`
//!     (`palw_canonical_per_draw_v1` / `palw_exposure_basis_v2` / `palw_exposure_pwu_v3` ×
//!     `slash_value_per_pwu` × `palw_claim_attempts_v1`), fed from testnet-12's OWN genesis state;
//!   * `palw_panel_seat_exposure_v1` — the seat duty `reserve_seat_duties` reserves at binding;
//!   * `palw_seat_has_headroom_v1` — the draw's own eligibility predicate;
//!   * `apply_palw_transition_v7` (the fold) — (3b) the class is GATED on genesis, (3c) a bound
//!     junk claim is charged at its second receipt timeout, (3d) the 2M attack end to end;
//!   * `palw_resource_profile_v1` — the FullSeat working set (node policy; reported, not asserted).
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_repro_3_one_junk_2m_claim_kills_the_2m_lane -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_panel_seat_exposure_v1, palw_seat_has_headroom_v1};
use kaspa_consensus_core::palw_pwu::{palw_claim_attempts_v1, palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateV2Error,
    PalwVoidReasonV2, palw_exposure_pwu_v3,
};

const GIB: f64 = (1u64 << 30) as f64;
const ATTACKER: u64 = 9_001;

/// **The runtime reservation of one 2M claim, computed exactly as `apply_attempt` writes
/// `claim.reserved` on testnet-12** (canonical-work armed at DAA 0):
/// `palw_exposure_pwu_v3(class, pwu, canonical_per_draw, exposure_basis) × slash_value_per_pwu ×
/// palw_claim_attempts_v1(pwu, canonical_per_draw)`. Returns `(reserved_sompi, pwu, attempts)`.
fn runtime_2m_reserved(p: &kaspa_consensus_core::config::params::Params, g: &PalwChainStateV2) -> (u128, u64, u64) {
    let b = bundle(p);
    let base = b.state.base_class_id();
    let (id2m, _leaves, target2m, slash2m) = genesis_classes(p)[2];
    // The floor's derived draw as the registry will write it (base_known_draw), read from the same
    // fold input the processor hands the transition.
    let base_known_draw = registry_fold(p, 0).and_then(|rf| rf.genesis_works.get(&base).map(|w| w.economic_ccu_per_claim));
    let accepted_daa = 1_000u64;
    let canonical = g.palw_canonical_per_draw_v1(&id2m, accepted_daa, Some(0)).map(|w| w.min(u64::MAX as u128) as u64);
    let basis = g.palw_exposure_basis_v2(&base, accepted_daa, Some(0), base_known_draw);
    let class2m = g.class(&id2m).expect("the 2M class is registered at genesis");
    // The one admissible pwu past the canonical-work fence — `palw_pwu_v1(target, one draw's work)`.
    let pwu = palw_attempt_derived_pwu_v1(target2m, canonical.expect("2M has a row") as u128);
    let attempts = palw_claim_attempts_v1(pwu, canonical);
    let exposure_pwu = palw_exposure_pwu_v3(class2m, pwu, canonical, basis);
    let reserved = exposure_pwu as u128 * slash2m as u128 * attempts as u128;
    (reserved, pwu, attempts)
}

/// How many 2M seat duties one bond of `collateral` holds at once — the draw's own predicate,
/// applied until it refuses.
fn duties_that_fit(collateral: u64, duty: u128, ratio: u32) -> u128 {
    let mut n = 0u128;
    while palw_seat_has_headroom_v1(collateral, n * duty, duty, ratio) {
        n += 1;
    }
    n
}

/// The concurrent junk 2M claims that leave fewer than `seat_count + 1` honest bonds with 2M
/// headroom (an honest claim's panel needs `seat_count` bonds besides its own executor), with each
/// junk panel seated on the least-loaded bonds — the spread the attacker can least control, so the
/// count is the most it can need. Returns `(claims, bonds with headroom after each claim)`.
fn claims_to_kill_the_lane(collaterals: &[u64], duty: u128, ratio: u32, seat_count: usize) -> (u64, Vec<usize>) {
    let mut load = vec![0u128; collaterals.len()];
    let room = |load: &[u128]| collaterals.iter().zip(load).filter(|(c, l)| palw_seat_has_headroom_v1(**c, **l, duty, ratio)).count();
    let mut trail = Vec::new();
    for k in 1..=64u64 {
        let mut order: Vec<usize> = (0..collaterals.len()).filter(|i| palw_seat_has_headroom_v1(collaterals[*i], load[*i], duty, ratio)).collect();
        order.sort_by_key(|i| load[*i]);
        assert!(order.len() >= seat_count, "claim {k} cannot be seated: the lane was already dead");
        for i in order.into_iter().take(seat_count) {
            load[i] += duty;
        }
        trail.push(room(&load));
        if room(&load) < seat_count + 1 {
            return (k, trail);
        }
    }
    panic!("64 junk claims did not close the lane");
}

#[test]
fn dos_repro_3a_2m_collateral_and_lane_kill() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let (id2m, leaves2m, target2m, _slash) = genesis_classes(&p)[2];
    let seat_count = b.panel.seat_count() as usize;
    let ratio = sp.fp_max_exposure_ratio_permille();
    let reward_multiple = p.palw_panel_reward_multiple_permille_at(0);

    // ---- the lottery: one draw wins ----------------------------------------------------------
    let draws = palw_expected_attempts_v1(target2m);

    // ---- the runtime reservation, seat duty and ceiling --------------------------------------
    let (reserved, pwu, attempts) = runtime_2m_reserved(&p, &g);
    // The escrow a genesis-era 2M claim carries: the block subsidy's worker carve. Option A puts it
    // on the producer's bond next to the weight; it feeds the ADR-0130 seat-exposure floor.
    // The same carve the fold escrows (`dos_l5_6`, `worker_carve_at` with t12's escrow carve).
    let escrow = sp.worker_carve_at(T12_BLOCK_SUBSIDY_SOMPI, p.palw_overlay_carve.and_then(|c| c.escrow_carve()));
    let seat_duty = palw_panel_seat_exposure_v1(reserved, escrow, seat_count, reward_multiple);
    // The registry's own cap on the class's claims in flight (the fold's `ClassInflightCapped`
    // below the work-target fence; `panel_room_v1` past it prices the same replay budget).
    let max_inflight = g.model_lifecycle(&id2m).expect("the 2M row").profile.max_inflight_claims;
    let producer_reservation = reserved + sp.claim_escrow_reservation_v1(1_000, escrow);

    let genesis_bonds = genesis_bonds(&p);
    let genesis_collateral = genesis_bonds[0].2;
    let ceiling = genesis_collateral as u128 * ratio as u128 / 1000;
    let duties_per_bond = duties_that_fit(genesis_collateral, seat_duty, ratio);
    let fits_first = palw_seat_has_headroom_v1(genesis_collateral, 0, seat_duty, ratio);
    let fits_second = palw_seat_has_headroom_v1(genesis_collateral, seat_duty, seat_duty, ratio);
    let fits_third = palw_seat_has_headroom_v1(genesis_collateral, 2 * seat_duty, seat_duty, ratio);

    // ---- one bound junk 2M claim, then how many kill the lane -----------------------------------
    // The attacker is its own bond now (#9 makes it post its own collateral), so all eight genesis
    // bonds are the panel-eligible pool.
    let collaterals: Vec<u64> = genesis_bonds.iter().map(|g| g.2).collect();
    let after_one = collaterals
        .iter()
        .enumerate()
        .filter(|(i, c)| palw_seat_has_headroom_v1(**c, if *i < seat_count { seat_duty } else { 0 }, seat_duty, ratio))
        .count();
    let (kill, trail) = claims_to_kill_the_lane(&collaterals, seat_duty, ratio, seat_count);

    // ---- the FullSeat replay a bound 2M claim asks each seat to stand up (node policy) --------
    let seat_working_set = full_seat_working_set_2m(&p);

    // ---- attacker vs defender ----------------------------------------------------------------
    let attacker_posted_per_claim = (producer_reservation * 1000).div_ceil(ratio as u128);
    let claim_life_daa = 2 * (sp.window_bind() + sp.window_receipt());
    let defender_pinned = seat_duty * seat_count as u128;
    let amp_on_reserved = defender_pinned as f64 / reserved as f64;
    let amp_on_reservation = defender_pinned as f64 / producer_reservation as f64;

    println!("=== repro 3a: the 2M lottery, the collateral pin, and the lane kill (all real functions) ===");
    println!("2M class {id2m}  declared leaves = {leaves2m}, target = {target2m:e}");
    println!("ATTACKER cost");
    println!("  lottery draws to win           = {draws}  (each draw = 1 BLAKE2b over a made-up execution_root; 0 inference)");
    println!("  admissible pwu (derived)       = {pwu}, claim attempts = {attempts}");
    println!("  claim.reserved                 = {reserved} sompi = {:.2} MSK", msk(reserved));
    println!("  + option A escrow on its bond  = {:.2} MSK -> reservation {:.2} MSK", msk(escrow as u128), msk(producer_reservation));
    println!("  collateral #9 asks per claim   = {:.2} MSK (ceiling {ratio} permille; before: {:.2} MSK)", msk(attacker_posted_per_claim), msk(reserved * 1000 / ratio as u128));
    println!("  forfeited at the 2nd timeout   = {:.2} MSK (#10; before: 0)", msk(producer_reservation));
    println!("  claim life <= 2 x (bind+recv)  = {claim_life_daa} DAA");
    println!("DEFENDER cost");
    println!("  seat duty per seat (runtime)   = {seat_duty} sompi = {:.2} MSK", msk(seat_duty));
    println!("  genesis bond collateral        = {:.2} MSK, ceiling {:.2} MSK -> 2M duties one bond holds = {duties_per_bond} (before: 1)", msk(genesis_collateral as u128), msk(ceiling));
    println!("  a bond fits ONE duty? {fits_first}  a SECOND? {fits_second}  a THIRD? {fits_third}");
    println!("  after ONE bound 2M claim: bonds with 2M headroom = {after_one} of {} (before: 2 of 7 -> lane dead)", collaterals.len());
    println!("  concurrent junk 2M claims that close the lane BY SEAT HEADROOM (spread panels) = {kill}; headroom after each = {trail:?}");
    println!("  the 2M row's max_inflight_claims = {max_inflight}: ONE junk claim in flight refuses every honest 2M claim BY ADMISSION (3d)");
    println!("  honest collateral pinned       = {seat_count} x {:.2} = {:.2} MSK per junk claim", msk(seat_duty), msk(defender_pinned));
    println!("  FullSeat replay each seat sets up = {:.2} GiB (node policy; consensus admission reads no memory term)", seat_working_set as f64 / GIB);
    println!("AMPLIFICATION");
    println!("  pinned / claim.reserved = {amp_on_reserved:.2}x (before: 15x);  pinned / producer reservation = {amp_on_reservation:.2}x");
    println!(
        "  to close the lane by headroom alone: {kill} claims, {:.2} MSK posted, {:.2} MSK forfeited if withheld, {:.2} MSK honest pinned",
        msk(attacker_posted_per_claim * kill as u128),
        msk(producer_reservation * kill as u128),
        msk(defender_pinned * kill as u128)
    );

    // ---- the fixed code's numbers, asserted ----------------------------------------------------
    assert_eq!(draws, 1, "the 2M target's lottery needs exactly one draw — one hash wins");
    assert_eq!(reserved, 5_974_294_206_820, "runtime reserved per 2M claim = 59,742.94 MSK");
    assert_eq!(seat_duty, 17_922_882_620_460, "runtime seat duty = 3 x reserved = 179,228.83 MSK");
    assert_eq!(duties_per_bond, 2, "option A: a genesis bond holds TWO 2M seat duties");
    assert!(fits_first && fits_second && !fits_third);
    assert_eq!(after_one, collaterals.len(), "one bound junk 2M claim leaves every bond with 2M headroom: the lane lives");
    assert!(kill > 1, "one junk claim no longer closes the 2M lane by seat headroom");
    assert_eq!(max_inflight, 1, "but the registry admits ONE 2M claim in flight, so one junk claim still holds the lane (3d)");
    assert_eq!(
        kill,
        (collaterals.len() as u64 * duties_per_bond as u64 - seat_count as u64).div_ceil(seat_count as u64),
        "the lane closes once fewer than seats + 1 bonds keep a free duty slot"
    );
}

/// The FullSeat working set `palw_resource_profile_v1` derives for the genesis 2M row, under the
/// shipped i16 K/V representation — node policy (consensus admission reads no memory term,
/// `palw_resource_profile_v1.rs:37-40`), reported so the RAM side is stated in bytes.
fn full_seat_working_set_2m(p: &kaspa_consensus_core::config::params::Params) -> u64 {
    use kaspa_consensus_core::palw_resource_profile_v1::*;
    let b = bundle(p);
    let court_ladder = b.court.max_step_leaf_count();
    let prof = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            n_ctx: kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX,
            ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
        },
    )
    .unwrap();
    let (pf, dc) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX);
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&prof, pf, dc);
    let ladder = kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(court_ladder, &prof);
    let leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&prof, &job, ladder).unwrap_or(0);
    let limits = PalwRuntimeLimitsV1 { threads: 8, prefill_run_positions: 16 };
    palw_resource_profile_v1(&prof, &job, leaves, PalwRuntimeProfileV1::A16KvI16, PalwResourceRoleV1::FullSeat, limits, PalwCaptureRetentionV1::Fold { retain_level: 12 })
        .map(|profile| profile.working_set_bytes())
        .unwrap_or(0)
}

#[test]
fn dos_repro_3b_the_2m_class_is_gated_until_active() {
    // **The premise, established through the REAL fold.** The finding's attack needs the 2M class
    // ADMITTING claims. On the shipped genesis it is `Prefetching` — it admits nothing until seven
    // seats prove possession of the real 2.6 GB artifact (`apply_seat_readiness_v2` verifies a
    // multiproof over the real inventory), which no core unit test can reach. So a junk 2M attempt
    // folded on genesis is REFUSED, and the amplification above is a property of the ACTIVE class —
    // which is the network's own goal, not the attacker's to bring about.
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let (id2m, leaves2m, target2m, _slash) = genesis_classes(&p)[2];

    // The 2M row's lifecycle at genesis.
    let state2m = format!("{:?}", g.model_lifecycle(&id2m).map(|r| r.state));
    println!("=== repro 3b: the 2M class is gated on genesis ===");
    println!("2M lifecycle at genesis = {state2m}");

    // Register the attacker bond, then fold a junk 2M attempt.
    let pwu = palw_pwu_v1(target2m, leaves2m); // whatever pwu — admission is refused before pricing
    let s: PalwChainStateV2 = fold(
        &p,
        &sp,
        &g,
        &ctx(1, 1_000, 1, 0),
        &[bond_obj(ATTACKER, 20_000_000_000_000)], // 200,000 MSK — above the 119,486 MSK the finding needs
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .unwrap()
    .0;
    let (env, key, _id) = junk_attempt(id2m, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 1, 0x5EED);
    let folded = fold(&p, &sp, &s, &ctx(0x1000, 1_001, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key);
    match &folded {
        Ok(_) => println!("junk 2M attempt FOLDED (the class admits) — the lane kill is directly reachable"),
        Err(e) => println!("junk 2M attempt REFUSED at admission: {e:?}"),
    }

    assert!(matches!(g.model_lifecycle(&id2m).map(|r| r.state.clone()), Some(kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Prefetching)),
        "the shipped 2M row opens Prefetching");
    assert!(
        matches!(folded, Err(PalwStateV2Error::ClassNotAdmitting { .. })),
        "a junk 2M attempt on genesis is refused ClassNotAdmitting; got {folded:?}"
    );
}

#[test]
fn dos_repro_3c_bound_claim_forfeits_weight_and_escrow_at_the_second_timeout() {
    // **The repeatability mechanic, on a claim the fold CAN bind (the floor class).** It is
    // class-independent — a bound claim whose producer withholds redraws ONCE and then voids at
    // `ReceiptTimeout`. At the audit's commit that was `void_claim`: the producer kept its whole
    // collateral and relocked it for the next claim. Past #10 it is `void_and_slash`, taking weight
    // + escrow. The producer posts what #9 admits for one floor attempt (runtime, not a guess).
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let (floor, leaves, target, _slash) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let per = admitted_per_attempt(&p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let attacker_collateral = at_least_the_floor(&p, per.collateral);
    let bonds = genesis_bonds(&p);
    let seats_a: Vec<(PalwBondKeyV2, Hash64)> = bonds[1..6].iter().map(|(k, o, _)| (*k, *o)).collect();
    let seats_b: Vec<(PalwBondKeyV2, Hash64)> = [bonds[6], bonds[7], bonds[1], bonds[2], bonds[3]].iter().map(|(k, o, _)| (*k, *o)).collect();

    let mut s: PalwChainStateV2 =
        fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, attacker_collateral)], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 1, 0x5EED);
    let mut daa = 1_001u64;
    let mut blue = 2u64;
    let step = |s: &PalwChainStateV2, daa: u64, blue: u64, objs: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, sub: u64| {
        fold(&p, &sp, s, &ctx(0x7000 + blue, daa, blue, sub), objs, work, key).unwrap_or_else(|e| panic!("DAA {daa}: {e:?}")).0
    };
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    let reserved = s.claim(&id).expect("an attacker sized by #9 gets its claim").reserved;

    // Panel #1 binds; the producer serves nothing.
    daa += 1;
    blue += 1;
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA1), seats: seats_of(&seats_a) }], PalwBlockWorkV3::None, Hash64::default(), 0);
    let other_after_bind: u128 = bonds.iter().map(|(k, _, _)| s.reserved_exposure(k)).sum();
    let receipt_window = sp.receipt_window_for_claim_v1(&s, &floor, daa);

    // First receipt window lapses -> redraw.
    daa += receipt_window + 1;
    blue += 1;
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let phase_after_first = format!("{:?}", s.claim(&id).unwrap().phase);

    // Panel #2 binds on the redraw; producer withholds again.
    daa += 1;
    blue += 1;
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA2), seats: seats_of(&seats_b) }], PalwBlockWorkV3::None, Hash64::default(), 0);

    // Second receipt window lapses -> void at ReceiptTimeout.
    daa += receipt_window + 1;
    blue += 1;
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let final_phase = s.claim(&id).map(|c| c.phase.clone());
    let attacker_after = s.bond(&bond_key(ATTACKER)).unwrap().collateral;
    let charge = (attacker_collateral - attacker_after) as u128;
    let seats_collateral: u64 = bonds.iter().map(|(k, _, _)| s.bond(k).unwrap().collateral).sum();
    let seats_posted: u64 = bonds.iter().map(|(_, _, c)| *c).sum();

    println!("=== repro 3c: a bound junk claim is charged at its second receipt timeout (FIXED: #10) ===");
    println!("producer posts {:.2} MSK (#9: reserved {reserved} + escrow {} at {} permille)", msk(attacker_collateral as u128), per.escrow, per.ratio_permille);
    println!("reserved on OTHER bonds after bind #1 = {other_after_bind} ({:.2} MSK)", msk(other_after_bind));
    println!("phase after first receipt window (redraw) = {phase_after_first}");
    println!("final phase = {final_phase:?}");
    println!("producer collateral {attacker_collateral} -> {attacker_after} (charged {charge} = {:.2} MSK; before: 0)", msk(charge));
    println!("seats' collateral {seats_posted} -> {seats_collateral} (charged {})", seats_posted - seats_collateral);
    println!("the next wave: {:.2} MSK left backs {} more attempt(s)", msk(attacker_after as u128), attacker_after / per.collateral);

    assert!(matches!(final_phase, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. })),
        "the withheld claim voids at ReceiptTimeout, got {final_phase:?}");
    assert_eq!(charge, per.reservation, "#10: the producer forfeits weight + escrow");
    assert!(charge >= other_after_bind, "and pays at least what it pinned on others");
    // The premise, restated in attempts re-funded: the producer can no longer post exactly one
    // #9 attempt's collateral (the registration floor — 13,000 MSK on t12 — is above it), so the
    // bond is the floor and funds several. `waves(c)` = the withheld attempts a bond of `c` can
    // still post if each costs one reservation; the forfeiture takes exactly one off it (at the
    // old one-attempt sizing: 1 -> 0, "the same collateral no longer re-funds the next attempt").
    let waves = |c: u64| -> u64 {
        if c < per.collateral { 0 } else { 1 + (c - per.collateral) / u64::try_from(per.reservation).unwrap().max(1) }
    };
    println!("withheld attempts the bond funds: {} -> {}", waves(attacker_collateral), waves(attacker_after));
    assert!(waves(attacker_collateral) >= 1, "the bond funds the attempt it made");
    assert_eq!(waves(attacker_after), waves(attacker_collateral) - 1, "each withheld attempt costs the bond one attempt's re-funding");
    assert_eq!(seats_collateral, seats_posted, "no seat is charged for the withheld claim");
}

/// **3d. The 2M attack end to end on the fixed code.** The shipped 2M row opens `Prefetching`
/// (3b) and needs seven seats to prove the real artifact, which no core test can reach — so the
/// row is set `Active` through the state carriage, the one thing this test does not fold (its
/// derived profile, `max_inflight_claims` 1, is the genesis row's own). The fixture carries no
/// work-target fold, so admission is the registry's inflight cap; past the work target the
/// processor prices the same one-claim replay budget through `panel_room_v1`.
///
/// An attacker bond posting exactly what #9 admits for ONE junk 2M attempt folds it, binds it on
/// the least-loaded honest bonds with 2M headroom, and withholds through two panels. Each DAA an
/// honest genesis bond's 2M attempt is folded on a copy of the state: refused = the lane is closed.
/// Measured: the charge #10 takes, the honest capital pinned (peak and x time), the DAA closed.
#[test]
fn dos_repro_3d_one_junk_2m_claim_forfeits_less_than_it_pins() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let economy = p.palw_seat_economy_at(0).expect("t12 arms the panel economy");
    let seat_count = b.panel.seat_count() as usize;
    let ratio = economy.max_exposure_ratio_permille;
    let g = genesis_state(&p);
    let (id2m, _leaves2m, _target2m, _slash) = genesis_classes(&p)[2];
    let (_, pwu2m, _) = runtime_2m_reserved(&p, &g);

    // The 2M row made Active (the only non-folded step).
    let mut carriage = PalwStateCarriageV2::from_state(&g);
    carriage.model_lifecycles.get_mut(&id2m).expect("the 2M row").state = kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
    let g = carriage.into_state(&sp, None).expect("a consistent carriage");
    let max_inflight = g.model_lifecycle(&id2m).unwrap().profile.max_inflight_claims;

    let per = admitted_per_attempt_on(&p, &g, id2m, pwu2m, T12_BLOCK_SUBSIDY_SOMPI);
    let honest = genesis_bonds(&p);
    let seat_2m = palw_panel_seat_exposure_v1(per.reserved, per.escrow as u64, seat_count, economy.reward_multiple_permille);
    let attacker_collateral = at_least_the_floor(&p, per.collateral);
    let mut s = fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, attacker_collateral)], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    // An honest producer: the first genesis bond, with its own keys.
    let exec = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => Some((*bond, pubkey.clone(), operator_pubkey.clone())),
            _ => None,
        })
        .unwrap();

    let honest_reserved = |s: &PalwChainStateV2| -> u128 { honest.iter().map(|(k, _, _)| s.reserved_exposure(k)).sum() };
    let bind = |s: &PalwChainStateV2, id: Hash64, anchor: u64| -> Option<PalwConsensusObjectV2> {
        let mut order: Vec<&(PalwBondKeyV2, Hash64, u64)> = honest
            .iter()
            .filter(|(k, _, _)| palw_seat_has_headroom_v1(s.bond(k).unwrap().collateral, s.reserved_exposure(k) + s.registration_exposure(k), seat_2m, ratio))
            .collect();
        order.sort_by_key(|(k, _, _)| s.reserved_exposure(k));
        (order.len() >= seat_count).then(|| {
            let seats: Vec<(PalwBondKeyV2, Hash64)> = order.iter().take(seat_count).map(|(k, o, _)| (*k, *o)).collect();
            PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(anchor), seats: seats_of(&seats) }
        })
    };

    // The honest probe: a genesis bond's 2M attempt folded on a copy of the next block. `Some` =
    // refused, with the reason.
    let probe = |s: &PalwChainStateV2, daa: u64, blue: u64| -> Option<String> {
        let (henv, hkey, _) = junk_attempt(id2m, exec.0, exec.1.clone(), &exec.2, pwu2m, 0x4D00 + daa, 0x4D_0000 + daa);
        match fold(&p, &sp, s, &ctx(0x3E_0000 + blue, daa + 1, blue + 1, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&henv), hkey) {
            Err(e) => Some(format!("{e:?}")),
            Ok((_, _, skips)) if !skips.is_empty() => Some(skips[0].1.clone()),
            Ok(_) => None,
        }
    };
    let mut probes: Vec<(u64, &'static str, Option<String>)> = Vec::new();

    let mut daa = 1_001u64;
    let mut blue = 2u64;
    probes.push((daa - 1, "before the junk claim", probe(&s, daa - 1, blue - 1)));
    let (env, key, id) = junk_attempt(id2m, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu2m, 0x3D01, 0x3D01_0000);
    let (next, _, skips) = fold(&p, &sp, &s, &ctx(0x3D_0000 + blue, daa, blue, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key).unwrap();
    assert!(skips.is_empty(), "an attacker sized by #9 is admitted: {skips:?}");
    s = next;
    let accepted = daa;
    probes.push((daa, "junk accepted", probe(&s, daa, blue)));
    // Integrate pinned / locked sompi x DAA between the event blocks (state is constant between).
    let (mut last, mut pinned_x_daa, mut locked_x_daa, mut peak_pinned, mut binds) = (daa, 0u128, 0u128, 0u128, 0u64);
    let mut tick = |s: &PalwChainStateV2, now: u64| {
        let dt = (now - last) as u128;
        pinned_x_daa += honest_reserved(s) * dt;
        locked_x_daa += s.reserved_exposure(&bond_key(ATTACKER)) * dt;
        last = now;
    };
    let step = |s: &mut PalwChainStateV2, daa: u64, blue: u64, objects: Vec<PalwConsensusObjectV2>| {
        *s = fold(&p, &sp, s, &ctx(0x3D_0000 + blue, daa, blue, 0), &objects, PalwBlockWorkV3::None, Hash64::default())
            .unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"))
            .0;
    };
    let mut receipt_window = 0u64;
    for panel in 0..2u64 {
        // Bind on the least-loaded honest bonds with 2M headroom.
        daa += 1;
        blue += 1;
        tick(&s, daa);
        let bound = bind(&s, id, 0xA000 + panel).expect("the honest bonds seat the junk claim");
        binds += 1;
        step(&mut s, daa, blue, vec![bound]);
        peak_pinned = peak_pinned.max(honest_reserved(&s));
        receipt_window = sp.receipt_window_for_claim_v1(&s, &id2m, daa);
        probes.push((daa, if panel == 0 { "bound #1" } else { "bound #2 (redraw)" }, probe(&s, daa, blue)));
        // Two blocks before the receipt deadline (the probe folds the next one, still inside it),
        // then the lapse.
        daa += receipt_window - 1;
        blue += 1;
        tick(&s, daa);
        step(&mut s, daa, blue, vec![]);
        probes.push((daa + 1, "last block of the receipt window", probe(&s, daa, blue)));
        daa += 2;
        blue += 1;
        tick(&s, daa);
        step(&mut s, daa, blue, vec![]);
        probes.push((daa, if panel == 0 { "receipt timeout #1 (redraw)" } else { "receipt timeout #2" }, probe(&s, daa, blue)));
    }
    let voided_at = daa;
    let dead: u64 = voided_at - accepted;
    let phase = s.claim(&id).map(|c| c.phase.clone());
    let after = s.bond(&bond_key(ATTACKER)).unwrap().collateral;
    let charge = (attacker_collateral - after) as u128;
    let honest_after: u64 = honest.iter().map(|(k, _, _)| s.bond(k).unwrap().collateral).sum();
    let honest_posted: u64 = honest.iter().map(|(_, _, c)| *c).sum();

    println!("=== repro 3d: ONE junk 2M claim on the fixed code (2M row set Active via the carriage; max_inflight_claims {max_inflight}) ===");
    println!(
        "per 2M junk claim: reserved {:.2} + escrow {:.2} = {:.2} MSK on the producer; #9 asks {:.2} MSK posted; seat duty {:.2} MSK",
        msk(per.reserved),
        msk(per.escrow),
        msk(per.reservation),
        msk(per.collateral as u128),
        msk(seat_2m)
    );
    println!("panel bindings {binds}; final phase {phase:?}");
    println!("attacker charged = {:.2} MSK (before: 0)", msk(charge));
    println!(
        "honest pinned: peak {:.2} MSK (before: 896,144.13); {:.3e} sompi*DAA vs attacker locked {:.3e} -> {:.2}x",
        msk(peak_pinned),
        pinned_x_daa as f64,
        locked_x_daa as f64,
        pinned_x_daa as f64 / locked_x_daa.max(1) as f64
    );
    println!("2M receipt window = {receipt_window} DAA (the row's verification window)");
    for (d, what, r) in &probes {
        println!("  honest 2M attempt at DAA {d:>6} ({what}): {}", r.as_deref().map(|r| r.split(" {").next().unwrap_or(r)).unwrap_or("admitted"));
    }
    println!("2M lane closed to an honest claim: {dead} DAA (accepted {accepted} -> voided {voided_at})");
    println!("honest collateral {honest_posted} -> {honest_after}");
    println!("peak pinned / charge = {:.2}x", peak_pinned as f64 / charge.max(1) as f64);

    assert_eq!(max_inflight, 1, "the 2M row admits one claim in flight");
    assert!(probes[0].2.is_none(), "before the junk claim an honest 2M attempt is admitted");
    assert!(
        probes[1..probes.len() - 1].iter().all(|(_, _, r)| r.as_deref().is_some_and(|r| r.starts_with("ClassInflightCapped"))),
        "while the junk claim is in flight every honest 2M attempt is refused ClassInflightCapped: {probes:?}"
    );
    assert!(probes.last().unwrap().2.is_none(), "the void reopens the lane: {:?}", probes.last());
    assert!(matches!(phase, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. })), "withheld to the second ReceiptTimeout: {phase:?}");
    assert_eq!(binds, 2, "bound twice (the redraw)");
    assert!(dead >= 2 * receipt_window, "closed for two receipt windows");
    assert_eq!(honest_after, honest_posted, "no honest bond is charged");
    assert_eq!(charge, per.reservation, "#10: the junk 2M claim forfeits weight + escrow");
    // **The bound: the attacker pays at least what it pins others for.**
    assert!(
        charge >= peak_pinned,
        "FINDING (row 7, open past #10): one junk 2M claim forfeits {:.2} MSK and pins {:.2} MSK of honest collateral ({:.2}x), \
         holding the 2M row's one inflight slot (the lane closed) {dead} DAA — each of 5 seats' duty is 3 x claim.reserved, \
         the forfeit is claim.reserved + escrow once",
        msk(charge),
        msk(peak_pinned),
        peak_pinned as f64 / charge.max(1) as f64
    );
}
