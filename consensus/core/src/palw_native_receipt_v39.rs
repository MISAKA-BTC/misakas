//! **Owning39 native receipt blocks** (RFC-0002 §II.14 "block, coinbase and Final weight"; DORMANT behind
//! `Params::palw_native_mining_v39`, refused when armed).
//!
//! A native mining receipt quantum is spent ONLY as a receipt block's own work: an algo-7 header whose PALW carriage is
//! `PNR9 ‖ borsh(PalwNativeReceiptSpendEnvelopeV39)`. This is the node's block gate for native receipts:
//!
//! * the processor admits the carriage only past the mining fence (below it the header is refused by name, exactly as a `PFS4`
//!   header below `palw_receipt_spend_v4` — old releases fail to decode it too, so the valid set below the fence is unchanged);
//! * the receipt must be live, unexpired, the quantum unspent, the header's producer the receipt's producer (its bond key
//!   signs the header-position challenge), its budget reservation able to pay this block strictly (one block unit, this block's
//!   worker carve, the 0.51 export cap), and Gate C must certify the mining claim
//!   ([`crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39`]);
//! * the coinbase pays the receipt block's subsidy-derived worker share as it pays every entitled receipt block — no extra issuance,
//!   no payout row; a refused carriage makes the block unentitled (no worker share) or disqualified (own block);
//! * the fold folds it through the shared receipt-spend site into the native fold, which credits Final weight only through the
//!   budget's F grant and Gate C.
//!
//! **Allocation (grepped 2026-10-15).** Carriage magic `PNR9`; the V3 fold-envelope version `39` marks a native spend inside the
//! shared `PalwBlockWorkV3::ReceiptSpend` site (`PALW_FP_V3_VERSION` is 3); no object tag, aux table, RPC op or p2p version.
use crate::{
    Hash64,
    palw_freeprompt_v3::{PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3},
    palw_native_mining_v39::*,
    palw_state_v2::{PalwBlockContextV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
    tx::TransactionOutpoint,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};

pub const PALW_NATIVE_RECEIPT_CARRIAGE_MAGIC_V39: [u8; 4] = *b"PNR9";
/// The V3 fold-envelope version that names a native spend (the FP lane's is 3).
pub const PALW_NATIVE_RECEIPT_FOLD_VERSION_V39: u16 = 39;
pub const PALW_NATIVE_RECEIPT_MLDSA87_CONTEXT_V39: &[u8] = b"misaka-palw/native/receipt-block-spend/mldsa87/v39";
pub const PALW_NATIVE_RECEIPT_MAX_BYTES_V39: usize = crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeReceiptSpendV39 {
    pub version: u16,
    pub network_domain: Hash64,
    /// [`native_receipt_spend_challenge_v39`] over this spend's header position.
    pub challenge: Hash64,
    pub receipt: Hash64,
    pub quantum_index: u32,
    pub producer_bond: TransactionOutpoint,
    pub producer_pubkey: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeReceiptSpendEnvelopeV39 {
    pub spend: PalwNativeReceiptSpendV39,
    pub signature: Vec<u8>,
}
pub fn palw_native_receipt_carriage_is_v39(payload: &[u8]) -> bool {
    payload.starts_with(&PALW_NATIVE_RECEIPT_CARRIAGE_MAGIC_V39)
}
/// `H(network ‖ pre_pow ‖ timestamp ‖ nonce ‖ receipt ‖ quantum ‖ bond)`: every header position is a distinct spend identity.
pub fn native_receipt_spend_challenge_v39(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    receipt: Hash64,
    quantum_index: u32,
    producer_bond: &TransactionOutpoint,
) -> Hash64 {
    Hash64::from_bytes(object_id(
        b"misaka-palw/native/receipt-block-spend-challenge/v39",
        &(network_domain, pre_pow_hash, timestamp, nonce, receipt, quantum_index, producer_bond.transaction_id, producer_bond.index),
    ))
}
pub fn native_receipt_spend_id_v39(spend: &PalwNativeReceiptSpendV39) -> Hash64 {
    Hash64::from_bytes(object_id(b"misaka-palw/native/receipt-block-spend-id/v39", spend))
}
/// The receipt block's PoW tag expands its own spend id under its own domain (same width as the FP lane's).
pub fn native_receipt_l1_tag_v39(spend_id: Hash64) -> [u8; crate::palw_freeprompt_v3::PALW_FP_V3_L1_TAG_BYTES] {
    let mut out = [0u8; crate::palw_freeprompt_v3::PALW_FP_V3_L1_TAG_BYTES];
    for (i, chunk) in out.chunks_mut(64).enumerate() {
        let d = object_id(b"misaka-palw/native/receipt-block-l1-tag/v39", &(spend_id, i as u32));
        chunk.copy_from_slice(&d[..chunk.len()]);
    }
    out
}
impl PalwNativeReceiptSpendEnvelopeV39 {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_NATIVE_RECEIPT_CARRIAGE_MAGIC_V39.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh of a plain struct"));
        out
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_NATIVE_RECEIPT_MAX_BYTES_V39 {
            return Err("native39 receipt carriage exceeds the header cap".into());
        }
        let body = bytes.strip_prefix(&PALW_NATIVE_RECEIPT_CARRIAGE_MAGIC_V39).ok_or("native39 receipt carriage magic")?;
        let mut slice = body;
        let decoded = Self::deserialize(&mut slice).map_err(|_| "native39 receipt carriage grammar")?;
        if !slice.is_empty()
            || decoded.spend.version != 39
            || decoded.spend.producer_pubkey.len() > 4096
            || decoded.signature.len() > 8192
        {
            return Err("native39 receipt carriage exact extent/version".into());
        }
        Ok(decoded)
    }
    /// The shared receipt-spend site's view: version 39 routes the fold to the native lane; the beacon is unused (zero).
    pub fn to_fold_envelope(&self) -> PalwReceiptSpendEnvelopeV3 {
        let s = &self.spend;
        PalwReceiptSpendEnvelopeV3 {
            spend: PalwReceiptSpendUnsignedV3 {
                version: PALW_NATIVE_RECEIPT_FOLD_VERSION_V39,
                network_domain: s.network_domain,
                challenge: s.challenge,
                claim_id: s.receipt,
                quantum_index: s.quantum_index,
                beacon_block: Hash64::default(),
                producer_bond: s.producer_bond,
                producer_pubkey: s.producer_pubkey.clone(),
            },
            signature: Vec::new(),
        }
    }
}
pub fn native_mining_row_key_v39<T: BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    borsh::to_vec(&(39u8, kind, id)).expect("bounded owning39 key")
}
/// Readonly: the live receipt, its claim and the spent marker of `index` (fixed bounded rows only).
pub fn native_mining_receipt_view_v39(
    state: &PalwChainStateV2,
    receipt: &Hash64,
) -> Option<(NativeMiningReceiptRecordV39, NativeMiningClaimRecordV39)> {
    let route = state.kernel_route_state_v1()?;
    let (rroot, r): (Digest, NativeMiningReceiptRecordV39) =
        route.aux_row(MINING_TABLE_V39, &native_mining_row_key_v39(MINING_RECEIPT_KIND_V39, &receipt.as_bytes()))?;
    let (croot, c): (Digest, NativeMiningClaimRecordV39) =
        route.aux_row(MINING_TABLE_V39, &native_mining_row_key_v39(MINING_CLAIM_KIND_V39, &r.claim))?;
    (rroot == r.root()
        && croot == c.root()
        && r.id == receipt.as_bytes()
        && c.id == r.claim
        && c.closed == Some((r.established_daa, true)))
    .then_some((r, c))
}
/// **G14 lane closure G-N2: an open post-Final accusation of `claim`** (a demand on a credited component's material awaiting its
/// answer). While one is open the claim's certification decision waits (Gate C pending) and no receipt quantum is spent.
pub fn native_mining_accusation_pending_v39(state: &PalwChainStateV2, claim: &Hash64) -> bool {
    state.kernel_route_state_v1().is_some_and(|route| {
        route
            .aux_row::<NativeMiningAccusationCountsV39>(
                MINING_TABLE_V39,
                &native_mining_row_key_v39(MINING_ACCUSATION_COUNT_KIND_V39, &claim.as_bytes()),
            )
            .is_some_and(|c| c.open > 0)
    })
}
/// G-N2: has `claim` been convicted (its tombstone exists)?
pub fn native_mining_convicted_v39(state: &PalwChainStateV2, claim: &Hash64) -> bool {
    state.kernel_route_state_v1().is_some_and(|route| {
        route.aux.contains_key(&(MINING_TABLE_V39, native_mining_row_key_v39(MINING_CONVICTED_KIND_V39, &claim.as_bytes())))
    })
}
pub fn native_mining_quantum_spent_v39(state: &PalwChainStateV2, receipt: &Hash64, index: u32) -> bool {
    native_mining_receipt_view_v39(state, receipt).is_none_or(|(r, _)| r.is_spent(index))
}
/// The per-receipt budget row id (ADR-0176 claim id of the mining claim).
pub fn native_mining_budget_id_v39(claim: &Digest) -> Hash64 {
    Hash64::from_bytes(object_id(b"misaka-palw/native/mining-claim-budget/v39", claim))
}

/// **The receipt-block gate** — every stateful check a validator runs against the PARENT state, the same conditions the fold
/// re-checks against the live state. `carve` is this block's subsidy-derived worker carve (the coinbase's number).
#[allow(clippy::too_many_arguments)]
pub fn check_native_receipt_spend_admission_v39<V>(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: &PalwBlockContextV2,
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    carve: u64,
    envelope: &PalwNativeReceiptSpendEnvelopeV39,
    verify_mldsa87: V,
) -> Result<(), String>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let s = &envelope.spend;
    if s.version != 39
        || s.network_domain != network_domain
        || s.challenge
            != native_receipt_spend_challenge_v39(
                network_domain,
                pre_pow_hash,
                timestamp,
                nonce,
                s.receipt,
                s.quantum_index,
                &s.producer_bond,
            )
    {
        return Err("native39 receipt spend version/network/header-position challenge differs".into());
    }
    let bond = state.bond(&crate::palw_state_v2::PalwBondKeyV2(s.producer_bond)).ok_or("native39 receipt producer bond absent")?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.pubkey != s.producer_pubkey {
        return Err("native39 receipt producer bond is not Active under the carried key".into());
    }
    if !verify_mldsa87(
        &s.producer_pubkey,
        native_receipt_spend_id_v39(s).as_byte_slice(),
        &envelope.signature,
        PALW_NATIVE_RECEIPT_MLDSA87_CONTEXT_V39,
    ) {
        return Err("native39 receipt spend signature invalid".into());
    }
    let (receipt, claim) = native_mining_receipt_view_v39(state, &s.receipt).ok_or("native39 receipt is not a live issued receipt")?;
    if receipt.producer.0 != s.producer_bond
        || point.daa_score > receipt.expires
        || s.quantum_index as u64 >= receipt.price.quanta
        || native_mining_quantum_spent_v39(state, &s.receipt, s.quantum_index)
    {
        return Err("native39 receipt producer/expiry/quantum range or single use differs".into());
    }
    if !crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(params, state, &claim.claim_hash(), claim.admitted_daa) {
        return Err("native39 receipt's mining claim is not SecurityCertified (Gate C)".into());
    }
    // G-N2: while a post-Final accusation is open no quantum is spent (the fold refuses the same).
    if native_mining_accusation_pending_v39(state, &claim.claim_hash()) {
        return Err("native39 receipt's mining claim has an open post-Final accusation".into());
    }
    if !state.bond_budget_native_receipt_fits_v39(params, &native_mining_budget_id_v39(&claim.id), point.daa_score, carve) {
        return Err("native39 receipt's budget reservation cannot pay this block (ADR-0176, 0.51 export)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spend(nonce: u64) -> PalwNativeReceiptSpendV39 {
        let bond = TransactionOutpoint::new(Hash64::from_bytes([3; 64]), 1);
        PalwNativeReceiptSpendV39 {
            version: 39,
            network_domain: Hash64::from_bytes([1; 64]),
            challenge: native_receipt_spend_challenge_v39(
                Hash64::from_bytes([1; 64]),
                Hash64::from_bytes([2; 64]),
                5,
                nonce,
                Hash64::from_bytes([4; 64]),
                0,
                &bond,
            ),
            receipt: Hash64::from_bytes([4; 64]),
            quantum_index: 0,
            producer_bond: bond,
            producer_pubkey: vec![9; 32],
        }
    }
    #[test]
    fn the_carriage_round_trips_and_never_decodes_as_another_family() {
        let e = PalwNativeReceiptSpendEnvelopeV39 { spend: spend(1), signature: vec![7; 16] };
        let bytes = e.encode();
        assert!(palw_native_receipt_carriage_is_v39(&bytes));
        assert!(!crate::palw_receipt_v4::palw_receipt_v4_carriage_is_v4(&bytes));
        assert!(crate::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(&bytes).is_err(), "PNR9 is not PFS3");
        assert_eq!(PalwNativeReceiptSpendEnvelopeV39::decode(&bytes).unwrap(), e);
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(PalwNativeReceiptSpendEnvelopeV39::decode(&trailing).is_err());
        let fold = e.to_fold_envelope();
        assert_eq!(fold.spend.version, PALW_NATIVE_RECEIPT_FOLD_VERSION_V39);
        assert_eq!(fold.spend.claim_id, e.spend.receipt);
    }
    #[test]
    fn every_header_position_is_a_distinct_spend_identity() {
        assert_ne!(spend(1).challenge, spend(2).challenge);
        assert_ne!(native_receipt_spend_id_v39(&spend(1)), native_receipt_spend_id_v39(&spend(2)));
    }
}
