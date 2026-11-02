//! Immutable context components for RFC23's distinct scoped archives.
//! This is NOT a new global state root or a computation-validity argument.
//! Updates produce an atomic write plan: unchanged branches and large row
//! literals are referenced, never copied into each source's historical frame.
//! The unchanged selector still reads the COMPLETE eligible population stream.
use crate::{Hash64, palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1}};
use borsh::{BorshSerialize, BorshDeserialize};
use std::io;

pub(crate) const CONTEXT_MAX_KEY_BYTES_V2: usize = 512;
#[derive(Clone,Copy,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct ComponentSpecV2 { pub owner:u8, pub table:u32, pub schema:u16, pub key_bytes:u16, pub value_bytes:u64 }
crate::palw_native_resource_v1::native_struct!(ComponentSpecV2;owner,table,schema,key_bytes,value_bytes);
impl ComponentSpecV2 {
    fn check(self)->Result<(),ContextErrorV2>{
        // The owner must derive key_bytes from its actual bounded key schema.
        // This implementation bound is NOT a new consensus input limit. A
        // future unsupported schema is a typed local codec/resource refusal.
        if self.key_bytes as usize>CONTEXT_MAX_KEY_BYTES_V2 {Err(ContextErrorV2::Resource(NativeCoverageV1::unsupported::<Self>("scoped component key schema","bounded key codec not implemented for this schema")))}else{Ok(())}
    }
    fn depth(self)->usize {self.key_bytes as usize*9+1}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct RowValueV2 { pub data:Hash64, pub bytes:u64 }
crate::palw_native_resource_v1::native_struct!(RowValueV2;data,bytes);
#[derive(Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) enum ContextNodeV2 {
    Leaf { key:Vec<u8>, value:RowValueV2 },
    Branch { bits:u32, prefix:Vec<u8>, children:[Hash64;2] },
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,BorshSerialize,BorshDeserialize)]
pub(crate) struct ComponentRootV2 { pub spec:ComponentSpecV2, pub node:Option<Hash64>, pub rows:u64 }
crate::palw_native_resource_v1::native_struct!(ComponentRootV2;spec,node,rows);
impl ComponentRootV2 {
    pub fn empty(spec:ComponentSpecV2)->Result<Self,ContextErrorV2>{spec.check()?;Ok(Self{spec,node:None,rows:0})}
    pub(crate) fn check(self)->Result<(),ContextErrorV2>{self.spec.check()?;if self.node.is_none()!=(self.rows==0){return Err(ContextErrorV2::Malformed("component count/empty root"))}Ok(())}
}
#[derive(Debug)]
pub(crate) enum ContextErrorV2 { Resource(NativeCoverageV1), Unavailable(&'static str), Malformed(&'static str), Stale }
fn overflow()->ContextErrorV2 {ContextErrorV2::Resource(NativeCoverageV1::overflow::<ContextNodeV2>("scoped context nodes"))}

/// Implementations return a typed retained decode owner. An unavailable node
/// cannot be represented by None: None is an empty root, not a storage answer.
pub(crate) trait ContextNodeReaderV2 {fn node(&self,id:Hash64)->Result<NativeRetainedV1<ContextNodeV2>,ContextErrorV2>;}

struct HashWriter(blake2b_simd::State);
impl io::Write for HashWriter {fn write(&mut self,bytes:&[u8])->io::Result<usize>{self.0.update(bytes);Ok(bytes.len())}fn flush(&mut self)->io::Result<()>{Ok(())}}
pub(crate) fn digest(spec:ComponentSpecV2,node:&ContextNodeV2)->Hash64 {
    let mut out=HashWriter(blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/claim-context-node/v2").to_state());
    spec.serialize(&mut out).expect("fixed component spec");node.serialize(&mut out).expect("node identity stream");
    let mut bytes=[0;64];bytes.copy_from_slice(out.0.finalize().as_bytes());Hash64::from_bytes(bytes)
}
fn bits(key:&[u8])->usize {key.len()*9+1}
fn key_bit(key:&[u8],at:usize)->u8 {if at==key.len()*9{0}else if at%9==0{1}else{(key[at/9]>>(8-at%9))&1}}
fn packed_bit(prefix:&[u8],at:usize)->u8 {(prefix[at/8]>>(7-at%8))&1}
fn node_bits(node:&ContextNodeV2)->usize {match node{ContextNodeV2::Leaf{key,..}=>bits(key),ContextNodeV2::Branch{bits,..}=>*bits as usize}}
fn node_bit(node:&ContextNodeV2,at:usize)->u8 {match node{ContextNodeV2::Leaf{key,..}=>key_bit(key,at),ContextNodeV2::Branch{prefix,..}=>packed_bit(prefix,at)}}
fn common(node:&ContextNodeV2,key:&[u8])->usize {(0..node_bits(node).min(bits(key))).take_while(|at|node_bit(node,*at)==key_bit(key,*at)).count()}
fn checked_node<R:ContextNodeReaderV2>(reader:&R,spec:ComponentSpecV2,id:Hash64)->Result<NativeRetainedV1<ContextNodeV2>,ContextErrorV2>{
    let node=reader.node(id)?;
    match &*node {
        ContextNodeV2::Leaf{key,value}=>{if key.len()>spec.key_bytes as usize||value.bytes>spec.value_bytes{return Err(ContextErrorV2::Malformed("component leaf shape"))}},
        ContextNodeV2::Branch{bits,prefix,children}=>{
            let n=*bits as usize;
            if n>=spec.depth()||prefix.len()!=n.div_ceil(8)||children[0]==children[1]||n%8!=0&&prefix.last().is_some_and(|b|b&((1<<(8-n%8))-1)!=0)
                ||(0..n).step_by(9).any(|at|packed_bit(prefix,at)!=1){return Err(ContextErrorV2::Malformed("component branch shape"))}
        },
    }
    if digest(spec,&node)!=id{return Err(ContextErrorV2::Malformed("component node identity"))}Ok(node)
}
fn child_placement(parent:&ContextNodeV2,child:&ContextNodeV2,side:usize)->Result<(),ContextErrorV2>{
    let ContextNodeV2::Branch{bits,prefix,..}=parent else{return Err(ContextErrorV2::Malformed("leaf as parent"))};
    let n=*bits as usize;
    if node_bits(child)<=n||(0..n).any(|at|node_bit(child,at)!=packed_bit(prefix,at))||node_bit(child,n)!=side as u8{return Err(ContextErrorV2::Malformed("component child placement"))}Ok(())
}

/// Same validators used by complete traversal; incremental GC must preserve
/// the exact digest/shape/placement rules while reusing classified subtrees.
pub(crate) fn checked_gc_node_v3<R:ContextNodeReaderV2>(reader:&R,spec:ComponentSpecV2,id:Hash64)->Result<NativeRetainedV1<ContextNodeV2>,ContextErrorV2>{checked_node(reader,spec,id)}
pub(crate) fn checked_gc_child_v3(parent:&ContextNodeV2,child:&ContextNodeV2,side:usize)->Result<(),ContextErrorV2>{child_placement(parent,child,side)}
/// Fixed per-mutation structural envelope, derived from the ACTUAL bounded
/// component key width. This does not price arbitrary retries/retention H: the
/// source owner must multiply by its real admitted mutation/branch envelope.
#[derive(Clone,Copy,Debug)]
pub(crate) struct ContextMutationGeometryV2{pub max_nodes:u64,pub max_node_wire:u64,pub retained_wire:u64,pub retained_heap:u64}
pub(crate) fn mutation_geometry(spec:ComponentSpecV2,new_body_bytes:u64)->Result<ContextMutationGeometryV2,ContextErrorV2>{
    spec.check()?;if new_body_bytes>spec.value_bytes{return Err(ContextErrorV2::Malformed("component body exceeds derived schema envelope"))}
    let max_nodes=(spec.depth() as u64).checked_add(2).ok_or_else(overflow)?;
    let max_node_wire=(137u64+(spec.key_bytes as u64*9).div_ceil(8)).max(77+spec.key_bytes as u64);
    let memberships=max_nodes.checked_add(2).ok_or_else(overflow)?;
    let retained_wire=max_nodes.checked_mul(max_node_wire+65).and_then(|v|v.checked_add(new_body_bytes)).and_then(|v|v.checked_add(4+65+18+90)).ok_or_else(overflow)?;
    let retained_heap=retained_wire.checked_mul(2).and_then(|v|v.checked_add(memberships.checked_mul(crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<(u8,Vec<u8>),Vec<u8>>() as u64)?)).ok_or_else(overflow)?;
    Ok(ContextMutationGeometryV2{max_nodes,max_node_wire,retained_wire,retained_heap})
}

/// A bounded iterative point read. Hash/data equality establishes identity only.
/// A present leaf still needs its actual row body from the retained blob owner.
pub(crate) fn lookup<R:ContextNodeReaderV2>(reader:&R,root:ComponentRootV2,key:&[u8])->Result<Option<RowValueV2>,ContextErrorV2>{
    root.check()?;if key.len()>root.spec.key_bytes as usize{return Err(ContextErrorV2::Malformed("component lookup key"))}
    let Some(mut id)=root.node else{return Ok(None)};
    let mut parent=None::<(NativeRetainedV1<ContextNodeV2>,usize)>;
    for _ in 0..=root.spec.depth(){
        let node=checked_node(reader,root.spec,id)?;
        if let Some((old,side))=&parent{child_placement(old,&node,*side)?;}
        match &*node {
            ContextNodeV2::Leaf{key:stored,value}=>return Ok((stored.as_slice()==key).then_some(*value)),
            ContextNodeV2::Branch{bits,children,..}=>{
                if common(&node,key)!=*bits as usize{return Ok(None)}
                let side=key_bit(key,*bits as usize) as usize;id=children[side];parent=Some((node,side));
            }
        }
    }Err(ContextErrorV2::Malformed("component traversal depth/cycle"))
}

pub(crate) struct ComponentUpdateV2 { before:ComponentRootV2, after:ComponentRootV2, nodes:Vec<(Hash64,ContextNodeV2)>, next_value:Option<RowValueV2> }
impl ComponentUpdateV2 {
    pub(crate) fn before(&self)->ComponentRootV2{self.before}
    pub(crate) fn after(&self)->ComponentRootV2{self.after}
    pub(crate) fn next_value(&self)->Option<RowValueV2>{self.next_value}
    pub(crate) fn new_nodes(&self)->&[(Hash64,ContextNodeV2)]{&self.nodes}
}
struct StepV2 { node:NativeRetainedV1<ContextNodeV2>, side:usize }
fn push_node(nodes:&mut Vec<(Hash64,ContextNodeV2)>,spec:ComponentSpecV2,node:ContextNodeV2)->Hash64 {let id=digest(spec,&node);nodes.push((id,node));id}
fn prefix(key:&[u8],n:usize)->Vec<u8>{let mut out=vec![0;n.div_ceil(8)];for at in 0..n{out[at/8]|=key_bit(key,at)<<(7-at%8)}out}
fn leaf(nodes:&mut Vec<(Hash64,ContextNodeV2)>,spec:ComponentSpecV2,key:&[u8],value:RowValueV2)->Hash64{push_node(nodes,spec,ContextNodeV2::Leaf{key:key.to_vec(),value})}
fn branch(nodes:&mut Vec<(Hash64,ContextNodeV2)>,spec:ComponentSpecV2,key:&[u8],n:usize,children:[Hash64;2])->Hash64{push_node(nodes,spec,ContextNodeV2::Branch{bits:n as u32,prefix:prefix(key,n),children})}

/// Build a path-sized immutable write plan. Caller journals publication of all
/// new nodes and the root atomically; failed plans leave the reader unchanged.
/// Every unchanged subtree/large row literal is shared by hash, not cloned.
pub(crate) fn update<R:ContextNodeReaderV2>(reader:&R,root:ComponentRootV2,key:&[u8],old:Option<RowValueV2>,new:Option<RowValueV2>)->Result<NativeRetainedV1<ComponentUpdateV2>,ContextErrorV2>{
    root.check()?;if key.len()>root.spec.key_bytes as usize||new.is_some_and(|v|v.bytes>root.spec.value_bytes){return Err(ContextErrorV2::Malformed("component update shape"))}
    let depth=root.spec.depth();
    let mut q=TirNativeRequestsV1::new();
    q.exact_v1::<StepV2>((depth+1) as u64).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<(Hash64,ContextNodeV2)>((depth+2) as u64).map_err(ContextErrorV2::Resource)?;
    q.exact_v1::<u8>(((depth+2) as u64).checked_mul(depth.div_ceil(8) as u64+root.spec.key_bytes as u64).ok_or_else(overflow)?).map_err(ContextErrorV2::Resource)?;
    let lease=reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut path:Vec<StepV2>=Vec::with_capacity(depth+1);let mut nodes=Vec::with_capacity(depth+2);
    let mut cursor=root.node;let mut parent=None::<(usize,usize)>;
    let (mut result,found)=loop{
        let Some(id)=cursor else{break(new.map(|value|leaf(&mut nodes,root.spec,key,value)),None)};
        let node=checked_node(reader,root.spec,id)?;
        if let Some((at,side))=parent{child_placement(&path[at].node,&node,side)?;}
        let shared=common(&node,key);
        if shared<node_bits(&node){
            if let ContextNodeV2::Branch{bits,..}=&*node{if shared==*bits as usize{unreachable!()}}
            if old.is_some(){return Err(ContextErrorV2::Stale)}
            let next=if let Some(value)=new{
                let other=leaf(&mut nodes,root.spec,key,value);let side=key_bit(key,shared) as usize;
                let mut children=[id;2];children[side]=other;Some(branch(&mut nodes,root.spec,key,shared,children))
            }else{Some(id)};break(next,None)
        }
        match &*node{
            ContextNodeV2::Leaf{key:stored,value}=>{
                if stored.as_slice()!=key{return Err(ContextErrorV2::Malformed("prefix-free leaf mismatch"))}
                let next=if new==Some(*value){Some(id)}else{new.map(|v|leaf(&mut nodes,root.spec,key,v))};break(next,Some(*value))
            },
            ContextNodeV2::Branch{bits,children,..}=>{
                if path.len()>=depth{return Err(ContextErrorV2::Malformed("component update cycle"))}
                let side=key_bit(key,*bits as usize) as usize;cursor=Some(children[side]);parent=Some((path.len(),side));path.push(StepV2{node,side});
            }
        }
    };
    if found!=old{return Err(ContextErrorV2::Stale)}
    while let Some(step)=path.pop(){
        let ContextNodeV2::Branch{bits,prefix,children}=&*step.node else{unreachable!()};
        result=Some(match result{
            None=>children[1-step.side],
            Some(id) if id==children[step.side]=>digest(root.spec,&step.node),
            Some(id)=>{let mut next=*children;next[step.side]=id;push_node(&mut nodes,root.spec,ContextNodeV2::Branch{bits:*bits,prefix:prefix.clone(),children:next})},
        });
    }
    let rows=match(old.is_some(),new.is_some()){(false,true)=>root.rows.checked_add(1).ok_or_else(overflow)?,(true,false)=>root.rows.checked_sub(1).ok_or(ContextErrorV2::Malformed("component negative row count"))?,_=>root.rows};
    let after=ComponentRootV2{node:result,rows,..root};after.check()?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(ComponentUpdateV2{before:root,after,nodes,next_value:new},lease))
}

struct WalkV2 { node:NativeRetainedV1<ContextNodeV2>, next:usize }
/// Walk every row in the OLD raw-key order, with explicit EOF/count closure.
/// Omitting a better eligible bond cannot produce a successful complete stream.
/// Memory is key-depth bounded; selector/census semantics and O(n) work stay old.
pub(crate) fn complete_rows<R:ContextNodeReaderV2>(reader:&R,root:ComponentRootV2,mut visit:impl FnMut(&[u8],RowValueV2)->Result<(),ContextErrorV2>)->Result<(),ContextErrorV2>{
    root.check()?;let Some(id)=root.node else{return Ok(())};
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<WalkV2>((root.spec.depth()+1) as u64).map_err(ContextErrorV2::Resource)?;
    let _lease=reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut stack=Vec::with_capacity(root.spec.depth()+1);stack.push(WalkV2{node:checked_node(reader,root.spec,id)?,next:0});let mut count=0u64;
    while let Some(top)=stack.last_mut(){
        match &*top.node{
            ContextNodeV2::Leaf{key,value}=>{if count>=root.rows{return Err(ContextErrorV2::Malformed("component complete stream exceeds row count"))}visit(key,*value)?;count=count.checked_add(1).ok_or_else(overflow)?;stack.pop();},
            ContextNodeV2::Branch{children,..} if top.next<2=>{
                let side=top.next;let next=checked_node(reader,root.spec,children[side])?;child_placement(&top.node,&next,side)?;top.next+=1;
                if stack.len()>root.spec.depth(){return Err(ContextErrorV2::Malformed("component stream depth"))}stack.push(WalkV2{node:next,next:0});
            },
            _=>{stack.pop();},
        }
    }
    if count!=root.rows{return Err(ContextErrorV2::Malformed("component complete stream row count"))}Ok(())
}

// Explicit bounded wire shapes before any row-key/prefix Vec allocation.
crate::palw_native_resource_v1::wire::wire_struct!(ComponentSpecV2;u8,u32,u16,u16,u64);
crate::palw_native_resource_v1::wire::wire_struct!(RowValueV2;Hash64,u64);
crate::palw_native_resource_v1::wire::wire_struct!(ComponentRootV2;ComponentSpecV2,Option<Hash64>,u64);
impl crate::palw_native_resource_v1::NativeWireScanV1 for ContextNodeV2 {
    fn scan_native_wire_v1<R:io::Read>(r:&mut crate::palw_native_resource_v1::NativeWireReaderV1<R>)->io::Result<crate::palw_native_resource_v1::NativeWireShapeV1>{
        use crate::palw_native_resource_v1::{NativeWireScanV1,NativeWireShapeV1};
        r.step()?;
        let vec=|r:&mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,max:u64|->io::Result<NativeWireShapeV1>{let n=r.word()? as u64;if n>max{return Err(io::Error::new(io::ErrorKind::InvalidData,"scoped context key/prefix exceeds codec bound"))}r.skip(n)?;NativeWireShapeV1::default().allocation(n,1)};
        match r.byte()?{
            0=>vec(r,CONTEXT_MAX_KEY_BYTES_V2 as u64)?.merge(RowValueV2::scan_native_wire_v1(r)?),
            1=>{let bits=r.word()? as usize;if bits>CONTEXT_MAX_KEY_BYTES_V2*9{return Err(io::Error::new(io::ErrorKind::InvalidData,"scoped context prefix depth"))}vec(r,((CONTEXT_MAX_KEY_BYTES_V2*9+1).div_ceil(8)) as u64)?.merge(<[Hash64;2]>::scan_native_wire_v1(r)?)},
            _=>Err(io::Error::new(io::ErrorKind::InvalidData,"unknown scoped context node tag")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory {nodes:BTreeMap<Hash64,NativeRetainedV1<Vec<u8>>>}
    impl ContextNodeReaderV2 for Memory {
        fn node(&self,id:Hash64)->Result<NativeRetainedV1<ContextNodeV2>,ContextErrorV2>{
            let raw=self.nodes.get(&id).ok_or(ContextErrorV2::Unavailable("scoped node missing"))?;
            crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw,||borsh::from_slice::<ContextNodeV2>(raw)).map_err(|e|match e{
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r)=>ContextErrorV2::Resource(r),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)=>ContextErrorV2::Malformed("stored scoped node codec"),
            })
        }
    }
    impl Memory {
        fn publish(&mut self,plan:&ComponentUpdateV2){for(id,node)in &plan.nodes{
            if self.nodes.contains_key(id){continue}
            let n=borsh::object_length(node).unwrap() as u64;
            let mut q=TirNativeRequestsV1::new();q.exact_v1::<u8>(n.max(1024)*3).unwrap();q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64,NativeRetainedV1<Vec<u8>>>>(1).unwrap();
            let owner=reserve_native_requests_v1(q.bytes_v1()).unwrap();
            self.nodes.insert(*id,NativeRetainedV1::from_owner_reserved_v1(borsh::to_vec(node).unwrap(),owner));
        }}
        fn set(&mut self,root:ComponentRootV2,key:&[u8],value:Option<RowValueV2>)->ComponentRootV2{
            let old=lookup(self,root,key).unwrap();let plan=update(self,root,key,old,value).unwrap();self.publish(&plan);plan.after
        }
    }
    fn spec()->ComponentSpecV2{ComponentSpecV2{owner:1,table:1,schema:2,key_bytes:8,value_bytes:64}}
    fn value(n:u64)->RowValueV2{RowValueV2{data:Hash64::from_u64_word(n),bytes:8}}
    #[test]
    fn rfc23s2_scoped_index_preserves_complete_raw_key_order_and_old_roots(){
        let mut m=Memory::default();let empty=ComponentRootV2::empty(spec()).unwrap();
        let a=m.set(empty,&[1],Some(value(1)));let b=m.set(a,&[1,0],Some(value(2)));let c=m.set(b,&[],Some(value(3)));let d=m.set(c,&[2],Some(value(4)));
        assert_eq!(lookup(&m,a,&[1,0]).unwrap(),None);assert_eq!(lookup(&m,b,&[1,0]).unwrap(),Some(value(2)));
        let mut rows=vec![];complete_rows(&m,d,|key,v|{rows.push((key.to_vec(),v));Ok(())}).unwrap();
        assert_eq!(rows,vec![(vec![],value(3)),(vec![1],value(1)),(vec![1,0],value(2)),(vec![2],value(4))]);
        let removed=m.set(d,&[1],None);assert_eq!(lookup(&m,removed,&[1]).unwrap(),None);assert_eq!(lookup(&m,d,&[1]).unwrap(),Some(value(1)));
        let mut reversed=Memory::default();let mut root=empty;for(key,v)in rows.into_iter().rev(){root=reversed.set(root,&key,Some(v))}assert_eq!(root,d,"same complete rows have one canonical immutable index");
    }
    #[test]
    fn rfc23s2_scoped_index_update_is_path_sized_and_stale_write_atomic(){
        let mut m=Memory::default();let mut root=ComponentRootV2::empty(spec()).unwrap();
        for n in 0..128u64{root=m.set(root,&n.to_be_bytes(),Some(value(n)))}
        let nodes=m.nodes.len();let plan=update(&m,root,&[0;8],Some(value(0)),Some(value(999))).unwrap();
        assert!(plan.nodes.len()<=spec().depth()+2);assert!(plan.nodes.len()<128,"no copy of all unrelated population rows");
        assert_eq!(m.nodes.len(),nodes,"building a plan cannot publish partial state");
        assert!(matches!(update(&m,root,&[0;8],Some(value(1)),None),Err(ContextErrorV2::Stale)));
        assert_eq!(lookup(&m,root,&[0;8]).unwrap(),Some(value(0)));
        m.publish(&plan);assert_eq!(lookup(&m,plan.after,&[0;8]).unwrap(),Some(value(999)));
        assert_eq!(lookup(&m,root,&[0;8]).unwrap(),Some(value(0)));
    }
    #[test]
    fn rfc23s2_scoped_index_missing_tampered_foreign_and_omitted_rows_refuse(){
        let mut m=Memory::default();let root=ComponentRootV2::empty(spec()).unwrap();let root=m.set(root,&[1],Some(value(1)));
        let bad=ComponentRootV2{spec:ComponentSpecV2{owner:2,..spec()},..root};assert!(matches!(lookup(&m,bad,&[1]),Err(ContextErrorV2::Malformed(_))));
        assert!(matches!(complete_rows(&m,ComponentRootV2{rows:2,..root},|_,_|Ok(())),Err(ContextErrorV2::Malformed(_))));
        let raw=m.nodes.remove(&root.node.unwrap()).unwrap();assert!(matches!(lookup(&m,root,&[1]),Err(ContextErrorV2::Unavailable(_))));m.nodes.insert(root.node.unwrap(),raw);
        let mut malformed=vec![0];malformed.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(crate::palw_native_resource_v1::preflight_native_value_v1::<ContextNodeV2>(&malformed[..],malformed.len() as u64).is_err(),"key allocation is rejected before Borsh");
    }
}

/// Enumerate the COMPLETE immutable node closure (including branches). Used by
/// the rooted store's reachability collector; callbacks must not publish credit
/// until EOF succeeds. Missing children and count/depth mismatches fail closed.
pub(crate) fn visit_nodes<R:ContextNodeReaderV2>(reader:&R,root:ComponentRootV2,mut visit:impl FnMut(Hash64,&ContextNodeV2)->Result<(),ContextErrorV2>)->Result<(),ContextErrorV2>{
    root.check()?;let Some(id)=root.node else{return Ok(())};
    struct Step{ id:Hash64,node:NativeRetainedV1<ContextNodeV2>,next:usize }
    let mut q=TirNativeRequestsV1::new();q.exact_v1::<Step>((root.spec.depth()+1) as u64).map_err(ContextErrorV2::Resource)?;
    let _lease=reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut stack=Vec::with_capacity(root.spec.depth()+1);stack.push(Step{id,node:checked_node(reader,root.spec,id)?,next:0});let mut count=0u64;
    while let Some(top)=stack.last_mut(){
        if top.next==0{visit(top.id,&top.node)?;}
        match &*top.node{
            ContextNodeV2::Leaf{..}=>{if count>=root.rows{return Err(ContextErrorV2::Malformed("component closure exceeds row count"))}count+=1;stack.pop();},
            ContextNodeV2::Branch{children,..} if top.next<2=>{
                let side=top.next;let id=children[side];let next=checked_node(reader,root.spec,id)?;child_placement(&top.node,&next,side)?;top.next+=1;
                if stack.len()>root.spec.depth(){return Err(ContextErrorV2::Malformed("component closure depth"))}stack.push(Step{id,node:next,next:0});
            },
            _=>{stack.pop();},
        }
    }
    if count!=root.rows{return Err(ContextErrorV2::Malformed("component closure row count"))}Ok(())
}
