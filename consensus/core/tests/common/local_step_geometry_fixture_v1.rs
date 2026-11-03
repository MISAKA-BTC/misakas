//! Typed metadata fixture only; no weights, model execution or trace binding.
use super::geometry_core;
use geometry_core::{
    Hash64,
    palw_native_resource_v1::{
        LocalScanWorkBudgetV1, LocalSourceProgramOwnerV1, SourceProgramLocalLimitsV1, scan_source_program_local_v1,
    },
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
};
use misaka_palw_tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_HELD, LOGITS_I64_Q24_SCHEME_V2, MAX_PROGRAM_BYTES},
};
pub fn class(wide: bool, layers: usize) -> PalwTirClassV1 {
    assert!(layers > 0);
    let mut b = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
    let seed = b.konst(DType::I32, &[4], &[1, 2, 3, 4]);
    let fixed = b.fixed_state("global.fixed", DType::I32, &[4], -10, 10, false);
    let hist = b.hist_state("global.hist", DType::I32, &[4], 8, false);
    let per_fixed = b.fixed_state("layer.fixed", DType::I32, &[4], -10, 10, true);
    let per_hist = b.hist_state("layer.hist", DType::I32, &[4], 8, true);
    let mut pre = b.block("pre", vec![]);
    let value = pre.state_write(fixed, seed);
    pre.commit(value);
    let history = pre.hist_append(hist, value);
    pre.commit(history);
    let pre = pre.finish(&[value]);
    let mut layer = b.block("layer", vec![TensorType::fixed(DType::I32, &[4])]);
    let value = layer.state_write(per_fixed, Ref::CarryIn(0));
    layer.commit(value);
    let history = layer.hist_append(per_hist, value);
    layer.commit(history);
    let layer = layer.finish(&[value]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
    let logits = post.cast(Ref::CarryIn(0), if wide { DType::I64 } else { DType::I32 });
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = b.finish(pre, vec![layer; layers], post, logits);
    program.logits_scheme_id =
        if wide { LOGITS_I64_Q24_SCHEME_V2 } else { geometry_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes() };
    PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(18),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 8,
            h_tile: 2,
            commit_tiles: vec![4; 5],
            state_tiles: vec![4; 4],
        },
    }
}
pub fn source(class: &PalwTirClassV1) -> LocalSourceProgramOwnerV1 {
    let mut budget = LocalScanWorkBudgetV1::new(1 << 40).unwrap();
    scan_source_program_local_v1(
        &class.program,
        SourceProgramLocalLimitsV1 {
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_scan_steps: 1 << 32,
            max_nf_items: 1 << 32,
            max_native_requests: 1 << 40,
        },
        &mut budget,
    )
    .expect("requires actual audited Source build; unsupported does not count as geometry success")
    .decode()
    .unwrap()
}
