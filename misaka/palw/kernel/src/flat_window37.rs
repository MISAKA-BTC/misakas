//! Complete typed window graph, version 37 (RFC02 §II.14.60). `flat_history8`
//! proves only the SELECTION over an authenticated history image; it explicitly
//! leaves the image's own continuity open. This module closes it:
//!
//! * **KV/state binding (append).** At every position p the newest row of each
//!   declared history image equals that position's committed writer node output
//!   (the K/V or state row actually computed at p), never a supplied value.
//! * **State continuity.** Every older row of the image at p equals the image at
//!   p-1, shifted by exactly one row once the window is full — no reset, skip,
//!   duplicate or reordered KV row, across segment boundaries alike.
//!
//! Both relations are checked on the same segment roots as every other court and
//! localize to one bounded two-point counterexample. The selection relation's
//! source must be one of these continuity-bound histories, so a v8 selection can
//! no longer run over an image the graph did not build. Unverified source
//! checkpoint (2026-10-15).
use crate::{
    flat_court4 as common,
    flat_court5::FlatOperandV5,
    flat_history8::{HistoryNodeV8, HistorySelectionPlanV8, source_point},
    flat_topk7::{at, operand},
    hash::{Digest, object_id},
    merkle5::flat_material_bounds_v5,
    seg::{NodeOpeningV1, segment_count_v1},
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::DType;
use std::io::{self, Cursor};

pub const WINDOW_GRAPH_VERSION_V37: u16 = 37;
pub const WINDOW_GRAPH_MAX_APPENDS_V37: usize = 256;
pub const WINDOW_GRAPH_MAX_BYTES_V37: usize = 64 << 10;
pub const WINDOW_FAULT_MAX_BYTES_V37: u64 = 128 << 10;

/// One continuity-bound history (a layer's K, V or recurrent state image).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WindowAppendV37 {
    /// Node holding the history image at p: shape `[min(p+1, window), row_shape..]`.
    pub history: HistoryNodeV8,
    /// Node whose output at p is the appended row: shape `row_shape`.
    pub writer: HistoryNodeV8,
    pub dtype: DType,
    pub row_shape: Vec<u32>,
}
/// The typed v8 graph: one program, its continuity-bound histories and the
/// selection relation that reads one of them.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WindowGraphV37 {
    pub version: u16,
    pub program_root: Digest,
    pub nodes_per_position: u64,
    pub window: u32,
    pub max_positions: u32,
    pub appends: Vec<WindowAppendV37>,
    pub selection: HistorySelectionPlanV8,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct WindowGraphScopeV37 {
    pub version: u16,
    pub graph_root: Digest,
    pub appends: u32,
    pub filing_bytes: u64,
    pub court_work: u64,
    pub court_scratch_bytes: u64,
    /// Ordinary verifier re-hash per position across all appends (bytes).
    pub continuity_hash_bytes_per_position: u128,
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded window-graph-v37 bytes")
}
fn rows_of(shape: &[u32]) -> Option<u64> {
    if shape.is_empty() || shape.len() > 3 {
        return None;
    }
    shape.iter().try_fold(1u64, |n, d| if *d == 0 || *d > misaka_palw_tir::types::MAX_DIM { None } else { n.checked_mul(*d as u64) })
}
impl WindowGraphV37 {
    /// Identity of the outer typed graph WITHOUT the selection plan (which binds it).
    pub fn graph_root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/window-graph-outer/v37",
            &(self.version, self.program_root, self.nodes_per_position, self.window, self.max_positions, &self.appends),
        )
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/window-graph/v37", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.selection.validate()?;
        if self.version != WINDOW_GRAPH_VERSION_V37
            || self.program_root == [0; 64]
            || self.appends.is_empty()
            || self.appends.len() > WINDOW_GRAPH_MAX_APPENDS_V37
            || self.window == 0
            || self.max_positions == 0
            || self.window > self.max_positions
            || self.nodes_per_position == 0
            || self.selection.graph_root != self.graph_root()
            || self.selection.window != self.window
            || self.selection.max_positions != self.max_positions
            || self.selection.nodes_per_position != self.nodes_per_position
        {
            return Err("window37 graph/selection identity, window or extent differs".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.appends {
            let rows = rows_of(&a.row_shape).ok_or("window37 append row shape")?;
            if rows.checked_mul(self.window as u64).is_none_or(|n| n > misaka_palw_tir::types::MAX_ELEMENTS)
                || a.writer.slot >= a.history.slot
                || a.history.slot >= self.nodes_per_position
                || !seen.insert((a.history.occurrence, a.history.node))
                || !seen.insert((a.writer.occurrence, a.writer.node))
            {
                return Err("window37 append bounds, order or duplicate coordinates".into());
            }
        }
        // KV/state binding of the selection: it must read a continuity-bound image.
        if !self.appends.iter().any(|a| {
            a.history == self.selection.source && a.dtype == self.selection.dtype && a.row_shape == self.selection.row_shape
        }) {
            return Err("window37 selection reads a history no append relation builds".into());
        }
        Ok(())
    }
    pub fn history_shape(&self, a: &WindowAppendV37, p: u32) -> Result<Vec<usize>, String> {
        if p >= self.max_positions {
            return Err("window37 position exceeds admitted extent".into());
        }
        let mut s = vec![p.saturating_add(1).min(self.window) as usize];
        s.extend(a.row_shape.iter().map(|d| *d as usize));
        Ok(s)
    }
    /// Cost/evidence at the admitted maximum; derived, never declared.
    pub fn scope(&self) -> Result<WindowGraphScopeV37, String> {
        self.validate()?;
        let opening = crate::element::node_opening_bytes_v1(self.nodes_per_position);
        let (mut leaf, mut hash) = (0u64, 0u128);
        for a in &self.appends {
            let full: Vec<u64> = std::iter::once(self.window as u64).chain(a.row_shape.iter().map(|d| *d as u64)).collect();
            let row: Vec<u64> = a.row_shape.iter().map(|d| *d as u64).collect();
            for shape in [&full, &row] {
                let elements: u64 = shape.iter().product();
                let bytes = if elements >= 64 {
                    flat_material_bounds_v5(a.dtype, shape).ok_or("window37 material bounds")?.leaf_value_bytes
                } else {
                    elements * a.dtype.width() as u64 * 4 + 512
                };
                leaf = leaf.max(bytes);
            }
            hash = hash
                .checked_add(rows_of(&a.row_shape).unwrap() as u128 * (self.window as u128 + 1) * a.dtype.width() as u128)
                .ok_or("window37 hash bound overflow")?;
        }
        let filing_bytes = 256 + 2 * opening + 2 * leaf;
        let court_work = filing_bytes * 128 + 16384;
        let court_scratch_bytes = filing_bytes * 16 + 4096;
        if filing_bytes > WINDOW_FAULT_MAX_BYTES_V37 || court_work > (1 << 30) {
            return Err("window37 terminal envelope exceeds cap".into());
        }
        Ok(WindowGraphScopeV37 {
            version: 37,
            graph_root: self.root(),
            appends: self.appends.len() as u32,
            filing_bytes,
            court_work,
            court_scratch_bytes,
            continuity_hash_bytes_per_position: hash,
        })
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > WINDOW_GRAPH_MAX_BYTES_V37 {
            return Err("window37 graph byte cap".into());
        }
        let g: Self = borsh::from_slice(raw).map_err(|_| "window37 graph grammar")?;
        if borsh::to_vec(&g).map_err(|e| e.to_string())? != raw {
            return Err("window37 noncanonical graph".into());
        }
        g.validate()?;
        Ok(g)
    }
}
/// Which continuity equation a counterexample breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum WindowFaultKindV37 {
    /// image_p[H_p - 1, row] != writer_p[row].
    Append,
    /// image_p[slot, row] != image_{p-1}[slot + shift, row], shift = 1 once p >= window.
    Continuity { slot: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct WindowFaultV37 {
    pub version: u16,
    pub graph_root: Digest,
    pub job_root: Digest,
    pub append: u16,
    pub row: u64,
    pub kind: WindowFaultKindV37,
    /// Image at p (its node opening fixes p).
    pub current: FlatOperandV5,
    /// Writer at p (Append) or image at p-1 (Continuity).
    pub other: FlatOperandV5,
}
impl WindowFaultV37 {
    fn structure(&self) -> io::Result<()> {
        if self.version != 37
            || self.current.node.is_none()
            || self.other.node.is_none()
            || self.current.flat.len() + self.current.legacy.len() > 2
            || self.other.flat.len() + self.other.legacy.len() > 2
        {
            return Err(invalid());
        }
        common::parts_structure(None, std::slice::from_ref(&self.current), &None)?;
        common::parts_structure(None, std::slice::from_ref(&self.other), &None)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        common::size_with_limit(self, WINDOW_FAULT_MAX_BYTES_V37)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > WINDOW_FAULT_MAX_BYTES_V37 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 37 {
            return Err(invalid());
        }
        let out = Self {
            version,
            graph_root: Digest::deserialize_reader(&mut r)?,
            job_root: Digest::deserialize_reader(&mut r)?,
            append: u16::deserialize_reader(&mut r)?,
            row: u64::deserialize_reader(&mut r)?,
            kind: WindowFaultKindV37::deserialize_reader(&mut r)?,
            current: operand(&mut r, 2)?,
            other: operand(&mut r, 2)?,
        };
        if r.position() as usize != bytes.len() {
            return Err(invalid());
        }
        out.structure()?;
        Ok(out)
    }
}
/// Public context: the graph, the job's committed positions/root and the claim's segment roots.
pub struct WindowContextV37<'a> {
    pub graph: &'a WindowGraphV37,
    pub scope: &'a WindowGraphScopeV37,
    pub job_root: Digest,
    pub positions: u32,
    pub segment_roots: &'a [Digest],
}
impl WindowContextV37<'_> {
    fn check(&self) -> Result<(), DismissalV1> {
        self.graph.validate().map_err(|s| na(&s))?;
        if self.graph.scope().map_err(|s| na(&s))? != *self.scope
            || self.positions == 0
            || self.positions > self.graph.max_positions
            || self.segment_roots.len() != segment_count_v1(self.positions) as usize
        {
            return Err(na("window37 scope or evidence extent differs"));
        }
        Ok(())
    }
    fn node(&self, o: &NodeOpeningV1, at: HistoryNodeV8, p: u32) -> bool {
        o.position == p
            && p < self.positions
            && o.occurrence == at.occurrence
            && o.node == at.node
            && o.authenticates(self.segment_roots, self.positions, at.slot, self.graph.nodes_per_position)
    }
}
/// Objective conviction from two authenticated points that violate an equation.
pub fn verify_window_fault_v37(c: &WindowContextV37<'_>, f: &WindowFaultV37) -> Result<(), DismissalV1> {
    c.check()?;
    f.structure().map_err(|_| na("malformed window37 proof"))?;
    if f.graph_root != c.graph.root() || f.job_root != c.job_root || common::size_with_limit(f, c.scope.filing_bytes).is_err() {
        return Err(na("window37 proof is outside its job/graph/scope"));
    }
    let a = c.graph.appends.get(f.append as usize).ok_or_else(|| na("window37 append index"))?;
    let current = f.current.node.as_ref().unwrap();
    let p = current.position;
    if !c.node(current, a.history, p) {
        return Err(na("window37 current history node differs"));
    }
    let shape = c.graph.history_shape(a, p).map_err(|s| na(&s))?;
    let h = shape[0] as u32;
    let row_shape: Vec<usize> = a.row_shape.iter().map(|d| *d as usize).collect();
    let mine = |slot: u32| -> Result<i128, DismissalV1> { source_point(&f.current, a.dtype, &shape, at(&shape, 0, f.row, slot)?) };
    let convict = match f.kind {
        WindowFaultKindV37::Append => {
            let writer = f.other.node.as_ref().unwrap();
            if !c.node(writer, a.writer, p) {
                return Err(na("window37 writer node differs"));
            }
            let total: u64 = row_shape.iter().map(|d| *d as u64).product();
            if f.row >= total {
                return Err(na("window37 append row out of range"));
            }
            mine(h - 1)? != source_point(&f.other, a.dtype, &row_shape, f.row)?
        }
        WindowFaultKindV37::Continuity { slot } => {
            if p == 0 || slot + 1 >= h {
                return Err(na("window37 continuity needs an older row of a later position"));
            }
            let previous = f.other.node.as_ref().unwrap();
            if !c.node(previous, a.history, p - 1) {
                return Err(na("window37 previous history node differs"));
            }
            let prev_shape = c.graph.history_shape(a, p - 1).map_err(|s| na(&s))?;
            let shift = u32::from(p >= c.graph.window);
            let old = source_point(&f.other, a.dtype, &prev_shape, at(&prev_shape, 0, f.row, slot + shift)?)?;
            mine(slot)? != old
        }
    };
    if convict { Ok(()) } else { Err(DismissalV1::NoFault) }
}
/// Honest-side ordinary check over two whole images (verifier re-execution path).
pub fn check_window_step_v37(
    graph: &WindowGraphV37,
    append: usize,
    p: u32,
    previous: Option<&misaka_palw_tir::Tensor>,
    current: &misaka_palw_tir::Tensor,
    writer: &misaka_palw_tir::Tensor,
) -> Result<(), String> {
    let a = graph.appends.get(append).ok_or("window37 append index")?;
    let shape = graph.history_shape(a, p)?;
    let rows: usize = a.row_shape.iter().map(|d| *d as usize).product();
    if current.shape != shape || current.dtype != a.dtype || writer.data.len() != rows || writer.dtype != a.dtype {
        return Err("window37 step image/writer typed extent differs".into());
    }
    let h = shape[0];
    if current.data[(h - 1) * rows..h * rows] != writer.data[..] {
        return Err("window37 newest row is not the committed writer output".into());
    }
    if p > 0 {
        let prev = previous.ok_or("window37 previous image required")?;
        let prev_shape = graph.history_shape(a, p - 1)?;
        if prev.shape != prev_shape {
            return Err("window37 previous image extent differs".into());
        }
        let shift = usize::from(p >= graph.window);
        if current.data[..(h - 1) * rows] != prev.data[shift * rows..(shift + h - 1) * rows] {
            return Err("window37 older rows are not the previous image (reset/skip/shift)".into());
        }
    }
    Ok(())
}
