//! RFC-0029 isolated in-memory shared-data candidate. No lifecycle tag, rooted
//! row, admission/payment, service supply, signer duty or default authority.
//! Public source identity openings are not model computation certification.
use crate::{Hash64,palw_state_v2::{PalwBondKeyV2,PalwChainStateV2}};
use crate::palw_court_tir_requests_v1::TirSharedRequestSetV1;
use crate::palw_tir_public_source_v1::{PalwTirPublicAuthenticatedUnitV1 as Unit,
    PalwTirPublicAuthenticatedIdsV1 as Ids,PalwTirPublicRequestV1 as Query};
use borsh::{BorshDeserialize,BorshSerialize};
use std::{collections::BTreeMap,io::{self,Read}};
const SIG:usize=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
pub const SHARED_REQUEST_CONTEXT_V1:&[u8]=b"misaka-palw/court-shared-request/v1/mldsa87";

/// Request intent only. Literal requester signs; the shared query identity
/// excludes requester/relay/owner. No optional zero fee or invented paid permit.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedRequestV1 {
    pub version:u16,pub scope:Hash64,pub ordinal:u64,pub request_id:Hash64,
    pub signer:PalwBondKeyV2,pub signature:Vec<u8>,
}
impl BorshDeserialize for SignedSharedRequestV1 {
    fn deserialize_reader<R:Read>(r:&mut R)->io::Result<Self> {
        let version=u16::deserialize_reader(r)?;let scope=Hash64::deserialize_reader(r)?;
        let ordinal=u64::deserialize_reader(r)?;let request_id=Hash64::deserialize_reader(r)?;
        let signer=PalwBondKeyV2::deserialize_reader(r)?;
        let n=u32::deserialize_reader(r)? as usize;
        if n!=SIG { return Err(io::Error::new(io::ErrorKind::InvalidData,"shared request signature length")); }
        let mut signature=vec![0;SIG];r.read_exact(&mut signature)?;
        Ok(Self {version,scope,ordinal,request_id,signer,signature})
    }
}
impl SignedSharedRequestV1 {
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String> {
        if self.version!=1 { return Err("unsupported shared request version".into()); }
        let bytes=borsh::to_vec(&(network,self.version,self.scope,self.ordinal,self.request_id,self.signer))
            .map_err(|e|e.to_string())?;
        Ok(Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64)
            .key(b"misaka-palw/shared-tir-request-intent/v1").hash(&bytes).as_bytes().try_into().expect("64")))
    }
    /// Bond identity only. Current native frame/profile/admission/fee/clock
    /// authorization is NOT inferred from a supplied query set or this signature.
    pub fn validate_bond_signature(&self,network:Hash64,state:&PalwChainStateV2)->Result<(),String> {
        if self.signature.len()!=SIG { return Err("shared request signature shape".into()); }
        let bond=state.bond(&self.signer).ok_or("shared requester bond is absent")?;
        let digest=self.signing_message(network)?;
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            &bond.pubkey,digest.as_byte_slice(),&self.signature,SHARED_REQUEST_CONTEXT_V1) {
            return Err("shared requester signature is invalid".into());
        }
        Ok(())
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum SharedRequestEffectV1 { Opened,Coalesced }
#[derive(Clone,Debug,PartialEq,Eq)]
struct Entry { request_id:Hash64,query:Query,unit:Option<Unit> }
/// Deliberately NOT serializable. Capability-valued data must be reauthenticated
/// after a future native load. No producer DTO can deserialize an `Unit`/`Ids`.
/// Retention/heap/IO of the complete set still needs original funded admission.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SharedTirFrontierV1 {
    scope:Hash64,source:Hash64,queries:u64,decode_count:u32,
    requests:BTreeMap<u64,Entry>,ids:Option<Ids>,
}
pub enum SharedDataV1<'a> { Unit(&'a Unit),Ids {rows_root:Hash64,ids:&'a [u32]} }
impl SharedTirFrontierV1 {
    /// The caller authenticates the native frame/source/profile. This constructor
    /// is NOT a complete paid reservation or a production native ingress.
    pub fn new(set:&TirSharedRequestSetV1<'_>)->Result<Self,String> {
        Ok(Self {scope:set.scope_id()?,source:set.source_id(),queries:set.len(),decode_count:set.decode_count(),
            requests:BTreeMap::new(),ids:None})
    }
    fn check_scope(&self,set:&TirSharedRequestSetV1<'_>)->Result<(),String> {
        if self.scope!=set.scope_id()? || self.source!=set.source_id() || self.queries!=set.len() || self.decode_count!=set.decode_count() {
            return Err("shared frontier substitutes query universe".into());
        }
        Ok(())
    }
    pub fn request(&mut self,set:&TirSharedRequestSetV1<'_>,state:&PalwChainStateV2,network:Hash64,
        intent:&SignedSharedRequestV1)->Result<SharedRequestEffectV1,String> {
        self.check_scope(set)?;
        let query=set.request_at(intent.ordinal)?;let request_id=set.request_id(intent.ordinal)?;
        if intent.scope!=self.scope || intent.request_id!=request_id { return Err("shared request substitutes scope/coordinate".into()); }
        intent.validate_bond_signature(network,state)?;
        if self.requests.contains_key(&intent.ordinal) { return Ok(SharedRequestEffectV1::Coalesced); }
        self.requests.insert(intent.ordinal,Entry {request_id,query,unit:None});
        Ok(SharedRequestEffectV1::Opened)
    }
    /// Shares authenticated public data, NOT evidence that a particular signer
    /// fulfilled a service duty. No signature/bond/credit/default effect follows.
    pub fn share_authenticated_unit(&mut self,set:&TirSharedRequestSetV1<'_>,ordinal:u64,unit:Unit)->Result<bool,String> {
        self.check_scope(set)?;
        let query=set.request_at(ordinal)?;
        if unit.source_id_v1()!=self.source || unit.request_v1()!=query { return Err("shared data substitutes source/coordinate".into()); }
        let entry=self.requests.get_mut(&ordinal).ok_or("shared data has no signaled request")?;
        // Already authenticated alternative openings are not faults or new work.
        if entry.unit.is_some() { return Ok(false); }
        entry.unit=Some(unit);Ok(true)
    }
    pub fn share_sealed_ids(&mut self,set:&TirSharedRequestSetV1<'_>,ids:Ids)->Result<bool,String> {
        self.check_scope(set)?;
        if ids.source_id_v1()!=self.source || ids.ids_v1().len()!=self.decode_count as usize {
            return Err("shared IDs are not sealed against this exact original source".into());
        }
        if self.ids.is_some() { return Ok(false); }
        self.ids=Some(ids);Ok(true)
    }
    pub fn read<'a>(&'a self,set:&TirSharedRequestSetV1<'_>,ordinal:u64)->Result<Option<SharedDataV1<'a>>,String> {
        self.check_scope(set)?;set.request_at(ordinal)?;
        let Some(entry)=self.requests.get(&ordinal) else {return Ok(None)};
        if let Some(unit)=&entry.unit {return Ok(Some(SharedDataV1::Unit(unit)))}
        if let Query::Ids {offset}=entry.query {
            if let Some(ids)=&self.ids {
                let start=offset as usize;
                let count=ids.ids_v1().len().checked_sub(start).ok_or("shared ID offset exceeds sealed stream")?
                    .min(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize);
                let end=start.checked_add(count).ok_or("shared ID end overflow")?;
                return Ok(Some(SharedDataV1::Ids {rows_root:ids.rows_root_v1(),ids:&ids.ids_v1()[start..end]}));
            }
        }
        Ok(None)
    }
    pub fn requested_count(&self)->usize {self.requests.len()}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::{PalwBondStateV2,PalwBondStatusV2};
    use crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1 as fixture;
    fn h(n:u64)->Hash64 {Hash64::from_u64_word(n)}
    fn bond(n:u64)->PalwBondKeyV2 {PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n),0))}
    fn signer(state:&mut PalwChainStateV2,who:PalwBondKeyV2,key:&[u8]) {
        // Identity-only registration fixture, not a funded native admission.
        state.set_bond_for_tests(who,Some(PalwBondStateV2 {pubkey:key.to_vec(),operator_id:h(1),collateral:0,
            slashed:0,status:PalwBondStatusV2::Active,registered_daa:0,payout_payload:h(2),capable_classes:Default::default()}));
    }
    fn sign(set:&TirSharedRequestSetV1<'_>,ordinal:u64,who:PalwBondKeyV2,seed:u8)->SignedSharedRequestV1 {
        let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed;32]);
        let mut intent=SignedSharedRequestV1 {version:1,scope:set.scope_id().unwrap(),ordinal,
            request_id:set.request_id(ordinal).unwrap(),signer:who,signature:vec![]};
        intent.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,intent.signing_message(h(9)).unwrap().as_byte_slice(),
            SHARED_REQUEST_CONTEXT_V1,[seed+1;32]).unwrap().as_ref().to_vec();intent
    }
    fn ordinal(set:&TirSharedRequestSetV1<'_>,q:Query)->u64 {
        (0..set.len()).find(|&i|set.request_at(i).unwrap()==q).unwrap()
    }
    #[test]
    fn court_tir_shared_frontier_real_signatures_coalesce_without_exclusive_choices() {
        let fx=fixture(None,4);let source=fx.frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        let set=TirSharedRequestSetV1::new(&fx.frame,&source,h(8)).unwrap();let mut frontier=SharedTirFrontierV1::new(&set).unwrap();
        let mut state=PalwChainStateV2::genesis();
        for (who,seed) in [(bond(10),11),(bond(20),12)] {
            let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed;32]);signer(&mut state,who,kp.verification_key.as_ref());
        }
        let index=ordinal(&set,Query::StepLeaf {index:0});let first=sign(&set,index,bond(10),11);
        assert_eq!(frontier.request(&set,&state,h(9),&first).unwrap(),SharedRequestEffectV1::Opened);
        assert_eq!(frontier.request(&set,&state,h(9),&sign(&set,index,bond(20),12)).unwrap(),SharedRequestEffectV1::Coalesced);
        assert_eq!(frontier.requested_count(),1);assert!(frontier.read(&set,index).unwrap().is_none());
        // A different legal choice coexists; the first requester cannot close it.
        let next=ordinal(&set,Query::StepRun {first:0,count:1});
        assert_eq!(frontier.request(&set,&state,h(9),&sign(&set,next,bond(20),12)).unwrap(),SharedRequestEffectV1::Opened);
        let response=crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:fx.execution.preimages[0].clone(),opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,0).unwrap()};
        let unit=source.authenticate_v1(&Query::StepLeaf {index:0},&response,None).unwrap();
        assert!(frontier.share_authenticated_unit(&set,index,unit.clone()).unwrap());
        assert!(!frontier.share_authenticated_unit(&set,index,unit.clone()).unwrap());
        assert!(matches!(frontier.read(&set,index).unwrap(),Some(SharedDataV1::Unit(u)) if u==&unit));
        let saved=frontier.clone();
        assert!(frontier.share_authenticated_unit(&set,next,unit).is_err());assert_eq!(frontier,saved);
        let foreign=fixture(Some((7,1)),4);
        let other_source=foreign.frame.source_v1(&foreign.record,&foreign.layout,foreign.rules.maximum_leaves).unwrap();
        let foreign_response=crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:foreign.execution.preimages[0].clone(),opening:crate::palw_step_leg::step_opening_v1(&foreign.execution.hashes,0).unwrap()};
        let foreign_unit=other_source.authenticate_v1(&Query::StepLeaf {index:0},&foreign_response,None).unwrap();
        assert!(frontier.share_authenticated_unit(&set,index,foreign_unit).is_err());assert_eq!(frontier,saved);
        assert!(frontier.request(&set,&PalwChainStateV2::genesis(),h(9),&first).is_err());assert_eq!(frontier,saved);
        let mut wrong_context=first.clone();let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([11;32]);
        wrong_context.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,first.signing_message(h(9)).unwrap().as_byte_slice(),
            crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1,[12;32]).unwrap().as_ref().to_vec();
        assert!(frontier.request(&set,&state,h(9),&wrong_context).is_err());assert_eq!(frontier,saved);
        for change in 0..5 {
            let mut invalid=first.clone();let mut network=h(9);
            match change {0=>network=h(99),1=>invalid.signer=bond(20),2=>invalid.signature[0]^=1,3=>invalid.version=2,_=>invalid.ordinal=next}
            assert!(frontier.request(&set,&state,network,&invalid).is_err());assert_eq!(frontier,saved);
        }
        let other=TirSharedRequestSetV1::new(&fx.frame,&source,h(88)).unwrap();
        assert!(frontier.request(&other,&state,h(9),&sign(&other,index,bond(10),11)).is_err());
        assert_eq!(frontier,saved);
        // No native owner/court rows are needed by this in-memory primitive.
        // This is NOT a node T05 or paid-queue/admission test.
        assert!(state.staged_court_row_v1(&fx.frame.key_v1()).is_none());
    }
    #[test]
    fn court_tir_shared_frontier_never_promotes_provisional_ids_to_public_data() {
        use crate::palw_tir_public_source_v1::{PalwTirPublicIdsAssemblerV1,PalwTirPublicResponseV1 as Response};
        let fx=fixture(None,4);let source=fx.frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        let set=TirSharedRequestSetV1::new(&fx.frame,&source,h(8)).unwrap();let mut frontier=SharedTirFrontierV1::new(&set).unwrap();
        let mut state=PalwChainStateV2::genesis();let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([11;32]);
        signer(&mut state,bond(10),kp.verification_key.as_ref());
        let query=Query::Ids {offset:0};let index=ordinal(&set,query);
        frontier.request(&set,&state,h(9),&sign(&set,index,bond(10),11)).unwrap();
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();
        let response=Response::Ids {rows_root:rows,ids:fx.execution.generated.clone()};
        assert!(source.authenticate_v1(&query,&response,None).is_err());
        assert!(frontier.read(&set,index).unwrap().is_none());
        let sealed=PalwTirPublicIdsAssemblerV1::default().push_v1(&source,&query,&response).unwrap().unwrap();
        assert!(frontier.share_sealed_ids(&set,sealed.clone()).unwrap());
        assert!(!frontier.share_sealed_ids(&set,sealed).unwrap());
        assert!(matches!(frontier.read(&set,index).unwrap(),Some(SharedDataV1::Ids {rows_root,ids})
            if rows_root==rows && ids==fx.execution.generated.as_slice()));
        let foreign=fixture(Some((7,1)),4);let other=foreign.frame.source_v1(&foreign.record,&foreign.layout,foreign.rules.maximum_leaves).unwrap();
        let other_set=TirSharedRequestSetV1::new(&foreign.frame,&other,h(8)).unwrap();
        assert!(frontier.read(&other_set,index).is_err());
    }
    #[test]
    fn court_tir_shared_frontier_signed_intent_decoder_bounds_signature_before_allocation() {
        let fx=fixture(None,4);let source=fx.frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        let set=TirSharedRequestSetV1::new(&fx.frame,&source,h(8)).unwrap();let intent=sign(&set,0,bond(10),11);
        let bytes=borsh::to_vec(&intent).unwrap();assert_eq!(borsh::from_slice::<SignedSharedRequestV1>(&bytes).unwrap(),intent);
        let prefix=bytes.len()-SIG-4;
        for count in [0u32,1,u32::MAX] {
            let mut bad=bytes.clone();bad[prefix..prefix+4].copy_from_slice(&count.to_le_bytes());
            assert!(borsh::from_slice::<SignedSharedRequestV1>(&bad).is_err());
        }
        let mut trailing=bytes.clone();trailing.push(0);assert!(borsh::from_slice::<SignedSharedRequestV1>(&trailing).is_err());
        assert!(borsh::from_slice::<SignedSharedRequestV1>(&bytes[..bytes.len()-1]).is_err());
    }
}
