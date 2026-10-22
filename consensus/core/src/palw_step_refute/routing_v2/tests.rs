//! Source controls only; execution waits for the consolidated validation phase.
use super::*;
use crate::palw_step_leg as leg;
struct NoWeights;
impl PalwWeightOracleV1 for NoWeights {
    fn operand_bytes(&self, _: &str, _: Option<u16>, _: u32, _: u32) -> Option<Vec<u8>> {
        None
    }
}

fn fixture(
    wrong: bool,
) -> (leg::PalwStepBindingV2, PalwReaderRouterDeclarationV2, Vec<Hash64>, Vec<(u64, leg::PalwStepTileLeafV1)>, PalwStepCoordinateV1) {
    let (mut profile, mut declaration) = crate::palw_reader_router_v2::tests::profile_and_mapping(true);
    for n in &mut profile.gdn_nodes {
        n.tile_len = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
    }
    // The first score row also supplies this fixture's two shared-expert rows.
    profile.gdn_nodes[0].out_len = crate::palw_step::PalwStepOutLenV1::Fixed { elements: 16 };
    declaration.class_id = profile.shape_profile_id();
    declaration.check_profile(&profile).unwrap();
    profile.validate_shape().unwrap();
    let mut binding = crate::palw_step_refute::tests::skeleton_refutation().binding;
    binding.shape_profile = profile;
    binding.job_context.shape_profile_id = declaration.class_id;
    let p = &binding.shape_profile;
    let first = p.pre_nodes.len();
    let mut rows: Vec<Vec<u32>> = (0..p.global_node_count())
        .map(|slot| {
            let (n, _) = p.resolve_node_slot(slot).unwrap();
            let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = n.out_len else {
                panic!("fixed fixture");
            };
            vec![1; elements as usize]
        })
        .collect();
    rows[first + 1] = vec![0, 1, 1 << 15, 1 << 15];
    rows[first + 3] = vec![0, 1, 2, 3, 1 << 14, 1 << 14, 1 << 14, 1 << 14];
    for (reader, router, output) in [(4, 1, 6), (5, 3, 7)] {
        let r = &rows[first + router];
        let k = r.len() / 2;
        let weights: Vec<_> =
            (0..k).map(|i| crate::palw_kimi_k3_ops::KimiK3RoutedExpert { expert: r[i] as u16, weight_q: r[k + i] as i32 }).collect();
        let values = crate::palw_kimi_k3_ops::kimi_k3_moe_combine(
            &rows[first + reader].iter().map(|x| *x as i32).collect::<Vec<_>>(),
            &rows[first].iter().map(|x| *x as i32).collect::<Vec<_>>(),
            &weights,
            8,
        )
        .unwrap();
        rows[first + output] = values.into_iter().map(|x| x as u32).collect();
    }
    if wrong {
        rows[first + 6][0] = rows[first + 6][0].wrapping_add(1);
    }
    let mut builder = leg::PalwStepLegBuilderV1::new(binding.job_context.clone(), p.clone()).unwrap();
    let count = builder.expected_main_leaves();
    let mut captured = Vec::new();
    for index in 0..count {
        let coord = crate::palw_step::canonical_step_coordinates(p, &binding.job_context, index).unwrap();
        let (node, _) = p.resolve_node_slot(coord.node_slot).unwrap();
        let row = &rows[coord.node_slot as usize];
        let from = coord.tile_index as usize * node.tile_len as usize;
        let to = (from + node.tile_len as usize).min(row.len());
        builder.push_step_tile(coord, &row[from..to]).unwrap();
        captured.push((
            index,
            leg::PalwStepTileLeafV1 {
                version: leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
                coord,
                value_count: (to - from) as u32,
                values_le: row[from..to].iter().flat_map(|x| x.to_le_bytes()).collect(),
            },
        ));
    }
    let material = builder.finish().unwrap();
    binding.step_leaf_count = material.leaf_count;
    binding.step_merkle_root = material.merkle_root;
    crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
    let target = PalwStepCoordinateV1 { call_index: 0, node_slot: (first + 6) as u32, position: 0, tile_index: 0 };
    (binding, declaration, material.leaf_hashes, captured, target)
}

#[test]
fn real_capture_selects_its_own_router_and_reaches_direct_arithmetic() {
    for wrong in [false, true] {
        let (binding, declaration, hashes, tiles, target) = fixture(wrong);
        let owner = routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, 1 << 26).unwrap();
        let verdict = check_routed_step_refutation_v2(&owner, binding.committed_execution_root, &declaration, &NoWeights, 1 << 26);
        if wrong {
            assert!(verdict.is_ok());
        } else {
            assert!(matches!(verdict, Err(PalwRoutingCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))));
        }
        // A different router does not become a second interpretation of the
        // authenticated explicit combine ref, even with identical row bytes.
        let mut foreign = declaration.clone();
        foreign.bindings[2].router_slot = foreign.bindings[1].router_slot;
        assert!(check_routed_step_refutation_v2(&owner, binding.committed_execution_root, &foreign, &NoWeights, 1 << 26).is_err());
        let mut altered = (*owner).clone();
        altered.step.inputs[0].preimages[0].values_le[0] ^= 1;
        assert!(matches!(
            check_routed_step_refutation_v2(&altered, binding.committed_execution_root, &declaration, &NoWeights, 1 << 26),
            Err(_)
        ));
        let gathered = routed_leaf_evidence_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, &[], 1 << 26).unwrap();
        assert!(gathered.artifact_openings.is_empty(), "Kimi combine requests no model parameter rows");
    }
}

#[test]
fn missing_duplicate_foreign_tiles_and_unrelated_carriage_never_select_a_fallback() {
    let (binding, declaration, hashes, tiles, target) = fixture(false);
    let built = routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, 1 << 26).unwrap();
    let mut duplicate = tiles.clone();
    duplicate.push(tiles[0].clone());
    // Duplicate an actually-required tile; unreferenced capture rows have no authority.
    let required = canonical_reader_inputs_v2(&binding.shape_profile, &binding.job_context, &target, &declaration).unwrap();
    let at = required[0][0].0;
    duplicate.push(tiles.iter().find(|(i, _)| *i == at).unwrap().clone());
    assert!(routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &duplicate, 1 << 26).is_err());
    let mut unrelated = (*built).clone();
    unrelated.step.prompt_token_ids.push(1);
    assert!(check_routed_step_refutation_v2(&unrelated, binding.committed_execution_root, &declaration, &NoWeights, 1 << 26).is_err());
    assert!(check_routed_step_refutation_v2(&built, Hash64::from_u64_word(9), &declaration, &NoWeights, 1 << 26).is_err());
}

#[test]
fn routed_retrieval_authenticates_committed_bytes_without_a_merits_verdict() {
    for wrong in [false, true] {
        let (binding, declaration, hashes, tiles, target) = fixture(wrong);
        let evidence = routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, 1 << 26).unwrap();
        check_routed_step_committed_half_v2(&evidence, binding.committed_execution_root, &declaration, 1 << 26).unwrap();
        let mut missing = (*evidence).clone();
        missing.step.inputs.pop();
        assert!(check_routed_step_committed_half_v2(&missing, binding.committed_execution_root, &declaration, 1 << 26).is_err());
    }
    let (mut binding, declaration, mut hashes, mut tiles, target) = fixture(false);
    let index = crate::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &target).unwrap();
    let (_, output) = tiles.iter_mut().find(|(at, _)| *at == index).unwrap();
    output.coord.node_slot = 0; // committed but structurally faulty output
    hashes[index as usize] = leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &declaration.class_id, output);
    binding.step_merkle_root = leg::step_merkle_root_capped_v1(&hashes, 1 << 26).unwrap();
    crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
    let evidence = routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, 1 << 26).unwrap();
    check_routed_step_committed_half_v2(&evidence, binding.committed_execution_root, &declaration, 1 << 26).unwrap();
    assert!(check_routed_step_refutation_v2(&evidence, binding.committed_execution_root, &declaration, &NoWeights, 1 << 26).is_ok());
}

#[test]
fn routed_answer_signature_domain_binds_target_generation_and_receipt() {
    use crate::palw_routed_material_v2::{PalwRoutedMaterialAnswerV2, PalwRoutedMaterialReceiptV2, PalwRoutedMaterialTargetV2};
    let (binding, declaration, hashes, tiles, target) = fixture(false);
    let evidence = routed_refutation_from_capture_v2(&binding, target, &declaration, &hashes, &tiles, 1 << 26).unwrap();
    let h = Hash64::from_u64_word;
    let mut answer = PalwRoutedMaterialAnswerV2 {
        version: 2,
        claim: h(1),
        provider: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(2), 0)),
        target: PalwRoutedMaterialTargetV2::Monetary { root: h(3), consent: h(4) },
        refutation: (*evidence).clone(),
        receipt: Some(PalwRoutedMaterialReceiptV2 { stream: h(5), sequence: 1, previous: h(6), challenge_deadline_daa: 10 }),
        signature: Vec::new(),
    };
    let id = answer.message(h(7));
    answer.receipt.as_mut().unwrap().challenge_deadline_daa += 1;
    assert_ne!(id, answer.message(h(7)));
    answer.receipt.as_mut().unwrap().challenge_deadline_daa -= 1;
    answer.target = PalwRoutedMaterialTargetV2::Monetary { root: h(3), consent: h(8) };
    assert_ne!(id, answer.message(h(7)));
    answer.target = PalwRoutedMaterialTargetV2::DaKernelWitness;
    assert_ne!(id, answer.message(h(7)));
}
