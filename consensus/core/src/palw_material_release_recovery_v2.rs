//! Bounded, explicitly signed recovery of a material release after historical
//! headers/deltas have been pruned. Only presence in a native finalized state
//! authorizes deletion; absence, a local clock or an off-chain boolean never does.
use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_mode_v2::PalwModeV2Error,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

pub const PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2: u8 = 134;
pub const PALW_MATERIAL_RELEASE_RECOVERY_MAX_CONTRACTS_V2: u32 = 4096;
pub const PALW_MATERIAL_RELEASE_RECOVERY_MAX_GENERATIONS_V2: u16 = 64;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialReleaseRecoveryPolicyV2 {
    pub version: u16,
    /// Includes active contracts AND terminal receipts awaiting all providers.
    pub max_reserved_contracts: u32,
    pub max_generations_per_contract: u16,
    /// One persistent current ACK checkpoint per consenting provider identity.
    pub max_provider_streams: u32,
    /// Reserved at enrollment, not acquired when Release must make progress.
    pub max_reserved_history_cells: u32,
    pub measurement_root: Hash64,
}
impl PalwMaterialReleaseRecoveryPolicyV2 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 2
            || self.max_reserved_contracts == 0
            || self.max_reserved_contracts > PALW_MATERIAL_RELEASE_RECOVERY_MAX_CONTRACTS_V2
            || self.max_provider_streams == 0
            || self.max_provider_streams > PALW_MATERIAL_RELEASE_RECOVERY_MAX_CONTRACTS_V2 * 8
            || self.max_generations_per_contract == 0
            || self.max_generations_per_contract > PALW_MATERIAL_RELEASE_RECOVERY_MAX_GENERATIONS_V2
            || self.max_reserved_history_cells < u32::from(self.max_generations_per_contract) * 8
            || self.measurement_root == Hash64::default()
        {
            return Err("release recovery capacity/provenance is missing or excessive");
        }
        Ok(())
    }
    pub fn id(&self) -> Hash64 {
        digest(b"MISAKA/PALW/RELEASE-RECOVERY-POLICY/V2", self)
    }
    pub fn reserved_cells(&self) -> u32 {
        u32::from(self.max_generations_per_contract) * 8
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMaterialReleaseRecoveryFenceV2 {
    pub activation: ForkActivation,
    pub policy: PalwMaterialReleaseRecoveryPolicyV2,
}
impl PalwMaterialReleaseRecoveryFenceV2 {
    /// Identity/fence test fixture only. These are not approved release terms.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwMaterialReleaseRecoveryPolicyV2 {
                version: 2,
                max_reserved_contracts: 16,
                max_generations_per_contract: 4,
                max_provider_streams: 128,
                max_reserved_history_cells: 512,
                measurement_root: Hash64::from_bytes([0xA6; 64]),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMaterialReleaseRecoveryMirrorV2 {
    pub from_daa: u64,
    pub policy: PalwMaterialReleaseRecoveryPolicyV2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialRecoveryTermsV2 {
    pub version: u16,
    pub root: Hash64,
    pub initial_consent: Hash64,
    pub sponsor: PalwBondKeyV2,
    pub nonce: Hash64,
    pub policy: Hash64,
    pub max_generations: u16,
    /// Each provider agrees to keep the terminal receipt until every generation's
    /// providers acknowledge their own durable deletion. This is not a TTL.
    pub all_generation_ack: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialRecoverySignatureV2 {
    pub provider: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMaterialRecoveryActionV2 {
    Enroll {
        terms: PalwMaterialRecoveryTermsV2,
        providers: Vec<PalwMaterialRecoverySignatureV2>,
    },
    Replace {
        recovery: Hash64,
        replacement: Box<crate::palw_monetary_material_v1::PalwMonetaryMaterialReplacementV1>,
        providers: Vec<PalwMaterialRecoverySignatureV2>,
        sponsor_signature: Vec<u8>,
    },
    Acknowledge {
        root: Hash64,
        receipt: Hash64,
        stream: Hash64,
        previous: Hash64,
        sequence: u64,
    },
    RetireProviderStream {
        provider: PalwBondKeyV2,
        stream: Hash64,
        sequence: u64,
    },
    /// Atomic new financial service + recovery contract; all reservations are
    /// checked before committing either financial or recovery writes.
    FundedEnroll {
        monetary: Box<crate::palw_monetary_material_v1::PalwMonetaryMaterialEnrollmentV1>,
        terms: PalwMaterialRecoveryTermsV2,
        providers: Vec<PalwMaterialRecoverySignatureV2>,
        sponsor_signature: Vec<u8>,
    },
}
impl PalwMaterialRecoveryActionV2 {
    pub fn root(&self) -> Hash64 {
        match self {
            Self::Enroll { terms, .. } => terms.root,
            Self::Replace { replacement, .. } => replacement.root,
            Self::Acknowledge { root, .. } => *root,
            Self::RetireProviderStream { stream, .. } => *stream,
            Self::FundedEnroll { terms, .. } => terms.root,
        }
    }
    /// Independent recovery domain, all obligations/fees and ordered provider keys
    /// signed by every new provider; signatures never create a nonce lottery.
    pub fn payload(&self) -> Vec<u8> {
        match self {
            Self::Enroll { terms, providers } => borsh::to_vec(&(
                b"material-recovery-enroll/v2".as_slice(),
                terms,
                providers.iter().map(|s| s.provider).collect::<Vec<_>>(),
            ))
            .unwrap(),
            Self::Replace { recovery, replacement, providers, .. } => borsh::to_vec(&(
                b"material-recovery-replace/v2".as_slice(),
                recovery,
                replacement.payload(),
                providers.iter().map(|s| s.provider).collect::<Vec<_>>(),
            ))
            .unwrap(),
            Self::Acknowledge { root, receipt, stream, previous, sequence } => {
                borsh::to_vec(&(b"material-recovery-ack/v2".as_slice(), root, receipt, stream, previous, sequence)).unwrap()
            }
            Self::RetireProviderStream { provider, stream, sequence } => {
                borsh::to_vec(&(b"material-recovery-stream-retire/v2".as_slice(), provider, stream, sequence)).unwrap()
            }
            Self::FundedEnroll { monetary, terms, providers, .. } => borsh::to_vec(&(
                b"material-recovery-funded-enroll/v2".as_slice(),
                monetary.payload(),
                terms,
                providers.iter().map(|s| s.provider).collect::<Vec<_>>(),
            ))
            .unwrap(),
        }
    }
    pub fn provider_signatures(&self) -> &[PalwMaterialRecoverySignatureV2] {
        match self {
            Self::Enroll { providers, .. } | Self::Replace { providers, .. } | Self::FundedEnroll { providers, .. } => providers,
            Self::Acknowledge { .. } | Self::RetireProviderStream { .. } => &[],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialRecoveryGenerationV2 {
    pub consent: Hash64,
    pub providers: Vec<PalwBondKeyV2>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialRecoveryContractV2 {
    pub terms: PalwMaterialRecoveryTermsV2,
    pub enrolled_daa: u64,
    pub generations: Vec<PalwMaterialRecoveryGenerationV2>,
}
impl PalwMaterialRecoveryContractV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", &self.terms)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialTerminalReceiptV2 {
    pub contract: PalwMaterialRecoveryContractV2,
    pub final_consent: Hash64,
    pub tail_block: Hash64,
    /// Informational audit commitment; GC never requires this old header.
    pub release_block: Hash64,
    pub acknowledged: BTreeSet<PalwBondKeyV2>,
}
impl PalwMaterialTerminalReceiptV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"MISAKA/PALW/MATERIAL-TERMINAL-RECEIPT/V2", &(&self.contract, self.final_consent, self.tail_block, self.release_block))
    }
    pub fn providers(&self) -> BTreeSet<PalwBondKeyV2> {
        self.contract.generations.iter().flat_map(|g| g.providers.iter().copied()).collect()
    }
    pub fn covers(&self, root: Hash64, stored_consent: Hash64) -> bool {
        self.contract.terms.root == root && self.contract.generations.iter().any(|g| g.consent == stored_consent)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMaterialRecoveryRowV2 {
    Active(PalwMaterialRecoveryContractV2),
    Terminal(PalwMaterialTerminalReceiptV2),
}
impl PalwMaterialRecoveryRowV2 {
    pub fn contract(&self) -> &PalwMaterialRecoveryContractV2 {
        match self {
            Self::Active(c) => c,
            Self::Terminal(r) => &r.contract,
        }
    }
    pub fn validate(&self, policy: &PalwMaterialReleaseRecoveryPolicyV2) -> Result<(), &'static str> {
        let c = self.contract();
        let t = &c.terms;
        if t.version != 2
            || t.root == Hash64::default()
            || t.nonce == Hash64::default()
            || t.initial_consent == Hash64::default()
            || t.policy != policy.id()
            || !t.all_generation_ack
            || t.max_generations != policy.max_generations_per_contract
            || c.generations.is_empty()
            || c.generations.len() > t.max_generations as usize
            || c.generations[0].consent != t.initial_consent
        {
            return Err("recovery contract terms/history differ");
        }
        let mut seen = BTreeSet::new();
        for g in &c.generations {
            if g.consent == Hash64::default()
                || !seen.insert(g.consent)
                || !(2..=8).contains(&g.providers.len())
                || g.providers.windows(2).any(|p| p[0] >= p[1])
            {
                return Err("recovery generation is duplicate, unordered or excessive");
            }
        }
        if let Self::Terminal(r) = self {
            if r.final_consent != c.generations.last().unwrap().consent
                || r.tail_block == Hash64::default()
                || r.release_block == Hash64::default()
                || !r.acknowledged.is_subset(&r.providers())
                || r.acknowledged == r.providers()
            {
                return Err("terminal receipt or acknowledgement set differs");
            }
        }
        Ok(())
    }
}
fn digest<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(domain, value))
}

/// Exact current-root proof of a successfully APPLIED provider ACK, retained
/// through terminal-receipt deletion and historical pruning. A provider signs
/// only one in-flight ACK; its durable outbox retires before the next signature.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialProviderAckCheckpointV2 {
    pub version: u16,
    pub provider: PalwBondKeyV2,
    pub stream: Hash64,
    pub created_daa: u64,
    pub sequence: u64,
    /// One ACK increment reserved for each accepted unacknowledged contract.
    pub reserved_acks: u64,
    pub last_receipt: Hash64,
}
pub fn material_provider_stream_key_v2(provider: &PalwBondKeyV2) -> Vec<u8> {
    let mut key = vec![0xA1];
    key.extend(borsh::to_vec(provider).unwrap());
    key
}
impl PalwChainStateV2 {
    pub fn material_provider_ack_checkpoint_v2(&self, provider: &PalwBondKeyV2) -> Option<PalwMaterialProviderAckCheckpointV2> {
        self.kernel_route()?.aux_row(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, &material_provider_stream_key_v2(provider))
    }
}

impl PalwChainStateV2 {
    pub fn material_recovery_row_v2(&self, root: Hash64) -> Option<PalwMaterialRecoveryRowV2> {
        self.kernel_route()?.aux_row(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, root.as_byte_slice())
    }
    pub fn require_material_recovery_v2(
        &self,
        params: &PalwStateParamsV2,
        root: Hash64,
        accepted_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(p) = params.material_release_recovery_at_v2(accepted_daa) else { return Ok(()) };
        // Existing work remains under its original terms; a future new consumer
        // must enroll explicitly and cannot reconstruct missing old generations.
        let from = params.material_release_recovery_from_v2().unwrap();
        if self.claim(&root).is_some_and(|c| c.accepted_daa < from)
            || self.kernel_route().and_then(|r| r.kernel_claim_row_v1(&root)).is_some_and(|c| c.committed_daa < from)
        {
            return Ok(());
        }
        let Some(PalwMaterialRecoveryRowV2::Active(c)) = self.material_recovery_row_v2(root) else {
            return Err(PalwStateV2Error::ExecV2Refused("new material lacks signed historical-release recovery enrollment".into()));
        };
        PalwMaterialRecoveryRowV2::Active(c).validate(p).map_err(|e| PalwStateV2Error::ExecV2Refused(e.into()))
    }
    pub fn assert_material_release_recovery_consistency_v2(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let bad = |s: &str| PalwStateV2Error::CarriageInconsistent(format!("release recovery: {s}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let mut cells = 0u64;
        let mut count = 0u64;
        let mut streams = 0u64;
        let mut reserved = std::collections::BTreeMap::new();
        let mut expected = std::collections::BTreeMap::<PalwBondKeyV2, u64>::new();
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 {
                continue;
            }
            if key.len() != 64 {
                let stream: PalwMaterialProviderAckCheckpointV2 =
                    borsh::from_slice(bytes).map_err(|_| bad("malformed provider stream"))?;
                let p =
                    params.material_release_recovery_at_v2(stream.created_daa).ok_or_else(|| bad("provider stream before gate"))?;
                streams += 1;
                if key != &material_provider_stream_key_v2(&stream.provider)
                    || stream.version != 2
                    || stream.stream == Hash64::default()
                    || stream.sequence.checked_add(stream.reserved_acks).is_none()
                    || (stream.sequence == 0) != (stream.last_receipt == Hash64::default())
                    || streams > u64::from(p.max_provider_streams)
                {
                    return Err(bad("provider stream key/count/checkpoint differs"));
                }
                reserved.insert(stream.provider, stream.reserved_acks);
                continue;
            }
            let row: PalwMaterialRecoveryRowV2 = borsh::from_slice(bytes).map_err(|_| bad("malformed typed row"))?;
            let c = row.contract();
            let p = params.material_release_recovery_at_v2(c.enrolled_daa).ok_or_else(|| bad("row before own fence"))?;
            row.validate(p).map_err(bad)?;
            if c.terms.root.as_byte_slice() != key.as_slice() {
                return Err(bad("root key differs"));
            }
            count += 1;
            cells += u64::from(p.reserved_cells());
            if count > u64::from(p.max_reserved_contracts) || cells > u64::from(p.max_reserved_history_cells) {
                return Err(bad("reserved capacity exceeded"));
            }
            let providers: BTreeSet<_> = c.generations.iter().flat_map(|g| g.providers.iter().copied()).collect();
            for provider in &providers {
                let acked = matches!(&row,PalwMaterialRecoveryRowV2::Terminal(r) if r.acknowledged.contains(provider));
                if !acked {
                    *expected.entry(*provider).or_default() += 1;
                }
                if !acked && self.material_provider_ack_checkpoint_v2(provider).is_none() {
                    return Err(bad("unacknowledged provider stream is absent"));
                }
            }
            match &row {
                PalwMaterialRecoveryRowV2::Active(c) => {
                    let m = self.monetary_material_row_v1(c.terms.root).ok_or_else(|| bad("active monetary row absent"))?;
                    if m.consent != c.generations.last().unwrap().consent || m.terms.sponsor != c.terms.sponsor {
                        return Err(bad("active generation differs"));
                    }
                }
                PalwMaterialRecoveryRowV2::Terminal(r) => {
                    if self.monetary_material_row_v1(r.contract.terms.root).is_some() {
                        return Err(bad("terminal root still live"));
                    }
                }
            }
        }
        if reserved.iter().any(|(provider, n)| *n != expected.remove(provider).unwrap_or(0)) || !expected.is_empty() {
            return Err(bad("provider reserved ACK credits differ from accepted obligations"));
        }
        Ok(())
    }
}
impl Params {
    pub fn validate_palw_material_release_recovery_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(f) = self.palw_material_release_recovery_v2.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        f.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        if self
            .palw_monetary_material_v1
            .as_ref()
            .is_none_or(|m| m.activation == ForkActivation::never() || m.activation.daa_score() > f.activation.daa_score())
        {
            return Err(PalwModeV2Error::Invalid("recovery requires the signed monetary covenant first"));
        }
        Err(PalwModeV2Error::Invalid("release recovery cannot be armed: cost and bounded provider-ack liveness approval pending"))
    }
}

/// Sign all newly enrolled providers, the ordinary replacement obligations and
/// the sponsor envelope. A partial signer set never produces a carrier.
pub fn material_recovery_object_v2(
    network: Hash64,
    mut action: PalwMaterialRecoveryActionV2,
    signer: PalwBondKeyV2,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as CONTEXT, palw_provider_court_message_v1};
    if let PalwMaterialRecoveryActionV2::FundedEnroll { monetary, sponsor_signature, .. } = &mut action {
        let payload = monetary.payload();
        for offer in &mut monetary.offers {
            let message = palw_provider_court_message_v1(network, 240, &offer.provider, &payload);
            offer.signature = sign(&offer.provider, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
        }
        let sponsor = monetary.terms.sponsor;
        let message = palw_provider_court_message_v1(network, 240, &sponsor, &payload);
        *sponsor_signature = sign(&sponsor, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
    }
    if let PalwMaterialRecoveryActionV2::Replace { replacement, sponsor_signature, .. } = &mut action {
        let payload = replacement.payload();
        for offer in &mut replacement.enrollment.offers {
            let message = palw_provider_court_message_v1(network, 241, &offer.provider, &payload);
            offer.signature = sign(&offer.provider, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
        }
        let sponsor = replacement.enrollment.terms.sponsor;
        let message = palw_provider_court_message_v1(network, 241, &sponsor, &payload);
        *sponsor_signature = sign(&sponsor, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
    }
    let payload = action.payload();
    let providers = match &mut action {
        PalwMaterialRecoveryActionV2::Enroll { providers, .. }
        | PalwMaterialRecoveryActionV2::Replace { providers, .. }
        | PalwMaterialRecoveryActionV2::FundedEnroll { providers, .. } => providers.as_mut_slice(),
        PalwMaterialRecoveryActionV2::Acknowledge { .. } | PalwMaterialRecoveryActionV2::RetireProviderStream { .. } => &mut [],
    };
    for provider in providers {
        let message = palw_provider_court_message_v1(network, 242, &provider.provider, &payload);
        provider.signature = sign(&provider.provider, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
    }
    let action = crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::RecoveryV2(Box::new(action));
    let payload = borsh::to_vec(&action).ok()?;
    let message = palw_provider_court_message_v1(network, 242, &signer, &payload);
    let signature = sign(&signer, message.as_byte_slice(), CONTEXT).filter(|s| !s.is_empty())?;
    Some(crate::palw_state_v2::PalwConsensusObjectV2::MonetaryMaterialActionV1 { action: Box::new(action), signer, signature })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    #[test]
    fn recovery_variant_has_its_own_a2_fence_and_ack_stream_changes_are_signed() {
        use crate::palw_lifecycle_objects_v2::{PalwLifecycleKindFenceV1, PalwLifecycleKindOwnerV1, palw_lifecycle_kind_owner_v1};
        let h = |n| Hash64::from_bytes([n; 64]);
        let provider = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(2), 0));
        let action =
            PalwMaterialRecoveryActionV2::Acknowledge { root: h(1), receipt: h(2), stream: h(3), previous: h(4), sequence: 5 };
        let object = material_recovery_object_v2(h(6), action.clone(), provider, |_, _, _| Some(vec![7])).unwrap();
        assert_eq!(
            palw_lifecycle_kind_owner_v1(&object),
            PalwLifecycleKindOwnerV1::Fence(PalwLifecycleKindFenceV1::MaterialReleaseRecoveryV2)
        );
        let mut different = action.clone();
        if let PalwMaterialRecoveryActionV2::Acknowledge { sequence, .. } = &mut different {
            *sequence += 1;
        }
        assert_ne!(action.payload(), different.payload());
        let old =
            crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::Release { root: h(1), consent: h(2), tail_block: h(3) };
        let bytes = borsh::to_vec(&old).unwrap();
        assert_eq!(bytes[0], 6, "legacy Release discriminant is unchanged");
        assert_eq!(borsh::from_slice::<crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>(&bytes).unwrap(), old);
    }
}
