//! Private RFC-0030 native archive construction. No public admission/funding
//! coordinator or settlement path is installed by this adapter.
use super::*;
use crate::palw_court_end_archive_v1::{CourtEndEvidenceV1 as Evidence, PendingCourtArchiveV1 as Archive};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key};
impl TransitionBuilder<'_> {
    pub(super) fn prepare_fulfilled_court_group_v1(&mut self,id:Hash64,side:PalwCourtSideV1,group:&PalwCourtCloseGroupV2)->Result<(),PalwStateV2Error>{
        if self.native_writers_tracking_v1(){
            let mut q=native_writers::NativeWriteQuoteV1::new("fulfilled court group retained before removal");
            q.copy(group)?.exact::<PalwCourtCloseGroupV2>(2)?.exact::<usize>(2)?;
            self.admit_native_write_v1(q)?;
        }
        self.court_fulfilled_group=Some((id,side,Box::new(group.clone()).into()));Ok(())
    }
    pub(super) fn prepare_native_court_end_cause_v1(&mut self, ctx: &PalwBlockContextV2, object: &PalwConsensusObjectV2) -> Result<(), PalwStateV2Error> {
        if self.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("court end native evidence");
        q.copy(object)?.exact::<PalwConsensusObjectV2>(1)?.exact::<Evidence>(2)?.exact::<usize>(2)?;
        self.admit_native_write_v1(q)?;
        }
        self.court_end_evidence = Some(Box::new(Evidence::Native { point:*ctx, object:Box::new(object.clone()) }).into());
        Ok(())
    }
    pub(super) fn prepare_tir_court_end_cause_v1(&mut self, ctx: &PalwBlockContextV2, claim: Hash64, filer: PalwBondKeyV2,
        evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1) -> Result<(), PalwStateV2Error> {
        if self.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("court end complete TIR evidence");
        q.copy(evidence)?.exact::<crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1>(1)?
            .exact::<Evidence>(2)?.exact::<usize>(2)?;
        self.admit_native_write_v1(q)?;
        }
        self.court_end_evidence = Some(Box::new(Evidence::TirConviction { point:*ctx, claim, filer, evidence:Box::new(evidence.clone()) }).into());
        Ok(())
    }
    pub(super) fn build_pending_court_archive_v1(&mut self, id: Hash64) -> Result<Archive, PalwStateV2Error> {
        if self.state.staged_court_row_v1(&Key::CourtEndArchive(id)).is_some() { return Err(PalwStateV2Error::DuplicateSession(id)); }
        let session = self.state.court_sessions.get(&id).ok_or(PalwStateV2Error::MissingSession(id))?;
        let service = self.state.court_service_state_v1().ok_or_else(|| PalwStateV2Error::CarriageInconsistent("archive service missing".into()))?;
        let binding = service.claims.get(&session.claim).ok_or_else(|| PalwStateV2Error::CarriageInconsistent("archive binding missing".into()))?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&self.state, session.claim)
            .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("archive original frame missing".into()))?;
        let material = crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame,binding.hard_deadline_daa)
            .map_err(PalwStateV2Error::CarriageInconsistent)?;
        let stake=self.state.claim(&session.claim).ok_or(PalwStateV2Error::MissingClaim(session.claim))?.reserved;
        let metadata = (service.profile_id, binding.allocation_id, binding.source_id, frame.custody_v1().pledge_v1().producer, material,stake);
        if self.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("court archive before source clones");
        q.copy(session)?.exact::<Archive>(2)?.exact::<usize>(2)?;
        if self.court_end_evidence.is_none() {q.exact::<Evidence>(2)?.exact::<usize>(2)?;}
        if let Some(p) = self.state.tir_dissections.get(&id) {q.copy(p)?;}
        for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {
            if let Some(g) = self.state.court_close_groups.get(&(id,side)) {
                q.copy(g)?.member::<PalwCourtSideV1,PalwCourtCloseGroupV2>(1)?;
            }
        }
        if self.court_settled_deposit.is_some_and(|(sid,_,_)|sid==id) {q.member::<PalwCourtSideV1,u64>(1)?;}
        if let Some((sid,_,g))=&self.court_fulfilled_group {if *sid==id {
            q.copy(g.as_ref())?.member::<PalwCourtSideV1,PalwCourtCloseGroupV2>(1)?.member::<PalwCourtSideV1,()>(1)?;
        }}
        self.admit_native_write_v1(q)?;
        }
        let session = self.state.court_sessions.get(&id).unwrap().clone();
        let mut groups: BTreeMap<_,_> = [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger].into_iter()
            .filter_map(|side|self.state.court_close_groups.get(&(id,side)).map(|g|(side,g.clone()))).collect();
        let mut fulfilled_groups=std::collections::BTreeSet::new();
        if let Some((sid,side,group))=&self.court_fulfilled_group {if *sid==id {
            if groups.insert(*side,group.as_ref().clone()).is_some(){return Err(PalwStateV2Error::CarriageInconsistent("fulfilled group was not removed from active state".into()));}
            fulfilled_groups.insert(*side);
        }}
        let applied_deposit_debits = self.court_settled_deposit.filter(|(sid,_,_)|*sid==id)
            .map(|(_,side,debit)|BTreeMap::from([(side,debit)])).unwrap_or_default();
        let archive = Archive {version:1,session_id:id,service_profile_id:metadata.0,
            archive_profile_id:crate::palw_court_end_archive_v1::court_end_archive_profile_id_v1(metadata.0),allocation_id:metadata.1,source_id:metadata.2,
            producer:metadata.3,material_until_daa:metadata.4,court_stake_nominal:metadata.5,session,tir_phase:self.state.tir_dissections.get(&id).cloned(),
            groups,applied_deposit_debits,fulfilled_groups,evidence:self.court_end_evidence.clone().unwrap_or_else(||Box::new(Evidence::Uncaptured).into())};
        archive.check(&Key::CourtEndArchive(id)).map_err(PalwStateV2Error::CarriageInconsistent)?;
        Ok(archive)
    }
}
