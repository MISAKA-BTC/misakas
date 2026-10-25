//! Public original-root identity fixtures, not accepted claims/model results.
use super::super::{PalwTirPublicFilerBookV1, apply_owner_cone_result_v2, cone_owned_consumer_tests, harvest};
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::*,
    palw_step_leg::*,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicSourceV1},
    palw_tir_step_v1::{PalwTirStepSpaceV1, palw_tir_execution_root_v1},
};
fn fixture() -> (
    PalwTirStepBindingV1,
    Hash64,
    Vec<Arc<SharedMaterialV2<PalwTirPublicAuthenticatedUnitV1>>>,
    Arc<SharedMaterialV2<PalwTirPublicAuthenticatedIdsV1>>,
) {
    let (original, mut hashes, rows_root, ids, record, layout) = harvest::tests::fixture_v2();
    let mut header = original.header_v1().clone();
    let class = PalwTirClassV1 {
        version: 1,
        program: record.program.as_ref().clone(),
        layout: layout.clone(),
        tokenizer_id: record.tokenizer_id,
    };
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let preimages = (0..2)
        .map(|i| {
            let leaf = space.leaf_at(&header.job_context, i).unwrap();
            PalwStepTileLeafV1 {
                version: 1,
                coord: leaf.coord,
                value_count: leaf.value_count,
                values_le: vec![0; leaf.value_count as usize * 4],
            }
        })
        .collect::<Vec<_>>();
    for (i, p) in preimages.iter().enumerate() {
        hashes[i] = step_tile_leaf_hash_v1(&header.job_context.context_hash(), &record.facts.class_id, p);
    }
    header.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
    header.committed_execution_root = palw_tir_execution_root_v1(
        &header.job_context.context_hash(),
        &header.full_logits_trace_root,
        &record.facts.class_id,
        header.step_leaf_count,
        &header.step_merkle_root,
    );
    let source = PalwTirPublicSourceV1::new_v1(
        &record,
        &layout,
        &header,
        header.committed_execution_root,
        header.full_logits_trace_root,
        1 << 20,
    )
    .unwrap();
    let leaf = source
        .authenticate_response_owned_v2(
            &Q::StepLeaf { index: 0 },
            &PalwTirPublicResponseV1::StepLeaf { preimage: preimages[0].clone(), opening: step_opening_v1(&hashes, 0).unwrap() },
            None,
        )
        .unwrap();
    let run = source
        .authenticate_response_owned_v2(
            &Q::StepRun { first: 0, count: 2 },
            &PalwTirPublicResponseV1::StepRun {
                preimages,
                range: PalwStepRangeOpeningV1 {
                    first_leaf_index: 0,
                    leaf_hashes: hashes[..2].to_vec(),
                    siblings: step_merkle_range_siblings_v1(&hashes, 0, 2).unwrap(),
                },
            },
            None,
        )
        .unwrap();
    let body = kaspa_consensus_core::palw_tir_public_owned_v1::palw_tir_public_ids_body_v1(&source, rows_root, &ids).unwrap();
    let ids = source.decode_complete_ids_body_native_v2(&body).unwrap();
    let mut compact = header.resolve_registered_layout_v1(&record, &layout, header.committed_execution_root, 1 << 20).unwrap();
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut compact);
    (
        compact,
        source.source_id_v1(),
        vec![SharedMaterialV2::share(leaf).unwrap(), SharedMaterialV2::share(run).unwrap()],
        SharedMaterialV2::share(ids).unwrap(),
    )
}
#[test]
fn tir_cone_derived_owned_real_copy_quota_retry_and_native_case_disposition() {
    const CHILD: &str = "MISAKA_CONE_DERIVED_COPY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_cone_derived_owned_real_copy_quota_retry_and_native_case_disposition", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let (compact, id, units, ids) = fixture();
    let (mut case, _host) = cone_owned_consumer_tests::case();
    let mut filer = PalwTirPublicFilerBookV1::default();
    let claim = case.claim;
    const LIMIT: u64 = 1 << 28;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let (count, q) = OwnedConeDerivedV2::requests_v2(&compact, &units, Some(&ids)).unwrap();
    assert_eq!(count, 2);
    assert!(q > 0 && q < LIMIT);
    let block = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    STAGES.with(|s| s.set((0, 0)));
    let fault = match OwnedConeDerivedV2::prepare_v2(id, &compact, &units, Some(&ids), 1 << 20) {
        Err(why) => why,
        Ok(_) => panic!("Q-1 must refuse before copies"),
    };
    assert!(matches!(&fault, Fault::LocalResource(_)));
    assert_eq!(STAGES.with(std::cell::Cell::get), (1, 0));
    assert!(apply_owner_cone_result_v2(&mut case, &mut filer, claim, 100, Err(fault)).is_err());
    assert!(!case.done && case.harvested.is_empty() && filer.len() == 0);
    drop(block);
    let exact = reserve_native_requests_v1(LIMIT - q).unwrap();
    let derived = OwnedConeDerivedV2::prepare_v2(id, &compact, &units, Some(&ids), 1 << 20).unwrap();
    assert_eq!(STAGES.with(std::cell::Cell::get), (2, 1));
    assert_eq!(derived.runs.len(), 2);
    assert_eq!(derived.runs[0].preimages.len(), 1);
    assert_eq!(derived.runs[1].preimages.len(), 2);
    assert_eq!(derived.intervals, vec![(0, 1), (0, 2)]);
    assert_eq!(derived.trace.as_ref().unwrap().generated_token_ids.as_slice(), ids.ids_v1());
    for run in &derived.runs {
        assert_eq!(run.binding, compact);
        assert!(run.binding.class.program.is_empty());
        assert_eq!(step_range_opening_root_capped_v1(compact.step_leaf_count, &run.range, 1 << 20).unwrap(), compact.step_merkle_root);
    }
    assert!(reserve_native_requests_v1(1).is_err());
    drop(derived);
    drop(exact);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
    assert!(matches!(
        OwnedConeDerivedV2::prepare_v2(Hash64::default(), &compact, &units, Some(&ids), 1 << 20),
        Err(Fault::Unavailable(_))
    ));
    let mut wrong = compact.clone();
    wrong.step_merkle_root = Hash64::from_u64_word(0xAB);
    assert!(matches!(OwnedConeDerivedV2::prepare_v2(id, &wrong, &units, Some(&ids), 1 << 20), Err(Fault::Unavailable(_))));
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
    assert!(!case.done && case.harvested.is_empty() && filer.len() == 0);
}
