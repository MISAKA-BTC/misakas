//! Conservative parameter-coordinate closure for a named native TIR dissection.
//!
//! This derives potential reads, not the reads of one honest execution. Unknown
//! Gather indices cover their legal axis and unknown Select conditions cover
//! both branches. Node-value bounds use dtype bounds, not an honest runtime
//! result or a range derived only for correct reductions. Authenticated claim
//! units remain boundaries (with actual Fixed-state replay tracked): their producers
//! are never required to supply model bytes. A caller must separately reserve
//! acquisition, authentication, retention and money for the returned cover.
use super::*;
use misaka_palw_tir::Ref;
use misaka_palw_tir::demand::history_length_v1;

/// Explicit caller-admitted exploration limit, not a consensus parameter or a
/// model-size policy. Exhaustion returns no partial cover.
#[derive(Clone, Copy, Debug)]
pub struct ParameterCoverBudgetV1 { pub max_edges: u64, pub max_coordinates: u64 }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotentialParameterCoverV1 {
    /// Canonical ascending inventory coordinates under the original artifact.
    pub leaves: Vec<u32>,
    pub explored_edges: u64,
    pub explored_coordinates: u64,
}

/// Resolve the original binding and named leaf through the existing native
/// verifier. This does not accept a caller-provided program, root claim, read set
/// or unchecked scope as authority. Callers must first bind `binding` to their
/// retained original with the existing `check_tir_binding_is_the_claims_v1`.
/// Binding decode, range analysis and inventory-index allocation precede the
/// exploration meter and require separate native resource preflight. The meter
/// limits neither their allocation nor the later cost of acquiring this cover.
pub fn derive_parameter_cover_v1(binding: &PalwTirStepBindingV1, leaf: u64,
    budget: ParameterCoverBudgetV1) -> Result<PotentialParameterCoverV1, String> {
    let (v, leaf, inventory) = builder_prelude(binding, leaf).map_err(|e| e.to_string())?;
    let intervals = analyze_ranges(&v.space.program).map_err(|e| e.to_string())?;
    let site = palw_tir_dissect_site_v1(&v.space, &intervals, &leaf).ok_or("parameter cover needs a dissected commit leaf")?;
    let block = occurrence_block(&v.space, site.ctx.occurrence).ok_or("cover occurrence missing")?;
    let layer = v.space.occurrences().get(usize::from(site.ctx.occurrence)).ok_or("cover occurrence missing")?.1;
    let h = usize::try_from(site.history_positions).map_err(|_| "cover H overflow")?;
    let mut walk = Walk { program: &v.space.program, block: usize::from(block), layer, h,
        space: &v.space, pos: site.ctx.pos, occurrence: site.ctx.occurrence,
        inventory: &inventory, budget,
        edges: 0, seen: BTreeSet::new(), pending: Vec::new(), leaves: BTreeSet::new() };
    for element in leaf_elements(&leaf) { walk.node(site.node, element)?; }
    // Every legal root element/closure and every legal bottom range is covered.
    // This intentionally includes elements not reached by one particular root;
    // it is a finite coordinate envelope, not a canonical carriage for a move.
    for (node, count) in site.reductions.iter().zip(&site.counts) {
        for element in 0..usize::try_from(*count).map_err(|_| "cover reduction count overflow")? {
            walk.node(*node, element)?;
        }
    }
    walk.run()?;
    Ok(PotentialParameterCoverV1 { leaves: walk.leaves.into_iter().collect(),
        explored_edges: walk.edges, explored_coordinates: walk.seen.len() as u64 })
}

struct Walk<'a> {
    program: &'a TirProgramV1, block: usize, layer: Option<u16>, h: usize,
    space: &'a PalwTirStepSpaceV1, pos: u32, occurrence: u16,
    inventory: &'a PalwTirInventoryIndexV1, budget: ParameterCoverBudgetV1,
    edges: u64, seen: BTreeSet<(u32, u16, u16, usize)>, pending: Vec<(u32, u16, u16, usize)>, leaves: BTreeSet<u32>,
}
fn count(shape: &[usize]) -> Result<usize, String> {
    shape.iter().try_fold(1usize, |a,b| a.checked_mul(*b).ok_or("cover shape overflow".into()))
}
fn coords(mut index: usize, shape: &[usize]) -> Result<Vec<usize>, String> {
    if index >= count(shape)? { return Err("cover index outside shape".into()); }
    let mut out = vec![0; shape.len()];
    for i in (0..shape.len()).rev() { out[i] = index % shape[i]; index /= shape[i]; }
    Ok(out)
}
fn index(c: &[usize], shape: &[usize]) -> Result<usize, String> {
    if c.len()!=shape.len() { return Err("cover coordinate rank".into()); }
    c.iter().zip(shape).try_fold(0usize, |a,(v,n)| {
        if v>=n { return Err("cover coordinate outside shape".into()); }
        a.checked_mul(*n).and_then(|x|x.checked_add(*v)).ok_or("cover coordinate overflow".into())
    })
}
fn broadcast(c: &[usize], shape: &[usize]) -> Result<usize,String> {
    if shape.len()>c.len() { return Err("cover broadcast rank".into()); }
    let mapped:Vec<_>=c[c.len()-shape.len()..].iter().zip(shape).map(|(v,n)|if *n==1 {0}else{*v}).collect();
    index(&mapped,shape)
}
impl Walk<'_> {
    fn edge(&mut self)->Result<(),String> { self.edges=self.edges.checked_add(1).ok_or("cover edge overflow")?;
        if self.edges>self.budget.max_edges { return Err("parameter cover edge budget exhausted".into()); } Ok(()) }
    fn node(&mut self,n:u16,i:usize)->Result<(),String> {
        self.edge()?;
        let shape=self.program.blocks[self.block].nodes.get(n as usize).ok_or("cover node missing")?.out.resolve(self.h);
        if i>=count(&shape)? {return Err("cover node element outside shape".into());}
        let key=(self.pos,self.occurrence,n,i);
        if !self.seen.contains(&key) {
            if self.seen.len() as u64>=self.budget.max_coordinates {return Err("parameter cover coordinate budget exhausted".into());}
            self.seen.insert(key); self.pending.push(key);
        } Ok(())
    }
    fn shape(&self,r:Ref)->Result<Vec<usize>,String> {
        let p=self.program; let b=&p.blocks[self.block];
        Ok(match r {
            Ref::Node(j)=>b.nodes.get(j as usize).ok_or("cover node missing")?.out.resolve(self.h),
            Ref::CarryIn(j)=>b.carry_in.get(j as usize).ok_or("cover carry missing")?.resolve(self.h),
            Ref::Param(j)=>p.params.get(j as usize).ok_or("cover param missing")?.shape.iter().map(|d|*d as usize).collect(),
            Ref::Const(j)=>p.consts.get(j as usize).ok_or("cover const missing")?.shape.iter().map(|d|*d as usize).collect(),
            Ref::State(j)=>p.states.get(j as usize).ok_or("cover state missing")?.shape.iter().map(|d|*d as usize).collect(),
            Ref::Input(_)=>vec![],
        })
    }
    fn bounds(&self,r:Ref,i:usize)->Result<Interval,String> {
        Ok(match r {
            Ref::Node(j)=>Interval::of(self.program.blocks[self.block].nodes.get(j as usize).ok_or("cover node missing")?.out.dtype),
            Ref::Const(j)=>{let c=self.program.consts.get(j as usize).ok_or("cover const missing")?;
                let w=c.dtype.width(); let off=i.checked_mul(w).ok_or("cover const offset overflow")?;
                Interval::point(c.dtype.decode_le(c.data.get(off..off.checked_add(w).ok_or("cover const end overflow")?).ok_or("cover const element missing")?))},
            Ref::Input(0)=>Interval::new(0,self.program.token_bound.saturating_sub(1) as i128),
            Ref::Input(1)=>Interval::point(self.pos as i128),
            Ref::Param(j)=>{let d=self.program.params.get(j as usize).ok_or("cover param missing")?.dtype; Interval::new(d.min_value(),d.max_value())},
            _=>Interval::new(i128::MIN,i128::MAX),
        })
    }
    fn fetch(&mut self,r:Ref,i:usize)->Result<(),String> {
        self.edge()?; if i>=count(&self.shape(r)?)? {return Err("cover operand index outside shape".into());}
        match r {
            Ref::Node(j)=>{if !self.program.blocks[self.block].nodes[j as usize].commit {self.node(j,i)?;}},
            Ref::Param(j)=>{let p=&self.program.params[j as usize]; let byte=(i as u64).checked_mul(p.dtype.width() as u64).ok_or("cover param byte overflow")?;
                let leaf=self.inventory.leaf_of(j,if p.per_layer {self.layer}else{None},byte).ok_or("cover parameter coordinate missing")?; if !self.leaves.contains(&leaf) && self.leaves.len() as u64>=self.budget.max_coordinates {return Err("parameter cover leaf budget exhausted".into());} self.leaves.insert(leaf);},
            Ref::State(j)=>{
                if self.pos>0 && !self.pos.is_multiple_of(self.space.layout.checkpoint_interval) {
                    let state=&self.program.states[j as usize];
                    let layer=if state.per_layer {self.layer}else{None};
                    if let Some((occ,writer))=state_writer_v1(self.program,j,layer) {
                        let block=usize::from(self.space.occurrences()[usize::from(occ)].0);
                        if !self.program.blocks[block].nodes[usize::from(writer)].commit {
                            let old=(self.pos,self.occurrence); self.set_context(self.pos-1,occ)?;
                            self.node(writer,i)?; self.set_context(old.0,old.1)?;
                        }
                    }
                }
            },
            // Carry and token sources are authenticated claim material. Fixed
            // states are boundaries only at actual checkpoints, handled above.
            Ref::CarryIn(_)|Ref::Const(_)|Ref::Input(_)=>{},
        } Ok(())
    }
    fn set_context(&mut self,pos:u32,occ:u16)->Result<(),String> {
        let (block,layer)=*self.space.occurrences().get(usize::from(occ)).ok_or("cover replay occurrence missing")?;
        self.block=usize::from(block); self.layer=layer; self.pos=pos; self.occurrence=occ;
        self.h=usize::try_from(history_length_v1(&self.space.info,block,pos).ok_or("cover replay H missing")?)
            .map_err(|_|"cover replay H overflow")?; Ok(())
    }
    fn run(&mut self)->Result<(),String> {
        while let Some((pos,occ,n,i))=self.pending.pop() {
            self.set_context(pos,occ)?;
            let node=self.program.blocks[self.block].nodes[n as usize].clone(); let shape=node.out.resolve(self.h);
            let c=coords(i,&shape)?; let inputs=&node.inputs;
            match node.prim {
                Prim::Iota{..}=>{},
                Prim::Reshape|Prim::Cast|Prim::Clamp{..}|Prim::Log2Floor|Prim::IntExp|Prim::IntRsqrt|Prim::IntLn|Prim::StateWrite{..}=>self.fetch(inputs[0],i)?,
                Prim::Transpose{perm}=>{let s=self.shape(inputs[0])?;let mut q=vec![0;s.len()];for(k,p)in perm.iter().enumerate(){q[*p as usize]=c[k];}self.fetch(inputs[0],index(&q,&s)?)?;},
                Prim::Slice{axis,start}=>{let s=self.shape(inputs[0])?;let mut q=c.clone();q[axis as usize]=q[axis as usize].checked_add(start as usize).ok_or("cover Slice overflow")?;self.fetch(inputs[0],index(&q,&s)?)?;},
                Prim::Concat{axis}=>{let mut q=c.clone();for r in inputs {let s=self.shape(*r)?;if q[axis as usize]<s[axis as usize]{self.fetch(*r,index(&q,&s)?)?;break;}q[axis as usize]-=s[axis as usize];}},
                Prim::Broadcast=>{let s=self.shape(inputs[0])?;self.fetch(inputs[0],broadcast(&c,&s)?)?;},
                Prim::Add|Prim::Sub|Prim::Mul|Prim::Div{..}|Prim::Compare{..}=>{for r in inputs {let s=self.shape(*r)?;self.fetch(*r,broadcast(&c,&s)?)?;}},
                Prim::Select=>{let s=self.shape(inputs[0])?;let ci=broadcast(&c,&s)?;self.fetch(inputs[0],ci)?;let iv=self.bounds(inputs[0],ci)?;
                    for (take,r) in [(iv.lo!=0||iv.hi!=0,inputs[1]),(iv.lo<=0&&iv.hi>=0,inputs[2])] {if take {let s=self.shape(r)?;self.fetch(r,broadcast(&c,&s)?)?;}}},
                Prim::Gather{axis,batch_dims}=>{let(a,bd)=(axis as usize,batch_dims as usize);let d=self.shape(inputs[0])?;let x=self.shape(inputs[1])?;let m=x.len().checked_sub(bd).ok_or("cover Gather batch rank")?;
                    let mut xi=c[..bd].to_vec();xi.extend_from_slice(&c[a..a+m]);let ix=index(&xi,&x)?;self.fetch(inputs[1],ix)?;let iv=self.bounds(inputs[1],ix)?;
                    let lo=iv.lo.max(0);let hi=iv.hi.min(d[a] as i128-1);if lo<=hi {for v in usize::try_from(lo).map_err(|_|"cover Gather lower overflow")?..=usize::try_from(hi).map_err(|_|"cover Gather upper overflow")? {let mut q=c[..a].to_vec();q.push(v);q.extend_from_slice(&c[a+m..]);self.fetch(inputs[0],index(&q,&d)?)?;}}},
                Prim::MatMul=>{let x=self.shape(inputs[0])?;let y=self.shape(inputs[1])?;let(xr,yr,cr)=(x.len(),y.len(),c.len());let batch=&c[..cr-2];let xb=broadcast(batch,&x[..xr-2])?;let yb=broadcast(batch,&y[..yr-2])?;
                    let(m,k,nn)=(x[xr-2],x[xr-1],y[yr-1]);for t in 0..k {self.fetch(inputs[0],xb*m*k+c[cr-2]*k+t)?;self.fetch(inputs[1],yb*k*nn+t*nn+c[cr-1])?;}},
                Prim::ReduceSum{axis}|Prim::ReduceMax{axis}|Prim::TopK{axis,..}=>{let s=self.shape(inputs[0])?;for t in 0..s[axis as usize] {let mut q=c.clone();q[axis as usize]=t;self.fetch(inputs[0],index(&q,&s)?)?;}},
                Prim::HistAppend{state}=>{let row=count(&self.program.states[state as usize].shape.iter().map(|d|*d as usize).collect::<Vec<_>>())?;
                    if i/row+1==self.h {self.fetch(inputs[0],i%row)?;}},
            }
        } Ok(())
    }
}
