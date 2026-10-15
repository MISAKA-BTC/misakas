//! Native credit lifecycle after delivery33: an ACCEPTED spend of the exact
//! original credit outpoint. Delivery33 proves the queue drained into a real
//! coinbase output; it does not prove maturity, that the output was ever
//! unspent in the selected chain's UTXO set, or that the payee could spend it.
//! Consensus accepts a transaction in a chain block only when every input is a
//! live, mature, correctly signed UTXO of that chain's past. Authenticating the
//! spend's membership in the chain block's committed accepted-id root therefore
//! proves all three, from pinned headers alone, without trusting the serving
//! node's flags, the producer, a Panel seat or any private state.
//!
//! The caller independently pins the chain block and its selected parent
//! (same rule as audit32's child/selected-parent pins). The chain block's
//! accepted-id root is `AcceptedIdMerkleBranchHash64(parent_root || leaves_root)`
//! with `leaves_root` over the sorted accepted transaction ids of its mergeset —
//! exactly `utxo_validation::calc_accepted_id_merkle_root`. The transport is the
//! complete bounded accepted-id list, so no partial path grammar is trusted.
use crate::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_native_redemption_v33::CheckedNativeRedemptionV33,
    tx::{Transaction, TransactionId},
};
use kaspa_hashes::{AcceptedIdMerkleBranchHash64, HasherBase};
use misaka_palw_kernel::hash::{Digest, object_id};

/// Generous structural cap on one chain block's accepted mergeset ids.
pub const NATIVE_SPEND_MAX_ACCEPTED_IDS_V35: usize = 1 << 20;

/// Raw transport only. Deserialization never mints a checked spend.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct NativeAcceptedSpendProofV35 {
    pub version: u16,
    /// The chain block whose accepted-id root commits the spend.
    pub chain_block: Hash64,
    /// Its selected parent (whose own accepted-id root chains into the child's).
    pub selected_parent: Hash64,
    /// Every accepted transaction id of the chain block, in any order.
    pub accepted_ids: Vec<TransactionId>,
    /// The spending transaction itself (its id must be accepted).
    pub spend: Transaction,
}

/// Private mint: the exact original credit outpoint was consumed by a
/// transaction accepted into an independently pinned chain block.
pub struct CheckedNativeAcceptedSpendV35 {
    redemption: Digest,
    outpoint: crate::tx::TransactionOutpoint,
    spend: TransactionId,
    chain_block: Hash64,
    chain_daa: u64,
    payment_daa: u64,
}
impl CheckedNativeAcceptedSpendV35 {
    pub fn spend(&self) -> TransactionId {
        self.spend
    }
    pub fn chain_block(&self) -> Hash64 {
        self.chain_block
    }
    pub fn chain_daa(&self) -> u64 {
        self.chain_daa
    }
    pub fn payment_daa(&self) -> u64 {
        self.payment_daa
    }
    pub fn outpoint(&self) -> crate::tx::TransactionOutpoint {
        self.outpoint
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/native/accepted-original-credit-spend/v35",
            &(self.redemption, self.outpoint, self.spend, self.chain_block, self.chain_daa, self.payment_daa),
        )
    }
}

/// Build the transport from a node's own acceptance data (untrusted until verified).
pub fn prove_native_accepted_spend_v35(
    chain_block: Hash64,
    selected_parent: Hash64,
    accepted_ids: Vec<TransactionId>,
    spend: Transaction,
) -> Result<NativeAcceptedSpendProofV35, String> {
    if accepted_ids.is_empty() || accepted_ids.len() > NATIVE_SPEND_MAX_ACCEPTED_IDS_V35 || !accepted_ids.contains(&spend.id()) {
        return Err("native spend35 accepted id list/membership".into());
    }
    Ok(NativeAcceptedSpendProofV35 { version: 35, chain_block, selected_parent, accepted_ids, spend })
}

/// Verify against pinned headers. `payment` is the independently pinned
/// header of the block carrying the delivery33 coinbase (`redemption.block()`).
#[allow(clippy::too_many_arguments)]
pub fn verify_native_accepted_spend_v35(
    redemption: &CheckedNativeRedemptionV33,
    payment: &Header,
    proof: &NativeAcceptedSpendProofV35,
    chain: &Header,
    pinned_chain: Hash64,
    selected_parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
) -> Result<CheckedNativeAcceptedSpendV35, String> {
    if proof.version != 35
        || proof.chain_block != pinned_chain
        || proof.selected_parent != pinned_selected_parent
        || crate::hashing::header::hash(chain) != pinned_chain
        || crate::hashing::header::hash(selected_parent) != pinned_selected_parent
        || crate::hashing::header::hash(payment) != redemption.block()
        || !chain.parents_by_level.get(0).is_some_and(|parents| parents.contains(&pinned_selected_parent))
        || proof.accepted_ids.is_empty()
        || proof.accepted_ids.len() > NATIVE_SPEND_MAX_ACCEPTED_IDS_V35
    {
        return Err("native spend35 pinned chain/selected-parent/payment headers or bounded list differ".into());
    }
    // A duplicated id would let a forged list hash differently from the committed set.
    let mut sorted = proof.accepted_ids.clone();
    sorted.sort();
    if sorted.windows(2).any(|w| w[0] == w[1]) {
        return Err("native spend35 duplicate accepted id".into());
    }
    let leaves = crate::merkle::calc_accepted_id_merkle_root_pre_crescendo(sorted);
    let mut hasher = AcceptedIdMerkleBranchHash64::new();
    hasher.update(selected_parent.accepted_id_merkle_root.as_byte_slice()).update(leaves.as_byte_slice());
    if hasher.finalize() != chain.accepted_id_merkle_root {
        return Err("native spend35 accepted ids are not the pinned chain block's committed acceptance".into());
    }
    let spend_id = proof.spend.id();
    if !proof.spend.id_is_current()
        || proof.spend.is_coinbase()
        || !proof.accepted_ids.contains(&spend_id)
        || !proof.spend.inputs.iter().any(|input| input.previous_outpoint == redemption.outpoint())
    {
        return Err("native spend35 accepted transaction does not consume the original credit outpoint".into());
    }
    // Consensus already enforces maturity for acceptance; re-check the clock so a
    // mis-pinned payment header cannot pass silently.
    if chain.daa_score < payment.daa_score.saturating_add(parameters.coinbase_maturity()) {
        return Err("native spend35 accepted before original coinbase maturity under the owning ruleset".into());
    }
    Ok(CheckedNativeAcceptedSpendV35 {
        redemption: redemption.root(),
        outpoint: redemption.outpoint(),
        spend: spend_id,
        chain_block: pinned_chain,
        chain_daa: chain.daa_score,
        payment_daa: payment.daa_score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Root reconstruction matches the processor's chained formula and refuses
    /// a list missing or adding one id. (A full lifecycle with a genuine
    /// delivery33 belongs to the consolidated node validation phase.)
    #[test]
    fn accepted_root_recomputation_is_exact() {
        let ids: Vec<TransactionId> = (1u64..=5).map(Hash64::from_u64_word).collect();
        let parent_root = Hash64::from_u64_word(77);
        let recompute = |mut list: Vec<TransactionId>| {
            list.sort();
            let leaves = crate::merkle::calc_accepted_id_merkle_root_pre_crescendo(list);
            let mut h = AcceptedIdMerkleBranchHash64::new();
            h.update(parent_root.as_byte_slice()).update(leaves.as_byte_slice());
            h.finalize()
        };
        let root = recompute(ids.clone());
        let mut reversed = ids.clone();
        reversed.reverse();
        assert_eq!(recompute(reversed), root);
        assert_ne!(recompute(ids[..4].to_vec()), root);
        let mut extra = ids.clone();
        extra.push(Hash64::from_u64_word(99));
        assert_ne!(recompute(extra), root);
    }
}
