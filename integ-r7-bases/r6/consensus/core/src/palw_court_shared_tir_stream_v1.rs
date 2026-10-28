//! RFC-0029 isolated signed stream envelope. No lifecycle, rooted stream writer,
//! Ready authorization, paid admission, data authentication or duty completion.
//! The private rooted adapter is separate; this type alone grants no authority.
use crate::{Hash64,palw_state_v2::{PalwBondKeyV2,PalwChainStateV2}};
use borsh::{BorshDeserialize,BorshSerialize};
use std::io::{self,Read};
use crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as PART;
const SIG:usize=crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
pub const SHARED_STREAM_CONTEXT_V1:&[u8]=b"misaka-palw/court-shared-stream/v1/mldsa87";
#[derive(Clone,Debug,PartialEq,Eq,BorshDeserialize,BorshSerialize)]
pub enum SharedStreamTargetV1 {
    Unit {ordinal:u64,request_id:Hash64},
    /// One sequence per original scope, not one repeated sequence per ID chunk.
    FullIds,
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub enum SharedStreamActionV1 {
    Begin {total_bytes:u64,body_hash:Hash64},
    Part {stream_id:Hash64,offset:u64,bytes:Vec<u8>},
    Finish {stream_id:Hash64},
}
impl BorshDeserialize for SharedStreamActionV1 {
    fn deserialize_reader<R:Read>(r:&mut R)->io::Result<Self> {
        Ok(match u8::deserialize_reader(r)? {
            0=>Self::Begin {total_bytes:u64::deserialize_reader(r)?,body_hash:Hash64::deserialize_reader(r)?},
            1=>Self::Part {stream_id:Hash64::deserialize_reader(r)?,offset:u64::deserialize_reader(r)?,
                bytes:crate::palw_public_binding_v1::read_bounded_bytes_v1(r,PART)?},
            2=>Self::Finish {stream_id:Hash64::deserialize_reader(r)?},
            _=>return Err(io::Error::new(io::ErrorKind::InvalidData,"shared stream action tag")),
        })
    }
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct SignedSharedStreamV1 {
    pub version:u16,pub claim:Hash64,pub scope:Hash64,pub target:SharedStreamTargetV1,
    pub signer:PalwBondKeyV2,pub action:SharedStreamActionV1,pub signature:Vec<u8>,
}
impl BorshDeserialize for SignedSharedStreamV1 {
    fn deserialize_reader<R:Read>(r:&mut R)->io::Result<Self> {
        let version=u16::deserialize_reader(r)?;
        if version!=1 {return Err(io::Error::new(io::ErrorKind::InvalidData,"shared stream version"));}
        let claim=Hash64::deserialize_reader(r)?;let scope=Hash64::deserialize_reader(r)?;
        let target=SharedStreamTargetV1::deserialize_reader(r)?;let signer=PalwBondKeyV2::deserialize_reader(r)?;
        let action=SharedStreamActionV1::deserialize_reader(r)?;let n=u32::deserialize_reader(r)? as usize;
        if n!=SIG {return Err(io::Error::new(io::ErrorKind::InvalidData,"shared stream signature length"));}
        let mut signature=vec![0;SIG];r.read_exact(&mut signature)?;
        Ok(Self {version,claim,scope,target,signer,action,signature})
    }
}
/// Borsh directly into the hash state: no temporary copy of Part bytes.
pub(crate) fn stream_digest<T:BorshSerialize>(domain:&[u8],value:&T)->Result<Hash64,String> {
    struct Sink<'a>(&'a mut blake2b_simd::State);
    impl std::io::Write for Sink<'_> {
        fn write(&mut self,bytes:&[u8])->io::Result<usize> {self.0.update(bytes);Ok(bytes.len())}
        fn flush(&mut self)->io::Result<()> {Ok(())}
    }
    let mut hash=blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    value.serialize(&mut Sink(&mut hash)).map_err(|e|e.to_string())?;
    Ok(Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64")))
}
impl SignedSharedStreamV1 {
    fn check_shape(&self)->Result<(),String> {
        if self.version!=1 {return Err("shared stream version".into());}
        match &self.action {
            SharedStreamActionV1::Begin {total_bytes:0,..}=>Err("empty shared stream".into()),
            SharedStreamActionV1::Part {bytes,..} if bytes.is_empty() || bytes.len()>PART=>Err("shared stream part bound".into()),
            _=>Ok(()),
        }
    }
    /// Fixed per actor slot. A different declaration/nonce/DAA cannot allocate
    /// another slot. Native authorization and immutable Begin remain required.
    pub fn slot_id(&self)->Result<Hash64,String> {
        self.check_shape()?;
        stream_digest(b"misaka-palw/shared-stream-slot/v1",&(self.version,self.claim,self.scope,&self.target,self.signer))
    }
    pub fn declared_stream_id(&self,total_bytes:u64,body_hash:Hash64)->Result<Hash64,String> {
        if total_bytes==0 {return Err("empty shared stream".into());}
        stream_digest(b"misaka-palw/shared-stream-body/v1",&(self.slot_id()?,total_bytes,body_hash))
    }
    pub fn signing_message(&self,network:Hash64)->Result<Hash64,String> {
        self.check_shape()?;
        stream_digest(b"misaka-palw/shared-stream-control/v1",&(network,self.version,self.claim,self.scope,&self.target,self.signer,&self.action))
    }
    /// Only authenticates the bond key. Does NOT authorize a current Ready
    /// custodian, original source/profile, stream cursor, window or paid work.
    pub fn validate_bond_signature(&self,network:Hash64,state:&PalwChainStateV2)->Result<(),String> {
        if self.signature.len()!=SIG {return Err("shared stream signature shape".into());}
        let bond=state.bond(&self.signer).ok_or("shared stream signer bond absent")?;
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            &bond.pubkey,self.signing_message(network)?.as_byte_slice(),&self.signature,SHARED_STREAM_CONTEXT_V1) {
            return Err("shared stream signature invalid".into());
        }
        Ok(())
    }
}
/// Pure checked geometry, not evidence of an accepted or authenticated prefix.
/// The caller supplies the retained declaration/cursor, never a producer quote.
pub fn check_shared_stream_part_v1(total:u64,accepted:u64,offset:u64,len:usize)->Result<u64,String> {
    let remaining=total.checked_sub(accepted).ok_or("shared cursor exceeds declaration")?;
    if offset!=accepted || remaining==0 || len as u64!=remaining.min(PART as u64) {
        return Err("shared stream noncanonical part".into());
    }
    accepted.checked_add(len as u64).ok_or_else(||"shared stream cursor overflow".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n:u64)->Hash64 {Hash64::from_u64_word(n)}
    fn control()->SignedSharedStreamV1 {
        SignedSharedStreamV1 {version:1,claim:h(1),scope:h(2),target:SharedStreamTargetV1::Unit {ordinal:3,request_id:h(4)},
            signer:PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(5),0)),
            action:SharedStreamActionV1::Begin {total_bytes:17_000,body_hash:h(6)},signature:vec![0;SIG]}
    }
    #[test]
    fn court_tir_shared_stream_slot_does_not_multiply_with_declarations() {
        let a=control();let slot=a.slot_id().unwrap();let stream=a.declared_stream_id(17_000,h(6)).unwrap();
        let mut b=a.clone();b.action=SharedStreamActionV1::Begin {total_bytes:99,body_hash:h(7)};
        assert_eq!(slot,b.slot_id().unwrap());assert_ne!(stream,b.declared_stream_id(99,h(7)).unwrap());
        b.action=SharedStreamActionV1::Part {stream_id:stream,offset:0,bytes:vec![0;PART]};assert_eq!(slot,b.slot_id().unwrap());
        b.action=SharedStreamActionV1::Finish {stream_id:stream};assert_eq!(slot,b.slot_id().unwrap());
        for n in 0..4 {let mut b=a.clone();match n {0=>b.claim=h(9),1=>b.scope=h(9),2=>b.target=SharedStreamTargetV1::FullIds,_=>b.signer.0.index=1};assert_ne!(slot,b.slot_id().unwrap());}
    }
    #[test]
    fn court_tir_shared_stream_geometry_retains_existing_part_ceiling() {
        assert_eq!(check_shared_stream_part_v1(17_000,0,0,PART).unwrap(),16_384);
        assert_eq!(check_shared_stream_part_v1(17_000,16_384,16_384,616).unwrap(),17_000);
        for (t,a,o,n) in [(17_000,0,0,PART+1),(17_000,0,0,1),(17_000,1,0,PART),(0,0,0,0),(1,2,2,1),(1,1,1,0)] {
            assert!(check_shared_stream_part_v1(t,a,o,n).is_err());
        }
        assert_eq!(check_shared_stream_part_v1(u64::MAX,u64::MAX-1,u64::MAX-1,1).unwrap(),u64::MAX);
    }
    #[test]
    fn court_tir_shared_stream_codec_rejects_lengths_before_reading_allocated_payload() {
        let mut c=control();let stream=c.declared_stream_id(17_000,h(6)).unwrap();
        for action in [c.action.clone(),SharedStreamActionV1::Part {stream_id:stream,offset:0,bytes:vec![7;PART]},SharedStreamActionV1::Finish {stream_id:stream}] {
            c.action=action;let wire=borsh::to_vec(&c).unwrap();assert_eq!(borsh::from_slice::<SignedSharedStreamV1>(&wire).unwrap(),c);
            assert!(borsh::from_slice::<SignedSharedStreamV1>(&wire[..wire.len()-1]).is_err());
            let mut trailing=wire.clone();trailing.push(0);assert!(borsh::from_slice::<SignedSharedStreamV1>(&trailing).is_err());
            let sig_at=wire.len()-SIG-4;
            for n in [0,u32::MAX] {let mut bad=wire[..sig_at].to_vec();bad.extend_from_slice(&n.to_le_bytes());assert_eq!(borsh::from_slice::<SignedSharedStreamV1>(&bad).unwrap_err().kind(),io::ErrorKind::InvalidData);}
        }
        // Only tag+ID+offset+length are present. Oversize rejects on length,
        // rather than trying to read/allocate a declared 4 GiB payload.
        let mut prefix=vec![1];prefix.extend(borsh::to_vec(&stream).unwrap());prefix.extend(0u64.to_le_bytes());prefix.extend(u32::MAX.to_le_bytes());
        assert_eq!(SharedStreamActionV1::deserialize_reader(&mut prefix.as_slice()).unwrap_err().kind(),io::ErrorKind::InvalidData);
        assert!(SignedSharedStreamV1::deserialize_reader(&mut 2u16.to_le_bytes().as_slice()).is_err());
    }
    #[test]
    fn court_tir_shared_stream_real_signature_binds_every_control_field() {
        use crate::palw_state_v2::{PalwBondStateV2,PalwBondStatusV2};
        let mut state=PalwChainStateV2::genesis();let mut c=control();let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([21;32]);
        state.set_bond_for_tests(c.signer,Some(PalwBondStateV2 {pubkey:kp.verification_key.as_ref().to_vec(),operator_id:h(8),collateral:0,
            slashed:0,status:PalwBondStatusV2::Active,registered_daa:0,payout_payload:h(9),capable_classes:Default::default()}));
        let stream=c.declared_stream_id(17_000,h(6)).unwrap();
        for action in [c.action.clone(),SharedStreamActionV1::Part {stream_id:stream,offset:0,bytes:vec![7;PART]},SharedStreamActionV1::Finish {stream_id:stream}] {
            c.action=action;c.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,c.signing_message(h(10)).unwrap().as_byte_slice(),
                SHARED_STREAM_CONTEXT_V1,[22;32]).unwrap().as_ref().to_vec();
            c.validate_bond_signature(h(10),&state).unwrap();
            // Preserve the previous byte-serialization message identity while
            // hashing directly from Borsh avoids copying Part payloads.
            let baseline=borsh::to_vec(&(h(10),c.version,c.claim,c.scope,&c.target,c.signer,&c.action)).unwrap();
            assert_eq!(c.signing_message(h(10)).unwrap().as_byte_slice(),blake2b_simd::Params::new().hash_length(64)
                .key(b"misaka-palw/shared-stream-control/v1").hash(&baseline).as_bytes());

            for n in 0..8 {let mut b=c.clone();let mut network=h(10);match n {
                0=>network=h(11),1=>b.claim=h(11),2=>b.scope=h(11),3=>b.target=SharedStreamTargetV1::FullIds,
                4=>b.signer.0.index=1,5=>b.version=2,6=>b.signature[0]^=1,_=>b.action=SharedStreamActionV1::Begin {total_bytes:1,body_hash:h(11)}}
                assert!(b.validate_bond_signature(network,&state).is_err());
            }
            // Bind fields within each variant, including content and position.
            for n in 0..3 {
                let mut b=c.clone();
                match &mut b.action {
                    SharedStreamActionV1::Begin {total_bytes,body_hash}=>if n==0 {*total_bytes+=1} else {*body_hash=h(99)},
                    SharedStreamActionV1::Part {stream_id,offset,bytes}=>match n {0=>*stream_id=h(99),1=>*offset+=1,_=>bytes[0]^=1},
                    SharedStreamActionV1::Finish {stream_id}=>*stream_id=h(99),
                }
                assert!(b.validate_bond_signature(h(10),&state).is_err());
            }
            for n in 0..2 {let mut b=c.clone();b.target=if n==0 {SharedStreamTargetV1::Unit {ordinal:4,request_id:h(4)}}
                else {SharedStreamTargetV1::Unit {ordinal:3,request_id:h(99)}};
                assert!(b.validate_bond_signature(h(10),&state).is_err());}
            let mut wrong=c.clone();wrong.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,c.signing_message(h(10)).unwrap().as_byte_slice(),
                crate::palw_court_shared_tir_frontier_v1::SHARED_REQUEST_CONTEXT_V1,[22;32]).unwrap().as_ref().to_vec();
            assert!(wrong.validate_bond_signature(h(10),&state).is_err());
        }
    }
}
