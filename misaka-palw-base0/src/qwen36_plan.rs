//! **The mmap (Qwen3.6) container's profile interpreter — ADR-0067's fourth clause.**
//!
//! [`Qwen36Engine::plan_from_profile`] compiles a registered [`PalwShapeProfileV3`] into a plan,
//! and [`Qwen36Engine::forward_token_planned`] walks it: one committed row per declared node, in
//! the declared order, from the declared operands. The dense (A16) family shipped this seam first
//! (`engine_a16::plan_from_profile`) and its differential caught a real defect before any claim
//! existed; this is the same construction for the hybrid family, against the `graph-v2` row —
//! the corrected tables (`qwen36_profile_v2`) are the graph an interpreter can follow, and the v1
//! tables are refusable here for their own measured reasons.
//!
//! # Where a fused name becomes bytes
//!
//! A declared node may stand for a computation over several stores — `ffn_gate_exps.routed` names
//! "the eight chosen experts' gate projections", not a tensor anyone stored. The mapping from each
//! fused name to the stores it reads is `RESOLUTION_V2` in
//! `tests/qwen36_profile_conformance.rs`, held there against a real artifact; the exec arms below
//! implement exactly those rules and say so in place. WHICH experts a `.routed` node reads is the
//! routing — resolved from the committed `RouterTopk` row, the row declared one step earlier, so a
//! court that disagrees about the selection convicts at that node rather than silently
//! adjudicating the wrong expert.
//!
//! # What the plan proves
//!
//! Construction is the admission check, and it is width-sound: every consumer's input width is
//! checked AT PLAN TIME against what its declared producer emits (the A16 fuzz gate's first find
//! was a gate-and-plan-accepted profile whose rewired refs fed a kv-width row to the q-rope and
//! panicked the head slicing — a width-sound plan makes that class unplannable). A refusal names
//! the node and the reason; an `Ok` means execution will emit exactly one row per declared node.

use kaspa_consensus_core::palw_base0_a16::{A16QuantParams, a16_add_elem, a16_requant, a16_rms_norm, a16_softmax_rows};
use kaspa_consensus_core::palw_base0_ops::silu;
use kaspa_consensus_core::palw_qwen36_ops::{
    Qwen36GdnParamsV1, q36_decay, q36_gate_apply, q36_gdn_step, q36_l2_norm, q36_moe_combine, q36_mul_wide, q36_rescale_row,
    q36_rms_norm_wide, q36_rope_partial, q36_router_topk, q36_sigmoid_gate, q36_ssm_conv,
};
use kaspa_consensus_core::palw_qwen36_profile::QWEN36_WEIGHT_DTYPE_I8;
use kaspa_consensus_core::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN, PALW_STEP_INPUT_SENTINEL_MIN, PalwShapeProfileV3,
    PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, kernel_semantics_id_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    KDESC_A16_ADD_ELEM, KDESC_A16_ATTN_SCORES, KDESC_A16_ATTN_VALUES, KDESC_A16_EMBED, KDESC_A16_MATMUL_RESCALE, KDESC_A16_REQUANTIZE,
    KDESC_A16_RMS_NORM, KDESC_A16_SOFTMAX, KDESC_Q36_DECAY, KDESC_Q36_GATE_APPLY, KDESC_Q36_GDN_STEP, KDESC_Q36_HEAD_RMS_NORM,
    KDESC_Q36_L2_NORM, KDESC_Q36_MATMUL_GROUPED, KDESC_Q36_MATMUL_GROUPED_WIDE, KDESC_Q36_MOE_COMBINE, KDESC_Q36_MUL_WIDE,
    KDESC_Q36_RESCALE_ROW, KDESC_Q36_RMS_NORM_WIDE, KDESC_Q36_ROPE_PARTIAL, KDESC_Q36_ROUTER_TOPK, KDESC_Q36_SIGMOID, KDESC_Q36_SILU,
    KDESC_Q36_SSM_CONV,
};

use crate::kernels::{a16_attn_scores_fast, a16_attn_values_fast};
use crate::qwen36::{Qwen36Cache, Qwen36Engine, Qwen36Error, Qwen36LayerKind, Qwen36ShapeV1};

/// Why the profile could not become a plan. Mirrors `A16PlanErrorV1`: every refusal is the
/// ADR-0067 kernel boundary — the class declared arithmetic this build does not serve — and the
/// error names the node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Qwen36PlanErrorV1 {
    /// This family's engines execute the integer lane only.
    NotAnIntegerLane,
    /// A profile field disagrees with the artifact this engine holds.
    GeometryMismatch { what: &'static str, profile: u64, artifact: u64 },
    /// A declared node this build cannot serve, and why.
    UnservedNode { table: &'static str, index: usize, reason: String },
    /// **ADR-0067 SA-1: the declaration would materialise more than the interpreted path may
    /// hold.** The mmap container's sibling of `A16PlanErrorV1::OverMemoryCeiling`, and the same
    /// argument: a chain-registered profile is a stranger's program, and the row widths and node
    /// counts in it are an allocation the registrant chose. Refused at PLAN time, before a byte is
    /// allocated. Leaving one of two interpreters unbounded would have made the ceiling a
    /// property of which container an attacker picked.
    OverMemoryCeiling { bytes: u64, ceiling: u64 },
}

impl std::fmt::Display for Qwen36PlanErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnIntegerLane => write!(f, "the profile is not the integer lane this family executes"),
            Self::GeometryMismatch { what, profile, artifact } => {
                write!(f, "the profile says {what} = {profile} and the artifact says {artifact}")
            }
            Self::UnservedNode { table, index, reason } => write!(f, "{table} node {index} cannot be served: {reason}"),
            Self::OverMemoryCeiling { bytes, ceiling } => write!(
                f,
                "one token's committed trace would be {bytes} bytes and the interpreted path is bounded at {ceiling} \
                 (ADR-0067 SA-1): a registered graph does not get to choose this node's memory"
            ),
        }
    }
}

impl std::error::Error for Qwen36PlanErrorV1 {}

/// An operand name as the declaration spells it: a per-layer suffix (`blk.{layer}.` stripped, the
/// layer substituted at execution) or a global name kept whole.
#[derive(Clone, Debug)]
enum NameRef {
    PerLayer(String),
    Global(String),
}

impl NameRef {
    fn resolve(&self, li: usize) -> String {
        match self {
            Self::PerLayer(suffix) => format!("blk.{li}.{suffix}"),
            Self::Global(name) => name.clone(),
        }
    }
}

/// Which of the three per-expert projections a `.routed` node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoutedStage {
    Gate,
    Up,
    Down,
}

/// Which segment of the convolution row an `L2Norm` node normalizes. Two declared nodes are
/// otherwise identical — same op, same kernel, same input — and the disambiguation is the
/// RECURRENCE's declared slots: the `GatedDeltaNet` node's input 0 is its key operand and input 2
/// its query operand, so the nodes those slots reference get the matching segment. Assigned in a
/// post-pass; an `L2Norm` no recurrence consumes has no defined segment and is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GdnSeg {
    Unassigned,
    Key,
    Query,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanInput {
    Row(usize),
    LayerIn,
    CachedK,
    CachedV,
}

/// One executable node. The fusion-derived store names (the expert triples, the decay's two
/// calibration rows, the recurrence's four narrowings, the convolution's requant) are NOT fields:
/// they are the `RESOLUTION_V2` conventions, fixed rules of the class rather than declared data,
/// and they live in the exec arms below where each cites its rule.
#[derive(Clone, Debug)]
enum PlanOpV1 {
    /// `token_embd.weight[token]`, lifted to i32 lanes.
    EmbedGather,
    /// The per-token lift: `embed_lift.a16` indexed by the token where the store has `vocab` rows.
    EmbedLift,
    /// `a16_rms_norm` over the whole row, at the class's `eps_q`.
    RmsNormUnit,
    /// `a16_rms_norm` per head of `head_dim` lanes — the attention QK-norm.
    HeadRmsNorm {
        heads: usize,
    },
    /// `q36_rms_norm_wide` per GDN value head, with the per-head eps rows of `linear_norm_eps.a16`.
    GdnHeadNormWide,
    /// `a16_requant` under `params_sized(name, width)` — the width-preserving narrowings.
    RequantSized {
        name: NameRef,
    },
    /// `a16_requant` under a `head_dim`-row store tiled per head — `attn_q_norm` / `attn_k_norm`.
    HeadRequant {
        name: NameRef,
        heads: usize,
    },
    /// `a16_requant` under one triple tiled to `heads × history` — `attn_probs.a16`.
    ProbsRequant {
        name: NameRef,
    },
    /// One projection through [`Qwen36Engine::project`]; `wide` is the DECLARED kernel's.
    Project {
        name: NameRef,
        out: usize,
        wide: bool,
    },
    /// The routed experts' projections, one stage across the `k` chosen — the ids from the
    /// committed `RouterTopk` row at `topk`.
    RoutedProject {
        stage: RoutedStage,
        wide: bool,
        topk: usize,
    },
    /// The four-tap causal convolution: window update plus `q36_ssm_conv`.
    SsmConv,
    Silu,
    /// Per value head: `q36_decay(dt + dt_bias, c)` from the two calibration rows.
    Decay,
    /// `q36_sigmoid_gate` over the row.
    SigmoidGate,
    /// Per key head: `q36_l2_norm` of the assigned convolution segment.
    L2NormSeg {
        seg: GdnSeg,
    },
    /// The recurrence: per value head, `q36_gdn_step` against this layer's state.
    GdnStep,
    /// `q36_rescale_row` under `params_sized(name, width)`.
    ScaleRow {
        name: NameRef,
    },
    /// `q36_mul_wide` under `params_sized(name, width)`.
    MulWide {
        name: NameRef,
    },
    /// The routed experts' silu-rescale and gated multiply, per chosen expert's own rows.
    RoutedGatedMul {
        topk: usize,
    },
    /// The shared expert's silu-rescale and gated multiply.
    SharedGatedMul,
    /// The scalar gate applied to the shared row: input 1 is one lane, broadcast across the row.
    SharedApply {
        name: NameRef,
    },
    /// `q36_gate_apply` under `one_param(name)` — the attention output gate.
    GateApply {
        name: NameRef,
    },
    /// `q36_rope_partial` over the class's `rotary_dim`, clamp from `one_param(name)`.
    RopePartial {
        name: NameRef,
    },
    AttnScores {
        name: NameRef,
    },
    Softmax {
        name: NameRef,
    },
    AttnValues {
        name: NameRef,
    },
    /// `q36_router_topk`; commits `[ids…, weights…]` — the order the engine's own probe uses.
    RouterTopk {
        name: NameRef,
        k: usize,
    },
    /// `q36_moe_combine`; the weights are the declared second input's weight lanes.
    MoeCombine {
        name: NameRef,
        k: usize,
    },
    AddElem,
}

#[derive(Clone, Debug)]
struct PlanNode {
    op: PlanOpV1,
    inputs: Vec<PlanInput>,
    role: PalwStepNodeRoleV1,
}

/// A compiled execution plan for one registered Qwen3.6-family profile against one artifact.
/// Holding one is the proof that every declared node is servable.
#[derive(Clone, Debug)]
pub struct Qwen36ProfilePlanV1 {
    pre: Vec<PlanNode>,
    gdn: Vec<PlanNode>,
    attn: Vec<PlanNode>,
    post: Vec<PlanNode>,
    layer_kinds: Vec<Qwen36LayerKind>,
}

/// The committed rows of one planned pass — one row per declared node, per table, in order.
/// The layer tables repeat per layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Qwen36PlanTraceV1 {
    pub pre: Vec<Vec<i32>>,
    pub layers: Vec<Vec<Vec<i32>>>,
    pub post: Vec<Vec<i32>>,
}

/// What a node's output IS, statically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum W {
    Fixed(u32),
    KvScaled(u32),
    Series,
}

impl<'a> Qwen36Engine<'a> {
    /// Compile the registered profile into a plan this engine can walk. Refusals are the ADR-0067
    /// kernel boundary; an `Ok` is a structural Decision-F proof for this class — the declaration
    /// is the program.
    pub fn plan_from_profile(&self, profile: &PalwShapeProfileV3) -> Result<Qwen36ProfilePlanV1, Qwen36PlanErrorV1> {
        self.plan_from_profile_within(profile, crate::engine_a16::PALW_INTERPRETER_TRACE_BYTES_CEILING_V1)
    }

    /// [`Self::plan_from_profile`] under a caller-chosen ceiling — ADR-0067 SA-1, and the same
    /// shape as the dense container's so the two families cannot be bounded differently by
    /// accident. The ceiling is a parameter so a test can show it BINDS rather than that nothing
    /// crashed.
    pub fn plan_from_profile_within(
        &self,
        profile: &PalwShapeProfileV3,
        ceiling_bytes: u64,
    ) -> Result<Qwen36ProfilePlanV1, Qwen36PlanErrorV1> {
        let s = &self.artifact.shape;
        if profile.lane != PalwStepLaneV1::Int32 {
            return Err(Qwen36PlanErrorV1::NotAnIntegerLane);
        }
        let check = |what: &'static str, p: u64, a: u64| -> Result<(), Qwen36PlanErrorV1> {
            if p != a { Err(Qwen36PlanErrorV1::GeometryMismatch { what, profile: p, artifact: a }) } else { Ok(()) }
        };
        check("layer_count", profile.layer_count as u64, s.n_layers() as u64)?;
        // The layer stack itself: the profile spells it as an interval, the artifact as a list,
        // and the two must be the same alternation — a GDN table walked on an attention layer
        // would read a recurrence the artifact does not carry.
        let layer_kinds: Vec<Qwen36LayerKind> = (0..profile.layer_count as usize)
            .map(|i| {
                if profile.full_attention_interval != 0 && (i + 1).is_multiple_of(profile.full_attention_interval as usize) {
                    Qwen36LayerKind::FullAttention
                } else {
                    Qwen36LayerKind::LinearAttention
                }
            })
            .collect();
        for (i, kind) in layer_kinds.iter().enumerate() {
            if *kind != s.layer_types[i] {
                return Err(Qwen36PlanErrorV1::GeometryMismatch {
                    what: "full_attention_interval (the layer stack)",
                    profile: profile.full_attention_interval as u64,
                    artifact: i as u64,
                });
            }
        }
        check("hidden_dim", profile.hidden_dim as u64, s.d_model as u64)?;
        check("ffn_dim", profile.ffn_dim as u64, s.moe_dim as u64)?;
        check("attn_heads", profile.attn_heads as u64, s.n_heads as u64)?;
        check("attn_kv_heads", profile.attn_kv_heads as u64, s.n_kv_heads as u64)?;
        check("attn_head_dim", profile.attn_head_dim as u64, s.head_dim as u64)?;
        check("rope_dims", profile.rope_dims as u64, s.rotary_dim as u64)?;
        check("gdn_heads", profile.gdn_heads as u64, s.linear_v_heads as u64)?;
        check("gdn_head_k_dim", profile.gdn_head_k_dim as u64, s.linear_head_dim as u64)?;
        check("gdn_head_v_dim", profile.gdn_head_v_dim as u64, s.linear_head_dim as u64)?;
        check("gdn_conv_kernel", profile.gdn_conv_kernel as u64, s.conv_kernel as u64)?;
        check("vocab_size", profile.vocab_size as u64, s.vocab as u64)?;
        // The eps is an artifact field AND a profile field, and it moves every activation.
        check("rms_eps_q", profile.base0_rms_eps_q as u64, s.eps_q as u64)?;

        // The memory ceiling, in the dense container's position and for its reason (ADR-0067
        // SA-1): after the free scalar comparisons, before `plan_table` spends a byte. Everything
        // allocated between here and there is bounded by `layer_count`, which the first `check`
        // above already pinned to the artifact. `max_position` is the artifact's own bound on a
        // kv-scaled row.
        let bytes = crate::engine_a16::interpreted_trace_bytes_v1(profile, s.max_position as u64);
        if bytes > ceiling_bytes {
            return Err(Qwen36PlanErrorV1::OverMemoryCeiling { bytes, ceiling: ceiling_bytes });
        }

        let gdn_layers = layer_kinds.iter().filter(|k| **k == Qwen36LayerKind::LinearAttention).count();
        let attn_layers = layer_kinds.len() - gdn_layers;
        let empty_where_layers = |table: &'static str, nodes: usize, layers: usize| -> Result<(), Qwen36PlanErrorV1> {
            if (nodes == 0) != (layers == 0) {
                return Err(Qwen36PlanErrorV1::UnservedNode {
                    table,
                    index: 0,
                    reason: format!("{layers} layer(s) of this kind against {nodes} declared node(s) — one of them is missing"),
                });
            }
            Ok(())
        };
        empty_where_layers("gdn", profile.gdn_nodes.len(), gdn_layers)?;
        empty_where_layers("attn", profile.attn_nodes.len(), attn_layers)?;

        let d = s.d_model as u32;
        let pre = plan_table(self, &profile.pre_nodes, "pre", None)?;
        terminal(&profile.pre_nodes, "pre", d)?;
        let gdn = plan_table(self, &profile.gdn_nodes, "gdn", Some(d))?;
        if !profile.gdn_nodes.is_empty() {
            terminal(&profile.gdn_nodes, "gdn", d)?;
        }
        let attn = plan_table(self, &profile.attn_nodes, "attn", Some(d))?;
        if !profile.attn_nodes.is_empty() {
            terminal(&profile.attn_nodes, "attn", d)?;
        }
        let post = plan_table(self, &profile.post_nodes, "post", Some(d))?;
        terminal(&profile.post_nodes, "post", s.vocab as u32)?;
        Ok(Qwen36ProfilePlanV1 { pre, gdn, attn, post, layer_kinds })
    }

    /// One position's forward, EXECUTED FROM THE PLAN: one committed row per declared node, in
    /// the declared order, honoring the declared cache-write roles. Bit-compatible with
    /// [`Qwen36Engine::forward_token_probed`] whenever the plan was compiled from the profile
    /// that describes this engine — the `graph-v2` row — which the differentials below pin.
    pub fn forward_token_planned(
        &self,
        plan: &Qwen36ProfilePlanV1,
        cache: &mut Qwen36Cache,
        token_id: usize,
        position: usize,
    ) -> Result<(Vec<i32>, Qwen36PlanTraceV1), Qwen36Error> {
        let s = &self.artifact.shape;
        if plan.layer_kinds.len() != s.n_layers() {
            return Err(Qwen36Error::BadParams("the plan and the artifact disagree about the layer count".into()));
        }
        // The compiled engine refuses an out-of-vocabulary token before reading anything.
        if token_id >= s.vocab {
            return Err(Qwen36Error::Position);
        }
        let mut trace = Qwen36PlanTraceV1::default();

        let rows = self.walk_table(&plan.pre, None, token_id, position, None)?;
        let mut h = rows.last().cloned().ok_or_else(|| Qwen36Error::BadParams("an empty pre table".into()))?;
        trace.pre = rows;

        for (li, kind) in plan.layer_kinds.iter().enumerate() {
            let table = match kind {
                Qwen36LayerKind::LinearAttention => &plan.gdn,
                Qwen36LayerKind::FullAttention => &plan.attn,
            };
            let rows = self.walk_table(table, Some(&h), token_id, position, Some((li, cache)))?;
            h = rows.last().cloned().ok_or_else(|| Qwen36Error::BadParams("an empty layer table".into()))?;
            trace.layers.push(rows);
        }

        let rows = self.walk_table(&plan.post, Some(&h), token_id, position, None)?;
        let logits = rows.last().cloned().ok_or_else(|| Qwen36Error::BadParams("an empty post table".into()))?;
        trace.post = rows;
        Ok((logits, trace))
    }

    /// The same planned pass, keeping only the logit row — the PRODUCER's entry. A walk must
    /// retain a table's rows while that table runs (any later node may read any earlier row),
    /// but nothing after a layer reads that layer's rows again, and the traced variant retains
    /// all of them for the life of the token — tens of megabytes per forward at the real class's
    /// widths, serving no reader. The roots this family commits are computed from logits alone.
    /// Held bit-identical to [`Self::forward_token_planned`] by `the_untraced_walk_is_the_traced_one`.
    pub fn forward_token_planned_logits(
        &self,
        plan: &Qwen36ProfilePlanV1,
        cache: &mut Qwen36Cache,
        token_id: usize,
        position: usize,
    ) -> Result<Vec<i32>, Qwen36Error> {
        let s = &self.artifact.shape;
        if plan.layer_kinds.len() != s.n_layers() {
            return Err(Qwen36Error::BadParams("the plan and the artifact disagree about the layer count".into()));
        }
        if token_id >= s.vocab {
            return Err(Qwen36Error::Position);
        }
        let rows = self.walk_table(&plan.pre, None, token_id, position, None)?;
        let mut h = rows.into_iter().next_back().ok_or_else(|| Qwen36Error::BadParams("an empty pre table".into()))?;
        for (li, kind) in plan.layer_kinds.iter().enumerate() {
            let table = match kind {
                Qwen36LayerKind::LinearAttention => &plan.gdn,
                Qwen36LayerKind::FullAttention => &plan.attn,
            };
            let rows = self.walk_table(table, Some(&h), token_id, position, Some((li, cache)))?;
            h = rows.into_iter().next_back().ok_or_else(|| Qwen36Error::BadParams("an empty layer table".into()))?;
        }
        let rows = self.walk_table(&plan.post, Some(&h), token_id, position, None)?;
        rows.into_iter().next_back().ok_or_else(|| Qwen36Error::BadParams("an empty post table".into()))
    }

    /// Walk one table. `layer` is `Some((index, cache))` for the layer tables — the only ones
    /// with cache reads, cache writes and per-layer operands.
    fn walk_table(
        &self,
        table: &[PlanNode],
        layer_in: Option<&Vec<i32>>,
        token_id: usize,
        position: usize,
        mut layer: Option<(usize, &mut Qwen36Cache)>,
    ) -> Result<Vec<Vec<i32>>, Qwen36Error> {
        let a = self.artifact;
        let s = &a.shape;
        let d = s.d_model;
        let (dk, dv, hd) = (s.linear_k_dim(), s.linear_v_dim(), s.linear_head_dim);
        let kv_dim = s.kv_dim();
        let refuse = |what: &'static str| {
            move |e: Qwen36Error| -> Qwen36Error {
                match e {
                    Qwen36Error::OpRefused(_, why) => Qwen36Error::OpRefused(what, why),
                    other => other,
                }
            }
        };
        let op_refuse = |what: &'static str| {
            move |e: kaspa_consensus_core::palw_qwen36_ops::PalwQwen36OpError| -> Qwen36Error {
                Qwen36Error::OpRefused(what, format!("{e:?}"))
            }
        };
        let a16_refuse = |what: &'static str| {
            move |e: kaspa_consensus_core::palw_base0_a16::PalwA16OpError| -> Qwen36Error {
                Qwen36Error::OpRefused(what, format!("{e:?}"))
            }
        };
        // The engine's full_arm resolves the rotation rows before its first projection, so an
        // attention layer at a position past the table fails BEFORE any node commits — matched
        // here so the two paths leave the same cache behind on the same refusal.
        let mut rope_rows: Option<(&[i32], &[i32])> = None;
        if layer.is_some() && table.iter().any(|n| matches!(n.op, PlanOpV1::RopePartial { .. })) {
            let (cos_row, sin_row) = a.rope.row(position).ok_or(Qwen36Error::Position)?;
            let pairs = s.rotary_dim / 2;
            if cos_row.len() < pairs {
                return Err(Qwen36Error::Position);
            }
            rope_rows = Some((&cos_row[..pairs], &sin_row[..pairs]));
        }
        let li = layer.as_ref().map(|(li, _)| *li).unwrap_or(0);
        let name_of = |n: &NameRef| n.resolve(li);
        let per_head = |rows: &[A16QuantParams], vh: usize| -> A16QuantParams {
            if rows.len() == 1 { rows[0] } else { rows[vh.min(rows.len() - 1)] }
        };

        let mut rows: Vec<Vec<i32>> = Vec::with_capacity(table.len());
        for node in table {
            let resolve = |input: &PlanInput, rows: &Vec<Vec<i32>>| -> Result<Vec<i32>, Qwen36Error> {
                match input {
                    PlanInput::Row(i) => rows.get(*i).cloned().ok_or_else(|| Qwen36Error::BadParams("a forward input ref".into())),
                    PlanInput::LayerIn => {
                        layer_in.cloned().ok_or_else(|| Qwen36Error::BadParams("layer input outside a layer".into()))
                    }
                    // The series are built at USE, after this position's cache writes, so a read
                    // sees the same history the compiled engine hands the kernels.
                    PlanInput::CachedK | PlanInput::CachedV => {
                        let Some((li, cache)) = layer.as_ref() else {
                            return Err(Qwen36Error::BadParams("a cache read outside a layer".into()));
                        };
                        let series = match input {
                            PlanInput::CachedK => &cache.keys[*li],
                            _ => &cache.values[*li],
                        };
                        let mut out = Vec::with_capacity(series.len() * kv_dim);
                        for row in series {
                            out.extend_from_slice(row);
                        }
                        Ok(out)
                    }
                }
            };

            let out: Vec<i32> = match &node.op {
                PlanOpV1::EmbedGather => {
                    let embed = a.tensor_sized("token_embd.weight", s.vocab * d)?;
                    embed[token_id * d..(token_id + 1) * d].iter().map(|c| *c as i32).collect()
                }
                PlanOpV1::EmbedLift => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let lift = a.param_rows("embed_lift.a16")?;
                    let p = if lift.len() == 1 {
                        lift[0]
                    } else {
                        *lift
                            .get(token_id)
                            .ok_or_else(|| Qwen36Error::BadParams("embed_lift.a16 is shorter than the vocabulary".into()))?
                    };
                    a16_requant(&x, &vec![p; d]).map_err(a16_refuse("embed_lift"))?
                }
                PlanOpV1::RmsNormUnit => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    a16_rms_norm(&x, s.eps_q).map_err(a16_refuse("rms_norm"))?
                }
                PlanOpV1::HeadRmsNorm { heads } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let mut out = Vec::with_capacity(x.len());
                    for head in 0..*heads {
                        let slice = &x[head * s.head_dim..(head + 1) * s.head_dim];
                        out.extend(a16_rms_norm(slice, s.eps_q).map_err(a16_refuse("qk_norm"))?);
                    }
                    out
                }
                PlanOpV1::GdnHeadNormWide => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    // RESOLUTION_V2: the node's eps rows are `linear_norm_eps.a16`, per value head.
                    let eps = a.param_rows(&format!("blk.{li}.linear_norm_eps.a16"))?;
                    let mut out = Vec::with_capacity(x.len());
                    for vh in 0..s.linear_v_heads {
                        let head = &x[vh * hd..(vh + 1) * hd];
                        out.extend(q36_rms_norm_wide(head, eps[vh.min(eps.len() - 1)]).map_err(op_refuse("gdn_norm"))?);
                    }
                    out
                }
                PlanOpV1::RequantSized { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let params = a.params_sized(&name_of(name), x.len())?;
                    a16_requant(&x, &params).map_err(a16_refuse("requant"))?
                }
                PlanOpV1::HeadRequant { name, heads } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let per = a.params_sized(&name_of(name), s.head_dim)?;
                    let mut params = Vec::with_capacity(per.len() * heads);
                    for _ in 0..*heads {
                        params.extend_from_slice(&per);
                    }
                    a16_requant(&x, &params).map_err(a16_refuse("qk_norm_req"))?
                }
                PlanOpV1::ProbsRequant { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let p = a.one_param(&name_of(name))?;
                    a16_requant(&x, &vec![p; x.len()]).map_err(a16_refuse("attn_probs"))?
                }
                PlanOpV1::Project { name, out, wide } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    self.project(&name_of(name), &x, *out, *wide).map_err(refuse("projection"))?
                }
                PlanOpV1::RoutedProject { stage, wide, topk } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let chosen = routed_ids(&rows, *topk, s.experts_per_token)?;
                    // RESOLUTION_V2: `ffn_*_exps.routed` reads each chosen expert's own store,
                    // `blk.N.ffn_expert.{e}_<stage>.weight` with the params riding beside it.
                    let (suffix, out_dim, in_dim) = match stage {
                        RoutedStage::Gate => ("_gate.weight", s.moe_dim, x.len()),
                        RoutedStage::Up => ("_up.weight", s.moe_dim, x.len()),
                        RoutedStage::Down => ("_down.weight", d, s.moe_dim),
                    };
                    let mut out = Vec::with_capacity(chosen.len() * out_dim);
                    for (i, e) in chosen.iter().enumerate() {
                        let input = match stage {
                            RoutedStage::Down => &x[i * in_dim..(i + 1) * in_dim],
                            _ => &x[..],
                        };
                        out.extend(
                            self.project(&format!("blk.{li}.ffn_expert.{e}{suffix}"), input, out_dim, *wide)
                                .map_err(refuse("routed projection"))?,
                        );
                    }
                    out
                }
                PlanOpV1::SsmConv => {
                    let q = resolve(&node.inputs[0], &rows)?;
                    let k = resolve(&node.inputs[1], &rows)?;
                    let v = resolve(&node.inputs[2], &rows)?;
                    let (li, cache) = layer.as_mut().ok_or_else(|| Qwen36Error::BadParams("a conv outside a layer".into()))?;
                    let width = 2 * dk + dv;
                    let mut current = Vec::with_capacity(width);
                    current.extend_from_slice(&q);
                    current.extend_from_slice(&k);
                    current.extend_from_slice(&v);
                    let window = &mut cache.conv[*li];
                    // The cache pre-fills windows only for the layers the ARTIFACT calls
                    // recurrent, but the declaration is the program: a gate-accepted profile may
                    // put a convolution in the attention table, and its window before the
                    // sequence start is zero rows — the same start the engine's own windows have.
                    // `remove(0)` on the unfilled window was a panic a stranger's registration
                    // could reach (found by this module's own fuzz harness design pass).
                    if window.is_empty() {
                        *window = vec![vec![0; width]; s.conv_kernel.max(1)];
                    }
                    window.remove(0);
                    window.push(current);
                    let flat: Vec<i32> = window.iter().flatten().copied().collect();
                    // RESOLUTION_V2: the declared `linear_conv.weight` is two stores — the taps
                    // and the requant of the convolution's output.
                    let taps: Vec<i32> = a
                        .tensor_sized(&format!("blk.{li}.linear_conv.weight"), s.conv_kernel * width)?
                        .iter()
                        .map(|c| *c as i32)
                        .collect();
                    let params = a.params_sized(&format!("blk.{li}.linear_conv.a16"), width)?;
                    q36_ssm_conv(&flat, &taps, width, &params).map_err(op_refuse("ssm_conv"))?
                }
                PlanOpV1::Silu => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    silu(&x)
                }
                PlanOpV1::Decay => {
                    let dt = resolve(&node.inputs[0], &rows)?;
                    // RESOLUTION_V2: the declared `linear_decay.a16` is a computation over the
                    // two calibration rows `linear_decay_c.a16` and `linear_dt_bias.a16`.
                    let decay_c = a.param_rows(&format!("blk.{li}.linear_decay_c.a16"))?;
                    let dt_bias = a.param_rows(&format!("blk.{li}.linear_dt_bias.a16"))?;
                    let mut out = Vec::with_capacity(dt.len());
                    for (vh, lane) in dt.iter().enumerate() {
                        let c = decay_c.get(vh.min(decay_c.len().saturating_sub(1))).map(|p| p.zero).unwrap_or(0);
                        let bias = dt_bias.get(vh.min(dt_bias.len().saturating_sub(1))).map(|p| p.zero).unwrap_or(0);
                        out.push(q36_decay(lane.saturating_add(bias as i32), c) as i32);
                    }
                    out
                }
                PlanOpV1::SigmoidGate => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    q36_sigmoid_gate(&x)
                }
                PlanOpV1::L2NormSeg { seg } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let offset = match seg {
                        GdnSeg::Query => 0,
                        GdnSeg::Key => dk,
                        GdnSeg::Unassigned => {
                            return Err(Qwen36Error::BadParams("an L2 norm no recurrence consumes reached execution".into()));
                        }
                    };
                    let mut out = Vec::with_capacity(dk);
                    for kh in 0..s.linear_k_heads {
                        let slice = &x[offset + kh * hd..offset + (kh + 1) * hd];
                        out.extend(q36_l2_norm(slice).map_err(op_refuse("l2_norm"))?);
                    }
                    out
                }
                PlanOpV1::GdnStep => {
                    let unit_k = resolve(&node.inputs[0], &rows)?;
                    let conv = resolve(&node.inputs[1], &rows)?;
                    let unit_q = resolve(&node.inputs[2], &rows)?;
                    let decays = resolve(&node.inputs[3], &rows)?;
                    let betas = resolve(&node.inputs[4], &rows)?;
                    let (li, cache) = layer.as_mut().ok_or_else(|| Qwen36Error::BadParams("a recurrence outside a layer".into()))?;
                    // Same rule as the convolution window above: the cache pre-allocates states
                    // only for the layers the ARTIFACT calls recurrent, and a gate-accepted
                    // declaration may put the recurrence in the attention table. Its state before
                    // the sequence start is zero — `Qwen36Cache::new`'s own start — so the states
                    // are made on first demand rather than indexed into a panic.
                    if cache.gdn[*li].len() != s.linear_v_heads {
                        cache.gdn[*li] = (0..s.linear_v_heads)
                            .map(|_| kaspa_consensus_core::palw_qwen36_ops::Qwen36GdnStateV1::zeros(hd, hd))
                            .collect();
                    }
                    // RESOLUTION_V2: the declared `linear_gdn.a16` is the recurrence's four
                    // per-head narrowings, each its own store.
                    let read_rows = a.param_rows(&format!("blk.{li}.linear_read.a16"))?;
                    let delta_rows = a.param_rows(&format!("blk.{li}.linear_delta.a16"))?;
                    let write_rows = a.param_rows(&format!("blk.{li}.linear_write.a16"))?;
                    let out_rows = a.param_rows(&format!("blk.{li}.linear_out.a16"))?;
                    let mut out = Vec::with_capacity(dv);
                    for vh in 0..s.linear_v_heads {
                        // `vh % n_k`, not `vh / (n_v/n_k)`: the heads tile, they do not group —
                        // the engine's own rule, and the reference measured what the other
                        // reading costs.
                        let kh = vh % s.linear_k_heads;
                        let vslice = &conv[2 * dk + vh * hd..2 * dk + (vh + 1) * hd];
                        let params = Qwen36GdnParamsV1 {
                            read: per_head(&read_rows, vh),
                            delta: per_head(&delta_rows, vh),
                            write_shift: per_head(&write_rows, vh).zero as i32,
                            out: per_head(&out_rows, vh),
                        };
                        let head_out = q36_gdn_step(
                            &mut cache.gdn[*li][vh],
                            &unit_k[kh * hd..(kh + 1) * hd],
                            vslice,
                            &unit_q[kh * hd..(kh + 1) * hd],
                            decays[vh] as i64,
                            betas[vh] as i64,
                            params,
                        )
                        .map_err(op_refuse("gdn_step"))?;
                        out.extend(head_out);
                    }
                    out
                }
                PlanOpV1::ScaleRow { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let params = a.params_sized(&name_of(name), x.len())?;
                    q36_rescale_row(&x, &params).map_err(op_refuse("rescale_row"))?
                }
                PlanOpV1::MulWide { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let y = resolve(&node.inputs[1], &rows)?;
                    let params = a.params_sized(&name_of(name), x.len())?;
                    q36_mul_wide(&x, &y, &params).map_err(op_refuse("mul_wide"))?
                }
                PlanOpV1::RoutedGatedMul { topk } => {
                    let gate = resolve(&node.inputs[0], &rows)?;
                    let up = resolve(&node.inputs[1], &rows)?;
                    let chosen = routed_ids(&rows, *topk, s.experts_per_token)?;
                    let mid = s.moe_dim;
                    // RESOLUTION_V2: the declared `ffn_expert_gated.a16` is each chosen expert's
                    // own two narrowings — the silu rescale and the gated multiply.
                    let mut out = Vec::with_capacity(chosen.len() * mid);
                    for (i, e) in chosen.iter().enumerate() {
                        let activated = q36_rescale_row(
                            &gate[i * mid..(i + 1) * mid],
                            &a.params_sized(&format!("blk.{li}.ffn_expert.{e}_silu.a16"), mid)?,
                        )
                        .map_err(op_refuse("expert_silu"))?;
                        out.extend(
                            q36_mul_wide(
                                &activated,
                                &up[i * mid..(i + 1) * mid],
                                &a.params_sized(&format!("blk.{li}.ffn_expert.{e}_gated.a16"), mid)?,
                            )
                            .map_err(op_refuse("expert_gated"))?,
                        );
                    }
                    out
                }
                PlanOpV1::SharedGatedMul => {
                    let gate = resolve(&node.inputs[0], &rows)?;
                    let up = resolve(&node.inputs[1], &rows)?;
                    let mid = s.shared_dim;
                    // RESOLUTION_V2: the shared expert's silu rescale rides inside its gated
                    // multiply, the same fusion as the routed experts'.
                    let activated = q36_rescale_row(&gate, &a.params_sized(&format!("blk.{li}.ffn_shared_expert_silu.a16"), mid)?)
                        .map_err(op_refuse("shared_silu"))?;
                    q36_mul_wide(&activated, &up, &a.params_sized(&format!("blk.{li}.ffn_shared_expert_gated.a16"), mid)?)
                        .map_err(op_refuse("shared_gated"))?
                }
                PlanOpV1::SharedApply { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let g = resolve(&node.inputs[1], &rows)?;
                    let g = *g.first().ok_or_else(|| Qwen36Error::BadParams("an empty scalar gate row".into()))?;
                    q36_mul_wide(&x, &vec![g; d], &vec![a.one_param(&name_of(name))?; d]).map_err(op_refuse("shared_apply"))?
                }
                PlanOpV1::GateApply { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let gate = resolve(&node.inputs[1], &rows)?;
                    q36_gate_apply(&x, &gate, a.one_param(&name_of(name))?).map_err(op_refuse("attn_gate_apply"))?
                }
                PlanOpV1::RopePartial { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let (cos_row, sin_row) = rope_rows.ok_or(Qwen36Error::Position)?;
                    let clamp = a.one_param(&name_of(name))?;
                    q36_rope_partial(&x, s.head_dim, s.rotary_dim, cos_row, sin_row, clamp).map_err(op_refuse("rope"))?
                }
                PlanOpV1::AttnScores { name } => {
                    let q = resolve(&node.inputs[0], &rows)?;
                    let k_series = resolve(&node.inputs[1], &rows)?;
                    let history = if kv_dim == 0 { 0 } else { k_series.len() / kv_dim };
                    let p = a.one_param(&name_of(name))?;
                    a16_attn_scores_fast(&q, &k_series, s.n_heads, s.n_kv_heads, s.head_dim, &vec![p; s.n_heads * history])
                        .map_err(a16_refuse("attn_scores"))?
                }
                PlanOpV1::Softmax { name } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let history = if s.n_heads == 0 { 0 } else { x.len() / s.n_heads };
                    let up_bits = a.scalar(&name_of(name))?.clamp(0, 62) as u8;
                    a16_softmax_rows(&x, history, up_bits).map_err(a16_refuse("attn_softmax"))?
                }
                PlanOpV1::AttnValues { name } => {
                    let p_row = resolve(&node.inputs[0], &rows)?;
                    let v_series = resolve(&node.inputs[1], &rows)?;
                    let p = a.one_param(&name_of(name))?;
                    a16_attn_values_fast(&p_row, &v_series, s.n_heads, s.n_kv_heads, s.head_dim, &vec![p; s.n_heads * s.head_dim])
                        .map_err(a16_refuse("attn_values"))?
                }
                PlanOpV1::RouterTopk { name, k } => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let up = a.scalar(&name_of(name))?.clamp(0, 62) as u8;
                    let routed = q36_router_topk(&x, *k, up).map_err(op_refuse("router_topk"))?;
                    // Hand the kernel every byte the chosen experts will read, before any of them
                    // is computed — residency, which changes no bit of arithmetic.
                    if let Some((li, _)) = layer.as_ref() {
                        self.admit_experts(*li, &routed.iter().map(|r| r.expert as usize).collect::<Vec<_>>());
                    }
                    // The committed row: ids then weights, the order the engine's own probe rows
                    // (`ffn_choice`, `ffn_weight`) already use.
                    let mut out = Vec::with_capacity(2 * k);
                    out.extend(routed.iter().map(|r| r.expert as i32));
                    out.extend(routed.iter().map(|r| r.weight_q));
                    out
                }
                PlanOpV1::MoeCombine { name, k } => {
                    let outputs = resolve(&node.inputs[0], &rows)?;
                    let topk_row = resolve(&node.inputs[1], &rows)?;
                    let weights = topk_row
                        .get(*k..2 * k)
                        .ok_or_else(|| Qwen36Error::BadParams("a top-k row shorter than its declared width".into()))?;
                    q36_moe_combine(&outputs, weights, d, a.one_param(&name_of(name))?).map_err(op_refuse("moe_combine"))?
                }
                PlanOpV1::AddElem => {
                    let x = resolve(&node.inputs[0], &rows)?;
                    let y = resolve(&node.inputs[1], &rows)?;
                    a16_add_elem(&x, &y).map_err(a16_refuse("add_elem"))?
                }
            };

            // The declared cache write, honored where declared: v2 carries `KCacheWrite` on the
            // rotated key and `VCacheWrite` on the V projection — the computations that actually
            // feed the caches.
            match node.role {
                PalwStepNodeRoleV1::KCacheWrite => {
                    let (li, cache) = layer.as_mut().ok_or_else(|| Qwen36Error::BadParams("a cache write outside a layer".into()))?;
                    cache.keys[*li].push(out.clone());
                }
                PalwStepNodeRoleV1::VCacheWrite => {
                    let (li, cache) = layer.as_mut().ok_or_else(|| Qwen36Error::BadParams("a cache write outside a layer".into()))?;
                    cache.values[*li].push(out.clone());
                }
                PalwStepNodeRoleV1::Plain => {}
            }
            rows.push(out);
        }
        Ok(rows)
    }
}

/// The committed `RouterTopk` row's expert ids, in the committed order.
fn routed_ids(rows: &[Vec<i32>], topk: usize, k: usize) -> Result<Vec<usize>, Qwen36Error> {
    let row = rows.get(topk).ok_or_else(|| Qwen36Error::BadParams("the routing row is not committed yet".into()))?;
    let ids = row.get(..k).ok_or_else(|| Qwen36Error::BadParams("a top-k row shorter than its declared width".into()))?;
    ids.iter()
        .map(|id| usize::try_from(*id).map_err(|_| Qwen36Error::BadParams("a negative expert id in the routing row".into())))
        .collect()
}

/// A table's last node must produce the fixed width the next stage reads.
fn terminal(nodes: &[PalwStepNodeV1], table: &'static str, want: u32) -> Result<(), Qwen36PlanErrorV1> {
    match nodes.last().map(|n| n.out_len) {
        Some(PalwStepOutLenV1::Fixed { elements }) if elements == want => Ok(()),
        other => Err(Qwen36PlanErrorV1::UnservedNode {
            table,
            index: nodes.len().saturating_sub(1),
            reason: format!("the table ends at {other:?}, and the stage after it reads a fixed {want}-lane row"),
        }),
    }
}

/// `blk.{layer}.suffix` → `suffix`. The `{layer}` template survives lowering; the ABI is the
/// literal template, exactly as the A16 planner reads it.
fn strip_layer(name: &str) -> Option<&str> {
    name.strip_prefix("blk.{layer}.")
}

/// Compile one declared table. Every refusal names the node and the reason — this function IS the
/// kernel-set boundary of ADR-0067 Decision 3 for the mmap container.
fn plan_table(
    engine: &Qwen36Engine<'_>,
    nodes: &[PalwStepNodeV1],
    table: &'static str,
    layer_in: Option<u32>,
) -> Result<Vec<PlanNode>, Qwen36PlanErrorV1> {
    use PalwStepOpKindV1 as Op;
    let s: &Qwen36ShapeV1 = &engine.artifact.shape;

    // Only the layer tables have a layer: a `blk.{layer}.*` operand elsewhere would silently
    // execute under layer 0's stores — the A16 planner's rule, kept.
    let per_layer_ok = table == "gdn" || table == "attn";

    let k_embed = kernel_semantics_id_v1(KDESC_A16_EMBED);
    let k_req = kernel_semantics_id_v1(KDESC_A16_REQUANTIZE);
    let k_rms = kernel_semantics_id_v1(KDESC_A16_RMS_NORM);
    let k_scores = kernel_semantics_id_v1(KDESC_A16_ATTN_SCORES);
    let k_soft = kernel_semantics_id_v1(KDESC_A16_SOFTMAX);
    let k_vals = kernel_semantics_id_v1(KDESC_A16_ATTN_VALUES);
    let k_add = kernel_semantics_id_v1(KDESC_A16_ADD_ELEM);
    let k_rescale_mm = kernel_semantics_id_v1(KDESC_A16_MATMUL_RESCALE);
    let k_grouped = kernel_semantics_id_v1(KDESC_Q36_MATMUL_GROUPED);
    let k_grouped_wide = kernel_semantics_id_v1(KDESC_Q36_MATMUL_GROUPED_WIDE);
    let k_silu = kernel_semantics_id_v1(KDESC_Q36_SILU);
    let k_conv = kernel_semantics_id_v1(KDESC_Q36_SSM_CONV);
    let k_decay = kernel_semantics_id_v1(KDESC_Q36_DECAY);
    let k_sigmoid = kernel_semantics_id_v1(KDESC_Q36_SIGMOID);
    let k_l2 = kernel_semantics_id_v1(KDESC_Q36_L2_NORM);
    let k_gdn = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let k_rms_wide = kernel_semantics_id_v1(KDESC_Q36_RMS_NORM_WIDE);
    let k_head_rms = kernel_semantics_id_v1(KDESC_Q36_HEAD_RMS_NORM);
    let k_rescale_row = kernel_semantics_id_v1(KDESC_Q36_RESCALE_ROW);
    let k_mul_wide = kernel_semantics_id_v1(KDESC_Q36_MUL_WIDE);
    let k_gate_apply = kernel_semantics_id_v1(KDESC_Q36_GATE_APPLY);
    let k_rope = kernel_semantics_id_v1(KDESC_Q36_ROPE_PARTIAL);
    let k_topk = kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK);
    let k_combine = kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE);

    let d = s.d_model as u32;
    let dk = s.linear_k_dim() as u32;
    let dv = s.linear_v_dim() as u32;
    let conv_w = 2 * dk + dv;
    let v_heads = s.linear_v_heads as u32;
    let q_dim = (s.n_heads * s.head_dim) as u32;
    let kv_dim = s.kv_dim() as u32;
    let experts = s.n_experts as u32;
    let k_of = s.experts_per_token as u32;
    let moe = s.moe_dim as u32;
    let shared = s.shared_dim as u32;
    let heads = s.n_heads as u32;
    let vocab = s.vocab as u32;

    let refuse = |index: usize, reason: String| Qwen36PlanErrorV1::UnservedNode { table, index, reason };

    let mut widths: Vec<W> = Vec::with_capacity(nodes.len());
    let mut out: Vec<PlanNode> = Vec::with_capacity(nodes.len());
    let mut last_topk: Option<usize> = None;
    for (index, node) in nodes.iter().enumerate() {
        let mut inputs = Vec::with_capacity(node.input_refs.len());
        for r in &node.input_refs {
            let input = match *r {
                PALW_STEP_INPUT_LAYER_IN => PlanInput::LayerIn,
                PALW_STEP_INPUT_KV_K => PlanInput::CachedK,
                PALW_STEP_INPUT_KV_V => PlanInput::CachedV,
                i if i >= PALW_STEP_INPUT_SENTINEL_MIN => {
                    return Err(refuse(index, format!("input sentinel {i:#x} is not one this family serves")));
                }
                i => {
                    if (i as usize) >= index {
                        return Err(refuse(index, format!("input ref {i} is not an earlier node of this table")));
                    }
                    PlanInput::Row(i as usize)
                }
            };
            inputs.push(input);
        }
        let arity = |n: usize| -> Result<(), Qwen36PlanErrorV1> {
            if inputs.len() != n { Err(refuse(index, format!("arity {} where the kernel takes {n}", inputs.len()))) } else { Ok(()) }
        };
        let width_of = |input: &PlanInput| -> W {
            match input {
                PlanInput::Row(i) => widths[*i],
                PlanInput::LayerIn => layer_in.map(W::Fixed).unwrap_or(W::Series),
                PlanInput::CachedK | PlanInput::CachedV => W::Series,
            }
        };
        let need = |slot: usize, want: W, what: &str| -> Result<(), Qwen36PlanErrorV1> {
            let got = width_of(&inputs[slot]);
            if got != want {
                return Err(refuse(index, format!("input {slot} is {got:?} where {what} takes {want:?}")));
            }
            Ok(())
        };
        let width = |want: u32, what: &str| -> Result<(), Qwen36PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::Fixed { elements } if elements == want => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where {what} is {want}"))),
            }
        };
        let kv_scaled = |want_mult: u32| -> Result<(), Qwen36PlanErrorV1> {
            match node.out_len {
                PalwStepOutLenV1::KvScaled { multiplier } if multiplier == want_mult => Ok(()),
                other => Err(refuse(index, format!("out width {other:?} where the kv-scaled multiplier is {want_mult}"))),
            }
        };
        // The one width-preserving shape: in == out, both fixed.
        let preserving = |what: &str| -> Result<u32, Qwen36PlanErrorV1> {
            let got = width_of(&inputs[0]);
            let W::Fixed(w) = got else {
                return Err(refuse(index, format!("input 0 is {got:?} where {what} takes a fixed-width row")));
            };
            width(w, what)?;
            Ok(w)
        };
        let dtype_i8 = || -> Result<(), Qwen36PlanErrorV1> {
            if node.weight_dtypes.iter().all(|b| *b == QWEN36_WEIGHT_DTYPE_I8) {
                Ok(())
            } else {
                Err(refuse(index, "a weight dtype this family's kernels do not read".to_string()))
            }
        };

        let full_name = node.weight_name.as_str();
        let stripped = strip_layer(full_name);
        if !per_layer_ok && stripped.is_some() {
            return Err(refuse(index, format!("operand {full_name:?} names a per-layer row, and the {table} table has no layer")));
        }
        let name_ref = || -> NameRef {
            match stripped {
                Some(suffix) => NameRef::PerLayer(suffix.to_string()),
                None => NameRef::Global(full_name.to_string()),
            }
        };
        let kid = node.kernel_semantics_id;
        let op = match (node.op_kind, stripped, full_name) {
            (Op::EmbedLookup, None, "token_embd.weight") if kid == k_embed => {
                arity(0)?;
                width(d, "hidden")?;
                dtype_i8()?;
                PlanOpV1::EmbedGather
            }
            (Op::MulElem, None, "embed_lift.a16") if kid == k_req => {
                arity(1)?;
                width(d, "hidden")?;
                need(0, W::Fixed(d), "the lift")?;
                PlanOpV1::EmbedLift
            }
            (Op::RmsNorm, _, "") if kid == k_rms => {
                arity(1)?;
                preserving("the norm")?;
                PlanOpV1::RmsNormUnit
            }
            (Op::RmsNorm, _, "") if kid == k_head_rms => {
                arity(1)?;
                let w = preserving("the per-head norm")?;
                let heads = match w {
                    _ if w == q_dim && s.head_dim > 0 => s.n_heads,
                    _ if w == kv_dim && s.head_dim > 0 => s.n_kv_heads,
                    _ => return Err(refuse(index, format!("a per-head norm over {w} lanes is neither the q nor the kv row"))),
                };
                PlanOpV1::HeadRmsNorm { heads }
            }
            (Op::RmsNorm, Some("linear_norm_eps.a16"), _) if kid == k_rms_wide => {
                arity(1)?;
                let w = preserving("the recurrence's output norm")?;
                if w != dv {
                    return Err(refuse(index, format!("the recurrence's output norm is over {dv} lanes, not {w}")));
                }
                PlanOpV1::GdnHeadNormWide
            }
            (Op::MulElem, name, _) if kid == k_req => {
                arity(1)?;
                match name {
                    Some("attn_q_norm.a16") => {
                        let w = preserving("the q norm's requant")?;
                        if w != q_dim {
                            return Err(refuse(index, format!("the q norm's requant is over {q_dim} lanes, not {w}")));
                        }
                        PlanOpV1::HeadRequant { name: name_ref(), heads: s.n_heads }
                    }
                    Some("attn_k_norm.a16") => {
                        let w = preserving("the k norm's requant")?;
                        if w != kv_dim {
                            return Err(refuse(index, format!("the k norm's requant is over {kv_dim} lanes, not {w}")));
                        }
                        PlanOpV1::HeadRequant { name: name_ref(), heads: s.n_kv_heads }
                    }
                    Some("attn_probs.a16") => {
                        kv_scaled(heads)?;
                        need(0, W::KvScaled(heads), "the probs requant")?;
                        PlanOpV1::ProbsRequant { name: name_ref() }
                    }
                    Some(
                        "attn_norm.a16"
                        | "attn_align.a16"
                        | "attn_residual.a16"
                        | "ffn_norm.a16"
                        | "ffn_align.a16"
                        | "ffn_residual.a16"
                        | "ffn_moe_out.a16"
                        | "linear_conv_act.a16"
                        | "ffn_router.a16",
                    ) => {
                        preserving("a width-preserving requant")?;
                        PlanOpV1::RequantSized { name: name_ref() }
                    }
                    None if full_name == "final_norm.a16" => {
                        preserving("the final requant")?;
                        PlanOpV1::RequantSized { name: name_ref() }
                    }
                    _ => return Err(refuse(index, format!("requant operand {full_name:?} is not one this store names"))),
                }
            }
            (Op::MatMulQuant, name, _) if kid == k_grouped || kid == k_grouped_wide || kid == k_rescale_mm => {
                arity(1)?;
                dtype_i8()?;
                // The DECLARED wideness: the grouped-wide and rescale kernels leave the row in
                // Q[`K`], the grouped kernel narrows to codes. The interpreter executes what the
                // declaration says; the differential is what holds the declaration to the engine.
                let wide = kid != k_grouped;
                match name {
                    Some("ffn_gate_exps.routed" | "ffn_up_exps.routed" | "ffn_down_exps.routed") => {
                        let stage = match name {
                            Some("ffn_gate_exps.routed") => RoutedStage::Gate,
                            Some("ffn_up_exps.routed") => RoutedStage::Up,
                            _ => RoutedStage::Down,
                        };
                        let Some(topk) = last_topk else {
                            return Err(refuse(index, "a routed projection with no earlier RouterTopk to read".to_string()));
                        };
                        match stage {
                            RoutedStage::Down => {
                                width(k_of * d, "the routed outputs")?;
                                need(0, W::Fixed(k_of * moe), "the routed down projection")?;
                            }
                            _ => {
                                width(k_of * moe, "the routed intermediates")?;
                                need(0, W::Fixed(d), "a routed projection's fan-in")?;
                            }
                        }
                        PlanOpV1::RoutedProject { stage, wide, topk }
                    }
                    Some(suffix) => {
                        let (out_w, in_w) = match suffix {
                            "linear_q.weight" | "linear_k.weight" => (dk, d),
                            "linear_v.weight" | "linear_z.weight" => (dv, d),
                            "linear_dt.weight" | "linear_beta.weight" => (v_heads, d),
                            "linear_o.weight" => (d, dv),
                            "attn_q.weight" | "attn_gate.weight" => (q_dim, d),
                            "attn_k.weight" | "attn_v.weight" => (kv_dim, d),
                            "attn_o.weight" => (d, q_dim),
                            "ffn_router.weight" => (experts, d),
                            "ffn_shared_expert_gate.weight" | "ffn_shared_expert_up.weight" => (shared, d),
                            "ffn_shared_expert_down.weight" => (d, shared),
                            "ffn_shared_gate.weight" => (1, d),
                            _ => return Err(refuse(index, format!("matmul operand {full_name:?} is not one this store names"))),
                        };
                        width(out_w, "the projection's width")?;
                        need(0, W::Fixed(in_w), "this projection's fan-in")?;
                        PlanOpV1::Project { name: name_ref(), out: out_w as usize, wide }
                    }
                    None if full_name == "output.weight" => {
                        width(vocab, "the vocabulary")?;
                        need(0, W::Fixed(d), "the unembedding's fan-in")?;
                        PlanOpV1::Project { name: name_ref(), out: vocab as usize, wide }
                    }
                    _ => return Err(refuse(index, format!("matmul operand {full_name:?} is not one this store names"))),
                }
            }
            (Op::MatMulQuant, Some("attn_logits.a16"), _) if kid == k_scores => {
                arity(2)?;
                kv_scaled(heads)?;
                need(0, W::Fixed(q_dim), "the query")?;
                if inputs.get(1) != Some(&PlanInput::CachedK) {
                    return Err(refuse(index, "scores read something other than the key series".to_string()));
                }
                PlanOpV1::AttnScores { name: name_ref() }
            }
            (Op::MatMulQuant, Some("attn_values.a16"), _) if kid == k_vals => {
                arity(2)?;
                width(q_dim, "the attention output")?;
                need(0, W::KvScaled(heads), "the probabilities")?;
                if inputs.get(1) != Some(&PlanInput::CachedV) {
                    return Err(refuse(index, "values read something other than the value series".to_string()));
                }
                PlanOpV1::AttnValues { name: name_ref() }
            }
            (Op::SsmConv, Some("linear_conv.weight"), _) if kid == k_conv => {
                arity(3)?;
                width(conv_w, "the convolution row")?;
                dtype_i8()?;
                need(0, W::Fixed(dk), "the convolution's q block")?;
                need(1, W::Fixed(dk), "the convolution's k block")?;
                need(2, W::Fixed(dv), "the convolution's v block")?;
                PlanOpV1::SsmConv
            }
            (Op::Silu, _, "") if kid == k_silu => {
                arity(1)?;
                preserving("silu")?;
                PlanOpV1::Silu
            }
            (Op::Softplus, Some("linear_decay.a16"), _) if kid == k_decay => {
                arity(1)?;
                let w = preserving("the decay")?;
                if w != v_heads {
                    return Err(refuse(index, format!("the decay is one lane per value head ({v_heads}), not {w}")));
                }
                PlanOpV1::Decay
            }
            (Op::Sigmoid, _, "") if kid == k_sigmoid => {
                arity(1)?;
                preserving("sigmoid")?;
                PlanOpV1::SigmoidGate
            }
            (Op::L2Norm, _, "") if kid == k_l2 => {
                arity(1)?;
                width(dk, "the normalized heads")?;
                need(0, W::Fixed(conv_w), "the convolution row")?;
                // The segment is the recurrence's to assign; see the post-pass below.
                PlanOpV1::L2NormSeg { seg: GdnSeg::Unassigned }
            }
            (Op::GatedDeltaNet, Some("linear_gdn.a16"), _) if kid == k_gdn => {
                arity(5)?;
                width(dv, "the recurrence's output")?;
                need(0, W::Fixed(dk), "the normalized key")?;
                need(1, W::Fixed(conv_w), "the convolution row")?;
                need(2, W::Fixed(dk), "the normalized query")?;
                need(3, W::Fixed(v_heads), "the decay row")?;
                need(4, W::Fixed(v_heads), "the beta row")?;
                if s.linear_k_heads == 0 || s.linear_head_dim == 0 {
                    return Err(refuse(index, "a recurrence in a geometry with no recurrent heads".to_string()));
                }
                PlanOpV1::GdnStep
            }
            (Op::Scale, Some("linear_norm.a16"), _) if kid == k_rescale_row => {
                arity(1)?;
                let w = preserving("the output rescale")?;
                if w != dv {
                    return Err(refuse(index, format!("the recurrence's rescale is over {dv} lanes, not {w}")));
                }
                PlanOpV1::ScaleRow { name: name_ref() }
            }
            (Op::MulElem, name, _) if kid == k_mul_wide => {
                arity(2)?;
                match name {
                    Some("linear_gated.a16") => {
                        let w = preserving("the gated multiply")?;
                        need(1, W::Fixed(w), "the gate")?;
                        PlanOpV1::MulWide { name: name_ref() }
                    }
                    Some("ffn_expert_gated.a16") => {
                        width(k_of * moe, "the routed intermediates")?;
                        need(0, W::Fixed(k_of * moe), "the activated gates")?;
                        need(1, W::Fixed(k_of * moe), "the up rows")?;
                        let Some(topk) = last_topk else {
                            return Err(refuse(index, "a routed multiply with no earlier RouterTopk to read".to_string()));
                        };
                        PlanOpV1::RoutedGatedMul { topk }
                    }
                    Some("ffn_shared_expert_gated.a16") => {
                        width(shared, "the shared intermediate")?;
                        need(0, W::Fixed(shared), "the activated gate")?;
                        need(1, W::Fixed(shared), "the up row")?;
                        PlanOpV1::SharedGatedMul
                    }
                    Some("ffn_shared_gated.a16") => {
                        width(d, "the gated shared row")?;
                        need(0, W::Fixed(d), "the shared row")?;
                        need(1, W::Fixed(1), "the scalar gate")?;
                        PlanOpV1::SharedApply { name: name_ref() }
                    }
                    _ => return Err(refuse(index, format!("wide-multiply operand {full_name:?} is not one this store names"))),
                }
            }
            (Op::MulElem, Some("attn_gated.a16"), _) if kid == k_gate_apply => {
                arity(2)?;
                width(q_dim, "the gated attention row")?;
                need(0, W::Fixed(q_dim), "the attention row")?;
                need(1, W::Fixed(q_dim), "the gate")?;
                PlanOpV1::GateApply { name: name_ref() }
            }
            (Op::MulElem, Some("ffn_combine.a16"), _) if kid == k_combine => {
                arity(2)?;
                width(d, "the combined mixture")?;
                need(0, W::Fixed(k_of * d), "the routed outputs")?;
                need(1, W::Fixed(2 * k_of), "the routing row")?;
                PlanOpV1::MoeCombine { name: name_ref(), k: k_of as usize }
            }
            (Op::RopeImrope, Some("attn_rope.a16"), _) if kid == k_rope => {
                arity(1)?;
                let w = preserving("the rotation")?;
                if w != q_dim && w != kv_dim {
                    return Err(refuse(index, format!("a rotation over {w} lanes is neither the q nor the kv row")));
                }
                PlanOpV1::RopePartial { name: name_ref() }
            }
            (Op::SoftMax, Some("ffn_router_up.a16"), _) if kid == k_topk => {
                arity(1)?;
                width(2 * k_of, "two lanes per chosen expert")?;
                need(0, W::Fixed(experts), "the router codes")?;
                last_topk = Some(index);
                PlanOpV1::RouterTopk { name: name_ref(), k: k_of as usize }
            }
            (Op::SoftMax, Some("attn_softmax_up.a16"), _) if kid == k_soft => {
                arity(1)?;
                kv_scaled(heads)?;
                need(0, W::KvScaled(heads), "the scores")?;
                PlanOpV1::Softmax { name: name_ref() }
            }
            (Op::AddElem, _, "") if kid == k_add => {
                arity(2)?;
                let w = preserving("the sum")?;
                need(1, W::Fixed(w), "the sum's other side")?;
                PlanOpV1::AddElem
            }
            _ => {
                return Err(refuse(
                    index,
                    format!(
                        "op {:?} under kernel {:?} with operand {full_name:?} is not arithmetic this build serves",
                        node.op_kind, kid
                    ),
                ));
            }
        };
        widths.push(match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => W::Fixed(elements),
            PalwStepOutLenV1::KvScaled { multiplier } => W::KvScaled(multiplier),
        });
        out.push(PlanNode { op, inputs, role: node.role });
    }

    // The recurrence's slots assign the L2 segments: input 0 is its KEY operand and input 2 its
    // QUERY operand, so the nodes those slots reference norm the matching convolution segment. A
    // declared L2 norm no recurrence consumes has no defined segment and is refused — an
    // interpreter that guessed would be executing an operand the declaration does not determine.
    let mut segs: Vec<GdnSeg> = out.iter().map(|_| GdnSeg::Unassigned).collect();
    for node in &out {
        if !matches!(node.op, PlanOpV1::GdnStep) {
            continue;
        }
        for (slot, seg) in [(0usize, GdnSeg::Key), (2usize, GdnSeg::Query)] {
            let PlanInput::Row(i) = node.inputs[slot] else { continue };
            if !matches!(out[i].op, PlanOpV1::L2NormSeg { .. }) {
                continue;
            }
            if segs[i] != GdnSeg::Unassigned && segs[i] != seg {
                return Err(refuse(i, "one L2 norm is read as both the key and the query".to_string()));
            }
            segs[i] = seg;
        }
    }
    for (i, node) in out.iter_mut().enumerate() {
        if let PlanOpV1::L2NormSeg { seg } = &mut node.op {
            if segs[i] == GdnSeg::Unassigned {
                return Err(refuse(i, "an L2 norm no recurrence consumes has no defined segment".to_string()));
            }
            *seg = segs[i];
        }
    }
    Ok(out)
}

/// The geometry a fixture's shape registers as — for tests in this crate. `interval` is the
/// profile's spelling of the layer alternation; the fixtures use the family's own rule, so 4
/// reproduces the hybrid stack and 1 the all-attention one.
#[cfg(test)]
pub(crate) fn fixture_geometry_of(
    s: &Qwen36ShapeV1,
    interval: u16,
) -> kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
    kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
        layer_count: s.n_layers() as u16,
        full_attention_interval: interval,
        hidden_dim: s.d_model as u32,
        attn_heads: s.n_heads as u16,
        attn_kv_heads: s.n_kv_heads as u16,
        attn_head_dim: s.head_dim as u32,
        rope_dims: s.rotary_dim as u16,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: s.linear_k_heads as u16,
        gdn_v_heads: s.linear_v_heads as u16,
        gdn_head_dim: s.linear_head_dim as u32,
        gdn_conv_kernel: s.conv_kernel as u16,
        n_experts: s.n_experts as u32,
        experts_per_token: s.experts_per_token as u32,
        moe_dim: s.moe_dim as u32,
        shared_dim: s.shared_dim as u32,
        attn_output_gate: if s.attn_output_gate() { 1 } else { 0 },
        vocab_size: s.vocab as u32,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: s.eps_q,
        tile_len: 512,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qwen36::{Qwen36Cache, qwen3moe_dev_fixture, qwen36_dev_fixture, test_fixture_for_shape};
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v1, qwen36_profile_v2};

    fn geometry_of(s: &Qwen36ShapeV1, interval: u16) -> PalwQwen36GeometryV1 {
        super::fixture_geometry_of(s, interval)
    }

    /// Both passes over a fresh cache each, compared to the bit at every position.
    fn differential(artifact: &crate::qwen36::Qwen36ArtifactV1, interval: u16, positions: usize) {
        let engine = Qwen36Engine::new(artifact);
        let profile = qwen36_profile_v2(geometry_of(&artifact.shape, interval)).expect("the corrected profile projects");
        let plan = engine.plan_from_profile(&profile).expect("the corrected profile is servable");
        let mut compiled_cache = Qwen36Cache::new(&artifact.shape);
        let mut planned_cache = Qwen36Cache::new(&artifact.shape);
        for position in 0..positions {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            let (a, _) = engine.forward_token_probed(&mut compiled_cache, token, position).expect("compiled");
            let (b, _) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
            assert_eq!(a, b, "logits at position {position}");
        }
        assert_eq!(compiled_cache.keys, planned_cache.keys, "the key caches must be the same state");
        assert_eq!(compiled_cache.values, planned_cache.values, "the value caches must be the same state");
        assert_eq!(compiled_cache.conv, planned_cache.conv, "the convolution windows must be the same state");
        for (li, (a, b)) in compiled_cache.gdn.iter().zip(planned_cache.gdn.iter()).enumerate() {
            assert_eq!(a, b, "layer {li}'s recurrent state must be the same");
        }
    }

    /// **ADR-0067's differential gate for the mmap container: the compiled engine is the
    /// interpreter's reference vector.** The plan is compiled from `graph-v2` — the row that
    /// names what the engine does — so walking it must land on the compiled engine's exact bits:
    /// logits, the caches left behind, and (below) the committed rows at every probe site.
    #[test]
    fn the_interpreter_and_the_compiled_engine_agree_bit_for_bit() {
        differential(&qwen36_dev_fixture(8, 16), 4, 6);
    }

    /// The all-attention (qwen3moe) flavor through the same plan machinery: the stripped v2
    /// table — no gate, no shared expert, no recurrence — walks against the same engine.
    #[test]
    fn the_qwen3moe_flavor_agrees_bit_for_bit() {
        differential(&qwen3moe_dev_fixture(3, 8), 1, 6);
    }

    /// The producer's untraced walk is the traced one — same logits, same cache left behind — or
    /// the two entries have drifted and the differential above vouches for only one of them.
    #[test]
    fn the_untraced_walk_is_the_traced_one() {
        let artifact = qwen36_dev_fixture(4, 8);
        let engine = Qwen36Engine::new(&artifact);
        let profile = qwen36_profile_v2(geometry_of(&artifact.shape, 4)).expect("projects");
        let plan = engine.plan_from_profile(&profile).expect("servable");
        let mut traced_cache = Qwen36Cache::new(&artifact.shape);
        let mut untraced_cache = Qwen36Cache::new(&artifact.shape);
        for position in 0..5usize {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            let (a, _) = engine.forward_token_planned(&plan, &mut traced_cache, token, position).expect("traced");
            let b = engine.forward_token_planned_logits(&plan, &mut untraced_cache, token, position).expect("untraced");
            assert_eq!(a, b, "logits at position {position}");
        }
        assert_eq!(traced_cache.keys, untraced_cache.keys);
        assert_eq!(traced_cache.values, untraced_cache.values);
        assert_eq!(traced_cache.conv, untraced_cache.conv);
        assert_eq!(traced_cache.gdn, untraced_cache.gdn);
    }

    /// **The committed rows, at every site the compiled engine can name.** The logits agreeing
    /// could survive two errors that cancel; the A16 differential caught its real defect (rows
    /// out of declared order) only because it compared rows. The engine has no per-node trace —
    /// the probe is its row-level voice — so each probe site is pinned to the v2 node index it
    /// must equal. The indices double as a structure pin: they move only when the table does.
    #[test]
    fn every_probe_site_lands_on_its_declared_node() {
        let artifact = qwen36_dev_fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let profile = qwen36_profile_v2(geometry_of(&artifact.shape, 4)).expect("projects");
        let plan = engine.plan_from_profile(&profile).expect("servable");
        let k = artifact.shape.experts_per_token;
        let mut compiled_cache = Qwen36Cache::new(&artifact.shape);
        let mut planned_cache = Qwen36Cache::new(&artifact.shape);
        for position in 0..4usize {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            let (_, probe) = engine.forward_token_probed(&mut compiled_cache, token, position).expect("compiled");
            let (_, trace) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
            let probe: std::collections::BTreeMap<String, Vec<i32>> = probe.into_iter().collect();
            let site = |name: String| probe.get(&name).unwrap_or_else(|| panic!("the engine probes {name}")).clone();

            assert_eq!(site("embed".into()), trace.pre[1], "the lifted embedding");
            for (li, kind) in artifact.shape.layer_types.iter().enumerate() {
                let rows = &trace.layers[li];
                let n = |suffix: &str| format!("blk.{li}.{suffix}");
                // `(probe site, v2 node index)`, per arm; the mixture's sites ride at the arm's
                // own offset. `linear_out` is the arm's output in both arms.
                let (pairs, first): (&[(&str, usize)], usize) = match kind {
                    Qwen36LayerKind::LinearAttention => (
                        &[
                            ("attn_norm", 1),
                            ("linear_z", 5),
                            ("linear_conv", 8),
                            ("linear_decay", 11),
                            ("linear_beta_gate", 12),
                            ("linear_state_out", 15),
                            ("linear_normed", 17),
                            ("linear_gate_act", 18),
                            ("linear_gated", 19),
                            ("linear_out", 20),
                            ("attn_residual", 23),
                        ],
                        24,
                    ),
                    Qwen36LayerKind::FullAttention => (
                        &[
                            ("attn_norm", 1),
                            ("attn_q", 2),
                            ("attn_gate", 3),
                            ("attn_v", 5),
                            ("attn_q_rot", 10),
                            ("attn_k_rot", 11),
                            ("attn_values", 15),
                            ("attn_gated", 17),
                            ("attn_out", 18),
                            ("attn_residual", 21),
                        ],
                        22,
                    ),
                };
                for (suffix, index) in pairs {
                    assert_eq!(site(n(suffix)), rows[*index], "layer {li} {suffix} at position {position}");
                }
                if *kind == Qwen36LayerKind::LinearAttention {
                    let qkv: Vec<i32> = rows[2].iter().chain(&rows[3]).chain(&rows[4]).copied().collect();
                    assert_eq!(site(n("linear_qkv")), qkv, "layer {li} qkv at position {position}");
                }
                for (suffix, offset) in [
                    ("ffn_norm", 1),
                    ("ffn_router", 3),
                    ("ffn_expert_out", 9),
                    ("ffn_routed", 10),
                    ("ffn_shared_out", 15),
                    ("ffn_moe_out", 20),
                ] {
                    assert_eq!(site(n(suffix)), rows[first + offset], "layer {li} {suffix} at position {position}");
                }
                let topk = &rows[first + 4];
                assert_eq!(site(n("ffn_choice")), topk[..k].to_vec(), "layer {li} routing ids at position {position}");
                assert_eq!(site(n("ffn_weight")), topk[k..].to_vec(), "layer {li} routing weights at position {position}");
            }
            assert_eq!(site("final_norm".into()), trace.post[1], "the final norm");
            assert_eq!(site("logits".into()), trace.post[2], "the logit row");
        }
    }

    /// **The kernel labels have to be RIGHT, and this is the test with the teeth to say so.**
    ///
    /// The plain differential above passed even while v2 still carried v1's backwards expert
    /// wideness (gate narrow, up wide — the engine does the opposite), because the fixture's
    /// derived scales keep every row inside the 16-bit code rail, where the narrow and wide
    /// matmuls agree bit for bit. A differential that passes either way at a site has no
    /// authority there. So this one heats every projection the corrected labels describe —
    /// gates AND ups, routed AND shared — and then:
    ///
    /// * the interpreter must still match the engine's bits (under swapped labels it cannot: a
    ///   declared-narrow gate clamps where the engine's wide one carries — measured before the
    ///   label fix, logits diverged at position 0 — and a declared-wide up carries where the
    ///   engine's narrow one clamps);
    /// * every heated site must prove it is hot SEPARATELY, in both arms — a wide row by lanes
    ///   past the rail, a narrow row by lanes saturated at it — or that site went cold and
    ///   quietly lost its authority, which is how the fourth defect survived the plain
    ///   differential and how the shared sites would have survived the first version of this
    ///   test, which heated and counted only the gates.
    #[test]
    fn a_hot_gate_row_distinguishes_the_declared_wideness() {
        use kaspa_consensus_core::palw_base0_a16::A16_CODE_MAX;
        let mut artifact = qwen36_dev_fixture(4, 8);
        let hot = kaspa_consensus_core::palw_base0_a16::A16QuantParams { multiplier: 1, shift: 2, zero: 0 };
        for li in 0..artifact.shape.n_layers() {
            for e in 0..artifact.shape.n_experts {
                artifact = artifact
                    .with_params(format!("blk.{li}.ffn_expert.{e}_gate.weight.a16"), &[hot])
                    .with_params(format!("blk.{li}.ffn_expert.{e}_up.weight.a16"), &[hot]);
            }
            artifact = artifact
                .with_params(format!("blk.{li}.ffn_shared_expert_gate.weight.a16"), &[hot])
                .with_params(format!("blk.{li}.ffn_shared_expert_up.weight.a16"), &[hot]);
        }
        let engine = Qwen36Engine::new(&artifact);
        let profile = qwen36_profile_v2(geometry_of(&artifact.shape, 4)).expect("projects");
        let plan = engine.plan_from_profile(&profile).expect("servable");
        let mut compiled_cache = Qwen36Cache::new(&artifact.shape);
        let mut planned_cache = Qwen36Cache::new(&artifact.shape);
        // [arm][site] hot-lane counts. The mixture starts at 24 in the GDN table and 22 in the
        // attention one; gate/up sit at +5/+6 (routed) and +11/+12 (shared).
        let mut hot_lanes = [[0usize; 4]; 2];
        for position in 0..4usize {
            let token = (position * 7 + 3) % artifact.shape.vocab;
            let (a, _) = engine.forward_token_probed(&mut compiled_cache, token, position).expect("compiled");
            let (b, trace) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
            assert_eq!(a, b, "logits at position {position} under hot expert projections");
            for (li, kind) in artifact.shape.layer_types.iter().enumerate() {
                let (arm, first) = match kind {
                    Qwen36LayerKind::LinearAttention => (0usize, 24usize),
                    Qwen36LayerKind::FullAttention => (1usize, 22usize),
                };
                for (site, offset, wide) in [(0usize, 5usize, true), (1, 6, false), (2, 11, true), (3, 12, false)] {
                    let row = &trace.layers[li][first + offset];
                    hot_lanes[arm][site] += row
                        .iter()
                        .filter(|v| if wide { (**v as i64).abs() > A16_CODE_MAX } else { (**v as i64).abs() == A16_CODE_MAX })
                        .count();
                }
            }
        }
        for (arm, arm_name) in [(0usize, "gdn"), (1, "attn")] {
            for (site, site_name, how) in [
                (0usize, "routed gate", "left the code rail"),
                (1, "routed up", "saturated at the code rail"),
                (2, "shared gate", "left the code rail"),
                (3, "shared up", "saturated at the code rail"),
            ] {
                assert!(
                    hot_lanes[arm][site] > 0,
                    "no {arm_name} {site_name} lane {how} — the site went cold and the differential lost its teeth there"
                );
            }
        }
    }

    /// **The v1 graph is refusable, and refused for its measured reasons.** The conformance
    /// suite convicted v1's names against the artifact; the planner is where that conviction
    /// becomes mechanical — a node reading the v1 declaration cannot be built, which is exactly
    /// why `graph-v2` exists and why the interpreter follows it and never v1.
    #[test]
    fn the_v1_graph_is_refused_by_name() {
        let artifact = qwen36_dev_fixture(8, 16);
        let engine = Qwen36Engine::new(&artifact);
        let profile = qwen36_profile_v1(geometry_of(&artifact.shape, 4)).expect("v1 projects");
        match engine.plan_from_profile(&profile) {
            Err(Qwen36PlanErrorV1::UnservedNode { reason, .. }) => {
                assert!(!reason.is_empty(), "a refusal names its reason");
            }
            other => panic!("the v1 declaration must be unservable, got {other:?}"),
        }
    }

    /// Same input, same bits, across two walks of the same plan — the recurrent state included.
    #[test]
    fn the_planned_pass_is_deterministic() {
        let artifact = qwen36_dev_fixture(4, 8);
        let engine = Qwen36Engine::new(&artifact);
        let profile = qwen36_profile_v2(geometry_of(&artifact.shape, 4)).expect("projects");
        let plan = engine.plan_from_profile(&profile).expect("servable");
        let run = || {
            let mut cache = Qwen36Cache::new(&artifact.shape);
            let mut last = Vec::new();
            for position in 0..6 {
                let (logits, _) =
                    engine.forward_token_planned(&plan, &mut cache, (position * 5 + 1) % 64, position).expect("completes");
                last = logits;
            }
            let states: Vec<Vec<i32>> = cache.gdn.iter().flatten().map(|st| st.s.clone()).collect();
            (last, states)
        };
        assert_eq!(run(), run());
    }

    /// **The differential on REAL WEIGHTS** — a converted `.palwq36` artifact of a ledger class,
    /// which is what the reduced fixtures cannot be: calibrated narrowings that actually bind
    /// their clamps, and per-32 group exponents that route every projection through the grouped
    /// kernels the fixtures never touch. Gated on `MISAKA_QWEN36_REAL_ARTIFACT` (a path to the
    /// artifact) because a checkout does not carry gigabytes; run it wherever an artifact lives,
    /// in release, and the interpreter must land on the compiled engine's exact bits.
    #[test]
    fn the_interpreter_matches_the_engine_on_a_real_artifact() {
        let Ok(path) = std::env::var("MISAKA_QWEN36_REAL_ARTIFACT") else {
            eprintln!("skipped: set MISAKA_QWEN36_REAL_ARTIFACT to a converted .palwq36 to run the real-weights differential");
            return;
        };
        let artifact = crate::qwen36::open_artifact(std::path::Path::new(&path)).expect("the artifact opens");
        let row = crate::classes::qwen36_canonical_classes_v1()
            .into_iter()
            .find(|c| c.graph_version >= 2 && c.shape_matches(&artifact.shape).is_ok())
            .expect("the artifact's shape matches a corrected ledger row");
        eprintln!("real-weights differential against {}", row.model_id);
        let engine = Qwen36Engine::new(&artifact);

        // **The fifth misdescription, found here and since disposed.** When this differential
        // first ran, every registered geometry declared `rms_eps_q = 17` while the converter
        // hardcodes `eps_q = 1` into every artifact header and the engine normalizes with the
        // artifact's — so the planner's geometry gate refused the row over its own class's
        // weights: right answer, wrong world. The graph-v3 rows now declare the artifact's
        // epsilon (`QWEN36_ARTIFACT_EPS_Q`, measured five ways), so the row plans directly below;
        // what stays pinned here is the refusal itself — a declaration carrying the old 17 must
        // still refuse on exactly that field and no other.
        let mut undisposed = row.geometry;
        undisposed.rms_eps_q = 17;
        let seventeen = qwen36_profile_v2(undisposed).expect("the undisposed geometry projects");
        match engine.plan_from_profile(&seventeen) {
            Err(Qwen36PlanErrorV1::GeometryMismatch { what: "rms_eps_q", profile, artifact: got }) => {
                eprintln!("a rms_eps_q {profile} declaration refuses against the artifact's {got} — the fifth finding, pinned");
            }
            other => panic!("an epsilon the artifact does not execute must refuse on rms_eps_q and nothing else, got {other:?}"),
        }

        let profile = row.profile().expect("the row projects");
        let plan = engine.plan_from_profile(&profile).expect("the graph-v3 row is servable over its own class's weights");
        let mut compiled_cache = Qwen36Cache::new(&artifact.shape);
        let mut planned_cache = Qwen36Cache::new(&artifact.shape);
        for (position, token) in [9_000usize, 42, 777].into_iter().enumerate() {
            let token = token % artifact.shape.vocab;
            let (a, _) = engine.forward_token_probed(&mut compiled_cache, token, position).expect("compiled");
            let (b, _) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
            assert_eq!(a.len(), artifact.shape.vocab);
            assert_eq!(a, b, "logits at position {position} on real weights");
        }
        assert_eq!(compiled_cache.keys, planned_cache.keys, "key caches on real weights");
        assert_eq!(compiled_cache.values, planned_cache.values, "value caches on real weights");
        assert_eq!(compiled_cache.conv, planned_cache.conv, "convolution windows on real weights");
        assert_eq!(compiled_cache.gdn, planned_cache.gdn, "recurrent state on real weights");
    }

    /// **ADR-0067 Decision 5, clause (b), for the mmap container: the differential over the
    /// classes THIS BUILD carries.** Every qwen36-family ledger row's graph is compiled at a
    /// reduced geometry that keeps the row's structure (the layer alternation, the gate, the
    /// shared expert, the expert counts) and walked against the compiled engine, bit for bit. A
    /// corrected (`graph-v3`) row must be servable; a v1 row must be refused. The real profiles
    /// themselves are additionally held to the geometry gate: against a mismatched artifact the
    /// planner must refuse on geometry, never accept.
    #[test]
    fn the_interpreter_serves_every_qwen36_class_this_build_carries() {
        let rows = crate::classes::qwen36_canonical_classes_v1();
        assert!(!rows.is_empty(), "the build carries qwen36 rows, or this test gates nothing");
        let mut served = 0usize;
        for row in &rows {
            let corrected = row.graph_version >= 2;
            let g = row.geometry;
            let hybrid = g.full_attention_interval != 1;
            let reduced = PalwQwen36GeometryV1 {
                layer_count: g.layer_count.min(8),
                full_attention_interval: g.full_attention_interval,
                hidden_dim: 32,
                attn_heads: 4,
                attn_kv_heads: 2,
                attn_head_dim: 16,
                // A full-rotation member stays full-rotation; a partial one stays partial.
                rope_dims: if g.rope_dims as u32 == g.attn_head_dim { 16 } else { 4 },
                rope_freq_base_bits: g.rope_freq_base_bits,
                gdn_k_heads: if hybrid { 2 } else { 0 },
                gdn_v_heads: if hybrid { 4 } else { 0 },
                gdn_head_dim: if hybrid { 8 } else { 0 },
                gdn_conv_kernel: if hybrid { 4 } else { 0 },
                n_experts: g.n_experts.min(16),
                experts_per_token: g.experts_per_token.min(4),
                moe_dim: 16,
                shared_dim: if g.shared_dim > 0 { 16 } else { 0 },
                attn_output_gate: g.attn_output_gate,
                vocab_size: 64,
                n_ctx: g.n_ctx,
                n_threads: 1,
                rms_eps_q: 1,
                tile_len: g.tile_len,
            };
            let shape = Qwen36ShapeV1 {
                layer_types: (0..reduced.layer_count as usize)
                    .map(|i| {
                        if reduced.full_attention_interval != 0 && (i + 1).is_multiple_of(reduced.full_attention_interval as usize) {
                            Qwen36LayerKind::FullAttention
                        } else {
                            Qwen36LayerKind::LinearAttention
                        }
                    })
                    .collect(),
                d_model: reduced.hidden_dim as usize,
                n_heads: reduced.attn_heads as usize,
                n_kv_heads: reduced.attn_kv_heads as usize,
                head_dim: reduced.attn_head_dim as usize,
                rotary_dim: reduced.rope_dims as usize,
                linear_k_heads: reduced.gdn_k_heads as usize,
                linear_v_heads: reduced.gdn_v_heads as usize,
                linear_head_dim: reduced.gdn_head_dim as usize,
                conv_kernel: reduced.gdn_conv_kernel as usize,
                n_experts: reduced.n_experts as usize,
                experts_per_token: reduced.experts_per_token as usize,
                moe_dim: reduced.moe_dim as usize,
                shared_dim: reduced.shared_dim as usize,
                vocab: reduced.vocab_size as usize,
                max_position: 32,
                eps_q: reduced.rms_eps_q,
                router_up_bits: 20,
            };
            let artifact = test_fixture_for_shape(shape);
            let engine = Qwen36Engine::new(&artifact);

            // The row's REAL profile against this small artifact: the geometry gate must fire.
            let real = row.profile().expect("the row's geometry projects");
            match engine.plan_from_profile(&real) {
                Err(Qwen36PlanErrorV1::GeometryMismatch { .. }) => {}
                other => panic!("{}: the real profile against a small artifact must refuse on geometry, got {other:?}", row.model_id),
            }

            // The same graph at the runnable geometry.
            let small = if corrected { qwen36_profile_v2(reduced) } else { qwen36_profile_v1(reduced) };
            let small = small.expect("the reduced geometry projects");
            match engine.plan_from_profile(&small) {
                Ok(plan) => {
                    assert!(corrected, "{}: a v1 row must be refused, and this one planned", row.model_id);
                    served += 1;
                    let mut compiled_cache = Qwen36Cache::new(&artifact.shape);
                    let mut planned_cache = Qwen36Cache::new(&artifact.shape);
                    for position in 0..4usize {
                        let token = (position * 11 + 5) % artifact.shape.vocab;
                        let (a, _) = engine.forward_token_probed(&mut compiled_cache, token, position).expect("compiled");
                        let (b, _) = engine.forward_token_planned(&plan, &mut planned_cache, token, position).expect("planned");
                        assert_eq!(a, b, "{}: logits at position {position}", row.model_id);
                    }
                    assert_eq!(compiled_cache.keys, planned_cache.keys, "{}: key caches", row.model_id);
                    assert_eq!(compiled_cache.values, planned_cache.values, "{}: value caches", row.model_id);
                    assert_eq!(compiled_cache.gdn, planned_cache.gdn, "{}: recurrent state", row.model_id);
                }
                Err(Qwen36PlanErrorV1::UnservedNode { reason, .. }) => {
                    assert!(!corrected, "{}: a corrected row must be servable, refused: {reason}", row.model_id);
                }
                Err(other) => panic!("{}: unexpected plan answer {other}", row.model_id),
            }
        }
        assert_eq!(served, 3, "the three graph-v3 rows are the servable half of the ledger");
    }
}
