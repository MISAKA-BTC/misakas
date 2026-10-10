//! **The court scope on F-C4R4-17's own class** (`dense_moe_v1(7)`, the C4R4 PoC's ledger world): the predicate leaves no clear
//! model-dependent value for the ledger to serve, so no union of demands rebuilds a weight; model-independent values stay owed.
//! (G14R's ledger hook applies `court_withheld_mask_v1`; this pins what it applies.)

use misaka_palw_kernel::scope::*;
use misaka_palw_tir::program::Ref;
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

#[test]
fn f_c4r4_17_class_serves_no_clear_activation_of_a_weight_relation() {
    let p = dense_moe_v1(7).program;
    let m = p.params.len();
    let e = court_model_exposure_v1(&p, m);
    assert!(e.min_linear_inner_dim.is_some(), "the class has model-linear relations: {e:?}");
    let budget = court_clear_position_budget_v1(&e);
    assert!(
        budget < e.min_linear_inner_dim.unwrap(),
        "the model's lifetime clear budget ({budget}) is below every relation's rank: no weight is ever determined"
    );
    // Past the budget (here: from the first demand when an elementwise relation exists) every position is CommitmentOnly.
    let d = court_position_disclosure_v1(budget, &e);
    assert_eq!(d, DisclosureV1::CommitmentOnly);
    let withheld = court_withheld_mask_v1(&p, m, d);
    let materials = court_node_materials_v1(&p, m);
    let mut relations = 0;
    for (o, (b, _)) in p.occurrences().iter().enumerate() {
        for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
            let reads_weight = node.inputs.iter().any(|r| matches!(r, Ref::Param(j) if (*j as usize) < m));
            if reads_weight && matches!(node.prim, misaka_palw_tir::Prim::MatMul) {
                relations += 1;
                assert!(withheld[o][n], "a weight relation's output is never served in the clear (occ {o}, node {n})");
                for r in &node.inputs {
                    if let Ref::Node(i) = r {
                        assert!(
                            withheld[o][*i as usize] || !materials[o][*i as usize].is_model_dependent(),
                            "its activation input is withheld unless it is model-independent"
                        );
                    }
                }
            }
        }
    }
    assert!(relations > 0, "F-C4R4-17 rebuilt such relations");
    // Model-independent values (none depends on a weight) stay owed in the clear: the claim-specific DA is intact.
    assert!(materials.iter().flatten().zip(withheld.iter().flatten()).all(|(mat, w)| *w == mat.is_model_dependent()));
}
