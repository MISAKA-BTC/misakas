//! Authenticated scalar39 courts over the original owning program, full actual
//! H, real source-PC, input roots and previous-position state writers. The
//! accepted claim root is supplied by native original-book extraction.
use std::{collections::BTreeMap,io::{Cursor,Read}};
use borsh::{BorshSerialize,BorshDeserialize};
use crate::{hash::{Digest,object_id},flat_prepared39::{PreparedV39,OperandSourceV39},flat_resources39::*,flat_source39::CheckedSourceV39,
    element::{OperandsV1,MissingV1},merkle3::LeafOpeningV3,seg::NodeOpeningV1};
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub struct FlatClaimV39 {pub version:u16,pub source:Digest,pub maximum_scope:Digest,pub actual_scope:Digest,pub input_binding:Digest,pub positions:u32,pub segment_roots:Vec<Digest>}
impl FlatClaimV39{pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/original-claim/v39",self)}}
pub struct CheckedPointContextV39<'a>{prepared:PreparedV39<'a>,scope:CheckedFlatResourcesV39,claim:&'a FlatClaimV39,input_roots:&'a BTreeMap<(u32,u8),Digest>,tokens:&'a[u32]}
impl<'a> CheckedPointContextV39<'a>{
    pub fn check(source:&'a CheckedSourceV39,maximum_positions:u32,accepted_claim:Digest,claim:&'a FlatClaimV39,input_roots:&'a BTreeMap<(u32,u8),Digest>,tokens:&'a[u32])->Result<Self,String>{
        let maximum=derive_flat_resources_v39(source,maximum_positions)?;let scope=derive_flat_resources_v39(source,claim.positions)?;
        if claim.version!=39||claim.source!=source.root()||claim.root()!=accepted_claim||claim.maximum_scope!=maximum.scope().root()||claim.actual_scope!=scope.scope().root()
            ||claim.positions>maximum_positions||claim.segment_roots.len()!=crate::seg::segment_count_v1(claim.positions)as usize||claim.segment_roots.iter().any(|r|*r==[0;64])
            ||tokens.len()!=claim.positions as usize||tokens.iter().any(|t|*t>=source.program().token_bound){return Err("point39 original accepted claim/max/actual/input scope differs".into());}
        // Exactly one actual input root for each declaration and position. A
        // native invocation must authenticate upstream/random values separately
        // before obtaining this original input_binding in its funded book.
        let expected=claim.positions as usize*source.program().inputs.len();if input_roots.len()!=expected{return Err("point39 original actual input root coverage".into());}
        for p in 0..claim.positions{for i in 0..source.program().inputs.len(){if input_roots.get(&(p,i as u8)).is_none_or(|r|*r==[0;64]){return Err("point39 original input root missing".into());}}}
        if object_id(b"misaka-palw/kernel/original-input-binding/v39",&(source.root(),input_roots,tokens))!=claim.input_binding{return Err("point39 actual input root binding differs".into());}
        scope.vector().quantities.within(&maximum.vector().quantities).map_err(|_|"point39 actual resources exceed original maximum")?;
        Ok(Self{prepared:PreparedV39::new(source,claim.positions)?,scope,claim,input_roots,tokens})
    }
}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct PointOperandV39 {pub node:Option<NodeOpeningV1>,pub leaves:Vec<LeafOpeningV3>}
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub struct FlatPointFaultV39 {pub version:u16,pub output:NodeOpeningV1,pub element:u64,pub output_leaf:LeafOpeningV3,pub operands:Vec<PointOperandV39>}
fn read<T:BorshDeserialize>(r:&mut Cursor<&[u8]>)->Result<T,String>{T::deserialize_reader(r).map_err(|_|"point39 bounded canonical field".into())}
fn bounded_count(r:&mut Cursor<&[u8]>,cap:usize)->Result<usize,String>{let n:u32=read(r)?;if n as usize>cap{return Err("point39 allocation count".into());}Ok(n as usize)}
fn read_leaf39(r:&mut Cursor<&[u8]>,values:&mut usize,filing_bytes:usize)->Result<LeafOpeningV3,String>{
    let tag=read::<u8>(r)?;let dtype=misaka_palw_tir::DType::ALL.into_iter().find(|d|d.tag()==tag).ok_or("point39 unknown leaf dtype")?;
    let rank=bounded_count(r,4)?;let mut shape=Vec::with_capacity(rank);for _ in 0..rank{let d=read::<u64>(r)?;if d==0{return Err("point39 zero leaf dimension".into());}shape.push(d);}
    let shape_usize=shape.iter().map(|d|usize::try_from(*d).map_err(|_|"point39 leaf shape extent")).collect::<Result<Vec<_>,_>>()?;
    let layout=crate::merkle3::LayoutV3::try_of(&shape_usize).ok_or("point39 leaf layout")?;
    let axis=read::<u8>(r)?;let line=read::<u64>(r)?;let tile=read::<u64>(r)?;
    if axis!=crate::merkle3::AXIS_ROW&&axis!=crate::merkle3::AXIS_COL{return Err("point39 leaf axis".into());}
    let count=bounded_count(r,4096)?;if layout.leaf_element_count(axis,line,tile)!=Some(count as u64)||count>r.get_ref().len().saturating_sub(r.position()as usize)/dtype.width(){return Err("point39 leaf exact original geometry/body extent".into());}
    let next=values.checked_add(count).ok_or("point39 leaf expanded count overflow")?;
    if next.checked_mul(128).and_then(|n|n.checked_add(filing_bytes)).is_none_or(|n|n>COURT39_MAX_SCRATCH as usize){return Err("point39 leaf expanded allocation refused before reserve".into());}
    *values=next;let mut data=Vec::with_capacity(count);let mut word=[0u8;16];for _ in 0..count{r.read_exact(&mut word[..dtype.width()]).map_err(|_|"point39 leaf body")?;data.push(dtype.decode_le(&word[..dtype.width()]));}
    let count=bounded_count(r,64)?;let mut siblings=Vec::with_capacity(count);for _ in 0..count{siblings.push(read::<Digest>(r)?);}let other_root=read::<Digest>(r)?;
    Ok(LeafOpeningV3{dtype:tag,shape,axis,line,tile,values:data,siblings,other_root})
}
impl FlatPointFaultV39 {
    pub fn to_bytes(&self)->Vec<u8>{borsh::to_vec(self).expect("point39 Vec encoding")}
    pub fn from_bytes(raw:&[u8])->Result<Self,String>{if raw.len()>COURT39_MAX_BYTES as usize{return Err("point39 original filing cap".into());}let mut r=Cursor::new(raw);
        let version=read::<u16>(&mut r)?;if version!=39{return Err("point39 version".into());}
        let output=crate::flat_court4::read_node(&mut r).map_err(|_|"point39 output node")?;let element=read::<u64>(&mut r)?;
        let mut values=0usize;let output_leaf=read_leaf39(&mut r,&mut values,raw.len())?;let mut total_leaves=1usize;
        let n=bounded_count(&mut r,9)?;let mut operands=Vec::with_capacity(n);for _ in 0..n{let tag=read::<u8>(&mut r)?;
            let node=match tag{0=>None,1=>Some(crate::flat_court4::read_node(&mut r).map_err(|_|"point39 input node")?),_=>return Err("point39 optional node grammar".into())};
            let n=bounded_count(&mut r,COURT39_MAX_LEAVES as usize-total_leaves)?;total_leaves+=n;let mut leaves=Vec::with_capacity(n);for _ in 0..n{let leaf=read_leaf39(&mut r,&mut values,raw.len())?;
                if values.checked_mul(128).and_then(|n|n.checked_add(raw.len())).is_none_or(|n|n>COURT39_MAX_SCRATCH as usize){return Err("point39 expanded leaf/map reservation exceeded before allocation".into());}leaves.push(leaf);}operands.push(PointOperandV39{node,leaves});}
        let f=Self{version,output,element,output_leaf,operands};if r.position()!=raw.len()as u64||f.to_bytes()!=raw{return Err("point39 exact canonical proof".into());}Ok(f)
    }
}
enum Operand<'a>{Leaves(BTreeMap<u64,i128>),Constant(&'a misaka_palw_tir::ConstDecl),Scalar(i128),Zero}
struct Inputs<'a>(Vec<Operand<'a>>);
impl OperandsV1 for Inputs<'_>{fn get(&self,operand:usize,flat:u64)->Result<i128,MissingV1>{match self.0.get(operand){Some(Operand::Leaves(m))=>m.get(&flat).copied(),Some(Operand::Scalar(v))if flat==0=>Some(*v),Some(Operand::Zero)=>Some(0),Some(Operand::Constant(c))=>{
    let width=c.dtype.width();let at=usize::try_from(flat).ok().and_then(|n|n.checked_mul(width));at.and_then(|at|c.data.get(at..at.checked_add(width)?)).and_then(|raw|Some(c.dtype.decode_le(raw)))},_=>None}.ok_or(MissingV1(operand,flat))}}
fn node_ok(pr:&PreparedV39<'_>,claim:&FlatClaimV39,node:&NodeOpeningV1,p:u32,s:u16,n:u16)->Result<(),String>{if(node.position,node.occurrence,node.node)!=(p,s,n)||!node.authenticates(&claim.segment_roots,claim.positions,pr.index(s,n)?,pr.node_count()){return Err("point39 node not original accepted trace".into());}Ok(())}
fn leaves(leaves:&[LeafOpeningV3],root:Digest,dtype:misaka_palw_tir::DType,shape:&[usize])->Result<BTreeMap<u64,i128>,String>{let mut out=BTreeMap::new();let mut seen=std::collections::BTreeSet::new();for leaf in leaves{
    if leaf.dtype()!=Some(dtype)||leaf.shape_usize().as_deref()!=Some(shape)||!seen.insert((leaf.axis,leaf.line,leaf.tile))||!leaf.authenticates(&root){return Err("point39 duplicate or unauthenticated typed operand leaf".into());}
    for(e,v)in leaf.elements().ok_or("point39 malformed leaf layout")?{if out.insert(e,v).is_some_and(|old|old!=v){return Err("point39 inconsistent operand leaves".into());}}}Ok(out)}
/// Success proves one false ordinary/selection/state value. Authentic malformed
/// arithmetic also convicts; missing dependencies never imply a fault.
pub fn check_flat_point_fault_v39(c:&CheckedPointContextV39<'_>,f:&FlatPointFaultV39)->Result<(),String>{
    if f.version!=39{return Err("point39 version".into());}let raw=f.to_bytes();if raw.len()>c.scope.scope().maximum_bytes as usize{return Err("point39 original priced filing extent".into());}
    let(p,s,n)=(f.output.position,f.output.occurrence,f.output.node);let pr=&c.prepared;node_ok(pr,c.claim,&f.output,p,s,n)?;let sh=pr.shapes(p,s,n)?;let node=pr.node(s,n)?;
    let output=leaves(std::slice::from_ref(&f.output_leaf),f.output.commitment,node.out.dtype,&sh.out)?;let actual=*output.get(&f.element).ok_or("point39 output scalar not opened")?;
    let sources=pr.operand_sources(p,s,n)?;if sources.len()!=f.operands.len(){return Err("point39 original operand coverage differs".into());}
    let mut operands=Vec::new();let mut count=1usize;let mut values=output.len();for(i,(src,proof))in sources.iter().zip(&f.operands).enumerate(){count=count.checked_add(proof.leaves.len()).ok_or("point39 proof count overflow")?;
        if count>c.scope.scope().maximum_leaves as usize{return Err("point39 original priced leaf count".into());}
        let root=match src{OperandSourceV39::Trace{position,occurrence,node}=>{let opening=proof.node.as_ref().ok_or("point39 original trace operand opening absent")?;node_ok(pr,c.claim,opening,*position,*occurrence,*node)?;Some(opening.commitment)},
            OperandSourceV39::Parameter{param,layer}=>{if proof.node.is_some(){return Err("point39 parameter cannot substitute trace".into());}Some(*pr.source().parameters().by_instance.get(&(*param,*layer)).ok_or("point39 original real PC missing")?)},
            OperandSourceV39::Input{position,input}=>{if proof.node.is_some(){return Err("point39 input cannot substitute trace".into());}Some(*c.input_roots.get(&(*position,*input)).ok_or("point39 original actual input PC missing")?)},_=>None};
        if let Some(root)=root{let m=leaves(&proof.leaves,root,sh.in_dtypes[i],&sh.inputs[i])?;values=values.checked_add(m.len()).ok_or("point39 operand values overflow")?;operands.push(Operand::Leaves(m));}
        else{if proof.node.is_some()||!proof.leaves.is_empty(){return Err("point39 public operand cannot inject leaves".into());}operands.push(match src{OperandSourceV39::Constant(j)=>Operand::Constant(&pr.program().consts[*j as usize]),OperandSourceV39::Token(p)=>Operand::Scalar(*c.tokens.get(*p as usize).ok_or("point39 original input token absent")?as i128),OperandSourceV39::Position(p)=>Operand::Scalar(*p as i128),OperandSourceV39::InitialZero=>Operand::Zero,_=>return Err("point39 operand source".into())});}}
    if values.checked_mul(128).and_then(|n|n.checked_add(raw.len())).is_none_or(|n|n>c.scope.scope().maximum_scratch as usize){return Err("point39 original priced expanded scratch".into());}
    match pr.scalar(p,s,n,f.element,&Inputs(operands))?{Ok(Some(expected))if expected==actual=>Err("point39 authentic relation is correct".into()),Ok(_)=>Ok(()),Err(_)=>Err("point39 complete original dependencies not opened".into())}
}
