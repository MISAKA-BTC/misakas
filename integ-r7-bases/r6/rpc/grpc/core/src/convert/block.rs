use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{RpcError, RpcHash};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcBlock, protowire::RpcBlock, {
    Self {
        header: Some(protowire::RpcBlockHeader::from(&item.header)),
        transactions: item.transactions.iter().map(protowire::RpcTransaction::from).collect(),
        verbose_data: item.verbose_data.as_ref().map(|x| x.into()),
        evm_payload: item.evm_payload.clone(),
    }
});

from!(item: &kaspa_rpc_core::RpcRawBlock, protowire::RpcBlock, {
    Self {
        header: Some(protowire::RpcBlockHeader::from(&item.header)),
        transactions: item.transactions.iter().map(protowire::RpcTransaction::from).collect(),
        verbose_data: None,
        evm_payload: item.evm_payload.clone(),
    }
});

from!(item: &kaspa_rpc_core::RpcBlockVerboseData, protowire::RpcBlockVerboseData, {
    Self {
        hash: item.hash.to_string(),
        difficulty: item.difficulty,
        selected_parent_hash: item.selected_parent_hash.to_string(),
        transaction_ids: item.transaction_ids.iter().map(|x| x.to_string()).collect(),
        is_header_only: item.is_header_only,
        blue_score: item.blue_score,
        children_hashes: item.children_hashes.iter().map(|x| x.to_string()).collect(),
        merge_set_blues_hashes: item.merge_set_blues_hashes.iter().map(|x| x.to_string()).collect(),
        merge_set_reds_hashes: item.merge_set_reds_hashes.iter().map(|x| x.to_string()).collect(),
        palw_merge_view: item.palw_merge_view.as_ref().map(|view| protowire::RpcPalwMergeView {
            round_blocks: view.round_blocks.iter().map(|hash| hash.to_string()).collect(),
            genuine_red_blocks: view.genuine_red_blocks.iter().map(|hash| hash.to_string()).collect(),
        }),
        is_chain_block: item.is_chain_block,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBlock, kaspa_rpc_core::RpcOptionalBlock, {
    Self {
        header: item
            .header
            .as_ref()
            .map(kaspa_rpc_core::RpcOptionalHeader::try_from)
            .transpose()?,
        transactions: item.transactions.iter().map(kaspa_rpc_core::RpcOptionalTransaction::try_from).collect::<Result<Vec<_>, _>>()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcBlockVerboseData::try_from).transpose()?,
    }
});

try_from!(item: &protowire::RpcBlock, kaspa_rpc_core::RpcBlock, {
    Self {
        header: item
            .header
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?
            .try_into()?,
        transactions: item.transactions.iter().map(kaspa_rpc_core::RpcTransaction::try_from).collect::<Result<Vec<_>, _>>()?,
        verbose_data: item.verbose_data.as_ref().map(kaspa_rpc_core::RpcBlockVerboseData::try_from).transpose()?,
        evm_payload: item.evm_payload.clone(),
    }
});

try_from!(item: &protowire::RpcBlock, kaspa_rpc_core::RpcRawBlock, {
    Self {
    header: kaspa_rpc_core::RpcRawHeader::try_from(item.header.as_ref().ok_or(RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?)?,
    transactions: item.transactions.iter().map(kaspa_rpc_core::RpcTransaction::try_from).collect::<Result<Vec<_>, _>>()?,
    evm_payload: item.evm_payload.clone(),
    }
});

try_from!(item: &protowire::RpcBlockVerboseData, kaspa_rpc_core::RpcBlockVerboseData, {
    Self {
        hash: RpcHash::from_str(&item.hash)?,
        difficulty: item.difficulty,
        selected_parent_hash: RpcHash::from_str(&item.selected_parent_hash)?,
        // PR-9.5c/f: transaction ids widened to Hash64.
        transaction_ids: item
            .transaction_ids
            .iter()
            .map(|x| kaspa_consensus_core::Hash64::from_str(x))
            .collect::<Result<Vec<kaspa_consensus_core::Hash64>, faster_hex::Error>>()?,
        is_header_only: item.is_header_only,
        blue_score: item.blue_score,
        children_hashes: item
            .children_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        merge_set_blues_hashes: item
            .merge_set_blues_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        merge_set_reds_hashes: item
            .merge_set_reds_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<kaspa_rpc_core::RpcHash>, faster_hex::Error>>()?,
        is_chain_block: item.is_chain_block,
        // ADR-0125 semantic amendment: the optional semantic view is additive in gRPC.
        palw_merge_view: item.palw_merge_view.as_ref().map(|view| {
            Ok::<_, RpcError>(kaspa_rpc_core::RpcPalwMergeView {
                round_blocks: view.round_blocks.iter().map(|hash| RpcHash::from_str(hash)).collect::<Result<_, _>>()?,
                genuine_red_blocks: view.genuine_red_blocks.iter().map(|hash| RpcHash::from_str(hash)).collect::<Result<_, _>>()?,
            })
        }).transpose()?,
        // ADR-0165: block_kind is still JSON/serde only.
        block_kind: String::new(),
        // Lane SCAN: not carried over gRPC (wRPC only).
        lane_class: String::new(),
        exec: None,
    }
});

#[cfg(test)]
mod adr0125_semantic_merge_view_tests {
    use super::*;
    use prost::Message;

    #[test]
    fn grpc_preserves_mixed_raw_reds_and_the_additive_semantic_view() {
        let hash = |n| RpcHash::from_u64_word(n).to_string();
        let old = protowire::RpcBlockVerboseData {
            hash: hash(1),
            selected_parent_hash: hash(2),
            merge_set_blues_hashes: vec![hash(2)],
            merge_set_reds_hashes: vec![hash(3), hash(4)],
            ..Default::default()
        };
        let mut data = kaspa_rpc_core::RpcBlockVerboseData::try_from(&old).unwrap();
        assert!(data.palw_merge_view.is_none(), "older gRPC values remain readable");
        data.palw_merge_view = Some(kaspa_rpc_core::RpcPalwMergeView {
            round_blocks: vec![RpcHash::from_u64_word(3)],
            genuine_red_blocks: vec![RpcHash::from_u64_word(4)],
        });
        let wire = protowire::RpcBlockVerboseData::from(&data);
        let decoded = protowire::RpcBlockVerboseData::decode(wire.encode_to_vec().as_slice()).unwrap();
        let result = kaspa_rpc_core::RpcBlockVerboseData::try_from(&decoded).unwrap();
        assert_eq!(result.palw_merge_view, data.palw_merge_view);
        assert_eq!(result.merge_set_reds_hashes, data.merge_set_reds_hashes);
        assert_eq!(result.merge_set_blues_hashes, data.merge_set_blues_hashes);
    }
}
