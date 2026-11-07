//! Original native credit delivered to a real pinned coinbase outpoint.
//! Maturity, current unspentness and subsequent spend remain separate checks.
use crate::native_audit_v32::{AnchoredNativeExecutionAuditV32, native_inner_state_of_pinned_headers_v32};
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1,
    palw_native_audit_proof_v32::verify_native_payout_rows_v32,
    palw_native_redemption_v33::{
        CheckedNativeRedemptionV33, NativeCoinbaseProofV33, derive_native_redemption_v33, verify_native_coinbase_v33,
    },
    palw_state_proof_v1::PalwFactProofV1,
};

pub struct AnchoredNativeCreditDeliveryV33 {
    delivery: CheckedNativeRedemptionV33,
    audit: misaka_palw_kernel::hash::Digest,
    post_header: Hash64,
}
impl AnchoredNativeCreditDeliveryV33 {
    pub fn delivery(&self) -> &CheckedNativeRedemptionV33 {
        &self.delivery
    }
    pub fn audit_root(&self) -> misaka_palw_kernel::hash::Digest {
        self.audit
    }
    pub fn post_header(&self) -> Hash64 {
        self.post_header
    }
    pub fn root(&self) -> misaka_palw_kernel::hash::Digest {
        misaka_palw_kernel::hash::object_id(
            b"misaka-palw/sdk/pinned-original-credit-delivery/v33",
            &(self.audit, self.post_header, self.delivery.root()),
        )
    }
}
pub fn derive_native_credit_delivery_v33(
    final_audit: &AnchoredNativeExecutionAuditV32<'_>,
    payment_header: &Header,
    pinned_payment: Hash64,
    post_header: &Header,
    pinned_post_header: Hash64,
    parameters: &Params,
    post_fork_opening: Option<&PalwForkChoiceOpeningV1>,
    post_payouts: &PalwFactProofV1,
    coinbase: &NativeCoinbaseProofV33,
    output_index: u32,
) -> Result<AnchoredNativeCreditDeliveryV33, String> {
    if final_audit.child() != pinned_payment || kaspa_consensus_core::hashing::header::hash(payment_header) != pinned_payment {
        return Err("native33 payment header is not the exact original Final audit child/pre-state anchor".into());
    }
    let after_state = native_inner_state_of_pinned_headers_v32(
        post_header,
        pinned_post_header,
        payment_header,
        pinned_payment,
        parameters,
        post_fork_opening,
    )?;
    let before = verify_native_payout_rows_v32(final_audit.payout_proof(), final_audit.inner_state())?;
    let after = verify_native_payout_rows_v32(post_payouts, after_state)?;
    let paid = verify_native_coinbase_v33(coinbase, payment_header, pinned_payment, parameters)?;
    let delivery = derive_native_redemption_v33(final_audit.native(), &before, &after, &paid, output_index)?;
    Ok(AnchoredNativeCreditDeliveryV33 { delivery, audit: final_audit.root(), post_header: pinned_post_header })
}
pub fn read_coinbase_proof_v33(path: &std::path::Path) -> Result<NativeCoinbaseProofV33, String> {
    // JSON arrays encode their actual elements instead of an attacker-selected
    // Borsh reserve count. The complete input cap precedes parsing allocation.
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(path, 16 << 20)?;
    let proof: NativeCoinbaseProofV33 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if proof.version != 33
        || proof.transactions == 0
        || proof.transactions > 65536
        || proof.siblings.len() > 16
        || proof.coinbase.outputs.len() > 16384
        || !proof.coinbase.inputs.is_empty()
        || proof.coinbase.payload.len() > 4 << 20
    {
        return Err("native33 raw coinbase proof nested bounds differ".into());
    }
    // Inclusion, original queue identity, exact prefix/script/value and its
    // consumption are checked only by the private derivation above.
    Ok(proof)
}

/// Native RPC33 uses canonical Borsh. Preflight every nested transaction count
/// on borrowed original bytes before Transaction's generic Vec deserializer.
pub fn decode_coinbase_proof_v33(raw: &[u8]) -> Result<NativeCoinbaseProofV33, String> {
    fn take<'a>(raw: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], String> {
        let end = at.checked_add(n).ok_or("coinbase33 offset overflow")?;
        let value = raw.get(*at..end).ok_or("coinbase33 truncated original body")?;
        *at = end;
        Ok(value)
    }
    fn count(raw: &[u8], at: &mut usize, cap: usize) -> Result<usize, String> {
        let n = u32::from_le_bytes(take(raw, at, 4)?.try_into().unwrap()) as usize;
        if n > cap {
            return Err("coinbase33 nested count exceeds paid original cap".into());
        }
        Ok(n)
    }
    if raw.len() > (4 << 20) + (64 << 10) {
        return Err("coinbase33 raw whole proof cap".into());
    }
    let mut at = 0;
    let version = u16::from_le_bytes(take(raw, &mut at, 2)?.try_into().unwrap());
    let block = Hash64::from_bytes(take(raw, &mut at, 64)?.try_into().unwrap());
    let transactions = count(raw, &mut at, 65536)?;
    if version != 33 || transactions == 0 {
        return Err("coinbase33 original version/transaction count".into());
    }
    let tx_start = at;
    take(raw, &mut at, 2)?; // Transaction version, unrelated to proof version.
    if count(raw, &mut at, 0)? != 0 {
        return Err("coinbase33 inputs forbidden".into());
    }
    let outputs = count(raw, &mut at, 16384)?;
    if outputs > raw.len().saturating_sub(at) / 14 {
        return Err("coinbase33 outputs exceed available actual bytes".into());
    }
    let mut priced = 0usize;
    for _ in 0..outputs {
        take(raw, &mut at, 10)?; // value:u64 + script version:u16
        let n = count(raw, &mut at, 4 << 20)?;
        priced = priced
            .checked_add(12)
            .and_then(|v| v.checked_add(n))
            .filter(|n| *n <= 4 << 20)
            .ok_or("coinbase33 script aggregate cap")?;
        take(raw, &mut at, n)?;
    }
    take(raw, &mut at, 36)?; // lock_time + exact20B subnetwork + gas.
    let payload = count(raw, &mut at, 4 << 20)?;
    priced.checked_add(payload).filter(|n| *n <= 4 << 20).ok_or("coinbase33 payload/output aggregate cap")?;
    take(raw, &mut at, payload)?;
    take(raw, &mut at, 72)?; // mass:u64 + cached transaction identity:64B.
    let tx_end = at;
    let n = count(raw, &mut at, 16)?;
    if n != transactions.next_power_of_two().trailing_zeros() as usize {
        return Err("coinbase33 exact index-zero path length".into());
    }
    take(raw, &mut at, n * 64)?;
    if at != raw.len() {
        return Err("coinbase33 original proof trailing bytes".into());
    }
    // Now, and only now, every generic nested count was preflighted.
    let coinbase: kaspa_consensus_core::tx::Transaction = borsh::from_slice(&raw[tx_start..tx_end]).map_err(|e| e.to_string())?;
    let siblings = raw[tx_end + 4..].chunks_exact(64).map(|v| Hash64::from_bytes(v.try_into().unwrap())).collect();
    Ok(NativeCoinbaseProofV33 { version, block, transactions: transactions as u32, coinbase, siblings })
}
pub fn read_coinbase_proof_borsh_v33(path: &std::path::Path) -> Result<NativeCoinbaseProofV33, String> {
    decode_coinbase_proof_v33(&crate::native_audit_io_v32::read_bounded_file_v32(path, (4 << 20) + (64 << 10))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_coinbase_wire_preflights_all_original_nested_counts() {
        use kaspa_consensus_core::{
            subnets::SUBNETWORK_ID_COINBASE,
            tx::{ScriptPublicKey, Transaction, TransactionOutput},
        };
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(12, ScriptPublicKey::from_vec(0, vec![1]))],
            0,
            SUBNETWORK_ID_COINBASE,
            0,
            vec![2; 64],
        );
        let proof = NativeCoinbaseProofV33 {
            version: 33,
            block: Hash64::from_bytes([3; 64]),
            transactions: 1,
            coinbase: tx,
            siblings: vec![],
        };
        let raw = borsh::to_vec(&proof).unwrap();
        let actual = decode_coinbase_proof_v33(&raw).unwrap();
        assert_eq!(actual.coinbase, proof.coinbase);
        assert_eq!(actual.block, proof.block);
        let mut hostile = raw.clone();
        // Proof70B, transaction version2B, original input-count4B, output-count.
        hostile[76..80].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_coinbase_proof_v33(&hostile).is_err());
        let mut hostile = raw.clone();
        hostile[72..76].copy_from_slice(&1u32.to_le_bytes());
        assert!(decode_coinbase_proof_v33(&hostile).is_err());
        let mut hostile = raw;
        hostile.extend_from_slice(&[0]);
        assert!(decode_coinbase_proof_v33(&hostile).is_err());
    }
}
