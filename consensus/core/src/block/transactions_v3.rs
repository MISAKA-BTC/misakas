use super::Transaction;
use crate::{coinbase::CoinbaseAllocationV2, palw_native_resource_v1::NativeCoverageV1};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, sync::Arc};

/// Immutable payload and its local allocation admission share one lifetime.
/// Serialization keeps the existing transaction-vector representation; a local
/// admission ticket is never persisted or supplied by a remote peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockTransactionsV3 {
    transactions: Arc<Vec<Transaction>>,
    #[serde(skip)]
    allocation: Option<Arc<CoinbaseAllocationV2>>,
}

impl Default for BlockTransactionsV3 {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl From<Vec<Transaction>> for BlockTransactionsV3 {
    fn from(transactions: Vec<Transaction>) -> Self {
        Self { transactions: Arc::new(transactions), allocation: None }
    }
}
impl From<Arc<Vec<Transaction>>> for BlockTransactionsV3 {
    fn from(transactions: Arc<Vec<Transaction>>) -> Self {
        Self { transactions, allocation: None }
    }
}
impl Deref for BlockTransactionsV3 {
    type Target = Vec<Transaction>;
    fn deref(&self) -> &Self::Target {
        &self.transactions
    }
}
impl AsRef<Vec<Transaction>> for BlockTransactionsV3 {
    fn as_ref(&self) -> &Vec<Transaction> {
        &self.transactions
    }
}
impl<'a> IntoIterator for &'a BlockTransactionsV3 {
    type Item = &'a Transaction;
    type IntoIter = std::slice::Iter<'a, Transaction>;
    fn into_iter(self) -> Self::IntoIter {
        self.transactions.iter()
    }
}
impl BlockTransactionsV3 {
    pub(super) fn retained_v3(transactions: Vec<Transaction>, allocation: CoinbaseAllocationV2) -> Result<Self, NativeCoverageV1> {
        let allocation = Self::prepare_allocation_v3(allocation)?;
        Ok(Self::from_prepared_v3(transactions, allocation))
    }

    pub(super) fn prepare_allocation_v3(
        mut allocation: CoinbaseAllocationV2,
    ) -> Result<Option<Arc<CoinbaseAllocationV2>>, NativeCoverageV1> {
        Ok(if allocation.is_admitted_v3() {
            // Both Arc headers and the separate allocation-owner object. The
            // transaction/render bytes were admitted before their construction.
            allocation.grow_v2((size_of::<CoinbaseAllocationV2>() + 4 * size_of::<usize>() + size_of::<Vec<Transaction>>()) as u64)?;
            Some(Arc::new(allocation))
        } else {
            None
        })
    }

    pub(super) fn from_prepared_v3(transactions: Vec<Transaction>, allocation: Option<Arc<CoinbaseAllocationV2>>) -> Self {
        Self { transactions: Arc::new(transactions), allocation }
    }

    /// A stale-id repair deep-copies transactions. Obtain a new coinbase ticket
    /// before that copy; a shared ticket would account for only the old bytes.
    /// This does not claim admission of the other transactions or EVM payload.
    pub(super) fn reserve_coinbase_copy_v3(&self) -> Result<CoinbaseAllocationV2, NativeCoverageV1> {
        let mut allocation = CoinbaseAllocationV2::default();
        let Some(tx) = self.transactions.first() else { return Ok(allocation) };
        let mut bytes = size_of::<Transaction>() as u64;
        let mut add = |n: usize| -> Result<(), NativeCoverageV1> {
            bytes = bytes.checked_add(n as u64).ok_or_else(|| NativeCoverageV1::overflow::<Self>("coinbase id repair"))?;
            Ok(())
        };
        add(tx.payload.len())?;
        for input in &tx.inputs {
            add(size_of::<crate::tx::TransactionInput>())?;
            add(input.signature_script.len())?;
        }
        for output in &tx.outputs {
            add(size_of::<crate::tx::TransactionOutput>())?;
            add(output.script_public_key.script().len())?;
        }
        allocation.grow_v2(bytes)?;
        Ok(allocation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{block::Block, config::params::MAINNET_PARAMS, palw_native_resource_v1::*, subnets, tx::*};

    fn transaction() -> Transaction {
        Transaction::new(
            1,
            vec![],
            vec![TransactionOutput::new(7, ScriptPublicKey::new(0, scriptvec![0x51]))],
            0,
            subnets::SUBNETWORK_ID_COINBASE,
            0,
            vec![1, 2, 3],
        )
    }

    #[test]
    fn retained_body_wire_is_the_existing_vector() {
        let txs = vec![transaction()];
        let old = bincode::serialize(&Arc::new(txs.clone())).unwrap();
        let body = BlockTransactionsV3::from(txs);
        assert_eq!(bincode::serialize(&body).unwrap(), old);
        let decoded: BlockTransactionsV3 = bincode::deserialize(&old).unwrap();
        assert_eq!(decoded[0].id(), body[0].id());
        assert!(decoded.allocation.is_none(), "a persisted body never grants a local ticket");
    }

    #[test]
    fn retained_body_capacity_lifetime() {
        let worker = format!("{}::retained_body_capacity_worker", module_path!().split_once("::").unwrap().1);
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
            .env("MISAKA_RETAINED_BODY_CHILD", "1")
            .status()
            .unwrap();
        assert!(result.success());
    }

    #[test]
    fn retained_body_capacity_worker() {
        if std::env::var("MISAKA_RETAINED_BODY_CHILD").as_deref() != Ok("1") {
            return;
        }
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 16384, database_bytes: 0 }));
        let mut allocation = CoinbaseAllocationV2::default();
        allocation.grow_v2(8192).unwrap();
        let body = BlockTransactionsV3::retained_v3(vec![transaction()], allocation).unwrap();
        let cached = body.clone();
        assert!(Arc::ptr_eq(&body.transactions, &cached.transactions));
        drop(body);
        assert!(reserve_native_requests_v1(8192).is_err(), "body cache holds the render admission");
        let block =
            Block { header: Arc::new((&MAINNET_PARAMS.genesis).into()), transactions: cached, evm_payload: Default::default() };
        let (block, stale) = block.with_current_tx_ids().unwrap();
        assert_eq!(stale, 0);
        assert!(reserve_native_requests_v1(8192).is_err(), "no-op repair must retain the ticket");
        drop(block);
        drop(reserve_native_requests_v1(16384).unwrap());

        let mut tx = transaction();
        tx.payload.push(4); // Deliberately stale local id, as in the native submit path.
        let body = BlockTransactionsV3::from(vec![tx]);
        let block = Block { header: Arc::new((&MAINNET_PARAMS.genesis).into()), transactions: body, evm_payload: Default::default() };
        let occupied = reserve_native_requests_v1(16384).unwrap();
        assert!(block.clone().with_current_tx_ids().is_err(), "refuse before deep repair allocation");
        drop(occupied);
        let old = block.clone();
        let (repaired, stale) = block.with_current_tx_ids().unwrap();
        assert_eq!(stale, 1);
        assert!(repaired.transactions[0].id_is_current());
        assert!(!old.transactions[0].id_is_current());
        drop(old);
        assert!(reserve_native_requests_v1(16384).is_err(), "repair has its own live ticket");
        drop(repaired);
        drop(reserve_native_requests_v1(16384).unwrap());
    }
}
