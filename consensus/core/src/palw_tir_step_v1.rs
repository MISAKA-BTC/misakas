//! **RFC-0002 Phase F, step F4: the step space of an IR class** — the leaves a producer commits,
//! in the one order a producer, a seat, the bisection and the court all enumerate
//! (`docs/design/palw/tir/phase-f-integration.md` §2.5–§2.6).
//!
//! A job of `P` prompt positions and `D` generated tokens runs the program at the absolute positions
//! `a = 0 … P + D − 2`: prefill position `p` is `a = p` at the legacy coordinate `(call 0, position
//! p)`, decode call `c ≥ 1` is `a = P + c − 1` at `(call c, position 0)`. Per position, in this
//! order, before anything of position `a + 1`:
//!
//! 1. **commit points** — every occurrence of the schedule (`pre`, each layer's block, `post`), and in
//!    each every node with `commit = true`, in node order, cut into tiles of the layout's `tile_len`
//!    over its output at `H = min(a + 1, W)`. `node_slot` is the node's unrolled index (spec 04b
//!    §3.3), so a slot names one node for everybody. `post` runs only where logits are consumed —
//!    `a ≥ P − 1` — which is exact because `post` writes no state (enforced below);
//! 2. **`Fixed` state checkpoints** — when `(a + 1) % C == 0`, each `Fixed` state instance (state
//!    order, then layer) after this position's write, in tiles of its `state_tiles` lanes, at the
//!    reserved slots past the program's last;
//! 3. **history tiles** — when `(a + 1) % h_tile == 0`, each `Hist` state instance's last `h_tile`
//!    rows, one leaf per sub-row of `state_tiles` lanes, at the next reserved slots.
//!
//! **The invariant this order buys** (design D7): every leaf is adjudicated from leaves that precede
//! it — a cone reads lower slots of its position and rows, tiles and checkpoints of earlier ones; a
//! checkpoint replays from the previous checkpoint over the positions between; a history tile
//! concatenates rows already committed. The bisection narrows to the FIRST divergent leaf, whose
//! predecessors all agree with the honest execution, so a forged checkpoint or history tile is
//! itself that leaf — there is no separate checkpoint leg for an IR class and no second court.
//!
//! **Lanes are 4 bytes** (PALW-TIR-5): `i8`/`i16`/`i32` values as little-endian `i32`, `idx` as
//! little-endian `u32`. A leaf is the legacy [`PalwStepTileLeafV1`], hashed with the IR class id
//! where a legacy leaf hashes its shape profile id.
//!
//! **Counting is closed form**: the leaves of a position are a sum over committed nodes of
//! `⌈f·H^k / t⌉` with `H = min(a + 1, W)`, and the running total is a floor-sum per node plus two
//! periodic terms, so the count and the leaf at an index are found without walking the context.

use crate::Hash64;
use crate::palw_step::{PALW_STEP_MAX_TILE_LEN, PALW_STEP_MIN_TILE_LEN, PalwStepCoordinateV1};
use crate::palw_step_leg::{PalwStepTileLeafV1, step_leg_root_v1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use crate::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::prim::Prim;
use misaka_palw_tir::program::{Ref, StateKind, TirProgramV1};
use misaka_palw_tir::types::{DType, Dim};
use misaka_palw_tir::validate::ProgramInfo;

/// The version a TIR step leaf's preimage carries (the legacy tile leaf's field).
pub const PALW_TIR_STEP_LEAF_VERSION_V1: u16 = 1;
/// Key of [`palw_tir_execution_root_v1`] — its own, so an IR execution root never verifies as a
/// legacy one.
pub const PALW_TIR_EXECUTION_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/execution-root/v1";
/// The version of [`PalwTirStepBindingV1`].
pub const PALW_TIR_STEP_BINDING_VERSION_V1: u16 = 1;
/// The widest canonical history tile.
pub const PALW_TIR_MAX_H_TILE_V1: u32 = 4096;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirStepError {
    #[error("the IR program is not a program in normal form: {0}")]
    Program(String),
    #[error("the layout is not this program's: {0}")]
    Layout(String),
    #[error("the job is not one this class can run: {0}")]
    Job(&'static str),
    #[error("{got} step leaves exceed the ladder's {max}")]
    TooManyLeaves { got: u64, max: u64 },
    #[error("the leaf is not the next canonical one: {0}")]
    NotCanonical(String),
    #[error("the binding does not verify: {0}")]
    Binding(&'static str),
}

fn layout_err<T>(msg: impl Into<String>) -> Result<T, PalwTirStepError> {
    Err(PalwTirStepError::Layout(msg.into()))
}

#[derive(Clone, Debug)]
struct CommitNodeV1 {
    node: u16,
    dtype: DType,
    /// The product of the output's fixed dimensions.
    fixed_elements: u64,
    /// Whether the output has the `H` dimension (at most one: spec 04b §2.2).
    has_h: bool,
    tile_len: u32,
}

impl CommitNodeV1 {
    fn elements(&self, h: u64) -> u64 {
        if self.has_h { self.fixed_elements.saturating_mul(h) } else { self.fixed_elements }
    }
    fn tiles(&self, h: u64) -> u64 {
        self.elements(h).div_ceil(self.tile_len as u64)
    }
}

#[derive(Clone, Debug)]
struct BlockSpaceV1 {
    window: Option<u32>,
    commits: Vec<CommitNodeV1>,
}

/// One instance of a state — `(state, layer)`, `layer` `None` for a global state — as the step
/// space enumerates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirStateInstanceV1 {
    pub state: u16,
    pub layer: Option<u16>,
    pub dtype: DType,
    /// `Fixed`: the state's element count. `Hist`: one row's element count.
    pub elements: u64,
    /// `Fixed`: lanes per checkpoint tile. `Hist`: lanes per sub-row.
    pub tile_lanes: u32,
}

impl PalwTirStateInstanceV1 {
    fn tiles(&self) -> u64 {
        self.elements.div_ceil(self.tile_lanes as u64)
    }
}

/// What a leaf is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirLeafKindV1 {
    /// A tile of committed node `node` of occurrence `occurrence` (`(block, layer)`): output elements
    /// `first_element ..` (row-major at the position's `H`).
    Commit { occurrence: u32, block: u8, layer: Option<u16>, node: u16, first_element: u64 },
    /// A tile of `Fixed` state instance `instance`'s value after this position (a checkpoint):
    /// elements `first_element ..` of the flattened state.
    State { instance: u32, state: u16, layer: Option<u16>, first_element: u64 },
    /// Sub-row lanes `first_lane .. first_lane + row_lanes` of each of the `h_tile` rows of `Hist`
    /// state instance `instance` appended at positions `first_position ..= this position`, rows
    /// oldest first.
    HistTile { instance: u32, state: u16, layer: Option<u16>, first_lane: u64, row_lanes: u32, first_position: u32 },
}

/// One leaf of the step space: where it sits and what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirLeafV1 {
    pub index: u64,
    pub coord: PalwStepCoordinateV1,
    /// The absolute position `a`.
    pub position: u32,
    pub value_count: u32,
    /// The dtype every lane of the leaf is a value of.
    pub dtype: DType,
    pub kind: PalwTirLeafKindV1,
}

/// The positions of one job: `prefill` prompt positions and `positions = prefill + decode − 1` in
/// all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirJobShapeV1 {
    pub prefill: u32,
    pub positions: u32,
}

/// Actual immutable job token coordinate, not supplied material or availability.
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub enum PalwTirTokenSourceV1 { Prompt(u32), Generated(u32) }
impl PalwTirJobShapeV1 {
    /// Same absolute-position split used by the court's token source.
    pub fn token_source_v1(&self,pos:u32)->PalwTirTokenSourceV1 {
        if pos<self.prefill {PalwTirTokenSourceV1::Prompt(pos)}
            else {PalwTirTokenSourceV1::Generated(pos-self.prefill)}
    }

    /// The legacy coordinate `(call, position)` of absolute position `a`.
    pub fn call_position(&self, a: u32) -> (u32, u32) {
        if a < self.prefill { (0, a) } else { (a - self.prefill + 1, 0) }
    }
    /// The absolute position of a legacy `(call, position)`, if it is one of this job's.
    pub fn absolute(&self, call: u32, position: u32) -> Option<u32> {
        let a = if call == 0 {
            (position < self.prefill).then_some(position)?
        } else {
            (position == 0).then_some(())?;
            self.prefill.checked_add(call - 1)?
        };
        (a < self.positions).then_some(a)
    }
    /// Does position `a` evaluate `post` (its logits are consumed)?
    pub fn runs_post(&self, a: u32) -> bool {
        a + 1 >= self.prefill
    }
}

/// **The step space of one IR class** — its program decoded and validated once, its layout
/// checked against it, and the tables every enumeration reads.
#[derive(Clone, Debug)]
pub struct PalwTirStepSpaceV1 {
    pub program: TirProgramV1,
    pub info: ProgramInfo,
    pub layout: PalwTirLayoutV1,
    blocks: Vec<BlockSpaceV1>,
    occurrences: Vec<(u8, Option<u16>)>,
    slot_bases: Vec<u32>,
    program_slots: u32,
    fixed: Vec<PalwTirStateInstanceV1>,
    hist: Vec<PalwTirStateInstanceV1>,
}

/// `Σ_{i=0}^{n−1} ⌊(a·i + b) / m⌋` for `m ≥ 1` (the standard floor-sum recursion).
pub(crate) fn floor_sum(n: u128, m: u128, a: u128, b: u128) -> u128 {
    let (mut n, mut m, mut a, mut b) = (n, m, a, b);
    let mut ans = 0u128;
    loop {
        if a >= m {
            ans = ans.saturating_add((n.saturating_sub(1)).saturating_mul(n) / 2 * (a / m));
            a %= m;
        }
        if b >= m {
            ans = ans.saturating_add(n.saturating_mul(b / m));
            b %= m;
        }
        let y_max = a.saturating_mul(n).saturating_add(b);
        if y_max < m {
            return ans;
        }
        n = y_max / m;
        b = y_max % m;
        std::mem::swap(&mut m, &mut a);
    }
}

/// Additional StepSpace vectors, derived from opaque actual wire metadata.
/// Does NOT include Source input clones/hash writers, a lease or admission.
/// Caller must separately retain the program codec/analysis quote and reserve
/// all coexisting owners before calling an allocating constructor.
pub(crate) fn native_tir_step_space_additional_requests_v1(
    shape: &crate::palw_native_resource_v1::NativeTirProgramShapeV1,
) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
    let (blocks,nodes,layers,fixed_global,fixed_layer,hist_global,hist_layer)=shape.step_space_counts_v1();
    step_space_requests_from_counts_v1(blocks,nodes,layers,fixed_global,fixed_layer,hist_global,hist_layer)
}
fn step_space_requests_from_counts_v1(blocks:u64,nodes:u64,layers:u64,
    fixed_global:u64,fixed_layer:u64,hist_global:u64,hist_layer:u64,
) -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1};
    let overflow = || NativeCoverageV1::overflow::<PalwTirStepSpaceV1>("StepSpace structural cardinality");
    let mut q = TirNativeRequestsV1::new();
    let occurrences=layers.checked_add(2).ok_or_else(overflow)?;
    q.exact_v1::<BlockSpaceV1>(blocks)?;
    // Every commit is one actual node. Independently growing per-block Vecs
    // need a pinned minimum even for blocks with just one committed node.
    q.vec_v1::<CommitNodeV1>(nodes)?;
    let mut one=TirNativeRequestsV1::new();
    one.vec_v1::<CommitNodeV1>(1)?;
    q.add_v1(blocks.checked_mul(one.bytes_v1()).ok_or_else(overflow)?)?;
    // occurrences() uses exact layers+2; occurrence_slot_bases creates a second
    // such list, then collects its bases while that temporary is still live.
    q.exact_v1::<(u8, Option<u16>)>(occurrences)?;
    q.exact_v1::<(u8, Option<u16>)>(occurrences)?;
    q.vec_v1::<u32>(occurrences)?;
    // Global states always instantiate once. Per-layer states instantiate at
    // only the schedule layers referencing that state, hence <= actual layers.
    // Separate fixed/history bounds avoid charging both kinds for every state.
    let fixed=fixed_layer.checked_mul(layers).and_then(|n|n.checked_add(fixed_global)).ok_or_else(overflow)?;
    let hist=hist_layer.checked_mul(layers).and_then(|n|n.checked_add(hist_global)).ok_or_else(overflow)?;
    q.vec_v1::<PalwTirStateInstanceV1>(fixed)?;
    q.vec_v1::<PalwTirStateInstanceV1>(hist)?;
    // At most one layers Vec coexists with both output Vecs; global [None]
    // still allocates one entry when there are zero scheduled layers.
    q.vec_v1::<Option<u16>>(layers.max(1))?;
    Ok(q.bytes_v1())
}

impl PalwTirStepSpaceV1 {
    /// Decode the class's program, validate it and check the layout against it.
    pub fn new(class: &PalwTirClassV1) -> Result<Self, PalwTirStepError> {
        let program = class.decode_program().map_err(|e| PalwTirStepError::Program(e.to_string()))?;
        let info = misaka_palw_tir::validate::validate(&program).map_err(|e| PalwTirStepError::Program(e.to_string()))?;
        Self::from_program(program, info, class.layout.clone())
    }

    /// [`Self::new`] over an already-validated program.
    pub fn from_program(program: TirProgramV1, info: ProgramInfo, layout: PalwTirLayoutV1) -> Result<Self, PalwTirStepError> {
        if layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
            return layout_err(format!("layout version {} is not {PALW_TIR_LAYOUT_VERSION_V1}", layout.version));
        }
        if layout.max_context == 0 || layout.max_context > program.history_bound {
            return layout_err("max_context must be in [1, history_bound]");
        }
        if layout.checkpoint_interval == 0 {
            return layout_err("the checkpoint interval must be at least 1");
        }
        if !layout.h_tile.is_power_of_two() || layout.h_tile > PALW_TIR_MAX_H_TILE_V1 {
            return layout_err("h_tile must be a power of two in [1, 4096]");
        }
        // `post` writes no state: it runs only where logits are consumed, so a write there would be
        // a write the prefill positions skip.
        let post = &program.blocks[program.schedule.post as usize];
        if post.nodes.iter().any(|n| matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. })) {
            return layout_err("the post block writes state: it runs only at positions whose logits are consumed");
        }
        let tile_ok = |t: u32| (PALW_STEP_MIN_TILE_LEN..=PALW_STEP_MAX_TILE_LEN).contains(&t);
        let mut tiles = layout.commit_tiles.iter();
        let mut blocks = Vec::with_capacity(program.blocks.len());
        let tiled_scheme = crate::Hash64::from_bytes(program.logits_scheme_id) == crate::palw_step_refute::tiled_logits_scheme_id_v1();
        for (bi, b) in program.blocks.iter().enumerate() {
            let mut commits = Vec::new();
            for (ni, n) in b.nodes.iter().enumerate() {
                if !n.commit {
                    continue;
                }
                let tile_len =
                    *tiles.next().ok_or_else(|| PalwTirStepError::Layout("fewer commit tiles than committed nodes".into()))?;
                if !tile_ok(tile_len) {
                    return layout_err(format!("block {bi} node {ni}: tile_len {tile_len} is outside [4, 65536]"));
                }
                // Under the tiled logits scheme a logits step tile lies inside one trace tile: its
                // length divides the scheme's 4,096 lanes (decision (1) of 2026-09-28 — a real
                // vocabulary's 4,096-lane tile reads 4,096 weight rows, a close no carrier holds, so
                // a class may tile its logits finer and every terminal close stays carriable).
                if tiled_scheme
                    && bi == program.schedule.post as usize
                    && ni == program.logits as usize
                    && !(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u32).is_multiple_of(tile_len)
                {
                    return layout_err("under the tiled logits scheme the logits node's tile length divides the scheme's 4,096 lanes");
                }
                let fixed_elements = n.out.shape.iter().fold(1u64, |acc, d| match d {
                    Dim::Fixed(k) => acc.saturating_mul(*k as u64),
                    Dim::H => acc,
                });
                commits.push(CommitNodeV1 { node: ni as u16, dtype: n.out.dtype, fixed_elements, has_h: n.out.has_h(), tile_len });
            }
            blocks.push(BlockSpaceV1 { window: info.blocks[bi].window, commits });
        }
        if tiles.next().is_some() {
            return layout_err("more commit tiles than committed nodes");
        }
        if layout.state_tiles.len() != program.states.len() {
            return layout_err("one state tile per state declaration");
        }
        let occurrences = program.occurrences();
        let slot_bases = program.occurrence_slot_bases();
        let program_slots =
            occurrences.iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u32).fold(0u32, |acc, n| acc.saturating_add(n));
        // State instances: a per-layer state has one instance at every layer whose block reads,
        // writes or appends to it; a global state has one.
        let references = |block: u8, j: u16| {
            program.blocks[block as usize].nodes.iter().any(|n| {
                n.inputs.contains(&Ref::State(j))
                    || matches!(n.prim, Prim::StateWrite { state } | Prim::HistAppend { state } if state == j)
            })
        };
        let (mut fixed, mut hist) = (Vec::new(), Vec::new());
        for (j, s) in program.states.iter().enumerate() {
            let j = j as u16;
            let lanes = layout.state_tiles[j as usize];
            let elements = s.shape.iter().fold(1u64, |acc, d| acc.saturating_mul(*d as u64));
            let layers: Vec<Option<u16>> = if s.per_layer {
                program.schedule.layers.iter().enumerate().filter(|(_, b)| references(**b, j)).map(|(l, _)| Some(l as u16)).collect()
            } else {
                vec![None]
            };
            match s.kind {
                StateKind::Fixed { .. } => {
                    if !tile_ok(lanes) {
                        return layout_err(format!("state {j}: a checkpoint tile of {lanes} lanes is outside [4, 65536]"));
                    }
                    fixed.extend(layers.into_iter().map(|layer| PalwTirStateInstanceV1 {
                        state: j,
                        layer,
                        dtype: s.dtype,
                        elements,
                        tile_lanes: lanes,
                    }));
                }
                StateKind::Hist { .. } => {
                    if lanes == 0 || (lanes as u64).saturating_mul(layout.h_tile as u64) > PALW_STEP_MAX_TILE_LEN as u64 {
                        return layout_err(format!("history {j}: h_tile × {lanes} lanes must be in [1, 65536]"));
                    }
                    hist.extend(layers.into_iter().map(|layer| PalwTirStateInstanceV1 {
                        state: j,
                        layer,
                        dtype: s.dtype,
                        elements,
                        tile_lanes: lanes,
                    }));
                }
            }
        }
        Ok(Self { program, info, layout, blocks, occurrences, slot_bases, program_slots, fixed, hist })
    }

    /// The `Fixed` state instances, in checkpoint order.
    pub fn fixed_instances(&self) -> &[PalwTirStateInstanceV1] {
        &self.fixed
    }

    /// The `Hist` state instances, in tile order.
    pub fn hist_instances(&self) -> &[PalwTirStateInstanceV1] {
        &self.hist
    }

    /// The first reserved slot (the Fixed instances', then the Hist instances').
    pub fn program_slots(&self) -> u32 {
        self.program_slots
    }

    /// The tile length the layout gives committed node `node` of `block`, or `None` if the node is
    /// not a commit point.
    pub fn commit_tile_len(&self, block: u8, node: u16) -> Option<u32> {
        self.blocks.get(block as usize)?.commits.iter().find(|c| c.node == node).map(|c| c.tile_len)
    }

    /// The index of `Fixed` instance `(state, layer)` among [`Self::fixed_instances`].
    pub fn fixed_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize> {
        self.fixed.iter().position(|s| s.state == state && s.layer == layer)
    }

    /// The index of `Hist` instance `(state, layer)` among [`Self::hist_instances`].
    pub fn hist_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize> {
        self.hist.iter().position(|s| s.state == state && s.layer == layer)
    }

    /// The occurrences of a position, `(block, layer)`, in schedule order.
    pub fn occurrences(&self) -> &[(u8, Option<u16>)] {
        &self.occurrences
    }

    /// The node slot of `(occurrence, node)` (spec 04b §3.3).
    pub fn node_slot(&self, occurrence: usize, node: u16) -> Option<u32> {
        let base = *self.slot_bases.get(occurrence)?;
        let (b, _) = self.occurrences[occurrence];
        ((node as usize) < self.program.blocks[b as usize].nodes.len()).then_some(base + node as u32)
    }

    /// The occurrence a program slot belongs to, and the node within it.
    pub fn resolve_slot(&self, slot: u32) -> Option<(usize, u16)> {
        if slot >= self.program_slots {
            return None;
        }
        let occ = self.slot_bases.partition_point(|base| *base <= slot).checked_sub(1)?;
        Some((occ, (slot - self.slot_bases[occ]) as u16))
    }

    /// The positions a job runs, refused unless it fits the class.
    pub fn job_shape(&self, ctx: &PalwJobContextV2) -> Result<PalwTirJobShapeV1, PalwTirStepError> {
        if ctx.declared_prefill_tokens == 0 {
            return Err(PalwTirStepError::Job("an IR job has at least one prompt position"));
        }
        if ctx.exact_decode_tokens == 0 {
            return Err(PalwTirStepError::Job("an IR job emits at least one token"));
        }
        let positions = ctx
            .declared_prefill_tokens
            .checked_add(ctx.exact_decode_tokens - 1)
            .ok_or(PalwTirStepError::Job("the job's positions overflow"))?;
        if positions > self.layout.max_context {
            return Err(PalwTirStepError::Job("the job touches more positions than the class's max_context"));
        }
        Ok(PalwTirJobShapeV1 { prefill: ctx.declared_prefill_tokens, positions })
    }

    fn h_of(&self, block: u8, a: u32) -> u64 {
        self.blocks[block as usize].window.map(|w| (a as u64 + 1).min(w as u64)).unwrap_or(1)
    }

    fn block_leaves_at(&self, block: u8, a: u32) -> u64 {
        let h = self.h_of(block, a);
        self.blocks[block as usize].commits.iter().fold(0u64, |acc, c| acc.saturating_add(c.tiles(h)))
    }

    fn fixed_leaves_per_checkpoint(&self) -> u64 {
        self.fixed.iter().fold(0u64, |acc, s| acc.saturating_add(s.tiles()))
    }

    fn hist_leaves_per_tile(&self) -> u64 {
        self.hist.iter().fold(0u64, |acc, s| acc.saturating_add(s.tiles()))
    }

    /// Measurement accessor (no rule): the history-tile leaves one completed tile holds, over every history instance.
    pub fn hist_leaves_per_tile_v1(&self) -> u64 {
        self.hist_leaves_per_tile()
    }

    /// The leaves of absolute position `a`.
    pub fn leaves_at_position(&self, job: &PalwTirJobShapeV1, a: u32) -> u64 {
        let post_occ = self.occurrences.len() - 1;
        let mut n = 0u64;
        for (i, (b, _)) in self.occurrences.iter().enumerate() {
            if i == post_occ && !job.runs_post(a) {
                continue;
            }
            n = n.saturating_add(self.block_leaves_at(*b, a));
        }
        if (a + 1).is_multiple_of(self.layout.checkpoint_interval) {
            n = n.saturating_add(self.fixed_leaves_per_checkpoint());
        }
        if (a + 1).is_multiple_of(self.layout.h_tile) {
            n = n.saturating_add(self.hist_leaves_per_tile());
        }
        n
    }

    /// **The widest leaf of `job`, in lanes** (G14 audit §11 (a)): every leaf holds at most this many four-byte lanes — a committed
    /// node's tile is `min(tile_len, elements at the job's widest H)` (H never shrinks with the position), a checkpoint tile
    /// `min(tile_lanes, elements)`, a history tile `h_tile` rows of `min(tile_lanes, elements)` lanes. Exact for the class and the job,
    /// unlike the layout's widest tile (a 4,096-lane logits tile over an 8-word vocabulary holds 8 lanes).
    pub fn maximum_leaf_lanes_v1(&self, job: &PalwTirJobShapeV1) -> u64 {
        let last = job.positions.saturating_sub(1);
        let mut widest = 0u64;
        for (block, _) in &self.occurrences {
            let h = self.h_of(*block, last);
            for c in &self.blocks[*block as usize].commits {
                widest = widest.max(c.elements(h).min(u64::from(c.tile_len)));
            }
        }
        for s in &self.fixed {
            widest = widest.max(s.elements.min(u64::from(s.tile_lanes)));
        }
        for s in &self.hist {
            widest = widest.max(s.elements.min(u64::from(s.tile_lanes)).saturating_mul(u64::from(self.layout.h_tile)));
        }
        widest
    }

    /// **The most four-byte lanes `count` consecutive leaves of `job` can hold** (G14 audit §11 (a)): the smaller of `count` widest
    /// leaves and the positions such a window can touch — a position has at least `L_min` leaves (its non-post commits at `H = 1`,
    /// every position has them), so `count` consecutive leaves span at most `⌈(count − 1) / L_min⌉ + 1` positions — each at the most
    /// lanes any position emits (its commits at the widest `H` with the post block, a checkpoint and a history tile).
    pub fn maximum_window_lanes_v1(&self, job: &PalwTirJobShapeV1, count: u64) -> u128 {
        let per_leaf = u128::from(count).saturating_mul(u128::from(self.maximum_leaf_lanes_v1(job)));
        let post = self.occurrences.len() - 1;
        let minimum: u64 = self
            .occurrences
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != post)
            .map(|(_, (b, _))| self.block_leaves_at(*b, 0))
            .fold(0u64, |acc, n| acc.saturating_add(n));
        if minimum == 0 || count == 0 {
            return per_leaf;
        }
        let last = job.positions.saturating_sub(1);
        let mut position = 0u128;
        for (block, _) in &self.occurrences {
            let h = self.h_of(*block, last);
            for c in &self.blocks[*block as usize].commits {
                position = position.saturating_add(u128::from(c.elements(h)));
            }
        }
        for s in &self.fixed {
            position = position.saturating_add(u128::from(s.elements));
        }
        for s in &self.hist {
            position = position.saturating_add(u128::from(s.elements).saturating_mul(u128::from(self.layout.h_tile)));
        }
        let spans = u128::from((count - 1).div_ceil(minimum)) + 1;
        per_leaf.min(spans.saturating_mul(position))
    }

    /// **The most step-leaf runs an honest owner needs to acquire the cone of ANY leaf of `job`** (G14 audit §11 (b)), when it fetches
    /// each missing stretch as one full run of `run` leaves from its first missing leaf. Derived from what the court's cone evaluation
    /// reads (`palw_tir_court_v1::TirSource`): committed values at the leaf's own position; a fixed state between checkpoints by
    /// REPLAY back to the last checkpoint (the checkpoint leaf at the position before it), so at most `checkpoint_interval` positions
    /// back; a history row in the current (incomplete) tile from its commit point, so at most `h_tile` positions further back; a
    /// completed history tile from its one tile leaf at the tile's last position, over the reading block's attention window `W`
    /// (the whole context where a block has none). So the cone lies in ONE contiguous stretch of positions
    /// `[a − checkpoint_interval − h_tile, a]` plus at most `⌈(W + checkpoint_interval) / h_tile⌉ + 1` single positions' history
    /// leaves; never more than the job's whole leaf range.
    pub fn cone_runs_bound_v1(&self, job: &PalwTirJobShapeV1, leaf_count: u64, run: u64) -> u64 {
        let run = run.max(1);
        let whole = leaf_count.div_ceil(run);
        let last = job.positions.saturating_sub(1);
        // Every occurrence's commits at the widest H (the post block included), a checkpoint and a history tile: the most leaves
        // any one position holds.
        let commits_per_position =
            self.occurrences.iter().fold(0u64, |acc, (block, _)| acc.saturating_add(self.block_leaves_at(*block, last)));
        let per_position =
            commits_per_position.saturating_add(self.fixed_leaves_per_checkpoint()).saturating_add(self.hist_leaves_per_tile());
        let ci = u64::from(self.layout.checkpoint_interval);
        let ht = u64::from(self.layout.h_tile);
        let stretch = ci.saturating_add(ht).saturating_add(1).saturating_mul(per_position);
        let mut bound = stretch.div_ceil(run).saturating_add(1);
        if !self.hist.is_empty() {
            let window = self
                .blocks
                .iter()
                .map(|b| b.window.map(u64::from).unwrap_or(u64::from(job.positions)))
                .max()
                .unwrap_or(u64::from(job.positions));
            let tiles = window.saturating_add(ci).div_ceil(ht.max(1)).saturating_add(1);
            let per_tile = self.hist_leaves_per_tile().div_ceil(run).saturating_add(1);
            bound = bound.saturating_add(tiles.saturating_mul(per_tile));
        }
        bound.min(whole).max(1)
    }

    /// Canonical four-byte lanes emitted at one position, without constructing its leaf
    /// metadata. Node replay admission pairs this with `leaves_at_position` to account
    /// preimage buffers before enumeration/allocation. Saturation refuses on conversion
    /// under a finite node storage budget; it changes no commitment or layout rule.
    pub fn lane_count_at_position_v1(&self, job: &PalwTirJobShapeV1, a: u32) -> u128 {
        if a >= job.positions {
            return 0;
        }
        let post = self.occurrences.len() - 1;
        let mut lanes = 0u128;
        for (i, (block, _)) in self.occurrences.iter().enumerate() {
            if i == post && !job.runs_post(a) {
                continue;
            }
            let h = self.h_of(*block, a);
            for commit in &self.blocks[*block as usize].commits {
                lanes = lanes.saturating_add(u128::from(commit.elements(h)));
            }
        }
        if (a + 1).is_multiple_of(self.layout.checkpoint_interval) {
            for instance in &self.fixed {
                lanes = lanes.saturating_add(u128::from(instance.elements));
            }
        }
        if (a + 1).is_multiple_of(self.layout.h_tile) {
            for instance in &self.hist {
                lanes = lanes.saturating_add(u128::from(instance.elements).saturating_mul(u128::from(self.layout.h_tile)));
            }
        }
        lanes
    }

    /// `Σ_{p ∈ [lo, hi)} ⌈f · min(p + 1, W) / t⌉` for one committed node, in closed form.
    fn node_leaves_over(c: &CommitNodeV1, window: Option<u32>, lo: u64, hi: u64) -> u128 {
        if hi <= lo {
            return 0;
        }
        let t = c.tile_len as u128;
        let f = c.fixed_elements as u128;
        let count = (hi - lo) as u128;
        match (c.has_h, window) {
            (true, Some(w)) => {
                let w = w as u64;
                // p + 1 ≤ W  ⇔  p < W
                let g = |n: u64| floor_sum(n as u128, t, f, f + t - 1);
                let rising_hi = hi.min(w);
                let rising = if rising_hi > lo { g(rising_hi).saturating_sub(g(lo)) } else { 0 };
                let flat_lo = lo.max(w);
                let flat = if hi > flat_lo { ((hi - flat_lo) as u128).saturating_mul((f * w as u128).div_ceil(t)) } else { 0 };
                rising.saturating_add(flat)
            }
            _ => count.saturating_mul(f.div_ceil(t)),
        }
    }

    /// **The leaves of positions `[0, a)`**, in closed form.
    pub fn running_total(&self, job: &PalwTirJobShapeV1, a: u32) -> u128 {
        let a64 = a as u64;
        let post_occ = self.occurrences.len() - 1;
        let post_block = self.occurrences[post_occ].0;
        // Occurrence multiplicity of every non-post block.
        let mut mult = vec![0u128; self.blocks.len()];
        for (i, (b, _)) in self.occurrences.iter().enumerate() {
            if i != post_occ {
                mult[*b as usize] += 1;
            }
        }
        let mut total = 0u128;
        for (bi, block) in self.blocks.iter().enumerate() {
            if mult[bi] > 0 {
                for c in &block.commits {
                    total = total.saturating_add(mult[bi].saturating_mul(Self::node_leaves_over(c, block.window, 0, a64)));
                }
            }
        }
        let post_lo = (job.prefill as u64).saturating_sub(1);
        for c in &self.blocks[post_block as usize].commits {
            total = total.saturating_add(Self::node_leaves_over(c, self.blocks[post_block as usize].window, post_lo, a64));
        }
        total = total.saturating_add(
            ((a / self.layout.checkpoint_interval) as u128).saturating_mul(self.fixed_leaves_per_checkpoint() as u128),
        );
        total.saturating_add(((a / self.layout.h_tile) as u128).saturating_mul(self.hist_leaves_per_tile() as u128))
    }

    /// **The job's step leaf count**, refused past `cap` (the class's ladder).
    pub fn leaf_count_capped(&self, ctx: &PalwJobContextV2, cap: u64) -> Result<u64, PalwTirStepError> {
        let job = self.job_shape(ctx)?;
        let total = self.running_total(&job, job.positions);
        if total > cap as u128 {
            return Err(PalwTirStepError::TooManyLeaves { got: total.min(u64::MAX as u128) as u64, max: cap });
        }
        Ok(total as u64)
    }

    /// **The leaf at `index`**, or `None` past the job's last.
    pub fn leaf_at(&self, ctx: &PalwJobContextV2, index: u64) -> Option<PalwTirLeafV1> {
        let job = self.job_shape(ctx).ok()?;
        let idx = index as u128;
        if idx >= self.running_total(&job, job.positions) {
            return None;
        }
        // The largest position whose running total is at most the index (non-decreasing in `a`).
        let mut before = 0u32;
        for bit in (0..32u32).rev() {
            let candidate = before + (1u32 << bit);
            if candidate < job.positions && self.running_total(&job, candidate) <= idx {
                before = candidate;
            }
        }
        let offset = (idx - self.running_total(&job, before)) as u64;
        self.leaf_in_position(&job, before, offset, index)
    }

    fn leaf_in_position(&self, job: &PalwTirJobShapeV1, a: u32, mut offset: u64, index: u64) -> Option<PalwTirLeafV1> {
        let (call, position) = job.call_position(a);
        let post_occ = self.occurrences.len() - 1;
        for (i, (b, layer)) in self.occurrences.iter().enumerate() {
            if i == post_occ && !job.runs_post(a) {
                continue;
            }
            let here = self.block_leaves_at(*b, a);
            if offset >= here {
                offset -= here;
                continue;
            }
            let h = self.h_of(*b, a);
            for c in &self.blocks[*b as usize].commits {
                let tiles = c.tiles(h);
                if offset >= tiles {
                    offset -= tiles;
                    continue;
                }
                let first = offset * c.tile_len as u64;
                let count = (c.elements(h) - first).min(c.tile_len as u64) as u32;
                return Some(PalwTirLeafV1 {
                    index,
                    coord: PalwStepCoordinateV1 {
                        call_index: call,
                        node_slot: self.slot_bases[i] + c.node as u32,
                        position,
                        tile_index: offset as u32,
                    },
                    position: a,
                    value_count: count,
                    dtype: c.dtype,
                    kind: PalwTirLeafKindV1::Commit {
                        occurrence: i as u32,
                        block: *b,
                        layer: *layer,
                        node: c.node,
                        first_element: first,
                    },
                });
            }
            return None;
        }
        if (a + 1).is_multiple_of(self.layout.checkpoint_interval) {
            for (k, s) in self.fixed.iter().enumerate() {
                let tiles = s.tiles();
                if offset >= tiles {
                    offset -= tiles;
                    continue;
                }
                let first = offset * s.tile_lanes as u64;
                return Some(PalwTirLeafV1 {
                    index,
                    coord: PalwStepCoordinateV1 {
                        call_index: call,
                        node_slot: self.program_slots + k as u32,
                        position,
                        tile_index: offset as u32,
                    },
                    position: a,
                    value_count: (s.elements - first).min(s.tile_lanes as u64) as u32,
                    dtype: s.dtype,
                    kind: PalwTirLeafKindV1::State { instance: k as u32, state: s.state, layer: s.layer, first_element: first },
                });
            }
        }
        if (a + 1).is_multiple_of(self.layout.h_tile) {
            for (k, s) in self.hist.iter().enumerate() {
                let tiles = s.tiles();
                if offset >= tiles {
                    offset -= tiles;
                    continue;
                }
                let first_lane = offset * s.tile_lanes as u64;
                let row_lanes = (s.elements - first_lane).min(s.tile_lanes as u64) as u32;
                return Some(PalwTirLeafV1 {
                    index,
                    coord: PalwStepCoordinateV1 {
                        call_index: call,
                        node_slot: self.program_slots + (self.fixed.len() + k) as u32,
                        position,
                        tile_index: offset as u32,
                    },
                    position: a,
                    value_count: row_lanes * self.layout.h_tile,
                    dtype: s.dtype,
                    kind: PalwTirLeafKindV1::HistTile {
                        instance: k as u32,
                        state: s.state,
                        layer: s.layer,
                        first_lane,
                        row_lanes,
                        first_position: a + 1 - self.layout.h_tile,
                    },
                });
            }
        }
        None
    }

    /// **The index of the leaf at `coord`**, or `None` if the coordinate names no leaf of the job.
    pub fn leaf_index(&self, ctx: &PalwJobContextV2, coord: &PalwStepCoordinateV1) -> Option<u64> {
        let job = self.job_shape(ctx).ok()?;
        let a = job.absolute(coord.call_index, coord.position)?;
        let mut offset = 0u64;
        let post_occ = self.occurrences.len() - 1;
        let tile = coord.tile_index as u64;
        let found = if let Some((occ, node)) = self.resolve_slot(coord.node_slot) {
            if occ == post_occ && !job.runs_post(a) {
                return None;
            }
            for (i, (b, _)) in self.occurrences.iter().enumerate().take(occ) {
                if i == post_occ && !job.runs_post(a) {
                    continue;
                }
                offset += self.block_leaves_at(*b, a);
            }
            let (b, _) = self.occurrences[occ];
            let h = self.h_of(b, a);
            let mut hit = None;
            for c in &self.blocks[b as usize].commits {
                if c.node == node {
                    hit = (tile < c.tiles(h)).then_some(offset + tile);
                    break;
                }
                offset += c.tiles(h);
            }
            hit?
        } else {
            for (i, (b, _)) in self.occurrences.iter().enumerate() {
                if i == post_occ && !job.runs_post(a) {
                    continue;
                }
                offset += self.block_leaves_at(*b, a);
            }
            let reserved = coord.node_slot - self.program_slots;
            let checkpoint = (a + 1).is_multiple_of(self.layout.checkpoint_interval);
            if (reserved as usize) < self.fixed.len() {
                if !checkpoint {
                    return None;
                }
                for s in &self.fixed[..reserved as usize] {
                    offset += s.tiles();
                }
                (tile < self.fixed[reserved as usize].tiles()).then_some(offset + tile)?
            } else {
                let k = reserved as usize - self.fixed.len();
                if k >= self.hist.len() || !(a + 1).is_multiple_of(self.layout.h_tile) {
                    return None;
                }
                if checkpoint {
                    offset += self.fixed_leaves_per_checkpoint();
                }
                for s in &self.hist[..k] {
                    offset += s.tiles();
                }
                (tile < self.hist[k].tiles()).then_some(offset + tile)?
            }
        };
        let index = (self.running_total(&job, a) as u64).checked_add(found)?;
        Some(index)
    }

    /// **The TIR prefix-state boundary** (behind `palw_staged_tir_route_v1`; the relation of
    /// [`crate::palw_tir_prefix_state_v1`]): the number of state leaves that commit the state after
    /// the first `k` positions, or `None` unless `k` is aligned to both the checkpoint interval and
    /// `h_tile` and lies strictly inside the prompt. Only then is that state exactly committed by
    /// leaves: the `Fixed` checkpoints at position `k − 1` and every `Hist` chunk of rows `0..k`.
    pub fn prefix_state_leaf_count_v1(&self, ctx: &PalwJobContextV2, k: u32) -> Option<u64> {
        let job = self.job_shape(ctx).ok()?;
        if k == 0 || k >= job.prefill || !k.is_multiple_of(self.layout.checkpoint_interval) || !k.is_multiple_of(self.layout.h_tile) {
            return None;
        }
        let chunks = u64::from(k / self.layout.h_tile);
        chunks.checked_mul(self.hist_leaves_per_tile())?.checked_add(self.fixed_leaves_per_checkpoint())
    }

    /// The `ordinal`-th state leaf of the prefix boundary `k`, in ascending leaf-index order: each
    /// `Hist` chunk's leaves at positions `h_tile − 1, 2·h_tile − 1, …`, with the `Fixed`
    /// checkpoint leaves at `k − 1` placed before that position's `Hist` leaves (their order in
    /// the step tree).
    pub fn prefix_state_leaf_v1(&self, ctx: &PalwJobContextV2, k: u32, ordinal: u64) -> Option<PalwTirLeafV1> {
        let total = self.prefix_state_leaf_count_v1(ctx, k)?;
        if ordinal >= total {
            return None;
        }
        let job = self.job_shape(ctx).ok()?;
        let hist = self.hist_leaves_per_tile();
        let fixed = self.fixed_leaves_per_checkpoint();
        let before_last = u64::from(k / self.layout.h_tile - 1) * hist;
        let (position, within) = if ordinal < before_last {
            let chunk = (ordinal / hist) as u32;
            ((chunk + 1) * self.layout.h_tile - 1, Some(ordinal % hist))
        } else {
            (k - 1, None)
        };
        let all = self.leaves_at_position(&job, position);
        let checkpoint = (position + 1).is_multiple_of(self.layout.checkpoint_interval);
        let commits = all - if checkpoint { fixed } else { 0 } - hist;
        let offset = match within {
            // An earlier chunk: its Hist leaves come after any checkpoint at that position.
            Some(j) => commits + if checkpoint { fixed } else { 0 } + j,
            // The boundary position: Fixed first, then Hist.
            None => commits + (ordinal - before_last),
        };
        let first = self.running_total(&job, position) as u64;
        self.leaf_in_position(&job, position, offset, first + offset)
    }

    /// Every leaf of position `a`, in order — what a producer commits there.
    pub fn leaves_of_position(&self, ctx: &PalwJobContextV2, a: u32) -> Vec<PalwTirLeafV1> {
        let Ok(job) = self.job_shape(ctx) else { return Vec::new() };
        if a >= job.positions {
            return Vec::new();
        }
        let first = self.running_total(&job, a) as u64;
        (0..self.leaves_at_position(&job, a)).filter_map(|k| self.leaf_in_position(&job, a, k, first + k)).collect()
    }
}

/// **A leaf's values as its 4-byte lanes** (PALW-TIR-5): every value must be one of the leaf's
/// dtype; `i8`/`i16`/`i32` are written as little-endian `i32`, `idx` as little-endian `u32`.
pub fn palw_tir_lanes_le_v1(dtype: DType, values: &[i128]) -> Result<Vec<u8>, PalwTirStepError> {
    if !dtype.committable() {
        return Err(PalwTirStepError::NotCanonical(format!("{} is never committed", dtype.name())));
    }
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        if !dtype.contains(*v) {
            return Err(PalwTirStepError::NotCanonical(format!("{v} is not a {}", dtype.name())));
        }
        if dtype == DType::Idx {
            out.extend_from_slice(&(*v as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(*v as i32).to_le_bytes());
        }
    }
    Ok(out)
}

/// **A leaf's lanes exactly as committed, whatever they hold** (PALW-TIR-5; spec 04b §15.14): each value
/// is its lane's four little-endian bytes, the **low 32 bits** — `idx`'s as `u32`, every other
/// dtype's as `i32` — with no judgement of whether the dtype or the node's interval contains it. A value
/// the dtype contains encodes as [`palw_tir_lanes_le_v1`] encodes it, byte for byte; a value outside it
/// (an `i16` node's 32,768, an `i8`'s 128, an `idx`'s −1) still has its lane, so a leaf the executor
/// committed with one **opens, proves under its stage's root and is convicted by PALW-TIR-33's interval
/// check** — instead of being an unprovable leaf that convicts nobody (finding G21: the executor hashes
/// its own tree, so it could otherwise commit such a lane and escape). Total except for a dtype that is
/// never committed. The IR builder's strict [`palw_tir_lanes_le_v1`] stays what an honest producer builds
/// with; every path that hashes or re-encodes a leaf DISCLOSED to a court uses this one.
pub fn palw_tir_lanes_wire_v1(dtype: DType, values: &[i128]) -> Result<Vec<u8>, PalwTirStepError> {
    if !dtype.committable() {
        return Err(PalwTirStepError::NotCanonical(format!("{} is never committed", dtype.name())));
    }
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        if dtype == DType::Idx {
            out.extend_from_slice(&(*v as u32).to_le_bytes());
        } else {
            out.extend_from_slice(&(*v as i32).to_le_bytes());
        }
    }
    Ok(out)
}

/// **The integers a leaf's lanes hold**, read without judging them: `idx` lanes as `u32`, every
/// other dtype's as `i32`. Whether each value is one its node can produce is PALW-TIR-33's
/// question, asked by the court against the node's proven interval (which lies inside the dtype).
pub fn palw_tir_lane_values_v1(dtype: DType, bytes: &[u8]) -> Result<Vec<i128>, PalwTirStepError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(PalwTirStepError::NotCanonical("a lane is four bytes".into()));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| {
            let b = [c[0], c[1], c[2], c[3]];
            if dtype == DType::Idx { u32::from_le_bytes(b) as i128 } else { i32::from_le_bytes(b) as i128 }
        })
        .collect())
}

/// The preimage of one leaf holding `values`.
pub fn palw_tir_leaf_preimage_v1(leaf: &PalwTirLeafV1, values: &[i128]) -> Result<PalwStepTileLeafV1, PalwTirStepError> {
    if values.len() != leaf.value_count as usize {
        return Err(PalwTirStepError::NotCanonical(format!("{} values for a leaf of {}", values.len(), leaf.value_count)));
    }
    Ok(PalwStepTileLeafV1 {
        version: PALW_TIR_STEP_LEAF_VERSION_V1,
        coord: leaf.coord,
        value_count: leaf.value_count,
        values_le: palw_tir_lanes_le_v1(leaf.dtype, values)?,
    })
}

/// **The fail-closed step-leg builder of an IR execution**: leaves must arrive in canonical order
/// with canonical lengths and values of their dtype, or construction fails — the non-canonical leaf
/// is unbuildable rather than carried.
pub struct PalwTirStepLegBuilderV1<'s> {
    space: &'s PalwTirStepSpaceV1,
    ctx: PalwJobContextV2,
    context_hash: Hash64,
    class_id: Hash64,
    total: u64,
    cap: u64,
    leaf_hashes: Vec<Hash64>,
}

impl<'s> PalwTirStepLegBuilderV1<'s> {
    pub fn new(space: &'s PalwTirStepSpaceV1, ctx: &PalwJobContextV2, class_id: Hash64, cap: u64) -> Result<Self, PalwTirStepError> {
        let total = space.leaf_count_capped(ctx, cap)?;
        Ok(Self { space, ctx: ctx.clone(), context_hash: ctx.context_hash(), class_id, total, cap, leaf_hashes: Vec::new() })
    }

    /// The leaf the next [`Self::push`] fills, or `None` when the leg is complete.
    pub fn next_leaf(&self) -> Option<PalwTirLeafV1> {
        self.space.leaf_at(&self.ctx, self.leaf_hashes.len() as u64)
    }

    /// Commit the next leaf's values.
    pub fn push(&mut self, values: &[i128]) -> Result<PalwTirLeafV1, PalwTirStepError> {
        let leaf = self.next_leaf().ok_or_else(|| PalwTirStepError::NotCanonical("the leg is already complete".into()))?;
        let preimage = palw_tir_leaf_preimage_v1(&leaf, values)?;
        self.leaf_hashes.push(step_tile_leaf_hash_v1(&self.context_hash, &self.class_id, &preimage));
        Ok(leaf)
    }

    /// The leaf count and the step Merkle root, refused unless every leaf arrived.
    pub fn finish(self) -> Result<(u64, Hash64), PalwTirStepError> {
        if self.leaf_hashes.len() as u64 != self.total {
            return Err(PalwTirStepError::NotCanonical(format!("{} of {} leaves", self.leaf_hashes.len(), self.total)));
        }
        let root =
            step_merkle_root_capped_v1(&self.leaf_hashes, self.cap).map_err(|e| PalwTirStepError::NotCanonical(e.to_string()))?;
        Ok((self.total, root))
    }
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **An IR execution's root**: the job context, the logits trace root, and the step leg (the legacy
/// `step_leg_root_v1` with the IR class id in the profile's place) — under its own key. No
/// checkpoint leg: an IR class's checkpoints are step leaves.
pub fn palw_tir_execution_root_v1(
    context_hash: &Hash64,
    full_logits_trace_root: &Hash64,
    class_id: &Hash64,
    step_leaf_count: u64,
    step_merkle_root: &Hash64,
) -> Hash64 {
    let step_root = step_leg_root_v1(context_hash, class_id, step_leaf_count, step_merkle_root);
    keyed64(
        PALW_TIR_EXECUTION_ROOT_DOMAIN_V1,
        &[context_hash.as_byte_slice(), full_logits_trace_root.as_byte_slice(), step_root.as_byte_slice()],
    )
}

/// **What pins an IR execution** — the twin of `PalwStepBindingV2`, carried by every IR court move.
/// The class is carried whole and re-hashed to the class id every time, never trusted.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirStepBindingV1 {
    pub version: u16,
    /// Its `shape_profile_id` is the IR class id.
    pub job_context: PalwJobContextV2,
    pub class: PalwTirClassV1,
    /// The class's TIR inventory root (inside the class id).
    pub artifact_root: Hash64,
    pub full_logits_trace_root: Hash64,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
    pub committed_execution_root: Hash64,
}

/// A binding that verified, with what verifying it derived.
#[derive(Clone, Debug)]
pub struct PalwTirVerifiedBindingV1 {
    pub space: PalwTirStepSpaceV1,
    pub class_id: Hash64,
    pub context_hash: Hash64,
    pub job: PalwTirJobShapeV1,
}

/// **Verify a binding**: its version, the job context's shape, the class id it recomputes to (which
/// the job context must name), the step space and the job, the canonical leaf count at the ladder
/// `max_step_leaf_count`, and the execution root it recomputes to.
pub fn verify_tir_binding_v1(
    binding: &PalwTirStepBindingV1,
    max_step_leaf_count: u64,
) -> Result<PalwTirVerifiedBindingV1, PalwTirStepError> {
    if binding.version != PALW_TIR_STEP_BINDING_VERSION_V1 {
        return Err(PalwTirStepError::Binding("unsupported binding version"));
    }
    crate::palw_slash::check_job_context_shape(&binding.job_context).map_err(|_| PalwTirStepError::Binding("job context shape"))?;
    let class_id = binding.class.class_id(&binding.artifact_root);
    if binding.job_context.shape_profile_id != class_id {
        return Err(PalwTirStepError::Binding("the job context names another class"));
    }
    let space = PalwTirStepSpaceV1::new(&binding.class)?;
    let job = space.job_shape(&binding.job_context)?;
    let count = space.leaf_count_capped(&binding.job_context, max_step_leaf_count)?;
    if count != binding.step_leaf_count {
        return Err(PalwTirStepError::Binding("the step leaf count is not the job's canonical count"));
    }
    let context_hash = binding.job_context.context_hash();
    let root = palw_tir_execution_root_v1(
        &context_hash,
        &binding.full_logits_trace_root,
        &class_id,
        binding.step_leaf_count,
        &binding.step_merkle_root,
    );
    if root != binding.committed_execution_root {
        return Err(PalwTirStepError::Binding("the parts do not produce the committed execution root"));
    }
    Ok(PalwTirVerifiedBindingV1 { space, class_id, context_hash, job })
}

#[cfg(test)]
mod native_step_space_inventory_tests_v1 {
    use super::*;
    #[test]
    fn actual_scanned_nonempty_step_space_vectors_fit_additional_quote() {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let class = &x.binding.class;
        let shape = crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&class.program).unwrap();
        assert!(shape.structural_items_v1() > 0 && shape.program_requests_v1() > 0);
        let decoded=class.decode_program().unwrap();
        let (b,n,l,fg,fl,hg,hl)=shape.step_space_counts_v1();
        assert_eq!((b,n,l),(decoded.blocks.len() as u64,decoded.blocks.iter().map(|b|b.nodes.len() as u64).sum::<u64>(),decoded.schedule.layers.len() as u64));
        assert_eq!(shape.parameter_count_v1(),decoded.params.len() as u64);
        let count=|fixed:bool,layer:bool| decoded.states.iter().filter(|s|matches!(s.kind,StateKind::Fixed{..})==fixed && s.per_layer==layer).count() as u64;
        assert_eq!((fg,fl,hg,hl),(count(true,false),count(true,true),count(false,false),count(false,true)));
        let quote = native_tir_step_space_additional_requests_v1(&shape).unwrap();
        let space = PalwTirStepSpaceV1::new(class).unwrap();
        let mut actual = space.blocks.capacity() * std::mem::size_of::<BlockSpaceV1>();
        actual += space.blocks.iter().map(|b| b.commits.capacity() * std::mem::size_of::<CommitNodeV1>()).sum::<usize>();
        actual += space.occurrences.capacity() * std::mem::size_of::<(u8,Option<u16>)>();
        actual += space.slot_bases.capacity() * std::mem::size_of::<u32>();
        actual += (space.fixed.capacity()+space.hist.capacity()) * std::mem::size_of::<PalwTirStateInstanceV1>();
        assert!(quote >= actual as u64);
    }
    #[test]
    fn actual_fixed_history_step_space_capacities_fit_scanned_quote() {
        for committed in [false,true] {
            let fx=crate::palw_tir_public_claim_v1::palw_tir_public_fixed_history_frame_fixture_v1(
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,committed);
            let class=&fx.execution.binding.class;
            let shape=crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&class.program).unwrap();
            let space=PalwTirStepSpaceV1::new(class).unwrap();
            assert!(!space.fixed_instances().is_empty());
            assert!(!space.hist_instances().is_empty());
            let (blocks,nodes,layers,fg,fl,hg,hl)=shape.step_space_counts_v1();
            assert_eq!(blocks,space.program.blocks.len() as u64);
            assert_eq!(nodes,space.program.blocks.iter().map(|b|b.nodes.len() as u64).sum::<u64>());
            assert_eq!(layers,space.program.schedule.layers.len() as u64);
            assert_eq!(space.occurrences.len() as u64,layers+2);
            assert!((space.fixed_instances().len() as u64)<=fg+fl*layers);
            assert!((space.hist_instances().len() as u64)<=hg+hl*layers);
            let mut actual=space.blocks.capacity()*std::mem::size_of::<BlockSpaceV1>();
            actual+=space.blocks.iter().map(|b|b.commits.capacity()*std::mem::size_of::<CommitNodeV1>()).sum::<usize>();
            actual+=space.occurrences.capacity()*std::mem::size_of::<(u8,Option<u16>)>();
            actual+=space.slot_bases.capacity()*std::mem::size_of::<u32>();
            actual+=(space.fixed.capacity()+space.hist.capacity())*std::mem::size_of::<PalwTirStateInstanceV1>();
            let quote=native_tir_step_space_additional_requests_v1(&shape).unwrap();
            assert!(quote>=actual as u64,"committed={committed} quote={quote} actual retained capacity={actual}");
        }
    }
    #[test]
    fn scanned_step_space_counts_distinguish_state_kinds_and_layer_flags() {
        let x=crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut p=x.binding.class.decode_program().unwrap();
        p.states.clear();
        for (index,(kind,per_layer)) in [
            (StateKind::Fixed{lo:-1,hi:1},false),(StateKind::Fixed{lo:-1,hi:1},true),
            (StateKind::Hist{window:4},false),(StateKind::Hist{window:4},true),
        ].into_iter().enumerate() {
            p.states.push(misaka_palw_tir::program::StateDecl{name:format!("scan-{index}"),kind,dtype:misaka_palw_tir::types::DType::I8,shape:vec![4],per_layer});
        }
        p.schedule.layers=vec![p.schedule.pre;7];
        // Wire-metadata test only: this synthetic state/schedule combination is
        // NOT passed off as a valid executable class or Source constructor.
        let s=crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&p.encode()).unwrap();
        let (_,_,layers,fg,fl,hg,hl)=s.step_space_counts_v1();
        assert_eq!((layers,fg,fl,hg,hl),(7,1,1,1,1));
    }
    #[test]
    fn structural_overflow_and_hostile_canonical_shape_are_refused_before_space_construction() {
        assert!(step_space_requests_from_counts_v1(0,0,u64::MAX,0,0,0,0).is_err());
        assert!(step_space_requests_from_counts_v1(0,0,2,0,u64::MAX,0,0).is_err());
        assert!(crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&[]).is_err());
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut bytes = x.binding.class.program.clone();
        bytes.push(0);
        assert!(crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&bytes).is_err());
        let mut bytes = vec![0u8;2+64+4+4];
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&bytes).is_err());
    }
}
