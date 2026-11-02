//! Local RPC source ownership, never serialized into a block or RPC wire field.
use super::{Block, TransactionId};
use kaspa_core::memory_budget::MemoryLease;

#[derive(Debug, Default)]
pub struct RetainedTransactionIdsV1 {
    pub ids: Vec<TransactionId>,
    pub memory: Option<MemoryLease>,
}
/// The UTXO body is deliberately absent. Header/EVM source reads remain the
/// ordinary getters and are not covered by the ID-only allocation ticket.
#[derive(Debug)]
pub struct RpcBlockProjectionV1 {
    pub block: Block,
    pub transaction_ids: RetainedTransactionIdsV1,
}
