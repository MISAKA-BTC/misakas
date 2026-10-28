//! AGENT 4 — the counterexample table, in the operator's own columns.
//!
//! Every number is computed by the repo's own consensus functions against the SHIPPED
//! testnet-11 params (`palw_rc_shipped_params`). Nothing is hand-entered except the two
//! published fleet replay timings, which are read from `palw_verification_profile_v1`.
//!
//! Columns: family | job | class/shape | input size | output size | observable compute proxy
//!          | protocol-measured work | protocol reward | estimated real cost | reward/cost

use kaspa_consensus_core::config::params::palw_rc_shipped_params;
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1};

const RATE: u64 = 900_000_000; // sompi per G MAC-eq, ADR-0132 armed at DAA 7,101
const ESCROW: u64 = 320_084_640_000; // 3,200.85 MSK, the 72 % worker carve past the flag day

fn ctx(p: &PalwShapeProfileV3, a: u32, b: u32) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
    kaspa_consensus_core::palw_base0_profile::rc_job_context(p, a, b)
}
fn retile(p: &PalwShapeProfileV3, tile: u32) -> PalwShapeProfileV3 {
    let mut c = p.clone();
    for t in [&mut c.pre_nodes, &mut c.gdn_nodes, &mut c.attn_nodes, &mut c.post_nodes] {
        for n in t.iter_mut() {
            n.tile_len = tile;
        }
    }
    c
}

struct Out {
    family: &'static str,
    name: String,
    job: String,
    input: String,
    output: String,
    /// the observable compute proxy a verifier could recompute: ADR-0131 MAC-equivalents of the job
    /// the network ACTUALLY executes (prefill_draw is armed on testnet-11 at DAA 4,000)
    executed_ccu: u128,
    /// protocol-measured work: `pwu_per_inference`, the step-leaf count of the DECLARED canonical job
    measured_leaves: u64,
    /// fork-choice weight one accepted block buys past the 7,101 flag day
    weight: u64,
    /// what the block is paid past 7,101 (ADR-0132 Upgrade C takes precedence over the leaf price)
    pay_sompi: u64,
    admissible: bool,
}

fn main() {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else { panic!() };
    let ladder = bundle.court.max_step_leaf_count();
    let w0 = kaspa_consensus_core::palw_work_target_v1::palw_work_floor_v1(ESCROW, RATE);

    println!("AGENT 4 — RED-TEAM COUNTEREXAMPLE TABLE");
    println!("worktree: wt-adr0124 ; params: palw_rc_shipped_params() ; ruleset ladder = {ladder} (2^26)");
    println!("fences read off the shipped preset: prefill_draw @4,000 (LIVE) ; work_priced_reward / economic_payout / work_target / model_registry @7,101 (FUTURE)");
    println!("W0 = palw_work_floor_v1(escrow {ESCROW}, rate {RATE}) = {w0} MAC-eq\n");

    let dense = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5(512).unwrap();
    let (cp, cd) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1();
    let floor = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .unwrap();
    let hybrid = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B,
        ),
    )
    .unwrap();
    let unit27b = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            kaspa_consensus_core::palw_qwen36_profile::QWEN38_27B,
        ),
    )
    .unwrap();

    let mut rows: Vec<Out> = Vec::new();
    let mut add = |family: &'static str, name: String, job: String, p: &PalwShapeProfileV3, pre: u32, dec: u32| {
        let Ok(l) = step_leaf_count_capped_v1(p, &ctx(p, pre, dec), ladder) else { return };
        let worst = worst_case_step_leaf_count_capped_v1(p, ladder);
        let admissible = worst.as_ref().map(|w| l <= *w).unwrap_or(false)
            && (pre as u64 + dec.max(1) as u64 - 1) <= p.n_ctx as u64;
        let draw = palw_attempt_economic_compute_v1(p, &ctx(p, pre, dec), true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        let target = kaspa_consensus_core::palw_work_target_v1::palw_work_ticket_target_v1(draw, w0);
        let e = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
        let weight = kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, l);
        let attempted = kaspa_consensus_core::palw_economic_compute_v1::palw_attempted_compute_per_claim_v1(e, draw);
        let pay = ((attempted * RATE as u128) / 1_000_000_000u128).min(ESCROW as u128) as u64;
        rows.push(Out {
            family,
            name,
            job,
            input: format!("{pre} tok"),
            output: "1 tok (prefill draw)".into(),
            executed_ccu: draw,
            measured_leaves: l,
            weight,
            pay_sompi: pay,
            admissible,
        });
    };

    // ---- (a) SAME APPARENT DIFFICULTY (near-equal protocol-measured work), DIFFERENT ACTUAL COST
    add("(a) same measured work, diff cost", "Qwen3.6-35B-A3B (35 B MoE)".into(), "canonical (7,2)".into(), &hybrid, 7, 2);
    add(
        "(a) same measured work, diff cost",
        "Qwen2.5-A16 re-tiled @64".into(),
        "canonical (63,2), tile_len 64 everywhere".into(),
        &retile(&dense, 64),
        63,
        2,
    );

    // ---- (b) SAME ACTUAL COST, DIFFERENT REWARD  (identical executed draw job, decode re-declared)
    for d in [2u32, 128, 370] {
        add(
            "(b) same cost, diff reward",
            format!("Qwen2.5-A16 gv5, canonical (63,{d})"),
            "executed draw is (63,1) in EVERY row".into(),
            &dense,
            63,
            d,
        );
    }

    // ---- (c) SAME TASK, DIFFERENT MODEL  (one anchor-derived inference, every registered class)
    add("(c) same task, diff model", "BASE-0 floor".into(), "canonical (8,4)".into(), &floor, 8, 4);
    add("(c) same task, diff model", "Qwen3.6-35B-A3B".into(), "canonical (7,2)".into(), &hybrid, 7, 2);
    add("(c) same task, diff model", "Qwen2.5-1.5B @512".into(), "canonical (63,2)".into(), &dense, 63, 2);
    add("(c) same task, diff model", "Qwen3.8-27B".into(), "canonical (7,2)".into(), &unit27b, 7, 2);

    // ---- (d) SAME TASK, DIFFERENT PROMPT REPRESENTATION  (the decode lever on ONE graph)
    add("(d) same task, diff representation", "Qwen2.5-A16 gv5 (1,432)".into(), "canonical (1,432)".into(), &dense, 1, 432);
    add("(d) same task, diff representation", "Qwen2.5-A16 gv5 (1,256)".into(), "canonical (1,256)".into(), &dense, 1, 256);
    add("(d) same task, diff representation", "Qwen2.5-A16 gv5 (1,128)".into(), "canonical (1,128)".into(), &dense, 1, 128);

    println!(
        "{:<36} {:<32} {:<10} {:>18} {:>16} {:>18} {:>12} {:>14} {:>12}",
        "family", "class / shape", "input", "EXECUTED MAC-eq", "measured work", "FORK WEIGHT", "pay MSK", "weight/MAC-eq", "admissible"
    );
    println!("{}", "-".repeat(180));
    for r in &rows {
        println!(
            "{:<36} {:<32} {:<10} {:>18} {:>16} {:>18} {:>12.2} {:>14.4} {:>12}",
            r.family,
            r.name,
            r.input,
            r.executed_ccu,
            r.measured_leaves,
            r.weight,
            r.pay_sompi as f64 / 1e8,
            r.weight as f64 / r.executed_ccu as f64,
            if r.admissible { "yes" } else { "NO" }
        );
        let _ = (&r.job, &r.output);
    }

    println!("\nREWARD / COST, normalised to the shipped Qwen2.5-A16 row (index 100):");
    let base = rows
        .iter()
        .find(|r| r.name == "Qwen2.5-1.5B @512")
        .map(|r| r.weight as f64 / r.executed_ccu as f64)
        .unwrap();
    println!("{:<36} {:<32} {:>16} {:>16}", "family", "class / shape", "weight index", "pay index");
    let pay_base = rows.iter().find(|r| r.name == "Qwen2.5-1.5B @512").map(|r| r.pay_sompi as f64).unwrap();
    for r in &rows {
        println!(
            "{:<36} {:<32} {:>16.1} {:>16.1}",
            r.family,
            r.name,
            100.0 * (r.weight as f64 / r.executed_ccu as f64) / base,
            100.0 * r.pay_sompi as f64 / pay_base
        );
    }

    println!("\nNOTE on A2-mlsys finding 2: the per-class replay timings it builds on");
    println!("  (palw_verification_profile_v1.rs:462-469, qwen25()/qwen36()) are inside `#[cfg(test)] mod tests`");
    println!("  (the attribute is at line 454). They are TEST FIXTURES, not consensus constants and not a rooted");
    println!("  fleet record. The only production timing constant is PALW_VERIFICATION_REFERENCE_V1.mac_eq_per_ms = {}.",
        kaspa_consensus_core::palw_verification_profile_v1::PALW_VERIFICATION_REFERENCE_V1.mac_eq_per_ms);
}
