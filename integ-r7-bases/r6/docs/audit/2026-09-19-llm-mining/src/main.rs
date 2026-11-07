//! AGENT 4 — RED-TEAM VERIFIER probe.
//!
//! Read-only. Links kaspa-consensus-core by path and calls ONLY the repo's own functions.
//! Nothing here modifies the repo, and nothing here touches a network.
//!
//! What it answers:
//!   R1  re-derive the four shipped classes (leaves, canonical CCU, draw CCU)
//!   R2  the decode lever: priced leaves vs the job `palw_attempt_job_v1` actually executes
//!   R3  the tile lever: CCU invariance under re-tiling
//!   R4  CLOSE A1's open lead: is `worst_case_step_leaf_count_capped_v1` actually the worst case?
//!   R5  the POST-7,101 configuration (ADR-0132 C + ADR-0137): pay vs FORK-CHOICE WEIGHT
//!   R6  counterexample families (a)-(d) as one table

use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1,
};

const LADDER: u64 = 1 << 26; // PALW_CONTEXT_LADDER 2^26, the RC bundle's ruleset ladder
/// testnet-11 attempt escrow pinned by palw_panel_economy_v1.rs:255 (62 % carve).
const T11_ESCROW_62: u64 = 275_628_448_680;
/// the 72 % carve the 7,101 flag day installs (ADR-0126 revised, params.rs overlay_carve 720‰)
const T11_ESCROW_72: u64 = 320_084_640_000;
/// ADR-0132 rate armed at the flag day: 9 MSK per G MAC-eq.
const RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

fn sep(t: &str) {
    println!("\n================ {t} ================");
}

fn msk(sompi: u64) -> f64 {
    sompi as f64 / 1e8
}

struct Row {
    name: &'static str,
    profile: PalwShapeProfileV3,
    canonical: (u32, u32),
}

fn rows() -> Vec<Row> {
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_A16_GRAPH_V5_N_CTX, qwen25_a16_graph_v5_canonical_v1};
    use kaspa_consensus_core::palw_qwen36_profile::{
        QWEN36_35B_A3B, QWEN36_RC_CANONICAL, QWEN38_27B, qwen36_geometry_artifact_eps, qwen36_profile_v2,
    };
    let mut v = Vec::new();
    v.push(Row { name: "BASE-0 floor", profile: base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap(), canonical: PALW_RC_BASE0_CANONICAL });
    v.push(Row {
        name: "Qwen3.6-35B-A3B",
        profile: qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)).unwrap(),
        canonical: QWEN36_RC_CANONICAL,
    });
    v.push(Row {
        name: "Qwen2.5-A16 gv5@512",
        profile: palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap(),
        canonical: qwen25_a16_graph_v5_canonical_v1(),
    });
    v.push(Row {
        name: "Qwen3.8-27B",
        profile: qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN38_27B)).unwrap(),
        canonical: QWEN36_RC_CANONICAL,
    });
    v
}

fn ctx(profile: &PalwShapeProfileV3, p: u32, d: u32) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
    kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, p, d)
}

fn leaves(profile: &PalwShapeProfileV3, p: u32, d: u32) -> Option<u64> {
    step_leaf_count_capped_v1(profile, &ctx(profile, p, d), LADDER).ok()
}

fn ccu_job(profile: &PalwShapeProfileV3, p: u32, d: u32) -> Option<u128> {
    palw_job_economic_compute_v1(profile, &ctx(profile, p, d), &PALW_ECONOMIC_COST_TABLE_V1).ok()
}

/// the compute one DRAW actually runs past `palw_prefill_draw` (armed on t11 at DAA 4,000)
fn ccu_draw(profile: &PalwShapeProfileV3, p: u32, d: u32) -> Option<u128> {
    palw_attempt_economic_compute_v1(profile, &ctx(profile, p, d), true, &PALW_ECONOMIC_COST_TABLE_V1).ok()
}

// ------------------------------------------------------------------------------------------
fn main() {
    let rows = rows();

    sep("R1  the four shipped classes, re-derived independently");
    println!(
        "{:<22} {:>6} {:>10} {:>14} {:>18} {:>18} {:>12}",
        "class", "n_ctx", "canonical", "pwu(leaves)", "canonical CCU", "DRAW CCU", "leaves/GMACeq"
    );
    for r in &rows {
        let (p, d) = r.canonical;
        let l = leaves(&r.profile, p, d).unwrap();
        let c = ccu_job(&r.profile, p, d).unwrap();
        let dr = ccu_draw(&r.profile, p, d).unwrap();
        println!(
            "{:<22} {:>6} {:>10} {:>14} {:>18} {:>18} {:>12.1}",
            r.name,
            r.profile.n_ctx,
            format!("({p},{d})"),
            l,
            c,
            dr,
            l as f64 * 1e9 / dr as f64
        );
    }

    sep("R2  the DECODE lever — priced leaves vs the job a draw actually runs (prefill_draw ARMED at DAA 4,000)");
    println!("  a class's pwu_per_inference counts the FULL canonical job; palw_attempt_job_v1 executes (prefill, 1).");
    let dense = &rows[2];
    println!("  probe row: {} (n_ctx {})", dense.name, dense.profile.n_ctx);
    let worst = worst_case_step_leaf_count_capped_v1(&dense.profile, LADDER).unwrap();
    println!("  declared worst_case_step_leaf_count = {worst}   (ladder {LADDER})");
    println!(
        "\n  {:>12} {:>16} {:>18} {:>16} {:>12}",
        "canonical", "pwu (leaves)", "DRAW CCU (MAC-eq)", "leaves/GMACeq", "vs shipped"
    );
    let mut base_ratio = 0f64;
    // every (P, D) with footprint P+D-1 <= n_ctx and counted <= worst is ADMISSIBLE
    for (p, d) in [(63u32, 2u32), (63, 128), (63, 370), (1, 128), (1, 256), (1, 432), (1, 450)] {
        let foot = p as u64 + d.max(1) as u64 - 1;
        let Some(l) = leaves(&dense.profile, p, d) else {
            println!("  ({p},{d}) refused by the ladder");
            continue;
        };
        let dr = ccu_draw(&dense.profile, p, d).unwrap();
        let ratio = l as f64 * 1e9 / dr as f64;
        if base_ratio == 0.0 {
            base_ratio = ratio;
        }
        let admissible = foot <= dense.profile.n_ctx as u64 && l <= worst;
        println!(
            "  {:>12} {:>16} {:>18} {:>16.1} {:>11.1}x  footprint {:>4} {:<8} counted<=worst {}",
            format!("({p},{d})"),
            l,
            dr,
            ratio,
            ratio / base_ratio,
            foot,
            if foot <= dense.profile.n_ctx as u64 { "fits" } else { "OVER" },
            if l <= worst { "yes" } else { "NO" }
        );
        let _ = admissible;
    }

    sep("R3  the TILE lever — is the ADR-0131 cost model really blind to tile_len?");
    let (p, d) = dense.canonical;
    let base_kernels = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&dense.profile);
    let base_ccu = ccu_job(&dense.profile, p, d).unwrap();
    println!("  {} canonical ({p},{d}); base CCU {base_ccu} MAC-eq", dense.name);
    println!("  {:>10} {:>16} {:>20} {:>14} {:>10}", "tile_len", "pwu (leaves)", "canonical CCU", "CCU moved?", "kernels=");
    for tile in [65536u32, 4096, 512, 128, 64, 32, 16, 8, 4] {
        let mut clone = dense.profile.clone();
        for t in [&mut clone.pre_nodes, &mut clone.gdn_nodes, &mut clone.attn_nodes, &mut clone.post_nodes] {
            for n in t.iter_mut() {
                n.tile_len = tile;
            }
        }
        match (leaves(&clone, p, d), ccu_job(&clone, p, d)) {
            (Some(l), Some(c)) => {
                let k = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&clone);
                println!(
                    "  {:>10} {:>16} {:>20} {:>14} {:>10}",
                    tile,
                    l,
                    c,
                    if c == base_ccu { "NO" } else { "yes" },
                    if k == base_kernels { "same" } else { "DIFFERENT" }
                );
            }
            _ => println!("  {:>10} {:>16}", tile, "refused (ladder or shape)"),
        }
    }

    sep("R4  CLOSING A1's OPEN LEAD — is `worst_case_step_leaf_count_capped_v1` the worst case?");
    println!("  the admission gate stores it as `max_step_leaf_count` and refuses `counted > worst`.");
    println!("  it enumerates n_ctx-1 prefill positions + exactly ONE decode call.");
    println!("\n  {:<22} {:>14} {:>16} {:>16} {:>10} {:>12}", "class", "declared worst", "deepest job", "job", "over?", "over ladder?");
    for r in &rows {
        let w = worst_case_step_leaf_count_capped_v1(&r.profile, LADDER).unwrap();
        // search the admissible (P, D) space (footprint <= n_ctx) for the deepest job
        let n = r.profile.n_ctx;
        let mut best = (0u64, (0u32, 0u32));
        for d in 1..=n {
            let p = n.saturating_sub(d).saturating_add(1).max(1);
            if let Some(l) = step_leaf_count_capped_v1(&r.profile, &ctx(&r.profile, p, d), u64::MAX).ok() {
                if l > best.0 {
                    best = (l, (p, d));
                }
            }
        }
        println!(
            "  {:<22} {:>14} {:>16} {:>16} {:>10} {:>12}",
            r.name,
            w,
            best.0,
            format!("({},{})", best.1.0, best.1.1),
            if best.0 > w { format!("+{:.1}%", (best.0 as f64 / w as f64 - 1.0) * 100.0) } else { "no".into() },
            if best.0 > LADDER { "YES" } else { "no" }
        );
    }

    sep("R5  the POST-7,101 configuration: ADR-0132 Upgrade C pays on CCU, ADR-0137 draws on CCU —");
    println!("     but `claim.pwu` (FORK-CHOICE WEIGHT) is still expected_attempts x LEAVES.");
    let w0 = kaspa_consensus_core::palw_work_target_v1::palw_work_floor_v1(T11_ESCROW_72, RATE_SOMPI_PER_GIGA);
    println!("  escrow(72% carve) = {} sompi = {:.2} MSK ; rate = 9 MSK/G", T11_ESCROW_72, msk(T11_ESCROW_72));
    println!("  W0 = palw_work_floor_v1(escrow, rate) = {} MAC-eq = {:.2} G", w0, w0 as f64 / 1e9);
    println!(
        "\n  {:<24} {:>16} {:>14} {:>16} {:>18} {:>14} {:>12}",
        "class / variant", "DRAW CCU", "E[attempts]", "attempted CCU", "claim.pwu = WEIGHT", "payout MSK", "weight/MSK"
    );
    let mut wrows: Vec<(String, u64, u128)> = Vec::new();
    for r in &rows {
        let (p, d) = r.canonical;
        wrows.push((r.name.to_string(), leaves(&r.profile, p, d).unwrap(), ccu_draw(&r.profile, p, d).unwrap()));
    }
    // the admissible re-declarations of the SAME dense graph
    for (p, d) in [(1u32, 432u32)] {
        wrows.push((
            format!("A16 gv5 re-declared ({p},{d})"),
            leaves(&dense.profile, p, d).unwrap(),
            ccu_draw(&dense.profile, p, d).unwrap(),
        ));
    }
    // the same graph re-tiled at 4, canonical unchanged
    {
        let mut clone = dense.profile.clone();
        for t in [&mut clone.pre_nodes, &mut clone.gdn_nodes, &mut clone.attn_nodes, &mut clone.post_nodes] {
            for n in t.iter_mut() {
                n.tile_len = 32;
            }
        }
        let (p, d) = dense.canonical;
        if let (Some(l), Some(c)) = (leaves(&clone, p, d), ccu_draw(&clone, p, d)) {
            wrows.push(("A16 gv5 re-tiled @32".into(), l, c));
        }
    }
    let mut baseline = 0f64;
    for (name, l, dr) in &wrows {
        let target = kaspa_consensus_core::palw_work_target_v1::palw_work_ticket_target_v1(*dr, w0);
        let e = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
        let attempted = kaspa_consensus_core::palw_economic_compute_v1::palw_attempted_compute_per_claim_v1(e, *dr);
        let pwu = kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, *l);
        let pay = ((attempted * RATE_SOMPI_PER_GIGA as u128) / 1_000_000_000u128).min(T11_ESCROW_72 as u128) as u64;
        let wpm = pwu as f64 / msk(pay).max(1e-9);
        if baseline == 0.0 {
            baseline = wpm;
        }
        println!(
            "  {:<24} {:>16} {:>14} {:>16} {:>18} {:>14.2} {:>12.3e}",
            name, dr, e, attempted, pwu, msk(pay), wpm
        );
    }
    println!("\n  weight bought per MSK paid, normalised to the BASE-0 floor:");
    for (name, l, dr) in &wrows {
        let target = kaspa_consensus_core::palw_work_target_v1::palw_work_ticket_target_v1(*dr, w0);
        let e = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
        let attempted = kaspa_consensus_core::palw_economic_compute_v1::palw_attempted_compute_per_claim_v1(e, *dr);
        let pwu = kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, *l);
        let pay = ((attempted * RATE_SOMPI_PER_GIGA as u128) / 1_000_000_000u128).min(T11_ESCROW_72 as u128) as u64;
        println!("    {:<28} {:>12.2}x", name, (pwu as f64 / msk(pay).max(1e-9)) / baseline);
    }

    sep("R6  COUNTEREXAMPLE TABLE — the four families the operator asked for");
    println!("  reward basis A = ADR-0124 Decision 6 leaf price: escrow x own_pwu / unit_pwu   (fence palw_work_priced_reward @ DAA 7,101)");
    println!("  reward basis B = ADR-0132 Upgrade C: min(escrow, attempted_ccu x rate)         (same height)");
    println!("  WEIGHT         = claim.pwu = expected_attempts x pwu_per_inference             (NO FENCE, live today)\n");

    // unit_pwu today = max pwu among the weight-bearing model classes
    let unit_today: u64 = rows[1..].iter().map(|r| leaves(&r.profile, r.canonical.0, r.canonical.1).unwrap()).max().unwrap();
    println!("  unit_pwu over the shipped weight-bearing model classes = {unit_today}");

    struct Case {
        family: &'static str,
        name: String,
        job: String,
        input: u32,
        output: u32,
        leaves: u64,
        draw_ccu: u128,
    }
    let mut cases: Vec<Case> = Vec::new();
    // (c) same task, different model — one anchor-derived inference on each registered class
    for r in &rows {
        let (p, d) = r.canonical;
        cases.push(Case {
            family: "(c) same task, diff model",
            name: r.name.to_string(),
            job: format!("canonical ({p},{d}); draw runs ({p},1)"),
            input: p,
            output: 1,
            leaves: leaves(&r.profile, p, d).unwrap(),
            draw_ccu: ccu_draw(&r.profile, p, d).unwrap(),
        });
    }
    // (b) same actual cost, different reward — same graph, same EXECUTED draw job, decode re-declared
    for d in [2u32, 128, 370] {
        cases.push(Case {
            family: "(b) same cost, diff reward",
            name: format!("A16 gv5 canonical (63,{d})"),
            job: "identical executed draw (63,1)".into(),
            input: 63,
            output: 1,
            leaves: leaves(&dense.profile, 63, d).unwrap(),
            draw_ccu: ccu_draw(&dense.profile, 63, d).unwrap(),
        });
    }
    // (a) same apparent difficulty (near-equal LEAVES), different actual cost
    {
        let mut clone = dense.profile.clone();
        for t in [&mut clone.pre_nodes, &mut clone.gdn_nodes, &mut clone.attn_nodes, &mut clone.post_nodes] {
            for n in t.iter_mut() {
                n.tile_len = 512;
            }
        }
        let (p, d) = dense.canonical;
        if let (Some(l), Some(c)) = (leaves(&clone, p, d), ccu_draw(&clone, p, d)) {
            cases.push(Case {
                family: "(a) same leaves, diff cost",
                name: "A16 gv5 re-tiled @512".into(),
                job: "canonical (63,2)".into(),
                input: 63,
                output: 1,
                leaves: l,
                draw_ccu: c,
            });
        }
        let h = &rows[1];
        cases.push(Case {
            family: "(a) same leaves, diff cost",
            name: h.name.to_string(),
            job: "canonical (7,2)".into(),
            input: 7,
            output: 1,
            leaves: leaves(&h.profile, 7, 2).unwrap(),
            draw_ccu: ccu_draw(&h.profile, 7, 2).unwrap(),
        });
    }
    // (d) same task, different prompt representation — the FREE-PROMPT lane
    {
        let quantum = leaves(&dense.profile, dense.canonical.0, dense.canonical.1).unwrap() / 8;
        println!("\n  free-prompt quantum for the dense row = canonical_leaves/8 = {quantum} leaves; cap 64 quanta/receipt\n");
        for p in [8u32, 63, 200, 500] {
            let l = leaves(&dense.profile, p, 2).unwrap();
            let honest = ccu_job(&dense.profile, p, 2).unwrap();
            // a producer holding the prefix's KV cache recomputes only the last position + decode
            let cached = ccu_job(&dense.profile, 1, 2).unwrap();
            let q = (l / quantum).min(64);
            let pwu = q * quantum;
            println!(
                "    (d) fp prompt {:>4} tok -> {:>12} work_leaves, {:>2} quanta, claim.pwu {:>12} | honest {:>7.2} G, prefix-cached {:>7.2} G, saving {:>7.1}x",
                p,
                l,
                q,
                pwu,
                honest as f64 / 1e9,
                cached as f64 / 1e9,
                honest as f64 / cached as f64
            );
        }
    }

    println!(
        "\n  {:<28} {:<26} {:>8} {:>8} {:>14} {:>16} {:>13} {:>13} {:>12}",
        "family", "class / variant", "in tok", "out tok", "pwu (leaves)", "DRAW CCU", "basis A MSK", "basis B MSK", "A-MSK/GMACeq"
    );
    for c in &cases {
        let unit = unit_today.max(c.leaves);
        let pay_a = kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1(T11_ESCROW_72, c.leaves, unit);
        let target = kaspa_consensus_core::palw_work_target_v1::palw_work_ticket_target_v1(c.draw_ccu, w0);
        let e = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
        let attempted = kaspa_consensus_core::palw_economic_compute_v1::palw_attempted_compute_per_claim_v1(e, c.draw_ccu);
        let pay_b = ((attempted * RATE_SOMPI_PER_GIGA as u128) / 1_000_000_000u128).min(T11_ESCROW_72 as u128) as u64;
        println!(
            "  {:<28} {:<26} {:>8} {:>8} {:>14} {:>16} {:>13.2} {:>13.2} {:>12.2}",
            c.family,
            c.name,
            c.input,
            c.output,
            c.leaves,
            c.draw_ccu,
            msk(pay_a),
            msk(pay_b),
            msk(pay_a) * 1e9 / c.draw_ccu as f64
        );
    }

    sep("R7  the DILUTION channel: does one registration reprice the incumbents?");
    println!("  work_price_unit() = max pwu_per_inference over Active weight-bearing MODEL classes");
    println!("  BUT palw_state_v2.rs:10019 takes `Some(snapshot) => snapshot.priced_reward(..)` FIRST,");
    println!("  and the snapshot exists for every class the registry holds work for.");
    let unit_after = leaves(&dense.profile, 1, 432).unwrap();
    println!("\n  unit_pwu now                        = {unit_today}");
    println!("  unit_pwu after ONE (1,432) register = {unit_after}  ({:.1}x)", unit_after as f64 / unit_today as f64);
    println!("\n  {:<24} {:>16} {:>16} {:>9}", "class", "basis A now", "basis A after", "change");
    for r in &rows[1..] {
        let l = leaves(&r.profile, r.canonical.0, r.canonical.1).unwrap();
        let now = kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1(T11_ESCROW_72, l, unit_today);
        let after = kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1(T11_ESCROW_72, l, unit_after);
        println!("  {:<24} {:>16.2} {:>16.2} {:>8.1}%", r.name, msk(now), msk(after), 100.0 * after as f64 / now as f64);
    }
    println!("\n  ...but under basis B (which arms at the SAME height) none of those numbers is read.");
    println!("  the dilution only bites a class with NO registry work row. See the write-up.");
}
