//! Native completion capability32. Kernel output projections prove mathematics;
//! only this private adapter can authenticate original chain/coin provenance.
use super::*;
use misaka_palw_kernel::flat_inputs12::composition32::{MasterRequestV32,CheckedMasterRequestV32,CheckedStageInvocationV32,CheckedComponentOutputV32,ComponentOutputContextV32,check_component_output_v32,check_master_final_v32,check_stage_invocation_v32};

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(super) struct NativeInvocationBindingV32 {
    version:u16,master:Digest,stage:u16,job:Digest,original_funding:Digest,invocation:Digest,input_binding:Digest,
    class:Digest,source:Digest,maximum_contract:Digest,original_shared_nonce:Digest,
    maximum_invocation:Digest,maximum_court:Digest,material:Digest,consent32:Digest,actual_positions:u32,
}
/// This type is not exported and has no decoder/public raw constructor. Its
/// kernel projection cannot be substituted for an authenticated native receipt.
struct NativeCheckedComponentFinalV32 {binding:NativeInvocationBindingV32,output:CheckedComponentOutputV32,terminal:Digest,credit:crate::Hash64}
fn component_final(
    builder:&TransitionBuilder<'_>,master:&CheckedMasterRequestV32,invocation:&CheckedStageInvocationV32<'_>,
    job:Digest,values:&[misaka_palw_tir::Tensor],p:&NativeStagePoliciesV28,
)->Result<NativeCheckedComponentFinalV32,PalwStateV2Error>{
    let h=original_header(builder,job)?;
    let r=builder.state.kernel_route.as_ref().unwrap();
    let binding:NativeInvocationBindingV32=read(r,32,&job,1024).ok_or_else(||refused("Master32 original invocation has no authenticated native funding binding"))?;
    if binding.version!=32||binding.master!=master.root()||binding.job!=job||binding.original_funding!=h.original||binding.invocation!=invocation.invocation_root()||binding.input_binding!=invocation.input_binding_root()||binding.class!=h.class||binding.source!=invocation.source().id()||binding.maximum_contract!=h.contract.root()||binding.original_shared_nonce==[0;64]||master.wire().classes.get(binding.stage as usize)!=Some(&h.class){return Err(refused("Master32 component native original funding/invocation/class/source/master binding differs"));}
    let class=invocation.class();
    let data:JobData=read(r,6,&job,(1<<20)+(2<<20)+16384).ok_or_else(||refused("Master32 original component body absent"))?;
    let book:StageJobBookV28=borsh::from_slice(&data.book).map_err(|_|refused("Master32 original component book grammar"))?;
    let maximum=class.check_job(book.job()).map_err(refused)?;
    let plan=derive_stage_quota_v32(class,&maximum).map_err(refused)?;book.check(class,&maximum,&plan).map_err(refused)?;
    if h.id!=job||h.class!=class.root()||h.original!=book.original_root()||h.book!=book.root()||data.book.len()!=h.book_bytes as usize||data.trace.len()!=h.trace_bytes as usize||book.claim().is_none(){return Err(refused("Master32 original component native book/claim differs"));}
    if book.terminal()!=StageTerminalV28::Final||book.settlement().is_none(){return Err(refused("Master32 component has no genuine native Final"));}
    let settlement=book.settlement().unwrap();let queue=settlement_queue(&h,true);
    let(rooted,stored_queue,payload):(StageSettlementV28,crate::Hash64,crate::Hash64)=read(r,14,&job,1024).ok_or_else(||refused("Master32 original actual terminal monetary receipt absent"))?;
    if rooted!=*settlement||stored_queue!=queue||payload!=h.producer_payout||settlement.producer_credit!=book.reward()||settlement.escrow_debit!=book.reward()||settlement.requester_credit!=0||settlement.actual_slash!=0||settlement.original!=h.original{return Err(refused("Master32 component actual once-credit receipt does not match genuine original settlement"));}
    // A redeemed queue need not remain pending; the immutable original credit
    // receipt survives redemption/GC and fixes its exact amount and recipient.
    if builder.state.pending_payouts.get(&queue).is_some_and(|paid|paid.payload!=payload||paid.amount!=settlement.producer_credit){return Err(refused("Master32 pending producer redemption disagrees with original income"));}
    let nonce=r.execution_nonce_counter_v11(&book.requester_operator()).map_err(refused)?;
    if nonce.next_nonce<=misaka_palw_kernel::flat_execution11::canonical_nonce_v11(&book.job().nonce).map_err(refused)?||nonce.root()!=misaka_palw_kernel::flat_execution11::native_requester_nonce_counter_root_v11(&book.requester_operator(),nonce.next_nonce){return Err(refused("Master32 component original native requester nonce is not spent"));}
    let trace=StageTraceV14::from_invocation_bytes_v32(&data.trace,invocation.positions()).map_err(refused)?;
    let projection=check_component_output_v32(invocation,&book,&plan,&trace,values,&ComponentOutputContextV32{master:master.root(),invocation:binding.invocation,inputs:binding.input_binding,job,funding:h.original,terminal:book.root(),source:binding.source,class:h.class,claim:book.claim().unwrap(),credit_queue:queue.as_bytes(),actual_credit:settlement.producer_credit,actual_escrow:settlement.escrow_debit,producer_operator:book.producer_operator(),established:settlement.established}).map_err(refused)?;
    Ok(NativeCheckedComponentFinalV32{binding,output:projection,terminal:book.root(),credit:queue})
}
fn compose_final(master:&CheckedMasterRequestV32,components:Vec<NativeCheckedComponentFinalV32>)->Result<Digest,PalwStateV2Error>{
    if components.len()!=master.wire().classes.len()||components.iter().enumerate().any(|(i,c)|c.binding.master!=master.root()||c.binding.stage as usize!=i||c.terminal==[0;64])||components.iter().map(|c|c.credit).collect::<std::collections::BTreeSet<_>>().len()!=components.len(){return Err(refused("Master32 native closure omits/reorders a genuine component or duplicates actual credit"));}
    let outputs=components.into_iter().map(|c|c.output).collect::<Vec<_>>();check_master_final_v32(master,&outputs).map_err(refused)
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct NativeMasterHeaderV32 {version:u16,id:Digest,requester:PalwBondKeyV2,requester_operator:Digest,source:Digest,manifest:Digest,nonce:Digest,policy:Digest,registered:u64,body_bytes:u32,classes:Vec<Digest>,maximum_courts:Vec<Digest>,maximum_resources:Vec<Digest>,next_stage:u16,terminal:Option<Digest>}
fn master_key<T:BorshSerialize>(kind:u8,id:&T)->Vec<u8>{bytes(&(32u8,kind,id))}
fn master_read<T:BorshDeserialize>(r:&PalwKernelRouteStateV1,kind:u8,id:&impl BorshSerialize,cap:usize)->Option<T>{let raw=r.aux.get(&(STAGE_TABLE_V14,master_key(kind,id)))?;if raw.len()>cap{return None;}borsh::from_slice(raw).ok()}
fn restore_master(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,id:Digest)->Result<(CheckedMasterRequestV32,Vec<CheckedStageClassV28>,NativeMasterHeaderV32),PalwStateV2Error>{
    let r=builder.state.kernel_route.as_ref().unwrap();let h:NativeMasterHeaderV32=master_read(r,0,&id,16384).ok_or_else(||refused("Master32 original native header absent"))?;
    let raw:Vec<u8>=master_read(r,1,&id,2<<20).ok_or_else(||refused("Master32 original request absent"))?;if h.version!=32||h.id!=id||h.body_bytes as usize!=raw.len()||h.policy!=p.root(){return Err(refused("Master32 original header/policy/extent differs"));}
    let wire=MasterRequestV32::from_bytes(&raw).map_err(refused)?;
    let bundle_raw:Vec<u8>=read(r,1,&h.source,16<<20).ok_or_else(||refused("Master32 original complete source absent"))?;
    let(bundle,sources)=NativeStageSourceBundleV28::from_bytes(&bundle_raw).map_err(refused)?.check().map_err(refused)?;
    let mut classes=Vec::with_capacity(h.classes.len());let mut class_bytes=0usize;
    for id in &h.classes {
        let ch:ClassHeader=read(r,2,id,1024).ok_or_else(||refused("Master32 original class header absent"))?;
        let bytes:Vec<u8>=read(r,3,id,1<<20).ok_or_else(||refused("Master32 original class wire absent"))?;
        class_bytes=class_bytes.checked_add(bytes.len()).ok_or_else(||refused("Master32 class aggregate overflow"))?;if class_bytes>1<<20{return Err(refused("Master32 original complete class metadata exceeds paid aggregate cap"));}
        let cw=FlatStageClassWireV28::from_bytes(&bytes).map_err(refused)?;
        let source=sources.get(cw.stage as usize).ok_or_else(||refused("Master32 original class source index absent"))?;
        let mut class=cw.check(source).map_err(refused)?;class.bind_complete_bundle(&bundle).map_err(refused)?;
        if ch.id!=*id||class.root()!=*id||ch.bundle!=bundle.root()||ch.source!=source.id()||ch.bytes as usize!=bytes.len()||ch.policy!=p.root()||cw.policy!=p.root()||ch.scope!=class.maximum_scope().root()||ch.resource!=class.maximum_resources().root()||r.stage_source_refuted_relation_v14(&source.relation_root()){return Err(refused("Master32 original checked class/source/policy/maxima differs or refuted"));}
        classes.push(class);
    }
    let master=wire.check(&bundle,&classes).map_err(refused)?;
    if master.root()!=id||wire.complete_source!=h.source||wire.manifest!=h.manifest||wire.requester!=palw_kernel_bond_id_v1(&h.requester)||wire.nonce!=h.nonce||wire.classes!=h.classes||h.maximum_courts.len()!=classes.len()||h.maximum_resources.len()!=classes.len(){return Err(refused("Master32 original full source/classes/requester/nonce roots differ"));}
    for(i,class)in classes.iter().enumerate(){let scope=misaka_palw_kernel::flat_inputs12::court32::derive_stage_court_scope_v32(class).map_err(refused)?;if scope.root()!=h.maximum_courts[i]||class.maximum_resources().root()!=h.maximum_resources[i]{return Err(refused("Master32 independently derived original maximum component courts/resources differ"));}}
    Ok((master,classes,h))
}
/// Original master declaration pays its own complete restoration and finite
/// metadata catalogue. Producer resources are reserved only by later exact
/// producer-consented child funding; this registration grants none.
pub(super) fn register(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    if body.len()<130||body.len()>2<<20||body[..2]!=32u16.to_le_bytes(){return Err(refused("Master32 initial bounded request grammar"));}let source:Digest=body[2..66].try_into().unwrap();let r=builder.state.kernel_route.as_ref().unwrap();let bh:BundleHeader=read(r,0,&source,8192).ok_or_else(||refused("Master32 original all-component source header absent"))?;
    if !pay_restore(builder,ctx,signer,p,bh.bytes,1<<20,1<<20,body.len() as u32)?{return Ok(());}
    let wire=MasterRequestV32::from_bytes(body).map_err(refused)?;if wire.requester!=palw_kernel_bond_id_v1(signer){return Err(refused("Master32 original requester signature differs"));}
    let r=builder.state.kernel_route.as_ref().unwrap();let actor=builder.state.bonds.get(signer).filter(|b|matches!(b.status,PalwBondStatusV2::Active)).ok_or_else(||refused("Master32 original requester is not active"))?;let operator=actor.operator_id.as_bytes();let nonce=r.execution_nonce_counter_v11(&operator).map_err(refused)?;nonce.advance(&wire.nonce).map_err(refused)?;
    let mut catalog:Vec<Digest>=master_read(r,2,signer,4+8*64).unwrap_or_default();if catalog.len()>=8{return Err(refused("Master32 original requester finite metadata catalogue full"));}
    let bundle_raw:Vec<u8>=read(r,1,&source,16<<20).ok_or_else(||refused("Master32 original complete bundle body absent"))?;let(bundle,_)=NativeStageSourceBundleV28::from_bytes(&bundle_raw).map_err(refused)?.check().map_err(refused)?;let classes=wire.classes.iter().map(|id|restore_class(builder,p,*id)).collect::<Result<Vec<_>,_>>()?;let master=wire.check(&bundle,&classes).map_err(refused)?;let id=master.root();if master_read::<NativeMasterHeaderV32>(builder.state.kernel_route.as_ref().unwrap(),0,&id,16384).is_some()||builder.state.kernel_route.as_ref().unwrap().aux.contains_key(&(STAGE_TABLE_V14,master_key(7,&id))){return Err(refused("Master32 original requester declaration already registered"));}
    let courts=classes.iter().map(misaka_palw_kernel::flat_inputs12::court32::derive_stage_court_scope_v32).collect::<Result<Vec<_>,_>>().map_err(refused)?;
    let h=NativeMasterHeaderV32{version:32,id,requester:*signer,requester_operator:operator,source,manifest:bundle.manifest().root(),nonce:wire.nonce,policy:p.root(),registered:ctx.daa_score,body_bytes:body.len() as u32,classes:wire.classes.clone(),maximum_courts:courts.iter().map(|c|c.root()).collect(),maximum_resources:classes.iter().map(|c|c.maximum_resources().root()).collect(),next_stage:0,terminal:None};catalog.push(id);
    builder.write_kernel_row(STAGE_TABLE_V14,master_key(0,&id),Some(bytes(&h)));builder.write_kernel_row(STAGE_TABLE_V14,master_key(1,&id),Some(bytes(&body.to_vec())));builder.write_kernel_row(STAGE_TABLE_V14,master_key(2,signer),Some(bytes(&catalog)));builder.note_kernel_bond(signer);Ok(())
}

// ---- Audit-time composite Final (lead default 2026-10-15, pending user review) ----
//
// The Master32 composite is NOT a consensus object: no carrier, row or credit is
// created. It is a deterministic function of public material only — the
// authenticated original aux rows of one pinned state (audit32's full opening),
// the owning ruleset's immutable Stage28 policy, the public master decode tokens
// and each component's public output tensors, which must authenticate against
// that component's own committed trace openings. Every component must already be
// an individually genuine, settled Stage28 Final; nothing here pays or finalizes.
pub(super) type AuditRows = std::collections::BTreeMap<(u8, Vec<u8>), Vec<u8>>;
fn aux_read<T: BorshDeserialize>(rows: &AuditRows, raw_key: Vec<u8>, cap: usize) -> Result<T, String> {
    let raw = rows.get(&(STAGE_TABLE_V14, raw_key)).ok_or("master32 audit original row absent")?;
    if raw.len() > cap {
        return Err("master32 audit original row cap".into());
    }
    borsh::from_slice(raw).map_err(|_| "master32 audit original row grammar".into())
}
pub(super) fn audit_class(rows: &AuditRows, p: &NativeStagePoliciesV28, id: Digest) -> Result<CheckedStageClassV28, String> {
    let h: ClassHeader = aux_read(rows, key(2, &id), 1024)?;
    let bh: BundleHeader = aux_read(rows, key(0, &h.bundle), 8192)?;
    let raw: Vec<u8> = aux_read(rows, key(1, &h.bundle), (16 << 20) + 8)?;
    if raw.len() != bh.bytes as usize {
        return Err("master32 audit original source byte count".into());
    }
    let (manifest, sources) = NativeStageSourceBundleV28::from_bytes(&raw)?.check()?;
    let wire_raw: Vec<u8> = aux_read(rows, key(3, &id), (1 << 20) + 8)?;
    let wire = FlatStageClassWireV28::from_bytes(&wire_raw)?;
    let source = sources.get(wire.stage as usize).ok_or("master32 audit source stage absent")?;
    let mut class = wire.check(source)?;
    class.bind_complete_bundle(&manifest)?;
    let refuted = rows.contains_key(&(STAGE_TABLE_V14, crate::palw_flat_stage_v14::stage_key_v14(14, &source.relation_root())));
    if h.version != 28
        || h.id != id
        || h.bundle != manifest.root()
        || h.source != source.id()
        || h.policy != p.root()
        || wire.policy != p.root()
        || class.root() != id
        || h.scope != class.maximum_scope().root()
        || h.resource != class.maximum_resources().root()
        || h.bytes as usize != wire_raw.len()
        || refuted
    {
        return Err("master32 audit original source/class/manifest/maxima differ or refuted".into());
    }
    Ok(class)
}
/// Returns the composite digest of `check_master_final_v32` and every component's
/// genuine-final root, in stage order. `jobs[i]`/`values[i]` belong to stage i.
pub(crate) fn audit_master_composite_from_rows_v32(
    rows: &AuditRows,
    p: &NativeStagePoliciesV28,
    master_id: Digest,
    jobs: &[Digest],
    values: &[Vec<misaka_palw_tir::Tensor>],
    generated: &[u32],
) -> Result<(Digest, Vec<Digest>), String> {
    let h: NativeMasterHeaderV32 = aux_read(rows, master_key(0, &master_id), 16384)?;
    let raw: Vec<u8> = aux_read(rows, master_key(1, &master_id), (2 << 20) + 8)?;
    if h.version != 32 || h.id != master_id || h.body_bytes as usize != raw.len() || h.policy != p.root() {
        return Err("master32 audit original header/policy/extent differs".into());
    }
    let wire = MasterRequestV32::from_bytes(&raw)?;
    let classes = h.classes.iter().map(|id| audit_class(rows, p, *id)).collect::<Result<Vec<_>, _>>()?;
    let bh: BundleHeader = aux_read(rows, key(0, &h.source), 8192)?;
    let bundle_raw: Vec<u8> = aux_read(rows, key(1, &h.source), (16 << 20) + 8)?;
    if bundle_raw.len() != bh.bytes as usize {
        return Err("master32 audit original complete source byte count".into());
    }
    let (bundle, _) = NativeStageSourceBundleV28::from_bytes(&bundle_raw)?.check()?;
    let master = wire.check(&bundle, &classes)?;
    if master.root() != master_id
        || wire.classes != h.classes
        || jobs.len() != classes.len()
        || values.len() != classes.len()
        || generated.len() > 1 << 20
    {
        return Err("master32 audit request/classes/component count differs".into());
    }
    let mut finals = Vec::with_capacity(classes.len());
    for (stage, class) in classes.iter().enumerate() {
        let job = jobs[stage];
        let jh: JobHeader = aux_read(rows, key(5, &job), 8192)?;
        let data: JobData = aux_read(rows, key(6, &job), (1 << 20) + (2 << 20) + 16384)?;
        if jh.id != job || jh.class != class.root() || data.book.len() != jh.book_bytes as usize || data.trace.len() != jh.trace_bytes as usize {
            return Err("master32 audit component header/class/extent differs".into());
        }
        let book: StageJobBookV28 = borsh::from_slice(&data.book).map_err(|_| "master32 audit component book grammar")?;
        let actual = class.check_job(book.job())?;
        let plan = derive_stage_quota_v32(class, &actual)?;
        book.check(class, &actual, &plan)?;
        let s = book.settlement().ok_or("master32 audit component settlement absent")?;
        let (receipt, queue, payload): (StageSettlementV28, crate::Hash64, crate::Hash64) = aux_read(rows, key(14, &job), 1024)?;
        if book.id() != job
            || book.root() != jh.book
            || book.terminal() != StageTerminalV28::Final
            || receipt != *s
            || queue != settlement_queue(&jh, true)
            || payload != jh.producer_payout
        {
            return Err("master32 audit component is not an individually genuine settled Final".into());
        }
        let binding:NativeInvocationBindingV32=aux_read(rows,key(32,&job),2048)?;
        let actual:InvocationActualV32=aux_read(rows,master_key(5,&job),(256<<10)+4096)?;
        let raw_material:Vec<u8>=aux_read(rows,master_key(4,&job),MASTER_INVOCATION_MAX_BYTES_V32+8)?;
        let material=NativeStageInvocationEnvelopeV32::from_bytes(&raw_material)?;
        let maximum=misaka_palw_kernel::flat_inputs12::composition32::derive_maximum_invocation_v32(&master,class,book.job(),&material.generated,&finals)?;
        let invocation=maximum.project(&master,class,&actual.generated,&finals)?;
        let consent32:InvocationConsentV32=aux_read(rows,master_key(3,&book.consent()),1024)?;
        if binding.version!=32||binding.master!=master.root()||binding.stage as usize!=stage||binding.job!=job||binding.original_funding!=jh.original||binding.maximum_contract!=jh.contract.root()||binding.class!=class.root()||binding.source!=class.source().id()||binding.maximum_invocation!=maximum.root()||binding.maximum_court!=maximum.scope().root()||binding.invocation!=invocation.invocation_root()||binding.input_binding!=invocation.input_binding_root()||binding.actual_positions!=invocation.positions()||binding.material!=material_root(&material)||binding.consent32!=consent32.root()||consent32.base_consent!=book.consent()||consent32.maximum!=maximum.root()||consent32.master!=master_id||consent32.stage as usize!=stage||actual.job!=*invocation.job().job()||actual.claim!=book.claim().ok_or("master32 original actual claim absent")?||actual.invocation!=binding.invocation||actual.inputs!=binding.input_binding||actual.maximum!=maximum.root(){return Err("master32 audit cannot substitute newly synthesized invocation for original native Fund/Commit binding".into());}
        if matches!(class.template().program.output,misaka_palw_tir::program_v2::OutputDecl::Logits{..})&&actual.generated!=generated{return Err("master32 audit supplied generation differs from the genuine decoder Final".into());}
        let trace = StageTraceV14::from_invocation_bytes_v32(&data.trace, invocation.positions())?;
        let projection = check_component_output_v32(
            &invocation,
            &book,
            &plan,
            &trace,
            &values[stage],
            &ComponentOutputContextV32 {
                master: master.root(),
                invocation: invocation.invocation_root(),
                inputs: invocation.input_binding_root(),
                job,
                funding: jh.original,
                terminal: book.root(),
                source: invocation.source().id(),
                class: jh.class,
                claim: book.claim().ok_or("master32 audit component claim absent")?,
                credit_queue: queue.as_bytes(),
                actual_credit: s.producer_credit,
                actual_escrow: s.escrow_debit,
                producer_operator: book.producer_operator(),
                established: s.established,
            },
        )?;
        finals.push(projection);
    }
    let roots = finals.iter().map(|f| f.root()).collect();
    Ok((check_master_final_v32(&master, &finals)?, roots))
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Stateless audit entry (no `self`): only `palw_native_audit_proof_v32::derive_native_master_composite_v32`
    /// calls this, with rows from a verified full aux opening. Not a consensus transition.
    pub(crate) fn native_master_composite_from_rows_v32(
        rows: &AuditRows,
        p: &NativeStagePoliciesV28,
        master: Digest,
        jobs: &[Digest],
        values: &[Vec<misaka_palw_tir::Tensor>],
        generated: &[u32],
    ) -> Result<(Digest, Vec<Digest>), String> {
        audit_master_composite_from_rows_v32(rows, p, master, jobs, values, generated)
    }
}


#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct InvocationConsentV32 {version:u16,master:Digest,stage:u16,class:Digest,maximum:Digest,court:Digest,material:Digest,base_consent:Digest,producer:PalwBondKeyV2,requester:PalwBondKeyV2,accepted:u64}
impl InvocationConsentV32 {fn root(&self)->Digest{object_id(b"misaka-palw/native/producer-original-maximum-invocation-consent/v32",self)}}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct InvocationActualV32 {version:u16,maximum:Digest,invocation:Digest,inputs:Digest,job:misaka_palw_kernel::flat_inputs12::bounded14::FlatStageJobV14,generated:Vec<u32>,claim:Digest,committed:u64}
fn binding_key(job:Digest)->Vec<u8>{key(32,&job)}
pub(super) fn binding(r:&PalwKernelRouteStateV1,job:Digest)->Result<Option<NativeInvocationBindingV32>,PalwStateV2Error>{
    let Some(raw)=r.aux.get(&(STAGE_TABLE_V14,binding_key(job)))else{return Ok(None)};
    if raw.len()>2048{return Err(refused("invocation32 fixed original binding cap"));}
    let b:NativeInvocationBindingV32=borsh::from_slice(raw).map_err(|_|refused("invocation32 fixed original binding grammar"))?;
    if b.version!=32||b.job!=job||[b.master,b.class,b.source,b.original_funding,b.maximum_contract,b.original_shared_nonce,b.maximum_invocation,b.maximum_court,b.material,b.consent32].contains(&[0;64]){return Err(refused("invocation32 original native funding binding differs"));}Ok(Some(b))
}
fn material_root(e:&NativeStageInvocationEnvelopeV32)->Digest{object_id(b"misaka-palw/native/original-upstream-invocation-material/v32",&(e.master,e.stage,&e.generated,&e.upstream_jobs,&e.upstream_outputs))}
fn material(r:&PalwKernelRouteStateV1,b:&NativeInvocationBindingV32)->Result<NativeStageInvocationEnvelopeV32,PalwStateV2Error>{
    let raw:Vec<u8>=master_read(r,4,&b.job,MASTER_INVOCATION_MAX_BYTES_V32+8).ok_or_else(||refused("invocation32 original material absent"))?;
    let e=NativeStageInvocationEnvelopeV32::from_bytes(&raw).map_err(refused)?;
    if material_root(&e)!=b.material||e.master!=b.master||e.stage!=b.stage{return Err(refused("invocation32 original material changed"));}Ok(e)
}
/// A fixed original restore cap is paid BEFORE decoding variable envelope32,
/// classes, upstream output tensors or actual position-input images.
fn pay_invocation(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<bool,PalwStateV2Error>{
    if body.len()>MASTER_INVOCATION_MAX_BYTES_V32{return Err(refused("invocation32 bounded original carrier exceeded"));}
    // The complete source bundle is shared by every class; <=64 independent
    // class wires are restored. Charge each bounded class before decoding any.
    if !pay_restore(builder,ctx,signer,p,16<<20,1<<20,1<<20,body.len() as u32)?{return Ok(false);}
    for _ in 1..64{if !pay_restore(builder,ctx,signer,p,0,1<<20,0,0)?{return Ok(false);}}
    Ok(true)
}
fn prior_finals(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,master:&CheckedMasterRequestV32,classes:&[CheckedStageClassV28],e:&NativeStageInvocationEnvelopeV32)->Result<Vec<CheckedComponentOutputV32>,PalwStateV2Error>{
    if e.stage as usize>=classes.len()||e.upstream_jobs.len()!=e.stage as usize{return Err(refused("invocation32 must bind every earlier genuine component"));}
    let values=if e.stage==0 {if !e.upstream_outputs.is_empty(){return Err(refused("invocation32 first stage has no upstream output carrier"));}vec![]}else{crate::palw_native_audit_proof_v32::decode_native_master_outputs_v32(&e.upstream_outputs).map_err(refused)?};
    if values.len()!=e.upstream_jobs.len(){return Err(refused("invocation32 upstream output catalogue differs"));}
    let mut finals=Vec::with_capacity(values.len());let mut original_bytes=0usize;
    for(i,job)in e.upstream_jobs.iter().enumerate(){
        let uh=original_header(builder,*job)?;original_bytes=original_bytes.checked_add(uh.book_bytes as usize).and_then(|n|n.checked_add(uh.trace_bytes as usize)).and_then(|n|n.checked_add(uh.input_bytes as usize)).ok_or_else(||refused("invocation32 prior body aggregate overflow"))?;if original_bytes>2<<20{return Err(refused("invocation32 upstream original bodies exceed before-Fund paid aggregate cap"));}
        let b=binding(builder.state.kernel_route.as_ref().unwrap(),*job)?.ok_or_else(||refused("invocation32 cannot relabel an unrelated standalone Final"))?;
        let a:InvocationActualV32=master_read(builder.state.kernel_route.as_ref().unwrap(),5,job, (256<<10)+4096).ok_or_else(||refused("invocation32 upstream actual invocation absent"))?;
        if b.master!=master.root()||b.stage as usize!=i||b.class!=classes[i].root()||a.version!=32||a.maximum!=b.maximum_invocation||a.invocation!=b.invocation||a.inputs!=b.input_binding||a.job.positions!=b.actual_positions||a.claim==[0;64]{return Err(refused("invocation32 prior genuine Final has another master/order/actual binding"));}
        let invocation=check_stage_invocation_v32(master,&classes[i],&a.job,&a.generated,&finals).map_err(refused)?;
        let final_=component_final(builder,master,&invocation,*job,&values[i],p)?;
        finals.push(final_.output);
    }
    // A later stage may use only the actual generation of the master's
    // independently finalized decoder; arbitrary new ids cannot be supplied.
    for(i,job)in e.upstream_jobs.iter().enumerate(){if matches!(classes[i].template().program.output,misaka_palw_tir::program_v2::OutputDecl::Logits{..}){let a:InvocationActualV32=master_read(builder.state.kernel_route.as_ref().unwrap(),5,job,(256<<10)+4096).ok_or_else(||refused("invocation32 original decoder actual absent"))?;if a.generated!=e.generated{return Err(refused("invocation32 replaces original upstream stopped ids"));}}}
    Ok(finals)
}
fn maximum<'a>(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,e:&NativeStageInvocationEnvelopeV32,master:&CheckedMasterRequestV32,classes:&'a[CheckedStageClassV28],job:&misaka_palw_kernel::flat_inputs12::bounded14::FlatStageJobV14)->Result<misaka_palw_kernel::flat_inputs12::composition32::CheckedMaximumInvocationV32,PalwStateV2Error>{
    let class=classes.get(e.stage as usize).ok_or_else(||refused("invocation32 original stage absent"))?;
    let finals=prior_finals(builder,p,master,classes,e)?;
    misaka_palw_kernel::flat_inputs12::composition32::derive_maximum_invocation_v32(master,class,job,&e.generated,&finals).map_err(refused)
}
pub(super) fn accept_invocation(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    if !pay_invocation(builder,ctx,signer,p,body)?{return Ok(());}
    let e=NativeStageInvocationEnvelopeV32::from_bytes(body).map_err(refused)?;let(master,classes,_)=restore_master(builder,p,e.master)?;
    let wire=StageAcceptSourceV28::from_bytes(&e.body).map_err(refused)?;let prep=StageAcceptRequestV14::from_bytes(&wire.preparation14).map_err(refused)?;let job=FlatStageJobV14::from_bytes(&prep.job_bytes).map_err(refused)?;
    let maximum=maximum(builder,p,&e,&master,&classes,&job)?;
    if prep.class!=classes[e.stage as usize].root(){return Err(refused("invocation32 original acceptance class differs"));}
    accept_impl(builder,ctx,signer,p,&e.body,true)?;
    let r=builder.state.kernel_route.as_ref().unwrap();let ids:Vec<Digest>=read(r,9,signer,4+8*64).ok_or_else(||refused("invocation32 own producer original acceptance catalog absent"))?;
    let id=*ids.last().ok_or_else(||refused("invocation32 own producer acceptance was not established"))?;let base:NativeStageConsentV28=read(r,10,&id,4096).ok_or_else(||refused("invocation32 base original acceptance absent"))?;
    if base.original14.job!=job.root()||base.original14.class!=prep.class||base.original14.producer!=*signer||base.original14.accepted_daa!=ctx.daa_score{return Err(refused("invocation32 cannot attach to unrelated/old producer consent"));}
    let c=InvocationConsentV32{version:32,master:e.master,stage:e.stage,class:prep.class,maximum:maximum.root(),court:maximum.scope().root(),material:material_root(&e),base_consent:id,producer:*signer,requester:prep.requester,accepted:ctx.daa_score};
    builder.write_kernel_row(STAGE_TABLE_V14,master_key(3,&id),Some(bytes(&c)));Ok(())
}
pub(super) fn fund_invocation(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    if !pay_invocation(builder,ctx,signer,p,body)?{return Ok(());}
    let mut e=NativeStageInvocationEnvelopeV32::from_bytes(body).map_err(refused)?;let(master,classes,_)=restore_master(builder,p,e.master)?;let request=StageFundRequestV14::from_bytes(&e.body).map_err(refused)?;let job=FlatStageJobV14::from_bytes(&request.job_bytes).map_err(refused)?;
    let maximum=maximum(builder,p,&e,&master,&classes,&job)?;
    let c:InvocationConsentV32=master_read(builder.state.kernel_route.as_ref().unwrap(),3,&request.acceptance,1024).ok_or_else(||refused("invocation32 original producer signed maximum acceptance absent"))?;
    if c.version!=32||c.master!=e.master||c.stage!=e.stage||c.class!=request.class||c.maximum!=maximum.root()||c.court!=maximum.scope().root()||c.material!=material_root(&e)||c.base_consent!=request.acceptance||c.producer!=request.producer||c.requester!=*signer{return Err(refused("invocation32 Fund changes original producer maximum/master/upstream material"));}
    let mut progress=can_fund(builder,&e,&master,&job)?;
    fund_impl(builder,ctx,signer,p,&e.body,true)?;
    let r=builder.state.kernel_route.as_ref().unwrap();let(id,original,_):(Digest,Digest,Digest)=read(r,11,&request.acceptance,256).ok_or_else(||refused("invocation32 original Fund did not establish actual debit"))?;
    let h=original_header(builder,id)?;let nonce=r.execution_nonce_counter_v11(&r.aux_row::<NativeStageConsentV28>(STAGE_TABLE_V14,&key(10,&request.acceptance)).ok_or_else(||refused("invocation32 original producer consent absent"))?.original14.requester_operator).map_err(refused)?;
    if binding(r,id)?.is_some()||h.original!=original||h.class!=c.class{return Err(refused("invocation32 original funding already bound or differs"));}
    let b=NativeInvocationBindingV32{version:32,master:e.master,stage:e.stage,job:id,original_funding:original,invocation:[0;64],input_binding:[0;64],class:h.class,source:classes[e.stage as usize].source().id(),maximum_contract:h.contract.root(),original_shared_nonce:nonce.root(),maximum_invocation:maximum.root(),maximum_court:maximum.scope().root(),material:c.material,consent32:c.root(),actual_positions:0};
    // Base carrier is unnecessary for re-execution; the maximum job remains in
    // the immutable original max-book. Keep only exact public upstream facts.
    progress.jobs[e.stage as usize]=id;builder.write_kernel_row(STAGE_TABLE_V14,master_key(6,&e.master),Some(bytes(&progress)));
    e.body.clear();builder.write_kernel_row(STAGE_TABLE_V14,binding_key(id),Some(bytes(&b)));builder.write_kernel_row(STAGE_TABLE_V14,master_key(4,&id),Some(bytes(&bytes(&e))));Ok(())
}
pub(super) fn actual_positions(r:&PalwKernelRouteStateV1,job:Digest,fallback:u32)->Result<u32,PalwStateV2Error>{Ok(binding(r,job)?.map(|b|if b.actual_positions==0{fallback}else{b.actual_positions}).unwrap_or(fallback))}
pub(super) fn actual_job_fixed(builder:&TransitionBuilder<'_>,h:&JobHeader,class:&CheckedStageClassV28)->Result<Option<CheckedStageJobV28>,PalwStateV2Error>{
    let Some(b)=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?else{return Ok(None)};
    let a:InvocationActualV32=master_read(builder.state.kernel_route.as_ref().unwrap(),5,&h.id,(256<<10)+4096).ok_or_else(||refused("invocation32 authenticated actual projection absent"))?;
    if a.version!=32||a.maximum!=b.maximum_invocation||a.invocation!=b.invocation||a.inputs!=b.input_binding||a.job.positions!=b.actual_positions||b.original_funding!=h.original||b.maximum_contract!=h.contract.root()||b.class!=class.root(){return Err(refused("invocation32 authenticated original actual projection changed"));}
    class.check_job(&a.job).map(Some).map_err(refused)
}
pub(super) fn commit_invocation(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    // Fixed outer frame cap before any variable material. The signed body is
    // predebited under the funded maximum before actual generation is decoded.
    if body.len()>MASTER_INVOCATION_MAX_BYTES_V32{return Err(refused("invocation32 Commit carrier cap"));}
    if !pay_invocation(builder,ctx,signer,p,body)?{return Ok(());}
    let e=NativeStageInvocationEnvelopeV32::from_bytes(body).map_err(refused)?;
    let id:Digest=e.body.get(66..130).ok_or_else(||refused("invocation32 Commit exact original job prefix absent"))?.try_into().unwrap();let mut h=original_header(builder,id)?;
    let mut b=binding(builder.state.kernel_route.as_ref().unwrap(),id)?.ok_or_else(||refused("invocation32 Commit lacks original before-Fund binding"))?;
    if h.producer!=*signer||h.terminal!=StageTerminalV28::Funded||b.invocation!=[0;64]||b.master!=e.master||b.stage!=e.stage{return Err(refused("invocation32 Commit own original producer/maximum state differs"));}
    let epoch=TrustedEpochV11::from_chain(&p.execution.quota,ctx.block.as_bytes(),ctx.daa_score).map_err(refused)?;
    let cached=cached_plan(&h,p)?; // Same canonical cached full vector; mode is rederived below.
    let binding_=original_binding(builder,p,&h,&cached)?;
    let mut shard=super::super::super::palw_flat_execution_fold_v11::quota_shard(builder,&p.execution,&epoch,palw_kernel_bond_id_v1(signer),id)?;
    let charge=cached.plan().action(misaka_palw_kernel::flat_quota11::QuotaActionKindV11::Metadata,&e.body).map_err(refused)?;shard.debit_reserved(&p.execution.quota,&epoch,&binding_,cached.plan(),&charge).map_err(refused)?;super::super::super::palw_flat_execution_fold_v11::write_shard(builder,&shard,id);
    let(class,maximum_job,plan,mut book,mut data)=restore_job(builder,p,&h)?;let(master,classes,_)=restore_master(builder,p,b.master)?;let original=material(builder.state.kernel_route.as_ref().unwrap(),&b)?;
    if e.upstream_jobs!=original.upstream_jobs||e.upstream_outputs!=original.upstream_outputs{return Err(refused("invocation32 Commit replaces original upstream Finals"));}
    let finals=prior_finals(builder,p,&master,&classes,&original)?;let maximum=misaka_palw_kernel::flat_inputs12::composition32::derive_maximum_invocation_v32(&master,&class,book.job(),&original.generated,&finals).map_err(refused)?;
    if maximum.root()!=b.maximum_invocation||maximum.scope().root()!=b.maximum_court||contract(builder,p,&class,&maximum_job,&plan)?.record()!=&h.contract||cached.root()!=plan.root(){return Err(refused("invocation32 original maximum resource/court contract changed"));}
    let invocation=maximum.project(&master,&class,&e.generated,&finals).map_err(refused)?;
    let trace=StageTraceV14::from_invocation_bytes_v32(&e.body,invocation.positions()).map_err(refused)?;
    book.commit_invocation_v32(&class,&maximum_job,&plan,&invocation,palw_kernel_bond_id_v1(signer),ctx.daa_score,&trace,h.producer_reserved).map_err(refused)?;
    b.invocation=invocation.invocation_root();b.input_binding=invocation.input_binding_root();b.actual_positions=invocation.positions();
    let actual=InvocationActualV32{version:32,maximum:b.maximum_invocation,invocation:b.invocation,inputs:b.input_binding,job:invocation.job().job().clone(),generated:e.generated,claim:book.claim().unwrap(),committed:ctx.daa_score};
    builder.write_kernel_row(STAGE_TABLE_V14,binding_key(id),Some(bytes(&b)));builder.write_kernel_row(STAGE_TABLE_V14,master_key(5,&id),Some(bytes(&actual)));
    data.book=bytes(&book);data.trace=e.body;write_job(builder,&mut h,&mut data,&book);
    let status=crate::palw_native_paged_proof_v29::NativePagedJobStatusV29{version:29,namespace:28,job:id,funding:h.original,claim:book.claim().unwrap(),maximum:h.contract.root(),paid_plan:h.contract.paged_plan,parameter:class.source().parameters().root(),inventory:class.source().inventory_root(),active:true,admitted_interval:book.admitted_interval(),deadline:book.deadline(),challenge_end:book.challenge_end().unwrap(),paid_owners:[[0;64];4],owners_admitted:0,latest_paid_horizon:0};builder.write_kernel_row(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,bytes(&(29u8,id,0u8)),Some(bytes(&(status.root(),status))));Ok(())
}

fn with_actual<T>(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,h:&JobHeader,class:&CheckedStageClassV28,book:&StageJobBookV28,f:impl FnOnce(&CheckedStageInvocationV32<'_>,&misaka_palw_kernel::flat_inputs12::court32::CheckedStageCourtScopeV32,&[u32])->Result<T,PalwStateV2Error>)->Result<T,PalwStateV2Error>{
    let b=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?.ok_or_else(||refused("invocation32 genuine original binding absent"))?;
    let e=material(builder.state.kernel_route.as_ref().unwrap(),&b)?;let(master,classes,_)=restore_master(builder,p,b.master)?;
    let finals=prior_finals(builder,p,&master,&classes,&e)?;
    let maximum=misaka_palw_kernel::flat_inputs12::composition32::derive_maximum_invocation_v32(&master,class,book.job(),&e.generated,&finals).map_err(refused)?;
    let a:InvocationActualV32=master_read(builder.state.kernel_route.as_ref().unwrap(),5,&h.id,(256<<10)+4096).ok_or_else(||refused("invocation32 immutable actual projection absent"))?;
    let invocation=maximum.project(&master,class,&a.generated,&finals).map_err(refused)?;
    if maximum.root()!=b.maximum_invocation||maximum.scope().root()!=b.maximum_court||a.version!=32||a.maximum!=maximum.root()||a.job!=*invocation.job().job()||a.invocation!=invocation.invocation_root()||b.invocation!=a.invocation||a.inputs!=invocation.input_binding_root()||b.input_binding!=a.inputs||b.actual_positions!=invocation.positions()||book.claim()!=Some(a.claim)||h.original!=b.original_funding||h.contract.root()!=b.maximum_contract{return Err(refused("invocation32 actual source/state/input/generated/claim no longer derives from original before-Fund maximum"));}
    f(&invocation,maximum.scope(),&a.generated)
}
pub(super) fn check_final_binding(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,h:&JobHeader,book:&StageJobBookV28,class:&CheckedStageClassV28,data:&JobData)->Result<(),PalwStateV2Error>{
    let Some(b)=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?else{return Ok(())};
    let mh:NativeMasterHeaderV32=master_read(builder.state.kernel_route.as_ref().unwrap(),0,&b.master,16384).ok_or_else(||refused("Master32 original declaration absent at Final"))?;let progress=progress(builder.state.kernel_route.as_ref().unwrap(),&mh)?;
    if progress.aborted||progress.jobs.get(b.stage as usize)!=Some(&h.id)||progress.finals[b.stage as usize]!=[0;64]||mh.next_stage!=b.stage{return Err(refused("Master32 original component is not the current unfinalized invocation"));}
    with_actual(builder,p,h,class,book,|invocation,_,_|{
        let trace=StageTraceV14::from_invocation_bytes_v32(&data.trace,invocation.positions()).map_err(refused)?;
        if trace.job_id!=h.id||trace.class!=h.class||trace.job_input!=invocation.trace_input_root()||trace.admission!=invocation.job().admission().root()||trace.quota!=book.quota_root()||book.claim()!=Some(object_id(b"misaka-palw/kernel/stage-authenticated-trace/v28",&(h.contract.root(),trace.root()))){return Err(refused("invocation32 Final changes original actual trace claim"));}Ok(())
    })
}
pub(super) fn verify_computation(builder:&TransitionBuilder<'_>,p:&NativeStagePoliciesV28,h:&JobHeader,book:&StageJobBookV28,class:&CheckedStageClassV28,data:&JobData,raw:&[u8])->Result<(),PalwStateV2Error>{
    use misaka_palw_kernel::{flat_inputs12::court32::*,flat_court5::{FlatSegContextV5,FlatFaultV5},element::SegClaimContextV1,seg::prompt_root_of_ids_v1,job::DecodeRuleV1};
    with_actual(builder,p,h,class,book,|invocation,scope,generated|{
        if raw.len() as u64>scope.scope().maximum_bytes{return Err(refused("invocation32 original paid court bound exceeded"));}
        let version=raw.get(..2).map(|v|u16::from_le_bytes(v.try_into().unwrap())).ok_or_else(||refused("invocation32 original typed computation court absent"))?;
        let position=match version {
            32=>{let f=StagePointFaultV32::from_bytes(raw).map_err(refused)?;let f=misaka_palw_kernel::flat_point25::parse_flat_point_fault_v25(&f.point25).map_err(|e|refused(e.to_string()))?;f.output.node.as_ref().ok_or_else(||refused("invocation32 point output absent"))?.position},
            7=>misaka_palw_kernel::flat_topk7::TopKFaultV7::from_bytes(raw).map_err(|e|refused(e.to_string()))?.output.node.as_ref().ok_or_else(||refused("invocation32 TopK output absent"))?.position,
            5=>match FlatFaultV5::from_bytes(raw).map_err(|e|refused(e.to_string()))?{FlatFaultV5::Decode(f)=>f.logits.node.as_ref().ok_or_else(||refused("invocation32 decoder logits opening absent"))?.position,FlatFaultV5::Malformed(f)=>f.output.position,_=>return Err(refused("invocation32 ordinary relation must use original V2 court32"))},
            _=>return Err(refused("invocation32 unknown computation court")),
        };
        let trace=StageTraceV14::from_invocation_bytes_v32(&data.trace,invocation.positions()).map_err(refused)?;
        let view=invocation.actual_program().v1_view();let pc=invocation.combined_parameters_at(position).map_err(refused)?;
        let logits=matches!(invocation.actual_program().output,misaka_palw_tir::program_v2::OutputDecl::Logits{..});
        let prompt=invocation.prompt();let generated=if logits{generated}else{&[0][..]};
        let base=FlatSegContextV5{profile:misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5(),claim:SegClaimContextV1{program:&view,params:&pc,segment_roots:&trace.segment_roots,positions:invocation.positions(),prompt_len:prompt.len() as u32,prompt_root:prompt_root_of_ids_v1(prompt),inline_prompt:Some(prompt),generated,decode:DecodeRuleV1::Greedy,encoder:None}};
        if version==32{let f=StagePointFaultV32::from_bytes(raw).map_err(refused)?;verify_stage_point_fault_v32(invocation,scope,base,&f).map_err(|e|refused(format!("{e:?}")))}else{verify_stage_special_fault_v32(invocation,scope,base,raw).map_err(|e|refused(format!("{e:?}")))}
    })
}

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
struct MasterProgressV32 {version:u16,master:Digest,jobs:Vec<Digest>,finals:Vec<Digest>,complete:Option<Digest>,aborted:bool}
fn progress(r:&PalwKernelRouteStateV1,h:&NativeMasterHeaderV32)->Result<MasterProgressV32,PalwStateV2Error>{
    let p=master_read(r,6,&h.id,16384).unwrap_or(MasterProgressV32{version:32,master:h.id,jobs:vec![[0;64];h.classes.len()],finals:vec![[0;64];h.classes.len()],complete:None,aborted:false});
    if p.version!=32||p.master!=h.id||p.jobs.len()!=h.classes.len()||p.finals.len()!=h.classes.len()||p.jobs.len()>64||p.finals.iter().zip(&p.jobs).any(|(f,j)|*f!=[0;64]&&*j==[0;64]){return Err(refused("Master32 original finite invocation/final catalogue differs"));}Ok(p)
}
fn can_fund(builder:&TransitionBuilder<'_>,e:&NativeStageInvocationEnvelopeV32,master:&CheckedMasterRequestV32,job:&FlatStageJobV14)->Result<MasterProgressV32,PalwStateV2Error>{
    let r=builder.state.kernel_route.as_ref().unwrap();let h:NativeMasterHeaderV32=master_read(r,0,&e.master,16384).ok_or_else(||refused("Master32 original native declaration absent"))?;let p=progress(r,&h)?;
    if e.stage as usize>=p.jobs.len()||h.next_stage!=e.stage||p.jobs[e.stage as usize]!=[0;64]||p.complete.is_some()||p.aborted||p.finals[..e.stage as usize].iter().any(|f|*f==[0;64])||e.stage==0&&job.nonce!=master.wire().nonce{return Err(refused("Master32 original stage is not the next unfunded component or declaration nonce differs"));}Ok(p)
}
pub(super) fn note_final(builder:&mut TransitionBuilder<'_>,h:&JobHeader,book:&StageJobBookV28)->Result<(),PalwStateV2Error>{
    let Some(b)=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?else{return Ok(())};
    let r=builder.state.kernel_route.as_ref().unwrap();let mut mh:NativeMasterHeaderV32=master_read(r,0,&b.master,16384).ok_or_else(||refused("Master32 native original declaration absent"))?;let mut p=progress(r,&mh)?;
    let s=book.settlement().ok_or_else(||refused("Master32 native component settlement absent"))?;let queue=settlement_queue(h,true);
    if book.terminal()!=StageTerminalV28::Final||p.jobs.get(b.stage as usize)!=Some(&h.id)||p.finals[b.stage as usize]!=[0;64]||mh.next_stage!=b.stage||book.original_root()!=b.original_funding||book.claim().is_none()||s.producer_credit!=book.reward()||s.actual_slash!=0||s.escrow_debit!=book.reward(){return Err(refused("Master32 native component completion has no original once-settled Final"));}
    p.finals[b.stage as usize]=object_id(b"misaka-palw/native/master-original-component-terminal/v32",&(b.master,b.stage,h.id,b.maximum_invocation,b.invocation,b.input_binding,book.original_root(),book.root(),book.claim(),s,queue));
    mh.next_stage=mh.next_stage.checked_add(1).ok_or_else(||refused("Master32 finite stage catalogue overflow"))?;
    if mh.next_stage as usize==p.jobs.len(){p.complete=Some(object_id(b"misaka-palw/native/master-all-components-completion-catalogue/v32",&(b.master,&p.jobs,&p.finals)));}
    builder.write_kernel_row(STAGE_TABLE_V14,master_key(0,&b.master),Some(bytes(&mh)));builder.write_kernel_row(STAGE_TABLE_V14,master_key(6,&b.master),Some(bytes(&p)));Ok(())
}
pub(super) fn check_collect(builder:&TransitionBuilder<'_>,h:&JobHeader,book:&StageJobBookV28)->Result<(),PalwStateV2Error>{
    let Some(b)=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?else{return Ok(())};
    let mh:NativeMasterHeaderV32=master_read(builder.state.kernel_route.as_ref().unwrap(),0,&b.master,16384).ok_or_else(||refused("Master32 original declaration absent at GC"))?;
    let p=progress(builder.state.kernel_route.as_ref().unwrap(),&mh)?;
    // Upstream values remain available to the next component until the complete
    // actual model has finished. A failed/refunded child releases the catalogue
    // after its original deadline; no deadlock and no same-interval recredit.
    let failed=p.jobs.iter().filter(|j|**j!=[0;64]).any(|j|read::<JobHeader>(builder.state.kernel_route.as_ref().unwrap(),5,j,8192).is_some_and(|h|matches!(h.terminal,StageTerminalV28::Refunded|StageTerminalV28::Convicted)));
    if p.jobs.get(b.stage as usize)!=Some(&h.id)||p.complete.is_none()&&!p.aborted&&!failed||book.original_root()!=b.original_funding{return Err(refused("Master32 original complete execution still owns this upstream component"));}Ok(())
}
pub(super) fn collect_material(builder:&mut TransitionBuilder<'_>,job:Digest){
    for kind in [4,5]{builder.write_kernel_row(STAGE_TABLE_V14,master_key(kind,&job),None);}
}

pub(super) fn note_abort(builder:&mut TransitionBuilder<'_>,h:&JobHeader,book:&StageJobBookV28)->Result<(),PalwStateV2Error>{
    if !matches!(book.terminal(),StageTerminalV28::Refunded|StageTerminalV28::Convicted){return Ok(());}
    let Some(b)=binding(builder.state.kernel_route.as_ref().unwrap(),h.id)?else{return Ok(())};let mh:NativeMasterHeaderV32=master_read(builder.state.kernel_route.as_ref().unwrap(),0,&b.master,16384).ok_or_else(||refused("Master32 original declaration absent at terminal"))?;let mut p=progress(builder.state.kernel_route.as_ref().unwrap(),&mh)?;
    if p.jobs.get(b.stage as usize)!=Some(&h.id)||book.original_root()!=b.original_funding||book.settlement().is_none(){return Err(refused("Master32 aborted child lacks genuine original settlement"));}p.aborted=true;builder.write_kernel_row(STAGE_TABLE_V14,master_key(6,&b.master),Some(bytes(&p)));Ok(())
}

/// Public metadata collection never creates a Final or pays again. The bounded
/// completion/failed catalogue and every original child credit/nonce lineage
/// survive, so later replay cannot turn a reclaimed slot into a second task.
pub(super) fn collect_master(builder:&mut TransitionBuilder<'_>,ctx:&PalwBlockContextV2,signer:&PalwBondKeyV2,p:&NativeStagePoliciesV28,body:&[u8])->Result<(),PalwStateV2Error>{
    let id:Digest=body.try_into().map_err(|_|refused("Master32 collection exact original master key"))?;
    if !pay_invocation(builder,ctx,signer,p,body)?{return Ok(());}
    let r=builder.state.kernel_route.as_ref().unwrap();let h:NativeMasterHeaderV32=master_read(r,0,&id,16384).ok_or_else(||refused("Master32 original declaration already collected or absent"))?;let state=progress(r,&h)?;
    if ctx.daa_score<=h.registered.checked_add(p.maximum_horizon).ok_or_else(||refused("Master32 original metadata horizon overflow"))?{return Err(refused("Master32 original metadata horizon remains open"));}
    for job in state.jobs.iter().filter(|j|**j!=[0;64]){
        if read::<JobHeader>(r,5,job,8192).is_some()||read::<(Digest,Digest,Digest,Digest,Option<Digest>,StageSettlementV28,Digest)>(r,20,job,2048).is_none(){return Err(refused("Master32 child original resources/coins have not been genuinely collected"));}
    }
    if state.jobs.iter().any(|j|*j!=[0;64])&&state.complete.is_none()&&!state.aborted{return Err(refused("Master32 incomplete original component catalogue remains active"));}
    let mut catalog:Vec<Digest>=master_read(r,2,&h.requester,4+8*64).ok_or_else(||refused("Master32 original requester catalogue absent"))?;
    if !catalog.contains(&id){return Err(refused("Master32 original requester catalogue membership differs"));}catalog.retain(|j|*j!=id);
    let tombstone=(32u16,id,h.source,h.manifest,h.requester,h.requester_operator,h.nonce,h.policy,&h.classes,&state);
    builder.write_kernel_row(STAGE_TABLE_V14,master_key(7,&id),Some(bytes(&tombstone)));
    // Preserve progress + compact child binding + native kind20 single-credit
    // receipts. Only bounded request payloads/owner catalogue slots disappear.
    for kind in [0,1]{builder.write_kernel_row(STAGE_TABLE_V14,master_key(kind,&id),None);}
    builder.write_kernel_row(STAGE_TABLE_V14,master_key(2,&h.requester),Some(bytes(&catalog)));builder.note_kernel_bond(signer);Ok(())
}

impl crate::palw_state_v2::PalwChainStateV2 {
    pub(crate) fn native_master_execution_record_from_rows_v32(rows:&AuditRows,p:&NativeStagePoliciesV28,master_id:Digest,jobs:&[Digest],values:&[Vec<misaka_palw_tir::Tensor>],generated:&[u32])->Result<NativeMasterExecutionRecordV32,String>{
        // Reconstruct native original before-Fund/actual bindings AND every
        // genuine settled component before deriving any complete-model label.
        let(composite,_)=audit_master_composite_from_rows_v32(rows,p,master_id,jobs,values,generated)?;
        let h:NativeMasterHeaderV32=aux_read(rows,master_key(0,&master_id),16384)?;
        let progress:MasterProgressV32=aux_read(rows,master_key(6,&master_id),16384)?;
        if progress.version!=32||progress.master!=master_id||progress.jobs!=jobs||progress.complete.is_none()||progress.aborted||progress.finals.len()!=jobs.len()||progress.finals.iter().any(|r|*r==[0;64]){return Err("master32 complete-model receipt lacks original all-component native completion catalogue".into());}
        let mut components=Vec::with_capacity(jobs.len());
        for(stage,job)in jobs.iter().enumerate(){
            let jh:JobHeader=aux_read(rows,key(5,job),8192)?;let data:JobData=aux_read(rows,key(6,job),(1<<20)+(2<<20)+16384)?;
            let book:StageJobBookV28=borsh::from_slice(&data.book).map_err(|_|"master32 receipt book grammar")?;
            let b:NativeInvocationBindingV32=aux_read(rows,key(32,job),2048)?;let a:InvocationActualV32=aux_read(rows,master_key(5,job),(256<<10)+4096)?;
            let class=audit_class(rows,p,jh.class)?;let s=book.settlement().ok_or("master32 receipt original settlement absent")?;let(s2,queue,payload):(StageSettlementV28,crate::Hash64,crate::Hash64)=aux_read(rows,key(14,job),1024)?;
            if s!=&s2||queue!=settlement_queue(&jh,true)||payload!=jh.producer_payout||b.master!=master_id||b.stage as usize!=stage||book.claim()!=Some(a.claim){return Err("master32 receipt original component money/binding differs".into());}
            let(admitted,_,committed)=book.receipt_clocks_v1();
            components.push(NativeMasterComponentReceiptV32{stage:stage as u16,class:jh.class,program:class.source().program_root(),source:class.source().id(),inventory:class.source().inventory_root(),real_parameter:class.source().parameters().root(),job:*job,original_funding:jh.original,consent:jh.consent,maximum_contract:jh.contract.root(),maximum_scope:book.maximum_scope_root(),maximum_resource:book.maximum_resources_root(),original_quota:jh.quota_record.root(),maximum_invocation:b.maximum_invocation,maximum_court:b.maximum_court,invocation:b.invocation,input_binding:b.input_binding,claim:a.claim,final_book:book.root(),settlement:object_id(b"misaka-palw/native/audit-stage-settlement/v28",s),single_credit:object_id(b"misaka-palw/native/audit-single-credit/v28",&(s,queue,payload)),payout_queue:queue.as_bytes(),requester_operator:book.requester_operator(),producer_operator:book.producer_operator(),admitted,committed:committed.ok_or("master32 receipt original Commit clock absent")?,established:s.established,challenge_end:book.challenge_end().ok_or("master32 original challenge clock absent")?,deadline:book.deadline(),reward:book.reward()});
        }
        let inventory=components.iter().map(|c|(c.stage,c.inventory)).collect::<Vec<_>>();
        let programs=components.iter().map(|c|(c.stage,c.program)).collect::<Vec<_>>();
        let pc=components.iter().map(|c|(c.stage,c.real_parameter)).collect::<Vec<_>>();
        let scopes=components.iter().map(|c|(c.stage,c.maximum_scope)).collect::<Vec<_>>();
        let resources=components.iter().map(|c|(c.stage,c.maximum_resource,c.original_quota,c.maximum_invocation,c.maximum_court,c.maximum_contract)).collect::<Vec<_>>();
        let credits=components.iter().map(|c|(c.stage,c.job,c.single_credit,c.payout_queue,c.reward)).collect::<Vec<_>>();
        Ok(NativeMasterExecutionRecordV32{version:32,master:master_id,composite,complete_source:h.source,manifest:h.manifest,complete_inventory:object_id(b"misaka-palw/native/all-original-component-inventory/v32",&(h.source,h.manifest,&inventory)),program_set:object_id(b"misaka-palw/native/all-original-component-programs/v32",&(h.source,&programs)),real_parameter_set:object_id(b"misaka-palw/native/all-original-real-parameters/v32",&(h.source,&pc)),maximum_scope_set:object_id(b"misaka-palw/native/all-original-requested-scopes/v32",&(master_id,&scopes)),maximum_resource_set:object_id(b"misaka-palw/native/all-original-requested-resources/v32",&(master_id,&resources)),single_credit_set:object_id(b"misaka-palw/native/all-original-single-credits/v32",&(master_id,&credits)),components})
    }
}
