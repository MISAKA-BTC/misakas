//! **The engine's op sequence, compiled from `BASE0_LAYER_IR`** (ADR-0049 Decision F).
//!
//! Decision F asks for four projections of one description: the shape profile, the adjudicator's
//! node table, the artifact inventory, and *the engine's op sequence*. Three of them were already
//! generated. The engine was not — it was written by hand and the IR transcribed from it, which
//! the profile's own doc comment said out loud: "the engine is still written by hand, so nothing
//! structural stops the two describing different computations — which they did, four times over,
//! and each divergence was found by someone reading rather than by anything failing."
//!
//! The interim guard that stood in for a generator compared the SHAPE of an execution: the slot
//! order, and each row's length against the width the graph declares. That catches a step the
//! engine performs and the graph omits. It cannot catch:
//!
//! * **the wrong operand** — a narrowing that reads `attn_output.requant` where the graph names
//!   `code_product.requant` produces a row of exactly the right length;
//! * **the wrong kernel** — `Rescale` where the graph declares `Requantize`, same width;
//! * **the wrong input** — an FFN up-projection reading the norm's accumulator instead of its
//!   narrowing, same width again.
//!
//! Each of those is a court that recomputes an honest step from the graph, gets a different
//! answer, and convicts the producer. So the sequence is not compared to the IR here; it is *made
//! from* it. There is one list of steps in this crate and consensus-core holds it.
//!
//! # What is generated and what is not
//!
//! The kernels are not: `rms_norm`, `matmul_quant`, `softmax` and the rest stay exactly the
//! arithmetic they were, in `palw_base0_ops`, because that arithmetic is the class. What is
//! generated is everything the divergences were actually made of — which step runs, in what order,
//! reading which earlier step, against which operand of the artifact, producing how many values.
//!
//! Three properties fall out of the IR rather than being declared beside it:
//!
//! * **a step's output type.** A narrowing emits `int8` codes; every other kernel emits `i32`
//!   accumulators. So "did this step produce codes" is `kernel == Requantize`, and a kernel that
//!   requires codes reading an accumulator is refused at compile time rather than silently
//!   truncating — which is the "a court would compare a Qk value against a code" defect, made
//!   unrepresentable;
//! * **where the cache is written**, from the node roles: the layer's `KCacheWrite` and
//!   `VCacheWrite` steps, committed together immediately before the first step that reads the
//!   cache, so a failure between them cannot leave the two halves at different lengths;
//! * **the diagnostics' sites** — which step is the softmax whose spread says attention is
//!   selecting something, which is the gate whose asymmetry says SiLU is not degenerate. They used
//!   to be wherever the hand-written loop happened to be standing.

use kaspa_consensus_core::palw_artifact::PalwArtifactInventoryV1;
use kaspa_consensus_core::palw_base0_ops::{
    add_elem, dot_i8, matmul_quant, mul_elem, requantize_row, requantize_row_uniform, rescale_row, rms_norm, rope_table, silu, softmax,
};
use kaspa_consensus_core::palw_base0_profile::{Base0IrInputV1, Base0IrNodeV1, Base0IrWidthV1, BASE0_LAYER_IR};
use kaspa_consensus_core::palw_step::{
    kernel_semantics_id_v1, PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepOpKindV1, PalwStepOutLenV1, PALW_STEP_INPUT_KV_K,
    PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN,
};
use kaspa_consensus_core::palw_step_refute::{
    KDESC_BASE0_ADD_ELEM, KDESC_BASE0_MATMUL, KDESC_BASE0_MUL_ELEM, KDESC_BASE0_REQUANTIZE, KDESC_BASE0_RESCALE, KDESC_BASE0_RMS_NORM,
    KDESC_BASE0_ROPE, KDESC_BASE0_SILU, KDESC_BASE0_SOFTMAX,
};

use crate::artifact::Base0ArtifactV1;
use crate::operands::{base0_resolve_operand_v1, Base0OperandV1, Base0QuantOperandV1, OperandError};

/// The engine's own head tensor.
///
/// A class with tied embeddings names `token_embd.weight` in its INVENTORY, because that is the
/// tensor a court opens; the container carries the head separately either way
/// (`Base0ArtifactV1::unembed`, equal bytes when tied), so the engine reads one name.
pub const BASE0_ENGINE_HEAD_TENSOR: &str = "output.weight";

/// Why a graph cannot be executed.
///
/// Every variant is a class this binary must refuse to run rather than run differently: a step it
/// cannot reproduce is a step the court cannot adjudicate, and computing something anyway is how a
/// producer commits to arithmetic nobody can check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    Operand(OperandError),
    /// A kernel descriptor this binary has no dispatch for.
    UnknownKernel {
        slot: u16,
        kernel: &'static str,
    },
    /// A step reading a step at or after itself — an order that cannot be executed at all.
    ForwardReference {
        slot: u16,
        input: u16,
    },
    /// The wrong number of inputs for the kernel.
    Arity {
        slot: u16,
        kernel: &'static str,
        got: usize,
    },
    /// A kernel that consumes `int8` codes reading a step that produces accumulators.
    NotCodes {
        slot: u16,
        kernel: &'static str,
        input: u16,
    },
    /// The cache is read before it is written, written more than once, or only half written.
    CacheOrder {
        detail: &'static str,
    },
    /// The layer's last step does not produce the codes the next layer reads.
    LayerOutputNotCodes {
        slot: u16,
    },
    /// A width the plan cannot size — a `KvScaled` multiplier of zero, or an empty table.
    NoSteps,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operand(e) => write!(f, "operand: {e}"),
            Self::UnknownKernel { slot, kernel } => write!(f, "slot {slot}: no dispatch for {kernel}"),
            Self::ForwardReference { slot, input } => write!(f, "slot {slot} reads slot {input}"),
            Self::Arity { slot, kernel, got } => write!(f, "slot {slot} ({kernel}): {got} inputs"),
            Self::NotCodes { slot, kernel, input } => write!(f, "slot {slot} ({kernel}) reads accumulators at slot {input}"),
            Self::CacheOrder { detail } => write!(f, "cache: {detail}"),
            Self::LayerOutputNotCodes { slot } => write!(f, "the layer ends at slot {slot}, which is not a narrowing"),
            Self::NoSteps => write!(f, "the graph has no steps"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<OperandError> for PlanError {
    fn from(e: OperandError) -> Self {
        Self::Operand(e)
    }
}

/// One row of an execution: `int8` codes or `i32` accumulators, never both.
///
/// The distinction is the class's own — `Requantize` is the only op that produces codes, and every
/// op that consumes codes is defined on `int8`. Carrying it in the type is what makes "compare a
/// Qk value against a code" a compile-time refusal instead of an `as i8` nobody notices.
#[derive(Clone, Debug)]
pub enum Base0RowV1 {
    Codes(Vec<i8>),
    Acc(Vec<i32>),
}

impl Base0RowV1 {
    pub fn len(&self) -> usize {
        match self {
            Self::Codes(v) => v.len(),
            Self::Acc(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The `i32` lanes a step leaf carries — which is the form the probe records, for both kinds.
    pub fn to_lanes(&self) -> Vec<i32> {
        match self {
            Self::Codes(v) => v.iter().map(|c| *c as i32).collect(),
            Self::Acc(v) => v.clone(),
        }
    }

    fn codes(&self) -> Option<&[i8]> {
        match self {
            Self::Codes(v) => Some(v),
            Self::Acc(_) => None,
        }
    }

    /// Accumulator lanes, widening codes when a kernel is defined on `i32` and reads a narrowing —
    /// which the rotation does, and is the only widening in the graph.
    fn lanes(&self) -> std::borrow::Cow<'_, [i32]> {
        match self {
            Self::Codes(v) => std::borrow::Cow::Owned(v.iter().map(|c| *c as i32).collect()),
            Self::Acc(v) => std::borrow::Cow::Borrowed(v),
        }
    }
}

/// One compiled step.
#[derive(Clone, Copy, Debug)]
pub struct Base0PlanNodeV1 {
    pub slot: u16,
    pub op: PalwStepOpKindV1,
    pub role: PalwStepNodeRoleV1,
    pub kernel: &'static str,
    /// The IR's tensor TEMPLATE, empty for a step with no registered operand.
    pub weight: &'static str,
    pub out: Base0IrWidthV1,
    pub inputs: &'static [Base0IrInputV1],
    /// Derived: a narrowing produces codes, everything else produces accumulators.
    pub emits_codes: bool,
    /// Derived: this step reads the K or V cache, so any staged write must be committed first.
    pub reads_cache: bool,
}

/// **The layer graph, compiled against one artifact.**
///
/// Compilation is where a class is refused: every tensor the graph names must resolve, every input
/// must be an earlier step, every kernel must have a dispatch, and the caches must be written
/// before they are read. A graph that passes can be executed; one that does not is not executed
/// differently, it is not executed.
#[derive(Clone, Debug)]
pub struct Base0PlanV1 {
    nodes: Vec<Base0PlanNodeV1>,
    /// The single softmax step, whose per-head spread says whether attention selects anything.
    softmax_slot: Option<usize>,
    /// The narrowing that consumes the SiLU — the gate whose asymmetry says SiLU is not linear.
    gate_slot: Option<usize>,
    k_write: Option<usize>,
    v_write: Option<usize>,
}

impl Base0PlanV1 {
    /// Compile [`BASE0_LAYER_IR`] against `artifact`.
    pub fn compile(artifact: &Base0ArtifactV1) -> Result<Self, PlanError> {
        Self::compile_table(BASE0_LAYER_IR, artifact)
    }

    pub fn compile_table(table: &'static [Base0IrNodeV1], artifact: &Base0ArtifactV1) -> Result<Self, PlanError> {
        if table.is_empty() {
            return Err(PlanError::NoSteps);
        }
        let mut nodes: Vec<Base0PlanNodeV1> = Vec::with_capacity(table.len());
        for (i, ir) in table.iter().enumerate() {
            let slot = i as u16;
            let emits_codes = ir.kernel == KDESC_BASE0_REQUANTIZE;
            let reads_cache = ir.inputs.iter().any(|r| matches!(r, Base0IrInputV1::CachedK | Base0IrInputV1::CachedV));
            for r in ir.inputs {
                if let Base0IrInputV1::Step(k) = r {
                    if *k >= slot {
                        return Err(PlanError::ForwardReference { slot, input: *k });
                    }
                }
            }
            // Every operand the step names must resolve at every layer. Asked here, once per
            // class, rather than per forward pass: a name that resolves at layer 0 and not at
            // layer 7 is a class that would fail in the middle of an execution.
            if !ir.weight.is_empty() {
                for li in 0..artifact.shape.n_layers {
                    base0_resolve_operand_v1(artifact, ir.weight, Some(li), BASE0_ENGINE_HEAD_TENSOR)?;
                }
            }
            let arity = |want: usize| -> Result<(), PlanError> {
                if ir.inputs.len() == want {
                    Ok(())
                } else {
                    Err(PlanError::Arity { slot, kernel: ir.kernel, got: ir.inputs.len() })
                }
            };
            match ir.kernel {
                KDESC_BASE0_RMS_NORM
                | KDESC_BASE0_REQUANTIZE
                | KDESC_BASE0_RESCALE
                | KDESC_BASE0_SOFTMAX
                | KDESC_BASE0_SILU
                | KDESC_BASE0_ROPE => arity(1)?,
                KDESC_BASE0_MUL_ELEM | KDESC_BASE0_ADD_ELEM => arity(2)?,
                KDESC_BASE0_MATMUL => arity(if ir.weight.is_empty() { 2 } else { 1 })?,
                other => return Err(PlanError::UnknownKernel { slot, kernel: other }),
            }
            // Which kernels are defined on codes. A step feeding one an accumulator is the
            // scale-confusion defect, refused before it can be committed to.
            let needs_codes: &[usize] = match ir.kernel {
                KDESC_BASE0_RMS_NORM => &[0],
                KDESC_BASE0_MUL_ELEM | KDESC_BASE0_ADD_ELEM => &[0, 1],
                KDESC_BASE0_MATMUL => &[0],
                _ => &[],
            };
            for idx in needs_codes {
                if let Some(Base0IrInputV1::Step(k)) = ir.inputs.get(*idx) {
                    if !nodes[*k as usize].emits_codes {
                        return Err(PlanError::NotCodes { slot, kernel: ir.kernel, input: *k });
                    }
                }
            }
            nodes.push(Base0PlanNodeV1 {
                slot,
                op: ir.op,
                role: ir.role,
                kernel: ir.kernel,
                weight: ir.weight,
                out: ir.out,
                inputs: ir.inputs,
                emits_codes,
                reads_cache,
            });
        }

        let single = |role: PalwStepNodeRoleV1| -> Result<Option<usize>, PlanError> {
            let found: Vec<usize> = nodes.iter().filter(|n| n.role == role).map(|n| n.slot as usize).collect();
            match found.len() {
                0 => Ok(None),
                1 => Ok(Some(found[0])),
                _ => Err(PlanError::CacheOrder { detail: "a layer writes one cache more than once" }),
            }
        };
        let k_write = single(PalwStepNodeRoleV1::KCacheWrite)?;
        let v_write = single(PalwStepNodeRoleV1::VCacheWrite)?;
        let first_read = nodes.iter().find(|n| n.reads_cache).map(|n| n.slot as usize);
        match (k_write, v_write) {
            (Some(k), Some(v)) => {
                if !nodes[k].emits_codes || !nodes[v].emits_codes {
                    return Err(PlanError::CacheOrder { detail: "a cache write must be a narrowing" });
                }
                if let Some(r) = first_read {
                    if r <= k.max(v) {
                        return Err(PlanError::CacheOrder { detail: "the cache is read before it is written" });
                    }
                }
            }
            (None, None) => {
                if first_read.is_some() {
                    return Err(PlanError::CacheOrder { detail: "the cache is read and never written" });
                }
            }
            _ => return Err(PlanError::CacheOrder { detail: "one half of the cache is written and the other is not" }),
        }

        let last = nodes.last().expect("the table is not empty");
        if !last.emits_codes {
            return Err(PlanError::LayerOutputNotCodes { slot: last.slot });
        }

        let softmax_slot = nodes.iter().find(|n| n.kernel == KDESC_BASE0_SOFTMAX).map(|n| n.slot as usize);
        let silu_slot = nodes.iter().find(|n| n.kernel == KDESC_BASE0_SILU).map(|n| n.slot as usize);
        let gate_slot = silu_slot.and_then(|s| {
            nodes.iter().find(|n| n.emits_codes && n.inputs.contains(&Base0IrInputV1::Step(s as u16))).map(|n| n.slot as usize)
        });

        Ok(Self { nodes, softmax_slot, gate_slot, k_write, v_write })
    }

    pub fn nodes(&self) -> &[Base0PlanNodeV1] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn softmax_slot(&self) -> Option<usize> {
        self.softmax_slot
    }

    pub fn gate_slot(&self) -> Option<usize> {
        self.gate_slot
    }

    /// How many values a step produces at this position — the width the graph declares, computed
    /// from the same `Base0IrWidthV1` the profile's node table is projected through.
    pub fn out_elements(&self, slot: usize, shape: &crate::artifact::Base0ShapeV1, kv_len: usize) -> usize {
        base0_ir_width_elements_v1(self.nodes[slot].out, shape, kv_len)
    }
}

/// The one place a width becomes a count. `base0_ir_attn_nodes_v1` projects the same match into
/// the graph the court reads, which is why the two can never disagree about how long a row is.
pub fn base0_ir_width_elements_v1(width: Base0IrWidthV1, shape: &crate::artifact::Base0ShapeV1, kv_len: usize) -> usize {
    match width {
        Base0IrWidthV1::Hidden => shape.d_model(),
        Base0IrWidthV1::KvDim => shape.kv_dim(),
        Base0IrWidthV1::FfnDim => shape.d_ff,
        Base0IrWidthV1::HeadDim => shape.d_head,
        Base0IrWidthV1::KvScaled(m) => m as usize * kv_len,
        Base0IrWidthV1::KvPerHead => shape.n_heads * kv_len,
    }
}

/// What one layer's execution reports back beyond its output row.
pub struct Base0LayerTraceV1 {
    /// One row per step, in slot order — what the probe records and a step leaf tiles.
    pub rows: Vec<Vec<i32>>,
    /// `max − min` of the softmax distribution, per query head.
    pub attention_spread: Vec<i32>,
    /// `(most negative, most positive)` code out of the SiLU gate.
    pub gate_extremes: Option<(i32, i32)>,
}

impl Base0PlanV1 {
    /// **Execute one layer by walking the plan.**
    ///
    /// `layer_in` is the residual stream entering the layer; the return is the stream leaving it,
    /// which is the last step's output because the graph says so rather than because this function
    /// decides it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_layer(
        &self,
        artifact: &Base0ArtifactV1,
        layer: usize,
        layer_in: &[i8],
        cache: &mut crate::engine::KvCache,
        position: usize,
    ) -> Result<(Vec<i8>, Base0LayerTraceV1), crate::engine::EngineError> {
        let shape = &artifact.shape;
        let kv_len = position + 1;
        let mut rows: Vec<Base0RowV1> = Vec::with_capacity(self.nodes.len());
        let mut trace =
            Base0LayerTraceV1 { rows: Vec::with_capacity(self.nodes.len()), attention_spread: Vec::new(), gate_extremes: None };
        let mut staged_k: Option<Vec<i8>> = None;
        let mut staged_v: Option<Vec<i8>> = None;
        let layer_in_row = Base0RowV1::Codes(layer_in.to_vec());

        for node in &self.nodes {
            // The two halves are committed together, immediately before the first step that reads
            // them: a write per step would let a failure in between leave the caches at different
            // lengths, which silently changes every LATER position's attention.
            if node.reads_cache {
                if let (Some(k), Some(v)) = (staged_k.take(), staged_v.take()) {
                    cache.push_layer(layer, k, v);
                }
            }
            let out = self.compute(artifact, node, layer, &layer_in_row, &rows, cache, position, kv_len)?;
            debug_assert_eq!(
                out.len(),
                base0_ir_width_elements_v1(node.out, shape, kv_len),
                "slot {} produced a row the graph does not declare",
                node.slot
            );
            if self.softmax_slot == Some(node.slot as usize) {
                let lanes = out.lanes();
                let per_head = out.len() / shape.n_heads.max(1);
                for h in 0..shape.n_heads {
                    let seg = &lanes[h * per_head..(h + 1) * per_head];
                    trace.attention_spread.push(seg.iter().max().copied().unwrap_or(0) - seg.iter().min().copied().unwrap_or(0));
                }
            }
            if self.gate_slot == Some(node.slot as usize) {
                let lanes = out.to_lanes();
                trace.gate_extremes = Some((lanes.iter().copied().min().unwrap_or(0), lanes.iter().copied().max().unwrap_or(0)));
            }
            if Some(node.slot as usize) == self.k_write {
                staged_k = out.codes().map(|c| c.to_vec());
            }
            if Some(node.slot as usize) == self.v_write {
                staged_v = out.codes().map(|c| c.to_vec());
            }
            trace.rows.push(out.to_lanes());
            rows.push(out);
        }
        // A graph that never reads its own cache still fills it, for the positions after this one.
        if let (Some(k), Some(v)) = (staged_k.take(), staged_v.take()) {
            cache.push_layer(layer, k, v);
        }

        let out = match rows.pop() {
            Some(Base0RowV1::Codes(v)) => v,
            _ => return Err(crate::engine::EngineError::Plan(PlanError::LayerOutputNotCodes { slot: (self.nodes.len() - 1) as u16 })),
        };
        Ok((out, trace))
    }

    #[allow(clippy::too_many_arguments)]
    fn compute(
        &self,
        artifact: &Base0ArtifactV1,
        node: &Base0PlanNodeV1,
        layer: usize,
        layer_in_row: &Base0RowV1,
        rows: &[Base0RowV1],
        cache: &crate::engine::KvCache,
        position: usize,
        kv_len: usize,
    ) -> Result<Base0RowV1, crate::engine::EngineError> {
        use crate::engine::EngineError;
        let shape = &artifact.shape;
        let d_head = shape.d_head;
        let out_elements = base0_ir_width_elements_v1(node.out, shape, kv_len);

        // A step's inputs, resolved the way the graph names them.
        let input = |idx: usize| -> Option<&Base0RowV1> {
            match node.inputs.get(idx)? {
                Base0IrInputV1::Step(k) => rows.get(*k as usize),
                Base0IrInputV1::LayerIn => None,
                Base0IrInputV1::CachedK | Base0IrInputV1::CachedV => None,
            }
        };
        let operand = |name: &'static str| -> Result<Base0OperandV1<'_>, EngineError> {
            let scope = if name.starts_with(crate::operands::BASE0_LAYER_PREFIX) { Some(layer) } else { None };
            base0_resolve_operand_v1(artifact, name, scope, BASE0_ENGINE_HEAD_TENSOR)
                .map_err(|e| EngineError::Plan(PlanError::Operand(e)))
        };
        // `LayerIn` is a row like any other, so a kernel does not need to know which of its inputs
        // it is.
        let at = |idx: usize| -> Result<&Base0RowV1, EngineError> {
            match node.inputs.get(idx) {
                Some(Base0IrInputV1::LayerIn) => Ok(layer_in_row),
                Some(Base0IrInputV1::Step(_)) => {
                    input(idx).ok_or(EngineError::Plan(PlanError::ForwardReference { slot: node.slot, input: idx as u16 }))
                }
                _ => Err(EngineError::Plan(PlanError::Arity { slot: node.slot, kernel: node.kernel, got: node.inputs.len() })),
            }
        };
        let codes = |row: &Base0RowV1, idx: usize| -> Result<Vec<i8>, EngineError> {
            row.codes().map(|c| c.to_vec()).ok_or(EngineError::Plan(PlanError::NotCodes {
                slot: node.slot,
                kernel: node.kernel,
                input: idx as u16,
            }))
        };

        let row = match node.kernel {
            KDESC_BASE0_RMS_NORM => {
                let x = codes(at(0)?, 0)?;
                Base0RowV1::Acc(rms_norm(&x, shape.eps_q)?)
            }
            KDESC_BASE0_REQUANTIZE => {
                let acc = at(0)?.lanes().into_owned();
                match operand(node.weight)? {
                    Base0OperandV1::Quant(Base0QuantOperandV1::Uniform(q)) => Base0RowV1::Codes(requantize_row_uniform(&acc, q)),
                    Base0OperandV1::Quant(Base0QuantOperandV1::PerChannel(ch)) => Base0RowV1::Codes(requantize_row(&acc, ch)?),
                    _ => return Err(EngineError::Plan(PlanError::UnknownKernel { slot: node.slot, kernel: node.kernel })),
                }
            }
            KDESC_BASE0_RESCALE => {
                let acc = at(0)?.lanes().into_owned();
                match operand(node.weight)? {
                    Base0OperandV1::Scale(s) => Base0RowV1::Acc(rescale_row(&acc, s)),
                    _ => return Err(EngineError::Plan(PlanError::UnknownKernel { slot: node.slot, kernel: node.kernel })),
                }
            }
            KDESC_BASE0_SILU => Base0RowV1::Acc(silu(&at(0)?.lanes())),
            KDESC_BASE0_SOFTMAX => {
                // Per QUERY HEAD, because the row is `attn_heads` distributions concatenated —
                // which is what the `KvPerHead` width means. A softmax over the concatenation
                // would normalise every head against every other one.
                let acc = at(0)?.lanes().into_owned();
                let segments = match node.out {
                    Base0IrWidthV1::KvPerHead => shape.n_heads.max(1),
                    _ => 1,
                };
                let width = acc.len() / segments;
                let mut out = Vec::with_capacity(acc.len());
                for s in 0..segments {
                    out.extend(softmax(&acc[s * width..(s + 1) * width])?);
                }
                Base0RowV1::Acc(out)
            }
            KDESC_BASE0_MUL_ELEM => {
                let a = codes(at(0)?, 0)?;
                let b = codes(at(1)?, 1)?;
                Base0RowV1::Acc(mul_elem(&a, &b)?)
            }
            KDESC_BASE0_ADD_ELEM => {
                let a = codes(at(0)?, 0)?;
                let b = codes(at(1)?, 1)?;
                Base0RowV1::Acc(add_elem(&a, &b)?)
            }
            KDESC_BASE0_ROPE => {
                // One rotation per head, over as many heads as the declared width holds: `Hidden`
                // is the query projection and `KvDim` the key projection, which under grouped-query
                // attention is fewer heads. Rotating `n_heads` of a `kv_dim` row would read past
                // the projection.
                let Base0OperandV1::Rope(table) = operand(node.weight)? else {
                    return Err(EngineError::Plan(PlanError::UnknownKernel { slot: node.slot, kernel: node.kernel }));
                };
                let (cos_row, sin_row) =
                    table.row(position).ok_or(EngineError::PositionOutOfRange { got: position, max: shape.max_position })?;
                let src = at(0)?.lanes().into_owned();
                let heads = out_elements / d_head.max(1);
                let mut out = Vec::with_capacity(out_elements);
                for h in 0..heads {
                    out.extend(rope_table(&src[h * d_head..(h + 1) * d_head], cos_row, sin_row)?);
                }
                Base0RowV1::Acc(out)
            }
            KDESC_BASE0_MATMUL => {
                if node.weight.is_empty() {
                    // Attention. The second operand is the cache rather than a registered tensor,
                    // and which cache decides the reduction: over KEYS it is one dot per history
                    // position per query head, over VALUES one dot per head lane.
                    let group = shape.gqa_group().max(1);
                    let history = cache.layer_len(layer);
                    let mut out = Vec::with_capacity(out_elements);
                    match node.inputs.get(1) {
                        Some(Base0IrInputV1::CachedK) => {
                            let q = codes(at(0)?, 0)?;
                            for h in 0..shape.n_heads {
                                let off = h * d_head;
                                let kv_off = (h / group) * d_head;
                                for j in 0..history {
                                    let key = cache.key_at(layer, j, kv_off, d_head);
                                    out.push(dot_i8(&q[off..off + d_head], key)?);
                                }
                            }
                        }
                        Some(Base0IrInputV1::CachedV) => {
                            let p8 = codes(at(0)?, 0)?;
                            let width = p8.len() / shape.n_heads.max(1);
                            for h in 0..shape.n_heads {
                                let seg = &p8[h * width..(h + 1) * width];
                                let kv_off = (h / group) * d_head;
                                for i in 0..d_head {
                                    let column: Vec<i8> = (0..history).map(|j| cache.value_at(layer, j, kv_off + i)).collect();
                                    out.push(dot_i8(seg, &column)?);
                                }
                            }
                        }
                        _ => {
                            return Err(EngineError::Plan(PlanError::Arity {
                                slot: node.slot,
                                kernel: node.kernel,
                                got: node.inputs.len(),
                            }));
                        }
                    }
                    Base0RowV1::Acc(out)
                } else {
                    let x = codes(at(0)?, 0)?;
                    match operand(node.weight)? {
                        Base0OperandV1::Matrix { data, .. } => Base0RowV1::Acc(matmul_quant(data, &x, out_elements)?),
                        _ => return Err(EngineError::Plan(PlanError::UnknownKernel { slot: node.slot, kernel: node.kernel })),
                    }
                }
            }
            other => return Err(EngineError::Plan(PlanError::UnknownKernel { slot: node.slot, kernel: other })),
        };
        Ok(row)
    }
}

/// Where two projections of one description stop agreeing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionMismatch {
    /// The graph and the engine perform a different number of steps.
    NodeCount { profile: usize, plan: usize },
    /// One step disagrees in one field — the field is named because "the engine and the graph
    /// disagree" is not something anyone can act on.
    Node { slot: u16, field: &'static str },
    /// The graph tiles one table at two lengths, so "tile 3" addresses two different things.
    TileLen { slot: u16 },
    /// A tensor the graph reads that the inventory does not carry — the step reading it opens
    /// nothing, which is `Unadjudicable` rather than innocent.
    TensorNotCarried { tensor: String },
    /// A tensor the inventory carries that no step reads. Not unsafe, but every such row lengthens
    /// the Merkle path of every opening that IS made, for a leaf nobody can ask for.
    RowNobodyOpens { tensor: String },
}

impl std::fmt::Display for ProjectionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeCount { profile, plan } => write!(f, "the graph declares {profile} steps, the engine performs {plan}"),
            Self::Node { slot, field } => write!(f, "slot {slot}: {field}"),
            Self::TileLen { slot } => write!(f, "slot {slot} tiles at a different length than the table"),
            Self::TensorNotCarried { tensor } => write!(f, "{tensor} is read and not carried"),
            Self::RowNobodyOpens { tensor } => write!(f, "{tensor} is carried and never read"),
        }
    }
}

impl std::error::Error for ProjectionMismatch {}

/// **The golden check: the four projections agree — node for node, tile for tile, tensor for
/// tensor** (ADR-0049 Decision F).
///
/// The four are one description seen from four sides: `BASE0_LAYER_IR` itself, the shape profile
/// the court adjudicates against (`profile.attn_nodes`, projected by `base0_ir_attn_nodes_v1`), the
/// engine's op sequence (this plan, compiled from the same table), and the artifact inventory an
/// opening addresses. Since all four are now generated, this cannot fail for a class built the
/// normal way — which is the point of running it anyway: it is the assertion that says so, and it
/// bites the moment anything reintroduces a second description.
///
/// `kv_len` is the position the widths are compared at. One position cannot tell a per-head width
/// from a per-layer one — both are `kv_len` at one — so a caller checking properly checks two.
pub fn base0_check_projections_v1(
    plan: &Base0PlanV1,
    profile: &PalwShapeProfileV3,
    inventory: &PalwArtifactInventoryV1,
    shape: &crate::artifact::Base0ShapeV1,
    kv_len: usize,
) -> Result<(), ProjectionMismatch> {
    let declared = &profile.attn_nodes;
    if declared.len() != plan.len() {
        return Err(ProjectionMismatch::NodeCount { profile: declared.len(), plan: plan.len() });
    }
    let tile_len = declared.first().map(|n| n.tile_len).unwrap_or(0);
    for (node, want) in plan.nodes().iter().zip(declared.iter()) {
        let bad = |field: &'static str| ProjectionMismatch::Node { slot: node.slot, field };
        if want.tile_len != tile_len {
            return Err(ProjectionMismatch::TileLen { slot: node.slot });
        }
        if want.op_kind != node.op {
            return Err(bad("op kind"));
        }
        if want.role != node.role {
            return Err(bad("cache role"));
        }
        if want.kernel_semantics_id != kernel_semantics_id_v1(node.kernel) {
            return Err(bad("kernel"));
        }
        if want.weight_name != node.weight {
            return Err(bad("operand"));
        }
        let want_elements = match want.out_len {
            PalwStepOutLenV1::Fixed { elements } => elements as usize,
            PalwStepOutLenV1::KvScaled { multiplier } => multiplier as usize * kv_len,
        };
        if want_elements != base0_ir_width_elements_v1(node.out, shape, kv_len) {
            return Err(bad("output width"));
        }
        let refs: Vec<u16> = node
            .inputs
            .iter()
            .map(|r| match r {
                Base0IrInputV1::Step(k) => *k,
                Base0IrInputV1::LayerIn => PALW_STEP_INPUT_LAYER_IN,
                Base0IrInputV1::CachedK => PALW_STEP_INPUT_KV_K,
                Base0IrInputV1::CachedV => PALW_STEP_INPUT_KV_V,
            })
            .collect();
        if want.input_refs != refs {
            return Err(bad("inputs"));
        }
    }

    // Tensor for tensor, both directions. `verify_covers_profile` already asks the first — every
    // tensor the graph reads is carried — over every table; this asks the second, which nothing
    // did: a row the inventory carries that no step in the graph can open.
    inventory.verify_covers_profile(profile).map_err(|e| ProjectionMismatch::TensorNotCarried { tensor: format!("{e:?}") })?;
    let read: Vec<&str> = [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flat_map(|t| t.iter())
        .filter(|n| !n.weight_name.is_empty())
        .map(|n| n.weight_name.as_str())
        .collect();
    for row in inventory.operands() {
        if !read.contains(&row.tensor_name.as_str()) {
            return Err(ProjectionMismatch::RowNobodyOpens { tensor: row.tensor_name.clone() });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use crate::inventory::base0_inventory_v1;
    use kaspa_consensus_core::palw_base0_profile::{base0_profile_v1, PALW_RC_BASE0_GEOMETRY};

    fn rc_shape() -> Base0ShapeV1 {
        let g = PALW_RC_BASE0_GEOMETRY;
        Base0ShapeV1 {
            n_layers: g.layer_count as usize,
            n_heads: g.attn_heads as usize,
            n_kv_heads: g.attn_heads as usize,
            d_head: g.attn_head_dim as usize,
            d_ff: g.ffn_dim as usize,
            vocab: g.vocab_size as usize,
            max_position: g.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: g.rms_eps_q,
        }
    }

    fn rc_artifact() -> Base0ArtifactV1 {
        Base0ArtifactV1::derive_deterministic(rc_shape(), 20_260_821).expect("the floor derives")
    }

    /// **ADR-0049 Decision F, asserted: the four projections agree — node for node, tile for tile,
    /// tensor for tensor.**
    ///
    /// The engine's op sequence, the court's node table, the artifact inventory and the IR are one
    /// description now, so this passes by construction. That is what makes it worth running: it is
    /// the statement that they are one, and it fails by name the moment a second description of any
    /// part of this computation appears.
    ///
    /// Checked at TWO positions, because every `KvScaled` width is a function of `kv_len` and a
    /// single position cannot tell a per-head width from a per-layer one — both are `kv_len` at one,
    /// which is exactly how the per-head attention nodes came to be declared once per layer.
    #[test]
    fn the_four_projections_agree_and_a_real_execution_agrees_with_them() {
        let artifact = rc_artifact();
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
        let inventory = base0_inventory_v1(&artifact, PALW_RC_BASE0_GEOMETRY).expect("the floor's inventory");
        let plan = Base0PlanV1::compile(&artifact).expect("the floor's graph is executable");

        for kv_len in 1..=2 {
            base0_check_projections_v1(&plan, &profile, &inventory, &artifact.shape, kv_len)
                .unwrap_or_else(|e| panic!("at kv_len {kv_len}: {e}"));
        }

        // And the rows an execution actually produces are those widths. The projections agreeing
        // with each other is a statement about tables; this is the one that says the arithmetic
        // that ran is the arithmetic the court will recompute.
        let engine = crate::engine::Base0Engine::new(&artifact);
        let mut cache = crate::engine::KvCache::new(&artifact);
        for position in 0..2usize {
            let (_, probe) = engine.forward_token_probed(&mut cache, 3, position).expect("the pass completes");
            let kv_len = position + 1;
            for (layer, slot, row) in &probe.steps {
                assert_eq!(
                    row.len(),
                    plan.out_elements(*slot as usize, &artifact.shape, kv_len),
                    "layer {layer} slot {slot} at kv_len {kv_len}"
                );
            }
            assert_eq!(
                probe.steps.len(),
                plan.len() * artifact.shape.n_layers,
                "every step of every layer is captured, at kv_len {kv_len}"
            );
        }
    }

    /// **The three divergences the width check could not see, each named.**
    ///
    /// The guard that stood in for a generator compared the slot order and each row's LENGTH. A
    /// narrowing reading the wrong parameters, a `Rescale` where a `Requantize` is declared, and a
    /// projection reading the wrong earlier step all produce rows of exactly the right length — and
    /// each of them is a court that recomputes an honest step and convicts the producer.
    ///
    /// Mutating the profile rather than the IR is deliberate: the profile is the side a REGISTERED
    /// class carries, so this is also the check that a registration whose graph does not describe
    /// this engine is refused rather than run.
    #[test]
    fn a_divergence_the_width_check_cannot_see_is_named() {
        let artifact = rc_artifact();
        let good = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
        let inventory = base0_inventory_v1(&artifact, PALW_RC_BASE0_GEOMETRY).expect("the floor's inventory");
        let plan = Base0PlanV1::compile(&artifact).expect("executable");
        let check = |p: &PalwShapeProfileV3| base0_check_projections_v1(&plan, p, &inventory, &artifact.shape, 2);
        check(&good).expect("the unmutated graph agrees");

        // The wrong operand: same op, same width, parameters the producer never applied.
        let mut wrong_operand = good.clone();
        wrong_operand.attn_nodes[17].weight_name = "blk.{layer}.attn_output.requant".to_string();
        assert_eq!(check(&wrong_operand), Err(ProjectionMismatch::Node { slot: 17, field: "operand" }));

        // The wrong kernel: `Rescale` and `Requantize` are both one input to one row.
        let mut wrong_kernel = good.clone();
        wrong_kernel.attn_nodes[21].kernel_semantics_id = wrong_kernel.attn_nodes[19].kernel_semantics_id;
        assert_eq!(check(&wrong_kernel), Err(ProjectionMismatch::Node { slot: 21, field: "kernel" }));

        // The wrong input: the FFN up-projection reading the norm's accumulator instead of its
        // narrowing. Same length, different arithmetic, and a court would convict on it.
        let mut wrong_input = good.clone();
        wrong_input.attn_nodes[29].input_refs = vec![23];
        assert_eq!(check(&wrong_input), Err(ProjectionMismatch::Node { slot: 29, field: "inputs" }));

        // The cache role on the wrong node — the divergence that made a court recompute attention
        // against unrotated keys, which convicts every honest producer.
        let mut wrong_role = good.clone();
        wrong_role.attn_nodes[11].role = PalwStepNodeRoleV1::Plain;
        assert_eq!(check(&wrong_role), Err(ProjectionMismatch::Node { slot: 11, field: "cache role" }));

        // A graph shorter than the engine — eighteen of thirty-eight steps is what this table
        // actually declared before it was projected.
        let mut short = good.clone();
        short.attn_nodes.truncate(18);
        assert_eq!(check(&short), Err(ProjectionMismatch::NodeCount { profile: 18, plan: plan.len() }));
    }

    // --- the compile-time refusals, exercised against graphs written to break them -------------

    const fn node(
        op: PalwStepOpKindV1,
        kernel: &'static str,
        weight: &'static str,
        inputs: &'static [Base0IrInputV1],
    ) -> Base0IrNodeV1 {
        Base0IrNodeV1 { op, role: PalwStepNodeRoleV1::Plain, kernel, weight, out: Base0IrWidthV1::Hidden, inputs }
    }

    static READS_ITSELF: &[Base0IrNodeV1] =
        &[node(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_norm.requant", &[Base0IrInputV1::Step(0)])];

    static FEEDS_AN_ACCUMULATOR_TO_A_CODE_KERNEL: &[Base0IrNodeV1] = &[
        node(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", &[Base0IrInputV1::LayerIn]),
        node(PalwStepOpKindV1::AddElem, KDESC_BASE0_ADD_ELEM, "", &[Base0IrInputV1::Step(0), Base0IrInputV1::LayerIn]),
    ];

    static READS_A_CACHE_IT_NEVER_WRITES: &[Base0IrNodeV1] = &[
        node(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", &[Base0IrInputV1::LayerIn]),
        node(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_norm.requant", &[Base0IrInputV1::Step(0)]),
        node(PalwStepOpKindV1::MatMulQuant, KDESC_BASE0_MATMUL, "", &[Base0IrInputV1::Step(1), Base0IrInputV1::CachedK]),
    ];

    static NAMES_A_TENSOR_NOBODY_CARRIES: &[Base0IrNodeV1] =
        &[node(PalwStepOpKindV1::MulElem, KDESC_BASE0_REQUANTIZE, "blk.{layer}.attn_norm.weight", &[Base0IrInputV1::LayerIn])];

    static ENDS_ON_AN_ACCUMULATOR: &[Base0IrNodeV1] =
        &[node(PalwStepOpKindV1::RmsNorm, KDESC_BASE0_RMS_NORM, "", &[Base0IrInputV1::LayerIn])];

    /// **A graph this binary cannot execute is refused, not executed differently.**
    ///
    /// Every one of these would previously have been a hand-written loop that computed *something*
    /// — a truncation, a zero row, a step reading whatever the variable happened to hold — and
    /// committed to it. Compiling the sequence is what turns each into a refusal with a name, and
    /// it happens once per class rather than in the middle of a forward pass.
    #[test]
    fn a_graph_that_cannot_be_executed_is_refused_by_name() {
        let artifact = rc_artifact();
        let refusal = |t| Base0PlanV1::compile_table(t, &artifact).err();

        assert_eq!(refusal(READS_ITSELF), Some(PlanError::ForwardReference { slot: 0, input: 0 }));
        assert_eq!(
            refusal(FEEDS_AN_ACCUMULATOR_TO_A_CODE_KERNEL),
            Some(PlanError::NotCodes { slot: 1, kernel: KDESC_BASE0_ADD_ELEM, input: 0 })
        );
        assert_eq!(
            refusal(READS_A_CACHE_IT_NEVER_WRITES),
            Some(PlanError::CacheOrder { detail: "the cache is read and never written" })
        );
        assert!(matches!(refusal(NAMES_A_TENSOR_NOBODY_CARRIES), Some(PlanError::Operand(_))));
        assert_eq!(refusal(ENDS_ON_AN_ACCUMULATOR), Some(PlanError::LayerOutputNotCodes { slot: 0 }));

        // And the one that must compile: the graph the class actually is.
        assert_eq!(Base0PlanV1::compile_table(BASE0_LAYER_IR, &artifact).expect("the floor compiles").len(), BASE0_LAYER_IR.len());
    }
}
