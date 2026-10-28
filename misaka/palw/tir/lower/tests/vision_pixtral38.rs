//! Explicit Pixtral profile: source geometry, bias inventory and layout semantics.
//! Test sources only until the ordered verification phase is authorized.
#![allow(warnings)]
use misaka_palw_tir as tir;
use misaka_palw_tir_lower::{
    encoder,
    lower::vision::{self, VisionMathProfileV38, VisionOut},
};
use std::path::Path;
fn tiny() -> vision::VisionSpec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/mistral3_vlm/config.json");
    let source = std::fs::read_to_string(path).unwrap();
    vision::parse_vision(&source, None, None).unwrap()
}
#[test]
fn pixtral_profile_keeps_tower_and_learned_projector_with_no_synthetic_bias() {
    let s = tiny();
    assert_eq!(s.math_profile, Some(VisionMathProfileV38::PixtralV38));
    assert!(s.pre_norm && s.rms && s.swiglu);
    assert_eq!(s.out_rows(), 1);
    assert!(matches!(s.out, VisionOut::PixtralProjectorV38 { merge: 2, .. }));
    let (hl, _) = vision::hl_program(&s).unwrap();
    let names = hl.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
    for name in ["pre_norm.gain", "projector.norm.gain", "patch_merger.w", "proj.up.w", "proj.down.w"] {
        assert!(names.contains(&name), "{name}");
    }
    assert!(!names.iter().any(|n| n.ends_with(".bias") || n.ends_with(".b")));
    let lowered = vision::lower_vision_with(&hl, &s, true).unwrap();
    let p = encoder::vision_v2(&lowered).unwrap();
    assert_eq!(p.inputs.len(), 1);
    assert!(matches!(p.output, tir::program_v2::OutputDecl::Final { .. }));
}
#[test]
fn axial_w_uses_odd_original_inverse_frequency_lanes() {
    let s = tiny();
    assert_eq!(s.head_dim(), 8);
    let (cos, _) = vision::rope_2d(&s).unwrap();
    // Patch row1 is (h=0,w=1). H frequencies are [1,.01], W [.1,.001].
    let at = s.head_dim();
    assert_eq!(cos[at], 1.0);
    assert_eq!(cos[at + 1], 1.0);
    assert_eq!(cos[at + 2], misaka_palw_tir_lower::detmath::cos_f32(0.1) as f64);
    assert_eq!(cos[at + 3], misaka_palw_tir_lower::detmath::cos_f32(0.001) as f64);
}
#[test]
fn unfold_primitive_graph_matches_independent_channel_first_coordinates() {
    for (gh, gw, width, merge) in [(2, 2, 3, 2), (4, 6, 3, 2), (3, 3, 2, 1)] {
        let mut builder = tir::builder::ProgramBuilder::new(1, tir::program::HISTORY_BOUND_V1_SMALL);
        let source = builder.param("source", tir::DType::I16, &[gh * gw, width], false);
        let pre = {
            let mut b = builder.block("pre", vec![]);
            let x = b.cast(source, tir::DType::I32);
            b.finish(&[x])
        };
        let post = {
            let mut b = builder.block("post", vec![tir::TensorType::fixed(tir::DType::I32, &[gh * gw, width])]);
            let x = vision::pixtral_unfold_v38(&mut b, tir::Ref::CarryIn(0), gh, gw, width, merge).unwrap();
            b.commit(x);
            b.finish(&[])
        };
        let node = (builder.blocks[post as usize].nodes.len() - 1) as u16;
        let p = builder.finish(pre, vec![], post, node);
        let p = tir::program_v2::TirProgramV2::from_v1_lifting_params(
            &p,
            &[(0, tir::program_v2::InputSource::External { lo: 0, hi: 1024 })],
            tir::program_v2::OutputDecl::Final { node },
        )
        .unwrap();
        tir::validate_v2::validate_v2(&p).unwrap();
        let mut inputs = tir::interp_v2::MapInputs::default();
        inputs.constant.insert(
            0,
            tir::Tensor::new(
                tir::DType::I16,
                vec![(gh * gw) as usize, width as usize],
                (0..gh * gw * width).map(|n| n as i128).collect(),
            )
            .unwrap(),
        );
        let got = tir::interp_v2::InterpreterV2::new(&p).unwrap().run_positions(&tir::MapParams::default(), &inputs, 1).unwrap();
        let mut expected = Vec::new();
        for gy in 0..gh / merge {
            for gx in 0..gw / merge {
                for channel in 0..width {
                    for dy in 0..merge {
                        for dx in 0..merge {
                            expected.push((((gy * merge + dy) * gw + gx * merge + dx) * width + channel) as i128);
                        }
                    }
                }
            }
        }
        assert_eq!(got[0].output.data, expected);
    }
}
#[test]
fn pixtral_profile_cannot_be_dropped_while_reusing_projector_variant() {
    let mut s = tiny();
    s.math_profile = None;
    assert!(s.validate().unwrap_err().to_string().contains("explicit math profile"));
}
