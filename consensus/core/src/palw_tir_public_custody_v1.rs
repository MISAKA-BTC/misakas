//! Explicit custody of an original TIR public source. A history/prefix grant or
//! Panel membership is not this grant. These types do not open a paid allocation:
//! the prospective accepting fold must first fix the complete native SUM, charge
//! it and retain this contract before it credits any receipt under the new rule.
//! The current native receipt adapter uses retained prospective FP ClaimBundles;
//! this module does not introduce an Attempt or generic-claim admission route.
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1: &[u8] = b"misaka-palw/tir-source/pledge/v1/mldsa87";
pub const PALW_TIR_SOURCE_GRANT_CONTEXT_V1: &[u8] = b"misaka-palw/tir-source/valid-grant/v1/mldsa87";

fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    let bytes = borsh::to_vec(value).expect("fixed custody scope serializes");
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes().try_into().expect("64"))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicCustodyPledgeV1 {
    pub version: u16,
    pub claim: Hash64,
    pub producer: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub job_identity: Hash64,
    pub source_id: Hash64,
    /// Recomputed complete native allocation, not the acquisition-only quote.
    pub resource_plan_id: Hash64,
    pub maximum_custodians: u64,
    pub accept_until_daa: u64,
    pub material_until_daa: u64,
    pub signature: Vec<u8>,
}
impl BorshDeserialize for PalwTirPublicCustodyPledgeV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(r)?,
            claim: BorshDeserialize::deserialize_reader(r)?,
            producer: BorshDeserialize::deserialize_reader(r)?,
            execution_root: BorshDeserialize::deserialize_reader(r)?,
            job_identity: BorshDeserialize::deserialize_reader(r)?,
            source_id: BorshDeserialize::deserialize_reader(r)?,
            resource_plan_id: BorshDeserialize::deserialize_reader(r)?,
            maximum_custodians: BorshDeserialize::deserialize_reader(r)?,
            accept_until_daa: BorshDeserialize::deserialize_reader(r)?,
            material_until_daa: BorshDeserialize::deserialize_reader(r)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        })
    }
}
impl PalwTirPublicCustodyPledgeV1 {
    pub fn authenticate_original_object_v1<F>(
        &self,
        object: &crate::palw_state_v2::PalwConsensusObjectV2,
        source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
        native_plan: Hash64,
        native_custodian_upper: u64,
        verify: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim,
            class_id,
            bond,
            execution_root,
            trace_root,
            job_pin,
            executor_pubkey,
            work_leaves,
            prompt_tokens,
            decode_tokens_executed,
            ..
        } = object
        else {
            return Err("TIR source pledge is not transaction-paired with an original FP commitment".into());
        };
        let header = source.header_v1();
        if (self.claim, self.producer, self.execution_root, self.job_identity) != (*claim, *bond, *execution_root, *job_pin)
            || self.source_id != source.source_id_v1()
            || self.resource_plan_id != native_plan
            || self.maximum_custodians != native_custodian_upper
            || header.job_context.shape_profile_id != *class_id
            || header.step_leaf_count != *work_leaves
            || header.job_context.declared_prefill_tokens != *prompt_tokens
            || header.job_context.exact_decode_tokens != *decode_tokens_executed
            || header.committed_execution_root != *execution_root
            || header.full_logits_trace_root != *trace_root
            || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&header.job_context) != *job_pin
        {
            return Err("TIR source pledge substitutes original claim, source, roots, job or native full allocation".into());
        }
        self.validate_signature_v1(executor_pubkey, verify)
    }
    pub fn id_v1(&self) -> Hash64 {
        hash(
            b"misaka-palw/tir-source/pledge-id/v1",
            &(
                self.version,
                self.claim,
                self.producer,
                self.execution_root,
                self.job_identity,
                self.source_id,
                self.resource_plan_id,
                self.maximum_custodians,
                self.accept_until_daa,
                self.material_until_daa,
            ),
        )
    }
    pub fn check_v1(&self) -> Result<(), String> {
        if self.version != 1
            || self.claim == Hash64::default()
            || self.execution_root == Hash64::default()
            || self.job_identity == Hash64::default()
            || self.source_id == Hash64::default()
            || self.resource_plan_id == Hash64::default()
            || self.maximum_custodians == 0
            || self.accept_until_daa >= self.material_until_daa
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err("TIR source pledge lacks an exact original scope, horizon or signature".into());
        }
        Ok(())
    }
    pub fn validate_signature_v1<F>(&self, public_key: &[u8], verify: F) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        if public_key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || !verify(public_key, self.id_v1().as_byte_slice(), &self.signature, PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1)
        {
            return Err("TIR source pledge is not signed by its original producer bond".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicCustodyGrantV1 {
    pub version: u16,
    pub claim: Hash64,
    pub seat: PalwBondKeyV2,
    pub signed_daa: u64,
    pub pledge_id: Hash64,
    pub valid_receipt_digest: Hash64,
    pub signature: Vec<u8>,
}
impl BorshDeserialize for PalwTirPublicCustodyGrantV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(r)?,
            claim: BorshDeserialize::deserialize_reader(r)?,
            seat: BorshDeserialize::deserialize_reader(r)?,
            signed_daa: BorshDeserialize::deserialize_reader(r)?,
            pledge_id: BorshDeserialize::deserialize_reader(r)?,
            valid_receipt_digest: BorshDeserialize::deserialize_reader(r)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        })
    }
}
impl PalwTirPublicCustodyGrantV1 {
    pub fn receipt_digest_v1(receipt: &crate::palw_panel_v2::PalwSeatReceiptV2) -> Hash64 {
        hash(b"misaka-palw/tir-source/valid-receipt/v1", receipt)
    }
    pub fn id_v1(&self) -> Hash64 {
        hash(
            b"misaka-palw/tir-source/valid-grant-id/v1",
            &(self.version, self.claim, self.seat, self.signed_daa, self.pledge_id, self.valid_receipt_digest),
        )
    }
    pub fn check_receipt_v1(&self, receipt: &crate::palw_panel_v2::PalwSeatReceiptV2) -> Result<(), String> {
        if self.version != 1
            || self.pledge_id == Hash64::default()
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || receipt.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || (self.claim, self.seat, self.signed_daa) != (receipt.claim, receipt.seat_bond, receipt.signed_daa)
            || receipt.verdict != crate::palw_panel_v2::PalwReceiptVerdictV2::Valid
            || self.valid_receipt_digest != Self::receipt_digest_v1(receipt)
        {
            return Err("TIR source grant does not bind the exact signed Valid receipt".into());
        }
        Ok(())
    }
    pub fn validate_signature_v1<F>(
        &self,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        public_key: &[u8],
        verify: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_receipt_v1(receipt)?;
        if public_key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || !verify(public_key, self.id_v1().as_byte_slice(), &self.signature, PALW_TIR_SOURCE_GRANT_CONTEXT_V1)
        {
            return Err("TIR source custody is not signed by the literal Valid bond".into());
        }
        Ok(())
    }
}

/// Public signed records, not a deserializable proof of native admission.
/// Once credited, a grant cannot be removed by a later redraw or claim outcome.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicCustodyContractV1 {
    pledge: PalwTirPublicCustodyPledgeV1,
    terms: crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
    drawn_panel: BTreeSet<PalwBondKeyV2>,
    grants: BTreeMap<PalwBondKeyV2, (u64, PalwTirPublicCustodyGrantV1)>,
}
impl BorshDeserialize for PalwTirPublicCustodyContractV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let pledge = PalwTirPublicCustodyPledgeV1::deserialize_reader(r)?;
        pledge.check_v1().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let terms = BorshDeserialize::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)?;
        if u64::from(count).checked_add(1).is_none_or(|n| n > pledge.maximum_custodians) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR custody panel exceeds signed bound"));
        }
        let mut drawn_panel = BTreeSet::new();
        for _ in 0..count {
            if !drawn_panel.insert(PalwBondKeyV2::deserialize_reader(r)?) {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "duplicate TIR custody panel bond"));
            }
        }
        let count = u32::deserialize_reader(r)?;
        if count as usize > drawn_panel.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR custody grants exceed drawn panel"));
        }
        let mut grants = BTreeMap::new();
        for _ in 0..count {
            let bond = PalwBondKeyV2::deserialize_reader(r)?;
            let grant = <(u64, PalwTirPublicCustodyGrantV1)>::deserialize_reader(r)?;
            if !drawn_panel.contains(&bond) || grants.insert(bond, grant).is_some() {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "foreign or duplicate TIR custody grant"));
            }
        }
        let contract = Self { pledge, terms, drawn_panel, grants };
        contract.check_v1().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(contract)
    }
}
impl PalwTirPublicCustodyContractV1 {
    /// Native caller supplies the recomputed plan/source, actual drawn Panel and
    /// original bond public key. Signature verification does not prove payment.
    pub fn new_v1<F>(
        pledge: PalwTirPublicCustodyPledgeV1,
        terms: crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
        drawn_panel: BTreeSet<PalwBondKeyV2>,
        native_source: Hash64,
        native_plan: Hash64,
        native_custodian_upper: u64,
        producer_key: &[u8],
        verify: F,
    ) -> Result<Self, String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        pledge.validate_signature_v1(producer_key, verify)?;
        if pledge.source_id != native_source
            || pledge.resource_plan_id != native_plan
            || pledge.maximum_custodians != native_custodian_upper
        {
            return Err("TIR custody pledge substitutes its native source or full resource plan".into());
        }
        let result = Self { pledge, terms, drawn_panel, grants: BTreeMap::new() };
        result.check_v1()?;
        Ok(result)
    }
    pub fn pledge_v1(&self) -> &PalwTirPublicCustodyPledgeV1 {
        &self.pledge
    }
    pub fn terms_v1(&self) -> crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1 {
        self.terms
    }
    pub fn drawn_panel_v1(&self) -> &BTreeSet<PalwBondKeyV2> {
        &self.drawn_panel
    }
    pub fn liable_signers_v1(&self) -> BTreeSet<PalwBondKeyV2> {
        std::iter::once(self.pledge.producer).chain(self.grants.keys().copied()).collect()
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.pledge.check_v1()?;
        let t = self.terms;
        if self.drawn_panel.contains(&self.pledge.producer)
            || (self.drawn_panel.len() as u64).checked_add(1).is_none_or(|n| n > self.pledge.maximum_custodians)
            || t.accepted_daa >= t.open_until_daa
            || t.open_until_daa >= t.stream_until_daa
            || t.stream_until_daa >= t.collect_until_daa
            || t.collect_until_daa > t.material_until_daa
            || t.material_until_daa > t.bond_exit_after_daa
            || t.accepted_daa >= self.pledge.accept_until_daa
            || t.material_until_daa > self.pledge.material_until_daa
            || self.grants.iter().any(|(bond, (credited_daa, grant))| {
                !self.drawn_panel.contains(bond)
                    || *bond != grant.seat
                    || grant.claim != self.pledge.claim
                    || grant.pledge_id != self.pledge.id_v1()
                    || grant.signed_daa < t.accepted_daa
                    || *credited_daa < grant.signed_daa
                    || *credited_daa >= t.material_until_daa
                    || grant.version != 1
                    || grant.valid_receipt_digest == Hash64::default()
                    || grant.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            })
        {
            return Err("TIR custody changes its original Panel, terms, literal scope or credited grants".into());
        }
        Ok(())
    }
    fn qualify_v1<F>(
        &mut self,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        grant: &PalwTirPublicCustodyGrantV1,
        now: u64,
        public_key: &[u8],
        verify: F,
    ) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        grant.validate_signature_v1(receipt, public_key, verify)?;
        if grant.claim != self.pledge.claim
            || grant.pledge_id != self.pledge.id_v1()
            || !self.drawn_panel.contains(&grant.seat)
            || self.grants.contains_key(&grant.seat)
            || grant.signed_daa < self.terms.accepted_daa
            || grant.signed_daa > now
            || now >= self.terms.material_until_daa
        {
            return Err("TIR custody grant is stale, duplicated, foreign or outside its original clock".into());
        }
        self.grants.insert(grant.seat, (now, grant.clone()));
        Ok(())
    }
    /// The existing custody record is consulted ONLY to authenticate the exact
    /// already-credited Valid signature. The NEW signature above is independently
    /// required; the old prefix/history duty is never promoted into raw custody.
    pub fn qualify_native_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        grant: &PalwTirPublicCustodyGrantV1,
        now: u64,
    ) -> Result<(), String> {
        let native = state.staged_bundle_contract_v1(self.pledge.claim).ok_or("TIR source grant has no accepted original bundle")?;
        if native.claim.producer != self.pledge.producer
            || native.plan.terms != self.terms
            || !self.drawn_panel.is_subset(&native.claim.panel)
            || !native.claim.prefix_custody.as_ref().is_some_and(|custody| {
                custody.execution_root == self.pledge.execution_root
                    && custody.job_identity == self.pledge.job_identity
                    && custody.matches_credited_receipt_v1(self.pledge.claim, receipt)
            })
        {
            return Err("TIR source grant has no matching native credited Valid and original terms".into());
        }
        let producer = state.bond(&self.pledge.producer).ok_or("TIR source producer bond is absent")?;
        self.pledge
            .validate_signature_v1(&producer.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)?;
        let seat = state.bond(&grant.seat).ok_or("TIR source grant bond is absent")?;
        self.qualify_v1(receipt, grant, now, &seat.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
    }
    pub fn check_owned_v1(&self, owned: &crate::palw_tir_public_owned_v1::PalwTirPublicContractV1) -> Result<(), String> {
        self.check_v1()?;
        if owned.claim != self.pledge.claim
            || owned.producer != self.pledge.producer
            || owned.source_id != self.pledge.source_id
            || owned.terms != self.terms
            || owned.plan.maximum_custodians != self.pledge.maximum_custodians
            || self.drawn_panel.contains(&owned.owner)
            || owned.custodians != self.liable_signers_v1()
        {
            return Err("TIR owner changes original signed custody, terms or independent verifier".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn key() -> Vec<u8> {
        vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]
    }
    fn signature() -> Vec<u8> {
        vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]
    }
    fn pledge() -> PalwTirPublicCustodyPledgeV1 {
        PalwTirPublicCustodyPledgeV1 {
            version: 1,
            claim: h(1),
            producer: bond(2),
            execution_root: h(3),
            job_identity: h(4),
            source_id: h(5),
            resource_plan_id: h(6),
            maximum_custodians: 3,
            accept_until_daa: 200,
            material_until_daa: u64::MAX,
            signature: signature(),
        }
    }
    fn contract() -> PalwTirPublicCustodyContractV1 {
        let config = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
        let terms = config.claim_terms(100, 1, 8).unwrap();
        // Signature verification is injected here to isolate scope/atomicity.
        // These unit fixtures do not claim MLDSA or native admission coverage.
        PalwTirPublicCustodyContractV1::new_v1(
            pledge(),
            terms,
            [bond(7), bond(8)].into_iter().collect(),
            h(5),
            h(6),
            3,
            &key(),
            |_, _, _, domain| domain == PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1,
        )
        .unwrap()
    }
    fn receipt() -> crate::palw_panel_v2::PalwSeatReceiptV2 {
        crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim: h(1),
            seat_bond: bond(7),
            signed_daa: 101,
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            signature: signature(),
        }
    }
    fn grant(receipt: &crate::palw_panel_v2::PalwSeatReceiptV2) -> PalwTirPublicCustodyGrantV1 {
        PalwTirPublicCustodyGrantV1 {
            version: 1,
            claim: receipt.claim,
            seat: receipt.seat_bond,
            signed_daa: receipt.signed_daa,
            pledge_id: pledge().id_v1(),
            valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(receipt),
            signature: signature(),
        }
    }
    #[test]
    fn every_original_scope_dimension_and_new_signing_domain_is_bound() {
        let p = pledge();
        for n in 0..10 {
            let mut changed = p.clone();
            match n {
                0 => changed.version += 1,
                1 => changed.claim = h(99),
                2 => changed.producer = bond(99),
                3 => changed.execution_root = h(99),
                4 => changed.job_identity = h(99),
                5 => changed.source_id = h(99),
                6 => changed.resource_plan_id = h(99),
                7 => changed.maximum_custodians += 1,
                8 => changed.accept_until_daa += 1,
                _ => changed.material_until_daa -= 1,
            }
            assert_ne!(changed.id_v1(), p.id_v1());
        }
        assert!(
            p.validate_signature_v1(&key(), |_, _, _, d| d == crate::palw_history_custody_v1::PALW_HISTORY_PLEDGE_CONTEXT_V1).is_err()
        );
        assert!(p.validate_signature_v1(&key(), |_, _, _, d| d == PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1).is_ok());
        let c = contract();
        assert!(
            PalwTirPublicCustodyContractV1::new_v1(p.clone(), c.terms, c.drawn_panel.clone(), h(99), h(6), 3, &key(), |_, _, _, _| {
                true
            },)
            .is_err()
        );
        assert!(PalwTirPublicCustodyContractV1::new_v1(p, c.terms, c.drawn_panel, h(5), h(6), 4, &key(), |_, _, _, _| true,).is_err());
    }
    #[test]
    fn membership_and_old_grants_never_create_literal_raw_custody() {
        let mut c = contract();
        assert_eq!(c.liable_signers_v1(), [bond(2)].into_iter().collect());
        let r = receipt();
        let g = grant(&r);
        let original = c.clone();
        assert!(
            c.qualify_v1(&r, &g, 101, &key(), |_, _, _, d| d == crate::palw_history_custody_v1::PALW_HISTORY_GRANT_CONTEXT_V1)
                .is_err()
        );
        assert_eq!(c, original);
        let mut sampled = r.clone();
        sampled.verdict = crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled;
        assert!(c.qualify_v1(&sampled, &grant(&sampled), 101, &key(), |_, _, _, _| true).is_err());
        let mut other_signature = r.clone();
        other_signature.signature[0] ^= 1;
        assert!(c.qualify_v1(&other_signature, &g, 101, &key(), |_, _, _, _| true).is_err());
        let mut foreign = g.clone();
        foreign.pledge_id = h(99);
        assert!(c.qualify_v1(&r, &foreign, 101, &key(), |_, _, _, _| true).is_err());
        assert!(c.qualify_v1(&r, &g, 100, &key(), |_, _, _, _| true).is_err());
        assert_eq!(c, original);
        c.qualify_v1(&r, &g, 101, &key(), |_, _, _, d| d == PALW_TIR_SOURCE_GRANT_CONTEXT_V1).unwrap();
        assert_eq!(c.liable_signers_v1(), [bond(2), bond(7)].into_iter().collect());
        let accepted = c.clone();
        assert!(c.qualify_v1(&r, &g, 102, &key(), |_, _, _, _| true).is_err());
        assert_eq!(c, accepted);
        assert!(!c.liable_signers_v1().contains(&bond(8)));
        assert_eq!(borsh::from_slice::<PalwTirPublicCustodyContractV1>(&borsh::to_vec(&c).unwrap()).unwrap(), c);
    }
    #[test]
    fn native_adapter_refuses_an_uncredited_receipt_and_bounded_decoder_rejects_count_bombs() {
        let mut c = contract();
        let before = c.clone();
        let r = receipt();
        assert!(c.qualify_native_v1(&crate::palw_state_v2::PalwChainStateV2::genesis(), &r, &grant(&r), 101).is_err());
        assert_eq!(c, before);
        let mut bytes = borsh::to_vec(&c.pledge).unwrap();
        bytes.extend(borsh::to_vec(&c.terms).unwrap());
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<PalwTirPublicCustodyContractV1>(&bytes).is_err());
        let mut bytes = borsh::to_vec(&c.pledge).unwrap();
        bytes.truncate(bytes.len() - crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN - 4);
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<PalwTirPublicCustodyPledgeV1>(&bytes).is_err());
    }
    #[test]
    fn original_fp_pairing_binds_class_and_geometry_separately_from_job_pin() {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            1 << 26,
        )
        .unwrap();
        let ctx = &header.job_context;
        let mut p = pledge();
        p.execution_root = header.committed_execution_root;
        p.job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx);
        p.source_id = source.source_id_v1();
        let object = crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim: p.claim,
            class_id: ctx.shape_profile_id,
            bond: p.producer,
            executor_pubkey: key(),
            work_leaves: header.step_leaf_count,
            prompt_token_ids_hash: ctx.prompt_token_ids_hash,
            prompt_tokens: ctx.declared_prefill_tokens,
            prompt_token_ids: x.prompt.clone(),
            decode_tokens_executed: ctx.exact_decode_tokens,
            trace_root: header.full_logits_trace_root,
            output_root: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(ctx, &x.generated),
            execution_root: header.committed_execution_root,
            trace_chunk_count: 1,
            trace_retention_daa: 1_000,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(ctx.shape_profile_id),
            job_pin: p.job_identity,
            public_job: None,
            eval: None,
        };
        assert!(p.authenticate_original_object_v1(&object, &source, p.resource_plan_id, 3, |_, _, _, _| true).is_ok());
        for dimension in 0..4 {
            let mut changed = object.clone();
            let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
                class_id,
                work_leaves,
                prompt_tokens,
                decode_tokens_executed,
                ..
            } = &mut changed
            else {
                unreachable!()
            };
            match dimension {
                0 => *class_id = h(99),
                1 => *work_leaves += 1,
                2 => *prompt_tokens += 1,
                _ => *decode_tokens_executed += 1,
            }
            assert!(p.authenticate_original_object_v1(&changed, &source, p.resource_plan_id, 3, |_, _, _, _| true).is_err());
        }
    }
}
