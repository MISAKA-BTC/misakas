//! Rooted prospective contract and owned continuation rows. Immutable contracts are stamped
//! at claim acceptance; mutable checkpoints never contain another copy of public job/grammar.
use crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1;
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
/// Heap-heavy owned continuations share an immutable allocation between candidate
/// states. Only the row actually changed by a fold pays its checkpoint COW copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSharedCourtV1<T>(std::sync::Arc<T>);
impl<T> From<Box<T>> for PalwSharedCourtV1<T> {
    fn from(value: Box<T>) -> Self {
        Self(std::sync::Arc::from(value))
    }
}
impl<T> std::ops::Deref for PalwSharedCourtV1<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> std::ops::DerefMut for PalwSharedCourtV1<T> {
    fn deref_mut(&mut self) -> &mut T {
        std::sync::Arc::make_mut(&mut self.0)
    }
}
impl<T> AsRef<T> for PalwSharedCourtV1<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}
impl<T: BorshSerialize> BorshSerialize for PalwSharedCourtV1<T> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.0.as_ref().serialize(writer)
    }
}
impl<T: BorshDeserialize> BorshDeserialize for PalwSharedCourtV1<T> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(Self(std::sync::Arc::new(T::deserialize_reader(reader)?)))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedCourtKeyV1 {
    Claim(Hash64),
    Owner(Hash64, PalwBondKeyV2),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedClaimContractV1 {
    pub terms: PalwRenderingClaimTermsV1,
    pub producer: PalwBondKeyV2,
    /// All bound seats remain held even after their panel/claim rows retire.
    pub panel: std::collections::BTreeSet<PalwBondKeyV2>,
    pub prefix_custody: Option<crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedCourtRowV1 {
    Claim(PalwStagedClaimContractV1),
    Constraint(PalwSharedCourtV1<PalwOwnedConstraintStreamV1>),
    Tokenizer(PalwSharedCourtV1<crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1>),
    Prefix(PalwSharedCourtV1<crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1>),
    PublicPrefix(PalwSharedCourtV1<crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCourtV1>),
    ClaimBundle(Box<crate::palw_staged_bundle_v1::PalwStagedBundleContractV1>),
    OwnerBundle(PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwOwnedStagedBundleV1>),
}
impl PalwStagedCourtRowV1 {
    pub fn active_row_v1(&self) -> Option<&Self> {
        match self {
            Self::OwnerBundle(bundle) => bundle.active_v1(),
            Self::Claim(_) | Self::ClaimBundle(_) => None,
            _ => Some(self),
        }
    }
    pub fn active_row_mut_v1(&mut self) -> Option<&mut Self> {
        match self {
            Self::OwnerBundle(bundle) => bundle.active_mut_v1(),
            Self::Claim(_) | Self::ClaimBundle(_) => None,
            _ => Some(self),
        }
    }
    pub fn public_cache_leases_v1(&self) -> Vec<crate::palw_prefix_history_v1::PalwStagedCacheLeaseV1> {
        let (claim, owner, reserved_daa, seed_id, material_until_daa) = match self {
            Self::Constraint(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::Tokenizer(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::Prefix(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::PublicPrefix(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::OwnerBundle(bundle) => {
                let mut leases = vec![crate::palw_prefix_history_v1::PalwStagedCacheLeaseV1 {
                    claim: bundle.claim,
                    owner: bundle.owner,
                    reserved_daa: bundle.reserved_daa,
                    seed_id: bundle.plan.id_v1(bundle.claim),
                    material_until_daa: bundle.plan.terms.material_until_daa,
                }];
                for stage in &bundle.opened {
                    leases.extend(stage.row.0.public_cache_leases_v1());
                }
                return leases;
            }
            _ => return vec![],
        };
        vec![crate::palw_prefix_history_v1::PalwStagedCacheLeaseV1 { claim, owner, reserved_daa, seed_id, material_until_daa }]
    }
    pub(crate) fn owner_accounting_v1(&self) -> Option<(Hash64, PalwBondKeyV2, u64, u64)> {
        match self {
            Self::Constraint(c) => Some((c.claim, c.reserver, c.deposit_locked, c.terms.material_until_daa)),
            Self::Tokenizer(c) => Some((c.claim, c.reserver, c.deposit_locked, c.terms.material_until_daa)),
            Self::Prefix(c) => Some((c.claim, c.reserver, c.deposit_locked, c.terms.material_until_daa)),
            Self::PublicPrefix(c) => Some((c.claim, c.reserver, c.deposit_locked, c.terms.material_until_daa)),
            Self::OwnerBundle(c) => Some((c.claim, c.owner, c.plan.terms.plan.deposit_sompi, c.plan.terms.material_until_daa)),
            _ => None,
        }
    }
    pub(crate) fn checkpoint_v1(&self) -> Result<PalwStagedCheckpointV1, String> {
        match self {
            Self::Constraint(c) => Ok(PalwStagedCheckpointV1::Constraint(c.checkpoint_v1())),
            Self::Tokenizer(c) => Ok(PalwStagedCheckpointV1::Tokenizer(Box::new(c.checkpoint_v1()))),
            Self::Prefix(c) => Ok(PalwStagedCheckpointV1::Prefix(Box::new(c.checkpoint_v1()))),
            Self::PublicPrefix(c) => Ok(PalwStagedCheckpointV1::PublicPrefix(Box::new(c.checkpoint_v1()))),
            Self::OwnerBundle(c) => Ok(PalwStagedCheckpointV1::Bundle(Box::new(c.checkpoint_v1()?))),
            _ => Err("claim contract is not an owned mutable court".into()),
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwStagedCheckpointV1) -> Result<(), String> {
        match (self, cp) {
            (Self::Constraint(c), PalwStagedCheckpointV1::Constraint(cp)) => c.restore_checkpoint_v1(cp),
            (Self::Tokenizer(c), PalwStagedCheckpointV1::Tokenizer(cp)) => c.restore_checkpoint_v1(cp),
            (Self::Prefix(c), PalwStagedCheckpointV1::Prefix(cp)) => c.restore_checkpoint_v1(cp),
            (Self::PublicPrefix(c), PalwStagedCheckpointV1::PublicPrefix(cp)) => c.restore_checkpoint_v1(cp),
            (Self::OwnerBundle(c), PalwStagedCheckpointV1::Bundle(cp)) => c.restore_checkpoint_v1(cp),
            _ => Err("staged checkpoint changes immutable court type".into()),
        }
    }
    pub fn check(&self, key: &PalwStagedCourtKeyV1) -> Result<(), String> {
        match (key, self) {
            (PalwStagedCourtKeyV1::Claim(_), Self::Claim(c))
                if c.terms.accepted_daa < c.terms.open_until_daa
                    && c.terms.open_until_daa < c.terms.stream_until_daa
                    && c.terms.stream_until_daa < c.terms.collect_until_daa
                    && c.terms.collect_until_daa <= c.terms.material_until_daa
                    && c.terms.material_until_daa <= c.terms.bond_exit_after_daa =>
            {
                if c.prefix_custody.as_ref().is_some_and(|p| p.accepted_daa != c.terms.accepted_daa) {
                    return Err("prefix custody changes immutable accepting clock".into());
                }
                c.prefix_custody.as_ref().map(|p| p.check(&c.panel)).unwrap_or(Ok(()))
            }
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::Constraint(c)) if (*claim, *bond) == c.key() => c.check(),
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::Tokenizer(c)) if (*claim, *bond) == c.key() => c.check(),
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::Prefix(c)) if (*claim, *bond) == (c.claim, c.reserver) => c.check(),
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::PublicPrefix(c)) if (*claim, *bond) == (c.claim, c.reserver) => c.check(),
            (PalwStagedCourtKeyV1::Claim(_), Self::ClaimBundle(c)) => {
                c.plan.check_v1()?;
                Self::Claim(c.claim.clone()).check(key)
            }
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::OwnerBundle(c)) if (*claim, *bond) == (c.claim, c.owner) => c.check_v1(),
            _ => Err("staged rooted row has inconsistent key/immutable contract".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedCheckpointV1 {
    Constraint(crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintCheckpointV1),
    Tokenizer(Box<crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCheckpointV1>),
    Prefix(Box<crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCheckpointV1>),
    PublicPrefix(Box<crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCheckpointV1>),
    Bundle(Box<crate::palw_staged_bundle_v1::PalwStagedBundleCheckpointV1>),
}
impl PalwStagedCheckpointV1 {
    pub fn public_prefix_checkpoint_v1(&self) -> Option<&crate::palw_prefix_public_owned_v1::PalwOwnedPublicPrefixCheckpointV1> {
        match self {
            Self::PublicPrefix(cp) => Some(cp),
            Self::Bundle(cp) => cp.active_checkpoint.as_ref().and_then(|cp| match &cp.0 {
                Self::PublicPrefix(cp) => Some(cp.as_ref()),
                _ => None,
            }),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwStagedFaultV1 {
    Constraint(crate::palw_constraint_stream_court_v1::PalwConstraintStreamFaultV1),
    Tokenizer(crate::palw_tokenizer_owned_court_v1::PalwTokenizerCourtFaultV1),
    Prefix(crate::palw_prefix_owned_court_v1::PalwPrefixCourtFaultV1),
}
/// A reference to a completed ROOTED court, not a challenger-supplied parser checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedCertificateRefV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
}
impl PalwStagedCertificateRefV1 {
    pub fn resolve(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        expected_root: Hash64,
    ) -> Result<crate::palw_constraint_stream_court_v1::PalwConstraintStreamFaultV1, String> {
        match self.resolve_fault_v1(state, expected_root)? {
            PalwStagedFaultV1::Constraint(fault) => Ok(fault),
            PalwStagedFaultV1::Tokenizer(_) | PalwStagedFaultV1::Prefix(_) => {
                Err("claim fault is not a constrained execution fault".into())
            }
        }
    }
    pub fn resolve_fault_v1(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        expected_root: Hash64,
    ) -> Result<PalwStagedFaultV1, String> {
        state.staged_claim_contract_v1(&self.claim).ok_or("certificate claim has no prospective contract")?;
        if let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(self.claim, self.reserver)
            && self.seed_id == bundle.plan.id_v1(self.claim)
        {
            let fault = bundle.inventory_fault_v1()?.ok_or("inventory completed without an objective length fault")?;
            if fault.execution_root != expected_root || self.reserved_daa != bundle.reserved_daa {
                return Err("inventory certificate changes accepted execution or owner clock".into());
            }
            return Ok(PalwStagedFaultV1::Tokenizer(fault));
        }
        match state
            .staged_owned_row_for_seed_v1(self.claim, self.reserver, self.seed_id)
            .ok_or("certificate has no literal retained owned seed")?
        {
            PalwStagedCourtRowV1::Constraint(row) => {
                if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
                    return Err("certificate changes immutable owner/seed/reservation".into());
                }
                let fault = row.certificate()?.ok_or("rooted court completed without objective fault")?;
                if (fault.claim_id, fault.reservation_owner, fault.reserved_daa, fault.seed_id, fault.execution_root)
                    != (self.claim, self.reserver, self.reserved_daa, self.seed_id, expected_root)
                {
                    return Err("rooted certificate does not convict target execution".into());
                }
                Ok(PalwStagedFaultV1::Constraint(fault))
            }
            PalwStagedCourtRowV1::Tokenizer(row) => {
                if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
                    return Err("certificate changes immutable owner/seed/reservation".into());
                }
                let fault = row.certificate()?.ok_or("rooted tokenizer court completed honestly")?;
                if (fault.claim, fault.reserver, fault.reserved_daa, fault.seed_id, fault.execution_root)
                    != (self.claim, self.reserver, self.reserved_daa, self.seed_id, expected_root)
                {
                    return Err("rooted tokenizer certificate does not convict target claim".into());
                }
                Ok(PalwStagedFaultV1::Tokenizer(fault))
            }
            PalwStagedCourtRowV1::Prefix(row) => {
                if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
                    return Err("prefix certificate changes immutable owner/seed/reservation".into());
                }
                let fault = row.certificate()?.ok_or("rooted prefix court completed honestly")?;
                if (fault.claim, fault.reserver, fault.reserved_daa, fault.seed_id, fault.execution_root)
                    != (self.claim, self.reserver, self.reserved_daa, self.seed_id, expected_root)
                {
                    return Err("rooted prefix certificate does not convict target claim".into());
                }
                Ok(PalwStagedFaultV1::Prefix(fault))
            }
            PalwStagedCourtRowV1::PublicPrefix(row) => {
                if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
                    return Err("public prefix certificate changes immutable owner/seed/reservation".into());
                }
                let fault = row.certificate()?.ok_or("public prefix court has no completed objective fault")?;
                if (fault.claim, fault.reserver, fault.reserved_daa, fault.seed_id, fault.execution_root)
                    != (self.claim, self.reserver, self.reserved_daa, self.seed_id, expected_root)
                {
                    return Err("public prefix certificate does not convict target claim".into());
                }
                Ok(PalwStagedFaultV1::Prefix(fault))
            }
            _ => Err("claim contract alone cannot convict".into()),
        }
    }
}
pub fn palw_staged_reference_v1(
    kind: crate::palw_offence_v1::PalwOffenceKindV1,
    evidence: &[u8],
) -> Option<PalwStagedCertificateRefV1> {
    use crate::palw_offence_attribution_v1::*;
    use crate::palw_offence_v1::{PalwOffenceKindV1 as K, PalwPanelContradictionV1 as C};
    let contradiction = match kind {
        K::ExecutorRefuted => borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok()?.contradiction,
        K::PanelFalseValidV2 => borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok()?.contradiction,
        _ => return None,
    };
    match contradiction {
        C::StagedCourt { reference } => Some(reference),
        _ => None,
    }
}
pub fn palw_is_staged_evidence_v1(kind: crate::palw_offence_v1::PalwOffenceKindV1, evidence: &[u8]) -> bool {
    palw_staged_reference_v1(kind, evidence).is_some()
}
/// The accepting block's exact clock, never the parent tip or a caller's declared timestamp.
pub fn palw_staged_collection_open_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    reference: &PalwStagedCertificateRefV1,
    accepting_daa: u64,
) -> bool {
    state
        .staged_claim_contract_v1(&reference.claim)
        .is_some_and(|c| accepting_daa >= c.terms.accepted_daa && accepting_daa < c.terms.collect_until_daa)
}

/// Read-only accepted snapshot. None owner cursor means this literal live reservation has
/// not opened its one paid court. No parser checkpoint or prior operand bytes are returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedOwnedProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
    /// Literal currently signed prospective custodians, including the producer. Not the
    /// drawn membership list. Only an authenticated qualified contract supplies this set.
    pub prefix_custodians: Option<std::collections::BTreeSet<PalwBondKeyV2>>,
    pub prefix_page_root: Option<(Hash64, u64, u8)>,
    pub bundle: Option<crate::palw_staged_bundle_v1::PalwStagedBundleProgressV1>,
    pub seed_id: Option<Hash64>,
    pub cursor: Option<PalwStagedOwnedCursorV1>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwStagedOwnedCursorV1 {
    Prefix {
        moves_used: u64,
        cursor: crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1,
        completed: Option<bool>,
    },
    Constraint {
        phase: crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        eog_offset: Option<u32>,
        rendering: Option<(u32, bool, u32, u32)>,
        next_rendering_id: Option<u32>,
        completed: Option<bool>,
    },
    Tokenizer {
        offset: u64,
        tokenizer_sealed: bool,
        pending_key: Option<(u64, Hash64)>,
        key_root: Hash64,
        key_events: u64,
        rendering_offset: u32,
        /// None is incomplete or unauthenticated; Some(false) honest, Some(true) objective fault.
        completed: Option<bool>,
    },
    PublicPrefix {
        cursor: crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1,
        history_count: u64,
        history_head: Hash64,
        demand: Option<crate::palw_prefix_public_source_v1::PalwPrefixPublicDemandV1>,
        pending: Option<crate::palw_prefix_public_source_v1::PalwPrefixPublicProgressV1>,
        completed: Option<bool>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwStagedPaidAuthorityV1 {
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub terms: PalwRenderingClaimTermsV1,
}
/// An already admitted paid row is pursuit authority until its original material
/// horizon. A different claim outcome cannot revoke it by releasing ordinary DA
/// reservations. This never authorizes opening a new seed or extends any clock.
/// The caller supplies a checked accepted state (native readers use verified Arc
/// snapshots); this bounded identity read does not rehash pending witness buffers.
pub fn palw_staged_paid_authority_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<PalwStagedPaidAuthorityV1>, String> {
    let Some(row) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    if row.owner_accounting_v1().is_none_or(|(claim, owner, _, _)| (claim, owner) != key) {
        return Err("paid continuation changes literal rooted owner key".into());
    }
    let (reserved_daa, seed_id, terms, root) = match row {
        PalwStagedCourtRowV1::Constraint(c) => (c.reserved_daa, c.seed_id(), c.terms, c.court().execution_root_v1()),
        PalwStagedCourtRowV1::Tokenizer(c) => (c.reserved_daa, c.seed_id(), c.terms, c.execution_root_v1()),
        PalwStagedCourtRowV1::Prefix(c) => (c.reserved_daa, c.seed_id(), c.terms, c.execution_root_v1()),
        PalwStagedCourtRowV1::PublicPrefix(c) => (c.reserved_daa, c.seed_id(), c.terms, c.execution_root_v1()),
        PalwStagedCourtRowV1::OwnerBundle(c) => {
            let seed = c
                .active_seed_v1()
                .map_or_else(|| c.plan.id_v1(c.claim), |seed| Hash64::from_bytes(seed.signing_message().try_into().expect("64")));
            let terms = match c.active_v1() {
                Some(PalwStagedCourtRowV1::Constraint(row)) => row.terms,
                Some(PalwStagedCourtRowV1::Tokenizer(row)) => row.terms,
                Some(PalwStagedCourtRowV1::Prefix(row)) => row.terms,
                Some(PalwStagedCourtRowV1::PublicPrefix(row)) => row.terms,
                _ => c.plan.terms,
            };
            (c.reserved_daa, seed, terms, c.execution_root)
        }
        _ => return Err("paid continuation key contains a claim contract".into()),
    };
    let contract = state.staged_claim_contract_v1(&key.0).ok_or("paid continuation has no claim contract")?;
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &key.0)
        .ok_or("paid continuation has no accepted claim or liability")?;
    if root != target.execution_root
        || contract.producer != target.executor_bond
        || terms.accepted_daa != contract.terms.accepted_daa
        || terms.material_until_daa > contract.terms.material_until_daa
        || terms.bond_exit_after_daa < terms.material_until_daa
    {
        return Err("paid continuation changes accepted execution or retained liability limits".into());
    }
    Ok(Some(PalwStagedPaidAuthorityV1 { reserved_daa, seed_id, terms }))
}
pub fn palw_staged_owned_progress_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<PalwStagedOwnedProgressV1>, String> {
    let point = state.last_point().ok_or("staged progress snapshot has no accepted clock")?;
    let Some(contract) = state.staged_claim_contract_v1(&key.0) else { return Ok(None) };
    let reserved_daa = match state.legacy_dispute_reservation_v1(&key.0, &key.1) {
        Some(reservation) => reservation.reserved_daa,
        None => match palw_staged_paid_authority_v1(state, key)? {
            Some(paid) if point.daa_score < paid.terms.material_until_daa => paid.reserved_daa,
            _ => return Ok(None),
        },
    };
    let owned = state.staged_owned_row_v1(key.0, key.1);
    let (seed_id, cursor) = match owned.and_then(PalwStagedCourtRowV1::active_row_v1) {
        Some(PalwStagedCourtRowV1::PublicPrefix(row)) => {
            row.check()?;
            let claim = state.claim(&key.0).ok_or("public prefix claim absent")?;
            let semantic = state.claim_semantics_v1(&key.0).ok_or("public prefix registered semantics absent")?;
            row.check_claim_identity_v1(
                claim.execution_root,
                claim.job_identity,
                semantic.cu_ruleset_id,
                claim.bond,
                &contract.panel,
            )?;
            if row.reserved_daa != reserved_daa {
                return Err("public prefix progress changes reservation".into());
            }
            (
                Some(row.seed_id()),
                Some(PalwStagedOwnedCursorV1::PublicPrefix {
                    cursor: row.cursor_v1()?,
                    history_count: row.history_cursor_v1().0,
                    history_head: row.history_cursor_v1().1,
                    demand: row.next_demand_v1(point.daa_score)?,
                    pending: row.pending_progress_v1(),
                    completed: row.certificate().ok().map(|r| r.is_some()),
                }),
            )
        }
        Some(PalwStagedCourtRowV1::Tokenizer(row)) => {
            row.check()?;
            if row.reserved_daa != reserved_daa {
                return Err("staged progress reservation differs from owned seed".into());
            }
            let court = row.court();
            (
                Some(row.seed_id()),
                Some(PalwStagedOwnedCursorV1::Tokenizer {
                    offset: court.tokenizer_offset(),
                    tokenizer_sealed: court.tokenizer_sealed_v1(),
                    pending_key: court.pending_key(),
                    key_root: court.key_set_root(),
                    key_events: court.key_events(),
                    rendering_offset: court.rendering_offset(),
                    completed: row.certificate().ok().map(|r| r.is_some()),
                }),
            )
        }
        Some(PalwStagedCourtRowV1::Constraint(row)) => {
            row.check()?;
            if row.reserved_daa != reserved_daa {
                return Err("staged progress reservation differs from owned seed".into());
            }
            (
                Some(row.seed_id()),
                Some(PalwStagedOwnedCursorV1::Constraint {
                    phase: row.court().phase(),
                    eog_offset: row.court().eog_offset_v1(),
                    rendering: row.court().rendering_cursor_v1(),
                    next_rendering_id: row.court().next_rendering_id().ok(),
                    completed: row.certificate().ok().map(|r| r.is_some()),
                }),
            )
        }
        Some(PalwStagedCourtRowV1::Prefix(row)) => {
            row.check()?;
            if row.reserved_daa != reserved_daa {
                return Err("prefix progress reservation differs from owned seed".into());
            }
            (
                Some(row.seed_id()),
                Some(PalwStagedOwnedCursorV1::Prefix {
                    moves_used: row.moves_used,
                    cursor: row.cursor_v1()?,
                    completed: row.certificate().ok().map(|r| r.is_some()),
                }),
            )
        }
        Some(_) => return Err("staged owner key contains a claim contract".into()),
        None => (
            match owned {
                Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => Some(bundle.plan.id_v1(bundle.claim)),
                _ => None,
            },
            None,
        ),
    };
    Ok(Some(PalwStagedOwnedProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        claim: key.0,
        reserver: key.1,
        reserved_daa,
        terms: match owned.and_then(PalwStagedCourtRowV1::active_row_v1) {
            Some(PalwStagedCourtRowV1::PublicPrefix(row)) => row.terms,
            Some(PalwStagedCourtRowV1::Prefix(row)) => row.terms,
            Some(PalwStagedCourtRowV1::Tokenizer(row)) => row.terms,
            Some(PalwStagedCourtRowV1::Constraint(row)) => row.terms,
            _ => match owned {
                Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => bundle.plan.terms,
                _ => contract.terms,
            },
        },
        prefix_custodians: match owned.and_then(PalwStagedCourtRowV1::active_row_v1) {
            Some(PalwStagedCourtRowV1::PublicPrefix(row)) => Some(row.custodian_bonds_v1()),
            _ => contract
                .prefix_custody
                .as_ref()
                .map(|c| c.current_signed_v1().expected_panel.into_iter().chain(std::iter::once(contract.producer)).collect()),
        },
        prefix_page_root: match owned.and_then(PalwStagedCourtRowV1::active_row_v1) {
            Some(PalwStagedCourtRowV1::Prefix(row)) => row.court_v1().page_root_v1(),
            Some(PalwStagedCourtRowV1::PublicPrefix(row)) => row.page_root_v1(),
            _ => None,
        },
        bundle: match owned {
            Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => Some(bundle.progress_v1()),
            _ => match state.staged_courts.get(&PalwStagedCourtKeyV1::Claim(key.0)) {
                Some(PalwStagedCourtRowV1::ClaimBundle(bundle)) => Some(crate::palw_staged_bundle_v1::PalwStagedBundleProgressV1 {
                    bundle_id: bundle.plan.id_v1(key.0),
                    plan: bundle.plan.clone(),
                    active_index: None,
                    pending_seed: None,
                    inventory: None,
                }),
                _ => None,
            },
        },
        seed_id,
        cursor,
    }))
}

/// Only the prefix family retains its full admitted binding/job. Other typed
/// courts must not invent an original signed binding from partial public fields.
pub fn palw_staged_owned_context_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<(crate::palw_step_leg::PalwStepBindingV2, crate::palw_freeprompt_v3::PalwFreePromptJobV3)>, String> {
    let Some(paid) = palw_staged_paid_authority_v1(state, key)? else { return Ok(None) };
    let point = state.last_point().ok_or("paid context has no accepted clock")?;
    if point.daa_score < paid.terms.accepted_daa || point.daa_score >= paid.terms.material_until_daa {
        return Ok(None);
    }
    let context = match state.staged_owned_row_v1(key.0, key.1) {
        Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => {
            let Some(stage) = bundle.active.and_then(|index| bundle.opened.get(index as usize)) else { return Ok(None) };
            return Ok(Some((stage.context.binding.as_ref().clone(), stage.context.job.as_ref().clone())));
        }
        Some(PalwStagedCourtRowV1::Prefix(row)) => row.court_v1().admitted_context_v1(),
        Some(PalwStagedCourtRowV1::PublicPrefix(row)) => row.admitted_context_v1(),
        _ => return Ok(None),
    };
    Ok(Some((context.0.clone(), context.1.clone())))
}
pub fn palw_staged_owned_seed_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>, String> {
    let Some(paid) = palw_staged_paid_authority_v1(state, key)? else { return Ok(None) };
    let point = state.last_point().ok_or("paid seed has no accepted clock")?;
    if point.daa_score < paid.terms.accepted_daa || point.daa_score >= paid.terms.material_until_daa {
        return Ok(None);
    }
    Ok(match state.staged_owned_row_v1(key.0, key.1) {
        Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => bundle.active_seed_v1().cloned(),
        _ => None,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPrefixResponseDutyV1 {
    pub progress: PalwStagedOwnedProgressV1,
    pub material: crate::palw_producer_v2::PalwDisclosureDutyV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPrefixResponsePageV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub duties: Vec<PalwPrefixResponseDutyV1>,
    pub next: Option<(u64, Hash64, PalwBondKeyV2)>,
}

/// Public material is retained by its original paid owner row even after pursuit release.
/// Export generation/clock checks use one rooted snapshot, never an archive's fork flag.
///
/// `state` must be an authenticated accepted snapshot: carriage loading checks every
/// row and its committed root; native readers use that cached Arc or a checked delta
/// walk. This is not an untrusted-state validator. Rechecking the whole pending court
/// here would hash its large witness buffers for every anonymous history request.
pub fn palw_prefix_history_request_check_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    expected_snapshot: Option<crate::BlockHash>,
    request: &crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
) -> Result<(), String> {
    request.check_v1()?;
    let point = state.last_point().ok_or("public history export lacks accepted clock")?;
    if expected_snapshot.is_some_and(|expected| expected != point.block) {
        return Err("public history export snapshot changed branch".into());
    }
    let Some(PalwStagedCourtRowV1::PublicPrefix(row)) =
        state.staged_owned_row_for_seed_v1(request.claim, request.owner, request.seed_id)
    else {
        return Err("public history export changes typed court family".into());
    };
    let (count, head) = row.history_cursor_v1();
    if row.seed_id() != request.seed_id
        || row.key() != (request.claim, request.owner, request.reserved_daa)
        || request.material_until_daa != row.terms.material_until_daa
        || point.daa_score >= request.material_until_daa
        || request.sequence > count
        || (request.sequence == count && request.head != head)
    {
        return Err("public history export changes frozen seed/clock or accepted head generation".into());
    }
    Ok(())
}

/// Duties and offsets share a single accepted snapshot; speculative mempool demands are
/// never served and a restart requires no locally saved producer cursor.
pub fn palw_prefix_response_duties_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    signer: PalwBondKeyV2,
    after: Option<(u64, Hash64, PalwBondKeyV2)>,
) -> Result<PalwPrefixResponsePageV1, String> {
    use crate::palw_prefix_public_source_v1::PalwPrefixPublicSignerCursorV1;
    use crate::palw_producer_v2::{PalwDisclosureDutyV1, PalwDisclosureRoleV1};
    let point = state.last_point().ok_or("public prefix duties lack accepted clock")?;
    let now = point.daa_score;
    let (rows, next) = state.staged_public_prefix_duties_after_v1(signer, now, after, 64)?;
    let mut duties = Vec::new();
    for row in rows {
        let Some(pending) = row.pending_progress_v1() else { continue };
        if now >= pending.deadline_daa
            || !pending.signers.get(&signer).is_some_and(|cursor| *cursor != PalwPrefixPublicSignerCursorV1::Invalid)
        {
            continue;
        }
        let Some(claim) = state.claim(&row.claim) else { continue };
        let Some(semantics) = state.claim_semantics_v1(&row.claim) else { continue };
        let progress = palw_staged_owned_progress_v1(state, (row.claim, row.reserver))?.ok_or("pending prefix has no reservation")?;
        duties.push(PalwPrefixResponseDutyV1 {
            progress,
            material: PalwDisclosureDutyV1 {
                claim_id: row.claim,
                accepted_block: claim.accepted_block,
                class_id: claim.class_id,
                artifact_root: semantics.artifact_root,
                executor_bond: claim.bond,
                discloser: signer,
                role: if signer == claim.bond { PalwDisclosureRoleV1::Producer } else { PalwDisclosureRoleV1::CoveringSigner },
                signer_rank: 0,
                unit: crate::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(
                    crate::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::KernelWitness { leaf: pending.source.leaf_index },
                ),
                deadline_daa: pending.deadline_daa,
                disclose_window_daa: pending.deadline_daa.saturating_sub(now),
                in_run_rows: 0,
                trace_root: claim.trace_root,
                execution_root: claim.execution_root,
                free_prompt: true,
                job_identity: claim.job_identity,
            },
        });
    }
    duties.sort_by_key(|d| (d.material.deadline_daa, d.progress.claim, d.progress.reserver));
    Ok(PalwPrefixResponsePageV1 { snapshot_block: point.block, now_daa: now, duties, next })
}
