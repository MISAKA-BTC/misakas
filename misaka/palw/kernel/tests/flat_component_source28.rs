#![allow(warnings)]
use misaka_palw_kernel::flat_component_source28::*;
use misaka_palw_tir::{
    DType, Ref,
    builder::ProgramBuilder,
    program::HISTORY_BOUND_V1_SMALL,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
fn program() -> TirProgramV2 {
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let p = b.param("pixels", DType::I16, &[4], false);
    let (pre, n) = {
        let mut block = b.block("pre", vec![]);
        let y = block.cast(p, DType::I32);
        let Ref::Node(n) = y else { panic!() };
        (block.finish(&[y]), n)
    };
    let carry = vec![b.blocks[pre as usize].nodes[n as usize].out.clone()];
    let (post, n) = {
        let mut block = b.block("post", carry);
        let y = block.cast(Ref::CarryIn(0), DType::I32);
        block.commit(y);
        let Ref::Node(n) = y else { panic!() };
        (block.finish(&[]), n)
    };
    TirProgramV2::from_v1_lifting_params(
        &b.finish(pre, vec![], post, n),
        &[(0, InputSource::External { lo: -5, hi: 5 })],
        OutputDecl::Rows { node: n },
    )
    .unwrap()
}
fn manifest(p: &TirProgramV2) -> CompleteComponentManifestV28 {
    CompleteComponentManifestV28 {
        version: 28,
        repository: "publisher/model".into(),
        revision: [1; 20],
        original_task_metadata: [2; 64],
        original_required_components: vec!["vision".into()],
        source_files: vec![OriginalFilePinV28 { path: "model.safetensors".into(), bytes: 100, sha256: [3; 32] }],
        components: vec![SourceComponentV28 { name: "vision".into(), source_files: vec![0], stages: vec![0] }],
        stages: vec![ComponentStageV28 {
            original_program: stage_program_root_v28(p),
            original_inventory: [4; 64],
            input_declarations: stage_input_declarations_root_v28(p),
            output_declaration: stage_output_declaration_root_v28(p),
            original_context_max: 1,
            lowered_context_max: 1,
        }],
        inputs: vec![ComponentInputBindingV28 {
            stage: 0,
            input: 0,
            source: ComponentInputSourceV28::External { job_input: 0, declaration: [5; 64] },
        }],
    }
}
#[test]
fn complete_original_inputs_components_and_context_are_exact_and_not_held_ceiling() {
    let p = program();
    let m = manifest(&p);
    let check = |m: &CompleteComponentManifestV28| {
        check_component_manifest_v28(m, std::slice::from_ref(&p), &[[4; 64]], &[1], &["vision".into()], [2; 64], &manifest(&p).inputs)
    };
    let good = check(&m).unwrap();
    assert_eq!(good.root(), m.root());
    assert!(p.history_bound > 1);
    let mut short = m.clone();
    short.components.clear();
    assert!(check(&short).is_err());
    let mut wrong = m.clone();
    wrong.stages[0].lowered_context_max = 2;
    assert!(check(&wrong).is_err());
    let mut omitted = m.clone();
    omitted.inputs.clear();
    assert!(check(&omitted).is_err());
    let mut borrowed = m.clone();
    borrowed.stages[0].original_inventory = [7; 64];
    assert!(check(&borrowed).is_err());
    let mut orphan = m.clone();
    orphan.source_files.push(OriginalFilePinV28 { path: "projector.safetensors".into(), bytes: 1, sha256: [6; 32] });
    assert!(check(&orphan).is_err());
}
#[test]
fn bounded_component_manifest_rejects_truncation_wrong_namespace_and_overflow_counts() {
    let bytes = manifest(&program()).to_bytes().unwrap();
    assert_eq!(CompleteComponentManifestV28::from_bytes(&bytes).unwrap().version, 28);
    for end in 0..bytes.len() {
        assert!(CompleteComponentManifestV28::from_bytes(&bytes[..end]).is_err());
    }
    let mut old = bytes.clone();
    old[0] = 14;
    assert!(CompleteComponentManifestV28::from_bytes(&old).is_err());
    let mut oversized = vec![28, 0];
    oversized.extend(u32::MAX.to_le_bytes());
    assert!(CompleteComponentManifestV28::from_bytes(&oversized).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(CompleteComponentManifestV28::from_bytes(&extra).is_err());
}
