//! Disk-backed original39 trace archive. There is no all-position/node RAM map,
//! and a failed position remains provisional. Raw values retain original dtype
//! widths. Public replies and independent localization use the same tree bytes.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_detect39::{IndependentArchiveV39, PublicReadsV39},
    flat_public39::*,
    flat_source39::CheckedSourceV39,
    flat_trace39::{TraceSinkV39, TraceSummaryV39},
    hash::Digest,
    merkle3::{LayoutV3, LeafOpeningV3},
    seg::{self, NodeOpeningV1},
};
use misaka_palw_tir::{DType, Tensor};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(BorshSerialize, BorshDeserialize)]
struct NodeHeader {
    dtype: DType,
    rank: u8,
    shape: [u64; 4],
    commitment: Digest,
    row: Digest,
    col: Digest,
    raw: u64,
}
impl NodeHeader {
    fn shape(&self) -> Result<Vec<usize>, String> {
        if self.rank > 4 || self.shape[self.rank as usize..].iter().any(|n| *n != 0) {
            return Err("archive39 original rank/padding".into());
        }
        self.shape[..self.rank as usize].iter().map(|d| usize::try_from(*d).map_err(|_| "archive39 dimension extent".into())).collect()
    }
    fn public(&self) -> Result<ValueHeaderV39, String> {
        Ok(ValueHeaderV39 {
            node: None,
            dtype: self.dtype.tag(),
            shape: self.shape()?.iter().map(|d| *d as u64).collect(),
            row_root: self.row,
            col_root: self.col,
        })
    }
}
pub struct DiskTraceArchiveV39 {
    root: PathBuf,
    source: Digest,
    offsets: Vec<u64>,
    counts: Vec<u16>,
    maximum_value_elements: u64,
    budget: u64,
    bytes: u64,
    binding: Option<(Digest, Digest)>,
    completed: u32,
    summary: Option<Arc<TraceSummaryV39>>,
}
impl DiskTraceArchiveV39 {
    /// `path` must be fresh. The explicit budget is charged BEFORE every file;
    /// exhaustion leaves partial files and cannot produce a finished archive.
    pub fn create(path: &Path, source: &CheckedSourceV39, maximum_disk_bytes: u64) -> Result<Self, String> {
        if maximum_disk_bytes == 0 {
            return Err("archive39 original disk budget missing".into());
        }
        std::fs::create_dir(path).map_err(|e| e.to_string())?;
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        let mut offsets = Vec::new();
        let mut counts = Vec::new();
        let mut at = 0;
        for (b, _) in source.program().occurrences() {
            offsets.push(at);
            let n = source.program().blocks[b as usize].nodes.len();
            counts.push(u16::try_from(n).map_err(|_| "archive39 nodecount")?);
            at += n as u64;
        }
        Ok(Self {
            root,
            source: source.root(),
            offsets,
            counts,
            maximum_value_elements: source.profile().max_value_elements,
            budget: maximum_disk_bytes,
            bytes: 0,
            binding: None,
            completed: 0,
            summary: None,
        })
    }
    pub fn root_directory(&self) -> &Path {
        &self.root
    }
    pub fn stored_bytes(&self) -> u64 {
        self.bytes
    }
    fn charge(&mut self, n: u64) -> Result<(), String> {
        self.bytes =
            self.bytes.checked_add(n).filter(|n| *n <= self.budget).ok_or("archive39 original disk budget exhausted before write")?;
        Ok(())
    }
    fn slot(&self, s: u16, n: u16) -> Result<u64, String> {
        let count = *self.counts.get(s as usize).ok_or("archive39 occurrence")?;
        if n >= count {
            return Err("archive39 original node index".into());
        }
        Ok(self.offsets[s as usize] + n as u64)
    }
    fn address(&self, slot: u64) -> Result<(u16, u16), String> {
        for (s, (first, n)) in self.offsets.iter().zip(&self.counts).enumerate() {
            if slot >= *first && slot < *first + *n as u64 {
                return Ok((s as u16, (slot - *first) as u16));
            }
        }
        Err("archive39 original flat slot".into())
    }
    fn nodes(&self) -> u64 {
        self.offsets.last().zip(self.counts.last()).map_or(0, |(o, n)| *o + *n as u64)
    }
    fn position_dir(&self, p: u32, partial: bool) -> PathBuf {
        self.root.join(if partial { format!("p{p:08x}.partial") } else { format!("p{p:08x}") })
    }
    fn node_file(&self, p: u32, s: u16, n: u16, suffix: &str) -> Result<PathBuf, String> {
        Ok(self.position_dir(p, false).join(format!("n{:08x}.{suffix}", self.slot(s, n)?)))
    }
    fn header_at(&self, p: u32, s: u16, n: u16) -> Result<NodeHeader, String> {
        let path = self.node_file(p, s, n, "header39")?;
        let raw = crate::native_audit_io_v32::read_bounded_file_v32(&path, 512)?;
        let h = NodeHeader::try_from_slice(&raw).map_err(|_| "archive39 exact fixed node header")?;
        let shape = h.shape()?;
        let cells = shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("archive39 original cell extent")?;
        if cells == 0
            || cells > self.maximum_value_elements
            || cells.checked_mul(h.dtype.width() as u64) != Some(h.raw)
            || h.public()?.commitment()? != h.commitment
        {
            return Err("archive39 original typed node header differs".into());
        }
        Ok(h)
    }
    fn values(&self, p: u32, s: u16, n: u16, indices: &[u64]) -> Result<Vec<i128>, String> {
        if indices.len() > 4096 {
            return Err("archive39 leaf buffer bound".into());
        }
        let h = self.header_at(p, s, n)?;
        let mut file = std::fs::File::open(self.node_file(p, s, n, "raw39")?).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != h.raw {
            return Err("archive39 immutable original raw extent differs".into());
        }
        let width = h.dtype.width();
        let mut word = [0u8; 16];
        let mut out = Vec::with_capacity(indices.len());
        for index in indices {
            let at = index
                .checked_mul(width as u64)
                .filter(|n| n.checked_add(width as u64).is_some_and(|e| e <= h.raw))
                .ok_or("archive39 original value offset")?;
            file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
            file.read_exact(&mut word[..width]).map_err(|e| e.to_string())?;
            out.push(h.dtype.decode_le(&word[..width]));
        }
        Ok(out)
    }
    fn summary(&self) -> Result<&TraceSummaryV39, String> {
        self.summary.as_deref().ok_or("archive39 full trace is not finished".into())
    }
    pub fn finish(&mut self, summary: &TraceSummaryV39) -> Result<(), String> {
        let (maximum, inputs) = self.binding.ok_or("archive39 original binding missing")?;
        if self.summary.is_some()
            || summary.claim.source != self.source
            || summary.claim.maximum != maximum
            || summary.claim.input_binding != inputs
            || summary.claim.positions != self.completed
            || summary.position_roots.len() != self.completed as usize
            || summary.claim.segment_roots != seg::segment_roots_of_position_roots_v1(&summary.position_roots)
        {
            return Err("archive39 whole original trace/position summary differs".into());
        }
        for (p, root) in summary.position_roots.iter().enumerate() {
            let raw = crate::native_audit_io_v32::read_bounded_file_v32(&self.position_dir(p as u32, false).join("root39"), 64)?;
            if raw.as_slice() != root {
                return Err("archive39 promoted original position root differs".into());
            }
        }
        let raw = borsh::to_vec(summary).map_err(|e| e.to_string())?;
        self.charge(raw.len() as u64)?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join("summary39.borsh"))
            .map_err(|e| e.to_string())?;
        f.write_all(&raw).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        self.summary = Some(Arc::new(summary.clone()));
        Ok(())
    }
    fn hash_leaf(&self, d: &DirectoryV39, index: u64) -> Result<Digest, String> {
        match d {
            DirectoryV39::Positions { segment, .. } => {
                let summary = self.summary()?;
                let (first, end) = seg::segment_bounds_v1(summary.claim.positions, *segment).ok_or("archive39 segment")?;
                let p = first
                    .checked_add(u32::try_from(index).map_err(|_| "archive39 position index")?)
                    .filter(|p| *p < end)
                    .ok_or("archive39 position outside segment")?;
                Ok(position_directory_leaf_v39(p, &summary.position_roots[p as usize]))
            }
            DirectoryV39::Nodes { opening } => {
                let (s, n) = self.address(index)?;
                Ok(seg::node_leaf_v1(s, n, &self.header_at(opening.position, s, n)?.commitment))
            }
            DirectoryV39::Tensor { value: ValueKeyV39::Trace { position, occurrence, node }, axis, .. } => {
                if *axis > 1 {
                    return Err("archive39 axis".into());
                }
                let h = self.header_at(*position, *occurrence, *node)?;
                let layout = LayoutV3::try_of(&h.shape()?).ok_or("archive39 local tensor layout")?;
                let tiles = if *axis == 0 { layout.row_tiles } else { layout.col_tiles };
                let (line, tile) = (index / tiles, index % tiles);
                let indices = layout.leaf_elements(*axis, line, tile).ok_or("archive39 local leaf geometry")?;
                tensor_leaf_hash_v39(h.dtype, *axis, line, tile, &self.values(*position, *occurrence, *node, &indices)?)
            }
            _ => Err("archive39 parameter/input tensors require original source/material reader".into()),
        }
    }
    fn tree(&self, d: &DirectoryV39, first: u64, count: u64) -> Result<Digest, String> {
        if count == 0 || first.checked_add(count).is_none() {
            return Err("archive39 empty/overflow tree".into());
        }
        let mut levels = [None; 64];
        for i in first..first + count {
            let mut x = self.hash_leaf(d, i)?;
            let mut placed = false;
            for level in &mut levels {
                match level.take() {
                    None => {
                        *level = Some(x);
                        placed = true;
                        break;
                    }
                    Some(left) => x = directory_pair_v39(d, &left, &x),
                }
            }
            if !placed {
                return Err("archive39 tree frontier overflow".into());
            }
        }
        let mut root = None;
        for x in levels.into_iter().flatten() {
            root = Some(match root {
                None => x,
                Some(r) => directory_pair_v39(d, &x, &r),
            });
        }
        root.ok_or("archive39 empty frontier".into())
    }
    fn path(&self, d: &DirectoryV39, index: u64, total: u64) -> Result<Vec<Digest>, String> {
        if index >= total {
            return Err("archive39 path index".into());
        }
        let (mut first, mut count) = (0, total);
        let mut path = Vec::new();
        while count > 1 {
            let left = 1u64 << (63 - (count - 1).leading_zeros());
            if index < first + left {
                path.push(self.tree(d, first + left, count - left)?);
                count = left;
            } else {
                path.push(self.tree(d, first, left)?);
                first += left;
                count -= left;
            }
        }
        path.reverse();
        Ok(path)
    }
    pub fn node_opening(&self, p: u32, s: u16, n: u16) -> Result<NodeOpeningV1, String> {
        let summary = self.summary()?;
        let index = self.slot(s, n)?;
        let header = self.header_at(p, s, n)?;
        let mut opening = NodeOpeningV1 {
            position: p,
            occurrence: s,
            node: n,
            commitment: header.commitment,
            node_siblings: Vec::new(),
            position_root: *summary.position_roots.get(p as usize).ok_or("archive39 original position")?,
            position_siblings: Vec::new(),
        };
        opening.node_siblings = self.path(&DirectoryV39::Nodes { opening: opening.clone() }, index, self.nodes())?;
        let (first, end) = seg::segment_bounds_v1(summary.claim.positions, p / seg::SEG_LEN_V4).ok_or("archive39 original segment")?;
        opening.position_siblings = self.path(
            &DirectoryV39::Positions { segment: p / seg::SEG_LEN_V4, tree: [0; 64] },
            (p - first) as u64,
            (end - first) as u64,
        )?;
        if !opening.authenticates(&summary.claim.segment_roots, summary.claim.positions, index, self.nodes()) {
            return Err("archive39 immutable trace roots differ from original summary".into());
        }
        Ok(opening)
    }
    fn trace_header(&self, value: &ValueKeyV39) -> Result<ValueHeaderV39, String> {
        let ValueKeyV39::Trace { position, occurrence, node } = *value else {
            return Err("archive39 original parameter/input reader required".into());
        };
        let mut h = self.header_at(position, occurrence, node)?.public()?;
        h.node = Some(self.node_opening(position, occurrence, node)?);
        Ok(h)
    }
    fn leaf(&self, value: &ValueKeyV39, axis: u8, line: u64, tile: u64) -> Result<FlatReplyV39, String> {
        let ValueKeyV39::Trace { position, occurrence, node } = *value else {
            return Err("archive39 original parameter/input reader required".into());
        };
        let h = self.trace_header(value)?;
        let shape = h.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
        let layout = LayoutV3::try_of(&shape).ok_or("archive39 original leaf layout")?;
        if axis > 1 {
            return Err("archive39 axis".into());
        }
        let indices = layout.leaf_elements(axis, line, tile).ok_or("archive39 original requested leaf")?;
        let directory = DirectoryV39::Tensor { value: value.clone(), header: h.clone(), axis };
        let index = layout.leaf_index(axis, line, tile);
        Ok(FlatReplyV39::Leaf {
            node: h.node,
            leaf: LeafOpeningV3 {
                dtype: h.dtype,
                shape: h.shape,
                axis,
                line,
                tile,
                values: self.values(position, occurrence, node, &indices)?,
                siblings: self.path(&directory, index, layout.leaves(axis))?,
                other_root: if axis == 0 { h.col_root } else { h.row_root },
            },
        })
    }
    /// Trace-owned public acquisition. Parameter and input branches return None
    /// for composition with independently pinned source/material lazy readers.
    pub fn public_reply(&self, d: &FlatDemandV39) -> Result<Option<FlatReplyV39>, String> {
        let reply = match d {
            FlatDemandV39::SegmentTree { segment } => {
                let summary = self.summary()?;
                let (a, b) = seg::segment_bounds_v1(summary.claim.positions, *segment).ok_or("archive39 segment")?;
                FlatReplyV39::Root(self.tree(&DirectoryV39::Positions { segment: *segment, tree: [0; 64] }, 0, (b - a) as u64)?)
            }
            FlatDemandV39::Node { position, occurrence, node } => {
                FlatReplyV39::Node(self.node_opening(*position, *occurrence, *node)?)
            }
            FlatDemandV39::Header { value: ValueKeyV39::Trace { .. } } => {
                let FlatDemandV39::Header { value } = d else { unreachable!() };
                FlatReplyV39::Header(self.trace_header(value)?)
            }
            FlatDemandV39::Leaf { value: ValueKeyV39::Trace { .. }, .. }
            | FlatDemandV39::HeaderLeaf { value: ValueKeyV39::Trace { .. }, .. } => {
                let (value, axis, line, tile) = match d {
                    FlatDemandV39::Leaf { value, axis, line, tile } | FlatDemandV39::HeaderLeaf { value, axis, line, tile, .. } => {
                        (value, *axis, *line, *tile)
                    }
                    _ => unreachable!(),
                };
                self.leaf(value, axis, line, tile)?
            }
            FlatDemandV39::Branch {
                directory: DirectoryV39::Tensor { value: ValueKeyV39::Parameter { .. } | ValueKeyV39::Input { .. }, .. },
                ..
            } => return Ok(None),
            FlatDemandV39::Branch { directory, first, count, .. } => {
                if *count < 2 {
                    return Err("archive39 branch count".into());
                }
                let left = 1u64 << (63 - (*count - 1).leading_zeros());
                FlatReplyV39::Children {
                    left: self.tree(directory, *first, left)?,
                    right: self.tree(directory, *first + left, *count - left)?,
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(reply))
    }
}
impl TraceSinkV39 for DiskTraceArchiveV39 {
    fn node(&mut self, p: u32, s: u16, n: u16, value: &Tensor, c: Digest) -> Result<(), String> {
        let (row, col) =
            misaka_palw_kernel::flat_material39::tensor_tree_roots_streamed_v39(value.dtype, &value.shape, |offset, count| {
                value
                    .data
                    .get(offset as usize..offset as usize + count)
                    .map(|v| v.to_vec())
                    .ok_or("archive39 source tensor range".into())
            })?;
        self.node_with_trees(p, s, n, value, c, row, col)
    }
    fn node_with_trees(&mut self, p: u32, s: u16, n: u16, value: &Tensor, c: Digest, row: Digest, col: Digest) -> Result<(), String> {
        if self.binding.is_none()
            || self.summary.is_some()
            || p != self.completed
            || value.shape.len() > 4
            || value.data.len() as u64 > self.maximum_value_elements
        {
            return Err("archive39 active original position/value bound".into());
        }
        let slot = self.slot(s, n)?;
        let dir = self.position_dir(p, true);
        if slot == 0 {
            std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        }
        let mut shape = [0; 4];
        for (i, d) in value.shape.iter().enumerate() {
            shape[i] = *d as u64;
        }
        let raw = (value.data.len() as u64).checked_mul(value.dtype.width() as u64).ok_or("archive39 raw extent")?;
        let h = NodeHeader { dtype: value.dtype, rank: value.shape.len() as u8, shape, commitment: c, row, col, raw };
        if h.public()?.commitment()? != c {
            return Err("archive39 original node tree roots differ".into());
        }
        let header = borsh::to_vec(&h).map_err(|e| e.to_string())?;
        self.charge(raw.checked_add(header.len() as u64).ok_or("archive39 disk charge")?)?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("n{slot:08x}.raw39")))
            .map_err(|e| e.to_string())?;
        let mut buffer = Vec::with_capacity(65536);
        for value in &value.data {
            h.dtype.encode_le(*value, &mut buffer);
            if buffer.len() >= 65536 {
                f.write_all(&buffer).map_err(|e| e.to_string())?;
                buffer.clear();
            }
        }
        f.write_all(&buffer).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        let mut m = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("n{slot:08x}.header39")))
            .map_err(|e| e.to_string())?;
        m.write_all(&header).and_then(|_| m.sync_all()).map_err(|e| e.to_string())
    }
    fn position_finished(&mut self, p: u32, root: Digest, _: &Tensor) -> Result<(), String> {
        if p != self.completed || self.summary.is_some() {
            return Err("archive39 original position completion order".into());
        }
        self.charge(64)?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.position_dir(p, true).join("root39"))
            .map_err(|e| e.to_string())?;
        f.write_all(&root).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        std::fs::rename(self.position_dir(p, true), self.position_dir(p, false)).map_err(|e| e.to_string())?;
        self.completed = self.completed.checked_add(1).ok_or("archive39 position count")?;
        Ok(())
    }
}
impl IndependentArchiveV39 for DiskTraceArchiveV39 {
    fn begin(&mut self, source: Digest, maximum: Digest, inputs: Digest) -> Result<(), String> {
        if self.binding.is_some() || source != self.source || self.completed != 0 || [maximum, inputs].contains(&[0; 64]) {
            return Err("archive39 fresh independent original replay binding".into());
        }
        self.binding = Some((maximum, inputs));
        Ok(())
    }
    fn finish(&mut self, summary: &TraceSummaryV39) -> Result<(), String> {
        DiskTraceArchiveV39::finish(self, summary)
    }
    fn subtree(&self, directory: &DirectoryV39, first: u64, count: u64) -> Result<Digest, String> {
        self.tree(directory, first, count)
    }
    fn header(&self, p: u32, s: u16, n: u16) -> Result<ValueHeaderV39, String> {
        self.header_at(p, s, n)?.public()
    }
    fn value(&self, p: u32, s: u16, n: u16, e: u64) -> Result<i128, String> {
        self.values(p, s, n, &[e])?.first().copied().ok_or("archive39 original scalar missing".into())
    }
}
impl PublicReadsV39 for DiskTraceArchiveV39 {
    fn reply(&mut self, d: &FlatDemandV39) -> Result<Option<Vec<u8>>, String> {
        self.public_reply(d)?
            .map(|r| {
                let raw = r.to_bytes();
                if raw.len() > REPLY_MAX_BYTES_V39 {
                    return Err("archive39 original typed reply cap".into());
                }
                Ok(raw)
            })
            .transpose()
    }
}
