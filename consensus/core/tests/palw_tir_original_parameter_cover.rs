//! Original-wide public-source dependency footprints. No producer root, model
//! bytes, paid admission or execution credit is inferred from these results.
#[path = "common/palw_tir_parameter_cover_fixture.rs"]
mod cover_fixture;
#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use cover_fixture::dynamic_fixture;
use fixture::*;
use kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1;
use kaspa_consensus_core::palw_tir_admission_v1::{PalwTirClassRecordV1, palw_tir_class_record_v1};
use kaspa_consensus_core::palw_tir_court_v1::parameter_cover::{ParameterCoverBudgetV1, derive_parameter_cover_v1};
use kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissect_site_v1;
use kaspa_consensus_core::palw_tir_public_source_v1::{PalwTirPublicSourceV1, parameter_cover_v1::OriginalParameterCoverBudgetV1};
const BUDGET: OriginalParameterCoverBudgetV1 = OriginalParameterCoverBudgetV1 {
    max_original_leaves: 100_000,
    max_total_edges: 10_000_000,
    max_total_coordinates: 1_000_000,
    max_root_memberships: 100_000,
};
fn source(f: &Fixture) -> (PalwTirPublicSourceV1, PalwTirClassRecordV1, Execution) {
    let x = f.honest();
    let (registered, _) = palw_tir_class_record_v1(&f.class, &f.artifact_root).unwrap();
    let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
    let source = PalwTirPublicSourceV1::new_v1(
        &registered,
        &f.class.layout,
        &header,
        x.binding.committed_execution_root,
        x.binding.full_logits_trace_root,
        BUDGET.max_original_leaves,
    )
    .unwrap();
    (source, registered, x)
}
#[test]
fn whole_original_relation_equals_every_native_named_cover_and_its_union() {
    for per_layer in [false, true] {
        let (f, _) = dynamic_fixture(false, per_layer, true);
        let (source, registered, x) = source(&f);
        let all = source.parameter_cover_v1(&registered, &f.class.layout, BUDGET).unwrap();
        let court = kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(BUDGET.max_original_leaves, 20, 2).unwrap();
        assert_eq!(court.dissection_arity(), 2);
        let topology = source
            .numerical_tree_upper_v1(
                &registered,
                &f.class.layout,
                &court,
                kaspa_consensus_core::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,
            )
            .unwrap();
        assert_eq!(
            all.roots.len() as u64,
            topology.eligible_roots,
            "independent native analytical topology covers the identical original root universe"
        );
        assert_eq!(all.source_id, source.source_id_v1());
        assert_eq!(all.original_leaves, f.leaves.len() as u64);
        let mut roots = Vec::new();
        let mut union = std::collections::BTreeSet::new();
        let (mut edges, mut coordinates, mut memberships) = (0u64, 0u64, 0u64);
        for (i, leaf) in f.leaves.iter().enumerate() {
            if palw_tir_dissect_site_v1(&f.space, f.intervals.as_ref().unwrap(), leaf).is_none() {
                continue;
            }
            let one = derive_parameter_cover_v1(
                &x.binding,
                i as u64,
                ParameterCoverBudgetV1 { max_edges: BUDGET.max_total_edges, max_coordinates: BUDGET.max_total_coordinates },
            )
            .unwrap();
            roots.push((i as u64, one.leaves.clone()));
            union.extend(one.leaves.iter().copied());
            edges += one.explored_edges;
            coordinates += one.explored_coordinates;
            memberships += one.leaves.len() as u64;
        }
        assert!(!roots.is_empty());
        assert_eq!(all.roots.iter().map(|r| (r.leaf, r.parameters.clone())).collect::<Vec<_>>(), roots);
        assert_eq!(all.union, union.into_iter().collect::<Vec<_>>());
        assert_eq!((all.total_edges, all.total_coordinates, all.root_memberships), (edges, coordinates, memberships));
        assert!(all.roots.windows(2).all(|w| w[0].leaf < w[1].leaf));
    }
}
#[test]
fn every_original_budget_is_global_and_refuses_without_partial_success() {
    let (f, _) = dynamic_fixture(false, true, true);
    let (source, registered, _) = source(&f);
    let exact = source.parameter_cover_v1(&registered, &f.class.layout, BUDGET).unwrap();
    assert!(exact.total_edges > 0 && exact.total_coordinates > 0 && exact.root_memberships > 0);
    let full = OriginalParameterCoverBudgetV1 {
        max_original_leaves: exact.original_leaves,
        max_total_edges: exact.total_edges,
        max_total_coordinates: exact.total_coordinates,
        max_root_memberships: exact.root_memberships,
    };
    assert_eq!(source.parameter_cover_v1(&registered, &f.class.layout, full).unwrap(), exact);
    let mut limits = Vec::new();
    let mut b = full;
    b.max_original_leaves -= 1;
    limits.push(b);
    let mut b = full;
    b.max_total_edges -= 1;
    limits.push(b);
    let mut b = full;
    b.max_total_coordinates -= 1;
    limits.push(b);
    let mut b = full;
    b.max_root_memberships -= 1;
    limits.push(b);
    for budget in limits {
        assert!(
            source.parameter_cover_v1(&registered, &f.class.layout, budget).is_err(),
            "budget must cover the complete original: {budget:?}"
        );
    }
}
#[test]
fn parameter_free_original_keeps_all_roots_but_has_no_model_byte_dependencies() {
    let (f, _) = dynamic_fixture(true, false, false);
    let (source, registered, _) = source(&f);
    let all = source.parameter_cover_v1(&registered, &f.class.layout, BUDGET).unwrap();
    assert!(!all.roots.is_empty());
    assert!(all.roots.iter().all(|r| r.parameters.is_empty()));
    assert!(all.union.is_empty());
    assert_eq!(all.root_memberships, 0);
    let mut budget = BUDGET;
    budget.max_root_memberships = 0;
    assert_eq!(source.parameter_cover_v1(&registered, &f.class.layout, budget).unwrap(), all);
}
#[test]
fn substituted_registration_layout_or_header_does_not_authorize_an_original_quote() {
    let (f, _) = dynamic_fixture(false, false, false);
    let (source, registered, x) = source(&f);
    let (other, _) = dynamic_fixture(false, true, false);
    let (_, other_record, _) = self::source(&other);
    assert!(source.parameter_cover_v1(&other_record, &f.class.layout, BUDGET).is_err());
    let mut layout = f.class.layout.clone();
    layout.commit_tiles[0] += 1;
    assert!(source.parameter_cover_v1(&registered, &layout, BUDGET).is_err());
    let mut header = PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
    header.step_leaf_count += 1;
    assert!(
        PalwTirPublicSourceV1::new_v1(
            &registered,
            &f.class.layout,
            &header,
            x.binding.committed_execution_root,
            x.binding.full_logits_trace_root,
            BUDGET.max_original_leaves
        )
        .is_err()
    );
    let mut registered = registered.clone();
    registered.program = std::sync::Arc::new(vec![]);
    assert!(source.parameter_cover_v1(&registered, &f.class.layout, BUDGET).is_err());
}
