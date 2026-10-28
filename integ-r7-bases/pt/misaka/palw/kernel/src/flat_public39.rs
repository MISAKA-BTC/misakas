//! Public39 acquisition grammar, separate from legacy25/32. Every response is
//! authenticated to original source/actual-input/accepted-claim roots before it
//! can satisfy a detector demand. Missing replies remain DA obligations.
use crate::{
    flat_point39::CheckedPointContextV39,
    hash::{Digest, finish, keyed, object_id},
    merkle3::{LayoutV3, LeafOpeningV3},
    seg::NodeOpeningV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::DType;
use std::io::Cursor;
pub const REQUEST_MAX_BYTES_V39: usize = 16 << 10;
pub const REPLY_MAX_BYTES_V39: usize = 128 << 10;
pub fn public_protocol_id_v39() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/public-acquisition-protocol/v39",b"rev1;original39-source/actual-input/acceptedclaim;segment-tree/node/typedheader/leaf4096/subtree-children/authenticated-header-leaf;canonical-largest-power2-split;request16KiB/reply128KiB;paths64;shape4;missing-is-DA-not-fault")
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ValueKeyV39 {
    Parameter { param: u16, layer: Option<u16> },
    Trace { position: u32, occurrence: u16, node: u16 },
    Input { position: u32, input: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ValueHeaderV39 {
    pub node: Option<NodeOpeningV1>,
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub row_root: Digest,
    pub col_root: Digest,
}
impl ValueHeaderV39 {
    pub fn commitment(&self) -> Result<Digest, String> {
        if self.shape.len() > 4 || !DType::ALL.iter().any(|d| d.tag() == self.dtype) {
            return Err("public39 bounded header grammar".into());
        }
        let mut h = keyed(crate::merkle3::TENSOR_COMMITMENT_DOMAIN_V3);
        h.update(&[self.dtype, self.shape.len() as u8]);
        for d in &self.shape {
            h.update(&d.to_le_bytes());
        }
        h.update(&self.row_root).update(&self.col_root);
        Ok(finish(h))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum DirectoryV39 {
    Positions { segment: u32, tree: Digest },
    Nodes { opening: NodeOpeningV1 },
    Tensor { value: ValueKeyV39, header: ValueHeaderV39, axis: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum FlatDemandV39 {
    SegmentTree { segment: u32 },
    Node { position: u32, occurrence: u16, node: u16 },
    Header { value: ValueKeyV39 },
    Leaf { value: ValueKeyV39, axis: u8, line: u64, tile: u64 },
    Branch { directory: DirectoryV39, first: u64, count: u64, root: Digest, path: Vec<Digest> },
    HeaderLeaf { value: ValueKeyV39, header: ValueHeaderV39, axis: u8, line: u64, tile: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum FlatReplyV39 {
    Root(Digest),
    Node(NodeOpeningV1),
    Header(ValueHeaderV39),
    Leaf { node: Option<NodeOpeningV1>, leaf: LeafOpeningV3 },
    Children { left: Digest, right: Digest },
}
fn get<T: BorshDeserialize>(r: &mut Cursor<&[u8]>) -> Result<T, String> {
    T::deserialize_reader(r).map_err(|_| "public39 truncated/unknown field".into())
}
fn count(r: &mut Cursor<&[u8]>, cap: usize, width: usize) -> Result<usize, String> {
    let n = get::<u32>(r)? as usize;
    if n > cap || n > r.get_ref().len().saturating_sub(r.position() as usize) / width {
        return Err("public39 vector bound before allocation".into());
    }
    Ok(n)
}
fn node(r: &mut Cursor<&[u8]>) -> Result<NodeOpeningV1, String> {
    crate::flat_court4::read_node(r).map_err(|_| "public39 bounded node path".into())
}
fn optional_node(r: &mut Cursor<&[u8]>) -> Result<Option<NodeOpeningV1>, String> {
    match get::<u8>(r)? {
        0 => Ok(None),
        1 => Ok(Some(node(r)?)),
        _ => Err("public39 optional node tag".into()),
    }
}
fn header(r: &mut Cursor<&[u8]>) -> Result<ValueHeaderV39, String> {
    let node = optional_node(r)?;
    let dtype = get(r)?;
    let n = count(r, 4, 8)?;
    let mut shape = Vec::with_capacity(n);
    for _ in 0..n {
        shape.push(get(r)?);
    }
    Ok(ValueHeaderV39 { node, dtype, shape, row_root: get(r)?, col_root: get(r)? })
}
fn directory(r: &mut Cursor<&[u8]>) -> Result<DirectoryV39, String> {
    match get::<u8>(r)? {
        0 => Ok(DirectoryV39::Positions { segment: get(r)?, tree: get(r)? }),
        1 => Ok(DirectoryV39::Nodes { opening: node(r)? }),
        2 => Ok(DirectoryV39::Tensor { value: get(r)?, header: header(r)?, axis: get(r)? }),
        _ => Err("public39 directory tag".into()),
    }
}
fn exact<T: BorshSerialize>(raw: &[u8], r: &Cursor<&[u8]>, value: &T) -> Result<(), String> {
    if r.position() != raw.len() as u64 || borsh::to_vec(value).map_err(|e| e.to_string())? != raw {
        return Err("public39 exact canonical body".into());
    }
    Ok(())
}
impl FlatDemandV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-public-demand/v39", &(public_protocol_id_v39(), self))
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("public39 Vec encoding")
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > REQUEST_MAX_BYTES_V39 {
            return Err("public39 request cap".into());
        }
        let mut r = Cursor::new(raw);
        let d = match get::<u8>(&mut r)? {
            0 => Self::SegmentTree { segment: get(&mut r)? },
            1 => Self::Node { position: get(&mut r)?, occurrence: get(&mut r)?, node: get(&mut r)? },
            2 => Self::Header { value: get(&mut r)? },
            3 => Self::Leaf { value: get(&mut r)?, axis: get(&mut r)?, line: get(&mut r)?, tile: get(&mut r)? },
            4 => {
                let directory = directory(&mut r)?;
                let first = get(&mut r)?;
                let count_value = get(&mut r)?;
                let root = get(&mut r)?;
                let n = count(&mut r, 64, 64)?;
                let mut path = Vec::with_capacity(n);
                for _ in 0..n {
                    path.push(get(&mut r)?);
                }
                Self::Branch { directory, first, count: count_value, root, path }
            }
            5 => Self::HeaderLeaf {
                value: get(&mut r)?,
                header: header(&mut r)?,
                axis: get(&mut r)?,
                line: get(&mut r)?,
                tile: get(&mut r)?,
            },
            _ => return Err("public39 demand tag".into()),
        };
        exact(raw, &r, &d)?;
        Ok(d)
    }
}
impl FlatReplyV39 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("public39 Vec encoding")
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > REPLY_MAX_BYTES_V39 {
            return Err("public39 reply cap".into());
        }
        let mut r = Cursor::new(raw);
        let d = match get::<u8>(&mut r)? {
            0 => Self::Root(get(&mut r)?),
            1 => Self::Node(node(&mut r)?),
            2 => Self::Header(header(&mut r)?),
            3 => {
                let node = optional_node(&mut r)?;
                let mut values = 0;
                Self::Leaf { node, leaf: crate::flat_point39::read_leaf39(&mut r, &mut values, raw.len())? }
            }
            4 => Self::Children { left: get(&mut r)?, right: get(&mut r)? },
            _ => return Err("public39 reply tag".into()),
        };
        exact(raw, &r, &d)?;
        Ok(d)
    }
}
fn pair(domain: &[u8], a: &Digest, b: &Digest) -> Digest {
    let mut h = keyed(domain);
    h.update(a).update(b);
    finish(h)
}
pub(crate) fn split(count: u64) -> Result<u64, String> {
    if count < 2 {
        return Err("public39 not a branch".into());
    }
    Ok(1u64 << (63 - (count - 1).leading_zeros()))
}
pub(crate) fn tree_pair(d: &DirectoryV39, a: &Digest, b: &Digest) -> Digest {
    match d {
        DirectoryV39::Positions { .. } => pair(crate::seg::SEG_SEG_NODE_DOMAIN_V1, a, b),
        DirectoryV39::Nodes { .. } => pair(crate::seg::SEG_POS_NODE_DOMAIN_V1, a, b),
        DirectoryV39::Tensor { .. } => crate::merkle3::node_hash_v3(a, b),
    }
}
fn node_check(c: &CheckedPointContextV39<'_>, o: &NodeOpeningV1, p: u32, s: u16, n: u16) -> Result<(), String> {
    if (o.position, o.occurrence, o.node) != (p, s, n)
        || !o.authenticates(&c.claim().segment_roots, c.claim().positions, c.prepared().index(s, n)?, c.prepared().node_count())
    {
        return Err("public39 node not original accepted claim".into());
    }
    Ok(())
}
pub(crate) fn value_geometry(c: &CheckedPointContextV39<'_>, key: &ValueKeyV39) -> Result<(DType, Vec<usize>), String> {
    let p = c.prepared();
    match *key {
        ValueKeyV39::Parameter { param, layer } => {
            let d = p.program().params.get(param as usize).ok_or("public39 parameter absent")?;
            if !p.source().parameters().by_instance.contains_key(&(param, layer)) {
                return Err("public39 real parameter instance absent".into());
            }
            Ok((d.dtype, d.shape.iter().map(|d| *d as usize).collect()))
        }
        ValueKeyV39::Trace { position, occurrence, node } => {
            Ok((p.node(occurrence, node)?.out.dtype, p.shapes(position, occurrence, node)?.out))
        }
        ValueKeyV39::Input { position, input } => {
            if position >= c.claim().positions {
                return Err("public39 input outside actual positions".into());
            }
            let d = p.program().inputs.get(input as usize).ok_or("public39 input absent")?;
            Ok((d.dtype, d.shape.iter().map(|d| *d as usize).collect()))
        }
    }
}
pub(crate) fn value_root(c: &CheckedPointContextV39<'_>, key: &ValueKeyV39, node: Option<&NodeOpeningV1>) -> Result<Digest, String> {
    value_geometry(c, key)?;
    match *key {
        ValueKeyV39::Trace { position, occurrence, node: n } => {
            let o = node.ok_or("public39 trace node proof absent")?;
            node_check(c, o, position, occurrence, n)?;
            Ok(o.commitment)
        }
        ValueKeyV39::Parameter { param, layer } => {
            if node.is_some() {
                return Err("public39 parameter cannot use trace namespace".into());
            }
            Ok(c.prepared().source().parameters().by_instance[&(param, layer)])
        }
        ValueKeyV39::Input { position, input } => {
            if node.is_some() {
                return Err("public39 input cannot use trace namespace".into());
            }
            c.inputs().input_root(position, input)
        }
    }
}
fn segment_tree(c: &CheckedPointContextV39<'_>, index: u32, tree: &Digest) -> Result<(), String> {
    let (first, end) = crate::seg::segment_bounds_v1(c.claim().positions, index).ok_or("public39 segment outside actual claim")?;
    let mut h = keyed(crate::seg::SEG_SEG_ROOT_DOMAIN_V1);
    h.update(&index.to_le_bytes()).update(&first.to_le_bytes()).update(&end.to_le_bytes()).update(tree);
    if finish(h) != c.claim().segment_roots[index as usize] {
        return Err("public39 segment directory root differs".into());
    }
    Ok(())
}
pub(crate) fn directory_root(c: &CheckedPointContextV39<'_>, d: &DirectoryV39) -> Result<(Digest, u64), String> {
    match d {
        DirectoryV39::Positions { segment, tree } => {
            segment_tree(c, *segment, tree)?;
            let (a, b) = crate::seg::segment_bounds_v1(c.claim().positions, *segment).unwrap();
            Ok((*tree, (b - a) as u64))
        }
        DirectoryV39::Nodes { opening: o } => {
            node_check(c, o, o.position, o.occurrence, o.node)?;
            let root = crate::merkle3::tree_root_from_path(
                crate::seg::node_leaf_v1(o.occurrence, o.node, &o.commitment),
                c.prepared().index(o.occurrence, o.node)?,
                c.prepared().node_count(),
                &o.node_siblings,
                |a, b| pair(crate::seg::SEG_POS_NODE_DOMAIN_V1, a, b),
            )
            .ok_or("public39 node directory path")?;
            Ok((root, c.prepared().node_count()))
        }
        DirectoryV39::Tensor { value, header: h, axis } => {
            let (dtype, shape) = value_geometry(c, value)?;
            if h.dtype != dtype.tag()
                || h.shape != shape.iter().map(|d| *d as u64).collect::<Vec<_>>()
                || h.commitment()? != value_root(c, value, h.node.as_ref())?
            {
                return Err("public39 original typed tensor directory differs".into());
            }
            let layout = LayoutV3::try_of(&shape).ok_or("public39 tensor layout")?;
            if *axis > 1 {
                return Err("public39 tensor axis".into());
            }
            Ok((if *axis == 0 { h.row_root } else { h.col_root }, layout.leaves(*axis)))
        }
    }
}
/// Verifies the complete canonical subtree address to the immutable tree root;
/// a caller cannot use an arbitrary opaque branch root to create liability.
fn subtree(
    c: &CheckedPointContextV39<'_>,
    directory: &DirectoryV39,
    first: u64,
    count: u64,
    root: Digest,
    path: &[Digest],
) -> Result<(), String> {
    let (expected, total) = directory_root(c, directory)?;
    if count == 0 || first.checked_add(count).is_none_or(|n| n > total) {
        return Err("public39 subtree outside original tree".into());
    }
    let (mut at, mut size) = (0, total);
    let mut turns = Vec::new();
    while (at, size) != (first, count) {
        let left = split(size)?;
        if first >= at + left {
            turns.push(false);
            at += left;
            size -= left;
        } else {
            if first.checked_add(count).is_none_or(|n| n > at + left) {
                return Err("public39 noncanonical cross-boundary subtree".into());
            }
            turns.push(true);
            size = left;
        }
    }
    if turns.len() != path.len() {
        return Err("public39 subtree exact path count".into());
    }
    let mut r = root;
    for (left, sibling) in turns.into_iter().rev().zip(path.iter().rev()) {
        r = if left { tree_pair(directory, &r, sibling) } else { tree_pair(directory, sibling, &r) };
    }
    if r != expected {
        return Err("public39 subtree not original directory".into());
    }
    Ok(())
}
pub fn check_flat_demand_v39(c: &CheckedPointContextV39<'_>, d: &FlatDemandV39) -> Result<(), String> {
    if d.to_bytes().len() > REQUEST_MAX_BYTES_V39 {
        return Err("public39 original request cap".into());
    }
    match d {
        FlatDemandV39::SegmentTree { segment } => {
            crate::seg::segment_bounds_v1(c.claim().positions, *segment).ok_or("public39 unknown segment")?;
        }
        FlatDemandV39::Node { position, occurrence, node } => {
            if *position >= c.claim().positions {
                return Err("public39 node position".into());
            }
            c.prepared().node(*occurrence, *node)?;
        }
        FlatDemandV39::Header { value } => {
            value_geometry(c, value)?;
        }
        FlatDemandV39::Leaf { value, axis, line, tile } => {
            let (_, shape) = value_geometry(c, value)?;
            if *axis > 1 || LayoutV3::try_of(&shape).and_then(|l| l.leaf_element_count(*axis, *line, *tile)).is_none() {
                return Err("public39 leaf outside original geometry".into());
            }
        }
        FlatDemandV39::Branch { directory, first, count, root, path } => {
            if *count < 2 {
                return Err("public39 leaf is not branch".into());
            }
            subtree(c, directory, *first, *count, *root, path)?;
        }
        FlatDemandV39::HeaderLeaf { value, header, axis, line, tile } => {
            if header.commitment()? != value_root(c, value, header.node.as_ref())? {
                return Err("public39 original malformed-header root differs".into());
            }
            let shape = header
                .shape
                .iter()
                .map(|d| usize::try_from(*d).map_err(|_| "public39 header extent"))
                .collect::<Result<Vec<_>, _>>()?;
            let layout = LayoutV3::try_of(&shape).ok_or("public39 malformed header has no bounded leaf geometry")?;
            if *axis > 1
                || layout.len > c.prepared().program().profile.max_parameter_elements
                || layout.leaf_element_count(*axis, *line, *tile).is_none()
            {
                return Err("public39 header leaf outside bounded actual geometry".into());
            }
        }
    }
    Ok(())
}
pub fn check_flat_reply_v39(c: &CheckedPointContextV39<'_>, d: &FlatDemandV39, r: &FlatReplyV39) -> Result<(), String> {
    check_flat_demand_v39(c, d)?;
    if r.to_bytes().len() > REPLY_MAX_BYTES_V39 {
        return Err("public39 original reply cap".into());
    }
    match (d, r) {
        (FlatDemandV39::SegmentTree { segment }, FlatReplyV39::Root(root)) => segment_tree(c, *segment, root),
        (FlatDemandV39::Node { position, occurrence, node }, FlatReplyV39::Node(o)) => node_check(c, o, *position, *occurrence, *node),
        (FlatDemandV39::Header { value }, FlatReplyV39::Header(h)) => {
            if h.commitment()? != value_root(c, value, h.node.as_ref())? {
                return Err("public39 authenticated header differs".into());
            }
            Ok(())
        }
        (FlatDemandV39::Leaf { value, axis, line, tile }, FlatReplyV39::Leaf { node, leaf }) => {
            let (dtype, shape) = value_geometry(c, value)?;
            if leaf.dtype() != Some(dtype)
                || leaf.shape_usize().as_deref() != Some(shape.as_slice())
                || leaf.axis != *axis
                || leaf.line != *line
                || leaf.tile != *tile
                || !leaf.authenticates(&value_root(c, value, node.as_ref())?)
            {
                return Err("public39 original typed leaf differs".into());
            }
            Ok(())
        }
        (FlatDemandV39::Branch { directory, root, .. }, FlatReplyV39::Children { left, right }) => {
            if tree_pair(directory, left, right) != *root {
                return Err("public39 branch not original subtree".into());
            }
            Ok(())
        }
        (FlatDemandV39::HeaderLeaf { value, header, axis, line, tile }, FlatReplyV39::Leaf { node, leaf }) => {
            if node != &header.node
                || leaf.dtype != header.dtype
                || leaf.shape != header.shape
                || leaf.axis != *axis
                || leaf.line != *line
                || leaf.tile != *tile
                || !leaf.authenticates(&value_root(c, value, node.as_ref())?)
            {
                return Err("public39 authentic malformed-header leaf differs".into());
            }
            Ok(())
        }
        _ => Err("public39 response does not satisfy original typed demand".into()),
    }
}

/// Bounded public producer helpers for the SAME original tree grammar.
pub fn directory_pair_v39(directory: &DirectoryV39, a: &Digest, b: &Digest) -> Digest {
    tree_pair(directory, a, b)
}
pub fn position_directory_leaf_v39(position: u32, root: &Digest) -> Digest {
    crate::seg::segment_leaf_v1(position, root)
}
pub fn tensor_leaf_hash_v39(dtype: DType, axis: u8, line: u64, tile: u64, values: &[i128]) -> Result<Digest, String> {
    if axis > 1 || values.is_empty() || values.len() > 4096 || values.iter().any(|v| !dtype.contains(*v)) {
        return Err("public39 bounded producer tensor leaf".into());
    }
    Ok(crate::merkle3::leaf_hash_v3(dtype, axis, line, tile, values))
}
