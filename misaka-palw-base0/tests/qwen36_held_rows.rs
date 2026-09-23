//! **The held Qwen3.6 rows in the node's table are the classes the chain registers.**
//!
//! testnet-12's genesis registers `qwen36_held_registration_v1(512)` — class `e108e736…` — and this
//! table had no graph-v7 row at all, so the SDK could pair no artifact with it and no node on the
//! fleet held one. The rows added here derive their ids through the same expression the card does,
//! which is the only way a row here and a class there can be one class.
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use misaka_palw_base0::classes::{QWEN36_GRAPH_V7_2M_MODEL_ID, QWEN36_GRAPH_V7_512_MODEL_ID, qwen36_canonical_classes_v1};

fn t12_genesis_class_ids() -> Vec<kaspa_consensus_core::Hash64> {
    let p = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };
    b.genesis_objects.iter().filter_map(|o| match o { PalwConsensusObjectV2::ClassRegistered { class_id, .. } => Some(*class_id), _ => None }).collect()
}

#[test]
fn the_held_512_row_is_testnet_12s_genesis_hybrid_class() {
    let rows = qwen36_canonical_classes_v1();
    let row = rows.iter().find(|r| r.model_id == QWEN36_GRAPH_V7_512_MODEL_ID).expect("the held 512 row exists");
    let id = row.class_id().expect("the held 512 profile projects");
    assert!(t12_genesis_class_ids().contains(&id), "row {} must be the class t12 registered at genesis, got {id}", row.model_id);
    assert_eq!(id.to_string()[..8], *"e108e736", "the id the fleet's logs name");
}

#[test]
fn the_held_2m_row_is_a_distinct_class_the_registry_can_add() {
    let rows = qwen36_canonical_classes_v1();
    let r2m = rows.iter().find(|r| r.model_id == QWEN36_GRAPH_V7_2M_MODEL_ID).expect("the held 2M row exists");
    let id = r2m.class_id().expect("the held 2M profile projects");
    assert!(!t12_genesis_class_ids().contains(&id), "2M is not a genesis class on t12 — it is the one the registry adds");
    let r512 = rows.iter().find(|r| r.model_id == QWEN36_GRAPH_V7_512_MODEL_ID).unwrap();
    assert_ne!(id, r512.class_id().unwrap(), "width is the axis: two ids");
    // Both held profiles register the operand-inventory root, so a manifest over either artifact
    // carries the form a registration pins — the A16 lesson, checked here before it can recur.
    for r in [r512, r2m] {
        let profile = r.profile().unwrap();
        assert!(misaka_palw_base0::inventory::qwen36_registers_inventory_root_v1(&profile), "{}: a held row registers the inventory root", r.model_id);
    }
    // Ids are stable across the table's own two derivations.
    assert_eq!(rows.iter().filter(|r| r.graph_version == 7).count(), 2, "exactly the two held rows");
}
