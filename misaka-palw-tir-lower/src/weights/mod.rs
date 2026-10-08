//! **Checkpoint tensors → HL params.**
//!
//! A deliberately small `safetensors` reader (single file, or sharded through
//! `model.safetensors.index.json`; BF16/F16/F32/F64 widened to f32), and the evaluator of the
//! [`Src`] expressions the HL builder attaches to every param: slicing fused tensors, transposing
//! GPT-2 `Conv1D`, stacking experts or per-head norms, `−exp(A_log)`, RWKV's per-layer rescale.
//! Every param is shape-checked against its declaration, and every checkpoint tensor the program
//! never reads is reported — an unread tensor is a feature this lowerer might be missing.

use crate::error::{LowerError, Result};
use crate::hl::HlProgram;
use crate::prequant::{QFormat, QLayout, QWeight};
use crate::quantfmt::tensors::RoleTensor;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod described;
pub mod expr;
mod remote;
pub use remote::{MemoryFetcher, RangeFetcher, RemoteCheckpoint};
#[cfg(feature = "remote")]
pub use remote::CurlFetcher;
pub mod stream;
pub mod torchzip;

/// A dense row-major f32 tensor.
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}

impl Tensor {
    pub fn new(shape: Vec<usize>, data: Vec<f32>) -> Self {
        debug_assert_eq!(shape.iter().product::<usize>(), data.len());
        Tensor { shape, data }
    }
    pub fn numel(&self) -> usize {
        self.data.len()
    }
}

/// What a source says about a tensor **without reading its data** (a header's entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorMeta {
    /// The stored element type as its format names it (`BF16`, `F32`, `I32`, `Q4_K`, …).
    pub dtype: String,
    /// The logical shape, Hugging Face order.
    pub shape: Vec<usize>,
    /// Bytes the tensor's data occupies as stored (`0` when the format's type is not known here:
    /// the size of an unknown block type is not guessed).
    pub bytes: u64,
}

impl TensorMeta {
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }
    /// `(rows, cols)`: every axis but the last flattened into rows (a rank-1 tensor is one row).
    pub fn rows_cols(&self) -> (usize, usize) {
        match self.shape.split_last() {
            Some((c, lead)) => (lead.iter().product(), *c),
            None => (1, 1),
        }
    }
    /// Stored bytes of one row (`bytes / rows`).
    pub fn row_bytes(&self) -> u64 {
        self.bytes / self.rows_cols().0.max(1) as u64
    }
}

/// Where checkpoint tensors come from.
///
/// The first four methods read DECODED values (`f32`, or a quantised weight's stored integers).
/// The last three are the **streaming loader's** face: [`metadata`](Self::metadata) is a header
/// lookup (no data), [`read_slice`](Self::read_slice) reads a byte range of a tensor's stored data
/// (a `pread` of a shard file, a range fetch of a remote one), and
/// [`load_rows`](Self::load_rows) decodes just a row range — so a conversion holds a block of a
/// tensor, never the whole of it. A source that cannot serve ranges keeps the defaults, which are
/// correct and read whole tensors.
pub trait TensorSource {
    fn shape(&self, name: &str) -> Option<Vec<usize>>;
    fn load(&self, name: &str) -> Result<Tensor>;
    fn names(&self) -> Vec<String>;
    /// An `I32` tensor as stored (a pre-quantised checkpoint's packed words, `g_idx`).
    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        Err(LowerError::weights(format!("`{name}`: this source holds no integer tensors")))
    }
    /// A block-quantised weight's stored integers (a GGUF tensor, `crate::gguf`).
    fn load_qweight(&self, name: &str) -> Result<QWeight> {
        Err(LowerError::weights(format!("`{name}`: this source holds no block-quantised tensors")))
    }
    /// The tensor's header entry. The default describes a source that serves decoded `f32`.
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.shape(name).map(|shape| {
            let bytes = shape.iter().product::<usize>() as u64 * 4;
            TensorMeta { dtype: "F32".into(), shape, bytes }
        })
    }
    /// Bytes `range` of the tensor's stored data (offsets within the tensor).
    fn read_slice(&self, name: &str, _range: Range<u64>) -> Result<Vec<u8>> {
        Err(LowerError::weights(format!("`{name}`: this source cannot serve byte ranges")))
    }
    /// Rows `rows` of the tensor as `f32`, a `[rows.len(), cols]` tensor (every axis but the last
    /// flattened into rows; see [`TensorMeta::rows_cols`]). The default loads the whole tensor.
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let t = self.load(name)?;
        slice_rows(&t, rows)
    }
    /// Whether [`load_rows`](Self::load_rows) reads only the rows asked for (false: it loads the
    /// whole tensor, so a caller streaming in blocks gains nothing).
    fn serves_row_ranges(&self) -> bool {
        false
    }
}

/// Rows `rows` of `t` (every axis but the last flattened), as a `[rows.len(), cols]` tensor.
pub fn slice_rows(t: &Tensor, rows: Range<usize>) -> Result<Tensor> {
    let cols = t.shape.last().copied().unwrap_or(1);
    let n = if cols == 0 { 0 } else { t.data.len() / cols };
    if rows.start > rows.end || rows.end > n {
        return Err(LowerError::weights(format!("rows {rows:?} of a tensor of {n} rows (shape {:?})", t.shape)));
    }
    Ok(Tensor::new(vec![rows.len(), cols], t.data[rows.start * cols..rows.end * cols].to_vec()))
}

/// Two sources read as one: `over` (an adapter's tensors) before `base` (the parent checkpoint).
/// Both are `Sync`, so an overlay streams to the float reference's parallel loader.
pub struct Overlay<'a> {
    pub base: &'a (dyn TensorSource + Sync),
    pub over: &'a (dyn TensorSource + Sync),
}

impl TensorSource for Overlay<'_> {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.over.shape(name).or_else(|| self.base.shape(name))
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        if self.over.shape(name).is_some() { self.over.load(name) } else { self.base.load(name) }
    }
    fn names(&self) -> Vec<String> {
        let mut n = self.base.names();
        n.extend(self.over.names());
        n.sort();
        n.dedup();
        n
    }
    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        if self.over.shape(name).is_some() { self.over.load_i32(name) } else { self.base.load_i32(name) }
    }
    fn load_qweight(&self, name: &str) -> Result<QWeight> {
        if self.over.shape(name).is_some() { self.over.load_qweight(name) } else { self.base.load_qweight(name) }
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.over.metadata(name).or_else(|| self.base.metadata(name))
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        if self.over.shape(name).is_some() { self.over.read_slice(name, range) } else { self.base.read_slice(name, range) }
    }
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        if self.over.shape(name).is_some() { self.over.load_rows(name, rows) } else { self.base.load_rows(name, rows) }
    }
    fn serves_row_ranges(&self) -> bool {
        self.over.serves_row_ranges() && self.base.serves_row_ranges()
    }
}

/// An in-memory source (tests, synthetic checkpoints).
#[derive(Default)]
pub struct MapSource(pub BTreeMap<String, Tensor>);

impl TensorSource for MapSource {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.0.get(name).map(|t| t.shape.clone())
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        self.0.get(name).cloned().ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))
    }
    fn names(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub dtype: String,
    pub shape: Vec<usize>,
    pub begin: u64,
    pub end: u64,
}

/// One `.safetensors` file: the header is parsed eagerly, tensor bytes are read on demand — by
/// `pread` of just the range asked for, on one open handle.
#[derive(Clone, Debug)]
pub struct SafetensorsFile {
    pub path: PathBuf,
    pub entries: BTreeMap<String, Entry>,
    pub data_offset: u64,
    file: Arc<std::fs::File>,
}

/// `pread`: fill `buf` from `offset` of `file`, without moving a cursor (so many readers share one
/// handle).
pub(crate) fn read_exact_at(file: &std::fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < buf.len() {
            let n = file.seek_read(&mut buf[done..], offset + done as u64)?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            done += n;
        }
        Ok(())
    }
}

/// Widen stored floats (`BF16`/`F16`/`F32`/`F64`, little-endian) to `f32`.
pub fn widen_floats(dtype: &str, raw: &[u8]) -> Result<Vec<f32>> {
    let sz = dtype_size(dtype).ok_or_else(|| LowerError::weights(format!("a {dtype} tensor is not a float type this reader widens")))?;
    if !raw.len().is_multiple_of(sz) {
        return Err(LowerError::weights(format!("{} bytes is not a whole number of {dtype} elements", raw.len())));
    }
    Ok(match dtype {
        // BF16 is the top half of an f32: widening is a shift.
        "BF16" => raw.chunks_exact(sz).map(|c| f32::from_bits((u16::from_le_bytes([c[0], c[1]]) as u32) << 16)).collect(),
        "F16" => raw.chunks_exact(sz).map(|c| f16_to_f32(u16::from_le_bytes([c[0], c[1]]))).collect(),
        "F32" => raw.chunks_exact(sz).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
        "F64" => raw.chunks_exact(sz).map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8])) as f32).collect(),
        // An integer table (DeepSeek-V4's frozen `tid2eid`): read as the exact float of each value; one past 2^24 would not be exact.
        "I64" | "I32" | "I16" | "I8" | "U8" => {
            let mut out = Vec::with_capacity(raw.len() / sz);
            for c in raw.chunks_exact(sz) {
                let v: i64 = match dtype {
                    "I64" => i64::from_le_bytes(c.try_into().unwrap_or([0; 8])),
                    "I32" => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as i64,
                    "I16" => i16::from_le_bytes([c[0], c[1]]) as i64,
                    "I8" => c[0] as i8 as i64,
                    _ => c[0] as i64,
                };
                if v.unsigned_abs() > 1 << 24 {
                    return Err(LowerError::weights(format!("a {dtype} value {v} is not exactly a float32")));
                }
                out.push(v as f32);
            }
            out
        }
        other => return Err(LowerError::weights(format!("dtype {other}"))),
    })
}

fn dtype_size(d: &str) -> Option<usize> {
    Some(match d {
        "BF16" | "F16" => 2,
        "F32" | "I32" => 4,
        "F64" | "I64" => 8,
        "I16" => 2,
        "I8" | "U8" => 1,
        _ => return None,
    })
}

/// Element size of an integer dtype (validated in the header; read by [`SafetensorsFile::read_i32`]).
fn int_size(d: &str) -> Option<usize> {
    Some(match d {
        "I8" | "U8" | "BOOL" => 1,
        "I16" | "U16" => 2,
        "I32" | "U32" => 4,
        "I64" | "U64" => 8,
        _ => return None,
    })
}

/// IEEE half → f32, exact.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if frac == 0 {
            sign << 31
        } else {
            // Subnormal: renormalise.
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((f & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        (sign << 31) | (0xff << 23) | (frac << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13)
    };
    f32::from_bits(bits)
}

/// Parse a safetensors header from its first bytes (8-byte length + JSON). Total: a malformed
/// file is an error, never a panic.
pub fn parse_header(bytes: &[u8], file_len: u64) -> Result<(BTreeMap<String, Entry>, u64)> {
    if bytes.len() < 8 {
        return Err(LowerError::weights("safetensors: shorter than the 8-byte header length"));
    }
    let n = u64::from_le_bytes(bytes[..8].try_into().map_err(|_| LowerError::weights("header length"))?);
    let end = 8u64.checked_add(n).ok_or_else(|| LowerError::weights("safetensors: header length overflows"))?;
    if end > file_len || end as usize > bytes.len() {
        return Err(LowerError::weights("safetensors: header runs past the end of the file"));
    }
    let json: serde_json::Value = serde_json::from_slice(&bytes[8..end as usize])
        .map_err(|e| LowerError::weights(format!("safetensors: header is not JSON: {e}")))?;
    let map = json.as_object().ok_or_else(|| LowerError::weights("safetensors: header is not an object"))?;
    let mut out = BTreeMap::new();
    let data_len = file_len - end;
    for (name, spec) in map {
        if name == "__metadata__" {
            continue;
        }
        let dtype =
            spec.get("dtype").and_then(|v| v.as_str()).ok_or_else(|| LowerError::weights(format!("`{name}`: no dtype")))?.to_string();
        let shape: Vec<usize> = spec
            .get("shape")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LowerError::weights(format!("`{name}`: no shape")))?
            .iter()
            .map(|v| v.as_u64().map(|u| u as usize).ok_or_else(|| LowerError::weights(format!("`{name}`: bad shape"))))
            .collect::<Result<_>>()?;
        let off = spec
            .get("data_offsets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LowerError::weights(format!("`{name}`: no data_offsets")))?;
        if off.len() != 2 {
            return Err(LowerError::weights(format!("`{name}`: data_offsets is not a pair")));
        }
        let (b, e) = (off[0].as_u64().unwrap_or(u64::MAX), off[1].as_u64().unwrap_or(0));
        if e < b || e > data_len {
            return Err(LowerError::weights(format!("`{name}`: data_offsets out of range")));
        }
        if let Some(sz) = dtype_size(&dtype).or_else(|| int_size(&dtype)) {
            let want = shape
                .iter()
                .try_fold(sz as u64, |a, d| a.checked_mul(*d as u64))
                .ok_or_else(|| LowerError::weights("size overflow"))?;
            if want != e - b {
                return Err(LowerError::weights(format!("`{name}`: {dtype}{shape:?} needs {want} bytes, has {}", e - b)));
            }
        }
        out.insert(name.clone(), Entry { dtype, shape, begin: b, end: e });
    }
    Ok((out, end))
}

impl SafetensorsFile {
    /// A PyTorch checkpoint (`.bin`, `.pt`, `.pth`) read as the same thing: its tensors' absolute offsets are the entries' (the data
    /// region begins at 0). The pickle that names them is *interpreted*, never run ([`torchzip`]); a file it refuses is a
    /// `FORMAT_UNSUPPORTED` error.
    pub fn open_torch(path: &Path) -> Result<Self> {
        let h = torchzip::read_header(path)?;
        let f = std::fs::File::open(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
        let entries = h
            .entries
            .into_iter()
            .map(|(n, e)| (n, Entry { dtype: e.dtype.to_string(), shape: e.shape, begin: e.begin, end: e.begin + e.bytes }))
            .collect();
        Ok(SafetensorsFile { path: path.to_path_buf(), entries, data_offset: 0, file: Arc::new(f) })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if torchzip::is_torch_path(path) {
            return Self::open_torch(path);
        }
        let mut f = std::fs::File::open(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
        let len = f.metadata().map_err(|e| LowerError::Io(e.to_string()))?.len();
        let mut head = [0u8; 8];
        f.read_exact(&mut head).map_err(|e| LowerError::weights(format!("{}: {e}", path.display())))?;
        let n = u64::from_le_bytes(head);
        if n > 100 * 1024 * 1024 || 8 + n > len {
            return Err(LowerError::weights(format!("{}: implausible header length {n}", path.display())));
        }
        let mut buf = vec![0u8; 8 + n as usize];
        buf[..8].copy_from_slice(&head);
        f.read_exact(&mut buf[8..]).map_err(|e| LowerError::weights(e.to_string()))?;
        let (entries, data_offset) = parse_header(&buf, len)?;
        Ok(SafetensorsFile { path: path.to_path_buf(), entries, data_offset, file: Arc::new(f) })
    }

    fn entry(&self, name: &str) -> Result<&Entry> {
        self.entries.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}` in {}", self.path.display())))
    }

    /// Bytes `range` of tensor `name`'s data, by `pread` — nothing else of the file is read.
    pub fn read_range(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let e = self.entry(name)?;
        if range.start > range.end || range.end > e.end - e.begin {
            return Err(LowerError::weights(format!("`{name}`: bytes {range:?} of a tensor of {}", e.end - e.begin)));
        }
        let mut raw = vec![0u8; (range.end - range.start) as usize];
        read_exact_at(&self.file, &mut raw, self.data_offset + e.begin + range.start)
            .map_err(|x| LowerError::weights(format!("`{name}`: {x}")))?;
        Ok(raw)
    }

    pub fn meta(&self, name: &str) -> Option<TensorMeta> {
        self.entries.get(name).map(|e| TensorMeta { dtype: e.dtype.clone(), shape: e.shape.clone(), bytes: e.end - e.begin })
    }

    pub fn read(&self, name: &str) -> Result<Tensor> {
        let e = self.entry(name)?;
        let data = widen_floats(&e.dtype, &self.read_range(name, 0..e.end - e.begin)?)
            .map_err(|x| LowerError::weights(format!("`{name}` is {}, not a float type this reader widens ({x})", e.dtype)))?;
        Ok(Tensor::new(e.shape.clone(), data))
    }

    /// Rows `rows` (every axis but the last flattened) widened to `f32`: only those rows' bytes
    /// are read.
    pub fn read_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let e = self.entry(name)?;
        let meta = TensorMeta { dtype: e.dtype.clone(), shape: e.shape.clone(), bytes: e.end - e.begin };
        let (n, cols) = meta.rows_cols();
        if rows.start > rows.end || rows.end > n {
            return Err(LowerError::weights(format!("`{name}`: rows {rows:?} of {n} (shape {:?})", e.shape)));
        }
        let rb = meta.row_bytes();
        let raw = self.read_range(name, rows.start as u64 * rb..rows.end as u64 * rb)?;
        let data = widen_floats(&e.dtype, &raw).map_err(|x| LowerError::weights(format!("`{name}`: {x}")))?;
        Ok(Tensor::new(vec![rows.len(), cols], data))
    }
}

impl SafetensorsFile {
    /// An `I32` tensor as stored.
    pub fn read_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        let e = self.entries.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}` in {}", self.path.display())))?;
        if e.dtype != "I32" {
            return Err(LowerError::weights(format!("`{name}` is {}, not I32", e.dtype)));
        }
        let raw = self.read_range(name, 0..e.end - e.begin)?;
        Ok((e.shape.clone(), raw.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()))
    }
}

/// A checkpoint: one or more safetensors files.
pub struct Checkpoint {
    pub files: Vec<SafetensorsFile>,
    index: BTreeMap<String, usize>,
}

impl Checkpoint {
    /// `path` is a `.safetensors` file, a `model.safetensors.index.json`, or a directory holding
    /// either.
    pub fn open(path: &Path) -> Result<Self> {
        let p = if path.is_dir() {
            let idx = path.join("model.safetensors.index.json");
            let one = path.join("model.safetensors");
            let (bidx, bone) = (path.join("pytorch_model.bin.index.json"), path.join("pytorch_model.bin"));
            // The safetensors checkpoint when there is one; else the PyTorch one (read, never run: `torchzip`).
            if idx.exists() {
                idx
            } else if one.exists() || !(bidx.exists() || bone.exists()) {
                one
            } else if bidx.exists() {
                bidx
            } else {
                bone
            }
        } else {
            path.to_path_buf()
        };
        let files: Vec<SafetensorsFile> = if p.extension().and_then(|e| e.to_str()) == Some("json") {
            let dir = p.parent().unwrap_or(Path::new("."));
            let v: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| LowerError::Io(format!("{}: {e}", p.display())))?)
                    .map_err(|e| LowerError::weights(format!("index: {e}")))?;
            let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or_else(|| LowerError::weights("index has no weight_map"))?;
            let shards: BTreeSet<String> = wm.values().filter_map(|x| x.as_str().map(str::to_string)).collect();
            let files = shards.iter().map(|s| SafetensorsFile::open(&dir.join(s))).collect::<Result<Vec<_>>>()?;
            // The index must agree with the shard headers.
            for (name, shard) in wm {
                let s = shard.as_str().unwrap_or("");
                let fi = shards.iter().position(|x| x == s).ok_or_else(|| LowerError::weights("index shard"))?;
                if !files[fi].entries.contains_key(name) {
                    return Err(LowerError::weights(format!("index names `{name}` in {s}, which does not contain it")));
                }
            }
            files
        } else {
            vec![SafetensorsFile::open(&p)?]
        };
        let mut index = BTreeMap::new();
        for (i, f) in files.iter().enumerate() {
            for n in f.entries.keys() {
                if index.insert(n.clone(), i).is_some() {
                    return Err(LowerError::weights(format!("tensor `{n}` appears in two shards")));
                }
            }
        }
        Ok(Checkpoint { files, index })
    }
}

impl TensorSource for Checkpoint {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.index.get(name).map(|i| self.files[*i].entries[name].shape.clone())
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let i = self.index.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        self.files[*i].read(name)
    }
    fn names(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }
    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        let i = self.index.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        self.files[*i].read_i32(name)
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.index.get(name).and_then(|i| self.files[*i].meta(name))
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let i = self.index.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        self.files[*i].read_range(name, range)
    }
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let i = self.index.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        self.files[*i].read_rows(name, rows)
    }
    fn serves_row_ranges(&self) -> bool {
        true
    }
}

/// Only a `model.safetensors.index.json` (shards absent): names are known, shapes are not.
pub struct IndexOnly(pub BTreeSet<String>);

impl IndexOnly {
    pub fn open(path: &Path) -> Result<Self> {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?)
                .map_err(|e| LowerError::weights(format!("index: {e}")))?;
        let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or_else(|| LowerError::weights("index has no weight_map"))?;
        Ok(IndexOnly(wm.keys().cloned().collect()))
    }
}

// ───────────────────────────── Src evaluation ─────────────────────────────

/// Which index of a checkpoint tensor axis to take.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Pick {
    Range {
        start: usize,
        len: usize,
    },
    /// For `g` in `0..groups`: indices `g·block + offset .. + len`, concatenated.
    Strided {
        block: usize,
        offset: usize,
        len: usize,
        groups: usize,
    },
    /// The layer's own `len` indices, `layer·len .. (layer + 1)·len`: a per-layer slice of a tensor
    /// packed over the layers (Gemma-3n/4's per-layer embeddings and their projection).
    PerLayer {
        len: usize,
    },
    /// Each of the `groups` indices `0..groups` repeated `each` times in a row (`0,0,…,1,1,…`): a per-head value spread over the head's
    /// channels (Kimi delta attention's `A_log[head]` as one decay rate per key channel). Every value is an exact copy.
    Repeat {
        each: usize,
        groups: usize,
    },
}

impl Pick {
    /// The indices at `layer` (only [`Pick::PerLayer`] reads it, and needs one).
    pub fn indices_at(&self, layer: Option<usize>) -> Result<Vec<usize>> {
        Ok(match self {
            Pick::Range { start, len } => (*start..start + len).collect(),
            Pick::Strided { block, offset, len, groups } => {
                (0..*groups).flat_map(|g| (g * block + offset)..(g * block + offset + len)).collect()
            }
            Pick::Repeat { each, groups } => (0..*groups).flat_map(|g| std::iter::repeat_n(g, *each)).collect(),
            Pick::PerLayer { len } => {
                let l = layer.ok_or_else(|| LowerError::weights("a per-layer slice outside a layer"))?;
                (l * len..(l + 1) * len).collect()
            }
        })
    }
    pub fn count(&self) -> usize {
        match self {
            Pick::Range { len, .. } | Pick::PerLayer { len } => *len,
            Pick::Strided { len, groups, .. } => len * groups,
            Pick::Repeat { each, groups } => each * groups,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum MapFn {
    /// `−exp(x)` (`A_log` → `A`).
    NegExp,
    Scale(f64),
    /// `tanh(x)` (Mllama's `tanh(cross_attn_attn_gate)`: a gate is a function of the weight only).
    Tanh,
    /// `x / 2^(layer / every)` (RWKV `rescale_every`).
    RescaleByLayer {
        every: usize,
    },
}

/// `λ_init(layer) = base − amp·e^{−rate·layer}` (DiffLlama's `lambda_init_fn`: 0.8 − 0.6·e^{−0.3·layer}), the layer 0-based.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, serde::Deserialize)]
pub struct LambdaInit {
    pub base: f64,
    pub amp: f64,
    pub rate: f64,
}

impl LambdaInit {
    pub fn at(&self, layer: usize) -> f64 {
        self.base - self.amp * crate::detmath::exp(-self.rate * layer as f64)
    }
}

/// A param computed from SEVERAL checkpoint tensors (and the layer index), at conversion.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum CombineFn {
    /// `ATTN_DIFFERENTIAL_V1`: `[q1, k1, q2, k2]` (each `[d]`) → `[exp(Σ q1⊙k1) − exp(Σ q2⊙k2) + λ_init(layer)]`.
    DiffLambda(LambdaInit),
    /// `ATTN_DIFFERENTIAL_V1`: no input → `[1 − λ_init(layer)]`.
    OneMinusLambdaInit(LambdaInit),
}

impl CombineFn {
    pub fn arity(&self) -> usize {
        match self {
            CombineFn::DiffLambda(_) => 4,
            CombineFn::OneMinusLambdaInit(_) => 0,
        }
    }
    /// The value from the inputs' tensors (the exact sums in `f64`, as torch's `dtype=float32` sums are to within an ulp).
    pub fn eval(&self, ins: &[Tensor], layer: Option<usize>) -> Result<Tensor> {
        if ins.len() != self.arity() {
            return Err(LowerError::weights(format!("{self:?} takes {} tensors, got {}", self.arity(), ins.len())));
        }
        let l = layer.ok_or_else(|| LowerError::weights("a layer-dependent constant outside a layer"))?;
        let v = match self {
            CombineFn::DiffLambda(init) => {
                let dot = |a: &Tensor, b: &Tensor| -> Result<f64> {
                    if a.data.len() != b.data.len() {
                        return Err(LowerError::weights(format!("λ vectors of {} and {} values", a.data.len(), b.data.len())));
                    }
                    Ok(a.data.iter().zip(&b.data).map(|(x, y)| *x as f64 * *y as f64).sum())
                };
                crate::detmath::exp(dot(&ins[0], &ins[1])?) - crate::detmath::exp(dot(&ins[2], &ins[3])?) + init.at(l)
            }
            CombineFn::OneMinusLambdaInit(init) => 1.0 - init.at(l),
        };
        Ok(Tensor::new(vec![1], vec![v as f32]))
    }
}

/// How to build one HL param from checkpoint tensors (a frontend's weight mapping produces
/// these; `crate::hf_weights` for Hugging Face). Templates may contain `{L}` (layer),
/// `{E}` (expert) and `{H}` (head), bound by the param's layer and by [`Src::Stack`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Src {
    Tensor(String),
    Take {
        src: Box<Src>,
        axis: usize,
        pick: Pick,
    },
    /// Swap the last two axes.
    Transpose(Box<Src>),
    /// Stack `count` instances of `src` with `var` ∈ {`E`, `H`} bound to `0..count`, on a new axis 0.
    Stack {
        src: Box<Src>,
        var: char,
        count: usize,
    },
    Map {
        src: Box<Src>,
        f: MapFn,
    },
    /// A `[1]` value computed from several tensors ([`CombineFn`]).
    Combine {
        srcs: Vec<Src>,
        f: CombineFn,
    },
    Reshape {
        src: Box<Src>,
        shape: Vec<usize>,
    },
    /// The value's leading axes flattened into rows (`[Π lead, cols]`), then zero rows appended up to
    /// `rows`: per-layer tables of different heights (a hashed n-gram embedding's primes differ by
    /// layer) brought to the one shape their shared block declares.
    PadRows {
        src: Box<Src>,
        rows: usize,
    },
    /// A group-quantised linear stored as `{module}.qweight`, `.qzeros`, `.scales` (and GPTQ's
    /// `.g_idx`): its value is the `[out, in]` weight the format defines ([`QWeight::dequant`]);
    /// [`eval_qsrc`] reads the stored integers themselves.
    Quant {
        module: String,
        fmt: QFormat,
    },
}

impl Src {
    /// Whether a pre-quantised tensor is read somewhere inside.
    pub fn is_quant(&self) -> bool {
        match self {
            Src::Quant { fmt, .. } => fmt.is_integers(),
            Src::Tensor(_) => false,
            Src::Combine { srcs, .. } => srcs.iter().any(Src::is_quant),
            Src::Take { src, .. }
            | Src::Transpose(src)
            | Src::Stack { src, .. }
            | Src::Map { src, .. }
            | Src::Reshape { src, .. }
            | Src::PadRows { src, .. } => {
                src.is_quant()
            }
        }
    }
    /// The quantised format read inside, if any.
    pub fn quant_format(&self) -> Option<&QFormat> {
        match self {
            Src::Quant { fmt, .. } => fmt.is_integers().then_some(fmt),
            Src::Tensor(_) => None,
            Src::Combine { srcs, .. } => srcs.iter().find_map(Src::quant_format),
            Src::Take { src, .. }
            | Src::Transpose(src)
            | Src::Stack { src, .. }
            | Src::Map { src, .. }
            | Src::Reshape { src, .. }
            | Src::PadRows { src, .. } => {
                src.quant_format()
            }
        }
    }
    pub fn t(name: impl Into<String>) -> Src {
        Src::Tensor(name.into())
    }
    pub fn rows(self, pick: Pick) -> Src {
        Src::Take { src: Box::new(self), axis: 0, pick }
    }
    pub fn take(self, axis: usize, pick: Pick) -> Src {
        Src::Take { src: Box::new(self), axis, pick }
    }
    pub fn transpose(self) -> Src {
        Src::Transpose(Box::new(self))
    }
    pub fn map(self, f: MapFn) -> Src {
        Src::Map { src: Box::new(self), f }
    }
    pub fn reshape(self, shape: Vec<usize>) -> Src {
        Src::Reshape { src: Box::new(self), shape }
    }
    /// [`Src::PadRows`].
    pub fn pad_rows(self, rows: usize) -> Src {
        Src::PadRows { src: Box::new(self), rows }
    }
    pub fn stack(self, var: char, count: usize) -> Src {
        Src::Stack { src: Box::new(self), var, count }
    }
}

/// Resolves template names against a source, honouring the spec's prefix aliases, and records
/// every tensor it touched.
pub struct Resolver<'a> {
    pub src: &'a dyn TensorSource,
    pub aliases: Vec<(String, String)>,
    pub touched: RefCell<BTreeSet<String>>,
    pub ignored_prefixes: Vec<String>,
    names: Arc<BTreeSet<String>>,
}

impl<'a> Resolver<'a> {
    pub fn new(src: &'a dyn TensorSource, aliases: &[(String, String)]) -> Self {
        Self::shared(src, aliases, Arc::new(src.names().into_iter().collect()))
    }

    /// A resolver over names listed once and shared (a streaming conversion makes one per row
    /// block).
    pub fn shared(src: &'a dyn TensorSource, aliases: &[(String, String)], names: Arc<BTreeSet<String>>) -> Self {
        Resolver { src, aliases: aliases.to_vec(), touched: RefCell::new(BTreeSet::new()), ignored_prefixes: vec![], names }
    }

    /// The checkpoint's tensor for `name`. **`TENSOR_NAME_ALTERNATIVES_V1`**: a name may list alternatives, `a|b|c` — complete names,
    /// the first the checkpoint has is the one (a family whose checkpoints spell a tensor two ways: Zamba2's Mamba layer under
    /// `layers.N.mamba` or, in a hybrid layer, `layers.N.mamba_decoder.mamba`; Kimi-Linear's dense MLP under `mlp` as the hub has it
    /// or `block_sparse_moe` as `transformers` 5.17 saves it). A name with no alternative is as before.
    pub fn resolve(&self, name: &str) -> Option<String> {
        if name.contains('|') {
            return name.split('|').find_map(|alt| self.resolve(alt));
        }
        if self.names.contains(name) {
            return Some(name.to_string());
        }
        for (canon, alt) in &self.aliases {
            if let Some(rest) = name.strip_prefix(canon.as_str()) {
                let n = format!("{alt}{rest}");
                if self.names.contains(&n) {
                    return Some(n);
                }
            }
        }
        None
    }

    pub fn with_ignored(mut self, prefixes: &[String]) -> Self {
        self.ignored_prefixes = prefixes.to_vec();
        self
    }

    /// Tensors of the checkpoint the program never read (excluding the ignored prefixes and the module
    /// buffers older checkpoints save, [`is_module_buffer`]).
    pub fn untouched(&self) -> Vec<String> {
        let t = self.touched.borrow();
        self.names
            .iter()
            .filter(|n| !t.contains(*n) && !is_module_buffer(n) && !self.ignored_prefixes.iter().any(|p| n.starts_with(p.as_str())))
            .cloned()
            .collect()
    }
}

/// A **buffer** of a module, not a parameter: rotary frequency tables and position-id ranges that older
/// checkpoints saved (newer ones do not) and no forward pass reads as a weight. A tensor the reading does not use
/// is a feature it may be missing (FR-26) — these are not. Any other buffer (a causal-mask table) is listed by
/// the adapter that knows its family, in `ignored_prefixes`.
pub fn is_module_buffer(name: &str) -> bool {
    ["inv_freq", "cos_cached", "sin_cached", "position_ids"]
        .iter()
        .any(|b| name == *b || name.ends_with(&format!(".{b}")))
}

/// `{L/n}` and `{L%n}` — the quotient and the remainder of the layer by `n` (a model whose HF layer holds `n` HL layers:
/// LongCat-Flash's `layers.{L/2}.self_attn.{L%2}`) — replaced by their values; every other variable is left to [`expand`].
fn expand_layer_arithmetic(s: &str, layer: Option<usize>) -> Result<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("{L") {
        let tail = &rest[i + 2..];
        let (op, after) = match tail.chars().next() {
            Some(c @ ('/' | '%')) => (c, &tail[1..]),
            _ => {
                out.push_str(&rest[..i + 2]);
                rest = tail;
                continue;
            }
        };
        let end = after.find('}').ok_or_else(|| LowerError::eval(format!("`{s}`: a `{{L{op}n` without its `}}`")))?;
        let n: usize = after[..end].parse().map_err(|_| LowerError::eval(format!("`{s}`: `{{L{op}{}}}` takes a positive integer", &after[..end])))?;
        if n == 0 {
            return Err(LowerError::eval(format!("`{s}`: `{{L{op}0}}` divides by zero")));
        }
        let l = layer.ok_or_else(|| LowerError::eval(format!("`{s}` needs a layer")))?;
        out.push_str(&rest[..i]);
        out.push_str(&(if op == '/' { l / n } else { l % n }).to_string());
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn expand(template: &str, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<String> {
    let mut s = expand_layer_arithmetic(template, layer)?;
    if s.contains("{L}") {
        let l = layer.ok_or_else(|| LowerError::eval(format!("`{template}` needs a layer")))?;
        s = s.replace("{L}", &l.to_string());
    }
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), &v.to_string());
    }
    if s.contains('{') {
        return Err(LowerError::eval(format!("unbound variable in `{s}`")));
    }
    Ok(s)
}

fn take(t: &Tensor, axis: usize, idx: &[usize]) -> Result<Tensor> {
    if axis >= t.shape.len() {
        return Err(LowerError::weights(format!("take axis {axis} of a rank-{} tensor", t.shape.len())));
    }
    let n = t.shape[axis];
    if let Some(bad) = idx.iter().find(|i| **i >= n) {
        return Err(LowerError::weights(format!("take index {bad} ≥ axis length {n} (shape {:?})", t.shape)));
    }
    let outer: usize = t.shape[..axis].iter().product();
    let inner: usize = t.shape[axis + 1..].iter().product();
    let mut data = Vec::with_capacity(outer * idx.len() * inner);
    for o in 0..outer {
        for &i in idx {
            let base = (o * n + i) * inner;
            data.extend_from_slice(&t.data[base..base + inner]);
        }
    }
    let mut shape = t.shape.clone();
    shape[axis] = idx.len();
    Ok(Tensor::new(shape, data))
}

fn transpose_last2(t: &Tensor) -> Result<Tensor> {
    let r = t.shape.len();
    if r < 2 {
        return Err(LowerError::weights("transpose of a rank < 2 tensor"));
    }
    let (a, b) = (t.shape[r - 2], t.shape[r - 1]);
    let outer: usize = t.shape[..r - 2].iter().product();
    let mut data = vec![0f32; t.data.len()];
    for o in 0..outer {
        let base = o * a * b;
        for i in 0..a {
            for j in 0..b {
                data[base + j * a + i] = t.data[base + i * b + j];
            }
        }
    }
    let mut shape = t.shape.clone();
    shape.swap(r - 2, r - 1);
    Ok(Tensor::new(shape, data))
}

fn pick_count(p: &Pick) -> usize {
    p.count()
}

/// Shape of a `Src` without reading data (headers only).
pub fn src_shape(src: &Src, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<Vec<usize>> {
    match src {
        Src::Tensor(t) => {
            let n = expand(t, layer, vars)?;
            let rn = r.resolve(&n).ok_or_else(|| LowerError::weights(format!("missing tensor `{n}`")))?;
            r.touched.borrow_mut().insert(rn.clone());
            r.src.shape(&rn).ok_or_else(|| LowerError::weights(format!("no shape for `{rn}`")))
        }
        Src::Take { src, axis, pick } => {
            let mut s = src_shape(src, r, layer, vars)?;
            if *axis >= s.len() {
                return Err(LowerError::weights("take axis out of range"));
            }
            let max = pick.indices_at(layer)?.into_iter().max().unwrap_or(0);
            if pick_count(pick) > 0 && max >= s[*axis] {
                return Err(LowerError::weights(format!("slice reaches {max} of an axis of {} (shape {s:?})", s[*axis])));
            }
            s[*axis] = pick_count(pick);
            Ok(s)
        }
        Src::Transpose(src) => {
            let mut s = src_shape(src, r, layer, vars)?;
            let n = s.len();
            if n < 2 {
                return Err(LowerError::weights("transpose of a rank < 2 tensor"));
            }
            s.swap(n - 2, n - 1);
            Ok(s)
        }
        Src::Stack { src, var, count } => {
            let mut first = None;
            for i in 0..*count {
                let mut v = vars.clone();
                v.insert(*var, i);
                let s = src_shape(src, r, layer, &v)?;
                match &first {
                    None => first = Some(s),
                    Some(f) if *f != s => return Err(LowerError::weights(format!("stacked shapes differ: {f:?} vs {s:?}"))),
                    _ => {}
                }
            }
            let mut s = vec![*count];
            s.extend(first.unwrap_or_default());
            Ok(s)
        }
        Src::Map { src, .. } => src_shape(src, r, layer, vars),
        Src::Combine { srcs, .. } => {
            for s in srcs {
                src_shape(s, r, layer, vars)?;
            }
            Ok(vec![1])
        }
        Src::Reshape { src, shape } => {
            let s = src_shape(src, r, layer, vars)?;
            if s.iter().product::<usize>() != shape.iter().product::<usize>() {
                return Err(LowerError::weights(format!("reshape {s:?} → {shape:?}")));
            }
            Ok(shape.clone())
        }
        Src::PadRows { src, rows } => {
            let s = src_shape(src, r, layer, vars)?;
            let Some((cols, lead)) = s.split_last() else { return Err(LowerError::weights("padding the rows of a scalar")) };
            let have: usize = lead.iter().product();
            if have > *rows {
                return Err(LowerError::weights(format!("{have} rows do not fit the {rows} the table is padded to")));
            }
            Ok(vec![*rows, *cols])
        }
        Src::Quant { module, fmt: QFormat::Gguf { .. } } => {
            let m = expand(module, layer, vars)?;
            let n = format!("{m}.weight");
            let rn = r.resolve(&n).ok_or_else(|| LowerError::weights(format!("missing tensor `{n}`")))?;
            r.touched.borrow_mut().insert(rn.clone());
            r.src.shape(&rn).ok_or_else(|| LowerError::weights(format!("no shape for `{rn}`")))
        }
        Src::Quant { module, fmt } => {
            let m = expand(module, layer, vars)?;
            let (f, params) = fmt_binding(fmt)?;
            let t = tensors_of(&f);
            if !t.is_integers()
                && let Some(w) = plain_float(&m, t, r)
            {
                r.touched.borrow_mut().insert(w.clone());
                return r.src.shape(&w).ok_or_else(|| LowerError::weights(format!("no shape for `{w}`")));
            }
            let roles = role_tensors(&m, t, r, false)?;
            let (out, inp) = t.dims(&roles, &params).map_err(|e| LowerError::weights(format!("`{m}`: {e}")))?;
            Ok(vec![out, inp])
        }
    }
}

/// The descriptor a pre-quantised format is read with, and its parameters.
fn fmt_binding(fmt: &QFormat) -> Result<(std::sync::Arc<crate::quantfmt::QuantFormat>, BTreeMap<String, i64>)> {
    let (f, params) = fmt.binding().ok_or_else(|| LowerError::weights("a block-quantised (GGUF) tensor is read by `load_qweight`"))?;
    if f.as_tensors().is_none() {
        return Err(LowerError::weights(format!("quant format `{}` is not a tensors format", f.name())));
    }
    Ok((f, params))
}

fn tensors_of(f: &crate::quantfmt::QuantFormat) -> &crate::quantfmt::tensors::TensorsFormat {
    f.as_tensors().expect("checked by fmt_binding")
}

/// The role tensors of the module `module` as a `tensors` descriptor declares them. A role's data is
/// read when `with_data`, or when it is small (a shape tensor: the shape expressions may read its
/// elements); otherwise only its header is.
fn role_tensors(module: &str, t: &crate::quantfmt::tensors::TensorsFormat, r: &Resolver, with_data: bool) -> Result<Vec<Option<RoleTensor>>> {
    let mut out = Vec::new();
    for (_role, suffix, required) in t.roles() {
        let n = format!("{module}{suffix}");
        match r.resolve(&n) {
            Some(rn) => {
                r.touched.borrow_mut().insert(rn.clone());
                let meta = r.src.metadata(&rn).ok_or_else(|| LowerError::weights(format!("no header for `{rn}`")))?;
                let data = if with_data || meta.bytes <= 4096 { r.src.read_slice(&rn, 0..meta.bytes)? } else { Vec::new() };
                out.push(Some(RoleTensor { shape: meta.shape, dtype: meta.dtype, data }));
            }
            None if required => {
                return Err(if r.resolve(&format!("{module}.weight")).is_some() && suffix != ".weight" {
                    LowerError::weights(format!("`{module}` is stored in float, but the config says it is quantised"))
                } else {
                    LowerError::weights(format!("missing tensor `{n}`"))
                });
            }
            None => out.push(None),
        }
    }
    Ok(out)
}

/// Read a quantised module's integers: the descriptor's decode of its role tensors.
fn load_quant(module: &str, fmt: &QFormat, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<QWeight> {
    let m = expand(module, layer, vars)?;
    if let QFormat::Gguf { .. } = fmt {
        let n = format!("{m}.weight");
        let rn = r.resolve(&n).ok_or_else(|| LowerError::weights(format!("missing tensor `{n}`")))?;
        r.touched.borrow_mut().insert(rn.clone());
        return r.src.load_qweight(&rn).map_err(|e| LowerError::weights(format!("`{m}`: {e}")));
    }
    let (f, params) = fmt_binding(fmt)?;
    let t = tensors_of(&f);
    let roles = role_tensors(&m, t, r, true)?;
    t.decode_integers(&roles, &params).map_err(|e| LowerError::weights(format!("`{m}`: {e}")))
}

/// The plain float tensor a module of a floats-decoding format is stored as, when the quantiser left
/// it alone (`<module>.weight` is an ordinary float tensor and the format's other roles are absent):
/// a float is its own value, so reading it as stored is exact.
fn plain_float(m: &str, t: &crate::quantfmt::tensors::TensorsFormat, r: &Resolver) -> Option<String> {
    let dtype_of = |suffix: &str| r.resolve(&format!("{m}{suffix}")).and_then(|n| r.src.metadata(&n)).map(|x| x.dtype);
    if t.stores(dtype_of) {
        return None;
    }
    let w = r.resolve(&format!("{m}.weight"))?;
    matches!(r.src.metadata(&w)?.dtype.as_str(), "F32" | "F16" | "BF16" | "F64").then_some(w)
}

/// A module of a format that decodes to floats (an FP8 weight and its scales): its `[out, in]` weight.
fn load_quant_floats(module: &str, fmt: &QFormat, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<Tensor> {
    let m = expand(module, layer, vars)?;
    let (f, params) = fmt_binding(fmt)?;
    let t = tensors_of(&f);
    if let Some(w) = plain_float(&m, t, r) {
        r.touched.borrow_mut().insert(w.clone());
        return r.src.load(&w);
    }
    let roles = role_tensors(&m, t, r, true)?;
    let (data, out, inp) = t.decode_floats(&roles, &params).map_err(|e| LowerError::weights(format!("`{m}`: {e}")))?;
    Ok(Tensor::new(vec![out, inp], data))
}

/// The stored integers of a quantised param (`None` when the param is not quantised): the
/// module's [`QWeight`], through the row slices a fused projection takes; one per expert through a
/// [`Src::Stack`] of experts. Anything else on the way (a transpose, a reshape, a map) is refused —
/// it would not keep the integers.
pub fn eval_qsrc(src: &Src, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<Option<Vec<QWeight>>> {
    match src {
        Src::Quant { module, fmt } if fmt.is_integers() => Ok(Some(vec![load_quant(module, fmt, r, layer, vars)?])),
        Src::Quant { .. } => Ok(None),
        Src::Take { src: inner, axis: 0, pick } => match eval_qsrc(inner, r, layer, vars)? {
            Some(v) if v.len() == 1 => Ok(Some(vec![v[0].take_rows(&pick.indices_at(layer)?)?])),
            Some(_) => Err(LowerError::not_lowerable("a row slice across stacked quantised experts")),
            None => Ok(None),
        },
        Src::Stack { src: inner, var, count } if inner.is_quant() => {
            let mut out = Vec::with_capacity(*count);
            for i in 0..*count {
                let mut v = vars.clone();
                v.insert(*var, i);
                match eval_qsrc(inner, r, layer, &v)? {
                    Some(q) if q.len() == 1 => out.extend(q),
                    _ => return Err(LowerError::not_lowerable("a nested stack of quantised tensors")),
                }
            }
            Ok(Some(out))
        }
        other if other.is_quant() => {
            Err(LowerError::not_lowerable(format!("a quantised weight read through {other:?} (only row slices keep the integers)")))
        }
        _ => Ok(None),
    }
}

/// The TIR structure of every quantised param of `prog` ([`QLayout`]), by param index.
pub fn quant_layouts(prog: &HlProgram, binding: &Binding) -> Result<BTreeMap<u32, QLayout>> {
    let mut out = BTreeMap::new();
    for (pi, s) in binding.srcs.iter().enumerate() {
        if let Some(fmt) = s.quant_format() {
            if !matches!(prog.params[pi].shape.len(), 2 | 3) {
                return Err(LowerError::not_lowerable(format!("quantised param `{}` is not a matrix or a stack of experts", prog.params[pi].name)));
            }
            out.insert(pi as u32, fmt.layout());
        }
    }
    Ok(out)
}

/// Evaluate a `Src` to a tensor.
pub fn eval_src(src: &Src, r: &Resolver, layer: Option<usize>, vars: &BTreeMap<char, usize>) -> Result<Tensor> {
    match src {
        Src::Tensor(t) => {
            let n = expand(t, layer, vars)?;
            let rn = r.resolve(&n).ok_or_else(|| LowerError::weights(format!("missing tensor `{n}`")))?;
            r.touched.borrow_mut().insert(rn.clone());
            r.src.load(&rn)
        }
        Src::Take { src, axis, pick } => take(&eval_src(src, r, layer, vars)?, *axis, &pick.indices_at(layer)?),
        Src::Transpose(src) => transpose_last2(&eval_src(src, r, layer, vars)?),
        Src::Stack { src, var, count } => {
            let mut shape = None;
            let mut data = Vec::new();
            for i in 0..*count {
                let mut v = vars.clone();
                v.insert(*var, i);
                let t = eval_src(src, r, layer, &v)?;
                match &shape {
                    None => shape = Some(t.shape.clone()),
                    Some(s) if *s != t.shape => return Err(LowerError::weights("stacked shapes differ")),
                    _ => {}
                }
                data.extend(t.data);
            }
            let mut s = vec![*count];
            s.extend(shape.unwrap_or_default());
            Ok(Tensor::new(s, data))
        }
        Src::Map { src, f } => {
            let mut t = eval_src(src, r, layer, vars)?;
            stream::apply_map(&mut t, f, layer)?;
            Ok(t)
        }
        Src::Combine { srcs, f } => {
            let ins = srcs.iter().map(|s| eval_src(s, r, layer, vars)).collect::<Result<Vec<_>>>()?;
            f.eval(&ins, layer)
        }
        Src::Reshape { src, shape } => {
            let t = eval_src(src, r, layer, vars)?;
            if t.numel() != shape.iter().product::<usize>() {
                return Err(LowerError::weights(format!("reshape {:?} → {shape:?}", t.shape)));
            }
            Ok(Tensor::new(shape.clone(), t.data))
        }
        Src::PadRows { src, rows } => {
            let t = eval_src(src, r, layer, vars)?;
            let Some(cols) = t.shape.last().copied() else { return Err(LowerError::weights("padding the rows of a scalar")) };
            let mut data = t.data;
            if data.len() > rows * cols {
                return Err(LowerError::weights(format!("{} rows do not fit the {rows} the table is padded to", data.len() / cols.max(1))));
            }
            data.resize(rows * cols, 0.0);
            Ok(Tensor::new(vec![*rows, cols], data))
        }
        Src::Quant { module, fmt } if fmt.is_integers() => Ok(load_quant(module, fmt, r, layer, vars)?.dequant()),
        Src::Quant { module, fmt } => load_quant_floats(module, fmt, r, layer, vars),
    }
}

/// The layers that bind a per-layer param (layers whose block references it).
pub fn layers_of_param(prog: &HlProgram, p: u32) -> Vec<usize> {
    let blocks: BTreeSet<usize> = (0..prog.blocks.len()).filter(|b| prog.block_params(*b).contains(&p)).collect();
    prog.schedule.iter().enumerate().filter(|(_, k)| blocks.contains(&(**k as usize))).map(|(l, _)| l).collect()
}

/// The MODEL layers whose checkpoint tensors a per-layer param reads (the occurrences of its block,
/// mapped through [`HlProgram::model_layer`]: a layer run as several blocks counts once).
pub fn model_layers_of_param(prog: &HlProgram, p: u32) -> Vec<Option<usize>> {
    layers_of_param(prog, p).into_iter().map(|l| prog.model_layer(l)).collect::<BTreeSet<usize>>().into_iter().map(Some).collect()
}

/// A frontend's weight mapping: one [`Src`] per HL param (same order as `HlProgram::params`),
/// plus the prefix aliases its tensor names may appear under.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Binding {
    pub srcs: Vec<Src>,
    pub aliases: Vec<(String, String)>,
    /// Tensor-name prefixes that are unread by design and not reported as unused.
    pub ignored_prefixes: Vec<String>,
}

/// The tail of a shape-mismatch message when `got` and `want` differ only by size-1 axes (a bias stored
/// `[1, E]` where the graph wants `[E]`): the fix an adapter author can apply. Shapes are never coerced
/// silently — an implicit squeeze would be the same class of defect as a dropped flag — so the message
/// names the step instead (`WEIGHTS_EXPR_V1`).
pub fn size_one_hint(param: &str, got: &[usize], want: &[usize]) -> String {
    let core = |s: &[usize]| s.iter().copied().filter(|d| *d != 1).collect::<Vec<usize>>();
    if got != want && core(got) == core(want) {
        format!(
            ": the shapes differ only by size-1 axes — add the step {{\"reshape\": \"param\"}} to the expression of `{param}` in the adapter's `spec.hf.weights`"
        )
    } else {
        String::new()
    }
}

/// A shape-only check of a checkpoint against a program.
#[derive(Debug, Default)]
pub struct WeightReport {
    pub bound: usize,
    pub errors: Vec<String>,
    pub unused: Vec<String>,
}

pub fn check_weights(prog: &HlProgram, binding: &Binding, source: &dyn TensorSource) -> WeightReport {
    let r = Resolver::new(source, &binding.aliases).with_ignored(&binding.ignored_prefixes);
    let mut rep = WeightReport::default();
    let none = BTreeMap::new();
    for (pi, d) in prog.params.iter().enumerate() {
        let layers: Vec<Option<usize>> =
            if d.per_layer { model_layers_of_param(prog, pi as u32) } else { vec![None] };
        for l in layers {
            match src_shape(&binding.srcs[pi], &r, l, &none) {
                Ok(s) if s == d.shape => rep.bound += 1,
                Ok(s) => rep.errors.push(format!(
                    "param `{}`{}: checkpoint gives {s:?}, graph needs {:?}{}",
                    d.name,
                    l.map(|x| format!(" (layer {x})")).unwrap_or_default(),
                    d.shape,
                    size_one_hint(&d.name, &s, &d.shape)
                )),
                Err(e) => {
                    rep.errors.push(format!("param `{}`{}: {e}", d.name, l.map(|x| format!(" (layer {x})")).unwrap_or_default()))
                }
            }
        }
    }
    rep.unused = r.untouched();
    rep
}

/// Only names are known (an index without shards): report missing tensors.
pub fn check_names(prog: &HlProgram, binding: &Binding, names: &BTreeSet<String>) -> WeightReport {
    struct Names<'a>(&'a BTreeSet<String>);
    impl TensorSource for Names<'_> {
        fn shape(&self, _: &str) -> Option<Vec<usize>> {
            None
        }
        fn load(&self, n: &str) -> Result<Tensor> {
            Err(LowerError::weights(format!("names only: `{n}`")))
        }
        fn names(&self) -> Vec<String> {
            self.0.iter().cloned().collect()
        }
    }
    let src = Names(names);
    let r = Resolver::new(&src, &binding.aliases).with_ignored(&binding.ignored_prefixes);
    let mut rep = WeightReport::default();
    fn leaves(s: &Src, out: &mut Vec<(String, Vec<(char, usize)>)>) {
        match s {
            Src::Tensor(t) => out.push((t.clone(), vec![])),
            Src::Quant { module, fmt: QFormat::Gguf { .. } } => out.push((format!("{module}.weight"), vec![])),
            Src::Quant { module, fmt } => {
                if let Ok((f, _)) = fmt_binding(fmt) {
                    for (_, suffix, required) in tensors_of(&f).roles() {
                        if required {
                            out.push((format!("{module}{suffix}"), vec![]));
                        }
                    }
                }
            }
            Src::Take { src, .. } | Src::Transpose(src) | Src::Map { src, .. } | Src::Reshape { src, .. } | Src::PadRows { src, .. } => {
                leaves(src, out)
            }
            Src::Combine { srcs, .. } => srcs.iter().for_each(|s| leaves(s, out)),
            Src::Stack { src, var, count } => {
                let mut inner = vec![];
                leaves(src, &mut inner);
                for (t, v) in inner {
                    for i in 0..*count {
                        let mut vv = v.clone();
                        vv.push((*var, i));
                        out.push((t.clone(), vv));
                    }
                }
            }
        }
    }
    for (pi, d) in prog.params.iter().enumerate() {
        let layers: Vec<Option<usize>> =
            if d.per_layer { model_layers_of_param(prog, pi as u32) } else { vec![None] };
        let mut ls = vec![];
        leaves(&binding.srcs[pi], &mut ls);
        for l in layers {
            for (t, vars) in &ls {
                let vars: BTreeMap<char, usize> = vars.iter().copied().collect();
                match expand(t, l, &vars) {
                    Ok(n) => match r.resolve(&n) {
                        Some(rn) => {
                            r.touched.borrow_mut().insert(rn);
                            rep.bound += 1;
                        }
                        None => rep.errors.push(format!("param `{}`: missing tensor `{n}`", d.name)),
                    },
                    Err(e) => rep.errors.push(e.to_string()),
                }
            }
        }
    }
    rep.unused = r.untouched();
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn write_safetensors(path: &Path, tensors: &[(&str, &str, Vec<usize>, Vec<u8>)]) {
        std::fs::write(path, safetensors_bytes(tensors)).unwrap();
    }

    /// A `.safetensors` file's bytes (8-byte length, JSON header, data).
    pub(crate) fn safetensors_bytes(tensors: &[(&str, &str, Vec<usize>, Vec<u8>)]) -> Vec<u8> {
        let mut header = serde_json::Map::new();
        let mut data = Vec::new();
        for (name, dtype, shape, bytes) in tensors {
            let b = data.len();
            data.extend_from_slice(bytes);
            header.insert(name.to_string(), serde_json::json!({"dtype": dtype, "shape": shape, "data_offsets": [b, data.len()]}));
        }
        header.insert("__metadata__".into(), serde_json::json!({"format": "pt"}));
        let h = serde_json::to_vec(&serde_json::Value::Object(header)).unwrap();
        let mut out = (h.len() as u64).to_le_bytes().to_vec();
        out.extend(h);
        out.extend(data);
        out
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tir-lower-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reads_bf16_f16_f32_and_rejects_bad_headers() {
        let d = tmpdir("st");
        let p = d.join("m.safetensors");
        let bf: Vec<u8> = [1.0f32, -2.5, 0.15625].iter().flat_map(|x| ((x.to_bits() >> 16) as u16).to_le_bytes()).collect();
        // f16: 1.0 = 0x3c00, -2.0 = 0xc000, 65504 = 0x7bff, smallest subnormal 0x0001.
        let hf: Vec<u8> = [0x3c00u16, 0xc000, 0x7bff, 0x0001].iter().flat_map(|x| x.to_le_bytes()).collect();
        let f: Vec<u8> = [1.5f32, 2.5, 3.5, 4.5].iter().flat_map(|x| x.to_le_bytes()).collect();
        write_safetensors(&p, &[("a", "BF16", vec![3], bf), ("b", "F16", vec![2, 2], hf), ("c", "F32", vec![2, 2], f)]);
        let ck = Checkpoint::open(&p).unwrap();
        assert_eq!(ck.load("a").unwrap().data, vec![1.0, -2.5, 0.15625]);
        let b = ck.load("b").unwrap();
        assert_eq!(b.shape, vec![2, 2]);
        assert_eq!(&b.data[..3], &[1.0, -2.0, 65504.0]);
        assert!((b.data[3] - 5.960_464_5e-8).abs() < 1e-12);
        assert_eq!(ck.load("c").unwrap().data, vec![1.5, 2.5, 3.5, 4.5]);
        // Truncated / garbage headers are errors, not panics.
        assert!(parse_header(&[0xff; 4], 4).is_err());
        assert!(parse_header(&[0xff; 16], 16).is_err());
        let mut bad = 10u64.to_le_bytes().to_vec();
        bad.extend(b"{not json}");
        assert!(parse_header(&bad, bad.len() as u64).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn sharded_index_resolves_tensors_across_files() {
        let d = tmpdir("shard");
        let f32b = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        write_safetensors(&d.join("model-00001-of-00002.safetensors"), &[("x.weight", "F32", vec![2], f32b(&[1.0, 2.0]))]);
        write_safetensors(&d.join("model-00002-of-00002.safetensors"), &[("y.weight", "F32", vec![1], f32b(&[3.0]))]);
        std::fs::write(
            d.join("model.safetensors.index.json"),
            serde_json::json!({"metadata": {}, "weight_map": {"x.weight": "model-00001-of-00002.safetensors", "y.weight": "model-00002-of-00002.safetensors"}}).to_string(),
        )
        .unwrap();
        let ck = Checkpoint::open(&d).unwrap();
        assert_eq!(ck.load("y.weight").unwrap().data, vec![3.0]);
        assert_eq!(ck.names(), vec!["x.weight".to_string(), "y.weight".to_string()]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn src_slices_fused_layouts_transposes_and_stacks() {
        let mut m = MapSource::default();
        // A 6×2 fused tensor, rows r·10 + c.
        m.0.insert(
            "qkv.weight".into(),
            Tensor::new(vec![6, 2], (0..6).flat_map(|r| [r as f32 * 10.0, r as f32 * 10.0 + 1.0]).collect()),
        );
        m.0.insert("e.0.w".into(), Tensor::new(vec![1, 2], vec![1.0, 2.0]));
        m.0.insert("e.1.w".into(), Tensor::new(vec![1, 2], vec![3.0, 4.0]));
        m.0.insert("a_log".into(), Tensor::new(vec![2], vec![0.0, (2f32).ln()]));
        let r = Resolver::new(&m, &[]);
        let none = BTreeMap::new();
        // Per-head interleaved q/k/v with 2 heads of 1 row: rows [q0,k0,v0,q1,k1,v1].
        let k =
            eval_src(&Src::t("qkv.weight").rows(Pick::Strided { block: 3, offset: 1, len: 1, groups: 2 }), &r, None, &none).unwrap();
        assert_eq!(k.data, vec![10.0, 11.0, 40.0, 41.0]);
        let t = eval_src(&Src::t("qkv.weight").rows(Pick::Range { start: 1, len: 2 }).transpose(), &r, None, &none).unwrap();
        assert_eq!(t.shape, vec![2, 2]);
        assert_eq!(t.data, vec![10.0, 20.0, 11.0, 21.0]);
        let s = eval_src(&Src::t("e.{E}.w").stack('E', 2), &r, None, &none).unwrap();
        assert_eq!((s.shape.clone(), s.data.clone()), (vec![2, 1, 2], vec![1.0, 2.0, 3.0, 4.0]));
        let a = eval_src(&Src::t("a_log").map(MapFn::NegExp), &r, None, &none).unwrap();
        assert!((a.data[0] + 1.0).abs() < 1e-7 && (a.data[1] + 2.0).abs() < 1e-6);
        assert!(eval_src(&Src::t("qkv.weight").rows(Pick::Range { start: 5, len: 2 }), &r, None, &none).is_err());
        assert_eq!(r.untouched(), Vec::<String>::new());
    }

    #[test]
    fn prefix_aliases_find_the_other_spelling() {
        let mut m = MapSource::default();
        m.0.insert("h.0.ln_1.weight".into(), Tensor::new(vec![1], vec![1.0]));
        let r = Resolver::new(&m, &[("transformer.".into(), String::new())]);
        assert_eq!(r.resolve("transformer.h.0.ln_1.weight").as_deref(), Some("h.0.ln_1.weight"));
        assert_eq!(r.resolve("transformer.h.1.ln_1.weight"), None);
    }
}
