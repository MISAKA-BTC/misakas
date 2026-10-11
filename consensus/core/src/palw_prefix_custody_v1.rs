//! Prospective signed custody qualification, never inferred from panel membership.
//! The accepted signed FP commitment binds this contract's root/job and immutable ruleset;
//! each later signed Valid opts its signer into those claim-specific original public sources.
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixCustodyReceiptV1 {
    pub signed_daa: u64,
    pub credited_daa: u64,
    /// Includes the exact accepted signature, claim and verdict. Root/job/ruleset/terms
    /// binding is the immutable containing custody contract, itself claim-keyed in state.
    pub receipt_digest: Hash64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    fn receipt(claim: Hash64, n: u64) -> crate::palw_panel_v2::PalwSeatReceiptV2 {
        crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim,
            seat_bond: bond(n),
            signed_daa: 101,
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            signature: vec![n as u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    #[test]
    fn all_actual_signed_valid_custody_is_required_membership_and_sampled_are_insufficient() {
        let terms = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().claim_terms(100, 4, 32).unwrap();
        let claim = Hash64::from_u64_word(100);
        let mut custody =
            PalwPrefixCustodyContractV1::new(Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3), &terms);
        custody.expected_panel = (1..=5).map(bond).collect();
        assert!(!custody.qualified());
        let before = custody.clone();
        let mut invalid = receipt(claim, 1);
        invalid.signed_daa = 99;
        assert!(!custody.qualify(claim, &invalid, 101));
        assert_eq!(custody, before);
        invalid = receipt(claim, 1);
        invalid.verdict = crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled;
        assert!(!custody.qualify(claim, &invalid, 101));
        assert_eq!(custody, before);
        assert!(!custody.qualify(Hash64::from_u64_word(999), &receipt(claim, 1), 101));
        assert!(!custody.qualify(claim, &receipt(claim, 6), 101));
        assert_eq!(custody, before);
        for n in 1..=5 {
            assert!(custody.qualify(claim, &receipt(claim, n), 101));
            assert_eq!(custody.qualified(), n == 5);
        }
        let original = custody.clone();
        assert!(!custody.qualify(claim, &receipt(claim, 1), 102));
        assert_eq!(custody, original);
        custody.check(&custody.expected_panel).unwrap();
        let restarted: PalwPrefixCustodyContractV1 = borsh::from_slice(&borsh::to_vec(&custody).unwrap()).unwrap();
        assert_eq!(restarted, custody);
        // Redraw cannot inherit an old member's signature for a new actual seat.
        custody.expected_panel.insert(bond(6));
        assert!(!custody.qualified());
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixCustodyContractV1 {
    pub execution_root: Hash64,
    pub job_identity: Hash64,
    pub cu_ruleset_id: Hash64,
    pub accepted_daa: u64,
    pub original_terms_id: Hash64,
    /// Latest actual bound panel, retained after ordinary panel/duty pruning. Historical
    /// drawn seats remain held separately; they are not promoted into this joint obligation.
    pub expected_panel: BTreeSet<PalwBondKeyV2>,
    pub credited: BTreeMap<PalwBondKeyV2, PalwPrefixCustodyReceiptV1>,
}
impl PalwPrefixCustodyContractV1 {
    pub fn new(
        execution_root: Hash64,
        job_identity: Hash64,
        cu_ruleset_id: Hash64,
        terms: &crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
    ) -> Self {
        let raw = borsh::to_vec(terms).expect("fixed claim terms serialize");
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/prefix-custody/terms/v1").to_state();
        hash.update(&raw);
        Self {
            execution_root,
            job_identity,
            cu_ruleset_id,
            accepted_daa: terms.accepted_daa,
            original_terms_id: Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64bytes")),
            expected_panel: BTreeSet::new(),
            credited: BTreeMap::new(),
        }
    }
    /// Caller supplies only already verified and actually credited signed receipts, below
    /// the independent prospective fence. Sampled, pre-acceptance, foreign and nonseat
    /// receipts cannot qualify even if an ordinary payment row credited them.
    pub fn qualify(&mut self, claim: Hash64, receipt: &crate::palw_panel_v2::PalwSeatReceiptV2, now: u64) -> bool {
        if receipt.claim != claim
            || receipt.verdict != crate::palw_panel_v2::PalwReceiptVerdictV2::Valid
            || receipt.signed_daa < self.accepted_daa
            || receipt.signed_daa > now
            || now < self.accepted_daa
            || !self.expected_panel.contains(&receipt.seat_bond)
            || self.credited.contains_key(&receipt.seat_bond)
        {
            return false;
        }
        let raw = borsh::to_vec(receipt).expect("accepted receipt serializes");
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/prefix-custody/receipt/v1").to_state();
        hash.update(&raw);
        hash.update(self.execution_root.as_byte_slice());
        hash.update(self.job_identity.as_byte_slice());
        hash.update(self.cu_ruleset_id.as_byte_slice());
        hash.update(self.original_terms_id.as_byte_slice());
        self.credited.insert(
            receipt.seat_bond,
            PalwPrefixCustodyReceiptV1 {
                signed_daa: receipt.signed_daa,
                credited_daa: now,
                receipt_digest: Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64bytes")),
            },
        );
        true
    }
    pub fn qualified(&self) -> bool {
        !self.expected_panel.is_empty() && self.expected_panel.iter().all(|seat| self.credited.contains_key(seat))
    }
    pub fn check(&self, held_panel: &BTreeSet<PalwBondKeyV2>) -> Result<(), String> {
        if !self.expected_panel.is_subset(held_panel)
            || self
                .credited
                .iter()
                .any(|(bond, r)| !held_panel.contains(bond) || r.signed_daa < self.accepted_daa || r.credited_daa < r.signed_daa)
        {
            return Err("prefix custody qualification differs from actual held panel or accepted clocks".into());
        }
        Ok(())
    }
}
