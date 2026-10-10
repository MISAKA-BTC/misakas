//! A separate verifier consumes only the recorded actual-weight public witness. No producer
//! process, private capture, full weights or ModelSpec is available to this test. This is a
//! terminal-court replay, not public RPC acquisition, chain registration, G14 or Final.
use misaka_palw_kernel::element::{SegClaimContextV1, SegConvictionKindV1, SegFaultV1, verify_seg_fault_v1};
use misaka_palw_kernel::verify::DismissalV1;
use misaka_palw_sdk::kernel_execution::DecodeCourtBundleV3;

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
