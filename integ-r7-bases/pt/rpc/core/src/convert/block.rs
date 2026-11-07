//! Conversion of Block related types

use std::sync::Arc;

use crate::{RpcBlock, RpcError, RpcOptionalBlock, RpcOptionalTransaction, RpcRawBlock, RpcResult, RpcTransaction};
use kaspa_consensus_core::block::{Block, MutableBlock};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
            evm_payload: if item.evm_payload.is_empty() { Vec::new() } else { item.evm_payload.payload_bytes() },
        }
    }
}

impl From<&Block> for RpcRawBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            evm_payload: if item.evm_payload.is_empty() { Vec::new() } else { item.evm_payload.payload_bytes() },
        }
    }
}

impl From<&MutableBlock> for RpcBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            verbose_data: None,
            evm_payload: if item.evm_payload.is_empty() { Vec::new() } else { item.evm_payload.payload_bytes() },
        }
    }
}

impl From<&MutableBlock> for RpcRawBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: item.header.as_ref().into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
            evm_payload: if item.evm_payload.is_empty() { Vec::new() } else { item.evm_payload.payload_bytes() },
        }
    }
}

impl From<MutableBlock> for RpcRawBlock {
    fn from(item: MutableBlock) -> Self {
        Self {
            evm_payload: if item.evm_payload.is_empty() { Vec::new() } else { item.evm_payload.payload_bytes() },
            header: item.header.into(),
            transactions: item.transactions.iter().map(RpcTransaction::from).collect(),
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(item.header.try_into()?),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
            // kaspa-pq EVM Lane v0.4: decode the canonical borsh payload bytes
            // (body validation re-derives evm_payload_hash from them).
            evm_payload: Arc::new(decode_evm_payload(&item.evm_payload)?),
        })
    }
}

impl TryFrom<RpcRawBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcRawBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(item.header.try_into()?),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
            // kaspa-pq EVM Lane v0.4: decode the canonical borsh payload bytes.
            evm_payload: Arc::new(decode_evm_payload(&item.evm_payload)?),
        })
    }
}

/// Decode the wire payload bytes (canonical borsh; empty = empty payload).
fn decode_evm_payload(bytes: &[u8]) -> RpcResult<kaspa_consensus_core::evm::EvmExecutionPayload> {
    if bytes.is_empty() {
        return Ok(Default::default());
    }
    borsh::from_slice(bytes).map_err(|_| RpcError::RpcSubsystem("malformed EVM payload bytes".to_string()))
}

// ----------------------------------------------------------------------------
// consensus_core to optional rpc_core
// ----------------------------------------------------------------------------

impl From<&Block> for RpcOptionalBlock {
    fn from(item: &Block) -> Self {
        Self {
            header: Some(item.header.as_ref().into()),
            transactions: item.transactions.iter().map(RpcOptionalTransaction::from).collect(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&MutableBlock> for RpcOptionalBlock {
    fn from(item: &MutableBlock) -> Self {
        Self {
            header: Some(item.header.as_ref().into()),
            transactions: item.transactions.iter().map(RpcOptionalTransaction::from).collect(),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// optional rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcOptionalBlock> for Block {
    type Error = RpcError;
    fn try_from(item: RpcOptionalBlock) -> RpcResult<Self> {
        Ok(Self {
            header: Arc::new(
                (item.header.ok_or(RpcError::MissingRpcFieldError("RpcBlock".to_string(), "header".to_string()))?).try_into()?,
            ),
            transactions: Arc::new(
                item.transactions
                    .into_iter()
                    .map(kaspa_consensus_core::tx::Transaction::try_from)
                    .collect::<RpcResult<Vec<kaspa_consensus_core::tx::Transaction>>>()?,
            ),
            // ADR-0020: RpcBlock carries no EVM payload in P1; default to empty.
            evm_payload: Default::default(),
        })
    }
}
