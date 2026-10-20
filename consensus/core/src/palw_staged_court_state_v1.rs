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
    InventoryOpen(Hash64, PalwBondKeyV2),
    Inventory(Hash64),
    /// A registered TIR class's published layout (behind `palw_staged_tir_route_v1`).
    TirLayout(Hash64),
    /// Dormant versioned TIR acquisition sidecars; not legacy Owner courts.
    TirAcquisition(Hash64, PalwBondKeyV2, Hash64),
    TirRawChunk(Hash64),
    TirAcceptedBody(Hash64),
    /// Claim-wide preparation is fixed at original acceptance, not owner Open.
    TirPreparation(Hash64, Hash64),
    /// Original versioned full-allocation TIR frame; never an old ClaimBundle.
    TirClaim(Hash64),
    /// G14 R7: a staged conviction witness being assembled, per (claim, filer).
    TirWitness(Hash64, PalwBondKeyV2),
    /// RFC-0002 02-A: a registered TIR class's bound `PALWART3` container (behind `palw_tir_container_identity_v1`).
    TirContainer(Hash64),
    /// Proposed service ledger; appended tag, no existing tag changes.
    CourtService,
    /// RFC-0028 candidate prerequisite history; appended key tag 13.
    CourtDutyClock(Hash64),
    CourtSharedContext(Hash64),
    CourtSharedQuery(Hash64),
    CourtSharedIds(Hash64),
    CourtSharedStream(Hash64),
    CourtSharedChunk(Hash64,u64),
    CourtSharedResponse(Hash64,PalwBondKeyV2),
    /// RFC-0030 isolated candidate; appended key20, no old tag changes.
    CourtEndArchive(Hash64),
    CourtCloseReceipt(Hash64,crate::palw_state_v2::PalwCourtSideV1),
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
    ArtifactInventoryOpen(PalwSharedCourtV1<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1>),
    ArtifactInventory(crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1),
    /// The TIR prefix-state family court (behind `palw_staged_tir_route_v1`).
    TirPrefix(PalwSharedCourtV1<crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCourtV1>),
    /// A registered TIR class's layout, published once, authenticated by the record's
    /// `layout_digest` (behind `palw_staged_tir_route_v1`): what a TIR prefix quote and court
    /// rebuild the class's step space from.
    TirLayout(Box<crate::palw_tir_class_v1::PalwTirLayoutV1>),
    TirAcquisition(PalwSharedCourtV1<crate::palw_tir_public_state_v1::PalwTirPublicAcquisitionRowV1>),
    TirRawChunk(PalwSharedCourtV1<crate::palw_tir_public_state_v1::PalwTirPublicChunkRowV1>),
    TirAcceptedBody(PalwSharedCourtV1<crate::palw_tir_public_state_v1::PalwTirPublicAcceptedRowV1>),
    TirPreparation(PalwSharedCourtV1<crate::palw_tir_public_state_v1::PalwTirPublicPreparationRowV1>),
    TirClaim(PalwSharedCourtV1<crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1>),
    TirWitness(PalwSharedCourtV1<crate::palw_tir_public_carriage_v1::PalwTirPublicWitnessRowV1>),
    /// A registered TIR class's `PALWART3` container, bound once by its registrant (behind `palw_tir_container_identity_v1`).
    TirContainer(Box<crate::palw_tir_container_v1::PalwTirContainerRowV1>),
    CourtService(PalwSharedCourtV1<crate::palw_court_service_state_v2::CourtServiceStateV2>),
    CourtDutyClock(Box<crate::palw_court_duty_clock_v1::CourtDutyClockV1>),
    CourtSharedContext(Box<crate::palw_court_shared_tir_state_v1::SharedTirContextV1>),
    CourtSharedQuery(Box<crate::palw_court_shared_tir_state_v1::SharedTirQueryV1>),
    CourtSharedIds(Box<crate::palw_court_shared_tir_state_v1::SharedTirIdsV1>),
    CourtSharedStream(Box<crate::palw_court_shared_tir_stream_state_v1::SharedTirStreamV1>),
    CourtSharedChunk(PalwSharedCourtV1<crate::palw_court_shared_tir_stream_state_v1::SharedTirChunkV1>),
    CourtSharedResponse(Box<crate::palw_court_shared_tir_response_v1::SharedDataResponseV1>),
    CourtEndArchive(PalwSharedCourtV1<crate::palw_court_end_archive_v1::PendingCourtArchiveV1>),
    CourtCloseReceipt(PalwSharedCourtV1<crate::palw_court_close_receipt_v1::CourtCloseReceiptV1>),
}
impl PalwStagedCourtRowV1 {
    pub fn active_row_v1(&self) -> Option<&Self> {
        match self {
            Self::OwnerBundle(bundle) => bundle.active_v1(),
            Self::Claim(_) | Self::ClaimBundle(_) | Self::ArtifactInventoryOpen(_) | Self::ArtifactInventory(_) | Self::TirLayout(_)
            | Self::TirAcquisition(_) | Self::TirRawChunk(_) | Self::TirAcceptedBody(_) | Self::TirPreparation(_) | Self::TirClaim(_)
            | Self::TirWitness(_) | Self::TirContainer(_) | Self::CourtService(_) | Self::CourtDutyClock(_) | Self::CourtSharedContext(_) | Self::CourtSharedQuery(_) | Self::CourtSharedIds(_) | Self::CourtSharedStream(_) | Self::CourtSharedChunk(_) | Self::CourtSharedResponse(_) | Self::CourtEndArchive(_) | Self::CourtCloseReceipt(_) => None,
            _ => Some(self),
        }
    }
    pub fn active_row_mut_v1(&mut self) -> Option<&mut Self> {
        match self {
            Self::OwnerBundle(bundle) => bundle.active_mut_v1(),
            Self::Claim(_) | Self::ClaimBundle(_) | Self::ArtifactInventoryOpen(_) | Self::ArtifactInventory(_) | Self::TirLayout(_)
            | Self::TirAcquisition(_) | Self::TirRawChunk(_) | Self::TirAcceptedBody(_) | Self::TirPreparation(_) | Self::TirClaim(_)
            | Self::TirWitness(_) | Self::TirContainer(_) | Self::CourtService(_) | Self::CourtDutyClock(_) | Self::CourtSharedContext(_) | Self::CourtSharedQuery(_) | Self::CourtSharedIds(_) | Self::CourtSharedStream(_) | Self::CourtSharedChunk(_) | Self::CourtSharedResponse(_) | Self::CourtEndArchive(_) | Self::CourtCloseReceipt(_) => None,
            _ => Some(self),
        }
    }
    pub fn public_cache_leases_v1(&self) -> Vec<crate::palw_prefix_history_v1::PalwStagedCacheLeaseV1> {
        let (claim, owner, reserved_daa, seed_id, material_until_daa) = match self {
            Self::Constraint(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::Tokenizer(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::Prefix(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::PublicPrefix(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
            Self::TirPrefix(c) => (c.claim, c.reserver, c.reserved_daa, c.seed_id(), c.terms.material_until_daa),
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
            Self::TirPrefix(c) => Some((c.claim, c.reserver, c.deposit_locked, c.terms.material_until_daa)),
            Self::OwnerBundle(c) => Some((c.claim, c.owner, c.plan.terms.plan.deposit_sompi, c.plan.terms.material_until_daa)),
            Self::ArtifactInventoryOpen(c) => Some((c.offer.artifact_hash, c.offer.opener, c.plan.deposit_sompi, c.until_daa)),
            _ => None,
        }
    }
    pub(crate) fn checkpoint_v1(&self) -> Result<PalwStagedCheckpointV1, String> {
        match self {
            Self::Constraint(c) => Ok(PalwStagedCheckpointV1::Constraint(c.checkpoint_v1())),
            Self::Tokenizer(c) => Ok(PalwStagedCheckpointV1::Tokenizer(Box::new(c.checkpoint_v1()))),
            Self::Prefix(c) => Ok(PalwStagedCheckpointV1::Prefix(Box::new(c.checkpoint_v1()))),
            Self::PublicPrefix(c) => Ok(PalwStagedCheckpointV1::PublicPrefix(Box::new(c.checkpoint_v1()))),
            Self::TirPrefix(c) => Ok(PalwStagedCheckpointV1::TirPrefix(Box::new(c.checkpoint_v1()))),
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
            (Self::TirPrefix(c), PalwStagedCheckpointV1::TirPrefix(cp)) => c.restore_checkpoint_v1(cp),
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
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::TirPrefix(c)) if (*claim, *bond) == c.key() => c.check(),
            (PalwStagedCourtKeyV1::TirLayout(_), Self::TirLayout(layout))
                if layout.version == crate::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1 =>
            {
                Ok(())
            }
            (PalwStagedCourtKeyV1::Claim(claim), Self::ClaimBundle(c)) => {
                c.plan.check_v1()?;
                c.config.check()?;
                c.history_custody.check_v1(&c.claim.panel)?;
                if c.history_custody.pledge.claim != *claim
                    || c.history_custody.pledge.material_until_daa < c.claim.terms.material_until_daa
                    || c.claim.terms != c.plan.terms
                    || c.history_custody.producer != c.claim.producer
                    || c.original_binding.execution_root_v1() != c.history_custody.pledge.execution_root
                    || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(c.original_binding.job_context_v1())
                        != c.history_custody.pledge.job_identity
                    || c.original_binding.job_context_v1().cu_ruleset_id != c.history_custody.pledge.cu_ruleset_id
                {
                    return Err("bundle contract substitutes original pledge claim or retention promise".into());
                }
                Self::Claim(c.claim.clone()).check(key)
            }
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::OwnerBundle(c)) if (*claim, *bond) == (c.claim, c.owner) => c.check_v1(),
            (PalwStagedCourtKeyV1::InventoryOpen(hash, owner), Self::ArtifactInventoryOpen(c))
                if (*hash, *owner) == (c.offer.artifact_hash, c.offer.opener) =>
            {
                c.check_v1()
            }
            (PalwStagedCourtKeyV1::Inventory(hash), Self::ArtifactInventory(c)) if *hash == c.artifact_hash => c.check_v1(),
            (PalwStagedCourtKeyV1::TirAcquisition(..), Self::TirAcquisition(row)) => row.check_structure_v1(key),
            (PalwStagedCourtKeyV1::TirRawChunk(head), Self::TirRawChunk(row)) => row.check_structure_v1(*head),
            (PalwStagedCourtKeyV1::TirAcceptedBody(hash), Self::TirAcceptedBody(row)) => row.check_structure_v1(*hash),
            (PalwStagedCourtKeyV1::TirPreparation(..), Self::TirPreparation(row)) => row.check_structure_v1(key),
            (PalwStagedCourtKeyV1::TirClaim(..), Self::TirClaim(frame)) => frame.check_structure_v1(key),
            (PalwStagedCourtKeyV1::TirWitness(claim, filer), Self::TirWitness(row)) => row.check_structure_v1(*claim, *filer),
            (PalwStagedCourtKeyV1::CourtService, Self::CourtService(row)) => row.check(),
            (PalwStagedCourtKeyV1::CourtDutyClock(_), Self::CourtDutyClock(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedContext(_), Self::CourtSharedContext(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedQuery(_), Self::CourtSharedQuery(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedIds(_), Self::CourtSharedIds(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedStream(_), Self::CourtSharedStream(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedChunk(..), Self::CourtSharedChunk(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtSharedResponse(..), Self::CourtSharedResponse(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtEndArchive(..), Self::CourtEndArchive(row)) => row.check(key),
            (PalwStagedCourtKeyV1::CourtCloseReceipt(..), Self::CourtCloseReceipt(row)) => row.check(key),
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
    TirPrefix(Box<crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCheckpointV1>),
}
impl PalwStagedCheckpointV1 {
    pub fn history_recoveries_v1(
        &self,
    ) -> Option<&std::collections::BTreeMap<Hash64, PalwSharedCourtV1<crate::palw_history_custody_v1::PalwHistoryRecoveryV1>>> {
        match self {
            Self::Bundle(cp) => Some(&cp.history_recovery),
            _ => None,
        }
    }
    pub fn history_recovery_for_seed_v1(&self, seed: Hash64) -> Option<&crate::palw_history_custody_v1::PalwHistoryRecoveryV1> {
        match self {
            Self::Bundle(cp) => cp.history_recovery.get(&seed).map(|row| row.as_ref()),
            _ => None,
        }
    }
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
pub struct PalwStagedSelectedProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub session: crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1,
    pub context: Option<PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>>,
    /// A TIR claim's native source context (behind `palw_staged_tir_route_v1`); exclusive with `context`.
    pub native: Option<PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwNativeSourceContextV1>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTokenizerArtifactInventoryProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub class_id: Hash64,
    pub semantics: crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    pub certificate: Option<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    pub attempt: Option<crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryV1>,
}
pub fn palw_tokenizer_artifact_inventory_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    class: Hash64,
    opener: PalwBondKeyV2,
) -> Result<Option<PalwTokenizerArtifactInventoryProgressV1>, String> {
    let point = state.last_point().ok_or("artifact inventory has no accepted snapshot")?;
    let Some(semantics) = state.class_semantics_v1(&class).filter(|s| s.raw_tokenizer_bytes.is_some()) else { return Ok(None) };
    Ok(Some(PalwTokenizerArtifactInventoryProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        class_id: class,
        semantics: semantics.clone(),
        certificate: state.staged_tokenizer_inventory_v1(semantics.tokenizer_id).cloned(),
        attempt: state.staged_tokenizer_inventory_open_v1(semantics.tokenizer_id, opener).cloned(),
    }))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedPublicContextV1 {
    pub binding: crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    pub job: crate::palw_freeprompt_v3::PalwFreePromptJobV3,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedHistoryDutyPageV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub keys: Vec<(Hash64, PalwBondKeyV2, Hash64)>,
    pub next: Option<(u64, Hash64, PalwBondKeyV2, Hash64)>,
}
/// Live required-family claims whose external producer delegated its answer slot to `custodian`.
/// One bounded page: at most `visit` claim contracts are scanned, delegated or not, and the
/// cursor continues after the last scanned claim. Nothing here grants authority; the native
/// validator alone accepts the custodian's signature for the producer's typed answers.
pub fn palw_staged_custody_delegations_to_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    custodian: PalwBondKeyV2,
    after: Option<Hash64>,
    visit: usize,
) -> Result<(Vec<(Hash64, PalwBondKeyV2)>, Option<Hash64>), String> {
    let point = state.last_point().ok_or("custody delegation page has no accepted clock")?;
    let mut out = Vec::new();
    let mut last = None;
    for (visited, (claim, row)) in state.staged_claim_rows_after_v1(after).enumerate() {
        if visited >= visit.clamp(1, 256) {
            return Ok((out, last));
        }
        last = Some(claim);
        if let PalwStagedCourtRowV1::ClaimBundle(contract) = row
            && contract.producer_custodian == Some(custodian)
            && point.daa_score < contract.claim.terms.material_until_daa
        {
            out.push((claim, contract.claim.producer));
        }
    }
    Ok((out, None))
}
/// What a Valid seat needs to sign its own history custody grant for one required-family claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHistoryGrantDutyV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub claim: Hash64,
    /// The exact accepted history contract commitment (the producer's signed pledge id).
    pub pledge_id: Hash64,
    /// The claim contract's original acceptance clock, carried as the grant move's reserved DAA.
    pub accepted_daa: u64,
    pub material_until_daa: u64,
}
/// A seat's grant is due only when native state has ALREADY credited exactly this signed Valid
/// receipt in the claim's semantic custody, the seat is in the history contract's bound panel,
/// and it is not yet a history custodian. The receipt itself comes from the seat's own signing;
/// nothing here makes membership or a foreign receipt a credential.
pub fn palw_staged_history_grant_duty_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
    seat: PalwBondKeyV2,
    receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
) -> Result<Option<PalwHistoryGrantDutyV1>, String> {
    let point = state.last_point().ok_or("history grant snapshot has no accepted clock")?;
    let Some(contract) = state.staged_bundle_contract_v1(claim) else { return Ok(None) };
    if receipt.claim != claim
        || receipt.seat_bond != seat
        || receipt.verdict != crate::palw_panel_v2::PalwReceiptVerdictV2::Valid
        || seat == contract.history_custody.producer
        || point.daa_score >= contract.claim.terms.material_until_daa
    {
        return Ok(None);
    }
    let Some(prefix) = contract.claim.prefix_custody.as_ref() else { return Ok(None) };
    if !prefix.matches_credited_receipt_v1(claim, receipt)
        || contract.history_custody.custody.credited.contains_key(&seat)
        || !contract.history_custody.custody.expected_panel.contains(&seat)
    {
        return Ok(None);
    }
    Ok(Some(PalwHistoryGrantDutyV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        claim,
        pledge_id: contract.history_custody.commitment_v1()?,
        accepted_daa: contract.claim.terms.accepted_daa,
        material_until_daa: contract.claim.terms.material_until_daa,
    }))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedHistoryRecoveryProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub bundle_id: Hash64,
    pub terms: crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
    pub initial: crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    pub response_span_daa: u64,
    pub maximum_custodians: u64,
    pub session: Option<PalwSharedCourtV1<crate::palw_history_custody_v1::PalwHistoryRecoveryV1>>,
}
/// Authenticated native snapshot only. No prospective custody set or protected
/// buffer is cloned. A missing session is not an admitted response obligation.
pub fn palw_staged_history_recovery_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
    seed: Hash64,
) -> Result<Option<PalwStagedHistoryRecoveryProgressV1>, String> {
    let point = state.last_point().ok_or("history recovery has no accepted snapshot")?;
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    if point.daa_score < bundle.plan.terms.accepted_daa || point.daa_score >= bundle.plan.terms.material_until_daa {
        return Ok(None);
    }
    let contract = state.staged_bundle_contract_v1(key.0).ok_or("history owner lost original signed custody contract")?;
    let Some(PalwStagedCourtRowV1::PublicPrefix(prefix)) = bundle.seed_row_v1(seed) else { return Ok(None) };
    let session = bundle.history_recovery.get(&seed);
    let initial = match session {
        Some(row) => row.initial.clone(),
        None => {
            let (sequence, head) = prefix.history_cursor_v1();
            if sequence == 0 {
                return Ok(None);
            }
            if sequence > contract.history_custody.maximum_records {
                return Err("history cursor exceeds original signed resource capacity".into());
            }
            crate::palw_prefix_history_v1::PalwPrefixHistoryRequestV1 {
                claim: key.0,
                owner: key.1,
                reserved_daa: bundle.reserved_daa,
                seed_id: seed,
                head,
                sequence,
                material_until_daa: prefix.terms.material_until_daa,
            }
        }
    };
    if (initial.claim, initial.owner, initial.reserved_daa, initial.seed_id, initial.material_until_daa)
        != (key.0, key.1, bundle.reserved_daa, seed, prefix.terms.material_until_daa)
    {
        return Err("history snapshot substitutes the original paid literal stage".into());
    }
    Ok(Some(PalwStagedHistoryRecoveryProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        bundle_id: bundle.plan.id_v1(bundle.claim),
        terms: bundle.plan.terms,
        initial,
        response_span_daa: contract.config.acquisition_daa,
        maximum_custodians: contract.history_custody.maximum_custodians,
        session: session.cloned(),
    }))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedFlatProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub session: PalwSharedCourtV1<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1>,
    pub context: PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>,
}
pub fn palw_staged_flat_source_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<PalwStagedFlatProgressV1>, String> {
    let point = state.last_point().ok_or("flat source has no accepted snapshot")?;
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    if point.daa_score < bundle.plan.terms.accepted_daa || point.daa_score >= bundle.plan.terms.material_until_daa {
        return Ok(None);
    }
    let Some(session) = &bundle.flat else { return Ok(None) };
    let context = bundle.flat_context.as_ref().ok_or("flat source lost original authenticated context")?;
    Ok(Some(PalwStagedFlatProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        session: session.clone(),
        context: context.clone(),
    }))
}
pub fn palw_staged_flat_pin_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
    source: Hash64,
    answerer: PalwBondKeyV2,
) -> Result<Option<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1>, String> {
    let Some(progress) = palw_staged_flat_source_v1(state, key)? else { return Ok(None) };
    if progress.session.source_id != source {
        return Err("flat pin lookup substitutes accepted original source".into());
    }
    progress.session.authenticated_pin_v1(&answerer).map(Some)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedFloatProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub session: PalwSharedCourtV1<crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1>,
    pub context: PalwSharedCourtV1<crate::palw_staged_bundle_v1::PalwSelectedSourceContextV1>,
}
/// The accepted greedy Float32 source of one owner frame (behind `palw_staged_float_constraint_v1`).
pub fn palw_staged_float_source_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<PalwStagedFloatProgressV1>, String> {
    let point = state.last_point().ok_or("Float32 source has no accepted snapshot")?;
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    if point.daa_score < bundle.plan.terms.accepted_daa || point.daa_score >= bundle.plan.terms.material_until_daa {
        return Ok(None);
    }
    let Some(session) = &bundle.float else { return Ok(None) };
    let context = bundle.float_context.as_ref().ok_or("Float32 source lost original authenticated context")?;
    Ok(Some(PalwStagedFloatProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        session: session.clone(),
        context: context.clone(),
    }))
}
pub fn palw_staged_float_pin_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
    source: Hash64,
    answerer: PalwBondKeyV2,
) -> Result<Option<crate::palw_float_constraint_source_v1::PalwFloatConstraintPinV1>, String> {
    let Some(progress) = palw_staged_float_source_v1(state, key)? else { return Ok(None) };
    if progress.session.source_id != source {
        return Err("Float32 pin lookup substitutes accepted original source".into());
    }
    progress.session.authenticated_pin_v1(&answerer).map(Some)
}
pub fn palw_staged_public_context_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<PalwStagedPublicContextV1>, String> {
    let point = state.last_point().ok_or("public context has no accepted snapshot")?;
    let Some(contract) = state.staged_bundle_contract_v1(claim) else { return Ok(None) };
    if point.daa_score < contract.claim.terms.accepted_daa || point.daa_score >= contract.claim.terms.material_until_daa {
        return Ok(None);
    }
    let original = state.staged_original_job_v1(claim)?.ok_or("public context lost accepted original FP job")?;
    Ok(Some(PalwStagedPublicContextV1 { binding: contract.original_binding.clone(), job: original.job }))
}
/// Hydrate only the original native-admitted public header, within its paid material horizon.
/// This accessor supplies no new admission, owner, reservation or prosecution clock.
pub fn palw_staged_resolved_public_context_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<(crate::palw_step_leg::PalwStepBindingV2, crate::palw_freeprompt_v3::PalwFreePromptJobV3)>, String> {
    let Some(original) = palw_staged_public_context_v1(state, claim)? else { return Ok(None) };
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(header) = original.binding else {
        return Ok(None);
    };
    let profile =
        state.fp_work_profile_of(&original.job.class_id).ok_or("original public context lacks authenticated registered profile")?;
    let binding = header.resolve_v1(profile, header.committed_execution_root, u64::MAX)?;
    Ok(Some((binding, original.job)))
}
pub fn palw_staged_original_commitment_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>, String> {
    if palw_staged_public_context_v1(state, claim)?.is_none() {
        return Ok(None);
    }
    state.staged_original_job_v1(claim)
}
/// The caller supplies an authenticated accepted state. The session is prospective
/// until its first native-checked demand; it does not authorize a new owner frame.
pub fn palw_staged_selected_events_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, crate::palw_state_v2::PalwBondKeyV2),
) -> Result<Option<PalwStagedSelectedProgressV1>, String> {
    let point = state.last_point().ok_or("selected source snapshot has no accepted clock")?;
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    if point.daa_score < bundle.plan.terms.accepted_daa || point.daa_score >= bundle.plan.terms.material_until_daa {
        return Ok(None);
    }
    let session = match &bundle.selected {
        Some(session) => session.clone(),
        None => {
            let next = bundle.active.map_or(0, |n| n as usize + 1);
            let Some(stage) =
                bundle.plan.stages.get(next).filter(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint)
            else {
                return Ok(None);
            };
            let target =
                crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &key.0).ok_or("selected source target absent")?;
            let contract = state.staged_claim_contract_v1(&key.0).ok_or("selected source contract absent")?;
            let bundle_contract = state.staged_bundle_contract_v1(key.0).ok_or("selected source required-family contract absent")?;
            // A TIR claim's scheme is its registered record's; a legacy claim's its shape profile's.
            let tiled = match &bundle_contract.original_binding {
                crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(_) => {
                    state.tir_class_v1(&target.class_id).ok_or("selected TIR source has no registered class record")?.facts.tiled
                }
                _ => {
                    let profile = state.fp_work_profile_of(&target.class_id).ok_or("selected source has no accepted profile")?;
                    profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1()
                        && profile.lane == crate::palw_step::PalwStepLaneV1::Int32
                }
            };
            if !tiled {
                return Ok(None);
            }
            let respond = bundle
                .plan
                .preseed_source_until_v1(crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint, bundle_contract.config)?;
            let custody = contract
                .prefix_custody
                .as_ref()
                .ok_or("selected source has no original signed material contract")?
                .current_signed_v1();
            if !custody.public_qualified_v1() {
                return Err("selected source original custody is unqualified".into());
            }
            let custodians: std::collections::BTreeSet<_> = custody.expected_panel.into_iter().chain([contract.producer]).collect();
            crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1::new_with_span_v1(
                key.0,
                key.1,
                bundle.reserved_daa,
                bundle.plan.id_v1(key.0),
                respond,
                stage.terms.collect_until_daa,
                crate::palw_selected_event_source_v1::selected_event_response_span_v1(
                    custodians.len() as u64,
                    bundle_contract.config,
                )?,
                custodians,
            )?
        }
    };
    Ok(Some(PalwStagedSelectedProgressV1 {
        snapshot_block: point.block,
        now_daa: point.daa_score,
        session,
        context: bundle.selected_context.clone(),
        native: bundle.selected_native.clone(),
    }))
}

/// **A TIR claim's native decode view** (behind `palw_staged_tir_route_v1`): the admitted public
/// header from the claim's native required-family contract (never a carrier) and the registered
/// class record. `None` for a legacy claim.
pub fn palw_staged_tir_view_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
    maximum_leaves: u64,
) -> Result<
    Option<(crate::palw_decode_view_v1::PalwDecodeViewV1, Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>)>,
    String,
> {
    let Some(contract) = state.staged_bundle_contract_v1(claim) else { return Ok(None) };
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header) = &contract.original_binding else {
        return Ok(None);
    };
    let record =
        state.tir_class_v1(&header.job_context.shape_profile_id).ok_or("TIR claim's registered class record is absent")?;
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, maximum_leaves)?;
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &claim).ok_or("TIR view claim target absent")?;
    if view.committed_execution_root != target.execution_root || view.class_id != target.class_id {
        return Err("TIR view does not reproduce the claim's accepted execution root or class".into());
    }
    Ok(Some((view, Box::new(header.clone()))))
}

/// A registered TIR class's published layout (behind `palw_staged_tir_route_v1`).
pub fn palw_staged_tir_layout_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    class_id: Hash64,
) -> Option<&crate::palw_tir_class_v1::PalwTirLayoutV1> {
    match state.staged_court_row_v1(&PalwStagedCourtKeyV1::TirLayout(class_id)) {
        Some(PalwStagedCourtRowV1::TirLayout(layout)) => Some(layout.as_ref()),
        _ => None,
    }
}

/// **The TIR prefix-state context**, rebuilt natively for one claim (behind
/// `palw_staged_tir_route_v1`): the class's step space from its registered program and published
/// layout (the layout authenticated by the record's digest and class id), the admitted header's
/// job context, and an unstarted stream over the job's declared prefix state. `None` for a claim
/// that is not a TIR prefix-state claim.
pub struct PalwTirPrefixContextV1 {
    pub space: crate::palw_tir_step_v1::PalwTirStepSpaceV1,
    pub job_context: crate::palw_v2::PalwJobContextV2,
    pub template: crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1,
    /// The largest boundary leaf's lanes, in bytes; and the step tree's opening depth.
    pub leaf_bytes: u64,
    pub depth: u64,
}
pub fn palw_tir_prefix_context_v1(
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
    maximum_leaves: u64,
) -> Result<PalwTirPrefixContextV1, String> {
    let binding = header.resolve_registered_layout_v1(record, layout, header.committed_execution_root, maximum_leaves)?;
    let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&binding.class).map_err(|e| e.to_string())?;
    if space.leaf_count_capped(&header.job_context, maximum_leaves).map_err(|e| e.to_string())? != header.step_leaf_count {
        return Err("TIR header's step leaf count is not its class's leaf space".into());
    }
    let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
    if named.class_id != record.facts.class_id {
        return Err("TIR prefix state names another class".into());
    }
    let template = crate::palw_tir_prefix_state_v1::PalwTirPrefixStateStreamV1::new_v1(
        &space,
        record.facts.class_id,
        &header.job_context,
        header.step_merkle_root,
        header.step_leaf_count,
        maximum_leaves,
        named.prefix_tokens,
        named.state_root,
    )?;
    let lanes = space
        .fixed_instances()
        .iter()
        .map(|s| u64::from(s.tile_lanes))
        .chain(space.hist_instances().iter().map(|s| u64::from(s.tile_lanes) * u64::from(space.layout.h_tile)))
        .max()
        .ok_or("TIR class declares no state to prefix")?;
    let leaf_bytes = lanes.checked_mul(4).ok_or("TIR prefix leaf bytes overflow")?;
    if leaf_bytes > crate::palw_tir_prefix_owned_v1::PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1 as u64 {
        return Err("a TIR prefix boundary leaf exceeds one paid staged carrier".into());
    }
    let depth = u64::from(64 - header.step_leaf_count.leading_zeros());
    Ok(PalwTirPrefixContextV1 { space, job_context: header.job_context.clone(), template, leaf_bytes, depth })
}

/// The accepted TIR prefix-state stage of one owner frame, for node readers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedTirPrefixProgressV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub row: PalwSharedCourtV1<crate::palw_tir_prefix_owned_v1::PalwOwnedTirPrefixCourtV1>,
    pub header: Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
}
pub fn palw_staged_tir_prefix_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<PalwStagedTirPrefixProgressV1>, String> {
    let point = state.last_point().ok_or("TIR prefix snapshot has no accepted clock")?;
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    let Some(PalwStagedCourtRowV1::TirPrefix(row)) = bundle.active_v1() else { return Ok(None) };
    let header = bundle.active_native_seed_v1().ok_or("TIR prefix stage lost its native context")?.header.clone();
    Ok(Some(PalwStagedTirPrefixProgressV1 { snapshot_block: point.block, now_daa: point.daa_score, row: row.clone(), header }))
}

/// The step-tree index of boundary leaf `ordinal` of an owner frame's active TIR prefix stage
/// (node responders turn a slot's cursor into the leaf they disclose).
pub fn palw_staged_tir_prefix_leaf_index_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
    ordinal: u64,
    maximum_leaves: u64,
) -> Result<Option<u64>, String> {
    let Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) = state.staged_owned_row_v1(key.0, key.1) else { return Ok(None) };
    let Some(native) = bundle.active_native_seed_v1() else { return Ok(None) };
    let Some(PalwStagedCourtRowV1::TirPrefix(_)) = bundle.active_v1() else { return Ok(None) };
    let class = native.header.job_context.shape_profile_id;
    let record = state.tir_class_v1(&class).ok_or("TIR prefix class record absent")?;
    let layout = palw_staged_tir_layout_v1(state, class).ok_or("TIR prefix class layout absent")?;
    let context = palw_tir_prefix_context_v1(record, layout, &native.header, &native.job, maximum_leaves)?;
    let crate::palw_constraint_stream_carriage_v1::PalwNativeSeedFamilyV1::PrefixState { prefix_tokens } = native.seed.family else {
        return Ok(None);
    };
    Ok(context.space.prefix_state_leaf_v1(&context.job_context, prefix_tokens, ordinal).map(|leaf| leaf.index))
}

/// The active native TIR stage's seed and admitted header (node readers; `None` for a legacy stage).
pub fn palw_staged_owned_native_seed_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: (Hash64, PalwBondKeyV2),
) -> Result<Option<crate::palw_staged_bundle_v1::PalwStagedNativeSeedContextV1>, String> {
    let Some(paid) = palw_staged_paid_authority_v1(state, key)? else { return Ok(None) };
    let point = state.last_point().ok_or("paid native seed has no accepted clock")?;
    if point.daa_score < paid.terms.accepted_daa || point.daa_score >= paid.terms.material_until_daa {
        return Ok(None);
    }
    Ok(match state.staged_owned_row_v1(key.0, key.1) {
        Some(PalwStagedCourtRowV1::OwnerBundle(bundle)) => bundle.active_native_seed_v1().cloned(),
        _ => None,
    })
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
            PalwStagedCourtRowV1::TirPrefix(row) => {
                if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
                    return Err("TIR prefix certificate changes immutable owner/seed/reservation".into());
                }
                let fault = row.certificate()?.ok_or("TIR prefix court re-derived the declared state root")?;
                if (fault.claim, fault.reserver, fault.reserved_daa, fault.seed_id, fault.execution_root)
                    != (self.claim, self.reserver, self.reserved_daa, self.seed_id, expected_root)
                {
                    return Err("TIR prefix certificate does not convict target claim".into());
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
    /// A TIR prefix-state stage (behind `palw_staged_tir_route_v1`): the boundary's leaf count,
    /// whether the custody clock was collected, and Some(false) honest-complete / Some(true) a
    /// re-derived root that differs from the declared one.
    TirPrefix {
        total: u64,
        collected: bool,
        completed: Option<bool>,
    },
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
        PalwStagedCourtRowV1::TirPrefix(c) => (c.reserved_daa, c.seed_id(), c.terms, c.execution_root_v1()),
        PalwStagedCourtRowV1::OwnerBundle(c) => {
            let seed = c.active_seed_id_v1().unwrap_or_else(|| c.plan.id_v1(c.claim));
            let terms = match c.active_v1() {
                Some(PalwStagedCourtRowV1::Constraint(row)) => row.terms,
                Some(PalwStagedCourtRowV1::Tokenizer(row)) => row.terms,
                Some(PalwStagedCourtRowV1::Prefix(row)) => row.terms,
                Some(PalwStagedCourtRowV1::PublicPrefix(row)) => row.terms,
                Some(PalwStagedCourtRowV1::TirPrefix(row)) => row.terms,
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
        Some(PalwStagedCourtRowV1::TirPrefix(row)) => {
            row.check()?;
            if row.reserved_daa != reserved_daa {
                return Err("TIR prefix progress reservation differs from owned seed".into());
            }
            let completed = match row.verdict_v1() {
                Some(crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1::StateRootMismatch { .. }) => Some(true),
                Some(crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1::Honest) if row.collected_v1() => Some(false),
                _ => None,
            };
            (
                Some(row.seed_id()),
                Some(PalwStagedOwnedCursorV1::TirPrefix { total: row.total_v1(), collected: row.collected_v1(), completed }),
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
            Some(PalwStagedCourtRowV1::TirPrefix(row)) => row.terms,
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
            _ => match state.staged_bundle_contract_v1(key.0) {
                Some(bundle) => Some(crate::palw_staged_bundle_v1::PalwStagedBundleProgressV1 {
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

/// Restore only native-admitted original context. Legacy single rows without that
/// public context must not invent a binding from partial court fields.
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
            if let Some(context) = &bundle.selected_context {
                if bundle.active.is_none()
                    || bundle.active.is_some_and(|n| {
                        bundle
                            .plan
                            .stages
                            .get(n as usize + 1)
                            .is_some_and(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint)
                    })
                {
                    return Ok(Some((context.binding.as_ref().clone(), context.job.as_ref().clone())));
                }
            }
            if let Some(context) = bundle.flat_context.as_ref().or(bundle.float_context.as_ref()) {
                if bundle.active.is_none()
                    || bundle.active.is_some_and(|n| {
                        bundle
                            .plan
                            .stages
                            .get(n as usize + 1)
                            .is_some_and(|s| s.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint)
                    })
                {
                    return Ok(Some((context.binding.as_ref().clone(), context.job.as_ref().clone())));
                }
            }
            let Some(stage) = bundle.active.and_then(|index| bundle.opened.get(index as usize)) else {
                let contract = state.staged_bundle_contract_v1(key.0).ok_or("paid bundle original claim context absent")?;
                let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(header) = &contract.original_binding
                else {
                    return Ok(None);
                };
                let original = state.staged_original_job_v1(key.0)?.ok_or("paid bundle original public job absent")?;
                let profile = state
                    .fp_work_profile_of(&original.job.class_id)
                    .ok_or("paid original context lacks authenticated class profile")?;
                // This is hydration of already admitted public material, not a fresh seed gate.
                let binding = header.resolve_v1(profile, original.execution_root, u64::MAX)?;
                return Ok(Some((binding, original.job)));
            };
            // A native TIR stage has no legacy binding to hydrate (`palw_staged_native_context_v1`).
            let Some(stage) = stage.context.legacy_v1() else { return Ok(None) };
            return Ok(Some((stage.binding.as_ref().clone(), stage.job.as_ref().clone())));
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
