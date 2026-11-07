//! Original39 wiring, parameter namespaces and state continuity. No resized
//! weights, synthetic V1 program, or caller-provided relation/length grants.
use misaka_palw_tir::{program::{Ref,StateKind},program_v39::*,DType};
use crate::{flat_source39::CheckedSourceV39,hash::{Digest,object_id},element::{ElementShapesV1,MissingV1,OperandsV1}};
use borsh::BorshSerialize;
#[derive(Clone,Debug,PartialEq,Eq,BorshSerialize)]
pub enum OperandSourceV39 {Trace{position:u32,occurrence:u16,node:u16},Parameter{param:u16,layer:Option<u16>},Constant(u16),Token(u32),Position(u32),Input{position:u32,input:u8},InitialZero}
pub struct PreparedV39<'a>{source:&'a CheckedSourceV39,occurrences:Vec<(u8,Option<u16>)>,windows:Vec<Option<u32>>,offsets:Vec<u64>,nodes:u64,positions:u32}
impl<'a> PreparedV39<'a>{
    pub fn new(source:&'a CheckedSourceV39,positions:u32)->Result<Self,String>{
        if positions==0||positions>source.original_context_max(){return Err("prepared39 positions exceed original source context".into());}
        let p=source.program();let info=misaka_palw_tir::validate_v39::validate_v39(p)?;let occurrences=p.occurrences();
        let mut offsets=Vec::with_capacity(occurrences.len());let mut nodes=0u64;for(b,_)in &occurrences{offsets.push(nodes);nodes=nodes.checked_add(p.blocks[*b as usize].nodes.len()as u64).ok_or("prepared39 node count overflow")?;}
        Ok(Self{source,occurrences,windows:info.windows,offsets,nodes,positions})
    }
    pub fn occurrences(&self)->&[(u8,Option<u16>)]{&self.occurrences}
    pub fn positions(&self)->u32{self.positions}
    pub fn node_count(&self)->u64{self.nodes}
    pub fn source(&self)->&CheckedSourceV39{self.source}
    pub fn program(&self)->&TirProgramV39{self.source.program()}
    pub fn root(&self)->Digest{object_id(b"misaka-palw/kernel/prepared-original/v39",&(self.source.root(),self.positions,self.nodes,&self.occurrences))}
    pub fn node(&self,s:u16,n:u16)->Result<&NodeV39,String>{let(b,_)=*self.occurrences.get(s as usize).ok_or("prepared39 occurrence")?;self.program().blocks[b as usize].nodes.get(n as usize).ok_or("prepared39 node".into())}
    pub fn index(&self,s:u16,n:u16)->Result<u64,String>{self.node(s,n)?;self.offsets[s as usize].checked_add(n as u64).ok_or("prepared39 node offset overflow".into())}
    pub fn history(&self,p:u32,s:u16)->Result<u32,String>{if p>=self.positions{return Err("prepared39 actual position exceeds original reservation".into());}let(b,_)=*self.occurrences.get(s as usize).ok_or("prepared39 occurrence")?;Ok(self.windows[b as usize].map_or(1,|w|(p+1).min(w)))}
    pub fn input_type(&self,s:u16,n:u16,r:&Ref)->Result<TensorTypeV39,String>{let(b,_)=*self.occurrences.get(s as usize).ok_or("prepared39 occurrence")?;let p=self.program();let block=&p.blocks[b as usize];Ok(match *r{
        Ref::Node(j)=>{if j>=n{return Err("prepared39 nonbackward ref".into());}block.nodes.get(j as usize).ok_or("prepared39 node operand")?.out.clone()},
        Ref::CarryIn(j)=>block.carry_in.get(j as usize).ok_or("prepared39 carry operand")?.clone(),
        Ref::Param(j)=>{let d=p.params.get(j as usize).ok_or("prepared39 parameter operand")?;TensorTypeV39::fixed(d.dtype,&d.shape)},
        Ref::Const(j)=>{let d=p.consts.get(j as usize).ok_or("prepared39 constant operand")?;TensorTypeV39::fixed(d.dtype,&d.shape)},
        Ref::State(j)=>{let d=p.states.get(j as usize).ok_or("prepared39 state operand")?;TensorTypeV39::fixed(d.dtype,&d.shape)},
        Ref::Input(j)if j<2=>TensorTypeV39::scalar(DType::Idx),Ref::Input(j)=>{let d=p.inputs.get(j as usize-2).ok_or("prepared39 input operand")?;TensorTypeV39::fixed(d.dtype,&d.shape)}})}
    fn writer(&self,state:u16,layer:Option<u16>)->Result<(u16,u16),String>{let mut found=None;for(s,(b,l))in self.occurrences.iter().enumerate(){for(n,node)in self.program().blocks[*b as usize].nodes.iter().enumerate(){if let NodePrimV39::Frozen(misaka_palw_tir::Prim::StateWrite{state:j}|misaka_palw_tir::Prim::HistAppend{state:j})=&node.prim{if *j==state&&(!self.program().states[state as usize].per_layer||*l==layer){if found.replace((s as u16,n as u16)).is_some(){return Err("prepared39 ambiguous original state writer".into());}}}}}found.ok_or("prepared39 original state writer missing".into())}
    pub fn operand_sources(&self,p:u32,s:u16,n:u16)->Result<Vec<OperandSourceV39>,String>{self.history(p,s)?;let node=self.node(s,n)?;let(_,layer)=self.occurrences[s as usize];let mut out=Vec::new();for r in &node.inputs{out.push(match *r{
        Ref::Node(j)=>OperandSourceV39::Trace{position:p,occurrence:s,node:j},
        Ref::CarryIn(j)=>{if s==0{return Err("prepared39 pre carry absent".into());}let(b,_)=self.occurrences[s as usize-1];let from=*self.program().blocks[b as usize].carry_out.get(j as usize).ok_or("prepared39 original carry index")?;OperandSourceV39::Trace{position:p,occurrence:s-1,node:from}},
        Ref::Param(j)=>OperandSourceV39::Parameter{param:j,layer:if self.program().params[j as usize].per_layer{layer}else{None}},
        Ref::Const(j)=>OperandSourceV39::Constant(j),Ref::Input(0)=>OperandSourceV39::Token(p),Ref::Input(1)=>OperandSourceV39::Position(p),Ref::Input(j)=>OperandSourceV39::Input{position:p,input:j-2},
        Ref::State(j)=>{if p==0{OperandSourceV39::InitialZero}else{let(w,n)=self.writer(j,layer)?;OperandSourceV39::Trace{position:p-1,occurrence:w,node:n}}}});}
        if let NodePrimV39::Frozen(misaka_palw_tir::Prim::HistAppend{state})=&node.prim{out.push(if p==0{OperandSourceV39::InitialZero}else{let(w,n)=self.writer(*state,layer)?;OperandSourceV39::Trace{position:p-1,occurrence:w,node:n}});}
        Ok(out)
    }
    pub fn shapes(&self,p:u32,s:u16,n:u16)->Result<ElementShapesV1,String>{let h=self.history(p,s)?;let node=self.node(s,n)?;let mut inputs=Vec::new();let mut in_dtypes=Vec::new();for r in &node.inputs{let t=self.input_type(s,n,r)?;inputs.push(t.shape.iter().map(|d|d.at(h)as usize).collect());in_dtypes.push(t.dtype);}
        let hist=if let NodePrimV39::Frozen(misaka_palw_tir::Prim::HistAppend{state})=&node.prim{let st=&self.program().states[*state as usize];let StateKind::Hist{window}=st.kind else{return Err("prepared39 history kind".into());};let row=st.shape.iter().try_fold(1usize,|n,d|n.checked_mul(*d as usize)).ok_or("prepared39 history row overflow")?;let before=p.min(window)as usize;inputs.push(std::iter::once(before).chain(st.shape.iter().map(|d|*d as usize)).collect());in_dtypes.push(st.dtype);Some((h as usize,before,row))}else{None};
        Ok(ElementShapesV1{out:node.out.shape.iter().map(|d|d.at(h)as usize).collect(),inputs,in_dtypes,hist})
    }
    /// The scalar arithmetic consumes authenticated operands through `ops`.
    /// It never accepts caller-defined shapes, node bodies, or selected lengths.
    pub fn scalar(&self,p:u32,s:u16,n:u16,element:u64,ops:&dyn OperandsV1)->Result<Result<Option<i128>,MissingV1>,String>{
        let node=self.node(s,n)?;let sh=self.shapes(p,s,n)?;let count=sh.out.iter().try_fold(1u64,|n,d|n.checked_mul(*d as u64)).ok_or("prepared39 output product")?;if element>=count{return Err("prepared39 scalar outside original output".into());}
        let prim=match &node.prim{NodePrimV39::Frozen(pr)=>pr.clone(),NodePrimV39::HistorySelect{axis,cap}=>misaka_palw_tir::Prim::TopK{axis:*axis,k:self.history(p,s)?.min(*cap)}};
        let scalar_node=misaka_palw_tir::Node{prim,inputs:Vec::new(),out:misaka_palw_tir::TensorType::fixed(node.out.dtype,&[]),commit:node.commit};
        Ok(crate::element::element_value_with_states_v39(&self.program().states,&scalar_node,&sh,element,ops))
    }
}
