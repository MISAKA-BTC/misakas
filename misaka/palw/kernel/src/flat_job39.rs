//! Original39 maximum invocation. Future decode IDs are absent before Fund;
//! Commit supplies actual first-EOS generation under this immutable contract.
use borsh::{BorshSerialize,BorshDeserialize};
use crate::{flat_source39::CheckedSourceV39,flat_resources39::{derive_flat_resources_v39,CheckedFlatResourcesV39},flat_material39::MaterialDescriptorV39,hash::{Digest,object_id}};
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub enum RequestStopV39 {Fixed{positions:u32,tokens:Vec<u32>},Decode{prompt:Vec<u32>,requested_new_tokens:u32,eos:Vec<u32>}}
impl RequestStopV39 {
    pub fn maximum_positions(&self)->Result<u32,String>{match self{Self::Fixed{positions,tokens}=>{if *positions==0||tokens.len()!=*positions as usize{return Err("job39 original fixed position/token contract".into());}Ok(*positions)},Self::Decode{prompt,requested_new_tokens,eos}=>{
        if prompt.is_empty()||*requested_new_tokens==0||eos.is_empty()||eos.len()>256||eos.windows(2).any(|w|w[0]>=w[1]){return Err("job39 original decode maximum/EOS contract".into());}
        u32::try_from(prompt.len()).ok().and_then(|n|n.checked_add(*requested_new_tokens)).and_then(|n|n.checked_sub(1)).ok_or("job39 maximum position overflow".into())}}}
    pub fn actual_tokens(&self,generated:&[u32],bound:u32)->Result<Vec<u32>,String>{let tokens=match self{Self::Fixed{tokens,..}=>{if !generated.is_empty(){return Err("job39 fixed stage cannot supply future decoder IDs".into());}tokens.clone()},Self::Decode{prompt,requested_new_tokens,eos}=>{
        if generated.is_empty()||generated.len()>*requested_new_tokens as usize||generated.iter().any(|t|*t>=bound)||generated[..generated.len()-1].iter().any(|t|eos.binary_search(t).is_ok())
            ||generated.len()<*requested_new_tokens as usize&&eos.binary_search(generated.last().unwrap()).is_err(){return Err("job39 actual generation is not firstEOS or original maximum stop".into());}
        let mut tokens=prompt.clone();tokens.extend_from_slice(&generated[..generated.len()-1]);tokens}};
        if tokens.iter().any(|t|*t>=bound){return Err("job39 original token outside source vocabulary".into());}Ok(tokens)}
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub enum MaterialSelectionV39 {Whole,RowAtPosition}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub enum InputBindingV39 {Material{descriptor:MaterialDescriptorV39,selection:MaterialSelectionV39},Random{seed:Digest,domain:u16,per_step:bool}}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct MaterialManifestV39 {pub version:u16,pub source:Digest,pub master:Digest,pub bindings:Vec<InputBindingV39>}
impl MaterialManifestV39{pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-input-material-manifest/v39",self)}}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct JobMaximumV39 {pub version:u16,pub source:Digest,pub requester:Digest,pub producer:Digest,pub nonce:Digest,pub stop:RequestStopV39,pub input_manifest:Digest}
impl JobMaximumV39{pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-job-maximum/v39",self)}}
pub struct CheckedJobMaximumV39 {request:JobMaximumV39,resources:CheckedFlatResourcesV39,manifest:MaterialManifestV39,positions:u32}
impl CheckedJobMaximumV39 {
    pub fn check(source:&CheckedSourceV39,r:&JobMaximumV39,m:&MaterialManifestV39)->Result<Self,String>{
        let positions=r.stop.maximum_positions()?;let p=source.program();
        if r.version!=39||r.source!=source.root()||[r.requester,r.producer].contains(&[0;64])||r.requester==r.producer||positions>source.original_context_max()
            ||m.version!=39||m.source!=source.root()||m.root()!=r.input_manifest||m.bindings.len()!=p.inputs.len(){return Err("job39 exact original source/request/material maximum differs".into());}
        match &r.stop{RequestStopV39::Decode{prompt,eos,..}=>{if !matches!(p.output,misaka_palw_tir::program_v2::OutputDecl::Logits{..})||prompt.iter().chain(eos).any(|t|*t>=p.token_bound){return Err("job39 source output/decode token contract".into());}},RequestStopV39::Fixed{tokens,..}=>if tokens.iter().any(|t|*t>=p.token_bound){return Err("job39 original fixed token range".into());}}
        for(d,b)in p.inputs.iter().zip(&m.bindings){match(b,d.source){(InputBindingV39::Material{descriptor,selection},misaka_palw_tir::program_v2::InputSource::External{..})=>{
            let expected=match selection{MaterialSelectionV39::Whole=>d.shape.clone(),MaterialSelectionV39::RowAtPosition=>std::iter::once(positions).chain(d.shape.iter().copied()).collect()};
            if descriptor.source!=source.root()||descriptor.master!=m.master||descriptor.requester!=r.requester||descriptor.nonce!=r.nonce||descriptor.dtype!=d.dtype||descriptor.shape!=expected{return Err("job39 material cannot expand original input shape or rebind upstream owner".into());}
            // Native registration independently authenticates the descriptor's
            // original paid seal and upstream Final before using this plan.
        },(InputBindingV39::Random{seed,domain,per_step},misaka_palw_tir::program_v2::InputSource::Random{domain:want,per_step:step,..})=>if *seed==[0;64]||*domain!=want||*per_step!=step{return Err("job39 exact original random source differs".into());},_=>return Err("job39 input origin differs from canonical declaration".into())}}
        let resources=derive_flat_resources_v39(source,positions)?;Ok(Self{request:r.clone(),resources,manifest:m.clone(),positions})
    }
    pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/checked-original-job-maximum/v39",&(self.request.root(),self.resources.root(),self.manifest.root()))}
    pub fn request(&self)->&JobMaximumV39{&self.request}pub fn resources(&self)->&CheckedFlatResourcesV39{&self.resources}pub fn maximum_positions(&self)->u32{self.positions}pub fn input_manifest(&self)->&MaterialManifestV39{&self.manifest}
    pub fn check_actual(&self,source:&CheckedSourceV39,generated:&[u32])->Result<(Vec<u32>,CheckedFlatResourcesV39),String>{if source.root()!=self.request.source{return Err("job39 actual source substitution".into());}
        let tokens=self.request.stop.actual_tokens(generated,source.program().token_bound)?;let positions=u32::try_from(tokens.len()).map_err(|_|"job39 actual position extent")?;
        if positions>self.positions{return Err("job39 actual length exceeds original maximum".into());}let actual=derive_flat_resources_v39(source,positions)?;
        actual.vector().quantities.within(&self.resources.vector().quantities).map_err(|_|"job39 actual resource exceeds original maximum")?;Ok((tokens,actual))}
}
