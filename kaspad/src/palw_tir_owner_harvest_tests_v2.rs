//! Actual OwnerCase publication and publicly authenticated root/material identities.
//! These identity fixtures assert no accepted claim or honest model execution.
use super::super::{PalwTirPublicRequestV1, PalwTirPublicResponseV1, PalwTirPublicSourceV1, cone_owned_consumer_tests};
use super::*;
use kaspa_consensus_core::{
    palw_tir_class_v1::*,
    palw_tir_public_source_v1::PalwTirPublicIdsAssemblerV1,
    palw_tir_step_v1::{PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1},
};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn material() -> (PalwTirPublicFinishedV1, PalwTirPublicFinishedV1) {
    use misaka_palw_tir::{
        builder::ProgramBuilder,
        program::{HISTORY_BOUND_V1_SMALL, Ref},
        types::{DType, TensorType},
    };
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 4], false);
    let w = pb.param("w", DType::I8, &[4, 4], true);
    let head = pb.param("head", DType::I8, &[8, 4], false);
    let sum = pb.fixed_state("sum", DType::I32, &[4], -1000, 1000, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let s = b.add(Ref::State(sum), Ref::CarryIn(0), DType::I64);
        let s = b.state_write(sum, s);
        let s = b.reshape_fixed(s, &[4, 1]);
        let y = b.matmul(w, s, DType::I32);
        let y = b.reshape_fixed(y, &[4]);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 4,
            checkpoint_interval: 2,
            h_tile: 1,
            commit_tiles: vec![4, 4, 4096],
            state_tiles: vec![4],
        },
        tokenizer_id: h(5),
    };

    let artifact = h(6);
    let class_id = class.class_id(&artifact);
    let zero = Hash64::default();
    let context = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: h(7),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [0; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class_id,
        trace_scheme_id: kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
        cu_ruleset_id: zero,
        tokenizer_id: class.tokenizer_id,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&[1, 6, 3]),
        declared_prefill_tokens: 3,
        exact_decode_tokens: 2,
        // Context budgets include all3 prompt +2 generated tokens. The layout
        // below separately covers4 executed positions (P + D -1).
        max_context_tokens: 5,
    };
    let maximum = 1 << 20;
    let count = PalwTirStepSpaceV1::new(&class).unwrap().leaf_count_capped(&context, maximum).unwrap();
    let hashes = (0..count).map(h).collect::<Vec<_>>();
    let root = kaspa_consensus_core::palw_step_leg::step_merkle_root_v1(&hashes).unwrap();
    let rows_root = h(8);
    let ids = vec![1, 2];
    let trace = kaspa_consensus_core::palw_step_refute::tiled_logits_outer_root_v1(&context, 2, &rows_root, &ids);
    let binding = PalwTirStepBindingV1 {
        version: 1,
        committed_execution_root: palw_tir_execution_root_v1(&context.context_hash(), &trace, &class_id, count, &root),
        job_context: context,
        class: class.clone(),
        artifact_root: artifact,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: root,
    };
    let (record, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).unwrap();
    let header = kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&binding);
    let source =
        PalwTirPublicSourceV1::new_v1(&record, &class.layout, &header, binding.committed_execution_root, trace, maximum).unwrap();
    let (frontier, siblings) = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&hashes, 1, 0).unwrap();
    let unit = source
        .authenticate_v1(
            &PalwTirPublicRequestV1::StepNode { level: 1, index: 0 },
            &PalwTirPublicResponseV1::StepNode { frontier, siblings },
            None,
        )
        .unwrap();
    let ids = PalwTirPublicIdsAssemblerV1::default()
        .push_v1(&source, &PalwTirPublicRequestV1::Ids { offset: 0 }, &PalwTirPublicResponseV1::Ids { rows_root, ids })
        .unwrap()
        .unwrap();
    (PalwTirPublicFinishedV1::Unit(unit), PalwTirPublicFinishedV1::Ids(ids))
}
#[test]
fn tir_owner_harvest_publishes_units_ids_once_and_deduplicates_requests() {
    let (mut case, _host) = cone_owned_consumer_tests::case();
    let (unit, ids) = material();
    let unit_again = match &unit {
        PalwTirPublicFinishedV1::Unit(unit) => PalwTirPublicFinishedV1::Unit(unit.clone()),
        _ => unreachable!(),
    };
    assert!(harvest_accepted_v2(&mut case, h(1), || Ok(Some(unit))).unwrap());
    assert_eq!(case.units.len(), 1);
    assert!(case.ids.is_none());
    assert!(case.harvested.contains(&h(1)));
    assert!(!harvest_accepted_v2(&mut case, h(1), || panic!("duplicate must not re-read")).unwrap());
    assert!(harvest_accepted_v2(&mut case, h(2), || Ok(Some(unit_again))).unwrap());
    assert_eq!(case.units.len(), 1, "same request from another accepted reference stays deduplicated");
    assert!(harvest_accepted_v2(&mut case, h(3), || Ok(Some(ids))).unwrap());
    assert_eq!(case.ids.as_ref().unwrap().ids_v1(), &[1, 2]);
    assert_eq!(case.harvested.len(), 3);
    assert!(!case.done);
    assert_eq!(case.demands, 0);
}
#[test]
fn tir_owner_harvest_failed_read_stays_retryable_until_publication() {
    const CHILD: &str = "MISAKA_TIR_OWNER_HARVEST_QUOTA_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_owner_harvest_failed_read_stays_retryable_until_publication", module_path!());
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
    use kaspa_consensus_core::palw_native_resource_v1::*;
    let (mut case, _host) = cone_owned_consumer_tests::case();
    let (_, ids) = material();
    let id = h(9);
    assert!(!harvest_accepted_v2(&mut case, id, || Ok(None)).unwrap());
    assert!(harvest_accepted_v2(&mut case, id, || Err("semantic material refusal".into())).is_err());
    assert!(case.harvested.is_empty() && case.units.is_empty() && case.ids.is_none() && !case.done);
    const LIMIT: u64 = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(LIMIT).unwrap();
    let mut reads = 0;
    let fault = harvest_accepted_v2(&mut case, id, || {
        reads += 1;
        let _read = reserve_native_requests_v1(1).map_err(|e| format!("{e:?}"))?;
        Ok(None)
    });
    assert!(fault.is_err());
    assert_eq!(reads, 1);
    assert!(case.harvested.is_empty() && case.units.is_empty() && case.ids.is_none() && !case.done);
    drop(blocker);
    assert!(
        harvest_accepted_v2(&mut case, id, || {
            reads += 1;
            let _read = reserve_native_requests_v1(1).map_err(|e| format!("{e:?}"))?;
            Ok(Some(ids))
        })
        .unwrap()
    );
    assert!(!harvest_accepted_v2(&mut case, id, || panic!("successful retry is consumed once")).unwrap());
    assert_eq!(reads, 2);
    assert_eq!(case.ids.as_ref().unwrap().ids_v1(), &[1, 2]);
    assert!(case.units.is_empty() && !case.done);
    assert_eq!(case.harvested.len(), 1);
    assert_eq!(case.demands, 0);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
