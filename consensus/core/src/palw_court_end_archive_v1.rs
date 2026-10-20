//! RFC-0030 isolated pending-liability archive. An archive is not escrow,
//! settlement, default permission, credit or a funded original admission.
use crate::{Hash64, palw_state_v2::*, palw_staged_court_state_v1::{PalwSharedCourtV1, PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row}};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap,BTreeSet};

/// Accepted native material, not a caller-carried verdict/permission flag.
/// Uncaptured preserves unresolved material on still-unadapted internal paths;
/// it explicitly does not claim that complete termination evidence was saved.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CourtEndEvidenceV1 {
    Uncaptured,
    Native { point: PalwBlockContextV2, object: Box<PalwConsensusObjectV2> },
    TirConviction { point: PalwBlockContextV2, claim: Hash64, filer: PalwBondKeyV2,
        evidence: Box<crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1> },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingCourtArchiveV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub service_profile_id: Hash64,
    pub archive_profile_id: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub material_until_daa: u64,
    pub session: PalwCourtSessionStateV2,
    pub court_stake_nominal: u128,
    pub tir_phase: Option<Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>,
    pub groups: BTreeMap<PalwCourtSideV1, PalwCourtCloseGroupV2>,
    /// Actual native debits already made for objectively malformed COMPLETE
    /// assemblies. These sides are not another pending charge.
    pub applied_deposit_debits: BTreeMap<PalwCourtSideV1, u64>,
    pub fulfilled_groups: BTreeSet<PalwCourtSideV1>,
    pub evidence: PalwSharedCourtV1<CourtEndEvidenceV1>,
}
/// Read-only description, never an authentication or release capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CourtEndDispositionV1 {
    Uncaptured, NativeCourtVerdict(PalwCourtVerdictV2), TirClaimFault,
    CompletedAssemblyFault(PalwCourtSideV1), RelatedNativeTransition,
}
impl PendingCourtArchiveV1 {
    pub fn adjudication(&self)->CourtEndDispositionV1 {
        if let Some(side)=self.applied_deposit_debits.keys().next(){return CourtEndDispositionV1::CompletedAssemblyFault(*side);}
        match self.evidence.as_ref(){
            CourtEndEvidenceV1::Uncaptured=>CourtEndDispositionV1::Uncaptured,
            CourtEndEvidenceV1::Native {object,..}=>match object.as_ref(){
                PalwConsensusObjectV2::CourtClosed {session_id,verdict,..} if *session_id==self.session_id=>CourtEndDispositionV1::NativeCourtVerdict(*verdict),
                _=>CourtEndDispositionV1::RelatedNativeTransition,
            },
            CourtEndEvidenceV1::TirConviction {claim,..} if *claim==self.session.claim=>CourtEndDispositionV1::TirClaimFault,
            _=>CourtEndDispositionV1::RelatedNativeTransition,
        }
    }

    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtEndArchive(self.session_id)
            || self.session.ladder.session_id() != self.session_id
            || self.session.deadline_daa <= self.session.opened_daa
            || self.session.dissection.as_ref().is_some_and(|p| p.session_id() != self.session_id)
            || self.tir_phase.as_ref().is_some_and(|p| p.session_id() != self.session_id)
            || self.archive_profile_id != court_end_archive_profile_id_v1(self.service_profile_id)
            || self.groups.len() > 2 || self.applied_deposit_debits.len() > self.groups.len()
            || self.fulfilled_groups.len() > self.groups.len()
        { return Err("court archive scope/session/phase is inconsistent".into()); }
        for (side, group) in &self.groups {
            let declarer = match side { PalwCourtSideV1::Executor => self.producer, PalwCourtSideV1::Challenger => self.session.challenger_bond };
            if group.declarer != declarer || group.deposit_outpoint != declarer.0
                || group.count == 0 || group.count > PALW_COURT_CLOSE_MAX_CHUNKS
                || group.chunk_digests.len() != group.count as usize
                || group.assembly_deadline_daa > self.session.deadline_daa
                || group.declared_daa < self.session.opened_daa
                || group.declared_daa.checked_add(palw_close_assembly_daa_v1(group.count)) != Some(group.assembly_deadline_daa)
                || group.present & !PalwCourtCloseGroupV2::full_mask(group.count) != 0
                || group.chunks.len() != group.present.count_ones() as usize
            { return Err("court archive changes an accepted group contract".into()); }
            for (i, bytes) in &group.chunks {
                if *i >= group.count || !group.has(*i) || bytes.is_empty() || bytes.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES
                    || palw_court_close_chunk_digest_v1(bytes) != group.chunk_digests[*i as usize]
                { return Err("court archive chunk/bitmap/digest mismatch".into()); }
            }
        }
        for (side, debit) in &self.applied_deposit_debits {
            if self.groups.get(side).is_none_or(|g| !g.is_complete() || *debit > g.deposit) {
                return Err("court archive settled debit has no complete bound group".into());
            }
        }
        for side in &self.fulfilled_groups {
            let group = self.groups.get(side).ok_or("fulfilled archive group missing")?;
            if !group.is_complete() || self.applied_deposit_debits.contains_key(side) {
                return Err("fulfilled group cannot also carry an applied penalty".into());
            }
            let CourtEndEvidenceV1::Native {object,..} = self.evidence.as_ref() else {
                return Err("fulfilled group lacks its complete native close".into());
            };
            // Raw accepted chunks are authoritative material. Do not require
            // re-encoding to the same bytes: legacy Borsh may admit an equivalent
            // non-canonical representation. This context check is not another
            // verdict adjudicator; the native verifier/reader consumes the raw
            // completed object before using it as evidence.
            if !matches!(object.as_ref(),PalwConsensusObjectV2::CourtClosed {session_id,verdict,..}
                if *session_id==self.session_id && *verdict==group.verdict)
                || palw_court_close_group_digest_v1(group).map_err(|e|e.to_string())? != group.close_digest
            { return Err("fulfilled group does not assemble to its retained native close".into()); }
        }
        let point = match self.evidence.as_ref() {
            CourtEndEvidenceV1::Uncaptured => None,
            CourtEndEvidenceV1::Native { point, .. } => Some(point),
            CourtEndEvidenceV1::TirConviction { point, .. } => Some(point),
        };
        if point.is_some_and(|p| p.daa_score < self.session.opened_daa) {
            return Err("court archive cause predates the active session".into());
        }
        Ok(())
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtEndArchive(self.session_id))?;
        if state.court_session(&self.session_id).is_some() || state.tir_dissection_v1(&self.session_id).is_some()
            || state.court_close_groups_iter().any(|((id, _), _)| *id == self.session_id)
        { return Err("archived court still has active rows".into()); }
        let service = state.court_service_state_v1().ok_or("court archive service book missing")?;
        let binding = service.claims.get(&self.session.claim).ok_or("court archive original binding missing")?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.session.claim)
            .ok_or("court archive original frame missing")?;
        let until = crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame, binding.hard_deadline_daa)?;
        if self.service_profile_id != service.profile_id || self.allocation_id != binding.allocation_id
            || self.source_id != binding.source_id || self.producer != frame.custody_v1().pledge_v1().producer
            || self.material_until_daa != until || state.claim(&self.session.claim).is_none()
        { return Err("court archive substitutes original scope/material".into()); }
        for bond in [self.producer,self.session.challenger_bond] {
            if self.pending_nominal(&bond) != 0 && state.bond(&bond).is_none() {
                return Err("court archive lost a pending financial actor".into());
            }
        }
        Ok(())
    }
    /// Snapshot time is checked separately: an in-flight native writer still
    /// has its parent's last_point until the whole block commits.
    pub fn check_loaded_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check_context(state)?;
        let point = match self.evidence.as_ref() {
            CourtEndEvidenceV1::Uncaptured => None,
            CourtEndEvidenceV1::Native {point,..} | CourtEndEvidenceV1::TirConviction {point,..} => Some(point),
        };
        if point.is_some_and(|p| state.last_point().is_none_or(|tip| p.daa_score > tip.daa_score)) {
            return Err("court archive receipt is beyond the loaded accepted clock".into());
        }
        Ok(())
    }
    /// Nominal outstanding charge, NOT a posted balance or a guaranteed debit.
    pub fn pending_nominal(&self, bond: &PalwBondKeyV2) -> u128 {
        let groups: u128 = self.groups.iter().filter(|(side, g)| g.declarer == *bond
            && !self.applied_deposit_debits.contains_key(side) && !self.fulfilled_groups.contains(side))
            .map(|(_, g)| u128::from(g.deposit)).sum();
        // An unadapted or merely related ending cannot silently free the unresolved
        // court stake. This is a nominal hold, not new debt or deposited money.
        if *bond == self.session.challenger_bond && matches!(self.adjudication(),CourtEndDispositionV1::Uncaptured | CourtEndDispositionV1::RelatedNativeTransition) {
            groups.saturating_add(self.court_stake_nominal)
        } else {groups}
    }
}
impl PalwChainStateV2 {
    pub fn court_end_archive_v1(&self, id: Hash64) -> Option<&PendingCourtArchiveV1> {
        match self.staged_court_row_v1(&Key::CourtEndArchive(id)) { Some(Row::CourtEndArchive(a)) => Some(a.as_ref()), _ => None }
    }
    /// Conservative financial exposure, not an escrow or a supply certificate.
    pub fn court_archive_pending_exposure_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        self.staged_cache_lease_rows_v1().filter_map(|(_,r)| match r { Row::CourtEndArchive(a) => Some(a.pending_nominal(bond)), _ => None })
            .fold(0u128, u128::saturating_add)
    }
    pub fn court_archive_holds_bond_v1(&self, bond: &PalwBondKeyV2) -> bool {
        self.staged_cache_lease_rows_v1().any(|(_,r)| matches!(r, Row::CourtEndArchive(a) if a.pending_nominal(bond) != 0))
    }
}

/// Candidate namespace identity only; not a funded fee/capacity profile.
pub fn court_end_archive_profile_id_v1(service:Hash64)->Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/court-end-archive/v1")
        .hash(service.as_byte_slice()).as_bytes().try_into().expect("64-byte archive namespace"))
}
