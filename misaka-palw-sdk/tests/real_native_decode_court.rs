//! A separate verifier consumes only the recorded actual-weight public witness. No producer
//! process, private capture, full weights or ModelSpec is available to this test. This is a
//! terminal-court replay, not public RPC acquisition, chain registration, G14 or Final.
use misaka_palw_kernel::element::{SegClaimContextV1, SegConvictionKindV1, SegFaultV1, verify_seg_fault_v1};
use misaka_palw_kernel::verify::DismissalV1;
use misaka_palw_sdk::kernel_execution::DecodeCourtBundleV3;

#[test]
fn public_actual_weight_record_binds_final_delivery_and_refuses_mixed_claim_material() {
    use misaka_palw_kernel::evidence::{EvidenceHeaderV1, SuiteParamsV1};
    use misaka_palw_kernel::job::KernelClaimV1;
    use misaka_palw_kernel::seg::{SEG_EVIDENCE_VERSION_V2, SEG_LEN_V4, SegmentedEvidenceV2, claim_root_v1, job_input_root_v2};
    use misaka_palw_kernel::seg_ledger::SegmentedClaimRecordV1;
    let bundle: DecodeCourtBundleV3 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-native-court-bundle.borsh"
    )))
    .unwrap();
    let header = EvidenceHeaderV1 {
        network_domain: [1; 64],
        ruleset_digest: [2; 64],
        class_binding_id: [3; 64],
        program_root: misaka_palw_kernel::public::program_root_v1(&bundle.program_bytes),
        artifact_root: bundle.params.root(),
        plan_root: bundle.plan.root(),
    };
    let roots = misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&bundle.position_roots);
    let prompt_root = misaka_palw_kernel::seg::prompt_root_of_ids_v1(&bundle.tokens);
    let evidence = SegmentedEvidenceV2 {
        version: SEG_EVIDENCE_VERSION_V2,
        header,
        job_input_root: job_input_root_v2(32, &prompt_root, &[]),
        positions: 32,
        segment_len: SEG_LEN_V4,
        claim_root: claim_root_v1(32, &roots),
        suite: SuiteParamsV1::of(&misaka_palw_kernel::descriptor::k2_tir_v4_descriptor()),
    };
    let claim = KernelClaimV1 { job_id: [4; 64], producer_bond: [5; 64], generated: vec![2], evidence_root: evidence.root() };
    let record = SegmentedClaimRecordV1 {
        claim_id: claim.id(),
        program_bytes: bundle.program_bytes,
        plan: bundle.plan,
        param_commitments: bundle.params.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
        evidence,
        segment_roots: roots,
        job_id: claim.job_id,
        prompt_len: 32,
        prompt_root,
        inline_prompt: Some(bundle.tokens),
        max_new_tokens: 1,
        decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
        generated: claim.generated,
    };
    let id = record.claim_id;
    let producer = [5; 64];
    let check = |r: &SegmentedClaimRecordV1| r.view_for_claim(&header, &id, &producer);
    let view = check(&SegmentedClaimRecordV1::from_bytes(&record.to_bytes()).unwrap()).unwrap();
    assert!(misaka_palw_kernel::element::verify_seg_fault_v1(&view.context(), &bundle.fault).is_ok());
    let mut altered = record.clone();
    altered.generated[0] = 1;
    assert!(altered.view(&header).is_ok(), "final delivery does not participate in trace/input roots");
    assert!(check(&altered).is_err(), "the requested claim identity binds the final delivered token");
    let mut altered = record.clone();
    altered.job_id[0] ^= 1;
    assert!(check(&altered).is_err());
    let mut altered = record.clone();
    altered.claim_id[0] ^= 1;
    assert!(check(&altered).is_err());
    assert!(record.view_for_claim(&header, &id, &[6; 64]).is_err());
    let mut altered = record.clone();
    altered.inline_prompt.as_mut().unwrap()[0] ^= 1;
    assert!(check(&altered).is_err(), "inline input must authenticate to the evidence's prompt root");
    let mut altered = record.clone();
    altered.max_new_tokens = 2;
    assert!(check(&altered).is_err(), "a shorter completion cannot satisfy a longer paid job");
    let mut altered = record.clone();
    altered.param_commitments.push(altered.param_commitments[0]);
    assert!(check(&altered).is_err(), "duplicate parameter records cannot shadow the canonical map");
    let mut altered = record.clone();
    altered.param_commitments.swap(0, 1);
    assert!(check(&altered).is_err());
    let mut altered = record.clone();
    altered.prompt_len = 0;
    assert!(check(&altered).is_err());
    let mut altered = record.clone();
    altered.evidence.claim_root[0] ^= 1;
    assert!(check(&altered).is_err());
}

#[test]
fn recorded_real_32_position_trace_convicts_substituted_delivery_from_public_witness_alone() {
    let bytes =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/design/palw/tir/evidence/qwen25-real-native-court-bundle.borsh"));
    assert_eq!(bytes.len(), 161_675);
    let bundle: DecodeCourtBundleV3 = borsh::from_slice(bytes).unwrap();
    assert_eq!(bundle.version, 1);
    assert_eq!(bundle.tokens.len(), 32);
    assert_eq!(bundle.position_roots.len(), 32);
    assert_eq!(bundle.honest_generated, [1]);
    assert_eq!(bundle.wrong_generated, [2]);
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&bundle.program_bytes).unwrap();
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let root = misaka_palw_kernel::public::program_root_v1(&bundle.program_bytes);
    assert_eq!(bundle.plan, misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &program, root, 32).unwrap());
    assert_eq!(bundle.params.by_instance.len(), 1636);
    let params: misaka_palw_kernel::trace::ParamCommitmentsV1 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-params-v3-commitments.borsh"
    )))
    .unwrap();
    assert_eq!(bundle.params, params);
    assert_ne!(bundle.inventory_root, bundle.params.root(), "inventory and kernel parameter roots have distinct domains");
    let roots = misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&bundle.position_roots);
    let mut context = SegClaimContextV1 {
        program: &program,
        params: &bundle.params,
        segment_roots: &roots,
        positions: 32,
        prompt_len: 32,
        prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&bundle.tokens),
        inline_prompt: Some(&bundle.tokens),
        generated: &bundle.wrong_generated,
        decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
        encoder: None,
    };
    let wire = bundle.fault.to_bytes();
    assert_eq!(wire.len(), 17_924);
    let fault = SegFaultV1::from_bytes(&wire).unwrap();
    let conviction = verify_seg_fault_v1(&context, &fault).unwrap();
    assert_eq!(conviction.position, 31);
    assert_eq!(conviction.kind, SegConvictionKindV1::Decode { index: 0 });
    context.generated = &bundle.honest_generated;
    let mut weaker = fault.clone();
    let SegFaultV1::Decode(f) = &mut weaker else { panic!("not a decode proof") };
    f.rival = bundle.wrong_generated[0];
    assert_eq!(verify_seg_fault_v1(&context, &weaker), Err(DismissalV1::NoFault));
    context.generated = &bundle.wrong_generated;
    let mut copied = fault.clone();
    let SegFaultV1::Decode(f) = &mut copied else { unreachable!() };
    f.logits.node.as_mut().unwrap().position = 30;
    assert!(matches!(verify_seg_fault_v1(&context, &copied), Err(DismissalV1::NotAuthentic(_))));
    let mut tampered = fault.clone();
    let SegFaultV1::Decode(f) = &mut tampered else { unreachable!() };
    f.logits.node.as_mut().unwrap().commitment[0] ^= 1;
    assert!(matches!(verify_seg_fault_v1(&context, &tampered), Err(DismissalV1::NotAuthentic(_))));
}
