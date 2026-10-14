//! Original complete-pipeline invocation32. Canonical V2 source, real weights,
//! public job/random facts and every genuine upstream terminal remain distinct.
//! This module gives no authority to a declaration-only component list.
use super::{bounded28::*,source28::CheckedStageSourceV28,*};
use crate::{flat_component_source28::*,pipeline_public::PipelineJobFactsV1,flat_quota11::stage14::{bounded28::*,StageTraceV14}};
use borsh::{BorshSerialize,BorshDeserialize};
use misaka_palw_tir::{pipeline::{TirPipelineV1,Binding,TripRule,stage_job_facts},program_v2::InputSource};

#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct MasterRequestV32 {
    pub version:u16,pub complete_source:Digest,pub manifest:Digest,pub requester:Digest,pub nonce:Digest,
    pub pipeline:Vec<u8>,pub classes:Vec<Digest>,pub facts:Vec<u8>,pub maximum_new_tokens:u32,pub eos_token:u32,
}
pub struct CheckedMasterRequestV32 {wire:MasterRequestV32,pipeline:TirPipelineV1,programs:Vec<TirProgramV2>,facts:PipelineJobFactsV1,root:Digest}
impl MasterRequestV32 {
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{
        use std::io::{Cursor,Read};
        if raw.len()>2<<20{return Err("master32 bounded original carrier".into());}let mut r=Cursor::new(raw);
        let version=u16::deserialize_reader(&mut r).map_err(|e|e.to_string())?;if version!=32{return Err("master32 version".into());}
        let complete_source=Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let manifest=Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let requester=Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let nonce=Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        fn blob(r:&mut Cursor<&[u8]>,cap:usize)->Result<Vec<u8>,String>{let n=u32::deserialize_reader(r).map_err(|e|e.to_string())? as usize;if n>cap||n>r.get_ref().len().saturating_sub(r.position() as usize){return Err("master32 bounded nested extent".into());}let mut bytes=vec![0;n];r.read_exact(&mut bytes).map_err(|e|e.to_string())?;Ok(bytes)}
        let pipeline=blob(&mut r,misaka_palw_tir::pipeline::MAX_PIPELINE_BYTES)?;let n=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())? as usize;if n==0||n>64||n>raw.len().saturating_sub(r.position() as usize)/64{return Err("master32 bounded class count".into());}let mut classes=Vec::with_capacity(n);for _ in 0..n{classes.push(Digest::deserialize_reader(&mut r).map_err(|e|e.to_string())?);}
        let facts=blob(&mut r,1<<20)?;decode_facts_v32(&facts)?;let maximum_new_tokens=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let eos_token=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;
        let wire=Self{version,complete_source,manifest,requester,nonce,pipeline,classes,facts,maximum_new_tokens,eos_token};if r.position() as usize!=raw.len()||borsh::to_vec(&wire).map_err(|e|e.to_string())?!=raw{return Err("master32 canonical original framing".into());}Ok(wire)
    }
    pub fn check(&self,bundle:&CheckedStageBundleV28,classes:&[CheckedStageClassV28])->Result<CheckedMasterRequestV32,String>{
        if self.version!=32||self.complete_source!=bundle.root()||self.manifest!=bundle.manifest().root()||self.requester==[0;64]||classes.len()!=bundle.manifest().manifest().stages.len()||self.classes!=classes.iter().map(|c|c.root()).collect::<Vec<_>>(){return Err("master32 does not bind all actual original component classes".into());}
        for(i,c)in classes.iter().enumerate(){if c.wire().stage as usize!=i||c.wire().complete_source!=bundle.root(){return Err("master32 stage/class order or original complete source differs".into());}c.check_component_manifest(bundle.manifest())?;}
        let programs=classes.iter().map(|c|c.template().program.clone()).collect::<Vec<_>>();let pipeline=TirPipelineV1::decode_canonical(&self.pipeline,&programs).map_err(|e|e.to_string())?;
        if pipeline.stages.len()!=classes.len()||pipeline.stages.iter().enumerate().any(|(i,s)|s.program as usize!=i||s.max_trip>classes[i].wire().max_positions){return Err("master32 omitted/reordered component or raised original maximum context".into());}
        for(i,s)in pipeline.stages.iter().enumerate(){let p=&programs[i];let mut external=0usize;for(k,d)in p.inputs.iter().enumerate(){let declared=bundle.manifest().manifest().inputs.iter().find(|b|b.stage as usize==i&&b.input as usize==k).ok_or("master32 original input declaration binding absent")?;match d.source{
            InputSource::Random{domain,..}=>{if !matches!(declared.source,ComponentInputSourceV28::Random{domain:d,..} if d==domain){return Err("master32 original random domain binding differs".into());}},
            InputSource::External{..}=>{let b=s.bind.get(external).ok_or("master32 actual external binding absent")?;external+=1;match b{
                Binding::StageRows{stage,..}|Binding::StageFinal{stage}|Binding::StageRowCount{stage,..}=>{if !matches!(declared.source,ComponentInputSourceV28::Upstream{stage:d,..} if d==*stage as u16){return Err("master32 upstream stage differs from original complete source manifest".into());}},
                _=>if !matches!(declared.source,ComponentInputSourceV28::External{..}){return Err("master32 replaces a declared upstream input with unbound external data".into());},
            }},
        }}}
        let facts=decode_facts_v32(&self.facts)?;if !facts.finalized.is_empty(){return Err("master32 finalized external generations require their own native terminal bindings".into());}
        let stream=pipeline.stages.iter().position(|s|matches!(s.trip,TripRule::TextStream|TripRule::Decode));
        if let Some(index)=stream{if self.maximum_new_tokens==0||self.eos_token>=programs[index].token_bound||self.maximum_new_tokens>classes[index].wire().max_positions{return Err("master32 original EOS/max request exceeds stream source".into());}}else if self.maximum_new_tokens!=0{return Err("master32 nontext pipeline advertises a decode budget".into());}
        let root=object_id(b"misaka-palw/kernel/original-complete-master-request/v32",&(self,bundle.root(),bundle.manifest().root()));Ok(CheckedMasterRequestV32{wire:self.clone(),pipeline,programs,facts,root})
    }
}
impl CheckedMasterRequestV32 {
    pub fn root(&self)->Digest{self.root}
    pub fn wire(&self)->&MasterRequestV32{&self.wire}
    pub fn pipeline(&self)->&TirPipelineV1{&self.pipeline}
    pub fn facts(&self)->&PipelineJobFactsV1{&self.facts}
}
/// Pure mathematical projection of an original book/output relation. This
/// context and its checked output DO NOT authenticate native coins, table rows
/// or grant Final eligibility. Core separately authenticates those rows and
/// wraps this projection in its private native completion capability.
pub struct ComponentOutputContextV32 {
    pub master:Digest,pub invocation:Digest,pub inputs:Digest,pub job:Digest,pub funding:Digest,pub terminal:Digest,
    pub source:Digest,pub class:Digest,pub claim:Digest,pub credit_queue:Digest,pub actual_credit:u64,pub actual_escrow:u64,
    pub producer_operator:Digest,pub established:u64,
}
pub struct CheckedComponentOutputV32 {master:Digest,stage:u16,invocation:Digest,inputs:Digest,job:Digest,funding:Digest,terminal:Digest,claim:Digest,outputs:Vec<Tensor>,output_root:Digest,credit:Digest}
impl CheckedComponentOutputV32 {
    pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/genuine-complete-component-final/v32",&(self.master,self.stage,self.invocation,self.inputs,self.job,self.funding,self.terminal,self.claim,self.output_root,self.credit))}
    pub fn outputs(&self)->&[Tensor]{&self.outputs}
    pub fn master(&self)->Digest{self.master}
    pub fn stage(&self)->u16{self.stage}
    pub fn input_binding_root(&self)->Digest{self.inputs}
}
pub fn check_component_output_v32(invocation:&CheckedStageInvocationV32<'_>,book:&StageJobBookV28,quota:&crate::flat_quota11::stage14::bounded28::CheckedStageQuotaV28,trace:&StageTraceV14,values:&[Tensor],n:&ComponentOutputContextV32)->Result<CheckedComponentOutputV32,String>{
    book.check(invocation.class,&invocation.job,quota)?;let s=book.settlement().ok_or("component32 genuine settlement absent")?;
    if book.terminal()!=StageTerminalV28::Final||n.master!=invocation.master_root()||n.invocation!=invocation.invocation_root()||n.inputs!=invocation.input_binding_root()||n.job!=book.id()||n.funding!=book.original_root()||n.terminal!=book.root()||n.class!=invocation.class.root()||n.source!=invocation.source().id()||Some(n.claim)!=book.claim()||n.actual_credit!=s.producer_credit||n.actual_credit!=book.reward()||n.actual_escrow!=s.escrow_debit||n.credit_queue==[0;64]||n.producer_operator!=book.producer_operator()||n.established!=s.established||trace.job_id!=book.id()||trace.class!=book.class()||trace.job_input!=book.job().root()||trace.admission!=invocation.job.admission().root()||trace.quota!=quota.root()||Some(object_id(b"misaka-palw/kernel/stage-authenticated-trace/v28",&(book.contract(),trace.root())))!=book.claim()||values.len()!=trace.output_openings.len(){return Err("component32 lacks genuine original Master/source/invocation/claim/actual once credit".into());}
    let mut bytes=0usize;let p=invocation.actual_program().v1_view();let pr=crate::element::PreparedV1::without_stage_v32(&p,invocation.actual_program())?;let occ=u16::try_from(pr.w.occurrences.len()-1).map_err(|_|"component32 output occurrence overflow")?;let node=invocation.actual_program().output.node();
    for(i,(value,opening))in values.iter().zip(&trace.output_openings).enumerate(){bytes=bytes.checked_add(value.data.len().checked_mul(value.dtype.width()).ok_or("component32 output bytes overflow")?).ok_or("component32 output sum overflow")?;if bytes>16<<20{return Err("component32 bounded genuine output extent".into());}let q=if matches!(invocation.actual_program().output,OutputDecl::Final{..}){invocation.positions()-1}else{i as u32};let shape=pr.shapes(q,occ,node).out;if value.shape!=shape||value.dtype!=pr.w.node(occ,node).out.dtype||opening.position!=q||opening.occurrence!=occ||opening.node!=node||!opening.authenticates(&trace.segment_roots,invocation.positions(),pr.index(occ,node),pr.node_count)||crate::flat_court5::flat_profile_commitment_v5(value).map_err(|e|e.to_string())?!=opening.commitment{return Err("component32 actual output image does not authenticate original trace/shape/flat root".into());}}
    let output_root=object_id(b"misaka-palw/kernel/genuine-component-delivered-outputs/v32",&(invocation.invocation_root(),&trace.output_openings));Ok(CheckedComponentOutputV32{master:n.master,stage:invocation.stage,invocation:n.invocation,inputs:n.inputs,job:n.job,funding:n.funding,terminal:n.terminal,claim:n.claim,outputs:values.to_vec(),output_root,credit:n.credit_queue})
}
pub struct CheckedStageInvocationV32<'a> {class:&'a CheckedStageClassV28,job:CheckedStageJobV28,master:Digest,stage:u16,inputs:Vec<Vec<Tensor>>,roots:Vec<Vec<Digest>>,input_root:Digest,prompt:Vec<u32>,maximum_new_tokens:u32,eos_token:u32,root:Digest}
impl<'a> CheckedStageInvocationV32<'a> {
    pub fn source(&self)->&CheckedStageSourceV28{self.class.source()}
    pub fn actual_program(&self)->&TirProgramV2{self.job.program()}
    pub fn real_parameters(&self)->&ParamCommitmentsV1{self.class.source().parameters()}
    pub fn combined_parameters_at(&self,position:u32)->Result<ParamCommitmentsV1,String>{let row=self.roots.get(position as usize).ok_or("invocation32 actual input position absent")?;let mut pc=self.real_parameters().clone();let first=self.source().program().params.len();for(i,root)in row.iter().enumerate(){pc.by_instance.insert(((first+i) as u16,None),*root);}Ok(pc)}
    pub fn master_root(&self)->Digest{self.master}
    pub fn input_binding_root(&self)->Digest{self.input_root}
    pub fn invocation_root(&self)->Digest{self.root}
    pub fn prompt(&self)->&[u32]{&self.prompt}
    pub fn maximum_new_tokens(&self)->u32{self.maximum_new_tokens}
    pub fn eos_token(&self)->u32{self.eos_token}
    pub fn positions(&self)->u32{self.job.job().positions}
    pub fn job(&self)->&CheckedStageJobV28{&self.job}
    pub fn input(&self,input:u16,position:u32)->Option<&Tensor>{self.inputs.get(position as usize)?.get(input as usize)}
}
/// All external and Random values are derived from the immutable master facts.
/// Every upstream tensor comes from a private genuine Final token of THIS
/// master. The native caller additionally supplies only its privately checked
/// original completion capabilities; these kernel projections alone never
/// authorize a native Final or a monetary credit.
pub fn check_stage_invocation_v32<'a>(master:&CheckedMasterRequestV32,class:&'a CheckedStageClassV28,job:&super::bounded14::FlatStageJobV14,generated:&[u32],upstream:&[CheckedComponentOutputV32])->Result<CheckedStageInvocationV32<'a>,String>{
    let stage=class.wire().stage as usize;if master.wire.classes.get(stage)!=Some(&class.root())||job.requester!=master.wire.requester{return Err("invocation32 original Master/class/requester differs".into());}
    let actual=class.check_job(job)?;let mut programs=master.programs.clone();programs[stage]=actual.program().clone();let pipeline_job=master.facts.job(generated);let facts=stage_job_facts(&master.pipeline,&programs,&pipeline_job).map_err(|e|e.to_string())?;
    if facts[stage].trip!=job.positions{return Err("invocation32 exact actual trip differs from original public facts".into());}
    if upstream.iter().any(|u|u.master!=master.root||u.stage as usize>=stage)||upstream.windows(2).any(|w|w[0].stage>=w[1].stage){return Err("invocation32 upstream Final belongs to another Master/order or is repeated".into());}
    let upstream_values=|u:u8|upstream.iter().find(|f|f.stage==u as u16).map(|f|f.outputs.clone());let mut inputs=Vec::with_capacity(job.positions as usize);let mut roots=Vec::with_capacity(job.positions as usize);let mut bytes=0usize;
    for q in 0..job.positions{let mut row=Vec::with_capacity(actual.program().inputs.len());let mut pcs=Vec::with_capacity(row.capacity());for k in 0..actual.program().inputs.len(){let value=crate::pipeline::stage_input_v1(&master.pipeline,&programs,&facts,&pipeline_job,&master.facts.random(),stage,k as u16,q,&upstream_values)?;bytes=bytes.checked_add(value.data.len().checked_mul(value.dtype.width()).ok_or("invocation32 public input bytes overflow")?).ok_or("invocation32 public input sum overflow")?;if bytes>1<<20{return Err("invocation32 position-specific input images exceed original paid cap".into());}pcs.push(tensor_commitment_v3(&value));row.push(value);}if q==0&&pcs!=job.input_roots{return Err("invocation32 first actual input PC differs from native before-Fund job".into());}inputs.push(row);roots.push(pcs);}
    let input_root=object_id(b"misaka-palw/kernel/original-master-position-input-bindings/v32",&(master.root,stage as u16,job.root(),&roots,upstream.iter().map(|f|f.root()).collect::<Vec<_>>(),master.facts.random().binding()));let root=object_id(b"misaka-palw/kernel/original-complete-stage-invocation/v32",&(master.root,stage as u16,class.root(),actual.admission().root(),input_root,master.wire.maximum_new_tokens,master.wire.eos_token));
    Ok(CheckedStageInvocationV32{class,job:actual,master:master.root,stage:stage as u16,inputs,roots,input_root,prompt:if matches!(class.template().program.output,OutputDecl::Logits{..}){master.facts.prompt.clone()}else{facts[stage].tokens.clone()},maximum_new_tokens:master.wire.maximum_new_tokens,eos_token:master.wire.eos_token,root})
}
/// Pure composition relation requires every stage output. This returned digest
/// is NOT a native Final or monetary authority; the Core private closure first
/// authenticates every original completion/coin receipt and its spent anchor.
pub fn check_master_final_v32(master:&CheckedMasterRequestV32,finals:&[CheckedComponentOutputV32])->Result<Digest,String>{
    if finals.len()!=master.wire.classes.len()||finals.iter().enumerate().any(|(i,f)|f.master!=master.root||f.stage as usize!=i)||finals.iter().map(|f|f.credit).collect::<std::collections::BTreeSet<_>>().len()!=finals.len(){return Err("master32 Final lacks every genuine component or repeats monetary credit".into());}
    Ok(object_id(b"misaka-palw/kernel/original-all-components-single-master-final/v32",&(master.root,finals.iter().map(|f|f.root()).collect::<Vec<_>>(),finals[master.pipeline.output_stage as usize].output_root)))
}
fn decode_facts_v32(raw:&[u8])->Result<PipelineJobFactsV1,String>{
    use std::io::{Cursor,Read};if raw.len()>1<<20{return Err("master32 original job facts cap".into());}let mut r=Cursor::new(raw);
    fn count(r:&mut Cursor<&[u8]>,cap:usize,width:usize)->Result<usize,String>{let n=u32::deserialize_reader(r).map_err(|e|e.to_string())? as usize;if n>cap||n>r.get_ref().len().saturating_sub(r.position() as usize)/width{return Err("master32 bounded job nested count".into());}Ok(n)}
    fn ids(r:&mut Cursor<&[u8]>)->Result<Vec<u32>,String>{let n=count(r,262144,4)?;let mut out=Vec::with_capacity(n);for _ in 0..n{out.push(u32::deserialize_reader(r).map_err(|e|e.to_string())?);}Ok(out)}
    let prompt=ids(&mut r)?;let negative=ids(&mut r)?;let steps=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let n=count(&mut r,16,8)?;let mut scalars=Vec::with_capacity(n);for _ in 0..n{scalars.push(i64::deserialize_reader(&mut r).map_err(|e|e.to_string())?);}let n=count(&mut r,16,12)?;let mut images=Vec::with_capacity(n);for _ in 0..n{let h=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let w=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let n=count(&mut r,1<<20,1)?;let mut rgb=vec![0;n];r.read_exact(&mut rgb).map_err(|e|e.to_string())?;images.push((h,w,rgb));}let source=ids(&mut r)?;let key=ids(&mut r)?;let n=count(&mut r,0,6)?;let finalized=Vec::with_capacity(n);let seed=<[u8;32]>::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let item=u32::deserialize_reader(&mut r).map_err(|e|e.to_string())?;let facts=PipelineJobFactsV1{prompt,negative,steps,scalars,images,source,key,finalized,seed,item};if r.position() as usize!=raw.len()||borsh::to_vec(&facts).map_err(|e|e.to_string())?!=raw{return Err("master32 job facts canonical framing".into());}Ok(facts)
}
