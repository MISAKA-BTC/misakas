//! Bounded, globally indexed block decode. Row/column/block/group coordinates retain their
//! original values even if a stream chunk ends halfway through a packed block or a tensor row.
use super::*;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct BlockStreamPlan {
    id: [u8; 32],
    rows: usize,
    inp: usize,
    bytes: usize,
}
impl BlockStreamPlan {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}
impl BlocksFormat {
    pub fn streaming_work(&self) -> R<usize> {
        let nodes: Vec<&Node> = match &self.target {
            Target::Floats { value } => vec![value],
            Target::Integers { q, scale, zero, min, .. } => {
                let mut ns = vec![q, scale];
                ns.extend(zero);
                ns.extend(min);
                ns
            }
        };
        nodes.into_iter().try_fold(8usize, |n, node| Ok(n + super::super::expr::streaming_nodes(node, None)?))
    }
    pub fn prepare_streamed(&self, rows: usize, inp: usize) -> R<BlockStreamPlan> {
        self.streaming_work()?;
        let bytes = rows
            .checked_mul(self.row_bytes(inp)?)
            .ok_or_else(|| DslError("FRONTEND_DESCRIPTOR_LIMIT: block byte count overflow".into()))?;
        rows.checked_mul(inp).ok_or_else(|| DslError("FRONTEND_DESCRIPTOR_LIMIT: block shape overflow".into()))?;
        if rows == 0 || inp == 0 || rows > i64::MAX as usize || inp > i64::MAX as usize || bytes > i64::MAX as usize {
            return Err(DslError("FRONTEND_DESCRIPTOR_LIMIT: block dimensions/offsets".into()));
        }
        Ok(BlockStreamPlan { id: self.stream_id, rows, inp, bytes })
    }
    pub fn decode_range_streamed(
        &self,
        plan: &BlockStreamPlan,
        range: Range<usize>,
        read: &dyn Fn(Range<usize>) -> R<Vec<u8>>,
    ) -> R<Vec<f32>> {
        if plan.id != self.stream_id {
            return Err(DslError("FRONTEND_DESCRIPTOR: stream plan belongs to another format".into()));
        }
        if range.start > range.end || range.end > plan.rows * plan.inp || range.len() > 1024 {
            return Err(DslError("FRONTEND_DESCRIPTOR_LIMIT: decode range".into()));
        }
        let n = range.len();
        let make_env = |groups: bool| {
            let mut vars = vec![Vec::with_capacity(n); VARS.len()];
            let mut base = Vec::with_capacity(n);
            for k in range.clone() {
                let row = k / plan.inp;
                let col = k % plan.inp;
                let col = if groups { col / self.group * self.group } else { col };
                let e = col % self.elems;
                let block = col / self.elems;
                let values = [e, e / self.group, block, row, col, col / self.group];
                for (v, x) in vars.iter_mut().zip(values) {
                    v.push(x as i64);
                }
                base.push(((row * (plan.inp / self.elems) + block) * self.bytes) as i64);
            }
            BEnv { raw: &[], n, vars: vars.into_iter().map(Col::I).collect(), base, fmt: self }
        };
        let env = RangeEnv { base: make_env(false), read };
        let values = match &self.target {
            Target::Floats { value } => eval(value, &env, Mask(None))?.into_floats(n),
            Target::Integers { q, scale, zero, min, code } => {
                let codes = eval(q, &env, Mask(None))?.into_ints(n)?;
                let groups = RangeEnv { base: make_env(true), read };
                let scales = eval(scale, &groups, Mask(None))?.into_floats(n);
                let zeros = match zero {
                    Some(z) => eval(z, &groups, Mask(None))?.into_ints(n)?,
                    None => vec![0; n],
                };
                let mins = match min {
                    Some(m) => eval(m, &groups, Mask(None))?.into_floats(n),
                    None => vec![0.0; n],
                };
                let mut values = Vec::with_capacity(n);
                for k in 0..n {
                    if codes[k] < code.min || codes[k] > code.max {
                        return Err(DslError("FRONTEND_DESCRIPTOR: code outside declared range".into()));
                    }
                    i16::try_from(zeros[k]).map_err(|_| DslError("FRONTEND_DESCRIPTOR: zero outside i16".into()))?;
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
    base: BEnv<'a>,
    read: &'a dyn Fn(Range<usize>) -> R<Vec<u8>>,
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
        self.base.read_with(s, index, mask, &|range| (self.read)(range).map(std::borrow::Cow::Owned))
    }
}
