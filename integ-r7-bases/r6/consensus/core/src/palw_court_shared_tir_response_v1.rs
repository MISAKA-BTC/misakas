//! RFC-0029 isolated fresh data responses. Not semantic court completion,
//! paid admission, credit, service opportunity or default permission.
use crate::{Hash64,BlockHash,palw_state_v2::{PalwBondKeyV2,PalwChainStateV2}};
use crate::palw_court_shared_tir_state_v1 as shared;
use crate::palw_court_shared_tir_stream_v1::stream_digest;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key,PalwStagedCourtRowV1 as Row};
use borsh::{BorshDeserialize,BorshSerialize};
use std::io::{self,Read};
const SIG:usize=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
pub const SHARED_DATA_RESPONSE_CONTEXT_V1:&[u8]=b"misaka-palw/court-shared-data-response/v1/mldsa87";
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedDataResponseV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub request_id:Hash64,pub signer:PalwBondKeyV2,
    pub query_block:BlockHash,pub query_daa:u64,pub data_digest:Hash64,pub signature:Vec<u8>,
}
impl BorshDeserialize for SignedSharedDataResponseV1 {
    fn deserialize_reader<R:Read>(r:&mut R)->io::Result<Self> {
        let version=u16::deserialize_reader(r)?;
        if version!=1 {return Err(io::Error::new(io::ErrorKind::InvalidData,"shared data response version"));}
        let claim=Hash64::deserialize_reader(r)?;let scope_id=Hash64::deserialize_reader(r)?;
        let request_id=Hash64::deserialize_reader(r)?;let signer=PalwBondKeyV2::deserialize_reader(r)?;
        let query_block=BlockHash::deserialize_reader(r)?;let query_daa=u64::deserialize_reader(r)?;
        let data_digest=Hash64::deserialize_reader(r)?;let n=u32::deserialize_reader(r)? as usize;
        if n!=SIG {return Err(io::Error::new(io::ErrorKind::InvalidData,"shared data response signature length"));}
        let mut signature=vec![0;SIG];r.read_exact(&mut signature)?;
        Ok(Self {version,claim,scope_id,request_id,signer,query_block,query_daa,data_digest,signature})
    }
}
impl SignedSharedDataResponseV1 {
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String> {
        if self.version!=1 {return Err("shared data response version".into());}
        stream_digest(b"misaka-palw/shared-data-response/v1",&(network,self.version,self.claim,self.scope_id,
            self.request_id,self.signer,self.query_block,self.query_daa,self.data_digest))
    }
    /// Identity only. Native query/preparation/data/accepting context are separate.
    pub fn validate_bond_signature(&self,network:Hash64,state:&PalwChainStateV2)->Result<(),String> {
        if self.signature.len()!=SIG {return Err("shared data response signature shape".into());}
        // G14 r5: the signer's CURRENT key at the response DAA (rotation-aware, Retiring still answers); relay-neutral.
        let key=crate::palw_court_clock_v2::palw_court_current_signer_key_v2(state,&self.signer)
            .map_err(|_|"shared data responder bond missing")?;
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            key,self.signing_message(network)?.as_byte_slice(),&self.signature,SHARED_DATA_RESPONSE_CONTEXT_V1) {
            return Err("shared data response signature invalid".into());
        }Ok(())
    }
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct SharedDataResponseV1 {
    pub version:u16,pub claim:Hash64,pub scope_id:Hash64,pub request_id:Hash64,pub signer:PalwBondKeyV2,
    pub query_block:BlockHash,pub query_daa:u64,pub data_digest:Hash64,
    pub accepted_block:BlockHash,pub accepted_daa:u64,pub intent_digest:Hash64,pub material_until_daa:u64,
}
/// Current data-response stage only. An outer hard bound is NOT an adopted
/// per-turn response deadline. A pending stage is never default authority.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SharedDataResponseStageV1 {
    pub claim:Hash64,pub scope_id:Hash64,pub request_id:Hash64,pub signer:PalwBondKeyV2,
    pub query_block:BlockHash,pub query_daa:u64,pub ready_daa:u64,
    pub resource_first_eligible_daa:Option<u64>,pub original_hard_daa:u64,
    pub data:Option<SharedDataAvailabilityV1>,pub response:Option<SharedDataResponseV1>,
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SharedDataAvailabilityV1 {pub block:BlockHash,pub daa:u64,pub digest:Hash64}
pub(crate) fn query(state:&PalwChainStateV2,id:Hash64)->Result<&shared::SharedTirQueryV1,String> {
    match state.staged_court_row_v1(&Key::CourtSharedQuery(id)) {Some(Row::CourtSharedQuery(q)) if q.request_id==id=>Ok(q),_=>Err("shared response query missing".into())}
}
pub fn shared_data_fingerprint_v1(state:&PalwChainStateV2,id:Hash64)->Result<Option<Hash64>,String> {
    Ok(shared_data_availability_v1(state,id)?.map(|data|data.digest))
}
pub fn shared_data_availability_v1(state:&PalwChainStateV2,id:Hash64)->Result<Option<SharedDataAvailabilityV1>,String> {
    let q=query(state,id)?;q.check_context(state)?;
    let Some(data)=shared::read_shared_data_v1(state,q.claim,q.ordinal)? else {return Ok(None)};
    let (block,daa,digest)=match data {
        shared::RootedSharedDataV1::Unit(unit)=>{
            let body=q.body.as_ref().ok_or("shared response unit lacks accepted data context")?;
            (body.block,body.accepted_daa,stream_digest(b"misaka-palw/shared-response-unit/v1",
                &(q.scope_id,q.request_id,unit.source_id_v1(),unit.response_v1()))?)
        },
        shared::RootedSharedDataV1::Ids {rows_root,ids}=>{
            let Some(Row::CourtSharedIds(row))=state.staged_court_row_v1(&Key::CourtSharedIds(q.claim)) else {return Err("shared response IDs lack accepted context".into());};
            (row.block,row.accepted_daa,stream_digest(b"misaka-palw/shared-response-ids/v1",&(q.scope_id,q.request_id,rows_root,ids))?)
        },
    };
    Ok(Some(SharedDataAvailabilityV1 {block,daa,digest}))
}
fn ready_daa(state:&PalwChainStateV2,claim:Hash64,signer:&PalwBondKeyV2)->Result<u64,String> {
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state,claim).ok_or("shared response original missing")?;
    let Some(Row::TirPreparation(row))=state.staged_court_row_v1(&frame.preparation_key_v1()) else {return Err("shared response preparation missing".into());};
    crate::palw_tir_public_state_v1::palw_tir_public_source_for_preparation_row_v1(state,row,frame.descriptor_v1().maximum_leaves)?;
    Ok(row.preparation.ready_v1(signer).ok_or("shared responder was not original Ready signer")?.accepted_daa)
}
pub fn shared_data_response_stage_v1(state:&PalwChainStateV2,id:Hash64,signer:PalwBondKeyV2)->Result<SharedDataResponseStageV1,String> {
    let q=query(state,id)?;q.check_context(state)?;let c=shared::context(state,q.claim)?;c.source(state)?;
    let ready=ready_daa(state,q.claim,&signer)?;
    let data=shared_data_availability_v1(state,id)?;
    let first=data.as_ref().map(|d|q.opened_daa.max(ready).max(d.daa).checked_add(1)
        .ok_or("shared response eligibility overflow")).transpose()?;
    let response=match state.staged_court_row_v1(&Key::CourtSharedResponse(id,signer)) {
        Some(Row::CourtSharedResponse(r))=>{r.check_context(state)?;Some((**r).clone())},None=>None,_=>return Err("shared response foreign row".into()),
    };
    Ok(SharedDataResponseStageV1 {claim:q.claim,scope_id:c.scope_id,request_id:id,signer,query_block:q.opened_block,query_daa:q.opened_daa,
        ready_daa:ready,resource_first_eligible_daa:first,original_hard_daa:c.hard_daa,
        data,response})
}
impl SharedDataResponseV1 {
    pub fn check(&self,key:&Key)->Result<(),String> {
        if self.version!=1 || *key!=Key::CourtSharedResponse(self.request_id,self.signer)
            || self.accepted_daa<self.query_daa || self.accepted_block==self.query_block {
            return Err("shared data response version/key/causal context".into());
        }Ok(())
    }
    pub fn check_context(&self,state:&PalwChainStateV2)->Result<(),String> {
        self.check(&Key::CourtSharedResponse(self.request_id,self.signer))?;
        let q=query(state,self.request_id)?;q.check_context(state)?;
        let c=shared::context(state,self.claim)?;c.source(state)?;
        if (self.claim,self.scope_id,self.query_block,self.query_daa,self.material_until_daa)
            !=(q.claim,c.scope_id,q.opened_block,q.opened_daa,c.material_until_daa)
            || self.accepted_daa<c.original_accepted_daa || self.accepted_daa>=c.hard_daa {
            return Err("shared response substitutes query/original/clock".into());
        }
        crate::palw_court_shared_tir_stream_state_v1::ready_at(state,c,&self.signer,self.accepted_daa)?;
        let data=shared_data_availability_v1(state,self.request_id)?.ok_or("shared response data unavailable")?;
        if data.digest!=self.data_digest || data.daa>self.accepted_daa {
            return Err("shared response lacks matching authenticated data".into());
        }
        // Current bond activity/key was checked at acceptance. Public historical
        // response facts must not be invalidated by later bond exit.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n:u64)->Hash64 {Hash64::from_u64_word(n)}
    fn control()->SignedSharedDataResponseV1 {
        SignedSharedDataResponseV1 {version:1,claim:h(1),scope_id:h(2),request_id:h(3),signer:PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(4),0)),
            query_block:h(5),query_daa:6,data_digest:h(7),signature:vec![0;SIG]}
    }
    #[test]
    fn court_tir_shared_data_response_codec_bounds_signature_before_allocation() {
        let c=control();let wire=borsh::to_vec(&c).unwrap();
        // Derived fixed envelope, not a carrier/fee or capacity assumption.
        assert_eq!(wire.len(),2+64*5+68+8+4+SIG);
        assert_eq!(borsh::from_slice::<SignedSharedDataResponseV1>(&wire).unwrap(),c);
        assert!(borsh::from_slice::<SignedSharedDataResponseV1>(&wire[..wire.len()-1]).is_err());
        let mut trailing=wire.clone();trailing.push(0);assert!(borsh::from_slice::<SignedSharedDataResponseV1>(&trailing).is_err());
        let at=wire.len()-SIG-4;
        for n in [0,u32::MAX] {let mut bad=wire[..at].to_vec();bad.extend(n.to_le_bytes());
            assert_eq!(borsh::from_slice::<SignedSharedDataResponseV1>(&bad).unwrap_err().kind(),io::ErrorKind::InvalidData);}
        assert!(SignedSharedDataResponseV1::deserialize_reader(&mut 2u16.to_le_bytes().as_slice()).is_err());
    }
    #[test]
    fn court_tir_shared_data_response_real_signatures_bind_query_block_and_data() {
        use crate::palw_state_v2::{PalwBondStateV2,PalwBondStatusV2};
        let mut c=control();let mut state=PalwChainStateV2::genesis();let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([41;32]);
        state.set_bond_for_tests(c.signer,Some(PalwBondStateV2 {pubkey:kp.verification_key.as_ref().to_vec(),operator_id:h(8),collateral:0,
            slashed:0,status:PalwBondStatusV2::Active,registered_daa:0,payout_payload:h(9),capable_classes:Default::default()}));
        c.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,c.signing_message(h(10)).unwrap().as_byte_slice(),
            SHARED_DATA_RESPONSE_CONTEXT_V1,[42;32]).unwrap().as_ref().to_vec();c.validate_bond_signature(h(10),&state).unwrap();
        for n in 0..10 {let mut bad=c.clone();let mut network=h(10);match n {
            0=>network=h(11),1=>bad.version=2,2=>bad.claim=h(11),3=>bad.scope_id=h(11),4=>bad.request_id=h(11),5=>bad.signer.0.index=1,
            6=>bad.query_block=h(11),7=>bad.query_daa+=1,8=>bad.data_digest=h(11),_=>bad.signature[0]^=1}
            assert!(bad.validate_bond_signature(network,&state).is_err());
        }
        let mut wrong_context=c.clone();wrong_context.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,c.signing_message(h(10)).unwrap().as_byte_slice(),
            crate::palw_court_shared_tir_stream_v1::SHARED_STREAM_CONTEXT_V1,[42;32]).unwrap().as_ref().to_vec();
        assert!(wrong_context.validate_bond_signature(h(10),&state).is_err());
    }

    #[test]
    fn r5_court_response_uses_the_current_key_retiring_answers_and_a_relay_carries_it() {
        use crate::palw_state_v2::{PalwBondStateV2,PalwBondStatusV2};
        let mut c=control();let mut state=PalwChainStateV2::genesis();
        let old=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([51;32]);let new=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([52;32]);
        let reg=|pk:&[u8],status|PalwBondStateV2 {pubkey:pk.to_vec(),operator_id:h(8),collateral:0,slashed:0,status,
            registered_daa:0,payout_payload:h(9),capable_classes:Default::default()};
        let sign=|c:&SignedSharedDataResponseV1,sk:&libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey|libcrux_ml_dsa::ml_dsa_87::sign(sk,
            c.signing_message(h(10)).unwrap().as_byte_slice(),SHARED_DATA_RESPONSE_CONTEXT_V1,[53;32]).unwrap().as_ref().to_vec();
        // Admitted under the old key.
        state.set_bond_for_tests(c.signer,Some(reg(old.verification_key.as_ref(),PalwBondStatusV2::Active)));
        c.signature=sign(&c,&old.signing_key);c.validate_bond_signature(h(10),&state).unwrap();
        // Rotated before the response DAA: the old key's signature is refused, the new key's accepted.
        state.set_bond_for_tests(c.signer,Some(reg(new.verification_key.as_ref(),PalwBondStatusV2::Active)));
        assert!(c.validate_bond_signature(h(10),&state).is_err(),"a key from admission is not the current authority");
        let mut fresh=c.clone();fresh.signature=sign(&fresh,&new.signing_key);fresh.validate_bond_signature(h(10),&state).unwrap();
        // Retiring: still backs its claims to resolution, so it still answers.
        state.set_bond_for_tests(c.signer,Some(reg(new.verification_key.as_ref(),PalwBondStatusV2::Retiring{since_daa:5,settled_at_since:0})));
        fresh.validate_bond_signature(h(10),&state).unwrap();
        // Gone: no authority.
        let mut gone=state.clone();gone.set_bond_for_tests(c.signer,None);assert!(fresh.validate_bond_signature(h(10),&gone).is_err());
        // Relay: the signed message names no carrier, so the bytes a third party carries verify unchanged (the signer is the authority).
        let relayed:SignedSharedDataResponseV1=borsh::from_slice(&borsh::to_vec(&fresh).unwrap()).unwrap();
        relayed.validate_bond_signature(h(10),&state).unwrap();
        // No replay as a NEW response: the same signature under another request id or query DAA is refused.
        let mut replay=relayed.clone();replay.request_id=h(77);assert!(replay.validate_bond_signature(h(10),&state).is_err());
        let mut later=relayed.clone();later.query_daa+=1;assert!(later.validate_bond_signature(h(10),&state).is_err());
    }
}
