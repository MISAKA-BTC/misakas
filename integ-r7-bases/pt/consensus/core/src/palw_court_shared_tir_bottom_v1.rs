//! Private per-terminal recomputation evidence. A local mismatch/no-mismatch
//! never certifies or closes the whole claim, pays, slashes or authorizes default.
use crate::{Hash64,palw_state_v2::{PalwChainStateV2,PalwStateParamsV2,PalwBondKeyV2,PalwBondStatusV2}};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_court_shared_tir_round_v1 as rounds;
use crate::palw_court_shared_tir_stream_v1::stream_digest;
use crate::palw_tir_court_v1::PalwTirConeRefutationV1;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key};
use borsh::{BorshSerialize,BorshDeserialize};
pub const SHARED_TIR_BOTTOM_CONTEXT_V1:&[u8]=b"misaka-palw/court-shared-tir-bottom/v1/mldsa87";
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedTirBottomV1 {
    pub version:u16,pub claim:Hash64,pub leaf:u64,pub root_id:Hash64,pub path:Vec<u8>,
    pub parent_id:Hash64,pub filer:PalwBondKeyV2,pub bottom:Box<PalwTirConeRefutationV1>,pub signature:Vec<u8>,
}
impl SignedSharedTirBottomV1 {
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String>{
        if self.version!=1 || self.path.len()>rounds::SHARED_TIR_MAX_PATH_V1 {return Err("shared bottom version/path".into());}
        stream_digest(b"misaka-palw/shared-tir-bottom-intent/v1",&(network,self.version,self.claim,self.leaf,self.root_id,
            &self.path,self.parent_id,self.filer,self.bottom.as_ref()))
    }
    pub fn id(&self)->Result<Hash64,String>{rounds::round_id(self.root_id,&self.path)}
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum SharedTirBottomOutcomeV1 {
    NoMismatchFound,
    /// Native local evidence; may concern a signed numerical statement rather
    /// than a false original execution. Not a global ExecutorGuilty transition.
    NativeFault(crate::palw_step_leg::PalwStepRefutationVerdictV1),
}
/// Cannot deserialize a verdict capability or construct it from a peer's flag.
pub struct CheckedSharedTirBottomV1<'a> { control:&'a SignedSharedTirBottomV1,outcome:SharedTirBottomOutcomeV1 }
impl<'a> CheckedSharedTirBottomV1<'a> {
    pub fn control(&self)->&'a SignedSharedTirBottomV1 {self.control}
    pub fn outcome(&self)->&SharedTirBottomOutcomeV1 {&self.outcome}
    /// Stored comparison checksum only, never a computation proof/certificate.
    pub fn outcome_digest(&self)->Result<Hash64,String>{
        let c=self.control;
        match &self.outcome {
            SharedTirBottomOutcomeV1::NoMismatchFound=>stream_digest(b"misaka-palw/shared-tir-bottom-result/v1",&(c.root_id,&c.path,c.parent_id,0u8)),
            SharedTirBottomOutcomeV1::NativeFault(v)=>stream_digest(b"misaka-palw/shared-tir-bottom-result/v1",&(c.root_id,&c.path,c.parent_id,1u8,v.fault,v.evidence_id)),
        }
    }
}
/// Exact existing TirDissection proof-variant size without allocating or
/// cloning the payload: Borsh enum discriminant is one byte, then this body.
pub fn shared_bottom_close_bytes_v1(bottom:&PalwTirConeRefutationV1)->Result<u64,String>{
    struct Count(u64);
    impl std::io::Write for Count {
        fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{
            self.0=self.0.checked_add(bytes.len() as u64).ok_or_else(||std::io::Error::other("bottom byte count overflow"))?;Ok(bytes.len())
        }
        fn flush(&mut self)->std::io::Result<()>{Ok(())}
    }
    let mut count=Count(1);bottom.serialize(&mut count).map_err(|e|e.to_string())?;Ok(count.0)
}
pub fn check_shared_bottom_cost_v1(bottom:&PalwTirConeRefutationV1,court:&crate::palw_mode_v2::PalwCourtParamsV2)->Result<(),String>{
    if shared_bottom_close_bytes_v1(bottom)?>court.max_close_bytes(){return Err("shared bottom exceeds native close byte ceiling".into());}Ok(())
}
/// Actual native rules/params and complete verifier work admission are the
/// caller's. Read-only result, not permission to end/pay/slash any court/claim.
pub fn check_shared_tir_bottom_v1<'a>(state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
    c:&'a SignedSharedTirBottomV1)->Result<CheckedSharedTirBottomV1<'a>,String>{
    let bond=state.bond(&c.filer).ok_or("shared bottom filer missing")?;
    if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<params.min_collateral_sompi(){return Err("shared bottom filer not active/above floor".into());}
    check_statement(state,rules,c,&bond.pubkey)
}
fn check_statement<'a>(state:&PalwChainStateV2,rules:&NativeCourtRulesV1,c:&'a SignedSharedTirBottomV1,key:&[u8])
    ->Result<CheckedSharedTirBottomV1<'a>,String>{
    check_shared_bottom_cost_v1(&c.bottom,&rules.court)?;
    let r=rounds::root(state,c.claim,c.leaf)?;
    let context=crate::palw_court_shared_tir_state_v1::context(state,c.claim)?;
    if c.version!=1 || c.path.len()>rounds::SHARED_TIR_MAX_PATH_V1 || c.root_id!=rounds::root_statement_id(r)?
        || crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?!=r.control.policy_id
        || rules.at.daa_score<r.rules.at.daa_score || rules.at.daa_score>=context.hard_daa
        || c.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("shared bottom changes canonical root/policy/interval/signature".into());
    }
    // Pin the actual leaf BEFORE the native checker: it can return a carriage
    // conviction before its own later narrowed-leaf check.
    if c.bottom.output_opening.leaf_index!=c.leaf
        || c.bottom.output_preimage!=r.control.root.finalize.output_preimage
        || crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&c.bottom.binding)
            !=crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&r.control.root.finalize.binding) {
        return Err("shared bottom substitutes leaf/preimage/original header".into());
    }
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(key,c.signing_message(rules.network_domain)?.as_byte_slice(),
        &c.signature,SHARED_TIR_BOTTOM_CONTEXT_V1){return Err("shared bottom filer signature invalid".into());}
    let prepared=rounds::prepare_shared_tir_round_v1(state,c.claim,c.leaf,&c.path)?;
    if c.parent_id!=prepared.parent_id || prepared.phase.terminal_range().is_none(){return Err("shared bottom missing terminal/parent binding".into());}
    for depth in 0..c.path.len(){
        let key=Key::CourtSharedRound(rounds::round_id(c.root_id,&c.path[..depth])?);
        let Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtSharedRound(a))=state.staged_court_row_v1(&key)
            else{return Err("shared bottom ancestor disappeared".into());};
        if a.rules.at.daa_score>rules.at.daa_score{return Err("shared bottom precedes its ancestor".into());}
    }
    let claim=state.claim(&c.claim).ok_or("shared bottom original claim missing")?;
    let mut full=c.bottom.as_ref().clone();
    full.binding=crate::palw_court_v2::tir_binding_filled_v1(state,claim,&c.bottom.binding).map_err(|e|e.to_string())?;
    crate::palw_court_v2::check_tir_binding_is_the_claims_v1(state,claim,&full.binding).map_err(|e|e.to_string())?;
    let prompt=if rules.prompt_ids_merkle{crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1}else{crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat};
    let native=crate::palw_tir_court_v1::PalwTirCourtRulesV1 {max_step_leaf_count:rules.step_ladder,prompt_form:prompt,
        limits:crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court)};
    let outcome=match crate::palw_tir_court_v1::check_tir_dissect_bottom_v1(&prepared.phase,&full,c.leaf,&native){
        Ok(v)=>SharedTirBottomOutcomeV1::NativeFault(v),
        Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound)
        |Err(crate::palw_step_refute::PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::NoFaultFound))=>SharedTirBottomOutcomeV1::NoMismatchFound,
        Err(e)=>return Err(e.to_string()),
    };
    Ok(CheckedSharedTirBottomV1 {control:c,outcome})
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SharedTirBottomRowV1 {
    pub version:u16,pub control:SignedSharedTirBottomV1,pub rules:NativeCourtRulesV1,pub filer_pubkey:Vec<u8>,
    pub material_until_daa:u64,pub outcome_digest:Hash64,
}
impl BorshDeserialize for SharedTirBottomRowV1 {
    fn deserialize_reader<R:std::io::Read>(r:&mut R)->std::io::Result<Self>{
        let version=u16::deserialize_reader(r)?;
        if version!=1{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared bottom row version"));}
        let v=u16::deserialize_reader(r)?;let claim=Hash64::deserialize_reader(r)?;let leaf=u64::deserialize_reader(r)?;
        let root_id=Hash64::deserialize_reader(r)?;let path=crate::palw_public_binding_v1::read_bounded_bytes_v1(r,rounds::SHARED_TIR_MAX_PATH_V1)?;
        let parent_id=Hash64::deserialize_reader(r)?;let filer=PalwBondKeyV2::deserialize_reader(r)?;
        // Snapshot-only: enclosing native shape preflight/reservation precedes
        // nested proof decoding. Signed public DTO has no wire deserializer.
        let bottom=Box::deserialize_reader(r)?;let signature=crate::palw_public_binding_v1::read_signature_v1(r)?;
        let rules=NativeCourtRulesV1::deserialize_reader(r)?;
        let filer_pubkey=crate::palw_public_binding_v1::read_bounded_bytes_v1(r,crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN)?;
        if filer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared bottom filer key length"));}
        let material_until_daa=u64::deserialize_reader(r)?;let outcome_digest=Hash64::deserialize_reader(r)?;
        Ok(Self {version,control:SignedSharedTirBottomV1 {version:v,claim,leaf,root_id,path,parent_id,filer,bottom,signature},rules,filer_pubkey,material_until_daa,outcome_digest})
    }
}
impl SharedTirBottomRowV1 {
    pub fn check(&self,key:&Key)->Result<(),String>{
        if self.version!=1 || self.control.version!=1 || *key!=Key::CourtSharedBottom(self.control.id()?)
            || self.filer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || self.control.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN{return Err("shared bottom row shape/key".into());}Ok(())
    }
    pub fn checked<'a>(&'a self,state:&PalwChainStateV2)->Result<CheckedSharedTirBottomV1<'a>,String>{
        self.check(&Key::CourtSharedBottom(self.control.id()?))?;
        let r=rounds::root(state,self.control.claim,self.control.leaf)?;
        if self.material_until_daa!=r.material_until_daa{return Err("shared bottom horizon substitution".into());}
        let checked=check_statement(state,&self.rules,&self.control,&self.filer_pubkey)?;
        if checked.outcome_digest()?!=self.outcome_digest{return Err("shared bottom forged comparison checksum".into());}Ok(checked)
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String>{self.checked(state).map(|_|())}
    pub fn check_loaded_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        self.check_context(state)?;
        if state.last_point().is_none_or(|p|p.daa_score<self.rules.at.daa_score){return Err("shared bottom beyond snapshot point".into());}Ok(())
    }
}
