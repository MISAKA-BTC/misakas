//! RFC-0036 (Proposed, dormant): the row access layer's full-state adapter is behaviour-identical to the in-memory state on the
//! existing fixtures — every row, every probe, every range — and the proven (pruned) backend built from it answers the same.
//! The conformance check itself is `crate::palw_state_row_access_v1::tests::palw_row_access_conformance_v1`.
#![allow(warnings)]
use super::*;
use crate::palw_state_row_access_v1::tests::palw_row_access_conformance_v1;

fn with_kernel_aux(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    let mut route = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    );
    // aux rows of several tables, keys of several lengths (a key that is a prefix of another included)
    for (t, k, v) in
        [(32u8, vec![1u8, 2], vec![9u8]), (32, vec![1, 2, 3], vec![8]), (33, vec![], vec![7]), (144, vec![0xff; 70], vec![6; 300])]
    {
        route.aux.insert((t, k), v);
    }
    state.kernel_route = Some(route);
    state
}

#[test]
fn the_full_state_adapter_conforms_on_every_fixture() {
    let p = params();
    let fixtures: Vec<(&str, PalwChainStateV2)> = vec![
        ("genesis", PalwChainStateV2::genesis()),
        ("m02 populated", m02_populated_state()),
        ("m02 populated + kernel aux", with_kernel_aux(m02_populated_state())),
        ("split close", split_close_fixture().1),
        ("split close funded", split_close_fixture_funded(50_000_000).1),
        ("derived work chain", derived_work_chain().1),
        ("chain with classes", chain_with_classes(4, 1_000_000).0),
        ("da setup", da_setup(&p, 130).0),
    ];
    let mut nonempty = std::collections::BTreeSet::new();
    for (name, state) in &fixtures {
        eprintln!("conformance: {name}");
        palw_row_access_conformance_v1(state);
        let full = crate::palw_state_row_access_v1::PalwFullStateRowSourceV1::new(state);
        for t in crate::palw_state_row_access_v1::PalwStateTableV1::ALL {
            if !full.table_rows(t).is_empty() {
                nonempty.insert(t);
            }
        }
    }
    // The fixtures exercise the tables that matter for decisions, not only empty ones.
    use crate::palw_state_row_access_v1::PalwStateTableV1 as T;
    for t in [T::Bonds, T::Classes, T::Claims, T::ClassShares, T::KernelRouteAux] {
        assert!(nonempty.contains(&t), "{t:?} is populated by some fixture");
    }
}
