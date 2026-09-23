//! AUDIT LANE B4 — the attacker model: an expensive-tier registration with cheap execution.
//! Read-only probe. Every number printed here is produced by the runtime in this worktree.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9,
};
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

pub fn t12() -> Params {
    palw_t12_shipped_params()
}

pub fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

pub fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor")
}
pub fn dense_2m() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense")
}
pub fn hybrid_512() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid")
}

/// The price (CCU per draw, MAC-eq) the chain puts on one attempt of this class.
pub fn price_of(profile: &PalwShapeProfileV3, canonical: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, canonical, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("priced")
}

/// Run the acceptance path's gate at DAA `daa`, for a registration carrying `share`.
pub fn admit(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    share_permille: u16,
    daa: u64,
) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(params, bundle, profile, daa).expect("shape");
    // pwu_rule must be the COUNTED leaves; the gate refuses anything else, so derive it the way an
    // honest registrant would and let every other check speak.
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(bundle.court.max_step_leaf_count(), profile),
    };
    let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, canonical, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(0xDEADBEEF),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v9(
        bundle,
        profile,
        canonical,
        &reg,
        &certified,
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        params.palw_canonical_work_at(daa),
        shape.held,
        shape.kimi_family,
        params.palw_audit_2026_09_23_active_at(daa),
    )
    .map(|e| e.canonical_step_leaf_count)
}

#[test]
fn b4_00_baseline_the_three_shipped_rows() {
    let p = t12();
    let b = bundle_of(&p);
    let shape = palw_admission_shape_at_v1(&p, &b, &hybrid_512(), 0).expect("shape");
    println!("\n== t12 admission shape at DAA 0 ==");
    println!("  court            {:?}", shape.court.map(|c| (c.dissection_arity, c.window_court_daa)));
    println!("  ladder           {:?}", shape.ladder.map(|l| l.ladder));
    println!("  token_lift       {}", shape.token_lift);
    println!("  fused_dissect    {}", shape.fused_dissectable);
    println!("  held             {:?}", shape.held);
    println!("  kimi             {}", shape.kimi_family);
    println!("  canonical_work   {}", p.palw_canonical_work_at(0));
    println!("  admission_indep  {}", p.palw_admission_independence_at(0));
    println!("  court ceilings: close_bytes {} chunks {} terminal_macs {} operands {}",
        b.court.max_close_bytes(), b.court.max_close_chunks(), b.court.max_terminal_macs(), b.court.max_operand_count());
    println!("  max_step_leaf_count {}", b.court.max_step_leaf_count());

    for (name, prof, can) in [
        ("floor  ", floor_profile(), rc_job_context(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)),
        ("hybrid ", hybrid_512(), {
            let (pf, d) = qwen36_held_canonical_v1(512);
            rc_job_context(&hybrid_512(), pf, d)
        }),
        ("dense2M", dense_2m(), {
            let (pf, d) = qwen25_a16_held_canonical_v1(2_097_152);
            rc_job_context(&dense_2m(), pf, d)
        }),
    ] {
        let ccu = price_of(&prof, &can);
        let verdict = admit(&p, &b, &prof, &can, 0, 0);
        println!(
            "  {name}: class {} n_ctx {:>9} layers {:>3} heads {:>3} head_dim {:>6} hidden {:>5} ccu/draw {:>22} admit {:?}",
            &prof.shape_profile_id().to_string()[..12],
            prof.n_ctx, prof.layer_count, prof.attn_heads, prof.attn_head_dim, prof.hidden_dim, ccu,
            verdict.map(|c| format!("OK leaves={c}")).map_err(|e| format!("{e:?}"))
        );
    }
}

// -------------------------------------------------------------------------------------------
// Construction 4 / 1: geometry fields that PRICE but are not read off the node tables.
// -------------------------------------------------------------------------------------------

fn report(tag: &str, p: &Params, b: &PalwConsensusParamsV2, prof: &PalwShapeProfileV3, can: &PalwJobContextV2, base_ccu: u128) {
    let ccu = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| price_of(prof, can))).unwrap_or(0);
    let verdict = admit(p, b, prof, can, 0, 0);
    println!(
        "  {tag:<46} ccu {:>24}  x{:>10.4}  admit {}",
        ccu,
        if base_ccu > 0 { ccu as f64 / base_ccu as f64 } else { 0.0 },
        match &verdict {
            Ok(c) => format!("OK leaves={c}"),
            Err(e) => format!("REFUSED {e:?}"),
        }
    );
}

#[test]
fn b4_01_attn_head_dim_is_a_free_price_multiplier() {
    let p = t12();
    let b = bundle_of(&p);
    let (pf, d) = qwen25_a16_held_canonical_v1(2_097_152);
    let base = dense_2m();
    let can = rc_job_context(&base, pf, d);
    let base_ccu = price_of(&base, &can);
    println!("\n== dense@2M: only attn_head_dim moves (node tables untouched) ==");
    println!("  baseline head_dim {} ccu {}", base.attn_head_dim, base_ccu);
    for mul in [2u32, 4, 8, 16, 64, 256, 1024] {
        let mut m = base.clone();
        m.attn_head_dim = base.attn_head_dim * mul;
        let can2 = rc_job_context(&m, pf, d);
        report(&format!("attn_head_dim x{mul} = {}", m.attn_head_dim), &p, &b, &m, &can2, base_ccu);
    }
    println!("\n== dense@2M: only attn_heads moves ==");
    for mul in [2u16, 4, 16, 64, 256, 1024, 5000] {
        let mut m = base.clone();
        m.attn_heads = base.attn_heads * mul;
        let can2 = rc_job_context(&m, pf, d);
        report(&format!("attn_heads x{mul} = {}", m.attn_heads), &p, &b, &m, &can2, base_ccu);
    }
}

#[test]
fn b4_02_weight_dtype_is_a_free_price_multiplier() {
    let p = t12();
    let b = bundle_of(&p);
    let (pf, d) = qwen25_a16_held_canonical_v1(2_097_152);
    let base = dense_2m();
    let can = rc_job_context(&base, pf, d);
    let base_ccu = price_of(&base, &can);
    println!("\n== dense@2M: every declared weight dtype rewritten, node tables otherwise untouched ==");
    println!("  baseline ccu {base_ccu}");
    for code in [1u8, 25, 26, 30, 24] {
        let mut m = base.clone();
        for t in [&mut m.pre_nodes, &mut m.gdn_nodes, &mut m.attn_nodes, &mut m.post_nodes] {
            for n in t.iter_mut() {
                if !n.weight_dtypes.is_empty() {
                    for x in n.weight_dtypes.iter_mut() {
                        *x = code;
                    }
                }
            }
        }
        let can2 = rc_job_context(&m, pf, d);
        report(&format!("every weight_dtype = {code}"), &p, &b, &m, &can2, base_ccu);
    }
}

#[test]
fn b4_03_gdn_state_geometry_is_a_free_price_multiplier() {
    let p = t12();
    let b = bundle_of(&p);
    let base = hybrid_512();
    let (pf, d) = qwen36_held_canonical_v1(512);
    let can = rc_job_context(&base, pf, d);
    let base_ccu = price_of(&base, &can);
    println!("\n== hybrid@512: gdn_heads / k_dim / v_dim only ==");
    println!("  baseline gdn {}x{}x{} ccu {}", base.gdn_heads, base.gdn_head_k_dim, base.gdn_head_v_dim, base_ccu);
    for mul in [2u32, 8, 64, 512, 4096] {
        let mut m = base.clone();
        m.gdn_head_k_dim = base.gdn_head_k_dim * mul;
        m.gdn_head_v_dim = base.gdn_head_v_dim * mul;
        let can2 = rc_job_context(&m, pf, d);
        report(&format!("gdn k_dim,v_dim x{mul}"), &p, &b, &m, &can2, base_ccu);
    }
    for mul in [2u16, 8, 64, 512, 2000] {
        let mut m = base.clone();
        m.gdn_heads = base.gdn_heads.saturating_mul(mul);
        let can2 = rc_job_context(&m, pf, d);
        report(&format!("gdn_heads x{mul} = {}", m.gdn_heads), &p, &b, &m, &can2, base_ccu);
    }
}

#[test]
fn b4_04_print_the_hybrid_node_tables() {
    let h = hybrid_512();
    for (name, t) in [("pre", &h.pre_nodes), ("gdn", &h.gdn_nodes), ("attn", &h.attn_nodes), ("post", &h.post_nodes)] {
        println!("\n-- {name} ({} nodes)", t.len());
        for (i, n) in t.iter().enumerate() {
            println!(
                "   [{i:>2}] {:?} out={:?} tile={} w='{}' dtypes={:?} refs={:?}",
                n.op_kind, n.out_len, n.tile_len, n.weight_name,
                n.weight_dtypes.first(), n.input_refs
            );
        }
    }
}

// -------------------------------------------------------------------------------------------
// THE CONSTRUCTION: one u16 field. Everything admission checks is byte-identical.
// -------------------------------------------------------------------------------------------

fn court_cost_of(p: &Params, b: &PalwConsensusParamsV2, prof: &PalwShapeProfileV3)
    -> Option<kaspa_consensus_core::palw_class_admission_v2::PalwCourtCostV1> {
    let shape = palw_admission_shape_at_v1(p, b, prof, 0).ok()?;
    let sh = shape.ladder?.cost_shape;
    kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_shaped_v1(prof, sh).ok()
}

#[test]
fn b4_05_gdn_heads_moves_the_price_and_nothing_admission_checks() {
    let p = t12();
    let b = bundle_of(&p);
    let base = hybrid_512();
    let (pf, d) = qwen36_held_canonical_v1(512);
    let can = rc_job_context(&base, pf, d);
    let base_ccu = price_of(&base, &can);
    let base_cost = court_cost_of(&p, &b, &base).expect("cost");
    let base_leaves = admit(&p, &b, &base, &can, 0, 0).expect("baseline admits");
    let base_worst = kaspa_consensus_core::palw_step::worst_case_step_leaf_count_deepest_job_capped_v1(&base, 1 << 40).unwrap();

    println!("\n==== B4: gdn_heads is a price multiplier admission does not bound ====");
    println!("baseline gdn_heads={} ccu={} leaves={} worst={} close_bytes={} terminal_macs={} operands={}",
        base.gdn_heads, base_ccu, base_leaves, base_worst,
        base_cost.max_close_bytes, base_cost.max_terminal_macs, base_cost.max_operand_count);

    for heads in [64u16, 256, 1623, 4096, 16384, 65535] {
        let mut m = base.clone();
        m.gdn_heads = heads;
        let can2 = rc_job_context(&m, pf, d);
        let ccu = price_of(&m, &can2);
        let verdict = admit(&p, &b, &m, &can2, 0, 0);
        let cost = court_cost_of(&p, &b, &m);
        let worst = kaspa_consensus_core::palw_step::worst_case_step_leaf_count_deepest_job_capped_v1(&m, 1 << 40).unwrap();
        println!(
            "  gdn_heads={heads:<6} ccu={ccu:<16} x{:>8.3}  leaves={:<12} worst={worst:<12} close={:<10} term_macs={:<10} ops={} => {}",
            ccu as f64 / base_ccu as f64,
            verdict.as_ref().map(|c| c.to_string()).unwrap_or_else(|e| format!("{e:?}")),
            cost.map(|c| c.max_close_bytes).unwrap_or(0),
            cost.map(|c| c.max_terminal_macs).unwrap_or(0),
            cost.map(|c| c.max_operand_count).unwrap_or(0),
            if verdict.is_ok() { "ADMITTED" } else { "refused" }
        );
    }

    // And the court's own slice of the GDN leaf, with the REAL ref widths of node gdn[15].
    println!("\n  -- qwen36_gdn_slice_v1 at the real gdn[15] ref widths [2048, 8192, 2048, 32, 32] --");
    let node = base.gdn_nodes[15].clone();
    for heads in [32u16, 33, 64, 1623, 65535] {
        let mut m = base.clone();
        m.gdn_heads = heads;
        let slice = kaspa_consensus_core::palw_step_refute::qwen36_gdn_slice_v1(&m, &node, &[2048, 8192, 2048, 32, 32], 0);
        println!("     gdn_heads={heads:<6} slice={}", if slice.is_some() { "Some(..) -> triable" } else { "None -> the court cannot open this leaf" });
    }
}

#[test]
fn b4_06_what_the_inflated_class_earns() {
    use kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1;
    use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
    use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};
    use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};

    let p = t12();
    let carve = p.palw_overlay_carve.expect("t12 arms the carve").worker_carve_permille as u64;
    // The block subsidy t12 actually pays (recon-measured through CoinbaseManager::calc_block_subsidy).
    const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
    let escrow = T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve;
    let rate = p.palw_economic_payout.expect("armed").rate_sompi_per_giga;
    let w0 = palw_work_floor_v1(escrow, rate);
    println!("\n==== B4: what the class is paid ====");
    println!("  carve {carve} permille, escrow {escrow} sompi ({:.2} MSK), rate {rate} sompi/G, W0 = {w0} CCU",
        escrow as f64 / 1e8);

    let base = hybrid_512();
    let (pf, d) = qwen36_held_canonical_v1(512);

    // floor basis: declared 7,708 leaves over 21,657,728 MAC-eq derived.
    let floor = floor_profile();
    let fcan = rc_job_context(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let base_declared = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&floor, &fcan, 1 << 40).unwrap();
    let base_canonical = price_of(&floor, &fcan) as u64;
    println!("  floor basis: declared {base_declared} leaves / canonical {base_canonical} MAC-eq (ratio {:.4})",
        base_canonical as f64 / base_declared as f64);

    let row = |tag: &str, heads: u16| {
        let mut m = base.clone();
        m.gdn_heads = heads;
        let can = rc_job_context(&m, pf, d);
        let ccu = price_of(&m, &can);
        let target = palw_work_ticket_target_v1(ccu, w0);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, ccu.min(u64::MAX as u128) as u64);
        // W sits at its floor while the epoch step has nothing above it.
        let reward = palw_work_priced_reward_v1(escrow, ccu.min(u64::MAX as u128) as u64, w0.min(u64::MAX as u128) as u64);
        let exposure = (ccu * base_declared as u128 / base_canonical as u128).min(u64::MAX as u128) as u64;
        let reserved_sompi = exposure as u128 * 5;
        let quanta = palw_execution_quantum_count_v1(ccu, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        // Real compute one WIN costs the producer: attempts x the cost of the graph it truly runs,
        // which is the UNMODIFIED hybrid's draw (the node tables never moved).
        let real_per_draw = price_of(&base, &rc_job_context(&base, pf, d));
        let real_per_win = real_per_draw * attempts.max(1) as u128;
        println!("\n  {tag}");
        println!("    ccu/draw            {ccu}");
        println!("    ticket target       {target}  ({:.6} of MAX)", target as f64 / u128::MAX as f64);
        println!("    expected draws/win  {attempts}");
        println!("    claim.pwu           {pwu}");
        println!("    priced reward       {reward} sompi = {:.2} MSK", reward as f64 / 1e8);
        println!("    reserved            {reserved_sompi} sompi = {:.2} MSK", reserved_sompi as f64 / 1e8);
        println!("    exec quanta a Final {quanta}");
        println!("    REAL MAC-eq a win   {real_per_win}");
        println!("    reward per real G MAC-eq  {:.4} MSK", (reward as f64 / 1e8) / (real_per_win as f64 / 1e9));
        println!("    fork weight per real MAC-eq {:.4}", pwu as f64 / real_per_win as f64);
        (reward as f64 / 1e8) / (real_per_win as f64 / 1e9)
    };

    let honest = row("HONEST  Qwen3.6 hybrid @512 (gdn_heads = 32)", 32);
    let cheat1 = row("CHEAT   gdn_heads = 1,623 (the smallest that saturates the ticket)", 1623);
    let cheat2 = row("CHEAT   gdn_heads = 65,535 (u16::MAX)", 65535);
    println!("\n  reward per real compute: cheat/honest = {:.3}x (at 1,623) and {:.3}x (at 65,535)", cheat1 / honest, cheat2 / honest);
}

/// `admit` with the registration's three registrant-written economic fields under test control.
#[allow(clippy::too_many_arguments)]
pub fn admit_raw(
    params: &Params,
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    class_id: Hash64,
    artifact_root: Hash64,
    declared_pwu: u64,
    share_permille: u16,
) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(params, bundle, profile, 0).expect("shape");
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: declared_pwu },
        initial_target: u128::MAX,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v9(
        bundle, profile, canonical, &reg, &certified, &[], shape.ladder, shape.court, false,
        shape.token_lift, shape.fused_dissectable, params.palw_canonical_work_at(0), shape.held, shape.kimi_family,
        params.palw_audit_2026_09_23_active_at(0),
    )
    .map(|e| e.canonical_step_leaf_count)
}

#[test]
fn b4_07_the_other_six_constructions() {
    let p = t12();
    let b = bundle_of(&p);
    let hy = hybrid_512();
    let (pf, d) = qwen36_held_canonical_v1(512);
    let can = rc_job_context(&hy, pf, d);
    let honest_leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&hy, &can, 1 << 40).unwrap();
    let base_ccu = price_of(&hy, &can);

    println!("\n==== C1: big declared leaves, trivial table ====");
    for declared in [honest_leaves, honest_leaves * 1000, u64::MAX / 2] {
        let r = admit_raw(&p, &b, &hy, &can, hy.shape_profile_id(), Hash64::from_u64_word(1), declared, 0);
        println!("   declared pwu_per_inference={declared:<24} => {}",
            r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));
    }

    println!("\n==== C5: a small model claiming a large model's ClassId ====");
    let dense = dense_2m();
    let r = admit_raw(&p, &b, &hy, &can, dense.shape_profile_id(), Hash64::from_u64_word(1), honest_leaves, 0);
    println!("   hybrid profile + dense@2M class_id => {}",
        r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));

    println!("\n==== C6: one artifact root, two reward tiers ====");
    let root = Hash64::from_u64_word(0xA271FAC7);
    let mut twin = hy.clone();
    twin.n_threads = hy.n_threads + 1; // a field no price, leaf or court cost reads
    let can_t = rc_job_context(&twin, pf, d);
    let a = admit_raw(&p, &b, &hy, &can, hy.shape_profile_id(), root, honest_leaves, 0);
    let bb = admit_raw(&p, &b, &twin, &can_t, twin.shape_profile_id(), root,
        kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&twin, &can_t, 1 << 40).unwrap(), 0);
    println!("   class A {} root {root} => {:?}", &hy.shape_profile_id().to_string()[..12], a.is_ok());
    println!("   class B {} root {root} => {:?}  (n_threads {} vs {})",
        &twin.shape_profile_id().to_string()[..12], bb.is_ok(), hy.n_threads, twin.n_threads);
    println!("   distinct class ids: {}", hy.shape_profile_id() != twin.shape_profile_id());
    println!("   both priced at ccu {} / {}", base_ccu, price_of(&twin, &can_t));

    println!("\n==== C3: the MoE router, which is found by a SUBSTRING of a registrant-written name ====");
    let mut unrouted = hy.clone();
    for t in [&mut unrouted.gdn_nodes, &mut unrouted.attn_nodes] {
        for n in t.iter_mut() {
            if n.weight_name.contains("router") {
                n.weight_name = n.weight_name.replace("router", "selector");
            }
        }
    }
    let can_u = rc_job_context(&unrouted, pf, d);
    let ccu_u = price_of(&unrouted, &can_u);
    let leaves_u = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&unrouted, &can_u, 1 << 40).unwrap();
    let r = admit_raw(&p, &b, &unrouted, &can_u, unrouted.shape_profile_id(), root, leaves_u, 0);
    println!("   rename 'router'->'selector' in the softmax node's weight_name");
    println!("   ccu {ccu_u} (x{:.4})  leaves {leaves_u} (honest {honest_leaves})  => {}",
        ccu_u as f64 / base_ccu as f64,
        r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));

    let mut unmarked = hy.clone();
    for t in [&mut unmarked.gdn_nodes, &mut unmarked.attn_nodes] {
        for n in t.iter_mut() {
            if n.weight_name.ends_with(".routed") {
                n.weight_name = n.weight_name.replace(".routed", ".dense_w");
            }
        }
    }
    let can_m = rc_job_context(&unmarked, pf, d);
    let ccu_m = price_of(&unmarked, &can_m);
    let leaves_m = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&unmarked, &can_m, 1 << 40).unwrap();
    let r = admit_raw(&p, &b, &unmarked, &can_m, unmarked.shape_profile_id(), root, leaves_m, 0);
    println!("   rename '.routed'->'.dense_w' on the three expert matmuls");
    println!("   ccu {ccu_m} (x{:.4})  leaves {leaves_m} (honest {honest_leaves})  => {}",
        ccu_m as f64 / base_ccu as f64,
        r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));

    println!("\n==== C4: dtype declared I32 while the artifact stores I8 ====");
    let mut i32d = hy.clone();
    for t in [&mut i32d.pre_nodes, &mut i32d.gdn_nodes, &mut i32d.attn_nodes, &mut i32d.post_nodes] {
        for n in t.iter_mut() {
            for x in n.weight_dtypes.iter_mut() { *x = 26; }
        }
    }
    let can_i = rc_job_context(&i32d, pf, d);
    let ccu_i = price_of(&i32d, &can_i);
    let leaves_i = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&i32d, &can_i, 1 << 40).unwrap();
    let r = admit_raw(&p, &b, &i32d, &can_i, i32d.shape_profile_id(), root, leaves_i, 0);
    println!("   every weight_dtype 24 (I8) -> 26 (I32): ccu {ccu_i} (x{:.4}) leaves {leaves_i} => {}",
        ccu_i as f64 / base_ccu as f64,
        r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));

    println!("\n==== combined: gdn_heads 1623 + unrouted + I32 ====");
    let mut combo = hy.clone();
    combo.gdn_heads = 1623;
    for t in [&mut combo.pre_nodes, &mut combo.gdn_nodes, &mut combo.attn_nodes, &mut combo.post_nodes] {
        for n in t.iter_mut() {
            if n.weight_name.contains("router") { n.weight_name = n.weight_name.replace("router", "selector"); }
            for x in n.weight_dtypes.iter_mut() { *x = 26; }
        }
    }
    let can_c = rc_job_context(&combo, pf, d);
    let ccu_c = price_of(&combo, &can_c);
    let leaves_c = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&combo, &can_c, 1 << 40).unwrap();
    let r = admit_raw(&p, &b, &combo, &can_c, combo.shape_profile_id(), root, leaves_c, 0);
    println!("   ccu {ccu_c} (x{:.4}) leaves {leaves_c} (honest {honest_leaves}) => {}",
        ccu_c as f64 / base_ccu as f64,
        r.map(|c| format!("ADMITTED leaves={c}")).unwrap_or_else(|e| format!("REFUSED {e:?}")));
}

#[test]
fn b4_08_attention_geometry_is_cross_checked_against_the_row_and_the_recurrence_is_not() {
    // `palw_fused_query_slice_is_openable_v1` refuses `attn_heads x attn_head_dim > query row`.
    // There is no counterpart asking `gdn_heads x gdn_head_v_dim == the GatedDeltaNet row`.
    let hy = hybrid_512();
    let q = hy.attn_nodes[10].clone();
    let gdn = hy.gdn_nodes[15].clone();
    println!("\n  fused query row (attn[10]) out={:?}  vs attn_heads x attn_head_dim = {}",
        q.out_len, hy.attn_heads as u64 * hy.attn_head_dim as u64);
    println!("  GatedDeltaNet row (gdn[15]) out={:?}  vs gdn_heads x gdn_head_v_dim  = {}",
        gdn.out_len, hy.gdn_heads as u64 * hy.gdn_head_v_dim as u64);
    for heads in [16u16, 17, 1623] {
        let mut m = hy.clone();
        m.attn_heads = heads;
        println!("   attn_heads={heads:<6} fused_query_slice_is_openable => {:?}",
            kaspa_consensus_core::palw_class_admission_v2::palw_fused_query_slice_is_openable_v1(&m).map(|_| "Ok").map_err(|e| format!("{e:?}")));
    }
    for heads in [32u16, 33, 1623] {
        let mut m = hy.clone();
        m.gdn_heads = heads;
        // the only per-class check of the GDN geometry anywhere in the admission gate:
        println!("   gdn_heads={heads:<6} validate_shape => {:?}, gdn row still {:?}",
            m.validate_shape().map(|_| "Ok").map_err(|e| format!("{e:?}")), m.gdn_nodes[15].out_len);
    }
}
