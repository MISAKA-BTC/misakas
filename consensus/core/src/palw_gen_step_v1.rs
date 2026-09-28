//! **The one step tree of a pipeline claim** (RFC-0003 §I.2.3, PALW-GEN-3): every stage's leaves,
//! stage-major, so every leaf is adjudicated from leaves that precede it.
//!
//! A stage's leaves, position by position (`p = 0 … T − 1`):
//!
//! * **commit leaves** — every commit point of every occurrence (`pre`, each layer, `post`) of the
//!   stage's version-2 program, in slot order, cut into its layout's commit tile (Phase F D5): the
//!   values the interpreter commits (`StepOutputV2::commits`), row-major at the position's `H`. The
//!   text stage's `post` commits (its logits) are leaves only at the positions whose logits the
//!   decode consumes (`p ≥ |prompt| − 1`), as a Phase F text class's are;
//! * **state leaves** — after every `C`-th position (`(p + 1) mod C = 0`), every `Fixed` state
//!   instance the program does not write in `post` (a `post`-written state is a commit leaf at every
//!   position already, NF-29), cut into its layout's state tile.
//!
//! A leaf is `H64(key "misaka-palw/gen/step-leaf/v1", borsh(coord) ‖ le32(n) ‖ lanes)`, its lanes
//! four little-endian bytes each (Phase F's lane encoding); a stage's root binds its leaf count over
//! a keyed Merkle tree (index order, odd node promoted); and the claim's step root binds every
//! stage's root in stage order. A leaf's proof is its stage's path and every stage root.
//!
//! The enumeration is structural — the programs, the layouts and the job's trip counts — so a court
//! derives any leaf's index from its coordinate without the run.

use std::collections::BTreeMap;

use crate::Hash64;
use crate::palw_gen_class_v1::PalwGenClassV1;
use crate::palw_tir_class_v1::PalwTirLayoutV1;
use misaka_palw_tir::demand::{DemandContext, history_length_v1};
use misaka_palw_tir::pipeline::{StageRun, TirPipelineV1, TripRule};
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::types::DType;
use misaka_palw_tir::validate_v2::{ProgramInfoV2, validate_v2};

pub const PALW_GEN_STEP_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/gen/step-leaf/v1";
pub const PALW_GEN_STEP_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/gen/step-node/v1";
pub const PALW_GEN_STAGE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/gen/stage-root/v1";
pub const PALW_GEN_STEP_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/gen/step-root/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// What a leaf holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwGenLeafKindV1 {
    /// A tile of commit point `node` of occurrence `occurrence` (spec 04b §3.3) at the position.
    Commit { occurrence: u16, node: u16 },
    /// A tile of `Fixed` state instance `(state, layer)` after the position: a checkpoint.
    State { state: u16, layer: Option<u16> },
}

/// Where a leaf sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenLeafCoordV1 {
    pub stage: u8,
    pub pos: u32,
    pub kind: PalwGenLeafKindV1,
    pub tile: u32,
}

/// One leaf of a stage: where it sits, its lanes' dtype and its elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenLeafV1 {
    pub coord: PalwGenLeafCoordV1,
    pub dtype: DType,
    pub first_element: u64,
    pub value_count: u32,
}

/// Why a step space or a leaf is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenStepErrorV1 {
    #[error("the class: {0}")]
    Class(String),
    #[error("stage {stage}: {msg}")]
    Stage { stage: u8, msg: String },
    #[error("a leaf: {0}")]
    Leaf(String),
}

/// **One stage's step space.**
#[derive(Clone, Debug)]
pub struct PalwGenStageSpaceV1 {
    pub stage: u8,
    pub program: TirProgramV2,
    pub info: ProgramInfoV2,
    pub layout: PalwTirLayoutV1,
    /// The positions the job runs the stage at.
    pub trip: u32,
    /// The text stage's first position whose logits the decode consumes (`|prompt| − 1`).
    pub consumed_from: Option<u32>,
    /// The `(block, node)` commit tile of every commit point.
    tiles: BTreeMap<(u8, u16), u32>,
    leaves: Vec<PalwGenLeafV1>,
    index: BTreeMap<PalwGenLeafCoordV1, u64>,
}

impl PalwGenStageSpaceV1 {
    fn new(
        stage: u8,
        program: &TirProgramV2,
        layout: &PalwTirLayoutV1,
        trip: u32,
        consumed_from: Option<u32>,
    ) -> Result<Self, PalwGenStepErrorV1> {
        let bad = |msg: String| PalwGenStepErrorV1::Stage { stage, msg };
        let info = validate_v2(program).map_err(|e| bad(e.to_string()))?;
        if layout.checkpoint_interval == 0 {
            return Err(bad("a zero checkpoint interval".into()));
        }
        // Commit tiles, in (block, node) order over the program's blocks (Phase F D5).
        let mut tiles = BTreeMap::new();
        let mut k = 0usize;
        for (bi, block) in program.blocks.iter().enumerate() {
            for (ni, node) in block.nodes.iter().enumerate() {
                if node.commit {
                    let t = *layout.commit_tiles.get(k).ok_or_else(|| bad("fewer commit tiles than commit points".into()))?;
                    if t == 0 {
                        return Err(bad("a zero commit tile".into()));
                    }
                    tiles.insert((bi as u8, ni as u16), t);
                    k += 1;
                }
            }
        }
        let post_written: Vec<u16> = info.post_writes.iter().map(|(_, s)| *s).collect();
        let occurrences = program.occurrences();
        let post_occ = occurrences.len() - 1;
        let mut leaves = Vec::new();
        for pos in 0..trip {
            for (occ, (block, _layer)) in occurrences.iter().enumerate() {
                if occ == post_occ && consumed_from.is_some_and(|c| pos < c) {
                    continue;
                }
                let h = history_length_v1(&info.v1, *block, pos).unwrap_or(1) as u64;
                for (ni, node) in program.blocks[*block as usize].nodes.iter().enumerate() {
                    if !node.commit {
                        continue;
                    }
                    let tile = tiles[&(*block, ni as u16)] as u64;
                    let n = node.out.elements_at(h);
                    let kind = PalwGenLeafKindV1::Commit { occurrence: occ as u16, node: ni as u16 };
                    for t in 0..n.div_ceil(tile) {
                        let first = t * tile;
                        leaves.push(PalwGenLeafV1 {
                            coord: PalwGenLeafCoordV1 { stage, pos, kind, tile: t as u32 },
                            dtype: node.out.dtype,
                            first_element: first,
                            value_count: (n - first).min(tile) as u32,
                        });
                    }
                }
            }
            if (pos + 1).is_multiple_of(layout.checkpoint_interval) {
                for (j, st) in program.states.iter().enumerate() {
                    if !matches!(st.kind, StateKind::Fixed { .. }) || post_written.contains(&(j as u16)) {
                        continue;
                    }
                    let tile = *layout.state_tiles.get(j).ok_or_else(|| bad("fewer state tiles than states".into()))? as u64;
                    if tile == 0 {
                        return Err(bad("a zero state tile".into()));
                    }
                    let n: u64 = st.shape.iter().map(|d| *d as u64).product();
                    let layers: Vec<Option<u16>> =
                        if st.per_layer { (0..program.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
                    for layer in layers {
                        let kind = PalwGenLeafKindV1::State { state: j as u16, layer };
                        for t in 0..n.div_ceil(tile) {
                            let first = t * tile;
                            leaves.push(PalwGenLeafV1 {
                                coord: PalwGenLeafCoordV1 { stage, pos, kind, tile: t as u32 },
                                dtype: st.dtype,
                                first_element: first,
                                value_count: (n - first).min(tile) as u32,
                            });
                        }
                    }
                }
            }
        }
        let index = leaves.iter().enumerate().map(|(i, l)| (l.coord, i as u64)).collect();
        Ok(Self { stage, program: program.clone(), info, layout: layout.clone(), trip, consumed_from, tiles, leaves, index })
    }

    pub fn leaves(&self) -> &[PalwGenLeafV1] {
        &self.leaves
    }

    pub fn leaf_index(&self, coord: &PalwGenLeafCoordV1) -> Option<u64> {
        self.index.get(coord).copied()
    }

    /// The commit tile of commit point `(block, node)`.
    pub fn commit_tile(&self, block: u8, node: u16) -> Option<u32> {
        self.tiles.get(&(block, node)).copied()
    }

    /// The block occurrence `occurrence` runs.
    pub fn occurrence_block(&self, occurrence: u16) -> Option<u8> {
        self.program.occurrences().get(occurrence as usize).map(|(b, _)| *b)
    }

    /// The commit leaf holding element `element` of node `node` of `ctx`.
    pub fn commit_leaf_of(&self, ctx: DemandContext, node: u16, element: u64) -> Option<(u64, usize)> {
        let block = self.occurrence_block(ctx.occurrence)?;
        let tile = self.commit_tile(block, node)? as u64;
        let coord = PalwGenLeafCoordV1 {
            stage: self.stage,
            pos: ctx.pos,
            kind: PalwGenLeafKindV1::Commit { occurrence: ctx.occurrence, node },
            tile: (element / tile) as u32,
        };
        Some((self.leaf_index(&coord)?, (element % tile) as usize))
    }

    /// Every leaf's values from the stage's run: its commit records and its states.
    pub fn leaf_values(&self, run: &StageRun) -> Result<Vec<Vec<i128>>, PalwGenStepErrorV1> {
        let bad = |msg: String| PalwGenStepErrorV1::Stage { stage: self.stage, msg };
        if run.steps.len() as u32 != self.trip || run.fixed_after.len() as u32 != self.trip {
            return Err(bad(format!("a run of {} positions for a trip of {}", run.steps.len(), self.trip)));
        }
        let post_occ = (self.program.occurrences().len() - 1) as u16;
        let occ_of = |block: u8, layer: Option<u16>| -> u16 {
            match layer {
                Some(l) => l + 1,
                None if block == self.program.schedule.pre => 0,
                None => post_occ,
            }
        };
        let mut out = Vec::with_capacity(self.leaves.len());
        for leaf in &self.leaves {
            let p = leaf.coord.pos as usize;
            let (first, n) = (leaf.first_element as usize, leaf.value_count as usize);
            let values = match leaf.coord.kind {
                PalwGenLeafKindV1::Commit { occurrence, node } => {
                    let rec = run.steps[p]
                        .commits
                        .iter()
                        .find(|c| c.node == node && occ_of(c.block, c.layer) == occurrence)
                        .ok_or_else(|| bad(format!("no commit of node {node} at occurrence {occurrence}, position {p}")))?;
                    rec.value.data.get(first..first + n).ok_or_else(|| bad("a commit shorter than its leaves".into()))?.to_vec()
                }
                PalwGenLeafKindV1::State { state, layer } => match run.fixed_after[p].get(&(state, layer)) {
                    Some(t) => t.data.get(first..first + n).ok_or_else(|| bad("a state shorter than its leaves".into()))?.to_vec(),
                    None => vec![0; n],
                },
            };
            out.push(values);
        }
        Ok(out)
    }
}

/// **A pipeline claim's step space**: every stage's, in stage order.
#[derive(Clone, Debug)]
pub struct PalwGenStepSpaceV1 {
    pub stages: Vec<PalwGenStageSpaceV1>,
}

impl PalwGenStepSpaceV1 {
    /// The space of a job whose stages run `trips` positions; `prompt_len` is the text stage's
    /// prompt (its logits are leaves from `prompt_len − 1` on).
    pub fn new(
        pipeline: &TirPipelineV1,
        programs: &[TirProgramV2],
        layouts: &[PalwTirLayoutV1],
        trips: &[u32],
        prompt_len: u32,
    ) -> Result<Self, PalwGenStepErrorV1> {
        if layouts.len() != pipeline.stages.len() || trips.len() != pipeline.stages.len() {
            return Err(PalwGenStepErrorV1::Class("one layout and one trip count per stage".into()));
        }
        let stages = pipeline
            .stages
            .iter()
            .enumerate()
            .map(|(s, st)| {
                let text = matches!(st.trip, TripRule::TextStream);
                let consumed = text.then(|| prompt_len.saturating_sub(1));
                PalwGenStageSpaceV1::new(s as u8, &programs[st.program as usize], &layouts[s], trips[s], consumed)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { stages })
    }

    /// The space of a class's job.
    pub fn of_class(
        class: &PalwGenClassV1,
        pipeline: &TirPipelineV1,
        programs: &[TirProgramV2],
        trips: &[u32],
        prompt_len: u32,
    ) -> Result<Self, PalwGenStepErrorV1> {
        Self::new(pipeline, programs, &class.layouts, trips, prompt_len)
    }
}

/// **A leaf's hash**: its coordinate, its count and its lanes (four little-endian bytes each).
pub fn palw_gen_step_leaf_hash_v1(leaf: &PalwGenLeafV1, values: &[i128]) -> Result<Hash64, PalwGenStepErrorV1> {
    if values.len() != leaf.value_count as usize {
        return Err(PalwGenStepErrorV1::Leaf(format!("{} values for a leaf of {}", values.len(), leaf.value_count)));
    }
    let lanes =
        crate::palw_tir_step_v1::palw_tir_lanes_le_v1(leaf.dtype, values).map_err(|e| PalwGenStepErrorV1::Leaf(e.to_string()))?;
    let coord = borsh::to_vec(&leaf.coord).expect("a coordinate is borsh-serializable");
    Ok(keyed64(PALW_GEN_STEP_LEAF_DOMAIN_V1, &[&coord, &leaf.value_count.to_le_bytes(), &lanes]))
}

fn node_hash(left: &Hash64, right: &Hash64) -> Hash64 {
    keyed64(PALW_GEN_STEP_NODE_DOMAIN_V1, &[left.as_byte_slice(), right.as_byte_slice()])
}

fn merkle(leaves: &[Hash64]) -> Hash64 {
    if leaves.is_empty() {
        return Hash64::from_bytes([0; 64]);
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = level.chunks(2).map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] }).collect();
    }
    level[0]
}

/// A stage's root over its leaf hashes.
pub fn palw_gen_stage_root_v1(stage: u8, leaves: &[Hash64]) -> Hash64 {
    keyed64(PALW_GEN_STAGE_ROOT_DOMAIN_V1, &[&[stage], &(leaves.len() as u64).to_le_bytes(), merkle(leaves).as_byte_slice()])
}

/// The claim's step root over every stage's root, in stage order.
pub fn palw_gen_step_root_v1(stage_roots: &[Hash64]) -> Hash64 {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(stage_roots.len() + 1);
    let n = (stage_roots.len() as u16).to_le_bytes();
    parts.push(&n);
    for r in stage_roots {
        parts.push(r.as_byte_slice());
    }
    keyed64(PALW_GEN_STEP_ROOT_DOMAIN_V1, &parts)
}

/// **A leaf's authentication path** in its stage's tree: its siblings, bottom-up; a level where the
/// node is the promoted odd one contributes nothing.
pub fn palw_gen_leaf_path_v1(leaves: &[Hash64], index: usize) -> Option<Vec<Hash64>> {
    if index >= leaves.len() {
        return None;
    }
    let (mut path, mut level, mut i) = (Vec::new(), leaves.to_vec(), index);
    while level.len() > 1 {
        let sibling = i ^ 1;
        if sibling < level.len() {
            path.push(level[sibling]);
        }
        level = level.chunks(2).map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] }).collect();
        i /= 2;
    }
    Some(path)
}

/// **An opened leaf**: its coordinate, values and path, and its stage's leaf count.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenOpenedLeafV1 {
    pub coord: PalwGenLeafCoordV1,
    pub values: Vec<i128>,
    pub path: Vec<Hash64>,
}

/// **Is an opened leaf under its stage's root?** The space names its index, dtype and count; the
/// path climbs to the stage's Merkle root, bound with the count under the stage root.
pub fn palw_gen_verify_leaf_v1(space: &PalwGenStageSpaceV1, stage_root: &Hash64, leaf: &PalwGenOpenedLeafV1) -> bool {
    let Some(index) = space.leaf_index(&leaf.coord) else { return false };
    let spec = space.leaves[index as usize];
    let Ok(mut h) = palw_gen_step_leaf_hash_v1(&spec, &leaf.values) else { return false };
    let (mut i, mut width, mut used) = (index, space.leaves.len() as u64, 0usize);
    while width > 1 {
        if i ^ 1 < width {
            let Some(s) = leaf.path.get(used) else { return false };
            used += 1;
            h = if i % 2 == 0 { node_hash(&h, s) } else { node_hash(s, &h) };
        }
        i /= 2;
        width = width.div_ceil(2);
    }
    used == leaf.path.len()
        && keyed64(PALW_GEN_STAGE_ROOT_DOMAIN_V1, &[&[space.stage], &(space.leaves.len() as u64).to_le_bytes(), h.as_byte_slice()])
            == *stage_root
}
