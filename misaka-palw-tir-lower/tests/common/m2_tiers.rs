//! **Vendored verbatim from lane M2's branch `tir/residency` (35e960397), `misaka-palw-tir-exec/src/tiers.rs`** — the node's classification of an IR's params into
//! pinned / routed / gathered, read off the PROGRAM's dataflow. It is here so that `tests/row_addressing.rs` can hold the lowerer to the shapes that
//! runtime can address by row (an axis-0 gather of the param, or of a reshape chain of it); when `tir/residency` merges, this copy is
//! replaced by the real module and the test keeps its meaning. Do not edit.

//! **Which of a class's weights a forward reads whole, which by a route and which by an input** —
//! the tiers of the generic executor's runtime residency (ADR-0112 for every IR class;
//! `docs/design/palw/tir/runtime-residency.md`).
//!
//! ADR-0112 told the Qwen3.6 runtime's two tiers apart by tensor NAME (`blk.N.ffn_expert.K_*`). An
//! IR class has no names a runtime may read meaning into — every model registers with no
//! per-architecture code — so the tiers here are read off the PROGRAM's dataflow, the one thing
//! every class has:
//!
//! * a param is **row-addressed** when every use a forward makes of it is a
//!   `Gather { axis: 0, batch_dims: 0 }` of the param itself or of an uncommitted, uncarried
//!   `Reshape` chain of it. Such a gather reads whole rows of the gathered view, and a row of a
//!   row-major view of a contiguous tensor is a contiguous run of the param's elements: `unit`
//!   elements at `row × unit`, one positional read of the file. Every other use — a `MatMul`, an
//!   elementwise operand, an index, a committed or carried view — reads it **densely**;
//! * a row-addressed param is **routed** when some gather's index is computed from the weights (a
//!   `TopK` over router logits: the mixture's experts) and **gathered** when every index is a
//!   function of the inputs alone — the token, the position, a history of tokens: embeddings,
//!   per-layer embeddings, n-gram tables, positional tables. The taint is a fixpoint over nodes,
//!   carries and states ([`weight_taint_v1`]);
//! * everything else is **pinned**: read densely, or too small for its rows to be worth a read each
//!   ([`TirTierRulesV1::pin_below_bytes`]), or read whole by every forward anyway.
//!
//! A tier moves bytes, never values. A budgeted store serves a routed or gathered row as exactly the
//! elements the mapping holds, and the executor answers any dense read of a row-served instance with
//! a counted whole read of it (`TirParams::get`), so no committed byte depends on this
//! classification being right — only how much is held and how much is read does
//! (`tests/residency.rs` holds the identity over random programs, the goldens and the corpus).

use std::collections::{BTreeMap, BTreeSet};

use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{Dim, Prim, Ref, TensorType};

/// **ADR-0112 Decision 2's ratio, for IR classes**: with no budget stated, a class's weights are
/// held within a fifth of their bytes (where the host can spare it).
pub const TIR_RESIDENT_FRACTION_DENOMINATOR_V1: u64 = 5;

/// **A row-addressed instance under this many bytes is pinned** — its rows are not worth a read
/// each, and a budget cannot notice it: an activation table (65,536 `i16` codes, 128 KiB), a
/// per-token scale vector, a router's selection bias. The embedding tables, the expert stacks and
/// the n-gram tables a residency exists for are hundreds of MiB.
pub const TIR_PIN_BELOW_BYTES_V1: u64 = 1 << 20;

/// The node's rules for telling the tiers apart. Not a consensus value: a node may hold more or
/// less, and every choice computes the same bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirTierRulesV1 {
    /// A row-addressed instance of fewer bytes is pinned ([`TIR_PIN_BELOW_BYTES_V1`] by default;
    /// `0` row-serves every row-addressed param — what the identity tests run under).
    pub pin_below_bytes: u64,
}

impl Default for TirTierRulesV1 {
    fn default() -> Self {
        Self { pin_below_bytes: TIR_PIN_BELOW_BYTES_V1 }
    }
}

/// **Why a param is pinned.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirPinnedWhyV1 {
    /// Some use reads it whole: a `MatMul` or elementwise operand, an index, a committed, carried
    /// or logits view, any view but a row-major reshape.
    Dense,
    /// Row-addressed, but an instance is under [`TirTierRulesV1::pin_below_bytes`].
    Small,
    /// Row-addressed, but a forward reads at least as many of its rows as it has.
    EveryRow,
    /// Row-addressed through views of different row shapes.
    UnevenRows,
    /// No node of any block reads it.
    Unread,
}

/// **What a forward does with a param.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirTierV1 {
    /// Read whole by every forward: read once, held for the class's life.
    Pinned(TirPinnedWhyV1),
    /// Rows chosen by a value the forward computes from the weights (the mixture's experts):
    /// read when the route commits, held under what the budget leaves.
    Routed,
    /// Rows chosen by the inputs alone (embedding, per-layer embedding, n-gram and positional
    /// tables): read per row, never resident whole.
    Gathered,
}

impl TirTierV1 {
    /// Routed or gathered: served by rows.
    pub fn is_rows(self) -> bool {
        matches!(self, Self::Routed | Self::Gathered)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Pinned(_) => "pinned",
            Self::Routed => "routed",
            Self::Gathered => "gathered",
        }
    }
}

/// One param's tier and the numbers its arithmetic is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirParamTierV1 {
    pub tier: TirTierV1,
    /// Bytes of one instance.
    pub instance_bytes: u64,
    /// Its instances: `None` for a global; every layer whose scheduled block reads a per-layer one.
    pub instances: Vec<Option<u16>>,
    /// Row-addressed: the gathered view's rows and elements per row (both 0 otherwise).
    pub rows: u32,
    pub unit: u64,
    /// Row-addressed: the most rows one forward reads of one instance, at most `rows` (index
    /// elements summed over its gathers, at the widest history its blocks keep).
    pub per_forward: u64,
}

impl TirParamTierV1 {
    /// Bytes of one row.
    pub fn unit_bytes(&self, width: usize) -> u64 {
        self.unit.saturating_mul(width as u64)
    }
}

/// **A row gather** of a compiled plan: node `node` of its block reads rows of param `param` through
/// the view `[rows, …]` (`unit` elements a row), indexed by its group's index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirRowGatherV1 {
    pub param: u16,
    pub rows: u32,
    pub unit: u64,
    /// The group (in the block's [`TirRowGroupV1`] list) of the gathers sharing this index.
    pub group: u16,
    /// This node is the group's first — where the group's rows are admitted (ADR-0112 Decision 4).
    pub first: bool,
}

/// **The row gathers of one block that share one index** — a route and every stack it selects
/// rows of (the gate, up and down experts, their scales). Admitted together, the moment the first
/// of them runs: the index is computed by then, and nothing of the group has been read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirRowGroupV1 {
    pub index: Ref,
    /// Index elements at the block's widest history.
    pub index_elems: u64,
    /// `(node, param)` of every member, ascending by node.
    pub members: Vec<(u16, u16)>,
}

/// **Every row gather of a program, by block** — structural (rules-independent), so a plan holds it
/// for every executor and every artifact of the class.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TirRowSitesV1 {
    /// `[block][node]`: the node's row gather, if it is one.
    pub gathers: Vec<Vec<Option<TirRowGatherV1>>>,
    /// `[block]`: its groups.
    pub groups: Vec<Vec<TirRowGroupV1>>,
    /// Per param: some use reads it whole.
    pub dense: Vec<bool>,
}

/// The type a `Ref` of block `bi` names, or `None` for a reference past the program.
fn ref_type(p: &TirProgramV1, bi: usize, r: Ref) -> Option<TensorType> {
    let b = p.blocks.get(bi)?;
    Some(match r {
        Ref::Node(j) => b.nodes.get(j as usize)?.out.clone(),
        Ref::CarryIn(k) => b.carry_in.get(k as usize)?.clone(),
        Ref::Param(j) => {
            let d = p.params.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::Const(j) => {
            let d = p.consts.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::State(j) => {
            let d = p.states.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::Input(_) => TensorType::scalar(misaka_palw_tir::DType::Idx),
    })
}

/// The widest history `H` block `bi` can see: the window of the histories it appends to (one,
/// PALW-TIR-31), capped at the program's bound; 1 for a block that appends to none.
fn block_window(p: &TirProgramV1, bi: usize) -> u64 {
    let Some(b) = p.blocks.get(bi) else { return 1 };
    b.nodes
        .iter()
        .filter_map(|n| match n.prim {
            Prim::HistAppend { state } => match p.states.get(state as usize)?.kind {
                misaka_palw_tir::program::StateKind::Hist { window } => Some(window as u64),
                _ => None,
            },
            _ => None,
        })
        .max()
        .unwrap_or(1)
        .min(p.history_bound as u64)
        .max(1)
}

fn fixed_dims(t: &TensorType) -> Option<Vec<u64>> {
    t.shape
        .iter()
        .map(|d| match d {
            Dim::Fixed(n) => Some(*n as u64),
            Dim::H => None,
        })
        .collect()
}

/// **Every row gather of `p`, grouped by index, and which params some use reads whole.**
pub fn tir_row_sites_v1(p: &TirProgramV1) -> TirRowSitesV1 {
    let mut out = TirRowSitesV1 {
        gathers: p.blocks.iter().map(|b| vec![None; b.nodes.len()]).collect(),
        groups: vec![Vec::new(); p.blocks.len()],
        dense: vec![false; p.params.len()],
    };
    let post = p.schedule.post as usize;
    for (bi, b) in p.blocks.iter().enumerate() {
        // `view[k]`: node k is a transparent view of a param — a Reshape chain from it that no one
        // outside the block sees (not committed, not carried, not the logits).
        let mut view: Vec<Option<u16>> = vec![None; b.nodes.len()];
        let carried: BTreeSet<u16> = b.carry_out.iter().copied().collect();
        let origin = |view: &[Option<u16>], r: Ref| -> Option<u16> {
            match r {
                Ref::Param(j) => Some(j),
                Ref::Node(k) => view.get(k as usize).copied().flatten(),
                _ => None,
            }
        };
        let window = block_window(p, bi);
        let mut by_index: BTreeMap<Ref, usize> = BTreeMap::new();
        for (ni, n) in b.nodes.iter().enumerate() {
            let observed = n.commit || carried.contains(&(ni as u16)) || (bi == post && ni == p.logits as usize);
            for (slot, r) in n.inputs.iter().enumerate() {
                let Some(j) = origin(&view, *r) else { continue };
                if j as usize >= p.params.len() {
                    continue;
                }
                match (&n.prim, slot) {
                    (Prim::Reshape, 0) if !observed && fixed_dims(&n.out).is_some() => view[ni] = Some(j),
                    (Prim::Gather { axis: 0, batch_dims: 0 }, 0) => {
                        let data = ref_type(p, bi, *r).and_then(|t| fixed_dims(&t));
                        let idx = n.inputs.get(1).and_then(|i| ref_type(p, bi, *i));
                        match (data, idx) {
                            (Some(dims), Some(idx)) if !dims.is_empty() && dims[0] > 0 && dims[0] <= u32::MAX as u64 => {
                                let rows = dims[0];
                                let unit = dims[1..].iter().product::<u64>();
                                let index = n.inputs[1];
                                let index_elems = idx.elements_at(window);
                                let g = *by_index.entry(index).or_insert_with(|| {
                                    out.groups[bi].push(TirRowGroupV1 { index, index_elems, members: Vec::new() });
                                    out.groups[bi].len() - 1
                                });
                                let first = out.groups[bi][g].members.is_empty();
                                out.groups[bi][g].members.push((ni as u16, j));
                                out.gathers[bi][ni] =
                                    Some(TirRowGatherV1 { param: j, rows: rows as u32, unit, group: g as u16, first });
                            }
                            _ => out.dense[j as usize] = true,
                        }
                    }
                    _ => out.dense[j as usize] = true,
                }
            }
        }
    }
    out
}

/// **Which values a forward computes from the weights** ([`weight_taint_v1`]): per node, per
/// state and per block carry-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirWeightTaintV1 {
    /// `[block][node]`.
    pub nodes: Vec<Vec<bool>>,
    /// Per state: some write to it is tainted.
    pub states: Vec<bool>,
    /// `[block][carry-in]`: the carry-out of some occurrence before the block's is tainted.
    pub carries: Vec<Vec<bool>>,
}

impl TirWeightTaintV1 {
    /// The taint of `r` read in block `bi` (a reference past the program is tainted: unknown is
    /// a route, which holds rows rather than reading them every forward).
    pub fn of(&self, bi: usize, r: Ref) -> bool {
        match r {
            Ref::Param(_) => true,
            Ref::Const(_) | Ref::Input(_) => false,
            Ref::State(j) => self.states.get(j as usize).copied().unwrap_or(true),
            Ref::CarryIn(k) => self.carries.get(bi).and_then(|c| c.get(k as usize)).copied().unwrap_or(true),
            Ref::Node(k) => self.nodes.get(bi).and_then(|n| n.get(k as usize)).copied().unwrap_or(true),
        }
    }
}

/// **Which values a forward computes from the weights** — a fixpoint over the nodes, the carries
/// between occurrences and the states: a param taints, the token, the position, a const and an
/// `Iota` do not; a node is tainted by any tainted input (a history window by its earlier rows as
/// well); a state by any tainted write; a carry-in by the carry-out of any occurrence that precedes
/// one of its block's. An index that is not tainted is a function of the inputs alone (a token, a
/// position, a history of tokens); one that is, is a route.
pub fn weight_taint_v1(p: &TirProgramV1) -> TirWeightTaintV1 {
    let mut t = TirWeightTaintV1 {
        nodes: p.blocks.iter().map(|b| vec![false; b.nodes.len()]).collect(),
        states: vec![false; p.states.len()],
        carries: p.blocks.iter().map(|b| vec![false; b.carry_in.len()]).collect(),
    };
    let occurrences = p.occurrences();
    loop {
        let mut next = t.clone();
        for (bi, b) in p.blocks.iter().enumerate() {
            for (ni, n) in b.nodes.iter().enumerate() {
                // Earlier nodes of this block from this pass; leaves from the last one.
                let any = n.inputs.iter().any(|r| match *r {
                    Ref::Node(k) => next.nodes[bi].get(k as usize).copied().unwrap_or(true),
                    other => t.of(bi, other),
                });
                next.nodes[bi][ni] = match n.prim {
                    Prim::StateWrite { state } => {
                        if let Some(w) = next.states.get_mut(state as usize) {
                            *w |= any;
                        }
                        any
                    }
                    Prim::HistAppend { state } => {
                        let earlier = t.states.get(state as usize).copied().unwrap_or(true);
                        if let Some(w) = next.states.get_mut(state as usize) {
                            *w |= any;
                        }
                        any || earlier
                    }
                    _ => any,
                };
            }
        }
        for w in occurrences.windows(2) {
            let (from, to) = (w[0].0 as usize, w[1].0 as usize);
            for (k, c) in p.blocks[from].carry_out.iter().enumerate() {
                let tainted = next.nodes[from].get(*c as usize).copied().unwrap_or(true);
                if let Some(slot) = next.carries[to].get_mut(k) {
                    *slot |= tainted;
                }
            }
        }
        if next == t {
            return t;
        }
        t = next;
    }
}

/// Every param's instances (the consensus inventory's rule, `palw_tir_param_instances_v1`): `None`
/// for a global; for a per-layer param every layer whose scheduled block references it.
pub fn tir_param_instances_v1(p: &TirProgramV1) -> Vec<Vec<Option<u16>>> {
    let mut readers: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); p.params.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for n in &b.nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r
                    && let Some(set) = readers.get_mut(*j as usize)
                {
                    set.insert(bi as u8);
                }
            }
        }
    }
    p.params
        .iter()
        .enumerate()
        .map(|(j, d)| {
            if d.per_layer {
                p.schedule.layers.iter().enumerate().filter(|(_, k)| readers[j].contains(k)).map(|(l, _)| Some(l as u16)).collect()
            } else {
                vec![None]
            }
        })
        .collect()
}

/// **The tiers of every param of a program**, under a node's rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirTiersV1 {
    pub rules: TirTierRulesV1,
    pub params: Vec<TirParamTierV1>,
    /// Per occurrence (`p.occurrences()` order), the bytes of each route group's routed rows — the
    /// admissions a forward makes, whose largest is the in-flight term of the floor.
    admissions: Vec<u64>,
    widths: Vec<usize>,
}

impl TirTiersV1 {
    /// **Classify every param of `p`** (see the module documentation).
    pub fn of(p: &TirProgramV1, rules: TirTierRulesV1) -> Self {
        let sites = tir_row_sites_v1(p);
        let taint = weight_taint_v1(p);
        let instances = tir_param_instances_v1(p);
        let occurrences = p.occurrences();
        let np = p.params.len();
        // Per block and param: rows a forward's occurrence of the block reads of it, and whether
        // some index is a route.
        let mut reads: Vec<BTreeMap<u16, u64>> = vec![BTreeMap::new(); p.blocks.len()];
        let mut view: Vec<Option<(u32, u64)>> = vec![None; np];
        let mut uneven = vec![false; np];
        let mut routed = vec![false; np];
        let mut row_read = vec![false; np];
        for (bi, groups) in sites.groups.iter().enumerate() {
            for g in groups {
                let tainted = taint.of(bi, g.index);
                for &(node, j) in &g.members {
                    let gather = sites.gathers[bi][node as usize].expect("a member is a row gather");
                    let shape = (gather.rows, gather.unit);
                    match view[j as usize] {
                        None => view[j as usize] = Some(shape),
                        Some(s) if s != shape => uneven[j as usize] = true,
                        Some(_) => {}
                    }
                    row_read[j as usize] = true;
                    routed[j as usize] |= tainted;
                }
                // Two gathers of one param by one index read the same rows: counted once.
                let members: BTreeSet<u16> = g.members.iter().map(|(_, j)| *j).collect();
                for j in members {
                    *reads[bi].entry(j).or_default() += g.index_elems;
                }
            }
        }
        let mut params = Vec::with_capacity(np);
        for (j, d) in p.params.iter().enumerate() {
            let instance_bytes = d.shape.iter().map(|x| *x as u64).product::<u64>().saturating_mul(d.dtype.width() as u64);
            let inst = instances[j].clone();
            let (rows, unit) = view[j].unwrap_or((0, 0));
            // The most rows one forward reads of one instance: a per-layer instance is read by the
            // occurrence at its layer, a global one by every occurrence of every block that reads it.
            let per_instance = |layer: Option<u16>| -> u64 {
                match layer {
                    Some(l) => {
                        p.schedule.layers.get(l as usize).and_then(|b| reads[*b as usize].get(&(j as u16))).copied().unwrap_or(0)
                    }
                    None => occurrences.iter().map(|(b, _)| reads[*b as usize].get(&(j as u16)).copied().unwrap_or(0)).sum(),
                }
            };
            let per_forward_raw = inst.iter().map(|l| per_instance(*l)).max().unwrap_or(0);
            let tier = if sites.dense[j] {
                TirTierV1::Pinned(TirPinnedWhyV1::Dense)
            } else if !row_read[j] {
                TirTierV1::Pinned(TirPinnedWhyV1::Unread)
            } else if uneven[j] {
                TirTierV1::Pinned(TirPinnedWhyV1::UnevenRows)
            } else if instance_bytes < rules.pin_below_bytes {
                TirTierV1::Pinned(TirPinnedWhyV1::Small)
            } else if per_forward_raw >= rows as u64 {
                TirTierV1::Pinned(TirPinnedWhyV1::EveryRow)
            } else if routed[j] {
                TirTierV1::Routed
            } else {
                TirTierV1::Gathered
            };
            let rowed = tier.is_rows();
            params.push(TirParamTierV1 {
                tier,
                instance_bytes,
                instances: inst,
                rows: if rowed { rows } else { 0 },
                unit: if rowed { unit } else { 0 },
                per_forward: if rowed { per_forward_raw.min(rows as u64) } else { 0 },
            });
        }
        // Every admission a forward makes: per occurrence, per group, the routed members' rows.
        let widths: Vec<usize> = p.params.iter().map(|d| d.dtype.width()).collect();
        let mut admissions = Vec::new();
        for (b, _) in &occurrences {
            for g in &sites.groups[*b as usize] {
                let members: BTreeSet<u16> =
                    g.members.iter().map(|(_, j)| *j).filter(|j| params[*j as usize].tier == TirTierV1::Routed).collect();
                let bytes: u64 = members
                    .iter()
                    .map(|j| {
                        let t = &params[*j as usize];
                        g.index_elems.min(t.rows as u64).saturating_mul(t.unit_bytes(widths[*j as usize]))
                    })
                    .sum();
                if bytes > 0 {
                    admissions.push(bytes);
                }
            }
        }
        Self { rules, params, admissions, widths }
    }

    pub fn tier(&self, j: u16) -> Option<TirTierV1> {
        self.params.get(j as usize).map(|t| t.tier)
    }

    /// **The residency's arithmetic** (ADR-0112 Decisions 2, 3 and 7, for an IR class).
    pub fn arithmetic(&self) -> TirResidencyArithmeticV1 {
        let mut a = TirResidencyArithmeticV1::default();
        for (j, t) in self.params.iter().enumerate() {
            let n = t.instances.len() as u64;
            let bytes = t.instance_bytes.saturating_mul(n);
            a.weight_bytes = a.weight_bytes.saturating_add(bytes);
            let token = t.per_forward.saturating_mul(t.unit_bytes(self.widths[j])).saturating_mul(n);
            match t.tier {
                TirTierV1::Pinned(_) => {
                    a.pinned_bytes = a.pinned_bytes.saturating_add(bytes);
                    a.pinned_instances += n as u32;
                }
                TirTierV1::Routed => {
                    a.routed_bytes = a.routed_bytes.saturating_add(bytes);
                    a.routed_token_bytes = a.routed_token_bytes.saturating_add(token);
                    a.routed_instances += n as u32;
                }
                TirTierV1::Gathered => {
                    a.gathered_bytes = a.gathered_bytes.saturating_add(bytes);
                    a.gathered_token_bytes = a.gathered_token_bytes.saturating_add(token);
                    a.gathered_instances += n as u32;
                }
            }
        }
        a.in_flight_bytes = self.admissions.iter().copied().max().unwrap_or(0);
        a.floor_bytes = a.pinned_bytes.saturating_add(a.routed_token_bytes).saturating_add(a.in_flight_bytes);
        a.fifth_bytes = a.weight_bytes.div_ceil(TIR_RESIDENT_FRACTION_DENOMINATOR_V1);
        a
    }

    /// **The routed rows a run of `forwards` forward passes is expected to read once each** — per
    /// routed instance of `R` rows choosing `k` a forward, `R · (1 − (1 − k/R)^forwards)`, the
    /// expected union of independent uniform choices (ADR-0112 §1: "eight draws of 256, nine times").
    /// A cold cache reads each of them once; a warm one fewer.
    pub fn routed_union_bytes(&self, forwards: u64) -> u64 {
        let mut total = 0f64;
        for (j, t) in self.params.iter().enumerate() {
            if t.tier != TirTierV1::Routed || t.rows == 0 {
                continue;
            }
            let (r, k) = (t.rows as f64, t.per_forward as f64);
            let union = r * (1.0 - (1.0 - k / r).powf(forwards as f64));
            total += union * t.unit_bytes(self.widths[j]) as f64 * t.instances.len() as f64;
        }
        total.ceil() as u64
    }
}

/// **The numbers an operator sizes a host by** (ADR-0112 Decision 7), from the tiers alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirResidencyArithmeticV1 {
    /// Every instance's bytes: the weights on disk.
    pub weight_bytes: u64,
    /// Read once at open and held.
    pub pinned_bytes: u64,
    /// The routed stacks, of which a budget holds what is left after the pinned set.
    pub routed_bytes: u64,
    /// The gathered tables, never resident whole.
    pub gathered_bytes: u64,
    /// One forward's routed rows.
    pub routed_token_bytes: u64,
    /// One forward's gathered rows, read every forward.
    pub gathered_token_bytes: u64,
    /// The largest single admission — one route group's rows — held while a gather copies them.
    pub in_flight_bytes: u64,
    /// **The floor**: the pinned set, one token's routed rows and one admission in flight — the
    /// least budget a forward runs in without re-reading what it just read.
    pub floor_bytes: u64,
    /// A fifth of the weights, rounded up — the default budget's ratio.
    pub fifth_bytes: u64,
    pub pinned_instances: u32,
    pub routed_instances: u32,
    pub gathered_instances: u32,
}

impl TirResidencyArithmeticV1 {
    /// What a budget of `budget` bytes leaves for the routed rows a cache keeps: the budget less
    /// the pinned set and one admission in flight.
    pub fn routed_capacity(&self, budget: u64) -> u64 {
        budget.saturating_sub(self.pinned_bytes).saturating_sub(self.in_flight_bytes)
    }
}
