//! **ADR-0152 v3.1 F1c (addendum §4-bis.6), Tier B: the logits row a token is selected from IS the
//! last post node's committed output** — the law `LogitsNotStepOutput` (12) stands on, measured on
//! every family the chain uses.
//!
//! `the_last_post_node_is_the_row_a_token_is_selected_from` (the test `base0/fp_interval.rs` cites):
//! for the floor, the held A16 graph-v7 fixture (a fold) at vocabularies 128 and 8,292 (ragged), the
//! per-call A16 v2 (dense), and the Qwen3.6 v2 and held v7 fixtures, attempts at the prefill draw and
//! free prompts of one and three decode calls — every logits row `r` at head tiles {0, 1, mid, last}: the leaf the
//! producer committed at `canonical_step_leaf_index(profile, ctx, (r, G − 1, r == 0 ? P − 1 : 0, t))`
//! (opened by the class's own prover, `refutation_for_index`) is `step_tile_leaf_hash_v1` of the
//! row's lanes `lo..hi`, and its preimage IS those lanes. `palw_logits_head_v1` holds on each
//! profile and the head tile never straddles a logits tile.

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::{
    PalwShapeProfileV3, canonical_step_leaf_index, palw_logits_head_coordinate_v1, palw_logits_head_v1,
};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepTileLeafV1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
use kaspa_hashes::Hash64;

/// Every row and head tile of one run: the committed head leaf is the row's lanes.
fn check_run(label: &str, backend: &dyn PalwExecutionBackendV1, profile: &PalwShapeProfileV3, material: &[u8]) -> usize {
    let head = palw_logits_head_v1(profile).unwrap_or_else(|| panic!("{label}: the head predicate holds"));
    assert_eq!(head.slot + 1, profile.global_node_count(), "{label}: G − 1");
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(material).expect("the capture decodes");
    let binding = retention.binding();
    let ctx = &binding.job_context;
    let rows = retention.logits_rows();
    assert_eq!(rows.len(), ctx.exact_decode_tokens as usize, "{label}: one row per call");
    let (ctx_hash, profile_id) = (ctx.context_hash(), profile.shape_profile_id());
    let vocab = profile.vocab_size as usize;
    let mut checked = 0;
    for (r, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), vocab, "{label}: a row is the vocabulary");
        // The first two, a middle and the last (ragged) head tile of every row — each opening is
        // one prover call, and a row at V = 8,292 cuts into 2,073 head tiles.
        let mut sampled = vec![0, 1.min(head.tiles - 1), head.tiles / 2, head.tiles - 1];
        sampled.dedup();
        for t in sampled {
            let lo = (t * head.tile_len) as usize;
            let hi = (lo + head.tile_len as usize).min(vocab);
            assert_eq!(lo / PALW_LOGITS_TILE_LANES, (hi - 1) / PALW_LOGITS_TILE_LANES, "{label}: a head tile inside one logits tile");
            let coord = palw_logits_head_coordinate_v1(&head, ctx, r as u32, t).expect("a coordinate");
            let index =
                canonical_step_leaf_index(profile, ctx, &coord).unwrap_or_else(|| panic!("{label}: row {r} tile {t} is a leaf"));
            let refutation =
                backend.refutation_for_index(material, index).unwrap_or_else(|e| panic!("{label}: row {r} tile {t} opens: {e}"));
            let values_le: Vec<u8> = row[lo..hi].iter().flat_map(|v| v.to_le_bytes()).collect();
            assert_eq!(refutation.output_preimage.coord, coord, "{label}: the opened leaf is the head's");
            assert_eq!(
                refutation.output_preimage.values_le, values_le,
                "{label}: row {r} tile {t}: the head output IS the logits lanes"
            );
            let derived = step_tile_leaf_hash_v1(
                &ctx_hash,
                &profile_id,
                &PalwStepTileLeafV1 { version: PALW_STEP_LEG_OBJECT_VERSION_V1, coord, value_count: (hi - lo) as u32, values_le },
            );
            assert_eq!(
                refutation.output_opening.leaf_hash, derived,
                "{label}: row {r} tile {t}: the committed leaf is the derived one"
            );
            assert_eq!(refutation.output_opening.leaf_index, index);
            checked += 1;
        }
    }
    checked
}

fn family(label: &str, backend: &dyn PalwExecutionBackendV1, profile: &PalwShapeProfileV3, fp_prompt: &[usize]) {
    let mut checked = 0;
    for n in 0u64..2 {
        let anchor = Hash64::from_u64_word(0xF1C0_0000 ^ n);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let job = palw_attempt_job_v1(canonical, true);
        let out = backend.execute(&job, &prompt).expect("the attempt runs");
        checked += check_run(&format!("{label} attempt {n}"), backend, profile, &out.material);
    }
    for decode in [1u32, 3] {
        let fp = fp_job(profile, PalwPromptIdsFormV1::MerkleV1, fp_prompt, decode);
        let run = backend.execute_free_prompt(&fp, fp_prompt).expect("the free prompt runs");
        checked += check_run(&format!("{label} free prompt D={decode}"), backend, profile, &run.outcome.material);
    }
    assert!(checked > 0, "{label}: something was checked");
}

#[test]
fn the_last_post_node_is_the_row_a_token_is_selected_from() {
    // The floor.
    let floor = floor_backend(PalwPromptIdsFormV1::MerkleV1).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let profile = floor.profile().clone();
    let vocab = profile.vocab_size as usize;
    family("floor", &floor, &profile, &(0..6).map(|i| (i * 7919 + 1013) % vocab).collect::<Vec<_>>());

    // A16: the held graph-v7 (a fold) at a ragged vocabulary too, and the per-call v2 (dense).
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    for vocab in [128u32, 8_292] {
        let artifact = a16_artifact(vocab);
        for (label, profile) in [
            ("A16 held v7 (fold)", qwen25_a16_profile_v7(a16_geometry(vocab)).expect("the held row")),
            ("A16 v2 (dense)", qwen25_a16_profile_v2(a16_geometry(vocab)).expect("the v2 row")),
        ] {
            let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx))
                .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
            let prompt: Vec<usize> = (0..12).map(|i| (i * 7919 + 1013) % vocab as usize).collect();
            family(&format!("{label} V={vocab}"), &backend, &profile, &prompt);
        }
    }

    // Qwen3.6: v2 and the held v7, at 32 positions.
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v2, qwen36_profile_v7};
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    for (label, profile) in
        [("Qwen3.6 v2", qwen36_profile_v2(geometry).expect("v2")), ("Qwen3.6 held v7", qwen36_profile_v7(geometry).expect("v7"))]
    {
        let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(profile.n_ctx))
            .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        family(label, &backend, &profile, &[3, 1, 4]);
    }
}

// ---------------------------------------------------------------------------------------------
// T18q (12) and T18r (11) on the fixtures: the chain's adjudicators over real runs
// ---------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PalwClaimSourceKindV1, PalwOffenceTargetV1, palw_forged_output_tiled_fault_v1, palw_logits_not_step_output_fault_v1,
};
use kaspa_consensus_core::palw_offence_v1::{PalwForgedOutputTiledProofV1, PalwOffenceVerifyError as E};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PalwStepBindingV2, PalwStepOpeningV1, binding_commitment_root_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PalwTiledDecodeTokensV1, base0_decode_token_select_v1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1,
    logits_event_disclosure_v1, tiled_decode_pin_v1, tiled_logits_rows_root_v1, tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::collections::BTreeMap;

/// One honest attempt run of a family, everything a filer of 11 or 12 reads: the binding, the rows,
/// the ids, the anchor, and the class's own openings of the head leaves (cached: the step tree is
/// the honest one under every bend below).
struct Run {
    anchor: Hash64,
    binding: PalwStepBindingV2,
    rows: Vec<Vec<i32>>,
    ids: Vec<u32>,
    material: Vec<u8>,
    openings: BTreeMap<(u32, u32), PalwStepOpeningV1>,
}

fn run_of(backend: &dyn PalwExecutionBackendV1, n: u64) -> Run {
    let anchor = Hash64::from_u64_word(0x18C0_0000 ^ n);
    let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
    let out: PalwExecutionOutcomeV1 = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).expect("the attempt runs");
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).expect("decodes");
    Run {
        anchor,
        binding: retention.binding().clone(),
        rows: retention.logits_rows().to_vec(),
        ids: retention.generated_token_ids().to_vec(),
        material: out.material.clone(),
        openings: BTreeMap::new(),
    }
}

impl Run {
    fn head_opening(&mut self, backend: &dyn PalwExecutionBackendV1, row: u32, tile: u32) -> PalwStepOpeningV1 {
        let (binding, material) = (&self.binding, &self.material);
        self.openings
            .entry((row, tile))
            .or_insert_with(|| {
                let head = palw_logits_head_v1(&binding.shape_profile).expect("a head");
                let coord = palw_logits_head_coordinate_v1(&head, &binding.job_context, row, tile).expect("a coordinate");
                let index = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord).expect("a leaf");
                backend.refutation_for_index(material, index).expect("the class opens its head").output_opening
            })
            .clone()
    }
}

/// `binding` with its logits trace root re-derived over `rows` and `ids` under the class's scheme,
/// and its execution root re-committed — the producer who committed those rows over this step tree.
fn with_rows(binding: &PalwStepBindingV2, rows: &[Vec<i32>], ids: &[u32]) -> PalwStepBindingV2 {
    let mut b = binding.clone();
    b.full_logits_trace_root = if b.shape_profile.logits_scheme_id == flat_logits_scheme_id_v1() {
        base0_logits_trace_root_v1(&b.job_context, rows, ids)
    } else {
        tiled_logits_trace_root_v1(&b.job_context, rows, ids).expect("rows build a tree")
    };
    b.committed_execution_root = binding_commitment_root_v1(&b);
    b
}

fn target_of(binding: &PalwStepBindingV2, anchor: Hash64) -> PalwOffenceTargetV1 {
    PalwOffenceTargetV1 {
        claim_id: Hash64::from_u64_word(0xC1A1),
        class_id: binding.shape_profile.shape_profile_id(),
        artifact_root: Hash64::default(),
        executor_bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0)),
        execution_root: binding.committed_execution_root,
        lane: Some(PalwClaimSourceKindV1::Attempt),
        segment_count: Some(4),
        phase: None,
        job_identity: anchor,
        trace_root: binding.full_logits_trace_root,
        output_root: Hash64::default(),
    }
}

/// 12's verdict on (row, head tile) of the claim committing `rows` over `run`'s step tree.
fn judge_12(backend: &dyn PalwExecutionBackendV1, run: &mut Run, rows: &[Vec<i32>], row: u32, tile: u32) -> Result<(), E> {
    let binding = with_rows(&run.binding, rows, &run.ids);
    let head = palw_logits_head_v1(&binding.shape_profile).unwrap();
    let logits_tile = if binding.shape_profile.logits_scheme_id == flat_logits_scheme_id_v1() {
        0
    } else {
        ((tile * head.tile_len) as usize / PALW_LOGITS_TILE_LANES) as u8
    };
    let event = logits_event_disclosure_v1(&binding, rows, &run.ids, row, logits_tile).expect("the event opens");
    let opening = run.head_opening(backend, row, tile);
    palw_logits_not_step_output_fault_v1(&target_of(&binding, run.anchor), &event, row, tile, &opening, PALW_STEP_LEG_MAX_LEAVES)
}

#[derive(Clone, Copy, Debug)]
enum Bend {
    /// The lane becomes the row's argmax.
    NewArgmax,
    /// The lane moves and the argmax does not.
    SameArgmax,
}

fn bent(row: &[i32], lane: usize, bend: Bend) -> Vec<i32> {
    let mut row = row.to_vec();
    let top = base0_decode_token_select_v1(&row);
    let max = row[top];
    match bend {
        Bend::NewArgmax => row[lane] = max.saturating_add(1),
        Bend::SameArgmax if lane == top => row[lane] = max.saturating_add(1),
        Bend::SameArgmax if row[lane].saturating_add(1) < max => row[lane] += 1,
        Bend::SameArgmax => row[lane] = row[lane].saturating_sub(1),
    }
    if matches!(bend, Bend::SameArgmax) {
        assert_eq!(base0_decode_token_select_v1(&row), top, "the argmax does not move");
    }
    row
}

/// **T18q: `LogitsNotStepOutput` convicts exactly the bent (row, tile), and nothing else.** On the
/// floor (flat, four rows), the held A16 v7 (a fold) at V = 128 and the ragged 8,292, the per-call A16
/// v2 (dense) at 8,292 and the held Qwen3.6 v7: rows {0, mid, last} × {NewArgmax, SameArgmax} × lanes
/// {0, interior, V − 1}, each bend re-committed over the honest step tree; every sampled (row, head
/// tile) — the first two, the bent one and its neighbours, the last two, in every row — is judged:
/// the bent one convicts, every other is `LogitsHold`, and every honest (row, tile) is `LogitsHold`.
#[test]
fn t18q_twelve_convicts_exactly_the_bent_row_and_tile() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    let mut families: Vec<(String, Box<dyn PalwExecutionBackendV1>)> = Vec::new();
    families
        .push(("floor".into(), Box::new(floor_backend(PalwPromptIdsFormV1::MerkleV1).with_attempt_rules(PalwAttemptRulesV1::CoreV1))));
    for (vocab, dense) in [(128u32, false), (8_292, false), (8_292, true)] {
        let artifact = a16_artifact(vocab);
        let profile = if dense {
            qwen25_a16_profile_v2(a16_geometry(vocab)).unwrap()
        } else {
            qwen25_a16_profile_v7(a16_geometry(vocab)).unwrap()
        };
        let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx))
            .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        families.push((format!("A16 {} V={vocab}", if dense { "v2 dense" } else { "held v7 fold" }), Box::new(backend)));
    }
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).unwrap();
    families.push((
        "Qwen3.6 held v7".into(),
        Box::new(qwen36_backend(&artifact, &held, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1)),
    ));
    for (label, backend) in &families {
        let backend = backend.as_ref();
        let mut run = run_of(backend, 0);
        let head = palw_logits_head_v1(&run.binding.shape_profile).expect("a head");
        let vocab = run.binding.shape_profile.vocab_size as usize;
        let d = run.rows.len() as u32;
        let honest_rows = run.rows.clone();
        let sampled = |lane_tile: u32| -> Vec<u32> {
            let mut tiles =
                vec![0, 1, lane_tile.saturating_sub(1), lane_tile, lane_tile + 1, head.tiles.saturating_sub(2), head.tiles - 1];
            tiles.retain(|t| *t < head.tiles);
            tiles.sort();
            tiles.dedup();
            tiles
        };
        // Honest: nothing convicts.
        for row in 0..d {
            for tile in sampled(head.tiles / 2) {
                assert_eq!(
                    judge_12(backend, &mut run, &honest_rows, row, tile),
                    Err(E::LogitsHold),
                    "{label}: honest ({row}, {tile})"
                );
            }
        }
        let mut rows_to_bend = vec![0, d / 2, d - 1];
        rows_to_bend.dedup();
        let mut convictions = 0;
        for bent_row in rows_to_bend {
            for bend in [Bend::NewArgmax, Bend::SameArgmax] {
                for lane in [0, vocab / 2, vocab - 1] {
                    let mut rows = honest_rows.clone();
                    rows[bent_row as usize] = bent(&honest_rows[bent_row as usize], lane, bend);
                    let lane_tile = (lane / head.tile_len as usize) as u32;
                    for row in 0..d {
                        for tile in sampled(lane_tile) {
                            let verdict = judge_12(backend, &mut run, &rows, row, tile);
                            if (row, tile) == (bent_row, lane_tile) {
                                assert_eq!(
                                    verdict,
                                    Ok(()),
                                    "{label}: {bend:?} lane {lane} of row {bent_row} convicts at ({row}, {tile})"
                                );
                                convictions += 1;
                            } else {
                                assert_eq!(
                                    verdict,
                                    Err(E::LogitsHold),
                                    "{label}: {bend:?} lane {lane} of row {bent_row}: ({row}, {tile}) holds"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(convictions >= 6, "{label}: every bend convicted once ({convictions})");
    }
}

/// **T18r: `ForgedOutputTiled` (11) — a committed token that is not its row's selection
/// (`NotSelected`, T4) and one past the vocabulary (`OutOfVocab`, T5) convict; the honest token is
/// `TokenHolds` both ways; a copied root beside another run's binding is refused.** On the tiled
/// families: A16 v2 dense and held v7 at the ragged V = 8,292, Qwen3.6 held v7. The flat floor is
/// refused by name (its forged token is `ForgedOutput`, 8).
#[test]
fn t18r_eleven_convicts_a_forged_tiled_token() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    let ladder = PALW_STEP_LEG_MAX_LEAVES;
    let judge = |binding: &PalwStepBindingV2, anchor: Hash64, proof: &PalwForgedOutputTiledProofV1| {
        palw_forged_output_tiled_fault_v1(&target_of(binding, anchor), binding, proof, ladder, false)
    };
    let artifact = a16_artifact(8_292);
    let mut families: Vec<(String, Box<dyn PalwExecutionBackendV1>)> = Vec::new();
    for (label, profile) in [
        ("A16 v2 dense", qwen25_a16_profile_v2(a16_geometry(8_292)).unwrap()),
        ("A16 held v7 fold", qwen25_a16_profile_v7(a16_geometry(8_292)).unwrap()),
    ] {
        let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx))
            .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        families.push((label.into(), Box::new(backend)));
    }
    let (q36, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).unwrap();
    families.push((
        "Qwen3.6 held v7".into(),
        Box::new(qwen36_backend(&q36, &held, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1)),
    ));
    for (label, backend) in &families {
        let run = run_of(backend.as_ref(), 0);
        let (ctx, vocab) = (&run.binding.job_context, run.binding.shape_profile.vocab_size);
        for position in 0..run.ids.len() as u32 {
            let row = &run.rows[position as usize];
            let top = base0_decode_token_select_v1(row) as u32;
            assert_eq!(run.ids[position as usize], top, "{label}: the honest token is its row's selection");
            // Honest: no lane beats the committed one; the id is in the vocabulary.
            let runner_up = (0..vocab).filter(|l| *l != top).max_by_key(|l| (row[*l as usize], std::cmp::Reverse(*l))).unwrap();
            let honest_pin = tiled_decode_pin_v1(ctx, &run.rows, &run.ids, position, runner_up).unwrap();
            assert_eq!(
                judge(&run.binding, run.anchor, &PalwForgedOutputTiledProofV1::NotSelected { pin: honest_pin }),
                Err(E::TokenHolds)
            );
            let rows_root = tiled_logits_rows_root_v1(ctx, &run.rows).unwrap();
            let honest_tokens = PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: run.ids.clone() };
            assert_eq!(
                judge(&run.binding, run.anchor, &PalwForgedOutputTiledProofV1::OutOfVocab { position, tokens: honest_tokens }),
                Err(E::TokenHolds)
            );
            // T4: the runner-up committed in place of the selection.
            let mut forged = run.ids.clone();
            forged[position as usize] = runner_up;
            let binding = with_rows(&run.binding, &run.rows, &forged);
            let pin = tiled_decode_pin_v1(ctx, &run.rows, &forged, position, top).unwrap();
            assert_eq!(
                judge(&binding, run.anchor, &PalwForgedOutputTiledProofV1::NotSelected { pin }),
                Ok(()),
                "{label}: T4 at {position}"
            );
            // T5: an id past the vocabulary.
            let mut past = run.ids.clone();
            past[position as usize] = vocab + 3;
            let binding = with_rows(&run.binding, &run.rows, &past);
            let tokens = PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: past.clone() };
            assert_eq!(
                judge(&binding, run.anchor, &PalwForgedOutputTiledProofV1::OutOfVocab { position, tokens }),
                Ok(()),
                "{label}: T5 at {position}"
            );
            let not_selected_on_it = tiled_decode_pin_v1(ctx, &run.rows, &past, position, top);
            if let Some(pin) = not_selected_on_it {
                assert_eq!(
                    judge(&binding, run.anchor, &PalwForgedOutputTiledProofV1::NotSelected { pin }),
                    Err(E::PanelFalseValidNeedsContradiction),
                    "{label}: NotSelected cannot open a lane past the vocabulary; OutOfVocab is its proof"
                );
            }
        }
        // A copied root beside another run's binding: the root pin refuses it.
        let other = run_of(backend.as_ref(), 1);
        let mut forged = other.ids.clone();
        forged[0] = (forged[0] + 1) % vocab;
        let pin = tiled_decode_pin_v1(&other.binding.job_context, &other.rows, &forged, 0, other.ids[0]).unwrap();
        let foreign = with_rows(&other.binding, &other.rows, &forged);
        let mut copied = target_of(&run.binding, run.anchor);
        copied.class_id = foreign.shape_profile.shape_profile_id();
        assert_eq!(
            palw_forged_output_tiled_fault_v1(
                &copied,
                &foreign,
                &PalwForgedOutputTiledProofV1::NotSelected { pin: pin.clone() },
                ladder,
                false
            ),
            Err(E::PanelFalseValidWorkMismatch),
            "{label}: another run's binding is not the claim's"
        );
        let mut unverified = foreign.clone();
        unverified.step_leaf_count += 1;
        assert_eq!(
            palw_forged_output_tiled_fault_v1(
                &target_of(&foreign, other.anchor),
                &unverified,
                &PalwForgedOutputTiledProofV1::NotSelected { pin },
                ladder,
                false
            ),
            Err(E::BindingUnverified),
            "{label}: a binding that does not reproduce its own root"
        );
    }
    // The flat floor files ForgedOutput (8).
    let floor = floor_backend(PalwPromptIdsFormV1::MerkleV1).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let run = run_of(&floor, 0);
    let tokens = PalwTiledDecodeTokensV1 { rows_root: Hash64::from_u64_word(1), generated_token_ids: run.ids.clone() };
    assert!(matches!(
        judge(&run.binding, run.anchor, &PalwForgedOutputTiledProofV1::OutOfVocab { position: 0, tokens }),
        Err(E::ContradictionNotAdmitted(_))
    ));
}

/// **Every drill fault is convicted by the contradiction it exists for** (the drill hook,
/// addendum §4-bis.10): each [`PalwDrillFaultV1`] produced by the family's own
/// `execute_with_drill_fault` — every root the producer commits re-derived from what it committed —
/// is named by the chain's adjudicator: 12 for bent logits over the honest tree, 11 (tiled) or 8
/// (flat) for a forged token, 9's J5b / J5a / J6 / J7 for a relabelled or garbage prompt root, a
/// short prefill or an instance field, a moved activation leg or checkpoint profile, 10 for a moved
/// output root; an execution root no binding reproduces is refused `WorkMismatch` (it is the DA
/// court's: nobody can answer for it). The step lie is T46's (5).
#[test]
fn every_drill_fault_is_convicted_by_its_contradiction() {
    use kaspa_consensus_core::palw_backend::{PalwDrillBendV1, PalwDrillFaultV1 as F};
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwIdentityFaultV1 as J, PalwIdentityRulesV1, palw_binding_identity_fault_v1, palw_output_fault_v1,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwPanelContradictionV1 as C, palw_panel_contradiction_convicts_execution_v1};
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2};
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    use kaspa_consensus_core::palw_step_refute::{PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1};
    let artifact = a16_artifact(8_292);
    let a16 = qwen25_a16_profile_v2(a16_geometry(8_292)).unwrap();
    let (q36, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).unwrap();
    let floor = floor_backend(PalwPromptIdsFormV1::MerkleV1).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let floor_id = floor.profile().shape_profile_id();
    let families: Vec<(&str, Box<dyn PalwExecutionBackendV1>)> = vec![
        ("floor", Box::new(floor)),
        (
            "A16 v2 dense 8,292",
            Box::new(
                a16_backend(&artifact, &a16, qwen25_a16_held_canonical_v1(a16.n_ctx)).with_attempt_rules(PalwAttemptRulesV1::CoreV1),
            ),
        ),
        (
            "Qwen3.6 held v7",
            Box::new(qwen36_backend(&q36, &held, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1)),
        ),
    ];
    let rules = PalwIdentityRulesV1 { prompt_ids_form: PalwPromptIdsFormV1::MerkleV1, base_class_id: floor_id };
    for (label, backend) in &families {
        let anchor = Hash64::from_u64_word(0xD1F7_0000);
        let (canonical, prompt) = backend.job_for_anchor(anchor).unwrap();
        let job = palw_attempt_job_v1(canonical, true);
        let honest = backend.execute(&job, &prompt).unwrap();
        let honest_rows = misaka_palw_base0::produce::base0_material_decode_any_v1(&honest.material).unwrap().logits_rows().to_vec();
        let top = base0_decode_token_select_v1(&honest_rows[0]) as u32;
        for fault in [
            F::BendLogits { row: 0, mode: PalwDrillBendV1::SameArgmax, lane: None },
            F::RelabelPrompt { from: Hash64::from_u64_word(0x4E1A_D1F7) },
            F::GarbagePromptRoot,
            F::ShortPrefill(prompt.len() as u32 - 1),
            F::LegacyContextField,
            F::ActivationRoot(Hash64::from_u64_word(0xAC70)),
            F::CheckpointInterval(1_000_003),
            F::TokenNotSelected { pos: 0, lane: (top + 1) % 64 },
            F::OutputRoot(Hash64::from_u64_word(0x0A70)),
            F::UnboundExecutionRoot(Hash64::from_u64_word(0xB0B0)),
        ] {
            let out = backend.execute_with_drill_fault(&job, &prompt, fault).unwrap_or_else(|e| panic!("{label}: {fault:?}: {e}"));
            let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).unwrap();
            let binding = retention.binding().clone();
            let (rows, ids) = (retention.logits_rows().to_vec(), retention.generated_token_ids().to_vec());
            let target = PalwOffenceTargetV1 {
                execution_root: out.execution_root,
                trace_root: out.trace_root,
                output_root: out.output_root,
                ..target_of(&binding, anchor)
            };
            let identity = palw_binding_identity_fault_v1(&target, &binding, rules, true);
            let tiled = binding.shape_profile.logits_scheme_id != flat_logits_scheme_id_v1();
            match fault {
                F::BendLogits { .. } => {
                    let head = palw_logits_head_v1(&binding.shape_profile).unwrap();
                    let lane = rows[0].iter().zip(honest_rows[0].iter()).position(|(a, b)| a != b).expect("a bent lane") as u32;
                    let tile = lane / head.tile_len;
                    let logits_tile = if tiled { (lane as usize / PALW_LOGITS_TILE_LANES) as u8 } else { 0 };
                    let event = logits_event_disclosure_v1(&binding, &rows, &ids, 0, logits_tile).unwrap();
                    let coord = palw_logits_head_coordinate_v1(&head, &binding.job_context, 0, tile).unwrap();
                    let index = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord).unwrap();
                    let opening = backend.refutation_for_index(&out.material, index).unwrap().output_opening;
                    assert_eq!(
                        palw_logits_not_step_output_fault_v1(&target, &event, 0, tile, &opening, PALW_STEP_LEG_MAX_LEAVES),
                        Ok(()),
                        "{label}: 12 convicts the bent row"
                    );
                }
                F::RelabelPrompt { .. } | F::GarbagePromptRoot => {
                    assert_eq!(identity, Ok(Some(J::PromptNotTheAnchors)), "{label}: {fault:?} is J5b")
                }
                F::ShortPrefill(_) | F::LegacyContextField => {
                    assert_eq!(identity, Ok(Some(J::ContextNotCanonical)), "{label}: {fault:?} is J5a")
                }
                F::ActivationRoot(_) => assert_eq!(identity, Ok(Some(J::ActivationLegNotCanonical)), "{label}: J6"),
                F::CheckpointInterval(_) => assert_eq!(identity, Ok(Some(J::CheckpointProfileNotCanonical)), "{label}: J7"),
                F::TokenNotSelected { .. } if tiled => {
                    let pin = tiled_decode_pin_v1(&binding.job_context, &rows, &ids, 0, top).unwrap();
                    assert_eq!(
                        palw_forged_output_tiled_fault_v1(
                            &target,
                            &binding,
                            &PalwForgedOutputTiledProofV1::NotSelected { pin },
                            PALW_STEP_LEG_MAX_LEAVES,
                            false
                        ),
                        Ok(()),
                        "{label}: 11 NotSelected"
                    );
                }
                F::TokenNotSelected { .. } => {
                    let forged = C::ForgedOutput {
                        binding: binding.clone(),
                        pin: PalwBase0DecodeTokensV1 { logits_rows: rows.clone(), generated_token_ids: ids.clone() },
                        position: 0,
                    };
                    assert_eq!(
                        palw_panel_contradiction_convicts_execution_v1(
                            &forged,
                            out.execution_root,
                            Hash64::default(),
                            PALW_STEP_LEG_MAX_LEAVES
                        ),
                        Ok(()),
                        "{label}: 8 ForgedOutput on the flat floor"
                    );
                }
                F::OutputRoot(_) => {
                    let rows_root = tiled_logits_rows_root_v1(&binding.job_context, &rows);
                    let pin = match rows_root {
                        Some(rows_root) if tiled => {
                            PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: ids.clone() })
                        }
                        _ => PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 {
                            logits_rows: rows.clone(),
                            generated_token_ids: ids.clone(),
                        }),
                    };
                    assert_eq!(palw_output_fault_v1(&target, &binding, &pin), Ok(true), "{label}: 10");
                }
                F::UnboundExecutionRoot(_) => {
                    assert_eq!(identity, Err(E::PanelFalseValidWorkMismatch), "{label}: no binding reproduces an unbound root")
                }
                _ => unreachable!(),
            }
        }
    }
}
