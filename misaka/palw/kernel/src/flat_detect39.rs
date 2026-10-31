//! Cold independent39 replay → authenticated first divergence → self-checked
//! filing. Public acquisition is logarithmic in trace/tensor directory size;
//! payload callbacks open only the one complete scalar relation. Missing data
//! returns the exact typed DA demand, never a false correctness verdict.
use crate::{
    element::{MissingV1, OperandsV1},
    flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39, RequestStopV39},
    flat_point39::*,
    flat_prepared39::OperandSourceV39,
    flat_public39::*,
    flat_quota11::owning39::CheckedQuotaV39,
    flat_source39::CheckedSourceV39,
    flat_trace39::{TraceSinkV39, TraceSummaryV39, replay_streamed_v39},
    hash::Digest,
    merkle3::{LayoutV3, LeafOpeningV3},
    seg::NodeOpeningV1,
};
use misaka_palw_tir::{Prim, interp_v39::ParamSourceV39, program_v39::NodePrimV39};
use std::{cell::RefCell, collections::BTreeMap};
/// Local independent replay archive, usually a disk/stream tree store. All tree
/// addresses are original39 positions/nodes; remote commitments are not local
/// execution authority. `subtree` uses the canonical split in flat_public39.
pub trait IndependentArchiveV39: TraceSinkV39 {
    fn begin(&mut self, source: Digest, maximum: Digest, inputs: Digest) -> Result<(), String>;
    fn finish(&mut self, _summary: &TraceSummaryV39) -> Result<(), String> {
        Ok(())
    }
    fn subtree(&self, directory: &DirectoryV39, first: u64, count: u64) -> Result<Digest, String>;
    fn header(&self, position: u32, occurrence: u16, node: u16) -> Result<ValueHeaderV39, String>;
    fn value(&self, position: u32, occurrence: u16, node: u16, element: u64) -> Result<i128, String>;
}
pub struct CheckedIndependentReplayV39<'a, A: IndependentArchiveV39> {
    archive: &'a A,
    summary: TraceSummaryV39,
}
impl<A: IndependentArchiveV39> CheckedIndependentReplayV39<'_, A> {
    pub fn summary(&self) -> &TraceSummaryV39 {
        &self.summary
    }
}
pub fn execute_independent_replay_v39<'a, A: IndependentArchiveV39>(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    inputs: &ActualInputResolverV39<'_>,
    params: &dyn ParamSourceV39,
    archive: &'a mut A,
) -> Result<CheckedIndependentReplayV39<'a, A>, String> {
    archive.begin(source.root(), maximum.root(), inputs.root())?;
    let summary = replay_streamed_v39(source, maximum, inputs, params, archive)?;
    archive.finish(&summary)?;
    Ok(CheckedIndependentReplayV39 { archive, summary })
}
pub trait PublicReadsV39 {
    fn reply(&mut self, demand: &FlatDemandV39) -> Result<Option<Vec<u8>>, String>;
}
#[derive(Clone, Debug)]
pub enum FindingV39 {
    /// Only arithmetic and firstEOS/greedy relations under this exact replay.
    /// This value grants no source fidelity, native Final or reward authority.
    Consistent {
        replayed_claim: Digest,
    },
    Need(FlatDemandV39),
    Fault {
        filing: Vec<u8>,
        conviction: FlatConvictionV39,
    },
}
enum Stop {
    Need(FlatDemandV39),
    Error(String),
}
impl From<String> for Stop {
    fn from(s: String) -> Self {
        Self::Error(s)
    }
}
struct Transport<'a, 'b> {
    context: &'a CheckedPointContextV39<'a>,
    public: &'b mut dyn PublicReadsV39,
    remaining: u32,
}
impl Transport<'_, '_> {
    #[allow(clippy::result_large_err)] // Stop::Need is the demand that was refused; boxing it would change every match
    fn ask(&mut self, d: FlatDemandV39) -> Result<FlatReplyV39, Stop> {
        check_flat_demand_v39(self.context, &d)?;
        self.remaining =
            self.remaining.checked_sub(1).ok_or_else(|| Stop::Error("detect39 original paid callback bound exhausted".into()))?;
        let Some(raw) = self.public.reply(&d).map_err(Stop::Error)? else {
            return Err(Stop::Need(d));
        };
        let Ok(reply) = FlatReplyV39::from_bytes(&raw) else {
            return Err(Stop::Need(d));
        };
        if check_flat_reply_v39(self.context, &d, &reply).is_err() {
            return Err(Stop::Need(d));
        }
        Ok(reply)
    }
}
#[allow(clippy::result_large_err)] // Stop::Need is the demand that was refused; boxing it would change every match
fn locate<A: IndependentArchiveV39>(a: &A, t: &mut Transport<'_, '_>, directory: DirectoryV39) -> Result<u64, Stop> {
    let (mut remote, mut count) = directory_root(t.context, &directory)?;
    let mut first = 0;
    let mut path = Vec::new();
    if a.subtree(&directory, first, count)? == remote {
        return Err(Stop::Error("detect39 directory has no independently replayed divergence".into()));
    }
    while count > 1 {
        let FlatReplyV39::Children { left, right } =
            t.ask(FlatDemandV39::Branch { directory: directory.clone(), first, count, root: remote, path: path.clone() })?
        else {
            return Err(Stop::Error("detect39 checked branch reply kind".into()));
        };
        let left_count = split(count)?;
        if a.subtree(&directory, first, left_count)? != left {
            path.push(right);
            remote = left;
            count = left_count;
        } else {
            path.push(left);
            remote = right;
            first += left_count;
            count -= left_count;
        }
    }
    Ok(first)
}
fn source_key(s: &OperandSourceV39) -> Option<ValueKeyV39> {
    match *s {
        OperandSourceV39::Trace { position, occurrence, node } => Some(ValueKeyV39::Trace { position, occurrence, node }),
        OperandSourceV39::Parameter { param, layer } => Some(ValueKeyV39::Parameter { param, layer }),
        OperandSourceV39::Input { position, input } => Some(ValueKeyV39::Input { position, input }),
        _ => None,
    }
}
fn choose_axis(prim: &NodePrimV39, operand: usize, rank: usize) -> u8 {
    match prim {
        NodePrimV39::Frozen(Prim::MatMul) => {
            if operand == 1 {
                1
            } else {
                0
            }
        }
        NodePrimV39::Frozen(Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. })
        | NodePrimV39::HistorySelect { axis, .. } => {
            if *axis as usize + 2 == rank {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}
struct Acquire<'a, 'b, 'c> {
    c: &'a CheckedPointContextV39<'a>,
    transport: RefCell<&'b mut Transport<'a, 'c>>,
    sources: Vec<OperandSourceV39>,
    proofs: RefCell<Vec<PointOperandV39>>,
    values: RefCell<Vec<BTreeMap<u64, i128>>>,
    axes: Vec<u8>,
    stop: RefCell<Option<Stop>>,
    expanded: RefCell<usize>,
    filing: RefCell<usize>,
}
impl OperandsV1 for Acquire<'_, '_, '_> {
    fn get(&self, operand: usize, element: u64) -> Result<i128, MissingV1> {
        let missing = MissingV1(operand, element);
        if self.stop.borrow().is_some() {
            return Err(missing);
        }
        let result = (|| -> Result<i128, Stop> {
            let src = self.sources.get(operand).ok_or_else(|| Stop::Error("detect39 original operand index".into()))?;
            if let Some(v) = self.values.borrow().get(operand).and_then(|m| m.get(&element)) {
                return Ok(*v);
            }
            let Some(key) = source_key(src) else {
                return match *src {
                    OperandSourceV39::Constant(j) => {
                        let c = &self.c.prepared().program().consts[j as usize];
                        let at = usize::try_from(element)
                            .ok()
                            .and_then(|n| n.checked_mul(c.dtype.width()))
                            .ok_or("detect39 constant offset".to_string())?;
                        let end = at.checked_add(c.dtype.width()).ok_or("detect39 constant extent".to_string())?;
                        Ok(c.dtype.decode_le(c.data.get(at..end).ok_or("detect39 constant value".to_string())?))
                    }
                    OperandSourceV39::InitialZero => Ok(0),
                    OperandSourceV39::Token(p) if element == 0 => {
                        Ok(*self.c.inputs().tokens().get(p as usize).ok_or("detect39 original token absent".to_string())? as i128)
                    }
                    OperandSourceV39::Position(p) if element == 0 => Ok(p as i128),
                    _ => Err(Stop::Error("detect39 public operand element".into())),
                };
            };
            let (_, shape) = value_geometry(self.c, &key)?;
            let layout = LayoutV3::try_of(&shape).ok_or("detect39 original operand layout".to_string())?;
            let axis = self.axes[operand];
            let (line, tile, _) = if axis == 0 { layout.row_leaf_of(element) } else { layout.col_leaf_of(element) }
                .ok_or("detect39 original scalar operand index".to_string())?;
            let FlatReplyV39::Leaf { node, leaf } =
                self.transport.borrow_mut().ask(FlatDemandV39::Leaf { value: key, axis, line, tile })?
            else {
                return Err(Stop::Error("detect39 checked operand leaf reply kind".into()));
            };
            let expanded =
                self.expanded.borrow().checked_add(leaf.values.len()).ok_or("detect39 expanded values overflow".to_string())?;
            let node_extra =
                if self.proofs.borrow()[operand].node.is_none() { node.as_ref().map_or(0, |n| n.byte_len() as usize) } else { 0 };
            let filing = self
                .filing
                .borrow()
                .checked_add(leaf.byte_len() as usize + node_extra + 16)
                .ok_or("detect39 filing allocation overflow".to_string())?;
            if expanded
                .checked_mul(128)
                .and_then(|n| n.checked_add(filing))
                .is_none_or(|n| n > self.c.resources().scope().maximum_scratch as usize)
                || filing > self.c.resources().scope().maximum_bytes as usize
            {
                return Err(Stop::Error("detect39 original court reservation exhausted before retaining leaf".into()));
            }
            *self.expanded.borrow_mut() = expanded;
            *self.filing.borrow_mut() = filing;
            let mut proofs = self.proofs.borrow_mut();
            let proof = &mut proofs[operand];
            if proof.node.is_some() && proof.node != node {
                return Err(Stop::Error("detect39 inconsistent original operand node".into()));
            }
            proof.node = node;
            let entries = leaf.elements().ok_or("detect39 typed leaf element layout".to_string())?;
            let mut values = self.values.borrow_mut();
            for (e, v) in entries {
                if values[operand].insert(e, v).is_some_and(|old| old != v) {
                    return Err(Stop::Error("detect39 inconsistent original operand values".into()));
                }
            }
            proof.leaves.push(leaf);
            values[operand].get(&element).copied().ok_or(Stop::Error("detect39 requested element missing from authentic leaf".into()))
        })();
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                *self.stop.borrow_mut() = Some(e);
                Err(missing)
            }
        }
    }
}
#[allow(clippy::result_large_err)] // Stop::Need is the demand that was refused; boxing it would change every match
fn filing(c: &CheckedPointContextV39<'_>, kind: u8, body: Vec<u8>) -> Result<FindingV39, Stop> {
    let mut out = Vec::with_capacity(body.len() + 7);
    out.extend_from_slice(&39u16.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    let conviction = check_flat_filing_v39(c, &out)?;
    Ok(FindingV39::Fault { filing: out, conviction })
}
#[allow(clippy::result_large_err)] // Stop::Need is the demand that was refused; boxing it would change every match
fn typed_leaf(
    t: &mut Transport<'_, '_>,
    key: ValueKeyV39,
    axis: u8,
    line: u64,
    tile: u64,
) -> Result<(Option<NodeOpeningV1>, LeafOpeningV3), Stop> {
    let FlatReplyV39::Leaf { node, leaf } = t.ask(FlatDemandV39::Leaf { value: key, axis, line, tile })? else {
        return Err(Stop::Error("detect39 checked typed reply kind".into()));
    };
    Ok((node, leaf))
}
#[allow(clippy::result_large_err)] // Stop::Need is the demand that was refused; boxing it would change every match
fn detect<'a, A: IndependentArchiveV39>(
    c: &'a CheckedPointContextV39<'a>,
    replay: &CheckedIndependentReplayV39<'_, A>,
    t: &mut Transport<'a, '_>,
) -> Result<FindingV39, Stop> {
    let own = &replay.summary.claim;
    let accepted = c.claim();
    if own.source != accepted.source
        || own.maximum != accepted.maximum
        || own.generated != accepted.generated
        || own.maximum_scope != accepted.maximum_scope
        || own.actual_scope != accepted.actual_scope
        || own.input_binding != accepted.input_binding
        || own.positions != accepted.positions
    {
        return Err(Stop::Error("detect39 independent replay does not bind original actual invocation".into()));
    }
    if let Some(segment) = own.segment_roots.iter().zip(&accepted.segment_roots).position(|(a, b)| a != b) {
        let FlatReplyV39::Root(tree) = t.ask(FlatDemandV39::SegmentTree { segment: segment as u32 })? else {
            return Err(Stop::Error("detect39 checked segment reply kind".into()));
        };
        let relative = locate(replay.archive, t, DirectoryV39::Positions { segment: segment as u32, tree })?;
        let (first, _) =
            crate::seg::segment_bounds_v1(accepted.positions, segment as u32).ok_or("detect39 original segment".to_string())?;
        let position = first + relative as u32;
        let FlatReplyV39::Node(anchor) = t.ask(FlatDemandV39::Node { position, occurrence: 0, node: 0 })? else {
            return Err(Stop::Error("detect39 checked node reply kind".into()));
        };
        let index = locate(replay.archive, t, DirectoryV39::Nodes { opening: anchor })?;
        let (mut occurrence, mut node, mut at) = (0u16, 0u16, 0u64);
        let mut found = false;
        for (s, (b, _)) in c.prepared().occurrences().iter().enumerate() {
            let count = c.prepared().program().blocks[*b as usize].nodes.len() as u64;
            if index < at + count {
                occurrence = s as u16;
                node = (index - at) as u16;
                found = true;
                break;
            }
            at += count;
        }
        if !found {
            return Err(Stop::Error("detect39 original flat node index".into()));
        }
        let key = ValueKeyV39::Trace { position, occurrence, node };
        let FlatReplyV39::Header(header) = t.ask(FlatDemandV39::Header { value: key.clone() })? else {
            return Err(Stop::Error("detect39 checked header reply kind".into()));
        };
        let (dtype, shape) = value_geometry(c, &key)?;
        if header.dtype != dtype.tag() || header.shape != shape.iter().map(|d| *d as u64).collect::<Vec<_>>() {
            let FlatReplyV39::Leaf { node: opening, leaf } =
                t.ask(FlatDemandV39::HeaderLeaf { value: key, header, axis: 0, line: 0, tile: 0 })?
            else {
                return Err(Stop::Error("detect39 checked header leaf reply".into()));
            };
            return filing(
                c,
                2,
                FlatHeaderFaultV39 { version: 39, output: opening.ok_or("detect39 original trace output opening".to_string())?, leaf }
                    .to_bytes(),
            );
        }
        let own_header = replay.archive.header(position, occurrence, node)?;
        let axis = if own_header.row_root != header.row_root {
            0
        } else if own_header.col_root != header.col_root {
            1
        } else {
            return Err(Stop::Error("detect39 node differs but both independently replayed typed trees agree".into()));
        };
        let leaf_index = locate(replay.archive, t, DirectoryV39::Tensor { value: key.clone(), header, axis })?;
        let layout = LayoutV3::try_of(&shape).ok_or("detect39 typed output layout".to_string())?;
        let tiles = if axis == 0 { layout.row_tiles } else { layout.col_tiles };
        let (opening, leaf) = typed_leaf(t, key, axis, leaf_index / tiles, leaf_index % tiles)?;
        let mut element = None;
        for (e, v) in leaf.elements().ok_or("detect39 typed output values".to_string())? {
            if replay.archive.value(position, occurrence, node, e)? != v {
                element = Some(e);
                break;
            }
        }
        let element = element.ok_or("detect39 divergent typed tree has no independently replayed value difference".to_string())?;
        let sources = c.prepared().operand_sources(position, occurrence, node)?;
        let sh = c.prepared().shapes(position, occurrence, node)?;
        let prim = &c.prepared().node(occurrence, node)?.prim;
        let axes = sh.inputs.iter().enumerate().map(|(i, shape)| choose_axis(prim, i, shape.len())).collect();
        let acquirer = Acquire {
            c,
            transport: RefCell::new(t),
            proofs: RefCell::new(sources.iter().map(|_| PointOperandV39 { node: None, leaves: Vec::new() }).collect()),
            values: RefCell::new(sources.iter().map(|_| BTreeMap::new()).collect()),
            sources,
            axes,
            stop: RefCell::new(None),
            expanded: RefCell::new(leaf.values.len()),
            filing: RefCell::new(leaf.byte_len() as usize + 8192),
        };
        let _ = c.prepared().scalar(position, occurrence, node, element, &acquirer)?;
        if let Some(stop) = acquirer.stop.into_inner() {
            return Err(stop);
        }
        return filing(
            c,
            0,
            FlatPointFaultV39 {
                version: 39,
                output: opening.ok_or("detect39 output node absent".to_string())?,
                element,
                output_leaf: leaf,
                operands: acquirer.proofs.into_inner(),
            }
            .to_bytes(),
        );
    }
    // Arithmetic can be identical while the original firstEOS decoder chose a
    // wrong token. Independently scan the actual logits, then file two leaves.
    if let RequestStopV39::Decode { prompt, .. } = c.inputs().stop() {
        let occurrence = (c.prepared().occurrences().len() - 1) as u16;
        let node = c.prepared().program().output.node();
        for (i, selected) in c.inputs().generated().iter().copied().enumerate() {
            let position = (prompt.len() - 1 + i) as u32;
            let score = replay.archive.value(position, occurrence, node, selected as u64)?;
            let mut rival = None;
            for id in 0..c.prepared().program().token_bound {
                let value = replay.archive.value(position, occurrence, node, id as u64)?;
                if value > score || value == score && id < selected {
                    rival = Some(id);
                    break;
                }
            }
            if let Some(alternative) = rival {
                let key = ValueKeyV39::Trace { position, occurrence, node };
                let (_, shape) = value_geometry(c, &key)?;
                let layout = LayoutV3::try_of(&shape).ok_or("detect39 logits layout".to_string())?;
                let (a, b, _) = layout.row_leaf_of(selected as u64).ok_or("detect39 selected token".to_string())?;
                let (opening, chosen) = typed_leaf(t, key.clone(), 0, a, b)?;
                let (a, b, _) = layout.row_leaf_of(alternative as u64).ok_or("detect39 rival token".to_string())?;
                let (_, better) = typed_leaf(t, key, 0, a, b)?;
                return filing(
                    c,
                    1,
                    FlatGreedyFaultV39 {
                        version: 39,
                        generation_index: i as u32,
                        output: opening.ok_or("detect39 logits node absent".to_string())?,
                        alternative,
                        chosen,
                        better,
                    }
                    .to_bytes(),
                );
            }
        }
    }
    Ok(FindingV39::Consistent { replayed_claim: own.root() })
}
/// The native before-Fund checked quota fixes the total acquisition bound.
/// The result is a bounded, self-verified filing or an exact DA demand.
pub fn detect_first_fault_v39<'a, A: IndependentArchiveV39>(
    context: &'a CheckedPointContextV39<'a>,
    quota: &CheckedQuotaV39,
    replay: &CheckedIndependentReplayV39<'_, A>,
    public: &mut dyn PublicReadsV39,
) -> Result<FindingV39, String> {
    if quota.maximum() != context.claim().maximum || quota.source().source != context.prepared().source().root() {
        return Err("detect39 original checked quota/source/maximum differs".into());
    }
    let mut t = Transport { context, public, remaining: quota.source().requests_per_owner };
    match detect(context, replay, &mut t) {
        Ok(v) => Ok(v),
        Err(Stop::Need(d)) => Ok(FindingV39::Need(d)),
        Err(Stop::Error(e)) => Err(e),
    }
}
