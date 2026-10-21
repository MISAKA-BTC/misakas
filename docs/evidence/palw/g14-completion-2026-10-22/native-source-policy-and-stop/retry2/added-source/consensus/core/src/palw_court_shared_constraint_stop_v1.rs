//! Private constrained original-stop evidence. Separate from old token intent,
//! no public wire/tag, paid admission, Complete flag or original-tail change.
use super::{SharedTirBuilderStoreV1 as Store,constraint_terminal::ConstraintMaterialV1 as Material};
use crate::{Hash64,palw_state_v2::{PalwChainStateV2,PalwStateParamsV2,PalwBondKeyV2,PalwBondStatusV2}};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1 as Court,PalwConstraintStreamFaultV1 as Fault,canonical_replay as replay};
use crate::palw_freeprompt_v3::PalwFpStopReasonV3;
pub(crate) const CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1:&[u8]=b"misaka-palw/shared-constraint-stop/v1/mldsa87";
#[derive(Clone,borsh::BorshSerialize)]
pub(crate) struct SignedConstraintStopIntentV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub policy_id:Hash64,
    pub semantics_id:Hash64,pub material_id:Hash64,
    /// Only the actual end position D is accepted; this is not a token root.
    pub position:u32,pub stop_reason:PalwFpStopReasonV3,pub filer:PalwBondKeyV2,pub signature:Vec<u8>,
}
impl SignedConstraintStopIntentV1 {
    pub(crate) fn signing_message(&self,network:Hash64)->Result<Hash64,String>{
        if self.version!=1{return Err("constraint stop intent version".into());}
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/shared-constraint-stop-intent/v1",
            &(network,self.version,self.claim,self.scope_id,self.policy_id,self.semantics_id,self.material_id,self.position,self.stop_reason,self.filer))
    }
}
pub(crate) struct ConstraintStopEvidenceV1<'a> {
    intent:SignedConstraintStopIntentV1,material:Material<'a>,finding:Option<Fault>,
    identity:(Hash64,Hash64,Hash64,Hash64),
}
impl ConstraintStopEvidenceV1<'_>{
    pub(crate) fn recheck(&self,store:&Store<'_>,state:&PalwChainStateV2,params:&PalwStateParamsV2,
        rules:&NativeCourtRulesV1,budget:replay::WorkBudget)->Result<Option<Fault>,String>{
        let result=store.replay_constraint_stop_v1(state,params,rules,&self.intent,self.material,budget)?;
        if result.identity()!=self.identity||result.finding()!=self.finding.as_ref(){return Err("constraint stop reconstruction mismatch".into());}
        Ok(result.finding().cloned())
    }
}
impl Store<'_>{
    /// Native caller resolves actual rules and separately admits source, parser,
    /// body proof/hash, actor signature, Court clone and evidence ownership.
    pub(crate) fn build_constraint_stop_terminal_v1<'a>(&self,state:&PalwChainStateV2,params:&PalwStateParamsV2,
        rules:&NativeCourtRulesV1,intent:&SignedConstraintStopIntentV1,material:Material<'a>,budget:replay::WorkBudget)
        ->Result<ConstraintStopEvidenceV1<'a>,String>{
        let result=self.replay_constraint_stop_v1(state,params,rules,intent,material,budget)?;
        let evidence=ConstraintStopEvidenceV1{intent:intent.clone(),material,finding:result.finding().cloned(),identity:result.identity()};
        evidence.recheck(self,state,params,rules,budget)?;Ok(evidence)
    }
    fn replay_constraint_stop_v1(&self,state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
        intent:&SignedConstraintStopIntentV1,material:Material<'_>,budget:replay::WorkBudget)->Result<replay::Candidate,String>{
        let c=crate::palw_court_shared_tir_state_v1::context(state,intent.claim)?;
        let source=c.source(state)?;
        let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,intent.claim).ok_or("constraint stop original absent")?;
        let original=frame.original_v1();let job=&original.job;
        if intent.scope_id!=c.scope_id||source.source_id_v1()!=self.source.source_id_v1()
            ||!rules.tir_active||!rules.constraint_active||rules.at.daa_score<c.original_accepted_daa||rules.at.daa_score>=c.hard_daa
            ||rules.step_ladder!=frame.plan_v1().owner_work.rules.max_step_leaf_count
            ||intent.policy_id!=crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            ||intent.position!=original.decode_tokens_executed||intent.stop_reason!=original.stop_reason {
            return Err("constraint stop actual scope/policy/end position/native reason mismatch".into());
        }
        let semantics=state.claim_semantics_v1(&intent.claim).ok_or("constraint stop immutable semantics absent")?;
        let header=source.header_v1();
        if !semantics.matches_context(&header.job_context)||!semantics.well_formed_for(&job.class_id,&header.artifact_root)
            ||semantics.bound_from_daa>c.original_accepted_daa||intent.semantics_id!=semantics.commitment()
            ||semantics.token_table_root.is_none()||semantics.token_table_root!=crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job){
            return Err("constraint stop immutable table semantics mismatch".into());
        }
        if intent.material_id!=material.identity(budget)? {return Err("constraint stop material identity mismatch".into());}
        let bond=state.bond(&intent.filer).ok_or("constraint stop filer absent")?;
        if intent.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN||!matches!(bond.status,PalwBondStatusV2::Active)
            ||bond.collateral<params.min_collateral_sompi()
            ||!crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(&bond.pubkey,
                intent.signing_message(rules.network_domain)?.as_byte_slice(),&intent.signature,CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1){
            return Err("constraint stop current filer authority/signature invalid".into());
        }
        let record=state.tir_class_v1(&job.class_id).ok_or("constraint stop actual registered class missing")?;
        let view=crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header,record,frame.descriptor_v1().maximum_leaves)?;
        let table=semantics.token_table_root.ok_or("constraint stop table missing")?;
        if !material.eog.verifies(&table,view.vocab_size)||material.renderings.iter().any(|r|!r.verifies(&table,view.vocab_size)){
            return Err("constraint stop actual material opening mismatch".into());
        }
        let ids=self.ids.ok_or("constraint stop source-authenticated FullIds missing")?;
        let identity=borsh::to_vec(header).map_err(|e|e.to_string())?;
        let base=Court::new_from_tiled_ids_with_original_stop_context_v1(intent.claim,intent.filer,c.original_accepted_daa,
            &view,&identity,&source,ids,original,u32::try_from(material.eog.ids.len()).map_err(|_|"constraint stop EOG count overflow")?,
            &material.eog.opening,intent.signing_message(rules.network_domain)?)?;
        replay::build(&base,&material,budget).map_err(|e|format!("constraint stop replay: {e:?}"))
    }
}
