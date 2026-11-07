use crate::tx::{ScriptPublicKey, Transaction};
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct MinerData<T: AsRef<[u8]> = Vec<u8>> {
    pub script_public_key: ScriptPublicKey,
    pub extra_data: T,
}

impl<T: AsRef<[u8]>> MinerData<T> {
    pub fn new(script_public_key: ScriptPublicKey, extra_data: T) -> Self {
        Self { script_public_key, extra_data }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct CoinbaseData<T: AsRef<[u8]> = Vec<u8>> {
    pub blue_score: u64,
    pub subsidy: u64,
    pub miner_data: MinerData<T>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BlockRewardData {
    pub subsidy: u64,
    pub total_fees: u64,
    /// kaspa-pq ADR-0018 §F bridge wiring: the subset of `total_fees` paid by **finality-class**
    /// txs — accepted txs creating ≥1 `EVM_DEPOSIT_LOCK` output (ADR-0020 §9.2), classified at fee
    /// accumulation in `calculate_utxo_state` (shared by coinbase construction AND validation, so
    /// c==v holds) and gated on `DnsParams::finality_fee_activation_daa_score`. Split at the
    /// validator-primary `split_finality_fees` ratios; the normal-class part is `total_fees −
    /// finality_fees`. `0` below the fence ⇒ all splits byte-identical to the pre-wiring math.
    /// Invariant: `finality_fees ≤ total_fees`. NOTE: `BlockRewardData` is persisted inside
    /// `VirtualState` (serde/bincode, not self-describing) — adding this field is a store-format
    /// change riding the ADR-0007 Phase-3 re-genesis. On mainnet/testnet an un-wiped data dir is
    /// caught by the startup genesis-mismatch guard (their genesis hash changed in Phase 3); on
    /// devnet/simnet (genesis unchanged) the old `VirtualState` fails to DECODE instead — surfaced
    /// as the actionable "store format changed — wipe the data directory" error in
    /// `DbVirtualStateStore::new`. Either way no old data dir can be silently resumed.
    pub finality_fees: u64,
    pub script_public_key: ScriptPublicKey,
}

impl BlockRewardData {
    pub fn new(subsidy: u64, total_fees: u64, finality_fees: u64, script_public_key: ScriptPublicKey) -> Self {
        Self { subsidy, total_fees, finality_fees, script_public_key }
    }
}

/// Holds a coinbase transaction along with meta-data obtained during creation
pub struct CoinbaseTransactionTemplate {
    pub tx: Transaction,
    pub has_red_reward: bool,
    /// Coinbase output indices whose script belongs to the current block miner
    /// and must be rewritten when `MinerData::script_public_key` changes.
    /// Currently this includes the aggregate red reward and the worker-inclusion
    /// bounty; validator/reserve outputs can be interleaved between them.
    pub miner_script_output_indices: Vec<usize>,
}
