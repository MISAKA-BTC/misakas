//! **L5 composite — "the network is alive but useful PALW execution stalls", end to end at fold level.**
//!
//! One attacker bond on testnet-12's genesis state (real fold, real params, registry fold armed):
//!
//!   1. registers cheap classes (the registration the flood test measured: 40,000 sompi each);
//!   2. floods junk floor claims — one per chain block, each a won floor draw that cost hashes, not
//!      inferences — and binds each one's panel itself (a `PanelBound` is derived from public state
//!      and costs a relay fee); every bound seat goes on duty at the ADR-0130 floor,
//!      `max(3 x reserved, λ x per-seat share of the ESCROW)`, while the producer reserves only
//!      `reserved` (0.000385 MSK on the floor);
//!   3. withholds: seats cannot sign Valid on junk; the receipt window lapses, the claim is redrawn
//!      once onto a fresh panel (more seats on duty), lapses again and voids `ReceiptTimeout`. At the
//!      audit's commit that charged the producer nothing; past #10 (b38356fe) it goes through
//!      `void_and_slash` and forfeits weight + escrow;
//!   4. at the audit's commit the attacker's reservation was back the instant the claim voided, so
//!      10 MSK re-funded the whole flood; past option A and #9 every concurrent claim holds its
//!      escrow on the attacker's bond and the live-state ceiling refuses what the bond cannot back,
//!      so the attacker here is sized from the runtime for the whole flood (`attacker_collateral`);
//!   5. with no `Final` in the table the D1 second-clock floor is `None` (the bootstrap waiver):
//!      a bond registered in the stall judges after the 1,000-DAA window alone (dos_l5_5 measures the
//!      same waiver re-opening after a real history retires).
//!
//! Measured per DAA: honest bonds' headroom, how many honest bonds can still seat a floor claim and
//! a 2M claim (the real headroom predicate with the registry-armed 2M reservation), the attacker's
//! locked capital, and its cumulative slash. Output: attacker cost vs honest PALW throughput lost.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l5_6_composite -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_exposure_unit_pwu_v1;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_panel_seat_exposure_v1, palw_seat_has_headroom_v1};
use kaspa_consensus_core::palw_panel_v2::palw_settled_anchor_floor_daa_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2};
use std::time::Instant;

const ATTACKER: u64 = 1;
/// The report's pre-fix composite (§2 row H / §1): 1,535,125.98 MSK of honest collateral pinned,
/// the 2M lane dead 1,436 of 3,650 DAA, the attacker charged 0 — at a 10 MSK producer.
const BEFORE_PINNED_MSK: f64 = 1_535_125.98;
const BEFORE_2M_DEAD_DAA: u64 = 1_436;

/// **The attacker #9 admits for this composite, from the runtime.** Every junk floor claim of the
/// flood can be live at once (a withheld claim lives ~2 receipt windows, longer than the flood),
/// so the bond must back `FLOOD_DAA` concurrent attempts at `admitted_per_attempt` each, plus the
/// `N_CLASSES` registrations' exposure under the same ceiling, plus the 1 MSK each registration
/// BURNS past the audit fence (2026-09-24 DoS audit #12 (b), `PALW_CLASS_REGISTRATION_BURN_SOMPI_V1`).
fn attacker_collateral(p: &kaspa_consensus_core::config::params::Params, floor: Hash64, pwu: u64) -> (u64, AdmittedPerAttempt) {
    let b = bundle(p);
    let per = admitted_per_attempt(p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let reg = (u128::from(b.state.registration_exposure_sompi()) * N_CLASSES as u128 * 1000).div_ceil(u128::from(per.ratio_permille)) as u64;
    let burn = N_CLASSES * kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1;
    (per.collateral * FLOOD_DAA + reg + burn, per)
}
const FLOOD_DAA: u64 = 1_200;
const N_CLASSES: u64 = 20;
const PANEL_RELAY_FEE_SOMPI: u64 = 10_000; // palw_state_v2.rs:16283 ("for a 10,000-sompi relay fee")

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
fn dos_l5_6_composite_alive_but_palw_stalls() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let economy = p.palw_seat_economy_at(0).expect("t12 arms the panel economy");
    let seat_count = b.panel.seat_count() as usize;
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let (attacker_collateral, per) = attacker_collateral(&p, floor, pwu);
    let honest: Vec<(PalwBondKeyV2, Hash64, u64)> = genesis_bonds(&p);
    let r2m = dense_2m_reserved_sompi();
    let escrow = sp.worker_carve_at(T12_BLOCK_SUBSIDY_SOMPI, p.palw_overlay_carve.and_then(|c| c.escrow_carve()));
    let seat_2m = palw_panel_seat_exposure_v1(r2m, escrow, seat_count, economy.reward_multiple_permille);
    let seat_floor_honest = palw_panel_seat_exposure_v1(38_540, escrow, seat_count, economy.reward_multiple_permille);

    // ---- 1. registration ------------------------------------------------------------------------
    let g = genesis_state(&p);
    let mut s: PalwChainStateV2 =
        fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, attacker_collateral)], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    let mut daa = 1_000u64;
    let mut blue = 1u64;
    let mut reg_fees = 0u64;
    for i in 0..N_CLASSES {
        let n_ctx = 600 + i as u32;
        let (_, _, reg) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_registration_v1(
            h(0xC0FFEE00 + i),
            n_ctx,
            0,
            5,
            u128::MAX / 2,
            &b,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            bond_key(ATTACKER),
        )
        .unwrap();
        reg_fees += kaspa_consensus_core::palw_state_v2::palw_relay_fee_for_mass_v1(borsh::to_vec(&reg).unwrap().len() as u64);
        daa += 1;
        blue += 1;
        s = fold(&p, &sp, &s, &ctx(0x100 + i, daa, blue, 0), &[reg], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    }
    let reg_exposure = s.registration_exposure(&bond_key(ATTACKER));
    // #12 (b): what the registrations destroyed — a price, not a charge for the attack below, so it
    // is taken out of `slashed` there.
    let reg_burned = attacker_collateral - s.bond(&bond_key(ATTACKER)).unwrap().collateral;

    // ---- 2-4. flood, bind, withhold, time out, reuse ------------------------------------------------
    let honest_room = |s: &PalwChainStateV2, stake: u128| -> usize {
        honest
            .iter()
            .filter(|(k, _, _)| {
                let bnd = s.bond(k).unwrap();
                palw_seat_has_headroom_v1(bnd.collateral, s.reserved_exposure(k) + s.registration_exposure(k), stake, economy.max_exposure_ratio_permille)
            })
            .count()
    };
    let honest_reserved = |s: &PalwChainStateV2| -> u128 { honest.iter().map(|(k, _, _)| s.reserved_exposure(k)).sum() };
    let t0 = Instant::now();
    let mut attempts = 0u64;
    let mut binds = 0u64;
    let mut unbindable = 0u64;
    let mut max_attacker_reserved = 0u128;
    let mut max_honest_reserved = 0u128;
    let mut min_2m_room = usize::MAX;
    let mut daa_2m_dead = 0u64;
    let mut daa_floor_dead = 0u64;
    let mut samples = Vec::new();
    let mut admitted = 0u64;
    let mut refused_by_ceiling = 0u64;
    let mut honest_integral = 0u128; // sompi x DAA pinned on honest bonds
    let mut attacker_integral = 0u128; // sompi x DAA held on the attacker's bond
    let mut void_reasons: std::collections::BTreeMap<String, u64> = Default::default();
    let mut seen_void: std::collections::BTreeSet<Hash64> = Default::default();
    let start = daa;
    let end = start + FLOOD_DAA + 2 * (sp.window_bind() + 600) + 50;
    let mut seed = 0u64;
    while daa < end {
        daa += 1;
        blue += 1;
        // bind every attacker claim that is Provisional (fresh ones and redraws), on a derived-style
        // panel: the first `seat_count` honest bonds with headroom, rotated by the claim id.
        let mut objects = Vec::new();
        let provisional: Vec<(Hash64, u128, u64)> = s
            .claims_iter()
            .filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::Provisional) && c.bond == bond_key(ATTACKER))
            .map(|(id, c)| (*id, c.reserved, c.escrowed_reward))
            .collect();
        let mut pending: std::collections::BTreeMap<PalwBondKeyV2, u128> = Default::default();
        for (id, reserved, esc) in provisional {
            let stake = palw_panel_seat_exposure_v1(reserved, esc, seat_count, economy.reward_multiple_permille);
            let rot = (id.as_bytes()[0] as usize) % honest.len();
            let mut seats = Vec::new();
            for j in 0..honest.len() {
                let (k, o, _) = honest[(rot + j) % honest.len()];
                let bnd = s.bond(&k).unwrap();
                let backed = s.reserved_exposure(&k) + s.registration_exposure(&k) + pending.get(&k).copied().unwrap_or(0);
                if palw_seat_has_headroom_v1(bnd.collateral, backed, stake, economy.max_exposure_ratio_permille) {
                    seats.push((k, o));
                }
                if seats.len() == seat_count {
                    break;
                }
            }
            if seats.len() < seat_count {
                unbindable += 1;
                continue;
            }
            for (k, _) in &seats {
                *pending.entry(*k).or_default() += stake;
            }
            objects.push(PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(seed ^ 0xA0), seats: seats_of(&seats) });
            binds += 1;
        }
        // one junk floor attempt per chain block while the flood lasts
        let flooding = daa <= start + FLOOD_DAA;
        let env_key = if flooding {
            seed += 1;
            let (env, key, _) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, seed, 0xF100D + seed);
            attempts += 1;
            Some((env, key))
        } else {
            None
        };
        let (work, key, subsidy) = match &env_key {
            Some((env, key)) => (PalwBlockWorkV3::Attempt(env), *key, T12_BLOCK_SUBSIDY_SOMPI),
            None => (PalwBlockWorkV3::None, Hash64::default(), 0),
        };
        let (next, _, skips) =
            fold(&p, &sp, &s, &ctx(0x10_0000 + blue, daa, blue, subsidy), &objects, work, key).unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"));
        s = next;
        if env_key.is_some() {
            if skips.is_empty() { admitted += 1 } else { refused_by_ceiling += 1 }
        }
        for (id, c) in s.claims_iter() {
            if c.bond == bond_key(ATTACKER) {
                if let PalwClaimPhaseV2::Voided { reason, .. } = &c.phase {
                    if seen_void.insert(*id) {
                        *void_reasons.entry(format!("{reason:?}")).or_default() += 1;
                    }
                }
            }
        }
        let ar = s.reserved_exposure(&bond_key(ATTACKER));
        let hr = honest_reserved(&s);
        honest_integral += hr;
        attacker_integral += ar;
        max_attacker_reserved = max_attacker_reserved.max(ar);
        max_honest_reserved = max_honest_reserved.max(hr);
        let room_2m = honest_room(&s, seat_2m);
        let room_floor = honest_room(&s, seat_floor_honest);
        min_2m_room = min_2m_room.min(room_2m);
        // An honest claim's panel needs `seat_count` bonds other than its own executor.
        if room_2m < seat_count + 1 {
            daa_2m_dead += 1;
        }
        if room_floor < seat_count + 1 {
            daa_floor_dead += 1;
        }
        if (daa - start) % 150 == 0 {
            samples.push((daa, ar, hr, room_floor, room_2m, s.claims_iter().count()));
        }
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let attacker_after = s.bond(&bond_key(ATTACKER)).unwrap().collateral;
    let slashed = attacker_collateral - attacker_after - reg_burned;
    let reserved_after = s.reserved_exposure(&bond_key(ATTACKER));
    let d1_floor = palw_settled_anchor_floor_daa_v1(&s, daa, p.palw_settled_anchor_depth.unwrap());

    // ---- the cost table -------------------------------------------------------------------------
    let draws = palw_expected_attempts_v1(target);
    let attacker_fees = reg_fees + binds * PANEL_RELAY_FEE_SOMPI;
    let forgone_escrow = attempts as u128 * escrow as u128;
    let honest_claims_lost_2m = daa_2m_dead; // at most one claim per DAA could have used a dead 2M lane
    println!(
        "=== composite (FIXED: #9 + #10): {attempts} junk floor attempts over {FLOOD_DAA} DAA, {admitted} admitted, {refused_by_ceiling} refused by #9, \
         {binds} panel bindings, {unbindable} could not bind (fold time {elapsed:.1} s debug) ==="
    );
    println!("{:>7} {:>16} {:>18} {:>18} {:>16} {:>7}", "DAA", "attacker res.", "honest res. (MSK)", "bonds: floor seat", "bonds: 2M seat", "claims");
    for (d, ar, hr, rf, r2, c) in &samples {
        println!("{d:>7} {ar:>16} {:>18.2} {rf:>18} {r2:>16} {c:>7}", msk(*hr));
    }
    println!("--- attacker side ---");
    println!(
        "collateral posted                       = {:.2} MSK  (#9: {:.2} MSK per concurrent floor attempt x {FLOOD_DAA} + registrations; before: 10 MSK + burn)",
        msk(attacker_collateral as u128),
        msk(per.collateral as u128)
    );
    println!("max collateral reserved at once         = {max_attacker_reserved} sompi ({:.6} MSK)", msk(max_attacker_reserved));
    println!("registration exposure ({N_CLASSES} classes)      = {reg_exposure} sompi");
    println!("registration burn ({N_CLASSES} classes, #12 (b))  = {reg_burned} sompi ({:.2} MSK)", msk(reg_burned as u128));
    println!("fees (registrations + PanelBound relay) = {attacker_fees} sompi ({:.4} MSK)", msk(attacker_fees as u128));
    println!("slashed (#10 forfeits, burn excluded)   = {slashed} sompi ({:.2} MSK; before: 0)", msk(slashed as u128));
    println!("lottery work                            = {attempts} x {draws} BLAKE2b tickets; 0 inferences");
    println!("forgone own escrow (opportunity only)   = {:.2} MSK ({:.2} MSK per claim, burned by don't-mint)", msk(forgone_escrow), msk(escrow as u128));
    println!("reserved after the flood                = {reserved_after}  -> the same collateral re-funds the next wave");
    println!("--- defender side ---");
    println!("max honest collateral pinned on duty    = {:.2} MSK  (before: {BEFORE_PINNED_MSK:.2} MSK)", msk(max_honest_reserved));
    println!("per-seat duty (floor junk claim)        = {:.2} MSK  (ADR-0130 floor: λ={}‰ x escrow share)", msk(seat_floor_honest), economy.reward_multiple_permille);
    println!("per-seat duty (2M claim, registry basis)= {:.2} MSK", msk(seat_2m));
    println!("min honest bonds with room for a 2M seat= {min_2m_room} (need {} = seats + the executor excluded)", seat_count + 1);
    println!("DAA with the 2M lane unbindable         = {daa_2m_dead} of {}  (before: {BEFORE_2M_DEAD_DAA} of 3,650)", end - start);
    println!("DAA with the floor lane unbindable      = {daa_floor_dead} of {}", end - start);
    println!("attacker claims voided, by reason       = {void_reasons:?}  (BindTimeout is uncharged by design)");
    println!("D1 second-clock floor now               = {d1_floor:?}  (None = bootstrap waiver: maturity is the DAA window alone)");
    println!("--- the ratio ---");
    println!(
        "capital x time (integrated per DAA): honest pinned {:.3e} / attacker locked {:.3e} sompi*DAA = {:.4}",
        honest_integral as f64,
        attacker_integral as f64,
        honest_integral as f64 / attacker_integral.max(1) as f64
    );
    println!("peak honest pinned / attacker charge = {:.4}", max_honest_reserved as f64 / (slashed as f64).max(1.0));
    println!(
        "honest PALW throughput lost: 2M lane dead {:.0}% of the window ({honest_claims_lost_2m} DAA; each an honest {:.2} MSK escrow not earned), \
         floor budget consumed by junk {attempts}/{} DAA",
        100.0 * daa_2m_dead as f64 / (end - start) as f64,
        msk(escrow as u128),
        FLOOD_DAA
    );
    println!("for: {:.4} MSK fees + {} sompi slashed + {:.6} MSK locked", msk(attacker_fees as u128), slashed, msk(max_attacker_reserved));

    // The attacker was sized by #9 for the whole flood, so every attempt is admitted.
    assert_eq!(admitted, attempts, "an attacker sized by #9 gets every attempt of the flood");
    assert_eq!(void_reasons.get("ReceiptTimeout").copied().unwrap_or(0), attempts, "every junk claim withholds to its second ReceiptTimeout");
    assert_eq!(slashed as u128, per.reservation * attempts as u128, "#10: each forfeits weight + escrow");
    assert_eq!(daa_2m_dead, 0, "option A's bonds hold the flood's floor duty and a 2M seat: this composite no longer closes the 2M lane");
    assert!(attacker_integral >= honest_integral, "the attacker locks at least the capital x time it pins");
    // **The bound: an attack that denies a lane must cost at least what it denies.** Stated as: a
    // producer whose claims pinned others' collateral and never settled must lose at least the
    // collateral it pinned. At the audit's commit it lost nothing (1,535,125.98 MSK pinned).
    assert!(
        slashed as u128 >= max_honest_reserved,
        "the composite pinned {:.2} MSK of honest collateral and killed the 2M lane for {daa_2m_dead} DAA; the attacker was charged {slashed} sompi",
        msk(max_honest_reserved)
    );
}
