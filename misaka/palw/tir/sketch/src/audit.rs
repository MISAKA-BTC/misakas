//! **A random leaf audit from openings alone** (RFC-0007 Part IV.1) — the one-move court's leaf
//! check, run off-chain by any bonded seat, holding no model.
//!
//! The auditor names one committed tile of a claim (a node's `tile_len`-value run at one position,
//! as the step tree cuts it) and recomputes it with the court's own evaluator
//! ([`misaka_palw_tir::demand::eval_demanded`], spec 04b §9.4). Every value the evaluation does not
//! compute it asks for, and every answer is an OPENING:
//!
//! * a committed value of the claim — another commit point of the tile's cone, a carry-in, a
//!   history row of an earlier position — opened against the step root (here: read from the served
//!   committed rows; counted per tile);
//! * a param element — opened against the artifact root as its inventory row (≤ 32 KiB pieces,
//!   closed-form index; counted per row);
//! * the position's token.
//!
//! The audit finds a lie when its recomputed tile differs from the committed one. It needs no weight
//! it does not open, and it opens only what the tile's cone reads — the court's cone, bounded by
//! admission (≤ 16 Mi terminal multiply-adds a tile). [`TirAuditTallyV1`] counts what it opened, so
//! the tests can report the bytes an audit costs and how often audits find a fabrication.
//!
//! **What it catches.** A claim whose producer fabricated a fraction `q` of its tiles is caught by
//! `m` independent uniform audits with probability `1 − (1 − q)^m`; a single lied tile among `N`, by
//! about `m/N`. It is a sensor against wholesale fabrication and partial skipping, never the security
//! floor for a one-point lie (RFC-0007 Part IV.1).

use std::collections::{BTreeMap, BTreeSet};

use misaka_palw_tir::demand::{
    DemandContext, DemandLimits, DemandRequest, DemandSource, DemandTarget, StateSupply, eval_demanded, hist_row_node_v1,
};
use misaka_palw_tir::{ParamSource, Tensor, TirError, TirErrorKind, TirResult};
use misaka_palw_tir_exec::TirPlan;

use crate::witness::TirWitnessV1;

/// What a set of audits opened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TirAuditTallyV1 {
    /// Committed tiles opened: `(pos, occurrence, node, tile)`.
    pub tiles: BTreeSet<(u32, u16, u16, usize)>,
    /// Artifact rows opened: `(param, layer, row)`, with each row's bytes.
    pub rows: BTreeMap<(u16, Option<u16>, usize), u64>,
    /// Tokens opened.
    pub tokens: BTreeSet<u32>,
}

impl TirAuditTallyV1 {
    /// Bytes of the opened values: committed tiles as 4-byte lanes, rows at their width, tokens.
    pub fn value_bytes(&self, tile_len: usize) -> u64 {
        self.tiles.len() as u64 * tile_len as u64 * 4 + self.rows.values().sum::<u64>() + self.tokens.len() as u64 * 4
    }

    /// Openings (each carries one Merkle path).
    pub fn openings(&self) -> u64 {
        (self.tiles.len() + self.rows.len()) as u64
    }
}

/// The auditor's source: the claim's committed rows and tokens, the artifact's params.
struct Openings<'a> {
    plan: &'a TirPlan,
    commits: BTreeMap<(u32, u16, u16), &'a Tensor>,
    witness: &'a TirWitnessV1,
    params: &'a dyn ParamSource,
    cache: BTreeMap<(u16, Option<u16>), Tensor>,
    tile_len: usize,
    tally: TirAuditTallyV1,
}

impl Openings<'_> {
    fn committed(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        let t = self.commits.get(&(ctx.pos, ctx.occurrence, node)).ok_or_else(|| {
            TirError::new(TirErrorKind::Missing, format!("no committed node {node} at ({}, {})", ctx.pos, ctx.occurrence))
        })?;
        let v = *t.data.get(index).ok_or_else(|| TirError::new(TirErrorKind::Missing, "an element past the node"))?;
        self.tally.tiles.insert((ctx.pos, ctx.occurrence, node, index / self.tile_len));
        Ok(v)
    }
}

impl DemandSource for Openings<'_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        self.committed(ctx, node, index)
    }

    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        let p = &self.plan.program.params[param as usize];
        let key = (param, if p.per_layer { layer } else { None });
        if !self.cache.contains_key(&key) {
            let t =
                self.params.param(key.0, key.1).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("param {}", p.name)))?;
            self.cache.insert(key, t);
        }
        let t = &self.cache[&key];
        // An inventory row: an axis-0 slice of a rank-≥2 tensor, the whole tensor otherwise.
        let row_len: usize = if p.shape.len() >= 2 { p.shape[1..].iter().map(|d| *d as usize).product() } else { t.data.len() };
        let row_bytes = (row_len * p.dtype.width()) as u64;
        self.tally.rows.insert((key.0, key.1, index / row_len.max(1)), row_bytes);
        t.data.get(index).copied().ok_or_else(|| TirError::new(TirErrorKind::Missing, "a param element past its tensor"))
    }

    fn state(&mut self, pos: u32, _state: u16, _layer: Option<u16>, _index: usize) -> TirResult<StateSupply> {
        // A checkpoint is a committed leaf too; these fixtures carry no `Fixed` state, and a replay
        // walks back to the initial zero.
        Ok(if pos == 0 { StateSupply::Value(0) } else { StateSupply::Replay })
    }

    fn hist_row(&mut self, _pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        let (ctx, node) = hist_row_node_v1(&self.plan.program, state, layer, row_pos)
            .ok_or_else(|| TirError::new(TirErrorKind::Missing, "no committed row for that history position"))?;
        self.committed(ctx, node, index)
    }

    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.tally.tokens.insert(pos);
        self.witness.tokens.get(pos as usize).copied().ok_or_else(|| TirError::new(TirErrorKind::Missing, "no token"))
    }
}

/// One committed tile of a claim: position, occurrence, node, tile index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TirAuditTileV1 {
    pub pos: u32,
    pub occurrence: u16,
    pub node: u16,
    pub tile: usize,
}

/// Every committed tile of a witness's claim, cut at `tile_len` values.
pub fn tir_audit_tiles_v1(plan: &TirPlan, witness: &TirWitnessV1, tile_len: usize) -> Vec<TirAuditTileV1> {
    let mut out = Vec::new();
    for step in &witness.steps {
        for c in &step.commits {
            let occ = plan.slot_bases.iter().rposition(|b| *b <= c.slot).unwrap_or(0);
            let node = (c.slot - plan.slot_bases[occ]) as u16;
            for tile in 0..c.value.data.len().div_ceil(tile_len) {
                out.push(TirAuditTileV1 { pos: step.pos, occurrence: occ as u16, node, tile });
            }
        }
    }
    out
}

/// **Audit one tile**: recompute it from openings with the court's evaluator. `Ok(true)` when the
/// committed tile is the recomputed one, `Ok(false)` when the audit found a lie; what it opened is
/// added to `tally`.
pub fn tir_leaf_audit_v1(
    plan: &TirPlan,
    witness: &TirWitnessV1,
    params: &dyn ParamSource,
    tile: TirAuditTileV1,
    tile_len: usize,
    tally: &mut TirAuditTallyV1,
) -> Result<bool, String> {
    let mut commits = BTreeMap::new();
    for step in &witness.steps {
        for c in &step.commits {
            let occ = plan.slot_bases.iter().rposition(|b| *b <= c.slot).unwrap_or(0);
            commits.insert((step.pos, occ as u16, (c.slot - plan.slot_bases[occ]) as u16), &c.value);
        }
    }
    let committed = *commits.get(&(tile.pos, tile.occurrence, tile.node)).ok_or("the tile is not committed")?;
    let from = tile.tile * tile_len;
    let to = (from + tile_len).min(committed.data.len());
    let elements: Vec<usize> = (from..to).collect();
    let mut source = Openings { plan, commits, witness, params, cache: BTreeMap::new(), tile_len, tally: std::mem::take(tally) };
    let request = DemandRequest {
        target: DemandTarget::Node { ctx: DemandContext { pos: tile.pos, occurrence: tile.occurrence }, node: tile.node },
        elements: &elements,
    };
    let r = eval_demanded(&plan.program, &plan.info, &request, &mut source, &DemandLimits::UNLIMITED);
    *tally = source.tally;
    match r {
        Ok((values, _)) => Ok(values[..] == committed.data[from..to]),
        // A recomputation that cannot run on the claim's own openings is a malformed commitment —
        // the producer's (PALW-TIR-33): an out-of-interval or out-of-dtype committed operand.
        Err(misaka_palw_tir::demand::DemandError::Tir(e)) if e.kind == TirErrorKind::Operand => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
