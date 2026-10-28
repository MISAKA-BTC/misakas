//! Owning version39 execution and cone replay. Frozen primitives share their
//! integer arithmetic implementation, never a substituted whole-program view.
//! Parameter ranges are authenticated by the caller's Source39/PC reader; this
//! evaluator independently checks their declared identity, extent and dtype.
use crate::{
    DType, Prim, Tensor, TirError, TirErrorKind, TirResult,
    error::err,
    eval::eval_prim,
    interp::{CommitRecord, ConeEnv, RunState, StateKey},
    interp_v2::InputProvider,
    program::{INPUT_POS, INPUT_TOKEN, Ref, StateKind},
    program_v2::FIRST_INPUT_REF_V2,
    program_v39::*,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamDescriptorV39 {
    pub dtype: DType,
    pub shape: Vec<u32>,
}

/// A source returns only the requested row-major range. In particular a direct
/// parameter Gather need not allocate or decode the entire vocabulary matrix.
/// The source/court layer must authenticate each range against the original PC.
pub trait ParamSourceV39 {
    fn descriptor(&self, index: u16, layer: Option<u16>) -> Option<ParamDescriptorV39>;
    fn read_range(&self, index: u16, layer: Option<u16>, offset: u64, elements: usize) -> TirResult<Vec<i128>>;
}

/// Compatibility for in-memory reference tools. Large artifact readers should
/// implement ParamSourceV39 directly, rather than materializing an old tensor.
pub struct MaterializedParamsV39<'a> {
    pub source: &'a dyn crate::ParamSource,
}
impl ParamSourceV39 for MaterializedParamsV39<'_> {
    fn descriptor(&self, index: u16, layer: Option<u16>) -> Option<ParamDescriptorV39> {
        let t = self.source.param(index, layer)?;
        Some(ParamDescriptorV39 { dtype: t.dtype, shape: t.shape.into_iter().map(u32::try_from).collect::<Result<_, _>>().ok()? })
    }
    fn read_range(&self, index: u16, layer: Option<u16>, offset: u64, elements: usize) -> TirResult<Vec<i128>> {
        let t = self.source.param(index, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, "parameter39"))?;
        let start = usize::try_from(offset).map_err(|_| TirError::new(TirErrorKind::Operand, "parameter39 offset"))?;
        let end = start.checked_add(elements).ok_or_else(|| TirError::new(TirErrorKind::Operand, "parameter39 range overflow"))?;
        t.data
            .get(start..end)
            .map(<[i128]>::to_vec)
            .ok_or_else(|| TirError::new(TirErrorKind::Operand, "parameter39 range outside source"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutputV39 {
    pub pos: u32,
    pub output: Tensor,
    pub commits: Vec<CommitRecord>,
}

/// Borrowed observer: a production trace builder can hash/write each committed
/// value immediately, without retaining every full tensor in the step result.
pub struct CommitPointV39<'a> {
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub value: &'a Tensor,
}

#[derive(Clone, Copy)]
enum StateView<'a> {
    Run(&'a RunState),
    Cone(&'a ConeEnv),
}
struct Occurrence<'a> {
    token: Option<u32>,
    pos: u32,
    layer: Option<u16>,
    carry: &'a BTreeMap<u8, Arc<Tensor>>,
    states: StateView<'a>,
    supplied: &'a BTreeMap<u16, Tensor>,
}

pub struct InterpreterV39<'p> {
    program: &'p TirProgramV39,
    info: crate::validate_v39::ProgramInfoV39,
}
fn product(shape: &[usize]) -> TirResult<usize> {
    shape
        .iter()
        .try_fold(1usize, |n, d| n.checked_mul(*d).ok_or_else(|| TirError::new(TirErrorKind::Operand, "tensor39 extent overflow")))
}
fn shape(t: &TensorTypeV39, h: u32) -> Vec<usize> {
    t.shape.iter().map(|d| d.at(h) as usize).collect()
}
fn check(t: &Tensor, want: &TensorTypeV39, h: u32, cap: u64) -> TirResult<()> {
    let expected = shape(want, h);
    let n = product(&expected)?;
    if n as u64 > cap
        || t.dtype != want.dtype
        || t.shape != expected
        || t.data.len() != n
        || t.data.iter().any(|v| !t.dtype.contains(*v))
    {
        return err(TirErrorKind::Operand, "tensor39 differs from original dtype/shape/value profile");
    }
    Ok(())
}
impl<'p> InterpreterV39<'p> {
    pub fn program(&self) -> &'p TirProgramV39 {
        self.program
    }
    pub fn info(&self) -> &crate::validate_v39::ProgramInfoV39 {
        &self.info
    }
    pub fn new(program: &'p TirProgramV39) -> TirResult<Self> {
        let info = crate::validate_v39::validate_v39(program).map_err(|e| TirError::new(TirErrorKind::NormalForm, e.to_string()))?;
        Ok(Self { program, info })
    }
    fn history(&self, block: u8, pos: u32) -> u32 {
        self.info.windows[block as usize].map(|window| (pos + 1).min(window)).unwrap_or(1)
    }
    fn param<'a>(
        &'a self,
        source: &dyn ParamSourceV39,
        index: u16,
        layer: Option<u16>,
    ) -> TirResult<(Option<u16>, &'a crate::ParamDecl, u64)> {
        let d = self.program.params.get(index as usize).ok_or_else(|| TirError::new(TirErrorKind::Malformed, "parameter39 index"))?;
        let layer = if d.per_layer {
            Some(layer.ok_or_else(|| TirError::new(TirErrorKind::Malformed, "per-layer parameter39 without layer"))?)
        } else {
            None
        };
        let descriptor =
            source.descriptor(index, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("parameter39 {}", d.name)))?;
        let n = d.shape.iter().try_fold(1u64, |n, d| {
            n.checked_mul(*d as u64).ok_or_else(|| TirError::new(TirErrorKind::Operand, "parameter39 extent overflow"))
        })?;
        if descriptor.dtype != d.dtype || descriptor.shape != d.shape || n > self.program.profile.max_parameter_elements {
            return err(TirErrorKind::Operand, "parameter39 source descriptor differs from original declaration");
        }
        Ok((layer, d, n))
    }
    fn range(
        &self,
        source: &dyn ParamSourceV39,
        index: u16,
        layer: Option<u16>,
        dtype: DType,
        total: u64,
        offset: u64,
        n: usize,
    ) -> TirResult<Vec<i128>> {
        if offset.checked_add(n as u64).is_none_or(|end| end > total) || n as u64 > self.program.profile.max_value_elements {
            return err(TirErrorKind::Operand, "parameter39 requested range outside original extent/profile");
        }
        let data = source.read_range(index, layer, offset, n)?;
        if data.len() != n || data.iter().any(|v| !dtype.contains(*v)) {
            return err(TirErrorKind::Operand, "parameter39 range has wrong count or out-of-dtype value");
        }
        Ok(data)
    }
    fn fetch(&self, inputs: &dyn InputProvider, index: u16, pos: u32) -> TirResult<Tensor> {
        let d = self.program.inputs.get(index as usize).ok_or_else(|| TirError::new(TirErrorKind::Malformed, "input39 index"))?;
        let t = inputs.input(index, pos).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("input39 {}", d.name)))?;
        check(&t, &TensorTypeV39::fixed(d.dtype, &d.shape), 1, self.program.profile.max_value_elements)?;
        let (lo, hi) = d.interval();
        if t.data.iter().any(|v| *v < lo || *v > hi) {
            return err(TirErrorKind::Operand, "input39 outside original interval");
        }
        Ok(t)
    }
    fn operand(
        &self,
        r: Ref,
        b: &BlockV39,
        h: u32,
        env: &Occurrence<'_>,
        values: &[Option<Arc<Tensor>>],
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
    ) -> TirResult<Arc<Tensor>> {
        let missing = || TirError::new(TirErrorKind::Missing, "operand39 not supplied/evaluated");
        match r {
            Ref::Node(j) => values.get(j as usize).and_then(Clone::clone).ok_or_else(missing),
            Ref::CarryIn(j) => {
                let t = env.carry.get(&j).ok_or_else(missing)?.clone();
                check(&t, &b.carry_in[j as usize], h, self.program.profile.max_value_elements)?;
                Ok(t)
            }
            Ref::Const(j) => {
                let d = &self.program.consts[j as usize];
                Ok(Arc::new(Tensor::from_le_bytes(d.dtype, &d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>(), &d.data)?))
            }
            Ref::Param(j) => {
                let (layer, d, n) = self.param(params, j, env.layer)?;
                let count = usize::try_from(n).map_err(|_| TirError::new(TirErrorKind::Operand, "parameter39 usize extent"))?;
                let data = self.range(params, j, layer, d.dtype, n, 0, count)?;
                Ok(Arc::new(Tensor::new(d.dtype, d.shape.iter().map(|n| *n as usize).collect(), data)?))
            }
            Ref::State(j) => {
                let d = &self.program.states[j as usize];
                let key = (j, if d.per_layer { env.layer } else { None });
                let t = match env.states {
                    StateView::Run(s) => s.fixed.get(&key),
                    StateView::Cone(e) => e.fixed.get(&j),
                };
                let t = match t {
                    Some(t) => t.clone(),
                    None if matches!(env.states, StateView::Run(_)) => {
                        Tensor::zeros(d.dtype, &d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>())
                    }
                    None => return Err(missing()),
                };
                check(&t, &TensorTypeV39::fixed(d.dtype, &d.shape), 1, self.program.profile.max_value_elements)?;
                if let StateKind::Fixed { lo, hi } = d.kind
                    && t.data.iter().any(|v| *v < lo as i128 || *v > hi as i128)
                {
                    return err(TirErrorKind::Operand, "state39 outside original interval");
                }
                Ok(Arc::new(t))
            }
            Ref::Input(j) if j == INPUT_TOKEN => {
                let t = env.token.ok_or_else(missing)?;
                if t >= self.program.token_bound {
                    return err(TirErrorKind::Operand, "token39 outside original bound");
                }
                Ok(Arc::new(Tensor::scalar(DType::Idx, t as i128)?))
            }
            Ref::Input(j) if j == INPUT_POS => Ok(Arc::new(Tensor::scalar(DType::Idx, env.pos as i128)?)),
            Ref::Input(j) => Ok(Arc::new(self.fetch(inputs, (j - FIRST_INPUT_REF_V2) as u16, env.pos)?)),
        }
    }
    /// Gather directly from an authenticated parameter range. Arbitrary axis
    /// and batch dimensions retain the frozen row-major Gather semantics.
    fn gather_param(
        &self,
        index: u16,
        axis: u8,
        batch: u8,
        ix: &Tensor,
        out: &TensorTypeV39,
        out_shape: &[usize],
        layer: Option<u16>,
        params: &dyn ParamSourceV39,
    ) -> TirResult<Tensor> {
        let (layer, d, total) = self.param(params, index, layer)?;
        let ds: Vec<usize> = d.shape.iter().map(|n| *n as usize).collect();
        let (a, b) = (axis as usize, batch as usize);
        if a >= ds.len() || b > a || b > ix.shape.len() || ix.dtype == DType::I128 || ds[..b] != ix.shape[..b] || out.dtype != d.dtype
        {
            return err(TirErrorKind::Shape, "lazy Gather39 axes/dtype/batch differ");
        }
        let m = ix.shape.len() - b;
        let mut expected = ds[..a].to_vec();
        expected.extend_from_slice(&ix.shape[b..]);
        expected.extend_from_slice(&ds[a + 1..]);
        if expected != out_shape {
            return err(TirErrorKind::Shape, "lazy Gather39 output differs from original indexing");
        }
        let n = product(out_shape)?;
        if n as u64 > self.program.profile.max_value_elements {
            return err(TirErrorKind::Operand, "lazy Gather39 result profile");
        }
        let dst = crate::types::strides(&ds);
        let xst = crate::types::strides(&ix.shape);
        let ost = crate::types::strides(out_shape);
        let mut data = Vec::with_capacity(n);
        let mut start = None;
        let mut length = 0usize;
        let flush = |start: &mut Option<u64>, length: &mut usize, data: &mut Vec<i128>| -> TirResult<()> {
            if let Some(offset) = start.take() {
                data.extend(self.range(params, index, layer, d.dtype, total, offset, *length)?);
                *length = 0;
            }
            Ok(())
        };
        for linear in 0..n {
            let mut rem = linear;
            let o: Vec<usize> = ost
                .iter()
                .map(|s| {
                    let c = rem / s;
                    rem %= s;
                    c
                })
                .collect();
            let mut xi = o[..b].to_vec();
            xi.extend_from_slice(&o[a..a + m]);
            let xp: usize = xi.iter().zip(&xst).map(|(c, s)| c * s).sum();
            let selected =
                *ix.data.get(xp).ok_or_else(|| TirError::new(TirErrorKind::Operand, "lazy Gather39 malformed index tensor"))?;
            if selected < 0 || selected >= ds[a] as i128 {
                return err(TirErrorKind::Index, "lazy Gather39 selected index outside original axis");
            }
            let mut di = o[..a].to_vec();
            di.push(selected as usize);
            di.extend_from_slice(&o[a + m..]);
            let offset = di.iter().zip(&dst).map(|(c, s)| (*c as u64) * (*s as u64)).sum::<u64>();
            if start.is_some_and(|s| s + length as u64 != offset) || length == 65536 {
                flush(&mut start, &mut length, &mut data)?;
            }
            if start.is_none() {
                start = Some(offset);
            }
            length += 1;
        }
        flush(&mut start, &mut length, &mut data)?;
        Tensor::new(out.dtype, out_shape.to_vec(), data)
    }
    fn occurrence(
        &self,
        block: u8,
        env: &Occurrence<'_>,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        needed: &[bool],
        base: u32,
        capture: Option<u16>,
        all_nodes: bool,
        observe: &mut dyn FnMut(CommitPointV39<'_>) -> TirResult<()>,
    ) -> TirResult<(BTreeMap<u8, Arc<Tensor>>, Option<Arc<Tensor>>, Vec<(StateKey, Arc<Tensor>)>, Vec<(StateKey, u32, Arc<Tensor>)>)>
    {
        let p = self.program;
        let b = &p.blocks[block as usize];
        let h = self.history(block, env.pos);
        let mut values = vec![None; b.nodes.len()];
        let mut uses = vec![0usize; b.nodes.len()];
        for (i, node) in b.nodes.iter().enumerate() {
            if needed[i] && !env.supplied.contains_key(&(i as u16)) {
                for r in &node.inputs {
                    if let Ref::Node(j) = r {
                        uses[*j as usize] += 1;
                    }
                }
            }
        }
        let mut roots = vec![false; b.nodes.len()];
        for j in &b.carry_out {
            roots[*j as usize] = true;
        }
        let output = capture
            .map(|j| j as usize)
            .or_else(|| if block == p.schedule.post && env.layer.is_none() { Some(p.output.node() as usize) } else { None });
        if let Some(j) = output {
            roots[j] = true;
        }
        let mut writes = Vec::new();
        let mut appends = Vec::new();
        for (i, node) in b.nodes.iter().enumerate() {
            if !needed[i] {
                continue;
            }
            let out_shape = shape(&node.out, h);
            let n = product(&out_shape)?;
            if n as u64 > p.profile.max_value_elements {
                return err(TirErrorKind::Operand, "node39 result exceeds original value profile");
            }
            let supplied = env.supplied.get(&(i as u16));
            let value = if let Some(t) = supplied {
                check(t, &node.out, h, p.profile.max_value_elements)?;
                Arc::new(t.clone())
            } else {
                let lazy = match (&node.prim, node.inputs.as_slice()) {
                    (NodePrimV39::Frozen(Prim::Gather { axis, batch_dims }), [Ref::Param(j), indices]) => {
                        let ix = self.operand(*indices, b, h, env, &values, params, inputs)?;
                        Some(self.gather_param(*j, *axis, *batch_dims, &ix, &node.out, &out_shape, env.layer, params)?)
                    }
                    _ => None,
                };
                let v = if let Some(t) = lazy {
                    t
                } else {
                    let operands = node
                        .inputs
                        .iter()
                        .map(|r| self.operand(*r, b, h, env, &values, params, inputs))
                        .collect::<TirResult<Vec<_>>>()?;
                    let refs: Vec<&Tensor> = operands.iter().map(AsRef::as_ref).collect();
                    match node.prim {
                        NodePrimV39::Frozen(Prim::HistAppend { state }) => {
                            let d = &p.states[state as usize];
                            let StateKind::Hist { window } = d.kind else {
                                return err(TirErrorKind::Shape, "history39 on Fixed state");
                            };
                            let key = (state, if d.per_layer { env.layer } else { None });
                            let prior: Vec<&Tensor> = match env.states {
                                StateView::Run(s) => s.hist.get(&key).map(|r| r.iter().collect()).unwrap_or_default(),
                                StateView::Cone(e) => e
                                    .hist_prior
                                    .get(&state)
                                    .ok_or_else(|| TirError::new(TirErrorKind::Missing, "history39 prior rows not opened"))?
                                    .iter()
                                    .collect(),
                            };
                            if prior.len() != (env.pos as usize).min(window as usize - 1) {
                                return err(TirErrorKind::Position, "history39 prior row count");
                            }
                            let mut data = Vec::with_capacity(n);
                            let row = TensorTypeV39::fixed(d.dtype, &d.shape);
                            for t in prior {
                                check(t, &row, 1, p.profile.max_value_elements)?;
                                data.extend_from_slice(&t.data);
                            }
                            check(refs[0], &row, 1, p.profile.max_value_elements)?;
                            data.extend_from_slice(&refs[0].data);
                            Tensor::new(d.dtype, out_shape.clone(), data)?
                        }
                        NodePrimV39::HistorySelect { axis, cap } => {
                            if refs.len() != 1
                                || axis as usize >= refs[0].shape.len()
                                || refs[0].shape[axis as usize] != h as usize
                                || cap == 0
                            {
                                return err(TirErrorKind::Shape, "HistorySelect39 input axis is not original history");
                            }
                            eval_prim(&Prim::TopK { axis, k: h.min(cap) }, &refs, node.out.dtype, &out_shape, &p.states)?
                        }
                        NodePrimV39::Frozen(ref prim) => eval_prim(prim, &refs, node.out.dtype, &out_shape, &p.states)?,
                    }
                };
                check(&v, &node.out, h, p.profile.max_value_elements)?;
                Arc::new(v)
            };
            if all_nodes || node.commit {
                observe(CommitPointV39 { slot: base + i as u32, block, layer: env.layer, node: i as u16, value: &value })?;
            }
            // Supplied cone points are authenticated operands, not effects.
            if supplied.is_none() {
                match node.prim {
                    NodePrimV39::Frozen(Prim::StateWrite { state }) => {
                        let d = &p.states[state as usize];
                        writes.push(((state, if d.per_layer { env.layer } else { None }), value.clone()));
                    }
                    NodePrimV39::Frozen(Prim::HistAppend { state }) => {
                        let d = &p.states[state as usize];
                        let StateKind::Hist { window } = d.kind else { unreachable!("validated39") };
                        let row = self.operand(node.inputs[0], b, h, env, &values, params, inputs)?;
                        appends.push(((state, if d.per_layer { env.layer } else { None }), window, row));
                    }
                    _ => {}
                }
                for r in &node.inputs {
                    if let Ref::Node(j) = r {
                        uses[*j as usize] -= 1;
                        if uses[*j as usize] == 0 && !roots[*j as usize] {
                            values[*j as usize] = None;
                        }
                    }
                }
            }
            if uses[i] > 0 || roots[i] {
                values[i] = Some(value);
            }
        }
        let carry = b.carry_out.iter().enumerate().filter_map(|(k, j)| values[*j as usize].clone().map(|t| (k as u8, t))).collect();
        let out = output.and_then(|j| values[j].clone());
        Ok((carry, out, writes, appends))
    }
    /// All state effects are staged until every node and observer succeeds.
    /// Production callers should use this streaming entry point for large
    /// committed tensors and keep observer output provisional until success.
    pub fn step_with_observer(
        &self,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        state: &mut RunState,
        token: u32,
        observe: &mut dyn FnMut(CommitPointV39<'_>) -> TirResult<()>,
    ) -> TirResult<Tensor> {
        self.step_observed(params, inputs, state, token, false, observe)
    }
    /// Observe every evaluated node at its original slot, including non-commit
    /// intermediates. The trace and all state writes remain provisional until
    /// the whole step succeeds. This does not change the commit-flag subset.
    pub fn step_with_trace_observer(
        &self,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        state: &mut RunState,
        token: u32,
        observe: &mut dyn FnMut(CommitPointV39<'_>) -> TirResult<()>,
    ) -> TirResult<Tensor> {
        self.step_observed(params, inputs, state, token, true, observe)
    }
    fn step_observed(
        &self,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        state: &mut RunState,
        token: u32,
        all_nodes: bool,
        observe: &mut dyn FnMut(CommitPointV39<'_>) -> TirResult<()>,
    ) -> TirResult<Tensor> {
        let p = self.program;
        let pos = state.pos;
        if pos >= p.history_bound {
            return err(TirErrorKind::Position, "position39 outside original history bound");
        }
        if token >= p.token_bound && p.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.inputs.contains(&Ref::Input(INPUT_TOKEN))) {
            return err(TirErrorKind::Operand, "token39 outside original vocabulary");
        }
        let mut carry = BTreeMap::new();
        let empty = BTreeMap::new();
        let mut writes = Vec::new();
        let mut appends = Vec::new();
        let mut output = None;
        let mut base = 0u32;
        for (block, layer) in p.occurrences() {
            let b = &p.blocks[block as usize];
            let env = Occurrence { token: Some(token), pos, layer, carry: &carry, states: StateView::Run(state), supplied: &empty };
            let (next, out, w, a) =
                self.occurrence(block, &env, params, inputs, &vec![true; b.nodes.len()], base, None, all_nodes, observe)?;
            carry = next;
            if out.is_some() {
                output = out;
            }
            writes.extend(w);
            appends.extend(a);
            base = base.checked_add(b.nodes.len() as u32).ok_or_else(|| TirError::new(TirErrorKind::Overflow, "slot39 overflow"))?;
        }
        let output = output.ok_or_else(|| TirError::new(TirErrorKind::Missing, "output39 absent"))?;
        for (key, value) in writes {
            state.fixed.insert(key, (*value).clone());
        }
        for (key, window, row) in appends {
            let rows = state.hist.entry(key).or_default();
            rows.push_back((*row).clone());
            while rows.len() > window as usize - 1 {
                rows.pop_front();
            }
        }
        state.pos = pos + 1;
        Ok((*output).clone())
    }
    /// Collecting reference convenience. It deliberately retains every commit;
    /// full-scale producers should use step_with_observer instead.
    pub fn step(
        &self,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        state: &mut RunState,
        token: u32,
    ) -> TirResult<StepOutputV39> {
        let pos = state.pos;
        let mut commits = Vec::new();
        let output = self.step_with_observer(params, inputs, state, token, &mut |c| {
            commits.push(CommitRecord { slot: c.slot, block: c.block, layer: c.layer, node: c.node, value: c.value.clone() });
            Ok(())
        })?;
        Ok(StepOutputV39 { pos, output, commits })
    }
    /// Recompute the disputed target, stopping only at other opened nodes. A
    /// caller cannot supply the target itself or imply initial court state.
    pub fn eval_cone(
        &self,
        block: u8,
        layer: Option<u16>,
        target: u16,
        params: &dyn ParamSourceV39,
        inputs: &dyn InputProvider,
        env: &ConeEnv,
    ) -> TirResult<Tensor> {
        let p = self.program;
        let b = p.blocks.get(block as usize).ok_or_else(|| TirError::new(TirErrorKind::Malformed, "cone39 block"))?;
        if target as usize >= b.nodes.len()
            || env.supplied.contains_key(&target)
            || env.supplied.keys().any(|j| *j as usize >= b.nodes.len())
            || !p.occurrences().contains(&(block, layer))
        {
            return err(TirErrorKind::Malformed, "cone39 target/supplied/occurrence differs");
        }
        if env.pos >= p.history_bound {
            return err(TirErrorKind::Position, "cone39 position outside original bound");
        }
        let mut needed = vec![false; b.nodes.len()];
        let mut stack = vec![target as usize];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut needed[i], true) || env.supplied.contains_key(&(i as u16)) {
                continue;
            }
            for r in &b.nodes[i].inputs {
                if let Ref::Node(j) = r {
                    stack.push(*j as usize);
                }
            }
        }
        let carry = env.carry_in.iter().map(|(k, t)| (*k, Arc::new(t.clone()))).collect();
        let occ =
            Occurrence { token: env.token, pos: env.pos, layer, carry: &carry, states: StateView::Cone(env), supplied: &env.supplied };
        // A target need not be committed. Capture it as an explicit local root
        // without changing the canonical program or its commit flags.
        let (_, out, _, _) = self.occurrence(block, &occ, params, inputs, &needed, 0, Some(target), false, &mut |_| Ok(()))?;
        out.map(|t| (*t).clone()).ok_or_else(|| TirError::new(TirErrorKind::Missing, "cone39 target was not evaluated"))
    }
}
