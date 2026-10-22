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
/// **RFC-0024 A on the shared court (lane r6-p2-shared; DORMANT, `palw_tir_bounded_door_v1`).** A bottom judged over ONE
/// history position of a terminal tile: `control` is an ordinary shared bottom statement at the tile's path with
/// `version == 2`, and `position` names the child of the tile's POSITION round — the writer row
/// ([`resolve_shared_position_writer_row_v2`]) the original's signer stored at that path, resolved from rooted state, never
/// carried. Signed under its own intent domain (binds `position`), stored in its own slot
/// ([`shared_position_bottom_id_v2`]): a whole-tile v1 bottom and a position bottom never collide.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedTirPositionBottomV2 { pub control:SignedSharedTirBottomV1, pub position:u8 }
pub const SHARED_TIR_POSITION_BOTTOM_VERSION_V2:u16=2;
pub fn shared_position_bottom_id_v2(root_id:Hash64,path:&[u8],position:u8)->Result<Hash64,String>{
    if path.len()>rounds::SHARED_TIR_MAX_PATH_V1 {return Err("shared position bottom path bound".into());}
    stream_digest(b"misaka-palw/shared-tir-bottom-position-slot/v2",&(root_id,path,position))
}
impl SignedSharedTirPositionBottomV2 {
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String>{
        let c=&self.control;
        if c.version!=SHARED_TIR_POSITION_BOTTOM_VERSION_V2 || c.path.len()>rounds::SHARED_TIR_MAX_PATH_V1 {return Err("shared position bottom version/path".into());}
        stream_digest(b"misaka-palw/shared-tir-bottom-intent/v2",&(network,c.version,c.claim,c.leaf,c.root_id,
            &c.path,c.parent_id,c.filer,self.position,c.bottom.as_ref()))
    }
    pub fn id(&self)->Result<Hash64,String>{shared_position_bottom_id_v2(self.control.root_id,&self.control.path,self.position)}
}
/// **The writer-row resolver for a position bottom (lane r6-p2-shared).** The position round a shared position bottom is
/// judged against is the ORIGINAL SIGNER's version-2 shared round row stored at the terminal tile's own path
/// (`palw_court_shared_tir_round_v1`, `Key::CourtSharedRound(round_id(root, path))`): resolved from rooted state, its shape
/// and signature re-checked, its numerical context re-checked against the projected terminal phase, and not later than
/// `at_daa`. Nothing the filer carries can substitute it.
pub fn resolve_shared_position_writer_row_v2<'s>(state:&'s PalwChainStateV2,claim:Hash64,leaf:u64,root_id:Hash64,path:&[u8],at_daa:u64)
    ->Result<&'s rounds::SharedTirRoundRowV1,String>{
    let key=Key::CourtSharedRound(rounds::round_id(root_id,path)?);
    let Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtSharedRound(row))=state.staged_court_row_v1(&key)
        else{return Err("shared position bottom has no position round on chain".into());};
    if row.control.version!=rounds::SHARED_TIR_POSITION_ROUND_VERSION_V2 || row.control.claim!=claim || row.control.leaf!=leaf
        || row.control.root_id!=root_id || row.control.path!=path {
        return Err("shared position writer row is not the terminal tile's position round".into());
    }
    if row.rules.at.daa_score>at_daa {return Err("shared position bottom precedes its position round".into());}
    row.check_context(state)?;
    Ok(&**row)
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub enum SharedTirBottomOutcomeV1 {
    NoMismatchFound,
    /// Native local evidence; may concern a signed numerical statement rather
    /// than a false original execution. Not a global ExecutorGuilty transition.
    NativeFault(crate::palw_step_leg::PalwStepRefutationVerdictV1),
}
/// Cannot deserialize a verdict capability or construct it from a peer's flag.
pub struct CheckedSharedTirBottomV1<'a> { control:&'a SignedSharedTirBottomV1,outcome:SharedTirBottomOutcomeV1,position:Option<u8> }
impl<'a> CheckedSharedTirBottomV1<'a> {
    pub fn control(&self)->&'a SignedSharedTirBottomV1 {self.control}
    pub fn outcome(&self)->&SharedTirBottomOutcomeV1 {&self.outcome}
    /// The named position of a version-2 (position) bottom; `None` for the whole-tile bottom.
    pub fn position(&self)->Option<u8> {self.position}
    /// Stored comparison checksum only, never a computation proof/certificate.
    pub fn outcome_digest(&self)->Result<Hash64,String>{
        let c=self.control;
        if let Some(p)=self.position {
            return match &self.outcome {
                SharedTirBottomOutcomeV1::NoMismatchFound=>stream_digest(b"misaka-palw/shared-tir-bottom-result/v2",&(c.root_id,&c.path,p,c.parent_id,0u8)),
                SharedTirBottomOutcomeV1::NativeFault(v)=>stream_digest(b"misaka-palw/shared-tir-bottom-result/v2",&(c.root_id,&c.path,p,c.parent_id,1u8,v.fault,v.evidence_id)),
            };
        }
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
    check_statement(state,rules,c,&bond.pubkey,None)
}
/// **The position bottom's check** (lane r6-p2-shared): [`check_shared_tir_bottom_v1`] with the position named by the
/// writer row the resolver finds. Read-only; the door fence is the fold's.
pub fn check_shared_tir_position_bottom_v2<'a>(state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
    c:&'a SignedSharedTirPositionBottomV2)->Result<CheckedSharedTirBottomV1<'a>,String>{
    let bond=state.bond(&c.control.filer).ok_or("shared position bottom filer missing")?;
    if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<params.min_collateral_sompi(){return Err("shared position bottom filer not active/above floor".into());}
    check_statement(state,rules,&c.control,&bond.pubkey,Some(c.position))
}
fn check_statement<'a>(state:&PalwChainStateV2,rules:&NativeCourtRulesV1,c:&'a SignedSharedTirBottomV1,key:&[u8],position:Option<u8>)
    ->Result<CheckedSharedTirBottomV1<'a>,String>{
    check_shared_bottom_cost_v1(&c.bottom,&rules.court)?;
    let r=rounds::root(state,c.claim,c.leaf)?;
    let context=crate::palw_court_shared_tir_state_v1::context(state,c.claim)?;
    let want_version=if position.is_some(){SHARED_TIR_POSITION_BOTTOM_VERSION_V2}else{1};
    if c.version!=want_version || c.path.len()>rounds::SHARED_TIR_MAX_PATH_V1 || c.root_id!=rounds::root_statement_id(r)?
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
    let message=match position {
        None=>c.signing_message(rules.network_domain)?,
        Some(p)=>SignedSharedTirPositionBottomV2 {control:c.clone(),position:p}.signing_message(rules.network_domain)?,
    };
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(key,message.as_byte_slice(),
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
    let judged=match position {
        None=>crate::palw_tir_court_v1::check_tir_dissect_bottom_v1(&prepared.phase,&full,c.leaf,&native),
        Some(p)=>{
            // RFC-0024 A: the writer row (the signer's position round at this terminal path) is resolved from rooted state
            // and the position named in it; the bottom is then evaluated over [p, p+1) only, against root totals.
            let writer=resolve_shared_position_writer_row_v2(state,c.claim,c.leaf,c.root_id,&c.path,rules.at.daa_score)?;
            let narrowing=prepared.phase.position_narrowing_v1().ok_or("shared position bottom: no terminal tile")?
                .choose_position_v1(&writer.control.round,p).map_err(|e|e.to_string())?;
            crate::palw_tir_court_v1::check_tir_dissect_position_bottom_v1(&prepared.phase,&narrowing,&full,c.leaf,&native)
        }
    };
    let outcome=match judged {
        Ok(v)=>SharedTirBottomOutcomeV1::NativeFault(v),
        Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound)
        |Err(crate::palw_step_refute::PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::NoFaultFound))=>SharedTirBottomOutcomeV1::NoMismatchFound,
        Err(e)=>return Err(e.to_string()),
    };
    Ok(CheckedSharedTirBottomV1 {control:c,outcome,position})
}
/// The stored comparison. **Row version 1** is the whole-tile bottom, byte for byte as before (`position` is `None` and is
/// not encoded). **Row version 2** (lane r6-p2-shared, RFC-0024 A) is the single-position bottom: the same fields, its
/// control at version 2, then `position` as one trailing byte. Version 2 is written only past `palw_tir_bounded_door_v1`
/// (the fold's gate); every version-1 row decodes unchanged.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SharedTirBottomRowV1 {
    pub version:u16,pub control:SignedSharedTirBottomV1,pub rules:NativeCourtRulesV1,pub filer_pubkey:Vec<u8>,
    pub material_until_daa:u64,pub outcome_digest:Hash64,
    /// The named position (row version 2 only).
    pub position:Option<u8>,
}
pub const SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2:u16=2;
impl BorshSerialize for SharedTirBottomRowV1 {
    fn serialize<W:std::io::Write>(&self,w:&mut W)->std::io::Result<()>{
        self.version.serialize(w)?;self.control.serialize(w)?;self.rules.serialize(w)?;self.filer_pubkey.serialize(w)?;
        self.material_until_daa.serialize(w)?;self.outcome_digest.serialize(w)?;
        match (self.version,self.position) {
            (1,None)=>Ok(()),
            (SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2,Some(p))=>p.serialize(w),
            _=>Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared bottom row version/position")),
        }
    }
}
impl BorshDeserialize for SharedTirBottomRowV1 {
    fn deserialize_reader<R:std::io::Read>(r:&mut R)->std::io::Result<Self>{
        let version=u16::deserialize_reader(r)?;
        if version!=1 && version!=SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared bottom row version"));}
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
        let position=if version==SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2 {Some(u8::deserialize_reader(r)?)} else {None};
        Ok(Self {version,control:SignedSharedTirBottomV1 {version:v,claim,leaf,root_id,path,parent_id,filer,bottom,signature},rules,filer_pubkey,material_until_daa,outcome_digest,position})
    }
}
impl SharedTirBottomRowV1 {
    /// The row's slot: the whole-tile bottom's path slot (version 1) or the position slot (version 2).
    pub fn id(&self)->Result<Hash64,String>{
        match self.position {
            None=>self.control.id(),
            Some(p)=>shared_position_bottom_id_v2(self.control.root_id,&self.control.path,p),
        }
    }
    pub fn check(&self,key:&Key)->Result<(),String>{
        let shape=match (self.version,self.control.version,self.position) {
            (1,1,None)=>true,
            (SHARED_TIR_BOTTOM_ROW_POSITION_VERSION_V2,SHARED_TIR_POSITION_BOTTOM_VERSION_V2,Some(_))=>true,
            _=>false,
        };
        if !shape || *key!=Key::CourtSharedBottom(self.id()?)
            || self.filer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || self.control.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN{return Err("shared bottom row shape/key".into());}Ok(())
    }
    pub fn checked<'a>(&'a self,state:&PalwChainStateV2)->Result<CheckedSharedTirBottomV1<'a>,String>{
        self.check(&Key::CourtSharedBottom(self.id()?))?;
        let r=rounds::root(state,self.control.claim,self.control.leaf)?;
        if self.material_until_daa!=r.material_until_daa{return Err("shared bottom horizon substitution".into());}
        let checked=check_statement(state,&self.rules,&self.control,&self.filer_pubkey,self.position)?;
        if checked.outcome_digest()?!=self.outcome_digest{return Err("shared bottom forged comparison checksum".into());}Ok(checked)
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String>{self.checked(state).map(|_|())}
    pub fn check_loaded_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        self.check_context(state)?;
        if state.last_point().is_none_or(|p|p.daa_score<self.rules.at.daa_score){return Err("shared bottom beyond snapshot point".into());}Ok(())
    }
}
