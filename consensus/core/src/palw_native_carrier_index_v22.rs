//! Node scheduling metadata for original paid native carriers. Parsing a kind
//! does not grant priority: the original fixed native rows and the complete
//! populated transaction must independently qualify at the template snapshot.
use crate::{
    palw_flat_execution_capacity_v22::{NativeCarriageServiceV22, NativePaidCarrierEligibilityV22, NativePaidCarrierRequestV22},
    palw_kernel_route_v1::PalwKernelChunkTargetV1,
    palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2,
    palw_native_carriage_v22::NativeCarriageMassParamsV22,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2},
    tx::{Transaction, VerifiableTransaction},
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCarrierCandidateV22 {
    pub namespace: u8,
    pub request: NativePaidCarrierRequestV22,
    pub owner: PalwBondKeyV2,
}
impl NativeCarrierCandidateV22 {
    /// One physical unit/part, independent of transaction fee, signature
    /// encoding or a repost's UTXO. A selector takes at most one variant per
    /// unit in a block, never many variants against an immutable old counter.
    pub fn key(&self) -> (u8, u8, [u8; 64], u16) {
        let (kind, id, part) = match self.request {
            NativePaidCarrierRequestV22::ProducerAnswerPartV29 {pending,index,..}=>(4,pending,index),
            NativePaidCarrierRequestV22::FundedCommitPartV32 {job,index,..}=>(5,job,index),
            NativePaidCarrierRequestV22::FundedCommitAssembleV32 {job,..}=>(6,job,0),
            NativePaidCarrierRequestV22::SourceAnswer { pending } => (0, pending, 0),
            NativePaidCarrierRequestV22::TraceAnswer { pending } => (1, pending, 0),
            NativePaidCarrierRequestV22::OwnProofPart { seal, index, .. } => (2, seal, index as u16),
            NativePaidCarrierRequestV22::OwnProofPartV29 { seal, index, .. } => (3, seal, index),
        };
        (self.namespace, kind, id, part)
    }
}

/// A singleton lifecycle payload is the same object extraction used by the
/// fold. This reads bounded carrier metadata, not nested PC/proof material.
pub fn native_carrier_candidate_v22(tx: &Transaction) -> Option<NativeCarrierCandidateV22> {
    // A complete paid source packet is bounded independently of the route's
    // much larger raw codec ceiling. This is only a cheap parser ceiling; the
    // actual original envelope is verified below, including contextual mass.
    if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        || tx.payload.len() > 128 << 10
        || tx.payload.get(..2)? != crate::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2.to_le_bytes()
        || !matches!(tx.payload.get(2), Some(110 | 113))
    {
        return None;
    }
    let mut extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx));
    if !extracted.skipped.is_empty() || extracted.objects.len() != 1 {
        return None;
    }
    match extracted.objects.pop()?.object {
        PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, .. } => {
            let a = misaka_palw_kernel::route::borrowed_native_action_v1(&bytes).ok()??;
            let pending = a.body.get(..64)?.try_into().ok()?;
            let request = match (a.tag, a.action) {
                (misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11, 15)
                | (misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14, 8)
                | (misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14, 26) => NativePaidCarrierRequestV22::SourceAnswer { pending },
                (misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11, 8) => NativePaidCarrierRequestV22::TraceAnswer { pending },
                (misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11, 32) => {
                    let part = crate::palw_native_paged_proof_v29::borrowed_native_paged_metadata_v29(a.body)?;
                    if part.owner != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&signer) {
                        return None;
                    }
                    NativePaidCarrierRequestV22::OwnProofPartV29 {
                        job: part.job,
                        seal: part.seal,
                        owner: part.owner,
                        index: part.index,
                        count: part.count,
                    }
                }
                (misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14,39)=>{
                    let p=crate::palw_flat_stage_v28::borrowed_invocation_commit_metadata_v32(a.body)?;
                    NativePaidCarrierRequestV22::FundedCommitPartV32{job:p.job,group:p.group,index:p.index,count:p.count,total:p.total}
                }
                (misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14,40)=>{
                    if a.body.len()!=130||a.body[..2]!=32u16.to_le_bytes(){return None;}
                    NativePaidCarrierRequestV22::FundedCommitAssembleV32{job:a.body[2..66].try_into().ok()?,group:a.body[66..130].try_into().ok()?}
                }
                (misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11,37)
                |(misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14,32)=>{
                    let p=crate::palw_native_reply_parts_v29::borrowed_native_reply_metadata_v29(a.body)?;
                    NativePaidCarrierRequestV22::ProducerAnswerPartV29 {
                        pending:p.pending,group:p.group,index:p.index,count:p.count,total:p.total,
                    }
                }
                _ => return None,
            };
            Some(NativeCarrierCandidateV22 { namespace: a.tag, request, owner: signer })
        }
        PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, .. } => {
            if chunk.count == 0
                || chunk.count > 64
                || chunk.index >= chunk.count
                || chunk.bytes.is_empty()
                || chunk.bytes.len() > 32 << 10
            {
                return None;
            }
            let (namespace, seal, actor) = match chunk.target {
                PalwKernelChunkTargetV1::FundedProofV11 { seal, actor } => {
                    (misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11, seal, actor)
                }
                PalwKernelChunkTargetV1::StageProofV14 { seal, actor } => {
                    (misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14, seal, actor)
                }
                _ => return None,
            };
            if actor != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&chunk.opener) {
                return None;
            }
            Some(NativeCarrierCandidateV22 {
                namespace,
                owner: chunk.opener,
                request: NativePaidCarrierRequestV22::OwnProofPart { seal, index: chunk.index, count: chunk.count },
            })
        }
        _ => None,
    }
}

/// The consumer creates this from its exact selected-parent state while holding
/// the template's virtual snapshot. Neither a transaction nor a mining selector
/// may replace the state, policy constants or clock.
#[derive(Clone)]
pub struct NativeCarriageSnapshotV22 {
    pub now: u64,
    pub selected_parent_daa: u64,
    pub mass: NativeCarriageMassParamsV22,
    pub service: NativeCarriageServiceV22,
    pub queue_policy: Option<crate::palw_native_queue_v30::NativeQueuePolicyV30>,
    state: Arc<crate::palw_native_resource_v1::NativeChainStateV1>,
}
impl NativeCarriageSnapshotV22 {
    pub fn from_consumer_state(
        now: u64,
        selected_parent_daa: u64,
        state: Arc<crate::palw_native_resource_v1::NativeChainStateV1>,
        mass: NativeCarriageMassParamsV22,
        service: NativeCarriageServiceV22,
    ) -> Result<Self, String> {
        service.check(&mass)?;
        if selected_parent_daa > now {
            return Err("native carriage snapshot clock precedes its selected parent".into());
        }
        Ok(Self { now, selected_parent_daa, state, mass, service, queue_policy: None })
    }
    pub fn with_queue_policy_v30(mut self, policy: crate::palw_native_queue_v30::NativeQueuePolicyV30) -> Result<Self, String> {
        policy.check(policy.network_domain)?;
        self.queue_policy = Some(policy);
        Ok(self)
    }
    pub fn is_service_turn(&self) -> bool {
        // Scores can skip an exact multiple. Serve the first template crossing
        // a service boundary as well, instead of waiting forever on odd scores.
        // This does not prove a network-wide bound on DAA advancement or miner
        // censorship; those remain separate completion evidence.
        self.service.check(&self.mass).is_ok()
            && self.service.implemented
            && (self.now.is_multiple_of(self.service.period_daa)
                || self.selected_parent_daa / self.service.period_daa < self.now / self.service.period_daa)
    }
    pub fn eligibility(&self, c: &NativeCarrierCandidateV22) -> Option<NativePaidCarrierEligibilityV22> {
        // Stage's separate physical consent/lease must be connected before its
        // carriers can enter this lane. Sharing a tag or source pending ID is
        // never enough to reinterpret a text native23 lease as a stage grant.
        let route=self.state.kernel_route()?;
        let e=match c.namespace {
            misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11=>
                route.native_paid_carrier_eligibility_v22(&c.request,self.now,&self.mass,&self.service)?,
            misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14=>match &c.request {
                NativePaidCarrierRequestV22::SourceAnswer{pending}=>
                    route.stage_paid_answer_eligibility_v28(*pending,self.now,&self.mass,&self.service)?,
                NativePaidCarrierRequestV22::ProducerAnswerPartV29 {pending,group,index,count,total}=>
                    route.stage_paid_reply_eligibility_v28(*pending,*group,*index,*count,*total,self.now,&self.mass,&self.service)?,
                NativePaidCarrierRequestV22::FundedCommitPartV32 {job,group,index,count,total}=>
                    route.native_paid_invocation_commit_eligibility_v32(*job,*group,*index,*count,*total,self.now,&self.mass,&self.service)?,
                NativePaidCarrierRequestV22::FundedCommitAssembleV32 {job,group}=>
                    route.native_paid_invocation_assemble_eligibility_v32(*job,*group,self.now,&self.mass,&self.service)?,
                _=>return None,
            },
            _=>return None,
        };
        (e.owner == c.owner && e.deadline >= self.now && e.original_service == self.service.root()).then_some(e)
    }
    pub fn qualify(&self, c: &NativeCarrierCandidateV22, tx: &impl VerifiableTransaction) -> Option<NativePaidCarrierEligibilityV22> {
        let e = self.eligibility(c)?;
        self.mass.check_transaction(&e.envelope, tx).ok()?;
        if let Some(policy) = &self.queue_policy {
            policy.check_transaction(&e.envelope, &self.mass, tx).ok()?;
            if (0..tx.tx().inputs.len()).any(|i| tx.utxo(i).is_none_or(|e| e.block_daa_score > self.selected_parent_daa)) {
                return None;
            }
        } else if matches!(c.request, NativePaidCarrierRequestV22::OwnProofPartV29 { .. }
            |NativePaidCarrierRequestV22::ProducerAnswerPartV29 { .. }
            |NativePaidCarrierRequestV22::FundedCommitPartV32 { .. }
            |NativePaidCarrierRequestV22::FundedCommitAssembleV32 { .. }) {
            return None;
        }
        Some(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_kernel_route_v1::{PalwKernelChunkV1, palw_kernel_bond_id_v1},
        palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2},
        tx::TransactionOutpoint,
    };
    use kaspa_hashes::Hash64;
    fn owner() -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([7; 64]), 0))
    }
    fn tx(object: PalwConsensusObjectV2) -> Transaction {
        Transaction::new(
            0,
            vec![],
            vec![],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap(),
        )
    }
    #[test]
    fn compact_commit32_metadata_keeps_one_original_part_identity(){
        let mut body=Vec::new();body.extend_from_slice(&32u16.to_le_bytes());body.push(0);body.extend_from_slice(&[21;64]);body.extend_from_slice(&[22;64]);body.extend_from_slice(&0u16.to_le_bytes());body.extend_from_slice(&1u16.to_le_bytes());body.extend_from_slice(&3u32.to_le_bytes());body.extend_from_slice(&u32::MAX.to_le_bytes());body.extend_from_slice(&[1,2,3]);
        let frame=|b:Vec<u8>|{let mut x=vec![1,misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14,39];x.extend_from_slice(&(b.len() as u32).to_le_bytes());x.extend(b);x};
        let candidate=|b:Vec<u8>|native_carrier_candidate_v22(&tx(PalwConsensusObjectV2::KernelRouteV1{bytes:frame(b),signer:owner(),signature:vec![1]})).unwrap();
        let a=candidate(body.clone());assert_eq!(a.key(),(64,5,[21;64],0));
        // Hostile nested part length is left for original paid predebit.
        body[67..131].fill(23);let b=candidate(body);assert_eq!(a.key(),b.key());
        let assembly=NativeCarrierCandidateV22{namespace:64,owner:owner(),request:NativePaidCarrierRequestV22::FundedCommitAssembleV32{job:[21;64],group:[22;64]}};
        assert_ne!(a.key(),assembly.key());
    }
    #[test]
    fn native_source_metadata_is_versioned_exact_and_cannot_decode_unrelated_objects() {
        let mut frame = vec![1, 63, 15];
        frame.extend_from_slice(&68u32.to_le_bytes());
        frame.extend_from_slice(&[9; 64]);
        // Hostile nested opening length is not decoded by metadata selection.
        frame.extend_from_slice(&u32::MAX.to_le_bytes());
        let carrier = tx(PalwConsensusObjectV2::KernelRouteV1 { bytes: frame.clone(), signer: owner(), signature: vec![1] });
        let c = native_carrier_candidate_v22(&carrier).unwrap();
        assert_eq!(c.request, NativePaidCarrierRequestV22::SourceAnswer { pending: [9; 64] });
        assert_eq!(c.owner, owner());
        frame.push(0);
        assert!(
            native_carrier_candidate_v22(&tx(PalwConsensusObjectV2::KernelRouteV1 {
                bytes: frame,
                signer: owner(),
                signature: vec![1],
            }))
            .is_none(),
            "a trailing byte cannot yield a scheduling identity"
        );
        let mut wrong = carrier.clone();
        wrong.payload[0] += 1;
        assert!(native_carrier_candidate_v22(&wrong).is_none());
        wrong = carrier;
        wrong.payload[2] = 104;
        assert!(native_carrier_candidate_v22(&wrong).is_none(), "unrelated objects are skipped before their decoder");
    }
    #[test]
    fn proof_variants_bind_original_namespace_owner_and_part_not_transaction_encoding() {
        let seal = [3; 64];
        let mut chunk = PalwKernelChunkV1 {
            opener: owner(),
            group: Hash64::from_bytes([4; 64]),
            target: PalwKernelChunkTargetV1::FundedProofV11 { seal, actor: palw_kernel_bond_id_v1(&owner()) },
            index: 0,
            count: 2,
            bytes: vec![1],
        };
        let object = |c: PalwKernelChunkV1| PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(c), signature: vec![1] };
        let c = native_carrier_candidate_v22(&tx(object(chunk.clone()))).unwrap();
        chunk.group = Hash64::from_bytes([5; 64]);
        chunk.bytes = vec![2];
        assert_eq!(c.key(), native_carrier_candidate_v22(&tx(object(chunk.clone()))).unwrap().key());
        chunk.index = 1;
        assert_ne!(c.key(), native_carrier_candidate_v22(&tx(object(chunk.clone()))).unwrap().key());
        chunk.target = PalwKernelChunkTargetV1::StageProofV14 { seal, actor: palw_kernel_bond_id_v1(&owner()) };
        assert_ne!(c.key(), native_carrier_candidate_v22(&tx(object(chunk.clone()))).unwrap().key());
        chunk.target = PalwKernelChunkTargetV1::StageProofV14 { seal, actor: [0; 64] };
        assert!(native_carrier_candidate_v22(&tx(object(chunk))).is_none());
    }

    #[test]
    fn paged29_indices_above_byte_range_are_distinct_and_bad_nested_lengths_remain_paid_metadata() {
        let frame = |index: u16, count: u16, declared: u32| {
            let mut body = 29u16.to_le_bytes().to_vec();
            body.extend_from_slice(&[3; 64]);
            body.extend_from_slice(&[9; 64]);
            body.extend_from_slice(&palw_kernel_bond_id_v1(&owner()));
            body.extend_from_slice(&index.to_le_bytes());
            body.extend_from_slice(&count.to_le_bytes());
            body.extend_from_slice(&declared.to_le_bytes());
            body.push(7);
            let mut bytes = vec![1, 63, 32];
            bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&body);
            tx(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: owner(), signature: vec![1] })
        };
        let first = native_carrier_candidate_v22(&frame(0, 512, 1)).unwrap();
        let high = native_carrier_candidate_v22(&frame(256, 512, 1)).unwrap();
        assert_ne!(first.key(), high.key(), "a new u16 part never aliases an old byte index");
        assert_eq!(high.key().3, 256);
        assert_eq!(
            high.key(),
            native_carrier_candidate_v22(&frame(256, 512, u32::MAX)).unwrap().key(),
            "hostile nested lengths are charged by the original paid fold after ownership, not decoded by this parser"
        );
        assert!(native_carrier_candidate_v22(&frame(512, 512, 1)).is_none());
        assert!(native_carrier_candidate_v22(&frame(0, 513, 1)).is_none());
        assert!(native_carrier_candidate_v22(&frame(0, 0, 1)).is_none());
    }
}
