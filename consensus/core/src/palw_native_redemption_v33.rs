//! Native credit redemption: a real coinbase leaf and the exact committed
//! queue row, before and after its one-shot drain. Header chain/fork choice
//! and UTXO validation are independently anchored by the SDK/node callers.
use crate::palw_native_audit_proof_v32::VerifiedNativePayoutRowsV32;
use crate::palw_native_audit_v27::CheckedNativeExecutionAuditViewV27;
use crate::{
    Hash64,
    config::params::Params,
    header::Header,
    tx::{Transaction, TransactionOutpoint},
};
use kaspa_hashes::{HasherBase, MerkleBranchHash64, ZERO_HASH64};
use misaka_palw_kernel::hash::{Digest, object_id};

pub const NATIVE_REDEMPTION_MAX_TRANSACTIONS_V33: usize = 65_536;
pub const NATIVE_REDEMPTION_MAX_COINBASE_BYTES_V33: usize = 4 << 20;

/// Raw transport only. Deserialization does not mint a checked redemption.
/// IO must bound nested transaction counts before decoding untrusted bytes.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, borsh::BorshSerialize)]
pub struct NativeCoinbaseProofV33 {
    pub version: u16,
    pub block: Hash64,
    pub transactions: u32,
    pub coinbase: Transaction,
    /// Index zero at every level; missing right subtrees are ZERO_HASH64,
    /// exactly as merkle.rs, rather than recursively hashed zero leaves.
    pub siblings: Vec<Hash64>,
}

fn branch(left: Hash64, right: Hash64) -> Hash64 {
    let mut h = MerkleBranchHash64::new();
    h.update(left.as_byte_slice()).update(right.as_byte_slice());
    h.finalize()
}
fn coinbase_extent(tx: &Transaction) -> Result<(), String> {
    if !tx.is_coinbase()
        || !tx.inputs.is_empty()
        || tx.outputs.len() > 16_384
        || tx.payload.len() > NATIVE_REDEMPTION_MAX_COINBASE_BYTES_V33
        || !tx.id_is_current()
    {
        return Err("native redemption33 canonical bounded coinbase differs".into());
    }
    let bytes = tx
        .outputs
        .iter()
        .try_fold(tx.payload.len(), |n, o| n.checked_add(12)?.checked_add(o.script_public_key.script().len()))
        .ok_or("native redemption33 coinbase extent overflow")?;
    if bytes > NATIVE_REDEMPTION_MAX_COINBASE_BYTES_V33 {
        return Err("native redemption33 coinbase byte bound".into());
    }
    Ok(())
}
fn leaf(tx: &Transaction, header: &Header, params: &Params) -> Hash64 {
    if params.crescendo_activation.is_active(header.daa_score) {
        crate::hashing::tx::hash(tx)
    } else {
        crate::hashing::tx::hash_pre_crescendo(tx)
    }
}
/// Build from the actual validated block body. No caller-supplied Merkle
/// root or producer flags enter this factory. Preflight precedes cloning.
pub fn prove_native_coinbase_v33<'a>(
    header: &Header,
    pinned_block: Hash64,
    parameters: &Params,
    transactions: impl ExactSizeIterator<Item = &'a Transaction>,
) -> Result<NativeCoinbaseProofV33, String> {
    if crate::hashing::header::hash(header) != pinned_block
        || transactions.len() == 0
        || transactions.len() > NATIVE_REDEMPTION_MAX_TRANSACTIONS_V33
    {
        return Err("native redemption33 original block/count differs".into());
    }
    let count = transactions.len();
    let mut iter = transactions;
    let coinbase = iter.next().ok_or("native redemption33 coinbase absent")?;
    coinbase_extent(coinbase)?;
    let mut level = Vec::with_capacity(count);
    level.push(Some(leaf(coinbase, header, parameters)));
    for tx in iter {
        if tx.is_coinbase() || !tx.id_is_current() {
            return Err("native redemption33 duplicate coinbase/stale transaction identity".into());
        }
        level.push(Some(leaf(tx, header, parameters)));
    }
    level.resize(count.next_power_of_two(), None);
    let mut siblings = Vec::new();
    while level.len() > 1 {
        siblings.push(level[1].unwrap_or(ZERO_HASH64));
        level = level.chunks_exact(2).map(|p| p[0].map(|left| branch(left, p[1].unwrap_or(ZERO_HASH64)))).collect();
    }
    if level[0] != Some(header.hash_merkle_root) {
        return Err("native redemption33 actual block body Merkle root differs".into());
    }
    Ok(NativeCoinbaseProofV33 { version: 33, block: pinned_block, transactions: count as u32, coinbase: coinbase.clone(), siblings })
}

pub struct VerifiedNativeCoinbaseV33<'a> {
    block: Hash64,
    daa: u64,
    transaction: &'a Transaction,
}
impl VerifiedNativeCoinbaseV33<'_> {
    pub fn block(&self) -> Hash64 {
        self.block
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
    pub fn transaction(&self) -> &Transaction {
        self.transaction
    }
}
pub fn verify_native_coinbase_v33<'a>(
    proof: &'a NativeCoinbaseProofV33,
    header: &Header,
    pinned_block: Hash64,
    parameters: &Params,
) -> Result<VerifiedNativeCoinbaseV33<'a>, String> {
    let count = proof.transactions as usize;
    if proof.version != 33
        || proof.block != pinned_block
        || crate::hashing::header::hash(header) != pinned_block
        || count == 0
        || count > NATIVE_REDEMPTION_MAX_TRANSACTIONS_V33
        || proof.siblings.len() != count.next_power_of_two().trailing_zeros() as usize
    {
        return Err("native redemption33 original header/count/path grammar differs".into());
    }
    coinbase_extent(&proof.coinbase)?;
    let mut hash = leaf(&proof.coinbase, header, parameters);
    for (depth, sibling) in proof.siblings.iter().enumerate() {
        if count <= (1usize << depth) && *sibling != ZERO_HASH64 {
            return Err("native redemption33 absent right subtree must be zero".into());
        }
        hash = branch(hash, *sibling);
    }
    if hash != header.hash_merkle_root {
        return Err("native redemption33 coinbase is not included in pinned block".into());
    }
    Ok(VerifiedNativeCoinbaseV33 { block: pinned_block, daa: header.daa_score, transaction: &proof.coinbase })
}

/// Private mint. This proves delivery of the original credit into a real
/// coinbase outpoint; maturity, current unspentness and spending are separate.
pub struct CheckedNativeRedemptionV33 {
    block: Hash64,
    outpoint: TransactionOutpoint,
    job: Digest,
    credit: Digest,
    amount: u64,
    payload: Digest,
    before: Hash64,
    after: Hash64,
}
impl CheckedNativeRedemptionV33 {
    pub fn block(&self) -> Hash64 {
        self.block
    }
    pub fn outpoint(&self) -> TransactionOutpoint {
        self.outpoint
    }
    pub fn amount(&self) -> u64 {
        self.amount
    }
    pub fn job(&self) -> Digest {
        self.job
    }
    pub fn credit(&self) -> Digest {
        self.credit
    }
    pub fn payload(&self) -> Digest {
        self.payload
    }
    pub fn before_state(&self) -> Hash64 {
        self.before
    }
    pub fn after_state(&self) -> Hash64 {
        self.after
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/native/exact-coinbase-redemption/v33",
            &(self.block, self.outpoint, self.job, self.credit, self.amount, self.payload, self.before, self.after),
        )
    }
}
/// Caller authenticates the before/after roots as the exact selected-parent
/// states immediately bracketing `coinbase.block`. Complete queue openings
/// prove original canonical rank and consumption, not a bare absence flag.
pub fn derive_native_redemption_v33(
    final_view: &CheckedNativeExecutionAuditViewV27,
    before: &VerifiedNativePayoutRowsV32<'_>,
    after: &VerifiedNativePayoutRowsV32<'_>,
    coinbase: &VerifiedNativeCoinbaseV33<'_>,
    output_index: u32,
) -> Result<CheckedNativeRedemptionV33, String> {
    let r = final_view.record();
    if r.state_root != before.state_root().as_bytes()
        || before.state_root() == after.state_root()
        || r.established_daa > coinbase.daa
        || r.producer_credit == 0
    {
        return Err("native redemption33 original Final/before/after/clock differs".into());
    }
    let key = borsh::to_vec(&Hash64::from_bytes(r.payout_queue)).map_err(|e| e.to_string())?;
    let rank =
        before.rows().iter().position(|(k, _)| *k == key).ok_or("native redemption33 original credit queue absent before payment")?;
    if rank >= crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK || after.rows().iter().any(|(k, _)| *k == key) {
        return Err("native redemption33 queue not in original drain prefix or not consumed".into());
    }
    let row: crate::palw_state_v2::PalwPayoutV2 =
        borsh::from_slice(&before.rows()[rank].1).map_err(|_| "native redemption33 bounded canonical payout")?;
    // A unique new-contract credit cannot be substituted by a pooled amount.
    if before.rows()[rank].1.len() != 72
        || row.payload.as_bytes() != r.payout_payload
        || row.amount != r.producer_credit
        || r.observed_queue_amount != Some(row.amount)
    {
        return Err("native redemption33 exact original credit/payout amount differs".into());
    }
    let expected = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&row.payload.as_bytes());
    // Namespace 28 keeps its own Stage28 credit identifier (fixed before its settlement
    // exists); the privately minted audit view already authenticated it against the
    // original header and immutable kind-14 receipt. Every other namespace re-derives
    // the shared unique Final27 identity here.
    if r.namespace != 28
        && Hash64::from_bytes(r.payout_queue)
            != crate::palw_native_audit_v27::native_final_credit_queue_v27(
            r.job,
            r.funding,
            r.settlement,
            Hash64::from_bytes(r.payout_payload),
        )
    {
        return Err("native redemption33 original unique credit queue identity differs".into());
    }
    let start =
        (output_index as usize).checked_sub(rank).ok_or("native redemption33 output does not preserve original queue order")?;
    for (offset, (_, raw)) in before.rows().iter().take(crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK).enumerate() {
        // Other legacy pooled keys may legitimately be enqueued again by
        // this block. Their post-state presence is not a second delivery of
        // our unique original credit. Authenticate the complete paid prefix
        // and require absence only of this non-reusable original job key.
        if raw.len() != 72 {
            return Err("native redemption33 bounded original full drain prefix differs".into());
        }
        let queued: crate::palw_state_v2::PalwPayoutV2 =
            borsh::from_slice(raw).map_err(|_| "native redemption33 bounded drain prefix grammar")?;
        let actual = coinbase
            .transaction
            .outputs
            .get(start + offset)
            .ok_or("native redemption33 complete original drain output prefix absent")?;
        if actual.value != queued.amount
            || actual.script_public_key != crate::mldsa87_primitives::p2pkh_mldsa87_spk(&queued.payload.as_bytes())
        {
            return Err("native redemption33 complete original drain output prefix differs".into());
        }
    }
    let output = coinbase.transaction.outputs.get(output_index as usize).ok_or("native redemption33 real output index absent")?;
    if output.value != row.amount || output.script_public_key != expected {
        return Err("native redemption33 original payout script/value differs".into());
    }
    Ok(CheckedNativeRedemptionV33 {
        block: coinbase.block,
        outpoint: TransactionOutpoint::new(coinbase.transaction.id(), output_index),
        job: r.job,
        credit: r.credit,
        amount: row.amount,
        payload: r.payout_payload,
        before: before.state_root(),
        after: after.state_root(),
    })
}

// ---- Owning39 payout and redemption (RFC-0002 §II.14 "real payout and redemption") ----
//
// Four separate facts, never inferred from one another:
//   1. independent Final settlement and its single credit — `CheckedNativeJobAuditV39`;
//   2. the pending payout row at one anchored state — `native_payout_status_v39`;
//   3. actual coinbase delivery — `derive_native_redemption_v39` over the two
//      states bracketing the delivering block plus its real coinbase leaf;
//   4. an actual UTXO spend — `verify_native_accepted_spend_v35` over the
//      redemption this returns (unchanged).
// If an auditor first meets the Final after the pending row is already gone,
// the status is `ConsumedDeliveryWitnessRequired`: delivery is never presumed
// from absence; a genuine bracketing witness must be supplied.

/// Where the original single credit stands in ONE anchored state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePayoutStatusV39 {
    /// No settlement yet: no credit exists.
    NotSettled,
    /// The exact unique credit row is pending in this state's payout queue.
    Pending { rank: usize, amount: u64 },
    /// Settled, but the unique queue row is absent here: consumed by some
    /// earlier block. A genuine delivery witness is required.
    ConsumedDeliveryWitnessRequired,
}
fn credit_row_v39(
    credit: &crate::palw_native_job_v39::NativeSingleCreditV39,
    rows: &VerifiedNativePayoutRowsV32<'_>,
) -> Result<Option<(usize, crate::palw_state_v2::PalwPayoutV2)>, String> {
    let key = borsh::to_vec(&credit.queue).map_err(|e| e.to_string())?;
    let Some(rank) = rows.rows().iter().position(|(k, _)| *k == key) else { return Ok(None) };
    let raw = &rows.rows()[rank].1;
    if raw.len() != 72 {
        return Err("native39 payout row fixed extent differs".into());
    }
    let row: crate::palw_state_v2::PalwPayoutV2 = borsh::from_slice(raw).map_err(|_| "native39 payout row grammar")?;
    if row.payload != credit.payout || row.amount != credit.amount {
        return Err("native39 pending payout differs from the original single credit".into());
    }
    Ok(Some((rank, row)))
}
pub fn native_payout_status_v39(
    job: &crate::palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    rows: &VerifiedNativePayoutRowsV32<'_>,
) -> Result<NativePayoutStatusV39, String> {
    if rows.state_root() != job.state_root() {
        return Err("native39 payout rows and job audit are not the same anchored state".into());
    }
    let Some(credit) = job.credit() else { return Ok(NativePayoutStatusV39::NotSettled) };
    Ok(match credit_row_v39(credit, rows)? {
        Some((rank, row)) => NativePayoutStatusV39::Pending { rank, amount: row.amount },
        None => NativePayoutStatusV39::ConsumedDeliveryWitnessRequired,
    })
}
/// Actual coinbase delivery of an owning39 single credit. `before` is the
/// job audit at the selected-parent state immediately preceding the paying
/// block (its credit row must be the same immutable credit the Final audit
/// names), with that state's complete payout queue; `after` is the queue of
/// the paying block's own post-state. Only the unique original credit key
/// must be consumed; the whole canonical drain prefix must match the coinbase
/// outputs in order. Returns the shared private redemption, so the unchanged
/// spend35 verifier proves a real UTXO spend of exactly this outpoint.
pub fn derive_native_redemption_v39(
    final_audit: &crate::palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    before_audit: &crate::palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    before: &VerifiedNativePayoutRowsV32<'_>,
    after: &VerifiedNativePayoutRowsV32<'_>,
    coinbase: &VerifiedNativeCoinbaseV33<'_>,
    output_index: u32,
) -> Result<CheckedNativeRedemptionV33, String> {
    let credit = final_audit.credit().ok_or("native39 redemption requires the original single credit")?;
    let witness = before_audit.credit().ok_or("native39 delivery witness state lacks the original single credit")?;
    if witness != credit
        || final_audit.original() != before_audit.original()
        || before.state_root() != before_audit.state_root()
        || before.state_root() == after.state_root()
        || credit.amount == 0
        || credit.established > coinbase.daa()
        // The witness is EXACTLY the paying block's selected-parent state: the state its own header anchors. (Was `daa <
        // coinbase.daa`, which both admits a stale witness from further back and refuses a real parent of equal DAA score.)
        || before_audit.anchor_child() != coinbase.block()
    {
        return Err("native39 delivery witness is not the bracketing state of the same immutable credit".into());
    }
    let (rank, row) = credit_row_v39(credit, before)?.ok_or("native39 original credit was not pending before the paying block")?;
    let key = borsh::to_vec(&credit.queue).map_err(|e| e.to_string())?;
    if rank >= crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK || after.rows().iter().any(|(k, _)| *k == key) {
        return Err("native39 credit not in the original drain prefix or not consumed by the paying block".into());
    }
    let start = (output_index as usize).checked_sub(rank).ok_or("native39 output does not preserve original queue order")?;
    for (offset, (_, raw)) in before.rows().iter().take(crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK).enumerate() {
        if raw.len() != 72 {
            return Err("native39 bounded original drain prefix differs".into());
        }
        let queued: crate::palw_state_v2::PalwPayoutV2 = borsh::from_slice(raw).map_err(|_| "native39 drain prefix grammar")?;
        let actual = coinbase.transaction.outputs.get(start + offset).ok_or("native39 complete drain output prefix absent")?;
        if actual.value != queued.amount
            || actual.script_public_key != crate::mldsa87_primitives::p2pkh_mldsa87_spk(&queued.payload.as_bytes())
        {
            return Err("native39 complete original drain output prefix differs".into());
        }
    }
    let output = coinbase.transaction.outputs.get(output_index as usize).ok_or("native39 real output index absent")?;
    if output.value != row.amount || output.script_public_key != crate::mldsa87_primitives::p2pkh_mldsa87_spk(&row.payload.as_bytes()) {
        return Err("native39 original payout script/value differs".into());
    }
    Ok(CheckedNativeRedemptionV33 {
        block: coinbase.block,
        outpoint: TransactionOutpoint::new(coinbase.transaction.id(), output_index),
        job: credit.job,
        credit: credit.root(),
        amount: row.amount,
        payload: credit.payout.as_bytes(),
        before: before.state_root(),
        after: after.state_root(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{DEVNET_PARAMS, ForkActivation};
    use crate::subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE};

    fn body(n: usize) -> Vec<Transaction> {
        (0..n)
            .map(|i| {
                Transaction::new_with_mass(
                    0,
                    vec![],
                    vec![],
                    0,
                    if i == 0 { SUBNETWORK_ID_COINBASE } else { SUBNETWORK_ID_NATIVE },
                    0,
                    vec![i as u8],
                    17 + i as u64,
                )
            })
            .collect()
    }
    fn header(txs: &[Transaction], params: &Params) -> Header {
        let mut header = Header::from(&params.genesis);
        header.hash_merkle_root = if params.crescendo_activation.is_active(header.daa_score) {
            crate::merkle::calc_hash_merkle_root(txs.iter())
        } else {
            crate::merkle::calc_hash_merkle_root_pre_crescendo(txs.iter())
        };
        header.hash = crate::hashing::header::hash(&header);
        header
    }
    #[test]
    fn original_uneven_block_paths_and_both_mass_fences() {
        for activation in [ForkActivation::always(), ForkActivation::never()] {
            let mut params = DEVNET_PARAMS;
            params.crescendo_activation = activation;
            for n in [1, 2, 3, 5, 8, 9, 31, 65] {
                let txs = body(n);
                let h = header(&txs, &params);
                let proof = prove_native_coinbase_v33(&h, h.hash, &params, txs.iter()).unwrap();
                let verified = verify_native_coinbase_v33(&proof, &h, h.hash, &params).unwrap();
                assert_eq!(verified.transaction().id(), txs[0].id());
                let mut altered = proof.clone();
                altered.coinbase.payload.push(0xff);
                altered.coinbase.finalize();
                assert!(verify_native_coinbase_v33(&altered, &h, h.hash, &params).is_err());
                let mut altered = proof.clone();
                altered.siblings.push(ZERO_HASH64);
                assert!(verify_native_coinbase_v33(&altered, &h, h.hash, &params).is_err());
                let mut wrong = params.clone();
                wrong.crescendo_activation =
                    if activation == ForkActivation::always() { ForkActivation::never() } else { ForkActivation::always() };
                assert!(verify_native_coinbase_v33(&proof, &h, h.hash, &wrong).is_err());
            }
        }
    }
    #[test]
    fn count_duplicate_coinbase_and_body_root_are_not_assertions() {
        let params = DEVNET_PARAMS;
        let mut txs = body(3);
        let h = header(&txs, &params);
        let mut proof = prove_native_coinbase_v33(&h, h.hash, &params, txs.iter()).unwrap();
        proof.transactions = 0;
        assert!(verify_native_coinbase_v33(&proof, &h, h.hash, &params).is_err());
        assert!(prove_native_coinbase_v33(&h, ZERO_HASH64, &params, txs.iter()).is_err());
        txs[1] = txs[0].clone();
        assert!(prove_native_coinbase_v33(&h, h.hash, &params, txs.iter()).is_err());
        txs = body(3);
        txs[2].payload.push(42);
        txs[2].finalize();
        assert!(prove_native_coinbase_v33(&h, h.hash, &params, txs.iter()).is_err());
    }
}
