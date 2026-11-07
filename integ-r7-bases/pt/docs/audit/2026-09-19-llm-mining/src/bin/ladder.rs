//! AGENT 4 — part 3. What ladder does testnet-11 ACTUALLY admit at, and prosecute at?
//! Everything here reads the shipped `palw_rc_shipped_params()` — no fixture, no hand-built bundle.

use kaspa_consensus_core::config::params::palw_rc_shipped_params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1};

fn ctx(p: &PalwShapeProfileV3, a: u32, b: u32) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
    kaspa_consensus_core::palw_base0_profile::rc_job_context(p, a, b)
}

fn main() {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &rc.palw_consensus_mode else { panic!("t11 is ConsensusV2") };
    let court = bundle.court;
    let ladder = court.max_step_leaf_count();
    println!("testnet-11 shipped params (palw_rc_shipped_params, this worktree):");
    println!("  court.max_step_leaf_count()            = {ladder}  (2^{:.1})", (ladder as f64).log2());
    println!("  PALW_STEP_LEG_MAX_LEAVES (walker const)= {}", kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES);
    println!("  palw_court_ladder fence                = {:?}", rc.palw_court_ladder);
    println!("  palw_context_ladder fence              = {:?}", rc.palw_context_ladder);
    println!("  palw_prefill_draw fence                = {:?}", rc.palw_prefill_draw);
    println!("  palw_work_priced_reward fence          = {:?}", rc.palw_work_priced_reward);
    println!("  palw_economic_payout fence             = {:?}", rc.palw_economic_payout.map(|p| p.activation));
    println!("  palw_work_target fence                 = {:?}", rc.palw_work_target);
    println!("  palw_model_registry fence              = {:?}", rc.palw_model_registry);
    println!("  palw_fp_decode_rules fence             = {:?}", rc.palw_fp_decode_rules);
    println!("  palw_uncertified_weightless fence      = {:?}", rc.palw_uncertified_weightless);
    for daa in [6_000u64, 6_300, 7_100, 7_101, 7_102] {
        println!(
            "  at DAA {daa:>6}: court_ladder {:<5} prefill_draw {:<5} work_priced {:<5} payout {:<5} work_target {:<5}",
            rc.palw_court_ladder_active_at(daa),
            rc.palw_prefill_draw_active_at(daa),
            rc.palw_work_priced_reward_active_at(daa),
            rc.palw_economic_payout_at(daa).is_some(),
            rc.palw_work_target_at(daa),
        );
    }

    println!("\nthe walkers' cap at this block, by the repo's own function:");
    for daa in [2_149u64, 2_150, 6_300, 7_101] {
        let cap = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&court, rc.palw_court_ladder_active_at(daa));
        println!("  DAA {daa:>6}: palw_refutation_leaf_cap_v2 = {cap}");
    }

    println!("\nthe SHIPPED dense class against that cap (nothing registered, nothing changed):");
    let dense = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5(512).unwrap();
    let cap = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&court, true);
    let worst = worst_case_step_leaf_count_capped_v1(&dense, ladder).unwrap();
    println!("  declared worst_case (max_step_leaf_count of the class) = {worst}");
    println!("  {:>10} {:>8} {:>16} {:>14} {:>22}", "prompt", "decode", "real job leaves", "over ladder?", "court can walk it?");
    for (p, d) in [(63u32, 2u32), (1, 256), (1, 432), (1, 512), (512, 1), (256, 256), (400, 112)] {
        let foot = p as u64 + d.max(1) as u64 - 1;
        if foot > dense.n_ctx as u64 {
            continue;
        }
        let Ok(l) = step_leaf_count_capped_v1(&dense, &ctx(&dense, p, d), u64::MAX) else { continue };
        println!(
            "  {:>10} {:>8} {:>16} {:>14} {:>22}",
            p,
            d,
            l,
            if l > ladder { "YES" } else { "no" },
            if l <= cap { "yes" } else { "*** NO ***" }
        );
    }
}
