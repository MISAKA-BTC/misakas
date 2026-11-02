//! RFC-0042 / ADR-0210 independently signed family-supplier contract.
//! Signatures authenticate consent to data service, never computation validity.
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;
pub const FAMILY_SUPPLIER_TABLE_V1: u8 = 192;

pub const FAMILY_SUPPLIER_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/court-family-supplier/v1/mldsa87";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum FamilySupplierRoleV1 {
    ParameterAndPrompt,
    SemanticTable,
    TokenizerArtifact,
    PrefixGeneratedSelectionAndStop,
}

/// An independent service consent. Original-relative scope is fixed before
/// acceptance; absolute deadlines bound it without signing an unknown birth DAA.
/// Fee/capital amounts and profile IDs must come from the adopted schedule.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupplierTermsV1 {
    pub version: u16,
    pub network: Hash64,
    pub ruleset: Hash64,
    pub claim: Hash64,
    pub source: Hash64,
    pub relative_graph: Hash64,
    pub role: FamilySupplierRoleV1,
    /// Commitment to exact native target inventory, not a declared count alone.
    pub inventory: Hash64,
    pub supplier: PalwBondKeyV2,
    pub sponsor: PalwBondKeyV2,
    pub max_material_bytes: u64,
    pub accept_from_daa: u64,
    pub accept_until_daa: u64,
    pub respond_until_daa: u64,
    pub material_until_daa: u64,
    pub fee_profile: Hash64,
    pub capital_profile: Hash64,
    pub retention_profile: Hash64,
    pub settlement_policy: Hash64,
    pub fee_sompi: u64,
    pub collateral_sompi: u64,
    pub nonce: Hash64,
}
impl FamilySupplierTermsV1 {
    pub fn validate_shape_v1(&self) -> Result<(), &'static str> {
        if self.version != 1
            || [
                self.network,
                self.ruleset,
                self.claim,
                self.source,
                self.relative_graph,
                self.inventory,
                self.fee_profile,
                self.capital_profile,
                self.retention_profile,
                self.settlement_policy,
                self.nonce,
            ]
            .contains(&Hash64::default())
            || self.max_material_bytes == 0
            || self.collateral_sompi == 0
            || self.accept_from_daa > self.accept_until_daa
            || self.accept_until_daa >= self.respond_until_daa
            || self.respond_until_daa > self.material_until_daa
            || self.material_until_daa == u64::MAX
        {
            return Err("family supplier scope, policy, capacity or finite horizons are missing");
        }
        Ok(())
    }
    pub fn signing_message_v1(&self) -> Result<Hash64, String> {
        self.validate_shape_v1().map_err(str::to_owned)?;
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/court-family-supplier/v1", self)
    }
}

/// Both parties sign the same terms; neither signature replaces the other.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SignedFamilySupplierV1 {
    pub terms: FamilySupplierTermsV1,
    pub supplier_signature: Vec<u8>,
    pub sponsor_signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupplierTargetV1 {
    pub id: Hash64,
    pub max_bytes: u64,
    pub packets: Vec<[u8; 64]>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum FamilySupplierFeeStateV1 {
    Held,
    Queued { payee: crate::palw_state_v2::PalwBondKeyV2, queue: Hash64, receipt: Hash64 },
    Paid { payee: crate::palw_state_v2::PalwBondKeyV2, daa: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupplierRowV1 {
    pub version: u16,
    pub consent: SignedFamilySupplierV1,
    pub allocation: Hash64,
    pub accepted_daa: u64,
    pub targets: Vec<FamilySupplierTargetV1>,
    pub completed: BTreeMap<Hash64, Hash64>,
    pub defaulted_daa: Option<u64>,
    pub fee_state: FamilySupplierFeeStateV1,
}
/// Internal versioned finance sidecars. The signed consent and complete target
/// catalogue are immutable at the 64-byte contract key. A material completion
/// must not journal that large signed value again.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupplierStatusV2 {
    pub version: u16,
    pub contract: Hash64,
    pub defaulted_daa: Option<u64>,
    pub fee_state: FamilySupplierFeeStateV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupplierCompletionV2 {
    pub version: u16,
    pub contract: Hash64,
    pub target: Hash64,
    pub digest: Hash64,
}
pub(crate) fn family_status_key_v2(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = 1;
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
pub(crate) fn family_completion_key_v2(id: Hash64, target: Hash64) -> [u8; 129] {
    let mut key = [0; 129];
    key[0] = 2;
    key[1..65].copy_from_slice(id.as_byte_slice());
    key[65..].copy_from_slice(target.as_byte_slice());
    key
}
impl crate::palw_state_v2::PalwChainStateV2 {
    /// Reconstruct the logical public view only after reserving all decoded
    /// consent/catalog ownership and every completion-tree node. Persistent
    /// mutation uses the fixed sidecars rather than serializing this view.
    pub fn family_supplier_rows_v1(
        &self,
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<Vec<(Hash64, FamilySupplierRowV1)>>,
        crate::palw_state_v2::PalwStateV2Error,
    > {
        use crate::palw_native_resource_v1::{
            BTreeNodeEnvelopeV1, NativeRetainedV1, TirNativeRequestsV1, preflight_native_value_v1, reserve_native_requests_v1,
        };
        use crate::palw_state_v2::PalwStateV2Error as E;
        let bad = || E::CarriageInconsistent("family supplier rooted row/sidecar".into());
        let Some(route) = self.kernel_route() else {
            return Ok(NativeRetainedV1::from_owner_reserved_v1(Vec::new(), None));
        };
        let count = route.aux.keys().filter(|(t, k)| *t == FAMILY_SUPPLIER_TABLE_V1 && k.len() == 64).count();
        let mut quote = TirNativeRequestsV1::new();
        quote.exact_v1::<(Hash64, FamilySupplierRowV1)>(count as u64).map_err(E::NativeResource)?;
        for ((table, key), bytes) in &route.aux {
            if *table != FAMILY_SUPPLIER_TABLE_V1 {
                continue;
            }
            let shape = match (key.len(), key.first().copied()) {
                (64, _) => preflight_native_value_v1::<FamilySupplierRowV1>(bytes.as_slice(), bytes.len() as u64),
                (65,Some(4))=>preflight_native_value_v1::<FamilyCarrierProjectionRowV3>(bytes.as_slice(),bytes.len() as u64),
                (65, Some(1)) => preflight_native_value_v1::<FamilySupplierStatusV2>(bytes.as_slice(), bytes.len() as u64),
                (129, Some(3)) => preflight_native_value_v1::<FamilyPrimaryWindowV2>(bytes.as_slice(),bytes.len() as u64),
                (129, Some(2)) => {
                    quote.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Hash64>>(1).map_err(E::NativeResource)?;
                    preflight_native_value_v1::<FamilySupplierCompletionV2>(bytes.as_slice(), bytes.len() as u64)
                }
                _ => return Err(bad()),
            }
            .map_err(|_| bad())?;
            quote.add_v1(shape.logical_requests_v1()).map_err(E::NativeResource)?;
        }
        let lease = reserve_native_requests_v1(quote.bytes_v1()).map_err(E::NativeResource)?;
        let mut rows = Vec::with_capacity(count);
        for ((table, key), bytes) in &route.aux {
            if *table != FAMILY_SUPPLIER_TABLE_V1 || key.len() != 64 {
                continue;
            }
            let mut row: FamilySupplierRowV1 = borsh::from_slice(bytes).map_err(|_| bad())?;
            let id = row.consent.id_v1().map_err(|_| bad())?;
            let t = &row.consent.terms;
            if key.as_slice() != id.as_byte_slice()
                || row.version != 2
                || row.accepted_daa < t.accept_from_daa
                || row.accepted_daa > t.accept_until_daa
                || row.targets.is_empty()
                || row.targets.windows(2).any(|p| p[0].id >= p[1].id)
                || row.targets.iter().any(|t| t.max_bytes == 0 || t.packets.is_empty())
                || !row.completed.is_empty()
                || row.defaulted_daa.is_some()
                || row.fee_state != FamilySupplierFeeStateV1::Held
            {
                return Err(bad());
            }
            row.version = 1; // public logical view, never persisted as mutable row
            rows.push((id, row));
        }
        let mut statuses = 0usize;
        let mut carrier_facts = 0usize;
        for ((table, key), bytes) in &route.aux {
            if *table != FAMILY_SUPPLIER_TABLE_V1 || key.len() == 64 {
                continue;
            }
            if key.len()==65&&key[0]==4 {
                let facts:FamilyCarrierProjectionRowV3=borsh::from_slice(bytes).map_err(|_|bad())?;
                let index=rows.binary_search_by_key(&facts.contract,|p|p.0).map_err(|_|bad())?;let row=&rows[index].1;
                if facts.version!=3||key.as_slice()!=family_carrier_projection_key_v3(facts.contract)||facts.claim!=row.consent.terms.claim||facts.accepted_daa!=row.accepted_daa||facts.source!=row.consent.terms.source||facts.allocation!=row.allocation||facts.accepted_block==Hash64::default()||facts.funding_contract==Hash64::default()||signed_profile_generation_v3(&row.consent.terms).map_err(|_|bad())?!=3{return Err(bad())}
                facts.facts.check_v3().map_err(|_|bad())?;carrier_facts+=1;continue
            }
            if key.len()==129 && key[0]==3 {
                let w:FamilyPrimaryWindowV2=borsh::from_slice(bytes).map_err(|_|bad())?;
                let index=rows.binary_search_by_key(&w.contract,|p|p.0).map_err(|_|bad())?;
                let row=&rows[index].1;
                if !matches!(signed_profile_generation_v3(&row.consent.terms).map_err(|_|bad())?,2|3)||w.version!=2||key.as_slice()!=family_primary_window_key_v2(w.contract,w.target)||row.targets.binary_search_by_key(&w.target,|t|t.id).is_err(){return Err(bad())}
                continue;
            }
            if key.len() == 65 {
                let status: FamilySupplierStatusV2 = borsh::from_slice(bytes).map_err(|_| bad())?;
                if status.version != 2 || key.as_slice() != family_status_key_v2(status.contract) {
                    return Err(bad());
                }
                let index = rows.binary_search_by_key(&status.contract, |p| p.0).map_err(|_| bad())?;
                let row = &mut rows[index].1;
                if status.defaulted_daa.is_some_and(|d| d < row.accepted_daa || d > row.consent.terms.material_until_daa) {
                    return Err(bad());
                }
                row.defaulted_daa = status.defaulted_daa;
                row.fee_state = status.fee_state;
                statuses += 1;
            } else {
                let done: FamilySupplierCompletionV2 = borsh::from_slice(bytes).map_err(|_| bad())?;
                if done.version != 2
                    || done.digest == Hash64::default()
                    || key.as_slice() != family_completion_key_v2(done.contract, done.target)
                {
                    return Err(bad());
                }
                let index = rows.binary_search_by_key(&done.contract, |p| p.0).map_err(|_| bad())?;
                let row = &mut rows[index].1;
                if row.targets.binary_search_by_key(&done.target, |t| t.id).is_err()
                    || row.completed.insert(done.target, done.digest).is_some()
                {
                    return Err(bad());
                }
            }
        }
        let expected_carriers=rows.iter().try_fold(0usize,|n,(_,row)|Ok::<_,E>(n+usize::from(signed_profile_generation_v3(&row.consent.terms).map_err(|_|bad())?==3)))?;
        if carrier_facts!=expected_carriers{return Err(bad())}
        if statuses != rows.len() {
            return Err(bad());
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(rows, lease))
    }
    /// All financial consumers use the checked reader; this aggregate is used
    /// only after state restore has validated every row.
    pub fn family_supplier_collateral_reserved_v1(&self, bond: &crate::palw_state_v2::PalwBondKeyV2, now: u64) -> u128 {
        let Some(route) = self.kernel_route() else { return 0 };
        let mut held = 0u128;
        for ((table, key), bytes) in &route.aux {
            if *table != FAMILY_SUPPLIER_TABLE_V1 || key.len() != 64 {
                continue;
            }
            let Ok((terms, _)) = borrow_family_capital_v1(bytes) else { return u128::MAX };
            let Ok(raw_id) = <[u8; 64]>::try_from(key.as_slice()) else { return u128::MAX };
            let id = Hash64::from_bytes(raw_id);
            // Fixed-size stack decode; capital accounting never allocates the
            // large signed catalogue or silently treats corrupt state as free.
            let status_key = family_status_key_v2(id);
            let Some((_, raw)) = route.aux.iter().find(|((t, k), _)| *t == FAMILY_SUPPLIER_TABLE_V1 && k.as_slice() == status_key)
            else {
                return u128::MAX;
            };
            let Ok(status) = borsh::from_slice::<FamilySupplierStatusV2>(raw) else { return u128::MAX };
            if status.version != 2 || status.contract != id {
                return u128::MAX;
            }
            if terms.supplier == *bond && status.defaulted_daa.is_none() && now <= terms.material_until_daa {
                held = held.saturating_add(u128::from(terms.collateral_sompi));
            }
        }
        held
    }
}
/// Stack-only view for the infallible collateral sum. Full restore separately
/// checks the exact typed codec, signatures, native inventory and money journal.
fn borrow_family_capital_v1(mut bytes: &[u8]) -> Result<(FamilySupplierTermsV1, Option<u64>), ()> {
    fn skip(bytes: &mut &[u8], n: usize) -> Result<(), ()> {
        if bytes.len() < n {
            return Err(());
        }
        *bytes = &bytes[n..];
        Ok(())
    }
    if !matches!(u16::deserialize_reader(&mut bytes).map_err(|_| ())?, 1 | 2) {
        return Err(());
    }
    let terms = FamilySupplierTermsV1::deserialize_reader(&mut bytes).map_err(|_| ())?;
    terms.validate_shape_v1().map_err(|_| ())?;
    for _ in 0..2 {
        let n = u32::deserialize_reader(&mut bytes).map_err(|_| ())? as usize;
        if n != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(());
        }
        skip(&mut bytes, n)?;
    }
    skip(&mut bytes, 64 + 8)?;
    let n = u32::deserialize_reader(&mut bytes).map_err(|_| ())?;
    for _ in 0..n {
        skip(&mut bytes, 64 + 8)?;
        let packets = u32::deserialize_reader(&mut bytes).map_err(|_| ())? as usize;
        skip(&mut bytes, packets.checked_mul(64).ok_or(())?)?;
    }
    let completed = u32::deserialize_reader(&mut bytes).map_err(|_| ())? as usize;
    skip(&mut bytes, completed.checked_mul(128).ok_or(())?)?;
    let defaulted = Option::<u64>::deserialize_reader(&mut bytes).map_err(|_| ())?;
    Ok((terms, defaulted))
}
impl BorshDeserialize for SignedFamilySupplierV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            terms: FamilySupplierTermsV1::deserialize_reader(r)?,
            supplier_signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
            sponsor_signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        })
    }
}
impl SignedFamilySupplierV1 {
    pub fn id_v1(&self) -> Result<Hash64, String> {
        self.terms.signing_message_v1()
    }
    /// `expected` is the native derivation at the admission seam, including the
    /// current signer identities and approved prices. This pure check cannot
    /// establish those facts or install a service reservation by itself.
    pub fn verify_v1(&self, expected: &FamilySupplierTermsV1, at: u64, supplier_key: &[u8], sponsor_key: &[u8]) -> Result<(), String> {
        if &self.terms != expected || at < expected.accept_from_daa || at > expected.accept_until_daa {
            return Err("family supplier consent differs from native inventory, policy or acceptance interval".into());
        }
        let msg = self.terms.signing_message_v1()?;
        for (key, signature) in [(supplier_key, &self.supplier_signature), (sponsor_key, &self.sponsor_signature)] {
            if signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
                || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                    key,
                    msg.as_byte_slice(),
                    signature,
                    FAMILY_SUPPLIER_SIGNATURE_CONTEXT_V1,
                )
            {
                return Err("family supplier or sponsor signature invalid".into());
            }
        }
        Ok(())
    }
}

/// SDK-ready draft builder. It creates no consensus object and reserves no tag.
pub fn sign_family_supplier_v1(
    terms: FamilySupplierTermsV1,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<SignedFamilySupplierV1> {
    let msg = terms.signing_message_v1().ok()?;
    let supplier_signature = sign(&terms.supplier, msg.as_byte_slice(), FAMILY_SUPPLIER_SIGNATURE_CONTEXT_V1)
        .filter(|s| s.len() == crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)?;
    let sponsor_signature = sign(&terms.sponsor, msg.as_byte_slice(), FAMILY_SUPPLIER_SIGNATURE_CONTEXT_V1)
        .filter(|s| s.len() == crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)?;
    Some(SignedFamilySupplierV1 { terms, supplier_signature, sponsor_signature })
}

impl crate::config::params::Params {
    pub fn sync_palw_court_family_supplier_v1(&mut self) {
        let from =
            self.palw_court_family_supplier_v1.filter(|f| *f != crate::config::params::ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_family_supplier_from(from);
        }
    }
    pub fn validate_palw_court_family_supplier_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::config::params::ForkActivation;
        let Some(at) = self.palw_court_family_supplier_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        let dependencies = [
            self.palw_tir_public_conviction_v1,
            self.palw_staged_tir_route_v1,
            self.palw_settlement_once_v1,
            self.palw_material_income_v3,
            self.palw_probabilistic_constraints_v1,
            self.palw_bond_budget_v1.as_ref().map(|f| f.activation),
            self.palw_claim_hard_deadline_v1.as_ref().map(|f| f.activation),
        ];
        if !matches!(self.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
            || dependencies.iter().any(|f| f.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > at.daa_score()))
        {
            return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                "family supplier fence requires ConsensusV2/public TIR/material intake/settlement/material income at or before it",
            ));
        }
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.family_supplier_from() != Some(at.daa_score()) {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid("family supplier activation mirror must be synchronized"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_supplier_fence_is_dedicated_some_only_and_preserves_dormant_identities() {
        use crate::config::params::ForkActivation;
        let base = crate::config::params::palw_t12_shipped_params();
        assert!(base.palw_court_family_supplier_v1.is_none());
        assert!(base.validate_palw_court_family_supplier_v1().is_ok());
        assert!(base.palw_fences_v1().iter().any(|(name, _)| *name == "palw_court_family_supplier_v1"));
        let mut never = base.clone();
        never.palw_court_family_supplier_v1 = Some(ForkActivation::never());
        never.sync_palw_court_family_supplier_v1();
        assert_eq!(base.consensus_params_id(), never.consensus_params_id());
        assert_eq!(base.consensus_schedule_id(), never.consensus_schedule_id());
        assert_eq!(base.consensus_identity_id(), never.consensus_identity_id());
        assert!(never.validate_palw_court_family_supplier_v1().is_ok());
        let mut early = base.clone();
        early.palw_court_family_supplier_v1 = Some(ForkActivation::new(9_000_000));
        early.sync_palw_court_family_supplier_v1();
        let mut late = base.clone();
        late.palw_court_family_supplier_v1 = Some(ForkActivation::new(9_000_001));
        late.sync_palw_court_family_supplier_v1();
        assert_ne!(base.consensus_params_id(), early.consensus_params_id());
        assert_ne!(base.consensus_schedule_id(), early.consensus_schedule_id());
        assert_ne!(early.consensus_params_id(), late.consensus_params_id());
        assert_ne!(early.consensus_schedule_id(), late.consensus_schedule_id());
        assert_eq!(early.consensus_identity_id(), late.consensus_identity_id(), "height belongs to schedule, not ruleset identity");
        assert!(early.validate_palw_court_family_supplier_v1().is_err(), "old-fence readiness does not implicitly arm new suppliers");
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &base.palw_consensus_mode {
            let armed = bundle.state.clone().with_family_supplier_from(Some(0));
            assert_eq!(
                borsh::to_vec(&bundle.state).unwrap(),
                borsh::to_vec(&armed).unwrap(),
                "runtime mirror never changes legacy StateParams wire"
            );
        }
    }
    #[test]
    fn family_supplier_joint_signatures_bind_scope_prices_and_horizons() {
        let supplier = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
        let sponsor = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([82; 32]);
        let h = |n| Hash64::from_bytes([n; 64]);
        let terms = FamilySupplierTermsV1 {
            version: 1,
            network: h(1),
            ruleset: h(2),
            claim: h(3),
            source: h(4),
            relative_graph: h(5),
            role: FamilySupplierRoleV1::ParameterAndPrompt,
            inventory: h(6),
            supplier: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(7), 0)),
            sponsor: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(8), 0)),
            max_material_bytes: 1024,
            accept_from_daa: 10,
            accept_until_daa: 20,
            respond_until_daa: 30,
            material_until_daa: 40,
            fee_profile: h(9),
            capital_profile: h(10),
            retention_profile: h(11),
            settlement_policy: h(12),
            fee_sompi: 17,
            collateral_sompi: 50,
            nonce: h(13),
        };
        let signed = sign_family_supplier_v1(terms.clone(), |who, msg, context| {
            let key = if *who == terms.supplier { &supplier.signing_key } else { &sponsor.signing_key };
            Some(libcrux_ml_dsa::ml_dsa_87::sign(key, msg, context, [83; 32]).unwrap().as_ref().to_vec())
        })
        .unwrap();
        let check = |s: &SignedFamilySupplierV1, expected: &FamilySupplierTermsV1, at| {
            s.verify_v1(expected, at, supplier.verification_key.as_ref(), sponsor.verification_key.as_ref())
        };
        assert!(check(&signed, &terms, 10).is_ok());
        assert!(check(&signed, &terms, 20).is_ok());
        assert!(check(&signed, &terms, 9).is_err());
        assert!(check(&signed, &terms, 21).is_err());
        for change in 0..7 {
            let mut changed = signed.clone();
            match change {
                0 => changed.terms.inventory = h(14),
                1 => changed.terms.role = FamilySupplierRoleV1::SemanticTable,
                2 => changed.terms.max_material_bytes += 1,
                3 => changed.terms.material_until_daa += 1,
                4 => changed.terms.fee_sompi += 1,
                5 => changed.terms.collateral_sompi += 1,
                _ => changed.terms.ruleset = h(15),
            }
            assert!(check(&changed, &terms, 10).is_err());
            assert!(check(&changed, &changed.terms, 10).is_err(), "old signatures cannot approve changed native terms");
        }
        let mut swapped = signed.clone();
        std::mem::swap(&mut swapped.supplier_signature, &mut swapped.sponsor_signature);
        assert!(check(&swapped, &terms, 10).is_err());
        let mut row = FamilySupplierRowV1 {
            version: 1,
            consent: signed,
            allocation: h(20),
            accepted_daa: 10,
            targets: vec![FamilySupplierTargetV1 { id: h(21), max_bytes: 1024, packets: vec![[22; 64]] }],
            completed: BTreeMap::new(),
            defaulted_daa: None,
            fee_state: FamilySupplierFeeStateV1::Held,
        };
        for defaulted in [None, Some(20)] {
            row.defaulted_daa = defaulted;
            let bytes = borsh::to_vec(&row).unwrap();
            let (view, actual) = borrow_family_capital_v1(&bytes).unwrap();
            assert_eq!(view, row.consent.terms);
            assert_eq!(actual, defaulted);
            crate::palw_native_resource_v1::preflight_native_value_v1::<FamilySupplierRowV1>(bytes.as_slice(), bytes.len() as u64)
                .unwrap();
            for cut in [0, 2, bytes.len() / 2] {
                assert!(borrow_family_capital_v1(&bytes[..cut]).is_err());
            }
        }
    }
}

crate::palw_native_resource_v1::native_enum!(FamilySupplierRoleV1;ParameterAndPrompt,SemanticTable,TokenizerArtifact,PrefixGeneratedSelectionAndStop);
crate::palw_native_resource_v1::native_struct!(FamilySupplierTermsV1;version,network,ruleset,claim,source,relative_graph,role,inventory,supplier,sponsor,max_material_bytes,accept_from_daa,accept_until_daa,respond_until_daa,material_until_daa,fee_profile,capital_profile,retention_profile,settlement_policy,fee_sompi,collateral_sompi,nonce);
crate::palw_native_resource_v1::native_struct!(SignedFamilySupplierV1;terms,supplier_signature,sponsor_signature);
crate::palw_native_resource_v1::native_struct!(FamilySupplierTargetV1;id,max_bytes,packets);
crate::palw_native_resource_v1::native_enum!(FamilySupplierFeeStateV1;Held,Queued{payee,queue,receipt},Paid{payee,daa});
crate::palw_native_resource_v1::native_struct!(FamilySupplierRowV1;version,consent,allocation,accepted_daa,targets,completed,defaulted_daa,fee_state);
crate::palw_native_resource_v1::wire::wire_struct!(FamilySupplierTermsV1;u16,Hash64,Hash64,Hash64,Hash64,Hash64,FamilySupplierRoleV1,Hash64,PalwBondKeyV2,PalwBondKeyV2,u64,u64,u64,u64,u64,Hash64,Hash64,Hash64,Hash64,u64,u64,Hash64);
crate::palw_native_resource_v1::wire::wire_struct!(SignedFamilySupplierV1;FamilySupplierTermsV1,Vec<u8>,Vec<u8>);
crate::palw_native_resource_v1::wire::wire_struct!(FamilySupplierTargetV1;Hash64,u64,Vec<[u8;64]>);
crate::palw_native_resource_v1::wire::wire_struct!(FamilySupplierRowV1;u16,SignedFamilySupplierV1,Hash64,u64,Vec<FamilySupplierTargetV1>,BTreeMap<Hash64,Hash64>,Option<u64>,FamilySupplierFeeStateV1);
impl crate::palw_native_resource_v1::NativeWireScanV1 for FamilySupplierRoleV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        if r.byte()? <= 3 {
            Ok(Default::default())
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "family supplier role"))
        }
    }
}
impl crate::palw_native_resource_v1::NativeWireScanV1 for FamilySupplierFeeStateV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => Ok(Default::default()),
            1 => <(PalwBondKeyV2, Hash64, Hash64) as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r),
            2 => <(PalwBondKeyV2, u64) as crate::palw_native_resource_v1::NativeWireScanV1>::scan_native_wire_v1(r),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "family fee state")),
        }
    }
}

crate::palw_native_resource_v1::native_struct!(FamilySupplierStatusV2;version,contract,defaulted_daa,fee_state);
crate::palw_native_resource_v1::native_struct!(FamilySupplierCompletionV2;version,contract,target,digest);
crate::palw_native_resource_v1::wire::wire_struct!(FamilySupplierStatusV2;u16,Hash64,Option<u64>,FamilySupplierFeeStateV1);
crate::palw_native_resource_v1::wire::wire_struct!(FamilySupplierCompletionV2;u16,Hash64,Hash64,Hash64);

/// New-profile native calendar birth. This immutable sidecar records the
/// selected concrete packing, while signatures retain the relative inventory,
/// existing inclusion interval, hard and material horizons.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct FamilyPrimaryWindowV2 {pub version:u16,pub contract:Hash64,pub target:Hash64,pub first_daa:u64,pub span_daa:u64}
pub(crate) fn family_primary_window_key_v2(contract:Hash64,target:Hash64)->[u8;129]{let mut key=[0u8;129];key[0]=3;key[1..65].copy_from_slice(contract.as_byte_slice());key[65..].copy_from_slice(target.as_byte_slice());key}
crate::palw_native_resource_v1::native_struct!(FamilyPrimaryWindowV2;version,contract,target,first_daa,span_daa);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyPrimaryWindowV2;u16,Hash64,Hash64,u64,u64);

/// Exact signed profile dispatch; a matching fee hash alone cannot silently
/// choose a different inventory, capital, retention or settlement derivation.
pub(crate) fn signed_profile_generation_v3(terms:&FamilySupplierTermsV1)->Result<u16,String>{
    let signed=[terms.fee_profile,terms.capital_profile,terms.retention_profile,terms.settlement_policy];
    if signed==crate::palw_state_v2::family_supplier_profiles_v1().map_err(|e|e.to_string())?{Ok(1)}
    else if signed==crate::palw_state_v2::family_supplier_profiles_v2().map_err(|e|e.to_string())?{Ok(2)}
    else if signed==crate::palw_state_v2::family_supplier_profiles_v3().map_err(|e|e.to_string())?{Ok(3)}
    else{Err("family contract unknown signed derivation profiles".into())}
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct FamilyCarrierProjectionRowV3{pub version:u16,pub contract:Hash64,pub claim:Hash64,pub accepted_daa:u64,pub accepted_block:Hash64,pub accepted_blue_score:u64,pub source:Hash64,pub allocation:Hash64,pub funding_contract:Hash64,pub facts:crate::palw_court_family_material_v1::FamilyCarrierProjectionFactsV3}
pub(crate) fn family_carrier_projection_key_v3(contract:Hash64)->[u8;65]{let mut key=[0;65];key[0]=4;key[1..].copy_from_slice(contract.as_byte_slice());key}
crate::palw_native_resource_v1::native_struct!(FamilyCarrierProjectionRowV3;version,contract,claim,accepted_daa,accepted_block,accepted_blue_score,source,allocation,funding_contract,facts);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyCarrierProjectionRowV3;u16,Hash64,Hash64,u64,Hash64,u64,Hash64,Hash64,Hash64,crate::palw_court_family_material_v1::FamilyCarrierProjectionFactsV3);
