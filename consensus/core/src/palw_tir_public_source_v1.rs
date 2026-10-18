//! Original-domain, bounded TIR public acquisition primitives. This module does
//! not admit a court, impose a custody obligation, or allocate a clock/budget.
//! Those are native paid-sidecar responsibilities. Flat trace EOF and Float32
//! trace acquisition are deliberately not supplied by the tiled trace route.
use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepRangeOpeningV1, PalwStepTileLeafV1};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

pub const PALW_TIR_PUBLIC_FRONTIER_HASHES_V1: usize = 1024;
pub const PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1: u32 = 1024;
pub const PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1: u32 = crate::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1;
const PATH: usize = 64;
const RUN: usize = PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1 as usize;
const LEAF_BYTES: usize = crate::palw_step::PALW_STEP_MAX_TILE_LEN as usize * 4;
const TILE_LANES: usize = crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
const DEPTH: u8 = crate::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirPublicRequestV1 {
    StepNode {
        level: u8,
        index: u64,
    },
    StepLeaf {
        index: u64,
    },
    StepRun {
        first: u64,
        count: u32,
    },
    /// Interior rows-tree node only. RowNode(0)'s whole vocabulary is never used.
    RowsNode {
        level: u8,
        index: u64,
    },
    TileNode {
        row: u32,
        level: u8,
        index: u64,
    },
    TraceTile {
        row: u32,
        tile: u32,
    },
    /// Canonical 1024-ID chunks, except the final partial chunk.
    Ids {
        offset: u32,
    },
}

/// Signed demand identity, not an admission certificate. The paid native fold
/// must authenticate these clocks/owner and purchase the complete response span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicDemandScopeV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub turn: u64,
    pub opened_daa: u64,
    pub deadline_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwTirPublicResponseV1 {
    StepNode { frontier: Vec<Hash64>, siblings: Vec<Hash64> },
    StepLeaf { preimage: PalwStepTileLeafV1, opening: PalwStepOpeningV1 },
    StepRun { preimages: Vec<PalwStepTileLeafV1>, range: PalwStepRangeOpeningV1 },
    RowsNode { frontier: Vec<Hash64>, siblings: Vec<Hash64> },
    TileNode { row_root: Hash64, row_opening: PalwStepOpeningV1, frontier: Vec<Hash64>, siblings: Vec<Hash64> },
    TraceTile { row_root: Hash64, row_opening: PalwStepOpeningV1, lanes: Vec<i32>, opening: PalwStepOpeningV1 },
    Ids { rows_root: Hash64, ids: Vec<u32> },
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn vector<T: BorshDeserialize, R: Read>(r: &mut R, maximum: usize) -> io::Result<Vec<T>> {
    let length = u32::deserialize_reader(r)? as usize;
    if length > maximum {
        return Err(invalid("TIR public vector exceeds its bound before allocation"));
    }
    let mut values = Vec::new();
    values.try_reserve_exact(length).map_err(|_| invalid("TIR public allocation failed"))?;
    for _ in 0..length {
        values.push(T::deserialize_reader(r)?);
    }
    Ok(values)
}
fn opening<R: Read>(r: &mut R) -> io::Result<PalwStepOpeningV1> {
    Ok(PalwStepOpeningV1 {
        leaf_index: BorshDeserialize::deserialize_reader(r)?,
        leaf_hash: BorshDeserialize::deserialize_reader(r)?,
        siblings: vector(r, PATH)?,
    })
}
fn leaf<R: Read>(r: &mut R, maximum_bytes: usize) -> io::Result<PalwStepTileLeafV1> {
    Ok(PalwStepTileLeafV1 {
        version: BorshDeserialize::deserialize_reader(r)?,
        coord: BorshDeserialize::deserialize_reader(r)?,
        value_count: BorshDeserialize::deserialize_reader(r)?,
        values_le: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, maximum_bytes)?,
    })
}
impl BorshDeserialize for PalwTirPublicResponseV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        Self::decode_with_leaf_bound_v1(r, LEAF_BYTES)
    }
}
impl PalwTirPublicResponseV1 {
    fn decode_with_leaf_bound_v1<R: Read>(r: &mut R, maximum_leaf_bytes: usize) -> io::Result<Self> {
        Ok(match u8::deserialize_reader(r)? {
            0 => Self::StepNode { frontier: vector(r, PALW_TIR_PUBLIC_FRONTIER_HASHES_V1)?, siblings: vector(r, PATH)? },
            1 => Self::StepLeaf { preimage: leaf(r, maximum_leaf_bytes)?, opening: opening(r)? },
            2 => {
                let length = u32::deserialize_reader(r)? as usize;
                if length > RUN {
                    return Err(invalid("TIR public run exceeds the native run bound"));
                }
                let mut preimages = Vec::with_capacity(length);
                for _ in 0..length {
                    preimages.push(leaf(r, maximum_leaf_bytes)?);
                }
                Self::StepRun {
                    preimages,
                    range: PalwStepRangeOpeningV1 {
                        first_leaf_index: BorshDeserialize::deserialize_reader(r)?,
                        leaf_hashes: vector(r, RUN)?,
                        siblings: vector(r, PATH * 2)?,
                    },
                }
            }
            3 => Self::RowsNode { frontier: vector(r, PALW_TIR_PUBLIC_FRONTIER_HASHES_V1)?, siblings: vector(r, PATH)? },
            4 => Self::TileNode {
                row_root: BorshDeserialize::deserialize_reader(r)?,
                row_opening: opening(r)?,
                frontier: vector(r, PALW_TIR_PUBLIC_FRONTIER_HASHES_V1)?,
                siblings: vector(r, PATH)?,
            },
            5 => Self::TraceTile {
                row_root: BorshDeserialize::deserialize_reader(r)?,
                row_opening: opening(r)?,
                lanes: vector(r, TILE_LANES)?,
                opening: opening(r)?,
            },
            6 => Self::Ids {
                rows_root: BorshDeserialize::deserialize_reader(r)?,
                ids: vector(r, PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize)?,
            },
            _ => return Err(invalid("unknown TIR public response")),
        })
    }
}

/// Private geometry is derived only from the original authenticated TIR header,
/// registered program, digest-authenticated layout and native ladder. No wire DTO
/// can deserialize an authenticated source or authenticated IDs.
#[derive(Clone, Debug)]
pub struct PalwTirPublicSourceV1 {
    header: crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    class_id: Hash64,
    context_hash: Hash64,
    vocab: u32,
    tiled: bool,
    maximum_leaf_bytes: u64,
    maximum_leaves: u64,
    source_id: Hash64,
    /// **G14 audit §11 (a): the most bytes of leaf values `c` consecutive leaves of THIS job hold, for `c = 1 ..= RUN`** — from the
    /// class's step space (`maximum_window_lanes_v1`: the job's actual widest leaf, and the positions a window can touch), not the
    /// layout's widest tile × 4. Index `c − 1`. Only response BOUNDS read it (prices, clocks, the accepted body ceiling); decoding
    /// stays at `maximum_leaf_bytes`.
    window_value_bytes: Vec<u64>,
    /// G14 audit §11 (b): the most full step-leaf runs the cone of any leaf of this job needs (`cone_runs_bound_v1`).
    cone_runs: u64,
}
fn hash(domain: &[u8], bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(bytes).as_bytes().try_into().expect("64"))
}
fn height(count: u64) -> u8 {
    if count <= 1 { 0 } else { (64 - (count - 1).leading_zeros()) as u8 }
}
impl PalwTirPublicSourceV1 {
    pub fn new_v1(
        registered: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        original_execution_root: Hash64,
        original_trace_root: Hash64,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        let binding = header.resolve_registered_layout_v1(registered, layout, original_execution_root, maximum_leaves)?;
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, maximum_leaves).map_err(|e| e.to_string())?;
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, registered, maximum_leaves)?;
        let program = &verified.space.program;
        let logits = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
        if header.full_logits_trace_root != original_trace_root
            || view.vocab_size == 0
            || u64::from(view.vocab_size) != u64::from(logits.out.elements_at(1))
            || view.logits_scheme_id != Hash64::from_bytes(program.logits_scheme_id)
            || header.job_context.exact_decode_tokens as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
        {
            return Err("TIR public source changes original roots or native decode geometry".into());
        }
        let maximum_lanes = layout
            .commit_tiles
            .iter()
            .copied()
            .map(u64::from)
            .chain(verified.space.fixed_instances().iter().map(|s| u64::from(s.tile_lanes)))
            .chain(verified.space.hist_instances().iter().map(|s| u64::from(s.tile_lanes) * u64::from(layout.h_tile)))
            .max()
            .ok_or("TIR public source has no native leaf geometry")?;
        let maximum_leaf_bytes = maximum_lanes
            .checked_mul(4)
            .filter(|n| *n <= LEAF_BYTES as u64)
            .ok_or("TIR native leaf geometry exceeds original format")?;
        let source_id = hash(b"misaka-palw/tir-public-source/v1", &borsh::to_vec(header).map_err(|e| e.to_string())?);
        let job = verified.space.job_shape(&header.job_context).map_err(|e| e.to_string())?;
        let window_value_bytes = (1..=RUN as u64)
            .map(|count| {
                let lanes = verified.space.maximum_window_lanes_v1(&job, count);
                // Never above the format's own per-leaf ceiling (the decode bound).
                u64::try_from(lanes.saturating_mul(4)).unwrap_or(u64::MAX).min(count.saturating_mul(maximum_leaf_bytes))
            })
            .collect();
        let cone_runs = verified.space.cone_runs_bound_v1(&job, header.step_leaf_count, RUN as u64);
        Ok(Self {
            header: header.clone(),
            class_id: verified.class_id,
            context_hash: verified.context_hash,
            vocab: view.vocab_size,
            tiled: view.is_tiled_v1(),
            maximum_leaf_bytes,
            maximum_leaves,
            source_id,
            window_value_bytes,
            cone_runs,
        })
    }
    pub fn source_id_v1(&self) -> Hash64 {
        self.source_id
    }
    pub fn header_v1(&self) -> &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
        &self.header
    }
    /// The largest leaf preimage this source decodes (`4 ×` the class layout's widest committed tile; every leaf is refused past it).
    pub fn maximum_leaf_bytes_v1(&self) -> u64 {
        self.maximum_leaf_bytes
    }
    /// The most full step-leaf runs the cone of any leaf of this job needs (G14 audit §11 (b)); at most `⌈leaves / run⌉`.
    pub fn cone_runs_v1(&self) -> u64 {
        self.cone_runs
    }
    /// The job's actual widest leaf, in value bytes (G14 audit §11 (a); at most [`Self::maximum_leaf_bytes_v1`]).
    pub fn widest_leaf_value_bytes_v1(&self) -> u64 {
        self.window_value_bytes[0]
    }
    pub fn leaf_count_v1(&self) -> u64 {
        self.header.step_leaf_count
    }
    pub fn decode_count_v1(&self) -> u32 {
        self.header.job_context.exact_decode_tokens
    }
    pub fn vocab_v1(&self) -> u32 {
        self.vocab
    }
    pub fn supports_tiled_trace_v1(&self) -> bool {
        self.tiled
    }
    pub fn step_descent_units_v1(&self) -> u64 {
        u64::from(height(self.leaf_count_v1()).div_ceil(DEPTH)) + 1
    }
    fn trace(&self) -> Result<(), String> {
        if !self.tiled {
            return Err(
                "bounded ID/row/tile acquisition requires the original integer tiled trace; Flat/Float EOF is not implemented".into(),
            );
        }
        Ok(())
    }
    fn node(&self, count: u64, level: u8, index: u64) -> Result<(), String> {
        if level == 0 || crate::palw_tir_court_v1::palw_tir_step_tree_width_v1(count, level).is_none_or(|width| index >= width) {
            return Err("TIR public request is not an interior original-tree node".into());
        }
        Ok(())
    }
    pub fn check_request_v1(&self, request: &PalwTirPublicRequestV1) -> Result<(), String> {
        use PalwTirPublicRequestV1::*;
        match *request {
            StepNode { level, index } => self.node(self.leaf_count_v1(), level, index),
            StepLeaf { index } if index < self.leaf_count_v1() => Ok(()),
            StepRun { first, count }
                if (1..=RUN as u32).contains(&count)
                    && first.checked_add(u64::from(count)).is_some_and(|end| end <= self.leaf_count_v1()) =>
            {
                Ok(())
            }
            RowsNode { level, index } => {
                self.trace()?;
                self.node(u64::from(self.decode_count_v1()), level, index)
            }
            TileNode { row, level, index } if row < self.decode_count_v1() => {
                self.trace()?;
                self.node(u64::from(self.vocab).div_ceil(TILE_LANES as u64), level, index)
            }
            TraceTile { row, tile }
                if row < self.decode_count_v1() && u64::from(tile) < u64::from(self.vocab).div_ceil(TILE_LANES as u64) =>
            {
                self.trace()
            }
            Ids { offset } if offset < self.decode_count_v1() && offset.is_multiple_of(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1) => {
                self.trace()
            }
            _ => Err("TIR public request is outside exact native original geometry".into()),
        }
    }
    pub fn demand_id_v1(&self, scope: &PalwTirPublicDemandScopeV1, request: &PalwTirPublicRequestV1) -> Result<Hash64, String> {
        self.check_request_v1(request)?;
        if scope.claim == Hash64::default()
            || scope.bundle_id == Hash64::default()
            || scope.reserved_daa > scope.opened_daa
            || scope.opened_daa >= scope.deadline_daa
        {
            return Err("TIR public demand has no exact nonempty owner/clock scope".into());
        }
        Ok(hash(b"misaka-palw/tir-public-demand/v1", &borsh::to_vec(&(self.source_id, scope, request)).map_err(|e| e.to_string())?))
    }
    /// Native ingress must use this context-bound decoder AFTER enforcing the
    /// paid logical-unit memory permit. Generic Borsh decoding has only absolute
    /// format bounds and does not know this class's smaller prepaid geometry.
    pub fn decode_response_v1(&self, request: &PalwTirPublicRequestV1, bytes: &[u8]) -> Result<PalwTirPublicResponseV1, String> {
        let bound = self.unit_bound_v1(request)?;
        if bytes.len() as u64 > bound.maximum_response_bytes {
            return Err("TIR public wire unit exceeds native geometry before decoding".into());
        }
        let mut remaining = bytes;
        let response = PalwTirPublicResponseV1::decode_with_leaf_bound_v1(&mut remaining, self.maximum_leaf_bytes as usize)
            .map_err(|e| e.to_string())?;
        if !remaining.is_empty() {
            return Err("TIR public response has trailing bytes".into());
        }
        self.check_response_bound_v1(request, &response)?;
        Ok(response)
    }
    fn ids<'a>(&self, ids: Option<&'a PalwTirPublicAuthenticatedIdsV1>) -> Result<&'a PalwTirPublicAuthenticatedIdsV1, String> {
        self.trace()?;
        ids.filter(|ids| ids.source_id == self.source_id && ids.ids.len() == self.decode_count_v1() as usize)
            .ok_or_else(|| "TIR trace response needs IDs authenticated against this exact original source".into())
    }
    fn row(&self, row: u32, root: Hash64, opening: &PalwStepOpeningV1, rows_root: Hash64) -> Result<(), String> {
        if opening.leaf_index != u64::from(row)
            || opening.leaf_hash != root
            || opening.siblings.len() > PATH
            || crate::palw_step_leg::step_opening_root_capped_v1(u64::from(self.decode_count_v1()), opening, self.maximum_leaves)
                .map_err(|e| e.to_string())?
                != rows_root
        {
            return Err("TIR public row opening does not reach the authenticated original rows root".into());
        }
        Ok(())
    }
    /// Authentication only. A malformed but genuinely committed step preimage is
    /// returned intact so the semantic court, not DA admission, adjudicates it.
    pub fn authenticate_v1(
        &self,
        request: &PalwTirPublicRequestV1,
        response: &PalwTirPublicResponseV1,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) -> Result<PalwTirPublicAuthenticatedUnitV1, String> {
        self.authenticate_with_rows_v1(request, response, ids, None)
    }
    pub(crate) fn authenticate_with_rows_v1(
        &self,
        request: &PalwTirPublicRequestV1,
        response: &PalwTirPublicResponseV1,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
        native: Option<&PalwTirPublicRowsAnchorV1>,
    ) -> Result<PalwTirPublicAuthenticatedUnitV1, String> {
        self.check_request_v1(request)?;
        self.check_response_bound_v1(request, response)?;
        let rows_root = || -> Result<Hash64, String> {
            if let Some(ids) = ids {
                return Ok(self.ids(Some(ids))?.rows_root);
            }
            self.trace()?;
            native
                .filter(|a| a.source_id == self.source_id && a.trace_root == self.header.full_logits_trace_root)
                .map(|a| a.rows_root)
                .ok_or_else(|| "TIR trace response has no current native accepted-ID rows anchor".into())
        };
        use PalwTirPublicRequestV1 as Q;
        use PalwTirPublicResponseV1 as A;
        match (request, response) {
            (Q::StepNode { level, index }, A::StepNode { frontier, siblings }) => {
                crate::palw_tir_court_v1::palw_tir_step_node_reaches_v1(
                    self.leaf_count_v1(),
                    &self.header.step_merkle_root,
                    *level,
                    *index,
                    frontier,
                    siblings,
                )
                .map_err(str::to_string)?
            }
            (Q::StepLeaf { index }, A::StepLeaf { preimage, opening }) => {
                if opening.leaf_index != *index
                    || opening.leaf_hash != crate::palw_step_leg::step_tile_leaf_hash_v1(&self.context_hash, &self.class_id, preimage)
                    || crate::palw_step_leg::step_opening_root_capped_v1(self.leaf_count_v1(), opening, self.maximum_leaves)
                        .map_err(|e| e.to_string())?
                        != self.header.step_merkle_root
                {
                    return Err("TIR raw preimage/opening does not reach the original step root".into());
                }
            }
            (Q::StepRun { first, count }, A::StepRun { preimages, range }) => {
                if range.first_leaf_index != *first
                    || preimages.len() != *count as usize
                    || range.leaf_hashes.len() != preimages.len()
                    || preimages.iter().zip(&range.leaf_hashes).any(|(preimage, leaf)| {
                        crate::palw_step_leg::step_tile_leaf_hash_v1(&self.context_hash, &self.class_id, preimage) != *leaf
                    })
                    || crate::palw_step_leg::step_range_opening_root_capped_v1(self.leaf_count_v1(), range, self.maximum_leaves)
                        .map_err(|e| e.to_string())?
                        != self.header.step_merkle_root
                {
                    return Err("TIR raw run is not exactly its original opened interval".into());
                }
            }
            (Q::RowsNode { level, index }, A::RowsNode { frontier, siblings }) => {
                let rows_root = rows_root()?;
                crate::palw_tir_court_v1::palw_step_node_reaches_at_depth_v1(
                    DEPTH,
                    u64::from(self.decode_count_v1()),
                    &rows_root,
                    *level,
                    *index,
                    frontier,
                    siblings,
                )
                .map_err(str::to_string)?;
            }
            (Q::TileNode { row, level, index }, A::TileNode { row_root, row_opening, frontier, siblings }) => {
                self.row(*row, *row_root, row_opening, rows_root()?)?;
                crate::palw_tir_court_v1::palw_step_node_reaches_at_depth_v1(
                    DEPTH,
                    u64::from(self.vocab).div_ceil(TILE_LANES as u64),
                    row_root,
                    *level,
                    *index,
                    frontier,
                    siblings,
                )
                .map_err(str::to_string)?;
            }
            (Q::TraceTile { row, tile }, A::TraceTile { row_root, row_opening, lanes, opening }) => {
                self.row(*row, *row_root, row_opening, rows_root()?)?;
                crate::palw_step_refute::tiled_tile_authenticate_v1(
                    &self.context_hash,
                    *row,
                    self.vocab as usize,
                    u64::from(self.vocab).div_ceil(TILE_LANES as u64),
                    row_root,
                    u64::from(*tile),
                    lanes,
                    opening,
                    self.maximum_leaves,
                )
                .map_err(|e| e.to_string())?;
            }
            (Q::Ids { .. }, _) => {
                return Err("ID chunks are provisional until the original outer trace hash seals their complete sequence".into());
            }
            _ => return Err("TIR public response has a different typed request".into()),
        }
        Ok(PalwTirPublicAuthenticatedUnitV1 { source_id: self.source_id, request: *request, response: response.clone() })
    }
    fn check_response_bound_v1(&self, request: &PalwTirPublicRequestV1, response: &PalwTirPublicResponseV1) -> Result<(), String> {
        let bound = self.unit_bound_v1(request)?;
        // Programmatically constructed responses have not passed the bounded
        // decoder; validate each vector before any hashing or authenticated clone.
        let path = |p: &PalwStepOpeningV1| p.siblings.len() <= PATH;
        let preimage = |p: &PalwStepTileLeafV1| p.values_le.len() as u64 <= self.maximum_leaf_bytes;
        let valid = match response {
            PalwTirPublicResponseV1::StepNode { frontier, siblings } | PalwTirPublicResponseV1::RowsNode { frontier, siblings } => {
                frontier.len() <= PALW_TIR_PUBLIC_FRONTIER_HASHES_V1 && siblings.len() <= PATH
            }
            PalwTirPublicResponseV1::StepLeaf { preimage: p, opening } => preimage(p) && path(opening),
            PalwTirPublicResponseV1::StepRun { preimages, range } => {
                preimages.len() <= RUN
                    && preimages.iter().all(preimage)
                    && range.leaf_hashes.len() <= RUN
                    && range.siblings.len() <= PATH * 2
            }
            PalwTirPublicResponseV1::TileNode { row_opening, frontier, siblings, .. } => {
                path(row_opening) && frontier.len() <= PALW_TIR_PUBLIC_FRONTIER_HASHES_V1 && siblings.len() <= PATH
            }
            PalwTirPublicResponseV1::TraceTile { row_opening, lanes, opening, .. } => {
                path(row_opening) && lanes.len() <= TILE_LANES && path(opening)
            }
            PalwTirPublicResponseV1::Ids { ids, .. } => ids.len() <= PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize,
        };
        if !valid {
            return Err("TIR public response exceeds a bounded vector".into());
        }
        if borsh::object_length(response).map_err(|e| e.to_string())? as u64 > bound.maximum_response_bytes {
            return Err("TIR public response exceeds the native unit byte bound".into());
        }
        Ok(())
    }
    /// One unit, independent of population/custody/attempt/reorg multiplicities.
    /// Not a full paid quote: admission must also fund original class hydration,
    /// own execution/disk indexes, all dependent waves, retained journals/history,
    /// custody slots, signatures, fees and collection on immutable native clocks.
    pub fn unit_bound_v1(&self, request: &PalwTirPublicRequestV1) -> Result<PalwTirPublicUnitBoundV1, String> {
        self.check_request_v1(request)?;
        use PalwTirPublicRequestV1::*;
        let h = u64::from(height(self.leaf_count_v1()));
        let rows_h = u64::from(height(u64::from(self.decode_count_v1())));
        let tiles = u64::from(self.vocab).div_ceil(TILE_LANES as u64);
        let tile_h = u64::from(height(tiles));
        let node = |count: u64, level: u8, index: u64| -> Result<u64, String> {
            let (_, lo, hi) =
                crate::palw_tir_court_v1::palw_tir_step_node_frontier_v1(count, level, index).ok_or("invalid native frontier")?;
            Ok((hi - lo) * 64 + u64::from(height(count)) * 64 + 9)
        };
        let bytes = match *request {
            StepNode { level, index } => node(self.leaf_count_v1(), level, index)?,
            // G14 audit §11 (a): leaf values at the job's actual widest leaf / window (`window_value_bytes`), framing as before.
            StepLeaf { .. } => 1 + self.window_value_bytes[0] + 26 + 76 + h * 64,
            StepRun { count, .. } => {
                1 + 4 + self.window_value_bytes[count as usize - 1] + u64::from(count) * (26 + 64) + 16 + h * 128
            }
            RowsNode { level, index } => node(u64::from(self.decode_count_v1()), level, index)?,
            TileNode { level, index, .. } => 64 + 76 + rows_h * 64 + node(tiles, level, index)?,
            TraceTile { tile, .. } => {
                1 + 64
                    + 76
                    + rows_h * 64
                    + 4
                    + (u64::from(self.vocab) - u64::from(tile) * TILE_LANES as u64).min(TILE_LANES as u64) * 4
                    + 76
                    + tile_h * 64
            }
            Ids { offset } => 1 + 64 + 4 + u64::from((self.decode_count_v1() - offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1)) * 4,
        };
        let work = bytes.checked_mul(16).and_then(|n| n.checked_add(65_536)).ok_or("TIR public unit work overflow")?;
        Ok(PalwTirPublicUnitBoundV1 {
            maximum_response_bytes: bytes,
            maximum_parser_steps: work,
            maximum_hash_input_bytes: work,
            // Raw logical bytes, decoded allocation capacity, authenticated
            // result clone and the original leaf/hash writer coexist.
            resident_ram_bytes: bytes
                .checked_mul(8)
                .and_then(|n| n.checked_add(u64::from(self.decode_count_v1()) * 4 + 131_072))
                .ok_or("TIR public unit RAM overflow")?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirPublicUnitBoundV1 {
    pub maximum_response_bytes: u64,
    pub maximum_parser_steps: u64,
    pub maximum_hash_input_bytes: u64,
    pub resident_ram_bytes: u64,
}
/// Produced only by successful original-root authentication, never deserialized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicAuthenticatedUnitV1 {
    source_id: Hash64,
    request: PalwTirPublicRequestV1,
    response: PalwTirPublicResponseV1,
}
impl PalwTirPublicAuthenticatedUnitV1 {
    pub fn source_id_v1(&self) -> Hash64 {
        self.source_id
    }
    pub fn request_v1(&self) -> PalwTirPublicRequestV1 {
        self.request
    }
    pub fn response_v1(&self) -> &PalwTirPublicResponseV1 {
        &self.response
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicAuthenticatedIdsV1 {
    source_id: Hash64,
    rows_root: Hash64,
    ids: Vec<u32>,
}
/// Native-view capability only. No public constructor, Clone or wire decoder.
/// It authenticates rows identity, not generated IDs or model computation.
pub(crate) struct PalwTirPublicRowsAnchorV1 {
    source_id: Hash64,
    trace_root: Hash64,
    rows_root: Hash64,
}
impl PalwTirPublicRowsAnchorV1 {
    /// Caller must resolve the CURRENT native owned pointer and exact rooted
    /// accepted all-ID body before using this. A cache/serialized row is not an
    /// authority for calling this crate-private native state boundary.
    pub(crate) fn from_native_v1(source: &PalwTirPublicSourceV1, rows_root: Hash64) -> Self {
        Self { source_id: source.source_id_v1(), trace_root: source.header.full_logits_trace_root, rows_root }
    }
}
impl PalwTirPublicAuthenticatedIdsV1 {
    pub fn source_id_v1(&self) -> Hash64 {
        self.source_id
    }
    pub fn ids_v1(&self) -> &[u32] {
        &self.ids
    }
    pub fn rows_root_v1(&self) -> Hash64 {
        self.rows_root
    }
}
/// Provisional IDs do not authorize any row/tile. Exact ordered completion must
/// reproduce the original outer hash; a failing chunk/EOF leaves this unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwTirPublicIdsAssemblerV1 {
    source_id: Option<Hash64>,
    rows_root: Option<Hash64>,
    ids: Vec<u32>,
}
impl PalwTirPublicIdsAssemblerV1 {
    pub fn next_offset_v1(&self) -> u32 {
        self.ids.len() as u32
    }
    pub fn push_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        request: &PalwTirPublicRequestV1,
        response: &PalwTirPublicResponseV1,
    ) -> Result<Option<PalwTirPublicAuthenticatedIdsV1>, String> {
        source.check_request_v1(request)?;
        source.check_response_bound_v1(request, response)?;
        let (PalwTirPublicRequestV1::Ids { offset }, PalwTirPublicResponseV1::Ids { rows_root, ids }) = (request, response) else {
            return Err("ID assembler accepts only typed ID chunks".into());
        };
        if *offset != self.next_offset_v1()
            || ids.len() != (source.decode_count_v1() - offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1) as usize
            || self.source_id.is_some_and(|id| id != source.source_id)
            || self.rows_root.is_some_and(|root| root != *rows_root)
        {
            return Err("ID chunk changes original source, root, exact offset or canonical length".into());
        }
        let old_len = self.ids.len();
        self.ids.try_reserve(ids.len()).map_err(|_| "cannot reserve provisional original IDs")?;
        self.ids.extend_from_slice(ids);
        let complete = self.ids.len() == source.decode_count_v1() as usize;
        if complete
            && crate::palw_step_refute::tiled_logits_outer_root_v1(
                &source.header.job_context,
                u64::from(source.decode_count_v1()),
                rows_root,
                &self.ids,
            ) != source.header.full_logits_trace_root
        {
            // Only EOF authentication can fail after mutation. Roll back the
            // provisional suffix without copying every prior chunk on each poll.
            self.ids.truncate(old_len);
            return Err("complete ID stream does not authenticate the original trace root".into());
        }
        let result = complete.then(|| PalwTirPublicAuthenticatedIdsV1 {
            source_id: source.source_id,
            rows_root: *rows_root,
            ids: self.ids.clone(),
        });
        self.source_id = Some(source.source_id);
        self.rows_root = Some(*rows_root);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_leg::{step_merkle_range_siblings_v1, step_opening_v1};
    use crate::palw_tir_court_v1::{
        self as court,
        test_support::{TinyExecution, tiny_execution},
    };

    /// ADR-0190 bounded-round path (DORMANT): refuses an unbounded gather term; otherwise two cone rounds whatever the run count, the
    /// round-1 body carrying the cone runs and the gather bytes, and the restart replaying both rounds.
    #[test]
    fn the_bounded_localization_path_refuses_an_unbounded_gather_and_prices_two_rounds() {
        use crate::palw_tir_frame_cost_v1 as fc;
        let x = tiny_execution(None);
        let src = source(&x);
        let cost = fc::palw_tir_frame_test_only_cost_v1();
        assert!(fc::tir_public_localization_path_bounded_v1(&src, 6, &cost.config, &cost.clock, None).is_err(), "gather unbounded: refused");
        let base = fc::tir_public_localization_path_bounded_v1(&src, 6, &cost.config, &cost.clock, Some(0)).unwrap();
        let with_gather = fc::tir_public_localization_path_bounded_v1(&src, 6, &cost.config, &cost.clock, Some(1_000)).unwrap();
        assert_eq!(base.cone_runs, src.cone_runs_v1(), "the cone is the source's own run count");
        assert_eq!(with_gather.round1_body - base.round1_body, 1_000, "the gather bytes are carried by round 1");
        assert!(with_gather.daa >= base.daa);
        let tiled = u64::from(src.supports_tiled_trace_v1() && src.decode_count_v1() > 0);
        let restarts = cost.config.prices.reorg_replay_moves;
        assert_eq!(
            base.turns,
            tiled + src.step_descent_units_v1() + 1 + fc::PALW_TIR_BOUNDED_CONE_ROUNDS_V1 + 1 + restarts * fc::PALW_TIR_BOUNDED_CONE_ROUNDS_V1,
            "two cone rounds, not one per run"
        );
    }

    fn source(x: &TinyExecution) -> PalwTirPublicSourceV1 {
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding),
            x.binding.committed_execution_root,
            x.binding.full_logits_trace_root,
            1 << 40,
        )
        .unwrap()
    }
    fn ids(source: &PalwTirPublicSourceV1, x: &TinyExecution) -> PalwTirPublicAuthenticatedIdsV1 {
        let rows_root = crate::palw_step_refute::tiled_logits_rows_root_v1(&x.binding.job_context, &x.rows).unwrap();
        PalwTirPublicIdsAssemblerV1::default()
            .push_v1(
                source,
                &PalwTirPublicRequestV1::Ids { offset: 0 },
                &PalwTirPublicResponseV1::Ids { rows_root, ids: x.generated.clone() },
            )
            .unwrap()
            .unwrap()
    }
    fn check(
        source: &PalwTirPublicSourceV1,
        request: PalwTirPublicRequestV1,
        response: PalwTirPublicResponseV1,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) {
        let bound = source.unit_bound_v1(&request).unwrap();
        let bytes = borsh::to_vec(&response).unwrap();
        assert!(bytes.len() as u64 <= bound.maximum_response_bytes);
        let decoded = source.decode_response_v1(&request, &bytes).unwrap();
        assert_eq!(decoded, response);
        let authenticated = source.authenticate_v1(&request, &decoded, ids).unwrap();
        assert_eq!(authenticated.source_id_v1(), source.source_id_v1());
        assert_eq!(authenticated.request_v1(), request);
    }
    #[test]
    fn registered_original_geometry_authenticates_nodes_raw_leaves_and_runs_without_decode_pin() {
        let x = tiny_execution(None);
        let source = source(&x);
        assert_eq!(source.header_v1().step_leaf_count, x.hashes.len() as u64);
        let level = height(source.leaf_count_v1());
        let disclosure = court::build_tir_step_node_disclosure_v1(&x.binding, level, 0, &x, 1 << 40).unwrap();
        check(
            &source,
            PalwTirPublicRequestV1::StepNode { level, index: 0 },
            PalwTirPublicResponseV1::StepNode { frontier: disclosure.frontier, siblings: disclosure.siblings },
            None,
        );
        let opening = step_opening_v1(&x.hashes, 0).unwrap();
        check(
            &source,
            PalwTirPublicRequestV1::StepLeaf { index: 0 },
            PalwTirPublicResponseV1::StepLeaf { preimage: x.preimages[0].clone(), opening: opening.clone() },
            None,
        );
        let count = source.leaf_count_v1().min(RUN as u64) as u32;
        check(
            &source,
            PalwTirPublicRequestV1::StepRun { first: 0, count },
            PalwTirPublicResponseV1::StepRun {
                preimages: x.preimages[..count as usize].to_vec(),
                range: PalwStepRangeOpeningV1 {
                    first_leaf_index: 0,
                    leaf_hashes: x.hashes[..count as usize].to_vec(),
                    siblings: step_merkle_range_siblings_v1(&x.hashes, 0, count as usize).unwrap(),
                },
            },
            None,
        );
        let mut altered = x.preimages[0].clone();
        altered.value_count = altered.value_count.wrapping_add(1);
        assert!(
            source
                .authenticate_v1(
                    &PalwTirPublicRequestV1::StepLeaf { index: 0 },
                    &PalwTirPublicResponseV1::StepLeaf { preimage: altered.clone(), opening },
                    None
                )
                .is_err()
        );
        // If those malformed bytes really ARE committed, DA authenticates them
        // intact; only the subsequent semantic court decides their structure.
        let mut malformed_hashes = x.hashes.clone();
        malformed_hashes[0] = crate::palw_step_leg::step_tile_leaf_hash_v1(&source.context_hash, &source.class_id, &altered);
        let mut malformed_header = source.header_v1().clone();
        malformed_header.step_merkle_root = crate::palw_step_leg::step_merkle_root_v1(&malformed_hashes).unwrap();
        malformed_header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &source.context_hash,
            &malformed_header.full_logits_trace_root,
            &source.class_id,
            source.leaf_count_v1(),
            &malformed_header.step_merkle_root,
        );
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let malformed_source = PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &malformed_header,
            malformed_header.committed_execution_root,
            malformed_header.full_logits_trace_root,
            1 << 40,
        )
        .unwrap();
        check(
            &malformed_source,
            PalwTirPublicRequestV1::StepLeaf { index: 0 },
            PalwTirPublicResponseV1::StepLeaf { preimage: altered, opening: step_opening_v1(&malformed_hashes, 0).unwrap() },
            None,
        );
        let mut header = source.header_v1().clone();
        header.full_logits_trace_root = Hash64::from_u64_word(987);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        assert!(
            PalwTirPublicSourceV1::new_v1(
                &record,
                &x.binding.class.layout,
                &header,
                x.binding.committed_execution_root,
                x.binding.full_logits_trace_root,
                1 << 40
            )
            .is_err()
        );
        let mut false_record = record;
        false_record.logits_vocab += 1;
        assert!(
            PalwTirPublicSourceV1::new_v1(
                &false_record,
                &x.binding.class.layout,
                source.header_v1(),
                x.binding.committed_execution_root,
                x.binding.full_logits_trace_root,
                1 << 40
            )
            .is_err()
        );
    }

    #[test]
    fn flat_original_binding_does_not_acquire_tiled_trace_obligations() {
        let x = tiny_execution(None);
        let mut class = x.binding.class.clone();
        let mut program = class.decode_program().unwrap();
        program.logits_scheme_id.copy_from_slice(crate::palw_step_refute::flat_logits_scheme_id_v1().as_byte_slice());
        class.program = program.encode();
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &x.binding.artifact_root).unwrap();
        let mut header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        header.job_context.shape_profile_id = record.facts.class_id;
        // A flat class's job context carries the v2 trace scheme (`palw_tir_attempt_v1`'s rule: tiled classes the tiled scheme,
        // every other the v2 one); the FLAT logits scheme is the class program's `logits_scheme_id`, never a job context's (the
        // context shape check admits only the v2 and tiled schemes).
        header.job_context.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &header.full_logits_trace_root,
            &record.facts.class_id,
            header.step_leaf_count,
            &header.step_merkle_root,
        );
        let source = PalwTirPublicSourceV1::new_v1(
            &record,
            &class.layout,
            &header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            1 << 40,
        )
        .unwrap();
        assert!(!source.supports_tiled_trace_v1());
        assert!(source.check_request_v1(&PalwTirPublicRequestV1::StepLeaf { index: 0 }).is_ok());
        for request in [
            PalwTirPublicRequestV1::Ids { offset: 0 },
            PalwTirPublicRequestV1::RowsNode { level: 1, index: 0 },
            PalwTirPublicRequestV1::TileNode { row: 0, level: 1, index: 0 },
            PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 },
        ] {
            assert!(source.check_request_v1(&request).is_err());
        }
    }
    #[test]
    fn original_ids_seal_before_rows_or_tiles_and_bad_eof_is_atomic() {
        let x = tiny_execution(None);
        let source = source(&x);
        let ids = ids(&source, &x);
        let request = PalwTirPublicRequestV1::Ids { offset: 0 };
        let mut wrong = x.generated.clone();
        wrong[0] ^= 1;
        let response = PalwTirPublicResponseV1::Ids { rows_root: ids.rows_root_v1(), ids: wrong };
        let mut intake = PalwTirPublicIdsAssemblerV1::default();
        let original = intake.clone();
        assert!(intake.push_v1(&source, &request, &response).is_err());
        assert_eq!(intake, original);
        assert!(source.authenticate_v1(&request, &response, None).is_err());
        let level = height(u64::from(source.decode_count_v1()));
        let disclosure = court::build_tir_row_node_disclosure_v1(&x.binding, level, 0, &x, 1 << 40).unwrap();
        let response = PalwTirPublicResponseV1::RowsNode { frontier: disclosure.frontier, siblings: disclosure.siblings };
        assert!(source.authenticate_v1(&PalwTirPublicRequestV1::RowsNode { level, index: 0 }, &response, None).is_err());
        check(&source, PalwTirPublicRequestV1::RowsNode { level, index: 0 }, response, Some(&ids));
        assert!(source.check_request_v1(&PalwTirPublicRequestV1::RowsNode { level: 0, index: 0 }).is_err());
        let event = court::tir_logits_event_disclosure_v1(&x.binding, &x.rows, &x.generated, 0, 0).unwrap();
        let court::PalwTirTraceEventDisclosureV1::Tiled { row_root, row_opening, tile_lanes, tile_opening, .. } = event else {
            panic!("tiled")
        };
        let response = PalwTirPublicResponseV1::TraceTile { row_root, row_opening, lanes: tile_lanes, opening: tile_opening };
        check(&source, PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 }, response.clone(), Some(&ids));
        let request = PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 };
        assert!(source.authenticate_v1(&request, &response, None).is_err(), "wire material cannot mint a native rows anchor");
        let native = PalwTirPublicRowsAnchorV1::from_native_v1(&source, ids.rows_root_v1());
        assert_eq!(source.authenticate_with_rows_v1(&request, &response, None, Some(&native)).unwrap().response_v1(), &response);
        let wrong = PalwTirPublicRowsAnchorV1::from_native_v1(&source, Hash64::from_u64_word(999));
        assert!(source.authenticate_with_rows_v1(&request, &response, None, Some(&wrong)).is_err());
        let (foreign, _) = geometry(8, 1);
        let foreign = PalwTirPublicRowsAnchorV1::from_native_v1(&foreign, ids.rows_root_v1());
        assert!(source.authenticate_with_rows_v1(&request, &response, None, Some(&foreign)).is_err());
        let PalwTirPublicResponseV1::TraceTile { mut row_opening, row_root, lanes, opening } = response else { unreachable!() };
        row_opening.leaf_index = 1;
        assert!(
            source
                .authenticate_v1(
                    &PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 },
                    &PalwTirPublicResponseV1::TraceTile { row_root, row_opening, lanes, opening },
                    Some(&ids)
                )
                .is_err()
        );
    }
    #[test]
    fn decoder_rejects_vector_bombs_before_allocating_and_request_ids_bind_every_scope_field() {
        for tag in [0u8, 3] {
            let mut bytes = vec![tag];
            bytes.extend_from_slice(&1025u32.to_le_bytes());
            assert!(borsh::from_slice::<PalwTirPublicResponseV1>(&bytes).is_err());
        }
        let mut bytes = vec![2];
        bytes.extend_from_slice(&257u32.to_le_bytes());
        assert!(borsh::from_slice::<PalwTirPublicResponseV1>(&bytes).is_err());
        let mut bytes = vec![6];
        bytes.extend_from_slice(&[0; 64]);
        bytes.extend_from_slice(&1025u32.to_le_bytes());
        assert!(borsh::from_slice::<PalwTirPublicResponseV1>(&bytes).is_err());
        let x = tiny_execution(None);
        let source = source(&x);
        let request = PalwTirPublicRequestV1::StepLeaf { index: 0 };
        let scope = PalwTirPublicDemandScopeV1 {
            claim: Hash64::from_u64_word(1),
            owner: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(2), 0)),
            reserved_daa: 10,
            bundle_id: Hash64::from_u64_word(3),
            turn: 0,
            opened_daa: 11,
            deadline_daa: 20,
        };
        let id = source.demand_id_v1(&scope, &request).unwrap();
        for n in 0..8 {
            let mut changed = scope;
            match n {
                0 => changed.claim = Hash64::from_u64_word(9),
                1 => changed.owner.0.index += 1,
                2 => changed.reserved_daa -= 1,
                3 => changed.bundle_id = Hash64::from_u64_word(9),
                4 => changed.turn += 1,
                5 => changed.opened_daa += 1,
                6 => changed.deadline_daa += 1,
                _ => {
                    assert_ne!(id, source.demand_id_v1(&scope, &PalwTirPublicRequestV1::StepLeaf { index: 1 }).unwrap());
                    continue;
                }
            }
            assert_ne!(id, source.demand_id_v1(&changed, &request).unwrap());
        }
        assert!(source.check_request_v1(&PalwTirPublicRequestV1::StepRun { first: u64::MAX, count: 2 }).is_err());
        assert!(source.check_request_v1(&PalwTirPublicRequestV1::StepRun { first: 0, count: 257 }).is_err());
        let full = PalwTirPublicRequestV1::TraceTile { row: u32::MAX, tile: 1023 };
        assert_eq!(borsh::from_slice::<PalwTirPublicRequestV1>(&borsh::to_vec(&full).unwrap()).unwrap(), full);
    }

    /// Registered geometry with no execution or model tensors allocated: the
    /// original header may commit any root; acquisition still has to open it.
    fn geometry(vocab: u32, decode: u32) -> (PalwTirPublicSourceV1, crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1) {
        geometry_with_trace(vocab, decode, None)
    }
    fn geometry_with_trace(
        vocab: u32,
        decode: u32,
        trace: Option<(Hash64, &[u32])>,
    ) -> (PalwTirPublicSourceV1, crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1) {
        use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder};
        let mut pb = ProgramBuilder::new(vocab, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
        let embed = pb.param("embed", DType::I8, &[vocab, 4], false);
        let head = pb.param("head", DType::I8, &[vocab, 4], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embed, Ref::Input(0), 0, 0);
            let x = b.cast(x, DType::I32);
            let x = b.commit(x);
            b.finish(&[x])
        };
        let (post, logits) = {
            let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let x = b.matmul(head, x, DType::I64);
            let x = b.clamp(x, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let x = b.reshape_fixed(x, &[vocab]);
            let Ref::Node(logits) = b.commit(x) else { unreachable!() };
            (b.finish(&[]), logits)
        };
        let mut program = pb.finish(pre, vec![], post, logits);
        program.logits_scheme_id.copy_from_slice(crate::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        let class = crate::palw_tir_class_v1::PalwTirClassV1 {
            version: crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: crate::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 4096,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: vec![4, 4096],
                state_tiles: vec![],
            },
            tokenizer_id: Hash64::from_u64_word(17),
        };
        let artifact = Hash64::from_u64_word(18);
        let mut ctx = tiny_execution(None).binding.job_context;
        ctx.declared_prefill_tokens = 1;
        ctx.exact_decode_tokens = decode;
        ctx.max_context_tokens = 4096;
        ctx.shape_profile_id = class.class_id(&artifact);
        ctx.tokenizer_id = class.tokenizer_id;
        let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).unwrap();
        let count = space.leaf_count_capped(&ctx, 1 << 40).unwrap();
        let step = Hash64::from_u64_word(19);
        let mut header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
            version: crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            artifact_root: artifact,
            full_logits_trace_root: Hash64::from_u64_word(20),
            step_leaf_count: count,
            step_merkle_root: step,
            committed_execution_root: Hash64::default(),
        };
        if let Some((rows_root, ids)) = trace {
            header.full_logits_trace_root =
                crate::palw_step_refute::tiled_logits_outer_root_v1(&header.job_context, u64::from(decode), &rows_root, ids);
        }
        header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &header.full_logits_trace_root,
            &header.job_context.shape_profile_id,
            count,
            &step,
        );
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact).unwrap();
        let source = PalwTirPublicSourceV1::new_v1(
            &record,
            &class.layout,
            &header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            1 << 40,
        )
        .unwrap();
        (source, header)
    }
    #[test]
    fn ids_multiple_chunks_are_untrusted_until_exact_eof_and_cannot_change_root_or_source() {
        let rows_root = Hash64::from_u64_word(27);
        let generated = vec![0; 2050];
        let (source, _) = geometry_with_trace(8, 2050, Some((rows_root, &generated)));
        let mut intake = PalwTirPublicIdsAssemblerV1::default();
        intake.ids.try_reserve_exact(generated.len()).unwrap();
        let allocation = intake.ids.as_ptr();
        for offset in [0, 1024] {
            assert!(
                intake
                    .push_v1(
                        &source,
                        &PalwTirPublicRequestV1::Ids { offset },
                        &PalwTirPublicResponseV1::Ids { rows_root, ids: generated[offset as usize..offset as usize + 1024].to_vec() }
                    )
                    .unwrap()
                    .is_none()
            );
            assert_eq!(intake.ids.as_ptr(), allocation, "no prior-prefix clone per chunk");
        }
        let before = intake.clone();
        let (foreign, _) = geometry_with_trace(8, 2050, Some((Hash64::from_u64_word(28), &generated)));
        assert!(
            intake
                .push_v1(
                    &foreign,
                    &PalwTirPublicRequestV1::Ids { offset: 2048 },
                    &PalwTirPublicResponseV1::Ids { rows_root, ids: vec![0, 0] }
                )
                .is_err()
        );
        assert_eq!(intake, before);
        assert_eq!(intake.ids.as_ptr(), allocation, "foreign chunks do not replace the provisional buffer");
        assert!(
            intake
                .push_v1(
                    &source,
                    &PalwTirPublicRequestV1::Ids { offset: 2048 },
                    &PalwTirPublicResponseV1::Ids { rows_root, ids: vec![0, 1] }
                )
                .is_err()
        );
        assert_eq!(intake, before);
        assert_eq!(intake.ids.as_ptr(), allocation, "bad EOF rolls back without replacing the provisional buffer");
        assert!(
            intake
                .push_v1(
                    &source,
                    &PalwTirPublicRequestV1::Ids { offset: 1024 },
                    &PalwTirPublicResponseV1::Ids { rows_root, ids: vec![0; 1024] }
                )
                .is_err()
        );
        let sealed = intake
            .push_v1(
                &source,
                &PalwTirPublicRequestV1::Ids { offset: 2048 },
                &PalwTirPublicResponseV1::Ids { rows_root, ids: vec![0, 0] },
            )
            .unwrap()
            .unwrap();
        assert_eq!(sealed.ids_v1(), generated);
        assert_eq!(sealed.source_id_v1(), source.source_id_v1());
        assert_eq!(intake.ids.as_ptr(), allocation);
    }
    #[test]
    fn original_tile_tree_frontier_and_full_u32_ragged_tile_authenticate_without_row_node_zero() {
        let vocab = 256 * TILE_LANES as u32 + 1;
        let (source, _) = geometry(vocab, 1);
        let tile_hashes: Vec<_> = (0..257u32)
            .map(|tile| {
                let lanes = if tile == 256 { vec![0] } else { vec![0; TILE_LANES] };
                crate::palw_step_refute::tiled_logits_tile_leaf_v1(&source.context_hash, 0, tile, &lanes)
            })
            .collect();
        let row_root = crate::palw_step_leg::step_merkle_root_v1(&tile_hashes).unwrap();
        let rows_root = crate::palw_step_leg::step_merkle_root_v1(&[row_root]).unwrap();
        let (source, _) = geometry_with_trace(vocab, 1, Some((rows_root, &[0])));
        let sealed = PalwTirPublicIdsAssemblerV1::default()
            .push_v1(&source, &PalwTirPublicRequestV1::Ids { offset: 0 }, &PalwTirPublicResponseV1::Ids { rows_root, ids: vec![0] })
            .unwrap()
            .unwrap();
        let row_opening = step_opening_v1(&[row_root], 0).unwrap();
        let level = height(tile_hashes.len() as u64);
        let (frontier, siblings) = court::palw_tir_step_node_parts_v1(&tile_hashes, level, 0).unwrap();
        check(
            &source,
            PalwTirPublicRequestV1::TileNode { row: 0, level, index: 0 },
            PalwTirPublicResponseV1::TileNode { row_root, row_opening: row_opening.clone(), frontier, siblings },
            Some(&sealed),
        );
        let response = PalwTirPublicResponseV1::TraceTile {
            row_root,
            row_opening,
            lanes: vec![0],
            opening: step_opening_v1(&tile_hashes, 256).unwrap(),
        };
        check(&source, PalwTirPublicRequestV1::TraceTile { row: 0, tile: 256 }, response.clone(), Some(&sealed));
        assert!(source.authenticate_v1(&PalwTirPublicRequestV1::TraceTile { row: 0, tile: 255 }, &response, Some(&sealed)).is_err());
    }
}
