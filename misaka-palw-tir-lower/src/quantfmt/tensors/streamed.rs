//! The stored tensor grammar evaluated through bounded reads, retaining the original two-pass
//! integer semantics: group indexes use column lanes, scales/zeros use group lanes, codes use
//! element lanes. JSON role constants are resolved once into the opaque plan.
use super::*;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct TensorStreamPlan {
    id: [u8; 32],
    headers: Vec<Option<(Vec<usize>, String)>>,
    metadata: Vec<Option<Vec<u8>>>,
    geom: (usize, usize, usize, usize),
    vars: Vec<Col>,
    code: Option<CodeRange>,
}
impl TensorStreamPlan {
    pub fn shape(&self) -> Vec<usize> {
        vec![self.geom.0, self.geom.1]
    }
    /// Resident metadata snapshot, capped at 64 KiB for the whole plan.
    pub fn metadata_bytes(&self) -> usize {
        self.metadata.iter().flatten().map(Vec::len).sum()
    }
}
impl TensorsFormat {
    fn metadata_nodes(&self) -> Vec<&Node> {
        let mut nodes: Vec<_> = [&self.out, &self.inp, &self.group_size]
            .into_iter()
            .chain(self.checks.iter().map(|(n, _)| n))
            .chain(self.offset_term.iter())
            .chain(self.order.iter())
            .collect();
        if let Target::Integers { code, .. } = &self.target {
            nodes.extend([&code.0, &code.1]);
        }
        nodes
    }
    pub fn streaming_work(&self) -> R<usize> {
        // Small role metadata (e.g. compressed-tensors' I64 shape) is snapshotted at preparation;
        // lane-dependent metadata remains refused. Virtual layouts still allow headers only.
        let meta = Some((LANES.len(), 0));
        for n in self.metadata_nodes() {
            super::super::expr::streaming_nodes(n, meta)?;
        }
        let nodes: Vec<&Node> = match &self.target {
            Target::Floats { value } => vec![value],
            Target::Integers { q, scale, zero, min, code } => {
                super::super::expr::streaming_nodes(&code.0, meta)?;
                super::super::expr::streaming_nodes(&code.1, meta)?;
                let mut ns = vec![q, scale];
                ns.extend(zero);
                ns.extend(min);
                ns
            }
        };
        nodes
            .into_iter()
            .chain(self.group_index.iter())
            .try_fold(8usize, |n, node| Ok(n + super::super::expr::streaming_nodes(node, None)?))
    }
    /// Headers suffice, except role metadata read by dimensions/checks or JSON role parameters.
    /// Those roles must be loaded; the total snapshot is capped at 64 KiB and retained in the plan.
    /// Preparing is independent of row/input width: no whole group-index or scale array is built.
    pub fn prepare_streamed(&self, roles: &[Option<RoleTensor>], params: &BTreeMap<String, i64>) -> R<TensorStreamPlan> {
        self.streaming_work()?;
        if roles.len() != self.roles.len() {
            return Err(DslError("FRONTEND_DESCRIPTOR: wrong number of roles".into()));
        }
        check_roles(&self.name, &self.roles, roles)?;
        let mut required = vec![false; self.roles.len()];
        let mut stack = self.metadata_nodes();
        while let Some(n) = stack.pop() {
            match n {
                Node::Read(s, ns) => {
                    if *s < required.len() {
                        required[*s] = true;
                    }
                    stack.extend(ns);
                }
                Node::Call(_, ns) => stack.extend(ns),
                Node::Table(_, n) | Node::Un(_, n) => stack.push(n),
                Node::Bin(_, a, b) => stack.extend([a.as_ref(), b.as_ref()]),
                Node::Cond(c, a, b) => stack.extend([c.as_ref(), a.as_ref(), b.as_ref()]),
                _ => {}
            }
        }
        for p in self.reader.params.values().filter_map(|p| p.from_role.as_ref()) {
            let slot = self.roles.iter().position(|r| r.name == p.role).expect("compiled role parameter");
            required[slot] = true;
        }
        let mut metadata = vec![None; roles.len()];
        let mut bytes = 0usize;
        for (slot, required) in required.into_iter().enumerate() {
            if !required {
                continue;
            }
            let r = roles[slot].as_ref().ok_or_else(|| DslError("FRONTEND_DESCRIPTOR: required role metadata absent".into()))?;
            let size = r.stored_bytes().ok_or_else(|| DslError("FRONTEND_DESCRIPTOR_LIMIT: metadata shape overflow".into()))?;
            bytes = bytes
                .checked_add(size)
                .filter(|n| *n <= 65536)
                .ok_or_else(|| DslError("FRONTEND_DESCRIPTOR_LIMIT: role metadata exceeds 64 KiB".into()))?;
            if size == 0 || r.data.len() != size {
                return Err(DslError("FRONTEND_DESCRIPTOR: role metadata must be loaded exactly".into()));
            }
            metadata[slot] = Some(r.data.clone());
        }
        let (out, inp) = self.dims(roles, params)?;
        out.checked_mul(inp).ok_or_else(|| DslError("FRONTEND_DESCRIPTOR_LIMIT: decoded tensor shape overflow".into()))?;
        if out > i64::MAX as usize || inp > i64::MAX as usize {
            return Err(DslError("FRONTEND_DESCRIPTOR_LIMIT: tensor dimension".into()));
        }
        let env = self.const_env(roles, params, Some((out, inp, 0, 0)))?;
        let gs = eval(&self.group_size, &env, Mask(None))?.int_at(0)?;
        let gs = usize::try_from(gs).ok().filter(|g| *g > 0).ok_or_else(|| DslError("FRONTEND_DESCRIPTOR: group size".into()))?;
        let geom = (out, inp, gs, inp.div_ceil(gs));
        let base = self.const_env(roles, params, Some(geom))?;
        for (n, message) in &self.checks {
            if eval(n, &base, Mask(None))?.int_at(0)? == 0 {
                return Err(DslError(format!("{}: {message}", self.name)));
            }
        }
        let code = match &self.target {
            Target::Integers { code, .. } => Some(
                CodeRange { min: eval(&code.0, &base, Mask(None))?.int_at(0)?, max: eval(&code.1, &base, Mask(None))?.int_at(0)? }
                    .check()?,
            ),
            _ => None,
        };
        let headers = roles.iter().map(|r| r.as_ref().map(|r| (r.shape.clone(), r.dtype.clone()))).collect();
        Ok(TensorStreamPlan { id: self.stream_id, headers, metadata, geom, vars: base.vars, code })
    }
    pub fn decode_range_streamed(
        &self,
        roles: &[Option<RoleTensor>],
        plan: &TensorStreamPlan,
        range: Range<usize>,
        read: &dyn Fn(usize, Range<usize>) -> R<Vec<u8>>,
    ) -> R<Vec<f32>> {
        if plan.id != self.stream_id {
            return Err(DslError("FRONTEND_DESCRIPTOR: stream plan belongs to another format".into()));
        }
        if roles.len() != plan.headers.len()
            || roles.iter().zip(&plan.headers).any(|(r, h)| match (r, h) {
                (Some(r), Some((shape, dtype))) => r.shape != *shape || r.dtype != *dtype,
                (None, None) => false,
                _ => true,
            })
        {
            return Err(DslError("FRONTEND_DESCRIPTOR: role headers changed after stream preparation".into()));
        }
        let (out, inp, gs, ng) = plan.geom;
        let total = out.checked_mul(inp).ok_or_else(|| DslError("decoded tensor shape overflow".into()))?;
        if range.start > range.end || range.end > total || range.len() > 1024 {
            return Err(DslError("FRONTEND_DESCRIPTOR_LIMIT: decode range".into()));
        }
        check_roles(&self.name, &self.roles, roles)?;
        let n = range.len();
        let read_role = |slot: usize, range: Range<usize>| match &plan.metadata[slot] {
            Some(data) => data.get(range).map(|b| b.to_vec()).ok_or_else(|| DslError("FRONTEND_DESCRIPTOR: metadata range".into())),
            None => read(slot, range),
        };
        let mut env = RangeEnv {
            base: TEnv { defs: Defs { roles: &self.roles, tables: &self.tables }, roles, vars: plan.vars.clone(), n },
            read: &read_role,
        };
        let rows = Col::I(range.clone().map(|k| (k / inp) as i64).collect());
        let columns = Col::I(range.clone().map(|k| (k % inp) as i64).collect());
        let values = match &self.target {
            Target::Floats { value } => {
                env.base.vars[0] = rows;
                env.base.vars[1] = columns;
                eval(value, &env, Mask(None))?.into_floats(n)
            }
            Target::Integers { q, scale, zero, min, .. } => {
                // Match decode_integers' gidx pass: o and g remain zero, only i varies.
                env.base.vars[1] = columns.clone();
                let groups = match &self.group_index {
                    Some(index) => eval(index, &env, Mask(None))?.into_ints(n)?,
                    None => range.clone().map(|k| ((k % inp) / gs) as i64).collect(),
                };
                for g in &groups {
                    if u32::try_from(*g).ok().is_none_or(|g| g as usize >= ng) {
                        return Err(DslError(format!("{}: a group index {g} outside [0, {ng})", self.name)));
                    }
                }
                // Group pass: i remains zero even for ragged or reordered input groups.
                env.base.vars[0] = rows.clone();
                env.base.vars[1] = Col::CI(0);
                env.base.vars[2] = Col::I(groups.clone());
                let scales = eval(scale, &env, Mask(None))?.into_floats(n);
                let zeros = match zero {
                    Some(z) => eval(z, &env, Mask(None))?.into_ints(n)?,
                    None => vec![0; n],
                };
                let mins = match min {
                    Some(m) => eval(m, &env, Mask(None))?.into_floats(n),
                    None => vec![0.0; n],
                };
                for z in &zeros {
                    i16::try_from(*z).map_err(|_| DslError(format!("{}: zero outside i16", self.name)))?;
                }
                // Element pass: q sees the selected group along with the original row and column.
                env.base.vars[0] = rows;
                env.base.vars[1] = columns;
                env.base.vars[2] = Col::I(groups);
                let codes = eval(q, &env, Mask(None))?.into_ints(n)?;
                let code = plan.code.expect("integer plan");
                let mut values = Vec::with_capacity(n);
                for k in 0..n {
                    if codes[k] < code.min || codes[k] > code.max {
                        return Err(DslError(format!("{}: code outside declared range", self.name)));
                    }
                    values.push(scales[k] * (codes[k] as f64 - zeros[k] as f64) - mins[k]);
                }
                values
            }
        };
        values
            .into_iter()
            .map(|v| {
                let f = v as f32;
                if f.is_finite() { Ok(f) } else { Err(DslError("FRONTEND_QUANT_NONFINITE: descriptor value".into())) }
            })
            .collect()
    }
}
struct RangeEnv<'a> {
    base: TEnv<'a>,
    read: &'a dyn Fn(usize, Range<usize>) -> R<Vec<u8>>,
}
impl Env for RangeEnv<'_> {
    fn lanes(&self) -> usize {
        self.base.n
    }
    fn var(&self, s: usize) -> &Col {
        self.base.var(s)
    }
    fn table(&self, s: usize) -> &Table {
        self.base.table(s)
    }
    fn read(&self, s: usize, index: &[Col], mask: Mask<'_>) -> R<Col> {
        self.base.read_with(s, index, mask, &|r, range| (self.read)(r, range).map(std::borrow::Cow::Owned))
    }
}
