#![cfg(feature = "node")]
use kaspa_consensus_core::{palw_decode_select_v2::PalwDecodeSamplingV2, palw_logits_q24_v2 as wide};
use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder};
use misaka_palw_tir_exec::{
    cellproc::{CELL_WIRE_VERSION_V1, CellResponseV1},
    cellproc_v2::{CELL_WIRE_VERSION_V2, CellResponseV2},
};
fn program() -> misaka_palw_tir::TirProgramV1 {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let mut pre = p.block("pre", vec![]);
    let x = pre.cast(Ref::Input(0), DType::I32);
    pre.commit(x);
    let pre = pre.finish(&[x]);
    let k = p.konst(DType::I64, &[], &[1 << 34]);
    let mut post = p.block("post", vec![TensorType::scalar(DType::I32)]);
    let x = post.cast(Ref::CarryIn(0), DType::I64);
    let y = post.mul(x, k, DType::I64);
    post.commit(y);
    let Ref::Node(logits) = y else { unreachable!() };
    let post = post.finish(&[]);
    let mut p = p.finish(pre, vec![], post, logits);
    p.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    p
}
#[test]
fn wide_is_only_final_logits_under_its_explicit_scheme() {
    let p = program();
    misaka_palw_tir::validate::validate(&p).unwrap();
    let mut old = p.clone();
    old.logits_scheme_id = [0; 64];
    assert!(misaka_palw_tir::validate::validate(&old).is_err());
    let mut internal = p.clone();
    internal.blocks[p.schedule.post as usize].nodes[0].commit = true;
    assert!(misaka_palw_tir::validate::validate(&internal).is_err());
    assert!(!DType::I64.committable());
    let name = b"tiled-logits-i64-q24-v2";
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tiled-logits/scheme/v2").to_state();
    h.update(&(name.len() as u64).to_le_bytes());
    h.update(name);
    assert_eq!(wide::scheme_id().as_byte_slice(), h.finalize().as_bytes());
}
#[test]
fn sampling_retains_units_extremes_and_legacy_answers() {
    let s = PalwDecodeSamplingV2 { seed: [7; 32], temperature_q: u32::MAX };
    let values = [i64::MIN, i64::MAX, 0];
    assert_eq!(wide::select(&values, &s, 1), Some(1));
    for row in [[i32::MIN, 0, i32::MAX], [1, 1, 0], [-7, -4, -1]] {
        for temp in [0, 1 << 24, u32::MAX] {
            let s = PalwDecodeSamplingV2 { temperature_q: temp, ..s };
            let w: Vec<_> = row.iter().map(|v| *v as i64).collect();
            assert_eq!(wide::select(&w, &s, 13), Some(s.select(&row, 13)));
        }
    }
    assert_eq!(wide::lane_key(1 << 34, &PalwDecodeSamplingV2::GREEDY, 0, 0), 1i128 << 58);
}
#[test]
fn ipc_v2_preserves_eight_byte_logits_and_rejects_old_decoder() {
    assert_ne!(CELL_WIRE_VERSION_V1, CELL_WIRE_VERSION_V2);
    let value = CellResponseV2::Stepped { values: vec![], carry: vec![], logits: vec![i64::MIN, 1082i64 << 24, i64::MAX] };
    let bytes = value.encode();
    assert_eq!(CellResponseV2::decode(&bytes).unwrap(), value);
    assert!(CellResponseV1::decode(&bytes).is_err());
    for n in 0..bytes.len() {
        assert!(CellResponseV2::decode(&bytes[..n]).is_err());
    }
    let old = CellResponseV1::Stepped { values: vec![], carry: vec![], logits: vec![123] };
    assert_eq!(CellResponseV1::decode(&old.encode()).unwrap(), old);
}
#[cfg(unix)]
#[test]
fn helper_negotiates_v2_and_mirrors_large_logits_without_narrowing() {
    use misaka_palw_tir_exec::cellproc_v2::{CpuDeviceV2, TirProcessDeviceV2, serve_cell_requests_v2};
    use misaka_palw_tir_exec::cellstep_v2::TirDeviceV2;
    use misaka_palw_tir_exec::{NoSink, TirParams, TirPlan};
    use std::os::unix::net::UnixStream;
    let (a, b) = UnixStream::pair().unwrap();
    let thread = std::thread::spawn(move || {
        let mut r = b.try_clone().unwrap();
        let mut w = b;
        serve_cell_requests_v2(&mut r, &mut w, &CpuDeviceV2).unwrap();
    });
    let device = TirProcessDeviceV2::connect(Box::new(a.try_clone().unwrap()), Box::new(a), true).unwrap();
    let p = program();
    let plan = TirPlan::compile(&p).unwrap();
    let params = TirParams::from_map(&plan, &misaka_palw_tir::MapParams::default()).unwrap();
    {
        let mut step = device.cell_stepper(&plan, &params, 0..2).unwrap();
        step.step_cell(3, 0..2, &[], &mut NoSink).unwrap();
        assert_eq!(step.logits_lanes(), vec![3i64 << 34]);
    }
    drop(device);
    thread.join().unwrap();
}
