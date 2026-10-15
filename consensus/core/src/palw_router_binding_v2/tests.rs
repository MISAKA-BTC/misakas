//! Source controls for the per-reader router binding (RFC-0006 consumer v2). Written for the consolidated validation phase; not run
//! here and not acceptance results.
use super::*;
use crate::palw_kimi_k3_profile::{KIMI_K3_CARD, kimi_k3_profile_v1};
use crate::palw_step::PalwStepNodeV1;

/// The routed up-projection's index in the Kimi KDA template (after the router at 13).
const READER: usize = 14;
const ROUTER: usize = 13;

/// **The Kimi card with its routed reader at the width the routed matmul serves** (`k · ffn_dim`). The card itself declares the
/// reader at `k · hidden_dim`, which no expert block divides — see [`the_kimi_card_is_not_bindable_as_declared`].
fn kimi_bindable() -> PalwShapeProfileV3 {
    let mut p = kimi_k3_profile_v1(KIMI_K3_CARD).expect("the card projects");
    let k = KIMI_K3_CARD.experts_per_token;
    for table in [&mut p.gdn_nodes, &mut p.attn_nodes] {
        for node in table.iter_mut() {
            if node.weight_name.ends_with(".ffn_up_exps.routed") {
                node.out_len = PalwStepOutLenV1::Fixed { elements: k * KIMI_K3_CARD.moe_dim };
            }
        }
    }
    p
}

fn router_index(table: &[PalwStepNodeV1]) -> u16 {
    table.iter().position(|n| palw_router_family_v2(n.kernel_semantics_id).is_some()).expect("a router") as u16
}

fn declaration(p: &PalwShapeProfileV3) -> PalwRouterBindingDeclarationV2 {
    let readers = palw_routed_readers_v2(p)
        .into_iter()
        .map(|(table, reader)| {
            let nodes = if table == PALW_ROUTER_BINDING_TABLE_GDN_V2 { &p.gdn_nodes } else { &p.attn_nodes };
            PalwRoutedReaderBindingV2 { table, reader, router: router_index(nodes) }
        })
        .collect();
    PalwRouterBindingDeclarationV2 {
        version: PALW_ROUTER_BINDING_VERSION_V2,
        class_id: p.shape_profile_id(),
        artifact_root: Hash64::from_bytes([3; 64]),
        profile: p.clone(),
        readers,
    }
}

/// **Kimi expert universe**: the card's routed reader is not bindable as declared (its expert blocks do not divide), so its routed
/// products stay unadjudicable — reported, not papered over by choosing a router.
#[test]
fn the_kimi_card_is_not_bindable_as_declared() {
    let card = kimi_k3_profile_v1(KIMI_K3_CARD).expect("the card projects");
    assert!(!palw_routed_readers_v2(&card).is_empty(), "the card has routed readers");
    assert!(!palw_router_binding_possible_v2(&card), "k * hidden is not k expert blocks of ffn_dim");
    assert!(palw_router_binding_check_v2(&declaration(&card)).is_err());
}

/// The bindable Kimi graph: every routed reader bound to its table's Kimi router, the universe (896 experts) at least k, the Kimi
/// combine's routing reference the Kimi router.
#[test]
fn a_kimi_table_binds_its_readers_to_its_own_router() {
    let p = kimi_bindable();
    assert!(palw_router_binding_possible_v2(&p));
    let d = declaration(&p);
    palw_router_binding_check_v2(&d).expect("the bindable Kimi graph binds");
    let row = PalwRouterBindingRowV2 { version: PALW_ROUTER_BINDING_VERSION_V2, bound_from_daa: 7, readers: d.readers.clone() };
    assert!(palw_router_binding_covers_v2(&p, &row));
    // The court's slot: the reader in a KDA layer reads THAT layer's router, never another layer's.
    let layer = (0..p.layer_count).find(|l| p.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).unwrap();
    let first = p.global_node_slot(PalwStepTableV1::Gdn, layer, 0).unwrap();
    assert_eq!(palw_bound_router_slot_v2(&p, &row, first + READER as u32), Some(first + ROUTER as u32));
    assert_eq!(palw_bound_router_slot_v2(&p, &row, first + ROUTER as u32), None, "the router is not a routed reader");
}

/// **Wrong selected router, missing / duplicate readers, a router after its reader, a non-router node, a foreign graph**: refused.
#[test]
fn a_binding_that_is_not_exactly_the_graphs_is_refused() {
    let p = kimi_bindable();
    let good = declaration(&p);
    let mut not_a_router = good.clone();
    not_a_router.readers[0].router = 12; // the router's score matmul, not a router
    assert!(palw_router_binding_check_v2(&not_a_router).is_err());
    let mut after = good.clone();
    after.readers[0].router = after.readers[0].reader + 1;
    assert!(palw_router_binding_check_v2(&after).is_err());
    let mut missing = good.clone();
    missing.readers.pop();
    assert!(palw_router_binding_check_v2(&missing).is_err());
    let mut duplicated = good.clone();
    duplicated.readers.push(duplicated.readers[0]);
    assert!(palw_router_binding_check_v2(&duplicated).is_err());
    let mut foreign = good.clone();
    foreign.class_id = Hash64::from_bytes([9; 64]);
    assert!(palw_router_binding_check_v2(&foreign).is_err(), "the carried graph must hash to the class id");
    let mut wrong_version = good;
    wrong_version.version = 2;
    assert!(palw_router_binding_check_v2(&wrong_version).is_err());
}

/// **Multiple independent routers**: two routers and two routed readers in one table, each reader bound to its own; the court's
/// slot follows the binding exactly, and the legacy unique-router rule (which would refuse) is never consulted.
#[test]
fn two_routers_in_one_table_each_serve_the_reader_bound_to_it() {
    let mut p = kimi_bindable();
    let second_router = p.gdn_nodes[ROUTER].clone();
    let mut second_reader = p.gdn_nodes[READER].clone();
    second_reader.input_refs = vec![11];
    p.gdn_nodes.push(second_router); // index 18
    p.gdn_nodes.push(second_reader); // index 19
    let mut d = declaration(&p);
    for b in &mut d.readers {
        if b.table == PALW_ROUTER_BINDING_TABLE_GDN_V2 && b.reader == 19 {
            b.router = 18;
        }
    }
    d.class_id = p.shape_profile_id();
    d.profile = p.clone();
    palw_router_binding_check_v2(&d).expect("each reader names an admissible router of its own table");
    let row = PalwRouterBindingRowV2 { version: PALW_ROUTER_BINDING_VERSION_V2, bound_from_daa: 0, readers: d.readers.clone() };
    let layer = (0..p.layer_count).find(|l| p.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).unwrap();
    let first = p.global_node_slot(PalwStepTableV1::Gdn, layer, 0).unwrap();
    assert_eq!(palw_bound_router_slot_v2(&p, &row, first + 19), Some(first + 18));
    assert_eq!(palw_bound_router_slot_v2(&p, &row, first + READER as u32), Some(first + ROUTER as u32));
    // The court's own resolution: the bound slot is accepted; another layer's router, a non-router and a later node are not.
    use crate::palw_step_refute::qwen36_router_slot_for_reader_v2 as resolve;
    assert_eq!(resolve(&p, first + 19, Some(first + 18)).map(|(slot, _)| slot), Some(first + 18));
    assert!(resolve(&p, first + 19, Some(first + 12)).is_none(), "not a router node");
    assert!(resolve(&p, first + READER as u32, Some(first + 18)).is_none(), "a router after the reader");
    let other = (layer + 1..p.layer_count).find(|l| p.layer_kind(*l) == PalwLayerKindV1::GatedDeltaNet).unwrap();
    let other_first = p.global_node_slot(PalwStepTableV1::Gdn, other, 0).unwrap();
    assert!(resolve(&p, first + 19, Some(other_first + 18)).is_none(), "another layer's router");
    assert!(resolve(&p, first + 19, None).is_none(), "the legacy rule finds no unique q36 router here");
}

/// **q36 and Kimi weight arithmetic stay separate**: a Kimi combine whose routing reference is not a Kimi router is refused.
#[test]
fn a_combine_reading_another_familys_router_is_refused() {
    let mut p = kimi_bindable();
    let q36_router = crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_ROUTER_TOPK);
    p.gdn_nodes[ROUTER].kernel_semantics_id = q36_router;
    assert!(palw_combine_router_families_v2(&p).is_err());
    assert!(!palw_router_binding_possible_v2(&p));
}

/// The stored row's encoding is the aux-139 wire form the native scanner reads (key = the 64-byte class id).
#[test]
fn the_row_round_trips_and_its_key_is_the_class_id() {
    let row = PalwRouterBindingRowV2 {
        version: PALW_ROUTER_BINDING_VERSION_V2,
        bound_from_daa: 42,
        readers: vec![PalwRoutedReaderBindingV2 { table: 0, reader: 14, router: 13 }],
    };
    let bytes = borsh::to_vec(&row).unwrap();
    assert_eq!(borsh::from_slice::<PalwRouterBindingRowV2>(&bytes).unwrap(), row);
    let class = Hash64::from_bytes([5; 64]);
    assert_eq!(palw_router_binding_key_v2(&class).len(), 64);
    assert_eq!(PALW_ROUTER_BINDING_TABLE_V2, 139);
}
