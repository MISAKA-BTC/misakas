//! Shared op203 request/response conversion, used by the production service and
//! real-socket consensus integration tests. Every request is bounded before a
//! consensus read or variable-size hex allocation.
use crate::{
    GetPalwForkChoiceOpeningRequest, GetPalwForkChoiceOpeningResponse, RpcError, RpcHeader, RpcPalwForkChoiceEntry, RpcResult,
};
use kaspa_consensus_core::palw_fork_choice_commitment_v1::{
    PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1, PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2, PalwForkChoiceExtrasRequestV2,
    PalwForkChoiceServedV1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;

pub fn parse_request(request: &GetPalwForkChoiceOpeningRequest) -> RpcResult<(Vec<Hash64>, PalwForkChoiceExtrasRequestV2)> {
    if request.block_hashes.len() > PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1 {
        return Err(RpcError::General(format!("at most {PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1} block hashes per request")));
    }
    if request.registry_bonds.len() > PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2 {
        return Err(RpcError::General(format!("at most {PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2} registry bonds per request")));
    }
    let blocks = request
        .block_hashes
        .iter()
        .map(|text| {
            let text = text.trim();
            if text.len() != 128 {
                return Err(RpcError::General("blockHashes: expected a 64-byte hex hash".into()));
            }
            text.parse::<Hash64>().map_err(|e| RpcError::General(format!("blockHashes: {e}")))
        })
        .collect::<RpcResult<Vec<_>>>()?;
    let registry_bonds = request
        .registry_bonds
        .iter()
        .map(|text| {
            // Borsh TransactionOutpoint = Hash64 (64 bytes) + u32, wrapped by the bond key.
            let text = text.trim();
            if text.len() != 136 {
                return Err(RpcError::General("registryBonds: expected a 68-byte hex borsh bond key".into()));
            }
            let mut bytes = [0u8; 68];
            faster_hex::hex_decode(text.as_bytes(), &mut bytes).map_err(|e| RpcError::General(format!("registryBonds: {e}")))?;
            borsh::from_slice::<PalwBondKeyV2>(&bytes).map_err(|e| RpcError::General(format!("registryBonds: {e}")))
        })
        .collect::<RpcResult<Vec<_>>>()?;
    Ok((blocks, PalwForkChoiceExtrasRequestV2 { rule_e_window_above_blue: request.rule_e_window_above_blue, registry_bonds }))
}

pub fn response(served: Result<PalwForkChoiceServedV1, String>) -> GetPalwForkChoiceOpeningResponse {
    let served = match served {
        Ok(served) => served,
        Err(reason) => return GetPalwForkChoiceOpeningResponse { available: false, reason, ..Default::default() },
    };
    GetPalwForkChoiceOpeningResponse {
        available: true,
        reason: String::new(),
        sink: served.sink.to_string(),
        tips: served.tips.iter().map(|tip| tip.to_string()).collect(),
        entries: served
            .entries
            .into_iter()
            .map(|(block, entry)| match entry {
                Ok(entry) => RpcPalwForkChoiceEntry {
                    block_hash: block.to_string(),
                    available: true,
                    reason: String::new(),
                    header: Some(RpcHeader::from(&entry.header)),
                    leaf: entry.opening.leaf.encode(),
                    inner_root: entry.opening.inner_root.to_string(),
                    committed_root: entry.committed_root.to_string(),
                    committed_form: entry.committed_form,
                    committed_leaf: entry.committed_leaf,
                    rule_e_window: entry.rule_e_window.unwrap_or_default(),
                    registry_openings: entry.registry_openings,
                },
                Err(reason) => RpcPalwForkChoiceEntry { block_hash: block.to_string(), reason, ..Default::default() },
            })
            .collect(),
        dns_overlay: served.dns_gate.is_some(),
        dns_stage_active: served.dns_gate.is_some_and(|gate| gate.stage_active),
        dns_confirmed_anchor: served
            .dns_gate
            .and_then(|gate| gate.confirmed_anchor)
            .map(|(anchor, _)| anchor.to_string())
            .unwrap_or_default(),
        dns_confirmed_anchor_daa: served.dns_gate.and_then(|gate| gate.confirmed_anchor).map(|(_, daa)| daa).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;

    #[test]
    fn exact_bond_bytes_and_bounded_requests_keep_their_identity() {
        let point = Hash64::from_u64_word(7);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(point, 9));
        let encoded = borsh::to_vec(&bond).unwrap();
        assert_eq!(encoded.len(), 68);
        let mut request = GetPalwForkChoiceOpeningRequest {
            block_hashes: vec![point.to_string()],
            rule_e_window_above_blue: Some(123),
            registry_bonds: vec![faster_hex::hex_string(&encoded)],
        };
        let (blocks, extras) = parse_request(&request).unwrap();
        assert_eq!(blocks, vec![point]);
        assert_eq!(extras.registry_bonds, vec![bond]);
        assert_eq!(extras.rule_e_window_above_blue, Some(123));
        request.block_hashes = vec![point.to_string(); PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1 + 1];
        assert!(parse_request(&request).is_err());
        request.block_hashes.clear();
        request.registry_bonds = vec![faster_hex::hex_string(&encoded); PALW_FORK_CHOICE_MAX_REGISTRY_BONDS_V2 + 1];
        assert!(parse_request(&request).is_err());
        for malformed in ["00".repeat(67), "00".repeat(69), "zz".repeat(68)] {
            request.registry_bonds = vec![malformed];
            assert!(parse_request(&request).is_err());
        }
    }

    #[test]
    fn missing_history_is_unavailable_instead_of_an_empty_available_view() {
        let read = response(Err("history is unavailable".into()));
        assert!(!read.available);
        assert!(read.entries.is_empty());
        assert_eq!(read.reason, "history is unavailable");
    }
}
