//! Explicit39 material store contract. The native adapter supplies original
//! requester signatures/coin debit and upstream Final authority; this module
//! validates byte geometry, bounded page attempts and actual source C_v3.
use borsh::{BorshSerialize,BorshDeserialize};
use crate::{hash::{Digest,object_id},merkle3::{LayoutV3,AXIS_ROW,AXIS_COL,leaf_hash_v3,node_hash_v3,commit_v3}};
use misaka_palw_tir::DType;
pub const MATERIAL_PAGE_BYTES_V39:u32=32<<10;
pub const MATERIAL_MAX_PAGES_V39:u16=8192;
pub const MATERIAL_MAX_RAW_V39:u64=256<<20;
pub const MATERIAL_MAX_EXPANDED_V39:u64=2<<30;
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub enum MaterialOriginV39 {
    External{original_task:Digest,raw_source:Digest},
    Upstream{master:Digest,stage:u16,job:Digest,class:Digest,source:Digest,claim:Digest,final_book:Digest,single_credit:Digest,output:Digest},
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct MaterialDescriptorV39 {pub version:u16,pub source:Digest,pub master:Digest,pub requester:Digest,pub requester_operator:Digest,pub nonce:Digest,pub dtype:DType,pub shape:Vec<u32>,pub tensor:Digest,pub page_root:Digest,pub origin:MaterialOriginV39}
impl MaterialDescriptorV39 {
    pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-material-descriptor/v39",self)}
    /// Excludes page_root, avoiding a circular page tree/descriptor commitment.
    pub fn content_binding(&self)->Digest{object_id(b"misaka-palw/kernel/original-material-content/v39",&(self.version,self.source,self.master,self.requester,self.requester_operator,self.nonce,self.dtype,&self.shape,self.tensor,&self.origin))}
}
pub fn material_page_leaf_v39(descriptor:&MaterialDescriptorV39,index:u16,body:&[u8])->Digest{object_id(b"misaka-palw/kernel/material-page/v39",&(descriptor.content_binding(),index,body))}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct MaterialPlanV39 {pub version:u16,pub descriptor:Digest,pub source:Digest,pub master:Digest,pub requester:Digest,pub requester_operator:Digest,pub nonce:Digest,pub tensor:Digest,pub page_root:Digest,pub pages:u16,pub raw_bytes:u64,pub expanded_bytes:u64,pub start:u64,pub deadline:u64,pub page_attempts:u8,pub seal_attempts:u8,pub stored_bytes:u64,pub delivery_bytes:u64,pub hashing_bytes:u128,pub read_calls:u128,pub scratch_bytes:u64,pub owned_bytes:u64}
impl MaterialPlanV39{pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-material-plan/v39",self)}}
pub struct CheckedMaterialPlanV39 {descriptor:MaterialDescriptorV39,plan:MaterialPlanV39}
impl CheckedMaterialPlanV39 {
    pub fn derive(d:&MaterialDescriptorV39,start:u64,deadline:u64)->Result<Self,String>{
        if d.version!=39||d.shape.len()>4||d.shape.iter().any(|n|*n==0||*n>1<<24)||[d.source,d.master,d.requester,d.requester_operator,d.tensor,d.page_root].contains(&[0;64])||deadline<=start{return Err("material39 original identity/shape/deadline".into());}
        let cells=d.shape.iter().try_fold(1u64,|n,d|n.checked_mul(*d as u64)).ok_or("material39 shape overflow")?;
        let raw=cells.checked_mul(d.dtype.width()as u64).filter(|n|*n>0&&*n<=MATERIAL_MAX_RAW_V39).ok_or("material39 raw extent exceeds explicit contract")?;
        let expanded=cells.checked_mul(16).filter(|n|*n<=MATERIAL_MAX_EXPANDED_V39).ok_or("material39 expanded extent exceeds explicit contract")?;
        let pages=u16::try_from(raw.div_ceil(MATERIAL_PAGE_BYTES_V39 as u64)).map_err(|_|"material39 page count")?;if pages>MATERIAL_MAX_PAGES_V39{return Err("material39 original page cap".into());}
        match &d.origin{MaterialOriginV39::External{original_task,raw_source}=>if [*original_task,*raw_source].contains(&[0;64]){return Err("material39 original external provenance absent".into());},MaterialOriginV39::Upstream{master,job,class,source,claim,final_book,single_credit,output,..}=>if *master!=d.master||*output!=d.tensor||[*job,*class,*source,*claim,*final_book,*single_credit].contains(&[0;64]){return Err("material39 original upstream Final/output provenance absent".into());}}
        let plan=MaterialPlanV39{version:39,descriptor:d.root(),source:d.source,master:d.master,requester:d.requester,requester_operator:d.requester_operator,nonce:d.nonce,tensor:d.tensor,page_root:d.page_root,pages,raw_bytes:raw,expanded_bytes:expanded,start,deadline,page_attempts:2,seal_attempts:2,stored_bytes:raw,delivery_bytes:raw.checked_mul(2).ok_or("material39 delivery extent")?,hashing_bytes:raw as u128*6,read_calls:cells as u128*4+pages as u128*4,scratch_bytes:512<<10,owned_bytes:raw.checked_mul(4).and_then(|n|n.checked_add(expanded)).and_then(|n|n.checked_add(1<<20)).ok_or("material39 original complete owned peak")?};
        Ok(Self{descriptor:d.clone(),plan})
    }
    pub fn record(&self)->&MaterialPlanV39{&self.plan}
    pub fn descriptor(&self)->&MaterialDescriptorV39{&self.descriptor}
    pub fn root(&self)->Digest{self.plan.root()}
    pub fn part_bytes(&self,index:u16)->Result<usize,String>{if index>=self.plan.pages{return Err("material39 page index outside original plan".into());}Ok((self.plan.raw_bytes-index as u64*MATERIAL_PAGE_BYTES_V39 as u64).min(MATERIAL_PAGE_BYTES_V39 as u64)as usize)}
}
/// Compact progress lives in aux; large pages must be streamed by audit39, not
/// silently loaded through audit32's unchanged64MiB full-opening contract.
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct MaterialProgressV39 {pub version:u16,pub plan:Digest,pub original_debit:Digest,pub owner:Digest,pub admitted:u64,pub deadline:u64,pub attempts:Vec<u8>,pub accepted:Vec<u8>,pub seal_attempts:u8,pub consumed:bool,pub sealed:bool}
impl MaterialProgressV39 {
    /// A native adapter calls only after the exact original requester debit and
    /// shared quota reservation have committed atomically. This is a pure row.
    pub fn new(plan:&CheckedMaterialPlanV39,original_debit:Digest)->Result<Self,String>{if original_debit==[0;64]{return Err("material39 original actual debit missing".into());}let p=plan.record();Ok(Self{version:39,plan:plan.root(),original_debit,owner:p.requester,admitted:p.start,deadline:p.deadline,attempts:vec![0;p.pages as usize],accepted:vec![0;(p.pages as usize).div_ceil(8)],seal_attempts:0,consumed:false,sealed:false})}
    pub fn check(&self,p:&CheckedMaterialPlanV39)->Result<(),String>{let r=p.record();if self.version!=39||self.plan!=p.root()||self.owner!=r.requester||self.admitted!=r.start||self.deadline!=r.deadline||self.original_debit==[0;64]||self.attempts.len()!=r.pages as usize||self.accepted.len()!=(r.pages as usize).div_ceil(8)||self.attempts.iter().any(|n|*n>2)||self.seal_attempts>2{return Err("material39 original progress binding differs".into());}Ok(())}
    /// Persist this increment BEFORE part body allocation or decoding, including
    /// failed attempts. No arbitrary actor can spend another requester's plan.
    pub fn debit_part(&mut self,p:&CheckedMaterialPlanV39,owner:Digest,index:u16,now:u64)->Result<(),String>{self.check(p)?;if owner!=self.owner||now<self.admitted||now>self.deadline||self.sealed||self.consumed{return Err("material39 original part owner/clock/state".into());}p.part_bytes(index)?;let n=&mut self.attempts[index as usize];if *n>=2{return Err("material39 original part attempts exhausted".into());}*n+=1;Ok(())}
    pub fn accept_part(&mut self,p:&CheckedMaterialPlanV39,index:u16,body:&[u8],siblings:&[Digest])->Result<(),String>{self.check(p)?;if p.part_bytes(index)?!=body.len()||self.attempts[index as usize]==0||self.accepted[index as usize/8]&(1<<(index%8))!=0{return Err("material39 exact paid original page differs/repeats".into());}
        let leaf=material_page_leaf_v39(p.descriptor(),index,body);let root=crate::merkle3::tree_root_from_path(leaf,index as u64,p.record().pages as u64,siblings,material_page_node_v39).ok_or("material39 page path grammar")?;
        if root!=p.record().page_root{return Err("material39 page not original descriptor root".into());}self.accepted[index as usize/8]|=1<<(index%8);Ok(())}
    pub fn debit_seal(&mut self,p:&CheckedMaterialPlanV39,owner:Digest,now:u64)->Result<(),String>{self.check(p)?;if owner!=self.owner||now<self.admitted||now>self.deadline||self.sealed||self.consumed||self.seal_attempts>=2||(0..p.record().pages as usize).any(|i|self.accepted[i/8]&(1<<(i%8))==0){return Err("material39 original seal attempt unavailable".into());}self.seal_attempts+=1;Ok(())}
    pub fn accept_seal(&mut self,p:&CheckedMaterialPlanV39,checked:&CheckedMaterialImageV39)->Result<(),String>{self.check(p)?;if self.seal_attempts==0||self.sealed||checked.plan!=p.root(){return Err("material39 original seal result differs".into());}self.sealed=true;Ok(())}
}
fn material_page_node_v39(a:&Digest,b:&Digest)->Digest{object_id(b"misaka-palw/kernel/material-page-node/v39",&(a,b))}
struct Frontier {levels:[Option<Digest>;64]}
impl Frontier{fn new()->Self{Self{levels:[None;64]}}fn push(&mut self,mut x:Digest)->Result<(),String>{for slot in &mut self.levels{match slot.take(){None=>{*slot=Some(x);return Ok(());},Some(left)=>x=node_hash_v3(&left,&x)}}Err("material39 merkle frontier overflow".into())}fn finish(self)->Result<Digest,String>{let mut root=None;for x in self.levels.into_iter().flatten(){root=Some(match root{None=>x,Some(r)=>node_hash_v3(&x,&r)});}root.ok_or("material39 empty tensor frontier".into())}}
/// Original C_v3, streamed with at most one4096-cell tile and logarithmic
/// frontier. The callback must read the same immutable descriptor/page root on
/// EVERY call. It is shared by source39 writers and native material sealing.
pub fn tensor_commitment_streamed_v39(dtype:DType,shape:&[usize],mut read_values:impl FnMut(u64,usize)->Result<Vec<i128>,String>)->Result<Digest,String>{
    let layout=LayoutV3::try_of(shape).ok_or("material39 original tensor layout")?;if shape.len()>4||layout.len==0{return Err("material39 empty/rank tensor".into());}
    let mut roots=Vec::with_capacity(2);for axis in [AXIS_ROW,AXIS_COL]{let mut tree=Frontier::new();let(lines,tiles)=if axis==AXIS_ROW{(layout.rows,layout.row_tiles)}else{(layout.cols,layout.col_tiles)};
        for line in 0..lines{for tile in 0..tiles{let indices=layout.leaf_elements(axis,line,tile).ok_or("material39 original leaf geometry")?;let mut values=Vec::with_capacity(indices.len());
            let contiguous=indices.windows(2).all(|w|w[1]==w[0]+1);if contiguous{if let Some(first)=indices.first(){values=read_values(*first,indices.len())?;}}else{for i in &indices{let v=read_values(*i,1)?;if v.len()!=1{return Err("material39 source callback range extent".into());}values.push(v[0]);}}
            if values.len()!=indices.len()||values.iter().any(|v|!dtype.contains(*v)){return Err("material39 actual original dtype/range differs".into());}tree.push(leaf_hash_v3(dtype,axis,line,tile,&values))?;}}
        roots.push(tree.finish()?);}
    Ok(commit_v3(dtype,shape,&roots[0],&roots[1]))
}
pub struct CheckedMaterialImageV39 {plan:Digest}
pub fn check_material_image_v39(plan:&CheckedMaterialPlanV39,mut read_raw:impl FnMut(u64,usize)->Result<Vec<u8>,String>)->Result<CheckedMaterialImageV39,String>{
    let d=plan.descriptor();let shape:Vec<_>=d.shape.iter().map(|n|*n as usize).collect();let width=d.dtype.width();
    let tensor=tensor_commitment_streamed_v39(d.dtype,&shape,|offset,count|{let start=offset.checked_mul(width as u64).ok_or("material39 raw read overflow")?;let bytes=count.checked_mul(width).ok_or("material39 raw read overflow")?;let raw=read_raw(start,bytes)?;
        if raw.len()!=bytes{return Err("material39 original material byte range missing".into());}Ok(raw.chunks_exact(width).map(|b|d.dtype.decode_le(b)).collect())})?;
    if tensor!=d.tensor{return Err("material39 actual original tensor C_v3 differs".into());}Ok(CheckedMaterialImageV39{plan:plan.root()})
}
