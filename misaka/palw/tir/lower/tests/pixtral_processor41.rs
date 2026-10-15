//! Unrun until the coordinated validation phase. Geometry is checked against
//! the independent publisher rule; integer processor/math is a new explicit profile.
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::lower::pixtral_processor41::*;
fn profile(h: u32, w: u32) -> ProcessorProfileV41 {
    ProcessorProfileV41 {
        version: 41,
        original_height: h,
        original_width: w,
        maximum_image_size: 1540,
        patch: 14,
        merge: 2,
        image_token: 10,
        break_token: 12,
        end_token: 13,
    }
}
#[test]
fn original_aspect_and_patch_merge_token_layout_are_preserved() {
    let g = geometry_v41(&profile(1024, 2048)).unwrap();
    assert_eq!(g.resized, [784, 1540]);
    assert_eq!(g.token_grid, [28, 55]);
    assert_eq!(g.image_tokens.len(), 28 * 56);
    assert_eq!(g.image_tokens.iter().filter(|t| **t == 10).count(), 28 * 55);
    assert_eq!(*g.image_tokens.last().unwrap(), 13);
    assert_eq!(g.image_tokens.iter().filter(|t| **t == 12).count(), 27);
    let g = geometry_v41(&profile(28, 56)).unwrap();
    assert_eq!(g.resized, [28, 56]);
    assert_eq!(g.image_tokens, vec![10, 10, 13]);
}
struct NoParameters;
impl tir::interp_v39::ParamSourceV39 for NoParameters {
    fn descriptor(&self, _: u16, _: Option<u16>) -> Option<tir::interp_v39::ParamDescriptorV39> {
        None
    }
    fn read_range(&self, _: u16, _: Option<u16>, _: u64, _: usize) -> tir::TirResult<Vec<i128>> {
        Err(tir::TirError::new(tir::TirErrorKind::Missing, "processor41 has no source weight parameters"))
    }
}
#[test]
fn parameterless_bicubic_preserves_constant_rgb_at_explicit_q6_unit() {
    let (p, g) = lower_processor_v41(&profile(3, 5), tir::program_v39::SourceProfileV39::full_source()).unwrap();
    assert!(p.params.is_empty());
    assert_eq!(p.inputs.len(), 1);
    assert_eq!(tir::program_v39::TirProgramV39::decode_canonical(&p.encode()).unwrap(), p);
    let mut inputs = tir::interp_v2::MapInputs::default();
    inputs.constant.insert(0, tir::Tensor::new(tir::DType::I16, vec![3, 5, 3], vec![128; 45]).unwrap());
    let mut state = tir::interp::RunState::default();
    let output = tir::interp_v39::InterpreterV39::new(&p).unwrap().step(&NoParameters, &inputs, &mut state, 0).unwrap();
    assert_eq!(output.output.shape, vec![g.resized[0] as usize, g.resized[1] as usize, 3]);
    assert!(output.output.data.iter().all(|v| *v == 128 * 64));
}
#[test]
fn rgba_white_ingress_and_invalid_original_axis_are_explicit() {
    assert_eq!(rgba_over_white_v41(&[0, 0, 0, 0, 100, 150, 200, 255]).unwrap(), vec![255, 255, 255, 100, 150, 200]);
    assert!(rgba_over_white_v41(&[0; 3]).is_err());
    assert!(geometry_v41(&profile(0, 20)).is_err());
    assert!(geometry_v41(&profile(1, u32::MAX)).is_err());
}
