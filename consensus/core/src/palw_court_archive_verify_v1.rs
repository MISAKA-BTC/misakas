//! RFC-0030 private archive verification: native v5 replay of the declared
//! bytes under retained native inputs. It neither changes chain state nor
//! releases money, grants credit, manufactures supply, or authorizes default.
use super::*;
use crate::palw_native_resource_v1::{NativeWireReaderV1,NativeWireScanV1};
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};

/// Only this verifier constructs the result. No Borsh/Clone/from-bool path.
/// A later financial transition must also check the bound state root and its
/// own funded contract/once-only effects; this result alone settles nothing.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct VerifiedArchivedCloseV1 {
    state_root:Hash64,
    session_id:Hash64,
    side:PalwCourtSideV1,
    verdict:PalwCourtVerdictV2,
    group:Box<PalwCourtCloseGroupV2>,
    close:Box<PalwConsensusObjectV2>,
}
#[allow(dead_code)]
impl VerifiedArchivedCloseV1 {
    pub(super) fn verdict(&self)->PalwCourtVerdictV2{self.verdict}
    pub(super) fn session_id(&self)->Hash64{self.session_id}
    pub(super) fn side(&self)->PalwCourtSideV1{self.side}
    pub(super) fn matches_state(&self,state:&PalwChainStateV2)->bool{self.state_root==state.state_root()}
    pub(super) fn group(&self)->&PalwCourtCloseGroupV2{&self.group}
    pub(super) fn close(&self)->&PalwConsensusObjectV2{&self.close}
}
fn same_contract(a:&PalwCourtCloseGroupV2,b:&PalwCourtCloseGroupV2)->bool {
    a.declarer==b.declarer && a.count==b.count && a.chunk_digests==b.chunk_digests
        && a.close_digest==b.close_digest && a.verdict==b.verdict && a.declared_daa==b.declared_daa
        && a.assembly_deadline_daa==b.assembly_deadline_daa && a.deposit_outpoint==b.deposit_outpoint && a.deposit==b.deposit
}
fn refusal(message:impl Into<String>)->PalwStateV2Error {PalwStateV2Error::CarriageInconsistent(message.into())}
/// Local construction limits must never turn a valid proof into misconduct.
/// Syntax/truncation remains a semantic refusal; budget/OOM is retryable local
/// NativeResource, including the wire reader's actual CPU/arithmetic limits.
pub(super) fn inspection_io_error(stage:&str,error:std::io::Error)->PalwStateV2Error {
    if matches!(error.kind(),std::io::ErrorKind::TimedOut | std::io::ErrorKind::OutOfMemory) {
        native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwConsensusObjectV2>(stage,&error.to_string()))
    }else{refusal(format!("{stage}: {error}"))}
}
impl TransitionBuilder<'_> {
    /// A private verifier, not a public ingestion or paid-admission endpoint.
    /// The candidate can add only the precommitted missing bytes. No clock,
    /// policy, signature, verdict, contract or previously accepted byte changes.
    #[allow(dead_code)]
    pub(super) fn verify_archived_close_v1(&mut self,id:Hash64,side:PalwCourtSideV1,candidate:&PalwCourtCloseGroupV2)
        ->Result<VerifiedArchivedCloseV1,PalwStateV2Error>{
        let archive=self.state.court_end_archive_v1(id).ok_or_else(||refusal("court archive missing"))?;
        archive.check_context(&self.state).map_err(refusal)?;
        let original=archive.groups.get(&side).ok_or_else(||refusal("archived declaration missing"))?;
        if archive.fulfilled_groups.contains(&side) || archive.applied_deposit_debits.contains_key(&side) {
            return Err(refusal("archived declaration already discharged"));
        }
        if !same_contract(original,candidate) || !candidate.is_complete()
            || candidate.chunks.len()!=usize::from(candidate.count)
            || candidate.present!=PalwCourtCloseGroupV2::full_mask(candidate.count)
            || original.chunks.iter().any(|(i,bytes)|candidate.chunks.get(i)!=Some(bytes))
        {return Err(refusal("completed archive group changes its accepted contract/prefix"));}
        let mut total=0u64;
        for i in 0..candidate.count {
            let bytes=candidate.chunks.get(&i).ok_or_else(||refusal("archive completion has a chunk hole"))?;
            if bytes.is_empty() || bytes.len()>PALW_COURT_CLOSE_CHUNK_MAX_BYTES
                || palw_court_close_chunk_digest_v1(bytes)!=candidate.chunk_digests[usize::from(i)]
            {return Err(refusal("archive completion chunk digest/size mismatch"));}
            total=total.checked_add(bytes.len() as u64).ok_or_else(||refusal("archive completion length overflow"))?;
        }
        let receipt=match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)) {
            Some(Row::CourtCloseReceipt(r))=>r.as_ref(),_=>return Err(refusal("original signed native close receipt missing")),
        };
        // Admit scratch, copies and BTree memberships before signature-message
        // construction, assembly, or restoring the local inspection view.
        if self.native_writers_tracking_v1(){
            let mut q=native_writers::NativeWriteQuoteV1::new("archive complete signed v5 replay before buffers");
            q.copy(candidate)?.exact::<PalwCourtCloseGroupV2>(1)?.copy(&receipt.session)?
                .member::<Hash64,PalwCourtSessionStateV2>(1)?
                .exact::<Hash64>(2*u64::from(candidate.count))?.exact::<u8>(256)?.exact::<u8>(total)?;
            if let Some(phase)=&receipt.tir_phase {
                q.copy(phase)?.member::<Hash64,Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>(1)?;
            }
            self.admit_native_write_v1(q)?;
        }
        let receipt=match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)) {Some(Row::CourtCloseReceipt(r))=>r.as_ref(),_=>unreachable!()};
        receipt.check_context(&self.state).map_err(refusal)?;
        let rules=receipt.rules.ok_or_else(||refusal("original native v5 inputs missing; no default fallback"))?;
        crate::palw_court_v2::check_close_declared_chunk_count_v2(&id,candidate.count,&rules.court).map_err(|e|refusal(e.to_string()))?;
        let total=usize::try_from(total).map_err(|_|refusal("archive completion cannot fit native address space"))?;
        let mut assembled=Vec::with_capacity(total);
        for i in 0..candidate.count {assembled.extend_from_slice(&candidate.chunks[&i]);}
        if palw_court_close_chunk_digest_v1(&assembled)!=candidate.close_digest {return Err(refusal("archive completion whole digest mismatch"));}
        let mut reader=NativeWireReaderV1::new(assembled.as_slice(),assembled.len() as u64);
        let shape=PalwConsensusObjectV2::scan_native_wire_v1(&mut reader).map_err(|e|inspection_io_error("archive close wire shape",e))?;
        if reader.remaining()!=0 {return Err(refusal("archive close has trailing material"));}
        if self.native_writers_tracking_v1(){
            let mut q=native_writers::NativeWriteQuoteV1::new("archive v5 proof decode before allocation");
            q.exact::<u8>(shape.heap)?.exact::<PalwConsensusObjectV2>(1)?;
            self.admit_native_write_v1(q)?;
        }
        let close:PalwConsensusObjectV2=borsh::from_slice(&assembled).map_err(|e|inspection_io_error("archive close decode",e))?;
        let PalwConsensusObjectV2::CourtClosed {session_id,verdict,proof}=&close else {return Err(refusal("archive bytes are not a court close"));};
        if *session_id!=id || *verdict!=candidate.verdict {return Err(refusal("archive close changes declared session/verdict"));}
        if (proof.is_tir_v1() && !rules.tir_active) || (proof.is_gen_v1() && !rules.gen_active)
            || (proof.is_eval_v1() && !rules.improvement_active)
            || (proof.is_constrained_decode_v1() && !rules.constraint_active)
            || (crate::palw_task_heads_v1::palw_object_needs_task_heads_v1(&close) && !rules.task_heads_active)
        {return Err(refusal("archive proof family was dormant in its retained native context"));}
        // The checkpoint owner admits the actual complete state copy. This view
        // is local inspection scratch: it is not a valid new chain state and is
        // never journalled, committed, or used to mint credit or settle money.
        let (mut view,_)=self.try_checkpoint()?;
        let receipt=match self.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)) {Some(Row::CourtCloseReceipt(r))=>r.as_ref(),_=>unreachable!()};
        view.court_sessions.insert(id,receipt.session.clone());
        if let Some(phase)=&receipt.tir_phase {view.tir_dissections.insert(id,phase.clone());}
        let prompt=if rules.prompt_ids_merkle {crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1}else{crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat};
        let derived=crate::palw_court_v2::adjudicate_court_close_v5(&view,&id,proof,&rules.court,rules.step_ladder,prompt,
            rules.held_regime,rules.decode_close_convicts_only,rules.fp_decode_from_daa,rules.cell_closure_from_daa,rules.memory_routing_from_daa)
            .map_err(|e|refusal(format!("archived v5 close does not adjudicate: {e}")))?;
        if derived!=*verdict {return Err(refusal("archived v5 result differs from the signed declaration"));}
        Ok(VerifiedArchivedCloseV1 {state_root:self.state.state_root(),session_id:id,side,verdict:derived,
            group:Box::new(candidate.clone()),close:Box::new(close)})
    }
}
