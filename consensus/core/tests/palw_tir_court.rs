//! **RFC-0002 Phase F, step F5's exit test: the IR court on real executions.**
//!
//! Every program of `consensus-vectors/tir-v1/programs` (the five corpus models and the two state
//! programs) is run as a job — a prompt, then greedily generated tokens fed back — under a layout
//! with ragged multi-tile commit points, two-position checkpoints and two-row history tiles, its
//! logits committed under the tiled scheme, its params committed as a TIR inventory. Then:
//!
//! * every leaf of every honest execution, refuted with the refutation the builder assembles, is
//!   ACQUITTED (the cone recomputes to the committed lanes);
//! * every single-lane forgery that stays inside its proven interval is CONVICTED as a computation
//!   mismatch at that lane, and every lane pushed outside it is convicted by PALW-TIR-33 — on the
//!   disputed leaf itself, and on an operand whichever leaf is disputed;
//! * a carriage that is not the canonical set (a unit dropped, a unit added, a pin or a prompt that
//!   is not read) is REFUSED, and a binding or a leaf whose own structure is wrong convicts from it;
//! * the logits-consistency accusation and the decode-token door convict what they must and acquit
//!   the honest run;
//! * mutated refutations never panic the court.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOpeningV1, artifact_leaf_v1, open_artifact_leaf_v1, palw_artifact_multiproof_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepFaultV1, step_merkle_root_v1, step_opening_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, tiled_decode_pin_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{palw_tir_leaf_index_v1, palw_tir_visit_inventory_rows_v1};
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirEvidenceStoreV1, PalwTirInventoryIndexV1, PalwTirLogitsConsistencyV1,
    PalwTirParamCarriageV1, palw_tir_param_carriage_bytes_v1,
    PalwTirTraceLanesV1, check_tir_cone_refutation_v1, check_tir_decode_token_tiled_v1, check_tir_logits_consistency_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, palw_tir_execution_root_v1};
use misaka_palw_tir::DType;

#[test]
fn every_honest_leaf_is_acquitted() {
    let mut total = 0usize;
    let mut kinds = BTreeMap::new();
    for f in fixtures() {
        let x = f.honest();
        for (i, leaf) in f.leaves.iter().enumerate() {
            let r = refute(&f, &x, i as u64);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES);
            assert_eq!(verdict, Err(PalwStepRefuteError::NoFaultFound), "{} leaf {i} {:?}", f.name, leaf.kind);
            let kind = match leaf.kind {
                PalwTirLeafKindV1::Commit { .. } => "commit",
                PalwTirLeafKindV1::State { .. } => "checkpoint",
                PalwTirLeafKindV1::HistTile { .. } => "history tile",
            };
            *kinds.entry(kind).or_insert(0usize) += 1;
            total += 1;
        }
    }
    assert_eq!(kinds.len(), 3, "commit tiles, checkpoints and history tiles all adjudicated");
    assert!(total > 900, "{total} leaves");
}

#[test]
fn every_single_lane_forgery_is_convicted() {
    let mut convicted = 0usize;
    for f in fixtures() {
        let stride = (f.leaves.len() / 150).max(1);
        for i in (0..f.leaves.len()).step_by(stride) {
            let leaf = &f.leaves[i];
            let lane = leaf.value_count as usize / 2;
            let v = f.values[i][lane];
            let iv = f.interval(leaf);
            let Some(forged) = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w)) else { continue };
            let mut values = f.values.clone();
            values[i][lane] = forged;
            let x = f.commit(&values, &f.rows, &f.generated);
            let r = refute(&f, &x, i as u64);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {i}: {e}", f.name));
            assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 }, "{} leaf {i}", f.name);
            convicted += 1;
        }
    }
    assert!(convicted > 500, "{convicted} forgeries convicted");
}

#[test]
fn a_lane_outside_its_proven_interval_convicts_by_palw_tir_33() {
    let mut on_output = 0usize;
    let mut on_operand = 0usize;
    for f in fixtures() {
        let honest = f.honest();
        // The first later leaf whose refutation carries each leaf as an operand.
        let mut first_reader: Vec<Option<usize>> = vec![None; f.leaves.len()];
        for j in 0..f.leaves.len() {
            for p in &refute(&f, &honest, j as u64).operands.preimages {
                let k = honest.preimages.iter().position(|q| q.coord == p.coord).unwrap();
                first_reader[k].get_or_insert(j);
            }
        }
        let stride = (f.leaves.len() / 60).max(1);
        for i in (0..f.leaves.len()).step_by(stride) {
            let leaf = &f.leaves[i];
            let iv = f.interval(leaf);
            let lane_max = if leaf.dtype == DType::Idx { u32::MAX as i128 } else { i32::MAX as i128 };
            if iv.hi >= lane_max {
                continue;
            }
            // The disputed leaf itself.
            let mut values = f.values.clone();
            values[i][0] = iv.hi + 1;
            let x = f.commit(&values, &f.rows, &f.generated);
            let mut r = refute(&f, &honest, i as u64);
            rebind(&mut r, &x);
            let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {i}: {e}", f.name));
            assert_eq!(verdict.fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 }, "{} leaf {i}", f.name);
            on_output += 1;
            // As an operand of the first later leaf that reads it: convicted there, at THIS leaf.
            if let Some(j) = first_reader[i] {
                let mut r = refute(&f, &honest, j as u64);
                rebind(&mut r, &x);
                let verdict = check_tir_cone_refutation_v1(&r, &RULES).unwrap_or_else(|e| panic!("{} leaf {j}: {e}", f.name));
                assert_eq!(verdict.fault, PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: 0 });
                let expect = kaspa_consensus_core::palw_step_leg::step_refutation_evidence_id(
                    &x.binding.committed_execution_root,
                    5,
                    i as u64,
                    verdict.fault,
                );
                assert_eq!(verdict.evidence_id, expect, "{}: the evidence names the operand's own leaf", f.name);
                on_operand += 1;
            }
        }
    }
    assert!(on_output > 100 && on_operand > 50, "{on_output} on the output, {on_operand} on an operand");
}

#[test]
fn a_carriage_that_is_not_the_canonical_set_is_refused() {
    let not_canonical = |v: Result<_, PalwStepRefuteError>| matches!(v, Err(PalwStepRefuteError::InputSetNotCanonical(_)));
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        // A leaf that reads step leaves, params, the prompt: the last commit tile of the first
        // decode position's `pre`.
        let target = (0..f.leaves.len())
            .rev()
            .find(|i| {
                let r = refute(&f, &x, *i as u64);
                !r.operands.preimages.is_empty() && (f.ops.is_empty() || r.params.is_some())
            })
            .expect("a leaf with operands");
        let honest = refute(&f, &x, target as u64);
        assert_eq!(check_tir_cone_refutation_v1(&honest, &RULES), Err(PalwStepRefuteError::NoFaultFound));

        // An operand dropped.
        let mut r = honest.clone();
        r.operands.preimages.pop();
        rebind(&mut r, &x);
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: an operand dropped", f.name);
        // An operand added: the earliest leaf the set does not hold.
        let mut r = honest.clone();
        let held: Vec<_> = r.operands.preimages.iter().map(|p| p.coord).collect();
        if let Some(extra) = (0..target).find(|i| !held.contains(&x.preimages[*i].coord)) {
            r.operands.preimages.push(x.preimages[extra].clone());
            r.operands.preimages.sort_by_key(|p| x.preimages.iter().position(|q| q.coord == p.coord));
            rebind(&mut r, &x);
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: an operand added", f.name);
        }
        // Operands out of order.
        if honest.operands.preimages.len() >= 2 {
            let mut r = honest.clone();
            r.operands.preimages.swap(0, 1);
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: out of order", f.name);
        }
        // A param dropped or added (each a valid multiproof of the wrong set).
        if let Some(proof) = &honest.params {
            let held: Vec<u32> = proof.opened.iter().map(|(i, _)| *i).collect();
            let mut r = honest.clone();
            r.params = multiproof(&f, &held[..held.len() - 1]);
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param dropped", f.name);
            if let Some(extra) = (0..f.ops.len() as u32).find(|l| !held.contains(l)) {
                let mut more = held.clone();
                more.push(extra);
                more.sort_unstable();
                let mut r = honest.clone();
                r.params = multiproof(&f, &more);
                assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param added", f.name);
            }
            // The right leaf with a byte changed does not reach the root.
            let mut r = honest.clone();
            r.params.as_mut().unwrap().opened[0].1.bytes[0] ^= 1;
            assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: a param byte changed", f.name);
        }
        // A prompt or a pin the evaluation does not read; or one it reads, withheld.
        let mut r = honest.clone();
        if r.prompt_token_ids.is_empty() {
            r.prompt_token_ids = f.prompt.clone();
        } else {
            r.prompt_token_ids.clear();
        }
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: the prompt", f.name);
        let mut r = honest.clone();
        r.decode_tokens = match r.decode_tokens {
            Some(_) => None,
            None => Store { f: &f, x: &x }.decode_pin(),
        };
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &RULES)), "{}: the decode pin", f.name);
        // The whole-list prompt on a Merkle network is refused by form.
        let merkle = PalwTirCourtRulesV1 { prompt_form: PalwPromptIdsFormV1::MerkleV1, ..RULES };
        let mut r = honest.clone();
        r.prompt_token_ids = f.prompt.clone();
        assert!(not_canonical(check_tir_cone_refutation_v1(&r, &merkle)), "{}: form", f.name);
        checked += 1;
    }
    assert_eq!(checked, 5);
}

/// A program the range analysis refuses is no class: nothing can be proven about its lanes, so its
/// leaves are unadjudicable — refused, nobody slashed — never judged on unproven arithmetic.
#[test]
fn a_program_without_proven_ranges_is_never_adjudicated() {
    let mut seen = 0;
    for (n, p, params, t) in programs() {
        let f = fixture(n, p, params, t);
        if f.intervals.is_some() {
            continue;
        }
        let x = f.honest();
        for i in [0, f.leaves.len() / 2, f.leaves.len() - 1] {
            assert_eq!(
                check_tir_cone_refutation_v1(&refute(&f, &x, i as u64), &RULES),
                Err(PalwStepRefuteError::Unadjudicable),
                "{}",
                f.name
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 2, "the two golden state programs are evaluator vectors, not classes");
}

#[test]
fn the_binding_and_the_leaf_convict_from_their_own_structure() {
    for f in fixtures().into_iter().take(3) {
        let x = f.honest();
        let honest = refute(&f, &x, 5);
        let ctx_hash = f.ctx.context_hash();
        // A leaf count that is not the job's, bound consistently: convicted from the binding.
        let mut r = honest.clone();
        r.binding.step_leaf_count += 1;
        r.binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx_hash,
            &r.binding.full_logits_trace_root,
            &f.class_id,
            r.binding.step_leaf_count,
            &r.binding.step_merkle_root,
        );
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::StepLeafCountNotCanonical, "{}", f.name);
        // A job longer than the class's context.
        let mut long = f.class.clone();
        long.layout.max_context = PREFILL + DECODE - 2;
        let class_id = long.class_id(&f.artifact_root);
        let mut r = honest.clone();
        r.binding.class = long;
        r.binding.job_context.shape_profile_id = class_id;
        let ctx_hash = r.binding.job_context.context_hash();
        r.binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx_hash,
            &r.binding.full_logits_trace_root,
            &class_id,
            r.binding.step_leaf_count,
            &r.binding.step_merkle_root,
        );
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::JobExceedsClassContext, "{}", f.name);
        // A binding whose parts do not produce its root is about another execution: refused.
        let mut r = honest.clone();
        r.binding.full_logits_trace_root = Hash64::from_bytes([9; 64]);
        assert!(matches!(check_tir_cone_refutation_v1(&r, &RULES), Err(PalwStepRefuteError::InputSetNotCanonical(_))));
        // A committed leaf with a value count that is not its coordinate's: convicted structurally.
        let mut x2 = x.clone();
        x2.preimages[5].value_count += 1;
        x2.preimages[5].values_le.extend_from_slice(&[0; 4]);
        let x2 = {
            let hashes: Vec<Hash64> = x2.preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash_of(&f), &f.class_id, p)).collect();
            let root = step_merkle_root_v1(&hashes).unwrap();
            let mut b = x2.binding.clone();
            b.step_merkle_root = root;
            b.committed_execution_root =
                palw_tir_execution_root_v1(&ctx_hash_of(&f), &b.full_logits_trace_root, &f.class_id, b.step_leaf_count, &root);
            Execution { hashes, binding: b, ..x2 }
        };
        let mut r = honest.clone();
        rebind(&mut r, &x2);
        assert_eq!(check_tir_cone_refutation_v1(&r, &RULES).unwrap().fault, PalwStepFaultV1::StepValueCountNotCanonical, "{}", f.name);
    }
}

fn ctx_hash_of(f: &Fixture) -> Hash64 {
    f.ctx.context_hash()
}

#[test]
fn the_logits_trace_and_the_decode_tokens_are_held_to_the_step_leaves() {
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        let post = (f.space.occurrences().len() - 1) as u32;
        let logits_leaves: Vec<usize> = f
            .leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == f.space.program.logits))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(logits_leaves.len(), DECODE as usize, "{}: one logits tile per selecting row", f.name);
        let accuse = |x: &Execution, leaf: usize| {
            let l = &f.leaves[leaf];
            let row = l.position + 1 - PREFILL;
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
            // Tile 0 of each row: the pin's committed tile is the one holding lane `generated`, which
            // is tile 0 for these vocabularies.
            assert_eq!(
                check_tir_logits_consistency_v1(&accuse(&x, leaf), &RULES),
                Err(PalwStepRefuteError::NoFaultFound),
                "{}",
                f.name
            );
            // The trace row forged at lane 1: the executor committed two different rows.
            let row = (f.leaves[leaf].position + 1 - PREFILL) as usize;
            let mut rows = f.rows.clone();
            let lane = 1 % rows[row].len();
            rows[row][lane] += 1;
            let forged = f.commit(&f.values, &rows, &f.generated);
            let verdict = check_tir_logits_consistency_v1(&accuse(&forged, leaf), &RULES).expect("convicted");
            assert_eq!(verdict.fault, PalwStepFaultV1::TirLogitsTraceMismatch { value_index: lane as u32 });
        }
        // The decode-token door: the honest greedy token stands against every lane; a token that is
        // not the greedy one falls to the lane that beats it.
        if f.greedy {
            let vocab = f.rows[0].len() as u32;
            for row in 0..DECODE {
                for beat in 0..vocab.min(8) {
                    let pin = tiled_decode_pin_v1(&f.ctx, &x.rows, &x.generated, row, beat).unwrap();
                    assert_eq!(check_tir_decode_token_tiled_v1(&x.binding, &pin, &RULES), Err(PalwStepRefuteError::NoFaultFound));
                }
            }
            if vocab > 1 {
                let mut generated = f.generated.clone();
                let honest_pick = generated[0];
                generated[0] = (honest_pick + 1) % vocab;
                let forged = f.commit(&f.values, &f.rows, &generated);
                let pin = tiled_decode_pin_v1(&f.ctx, &forged.rows, &forged.generated, 0, honest_pick).unwrap();
                let verdict = check_tir_decode_token_tiled_v1(&forged.binding, &pin, &RULES).expect("convicted");
                assert_eq!(verdict.fault, PalwStepFaultV1::DecodeTokenMismatch { position: 0 }, "{}", f.name);
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn the_inventory_index_is_the_inventory() {
    for (name, program, _, _) in programs() {
        let index = PalwTirInventoryIndexV1::new(&program).expect("an inventory");
        if program.params.is_empty() {
            assert_eq!(index.leaf_count(), 0);
            continue;
        }
        let mut rows = Vec::new();
        palw_tir_visit_inventory_rows_v1(&program, &mut |r| rows.push(r)).expect("rows");
        assert_eq!(rows.len() as u32, index.leaf_count(), "{name}");
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(index.piece_of(i as u32), Some((r.param, r.layer, r.row_start, r.len)), "{name} leaf {i}");
            for byte in [r.row_start as u64, (r.row_start + r.len - 1) as u64, (r.row_start + r.len / 2) as u64] {
                assert_eq!(index.leaf_of(r.param, r.layer, byte), Some(i as u32), "{name}");
                assert_eq!(palw_tir_leaf_index_v1(&program, r.param, r.layer, byte), Some(i as u32), "{name}");
            }
        }
        assert_eq!(index.piece_of(index.leaf_count()), None);
    }
}

#[test]
fn hostile_refutations_never_panic_the_court() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut decoded = 0usize;
    for f in fixtures() {
        let x = f.honest();
        for leaf in [0usize, f.leaves.len() / 2, f.leaves.len() - 1] {
            let bytes = borsh::to_vec(&refute(&f, &x, leaf as u64)).unwrap();
            for _ in 0..40 {
                let mut m = bytes.clone();
                for _ in 0..1 + next() % 4 {
                    let at = (next() % m.len() as u64) as usize;
                    m[at] ^= 1 << (next() % 8);
                }
                if let Ok(r) = borsh::from_slice::<PalwTirConeRefutationV1>(&m) {
                    let _ = check_tir_cone_refutation_v1(&r, &RULES);
                    decoded += 1;
                }
            }
        }
    }
    assert!(decoded > 100, "{decoded} mutated refutations adjudicated without a panic");
}

/// A multiproof of inventory leaves `leaves` of `f` (`None` for none), built from the whole inventory.
fn multiproof(f: &Fixture, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
    if leaves.is_empty() {
        return None;
    }
    let hashes: Vec<Hash64> = f.ops.iter().map(artifact_leaf_v1).collect();
    let opened: Vec<_> = leaves.iter().map(|i| (*i, f.ops[*i as usize].clone())).collect();
    palw_artifact_multiproof_v1(&hashes, &opened)
}

/// **The parameter carriage's vectors (design §2.12.1)**: across the corpus's honest closes, the
/// carried multiproof opens a run of consecutive leaves, a set with gaps, and a single leaf; each is
/// the builder's multiproof of exactly the leaves read, its borsh bytes are
/// `palw_tir_param_carriage_bytes_v1(Multiproof)` and the per-leaf vector's are
/// `…(PerLeafOpenings)` to the byte, and a set of two or more leaves never costs more than per-leaf.
#[test]
fn the_parameter_carriage_is_one_multiproof_priced_to_the_byte() {
    let (mut runs, mut gaps, mut singles, mut none) = (0usize, 0usize, 0usize, 0usize);
    let (mut per_leaf_total, mut multiproof_total) = (0u64, 0u64);
    for f in fixtures() {
        let x = f.honest();
        let program = f.class.decode_program().expect("the fixture's program");
        for i in 0..f.leaves.len() {
            let r = refute(&f, &x, i as u64);
            let leaves: Vec<u32> = r.params.iter().flat_map(|p| p.opened.iter().map(|(l, _)| *l)).collect();
            assert_eq!(r.params, multiproof(&f, &leaves), "{} leaf {i}: the builder's multiproof of the leaves read", f.name);
            let carried = borsh::to_vec(&r.params).unwrap().len() as u64;
            assert_eq!(palw_tir_param_carriage_bytes_v1(&program, PalwTirParamCarriageV1::Multiproof, &leaves), Some(carried), "{} leaf {i}", f.name);
            let openings: Vec<PalwArtifactOpeningV1> = leaves.iter().map(|l| open_artifact_leaf_v1(&f.ops, *l).unwrap()).collect();
            let per_leaf = borsh::to_vec(&openings).unwrap().len() as u64;
            assert_eq!(palw_tir_param_carriage_bytes_v1(&program, PalwTirParamCarriageV1::PerLeafOpenings, &leaves), Some(per_leaf));
            match leaves.len() {
                0 => none += 1,
                1 => singles += 1,
                _ => {
                    assert!(carried <= per_leaf + 1, "{} leaf {i}: {carried} > {per_leaf}", f.name);
                    if leaves.windows(2).all(|w| w[1] == w[0] + 1) {
                        runs += 1;
                    } else {
                        gaps += 1;
                    }
                }
            }
            per_leaf_total += per_leaf;
            multiproof_total += carried;
        }
    }
    println!("closes: {runs} runs, {gaps} with gaps, {singles} single leaves, {none} with none; bytes {multiproof_total} vs {per_leaf_total} per leaf");
    // The corpus's closes read runs and gapped sets (the single-leaf close is the tiny program's:
    // `palw_tir_court_v1`'s `the_tiny_program_s_closes_carry_a_single_leaf_a_run_and_none`).
    assert!(runs > 0 && gaps > 0 && none > 0, "every shape occurs: {runs} runs, {gaps} gaps, {none} none");
    assert!(multiproof_total < per_leaf_total, "the multiproof is the smaller carriage over the corpus");
}

/// **A malformed multiproof is refused, never a panic, never a conviction** (the rcore/hf-court-total
/// pattern: the court runs inside block processing, where a panic halts the node). Every structural
/// mutation of an honest close's multiproof — its size, its index set, its siblings, its operands —
/// under `catch_unwind`.
#[test]
fn a_malformed_multiproof_is_refused_never_a_panic() {
    let mut adjudicated = 0usize;
    for f in fixtures() {
        let x = f.honest();
        let Some(i) = (0..f.leaves.len()).find(|i| refute(&f, &x, *i as u64).params.as_ref().is_some_and(|p| p.opened.len() >= 2)) else {
            continue;
        };
        let honest = refute(&f, &x, i as u64);
        assert_eq!(check_tir_cone_refutation_v1(&honest, &RULES), Err(PalwStepRefuteError::NoFaultFound));
        let proof = honest.params.clone().unwrap();
        let mut mutations: Vec<(String, Option<PalwArtifactMultiproofV1>)> = vec![("none at all".into(), None)];
        let mut add = |what: &str, edit: &dyn Fn(&mut PalwArtifactMultiproofV1)| {
            let mut p = proof.clone();
            edit(&mut p);
            mutations.push((what.into(), Some(p)));
        };
        add("empty", &|p| p.opened.clear());
        add("leaf count 0", &|p| p.leaf_count = 0);
        add("leaf count 1", &|p| p.leaf_count = 1);
        add("leaf count + 1", &|p| p.leaf_count += 1);
        add("leaf count - 1", &|p| p.leaf_count -= 1);
        add("leaf count max", &|p| p.leaf_count = u32::MAX);
        add("reversed", &|p| p.opened.reverse());
        add("duplicate", &|p| {
            let first = p.opened[0].clone();
            p.opened.insert(0, first);
        });
        add("index = leaf count", &|p| p.opened.last_mut().unwrap().0 = p.leaf_count);
        add("index max", &|p| p.opened.last_mut().unwrap().0 = u32::MAX);
        add("index shifted", &|p| p.opened[0].0 = p.opened[0].0.wrapping_add(1));
        add("siblings empty", &|p| p.siblings.clear());
        add("a sibling short", &|p| {
            p.siblings.pop();
        });
        add("a sibling extra", &|p| p.siblings.push(Hash64::from_bytes([7; 64])));
        add("an operand renamed", &|p| p.opened[0].1.tensor_name.push('x'));
        add("an operand relayered", &|p| p.opened[0].1.layer = Some(p.opened[0].1.layer.map_or(0, |l| l.wrapping_add(1))));
        add("an operand moved", &|p| p.opened[0].1.row_start = p.opened[0].1.row_start.wrapping_add(1));
        add("an operand truncated", &|p| {
            p.opened[0].1.bytes.pop();
        });
        add("an operand extended", &|p| p.opened[0].1.bytes.push(0));
        add("an operand emptied", &|p| p.opened[0].1.bytes.clear());
        for k in 0..proof.siblings.len() {
            add(&format!("sibling {k} altered"), &|p| {
                let mut b = p.siblings[k].as_bytes();
                b[k % 64] ^= 0x5a;
                p.siblings[k] = Hash64::from_bytes(b);
            });
        }
        for (what, params) in mutations {
            let mut r = honest.clone();
            r.params = params;
            let verdict = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check_tir_cone_refutation_v1(&r, &RULES)))
                .unwrap_or_else(|_| panic!("{}: {what}: the court panicked", f.name));
            assert!(verdict.is_err(), "{}: {what}: a malformed multiproof convicted an honest executor: {verdict:?}", f.name);
            adjudicated += 1;
        }
    }
    assert!(adjudicated > 50, "{adjudicated} malformed multiproofs adjudicated");
}
