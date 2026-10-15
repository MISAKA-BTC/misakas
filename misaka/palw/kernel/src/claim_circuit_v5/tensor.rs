//! All PALW-TIR-v1 tensor primitives lowered to the Boolean scalar circuit.
//! Shape/index transforms are computed from public dimensions; dynamic Gather,
//! TopK and Select indices are circuit selectors. No witness value controls a
//! host branch. Native endpoints use the same symbolic tensor wires.
use super::scalar::IntV5;
use super::*;
use misaka_palw_tir::{
    DType, Dim, Prim, Tensor, TensorType,
    program::{StateDecl, StateKind},
};
const RANK: usize = misaka_palw_tir::types::MAX_RANK;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitShapeV5 {
    rank: usize,
    dims: [usize; RANK],
    elements: usize,
}
impl CircuitShapeV5 {
    pub fn new(dims: &[usize]) -> Result<Self, String> {
        if dims.len() > RANK || dims.iter().any(|d| *d == 0 || *d > misaka_palw_tir::types::MAX_DIM as usize) {
            return Err("circuit tensor shape outside TIR-v1".into());
        }
        let elements = dims.iter().try_fold(1usize, |n, d| n.checked_mul(*d)).ok_or("circuit shape product overflow")?;
        if elements as u64 > misaka_palw_tir::types::MAX_ELEMENTS {
            return Err("circuit tensor element cap exceeded".into());
        }
        let mut shape = Self { rank: dims.len(), dims: [0; RANK], elements };
        shape.dims[..dims.len()].copy_from_slice(dims);
        Ok(shape)
    }
    pub fn dims(&self) -> &[usize] {
        &self.dims[..self.rank]
    }
    pub fn elements(&self) -> usize {
        self.elements
    }
    fn coordinate(&self, mut index: usize) -> Result<[usize; RANK], String> {
        if index >= self.elements {
            return Err("circuit tensor linear index".into());
        }
        let mut coords = [0; RANK];
        for i in (0..self.rank).rev() {
            coords[i] = index % self.dims[i];
            index /= self.dims[i];
        }
        Ok(coords)
    }
    fn index(&self, coords: &[usize]) -> Result<usize, String> {
        if coords.len() < self.rank {
            return Err("circuit coordinate rank".into());
        }
        let mut index = 0usize;
        for (i, c) in coords.iter().enumerate().take(self.rank) {
            if *c >= self.dims[i] {
                return Err("circuit tensor coordinate out of range".into());
            }
            index = index.checked_mul(self.dims[i]).and_then(|n| n.checked_add(*c)).ok_or("circuit index overflow")?;
        }
        Ok(index)
    }
    fn broadcast_index(&self, output: CircuitShapeV5, index: usize) -> Result<usize, String> {
        if self.rank > output.rank {
            return Err("circuit broadcast rank".into());
        }
        let coords = output.coordinate(index)?;
        let offset = output.rank - self.rank;
        let mut own = [0; RANK];
        for i in 0..self.rank {
            if self.dims[i] != 1 && self.dims[i] != output.dims[offset + i] {
                return Err("circuit broadcast shape mismatch".into());
            }
            own[i] = if self.dims[i] == 1 { 0 } else { coords[offset + i] };
        }
        self.index(&own)
    }
}
enum TensorStorageV5<'a> {
    Input(InputByteSpanV5),
    Constant(&'a Tensor),
    Raw(&'a [u8]),
    Repeated(IntV5),
    Words(Vec<IntV5>),
    Counting,
}
pub struct CircuitTensorV5<'a> {
    dtype: DType,
    shape: CircuitShapeV5,
    storage: TensorStorageV5<'a>,
    written: usize,
    _lease: Option<CircuitMemoryLeaseV5>,
}
impl<'a> CircuitTensorV5<'a> {
    pub fn dtype(&self) -> DType {
        self.dtype
    }
    pub fn shape(&self) -> CircuitShapeV5 {
        self.shape
    }
    pub fn input(builder: &mut CircuitBuilderV5, dtype: DType, shape: &[usize]) -> Result<Self, String> {
        let shape = CircuitShapeV5::new(shape)?;
        let bytes = shape.elements.checked_mul(dtype.width()).ok_or("circuit tensor input bytes overflow")?;
        let span = builder.input_byte_span(bytes)?;
        Ok(Self { dtype, shape, storage: TensorStorageV5::Input(span), written: shape.elements, _lease: None })
    }
    pub(crate) fn raw_constant(dtype: DType, shape: &[usize], bytes: &'a [u8]) -> Result<Self, String> {
        let shape = CircuitShapeV5::new(shape)?;
        if shape.elements.checked_mul(dtype.width()) != Some(bytes.len()) {
            return Err("raw circuit constant length".into());
        }
        Ok(Self { dtype, shape, storage: TensorStorageV5::Raw(bytes), written: shape.elements, _lease: None })
    }
    pub(crate) fn repeated(dtype: DType, shape: &[usize], value: i128) -> Result<Self, String> {
        if !dtype.contains(value) {
            return Err("repeated circuit constant outside dtype".into());
        }
        let shape = CircuitShapeV5::new(shape)?;
        let word = std::array::from_fn(|i| u32::from(((value as u128) >> i) & 1 != 0));
        Ok(Self { dtype, shape, storage: TensorStorageV5::Repeated(word), written: shape.elements, _lease: None })
    }
    pub fn constant(tensor: &'a Tensor) -> Result<Self, String> {
        let shape = CircuitShapeV5::new(&tensor.shape)?;
        if shape.elements != tensor.data.len() || tensor.data.iter().any(|v| !tensor.dtype.contains(*v)) {
            return Err("circuit constant tensor malformed".into());
        }
        Ok(Self { dtype: tensor.dtype, shape, storage: TensorStorageV5::Constant(tensor), written: shape.elements, _lease: None })
    }
    fn output(
        builder: &CircuitBuilderV5,
        dtype: DType,
        shape: CircuitShapeV5,
        memory: &CircuitMemoryBudgetV5,
    ) -> Result<Self, String> {
        if builder.is_preflight() {
            return Ok(Self { dtype, shape, storage: TensorStorageV5::Counting, written: 0, _lease: None });
        }
        let bytes = shape.elements.checked_mul(std::mem::size_of::<IntV5>()).ok_or("circuit tensor wire storage overflow")?;
        let lease = memory.reserve(u64::try_from(bytes).map_err(|_| "circuit wire bytes not representable")?)?;
        let mut values = Vec::new();
        values.try_reserve_exact(shape.elements).map_err(|e| e.to_string())?;
        if values.capacity() != shape.elements {
            return Err("circuit tensor allocation differs from exact shape plan".into());
        }
        Ok(Self { dtype, shape, storage: TensorStorageV5::Words(values), written: 0, _lease: Some(lease) })
    }
    /// Allocate only after the resolved typed shape is admitted. Each element
    /// comes from existing wires or a public constant, never an opaque result.
    pub(crate) fn from_word_stream(
        builder: &mut CircuitBuilderV5,
        dtype: DType,
        shape: &[usize],
        memory: &CircuitMemoryBudgetV5,
        mut element: impl FnMut(&mut CircuitBuilderV5, usize) -> Result<IntV5, String>,
    ) -> Result<Self, String> {
        let shape = CircuitShapeV5::new(shape)?;
        let mut output = Self::output(builder, dtype, shape, memory)?;
        for index in 0..shape.elements {
            let value = element(builder, index)?;
            builder.assert_fit(&value, dtype)?;
            output.push(value)?;
        }
        output.complete()
    }
    pub fn at(&self, index: usize) -> Result<IntV5, String> {
        if index >= self.shape.elements || index >= self.written {
            return Err("circuit tensor element index or forward reference".into());
        }
        match &self.storage {
            TensorStorageV5::Counting => Ok([0; 128]),
            TensorStorageV5::Words(values) => values.get(index).copied().ok_or("unwritten circuit tensor element".into()),
            TensorStorageV5::Repeated(word) => Ok(*word),
            TensorStorageV5::Raw(bytes) => {
                let at = index.checked_mul(self.dtype.width()).ok_or("raw constant offset overflow")?;
                let value = self.dtype.decode_le(&bytes[at..at + self.dtype.width()]);
                Ok(std::array::from_fn(|i| u32::from(((value as u128) >> i) & 1 != 0)))
            }
            TensorStorageV5::Constant(t) => Ok(std::array::from_fn(|i| u32::from(((t.data[index] as u128) >> i) & 1 != 0))),
            TensorStorageV5::Input(span) => {
                let width = self.dtype.width();
                let mut word = [0; 128];
                for i in 0..width {
                    word[i * 8..i * 8 + 8].copy_from_slice(&span.byte(
                        index.checked_mul(width).and_then(|n| n.checked_add(i)).ok_or("circuit input element byte overflow")?,
                    )?);
                }
                let fill = if self.dtype == DType::Idx { 0 } else { word[width * 8 - 1] };
                word[width * 8..].fill(fill);
                Ok(word)
            }
        }
    }
    fn push(&mut self, value: IntV5) -> Result<(), String> {
        if self.written >= self.shape.elements {
            return Err("circuit tensor output exceeds shape plan".into());
        }
        if let TensorStorageV5::Words(values) = &mut self.storage {
            if values.len() >= values.capacity() {
                return Err("circuit output exceeds reserved typed capacity".into());
            }
            values.push(value);
        }
        self.written += 1;
        Ok(())
    }
    fn complete(self) -> Result<Self, String> {
        if self.written != self.shape.elements { Err("circuit tensor output incomplete".into()) } else { Ok(self) }
    }
}
impl CircuitByteStreamV5 for CircuitTensorV5<'_> {
    fn len(&self) -> usize {
        self.shape.elements * self.dtype.width()
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        if index >= self.len() {
            return Err("circuit tensor byte index".into());
        }
        let word = self.at(index / self.dtype.width())?;
        let offset = (index % self.dtype.width()) * 8;
        Ok(std::array::from_fn(|i| word[offset + i]))
    }
}
fn term_sum(
    builder: &mut CircuitBuilderV5,
    dtype: DType,
    count: usize,
    mut term: impl FnMut(&mut CircuitBuilderV5, usize) -> Result<IntV5, String>,
) -> Result<IntV5, String> {
    let zero = builder.integer(0);
    let mut positive = zero;
    let mut negative = zero;
    for i in 0..count {
        let value = term(builder, i)?;
        builder.assert_fit(&value, dtype)?;
        let pos = builder.positive(&value)?;
        let p = builder.select_word(pos, &value, &zero)?;
        let n = builder.select_word(pos, &zero, &value)?;
        positive = builder.checked_add_integer(&positive, &p)?;
        negative = builder.checked_add_integer(&negative, &n)?;
        // Every partial sum in every ordering is within the dtype iff these
        // positive and negative extremes are both within it (native semantics).
        builder.assert_fit(&positive, dtype)?;
        builder.assert_fit(&negative, dtype)?;
    }
    builder.checked_add_integer(&positive, &negative)
}
fn expect_shape(actual: CircuitShapeV5, wanted: &[usize]) -> Result<(), String> {
    if actual.dims() != wanted { Err("circuit output shape differs from primitive relation".into()) } else { Ok(()) }
}
/// Bound the reference type check's actual temporary buffers before creating
/// them. TIR-v1 has at most eight operands and rank four. The largest scratch
/// path is HistAppend: row[4], fixed_shape[4], and growing result[capacity 8].
/// Gather can grow to capacity eight even for an invalid output shape; Select
/// has two rank-four broadcast results. Error strings are diagnostic data and
/// are not retained in a successful circuit owner.
fn validate_primitive_v5(
    prim: &Prim,
    inputs: &[&CircuitTensorV5<'_>],
    output: DType,
    shape: CircuitShapeV5,
    states: &[StateDecl],
    memory: &CircuitMemoryBudgetV5,
) -> Result<(), String> {
    const OPERANDS: usize = misaka_palw_tir::program::MAX_NODE_INPUTS;
    if inputs.len() > OPERANDS {
        return Err("circuit operand bound exceeded".into());
    }
    if let Prim::StateWrite { state } | Prim::HistAppend { state } = prim {
        let state = states.get(*state as usize).ok_or("circuit state absent")?;
        if state.shape.len() > RANK
            || state.shape.iter().any(|d| *d == 0 || *d > misaka_palw_tir::types::MAX_DIM)
            || state
                .shape
                .iter()
                .try_fold(1u64, |n, d| n.checked_mul(u64::from(*d)))
                .is_none_or(|n| n > misaka_palw_tir::types::MAX_ELEMENTS)
        {
            return Err("circuit state shape outside bounded TIR-v1".into());
        }
    }
    let metadata = (inputs.len() + 1)
        .checked_mul(RANK)
        .and_then(|n| n.checked_add(4 * RANK))
        .and_then(|n| n.checked_mul(std::mem::size_of::<Dim>()))
        .and_then(|n| n.checked_add(RANK * std::mem::size_of::<bool>()))
        .ok_or("typecheck metadata byte overflow")?;
    let _metadata = memory.reserve(metadata as u64)?;
    let ty = |dtype, dims: &[usize], history: bool| -> Result<TensorType, String> {
        let mut shape = Vec::new();
        shape.try_reserve_exact(dims.len()).map_err(|e| e.to_string())?;
        if shape.capacity() != dims.len() {
            return Err("type metadata capacity differs from plan".into());
        }
        for (i, d) in dims.iter().enumerate() {
            shape.push(if history && i == 0 { Dim::H } else { Dim::Fixed(u32::try_from(*d).map_err(|_| "type dimension overflow")?) });
        }
        Ok(TensorType::new(dtype, shape))
    };
    let mut ins: [TensorType; OPERANDS] = std::array::from_fn(|_| TensorType::scalar(DType::I8));
    for (i, input) in inputs.iter().enumerate() {
        ins[i] = ty(input.dtype, input.shape.dims(), false)?;
    }
    let out = ty(output, shape.dims(), matches!(prim, Prim::HistAppend { .. }))?;
    misaka_palw_tir::validate::check_node_type(prim, &ins[..inputs.len()], &out, states)
}
/// All25 primitive semantics. Caller provides a registered program's resolved
/// dimensions and exact prior history rows. State continuity is reuse of the
/// previous output's wires, never a separately chosen entry-state witness.
pub fn emit_primitive_v5<'a>(
    builder: &mut CircuitBuilderV5,
    prim: &Prim,
    inputs: &[&CircuitTensorV5<'a>],
    output: DType,
    shape: &[usize],
    states: &[StateDecl],
    prior: &[&CircuitTensorV5<'a>],
    memory: &CircuitMemoryBudgetV5,
) -> Result<CircuitTensorV5<'a>, String> {
    let arity = prim.arity();
    if inputs.len() < arity.0 || inputs.len() > arity.1 {
        return Err("circuit primitive arity mismatch".into());
    }
    let out_shape = CircuitShapeV5::new(shape)?;
    validate_primitive_v5(prim, inputs, output, out_shape, states, memory)?;
    let mut result = CircuitTensorV5::output(builder, output, out_shape, memory)?;
    match prim {
        Prim::TopK { axis, k } => {
            let input = inputs[0];
            let axis = *axis as usize;
            if axis >= input.shape.rank || *k == 0 || *k as usize > input.shape.dims[axis] || output != DType::Idx {
                return Err("circuit TopK axis/k/dtype".into());
            }
            let mut wanted = input.shape;
            wanted.dims[axis] = *k as usize;
            wanted.elements = wanted.dims().iter().product();
            expect_shape(out_shape, wanted.dims())?;
            for at in 0..out_shape.elements {
                let coords = out_shape.coordinate(at)?;
                let slot = coords[axis];
                let mut prefix = builder.integer(0);
                let mut selected = builder.integer(0);
                let one = builder.integer(1);
                for candidate in 0..input.shape.dims[axis] {
                    let mut candidate_coords = coords;
                    candidate_coords[axis] = candidate;
                    let value = input.at(input.shape.index(&candidate_coords)?)?;
                    let mut rank = builder.integer(0);
                    for other in 0..input.shape.dims[axis] {
                        let mut other_coords = coords;
                        other_coords[axis] = other;
                        let other_value = input.at(input.shape.index(&other_coords)?)?;
                        let greater = builder.signed_less(&value, &other_value)?;
                        let before = if other < candidate {
                            let equal = builder.equal_word(&value, &other_value)?;
                            builder.or(greater, equal)?
                        } else {
                            greater
                        };
                        let zero = builder.integer(0);
                        let increment = builder.select_word(before, &one, &zero)?;
                        rank = builder.add_word(&rank, &increment)?.0;
                    }
                    let kept = builder.unsigned_less(&rank, &builder.integer(*k as i128))?;
                    let is_slot = builder.equal_word(&prefix, &builder.integer(slot as i128))?;
                    let choose = builder.and(kept, is_slot)?;
                    selected = builder.select_word(choose, &builder.integer(candidate as i128), &selected)?;
                    let zero = builder.integer(0);
                    let increment = builder.select_word(kept, &one, &zero)?;
                    prefix = builder.add_word(&prefix, &increment)?.0;
                }
                builder.assert_integer_equal(&prefix, &builder.integer(*k as i128))?;
                result.push(selected)?;
            }
        }
        Prim::HistAppend { state } => {
            let state = states.get(*state as usize).ok_or("circuit history state absent")?;
            let StateKind::Hist { window } = state.kind else { return Err("circuit history on Fixed state".into()) };
            let current = inputs[0];
            if current.dtype != state.dtype
                || current.shape.rank != state.shape.len()
                || current.shape.dims().iter().zip(&state.shape).any(|(a, b)| *a != *b as usize)
                || prior.len() >= window as usize
            {
                return Err("circuit history row/window mismatch".into());
            }
            if out_shape.rank != current.shape.rank + 1
                || out_shape.dims[0] != prior.len() + 1
                || &out_shape.dims()[1..] != current.shape.dims()
            {
                return Err("circuit history output shape".into());
            }
            for row in prior.iter().copied().chain(std::iter::once(current)) {
                if row.dtype != current.dtype || row.shape != current.shape {
                    return Err("circuit prior history tensor differs from state row".into());
                }
                for i in 0..current.shape.elements {
                    result.push(row.at(i)?)?;
                }
            }
        }
        _ => {
            for index in 0..out_shape.elements {
                let coords = out_shape.coordinate(index)?;
                let value = match prim {
                    Prim::Reshape | Prim::Cast => {
                        if inputs[0].shape.elements != out_shape.elements {
                            return Err("circuit reshape/cast element count".into());
                        }
                        inputs[0].at(index)?
                    }
                    Prim::Transpose { perm } => {
                        if perm.len() != out_shape.rank || inputs[0].shape.rank != out_shape.rank {
                            return Err("circuit transpose rank".into());
                        }
                        let mut own = [0; RANK];
                        let mut seen = [false; RANK];
                        for (i, p) in perm.iter().enumerate() {
                            let p = *p as usize;
                            if p >= out_shape.rank || seen[p] {
                                return Err("circuit transpose permutation".into());
                            }
                            seen[p] = true;
                            own[p] = coords[i];
                        }
                        inputs[0].at(inputs[0].shape.index(&own)?)?
                    }
                    Prim::Slice { axis, start } => {
                        let axis = *axis as usize;
                        if axis >= out_shape.rank {
                            return Err("circuit slice axis".into());
                        }
                        let mut own = coords;
                        own[axis] = own[axis].checked_add(*start as usize).ok_or("circuit slice offset overflow")?;
                        inputs[0].at(inputs[0].shape.index(&own)?)?
                    }
                    Prim::Concat { axis } => {
                        let axis = *axis as usize;
                        if axis >= out_shape.rank {
                            return Err("circuit concat axis".into());
                        }
                        let mut own = coords;
                        let mut found = None;
                        for input in inputs {
                            if input.shape.rank != out_shape.rank {
                                return Err("circuit concat rank".into());
                            }
                            if own[axis] < input.shape.dims[axis] {
                                found = Some(input.at(input.shape.index(&own)?)?);
                                break;
                            }
                            own[axis] -= input.shape.dims[axis];
                        }
                        found.ok_or("circuit concat coordinate missing")?
                    }
                    Prim::Broadcast => inputs[0].at(inputs[0].shape.broadcast_index(out_shape, index)?)?,
                    Prim::Iota { axis, start, step } => {
                        let coordinate =
                            *coords.get(*axis as usize).filter(|_| (*axis as usize) < out_shape.rank).ok_or("circuit iota axis")?;
                        let value = i128::from(*step)
                            .checked_mul(coordinate as i128)
                            .and_then(|v| v.checked_add(i128::from(*start)))
                            .ok_or("circuit iota constant overflow")?;
                        if !output.contains(value) {
                            return Err("circuit iota outside dtype".into());
                        }
                        builder.integer(value)
                    }
                    Prim::Gather { axis, batch_dims } => {
                        let (data, indices) = (inputs[0], inputs[1]);
                        let (a, b) = (*axis as usize, *batch_dims as usize);
                        if a >= data.shape.rank || b > indices.shape.rank || b > a {
                            return Err("circuit Gather axis/batch/index type".into());
                        }
                        let m = indices.shape.rank - b;
                        if out_shape.rank != data.shape.rank - 1 + m {
                            return Err("circuit Gather output rank".into());
                        }
                        let mut ix = [0; RANK];
                        ix[..b].copy_from_slice(&coords[..b]);
                        ix[b..indices.shape.rank].copy_from_slice(&coords[a..a + m]);
                        let chosen = indices.at(indices.shape.index(&ix)?)?;
                        builder.assert_interval(&chosen, 0, data.shape.dims[a] as i128 - 1)?;
                        let mut di = [0; RANK];
                        di[..a].copy_from_slice(&coords[..a]);
                        di[a + 1..data.shape.rank].copy_from_slice(&coords[a + m..out_shape.rank]);
                        let mut value = builder.integer(0);
                        for j in 0..data.shape.dims[a] {
                            di[a] = j;
                            let match_index = builder.equal_word(&chosen, &builder.integer(j as i128))?;
                            value = builder.select_word(match_index, &data.at(data.shape.index(&di)?)?, &value)?;
                        }
                        value
                    }
                    Prim::MatMul => {
                        let (x, y) = (inputs[0], inputs[1]);
                        if x.shape.rank < 2 || y.shape.rank < 2 || out_shape.rank < 2 {
                            return Err("circuit MatMul ranks".into());
                        }
                        let (xr, yr, or) = (x.shape.rank, y.shape.rank, out_shape.rank);
                        let (m, k, n) = (x.shape.dims[xr - 2], x.shape.dims[xr - 1], y.shape.dims[yr - 1]);
                        if y.shape.dims[yr - 2] != k || out_shape.dims[or - 2] != m || out_shape.dims[or - 1] != n {
                            return Err("circuit MatMul contraction/output shape".into());
                        }
                        let batch = index / (m * n);
                        let output_batch = CircuitShapeV5::new(&shape[..or - 2])?;
                        let xb = CircuitShapeV5::new(&x.shape.dims()[..xr - 2])?.broadcast_index(output_batch, batch)?;
                        let yb = CircuitShapeV5::new(&y.shape.dims()[..yr - 2])?.broadcast_index(output_batch, batch)?;
                        let row = coords[or - 2];
                        let column = coords[or - 1];
                        term_sum(builder, output, k, |b, t| {
                            b.checked_multiply_integer(&x.at(xb * m * k + row * k + t)?, &y.at(yb * k * n + t * n + column)?)
                        })?
                    }
                    Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                        let input = inputs[0];
                        let axis = *axis as usize;
                        if axis >= input.shape.rank || out_shape.rank != input.shape.rank || out_shape.dims[axis] != 1 {
                            return Err("circuit reduction axis/shape".into());
                        }
                        if matches!(prim, Prim::ReduceSum { .. }) {
                            term_sum(builder, output, input.shape.dims[axis], |_, i| {
                                let mut at = coords;
                                at[axis] = i;
                                input.at(input.shape.index(&at)?)
                            })?
                        } else {
                            let mut at = coords;
                            at[axis] = 0;
                            let mut value = input.at(input.shape.index(&at)?)?;
                            for i in 1..input.shape.dims[axis] {
                                at[axis] = i;
                                let next = input.at(input.shape.index(&at)?)?;
                                let greater = builder.signed_less(&value, &next)?;
                                value = builder.select_word(greater, &next, &value)?;
                            }
                            value
                        }
                    }
                    Prim::StateWrite { state } => {
                        let state = states.get(*state as usize).ok_or("circuit Fixed state absent")?;
                        let StateKind::Fixed { lo, hi } = state.kind else { return Err("circuit write on Hist state".into()) };
                        expect_shape(out_shape, inputs[0].shape.dims())?;
                        builder.clamp_integer(&inputs[0].at(index)?, i128::from(lo), i128::from(hi))?
                    }
                    Prim::TopK { .. } | Prim::HistAppend { .. } => unreachable!("handled whole-tensor primitives"),
                    _ => {
                        let mut values = [[0; 128]; 3];
                        for (i, input) in inputs.iter().enumerate() {
                            if i >= 3 {
                                return Err("scalar primitive operand count".into());
                            }
                            values[i] = input.at(input.shape.broadcast_index(out_shape, index)?)?;
                        }
                        super::scalar::emit_scalar_v5(builder, prim, &values[..inputs.len()], output)?
                    }
                };
                builder.assert_fit(&value, output)?;
                result.push(value)?;
            }
        }
    }
    result.complete()
}

/// Public native tensor identity whose exact bytes are constrained by the
/// circuit. No tensor root is accepted as an unconstrained witness value.
#[derive(Clone, Copy)]
pub struct NativeTensorEndpointV5<'a> {
    pub dtype: DType,
    pub shape: &'a [usize],
    pub suite: super::native_commitment::NativeTensorCommitmentV5,
    pub root: Digest,
}
pub(crate) fn constrain_endpoint_v5(
    builder: &mut CircuitBuilderV5,
    endpoint: NativeTensorEndpointV5<'_>,
    tensor: &CircuitTensorV5<'_>,
) -> Result<(), String> {
    if endpoint.dtype != tensor.dtype || endpoint.shape != tensor.shape.dims() {
        return Err("native endpoint differs from symbolic tensor type".into());
    }
    let root = super::native_commitment::native_tensor_root_v5(builder, endpoint.suite, endpoint.dtype, endpoint.shape, tensor)?;
    builder.assert_bytes(&root, &endpoint.root)
}
/// Compile a real primitive relation, including all input/prior-history native
/// commitments and the output commitment. Prior history and the current row
/// share the exact wires used by HistAppend. A whole trace compiler must still
/// supply the authenticated prior-state and program graph; this API does not
/// elevate an isolated relation into a native claim or a reviewed proof.
pub fn compile_native_primitive_v5(
    prim: &Prim,
    inputs: &[NativeTensorEndpointV5<'_>],
    output: NativeTensorEndpointV5<'_>,
    states: &[StateDecl],
    prior: &[NativeTensorEndpointV5<'_>],
    limits: CircuitLimitsV5,
    memory: CircuitMemoryBudgetV5,
) -> Result<ReservedCircuitV5, String> {
    const OPERANDS: usize = misaka_palw_tir::program::MAX_NODE_INPUTS;
    let arity = prim.arity();
    if inputs.len() < arity.0 || inputs.len() > arity.1 || inputs.len() > OPERANDS {
        return Err("native primitive operand count".into());
    }
    if !matches!(prim, Prim::HistAppend { .. }) && !prior.is_empty() {
        return Err("only HistAppend may carry prior history endpoints".into());
    }
    CircuitShapeV5::new(output.shape)?;
    for endpoint in inputs.iter().chain(prior) {
        CircuitShapeV5::new(endpoint.shape)?;
    }
    // Prior rows need one owner slot each; reserve this typed header buffer
    // before either preflight or graph emission. The tensor values themselves
    // remain symbolic spans rather than an independently allocated witness.
    let prior_bytes = prior
        .len()
        .checked_mul(std::mem::size_of::<CircuitTensorV5<'_>>())
        .and_then(|n| n.checked_add(prior.len().checked_mul(std::mem::size_of::<&CircuitTensorV5<'_>>())?))
        .ok_or("prior history header reservation overflow")?;
    let _headers = memory.reserve(prior_bytes as u64)?;
    let _hash = memory.reserve(super::native_commitment::native_tensor_workspace_bytes_v5())?;
    let emit = |builder: &mut CircuitBuilderV5| -> Result<(), String> {
        let mut tensors: [Option<CircuitTensorV5<'_>>; OPERANDS] = std::array::from_fn(|_| None);
        for (i, endpoint) in inputs.iter().enumerate() {
            let tensor = CircuitTensorV5::input(builder, endpoint.dtype, endpoint.shape)?;
            constrain_endpoint_v5(builder, *endpoint, &tensor)?;
            tensors[i] = Some(tensor);
        }
        let mut rows = Vec::new();
        rows.try_reserve_exact(prior.len()).map_err(|e| e.to_string())?;
        if rows.capacity() != prior.len() {
            return Err("prior row header capacity differs from plan".into());
        }
        for endpoint in prior {
            let row = CircuitTensorV5::input(builder, endpoint.dtype, endpoint.shape)?;
            constrain_endpoint_v5(builder, *endpoint, &row)?;
            rows.push(row);
        }
        let mut row_refs = Vec::new();
        row_refs.try_reserve_exact(rows.len()).map_err(|e| e.to_string())?;
        if row_refs.capacity() != rows.len() {
            return Err("prior reference capacity differs from plan".into());
        }
        row_refs.extend(rows.iter());
        // Fixed stack storage avoids an uncharged Vec of operand references.
        let mut refs: [Option<&CircuitTensorV5<'_>>; OPERANDS] = [None; OPERANDS];
        for (i, tensor) in tensors.iter().take(inputs.len()).enumerate() {
            refs[i] = tensor.as_ref();
        }
        let empty = CircuitTensorV5 {
            dtype: DType::I8,
            shape: CircuitShapeV5::new(&[])?,
            storage: TensorStorageV5::Counting,
            written: 1,
            _lease: None,
        };
        let refs: [&CircuitTensorV5<'_>; OPERANDS] = std::array::from_fn(|i| refs[i].unwrap_or(&empty));
        let result = emit_primitive_v5(builder, prim, &refs[..inputs.len()], output.dtype, output.shape, states, &row_refs, &memory)?;
        constrain_endpoint_v5(builder, output, &result)
    };
    let mut counter = CircuitBuilderV5::preflight(limits)?;
    emit(&mut counter)?;
    let plan = counter.preflight_cost()?;
    let mut builder = CircuitBuilderV5::new_planned(limits, plan, memory.clone())?;
    emit(&mut builder)?;
    builder.finish_reserved()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: 20_000_000,
            maximum_input_bits: 100_000,
            maximum_assertions: 1_000_000,
            maximum_graph_bytes: 500_000_000,
            maximum_witness_bytes: 20_000_000,
        }
    }
    fn tensor(dtype: DType, shape: &[usize], data: &[i128]) -> Tensor {
        Tensor::new(dtype, shape.to_vec(), data.to_vec()).unwrap()
    }
    fn relation(prim: &Prim, inputs: &[Tensor], expected: &Tensor, states: &[StateDecl], prior: &[Tensor]) -> Result<(), String> {
        let memory = CircuitMemoryBudgetV5::new(600_000_000);
        let emit = |b: &mut CircuitBuilderV5| -> Result<(), String> {
            let tensors: Vec<_> = inputs.iter().map(|t| CircuitTensorV5::input(b, t.dtype, &t.shape)).collect::<Result<_, _>>()?;
            let rows: Vec<_> = prior.iter().map(|t| CircuitTensorV5::input(b, t.dtype, &t.shape)).collect::<Result<_, _>>()?;
            let refs: Vec<_> = tensors.iter().collect();
            let rows: Vec<_> = rows.iter().collect();
            let result = emit_primitive_v5(b, prim, &refs, expected.dtype, &expected.shape, states, &rows, &memory)?;
            for (i, value) in expected.data.iter().enumerate() {
                b.assert_integer_equal(&result.at(i)?, &b.integer(*value))?;
            }
            Ok(())
        };
        let mut count = CircuitBuilderV5::preflight(limits())?;
        emit(&mut count)?;
        let plan = count.preflight_cost()?;
        let mut builder = CircuitBuilderV5::new_planned(limits(), plan, memory.clone())?;
        emit(&mut builder)?;
        let graph = builder.finish_reserved()?;
        let bytes: Vec<_> = inputs.iter().chain(prior).flat_map(Tensor::to_le_bytes).collect();
        let witness = graph.evaluate(&input_bits_v5(&bytes), limits())?;
        drop(witness);
        drop(graph);
        if memory.held() != 0 {
            return Err("tensor circuit leaked a typed memory lease".into());
        }
        Ok(())
    }
    #[test]
    fn stateless_indexing_ranking_and_reduction_match_the_reference() {
        let x = tensor(DType::I8, &[2, 3], &[4, -2, 4, 0, 5, -7]);
        let cases = [
            (Prim::Reshape, vec![x.clone()], DType::I8, vec![3, 2]),
            (Prim::Transpose { perm: vec![1, 0] }, vec![x.clone()], DType::I8, vec![3, 2]),
            (Prim::Slice { axis: 1, start: 1 }, vec![x.clone()], DType::I8, vec![2, 2]),
            (Prim::Concat { axis: 0 }, vec![x.clone(), x.clone()], DType::I8, vec![4, 3]),
            (Prim::Broadcast, vec![tensor(DType::I8, &[1, 3], &[4, -2, 4])], DType::I8, vec![2, 3]),
            (Prim::Iota { axis: 1, start: -2, step: 2 }, vec![], DType::I8, vec![2, 3]),
            // Signed integer indices are accepted by the actual TIR type rule.
            (Prim::Gather { axis: 1, batch_dims: 0 }, vec![x.clone(), tensor(DType::I16, &[2], &[2, 0])], DType::I8, vec![2, 2]),
            (Prim::MatMul, vec![x.clone(), tensor(DType::I8, &[3, 1], &[2, 1, -1])], DType::I32, vec![2, 1]),
            (Prim::ReduceSum { axis: 1 }, vec![x.clone()], DType::I32, vec![2, 1]),
            (Prim::ReduceMax { axis: 1 }, vec![x.clone()], DType::I8, vec![2, 1]),
            (Prim::TopK { axis: 1, k: 2 }, vec![x], DType::Idx, vec![2, 2]),
        ];
        for (prim, inputs, dtype, shape) in cases {
            let expected = misaka_palw_tir::eval::eval_primitive(&prim, &inputs, dtype, &shape).unwrap();
            relation(&prim, &inputs, &expected, &[], &[]).unwrap();
            let mut bad = expected;
            bad.data[0] += 1;
            assert!(relation(&prim, &inputs, &bad, &[], &[]).is_err(), "{} admitted an altered result", prim.name());
        }
    }
    #[test]
    fn history_wires_are_prior_rows_then_current_and_fixed_state_saturates() {
        let state = StateDecl {
            name: "history".into(),
            dtype: DType::I8,
            shape: vec![2],
            per_layer: false,
            kind: StateKind::Hist { window: 3 },
        };
        let current = tensor(DType::I8, &[2], &[4, 5]);
        let prior = [tensor(DType::I8, &[2], &[1, 2]), tensor(DType::I8, &[2], &[-3, 0])];
        let expected = tensor(DType::I8, &[3, 2], &[1, 2, -3, 0, 4, 5]);
        relation(&Prim::HistAppend { state: 0 }, &[current], &expected, &[state], &prior).unwrap();
        let state = StateDecl {
            name: "fixed".into(),
            dtype: DType::I8,
            shape: vec![3],
            per_layer: false,
            kind: StateKind::Fixed { lo: -2, hi: 3 },
        };
        relation(
            &Prim::StateWrite { state: 0 },
            &[tensor(DType::I32, &[3], &[-10, 2, 99])],
            &tensor(DType::I8, &[3], &[-2, 2, 3]),
            &[state],
            &[],
        )
        .unwrap();
    }
    #[test]
    fn native_relation_binds_the_same_input_and_output_wires_to_existing_roots() {
        use super::super::native_commitment::NativeTensorCommitmentV5;
        let x = tensor(DType::I8, &[], &[9]);
        let y = tensor(DType::I8, &[], &[-4]);
        let z = tensor(DType::I8, &[], &[5]);
        let endpoint = |t: &Tensor| NativeTensorEndpointV5 {
            dtype: t.dtype,
            shape: &[] as &[usize],
            suite: NativeTensorCommitmentV5::DualRootV2,
            root: crate::merkle::tensor_commitment_v2(t),
        };
        let memory = CircuitMemoryBudgetV5::new(600_000_000);
        let graph =
            compile_native_primitive_v5(&Prim::Add, &[endpoint(&x), endpoint(&y)], endpoint(&z), &[], &[], limits(), memory.clone())
                .unwrap();
        graph.evaluate(&input_bits_v5(&[9, (-4i8) as u8]), limits()).unwrap();
        assert!(graph.evaluate(&input_bits_v5(&[8, (-4i8) as u8]), limits()).is_err());
        drop(graph);
        assert_eq!(memory.held(), 0);
    }
}
