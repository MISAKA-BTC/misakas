//! **PALW-TIR-38's soundness: the carried size admission bounds every terminal close by is never below
//! the close the court's own builders make.**
//!
//! For every corpus model (its fixture job and layout), [`palw_tir_worst_closes_v1`] bounds each commit
//! point's worst terminal close; then every leaf of the honest execution is closed as a node closes it
//! — a cone close (`build_tir_cone_refutation_v1`) at a leaf that is not dissected, and at a dissected
//! one its root claim (`build_tir_root_claim_v1`) and the bottom of an honest dissection played to its
//! first and to its last tile (`build_tir_dissect_bottom_v1`) — its program stripped as it rides, and
//! the borsh length of the object as filed is held under the bound. The measured margins are printed.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1;
use kaspa_consensus_core::palw_tir_close_size_v1::{PalwTirCloseSizingV1, PalwTirParamFormV1, palw_tir_worst_closes_v1};
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirInventoryIndexV1, build_tir_cone_refutation_v1, build_tir_dissect_bottom_v1, build_tir_dissect_round_v1,
    build_tir_root_claim_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);

fn close_bytes(mut proof: PalwCourtVerdictProofV2) -> u64 {
    proof.tir_strip_program_v1();
    let object = PalwConsensusObjectV2::CourtClosed { session_id: SESSION, verdict: PalwCourtVerdictV2::ChallengerDefeated, proof };
    borsh::to_vec(&object).unwrap().len() as u64
}

#[test]
fn every_built_close_is_within_its_bound() {
    let mut measured = 0usize;
    for f in fixtures() {
        let Some(intervals) = f.intervals.as_ref() else { continue };
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        let inventory = PalwTirInventoryIndexV1::new(&f.space.program).expect("an inventory");
        // The bound, over the layout's longest job (`prefill` 1: every position runs `post`).
        let positions = f.class.layout.max_context;
        let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&f.class, f.class_id, (1, positions))
            .expect("the longest job");
        let bounds = palw_tir_worst_closes_v1(
            &f.space,
            &inventory,
            &longest,
            &PalwTirCloseSizingV1 { form: PalwTirParamFormV1::Multiproof, court: true, cap: 1 << 36 },
        )
        .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let bound_of = |block: u8, node: u16| bounds.iter().find(|b| b.block == block && b.node == node).expect("every commit point");
        let mut worst: std::collections::BTreeMap<(u8, u16), (u64, u64)> = Default::default();
        for (i, leaf) in f.leaves.iter().enumerate() {
            let PalwTirLeafKindV1::Commit { block, node, .. } = leaf.kind else { continue };
            let b = bound_of(block, node);
            let index = i as u64;
            let site = palw_tir_dissect_site_v1(&f.space, intervals, leaf);
            let (actual, root_actual) = match (b.dissected, site) {
                (true, Some(site)) if leaf.position >= 1 => {
                    let mut root = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the root claim");
                    palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
                    let object = PalwConsensusObjectV2::CourtTirRootClaimed {
                        session_id: SESSION,
                        root: Box::new(root.clone()),
                        arity: 2,
                        signature: vec![0; 4_627],
                    };
                    let root_bytes = borsh::to_vec(&object).unwrap().len() as u64;
                    let honest = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the root claim");
                    let mut close = 0u64;
                    for last in [false, true] {
                        let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &honest, 2, 0, 10).expect("opens");
                        let mut t = 1;
                        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                            let round =
                                build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
                            phase.apply_round(&round, t, 10).expect("folds");
                            let child = if last { (phase.child_ranges().len() - 1) as u8 } else { 0 };
                            let choice = PalwTirDissectChoiceV1 {
                                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                                session_id: SESSION,
                                round: phase.round(),
                                child,
                            };
                            phase.apply_choice(&choice, t + 1, 10).expect("a choice");
                            t += 2;
                        }
                        let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("the bottom");
                        close = close.max(close_bytes(PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) }));
                    }
                    (close, root_bytes)
                }
                _ => {
                    let refutation = build_tir_cone_refutation_v1(&x.binding, index, &store, &RULES).expect("a close");
                    (close_bytes(PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) }), 0)
                }
            };
            assert!(
                actual <= b.close_bytes,
                "{}: leaf {i} (block {block} node {node}, position {}): the built close carries {actual} B, the bound is {}",
                f.name,
                leaf.position,
                b.close_bytes
            );
            assert!(
                root_actual <= b.root_claim_bytes,
                "{}: leaf {i}: the built root claim carries {root_actual} B, the bound is {}",
                f.name,
                b.root_claim_bytes
            );
            let e = worst.entry((block, node)).or_default();
            e.0 = e.0.max(actual);
            e.1 = e.1.max(root_actual);
            measured += 1;
        }
        for ((block, node), (actual, root)) in &worst {
            let b = bound_of(*block, *node);
            eprintln!(
                "{:>24} block {block} node {node:>3}{}: built ≤ {actual:>8} B, bound {:>8} B ({:.2}x){}",
                f.name,
                if b.dissected { " (dissected)" } else { "" },
                b.close_bytes,
                b.close_bytes as f64 / (*actual).max(1) as f64,
                if b.dissected { format!("; root claim {root} ≤ {}", b.root_claim_bytes) } else { String::new() }
            );
        }
    }
    assert!(measured > 100, "{measured} closes measured");
}
