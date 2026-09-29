//! **Spec 04b §10.3, PALW-TIR-38: the carried size of every terminal close of an IR class**
//! (`TirCloseDemandV1`, design §2.12.1).
//!
//! A class with a dissected tile clocks its executor at every terminal leaf, so the acquitting close of
//! every tile — a whole tile's cone close, a dissected cone's bottom — must be one the chain can carry.
//! This module measures that close as the court builds it: the units the court's own evaluation reads
//! (its read set, spec 04b §9.4 — never the box rule's over-approximation), each priced as the close
//! carries it.
//!
//! **The read set** ([`palw_tir_close_reads_v1`]) is an abstract twin of `eval_demanded` over element
//! SETS instead of values: the same contexts, the same index maps primitive by primitive, the same
//! leaves (commit points, carry-outs, checkpoints, history tiles, inventory leaves, prompt and decode
//! tokens) mapped to the same step-leaf and inventory indices the court's source serves. Where the court
//! reads by a VALUE it cannot know here, the twin reads a superset: a `Select` reads its condition and
//! BOTH operands; a `Gather` reads its index exactly and, for its data, one location-free row per index
//! element (a param's row at axis 0), or the whole indexed fiber otherwise.
//!
//! **The worst close over every job** ([`palw_tir_worst_closes_v1`]) is reached without enumerating
//! positions, from the structure of spec 04b §10.3 (*Dissectability is structural*):
//!
//! * a tile that is `H`-free and not dissected reads no history, so its read set depends on the
//!   position only through the replay distance of `Fixed` states (maximal at a position
//!   `≡ C − 1 (mod C)`), the token (priced at its larger form), and the alignment of an `H`-carrying
//!   commit point it reads (one more leaf a run); every such tile is read at that position;
//! * a tile of an `H`-carrying commit point spans many outer rows only while `H · inner < T` — those
//!   early positions are read tile by tile — and at most two row-parts after: the END of one row (with
//!   the position's own row) and the START of the next. Each part splits into what it reads through a
//!   history row and everything else. Everything else is within the reads of the `T`-window ending its
//!   row, or of the one starting it — the same at every position up to the alignment allowance above —
//!   so a tile's is bounded by their union over its rows. When the tile's cone reduces nothing over `H`
//!   it is H-LOCAL (spec 04b §10.3, *Dissectability is structural*): its element at history index `t`
//!   reads the history at row `t` only, through the same history tiles and row commits at every
//!   position — one row's pattern, read once — so the early tiles count their history rows exactly (a
//!   complete history tile, or the row's commit) and a late part at most `T` rows in both forms over
//!   the most history tiles `T` rows can touch. Otherwise the history a part reads is bounded by the
//!   worst `T`-window of its row read in *both-mode* (every history row priced as BOTH its history tile
//!   and its row commit) at every alignment of the history tiles. Either way every history leaf is
//!   priced as a run of its own: a bound no alignment, position or job exceeds;
//! * a dissected tile's bottom reads the claim's elements over one history tile, bounded in both-mode
//!   by its first and last tiles together; its root claim is the finalize and the fixpoint of its
//!   probes at the history's first row, exactly as the builder reads them.
//!
//! **The price** ([`PalwTirClosePriceV1`]) is the carried bytes: the frame — the close object itself,
//! serialized, with nothing opened (the binding with its program referenced, the job context at its
//! widest network id, the disputed leaf's opening at the tree's depth) — plus the disputed leaf's
//! lanes, every step leaf's preimage, a sibling set per contiguous run (a level's two edges at most,
//! one for a single leaf), the parameters in the close's opening form ([`PalwTirParamFormV1`]; the
//! multiproof in Phase F's byte-exact format, `palw_artifact_multiproof_borsh_len_v1`), and the token.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::Hash64;
use crate::palw_artifact::{palw_artifact_multiproof_borsh_len_v1, palw_artifact_operand_borsh_len_v1};
use crate::palw_court_v2::PalwCourtVerdictProofV2;
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1};
use crate::palw_step_refute::PalwStepInputRowV1;
use crate::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1;
use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use crate::palw_tir_court_v1::{PalwTirConeRefutationV1, PalwTirInventoryIndexV1};
use crate::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirRangeClaimV1, PalwTirRootClaimV1};
use crate::palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirJobShapeV1, PalwTirStepBindingV1, PalwTirStepSpaceV1};
use crate::palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2};
use misaka_palw_tir::demand::{DemandContext, carry_in_node_v1, hist_row_node_v1, history_length_v1, state_writer_v1};
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{Prim, Ref};

/// **The most work one class's close sizing may do**: admission refuses a class whose sizing would do
/// more, by name, rather than run it — a registration's CPU is bounded before it is spent (one IR
/// registration counts per block). Work is counted in steps that track the time they take: an
/// element read or visited, a context made (a step a node), a request seeded (a step an element), a
/// step leaf placed (`16` plus the program's commit points and occurrences, what its index costs), a
/// union or count outside the twin (a step an entry). The Qwen2.5-1.5B A16 class at 8,192 positions
/// (D-F1) sizes in 53.2 M steps, four fifths of it; the sizing stops at its first refusal.
pub const PALW_TIR_CLOSE_SIZING_WORK_CAP_V1: u64 = 1 << 26;

/// The refusal of a sizing that would pass [`PALW_TIR_CLOSE_SIZING_WORK_CAP_V1`] (or the cap it was
/// given), exactly as [`palw_tir_worst_closes_v1`] returns it.
pub const PALW_TIR_CLOSE_SIZING_OVER_CAP_V1: &str = "the close sizing exceeds its work cap";

/// The mover's ML-DSA-87 signature a dissection move carries.
const MOVE_SIGNATURE_BYTES: usize = 4_627;

/// What a carrier holds beside the move object it carries (the signer's key reference), as
/// [`crate::palw_tir_admission_v1::PALW_TIR_DISSECT_MOVE_FRAME_BYTES_V1`] allows it.
const MOVE_CARRIER_EXTRA_BYTES: u64 = 64;

/// How a close carries its parameter openings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirParamFormV1 {
    /// One `PalwArtifactOpeningV1` per inventory leaf, each with its full path (before §2.12.1).
    PerLeaf,
    /// ONE `PalwArtifactMultiproofV1` for all of them (`PalwTirConeRefutationV1::params`, the carried
    /// format).
    Multiproof,
}

/// **What one terminal close reads**, in the units it carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwTirCloseReadsV1 {
    /// Step leaves by index, with each one's value count.
    pub steps: BTreeMap<u64, u32>,
    /// The step leaves of `steps` that hold an `H`-carrying commit point's tile (read other than as a
    /// history row): their number moves with the alignment of the history.
    pub h_steps: BTreeSet<u64>,
    /// Hypothetical step leaves (both-mode's history tiles past the job's end): value counts only.
    pub loose_steps: Vec<u32>,
    /// Inventory leaves.
    pub params: BTreeSet<u32>,
    /// Location-free rows: per `(param, layer, the index element that names the row)`, the pieces of
    /// the row read — a row nobody can place, so priced apart from every other.
    pub wild_rows: BTreeMap<(u16, Option<u16>, (u32, u16, u16, usize)), BTreeSet<u64>>,
    /// Whether a prompt id or a generated token is read.
    pub token: bool,
    /// Elements of supplied nodes read, `(node, element)` — a dissection claim's values.
    pub supplied: BTreeSet<(u16, usize)>,
}

impl PalwTirCloseReadsV1 {
    /// `self ∪ other`, unit by unit.
    pub fn merge(&mut self, other: &Self) {
        self.steps.extend(other.steps.iter().map(|(k, v)| (*k, *v)));
        self.h_steps.extend(other.h_steps.iter().copied());
        self.loose_steps.extend_from_slice(&other.loose_steps);
        self.params.extend(other.params.iter().copied());
        for (k, v) in &other.wild_rows {
            self.wild_rows.entry(*k).or_default().extend(v.iter().copied());
        }
        self.token |= other.token;
        self.supplied.extend(other.supplied.iter().copied());
    }
}

/// One read request: the target, its elements, and the dissection's supplied nodes and range.
#[derive(Clone, Debug)]
pub struct PalwTirCloseRequestV1<'r> {
    pub ctx: DemandContext,
    pub target: u16,
    pub elements: &'r [usize],
    /// Nodes of the target's context whose elements are a claim's values (never read from a unit).
    pub supplied: &'r [u16],
    /// The target's reduction runs over history positions `[from, to)` only.
    pub range: Option<(usize, usize)>,
    /// Both-mode: every history row is read as BOTH its history tile and its row commit.
    pub both: bool,
}

struct CtxState {
    block: u8,
    layer: Option<u16>,
    h: usize,
    shapes: Vec<Vec<usize>>,
    demand: Vec<BTreeSet<usize>>,
}

struct Twin<'a> {
    space: &'a PalwTirStepSpaceV1,
    job_ctx: &'a PalwJobContextV2,
    job: PalwTirJobShapeV1,
    inventory: &'a PalwTirInventoryIndexV1,
    target: DemandContext,
    target_node: u16,
    supplied: BTreeSet<u16>,
    range: Option<(usize, usize)>,
    both: bool,
    ctxs: BTreeMap<(u32, u16), CtxState>,
    /// Everything read other than through a history row.
    reads: PalwTirCloseReadsV1,
    /// What is read through a history row (`hist_row`).
    hist: PalwTirCloseReadsV1,
    in_hist: bool,
    /// Step leaves already placed, `(through a history row, call, slot, position, tile)`.
    placed: HashSet<(bool, u32, u32, u32, u32)>,
    /// Runs of a computed node already demanded, `(pos, occurrence, node, start, stride, count)`.
    runs: HashSet<(u32, u16, u16, usize, usize, usize)>,
    /// The inventory piece read last, `(param, layer, first byte, end byte)`.
    last_piece: Option<(u16, Option<u16>, u64, u64)>,
    /// History-only: read nothing but what the target's context reads through a history row
    /// (`reaches_hist` marks the nodes whose cone, within the context, holds a `HistAppend`).
    hist_only: bool,
    reaches_hist: Vec<bool>,
    /// The node each history instance appends, `(state, layer) → (occurrence, node)`.
    hist_nodes: HashMap<(u16, Option<u16>), (u16, u16)>,
    /// The per-row pattern, when recorded: the history tiles `(instance, lane tile) → lanes` and the
    /// row commits `(occurrence, node, tile) → lanes` a row is read through.
    pattern: Option<HistPattern>,
    /// The work of placing one step leaf (its index walks the program's commit points and
    /// occurrences).
    leaf_cost: u64,
    work: u64,
    cap: u64,
}

/// What one history row is read through, whatever its position: its history tiles (one per
/// `h_tile` rows) and its row's commit tiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistPattern {
    /// `(history instance, lane tile) → lanes`.
    pub tiles: BTreeMap<(usize, u64), u32>,
    /// `(occurrence, node, tile) → lanes`.
    pub commits: BTreeMap<(u16, u16, u32), u32>,
}

/// The nodes of `block` whose cone within the block (through computed nodes; a commit point other
/// than `target` is a leaf) holds a `HistAppend`.
pub(crate) fn reaches_hist(block: &misaka_palw_tir::program::Block, target: u16) -> Vec<bool> {
    let mut r = vec![false; block.nodes.len()];
    for (j, n) in block.nodes.iter().enumerate() {
        r[j] = matches!(n.prim, Prim::HistAppend { .. })
            || n.inputs
                .iter()
                .any(|x| matches!(x, Ref::Node(i) if (*i == target || !block.nodes[*i as usize].commit) && r[*i as usize]));
    }
    r
}

pub(crate) fn strides(shape: &[usize]) -> Vec<usize> {
    let mut s = vec![1usize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1].saturating_mul(shape[i + 1]);
    }
    s
}

fn unravel(mut index: usize, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let s = (*s).max(1);
            let v = index / s;
            index %= s;
            v
        })
        .collect()
}

fn ravel(ix: &[usize], st: &[usize]) -> usize {
    ix.iter().zip(st).map(|(i, s)| i * s).sum()
}

/// The index of `o` (the output's coordinates) in an operand broadcast to it (spec 04b §2.3).
fn broadcast_index(o: &[usize], shape: &[usize]) -> usize {
    let st = strides(shape);
    let off = o.len() - shape.len();
    shape.iter().enumerate().map(|(k, d)| if *d == 1 { 0 } else { o[off + k] * st[k] }).sum()
}

pub(crate) fn element_count(shape: &[usize]) -> usize {
    shape.iter().product()
}

impl Twin<'_> {
    fn tick(&mut self, n: u64) -> Result<(), String> {
        self.work = self.work.saturating_add(n);
        if self.work > self.cap { Err(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.to_string()) } else { Ok(()) }
    }

    fn block_of(&self, occurrence: u16) -> Result<(u8, Option<u16>), String> {
        self.space.occurrences().get(occurrence as usize).copied().ok_or_else(|| "no such occurrence".to_string())
    }

    fn ctx(&mut self, key: DemandContext) -> Result<(), String> {
        let k = (key.pos, key.occurrence);
        if self.ctxs.contains_key(&k) {
            return Ok(());
        }
        let (block, layer) = self.block_of(key.occurrence)?;
        let h = history_length_v1(&self.space.info, block, key.pos).ok_or("the program's info does not cover the block")?;
        let b = &self.space.program.blocks[block as usize];
        self.tick(b.nodes.len() as u64)?;
        let shapes: Vec<Vec<usize>> = b.nodes.iter().map(|n| n.out.resolve(h)).collect();
        let demand = vec![BTreeSet::new(); b.nodes.len()];
        self.ctxs.insert(k, CtxState { block, layer, h, shapes, demand });
        Ok(())
    }

    fn is_leaf(&self, key: DemandContext, node: u16) -> bool {
        let Ok((block, _)) = self.block_of(key.occurrence) else { return false };
        let committed = self.space.program.blocks[block as usize].nodes.get(node as usize).is_some_and(|n| n.commit);
        let in_target = key == self.target;
        (committed && !(in_target && node == self.target_node)) || (in_target && self.supplied.contains(&node))
    }

    fn sink(&mut self) -> &mut PalwTirCloseReadsV1 {
        if self.in_hist { &mut self.hist } else { &mut self.reads }
    }

    /// Place step leaf `coord` (once), holding `value_count` lanes; `h_tile` marks an `H`-carrying
    /// commit point's tile.
    fn step_leaf(&mut self, coord: PalwStepCoordinateV1, value_count: u32, h_tile: bool) -> Result<(), String> {
        if !self.placed.insert((self.in_hist, coord.call_index, coord.node_slot, coord.position, coord.tile_index)) {
            return Ok(());
        }
        self.tick(self.leaf_cost)?;
        let i = self.space.leaf_index(self.job_ctx, &coord).ok_or_else(|| format!("{coord:?} is no leaf of the job"))?;
        let marks = h_tile && !self.in_hist;
        let sink = self.sink();
        sink.steps.insert(i, value_count);
        if marks {
            sink.h_steps.insert(i);
        }
        Ok(())
    }

    fn reserved_coord(&self, pos: u32, reserved: u32, tile: u64) -> PalwStepCoordinateV1 {
        let (call_index, position) = self.job.call_position(pos);
        PalwStepCoordinateV1 { call_index, node_slot: self.space.program_slots() + reserved, position, tile_index: tile as u32 }
    }

    /// A committed node's element (the court source's `node`).
    fn leaf_read(&mut self, key: DemandContext, node: u16, index: usize) -> Result<(), String> {
        if key == self.target && self.supplied.contains(&node) {
            self.reads.supplied.insert((node, index));
            return Ok(());
        }
        let (block, _) = self.block_of(key.occurrence)?;
        let tile_len = self.space.commit_tile_len(block, node).ok_or_else(|| format!("node {node} is not a commit point"))? as usize;
        let tile = index / tile_len;
        let (call_index, position) = self.job.call_position(key.pos);
        let slot = self.space.node_slot(key.occurrence as usize, node).ok_or("no such slot")?;
        let coord = PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: tile as u32 };
        if self.placed.contains(&(self.in_hist, coord.call_index, coord.node_slot, coord.position, coord.tile_index)) {
            return Ok(());
        }
        let h = history_length_v1(&self.space.info, block, key.pos).ok_or("no block info")?;
        let out = &self.space.program.blocks[block as usize].nodes[node as usize].out;
        let count = element_count(&out.resolve(h));
        let values = tile_len.min(count.saturating_sub(tile * tile_len)) as u32;
        self.step_leaf(coord, values, out.has_h())
    }

    fn param_read(&mut self, param: u16, layer: Option<u16>, index: usize) -> Result<(), String> {
        let d = self.space.program.params.get(param as usize).ok_or("no such param")?;
        let byte = (index as u64).saturating_mul(d.dtype.width() as u64);
        if let Some((p, l, from, to)) = self.last_piece
            && p == param
            && l == layer
            && (from..to).contains(&byte)
        {
            return Ok(());
        }
        let leaf = self.inventory.leaf_of(param, layer, byte).ok_or_else(|| format!("param {param} has no byte {byte}"))?;
        let (_, _, start, len) = self.inventory.piece_of(leaf).ok_or("an inventory leaf with no piece")?;
        self.last_piece = Some((param, layer, start as u64, start as u64 + len as u64));
        self.reads.params.insert(leaf);
        Ok(())
    }

    /// A `Fixed` state's element at the start of `pos` (the court's `state_at_start`, its source's
    /// checkpoint schedule).
    fn state_at_start(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> Result<(), String> {
        let c = self.space.layout.checkpoint_interval.max(1);
        let mut at = pos;
        loop {
            self.tick(1)?;
            if at == 0 {
                return Ok(());
            }
            if at.is_multiple_of(c) {
                let k = self.space.fixed_instance_index(state, layer).ok_or("a state with no checkpoint")?;
                let inst = self.space.fixed_instances()[k];
                let tile = index as u64 / inst.tile_lanes as u64;
                let values = (inst.tile_lanes as u64).min(inst.elements - tile * inst.tile_lanes as u64) as u32;
                let coord = self.reserved_coord(at - 1, k as u32, tile);
                return self.step_leaf(coord, values, false);
            }
            match state_writer_v1(&self.space.program, state, layer) {
                Some((occ, w)) => {
                    let key = DemandContext { pos: at - 1, occurrence: occ };
                    self.ctx(key)?;
                    if self.is_leaf(key, w) {
                        return self.leaf_read(key, w, index);
                    }
                    self.ctxs.get_mut(&(key.pos, key.occurrence)).expect("made above").demand[w as usize].insert(index);
                    return Ok(());
                }
                None => at -= 1,
            }
        }
    }

    /// A past history row (the court source's `hist_row`): its history tile once complete, the row's
    /// commit before — or, in both-mode, both. Recorded apart from every other read.
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> Result<(), String> {
        self.in_hist = true;
        let read = self.hist_row_reads(pos, state, layer, row_pos, index);
        self.in_hist = false;
        read
    }

    fn hist_row_reads(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> Result<(), String> {
        let h_tile = self.space.layout.h_tile.max(1);
        let tile_start = row_pos - row_pos % h_tile;
        let tile_end = tile_start + (h_tile - 1);
        let complete = tile_end < pos;
        if complete || self.both {
            let k = self.space.hist_instance_index(state, layer).ok_or("a history with no tiles")?;
            let inst = self.space.hist_instances()[k];
            let sub = index as u64 / inst.tile_lanes as u64;
            let first_lane = sub * inst.tile_lanes as u64;
            let row_lanes = (inst.elements - first_lane).min(inst.tile_lanes as u64);
            let values = (h_tile as u64 * row_lanes) as u32;
            if let Some(pattern) = self.pattern.as_mut() {
                pattern.tiles.insert((k, sub), values);
            }
            if tile_end < self.job.positions {
                let coord = self.reserved_coord(tile_end, (self.space.fixed_instances().len() + k) as u32, sub);
                self.step_leaf(coord, values, false)?;
            } else if self.placed.insert((true, u32::MAX, k as u32, tile_end, sub as u32)) {
                self.hist.loose_steps.push(values);
            }
        }
        if !complete || self.both {
            let (occurrence, node) = match self.hist_nodes.get(&(state, layer)) {
                Some(x) => *x,
                None => {
                    let (key, node) =
                        hist_row_node_v1(&self.space.program, state, layer, 0).ok_or("a history that appends nothing")?;
                    self.hist_nodes.insert((state, layer), (key.occurrence, node));
                    (key.occurrence, node)
                }
            };
            let key = DemandContext { pos: row_pos, occurrence };
            if self.pattern.is_some() {
                let (block, _) = self.block_of(occurrence)?;
                let tile_len = self.space.commit_tile_len(block, node).ok_or("a history row that is no commit point")? as usize;
                let h = history_length_v1(&self.space.info, block, row_pos).ok_or("no block info")?;
                let count = element_count(&self.space.program.blocks[block as usize].nodes[node as usize].out.resolve(h));
                let tile = index / tile_len;
                let values = tile_len.min(count.saturating_sub(tile * tile_len)) as u32;
                if let Some(pattern) = self.pattern.as_mut() {
                    pattern.commits.insert((occurrence, node, tile as u32), values);
                }
            }
            self.leaf_read(key, node, index)?;
        }
        Ok(())
    }

    /// Whether a history-only read skips operand `r` of context `key`.
    fn skips(&self, key: DemandContext, r: Ref) -> bool {
        self.hist_only && !(key == self.target && matches!(r, Ref::Node(j) if !self.is_leaf(key, j) && self.reaches_hist[j as usize]))
    }

    /// One operand element of context `key`.
    fn want(&mut self, key: DemandContext, r: Ref, index: usize) -> Result<(), String> {
        self.tick(1)?;
        if self.skips(key, r) {
            return Ok(());
        }
        let space = self.space;
        let program = &space.program;
        match r {
            Ref::Node(j) => {
                if self.is_leaf(key, j) {
                    self.leaf_read(key, j, index)
                } else {
                    self.ctxs.get_mut(&(key.pos, key.occurrence)).expect("live").demand[j as usize].insert(index);
                    Ok(())
                }
            }
            Ref::CarryIn(k) => {
                let (prev, node) = carry_in_node_v1(program, key, k).ok_or("a carry-in with no carry-out")?;
                self.leaf_read(prev, node, index)
            }
            Ref::Param(j) => {
                let d = program.params.get(j as usize).ok_or("no such param")?;
                let layer = if d.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                self.param_read(j, layer, index)
            }
            Ref::Const(_) => Ok(()),
            Ref::State(j) => {
                let s = program.states.get(j as usize).ok_or("no such state")?;
                let layer = if s.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                self.state_at_start(key.pos, j, layer, index)
            }
            Ref::Input(0) => {
                self.reads.token = true;
                Ok(())
            }
            Ref::Input(_) => Ok(()),
        }
    }

    /// `count` operand elements `start, start + stride, …` of context `key` — [`Self::want`] for each,
    /// with a computed node's run demanded once and a contiguous param run read piece by piece.
    fn want_run(&mut self, key: DemandContext, r: Ref, start: usize, stride: usize, count: usize) -> Result<(), String> {
        if count == 0 || self.skips(key, r) {
            return Ok(());
        }
        match r {
            Ref::Node(j) if !self.is_leaf(key, j) => {
                if count > 1 && !self.runs.insert((key.pos, key.occurrence, j, start, stride, count)) {
                    return self.tick(1);
                }
                self.tick(count as u64)?;
                let demand = &mut self.ctxs.get_mut(&(key.pos, key.occurrence)).expect("live").demand[j as usize];
                for k in 0..count {
                    demand.insert(start + k * stride);
                }
                Ok(())
            }
            Ref::Param(j) if stride == 1 => {
                let d = self.space.program.params.get(j as usize).ok_or("no such param")?;
                let layer = if d.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                let width = d.dtype.width() as u64;
                let (mut byte, end) = ((start as u64).saturating_mul(width), ((start + count) as u64).saturating_mul(width));
                while byte < end {
                    self.tick(1)?;
                    let leaf = self.inventory.leaf_of(j, layer, byte).ok_or_else(|| format!("param {j} has no byte {byte}"))?;
                    let (_, _, first, len) = self.inventory.piece_of(leaf).ok_or("an inventory leaf with no piece")?;
                    self.reads.params.insert(leaf);
                    byte = first as u64 + (len as u64).max(1);
                }
                Ok(())
            }
            _ => {
                for k in 0..count {
                    self.want(key, r, start + k * stride)?;
                }
                Ok(())
            }
        }
    }

    fn operand_shape(&self, key: DemandContext, r: Ref) -> Result<Vec<usize>, String> {
        let p = &self.space.program;
        let st = &self.ctxs[&(key.pos, key.occurrence)];
        Ok(match r {
            Ref::Node(j) => st.shapes.get(j as usize).ok_or("no such node")?.clone(),
            Ref::CarryIn(k) => p.blocks[st.block as usize].carry_in.get(k as usize).ok_or("no such carry-in")?.resolve(st.h),
            Ref::Param(j) => p.params.get(j as usize).ok_or("no such param")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Const(j) => p.consts.get(j as usize).ok_or("no such const")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::State(j) => p.states.get(j as usize).ok_or("no such state")?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Input(_) => Vec::new(),
        })
    }

    fn span(&self, key: DemandContext, node: u16, n: usize) -> (usize, usize) {
        match self.range {
            Some((from, to)) if key == self.target && node == self.target_node => (from.min(n), to.min(n)),
            _ => (0, n),
        }
    }

    /// What element `index` of computed node `node` reads (the court's `step`, over sets).
    fn visit(&mut self, key: DemandContext, node: u16, index: usize) -> Result<(), String> {
        self.tick(1)?;
        let space = self.space;
        let (block, h, ost, out_rank) = {
            let st = &self.ctxs[&(key.pos, key.occurrence)];
            let shape = &st.shapes[node as usize];
            (st.block, st.h, strides(shape), shape.len())
        };
        let n = &space.program.blocks[block as usize].nodes[node as usize];
        let inputs = &n.inputs;
        match n.prim {
            Prim::Reshape
            | Prim::Cast
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::StateWrite { .. } => self.want(key, inputs[0], index),
            Prim::Transpose { ref perm } => {
                let ish = self.operand_shape(key, inputs[0])?;
                let o = unravel(index, &ost);
                let mut j = vec![0usize; o.len()];
                for (k, p) in perm.iter().enumerate() {
                    j[*p as usize] = o[k];
                }
                self.want(key, inputs[0], ravel(&j, &strides(&ish)))
            }
            Prim::Slice { axis, start, .. } => {
                let ish = self.operand_shape(key, inputs[0])?;
                let mut o = unravel(index, &ost);
                o[axis as usize] += start as usize;
                self.want(key, inputs[0], ravel(&o, &strides(&ish)))
            }
            Prim::Concat { axis } => {
                let a = axis as usize;
                let mut o = unravel(index, &ost);
                for r in inputs.iter() {
                    let t = self.operand_shape(key, *r)?;
                    if o[a] < t[a] {
                        return self.want(key, *r, ravel(&o, &strides(&t)));
                    }
                    o[a] -= t[a];
                }
                Err("Concat: the inputs do not cover the output".into())
            }
            Prim::Broadcast => {
                let ish = self.operand_shape(key, inputs[0])?;
                let o = unravel(index, &ost);
                self.want(key, inputs[0], broadcast_index(&o, &ish))
            }
            Prim::Iota { .. } => Ok(()),
            Prim::Gather { axis, batch_dims } => {
                let (a, bd) = (axis as usize, batch_dims as usize);
                let dsh = self.operand_shape(key, inputs[0])?;
                let xsh = self.operand_shape(key, inputs[1])?;
                let m = xsh.len() - bd;
                let o = unravel(index, &ost);
                let mut xi: Vec<usize> = o[..bd].to_vec();
                xi.extend_from_slice(&o[a..a + m]);
                let xflat = ravel(&xi, &strides(&xsh));
                self.want(key, inputs[1], xflat)?;
                // The data element sits at a VALUE's index along `a`: a location-free row for a param
                // gathered along its rows, the whole fiber otherwise.
                let mut di: Vec<usize> = o[..a].to_vec();
                di.push(0);
                di.extend_from_slice(&o[a + m..]);
                let dst = strides(&dsh);
                match inputs[0] {
                    Ref::Const(_) => Ok(()),
                    Ref::Param(pj) if a == 0 && dsh.len() >= 2 => {
                        let d = &space.program.params[pj as usize];
                        let layer = if d.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                        let within = ravel(&di, &dst) as u64 * d.dtype.width() as u64;
                        let piece = within / PALW_TIR_ROW_PIECE_BYTES_V1;
                        self.reads.wild_rows.entry((pj, layer, (key.pos, key.occurrence, node, xflat))).or_default().insert(piece);
                        Ok(())
                    }
                    r => self.want_run(key, r, ravel(&di, &dst), dst[a], dsh[a]),
                }
            }
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                let o = unravel(index, &ost);
                for r in [inputs[0], inputs[1]] {
                    let sh = self.operand_shape(key, r)?;
                    self.want(key, r, broadcast_index(&o, &sh))?;
                }
                Ok(())
            }
            // A choice the court makes by a value: the condition and BOTH operands.
            Prim::Select => {
                let o = unravel(index, &ost);
                for r in [inputs[0], inputs[1], inputs[2]] {
                    let sh = self.operand_shape(key, r)?;
                    self.want(key, r, broadcast_index(&o, &sh))?;
                }
                Ok(())
            }
            Prim::MatMul => {
                let xs = self.operand_shape(key, inputs[0])?;
                let ys = self.operand_shape(key, inputs[1])?;
                let (xr, yr, or) = (xs.len(), ys.len(), out_rank);
                let (mm, kk, nn) = (xs[xr - 2], xs[xr - 1], ys[yr - 1]);
                let o = unravel(index, &ost);
                let (r, c) = (o[or - 2], o[or - 1]);
                let batch = &o[..or - 2];
                let xb = broadcast_index(batch, &xs[..xr - 2]);
                let yb = broadcast_index(batch, &ys[..yr - 2]);
                let (xo, yo) = (xb * mm * kk + r * kk, yb * kk * nn + c);
                let (from, to) = self.span(key, node, kk);
                let terms = to.saturating_sub(from);
                self.want_run(key, inputs[0], xo + from, 1, terms)?;
                self.want_run(key, inputs[1], yo + from * nn, nn, terms)
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                let a = axis as usize;
                let xs = self.operand_shape(key, inputs[0])?;
                let ist = strides(&xs);
                let mut o = unravel(index, &ost);
                let (from, to) = self.span(key, node, xs[a]);
                o[a] = 0;
                self.want_run(key, inputs[0], ravel(&o, &ist) + from * ist[a], ist[a], to.saturating_sub(from))
            }
            Prim::TopK { axis, .. } => {
                let a = axis as usize;
                let xs = self.operand_shape(key, inputs[0])?;
                let ist = strides(&xs);
                let mut o = unravel(index, &ost);
                o[a] = 0;
                self.want_run(key, inputs[0], ravel(&o, &ist), ist[a], xs[a])
            }
            Prim::HistAppend { state } => {
                let s = space.program.states.get(state as usize).ok_or("no such state")?;
                let StateKind::Hist { .. } = s.kind else { return Err("HistAppend on a Fixed state".into()) };
                let row_len: usize = s.shape.iter().map(|d| *d as usize).product();
                if row_len == 0 {
                    return Err("an empty history row".into());
                }
                let (t, within) = (index / row_len, index % row_len);
                if t + 1 == h {
                    self.want(key, inputs[0], within)
                } else {
                    let row_pos = (key.pos as usize + 1 - h + t) as u32;
                    let layer = if s.per_layer { self.ctxs[&(key.pos, key.occurrence)].layer } else { None };
                    self.hist_row(key.pos, state, layer, row_pos, within)
                }
            }
        }
    }

    fn run(&mut self) -> Result<(), String> {
        loop {
            // The largest context with pending demand: every demand into a context comes from a later
            // one (a replay reads an earlier position) or a later node of its own.
            let Some(k) = self.ctxs.iter().rev().find(|(_, s)| s.demand.iter().any(|d| !d.is_empty())).map(|(k, _)| *k) else {
                return Ok(());
            };
            let key = DemandContext { pos: k.0, occurrence: k.1 };
            let nodes = self.ctxs[&k].demand.len();
            for node in (0..nodes).rev() {
                let elements: Vec<usize> =
                    std::mem::take(&mut self.ctxs.get_mut(&k).expect("live").demand[node]).into_iter().collect();
                if elements.is_empty() {
                    continue;
                }
                if self.is_leaf(key, node as u16) {
                    for e in elements {
                        self.leaf_read(key, node as u16, e)?;
                    }
                    continue;
                }
                for e in elements {
                    self.visit(key, node as u16, e)?;
                }
            }
        }
    }
}

/// A twin read's outcome: everything but the history rows, the history rows, the row pattern when
/// asked for, and the work done.
struct Split {
    reads: PalwTirCloseReadsV1,
    hist: PalwTirCloseReadsV1,
    pattern: HistPattern,
    work: u64,
}

/// The work of placing one step leaf of `space`: its index walks the program's commit points and
/// occurrences.
pub(crate) fn leaf_cost_of(space: &PalwTirStepSpaceV1) -> u64 {
    let commits: u64 = space.program.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count() as u64).sum();
    16 + commits + space.occurrences().len() as u64
}

/// The twin's reads of `request`, split — history-only (`hist_only`) or recording the row pattern
/// (`pattern`) when asked.
#[allow(clippy::too_many_arguments)]
fn close_reads_split(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    request: &PalwTirCloseRequestV1<'_>,
    cap: u64,
    hist_only: bool,
    pattern: bool,
    leaf_cost: u64,
) -> Result<Split, String> {
    let job = space.job_shape(job_ctx).map_err(|e| e.to_string())?;
    let (block, _) = space.occurrences().get(request.ctx.occurrence as usize).copied().ok_or("no such occurrence")?;
    let mut twin = Twin {
        space,
        job_ctx,
        job,
        inventory,
        target: request.ctx,
        target_node: request.target,
        supplied: request.supplied.iter().copied().collect(),
        range: request.range,
        both: request.both,
        ctxs: BTreeMap::new(),
        reads: PalwTirCloseReadsV1::default(),
        hist: PalwTirCloseReadsV1::default(),
        in_hist: false,
        placed: HashSet::new(),
        runs: HashSet::new(),
        last_piece: None,
        hist_only,
        reaches_hist: if hist_only { reaches_hist(&space.program.blocks[block as usize], request.target) } else { Vec::new() },
        hist_nodes: HashMap::new(),
        pattern: pattern.then(HistPattern::default),
        leaf_cost,
        work: 0,
        cap,
    };
    // A request's setup: its seed, and (history-only) the block's reach.
    twin.tick(
        32 + request.elements.len() as u64 + if hist_only { space.program.blocks[block as usize].nodes.len() as u64 } else { 0 },
    )?;
    twin.ctx(request.ctx)?;
    {
        let st = twin.ctxs.get_mut(&(request.ctx.pos, request.ctx.occurrence)).expect("made above");
        let count = element_count(st.shapes.get(request.target as usize).ok_or("no such target")?);
        for e in request.elements {
            if *e >= count {
                return Err(format!("element {e} outside the target's {count}"));
            }
            st.demand[request.target as usize].insert(*e);
        }
    }
    // The target is computed even when committed: seed it, then run.
    twin.run()?;
    Ok(Split { reads: twin.reads, hist: twin.hist, pattern: twin.pattern.unwrap_or_default(), work: twin.work })
}

/// **One element-twin read, as the sizing asks it** — everything but the history rows, the history
/// rows, the row pattern (when `pattern`), and the work — history-only when `hist_only`. What the
/// range twin (`crate::palw_tir_close_range_v1`) is held equal to, request by request.
pub fn palw_tir_close_reads_split_v1(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    request: &PalwTirCloseRequestV1<'_>,
    cap: u64,
    hist_only: bool,
    pattern: bool,
) -> Result<(PalwTirCloseReadsV1, PalwTirCloseReadsV1, HistPattern, u64), String> {
    let Split { reads, hist, pattern, work } =
        close_reads_split(space, job_ctx, inventory, request, cap, hist_only, pattern, leaf_cost_of(space))?;
    Ok((reads, hist, pattern, work))
}

/// **The units the court's evaluation of `request` reads** (a superset where the court reads by a
/// value), over the job `job_ctx` of `space`, within `cap` steps of work; with the work done.
pub fn palw_tir_close_reads_v1(
    space: &PalwTirStepSpaceV1,
    job_ctx: &PalwJobContextV2,
    inventory: &PalwTirInventoryIndexV1,
    request: &PalwTirCloseRequestV1<'_>,
    cap: u64,
) -> Result<(PalwTirCloseReadsV1, u64), String> {
    let Split { mut reads, hist, work, .. } =
        close_reads_split(space, job_ctx, inventory, request, cap, false, false, leaf_cost_of(space))?;
    reads.merge(&hist);
    Ok((reads, work))
}

// =================================================================================================
// The price
// =================================================================================================

fn ceil_log2(n: u64) -> u64 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() as u64 }
}

/// A step leaf's carried preimage: version, coordinate, value count, the lanes (a length and four
/// bytes each).
pub(crate) fn step_preimage_bytes(values: u32) -> u64 {
    2 + 16 + 4 + 4 + 4 * values as u64
}

/// **How one class's closes are priced**: the frames, serialized, and the opening forms.
#[derive(Clone, Debug)]
pub struct PalwTirClosePriceV1<'a> {
    space: &'a PalwTirStepSpaceV1,
    inventory: &'a PalwTirInventoryIndexV1,
    /// The step tree's depth over the longest job (a path's siblings at most).
    depth: u64,
    inv_depth: u64,
    form: PalwTirParamFormV1,
    /// A read token's carriage: the larger of the prompt's and the decode pin's.
    token_bytes: u64,
    /// `CourtClosed` with a cone close that opens nothing, its leaf of no lane.
    frame: u64,
    /// `CourtTirRootClaimed` likewise, its claim of no reduction, signed, with its carrier's extra.
    root_frame: u64,
}

impl<'a> PalwTirClosePriceV1<'a> {
    /// The price of closes over `job_ctx` (the layout's longest job: the deepest tree) of `space`.
    pub fn new(
        space: &'a PalwTirStepSpaceV1,
        inventory: &'a PalwTirInventoryIndexV1,
        job_ctx: &PalwJobContextV2,
        form: PalwTirParamFormV1,
    ) -> Result<Self, String> {
        let job = space.job_shape(job_ctx).map_err(|e| e.to_string())?;
        let step_leaves = space.running_total(&job, job.positions).min(u64::MAX as u128) as u64;
        let depth = ceil_log2(step_leaves.max(1));
        let zero = Hash64::from_bytes([0; 64]);
        let mut job_context = job_ctx.clone();
        job_context.network_id = vec![0; PALW_V2_MAX_NETWORK_ID_BYTES];
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: Vec::new(),
            layout: space.layout.clone(),
            tokenizer_id: zero,
        };
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context,
            class,
            artifact_root: zero,
            full_logits_trace_root: zero,
            step_leaf_count: 0,
            step_merkle_root: zero,
            committed_execution_root: zero,
        };
        let empty = PalwTirConeRefutationV1 {
            binding,
            output_opening: PalwStepOpeningV1 { leaf_index: 0, leaf_hash: zero, siblings: vec![zero; depth as usize] },
            output_preimage: PalwStepTileLeafV1 {
                version: 0,
                coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
                value_count: 0,
                values_le: Vec::new(),
            },
            operands: PalwStepInputRowV1 { preimages: Vec::new(), run_siblings: Vec::new() },
            params: None,
            prompt_token_ids: Vec::new(),
            prompt_ids_openings: Vec::new(),
            decode_tokens: None,
        };
        let len = |o: &PalwConsensusObjectV2| borsh::to_vec(o).map(|v| v.len() as u64).map_err(|e| e.to_string());
        let frame = len(&PalwConsensusObjectV2::CourtClosed {
            session_id: zero,
            verdict: PalwCourtVerdictV2::ChallengerDefeated,
            proof: PalwCourtVerdictProofV2::TirCone { refutation: Box::new(empty.clone()) },
        })?;
        let root_frame = len(&PalwConsensusObjectV2::CourtTirRootClaimed {
            session_id: zero,
            root: Box::new(PalwTirRootClaimV1 {
                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                elements: Vec::new(),
                totals: PalwTirRangeClaimV1 { partials: Vec::new() },
                finalize: Box::new(empty),
            }),
            arity: 0,
            signature: vec![0; MOVE_SIGNATURE_BYTES],
        })? + MOVE_CARRIER_EXTRA_BYTES;
        Ok(Self {
            space,
            inventory,
            depth,
            inv_depth: ceil_log2(inventory.leaf_count().max(1) as u64),
            form,
            token_bytes: 64 + 16 + 4 * (space.layout.max_context as u64 + 1) + 64 * 64,
            frame,
            root_frame,
        })
    }

    /// The step tree's depth over the longest job: the most siblings one path carries.
    pub(crate) fn depth(&self) -> u64 {
        self.depth
    }

    /// The step leaves of `reads`: preimages, and a sibling set per contiguous run — at most one
    /// sibling a level for a single leaf and two for a longer run, at ANY alignment; with
    /// `every_leaf_a_run`, as if no two were adjacent (a bound no alignment or job exceeds).
    pub fn steps(&self, reads: &PalwTirCloseReadsV1, every_leaf_a_run: bool) -> u64 {
        let mut bytes = 0u64;
        let mut runs: Vec<u64> = Vec::new();
        let mut last: Option<u64> = None;
        for (i, v) in &reads.steps {
            bytes += step_preimage_bytes(*v);
            match runs.last_mut() {
                Some(len) if !every_leaf_a_run && last == Some(i.wrapping_sub(1)) => *len += 1,
                _ => runs.push(1),
            }
            last = Some(*i);
        }
        for v in &reads.loose_steps {
            bytes += step_preimage_bytes(*v);
            runs.push(1);
        }
        bytes + runs.iter().map(|len| 4 + 64 * if *len == 1 { self.depth } else { 2 * self.depth }).sum::<u64>()
    }

    /// The parameter carriage of `reads` beyond the frame's empty option, in Phase F's byte-exact
    /// format (`palw_artifact_operand_borsh_len_v1`, `palw_artifact_multiproof_borsh_len_v1`): every
    /// opened leaf's operand, and the siblings of each contiguous run of leaves at most one a level for
    /// a single leaf and two for a longer run — at ANY alignment, so the count holds for every layer's
    /// instance of a per-layer param, whose leaves sit elsewhere in the inventory (a proof's siblings
    /// are at most the sum of its runs'). A location-free row's pieces are a run of their own.
    pub fn params(&self, reads: &PalwTirCloseReadsV1) -> u64 {
        let program = &self.space.program;
        let run_siblings = |len: u64| if len == 1 { self.inv_depth } else { 2 * self.inv_depth };
        let mut lens: Vec<u64> = Vec::with_capacity(reads.params.len());
        let mut siblings = 0u64;
        let mut run = 0u64;
        let mut last: Option<u32> = None;
        for leaf in &reads.params {
            let (p, layer, _, len) = self.inventory.piece_of(*leaf).unwrap_or((0, Some(0), 0, PALW_TIR_ROW_PIECE_BYTES_V1 as u32));
            let name = program.params.get(p as usize).map_or(256, |d| d.name.len());
            lens.push(palw_artifact_operand_borsh_len_v1(name, layer.is_some(), len as usize));
            if last.is_some_and(|l| l.wrapping_add(1) == *leaf) {
                run += 1;
            } else {
                if run > 0 {
                    siblings += run_siblings(run);
                }
                run = 1;
            }
            last = Some(*leaf);
        }
        if run > 0 {
            siblings += run_siblings(run);
        }
        for ((p, layer, _), pieces) in &reads.wild_rows {
            let d = &program.params[*p as usize];
            let row_bytes = if d.shape.len() >= 2 {
                d.shape[1..].iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
            } else {
                d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
            };
            for k in pieces {
                let len = row_bytes.saturating_sub(k * PALW_TIR_ROW_PIECE_BYTES_V1).min(PALW_TIR_ROW_PIECE_BYTES_V1);
                lens.push(palw_artifact_operand_borsh_len_v1(d.name.len(), layer.is_some(), len as usize));
            }
            siblings += run_siblings(pieces.len() as u64);
        }
        if lens.is_empty() {
            return 0;
        }
        match self.form {
            // One opening per leaf, each with a whole path (`palw_artifact_opening_path_len_v1` is at
            // most the depth).
            PalwTirParamFormV1::PerLeaf => 4 + lens.iter().map(|operand| operand + 4 + 4 + 4 + 64 * self.inv_depth).sum::<u64>(),
            PalwTirParamFormV1::Multiproof => palw_artifact_multiproof_borsh_len_v1(lens, siblings),
        }
    }

    /// Every unit of `reads` beyond the frame.
    pub fn units(&self, reads: &PalwTirCloseReadsV1, every_leaf_a_run: bool) -> u64 {
        self.steps(reads, every_leaf_a_run) + self.params(reads) + if reads.token { self.token_bytes } else { 0 }
    }

    /// The alignment allowance of `reads`' `H`-carrying commit tiles: one more leaf, and a run of its
    /// own, for each of their runs.
    pub fn h_allowance(&self, reads: &PalwTirCloseReadsV1) -> u64 {
        let widest = reads.h_steps.iter().filter_map(|i| reads.steps.get(i)).copied().max().unwrap_or(0);
        let mut runs = 0u64;
        let mut last: Option<u64> = None;
        for i in &reads.h_steps {
            if last != Some(i.wrapping_sub(1)) {
                runs += 1;
            }
            last = Some(*i);
        }
        runs * (step_preimage_bytes(widest) + 4 + 2 * 64 * self.depth)
    }

    /// One whole close of `reads`, its disputed leaf of `leaf_values` lanes.
    pub fn close(&self, reads: &PalwTirCloseReadsV1, leaf_values: u32) -> u64 {
        self.frame + 4 * leaf_values as u64 + self.units(reads, false)
    }

    /// The frame alone: a close that opens nothing, its disputed leaf of `leaf_values` lanes.
    pub fn frame(&self, leaf_values: u32) -> u64 {
        self.frame + 4 * leaf_values as u64
    }

    /// A root claim's frame, as carried: the move object with nothing opened, its leaf of
    /// `leaf_values` lanes and `reductions` empty element and total lists.
    pub fn root_frame(&self, leaf_values: u32, reductions: usize) -> u64 {
        self.root_frame + 4 * leaf_values as u64 + 8 * reductions as u64
    }
}

// =================================================================================================
// The worst close of every commit point, over every job
// =================================================================================================

/// **One commit point's worst terminal close**, and, for a dissected one, its worst root claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirCloseBoundV1 {
    pub block: u8,
    pub node: u16,
    pub dissected: bool,
    /// The carried bytes of the worst terminal close (a whole tile's, or a bottom's).
    pub close_bytes: u64,
    /// For a dissected commit point: the carried bytes of the worst root claim as its carrier holds it
    /// (the move object, signed; past the first position, where the whole close is the executor's).
    pub root_claim_bytes: u64,
}

/// What the bound reads beside the class.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirCloseSizingV1 {
    pub form: PalwTirParamFormV1,
    /// Whether a history-reducing cone is dissected (the k-ary court is armed).
    pub court: bool,
    /// The most work the whole sizing may do.
    pub cap: u64,
    /// `(close, root claim)`: stop at the first commit point whose worst close or root claim passes
    /// these (admission needs no more than its first refusal); `None` sizes every commit point.
    pub stop_above: Option<(u64, u64)>,
}

fn tile_elements(first: usize, len: usize) -> Vec<usize> {
    (first..first + len).collect()
}

/// The entries of a read set (what merging or pricing it walks).
pub(crate) fn size_of_reads(reads: &PalwTirCloseReadsV1) -> u64 {
    (reads.steps.len() + reads.loose_steps.len() + reads.params.len() + reads.wild_rows.len() + reads.supplied.len()) as u64
}

/// The contiguous runs of step leaves a read set holds (a unit moved by an alignment adds at most one
/// leaf to each).
pub(crate) fn step_runs(reads: &PalwTirCloseReadsV1) -> u64 {
    let mut runs = reads.loose_steps.len() as u64;
    let mut last: Option<u64> = None;
    for i in reads.steps.keys() {
        if last != Some(i.wrapping_sub(1)) {
            runs += 1;
        }
        last = Some(*i);
    }
    runs
}

/// **The worst terminal close of every commit point of `space`'s class**, over every job of the
/// layout's longest (`job_ctx`), in carried bytes — see the module's doc for how every position is
/// reached without being enumerated.
pub fn palw_tir_worst_closes_v1(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
) -> Result<Vec<PalwTirCloseBoundV1>, String> {
    palw_tir_worst_closes_work_v1(space, inventory, job_ctx, sizing).map(|(bounds, _)| bounds)
}

/// [`palw_tir_worst_closes_v1`], with the work it did (what [`PALW_TIR_CLOSE_SIZING_WORK_CAP_V1`]
/// bounds).
pub fn palw_tir_worst_closes_work_v1(
    space: &PalwTirStepSpaceV1,
    inventory: &PalwTirInventoryIndexV1,
    job_ctx: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV1,
) -> Result<(Vec<PalwTirCloseBoundV1>, u64), String> {
    let program = &space.program;
    let job = space.job_shape(job_ctx).map_err(|e| e.to_string())?;
    let p_max = job.positions.checked_sub(1).ok_or("a job with no position")?;
    let price = PalwTirClosePriceV1::new(space, inventory, job_ctx, sizing.form)?;
    let c = space.layout.checkpoint_interval.max(1);
    let has_fixed = !space.fixed_instances().is_empty();
    // The position of `[lo, p_max]` whose replay is the longest: the largest `≡ C − 1 (mod C)`, or
    // the last one where none is (the replay then grows to the end).
    let rep_from = |lo: u32| -> u32 {
        if !has_fixed || p_max + 1 < c {
            return p_max;
        }
        let p = p_max - ((p_max + 1) % c);
        if p >= lo { p } else { p_max }
    };
    let h_tile = space.layout.h_tile.max(1) as usize;
    // The misalignment allowance of a part read at one alignment (the dissected bound's): one more
    // step leaf per run.
    let slack = |reads: &PalwTirCloseReadsV1| {
        let widest = reads.steps.values().chain(reads.loose_steps.iter()).copied().max().unwrap_or(0);
        step_runs(reads) * (step_preimage_bytes(widest) + 4 + 2 * 64 * price.depth)
    };
    // The work left, shared by the twin's reads and the counting outside them.
    let budget = std::cell::Cell::new(sizing.cap);
    let leaf_cost = leaf_cost_of(space);
    let charge = |n: u64| -> Result<(), String> {
        let left = budget.get();
        if n > left {
            return Err(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.to_string());
        }
        budget.set(left - n);
        Ok(())
    };
    let occurrences: Vec<(u8, Option<u16>)> = space.occurrences().to_vec();
    // One occurrence per block kind and predecessor: `pre`, the first two layers, `post`.
    let mut chosen: Vec<u16> = Vec::new();
    for (o, (b, _)) in occurrences.iter().enumerate() {
        if chosen.iter().filter(|x| occurrences[**x as usize].0 == *b).count() < 2 {
            chosen.push(o as u16);
        }
    }
    // A leaf priced as a run of its own.
    let lone = |values: u32| step_preimage_bytes(values) + 4 + 64 * price.depth;
    let mut out = Vec::new();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let (bi8, ni16) = (bi as u8, ni as u16);
            let tile_len = space.commit_tile_len(bi8, ni16).ok_or("a commit point with no tile")? as usize;
            let reductions = crate::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(block, ni16);
            let dissected = sizing.court && !reductions.is_empty();
            let h_axes = node.out.shape.iter().filter(|d| d.is_h()).count();
            if h_axes > 1 {
                return Err(format!("block {bi} node {ni} carries {h_axes} history axes; the sizing covers one"));
            }
            let has_h = h_axes == 1;
            let h_axis = node.out.shape.iter().position(|d| d.is_h());
            let inner = match h_axis {
                Some(a) => node.out.resolve(1)[a + 1..].iter().product::<usize>().max(1),
                None => 1,
            };
            let h_at = |pos: u32| history_length_v1(&space.info, bi8, pos).unwrap_or(1);
            let count_at = |pos: u32| element_count(&node.out.resolve(h_at(pos)));
            // Every tile of `pos`, `(first element, length)`: its elements are made one tile at a time.
            let tiles_at = |pos: u32| {
                let count = count_at(pos);
                (0..count).step_by(tile_len).map(move |first| (first, tile_len.min(count - first)))
            };
            // The first position at which a tile spans at most two row-parts.
            let mut p_late = 0u32;
            if has_h {
                while p_late <= p_max && h_at(p_late) * inner < tile_len {
                    p_late += 1;
                }
            }
            // An `H`-carrying tile whose cone reduces nothing over `H` is H-LOCAL (spec 04b §10.3,
            // *Dissectability is structural*): its element at history index `t` reads the history at
            // row `t` only, the same units at every position — so its history reads are counted from
            // one row's pattern.
            let local = !dissected && has_h && reductions.is_empty() && !has_fixed && p_late <= p_max;
            let mut worst = PalwTirCloseBoundV1 { block: bi8, node: ni16, dissected, close_bytes: 0, root_claim_bytes: 0 };
            for occ in chosen.iter().copied().filter(|o| occurrences[*o as usize].0 == bi8) {
                let twin = |pos: u32,
                            elements: &[usize],
                            supplied: &[u16],
                            target: u16,
                            range: Option<(usize, usize)>,
                            both: bool,
                            hist_only: bool,
                            pattern: bool| {
                    let request =
                        PalwTirCloseRequestV1 { ctx: DemandContext { pos, occurrence: occ }, target, elements, supplied, range, both };
                    let split = close_reads_split(space, job_ctx, inventory, &request, budget.get(), hist_only, pattern, leaf_cost)?;
                    budget.set(budget.get().saturating_sub(split.work));
                    Ok::<_, String>(split)
                };
                // A whole read: `(everything but the history rows, the history rows)`.
                macro_rules! read {
                    ($pos:expr, $elements:expr, $supplied:expr, $target:expr, $range:expr, $both:expr) => {{
                        let split = twin($pos, $elements, $supplied, $target, $range, $both, false, false)?;
                        (split.reads, split.hist)
                    }};
                }
                if local {
                    let pl = rep_from(p_late);
                    let h = h_at(pl);
                    let a = h_axis.expect("has H");
                    let outer: usize = node.out.resolve(h)[..a].iter().product::<usize>().max(1);
                    let t_len = (tile_len.div_ceil(inner) + 1).min(h);
                    let row = |o: usize, from: usize, to: usize| -> Vec<usize> {
                        (from..to).flat_map(|t| (0..inner).map(move |i| (o * h + t) * inner + i)).collect()
                    };
                    // Outside the history rows, per row: the window ending it (with the position's own
                    // row) and the one starting it — at every position the same units, but for the
                    // alignment of an `H`-carrying commit point read (its allowance).
                    let mut ends = Vec::with_capacity(outer);
                    let mut starts = Vec::with_capacity(outer);
                    // Through the history rows, per row: one row's history tiles and row commits.
                    let mut tiles_cost = Vec::with_capacity(outer);
                    let mut commits_cost = Vec::with_capacity(outer);
                    let mut patterns = Vec::with_capacity(outer);
                    for o in 0..outer {
                        ends.push(read!(pl, &row(o, h - t_len, h), &[], ni16, None, false).0);
                        starts.push(read!(pl, &row(o, 0, t_len), &[], ni16, None, false).0);
                        let pattern = if h >= 2 {
                            twin(pl, &row(o, h - 2, h - 1), &[], ni16, None, true, true, true)?.pattern
                        } else {
                            HistPattern::default()
                        };
                        tiles_cost.push(pattern.tiles.values().map(|v| lone(*v)).sum::<u64>());
                        commits_cost.push(pattern.commits.values().map(|v| lone(*v)).sum::<u64>());
                        patterns.push(pattern);
                    }
                    // The rows `lo..=hi` outside the history: the union of their windows.
                    let mut outside_of: BTreeMap<(usize, usize), u64> = BTreeMap::new();
                    let mut outside = |lo: usize, hi: usize| -> Result<u64, String> {
                        if let Some(units) = outside_of.get(&(lo, hi)) {
                            return Ok(*units);
                        }
                        let mut u = PalwTirCloseReadsV1::default();
                        for o in lo..=hi {
                            charge(16 + size_of_reads(&ends[o]) + size_of_reads(&starts[o]))?;
                            u.merge(&ends[o]);
                            u.merge(&starts[o]);
                        }
                        let units = price.units(&u, false) + price.h_allowance(&u);
                        outside_of.insert((lo, hi), units);
                        Ok(units)
                    };
                    // Early: every tile at every position, its history rows counted exactly (a row
                    // through its complete history tile, or its row's commit before).
                    for p in 0..p_late.min(p_max + 1) {
                        let hp = h_at(p);
                        let row_len = hp * inner;
                        let count = count_at(p);
                        for first in (0..count).step_by(tile_len) {
                            let last = (first + tile_len).min(count) - 1;
                            let (o_lo, o_hi) = (first / row_len, last / row_len);
                            let mut tiles: BTreeSet<(u32, usize, u64)> = BTreeSet::new();
                            let mut commits: BTreeSet<(u32, u16, u16, u32)> = BTreeSet::new();
                            let mut hist_cost = 0u64;
                            charge(16 + (o_hi - o_lo + 1) as u64)?;
                            for (o, pattern) in patterns.iter().enumerate().take(o_hi + 1).skip(o_lo) {
                                let t_lo = if o == o_lo { (first % row_len) / inner } else { 0 };
                                let t_hi = if o == o_hi { (last % row_len) / inner + 1 } else { hp };
                                let rows = t_hi.min(hp.saturating_sub(1)).saturating_sub(t_lo) as u64;
                                charge(rows * (1 + (pattern.tiles.len() + pattern.commits.len()) as u64))?;
                                for t in t_lo..t_hi.min(hp.saturating_sub(1)) {
                                    let row_pos = (p as usize + 1 - hp + t) as u32;
                                    let group = row_pos / h_tile as u32;
                                    if (group as usize + 1) * h_tile - 1 < p as usize {
                                        for ((k, sub), v) in &pattern.tiles {
                                            if tiles.insert((group, *k, *sub)) {
                                                hist_cost += lone(*v);
                                            }
                                        }
                                    } else {
                                        for ((oc, n, tile), v) in &pattern.commits {
                                            if commits.insert((row_pos, *oc, *n, *tile)) {
                                                hist_cost += lone(*v);
                                            }
                                        }
                                    }
                                }
                            }
                            let close = price.frame((last + 1 - first) as u32) + outside(o_lo, o_hi)? + hist_cost;
                            worst.close_bytes = worst.close_bytes.max(close);
                        }
                    }
                    // Late: at most two row-parts, the END of row `o` and the START of row `o + 1`; a
                    // part's history rows are at most `t_len` rows of at most `groups` history tiles,
                    // priced in both forms (a bound for every position and alignment).
                    let groups = (t_len.saturating_sub(1)).div_ceil(h_tile) + 1;
                    let part = |o: usize| t_len as u64 * commits_cost[o] + groups as u64 * tiles_cost[o];
                    let mut late = 0u64;
                    for o in 0..outer {
                        late = late.max(outside(o, o)? + part(o));
                        if o + 1 < outer {
                            late = late.max(outside(o, o + 1)? + part(o) + part(o + 1));
                        }
                    }
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + late);
                    continue;
                }
                // Every tile's whole close, exactly, at `pos` (with the alignment allowance where the
                // position stands for others).
                let whole_at = |read: &mut dyn FnMut(
                    u32,
                    &[usize],
                    &[u16],
                    u16,
                    Option<(usize, usize)>,
                    bool,
                )
                    -> Result<(PalwTirCloseReadsV1, PalwTirCloseReadsV1), String>,
                                pos: u32,
                                allowance: bool|
                 -> Result<u64, String> {
                    let mut worst = 0u64;
                    for (first, len) in tiles_at(pos) {
                        let tile = tile_elements(first, len);
                        let (mut reads, hist) = read(pos, &tile, &[], ni16, None, false)?;
                        reads.merge(&hist);
                        let extra = if allowance { price.h_allowance(&reads) } else { 0 };
                        worst = worst.max(price.close(&reads, tile.len() as u32) + extra);
                    }
                    Ok(worst)
                };
                if !dissected {
                    if !has_h {
                        worst.close_bytes = worst.close_bytes.max(whole_at(
                            &mut |p: u32, e: &[usize], sp: &[u16], t: u16, r: Option<(usize, usize)>, b: bool| {
                                let x = twin(p, e, sp, t, r, b, false, false)?;
                                Ok((x.reads, x.hist))
                            },
                            rep_from(0),
                            true,
                        )?);
                        continue;
                    }
                    for p in 0..p_late.min(p_max + 1) {
                        worst.close_bytes = worst.close_bytes.max(whole_at(
                            &mut |p: u32, e: &[usize], sp: &[u16], t: u16, r: Option<(usize, usize)>, b: bool| {
                                let x = twin(p, e, sp, t, r, b, false, false)?;
                                Ok((x.reads, x.hist))
                            },
                            p,
                            false,
                        )?);
                    }
                    if p_late > p_max {
                        continue;
                    }
                    // Late: at most two row-parts — the END of row `o` (with the position's own row)
                    // and the START of row `o + 1`.
                    let pl = rep_from(p_late);
                    let h = h_at(pl);
                    let a = h_axis.expect("has H");
                    let outer: usize = node.out.resolve(h)[..a].iter().product::<usize>().max(1);
                    let t_len = (tile_len.div_ceil(inner) + 1).min(h);
                    let row = |o: usize, from: usize, to: usize| -> Vec<usize> {
                        (from..to).flat_map(|t| (0..inner).map(move |i| (o * h + t) * inner + i)).collect()
                    };
                    // Outside the history rows: the window ending each row and the one starting it.
                    let mut ends = Vec::with_capacity(outer);
                    let mut starts = Vec::with_capacity(outer);
                    for o in 0..outer {
                        ends.push(read!(pl, &row(o, h - t_len, h), &[], ni16, None, false).0);
                        starts.push(read!(pl, &row(o, 0, t_len), &[], ni16, None, false).0);
                    }
                    let mut outside = 0u64;
                    for o in 0..outer {
                        for part in [&ends[o], &starts[o]] {
                            outside = outside.max(price.units(part, false) + price.h_allowance(part));
                        }
                        if o + 1 < outer {
                            let mut pair = ends[o].clone();
                            pair.merge(&starts[o + 1]);
                            outside = outside.max(price.units(&pair, false) + price.h_allowance(&pair));
                        }
                    }
                    // Through the history rows: every `T`-window of every row at every alignment of the
                    // history tiles, in both-mode, every leaf a run of its own.
                    let mut history = 0u64;
                    for o in 0..outer {
                        for r in 0..=h_tile.min(h) {
                            let end = h - r;
                            let start = end.saturating_sub(t_len);
                            if start >= end {
                                continue;
                            }
                            let hist = twin(pl, &row(o, start, end), &[], ni16, None, true, true, false)?.hist;
                            history = history.max(price.units(&hist, true));
                        }
                    }
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + outside + 2 * history);
                    continue;
                }
                // Position 0: the history is one row, the dissection has no rounds, and the whole close
                // is the executor's move — it must be carriable.
                worst.close_bytes = worst.close_bytes.max(whole_at(
                    &mut |p: u32, e: &[usize], sp: &[u16], t: u16, r: Option<(usize, usize)>, b: bool| {
                        let x = twin(p, e, sp, t, r, b, false, false)?;
                        Ok((x.reads, x.hist))
                    },
                    0,
                    false,
                )?);
                // Every other position: the root claim (one carrier) and the bottom (the cap).
                let mut positions: BTreeSet<u32> = BTreeSet::new();
                if p_max >= 1 {
                    positions.insert(1);
                }
                positions.insert(rep_from(1));
                if has_h {
                    positions.extend(1..p_late.min(p_max + 1));
                    if p_late <= p_max {
                        positions.insert(rep_from(p_late.max(1)));
                    }
                }
                for pos in positions.into_iter().filter(|p| *p >= 1) {
                    let late_h = has_h && pos >= p_late;
                    let parts = if late_h { 2 } else { 1 };
                    let mut root_worst = 0u64;
                    let mut bottom_worst = 0u64;
                    for (first, len) in tiles_at(pos) {
                        let tile = tile_elements(first, len);
                        let supplied: Vec<u16> = reductions.iter().copied().filter(|r| *r != ni16).collect();
                        let (mut finalize, hist) = read!(pos, &tile, &supplied, ni16, None, false);
                        finalize.merge(&hist);
                        let mut closure: BTreeMap<u16, BTreeSet<usize>> = BTreeMap::new();
                        for (n, e) in &finalize.supplied {
                            closure.entry(*n).or_default().insert(*e);
                        }
                        if reductions.contains(&ni16) {
                            closure.entry(ni16).or_default().extend(tile.iter().copied());
                        }
                        let mut root_reads = finalize.clone();
                        let mut probed: BTreeSet<(u16, usize)> = BTreeSet::new();
                        // The closure's probes at the history's first row, a reduction's pending
                        // elements at once (as the builder asks them).
                        loop {
                            let mut pending: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
                            for (n, es) in &closure {
                                for e in es {
                                    if probed.insert((*n, *e)) {
                                        pending.entry(*n).or_default().push(*e);
                                    }
                                }
                            }
                            if pending.is_empty() {
                                break;
                            }
                            for (r, es) in pending {
                                let others: Vec<u16> = reductions.iter().copied().filter(|x| *x != r).collect();
                                let (mut probe, hist) = read!(pos, &es, &others, r, Some((0, 1)), false);
                                probe.merge(&hist);
                                for (n, e2) in &probe.supplied {
                                    closure.entry(*n).or_default().insert(*e2);
                                }
                                root_reads.merge(&probe);
                            }
                        }
                        let values: u64 = closure.values().map(|s| s.len() as u64).sum();
                        root_reads.supplied.clear();
                        let root_units = price.units(&root_reads, false) + if late_h { slack(&root_reads) } else { 0 };
                        root_worst = root_worst.max(root_units * parts + values * parts * 20);
                        // The bottom: every reduction over the first and the last history tile, in
                        // both-mode — the two together bound any tile at any position.
                        let h = h_at(pos);
                        let tiles_h = h.div_ceil(h_tile);
                        let mut bottom = 0u64;
                        for tau in [0usize, tiles_h.saturating_sub(1)].into_iter().collect::<BTreeSet<_>>() {
                            let from = tau * h_tile;
                            let to = ((tau + 1) * h_tile).min(h);
                            let mut reads = PalwTirCloseReadsV1::default();
                            for r in &reductions {
                                let es: Vec<usize> = closure.get(r).map(|s| s.iter().copied().collect()).unwrap_or_default();
                                if es.is_empty() {
                                    continue;
                                }
                                let others: Vec<u16> = reductions.iter().copied().filter(|x| x != r).collect();
                                let (part, hist) = read!(pos, &es, &others, *r, Some((from, to)), true);
                                reads.merge(&part);
                                reads.merge(&hist);
                            }
                            reads.supplied.clear();
                            bottom += price.units(&reads, false) + if late_h { slack(&reads) } else { 0 };
                        }
                        bottom_worst = bottom_worst.max(bottom * parts);
                    }
                    worst.root_claim_bytes =
                        worst.root_claim_bytes.max(price.root_frame(tile_len as u32, reductions.len()) + root_worst);
                    worst.close_bytes = worst.close_bytes.max(price.frame(tile_len as u32) + bottom_worst);
                }
            }
            let past = sizing
                .stop_above
                .is_some_and(|(close, root)| worst.close_bytes > close || (worst.dissected && worst.root_claim_bytes > root));
            out.push(worst);
            if past {
                return Ok((out, sizing.cap - budget.get()));
            }
        }
    }
    Ok((out, sizing.cap - budget.get()))
}
