//! AGENT 4 — part 2. Closes the leads part 1 opened.
//!
//!  C1  the TILE lever's true admissible bound (the ladder is checked on `worst_case`, not the canonical)
//!  C2  CLOSE A1's open lead for real: a profile whose declared worst case clears the ladder while a
//!      LEGAL job of the same class does not — accepted, unprosecutable
//!  C3  is a clone with one changed runtime field a different CLASS with identical work?
//!  C4  the free-prompt lane's true ceiling: claim.pwu = quanta x quantum, capped at 8x canonical
//!  C5  invariant (iii): does more NON-USEFUL work mean more profit? weight per executed MAC-eq.

use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1};

const LADDER: u64 = 1 << 26;

fn sep(t: &str) {
    println!("\n================ {t} ================");
}
fn ctx(p: &PalwShapeProfileV3, a: u32, b: u32) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
    kaspa_consensus_core::palw_base0_profile::rc_job_context(p, a, b)
}
fn lv(p: &PalwShapeProfileV3, a: u32, b: u32, cap: u64) -> Option<u64> {
    step_leaf_count_capped_v1(p, &ctx(p, a, b), cap).ok()
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

/// the deepest job any (P, D) with footprint P+D-1 <= n_ctx reaches
fn deepest(p: &PalwShapeProfileV3) -> (u64, (u32, u32)) {
    let n = p.n_ctx;
    let mut best = (0u64, (0u32, 0u32));
    for d in 1..=n {
        let pr = n.saturating_sub(d).saturating_add(1).max(1);
        if let Some(l) = lv(p, pr, d, u64::MAX) {
            if l > best.0 {
                best = (l, (pr, d));
            }
        }
    }
    best
}

fn main() {
    use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1;
    let dense = palw_a16_context_row_profile_v5(512).unwrap();
    let (cp, cd) = qwen25_a16_graph_v5_canonical_v1();

    sep("C1  the TILE lever's TRUE admissible bound — admission checks the ladder on worst_case too");
    println!("  palw_class_admission_v2 stores `max_step_leaf_count = worst_case_step_leaf_count_capped_v1(profile, ladder)`");
    println!("  and that call ERRORS (TooManyLeaves) above the ladder, so a too-finely-tiled class is refused there.\n");
    println!("  {:>10} {:>16} {:>16} {:>18} {:>12} {:>12}", "tile_len", "worst_case", "canonical pwu", "canonical CCU", "admissible?", "leaves/GMAC");
    let base_ccu = palw_job_economic_compute_v1(&dense, &ctx(&dense, cp, cd), &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    let base_kern = reachable_kernels_v1(&dense);
    let mut finest_ok: Option<(u32, u64)> = None;
    for tile in [65536u32, 4096, 512, 128, 64, 48, 40, 36, 32, 24, 16, 8, 4] {
        let c = retile(&dense, tile);
        let w = worst_case_step_leaf_count_capped_v1(&c, LADDER);
        let l = lv(&c, cp, cd, LADDER);
        let ccu = palw_attempt_economic_compute_v1(&c, &ctx(&c, cp, cd), true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        let ok = w.is_ok() && l.is_some() && l.unwrap() <= *w.as_ref().unwrap_or(&0);
        if ok && finest_ok.map(|(t, _)| tile < t).unwrap_or(true) {
            finest_ok = Some((tile, l.unwrap()));
        }
        println!(
            "  {:>10} {:>16} {:>16} {:>18} {:>12} {:>12.0}",
            tile,
            w.as_ref().map(|v| v.to_string()).unwrap_or_else(|_| "OVER LADDER".into()),
            l.map(|v| v.to_string()).unwrap_or_else(|| "over".into()),
            palw_job_economic_compute_v1(&c, &ctx(&c, cp, cd), &PALW_ECONOMIC_COST_TABLE_V1).unwrap(),
            if ok { "yes" } else { "REFUSED" },
            l.map(|v| v as f64 * 1e9 / ccu as f64).unwrap_or(0.0)
        );
        assert_eq!(
            palw_job_economic_compute_v1(&c, &ctx(&c, cp, cd), &PALW_ECONOMIC_COST_TABLE_V1).unwrap(),
            base_ccu,
            "ADR-0131 CCU must be invariant under re-tiling"
        );
        assert_eq!(reachable_kernels_v1(&c), base_kern, "re-tiling must not move the reachable kernel set");
    }
    let shipped = lv(&dense, cp, cd, LADDER).unwrap();
    if let Some((t, l)) = finest_ok {
        println!(
            "\n  finest ADMISSIBLE uniform tile = {t}; pwu {l} vs shipped {shipped} = {:.2}x more weight and pay\n  for BYTE-IDENTICAL arithmetic (CCU asserted equal at every row above) and the SAME kernel set.",
            l as f64 / shipped as f64
        );
    }

    sep("C2  CLOSING THE LEAD: a class whose DECLARED worst case clears the ladder while a LEGAL job does not");
    println!("  `worst_case_step_leaf_count_capped_v1` enumerates n_ctx-1 prefill positions + ONE decode call.");
    println!("  a real job pays a logits term at EVERY decode call, so (1, n_ctx) is deeper than the declared worst.\n");
    println!("  {:>7} {:>16} {:>16} {:>9} {:>10} {:>12} {:>14}", "n_ctx", "declared worst", "deepest legal", "gap", "worst<=2^26", "job<=2^26", "ACCEPTED+UNPROSECUTABLE");
    let mut hit = false;
    for n_ctx in [512u32, 576, 608, 624, 640, 672, 704, 768] {
        let Ok(p) = palw_a16_context_row_profile_v5(n_ctx) else {
            println!("  {:>7}  (profile does not project)", n_ctx);
            continue;
        };
        let w = worst_case_step_leaf_count_capped_v1(&p, LADDER);
        let (deep, job) = deepest(&p);
        let worst_ok = w.is_ok();
        let job_ok = deep <= LADDER;
        let bad = worst_ok && !job_ok;
        if bad {
            hit = true;
        }
        println!(
            "  {:>7} {:>16} {:>16} {:>8.1}% {:>10} {:>12} {:>14}   deepest job ({},{})",
            n_ctx,
            w.as_ref().map(|v| v.to_string()).unwrap_or_else(|_| "OVER".into()),
            deep,
            w.as_ref().map(|v| (deep as f64 / *v as f64 - 1.0) * 100.0).unwrap_or(f64::NAN),
            if worst_ok { "yes" } else { "no" },
            if job_ok { "yes" } else { "NO" },
            if bad { "*** YES ***" } else { "-" },
            job.0,
            job.1
        );
    }
    println!("\n  result: {}", if hit { "a geometry EXISTS. the class is admitted and holds legal jobs the ladder cannot walk." } else { "no geometry found in this sweep." });

    sep("C3  one changed runtime field = a new CLASS with byte-identical work");
    let mut clone = dense.clone();
    clone.n_threads = dense.n_threads.wrapping_add(1).max(1);
    println!("  shipped  n_threads {:>3}  class_id {:?}", dense.n_threads, dense.shape_profile_id());
    println!("  clone    n_threads {:>3}  class_id {:?}", clone.n_threads, clone.shape_profile_id());
    println!("  distinct class id            : {}", dense.shape_profile_id() != clone.shape_profile_id());
    println!("  identical pwu (leaves)       : {}", lv(&dense, cp, cd, LADDER) == lv(&clone, cp, cd, LADDER));
    println!(
        "  identical ADR-0131 CCU       : {}",
        palw_job_economic_compute_v1(&dense, &ctx(&dense, cp, cd), &PALW_ECONOMIC_COST_TABLE_V1).unwrap()
            == palw_job_economic_compute_v1(&clone, &ctx(&clone, cp, cd), &PALW_ECONOMIC_COST_TABLE_V1).unwrap()
    );
    println!("  identical reachable kernels  : {}", reachable_kernels_v1(&dense) == reachable_kernels_v1(&clone));
    println!("  (so `family_certified_for_weight_v2`'s subset test certifies the clone exactly as the original)");

    sep("C4  free-prompt: what claim.pwu can actually reach (quanta x quantum, cap 64)");
    let canon = lv(&dense, cp, cd, LADDER).unwrap();
    let quantum = canon / 8;
    println!("  canonical leaves {canon}; quantum = canonical/8 = {quantum}; MAX_QUANTA_PER_RECEIPT = 64");
    println!("  => claim.pwu is capped at 64 x quantum = {} = {:.1}x the canonical pwu", 64 * quantum, 64.0 * quantum as f64 / canon as f64);
    println!("\n  {:>10} {:>16} {:>8} {:>16} {:>14} {:>14} {:>12}", "prompt tok", "work_leaves", "quanta", "claim.pwu", "honest G-MAC", "cached G-MAC", "pwu/cachedG");
    let cached = palw_job_economic_compute_v1(&dense, &ctx(&dense, 1, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    for p in [8u32, 63, 128, 256, 400, 500] {
        let l = lv(&dense, p, 2, LADDER).unwrap();
        let honest = palw_job_economic_compute_v1(&dense, &ctx(&dense, p, 2), &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        let q = (l / quantum).min(64);
        let pwu = q * quantum;
        println!(
            "  {:>10} {:>16} {:>8} {:>16} {:>14.2} {:>14.2} {:>12.0}",
            p,
            l,
            q,
            pwu,
            honest as f64 / 1e9,
            cached as f64 / 1e9,
            pwu as f64 * 1e9 / cached as f64
        );
    }

    sep("C5  INVARIANT (iii): does more NON-USEFUL work mean more profit? weight per MAC-eq ACTUALLY EXECUTED");
    println!("  every row below is the SAME artifact, the SAME graph, the SAME kernel set, the SAME certification.");
    println!("  only the registrant's two free choices move: the canonical job's decode count, and tile_len.\n");
    println!("  {:<34} {:>16} {:>18} {:>16} {:>10}", "declaration", "pwu (leaves)", "executed draw CCU", "leaves/G-MACeq", "vs shipped");
    let mut base = 0f64;
    let variants: Vec<(String, PalwShapeProfileV3, (u32, u32))> = vec![
        ("SHIPPED  canonical (63,2) tile as-is".into(), dense.clone(), (cp, cd)),
        ("canonical (1,432) tile as-is".into(), dense.clone(), (1, 432)),
        ("canonical (63,2)  tile 64".into(), retile(&dense, 64), (cp, cd)),
        ("canonical (1,128) tile 64".into(), retile(&dense, 64), (1, 128)),
    ];
    for (name, prof, (p, d)) in &variants {
        let w = worst_case_step_leaf_count_capped_v1(prof, LADDER);
        let Some(l) = lv(prof, *p, *d, LADDER) else { continue };
        let admissible = w.as_ref().map(|v| l <= *v).unwrap_or(false);
        let draw = palw_attempt_economic_compute_v1(prof, &ctx(prof, *p, *d), true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
        let r = l as f64 * 1e9 / draw as f64;
        if base == 0.0 {
            base = r;
        }
        println!(
            "  {:<34} {:>16} {:>18} {:>16.0} {:>9.1}x   admissible {}",
            name,
            l,
            draw,
            r,
            r / base,
            if admissible { "yes" } else { "NO" }
        );
    }
}
