//! Private signed numerical disclosure retention. No public lifecycle, paid
//! reservation, signer-duty/default/credit/settlement permission.
use crate::{Hash64, palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, PalwBondKeyV2, PalwBondStatusV2}};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_court_shared_tir_root_v1::{SharedTirRootRowV1, shared_tir_root_policy_id_v1};
use crate::palw_court_shared_tir_stream_v1::stream_digest;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_dissect_v1::{PalwTirDissectRoundV1, PalwTirDissectPhaseV1};
use borsh::{BorshSerialize,BorshDeserialize};
/// History positions are u32 and every cut has at least two nonempty children.
/// Actual root round/terminal geometry is checked too; this is no new clock/cap.
pub const SHARED_TIR_MAX_PATH_V1: usize = u32::BITS as usize;
pub const SHARED_TIR_ROUND_CONTEXT_V1: &[u8] = b"misaka-palw/court-shared-tir-round/v1/mldsa87";
/// **RFC-0024 A on the shared court (lane r6-p2-shared).** A round control at version 2 is the terminal tile's POSITION
/// round — one claim per history position, folding exactly to the tile's claim — stored at the tile's own path (a slot no
/// version-1 round can fill: a terminal phase refuses a tile round). The row layout is unchanged; the fold writes it only
/// past `palw_tir_bounded_door_v1`. It is the writer row a shared position bottom resolves.
pub const SHARED_TIR_POSITION_ROUND_VERSION_V2: u16 = 2;
fn round_version_ok(v:u16)->bool {v==1 || v==SHARED_TIR_POSITION_ROUND_VERSION_V2}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedTirRoundV1 {
    pub version:u16,pub claim:Hash64,pub leaf:u64,pub root_id:Hash64,pub path:Vec<u8>,
    pub parent_id:Hash64,pub signer:PalwBondKeyV2,pub round:Box<PalwTirDissectRoundV1>,pub signature:Vec<u8>,
}
impl SignedSharedTirRoundV1 {
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String> {
        if !round_version_ok(self.version) || self.path.len()>SHARED_TIR_MAX_PATH_V1 {return Err("shared round version/path".into());}
        stream_digest(b"misaka-palw/shared-tir-round-intent/v1",&(network,self.version,self.claim,self.leaf,
            self.root_id,&self.path,self.parent_id,self.signer,self.round.as_ref()))
    }
    pub fn id(&self)->Result<Hash64,String>{round_id(self.root_id,&self.path)}
}
pub fn root_statement_id(root:&SharedTirRootRowV1)->Result<Hash64,String>{
    let c=&root.control;
    stream_digest(b"misaka-palw/shared-tir-numerical-root/v1",&(root.rules.network_domain,c.version,c.claim,c.scope_id,
        c.policy_id,c.leaf,c.arity,c.signer,c.root.version,&c.root.elements,&c.root.totals))
}
pub fn round_id(root_id:Hash64,path:&[u8])->Result<Hash64,String>{
    if path.len()>SHARED_TIR_MAX_PATH_V1 {return Err("shared round path exceeds history geometry".into());}
    stream_digest(b"misaka-palw/shared-tir-round-slot/v1",&(root_id,path))
}
fn next_parent(parent:Hash64,path:&[u8],round:&PalwTirDissectRoundV1)->Result<Hash64,String>{
    stream_digest(b"misaka-palw/shared-tir-round-parent/v1",&(parent,path,round))
}
pub fn root(state:&PalwChainStateV2,claim:Hash64,leaf:u64)->Result<&SharedTirRootRowV1,String>{
    match state.staged_court_row_v1(&Key::CourtSharedRoot(claim,leaf)) {
        Some(Row::CourtSharedRoot(r))=>Ok(r),_=>Err("shared round canonical root missing".into()),
    }
}
/// Arithmetic context only; phase clocks are inherited historical root clocks,
/// never a processing opportunity, accepted child choice or default deadline.
pub struct PreparedSharedTirRoundV1 {
    pub root_id:Hash64,pub parent_id:Hash64,pub phase:PalwTirDissectPhaseV1,
}
fn check_parts(state:&PalwChainStateV2,root:&SharedTirRootRowV1,rules:&NativeCourtRulesV1,
    control:&SignedSharedTirRoundV1,pubkey:&[u8])->Result<(),String>{
    let c=crate::palw_court_shared_tir_state_v1::context(state,control.claim)?;
    if !round_version_ok(control.version) || control.path.len()>SHARED_TIR_MAX_PATH_V1 || control.claim!=root.control.claim
        || control.leaf!=root.control.leaf || control.root_id!=root_statement_id(root)? || control.signer!=root.control.signer
        || shared_tir_root_policy_id_v1(rules)?!=root.control.policy_id
        || rules.at.daa_score<root.rules.at.daa_score || rules.at.daa_score>=c.hard_daa
        || control.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("shared round changes canonical root/signer/policy/interval".into());
    }
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(pubkey,
        control.signing_message(rules.network_domain)?.as_byte_slice(),&control.signature,SHARED_TIR_ROUND_CONTEXT_V1){
        return Err("shared round producer signature invalid".into());
    }
    Ok(())
}
/// Resolves only strictly shorter prefix paths, never scans the entire map or
/// recursively revalidates whole subtrees. All sibling paths remain available.
/// Caller admits root/source/numerical scratch and signature work before use.
pub fn prepare_shared_tir_round_v1(state:&PalwChainStateV2,claim:Hash64,leaf:u64,path:&[u8])
    ->Result<PreparedSharedTirRoundV1,String>{
    if path.len()>SHARED_TIR_MAX_PATH_V1 {return Err("shared round path bound".into());}
    let r=root(state,claim,leaf)?;let root_id=root_statement_id(r)?;let mut parent_id=root_id;
    let mut phase=r.numerical_frontier(state)?.project_unrooted_phase(&[])?;
    for depth in 0..path.len(){
        let key=Key::CourtSharedRound(round_id(root_id,&path[..depth])?);
        let Some(Row::CourtSharedRound(a))=state.staged_court_row_v1(&key) else{return Err("shared round ancestor missing".into());};
        a.check(&key)?;
        // A position round (version 2) is a terminal leaf of the tree, never an ancestor of a deeper path.
        if a.control.version!=1 {return Err("shared round ancestor is a position round".into());}
        if a.control.path!=path[..depth] || a.control.parent_id!=parent_id || a.material_until_daa!=r.material_until_daa {
            return Err("shared round ancestor substitutes path/parent/horizon".into());
        }
        check_parts(state,r,&a.rules,&a.control,&a.signer_pubkey)?;
        phase=phase.project_unrooted_child_v1(&a.control.round,path[depth])?;
        parent_id=next_parent(parent_id,&a.control.path,&a.control.round)?;
    }
    Ok(PreparedSharedTirRoundV1 {root_id,parent_id,phase})
}
pub fn check_shared_tir_round_v1(state:&PalwChainStateV2,params:&PalwStateParamsV2,rules:&NativeCourtRulesV1,
    control:&SignedSharedTirRoundV1)->Result<(),String>{
    let r=root(state,control.claim,control.leaf)?;
    let bond=state.bond(&control.signer).ok_or("shared round producer bond missing")?;
    if !matches!(bond.status,PalwBondStatusV2::Active) || bond.collateral<params.min_collateral_sompi(){
        return Err("shared round producer is not active/above existing floor".into());
    }
    check_parts(state,r,rules,control,&bond.pubkey)?;
    check_numerical_context(state,r,rules,control)
}
fn check_numerical_context(state:&PalwChainStateV2,r:&SharedTirRootRowV1,rules:&NativeCourtRulesV1,
    control:&SignedSharedTirRoundV1)->Result<(),String>{
    let prepared=prepare_shared_tir_round_v1(state,control.claim,control.leaf,&control.path)?;
    if control.parent_id!=prepared.parent_id {return Err("shared round does not bind accepted parent chain".into());}
    for depth in 0..control.path.len(){
        let key=Key::CourtSharedRound(round_id(prepared.root_id,&control.path[..depth])?);
        let Some(Row::CourtSharedRound(a))=state.staged_court_row_v1(&key) else{return Err("shared round lost ancestor".into());};
        if a.rules.at.daa_score>rules.at.daa_score {return Err("shared round precedes accepted ancestor".into());}
    }
    if control.root_id!=root_statement_id(r)? {return Err("shared round root substitution".into());}
    if control.version==SHARED_TIR_POSITION_ROUND_VERSION_V2 {
        // The terminal tile's position round, checked as the dissection's own (shape, intervals, exact fold to the tile).
        let narrowing=prepared.phase.position_narrowing_v1().ok_or("shared position round before a terminal tile")?;
        if !narrowing.awaits_position_round() {return Err("shared position round on a one-position tile".into());}
        return narrowing.check_position_round_v1(&control.round).map_err(|e|e.to_string());
    }
    prepared.phase.check_unrooted_round_v1(&control.round)
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SharedTirRoundRowV1 {
    pub version:u16,pub control:SignedSharedTirRoundV1,pub rules:NativeCourtRulesV1,
    pub signer_pubkey:Vec<u8>,pub material_until_daa:u64,
}
impl BorshDeserialize for SharedTirRoundRowV1 {
    fn deserialize_reader<R:std::io::Read>(r:&mut R)->std::io::Result<Self>{
        let version=u16::deserialize_reader(r)?;
        if version!=1 {return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared round row version"));}
        let v=u16::deserialize_reader(r)?;let claim=Hash64::deserialize_reader(r)?;let leaf=u64::deserialize_reader(r)?;
        let root_id=Hash64::deserialize_reader(r)?;
        let path=crate::palw_public_binding_v1::read_bounded_bytes_v1(r,SHARED_TIR_MAX_PATH_V1)?;
        let parent_id=Hash64::deserialize_reader(r)?;let signer=PalwBondKeyV2::deserialize_reader(r)?;
        // Snapshot-only nested body; enclosing native carriage shape preflight
        // and reservation precede deserialization. No public DTO decoder.
        let round=Box::deserialize_reader(r)?;let signature=crate::palw_public_binding_v1::read_signature_v1(r)?;
        let rules=NativeCourtRulesV1::deserialize_reader(r)?;
        let signer_pubkey=crate::palw_public_binding_v1::read_bounded_bytes_v1(r,crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN)?;
        if signer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN {return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"shared round public key length"));}
        let material_until_daa=u64::deserialize_reader(r)?;
        Ok(Self {version,control:SignedSharedTirRoundV1 {version:v,claim,leaf,root_id,path,parent_id,signer,round,signature},rules,signer_pubkey,material_until_daa})
    }
}
impl SharedTirRoundRowV1 {
    pub fn check(&self,key:&Key)->Result<(),String>{
        if self.version!=1 || !round_version_ok(self.control.version) || *key!=Key::CourtSharedRound(self.control.id()?)
            || self.signer_pubkey.len()!=crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || self.control.signature.len()!=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {return Err("shared round row shape/key".into());}
        Ok(())
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        self.check(&Key::CourtSharedRound(self.control.id()?))?;
        let r=root(state,self.control.claim,self.control.leaf)?;
        if self.material_until_daa!=r.material_until_daa {return Err("shared round horizon substitution".into());}
        check_parts(state,r,&self.rules,&self.control,&self.signer_pubkey)?;
        check_numerical_context(state,r,&self.rules,&self.control)
    }
    pub fn check_loaded_context(&self,state:&PalwChainStateV2)->Result<(),String>{
        self.check_context(state)?;
        if state.last_point().is_none_or(|p|p.daa_score<self.rules.at.daa_score){return Err("shared round beyond snapshot point".into());}
        Ok(())
    }
    pub fn same_statement(&self,c:&SignedSharedTirRoundV1)->bool{
        let a=&self.control;
        (a.version,a.claim,a.leaf,a.root_id,&a.path,a.parent_id,a.signer)
            ==(c.version,c.claim,c.leaf,c.root_id,&c.path,c.parent_id,c.signer) && a.round==c.round
    }
}
