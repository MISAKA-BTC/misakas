//! **The int-10 drill's IR class files under the rules of the DAA-3,600 flag day** (an `#[ignore]`d check the drill kit's
//! `audit-tir/README.md` names; it reads class files, so it runs only when they are named).
//!
//! `palw-class declare-layout` judges a layout at the IR fence's height, where the release's element twin and box demand
//! rules are in force, not palw_tir_fence2's (R4 of `docs/design/palw/t12-int10-court-window-review.md`). The drill registers
//! the small class PAST the fence, where admission v10 sizes with the range twin under the H7 demand rules and refuses a
//! canonical job past one seat's reach. This asks the registration preflight — the one the node's registrant runs, admission
//! v10 under the rules in force at the DAA — of each class file at the last DAA below the fence and at the fence, and prints
//! the canonical leaves the registrant's builder counts (the number `getPalwClasses.canonicalLeaves` must show on the drill
//! chain: `$WORK_DIR/ir/leaf-count.txt` for the A16 class, `small-leaf-count.txt` for the small one).
//!
//!   DRILL_SMALL_CLASS=<small.class.palwtir> DRILL_A16_CLASS=<qwen25-a16.class.palwtir> \
//!     cargo test -p misaka-palw-sdk --test tir_drill_class_admission_fence2 -- --ignored --nocapture
//!
//! 2026-10-01, rcore/int-10 (da6ac8beea6d): small class 120 canonical leaves, A16 class 2,199,114 — both ADMITTED at DAA 3,599
//! and at 3,600.

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_post_genesis_registration_v1, palw_tir_registration_preflight_at_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_sdk::lineages::tir::TirLineageV1;

#[test]
#[ignore]
fn the_drills_classes_under_the_rules_of_the_flag_day() {
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let fence2 = params.palw_tir_fence2.expect("the DAA-3,600 flag day").daa_score();
    let mut judged = 0;
    for (label, var) in [("small", "DRILL_SMALL_CLASS"), ("A16", "DRILL_A16_CLASS")] {
        let Ok(path) = std::env::var(var) else { continue };
        let entry = TirLineageV1::open_entry(std::path::Path::new(&path)).expect("the class file opens");
        for daa in [fence2 - 1, fence2] {
            let object = palw_tir_post_genesis_registration_v1(
                entry.class.as_ref().clone(),
                entry.canonical_context(),
                entry.artifact_root,
                0,
                1u128 << 100,
                1,
                daa + 10,
                PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_bytes([7; 64]), index: 0 }),
                vec![9; 16],
                bundle.court.max_step_leaf_count(),
            )
            .expect("the builder counts the canonical job");
            let verdict = palw_tir_registration_preflight_at_v1(&params, bundle, &object, daa, &[]);
            let counted = match &object {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 { pwu_rule, .. } => {
                    pwu_rule.canonical_leaves_v1()
                }
                _ => 0,
            };
            println!(
                "{label:<6} class {}… registered at DAA {daa} ({}): {} canonical leaves — {}",
                &entry.class_id().to_string()[..16],
                if daa < fence2 { "release rules" } else { "palw_tir_fence2 in force" },
                counted,
                match &verdict {
                    Ok(_) => "ADMITTED".to_string(),
                    Err(e) => format!("REFUSED: {e}"),
                }
            );
            judged += 1;
        }
    }
    assert!(judged > 0, "set DRILL_SMALL_CLASS and/or DRILL_A16_CLASS");
}
