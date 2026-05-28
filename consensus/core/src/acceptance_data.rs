use serde::{Deserialize, Serialize};

use crate::{BlockHash, tx::TransactionId};

/// Holds a mergeset acceptance data, a list of all its merged block with their accepted transactions
pub type AcceptanceData = Vec<MergesetBlockAcceptanceData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Holds a merged block with its accepted transactions
pub struct MergesetBlockAcceptanceData {
    pub block_hash: BlockHash,
    pub accepted_transactions: Vec<AcceptedTxEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedTxEntry {
    pub transaction_id: TransactionId,
    pub index_within_block: u32,
}
