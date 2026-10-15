//! Owning39 source binding. Parameter commitments name ONLY original model
//! parameters. External/upstream/random inputs are independent declarations.
use borsh::{BorshDeserialize,BorshSerialize};
use std::{collections::{BTreeMap,BTreeSet},io::{Cursor,Read}};
use crate::{hash::{Digest,object_id},trace::ParamCommitmentsV1};
use misaka_palw_tir::program_v39::{SourceProfileV39,TirProgramV39};
pub const SOURCE39_MAX_BYTES:usize=(16<<20)+16384*69+256;
/// Independently streamed ORIGINAL tensor descriptors, separate from candidate
/// PC and job inputs. Each C_v3 is computed from original bytes by the writer.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct OriginalTensorV39 {pub param:u16,pub layer:Option<u16>,pub dtype:misaka_palw_tir::DType,pub shape:Vec<u32>,pub raw_bytes:u64,pub source_tensor:Digest}
/// Canonical new inventory identity. This proves exact geometry/commitment
/// agreement, not that a caller's repository label is semantically truthful.
pub fn derive_inventory_root_v39(program:&TirProgramV39,original_task:Digest,records:&[OriginalTensorV39])->Result<Digest,String>{
    misaka_palw_tir::validate_v39::validate_v39(program)?;
    if original_task==[0;64]||records.len()>16384{return Err("source39 original inventory extent/task".into());}
    let mut expected=Vec::new();for (j,d) in program.params.iter().enumerate(){let layers:Vec<_>=if d.per_layer{(0..program.schedule.layers.len()).map(|l|Some(l as u16)).collect()}else{vec![None]};
        let n=d.shape.iter().try_fold(1u64,|n,d|n.checked_mul(*d as u64)).ok_or("source39 original tensor product")?;
        let bytes=n.checked_mul(d.dtype.width()as u64).ok_or("source39 original bytes product")?;
        for layer in layers{expected.push((j as u16,layer,d.dtype,&d.shape,bytes));}}
    if expected.len()!=records.len(){return Err("source39 original tensor inventory coverage".into());}
    for(e,r)in expected.iter().zip(records){if (r.param,r.layer,r.dtype,&r.shape,r.raw_bytes)!=(e.0,e.1,e.2,e.3,e.4)||r.source_tensor==[0;64]{return Err("source39 original tensor record key/dtype/shape/byte extent".into());}}
    Ok(object_id(b"misaka-palw/source/original-tensor-inventory/v39",&(39u16,original_task,&program.profile,records)))
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct FlatSourceWireV39 {
    pub version:u16,pub program_bytes:Vec<u8>,pub inventory_root:Digest,pub real_parameters:ParamCommitmentsV1,
    pub original_context_max:u32,pub original_task_metadata:Digest,
}
#[derive(Clone)]
pub struct CheckedSourceV39 {wire:FlatSourceWireV39,program:TirProgramV39,id:Digest}
impl CheckedSourceV39 {
    pub fn root(&self)->Digest{self.id}
    pub fn program(&self)->&TirProgramV39{&self.program}
    pub fn profile(&self)->&SourceProfileV39{&self.program.profile}
    pub fn parameters(&self)->&ParamCommitmentsV1{&self.wire.real_parameters}
    pub fn inventory_root(&self)->Digest{self.wire.inventory_root}
    pub fn original_context_max(&self)->u32{self.wire.original_context_max}
    pub fn original_task_metadata(&self)->Digest{self.wire.original_task_metadata}
    pub fn program_root(&self)->Digest{object_id(b"misaka-palw/source/program/v39",&self.wire.program_bytes)}
    pub fn profile_root(&self)->Digest{object_id(b"misaka-palw/source/profile/v39",&self.program.profile)}
    pub fn input_declarations_root(&self)->Digest{object_id(b"misaka-palw/source/input-declarations/v39",&self.program.inputs)}
    pub fn relation_root(&self)->Digest{object_id(b"misaka-palw/source/real-weight-relation/v39",&(39u16,&self.program.params,self.parameters().root(),self.inventory_root()))}
    pub fn wire(&self)->&FlatSourceWireV39{&self.wire}
}
impl FlatSourceWireV39 {
    pub fn check(&self)->Result<CheckedSourceV39,String>{
        if self.version!=39||self.program_bytes.len()>16<<20||self.real_parameters.by_instance.len()>16384
            ||[self.inventory_root,self.original_task_metadata].contains(&[0;64]){return Err("source39 original binding extent/roots".into());}
        let program=TirProgramV39::decode_canonical(&self.program_bytes)?;
        if self.original_context_max==0||self.original_context_max>program.history_bound{return Err("source39 independent original context exceeds owning program".into());}
        let mut keys=BTreeSet::new();for (j,d) in program.params.iter().enumerate(){if d.per_layer{for l in 0..program.schedule.layers.len(){keys.insert((j as u16,Some(l as u16)));}}else{keys.insert((j as u16,None));}}
        if keys.len()>16384||keys!=self.real_parameters.by_instance.keys().copied().collect()||self.real_parameters.by_instance.values().any(|r|*r==[0;64]){
            return Err("source39 real PC namespace contains missing weights or virtual job inputs".into());}
        let id=object_id(b"misaka-palw/source/binding/v39",&(39u16,object_id(b"misaka-palw/source/program/v39",&self.program_bytes),
            object_id(b"misaka-palw/source/profile/v39",&program.profile),object_id(b"misaka-palw/source/input-declarations/v39",&program.inputs),
            self.inventory_root,self.real_parameters.root(),self.original_context_max,self.original_task_metadata));
        Ok(CheckedSourceV39{wire:self.clone(),program,id})
    }
    pub fn to_bytes(&self)->Vec<u8>{borsh::to_vec(self).expect("source39 Vec encoding")}
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{
        if raw.len()>SOURCE39_MAX_BYTES{return Err("source39 wire cap".into());}let mut r=Cursor::new(raw);
        let version=u16::deserialize_reader(&mut r).map_err(|_|"source39 version grammar")?;if version!=39{return Err("source39 version".into());}
        let n=u32::deserialize_reader(&mut r).map_err(|_|"source39 byte count")?as usize;if n>16<<20||n>raw.len().saturating_sub(r.position()as usize){return Err("source39 canonical program byte extent".into());}
        let mut program_bytes=vec![0;n];r.read_exact(&mut program_bytes).map_err(|_|"source39 program body")?;
        let inventory_root=Digest::deserialize_reader(&mut r).map_err(|_|"source39 inventory")?;
        let n=u32::deserialize_reader(&mut r).map_err(|_|"source39 PC count")?as usize;if n>16384||n>raw.len().saturating_sub(r.position()as usize)/67{return Err("source39 real PC extent".into());}
        let mut by_instance=BTreeMap::new();let mut prior=None;for _ in 0..n{let key=<(u16,Option<u16>)>::deserialize_reader(&mut r).map_err(|_|"source39 PC key")?;
            if prior.is_some_and(|p|p>=key){return Err("source39 PC canonical order".into());}prior=Some(key);by_instance.insert(key,Digest::deserialize_reader(&mut r).map_err(|_|"source39 PC root")?);}
        let original_context_max=u32::deserialize_reader(&mut r).map_err(|_|"source39 independent original context")?;
        let original_task_metadata=Digest::deserialize_reader(&mut r).map_err(|_|"source39 original task binding")?;
        let w=Self{version,program_bytes,inventory_root,real_parameters:ParamCommitmentsV1{by_instance},original_context_max,original_task_metadata};
        if r.position()!=raw.len()as u64||w.to_bytes()!=raw{return Err("source39 exact canonical frame".into());}w.check()?;Ok(w)
    }
}
