//! RFC-0030 candidate: preserve the signed declaration and native v5 context.
//! These are historical inputs, not settlement/default/credit capabilities.
use crate::{Hash64, palw_state_v2::*, palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key}};
use borsh::{BorshDeserialize,BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCourtRulesV1 {
    pub network_domain: Hash64,
    pub at: PalwBlockContextV2,
    pub court: crate::palw_mode_v2::PalwCourtParamsV2,
    pub step_ladder: u64,
    pub prompt_ids_merkle: bool,
    pub held_regime: bool,
    pub decode_close_convicts_only: bool,
    pub fp_decode_from_daa: Option<u64>,
    pub cell_closure_from_daa: Option<u64>,
    pub memory_routing_from_daa: Option<u64>,
    pub tir_active: bool,
    pub gen_active: bool,
    pub task_heads_active: bool,
    pub improvement_active: bool,
    pub constraint_active: bool,
}
impl NativeCourtRulesV1 {
    pub fn check(&self)->Result<(),String>{
        let c=self.court;
        let rebuilt=crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(c.max_step_leaf_count(),c.turn_deadline_daa(),
            c.terminal_rounds(),c.max_close_bytes(),c.max_terminal_macs(),c.max_operand_count())
            .and_then(|c2|c2.with_dissection_arity(c.dissection_arity())).map_err(|e|e.to_string())?;
        if self.step_ladder==0 || rebuilt!=c {return Err("invalid retained native court rules".into());} Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtCloseReceiptV1 {
    pub version:u16,
    pub service_profile_id:Hash64,
    pub allocation_id:Hash64,
    pub source_id:Hash64,
    pub material_until_daa:u64,
    pub point:PalwBlockContextV2,
    pub session:PalwCourtSessionStateV2,
    pub tir_phase:Option<Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>,
    pub declarer:PalwBondKeyV2,
    pub declarer_pubkey:Vec<u8>,
    pub declaration:Box<PalwConsensusObjectV2>,
    /// None is an explicitly missing native context, never default parameters.
    pub rules:Option<NativeCourtRulesV1>,
}
impl CourtCloseReceiptV1 {
    pub fn check(&self,key:&Key)->Result<(),String>{
        let PalwConsensusObjectV2::CourtCloseDeclared {session_id,side,count,chunk_digests,close_digest,verdict,signature}=self.declaration.as_ref()
            else{return Err("receipt is not a signed close declaration".into());};
        if self.version!=1 || *key!=Key::CourtCloseReceipt(*session_id,*side)
            || self.session.ladder.session_id()!=*session_id || self.point.daa_score<self.session.opened_daa
            || self.session.dissection.as_ref().is_some_and(|p|p.session_id()!=*session_id)
            || self.tir_phase.as_ref().is_some_and(|p|p.session_id()!=*session_id)
            || *count==0 || *count>PALW_COURT_CLOSE_MAX_CHUNKS || chunk_digests.len()!=usize::from(*count)
            || self.point.daa_score.checked_add(palw_close_assembly_daa_v1(*count)).is_none_or(|d|d>self.session.deadline_daa)
        {return Err("close receipt changes declaration/session scope".into());}
        if let Some(rules)=&self.rules {
            rules.check()?;
            if rules.at!=self.point {return Err("receipt rules belong to another native point".into());}
            crate::palw_court_v2::check_close_declared_chunk_count_v2(session_id,*count,&rules.court).map_err(|e|e.to_string())?;
        }
        if self.declarer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN || signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {return Err("close receipt key/signature length".into());}
        let message=crate::palw_court_v2::palw_court_close_declaration_message_v1(session_id,*side,*count,chunk_digests,close_digest,*verdict);
        if !crate::palw_attempt_material_v3::verify_ready_signature_v3(&self.declarer_pubkey,&message,signature,
            crate::palw_court_v2::PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT){return Err("retained close declaration signature is invalid".into());}
        Ok(())
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        let PalwConsensusObjectV2::CourtCloseDeclared {session_id,side,count,chunk_digests,close_digest,verdict,..}=self.declaration.as_ref()
            else{return Err("receipt declaration missing".into());};
        self.check(&Key::CourtCloseReceipt(*session_id,*side))?;
        let active=state.court_session(session_id);
        let archive=state.court_end_archive_v1(*session_id);
        if active.is_some()==archive.is_some(){return Err("receipt requires exactly one active or archived court".into());}
        let (session,group)=match (active,archive) {
            (Some(s),None)=>(s,state.court_close_group(session_id,*side)),
            (None,Some(a))=>(&a.session,a.groups.get(side)),_=>unreachable!(),
        };
        let group=group.ok_or("close receipt lost its declared group")?;
        if session.claim!=self.session.claim || session.challenger_bond!=self.session.challenger_bond
            || session.opened_daa!=self.session.opened_daa || session.deadline_daa!=self.session.deadline_daa
            || group.declarer!=self.declarer || group.count!=*count || group.chunk_digests!=*chunk_digests
            || group.close_digest!=*close_digest || group.verdict!=*verdict || group.declared_daa!=self.point.daa_score
        {return Err("close receipt substitutes its accepted contract".into());}
        let service=state.court_service_state_v1().ok_or("close receipt service missing")?;
        let binding=service.claims.get(&session.claim).ok_or("close receipt original binding missing")?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,session.claim).ok_or("close receipt frame missing")?;
        let producer=frame.custody_v1().pledge_v1().producer;
        let expected=if *side==PalwCourtSideV1::Executor {producer}else{session.challenger_bond};
        if self.declarer!=expected || self.service_profile_id!=service.profile_id || self.allocation_id!=binding.allocation_id
            || self.source_id!=binding.source_id || self.material_until_daa!=crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame,binding.hard_deadline_daa)?
            || state.bond(&self.declarer).is_some_and(|b|b.pubkey!=self.declarer_pubkey)
        {return Err("close receipt changes native original/key/material".into());} Ok(())
    }
    pub fn check_loaded_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        self.check_context(state)?;
        if state.last_point().is_none_or(|tip|tip.daa_score<self.point.daa_score){return Err("close receipt lies beyond accepted clock".into());}Ok(())
    }
}
