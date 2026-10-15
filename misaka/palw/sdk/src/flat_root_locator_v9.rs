//! Resumable public root localization from a complete independently replayed
//! commitment directory. No poster coordinate or prepared fault is accepted.
//! Each successful read authenticates to the immutable public segment roots.
//! Root localization is distinct from typed leaf/operand acquisition and courts;
//! its bound must not be advertised as their complete pursuit clock.
use misaka_palw_kernel::{
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    hash::Digest,
    seg::{NodeOpeningV1, SEG_LEN_V4, SegmentedCommitmentsV1},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootFindingV9 {
    Clean,
    Divergent(NodeOpeningV1),
    Demand(FlatReadV5),
    Inconsistent(String),
}
/// The bound includes the final public leaf acquisition. The same opening
/// authenticates the position and its initial node, so it is counted once.
pub fn root_reads_bound_v9(positions: u32, nodes_per_position: u64) -> Option<u32> {
    if positions == 0 || nodes_per_position == 0 {
        return None;
    }
    let log = |n: u64| if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() };
    Some(log(positions.min(SEG_LEN_V4) as u64) + log(nodes_per_position) + 1)
}
pub struct FlatRootLocatorV9<'a> {
    own: &'a SegmentedCommitmentsV1,
    public_roots: &'a [Digest],
    coordinates: Vec<(u16, u16)>,
    position: u32,
    position_end: u32,
    segment_first: u32,
    searching_position: bool,
    index: u64,
    node_end: u64,
    pending: Option<NodeOpeningV1>,
    clean: bool,
    successful_reads: u32,
    bound: u32,
    terminal: Option<NodeOpeningV1>,
}
/// Select the earliest mismatching right sibling inside the current subtree.
/// This grammar carries an odd tail upward: a path has no sibling at those
/// levels, so vector offset is deliberately distinct from tree height.
fn right_divergence(index: u64, count: u64, upper: u64, own: &[Digest], public: &[Digest]) -> Option<(u64, u64)> {
    let (mut i, mut width, mut at, mut height) = (index, count, 0, 0);
    while width > 1 {
        let sibling = i ^ 1;
        if sibling < width {
            if i & 1 == 0 && own.get(at)? != public.get(at)? {
                let lo = sibling.checked_shl(height)?;
                let hi = lo.checked_add(1u64.checked_shl(height)?)?.min(count);
                if lo < upper {
                    return Some((lo, hi.min(upper)));
                }
            }
            at += 1;
        }
        i >>= 1;
        width = width.div_ceil(2);
        height += 1;
    }
    None
}
impl<'a> FlatRootLocatorV9<'a> {
    pub fn new(own: &'a SegmentedCommitmentsV1, public_roots: &'a [Digest]) -> Result<Self, String> {
        let positions = own.positions();
        let first = own.commitments.first().ok_or("empty independent directory")?;
        if positions == 0
            || own.commitments.len() != positions as usize
            || own.position_roots.len() != positions as usize
            || own.commitments.iter().any(|p| p.len() != first.len() || p.iter().zip(first).any(|(a, b)| a.len() != b.len()))
        {
            return Err("incomplete/inconsistent independent directory geometry".into());
        }
        if own
            .commitments
            .iter()
            .enumerate()
            .any(|(q, p)| misaka_palw_kernel::seg::position_root_of_v1(q as u32, p) != own.position_roots[q])
        {
            return Err("owned position root does not bind the complete node directory".into());
        }
        let mut coordinates = vec![];
        for (s, nodes) in first.iter().enumerate() {
            for n in 0..nodes.len() {
                coordinates.push((u16::try_from(s).map_err(|_| "occurrence range")?, u16::try_from(n).map_err(|_| "node range")?));
            }
        }
        let bound = root_reads_bound_v9(positions, coordinates.len() as u64).ok_or("empty node directory")?;
        let roots = own.segment_roots();
        if public_roots.len() != roots.len() {
            return Err("public segment count differs from owned positions".into());
        }
        let segment = roots.iter().zip(public_roots).position(|(a, b)| a != b);
        let segment_first = segment.unwrap_or(0) as u32 * SEG_LEN_V4;
        Ok(Self {
            own,
            public_roots,
            node_end: coordinates.len() as u64,
            coordinates,
            position: segment_first,
            position_end: (segment_first + SEG_LEN_V4).min(positions),
            segment_first,
            searching_position: true,
            index: 0,
            pending: None,
            clean: segment.is_none(),
            successful_reads: 0,
            bound,
            terminal: None,
        })
    }
    pub fn successful_reads(&self) -> u32 {
        self.successful_reads
    }
    pub fn bound(&self) -> u32 {
        self.bound
    }
    pub fn advance(&mut self, public: &dyn FlatPublicReadsV5) -> RootFindingV9 {
        if self.clean {
            return RootFindingV9::Clean;
        }
        if let Some(o) = &self.terminal {
            return RootFindingV9::Divergent(o.clone());
        }
        loop {
            let (s, n) = self.coordinates[self.index as usize];
            let unit = FlatReadV5 { position: self.position, occurrence: s, node: n, leaf: None };
            let opening = match self.pending.take() {
                Some(o) => o,
                None => {
                    if self.successful_reads >= self.bound {
                        return RootFindingV9::Inconsistent("logarithmic root read bound exceeded".into());
                    }
                    let Some(o) = public.node(unit).filter(|o| {
                        (o.position, o.occurrence, o.node) == (unit.position, unit.occurrence, unit.node)
                            && o.authenticates(self.public_roots, self.own.positions(), self.index, self.coordinates.len() as u64)
                    }) else {
                        return RootFindingV9::Demand(unit);
                    };
                    self.successful_reads += 1;
                    o
                }
            };
            let Some(own) = self.own.node_opening(self.position, s, n) else {
                return RootFindingV9::Inconsistent("missing owned opening".into());
            };
            if self.searching_position {
                if opening.position_root != own.position_root {
                    self.searching_position = false;
                    self.pending = Some(opening);
                    continue;
                }
                let count = (self.own.positions() - self.segment_first).min(SEG_LEN_V4) as u64;
                let Some((lo, hi)) = right_divergence(
                    (self.position - self.segment_first) as u64,
                    count,
                    (self.position_end - self.segment_first) as u64,
                    &own.position_siblings,
                    &opening.position_siblings,
                ) else {
                    return RootFindingV9::Inconsistent("divergent segment has no divergent public position subtree".into());
                };
                self.position = self.segment_first + lo as u32;
                self.position_end = self.segment_first + hi as u32;
            } else {
                if opening.commitment != own.commitment {
                    self.terminal = Some(opening.clone());
                    return RootFindingV9::Divergent(opening);
                }
                let Some((lo, hi)) = right_divergence(
                    self.index,
                    self.coordinates.len() as u64,
                    self.node_end,
                    &own.node_siblings,
                    &opening.node_siblings,
                ) else {
                    return RootFindingV9::Inconsistent("divergent position has no divergent public node subtree".into());
                };
                self.index = lo;
                self.node_end = hi;
            }
        }
    }
}
