//! **Spec 04b §10.3 (decision (1) of 2026-09-28): a class may tile its logits finer than the tiled
//! scheme's 4,096 lanes** — at any divisor of it — so a real vocabulary's logits-tile close (the
//! tile's weight rows) can be carried. A logits step tile then lies inside one trace tile at an
//! offset, and the logits consistency check (`check_tir_logits_consistency_v1`) compares it with that
//! sub-range: an honest execution is held to its trace at every sub-tile, and a trace row forged at
//! one lane is convicted at the sub-tile holding it, at the lane's place in it. A tile length that
//! does not divide 4,096 is refused by the layout.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::palw_step_leg::{PalwStepFaultV1, step_opening_v1};
use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, tiled_decode_pin_v1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirLogitsConsistencyV1, PalwTirTraceLanesV1, check_tir_logits_consistency_v1};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1};

const SUB: u32 = 4;

#[test]
fn a_logits_sub_tile_is_held_to_its_part_of_the_trace_tile() {
    let mut checked = 0;
    for (name, program, params, tokens) in programs() {
        let f = fixture_tiled(name, program, params, tokens, PREFILL, DECODE, SUB);
        if f.intervals.is_none() {
            continue; // a program the range analysis refuses is no class
        }
        let x = f.honest();
        let post = (f.space.occurrences().len() - 1) as u32;
        let vocab = f.rows[0].len();
        let logits_leaves: Vec<usize> = f
            .leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == f.space.program.logits))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(logits_leaves.len(), DECODE as usize * vocab.div_ceil(SUB as usize), "{}: every row in {SUB}-lane tiles", f.name);
        let accuse = |x: &Execution, leaf: usize| {
            let row = f.leaves[leaf].position + 1 - PREFILL;
            let pin = tiled_decode_pin_v1(&f.ctx, &x.rows, &x.generated, row, 0).expect("pin");
            PalwTirLogitsConsistencyV1 {
                binding: x.binding.clone(),
                step_opening: step_opening_v1(&x.hashes, leaf as u64).unwrap(),
                step_preimage: x.preimages[leaf].clone(),
                trace: PalwTirTraceLanesV1::Tiled {
                    generated_token_ids: x.generated.clone(),
                    row_root: pin.row_root,
                    row_opening: pin.row_opening,
                    tile_lanes: pin.committed_tile_lanes.clone(),
                    tile_opening: pin.committed_opening.clone(),
                },
            }
        };
        for &leaf in &logits_leaves {
            assert_eq!(
                check_tir_logits_consistency_v1(&accuse(&x, leaf), &RULES),
                Err(PalwStepRefuteError::NoFaultFound),
                "{}",
                f.name
            );
        }
        // A trace row forged at lane 5: convicted at the sub-tile holding it, at its place there.
        let lane = 5usize;
        if vocab > lane {
            let first = logits_leaves[0];
            let row = (f.leaves[first].position + 1 - PREFILL) as usize;
            let mut rows = f.rows.clone();
            rows[row][lane] += 1;
            let forged = f.commit(&f.values, &rows, &f.generated);
            for &leaf in logits_leaves.iter().filter(|l| (f.leaves[**l].position + 1 - PREFILL) as usize == row) {
                let PalwTirLeafKindV1::Commit { first_element, .. } = f.leaves[leaf].kind else { unreachable!() };
                let verdict = check_tir_logits_consistency_v1(&accuse(&forged, leaf), &RULES);
                if (first_element as usize..first_element as usize + SUB as usize).contains(&lane) {
                    let fault = verdict.expect("convicted").fault;
                    assert_eq!(fault, PalwStepFaultV1::TirLogitsTraceMismatch { value_index: (lane - first_element as usize) as u32 });
                } else {
                    assert_eq!(verdict, Err(PalwStepRefuteError::NoFaultFound), "{}: another sub-tile holds its part", f.name);
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 5, "the five admissible corpus models");
}

#[test]
fn a_logits_tile_that_does_not_divide_the_scheme_s_width_is_refused() {
    let (name, program, params, tokens) = programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense model");
    let f = fixture_tiled(name, program, params, tokens, PREFILL, DECODE, 1024);
    let mut class = f.class.clone();
    assert!(PalwTirStepSpaceV1::new(&class).is_ok(), "1,024 divides 4,096");
    let logits_index = class.layout.commit_tiles.iter().position(|t| *t == 1024).expect("the logits tile");
    class.layout.commit_tiles[logits_index] = 3000;
    let err = PalwTirStepSpaceV1::new(&class).expect_err("3,000 does not divide 4,096");
    assert!(err.to_string().contains("divides the scheme's 4,096 lanes"), "{err}");
}
