//! RFC-0002 item 3 (lane RFC02-T): the flat-v5 candidate pursuit on G14-B's clock (`misaka_palw_kernel::flat_pursuit5`), at a toy and
//! at the actual 32-position Qwen2.5 pilot program's shapes. Every number is derived from policy terms and shapes, nothing measured.

mod common;

use common::ledger_world::policy;
use common::opv_world::opv_example;
use misaka_palw_kernel::flat_pursuit5::flat_pursuit_v5;
use misaka_palw_kernel::flat_scope5::flat_resource_scope_v5;
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

#[test]
fn rfc02t_flat_pursuit_counts_position_node_and_leaf_halvings_and_the_sealed_filing() {
    let (l, o) = (policy(), opv_example());
    let program = dense_moe_v1(7).program;
    let scope = flat_resource_scope_v5(&program, 5, None).unwrap();
    let p = flat_pursuit_v5(&program, 5, scope.worst_court_bytes, &l, &o).unwrap();
    assert_eq!(p.rounds, p.position_rounds + p.node_rounds + p.leaf_rounds + 1);
    assert_eq!(p.position_rounds, 3, "5 positions: three halvings");
    assert_eq!(p.response_window_daa, l.court_deadline_daa, "a 10 KiB reply is one chunk: the base deadline");
    // More positions never shorten it; a later seal maturity lengthens it one for one.
    let longer = flat_pursuit_v5(&program, 64, scope.worst_court_bytes, &l, &o).unwrap();
    assert!(longer.filed_daa > p.filed_daa);
    let mut slow = l;
    slow.claim_seal_delay_daa += 7;
    assert_eq!(flat_pursuit_v5(&program, 5, scope.worst_court_bytes, &slow, &o).unwrap().filed_daa, p.filed_daa + 7);
    eprintln!("[rfc02t] toy dense-moe @5: {p:?}");
}

#[test]
fn rfc02t_flat_pursuit_at_the_actual_qwen25_pilot_shapes() {
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docs/design/palw/tir/evidence/qwen25-real-pilot-program.tir"));
    let program = TirProgramV1::decode_canonical(bytes).unwrap();
    let (l, o) = (policy(), opv_example());
    match flat_resource_scope_v5(&program, 32, None) {
        Ok(scope) => {
            let p = flat_pursuit_v5(&program, 32, scope.worst_court_bytes, &l, &o).unwrap();
            assert_eq!(p.position_rounds, 5);
            eprintln!(
                "[rfc02t] qwen25 pilot @32: rounds {} (positions {}, nodes {}, leaves {}), window {}, filing chunks {}, filed at DAA {} \
                 (fits 200: {})",
                p.rounds,
                p.position_rounds,
                p.node_rounds,
                p.leaf_rounds,
                p.response_window_daa,
                p.filing_chunks,
                p.filed_daa,
                p.fits(200)
            );
        }
        Err(e) => eprintln!("[rfc02t] qwen25 pilot @32: the flat scope refuses: {e:?}"),
    }
}
