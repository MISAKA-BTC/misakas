//! **The failed-block refetch** (RFC-0007 Part II, §II.8, node-local): when a weight `MatMul` fails its check, the seat names the failing
//! free-axis blocks ([`crate::TirCheckFailureV1::blocks`]), asks the producer or any holder for exactly those blocks' weight bytes
//! (at most `F` = [`crate::sketch::TIR_BLOCK_FETCH_CAP_BYTES_V1`] each), recomputes the node's outputs on those blocks exactly, and — with the
//! node's honest value in hand — recomputes the cone to the first committed row and compares it with the claim's: the first difference is the
//! named leaf the unchanged exact court tries ([`TirEscalationV1::Accuse`]); no difference clears the claim ([`TirEscalationV1::Cleared`]);
//! a block nobody serves ends in [`TirEscalationV1::Unavailable`], naming it.
//!
//! This module is the seat's side and knows nothing of the wire: a [`TirBlockTransportV1`] returns bytes **already verified against the
//! class's `artifact_root`** (the node's implementation opens them as inventory multiproofs), and the checker never reads an unverified byte.

use std::cell::RefCell;

use misaka_palw_tir::{ParamSource, Tensor, TirProgramV1};

/// Why a block (or a whole weight) did not arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirFetchRefusalV1 {
    /// Nobody answered in time, or the answer did not open against the artifact root: the bytes are withheld.
    Withheld,
    /// The request cannot be answered (a range outside the instance, an unknown param).
    Malformed(String),
}

/// **Where a seat gets weight bytes it does not hold.** Every byte returned is verified against the artifact root by the implementor.
pub trait TirBlockTransportV1 {
    /// Pieces covering `ranges` (byte `(start, len)` ranges of instance `(param, layer)`): `(offset, bytes)` pairs, each at least what a
    /// verified inventory leaf carries; they may extend past the ranges.
    fn fetch_ranges(&self, param: u16, layer: Option<u16>, ranges: &[(u64, u64)]) -> Result<Vec<(u64, Vec<u8>)>, TirFetchRefusalV1>;

    /// The whole instance's bytes (a cone node's weight, held whole for the one exact recompute).
    fn fetch_param(&self, param: u16, layer: Option<u16>) -> Result<Vec<u8>, TirFetchRefusalV1>;
}

/// **What an escalation concluded.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirEscalationV1 {
    /// The first committed row the seat derives from the node's honest value differs from the claim's: the leaf (`slot`) for the court.
    Accuse { pos: u32, occurrence: u16, node: u16, slot: u32 },
    /// The check is cleared: the witness was wrong, or the failure did not reproduce, and the claim's committed rows agree.
    Cleared(TirClearedV1),
    /// A block (or a cone node's weight) was not served: it is named, and the seat files no `Valid`.
    Unavailable { occurrence: u16, node: u16, block: Option<u32>, param: Option<u16> },
    /// The node is not one a block can be fetched for (routed, batched, or derived weight): the interval goes to a holder (§II.8 b).
    NotBlockAddressable(String),
    /// The escalation could not reach a conclusion (a fault of another kind, a malformed answer).
    Inconclusive(String),
}

/// Why a cleared escalation cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirClearedV1 {
    /// Every named block recomputed to exactly what the producer served: the failure does not reproduce.
    BlocksAgree,
    /// The node's served value was wrong, but the cone's first committed row equals the claim's: the claim is honest, the witness was not.
    ConeAgrees,
}

/// What an escalation noted on the way (read by [`crate::TirSketchCheckerV1::escalate`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TirEscNoteV1 {
    Unavailable { occurrence: u16, node: u16, block: Option<u32> },
    NotBlockAddressable(String),
    BlocksAgree,
    Malformed(String),
}

/// A `ParamSource` that returns what the seat holds and fetches the rest whole, once, through the transport; a refusal is remembered.
pub(crate) struct TirFetchingParamsV1<'a> {
    pub held: &'a dyn ParamSource,
    pub program: &'a TirProgramV1,
    pub transport: &'a dyn TirBlockTransportV1,
    pub refused: RefCell<Option<(u16, Option<u16>)>>,
}

impl ParamSource for TirFetchingParamsV1<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        if let Some(t) = self.held.param(index, layer) {
            return Some(t);
        }
        let d = self.program.params.get(index as usize)?;
        match self.transport.fetch_param(index, layer) {
            Ok(bytes) => {
                let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                Tensor::from_le_bytes(d.dtype, &shape, &bytes).ok()
            }
            Err(_) => {
                *self.refused.borrow_mut() = Some((index, layer));
                None
            }
        }
    }
}

/// Fetched pieces of one instance, decoded: element lookup by element index.
pub(crate) struct TirPieceViewV1 {
    pieces: Vec<(usize, Vec<i128>)>,
}

impl TirPieceViewV1 {
    pub fn new(dtype: misaka_palw_tir::DType, pieces: Vec<(u64, Vec<u8>)>) -> Result<Self, String> {
        let w = dtype.width();
        let mut decoded = Vec::with_capacity(pieces.len());
        for (offset, bytes) in pieces {
            if offset as usize % w != 0 || bytes.len() % w != 0 {
                return Err("a piece is not aligned to its element width".into());
            }
            let t = Tensor::from_le_bytes(dtype, &[bytes.len() / w], &bytes).map_err(|e| e.to_string())?;
            decoded.push((offset as usize / w, t.data));
        }
        decoded.sort_by_key(|(start, _)| *start);
        Ok(Self { pieces: decoded })
    }

    pub fn elem(&self, index: usize) -> Option<i128> {
        let at = self.pieces.partition_point(|(start, _)| *start <= index).checked_sub(1)?;
        let (start, data) = &self.pieces[at];
        data.get(index - start).copied()
    }
}

/// **Where a block of a weight lives**, as the seat that asks and the holder that serves both derive it from the program alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirBlockAddressV1 {
    pub occurrence: u16,
    pub node: u16,
    pub param: u16,
    pub layer: Option<u16>,
    /// Blocks the weight is sketched in (`ceil(bytes / F)` clamped to its free extent) and this one.
    pub blocks: u32,
    pub block: u32,
    /// Byte ranges of the instance the block reads.
    pub ranges: Vec<(u64, u64)>,
}

/// The weight products a class can be asked about, in a fixed order: `(occurrence, node)` of every weight `MatMul`, occurrences in schedule order.
/// A site's position here is its ordinal on the wire.
pub fn tir_weight_sites_v1(
    plan: &misaka_palw_tir_exec::TirPlan,
    analysis: &crate::analysis::TirSketchAnalysisV1,
) -> Vec<(u16, u16)> {
    let mut sites = Vec::new();
    for (occ, &(block, _)) in plan.occurrences.iter().enumerate() {
        for site in &analysis.blocks[block as usize].matmuls {
            if matches!(site.kind, crate::analysis::TirMatMulKindV1::Weight { .. }) {
                sites.push((occ as u16, site.node));
            }
        }
    }
    sites
}

/// **The address of block `block` of weight site `ordinal`** — `None` for a site that is not a plain static param weight (routed, batched or
/// derived: a holder takes the interval, §II.8 b) or a block outside it.
pub fn tir_block_address_v1(
    plan: &misaka_palw_tir_exec::TirPlan,
    analysis: &crate::analysis::TirSketchAnalysisV1,
    ordinal: u32,
    block: u32,
) -> Option<TirBlockAddressV1> {
    use crate::analysis::{TirMatMulKindV1, TirWeightSourceV1};
    let (occ, node) = *tir_weight_sites_v1(plan, analysis).get(ordinal as usize)?;
    let (blk, _) = plan.occurrences[occ as usize];
    let site = analysis.site(blk, node)?;
    let TirMatMulKindV1::Weight { side, source: TirWeightSourceV1::Static(misaka_palw_tir::Ref::Param(j)) } = site.kind else { return None };
    let np = &plan.blocks[blk as usize].nodes[node as usize];
    let g = crate::geom::TirCheckGeomV1::new(side, &np.in_types[0], &np.in_types[1], 1, 0);
    if !g.plain_weight() {
        return None;
    }
    let decl = plan.program.params.get(j as usize)?;
    let elements: u64 = decl.shape.iter().map(|x| u64::from(*x)).product();
    let bytes = elements.checked_mul(decl.dtype.width() as u64)?;
    let blocks = crate::sketch::tir_block_count_v1(bytes).clamp(1, g.free().max(1)) as u32;
    let layer = if decl.per_layer { plan.occurrences[occ as usize].1 } else { None };
    let ranges = g.block_byte_ranges(blocks as usize, block as usize, decl.dtype.width())?;
    Some(TirBlockAddressV1 { occurrence: occ, node, param: j, layer, blocks, block, ranges })
}
