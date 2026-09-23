//! **L5 stress test 2 — model-registration flood.**
//!
//! The cheapest admissible registration testnet-12 accepts past its fences: a held Qwen2.5 A16 row
//! at an attacker-chosen `n_ctx` with an attacker-chosen (unverified) `artifact_root`, built by the
//! shipped `qwen25_a16_held_registration_v1`, admitted by the real gate `verify_class_admission_v9`
//! (called exactly as processor.rs calls it at DAA 0), then folded through the real transition with
//! the registry fold armed.
//!
//! Measured: per-registration admission CPU on every node, rooted bytes per class, what the
//! registrant has reserved (`registration_exposure_sompi`, charged against COLLATERAL — not the
//! 50 % ceiling — at palw_state_v2.rs:15767-15784), the relay fee the carrier's mass implies
//! (`palw_relay_fee_for_mass_v1` over the object's borsh bytes — a LOWER bound on the tx's mass),
//! the registry's span-boundary step over N rows, and which rows survive once the classes go idle.
//! The registry's derived registration bond (`registration_bond_sompi`, palw_model_registry_v1.rs:256)
//! is printed next to what is enforced.
//!
//! **Converted to the fix (2026-09-24 DoS audit #12 (b)).** Past t12's audit fence every bought
//! registration BURNS 1 MSK of its registrant's bond (`PALW_CLASS_REGISTRATION_BURN_SOMPI_V1`,
//! into `slashed`, which the bond's release spend must destroy) on top of the 40,000-sompi
//! reservation it gets back at reclamation, and a block folds at most four
//! (`PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1`). The flood's non-refundable price per class is
//! asserted from the registrant's own bond. The comparison the audit asserted — the reservation
//! against the registry's DERIVED registration bond (2,000 MSK here) — is not what #12 decided
//! and stays a printed residual, not an assertion.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_l5_2_registration_flood -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_class_admission_v2::{palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1, PalwBlockWorkV3, PalwChainStateV2, PalwConsensusObjectV2,
    palw_bond_burn_obligation_v2, palw_relay_fee_for_mass_v1,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use std::time::Instant;

const N_REG: u64 = 200;
const REGISTRANT: u64 = 9;
/// 1,000 MSK, or the registration floor where that is higher (the producer floor, 13,000 MSK on
/// testnet-12's regenesis params — `palw_bond_registration_floor_v1`).
fn registrant_collateral(p: &kaspa_consensus_core::config::params::Params) -> u64 {
    at_least_the_floor(p, 100_000_000_000)
}

#[test]
fn dos_l5_2_registration_flood() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();

    let admit = |profile: &PalwShapeProfileV3, job: &PalwJobContextV2, reg: &PalwConsensusObjectV2| -> Result<u64, String> {
        let shape = palw_admission_shape_at_v1(&p, &b, profile, 0).map_err(|e| format!("{e:?}"))?;
        verify_class_admission_v9(
            &b,
            profile,
            job,
            reg,
            &certified,
            &[],
            shape.ladder,
            shape.court,
            false,
            shape.token_lift,
            shape.fused_dissectable,
            p.palw_canonical_work_at(0),
            shape.held,
            shape.kimi_family,
            p.palw_audit_2026_09_23_active_at(0),
        )
        .map(|e| e.canonical_step_leaf_count)
        .map_err(|e| format!("{e}"))
    };

    // ---- build + admit N distinct registrations -----------------------------------------------
    let mut regs = Vec::new();
    let mut admit_s = 0f64;
    let mut refused = 0u64;
    let mut n_ctx = 512u32;
    let mut tried = 0u64;
    while (regs.len() as u64) < N_REG && tried < 4 * N_REG {
        tried += 1;
        n_ctx += 1;
        let Ok((prof, _, reg)) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_registration_v1(
            h(0xBAD0_0000 + n_ctx as u64),
            n_ctx,
            0,
            5,
            u128::MAX / 2,
            &b,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            bond_key(REGISTRANT),
        ) else {
            refused += 1;
            continue;
        };
        let (pf, d) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(n_ctx);
        let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&prof, pf, d);
        let t = Instant::now();
        let verdict = admit(&prof, &job, &reg);
        admit_s += t.elapsed().as_secs_f64();
        match verdict {
            Ok(_) => regs.push(reg),
            Err(_) => refused += 1,
        }
    }
    let admit_ms = admit_s * 1e3 / tried as f64;
    let mass: Vec<usize> = regs.iter().map(|r| borsh::to_vec(r).unwrap().len()).collect();
    let avg_mass = mass.iter().sum::<usize>() as f64 / mass.len() as f64;
    let fee = palw_relay_fee_for_mass_v1(avg_mass as u64);

    // ---- fold them, one per chain block ---------------------------------------------------------
    let g = genesis_state(&p);
    let mut s: PalwChainStateV2 =
        fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(REGISTRANT, registrant_collateral(&p))], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
    let base_bytes = carriage_bytes(&s);
    let bond_before = s.bond(&bond_key(REGISTRANT)).unwrap().clone();
    let t0 = Instant::now();
    let base_root = {
        let t = Instant::now();
        for _ in 0..20 {
            let _ = s.state_root();
        }
        t.elapsed().as_secs_f64() / 20.0
    };
    let mut daa = 1_000u64;
    let mut folded = 0u64;
    let mut fold_errors = Vec::new();
    for (i, reg) in regs.iter().enumerate() {
        daa += 1;
        match fold(&p, &sp, &s, &ctx(0x2000 + i as u64, daa, 2 + i as u64, 0), std::slice::from_ref(reg), PalwBlockWorkV3::None, Hash64::default()) {
            Ok((next, _, _)) => {
                s = next;
                folded += 1;
            }
            Err(e) => fold_errors.push(format!("{e:?}")),
        }
    }
    let fold_total = t0.elapsed().as_secs_f64();
    let bond_after = s.bond(&bond_key(REGISTRANT)).unwrap().clone();
    // #12 (b)'s cap: five of them in ONE block is refused whole, the fold's second lock.
    let five_refused = {
        let extra: Vec<PalwConsensusObjectV2> = regs.iter().take(5).cloned().collect();
        let fresh = fold(&p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(REGISTRANT, registrant_collateral(&p))], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;
        fold(&p, &sp, &fresh, &ctx(0x2FFF, 1_001, 2, 0), &extra, PalwBlockWorkV3::None, Hash64::default()).is_err()
    };
    let peak_bytes = carriage_bytes(&s);
    let reserved = s.registration_exposure(&bond_key(REGISTRANT));
    let peak_root = {
        let t = Instant::now();
        for _ in 0..20 {
            let _ = s.state_root();
        }
        t.elapsed().as_secs_f64() / 20.0
    };
    let lifecycles_peak = s.model_lifecycles_iter().count();

    // ---- idle for many epochs / registry spans ----------------------------------------------------
    let span = p.palw_execution_lane_at(daa).map(|l| l.schedule_span_daa_at(daa)).unwrap_or(0);
    let mut blue = 2 + regs.len() as u64;
    let mut boundary_ms = Vec::new();
    for k in 1..=12u64 {
        blue += 1;
        let jump = daa + k * sp.epoch_length().max(span.max(1));
        let t = Instant::now();
        let (next, _, _) = fold(&p, &sp, &s, &ctx(0x3000 + k, jump, blue, 0), &[], PalwBlockWorkV3::None, Hash64::default()).unwrap();
        boundary_ms.push(t.elapsed().as_secs_f64() * 1e3);
        s = next;
    }
    let idle_bytes = carriage_bytes(&s);
    let idle_reserved = s.registration_exposure(&bond_key(REGISTRANT));
    let classes_left = s.classes_iter().count();
    let statuses: std::collections::BTreeMap<String, usize> = s.classes_iter().fold(Default::default(), |mut m, (_, c)| {
        let k = format!("{:?}", c.status).split([' ', '{', '(']).next().unwrap_or("?").to_string();
        *m.entry(k).or_default() += 1;
        m
    });
    let lifecycle_bond = s
        .model_lifecycles_iter()
        .filter_map(|(id, row)| regs.iter().any(|r| matches!(r, PalwConsensusObjectV2::ClassRegistered { class_id, .. } if class_id == id)).then_some(row.profile.registration_bond_sompi))
        .max()
        .unwrap_or(0);

    let per_class = (peak_bytes - base_bytes) as f64 / folded.max(1) as f64;
    let residue_per_class = (idle_bytes.saturating_sub(base_bytes)) as f64 / folded.max(1) as f64;
    let burned = bond_before.collateral - bond_after.collateral;
    let attacker_sompi = (burned as f64 / folded.max(1) as f64) + fee as f64;
    println!("=== registration flood: {} admitted of {tried} tried ({refused} refused) ===", regs.len());
    println!("admission CPU (verify_class_admission_v9, debug)   = {admit_ms:.3} ms per registration per node");
    println!("object bytes (borsh, lower bound on mass)          = {avg_mass:.0} B  -> relay fee >= {fee} sompi ({:.6} MSK)", msk(fee as u128));
    println!("registration exposure (enforced)                   = {} sompi per live class, against COLLATERAL (not the 50% ceiling)", sp.registration_exposure_sompi());
    println!("registry-derived registration bond (NOT enforced)  = {lifecycle_bond} sompi ({:.2} MSK)", msk(lifecycle_bond as u128));
    println!("classes one {:.0} MSK bond may register            = {}", msk(registrant_collateral(&p) as u128), registrant_collateral(&p) / sp.registration_exposure_sompi().max(1));
    println!("folded {folded}/{} ({} fold errors{}) in {fold_total:.2} s", regs.len(), fold_errors.len(), fold_errors.first().map(|e| format!(", first: {e}")).unwrap_or_default());
    println!("rooted bytes: {base_bytes} -> {peak_bytes}  = {per_class:.0} B per class; lifecycle rows {lifecycles_peak}; registrant reserved {reserved}");
    println!("state_root(): {:.3} ms -> {:.3} ms", base_root * 1e3, peak_root * 1e3);
    println!("idle boundary folds (ms, debug): {:?}", boundary_ms.iter().map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>());
    println!("after 12 idle epochs/spans: bytes {idle_bytes} ({residue_per_class:.0} B/class residue), classes {classes_left} {statuses:?}, registrant reserved {idle_reserved}");
    println!("--- the ratio ---");
    println!("burned by the registrant: {burned} sompi over {folded} registrations (+{} to `slashed`, the release spend's burn)", bond_after.slashed - bond_before.slashed);
    println!("five registrations in one block refused: {five_refused}");
    println!("attacker per class, NOT returned: {attacker_sompi:.0} sompi ({:.6} MSK: the 1 MSK burn + the fee; the {} sompi reservation comes back at Dormant)", attacker_sompi / 1e8, sp.registration_exposure_sompi());
    println!("defender per class: {per_class:.0} rooted B on every node ({residue_per_class:.0} B forever) + {admit_ms:.3} ms admission CPU + a re-hash every block");
    println!("RATIO rooted bytes per MSK the attacker pays = {:.0} B/MSK", per_class / (attacker_sompi / 1e8));

    assert!(folded > 0, "the flood folds");
    assert!(fold_errors.is_empty(), "one registration a block is under the cap: {fold_errors:?}");
    // **The bound #12 (b) enforces: every registration destroys 1 MSK of its registrant's bond**,
    // recorded where the release spend must burn it, and none of it comes back when the class idles.
    assert_eq!(burned, folded * PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, "each folded registration burned 1 MSK");
    assert_eq!(bond_after.slashed - bond_before.slashed, burned);
    assert_eq!(palw_bond_burn_obligation_v2(&bond_after) - palw_bond_burn_obligation_v2(&bond_before), burned);
    assert_eq!(s.bond(&bond_key(REGISTRANT)).unwrap().collateral, bond_after.collateral, "idling returns no burned sompi");
    assert!(PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1 == 4 && five_refused, "and a block folds at most four");
    // Residual, printed not asserted: the registry's derived registration bond is still not what a
    // registration reserves — #12 decided a burn, not that bond.
    println!(
        "RESIDUAL: reservation {} sompi vs registry-derived registration bond {lifecycle_bond} sompi (not enforced)",
        sp.registration_exposure_sompi()
    );
}
