use crate::tx::{ScriptPublicKey, Transaction};
use serde::{Deserialize, Serialize};

/// Fixed fields preceding the variable script in a canonical coinbase payload.
pub const COINBASE_PAYLOAD_FIXED_PREFIX_LEN: usize = 8 + 8 + 2 + 1;

/// Locate the extra data without allocating. The node's coinbase validator
/// also enforces the network's payload and script length limits.
pub fn coinbase_payload_extra_data(payload: &[u8]) -> Option<&[u8]> {
    let script_len = usize::from(*payload.get(COINBASE_PAYLOAD_FIXED_PREFIX_LEN - 1)?);
    payload.get(COINBASE_PAYLOAD_FIXED_PREFIX_LEN + script_len..)
}

/// A PALW marker is the whole miner extra-data prefix, including when an
/// EXEC v2 anchor follows it.
pub fn palw_coinbase_marker(extra_data: &[u8]) -> Option<[u8; 64]> {
    extra_data.try_into().ok().or_else(|| {
        crate::palw_exec_v2_anchor::palw_exec_v2_anchor_split(extra_data).ok().and_then(|(prefix, _)| prefix.try_into().ok())
    })
}

pub const MOE2_EXEC_AWARD_MARKER_TAG: &[u8; 8] = b"MOE2AW01";

/// Read the private MOE2 award tag and its commitment. An EXEC v2 anchor
/// may follow the miner's extra data. Bare 64-byte markers retain their
/// original meaning even if their digest starts with the award tag.
pub fn private_moe2_award_marker(extra_data: &[u8]) -> Result<Option<[u8; 64]>, &'static str> {
    let Some(tagged) = extra_data.strip_prefix(MOE2_EXEC_AWARD_MARKER_TAG) else { return Ok(None) };
    if extra_data.len() == 64 {
        return Ok(None);
    }
    if let Ok(commitment) = tagged.try_into() {
        return Ok(Some(commitment));
    }
    let (prefix, anchor) = crate::palw_exec_v2_anchor::palw_exec_v2_anchor_split(extra_data)
        .map_err(|_| "invalid EXEC v2 trailer after the private MOE2 award tag")?;
    if anchor.is_some() && prefix.len() == 64 {
        return Ok(None);
    }
    prefix
        .strip_prefix(MOE2_EXEC_AWARD_MARKER_TAG)
        .and_then(|data| data.try_into().ok())
        .map(Some)
        .ok_or("invalid private MOE2 award marker length")
}

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
    /// Reservation for the owning coinbase render. Transfer with the transaction
    /// into the block template; borrowing the queue did not admit this copy.
    pub allocation_v2: CoinbaseAllocationV2,
}

/// Typed coinbase allocation requests, not whole-block or physical RSS coverage.
/// Deliberately not Clone: a deep copy must obtain its own reservation first.
#[derive(Default)]
pub struct CoinbaseAllocationV2 {
    lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl std::fmt::Debug for CoinbaseAllocationV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoinbaseAllocationV2").field("admitted", &self.lease.is_some()).finish()
    }
}
impl CoinbaseAllocationV2 {
    pub(crate) fn is_admitted_v3(&self) -> bool {
        self.lease.is_some()
    }

    pub fn grow_v2(&mut self, requests: u64) -> Result<(), crate::palw_native_resource_v1::NativeCoverageV1> {
        match self.lease.as_mut() {
            Some(lease) => lease.grow_memory_v1(requests),
            None => {
                self.lease = crate::palw_native_resource_v1::reserve_native_requests_v1(requests)?;
                Ok(())
            }
        }
    }
}
