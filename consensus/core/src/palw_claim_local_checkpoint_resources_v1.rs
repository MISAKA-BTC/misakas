//! Borrowed RFC-0023 checkpoint construction inventory. Codec lengths fund byte
//! vectors only; maps, nested clones and growing arrays are counted by type.
use super::*;
use crate::palw_native_resource_v1::{BTreeNodeEnvelopeV1, NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1};

fn overflow() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<PalwClaimLocalCheckpointV1>("checkpoint construction")
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(overflow)
}
fn encoded<T: borsh::BorshSerialize>(value: &T) -> Result<u64, NativeCoverageV1> {
    borsh::object_length(value).map(|n| n as u64).map_err(|_| overflow())
}
fn bytes(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), NativeCoverageV1> {
    // borsh::to_vec starts with 1024 bytes and grows geometrically. Include
    // both old and new allocations at a growth boundary, even if realloc moves.
    q.exact_v1::<u8>(n.max(1024).checked_mul(3).ok_or_else(overflow)?)
}

fn growing<T>(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), NativeCoverageV1> {
    if n == 0 {
        return Ok(());
    }
    let width = std::mem::size_of::<T>();
    let minimum = if width == 1 {
        8
    } else if width <= 1024 {
        4
    } else {
        1
    };
    q.exact_v1::<T>(n.max(minimum).checked_mul(3).ok_or_else(overflow)?)
}

pub(super) fn requests(state: &PalwChainStateV2) -> Result<u64, NativeCoverageV1> {
    if cfg!(not(target_pointer_width = "64")) {
        return Err(NativeCoverageV1::unsupported::<PalwClaimLocalCheckpointV1>(
            "checkpoint construction trees",
            "pinned BTree layout requires 64-bit",
        ));
    }
    let mut q = TirNativeRequestsV1::new();
    let claims = state.claims.len() as u64;
    let courts = state.court_sessions.len() as u64;
    let frames = rewards::frames(state).count() as u64;
    q.exact_v1::<BTreeNodeEnvelopeV1<(u64, u64), Hash64>>(frames)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<(Hash64, PalwCourtSessionStateV2)>>>(courts)?;
    // Initial grouping uses growing Vecs (minimum 4 slots); subsequent per-claim
    // clones have exact length. Include old+new arrays at a growth boundary:
    // twelve slots per session bounds grouping, plus one exact clone slot.
    q.exact_v1::<(Hash64, PalwCourtSessionStateV2)>(courts.checked_mul(13).ok_or_else(overflow)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1, PalwClaimLocalRecordV1>>(claims)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, Vec<u8>>>(add(claims, courts)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64, ()>>(add(claims, courts)?)?;
    growing::<(Hash64, PalwClaimStateV2, Vec<(Hash64, PalwCourtSessionStateV2)>)>(&mut q, claims)?;
    growing::<Hash64>(&mut q, claims)?;
    // Two Vec length prefixes. Count every claim/court as open, independently
    // of readiness; a failed/partial state cannot borrow a semantic invariant.
    let mut liability_bytes = 8u64;
    for claim in state.claims.values() {
        let heap = claim.native_heap_v1("checkpoint liability claim copy");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
        let wire = encoded(claim)?;
        bytes(&mut q, wire)?;
        // id + claim + court Vec prefix + one birth frame id.
        liability_bytes = add(liability_bytes, add(wire, 64 + 4 + 64)?)?;
    }
    for session in state.court_sessions.values() {
        // The grouping and retained liability copy coexist. Serialization
        // borrows the session and does not create a third deep clone.
        let heap = session.native_heap_v1("checkpoint court copies").scale::<PalwCourtSessionStateV2>(2, "court copies");
        q.add_v1(heap.bytes().ok_or(heap)?)?;
        let wire = add(64, encoded(session)?)?;
        bytes(&mut q, wire)?;
        liability_bytes = add(liability_bytes, wire)?;
    }
    bytes(&mut q, liability_bytes)?;
    // One retained copy of every historical frame and both of its carriages.
    // Nested target/source/retirement readers reserve their own copies; the
    // test quota allows four of those sets plus 32MiB of construction workspace.
    // Malformed frame bytes are skipped here so capacity admission still happens
    // before the fold reports the original inconsistency.
    q.add_v1(retained_history_v1(state)?)?;
    quote_carrier_input_nodes_v9(state, &mut q)?;
    quote_chain_material_v10(state, &mut q)?;
    quote_cohort_material_v15(state, &mut q)?;
    quote_current_material_v18(state,&mut q)?;
    quote_kernel_dependency_rows_v1(state, &mut q)?;
    quote_native_vertices_v1(state,&mut q)?;
    Ok(q.bytes_v1())
}

// The scoped native fact row survives collection of F1. Its projected input
// nodes must remain admitted even when retained_history_v1 has no frame to count.
// This is local construction capacity, not a signed storage/service payment.
fn quote_carrier_input_nodes_v9(state:&PalwChainStateV2,q:&mut TirNativeRequestsV1)->Result<(),NativeCoverageV1>{
    let quote=(||->Result<(),PalwStateV2Error>{
        use super::super::palw_claim_local_carrier_lineage_v9 as lineage;
        for (id,claim) in &state.claims {
            let Some(source)=super::super::palw_claim_local_source_admission_v8::for_stored_claim_v8(state,*id,claim)? else{continue};
            lineage::visit_consumer_facts_v9(state,*id,source.accepting_point(),|_,owner|{
                if lineage::is_exact_live_source_v9(state,&owner){
                    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,()>>(1).map_err(PalwStateV2Error::NativeResource)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })();
    // Preserve the actual constructor's semantic fault after capacity admission.
    // Resource faults from authentic typed readers stay local Resource faults.
    match quote{Err(PalwStateV2Error::NativeResource(e))=>Err(e),_=>Ok(())}
}

fn io_resource(error: std::io::Error) -> Option<NativeCoverageV1> {
    match error.kind() {
        std::io::ErrorKind::OutOfMemory => Some(overflow()),
        std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut => Some(NativeCoverageV1::unsupported::<
            crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1,
        >("checkpoint historical scan", &error.to_string())),
        _ => None,
    }
}

fn retained_history_v1(state: &PalwChainStateV2) -> Result<u64, NativeCoverageV1> {
    use crate::palw_claim_local_v1::{PalwClaimLiabilityFrameV1, PalwClaimLocalInputV1, decode_claim_local_value_v1};
    use crate::palw_native_resource_v1::{NativeDecodeErrorV1, preflight_native_carriage_v1, preflight_native_value_v1};
    let mut total = 0u64;
    let mut prefix_edges = 0u64;
    for (_, bytes) in rewards::frames(state) {
        let shape = match preflight_native_value_v1::<PalwClaimLiabilityFrameV1>(bytes.as_slice(), bytes.len() as u64) {
            Ok(shape) => shape,
            Err(error) => {
                if let Some(fault) = io_resource(error) {
                    return Err(fault);
                }
                continue;
            }
        };
        total = add(total, shape.logical_requests_v1())?;
        let frame = match decode_claim_local_value_v1::<PalwClaimLiabilityFrameV1>(bytes) {
            Ok(frame) => frame,
            Err(NativeDecodeErrorV1::Resource(fault)) => return Err(fault),
            Err(NativeDecodeErrorV1::Decode(_)) => continue,
        };
        prefix_edges = add(
            prefix_edges,
            frame.inputs.iter().filter(|input| matches!(input, PalwClaimLocalInputV1::PrefixDependency { .. } | PalwClaimLocalInputV1::ConsumedCarrier { .. })).count() as u64,
        )?;
        for carriage in [&frame.prior_state, &frame.current_state] {
            match preflight_native_carriage_v1(carriage.as_slice(), carriage.len() as u64) {
                Ok(scan) => {
                    let memory = match scan.memory_requests_v1() {
                        Ok(memory) => memory,
                        Err(error) => {
                            if let Some(fault) = io_resource(error) {
                                return Err(fault);
                            }
                            continue;
                        }
                    };
                    total = add(total, memory)?;
                }
                Err(error) => {
                    if let Some(fault) = io_resource(error) {
                        return Err(fault);
                    }
                }
            }
        }
    }
    // Input-set nodes for authentic legacy prefix/carrier edges. Scoped facts
    // are separately counted above; retained legacy frames may conservatively overlap.
    let node = std::mem::size_of::<BTreeNodeEnvelopeV1<crate::palw_claim_local_v1::PalwClaimLocalWorkRefV1, ()>>() as u64;
    total = add(total, prefix_edges.checked_mul(node).ok_or_else(overflow)?)?;
    Ok(total)
}

fn quote_kernel_dependency_rows_v1(state: &PalwChainStateV2, q: &mut TirNativeRequestsV1) -> Result<(), NativeCoverageV1> {
    use crate::palw_native_resource_v1::preflight_native_value_v1;
    let Some(route)=state.kernel_route.as_ref() else{return Ok(())};
    let count=route.rows.keys().filter(|(t,_)|*t==misaka_palw_kernel::rows::TABLE_CLAIMS_V1).count() as u64;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(count)?;
    growing::<(Hash64,Vec<u8>,PalwClaimLocalRecordV1)>(q,count)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,PalwClaimLocalRecordV1>>(count)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,()>>(count)?;
    for ((table,_),row) in &route.rows{if *table!=misaka_palw_kernel::rows::TABLE_CLAIMS_V1{continue}
        match preflight_native_value_v1::<misaka_palw_kernel::ledger::ClaimRowV1>(row.as_slice(),row.len() as u64){
            Ok(shape)=>{q.add_v1(shape.logical_requests_v1())?;bytes(q,row.len() as u64)?;},
            Err(error)=>{if let Some(e)=io_resource(error){return Err(e)}},
        }
    }
    Ok(())
}

fn quote_native_vertices_v1(state:&PalwChainStateV2,q:&mut TirNativeRequestsV1)->Result<(),NativeCoverageV1>{
    use crate::palw_claim_local_v1::{PalwClaimLiabilityFrameV1,decode_claim_local_value_v1,PalwClaimLocalNativeEventV1};
    let mut events=0u64;let mut frame_bytes=0u64;let mut frames=0u64;
    for (_,bytes) in rewards::frames(state){
        frames=add(frames,1)?;frame_bytes=add(frame_bytes,bytes.len() as u64)?;
        match decode_claim_local_value_v1::<PalwClaimLiabilityFrameV1>(bytes){
            Ok(frame)=>events=add(events,frame.native_changes.len() as u64)?,
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e))=>return Err(e),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))=>{},
        }
    }
    let kernels=state.kernel_route.as_ref().map_or(0,|r|r.rows.keys().filter(|(t,_)|*t==misaka_palw_kernel::rows::TABLE_CLAIMS_V1).count()) as u64;
    let anchors=match crate::palw_native39_frontier_v1::native39_frontier_anchors_v1(state){
        Ok(rows)=>rows,
        Err(PalwStateV2Error::NativeResource(e))=>return Err(e),
        // Preserve the constructor's later semantic error for corrupt bytes.
        Err(_)=>crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(Vec::new(),None),
    };
    let sealed=state.kernel_route.as_ref().map_or(0,|r|r.aux.keys().filter(|(table,_)|*table==crate::palw_native_closed_facts_v1::NATIVE_CLOSED_FACTS_TABLE_V1).count()) as u64;
    let mut scoped_events=0u64;
    let observed=super::super::palw_claim_local_exec_archive_v10::visit_settlement_events_v10(state,|_,_|{
        scoped_events=scoped_events.checked_add(1).ok_or_else(||PalwStateV2Error::NativeResource(overflow()))?;Ok(())
    });
    if let Err(PalwStateV2Error::NativeResource(e))=observed{return Err(e)}
    let current_mining=state.kernel_route.as_ref().map_or(0,|r|r.aux.keys().filter(|(table,key)|*table==crate::palw_native_mining_v39::MINING_TABLE_V39&&key.len()==66&&key[..2]==[39,crate::palw_native_mining_v39::MINING_CLAIM_KIND_V39]).count())as u64;
    let scoped_mining=match super::super::palw_claim_local_native_mining_birth_v20::source_count_v20(state){Ok(n)=>n,Err(PalwStateV2Error::NativeResource(e))=>return Err(e),Err(_)=>0};
    // Native mining exists without F1 or anchors. Count its actual current rows
    // AND scoped sources (including collected-but-anchored ones); their overlap
    // conservatively covers replacement/dedup construction peaks too.
    let native=add(add(state.exec_v2.roots.len() as u64,state.exec_v2.slices.len() as u64)?,add(add(add(kernels,events)?,scoped_events)?,add(add(anchors.len() as u64,sealed)?,add(current_mining,scoped_mining)?)?)?)?;
    for (_,anchor) in anchors.iter(){
        bytes(q,encoded(anchor)?)?;
        if let Some(route)=&state.kernel_route{
            use crate::palw_native_mining_v39::*;
            for (kind,id) in [(MINING_CLAIM_KIND_V39,anchor.claim),(MINING_RECEIPT_KIND_V39,anchor.receipt)]{
                let key=crate::palw_native_receipt_v39::native_mining_row_key_v39(kind,&id.as_bytes());
                if let Some(raw)=route.aux.get(&(MINING_TABLE_V39,key)){
                    bytes(q,raw.len() as u64)?;
                    let shape=if kind==MINING_CLAIM_KIND_V39{
                        crate::palw_native_resource_v1::preflight_native_value_v1::<(misaka_palw_kernel::hash::Digest,NativeMiningClaimRecordV39)>(raw.as_slice(),raw.len() as u64)
                    }else{
                        crate::palw_native_resource_v1::preflight_native_value_v1::<(misaka_palw_kernel::hash::Digest,NativeMiningReceiptRecordV39)>(raw.as_slice(),raw.len() as u64)
                    };
                    match shape{Ok(shape)=>q.add_v1(shape.logical_requests_v1())?,Err(error)=>{if let Some(e)=io_resource(error){return Err(e)}}}
                }
            }
        }
    }
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,(u64,usize,crate::palw_native_resource_v1::NativeRetainedV1<(misaka_palw_kernel::hash::Digest,crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>)>>(native)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,u64>>(add(events,scoped_mining)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,super::native_checkpoint::Birth<'static>>>(events)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,super::native_checkpoint::Birth<'static>>>(native)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,(PalwClaimLocalNativeEventV1,&'static Vec<u8>,bool)>>(add(events,scoped_events)?)?;
    q.exact_v1::<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>>(add(frames,scoped_events)?.max(4).checked_mul(3).ok_or_else(overflow)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,PalwClaimLocalRecordV1>>(native)?;
    let edges=native.checked_mul(add(state.exec_v2.slices.len() as u64,4)?).ok_or_else(overflow)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<PalwClaimLocalWorkRefV1,()>>(edges)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(add(state.claims.len() as u64,native.checked_mul(add(frames,5)?).ok_or_else(overflow)?)?)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(add(frames,native.checked_mul(3).ok_or_else(overflow)?)?)?;
    // Distinct frame bodies are retained once. Include a simultaneous duplicate
    // at a dedup boundary, without multiplying bytes by native vertex count.
    q.exact_v1::<u8>(frame_bytes.checked_mul(2).ok_or_else(overflow)?)?;
    if let Some(route)=&state.kernel_route{
        for ((table,key),raw) in &route.aux{
            if *table==crate::palw_native_mining_v39::MINING_TABLE_V39&&key.starts_with(&[39,crate::palw_native_mining_v39::MINING_CLAIM_KIND_V39]){
                bytes(q,raw.len() as u64)?;
                match crate::palw_native_resource_v1::preflight_native_value_v1::<(misaka_palw_kernel::hash::Digest,crate::palw_native_mining_v39::NativeMiningClaimRecordV39)>(raw.as_slice(),raw.len() as u64){
                    Ok(shape)=>q.add_v1(shape.logical_requests_v1())?,Err(error)=>{if let Some(e)=io_resource(error){return Err(e)}}
                }
            }
        }
    }
    // Exact source-only material for current Kernel coordinates. This bounds
    // clones/dedup nodes even when their old F1 has already been collected.
    if let Some(route)=&state.kernel_route {
        for ((table,key),_) in &route.rows {
            if *table!=misaka_palw_kernel::rows::TABLE_CLAIMS_V1 || key.len()!=64 {continue}
            let id=Hash64::from_bytes(key.as_slice().try_into().expect("fixed native Claim key"));
            let quote=super::super::palw_claim_local_kernel_current_birth_v9::visit_current_checkpoint_material_v10(state,id,|raw|{
                q.exact_v1::<u8>(raw.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
                q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
                q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;
                Ok(())
            });
            match quote {Err(PalwStateV2Error::NativeResource(e))=>return Err(e),_=>{}}
            // A malformed present native source is reported by the constructor,
            // after capacity admission, using its actual semantic fault.
        }
    }
    // EACH actual Source20 material callback becomes an owned raw Vec, a
    // per-record material-set insertion and a canonical material-map entry.
    // Count retained bytes plus a simultaneous duplicate at dedup boundaries;
    // reader/decode transient leases do not own these resulting checkpoint Vecs.
    let observed=super::super::palw_claim_local_native_mining_birth_v20::visit_receipts_v20(state,|id,_,_,_,_,_,_|{
        let quoted=super::super::palw_claim_local_native_mining_birth_v20::visit_checkpoint_material_v20(state,id,|raw|{
            q.exact_v1::<u8>((raw.len()as u64).checked_mul(2).ok_or_else(||PalwStateV2Error::NativeResource(overflow()))?).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;Ok(())
        })?;if !quoted{return Err(PalwStateV2Error::CarriageInconsistent("Source20 allocation source disappeared".into()))}Ok(())
    });
    if let Err(PalwStateV2Error::NativeResource(e))=observed{return Err(e)}
    // Semantic faults remain the constructor's errors after capacity admission.
    // Typed EXEC material clones/dedup nodes are admitted even with zero F1
    // frames. Event input leases are separately retained by Context above.
    let mut quote_material=|work:PalwClaimLocalWorkRefV1|->Result<(),PalwStateV2Error>{
        super::super::palw_claim_local_exec_archive_v10::visit_material_v10(state,work,|raw|{
            q.exact_v1::<u8>(raw.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;Ok(())
        })?;Ok(())
    };
    let births=super::super::palw_claim_local_exec_archive_v10::visit_births_v10(state,|view,_|quote_material(view.work()));
    if let Err(PalwStateV2Error::NativeResource(e))=births{return Err(e)}
    let settlements=super::super::palw_claim_local_exec_archive_v10::visit_settlement_events_v10(state,|event,_|quote_material(event.work));
    if let Err(PalwStateV2Error::NativeResource(e))=settlements{return Err(e)}
    drop(quote_material);
    for (id,root) in &state.exec_v2.roots{bytes(q,encoded(&(*id,root))?)?;}
    for ((id,index),slice) in &state.exec_v2.slices{bytes(q,encoded(&(*id,*index,slice))?)?;}
    Ok(())
}

// Scoped A8 material remains a real projection allocation when there are zero
// F1 frames. Reader-owned typed leases are separate from the copied checkpoint.
fn quote_chain_material_v10(state:&PalwChainStateV2,q:&mut TirNativeRequestsV1)->Result<(),NativeCoverageV1>{
    for (id,current) in &state.claims {
        let result=super::super::palw_claim_local_source_admission_v8::visit_checkpoint_material_v10(state,*id,current,|raw|{
            q.exact_v1::<u8>(raw.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;
            Ok(())
        });
        // Original semantic errors stay with the real constructor after its
        // capacity admission. Actual local resource errors retain their class.
        if let Err(PalwStateV2Error::NativeResource(resource))=result{return Err(resource)}
    }
    Ok(())
}

// New scoped cohort components are real copied node/literal data even at F0.
// Their own reader keeps decode leases; the outer checkpoint owns these clones.
fn quote_cohort_material_v15(state:&PalwChainStateV2,q:&mut TirNativeRequestsV1)->Result<(),NativeCoverageV1>{
    for (id,current) in &state.claims{
        let result=super::super::palw_claim_local_execution_cohort_archive_v15::visit_checkpoint_material_v15(state,*id,current,|raw|{
            q.exact_v1::<u8>(raw.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;Ok(())
        });
        if let Err(PalwStateV2Error::NativeResource(resource))=result{return Err(resource)}
    }
    Ok(())
}

fn quote_current_material_v18(state:&PalwChainStateV2,q:&mut TirNativeRequestsV1)->Result<(),NativeCoverageV1>{
    for(id,current)in &state.claims{
        let mut refused=None;
        let result=super::super::palw_claim_local_legacy_terminal_v18::quote_current_checkpoint_material_v18(state,*id,current,|raw|{
            let mut total=||->Result<(),NativeCoverageV1>{q.exact_v1::<u8>((raw.len()as u64).checked_mul(2).ok_or_else(overflow)?)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1)?;Ok(())};
            total().map_err(|e|{refused=Some(e);bad("current18 material allocation inventory")})
        });
        if let Some(e)=refused{return Err(e)}if let Err(PalwStateV2Error::NativeResource(e))=result{return Err(e)}
    }Ok(())
}

/// Actual params/point normal path. Related historical bytes are counted from
/// the SAME source-local traversal as attach, before checkpoint-owned copies.
/// Reader/decode leases are separate; each callback also pays both retained
/// bytes and a simultaneous duplicate at a map-dedup boundary.
pub(super) fn requests_targeted_v24(state:&PalwChainStateV2,params:&PalwStateParamsV2,point:&PalwBlockContextV2)->Result<u64,NativeCoverageV1>{
 let base=requests(state)?;let mut q=TirNativeRequestsV1::new();q.add_v1(base)?;
 let mut seen=BTreeSet::<Hash64>::new();let mut seen_lease=crate::palw_native_resource_v1::reserve_native_requests_v1(0)?;
 for(id,current)in &state.claims{
  let observed=(||->Result<(),PalwStateV2Error>{
   if !super::super::palw_claim_local_retired_dependency_material_v24::consumer_material_open_v24(state,params,point.daa_score,*id,current)?{return Ok(())}
   let _complete=super::super::palw_claim_local_retired_dependency_material_v24::visit_consumer_material_v24(state,params,*id,current,|raw|{
    let mid=crate::palw_claim_local_v1::palw_liability_material_id_v1(raw);
    if !seen.contains(&mid){let mut scratch=TirNativeRequestsV1::new();scratch.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;if let Some(lease)=&mut seen_lease{lease.grow_memory_v1(scratch.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;}seen.insert(mid);
     q.exact_v1::<u8>((raw.len()as u64).checked_mul(2).ok_or_else(||PalwStateV2Error::NativeResource(overflow()))?).map_err(PalwStateV2Error::NativeResource)?;
     q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
    }
    q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(1).map_err(PalwStateV2Error::NativeResource)?;Ok(())
   })?;Ok(())
  })();if let Err(PalwStateV2Error::NativeResource(e))=observed{return Err(e)}
  // The normal constructor preserves its own original malformed/missing error
  // after capacity admission. An incomplete dependency is never paid/Ready.
 }Ok(q.bytes_v1())
}
/// Old no-params public quote compatibility only. It cannot select exact
/// semantic consumer scopes, so retain a finite conservative raw-state bound.
/// NORMAL construction and local-checker paths use targeted_v24 above; no
/// unrelated Pending multiplies their capacity reservation.
pub(super) fn requests_compatibility_v24(state:&PalwChainStateV2)->Result<u64,NativeCoverageV1>{
 let mut q=TirNativeRequestsV1::new();q.add_v1(requests(state)?)?;let claims=state.claims.len()as u64;if claims==0{return Ok(q.bytes_v1())}
 if let Some(route)=&state.kernel_route{for(_,raw)in &route.aux{
  q.exact_v1::<u8>((raw.len()as u64).checked_mul(2).and_then(|n|n.checked_mul(claims)).ok_or_else(overflow)?)?;
  q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(claims)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(claims)?;
 }}
 let mut semantic=None;let observed=super::super::palw_claim_local_legacy_terminal_v18::quote_pending_checkpoint_material_v18(state,|source|source.visit_material_v18(state,|raw|{
  q.exact_v1::<u8>((raw.len()as u64).checked_mul(2).and_then(|n|n.checked_mul(claims)).ok_or_else(||PalwStateV2Error::NativeResource(overflow()))?).map_err(PalwStateV2Error::NativeResource)?;
  q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(claims).map_err(PalwStateV2Error::NativeResource)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(claims).map_err(PalwStateV2Error::NativeResource)?;Ok(())
 }));if let Err(PalwStateV2Error::NativeResource(e))=observed{semantic=Some(e)}if let Some(e)=semantic{return Err(e)}
 // One fixed subject-binding header per encountered source. Every retained
 // source has a live native row or a rooted metadata/leaf entry, so this count
 // bounds all such headers without asserting a source birth or future point.
 let mut subjects=add(claims,state.kernel_route.as_ref().map_or(0,|r|r.aux.len()as u64))?;
 // Genuine old F1 can contain many retired subjects without separate modern
 // metadata rows. Count its actual full native carriage/event inventory too;
 // an aux-row count alone is not a source-population bound for old codecs.
 for(_,raw)in rewards::frames(state){
  match crate::palw_claim_local_v1::decode_claim_local_value_v1::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(raw){
   Ok(frame)=>{subjects=add(subjects,add(frame.claims.len()as u64,frame.native_changes.len()as u64)?)?;
    match frame.with_context_carriages_v1(|prior,current|Ok((prior.claims.len()as u64,current.claims.len()as u64))){Ok((a,b))=>subjects=add(subjects,add(a,b)?)?,Err(crate::palw_claim_local_v1::PalwClaimCheckpointErrorV1::Resource(e))=>return Err(e),Err(_)=>{}}
   },Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e))=>return Err(e),Err(_)=>{}
  }
 }
 let header=super::super::palw_claim_local_retired_dependency_material_v24::binding_header_bytes_v24().map_err(|e|match e{PalwStateV2Error::NativeResource(e)=>e,_=>overflow()})?;
 let n=subjects.checked_mul(claims).ok_or_else(overflow)?;q.exact_v1::<u8>(header.checked_mul(2).and_then(|x|x.checked_mul(n)).ok_or_else(overflow)?)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,Vec<u8>>>(n)?;q.exact_v1::<BTreeNodeEnvelopeV1<Hash64,()>>(n)?;
 Ok(q.bytes_v1())
}
