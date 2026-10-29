//! Frozen campaign inputs derived from the exact carrier. This exports no observation or approval.
use kaspa_consensus_core::{
    Hash64,
    palw_memory_routing_v2::{PalwMemoryRoutingEvidenceV2, PalwMemoryRoutingObjectV2},
    palw_router_binding_v2::{PalwRouterBindingRowV2, palw_bound_router_slot_v2, palw_router_binding_covers_v2},
    palw_state_v2::PalwConsensusObjectV2,
    palw_step::{PalwLayerKindV1, PalwStepTableV1},
};
use sha2::{Digest, Sha256};

pub fn expectation(
    object: &PalwConsensusObjectV2,
    row: &PalwRouterBindingRowV2,
    block: Hash64,
    verdict: &str,
    id: &str,
    role: &str,
) -> Result<serde_json::Value, String> {
    if !matches!(verdict, "ExecutorGuilty" | "FalseAccusation") || id.is_empty() || role.is_empty() {
        return Err("explicit requirement ID, role and expected terminal verdict are required".into());
    }
    let PalwConsensusObjectV2::MemoryRoutingV2 { object } = object else { return Err("tag244 accusation required".into()) };
    let PalwMemoryRoutingObjectV2::Accused { accusation } = object.as_ref() else {
        return Err("accusation required, not declaration".into());
    };
    let PalwMemoryRoutingEvidenceV2::RoutedStep { refutation, .. } = &accusation.evidence else {
        return Err("RoutedStep accusation required".into());
    };
    if accusation.version != 2 || accusation.signature.is_empty() || row.version != 1 {
        return Err("versioned signed accusation and binding row required".into());
    }
    let profile = &refutation.binding.shape_profile;
    if !palw_router_binding_covers_v2(profile, row) {
        return Err("row does not cover the carrier's class".into());
    }
    let slot = refutation.output_preimage.coord.node_slot;
    let (_, layer) = profile.resolve_node_slot(slot).ok_or("reader slot is outside class")?;
    let layer = layer.ok_or("reader is outside layer table")?;
    let (kind, table) = match profile.layer_kind(layer) {
        PalwLayerKindV1::GatedDeltaNet => (PalwStepTableV1::Gdn, 0u8),
        PalwLayerKindV1::Attention => (PalwStepTableV1::Attn, 1u8),
    };
    let first = profile.global_node_slot(kind, layer, 0).ok_or("layer has no first slot")?;
    let reader = u16::try_from(slot.checked_sub(first).ok_or("reader underflow")?).map_err(|e| e.to_string())?;
    let bound = palw_bound_router_slot_v2(profile, row, slot).ok_or("reader has no bound router")?;
    let router = u16::try_from(bound.checked_sub(first).ok_or("router underflow")?).map_err(|e| e.to_string())?;
    if router >= reader {
        return Err("router must precede reader".into());
    }
    let row_bytes = borsh::to_vec(row).map_err(|e| e.to_string())?;
    let digest = Sha256::digest(&row_bytes).iter().map(|b| format!("{b:02x}")).collect::<String>();
    let object_id =
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/LOCAL-COURT-OBSERVATION/V1", &(244u8, accusation)));
    Ok(serde_json::json!({
        "id": id, "kind": "reader-router-court", "role": role, "claimId": accusation.claim.to_string(),
        "expected": {
            "classId": profile.shape_profile_id().to_string(),
            "producerBond": format!("{}:{}", accusation.executor_bond.0.transaction_id, accusation.executor_bond.0.index),
            "bindingSha256": digest, "table": table, "reader": reader, "router": router,
            "court": {"blockHash": block.to_string(), "objectId": object_id.to_string(), "objectTag": 244,
                "route": "RoutedStep", "leafIndex": refutation.output_opening.leaf_index, "verdict": verdict}
        }
    }))
}
