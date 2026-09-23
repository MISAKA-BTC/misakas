//! AUDIT LANE B4 (independent re-run) — the attacker model: register an EXPENSIVE tier, execute
//! the cheap thing. Read-only probe; every number below is produced by this worktree's runtime.
//!
//! The economic chain this file measures, end to end, in the units the code uses:
//!   ccu_declared (MAC-eq/draw, a pure function of the registrant's profile + canonical job)
//!     -> class_target = MAX * min(1, ccu_declared / W)          (palw_work_ticket_target_v1)
//!     -> expected draws per win, Q32                            (palw_expected_attempts_q32_v1)
//!     -> attempted_ccu = draws x ccu_declared                   (palw_attempted_ccu_v1)
//!     -> priced_reward = min(escrow, attempted_ccu * rate/1e9)  (palw_rate_priced_reward_v1)
//! and, beside it, the compute the producer REALLY spends: draws x ccu_real.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_attempted_compute_q32_per_claim_v1, palw_economic_shape_v1, palw_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::{
    PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1,
};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

// ---------------------------------------------------------------- the t12 card

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

/// The escrow a t12 claim carries: the block subsidy through the overlay's worker carve.
/// Measured two ways in the recon; recomputed here from the fence so the number has a source.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

fn t12_escrow_sompi(p: &Params) -> u64 {
    let carve = p.palw_overlay_carve.expect("t12 arms the carve").worker_carve_permille as u64;
    T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve
}

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor")
}
fn floor_job() -> PalwJobContextV2 {
    rc_job_context(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)
}
fn hybrid_512() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid")
}
fn hybrid_job() -> PalwJobContextV2 {
    let (pf, d) = qwen36_held_canonical_v1(512);
    rc_job_context(&hybrid_512(), pf, d)
}
fn dense_2m() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense")
}
fn dense_job() -> PalwJobContextV2 {
    let (pf, d) = qwen25_a16_held_canonical_v1(2_097_152);
    rc_job_context(&dense_2m(), pf, d)
}

/// One draw's declared compute, in MAC-eq. This IS `economic_ccu_per_claim`
/// (`palw_model_work_from_carriage_v1`, palw_model_registry_v1.rs:581) and, by the pin at
/// palw_canonical_work_v1.rs:889, also `provisional_scalar_v1()` = the fork-weight scalar.
fn ccu(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("priced")
}

/// The t12 admission gate, exactly as `processor.rs:6886` calls it, at DAA 0.
/// `share_permille` is 0 because `palw_admission_independence` is armed at 0 on t12
/// (processor.rs:6779: `required = if prosecutable && !independence { floor } else { 0 }`).
fn admit(p: &Params, b: &PalwConsensusParamsV2, profile: &PalwShapeProfileV3, job: &PalwJobContextV2, root: Hash64) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(p, b, profile, 0).expect("shape");
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    };
    let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, job, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v9(
        b,
        profile,
        job,
        &reg,
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
        // 2026-09-23 audit C-4 fence, as the network resolves it.
        p.palw_audit_2026_09_23_active_at(0),
    )
    .map(|e| e.canonical_step_leaf_count)
}

fn verdict(r: &Result<u64, PalwClassAdmissionError>) -> String {
    match r {
        Ok(l) => format!("ADMITTED (leaves {l})"),
        Err(e) => format!("REFUSED {e:?}"),
    }
}

// ------------------------------------------------- the economic chain, measured

#[derive(Debug, Clone, Copy)]
struct Econ {
    ccu_declared: u128,
    ccu_real: u128,
    target: u128,
    draws_q32: u128,
    draws_int: u64,
    attempted_ccu: u128,
    reward_sompi: u64,
    real_work_per_claim: u128,
}

fn econ(ccu_declared: u128, ccu_real: u128, w: u128, escrow: u64) -> Econ {
    let target = palw_work_ticket_target_v1(ccu_declared, w);
    let draws_q32 = palw_expected_attempts_q32_v1(target);
    let draws_int = palw_expected_attempts_v1(target);
    // block_bits = 0 on a chain with no bits-priced lane -> the network factor is exactly one
    // (palw_network_draws_q32_from_bits_v1).
    let attempted_ccu = palw_attempted_compute_q32_per_claim_v1(draws_q32, ccu_declared);
    let reward_sompi = palw_rate_priced_reward_v1(escrow, attempted_ccu, T12_RATE_SOMPI_PER_GIGA as u128);
    let real_work_per_claim = palw_attempted_compute_q32_per_claim_v1(draws_q32, ccu_real);
    Econ { ccu_declared, ccu_real, target, draws_q32, draws_int, attempted_ccu, reward_sompi, real_work_per_claim }
}

fn show(tag: &str, e: &Econ) {
    let msk = |s: u64| s as f64 / 1e8;
    println!(
        "  {tag:<46} ccu_decl {:>20}  ccu_real {:>20}  draws {:>10.4} (int {:>6})  attempted {:>22}  reward {:>10.4} MSK  real/claim {:>22}  MSK per G real MAC-eq {:>12.6}",
        e.ccu_declared,
        e.ccu_real,
        e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
        e.draws_int,
        e.attempted_ccu,
        msk(e.reward_sompi),
        e.real_work_per_claim,
        (e.reward_sompi as f64 / 1e8) / (e.real_work_per_claim as f64 / 1e9),
    );
}

#[test]
fn b4r_00_the_economic_chain_on_the_three_shipped_rows() {
    let p = t12();
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    println!("\n==== B4R.0  t12 economics ====");
    println!("  subsidy           {T12_BLOCK_SUBSIDY_SOMPI} sompi/block");
    println!("  worker carve      {} permille", p.palw_overlay_carve.unwrap().worker_carve_permille);
    println!("  escrow per claim  {escrow} sompi = {:.5} MSK", escrow as f64 / 1e8);
    println!("  rate              {T12_RATE_SOMPI_PER_GIGA} sompi per 1e9 MAC-eq");
    println!("  W0 = escrow*1e9/rate = {w} CCU (MAC-eq)");
    println!();
    for (name, prof, job) in [
        ("floor  BASE-0 (8,4)", floor_profile(), floor_job()),
        ("hybrid Qwen3.6 @512", hybrid_512(), hybrid_job()),
        ("dense  Qwen2.5 @2M ", dense_2m(), dense_job()),
    ] {
        let c = ccu(&prof, &job);
        let e = econ(c, c, w, escrow);
        println!("  {name}: ccu/draw {c}   ccu/W0 = {:.9}", c as f64 / w as f64);
        show("   honest", &e);
    }
}

// ---------------------------------------------------------------- the sweeps

/// Rewrite every declared weight dtype of every node in every table.
fn with_dtype(mut p: PalwShapeProfileV3, code: u8) -> PalwShapeProfileV3 {
    for table in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
        for n in table.iter_mut() {
            for d in n.weight_dtypes.iter_mut() {
                *d = code;
            }
        }
    }
    p
}

/// Strip the `.routed` suffix the economic module uses to find a block-diagonal expert matmul.
fn unroute(mut p: PalwShapeProfileV3) -> PalwShapeProfileV3 {
    for table in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
        for n in table.iter_mut() {
            if let Some(stem) = n.weight_name.strip_suffix(".routed") {
                n.weight_name = format!("{stem}.dense_w");
            }
        }
    }
    p
}

/// Rename the router so `routed_group_count` cannot find it.
fn unrouter(mut p: PalwShapeProfileV3) -> PalwShapeProfileV3 {
    for table in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
        for n in table.iter_mut() {
            if n.weight_name.contains("router") {
                n.weight_name = n.weight_name.replace("router", "selector");
            }
        }
    }
    p
}

#[test]
fn b4r_01_the_hybrid_row_every_lever_priced_and_gated() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let base = hybrid_512();
    let job = hybrid_job();
    let ccu_real = ccu(&base, &job);
    let root = Hash64::from_u64_word(0x1111_2222_3333_4444);

    println!("\n==== B4R.1  Qwen3.6 hybrid @512: what moves the PRICE, and what the GATE says ====");
    println!("  honest ccu/draw {ccu_real}   W0 {w}   headroom to W0 = {:.4}x", w as f64 / ccu_real as f64);
    show("honest", &econ(ccu_real, ccu_real, w, escrow));

    let mut cases: Vec<(String, PalwShapeProfileV3)> = Vec::new();
    for heads in [64u16, 256, 1623, 4096, 16384, 65535] {
        let mut v = base.clone();
        v.gdn_heads = heads;
        cases.push((format!("gdn_heads = {heads} (honest 32)"), v));
    }
    for code in [0u8, 1, 25, 26, 30] {
        cases.push((format!("every weight_dtypes byte = {code}"), with_dtype(base.clone(), code)));
    }
    cases.push(("weight_name '.routed' -> '.dense_w'".to_string(), unroute(base.clone())));
    cases.push(("weight_name 'router' -> 'selector'".to_string(), unrouter(base.clone())));
    cases.push(("both renames".to_string(), unrouter(unroute(base.clone()))));
    {
        let mut v = unrouter(unroute(base.clone()));
        v.gdn_heads = 1623;
        v = with_dtype(v, 26);
        cases.push(("renames + gdn_heads 1623 + I32".to_string(), v));
    }

    for (tag, prof) in cases {
        let c = match palw_attempt_economic_compute_v1(&prof, &job, true, &PALW_ECONOMIC_COST_TABLE_V1) {
            Ok(c) => c,
            Err(e) => {
                println!("  {tag:<46} UNPRICEABLE {e:?}");
                continue;
            }
        };
        let e = econ(c, ccu_real, w, escrow);
        let gain = (ccu_real as f64) / (e.real_work_per_claim as f64) * (w as f64 / ccu_real as f64);
        println!(
            "  {tag:<46} ccu x{:>10.4}  draws {:>8.4}  reward {:>9.4} MSK  real/claim {:>20}  GAIN vs honest {:>8.4}x  {}",
            c as f64 / ccu_real as f64,
            e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
            e.reward_sompi as f64 / 1e8,
            e.real_work_per_claim,
            gain,
            verdict(&admit(&p, &b, &prof, &job, root))
        );
    }
}

#[test]
fn b4r_02_the_floor_row_is_the_big_prize() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let base = floor_profile();
    let job = floor_job();
    let ccu_real = ccu(&base, &job);
    let root = Hash64::from_u64_word(0x5555_6666_7777_8888);

    println!("\n==== B4R.2  BASE-0 floor: honest ccu is {:.1}x BELOW W0 ====", w as f64 / ccu_real as f64);
    println!(
        "  floor geometry: layers {} interval {} hidden {} heads {} head_dim {} gdn_heads {} gdn_k {} gdn_v {} n_ctx {}",
        base.layer_count,
        base.full_attention_interval,
        base.hidden_dim,
        base.attn_heads,
        base.attn_head_dim,
        base.gdn_heads,
        base.gdn_head_k_dim,
        base.gdn_head_v_dim,
        base.n_ctx
    );
    println!("  gdn table nodes {}  attn table nodes {}  pre {} post {}", base.gdn_nodes.len(), base.attn_nodes.len(), base.pre_nodes.len(), base.post_nodes.len());
    let shape = palw_economic_shape_v1(&base, &PALW_ECONOMIC_COST_TABLE_V1).expect("shape");
    println!("  per-position body at kv_len 1: {:?}", shape.body_at(1));
    show("honest", &econ(ccu_real, ccu_real, w, escrow));

    let mut cases: Vec<(String, PalwShapeProfileV3)> = Vec::new();
    for heads in [1u16, 64, 1024, 16384, 65535] {
        let mut v = base.clone();
        v.gdn_heads = heads;
        cases.push((format!("gdn_heads = {heads}"), v));
    }
    for (k, vd) in [(4096u32, 4096u32), (65535, 65535)] {
        let mut v = base.clone();
        v.gdn_heads = v.gdn_heads.max(1);
        v.gdn_head_k_dim = k;
        v.gdn_head_v_dim = vd;
        cases.push((format!("gdn k_dim = v_dim = {k} (heads {})", v.gdn_heads), v));
    }
    for code in [0u8, 26, 30] {
        cases.push((format!("every weight_dtypes byte = {code}"), with_dtype(base.clone(), code)));
    }
    for hd in [128u32, 1024, 16384] {
        let mut v = base.clone();
        v.attn_head_dim = hd;
        cases.push((format!("attn_head_dim = {hd} (honest {})", base.attn_head_dim), v));
    }
    for h in [8u16, 1024, 65535] {
        let mut v = base.clone();
        v.attn_heads = h;
        cases.push((format!("attn_heads = {h} (honest {})", base.attn_heads), v));
    }
    for kvh in [8u16, 1024] {
        let mut v = base.clone();
        v.attn_kv_heads = kvh;
        cases.push((format!("attn_kv_heads = {kvh}"), v));
    }

    for (tag, prof) in cases {
        let c = match palw_attempt_economic_compute_v1(&prof, &job, true, &PALW_ECONOMIC_COST_TABLE_V1) {
            Ok(c) => c,
            Err(e) => {
                println!("  {tag:<44} UNPRICEABLE {e:?}");
                continue;
            }
        };
        let e = econ(c, ccu_real, w, escrow);
        let gain = (ccu_real as f64) / (e.real_work_per_claim as f64) * (w as f64 / ccu_real as f64);
        println!(
            "  {tag:<44} ccu x{:>12.4}  draws {:>12.2}  reward {:>9.4} MSK  real/claim {:>18}  GAIN {:>10.2}x  {}",
            c as f64 / ccu_real as f64,
            e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
            e.reward_sompi as f64 / 1e8,
            e.real_work_per_claim,
            gain,
            verdict(&admit(&p, &b, &prof, &job, root))
        );
    }
}

#[test]
fn b4r_03_the_artifact_root_is_never_read_by_the_gate() {
    let p = t12();
    let b = bundle_of(&p);
    // The SAME artifact root, carried by two classes whose declared price differs by a factor the
    // gate never compares against anything the root commits to.
    let one_root = Hash64::from_u64_word(0xDEAD_BEEF_CAFE_F00D);
    let honest = hybrid_512();
    let mut fat = hybrid_512();
    fat.gdn_heads = 65535;
    let job = hybrid_job();
    let a = ccu(&honest, &job);
    let f = ccu(&fat, &job);
    println!("\n==== B4R.3  one artifact root, two prices ====");
    println!("  class A id {}  ccu {a}", honest.shape_profile_id());
    println!("  class B id {}  ccu {f}   (x{:.4})", fat.shape_profile_id(), f as f64 / a as f64);
    println!("  A admit: {}", verdict(&admit(&p, &b, &honest, &job, one_root)));
    println!("  B admit: {}", verdict(&admit(&p, &b, &fat, &job, one_root)));
    println!("  distinct class ids: {}", honest.shape_profile_id() != fat.shape_profile_id());
    // And the gate's entry copies the root through unexamined.
    let src = include_str!("../src/palw_class_admission_v2.rs");
    let reads: Vec<&str> = src.lines().filter(|l| l.contains("artifact_root")).collect();
    println!("  every line of palw_class_admission_v2.rs mentioning artifact_root:");
    for l in reads {
        println!("     {}", l.trim());
    }
}

#[test]
fn b4r_04_declared_prefill_is_the_registrants_and_it_is_the_whole_price() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let prof = dense_2m();
    println!("\n==== B4R.4  construction 2: who chooses `from` and `to` in body_over? ====");
    println!("  the canonical job is the REGISTRANT's argument (verify_class_admission_v9's `canonical`).");
    println!("  the only bounds are footprint <= n_ctx and floor_footprint >= canonical_footprint_floor.");
    let shape = palw_admission_shape_at_v1(&p, &b, &prof, 0).expect("shape");
    println!("  t12 canonical_footprint_floor for the dense @2M row: {:?}", shape.ladder.map(|l| l.canonical_footprint_floor));
    for (pf, d) in [(1u32, 2u32), (64, 2), (512, 2), (262_143, 2), (2_097_151, 2)] {
        let job = rc_job_context(&prof, pf, d);
        match palw_attempt_economic_compute_v1(&prof, &job, true, &PALW_ECONOMIC_COST_TABLE_V1) {
            Ok(c) => {
                let e = econ(c, c, w, escrow);
                println!(
                    "  prefill {pf:>9} decode {d}: ccu {c:>20}  ccu/W0 {:>12.6}  draws {:>10.2}  reward {:>9.4} MSK   {}",
                    c as f64 / w as f64,
                    e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
                    e.reward_sompi as f64 / 1e8,
                    verdict(&admit(&p, &b, &prof, &job, Hash64::from_u64_word(7)))
                );
            }
            Err(e) => println!("  prefill {pf:>9} decode {d}: UNPRICEABLE {e:?}"),
        }
    }
}

#[test]
fn b4r_05_the_minimal_floor_variant_that_saturates_the_work_target() {
    use kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1;
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let base = floor_profile();
    let job = floor_job();
    let ccu_real = ccu(&base, &job);
    let root = Hash64::from_u64_word(0xB4B4_B4B4_B4B4_B4B4);

    println!("\n==== B4R.5  the floor variant that reaches W0 ====");
    println!("  floor has a fused attention site: {}", palw_profile_has_fused_attention_v1(&base));
    println!("  honest ccu {ccu_real}, W0 {w}, needed multiplier {:.1}x", w as f64 / ccu_real as f64);
    println!("  honest leaves (canonical_step_leaf_count) and node widths are UNCHANGED by every knob below.");

    // Sweep attn_heads alone, then attn_heads x attn_head_dim, then + I32 dtype.
    let mut best: Option<(String, PalwShapeProfileV3, u128)> = None;
    for heads in [4u16, 64, 256, 1024, 4096, 16384, 65535] {
        for hd in [64u32, 256, 1024, 4096, 16384, 65535] {
            for (dt, code) in [("I8", 24u8), ("I32", 26u8)] {
                let mut v = base.clone();
                v.attn_heads = heads;
                v.attn_head_dim = hd;
                let v = with_dtype(v, code);
                let Ok(c) = palw_attempt_economic_compute_v1(&v, &job, true, &PALW_ECONOMIC_COST_TABLE_V1) else { continue };
                if c < w {
                    continue;
                }
                let a = admit(&p, &b, &v, &job, root);
                if a.is_ok() {
                    let cost = (heads as u128) * (hd as u128) * if code == 26 { 4 } else { 1 };
                    let tag = format!("attn_heads={heads} attn_head_dim={hd} dtype={dt}");
                    println!("  reaches W0 and ADMITS: {tag:<52} ccu {c:>22} (x{:>10.1})  {}", c as f64 / ccu_real as f64, verdict(&a));
                    if best.as_ref().is_none_or(|(_, _, bc)| cost < *bc) {
                        best = Some((tag, v, cost));
                    }
                }
            }
        }
    }

    let Some((tag, prof, _)) = best else {
        println!("  NONE of the swept variants both reached W0 and admitted");
        return;
    };
    let c = ccu(&prof, &job);
    let e = econ(c, ccu_real, w, escrow);
    println!("\n  ---- the chosen construction: {tag} ----");
    println!("  class id            {}", prof.shape_profile_id());
    println!("  declared ccu/draw   {c} MAC-eq   (honest {ccu_real})");
    println!("  ticket target       {} = {:.9} of MAX", e.target, e.target as f64 / u128::MAX as f64);
    println!("  expected draws/win  {:.6} (honest {:.4})", e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64, 
             palw_expected_attempts_q32_v1(palw_work_ticket_target_v1(ccu_real, w)) as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64);
    println!("  priced reward       {} sompi = {:.4} MSK", e.reward_sompi, e.reward_sompi as f64 / 1e8);
    println!("  REAL MAC-eq a claim {} (honest {})", e.real_work_per_claim, palw_attempted_compute_q32_per_claim_v1(palw_expected_attempts_q32_v1(palw_work_ticket_target_v1(ccu_real, w)), ccu_real));
    println!("  MSK per G real MAC-eq  cheat {:.6}   honest {:.6}",
        (e.reward_sompi as f64 / 1e8) / (e.real_work_per_claim as f64 / 1e9),
        (escrow as f64 / 1e8) / (palw_attempted_compute_q32_per_claim_v1(palw_expected_attempts_q32_v1(palw_work_ticket_target_v1(ccu_real, w)), ccu_real) as f64 / 1e9));
    println!("  GAIN                {:.2}x", (w as f64) / (e.real_work_per_claim as f64) * (ccu_real as f64 / ccu_real as f64));
    // Nothing else about the class moved:
    let honest_leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&base, &job, 1u64 << 40).unwrap();
    let cheat_leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&prof, &job, 1u64 << 40).unwrap();
    println!("  declared leaves: honest {honest_leaves}  cheat {cheat_leaves}  (equal: {})", honest_leaves == cheat_leaves);
    println!("  node tables byte-identical to the floor's: pre {} gdn {} attn {} post {}",
        prof.pre_nodes == base.pre_nodes, prof.gdn_nodes == base.gdn_nodes,
        prof.attn_nodes.iter().zip(base.attn_nodes.iter()).all(|(a, b)| a.out_len == b.out_len && a.op_kind == b.op_kind && a.input_refs == b.input_refs && a.tile_len == b.tile_len && a.kernel_semantics_id == b.kernel_semantics_id),
        prof.post_nodes.iter().zip(base.post_nodes.iter()).all(|(a, b)| a.out_len == b.out_len && a.op_kind == b.op_kind));
}

#[test]
fn b4r_06_where_the_floor_variant_is_finally_refused() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let base = floor_profile();
    let job = floor_job();
    let ccu_real = ccu(&base, &job);
    let root = Hash64::from_u64_word(0xB4B4);
    println!("\n==== B4R.6  scaling the floor's attention geometry until something refuses ====");
    println!("  needed to reach W0: x{:.1}", w as f64 / ccu_real as f64);
    for (heads, hd, code) in [
        (65535u16, 64u32, 24u8),
        (65535, 64, 26),
        (65535, 128, 24),
        (65535, 256, 24),
        (65535, 1024, 24),
        (65535, 4096, 24),
        (65535, 65535, 24),
        (4096, 4096, 24),
        (1024, 1024, 24),
        (256, 256, 24),
        (256, 1024, 24),
        (1024, 256, 24),
    ] {
        let mut v = base.clone();
        v.attn_heads = heads;
        v.attn_head_dim = hd;
        let v = with_dtype(v, code);
        match palw_attempt_economic_compute_v1(&v, &job, true, &PALW_ECONOMIC_COST_TABLE_V1) {
            Ok(c) => {
                let e = econ(c, ccu_real, w, escrow);
                println!(
                    "  heads {heads:>6} head_dim {hd:>6} dtype {code:>3}: ccu x{:>12.1} (>=W0 {})  draws {:>12.4}  real/claim {:>18}  GAIN {:>9.2}x  {}",
                    c as f64 / ccu_real as f64,
                    c >= w,
                    e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
                    e.real_work_per_claim,
                    (w as f64) / (e.real_work_per_claim as f64),
                    verdict(&admit(&p, &b, &v, &job, root))
                );
            }
            Err(e) => println!("  heads {heads:>6} head_dim {hd:>6} dtype {code:>3}: UNPRICEABLE {e:?}"),
        }
    }
}

#[test]
fn b4r_07_the_maximal_admitted_floor_forgery() {
    use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let w = palw_work_floor_v1(escrow, T12_RATE_SOMPI_PER_GIGA);
    let base = floor_profile();
    let job = floor_job();
    let ccu_real = ccu(&base, &job);
    let root = Hash64::from_u64_word(0xB4B4);

    println!("\n==== B4R.7  the maximal ADMITTED floor forgery ====");
    // Binary search the largest admitted attn_head_dim at attn_heads = u16::MAX, dtype I32.
    let build = |hd: u32, code: u8| {
        let mut v = base.clone();
        v.attn_heads = u16::MAX;
        v.attn_head_dim = hd;
        with_dtype(v, code)
    };
    for code in [24u8, 26u8] {
        let (mut lo, mut hi) = (64u32, 65_535u32);
        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            if admit(&p, &b, &build(mid, code), &job, root).is_ok() {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let prof = build(lo, code);
        let c = ccu(&prof, &job);
        let e = econ(c, ccu_real, w, escrow);
        let desc = PalwCanonicalClassDescriptorV1::of(&prof, Hash64::default()).expect("descriptor");
        let attempt_job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job.clone(), true);
        let scalar = palw_canonical_draw_work_v1(&desc, &attempt_job, true).expect("draw work").provisional_scalar_v1();
        println!("\n  --- dtype code {code}: largest admitted attn_head_dim = {lo} (attn_heads = 65535) ---");
        println!("  class id             {}", prof.shape_profile_id());
        println!("  declared ccu/draw    {c} MAC-eq  (honest {ccu_real}, x{:.2})", c as f64 / ccu_real as f64);
        println!("  provisional_scalar   {scalar}  (same number: {})", scalar == c);
        println!("  declared leaves      {} (honest {})",
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&prof, &job, 1u64 << 40).unwrap(),
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&base, &job, 1u64 << 40).unwrap());
        println!("  ticket target        {} ({:.9} of MAX)", e.target, e.target as f64 / u128::MAX as f64);
        println!("  expected draws/win   {:.6}   (honest {:.2})",
            e.draws_q32 as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64,
            palw_expected_attempts_q32_v1(palw_work_ticket_target_v1(ccu_real, w)) as f64 / PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 as f64);
        println!("  priced reward        {} sompi = {:.4} MSK", e.reward_sompi, e.reward_sompi as f64 / 1e8);
        println!("  REAL MAC-eq a claim  {}  (honest {})", e.real_work_per_claim, palw_attempted_compute_q32_per_claim_v1(
            palw_expected_attempts_q32_v1(palw_work_ticket_target_v1(ccu_real, w)), ccu_real));
        println!("  MSK per G real MAC-eq: cheat {:.4}, honest {:.4}  => GAIN {:.2}x",
            (e.reward_sompi as f64 / 1e8) / (e.real_work_per_claim as f64 / 1e9),
            (escrow as f64 / 1e8) / 355.649611199,
            (w as f64) / (e.real_work_per_claim as f64));
        // the node the price is charged on, unchanged:
        for (i, n) in prof.attn_nodes.iter().enumerate() {
            if n.input_refs.iter().any(|r| *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_K || *r == kaspa_consensus_core::palw_step::PALW_STEP_INPUT_KV_V) {
                println!("  over-cache node attn[{i}] op {:?} out {:?} (identical to the honest floor's: {})",
                    n.op_kind, n.out_len, n.out_len == base.attn_nodes[i].out_len);
            }
        }
        // the one refusal just past it:
        if lo < 65_535 {
            println!("  attn_head_dim {} => {}", lo + 1, verdict(&admit(&p, &b, &build(lo + 1, code), &job, root)));
        }
    }
}
