//! Generic preparation of segmented decoder/encoder parameter roots from artifact byte ranges.
//! No ModelSpec, config, model name or compiler registry is consulted. These are newly computed
//! commitments, not proof that this file is the registered or faithful source model. A verifier
//! must compare the resulting root to its authenticated class binding. Bounds apply to off-chain
//! preparation; they neither alter node admission nor make a dormant kernel active.

use crate::tir_stream::PalwTirRangeSourceV1;
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, is_encoder_v1, is_segmented_v1};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::merkle3_stream::{streamed_commitment_shape_v3, tensor_commitment_streamed_v3};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{PalwTirContainerV1, param_instances_v1};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct KernelParamsPreparedV3 {
    pub descriptor_digest: Digest,
    pub program_root: Digest,
    pub params: ParamCommitmentsV1,
    pub model_param_declarations: u16,
    pub payload_bytes: u64,
    /// Kernel hash buffers/states only, not program/instance metadata, reader cache or process RSS.
    pub max_tensor_workspace_bytes: u64,
}

/// Check every model instance's workspace and aggregate payload before reading any of them.
/// v5 excludes the job-bound ids/count explicitly through the descriptor's memory model; v4
/// treats all declared parameters as model parameters. Progress fires before each instance.
pub fn prepare_kernel_params_v3(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    source: &dyn PalwTirRangeSourceV1,
    tensor_workspace_limit: u64,
    payload_limit: u64,
    progress: &mut dyn FnMut(u16, Option<u16>, u64),
) -> Result<KernelParamsPreparedV3, String> {
    if !is_segmented_v1(descriptor) {
        return Err("parameter preparation requires a segmented v3-commitment kernel".into());
    }
    if !matches!(descriptor.memory_model_id, 1 | misaka_palw_kernel::descriptor::MEMORY_MODEL_JOB_INPUTS_V1) {
        return Err("parameter preparation cannot silently assume an unknown memory model".into());
    }
    descriptor.well_formed().map_err(|e| format!("descriptor: {e:?}"))?;
    misaka_palw_tir::validate::validate(program).map_err(|e| e.to_string())?;
    let model_count = if is_encoder_v1(descriptor) {
        misaka_palw_kernel::seg_encoder::encoder_binding_v1(program)?.first_input as usize
    } else {
        program.params.len()
    };
    let instances = param_instances_v1(program);
    let mut payload_bytes = 0u64;
    let mut max_tensor_workspace_bytes = 0;
    for (j, d) in program.params[..model_count].iter().enumerate() {
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let bound = streamed_commitment_shape_v3(d.dtype, &shape)?;
        if bound.workspace_bytes > tensor_workspace_limit {
            return Err(format!("param {j}: hash workspace {} > limit {tensor_workspace_limit}", bound.workspace_bytes));
        }
        max_tensor_workspace_bytes = max_tensor_workspace_bytes.max(bound.workspace_bytes);
        payload_bytes = payload_bytes
            .checked_add(bound.tensor_bytes.checked_mul(instances[j].len() as u64).ok_or("model payload overflows")?)
            .ok_or("model payload overflows")?;
    }
    if payload_bytes > payload_limit {
        return Err(format!("model payload {payload_bytes} > limit {payload_limit}"));
    }
    let mut by_instance = BTreeMap::new();
    for (j, d) in program.params[..model_count].iter().enumerate() {
        let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        for &layer in &instances[j] {
            let j = j as u16;
            let bytes = streamed_commitment_shape_v3(d.dtype, &shape)?.tensor_bytes;
            progress(j, layer, bytes);
            let (root, _) = tensor_commitment_streamed_v3(d.dtype, &shape, tensor_workspace_limit, &mut |offset, out| {
                source.read_range(j, layer, offset..offset + out.len() as u64, out)
            })
            .map_err(|e| format!("param {j}, layer {layer:?}: {e}"))?;
            by_instance.insert((j, layer), root);
        }
    }
    Ok(KernelParamsPreparedV3 {
        descriptor_digest: descriptor.digest(),
        program_root: misaka_palw_kernel::public::program_root_v1(&program.encode()),
        params: ParamCommitmentsV1 { by_instance },
        model_param_declarations: model_count as u16,
        payload_bytes,
        max_tensor_workspace_bytes,
    })
}

/// One open payload file descriptor; positional reads fill the kernel's <=64-KiB buffer directly,
/// without a second read-ahead buffer or decoding the complete tensor.
pub fn prepare_kernel_params_file_v3(
    descriptor: &KernelDescriptorV1,
    path: &Path,
    tensor_workspace_limit: u64,
    payload_limit: u64,
    progress: &mut dyn FnMut(u16, Option<u16>, u64),
) -> Result<KernelParamsPreparedV3, String> {
    struct Direct<'a> {
        container: &'a PalwTirContainerV1,
        file: std::fs::File,
    }
    impl PalwTirRangeSourceV1 for Direct<'_> {
        fn read_range(&self, j: u16, l: Option<u16>, r: std::ops::Range<u64>, out: &mut [u8]) -> Result<(), String> {
            let (offset, bytes) = self.container.locate(j, l).ok_or("missing param instance")?;
            if r.start > r.end || r.end > bytes || r.end - r.start != out.len() as u64 {
                return Err("outside tensor byte range".into());
            }
            crate::tir_stream::pread(&self.file, out, offset + r.start).map_err(|e| e.to_string())
        }
    }
    let container = PalwTirContainerV1::open(path).map_err(|e| e.to_string())?;
    let source = Direct { container: &container, file: std::fs::File::open(path).map_err(|e| e.to_string())? };
    prepare_kernel_params_v3(descriptor, &container.program, &source, tensor_workspace_limit, payload_limit, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v4_descriptor};
    use std::cell::Cell;
    struct Source {
        params: misaka_palw_tir::MapParams,
        calls: Cell<u64>,
        fail: Option<(u16, Option<u16>)>,
    }
    impl PalwTirRangeSourceV1 for Source {
        fn read_range(&self, j: u16, l: Option<u16>, r: std::ops::Range<u64>, out: &mut [u8]) -> Result<(), String> {
            self.calls.set(self.calls.get() + 1);
            if self.fail == Some((j, l)) {
                return Err("missing last instance".into());
            }
            let bytes = self.params.tensors.get(&(j, l)).ok_or("missing tensor")?.to_le_bytes();
            out.copy_from_slice(&bytes[r.start as usize..r.end as usize]);
            Ok(())
        }
    }
    #[test]
    fn independent_range_source_produces_all_layer_bound_roots_and_refuses_missing_or_changed_weights() {
        let f = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
        let expected = ParamCommitmentsV1::of_v3(&f.params);
        let mut src = Source { params: f.params, calls: Cell::new(0), fail: None };
        let d = k2_tir_v4_descriptor();
        let prepared = prepare_kernel_params_v3(&d, &f.program, &src, 1 << 20, 1 << 30, &mut |_, _, _| {}).unwrap();
        assert_eq!(prepared.params, expected);
        assert_eq!(prepared.params.root(), expected.root());
        assert_eq!(prepared.program_root, misaka_palw_kernel::public::program_root_v1(&f.program.encode()));
        assert_eq!(prepared.params.by_instance.len(), src.params.tensors.len());
        let last = *src.params.tensors.keys().next_back().unwrap();
        src.fail = Some(last);
        assert!(prepare_kernel_params_v3(&d, &f.program, &src, 1 << 20, 1 << 30, &mut |_, _, _| {}).is_err());
        src.fail = None;
        src.params.tensors.get_mut(&last).unwrap().data[0] ^= 1;
        let changed = prepare_kernel_params_v3(&d, &f.program, &src, 1 << 20, 1 << 30, &mut |_, _, _| {}).unwrap();
        assert_ne!(changed.params.root(), expected.root());
    }
    #[test]
    fn all_preparation_limits_and_wrong_kernel_are_checked_before_any_payload_is_read() {
        let f = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
        let src = Source { params: f.params, calls: Cell::new(0), fail: None };
        let d = k2_tir_v4_descriptor();
        for (workspace, payload) in [(0, 1 << 30), (1 << 20, 0)] {
            assert!(
                prepare_kernel_params_v3(&d, &f.program, &src, workspace, payload, &mut |_, _, _| panic!(
                    "progress before bound check"
                ))
                .is_err()
            );
            assert_eq!(src.calls.get(), 0);
        }
        assert!(prepare_kernel_params_v3(&k2_tir_v1_descriptor(), &f.program, &src, 1 << 20, 1 << 30, &mut |_, _, _| {}).is_err());
        assert_eq!(src.calls.get(), 0);
    }
    #[test]
    fn encoder_job_inputs_are_excluded_only_by_the_explicit_encoder_descriptor() {
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref};
        use misaka_palw_tir::{DType, Tensor, TensorType};
        let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
        let embedding = pb.param("embedding", DType::I8, &[8, 2], false);
        let head = pb.param("head", DType::I8, &[8, 2], false);
        let ids = pb.param("input.ids", DType::Idx, &[4], false);
        let count = pb.param("input.count", DType::Idx, &[], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embedding, ids, 0, 0);
            let x = b.reduce_sum(x, 0, DType::I32);
            let c = b.clamp(count, 1, 4, DType::I32);
            let x = b.add(x, c, DType::I32);
            let x = b.reshape_fixed(x, &[2]);
            b.finish(&[x])
        };
        let post = {
            let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[2, 1]);
            let x = b.matmul(head, x, DType::I32);
            let x = b.reshape_fixed(x, &[8]);
            b.commit(x);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        let program = pb.finish(pre, vec![], post, logits);
        let params = misaka_palw_tir::MapParams {
            tensors: BTreeMap::from([
                ((0, None), Tensor::new(DType::I8, vec![8, 2], (0..16).map(|i| i - 7).collect()).unwrap()),
                ((1, None), Tensor::new(DType::I8, vec![8, 2], (0..16).map(|i| i * 3 - 7).collect()).unwrap()),
            ]),
        };
        let expected = ParamCommitmentsV1::of_v3(&params);
        let src = Source { params, calls: Cell::new(0), fail: None };
        let d = misaka_palw_kernel::descriptor::k2_tir_v5_descriptor();
        let prepared = prepare_kernel_params_v3(&d, &program, &src, 1 << 20, 1 << 20, &mut |j, _, _| assert!(j < 2)).unwrap();
        assert_eq!(prepared.model_param_declarations, 2);
        assert_eq!(prepared.payload_bytes, 32);
        assert_eq!(prepared.params, expected);
        // Under decoder semantics ids/count would be artifact parameters, so missing them refuses.
        assert!(prepare_kernel_params_v3(&k2_tir_v4_descriptor(), &program, &src, 1 << 20, 1 << 20, &mut |_, _, _| {}).is_err());
        let mut unknown = d;
        unknown.memory_model_id = 99;
        let before = src.calls.get();
        assert!(prepare_kernel_params_v3(&unknown, &program, &src, 1 << 20, 1 << 20, &mut |_, _, _| {}).is_err());
        assert_eq!(src.calls.get(), before);
    }
}
