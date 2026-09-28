//! **ref2's H7 under the second IR fence** (`Params::palw_tir_fence2`; spec 04b §10.3 and §9.5.6):
//! the box demand's `TopK` row counts the TopK rows a run of `d` demanded elements can touch, and the
//! value bound `V` inherits it.
//!
//! The vector is ref2's shape (`tir/ref2` 3ccd30ec5, `h7_topk_tile_exceeds_v`): `TopK { axis 0, k 4 }`
//! of a `[4, 3]` `MatMul` that contracts the history, committed in tiles of 4 lanes. A tile's four
//! consecutive flat indices lie in three TopK rows (one per column), so its closure is all twelve
//! scores — the release's row counts one row (`V = 4`), H7 counts the three (`V = 12`). Under the
//! release's rules the court's own honest claim of such a tile carries more values than `V`; under
//! H7 `V` bounds it. The same row prices the TopK tile's terminal close in `tir_admit`.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_h7`

use kaspa_consensus_core::palw_tir_dissect_v1::{palw_tir_dissect_value_bound_v1, palw_tir_dissect_value_bound_v2};
use kaspa_consensus_core::palw_tir_fence2_v1::PalwTirDemandRulesV1;
use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_v1, tir_admit_with_rules_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, TensorType, TirProgramV1};

/// ref2's shape: the scores `S = Qᵀ · K` over a 16-row window (`[4, H] × [H, 3]`), then
/// `TopK { axis 0, k 4 }` of `S`, committed. Returns the program, the layer block and the TopK node.
fn program() -> (TirProgramV1, usize, u16) {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 7], false);
    let head = pb.param("head", DType::I8, &[8, 7], false);
    let ks = pb.hist_state("k", DType::I32, &[3], 16, true);
    let qs = pb.hist_state("q", DType::I32, &[4], 16, true);
    let carry = vec![TensorType::fixed(DType::I32, &[7])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let (layer, topk) = {
        let mut b = pb.block("layer", carry.clone());
        // Rows clamped to a byte's range, so the scores' proven interval stays inside `i64`.
        let rk = b.slice(Ref::CarryIn(0), 0, 0, 3);
        let rk = b.clamp(rk, -127, 127, DType::I32);
        let k = b.hist_append(ks, rk);
        let rq = b.slice(Ref::CarryIn(0), 0, 3, 4);
        let rq = b.clamp(rq, -127, 127, DType::I32);
        let q = b.hist_append(qs, rq);
        let qt = b.transpose(q, &[1, 0]);
        let s = b.matmul(qt, k, DType::I64);
        let s = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let t = b.topk(s, 0, 4);
        let Ref::Node(topk) = t else { unreachable!() };
        let y = b.cast(Ref::CarryIn(0), DType::I32);
        (b.finish(&[y]), topk)
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[7, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    (program, layer as usize, topk)
}

#[test]
fn the_value_bound_counts_the_rows_a_topk_tile_touches() {
    let (p, layer, topk) = program();
    let block = &p.blocks[layer];
    let release = palw_tir_dissect_value_bound_v2(&p, block, topk, 4, PalwTirDemandRulesV1::Release2000);
    let h7 = palw_tir_dissect_value_bound_v2(&p, block, topk, 4, PalwTirDemandRulesV1::H7);
    assert_eq!(release, 4, "the release's row: ⌈4/4⌉ · 4 — below the closure of twelve");
    assert_eq!(h7, 12, "H7: min(3 rows, 4) · 4 — every score the tile's closure reads");
    assert_eq!(palw_tir_dissect_value_bound_v1(&p, block, topk, 4), release, "v1 is the release's rule, unchanged");
    // A tile of one lane touches one row either way; a tile as wide as the value touches all.
    assert_eq!(palw_tir_dissect_value_bound_v2(&p, block, topk, 1, PalwTirDemandRulesV1::H7), 4);
    assert_eq!(palw_tir_dissect_value_bound_v2(&p, block, topk, 12, PalwTirDemandRulesV1::H7), 12);
}

#[test]
fn admission_prices_the_topk_tile_by_the_rows_it_touches() {
    let (p, layer, topk) = program();
    let bytes = p.encode();
    let inputs = TirAdmitInputsV1 { tile_len: 4, h_chunk: 16, ceilings: TirCeilingsV1::legacy_court_v1() };
    let release = tir_admit_with_rules_v1(&bytes, &inputs, PalwTirDemandRulesV1::Release2000).expect("admitted under the release");
    let h7 = tir_admit_with_rules_v1(&bytes, &inputs, PalwTirDemandRulesV1::H7).expect("admitted under H7");
    assert_eq!(tir_admit_v1(&bytes, &inputs).expect("admitted"), release, "tir_admit_v1 is the release's rules");
    let cone = |a: &misaka_palw_tir::admit::TirAdmissionV1| {
        a.cones.iter().find(|c| c.block as usize == layer && c.node == topk).expect("the TopK's cone").clone()
    };
    let (r, h) = (cone(&release), cone(&h7));
    assert!(h.tile.elementwise > r.tile.elementwise, "the TopK reads three rows, not one: {:?} vs {:?}", h.tile, r.tile);
    assert!(h.terminal().elementwise >= h.tile.elementwise.min(h.terminal().elementwise));
    assert!(h.tile_opened_bytes >= r.tile_opened_bytes);
    // Every other commit point is priced as the release prices it.
    for (a, b) in release.cones.iter().zip(h7.cones.iter()) {
        if !(a.block as usize == layer && a.node == topk) {
            assert_eq!(a, b, "only the TopK's cone moves");
        }
    }
}
