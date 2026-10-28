//! **Params served by rows** — the executor's half of a runtime residency (ADR-0112 for IR classes;
//! `docs/design/palw/tir/runtime-residency.md`).
//!
//! A residency holds a class's pinned params whole and serves the rest — the instances the tiers
//! route or gather ([`crate::tiers`]) — as the rows a `Gather` names. The executor asks a
//! [`TirRowSourceV1`] three things: to **admit** a route's rows the moment its index is computed
//! (every stack of the route group at once, ADR-0112 Decision 4), to **gather** a node's rows into
//! its output, and, for the one read the tiers never plan, the **whole** instance (counted, so the
//! tests can say it never happened).
//!
//! # What the executor guarantees
//!
//! A row gather computes exactly what [`crate::kernels::misc::gather`] computes on the whole
//! instance: the same indices checked the same way (an index outside the view's rows is the same
//! `Index` failure, before anything is read), and row `i` of the view is elements
//! `i·unit .. (i+1)·unit` of the param, which is where a row-major view puts it. Nothing else in the
//! executor knows a source exists.

use std::collections::BTreeMap;
use std::sync::Mutex;

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::{Prim, TirError, TirErrorKind, TirResult};

use crate::elem::{Buf, Elem};
use crate::exec::{Reader, Src};
use crate::kernels::{Opd, Scratch, out_vec, widen};
use crate::layout::Layout;
use crate::params::ParamData;
use crate::plan::{NodePlan, TirPlan};
use crate::tiers::TirRowGatherV1;

/// **What serves a residency's row-addressed instances.** Implemented by the node's file-backed
/// store (`node::residency`) and, for tests and tools, by [`TirRowsInMemoryV1`].
pub trait TirRowSourceV1: Send + Sync {
    /// `(rows, unit)` of the view instance `(j, layer)` is served in, if this source serves it.
    fn row_shape(&self, j: u16, layer: Option<u16>) -> Option<(u32, u64)>;

    /// **Admit** every row `wants` names — `(param, layer, rows)` — before the first of them is
    /// gathered: what is missing is read together (ADR-0112 Decision 4). Advisory: a row that is not
    /// held when it is gathered is read then. A source that holds nothing does nothing.
    fn admit(&self, wants: &[(u16, Option<u16>, &[u32])]);

    /// **Rows `rows` of instance `(j, layer)`, in order, into `out`** (replacing its contents), as
    /// the param's dtype. Every row is inside the view (the caller checked).
    fn gather_rows(&self, j: u16, layer: Option<u16>, rows: &[u32], out: &mut Buf) -> Result<(), String>;

    /// **The whole instance** — the fallback for a dense read of a served instance, which the tiers
    /// never plan; a source counts it ([`TirRowCountsV1::whole_reads`]).
    fn read_whole(&self, j: u16, layer: Option<u16>) -> Result<ParamData<'static>, String>;

    /// `[min, max]` of the instance's elements, known without reading it now.
    fn range(&self, j: u16, layer: Option<u16>) -> Option<Interval>;

    /// What this source has done, for a log line or a test.
    fn counts(&self) -> TirRowCountsV1;
}

/// **A row source's counters.**
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirRowCountsV1 {
    /// Rows handed to gathers.
    pub rows_gathered: u64,
    /// Admissions asked (route groups).
    pub admissions: u64,
    /// Whole instances read for a dense read — zero on every path the tiers plan.
    pub whole_reads: u64,
}

/// Is `layout` the view a row gather serves: contiguous from the param's first element, `rows`
/// rows of `unit` elements?
pub(crate) fn is_row_view(layout: &Layout, site: &TirRowGatherV1) -> bool {
    layout.is_contiguous()
        && layout.offset == 0
        && layout.rank >= 1
        && layout.shape()[0] as u64 == site.rows as u64
        && layout.numel() as u64 == (site.rows as u64).saturating_mul(site.unit)
}

fn index_err(v: i64, n: usize) -> TirError {
    TirError::new(TirErrorKind::Index, format!("Gather: index {v} outside [0, {n})"))
}

/// **A row gather through the params' row source**: node `node` (at `(block, node index)` = `at`)
/// gathers rows of served param `j` through the view `site` by `idx`. Checks every index as the
/// whole-instance kernel does, admits the node's route group when it is the group's first member,
/// then reads its own rows into `out`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gather_rows(
    plan: &TirPlan,
    node: &NodePlan,
    rd: &Reader<'_>,
    at: (u8, u16),
    site: &TirRowGatherV1,
    j: u16,
    idx: &Opd<'_>,
    out: &mut Buf,
    scratch: &mut Scratch,
) -> TirResult<()> {
    debug_assert!(matches!(node.prim, Prim::Gather { axis: 0, batch_dims: 0 }));
    let source = rd.params.row_source().ok_or_else(|| TirError::new(TirErrorKind::Missing, "no row source"))?;
    let n = site.rows as usize;
    let [s0, _, _] = &mut scratch.w64;
    let iv = widen::<i64>(idx, s0);
    // The kernel's check, in its order: the first index outside the view fails the node.
    if let Some(bad) = iv.iter().find(|v| **v < 0 || **v as u128 >= n as u128) {
        return Err(index_err(*bad, n));
    }
    let rows: Vec<u32> = iv.iter().map(|v| *v as u32).collect();
    if site.first {
        let group = &plan.rows.groups[at.0 as usize][site.group as usize];
        if group.members.len() > 1 {
            // Every stack this route selects from, at this occurrence's layer, admitted together:
            // the members share this index, so these are their rows too (each within its own view).
            let mut wants: Vec<(u16, Option<u16>, Vec<u32>)> = Vec::new();
            for &(m, pj) in &group.members {
                let Some(other) = plan.rows.gathers[at.0 as usize][m as usize] else { continue };
                let instance = rd.params.instance_layer(pj, rd.layer);
                if !rd.params.serves_rows(pj, rd.layer) || source.row_shape(pj, instance) != Some((other.rows, other.unit)) {
                    continue;
                }
                if wants.iter().any(|(p, _, _)| *p == pj) {
                    continue;
                }
                let mine: Vec<u32> = rows.iter().copied().filter(|r| (*r as u64) < other.rows as u64).collect();
                wants.push((pj, instance, mine));
            }
            let borrowed: Vec<(u16, Option<u16>, &[u32])> = wants.iter().map(|(p, l, r)| (*p, *l, r.as_slice())).collect();
            source.admit(&borrowed);
        } else {
            source.admit(&[(j, rd.params.instance_layer(j, rd.layer), rows.as_slice())]);
        }
    }
    source
        .gather_rows(j, rd.params.instance_layer(j, rd.layer), &rows, out)
        .map_err(|e| TirError::new(TirErrorKind::Missing, format!("param {} rows: {e}", plan.program.params[j as usize].name)))
}

/// The value `v` of a gather's data operand is a row-served param's view this site serves.
pub(crate) fn served_view(rd: &Reader<'_>, src: Src, layout: &Layout, site: Option<&TirRowGatherV1>) -> Option<u16> {
    let Src::Param(j) = src else { return None };
    let site = site?;
    if site.param != j || !rd.params.serves_rows(j, rd.layer) || !is_row_view(layout, site) {
        return None;
    }
    let source = rd.params.row_source()?;
    (source.row_shape(j, rd.params.instance_layer(j, rd.layer)) == Some((site.rows, site.unit))).then_some(j)
}

/// **Rows held in memory, served as a residency serves them** — every routed or gathered instance
/// of a reference map ([`crate::params::TirParams::from_map_served`]). What the identity tests run
/// the executor's row path against, with no file in the way; it caches nothing, so every row a
/// gather asks for is a row it hands out.
pub struct TirRowsInMemoryV1 {
    held: BTreeMap<(u16, Option<u16>), (Buf, u32, u64)>,
    counts: Mutex<TirRowCountsV1>,
}

impl TirRowsInMemoryV1 {
    /// `held`: per instance, its elements, its view's rows and its elements per row.
    pub fn new(held: BTreeMap<(u16, Option<u16>), (Buf, u32, u64)>) -> Self {
        Self { held, counts: Mutex::new(TirRowCountsV1::default()) }
    }

    /// The instances this source serves.
    pub fn instances(&self) -> impl Iterator<Item = (u16, Option<u16>)> + '_ {
        self.held.keys().copied()
    }
}

impl TirRowSourceV1 for TirRowsInMemoryV1 {
    fn row_shape(&self, j: u16, layer: Option<u16>) -> Option<(u32, u64)> {
        self.held.get(&(j, layer)).map(|(_, rows, unit)| (*rows, *unit))
    }

    fn admit(&self, _wants: &[(u16, Option<u16>, &[u32])]) {
        self.counts.lock().expect("never poisoned").admissions += 1;
    }

    fn gather_rows(&self, j: u16, layer: Option<u16>, rows: &[u32], out: &mut Buf) -> Result<(), String> {
        let (data, n, unit) = self.held.get(&(j, layer)).ok_or_else(|| format!("param {j} (layer {layer:?}) is not served"))?;
        let unit = *unit as usize;
        crate::with_slice!(data.slice(), v => {
            let o = out_vec(out);
            o.clear();
            o.reserve(rows.len() * unit);
            for r in rows {
                if *r >= *n {
                    return Err(format!("row {r} outside {n}"));
                }
                let at = *r as usize * unit;
                o.extend_from_slice(&v[at..at + unit]);
            }
        });
        self.counts.lock().expect("never poisoned").rows_gathered += rows.len() as u64;
        Ok(())
    }

    fn read_whole(&self, j: u16, layer: Option<u16>) -> Result<ParamData<'static>, String> {
        let (data, _, _) = self.held.get(&(j, layer)).ok_or_else(|| format!("param {j} (layer {layer:?}) is not served"))?;
        self.counts.lock().expect("never poisoned").whole_reads += 1;
        ParamData::from_buf(data.clone()).map_err(|e| e.to_string())
    }

    fn range(&self, j: u16, layer: Option<u16>) -> Option<Interval> {
        crate::params::slice_range(self.held.get(&(j, layer))?.0.slice())
    }

    fn counts(&self) -> TirRowCountsV1 {
        *self.counts.lock().expect("never poisoned")
    }
}

/// Append `bytes` — little-endian elements of `T` — to `out`.
#[cfg_attr(not(feature = "node"), allow(dead_code))]
pub(crate) fn extend_le<T: Elem>(out: &mut Vec<T>, bytes: &[u8]) {
    let w = std::mem::size_of::<T>();
    if cfg!(target_endian = "little") {
        // SAFETY: every bit pattern is a value of the integer type T, and `align_to` only returns a
        // middle part that is correctly aligned.
        let (pre, mid, post) = unsafe { bytes.align_to::<T>() };
        if pre.is_empty() && post.is_empty() {
            out.extend_from_slice(mid);
            return;
        }
    }
    out.extend(bytes.chunks_exact(w).map(|c| T::from_i128(T::DTYPE.decode_le(c))));
}
