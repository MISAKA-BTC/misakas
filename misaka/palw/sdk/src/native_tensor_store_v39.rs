//! Reusable original39 tensor store. Raw values and every original C_v3 Merkle
//! node are written once. Replies read one tile plus O(log leaves) hashes;
//! opening a large parameter never rehashes the whole tensor per DA reply.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_public39::{ValueHeaderV39, tensor_leaf_hash_v39},
    hash::Digest,
    merkle3::{LayoutV3, LeafOpeningV3},
};
use misaka_palw_tir::DType;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct Header {
    version: u16,
    dtype: DType,
    rank: u8,
    shape: [u64; 4],
    commitment: Digest,
    row: Digest,
    col: Digest,
    raw: u64,
}
pub struct DiskTensorV39 {
    root: PathBuf,
    h: Header,
    shape: Vec<usize>,
    layout: LayoutV3,
    stored: u64,
}
fn pair(a: &Digest, b: &Digest) -> Digest {
    let mut h = misaka_palw_kernel::hash::keyed(misaka_palw_kernel::merkle3::TENSOR_NODE_DOMAIN_V3);
    h.update(a).update(b);
    misaka_palw_kernel::hash::finish(h)
}
fn counts(total: u64) -> Vec<u64> {
    let mut out = vec![total];
    while *out.last().unwrap() > 1 {
        out.push(out.last().unwrap().div_ceil(2));
    }
    out
}
fn offset(total: u64, level: usize) -> Result<u64, String> {
    let c = counts(total);
    if level >= c.len() {
        return Err("tensorstore39 level".into());
    }
    let mut at = total;
    for n in c.iter().take(level.saturating_sub(1)) {
        at = at.checked_add(n / 2).ok_or("tensorstore39 level offset")?;
    }
    if level == 0 { Ok(0) } else { at.checked_mul(64).ok_or("tensorstore39 level byte offset".into()) }
}
fn hash_at(file: &mut std::fs::File, total: u64, mut level: usize, mut index: u64) -> Result<Digest, String> {
    let c = counts(total);
    if level >= c.len() || index >= c[level] {
        return Err("tensorstore39 tree address".into());
    }
    while level > 0 && index >= c[level - 1] / 2 {
        index = index.checked_mul(2).ok_or("tensorstore39 promoted index")?;
        level -= 1;
    }
    let at = offset(total, level)?
        .checked_add(index.checked_mul(64).ok_or("tensorstore39 hash index")?)
        .ok_or("tensorstore39 hash offset")?;
    let mut hash = [0; 64];
    file.seek(SeekFrom::Start(at)).and_then(|_| file.read_exact(&mut hash)).map_err(|e| e.to_string())?;
    Ok(hash)
}
fn complete(file: &mut std::fs::File, total: u64) -> Result<Digest, String> {
    let c = counts(total);
    for level in 1..c.len() {
        for index in 0..c[level - 1] / 2 {
            let a = hash_at(file, total, level - 1, index * 2)?;
            let b = hash_at(file, total, level - 1, index * 2 + 1)?;
            let at = offset(total, level)?.checked_add(index * 64).ok_or("tensorstore39 complete offset")?;
            file.seek(SeekFrom::Start(at)).and_then(|_| file.write_all(&pair(&a, &b))).map_err(|e| e.to_string())?;
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    hash_at(file, total, c.len() - 1, 0)
}
impl DiskTensorV39 {
    pub fn create(
        path: &Path,
        dtype: DType,
        shape: &[usize],
        expected: Digest,
        maximum_elements: u64,
        maximum_disk_bytes: u64,
        mut read: impl FnMut(u64, usize) -> Result<Vec<i128>, String>,
    ) -> Result<Self, String> {
        let layout = LayoutV3::try_of(shape).ok_or("tensorstore39 exact original geometry")?;
        if shape.len() > 4 || layout.len == 0 || maximum_elements > 1 << 34 || layout.len > maximum_elements || expected == [0; 64] {
            return Err("tensorstore39 original profile/commitment".into());
        }
        let raw = layout.len.checked_mul(dtype.width() as u64).ok_or("tensorstore39 raw extent")?;
        let trees = [layout.leaves(0), layout.leaves(1)].into_iter().try_fold(0u64, |n, l| {
            l.checked_mul(2)
                .and_then(|n| n.checked_sub(1))
                .and_then(|n| n.checked_mul(64))
                .and_then(|b| n.checked_add(b))
                .ok_or("tensorstore39 complete tree extent")
        })?;
        let stored = raw
            .checked_add(trees)
            .and_then(|n| n.checked_add(512))
            .filter(|n| *n <= maximum_disk_bytes)
            .ok_or("tensorstore39 original disk budget before write")?;
        std::fs::create_dir(path).map_err(|e| e.to_string())?;
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(root.join("raw39")).map_err(|e| e.to_string())?;
        let mut at = 0;
        while at < layout.len {
            let count = (layout.len - at).min(4096) as usize;
            let values = read(at, count)?;
            if values.len() != count || values.iter().any(|v| !dtype.contains(*v)) {
                return Err("tensorstore39 exact original typed read".into());
            }
            let mut bytes = Vec::with_capacity(count * dtype.width());
            for v in values {
                dtype.encode_le(v, &mut bytes);
            }
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            at += count as u64;
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        let mut fixed = [0; 4];
        for (i, d) in shape.iter().enumerate() {
            fixed[i] = *d as u64;
        }
        let mut out = Self {
            root,
            h: Header {
                version: 39,
                dtype,
                rank: shape.len() as u8,
                shape: fixed,
                commitment: expected,
                row: [0; 64],
                col: [0; 64],
                raw,
            },
            shape: shape.to_vec(),
            layout,
            stored,
        };
        let mut roots = Vec::with_capacity(2);
        for axis in 0..=1 {
            let mut tree = std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .create_new(true)
                .open(out.tree_path(axis))
                .map_err(|e| e.to_string())?;
            let (lines, tiles) = if axis == 0 { (layout.rows, layout.row_tiles) } else { (layout.cols, layout.col_tiles) };
            for line in 0..lines {
                for tile in 0..tiles {
                    let indices = layout.leaf_elements(axis, line, tile).ok_or("tensorstore39 leaf geometry")?;
                    let values = out.read_indices(&indices)?;
                    tree.write_all(&tensor_leaf_hash_v39(dtype, axis, line, tile, &values)?).map_err(|e| e.to_string())?;
                }
            }
            roots.push(complete(&mut tree, layout.leaves(axis))?);
        }
        out.h.row = roots[0];
        out.h.col = roots[1];
        if out.header().commitment()? != expected {
            return Err("tensorstore39 immutable stored bytes differ from original C_v3".into());
        }
        let body = borsh::to_vec(&out.h).map_err(|e| e.to_string())?;
        let mut header =
            std::fs::OpenOptions::new().write(true).create_new(true).open(out.root.join("header39")).map_err(|e| e.to_string())?;
        header.write_all(&body).and_then(|_| header.sync_all()).map_err(|e| e.to_string())?;
        Ok(out)
    }
    pub fn open_existing(path: &Path, expected: Digest, maximum_elements: u64, maximum_disk_bytes: u64) -> Result<Self, String> {
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        let body = crate::native_audit_io_v32::read_bounded_file_v32(&root.join("header39"), 512)?;
        let h = Header::try_from_slice(&body).map_err(|_| "tensorstore39 exact fixed header")?;
        if h.version != 39 || h.rank > 4 || h.shape[h.rank as usize..].iter().any(|n| *n != 0) {
            return Err("tensorstore39 bounded original header".into());
        }
        let shape = h.shape[..h.rank as usize]
            .iter()
            .map(|d| usize::try_from(*d).map_err(|_| "tensorstore39 platform shape"))
            .collect::<Result<Vec<_>, _>>()?;
        let layout = LayoutV3::try_of(&shape).ok_or("tensorstore39 original geometry")?;
        if maximum_elements > 1 << 34
            || layout.len == 0
            || layout.len > maximum_elements
            || layout.len.checked_mul(h.dtype.width() as u64) != Some(h.raw)
            || h.commitment != expected
        {
            return Err("tensorstore39 original pinned tensor/profile".into());
        }
        let mut out = Self { root, h, shape, layout, stored: body.len() as u64 };
        if out.header().commitment()? != expected {
            return Err("tensorstore39 original pinned header C differs".into());
        }
        for name in ["raw39", "tree0.39", "tree1.39"] {
            let path = out.root.join(name);
            let real = path.canonicalize().map_err(|e| e.to_string())?;
            if !real.starts_with(&out.root) {
                return Err("tensorstore39 stored file escapes original root".into());
            }
            out.stored = out
                .stored
                .checked_add(std::fs::metadata(path).map_err(|e| e.to_string())?.len())
                .filter(|n| *n <= maximum_disk_bytes)
                .ok_or("tensorstore39 original reopened disk budget")?;
        }
        if std::fs::metadata(out.root.join("raw39")).map_err(|e| e.to_string())?.len() != out.h.raw {
            return Err("tensorstore39 exact immutable raw extent".into());
        }
        for axis in 0..=1 {
            let leaves = layout.leaves(axis);
            let mut tree = std::fs::File::open(out.tree_path(axis)).map_err(|e| e.to_string())?;
            if tree.metadata().map_err(|e| e.to_string())?.len() != (leaves * 2 - 1) * 64
                || hash_at(&mut tree, leaves, counts(leaves).len() - 1, 0)? != if axis == 0 { out.h.row } else { out.h.col }
            {
                return Err("tensorstore39 exact original tree extent/root".into());
            }
        }
        Ok(out)
    }
    fn tree_path(&self, axis: u8) -> PathBuf {
        self.root.join(format!("tree{axis}.39"))
    }
    pub fn root(&self) -> Digest {
        self.h.commitment
    }
    pub fn stored_bytes(&self) -> u64 {
        self.stored
    }
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    pub fn dtype(&self) -> DType {
        self.h.dtype
    }
    pub fn header(&self) -> ValueHeaderV39 {
        ValueHeaderV39 {
            node: None,
            dtype: self.h.dtype.tag(),
            shape: self.shape.iter().map(|d| *d as u64).collect(),
            row_root: self.h.row,
            col_root: self.h.col,
        }
    }
    pub fn read_indices(&self, indices: &[u64]) -> Result<Vec<i128>, String> {
        if indices.len() > 4096 {
            return Err("tensorstore39 bounded original tile".into());
        }
        let mut file = std::fs::File::open(self.root.join("raw39")).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != self.h.raw {
            return Err("tensorstore39 immutable byte extent changed".into());
        }
        let mut out = Vec::with_capacity(indices.len());
        let width = self.h.dtype.width();
        let mut word = [0; 16];
        for index in indices {
            let at = index
                .checked_mul(width as u64)
                .filter(|n| n.checked_add(width as u64).is_some_and(|e| e <= self.h.raw))
                .ok_or("tensorstore39 original read range")?;
            file.seek(SeekFrom::Start(at)).and_then(|_| file.read_exact(&mut word[..width])).map_err(|e| e.to_string())?;
            out.push(self.h.dtype.decode_le(&word[..width]));
        }
        Ok(out)
    }
    pub fn read_value_range(&self, offset: u64, count: usize) -> Result<Vec<i128>, String> {
        if count > 4096 || offset.checked_add(count as u64).is_none_or(|n| n > self.layout.len) {
            return Err("tensorstore39 original contiguous range cap".into());
        }
        self.read_indices(&(offset..offset + count as u64).collect::<Vec<_>>())
    }
    pub fn subtree(&self, axis: u8, first: u64, count: u64) -> Result<Digest, String> {
        if axis > 1 || count == 0 {
            return Err("tensorstore39 original subtree axis/count".into());
        }
        let total = self.layout.leaves(axis);
        let (mut start, mut size) = (0, total);
        while (start, size) != (first, count) {
            if size < 2 {
                return Err("tensorstore39 noncanonical subtree".into());
            }
            let left = 1u64 << (63 - (size - 1).leading_zeros());
            if first >= start + left {
                start += left;
                size -= left;
            } else {
                if first.checked_add(count).is_none_or(|n| n > start + left) {
                    return Err("tensorstore39 crossing subtree".into());
                }
                size = left;
            }
        }
        let level = if count <= 1 { 0 } else { (64 - (count - 1).leading_zeros()) as usize };
        let index = first >> level;
        hash_at(&mut std::fs::File::open(self.tree_path(axis)).map_err(|e| e.to_string())?, total, level, index)
    }
    pub fn leaf(&self, axis: u8, line: u64, tile: u64) -> Result<LeafOpeningV3, String> {
        if axis > 1 {
            return Err("tensorstore39 original axis".into());
        }
        let indices = self.layout.leaf_elements(axis, line, tile).ok_or("tensorstore39 original leaf geometry")?;
        let index = self.layout.leaf_index(axis, line, tile);
        let (mut first, mut count) = (0, self.layout.leaves(axis));
        let mut siblings = Vec::new();
        while count > 1 {
            let left = 1u64 << (63 - (count - 1).leading_zeros());
            if index < first + left {
                siblings.push(self.subtree(axis, first + left, count - left)?);
                count = left;
            } else {
                siblings.push(self.subtree(axis, first, left)?);
                first += left;
                count -= left;
            }
        }
        siblings.reverse();
        let leaf = LeafOpeningV3 {
            dtype: self.h.dtype.tag(),
            shape: self.shape.iter().map(|d| *d as u64).collect(),
            axis,
            line,
            tile,
            values: self.read_indices(&indices)?,
            siblings,
            other_root: if axis == 0 { self.h.col } else { self.h.row },
        };
        if !leaf.authenticates(&self.root()) {
            return Err("tensorstore39 original typed leaf/file hashes changed".into());
        }
        Ok(leaf)
    }
}

/// Fixed hash directory used by position/node trees as well. It stores exactly
/// 2*leaves-1 hashes; odd promoted children are addressed in the previous level
/// instead of allocating duplicate levels or a full in-memory directory.
pub struct DiskHashTreeV39 {
    path: PathBuf,
    leaves: u64,
    root: Digest,
    bytes: u64,
}
impl DiskHashTreeV39 {
    pub fn create(
        path: &Path,
        leaves: u64,
        maximum_bytes: u64,
        hashes: impl IntoIterator<Item = Result<Digest, String>>,
    ) -> Result<Self, String> {
        let bytes = leaves
            .checked_mul(2)
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| n.checked_mul(64))
            .filter(|n| *n <= maximum_bytes)
            .ok_or("hashstore39 original directory budget before write")?;
        if leaves == 0 {
            return Err("hashstore39 empty original tree".into());
        }
        let mut file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
        let mut count = 0;
        for hash in hashes {
            if count >= leaves {
                return Err("hashstore39 extra original leaves".into());
            }
            file.write_all(&hash?).map_err(|e| e.to_string())?;
            count += 1;
        }
        if count != leaves {
            return Err("hashstore39 missing original leaves".into());
        }
        // Tensor and trace trees have different node domains; callers use
        // create_with_pair for trace directories.
        let root = complete(&mut file, leaves)?;
        Ok(Self { path: path.to_path_buf(), leaves, root, bytes })
    }
    pub fn create_with_pair(
        path: &Path,
        leaves: u64,
        maximum_bytes: u64,
        hashes: impl IntoIterator<Item = Result<Digest, String>>,
        pair: impl Fn(&Digest, &Digest) -> Digest,
    ) -> Result<Self, String> {
        let bytes = leaves
            .checked_mul(2)
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| n.checked_mul(64))
            .filter(|n| *n <= maximum_bytes)
            .ok_or("hashstore39 original directory budget before write")?;
        if leaves == 0 {
            return Err("hashstore39 empty original tree".into());
        }
        let mut file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
        let mut seen = 0;
        for hash in hashes {
            if seen >= leaves {
                return Err("hashstore39 extra original leaves".into());
            }
            file.write_all(&hash?).map_err(|e| e.to_string())?;
            seen += 1;
        }
        if seen != leaves {
            return Err("hashstore39 missing original leaves".into());
        }
        let c = counts(leaves);
        for level in 1..c.len() {
            for index in 0..c[level - 1] / 2 {
                let a = hash_at(&mut file, leaves, level - 1, index * 2)?;
                let b = hash_at(&mut file, leaves, level - 1, index * 2 + 1)?;
                file.seek(SeekFrom::Start(offset(leaves, level)? + index * 64))
                    .and_then(|_| file.write_all(&pair(&a, &b)))
                    .map_err(|e| e.to_string())?;
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        let root = hash_at(&mut file, leaves, c.len() - 1, 0)?;
        Ok(Self { path: path.to_path_buf(), leaves, root, bytes })
    }
    pub fn open_existing(path: &Path, leaves: u64, expected: Digest, maximum_bytes: u64) -> Result<Self, String> {
        let bytes = leaves
            .checked_mul(2)
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| n.checked_mul(64))
            .filter(|n| *n <= maximum_bytes)
            .ok_or("hashstore39 bounded original directory")?;
        if leaves == 0 {
            return Err("hashstore39 empty original tree".into());
        }
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if f.metadata().map_err(|e| e.to_string())?.len() != bytes || hash_at(&mut f, leaves, counts(leaves).len() - 1, 0)? != expected
        {
            return Err("hashstore39 immutable original extent/root differs".into());
        }
        Ok(Self { path: path.to_path_buf(), leaves, root: expected, bytes })
    }
    pub fn root(&self) -> Digest {
        self.root
    }
    pub fn stored_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn subtree(&self, first: u64, count: u64) -> Result<Digest, String> {
        if count == 0 {
            return Err("hashstore39 empty subtree".into());
        }
        let (mut at, mut size) = (0, self.leaves);
        while (at, size) != (first, count) {
            if size < 2 {
                return Err("hashstore39 noncanonical subtree".into());
            }
            let left = 1u64 << (63 - (size - 1).leading_zeros());
            if first >= at + left {
                at += left;
                size -= left;
            } else {
                if first.checked_add(count).is_none_or(|n| n > at + left) {
                    return Err("hashstore39 crossing subtree".into());
                }
                size = left;
            }
        }
        let level = if count <= 1 { 0 } else { (64 - (count - 1).leading_zeros()) as usize };
        hash_at(&mut std::fs::File::open(&self.path).map_err(|e| e.to_string())?, self.leaves, level, first >> level)
    }
    pub fn path(&self, index: u64) -> Result<Vec<Digest>, String> {
        if index >= self.leaves {
            return Err("hashstore39 original leaf index".into());
        }
        let (mut at, mut count) = (0, self.leaves);
        let mut path = Vec::new();
        while count > 1 {
            let left = 1u64 << (63 - (count - 1).leading_zeros());
            if index < at + left {
                path.push(self.subtree(at + left, count - left)?);
                count = left;
            } else {
                path.push(self.subtree(at, left)?);
                at += left;
                count -= left;
            }
        }
        path.reverse();
        Ok(path)
    }
}
