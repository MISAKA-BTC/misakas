//! **GGUF (llama.cpp) checkpoints**: the file read directly, its block formats unpacked to their
//! stored integers, and its metadata mapped onto a Hugging Face config.
//!
//! * [`GgufFile`] parses a GGUF v2/v3 file (magic, key–value metadata, tensor infos, the aligned
//!   data blob) with every count and length bounded before anything is allocated.
//! * The block formats are unpacked into [`QWeight`] — the stored integers with their per-group
//!   scales, zero points and float minimums, exactly — so a projection lowers from them
//!   (`lower::qlinear`) and the float reference uses their dequantisation, which is the value
//!   llama.cpp's `dequantize_row_*` computes (one f32 rounding of the exact value):
//!
//!   | type | block | groups | `W = scale · (q − zero) − min` |
//!   | --- | --- | --- | --- |
//!   | `Q8_0` | 32 × i8 + fp16 `d` | 32 | `d · q` |
//!   | `Q4_0` / `Q5_0` | 32 nibbles (+ 32 high bits) + `d` | 32 | `d · (q − 8)` / `d · (q − 16)` |
//!   | `Q4_1` / `Q5_1` | as above + fp16 `m` | 32 | `d · q + m` |
//!   | `Q4_K` / `Q5_K` | 256: `d`, `dmin`, 8 × 6-bit (scale, min), nibbles (+ high bits) | 32 | `d·sc · q − dmin·m` |
//!   | `Q6_K` | 256: 4 + 2 bits, 16 × i8 scales, `d` | 16 | `d·sc · (q − 32)` |
//!
//! * [`GgufModel`] maps the metadata of the `llama` (Llama, Mistral), `qwen2`, `qwen3`, `gemma`
//!   and `gemma2` architectures onto the Hugging Face `config.json` of the same model, and serves
//!   the tensors under their Hugging Face names and layouts ([`TensorSource`]): llama.cpp's
//!   converter permutes `llama`'s q/k rows for its interleaved rotary (undone here, exactly: the
//!   rows are taken back), and stores Gemma's RMSNorm gains as `1 + w` (the spec then multiplies
//!   by the stored value). The lowering downstream is the Hugging Face one, unchanged.

use crate::error::{LowerError, Result};
use crate::prequant::{QFormat, QLayout, QWeight, QuantConfig};
use crate::spec::{ArchSpec, Gain, NormSpec, Residual};
use crate::weights::{Tensor, TensorSource, f16_to_f32};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAX_STRING: u64 = 1 << 24;
const MAX_ARRAY: u64 = 1 << 26;
const MAX_TENSORS: u64 = 1 << 20;
const MAX_KV: u64 = 1 << 20;
const MAX_DIMS: u32 = 4;

/// A ggml tensor type (the ones this reader decodes by name; any other is `Other`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(non_camel_case_types)]
pub enum GgmlType {
    F32,
    F16,
    BF16,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    Q4_K,
    Q5_K,
    Q6_K,
    Other(u32),
}

impl GgmlType {
    pub fn from_id(id: u32) -> Self {
        match id {
            0 => GgmlType::F32,
            1 => GgmlType::F16,
            2 => GgmlType::Q4_0,
            3 => GgmlType::Q4_1,
            6 => GgmlType::Q5_0,
            7 => GgmlType::Q5_1,
            8 => GgmlType::Q8_0,
            12 => GgmlType::Q4_K,
            13 => GgmlType::Q5_K,
            14 => GgmlType::Q6_K,
            30 => GgmlType::BF16,
            x => GgmlType::Other(x),
        }
    }
    pub fn id(self) -> u32 {
        match self {
            GgmlType::F32 => 0,
            GgmlType::F16 => 1,
            GgmlType::Q4_0 => 2,
            GgmlType::Q4_1 => 3,
            GgmlType::Q5_0 => 6,
            GgmlType::Q5_1 => 7,
            GgmlType::Q8_0 => 8,
            GgmlType::Q4_K => 12,
            GgmlType::Q5_K => 13,
            GgmlType::Q6_K => 14,
            GgmlType::BF16 => 30,
            GgmlType::Other(x) => x,
        }
    }
    /// `(elements, bytes)` of one block.
    pub fn block(self) -> Option<(usize, usize)> {
        Some(match self {
            GgmlType::F32 => (1, 4),
            GgmlType::F16 | GgmlType::BF16 => (1, 2),
            GgmlType::Q4_0 => (32, 18),
            GgmlType::Q4_1 => (32, 20),
            GgmlType::Q5_0 => (32, 22),
            GgmlType::Q5_1 => (32, 24),
            GgmlType::Q8_0 => (32, 34),
            GgmlType::Q4_K => (256, 144),
            GgmlType::Q5_K => (256, 176),
            GgmlType::Q6_K => (256, 210),
            GgmlType::Other(_) => return None,
        })
    }
    pub fn is_float(self) -> bool {
        matches!(self, GgmlType::F32 | GgmlType::F16 | GgmlType::BF16)
    }
    /// Columns per group of the unpacked weight.
    fn group(self) -> usize {
        match self {
            GgmlType::Q6_K => 16,
            _ => 32,
        }
    }
    /// A float offset per group (`Q4_1`, `Q5_1`, the K-quant minimums).
    fn has_min(self) -> bool {
        matches!(self, GgmlType::Q4_1 | GgmlType::Q5_1 | GgmlType::Q4_K | GgmlType::Q5_K)
    }
    fn bits(self) -> u8 {
        match self {
            GgmlType::Q4_0 | GgmlType::Q4_1 | GgmlType::Q4_K => 4,
            GgmlType::Q5_0 | GgmlType::Q5_1 | GgmlType::Q5_K => 5,
            GgmlType::Q6_K => 6,
            _ => 8,
        }
    }
    pub fn name(self) -> String {
        match self {
            GgmlType::Other(x) => format!("type{x}"),
            t => format!("{t:?}"),
        }
    }
}

/// A metadata value.
#[derive(Clone, Debug, PartialEq)]
pub enum GValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    Str(String),
    Arr(Vec<GValue>),
    U64(u64),
    I64(i64),
    F64(f64),
}

impl GValue {
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            GValue::U8(v) => Some(v as u64),
            GValue::U16(v) => Some(v as u64),
            GValue::U32(v) => Some(v as u64),
            GValue::U64(v) => Some(v),
            GValue::I8(v) if v >= 0 => Some(v as u64),
            GValue::I16(v) if v >= 0 => Some(v as u64),
            GValue::I32(v) if v >= 0 => Some(v as u64),
            GValue::I64(v) if v >= 0 => Some(v as u64),
            _ => None,
        }
    }
    /// A float as written: an `f32` becomes the shortest decimal that reads back as it (`1e-6`,
    /// not `9.99999997e-7`), which is the value the Hugging Face config had.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            GValue::F32(v) => format!("{v}").parse().ok(),
            GValue::F64(v) => Some(v),
            _ => self.as_u64().map(|v| v as f64),
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let GValue::Str(s) = self { Some(s) } else { None }
    }
    pub fn as_arr(&self) -> Option<&[GValue]> {
        if let GValue::Arr(a) = self { Some(a) } else { None }
    }
}

/// One tensor's header entry.
#[derive(Clone, Debug, PartialEq)]
pub struct GgufTensorInfo {
    pub name: String,
    /// ggml order: `dims[0]` is the innermost (a row).
    pub dims: Vec<u64>,
    pub ty: GgmlType,
    /// Absolute file offset of the data.
    pub offset: u64,
    pub bytes: u64,
}

impl GgufTensorInfo {
    /// The Hugging Face shape: the dims reversed (`[out, in]` for a matrix).
    pub fn shape(&self) -> Vec<usize> {
        self.dims.iter().rev().map(|d| *d as usize).collect()
    }
    pub fn numel(&self) -> u64 {
        self.dims.iter().product()
    }
}

/// A parsed GGUF file: metadata and tensor table (data read on demand).
#[derive(Clone, Debug)]
pub struct GgufFile {
    pub path: PathBuf,
    pub version: u32,
    pub meta: BTreeMap<String, GValue>,
    pub tensors: BTreeMap<String, GgufTensorInfo>,
    pub alignment: u64,
    pub data_start: u64,
}

struct Rd<R: Read> {
    r: R,
    pos: u64,
}

impl<R: Read> Rd<R> {
    fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut b = vec![0u8; n];
        self.r.read_exact(&mut b).map_err(|e| LowerError::weights(format!("GGUF: truncated header ({e})")))?;
        self.pos += n as u64;
        Ok(b)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut b = [0u8; N];
        self.r.read_exact(&mut b).map_err(|e| LowerError::weights(format!("GGUF: truncated header ({e})")))?;
        self.pos += N as u64;
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u64()?;
        if n > MAX_STRING {
            return Err(LowerError::weights(format!("GGUF: a string of {n} bytes")));
        }
        String::from_utf8(self.bytes(n as usize)?).map_err(|_| LowerError::weights("GGUF: a string that is not UTF-8"))
    }
    fn value(&mut self, ty: u32, depth: u32) -> Result<GValue> {
        Ok(match ty {
            0 => GValue::U8(self.arr::<1>()?[0]),
            1 => GValue::I8(self.arr::<1>()?[0] as i8),
            2 => GValue::U16(u16::from_le_bytes(self.arr()?)),
            3 => GValue::I16(i16::from_le_bytes(self.arr()?)),
            4 => GValue::U32(self.u32()?),
            5 => GValue::I32(i32::from_le_bytes(self.arr()?)),
            6 => GValue::F32(f32::from_le_bytes(self.arr()?)),
            7 => GValue::Bool(self.arr::<1>()?[0] != 0),
            8 => GValue::Str(self.string()?),
            9 => {
                if depth > 2 {
                    return Err(LowerError::weights("GGUF: arrays nested too deep"));
                }
                let et = self.u32()?;
                let n = self.u64()?;
                if n > MAX_ARRAY {
                    return Err(LowerError::weights(format!("GGUF: an array of {n} elements")));
                }
                let mut v = Vec::with_capacity((n as usize).min(1 << 20));
                for _ in 0..n {
                    v.push(self.value(et, depth + 1)?);
                }
                GValue::Arr(v)
            }
            10 => GValue::U64(self.u64()?),
            11 => GValue::I64(i64::from_le_bytes(self.arr()?)),
            12 => GValue::F64(f64::from_le_bytes(self.arr()?)),
            x => return Err(LowerError::weights(format!("GGUF: metadata value type {x}"))),
        })
    }
}

impl GgufFile {
    /// Parse the header of `path`. Total: a malformed or hostile file is an error, never a panic
    /// or an unbounded allocation.
    pub fn open(path: &Path) -> Result<Self> {
        let f = std::fs::File::open(path).map_err(|e| LowerError::Io(format!("{}: {e}", path.display())))?;
        let len = f.metadata().map_err(|e| LowerError::Io(e.to_string()))?.len();
        let mut r = Rd { r: BufReader::new(f), pos: 0 };
        if &r.arr::<4>()? != b"GGUF" {
            return Err(LowerError::weights(format!("{}: not a GGUF file", path.display())));
        }
        let version = r.u32()?;
        if !(2..=3).contains(&version) {
            return Err(LowerError::weights(format!("GGUF version {version} (2 and 3 are read)")));
        }
        let n_tensors = r.u64()?;
        let n_kv = r.u64()?;
        if n_tensors > MAX_TENSORS || n_kv > MAX_KV {
            return Err(LowerError::weights(format!("GGUF: {n_tensors} tensors, {n_kv} metadata keys")));
        }
        let mut meta = BTreeMap::new();
        for _ in 0..n_kv {
            let k = r.string()?;
            let ty = r.u32()?;
            let v = r.value(ty, 0)?;
            if meta.insert(k.clone(), v).is_some() {
                return Err(LowerError::weights(format!("GGUF: metadata key `{k}` twice")));
            }
        }
        let alignment = match meta.get("general.alignment") {
            None => 32,
            Some(v) => v.as_u64().filter(|a| *a > 0 && a.is_power_of_two()).ok_or_else(|| LowerError::weights("GGUF: bad general.alignment"))?,
        };
        let mut infos = Vec::with_capacity(n_tensors as usize);
        for _ in 0..n_tensors {
            let name = r.string()?;
            let nd = r.u32()?;
            if nd == 0 || nd > MAX_DIMS {
                return Err(LowerError::weights(format!("GGUF: tensor `{name}` has {nd} dimensions")));
            }
            let mut dims = Vec::with_capacity(nd as usize);
            for _ in 0..nd {
                dims.push(r.u64()?);
            }
            let ty = GgmlType::from_id(r.u32()?);
            let off = r.u64()?;
            infos.push((name, dims, ty, off));
        }
        let data_start = r.pos.div_ceil(alignment) * alignment;
        let mut tensors = BTreeMap::new();
        for (name, dims, ty, off) in infos {
            let numel = dims.iter().try_fold(1u64, |a, d| a.checked_mul(*d)).ok_or_else(|| LowerError::weights("GGUF: size overflow"))?;
            let bytes = match ty.block() {
                Some((be, bb)) => {
                    if dims[0] % be as u64 != 0 {
                        return Err(LowerError::weights(format!("GGUF: `{name}` rows of {} are not whole {} blocks", dims[0], ty.name())));
                    }
                    numel / be as u64 * bb as u64
                }
                None => 0,
            };
            if off % alignment != 0 {
                return Err(LowerError::weights(format!("GGUF: `{name}` at an unaligned offset")));
            }
            let offset = data_start.checked_add(off).ok_or_else(|| LowerError::weights("GGUF: offset overflow"))?;
            if ty.block().is_some() && offset.checked_add(bytes).is_none_or(|e| e > len) {
                return Err(LowerError::weights(format!("GGUF: `{name}` runs past the end of the file")));
            }
            let info = GgufTensorInfo { name: name.clone(), dims, ty, offset, bytes };
            if tensors.insert(name.clone(), info).is_some() {
                return Err(LowerError::weights(format!("GGUF: tensor `{name}` twice")));
            }
        }
        Ok(GgufFile { path: path.to_path_buf(), version, meta, tensors, alignment, data_start })
    }

    fn info(&self, name: &str) -> Result<&GgufTensorInfo> {
        self.tensors.get(name).ok_or_else(|| LowerError::weights(format!("GGUF: no tensor `{name}`")))
    }

    fn raw(&self, t: &GgufTensorInfo) -> Result<Vec<u8>> {
        if t.ty.block().is_none() {
            return Err(LowerError::not_lowerable(format!("GGUF: `{}` is {} (not read)", t.name, t.ty.name())));
        }
        let mut f = std::fs::File::open(&self.path).map_err(|e| LowerError::Io(e.to_string()))?;
        f.seek(SeekFrom::Start(t.offset)).map_err(|e| LowerError::Io(e.to_string()))?;
        let mut b = vec![0u8; t.bytes as usize];
        f.read_exact(&mut b).map_err(|e| LowerError::weights(format!("GGUF `{}`: {e}", t.name)))?;
        Ok(b)
    }

    /// A tensor's stored integers (block-quantised matrices only).
    pub fn qweight(&self, name: &str) -> Result<QWeight> {
        let t = self.info(name)?;
        if t.ty.is_float() || t.dims.len() != 2 {
            return Err(LowerError::weights(format!("GGUF `{name}`: {} {:?} is not a quantised matrix", t.ty.name(), t.dims)));
        }
        let (inp, out) = (t.dims[0] as usize, t.dims[1] as usize);
        unpack(t.ty, &self.raw(t)?, inp, out, name)
    }

    /// A tensor as f32 in its Hugging Face shape: floats widened, quantised blocks dequantised
    /// (`W = scale · (q − zero) − min`, the exact value rounded once — llama.cpp's
    /// `dequantize_row_*`).
    pub fn tensor_f32(&self, name: &str) -> Result<Tensor> {
        let t = self.info(name)?;
        let raw = self.raw(t)?;
        let data: Vec<f32> = match t.ty {
            GgmlType::F32 => raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
            GgmlType::F16 => raw.chunks_exact(2).map(|c| f16_to_f32(u16::from_le_bytes([c[0], c[1]]))).collect(),
            GgmlType::BF16 => raw.chunks_exact(2).map(|c| f32::from_bits((u16::from_le_bytes([c[0], c[1]]) as u32) << 16)).collect(),
            ty => {
                let inp = t.dims[0] as usize;
                let rows = (t.numel() / t.dims[0]) as usize;
                return Ok(Tensor::new(t.shape(), unpack(ty, &raw, inp, rows, name)?.dequant().data));
            }
        };
        Ok(Tensor::new(t.shape(), data))
    }

    fn raw_range(&self, t: &GgufTensorInfo, start: u64, len: u64) -> Result<Vec<u8>> {
        if start.checked_add(len).is_none_or(|e| e > t.bytes) {
            return Err(LowerError::weights(format!("GGUF `{}`: a slice past the tensor", t.name)));
        }
        let mut f = std::fs::File::open(&self.path).map_err(|e| LowerError::Io(e.to_string()))?;
        f.seek(SeekFrom::Start(t.offset + start)).map_err(|e| LowerError::Io(e.to_string()))?;
        let mut b = vec![0u8; len as usize];
        f.read_exact(&mut b).map_err(|e| LowerError::weights(format!("GGUF `{}`: {e}", t.name)))?;
        Ok(b)
    }

    /// Expert `e` of a stacked `[ne0, ne1, E]` tensor: its `ne1` rows, and their byte range.
    fn expert_rows(&self, name: &str, e: usize) -> Result<(&GgufTensorInfo, usize, usize, u64, u64)> {
        let t = self.info(name)?;
        if t.dims.len() != 3 || e as u64 >= t.dims[2] {
            return Err(LowerError::weights(format!("GGUF `{name}` {:?} has no expert {e}", t.dims)));
        }
        let (be, bb) = t.ty.block().ok_or_else(|| LowerError::not_lowerable(format!("GGUF `{name}`: {} is not read", t.ty.name())))?;
        let (inp, rows) = (t.dims[0] as usize, t.dims[1] as usize);
        let row_bytes = (inp / be * bb) as u64;
        Ok((t, inp, rows, e as u64 * rows as u64 * row_bytes, rows as u64 * row_bytes))
    }

    /// Expert `e` of a stacked block-quantised tensor, as stored.
    pub fn qweight_expert(&self, name: &str, e: usize) -> Result<QWeight> {
        let (t, inp, rows, start, len) = self.expert_rows(name, e)?;
        if t.ty.is_float() {
            return Err(LowerError::weights(format!("GGUF `{name}` is {}, not quantised", t.ty.name())));
        }
        unpack(t.ty, &self.raw_range(t, start, len)?, inp, rows, name)
    }

    /// Expert `e` of a stacked tensor as f32 `[rows, cols]`.
    pub fn tensor_f32_expert(&self, name: &str, e: usize) -> Result<Tensor> {
        let (t, inp, rows, start, len) = self.expert_rows(name, e)?;
        let raw = self.raw_range(t, start, len)?;
        let data: Vec<f32> = match t.ty {
            GgmlType::F32 => raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
            GgmlType::F16 => raw.chunks_exact(2).map(|c| f16_to_f32(u16::from_le_bytes([c[0], c[1]]))).collect(),
            GgmlType::BF16 => raw.chunks_exact(2).map(|c| f32::from_bits((u16::from_le_bytes([c[0], c[1]]) as u32) << 16)).collect(),
            ty => unpack(ty, &raw, inp, rows, name)?.dequant().data,
        };
        Ok(Tensor::new(vec![rows, inp], data))
    }

    /// Tensor count by type (for reports).
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for t in self.tensors.values() {
            *m.entry(t.ty.name()).or_default() += 1;
        }
        m
    }
}

fn f16_at(b: &[u8], i: usize) -> f64 {
    f16_to_f32(u16::from_le_bytes([b[i], b[i + 1]])) as f64
}

/// `get_scale_min_k4`: the 6-bit scale and minimum of sub-block `j` of a K-quant block.
fn scale_min_k4(j: usize, q: &[u8]) -> (u8, u8) {
    if j < 4 {
        (q[j] & 63, q[j + 4] & 63)
    } else {
        ((q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4), (q[j + 4] >> 4) | ((q[j] >> 6) << 4))
    }
}

/// Unpack `out` rows of `inp` values of a block-quantised type into its stored integers.
fn unpack(ty: GgmlType, raw: &[u8], inp: usize, out: usize, name: &str) -> Result<QWeight> {
    let (be, bb) = ty.block().ok_or_else(|| LowerError::not_lowerable(format!("GGUF `{name}`: {} is not read", ty.name())))?;
    if ty.is_float() || inp % be != 0 || raw.len() != out * (inp / be) * bb {
        return Err(LowerError::weights(format!("GGUF `{name}`: {} bytes for {out} rows of {inp} {}", raw.len(), ty.name())));
    }
    let group = ty.group();
    let (nb, gpb, ng) = (inp / be, be / group, inp / group);
    let mut q = vec![0i16; out * inp];
    let mut scale = vec![0f64; out * ng];
    let mut zero = vec![0i16; out * ng];
    let mut min = if ty.has_min() { Some(vec![0f64; out * ng]) } else { None };
    for o in 0..out {
        for b in 0..nb {
            let blk = &raw[(o * nb + b) * bb..(o * nb + b + 1) * bb];
            let qo = o * inp + b * be;
            let go = o * ng + b * gpb;
            let qs = &mut q[qo..qo + be];
            match ty {
                GgmlType::Q8_0 => {
                    scale[go] = f16_at(blk, 0);
                    for j in 0..32 {
                        qs[j] = blk[2 + j] as i8 as i16;
                    }
                }
                GgmlType::Q4_0 | GgmlType::Q4_1 => {
                    let at = if ty == GgmlType::Q4_1 { 4 } else { 2 };
                    scale[go] = f16_at(blk, 0);
                    if let Some(m) = min.as_mut() {
                        m[go] = -f16_at(blk, 2);
                    } else {
                        zero[go] = 8;
                    }
                    for j in 0..16 {
                        qs[j] = (blk[at + j] & 0xF) as i16;
                        qs[j + 16] = (blk[at + j] >> 4) as i16;
                    }
                }
                GgmlType::Q5_0 | GgmlType::Q5_1 => {
                    let at = if ty == GgmlType::Q5_1 { 4 } else { 2 };
                    scale[go] = f16_at(blk, 0);
                    if let Some(m) = min.as_mut() {
                        m[go] = -f16_at(blk, 2);
                    } else {
                        zero[go] = 16;
                    }
                    let qh = u32::from_le_bytes([blk[at], blk[at + 1], blk[at + 2], blk[at + 3]]);
                    for j in 0..16 {
                        qs[j] = ((blk[at + 4 + j] & 0xF) as u32 | (((qh >> j) & 1) << 4)) as i16;
                        qs[j + 16] = ((blk[at + 4 + j] >> 4) as u32 | (((qh >> (j + 16)) & 1) << 4)) as i16;
                    }
                }
                GgmlType::Q4_K | GgmlType::Q5_K => {
                    let (d, dmin) = (f16_at(blk, 0), f16_at(blk, 2));
                    let sc = &blk[4..16];
                    let (qh, ql): (Option<&[u8]>, &[u8]) =
                        if ty == GgmlType::Q5_K { (Some(&blk[16..48]), &blk[48..176]) } else { (None, &blk[16..144]) };
                    let m = min.as_mut().expect("K-quants carry minimums");
                    for c in 0..4 {
                        for h in 0..2 {
                            let s = 2 * c + h;
                            let (scv, mv) = scale_min_k4(s, sc);
                            scale[go + s] = d * scv as f64;
                            m[go + s] = dmin * mv as f64;
                            for l in 0..32 {
                                let nib = if h == 0 { ql[32 * c + l] & 0xF } else { ql[32 * c + l] >> 4 };
                                let hi = qh.map_or(0, |qh| ((qh[l] >> s) & 1) << 4);
                                qs[64 * c + 32 * h + l] = (nib | hi) as i16;
                            }
                        }
                    }
                }
                GgmlType::Q6_K => {
                    let (ql, qh, sc) = (&blk[0..128], &blk[128..192], &blk[192..208]);
                    let d = f16_at(blk, 208);
                    for k in 0..16 {
                        scale[go + k] = d * (sc[k] as i8) as f64;
                        zero[go + k] = 32;
                    }
                    for n in 0..2 {
                        let (l0, h0) = (64 * n, 32 * n);
                        for l in 0..32 {
                            let h = qh[h0 + l];
                            qs[128 * n + l] = ((ql[l0 + l] & 0xF) | ((h & 3) << 4)) as i16;
                            qs[128 * n + l + 32] = ((ql[l0 + l + 32] & 0xF) | (((h >> 2) & 3) << 4)) as i16;
                            qs[128 * n + l + 64] = ((ql[l0 + l] >> 4) | (((h >> 4) & 3) << 4)) as i16;
                            qs[128 * n + l + 96] = ((ql[l0 + l + 32] >> 4) | (((h >> 6) & 3) << 4)) as i16;
                        }
                    }
                }
                _ => unreachable!("float types are refused above"),
            }
        }
    }
    let gidx = (0..inp).map(|i| (i / group) as u32).collect();
    Ok(QWeight {
        out,
        inp,
        group,
        q,
        scale,
        zero,
        min,
        gidx,
        bits: ty.bits(),
        signed: ty == GgmlType::Q8_0,
        label: format!("gguf {}", ty.name()),
    })
}

// ───────────────────────────── the Hugging Face view ─────────────────────────────

/// Where a Hugging Face tensor comes from in the file.
#[derive(Clone, Debug)]
struct Source {
    gguf: String,
    /// Hugging Face row `i` is GGUF row `rows[i]` (llama's q/k, Qwen3.5's tiled value heads).
    rows: Option<Vec<usize>>,
    /// Hugging Face column `j` is GGUF column `cols[j]` (Qwen3.5's `out_proj` over tiled heads).
    cols: Option<Vec<usize>>,
    /// Expert `e` of a stacked `[E, rows, cols]` tensor (`ffn_*_exps`).
    expert: Option<usize>,
}

/// A GGUF checkpoint seen as the Hugging Face model it was converted from.
pub struct GgufModel {
    pub file: GgufFile,
    pub arch: String,
    /// The Hugging Face `config.json` of the model (`hf_config::parse_config` reads it).
    pub config: Value,
    map: BTreeMap<String, Source>,
    /// Every GGUF tensor the view consumed (the rest is reported as unread).
    consumed: std::collections::BTreeSet<String>,
}

/// `llama`'s q/k rows: llama.cpp's converter permutes Hugging Face's `[heads, 2, d/2]` rows to
/// `[heads, d/2, 2]` (its interleaved rotary); Hugging Face row `i` is GGUF row `perm[i]`.
fn unpermute(heads: usize, rows: usize) -> Vec<usize> {
    let hd = rows / heads;
    (0..rows)
        .map(|i| {
            let (h, r) = (i / hd, i % hd);
            let (t, j) = (r / (hd / 2), r % (hd / 2));
            h * hd + 2 * j + t
        })
        .collect()
}

/// Qwen3.5's value heads: Hugging Face groups them by key head `[k][v][d]`, llama.cpp tiles them
/// `[v][k][d]` (`_reorder_v_heads`). Hugging Face index `i` of a `[nk · r · d]` axis is GGUF index
/// `untile[i]`.
fn untile(nk: usize, r: usize, d: usize) -> Vec<usize> {
    (0..nk * r * d)
        .map(|i| {
            let (k, rest) = (i / (r * d), i % (r * d));
            let (v, e) = (rest / d, rest % d);
            v * nk * d + k * d + e
        })
        .collect()
}

/// `{arch}.*` metadata keys each mapping reads; any other `{arch}.*` key is a refusal (it may
/// change the math). `general.*`, `tokenizer.*` and `quantize.*` are provenance.
const COMMON_KEYS: &[&str] = &[
    "context_length",
    "embedding_length",
    "block_count",
    "feed_forward_length",
    "attention.head_count",
    "attention.head_count_kv",
    "attention.layer_norm_rms_epsilon",
    "attention.key_length",
    "attention.value_length",
    "rope.freq_base",
    "rope.dimension_count",
    "rope.scaling.type",
    "rope.scaling.factor",
    "rope.scaling.original_context_length",
    "vocab_size",
];

fn arch_keys(arch: &str) -> &'static [&'static str] {
    match arch {
        "gemma2" => &["attn_logit_softcapping", "final_logit_softcapping", "attention.sliding_window"],
        "gemma3" => &["final_logit_softcapping", "attention.sliding_window"],
        "phi3" => &["attention.sliding_window", "rope.scaling.attn_factor"],
        "llama" | "qwen3moe" | "qwen2moe" => &[
            "expert_count",
            "expert_used_count",
            "expert_feed_forward_length",
            "expert_shared_feed_forward_length",
            "expert_gating_func",
            "expert_weights_scale",
            "expert_weights_norm",
        ],
        "qwen35" => &[
            "rope.dimension_sections",
            "ssm.conv_kernel",
            "ssm.state_size",
            "ssm.group_count",
            "ssm.time_step_rank",
            "ssm.inner_size",
            "full_attention_interval",
        ],
        _ => &[],
    }
}

/// Llama-3's rotary frequency factors (`rope_freqs.weight`) are stored as a tensor; a factor set
/// maps back to its `rope_scaling` only when it is one of these (checked value by value).
const LLAMA3_ROPES: &[(f64, f64, f64, u64)] = &[(8.0, 1.0, 4.0, 8192), (32.0, 1.0, 4.0, 8192), (16.0, 1.0, 4.0, 8192)];

fn llama3_factors(dim: usize, theta: f64, (factor, lo, hi, orig): (f64, f64, f64, u64)) -> Vec<f32> {
    let (low_wl, high_wl) = (orig as f64 / lo, orig as f64 / hi);
    (0..dim / 2)
        .map(|i| {
            let freq = 1.0 / theta.powf((2 * i) as f64 / dim as f64);
            let wl = 2.0 * std::f64::consts::PI / freq;
            let f = if wl < high_wl {
                1.0
            } else if wl > low_wl {
                factor
            } else {
                let smooth = (orig as f64 / wl - lo) / (hi - lo);
                1.0 / ((1.0 - smooth) / factor + smooth)
            };
            f as f32
        })
        .collect()
}

impl GgufModel {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_file(GgufFile::open(path)?)
    }

    pub fn from_file(file: GgufFile) -> Result<Self> {
        let arch = file.meta.get("general.architecture").and_then(GValue::as_str).unwrap_or("").to_string();
        let (hf_arch, model_type) = match arch.as_str() {
            "llama" => ("LlamaForCausalLM", "llama"),
            "qwen2" => ("Qwen2ForCausalLM", "qwen2"),
            "qwen3" => ("Qwen3ForCausalLM", "qwen3"),
            "gemma" => ("GemmaForCausalLM", "gemma"),
            "gemma2" => ("Gemma2ForCausalLM", "gemma2"),
            "gemma3" => ("Gemma3ForCausalLM", "gemma3_text"),
            "phi3" => ("Phi3ForCausalLM", "phi3"),
            "qwen35" => ("Qwen3_5ForCausalLM", "qwen3_5_text"),
            "qwen3moe" => ("Qwen3MoeForCausalLM", "qwen3_moe"),
            "qwen2moe" => ("Qwen2MoeForCausalLM", "qwen2_moe"),
            other => {
                return Err(LowerError::not_lowerable(format!(
                    "GGUF architecture `{other}` has no mapping (llama, qwen2, qwen3, qwen35, qwen2moe, qwen3moe, gemma, gemma2, gemma3 and phi3 do)"
                )));
            }
        };
        if file.meta.get("split.count").and_then(GValue::as_u64).is_some_and(|n| n > 1) {
            return Err(LowerError::not_lowerable("a split GGUF (merge the shards first)"));
        }
        let pre = format!("{arch}.");
        for k in file.meta.keys() {
            if let Some(rest) = k.strip_prefix(&pre)
                && !COMMON_KEYS.contains(&rest)
                && !arch_keys(&arch).contains(&rest)
            {
                return Err(LowerError::not_lowerable(format!("GGUF metadata `{k}` is not mapped (it may change the math)")));
            }
        }
        let get = |k: &str| file.meta.get(&format!("{pre}{k}"));
        let need_u = |k: &str| -> Result<usize> {
            get(k).and_then(GValue::as_u64).map(|v| v as usize).ok_or_else(|| LowerError::bad(format!("GGUF: no `{pre}{k}`")))
        };
        let experts = get("expert_count").and_then(GValue::as_u64).unwrap_or(0) as usize;
        let (hf_arch, model_type) = if arch == "llama" && experts > 0 { ("MixtralForCausalLM", "mixtral") } else { (hf_arch, model_type) };
        if experts > 0 {
            if get("expert_gating_func").and_then(GValue::as_u64).is_some_and(|g| g != 1) {
                return Err(LowerError::not_lowerable("GGUF: an expert gating function other than softmax"));
            }
            if get("expert_weights_scale").and_then(GValue::as_f64).is_some_and(|w| w != 1.0) {
                return Err(LowerError::not_lowerable("GGUF: scaled expert weights"));
            }
        }
        let hidden = need_u("embedding_length")?;
        let layers = need_u("block_count")?;
        let ffn = need_u("feed_forward_length")?;
        let heads = need_u("attention.head_count")?;
        let kv = get("attention.head_count_kv").and_then(GValue::as_u64).map_or(heads, |v| v as usize);
        let head_dim = get("attention.key_length").and_then(GValue::as_u64).map_or(hidden / heads.max(1), |v| v as usize);
        if get("attention.value_length").and_then(GValue::as_u64).is_some_and(|v| v as usize != head_dim) {
            return Err(LowerError::not_lowerable("GGUF: value heads of another size than the key heads"));
        }
        let rope_dim = get("rope.dimension_count").and_then(GValue::as_u64).map_or(head_dim, |v| v as usize);
        let partial = rope_dim as f64 / head_dim as f64;
        if rope_dim != head_dim && !matches!(arch.as_str(), "phi3" | "qwen35") {
            return Err(LowerError::not_lowerable("GGUF: partial rotary for this architecture"));
        }
        let eps = get("attention.layer_norm_rms_epsilon").and_then(GValue::as_f64).ok_or_else(|| LowerError::bad("GGUF: no RMS epsilon"))?;
        let theta = get("rope.freq_base").and_then(GValue::as_f64).unwrap_or(10000.0);
        let ctx = get("context_length").and_then(GValue::as_u64).unwrap_or(2048);
        let embd = file.tensors.get("token_embd.weight").ok_or_else(|| LowerError::weights("GGUF: no token_embd.weight"))?;
        let vocab = match get("vocab_size").and_then(GValue::as_u64) {
            Some(v) => v as usize,
            None => embd.dims.get(1).copied().unwrap_or(0) as usize,
        };
        let has = |n: &str| file.tensors.contains_key(n);
        let tied = !has("output.weight");
        let mut cfg = json!({
            "architectures": [hf_arch],
            "model_type": model_type,
            "hidden_size": hidden,
            "intermediate_size": ffn,
            "num_hidden_layers": layers,
            "num_attention_heads": heads,
            "num_key_value_heads": kv,
            "head_dim": head_dim,
            "rms_norm_eps": eps,
            "max_position_embeddings": ctx,
            "vocab_size": vocab,
            "tie_word_embeddings": tied,
        });
        let mut consumed = std::collections::BTreeSet::new();
        let o = cfg.as_object_mut().expect("object");
        let rope_scaling = |o: &mut serde_json::Map<String, Value>| -> Result<()> {
            match get("rope.scaling.type").and_then(GValue::as_str) {
                None | Some("none") => Ok(()),
                Some("linear") => {
                    let f = get("rope.scaling.factor").and_then(GValue::as_f64).ok_or_else(|| LowerError::bad("GGUF: linear rope without a factor"))?;
                    o.insert("rope_scaling".into(), json!({"rope_type": "linear", "factor": f}));
                    Ok(())
                }
                Some(t) => Err(LowerError::not_lowerable(format!("GGUF rope scaling `{t}` is not mapped"))),
            }
        };
        match arch.as_str() {
            "llama" if experts > 0 => {
                // Mixtral: every layer's feed-forward is the experts (`feed_forward_length` is
                // theirs); llama.cpp renormalises the top-k weights, as Mixtral does.
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_act".into(), json!("silu"));
                o.insert("num_local_experts".into(), json!(experts));
                o.insert("num_experts_per_tok".into(), json!(need_u("expert_used_count")?));
                o.insert("sliding_window".into(), Value::Null);
                rope_scaling(o)?;
            }
            "qwen3moe" | "qwen2moe" => {
                // llama.cpp: every layer is sparse; Qwen3-MoE renormalises the top-k weights and
                // Qwen2-MoE does not (its shared expert has a sigmoid gate).
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_act".into(), json!("silu"));
                o.insert("num_experts".into(), json!(experts));
                o.insert("num_experts_per_tok".into(), json!(need_u("expert_used_count")?));
                o.insert("moe_intermediate_size".into(), json!(need_u("expert_feed_forward_length")?));
                o.insert("decoder_sparse_step".into(), json!(1));
                o.insert("mlp_only_layers".into(), json!(Vec::<usize>::new()));
                let v3 = arch == "qwen3moe";
                o.insert("norm_topk_prob".into(), json!(v3));
                if let Some(nw) = get("expert_weights_norm").and_then(|v| if let GValue::Bool(b) = v { Some(*b) } else { None })
                    && nw != v3
                {
                    return Err(LowerError::not_lowerable("GGUF: expert weight normalisation differs from llama.cpp's for this architecture"));
                }
                if v3 {
                    o.insert("attention_bias".into(), json!(has("blk.0.attn_q.bias")));
                } else {
                    o.insert("shared_expert_intermediate_size".into(), json!(need_u("expert_shared_feed_forward_length")?));
                }
                rope_scaling(o)?;
            }
            "llama" | "qwen3" => {
                o.insert("rope_theta".into(), json!(theta));
                o.insert("attention_bias".into(), json!(has("blk.0.attn_q.bias")));
                if arch == "llama" {
                    o.insert("mlp_bias".into(), json!(false));
                }
                o.insert("hidden_act".into(), json!("silu"));
                rope_scaling(o)?;
            }
            "qwen2" => {
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_act".into(), json!("silu"));
                rope_scaling(o)?;
            }
            "gemma" => {
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_activation".into(), json!("gelu_pytorch_tanh"));
                rope_scaling(o)?;
            }
            "gemma2" => {
                // llama.cpp scales the queries by 1/√head_dim, except for the 27B (46 layers,
                // 1/√(hidden/heads)) — the GGUF carries no query_pre_attn_scalar.
                let qpas = if layers == 46 { hidden / heads } else { head_dim };
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_activation".into(), json!("gelu_pytorch_tanh"));
                o.insert("query_pre_attn_scalar".into(), json!(qpas));
                for k in ["attn_logit_softcapping", "final_logit_softcapping"] {
                    if let Some(v) = get(k).and_then(GValue::as_f64) {
                        o.insert(k.into(), json!(v));
                    }
                }
                let sw = get("attention.sliding_window").and_then(GValue::as_u64).ok_or_else(|| LowerError::bad("GGUF gemma2: no sliding window"))?;
                o.insert("sliding_window".into(), json!(sw));
                rope_scaling(o)?;
            }
            "gemma3" => {
                // llama.cpp: every 6th layer global (`set_swa_pattern(6)`), local layers at base
                // 10,000 unscaled, global ones at `rope.freq_base` with the file's scaling; the
                // 27B (62 layers) scales queries by 1/√(hidden/heads).
                let qpas = if layers == 62 { hidden / heads } else { head_dim };
                o.insert("hidden_activation".into(), json!("gelu_pytorch_tanh"));
                o.insert("query_pre_attn_scalar".into(), json!(qpas));
                if let Some(v) = get("final_logit_softcapping").and_then(GValue::as_f64) {
                    o.insert("final_logit_softcapping".into(), json!(v));
                }
                match get("attention.sliding_window").and_then(GValue::as_u64) {
                    Some(sw) => {
                        o.insert("sliding_window".into(), json!(sw));
                        let types: Vec<&str> =
                            (0..layers).map(|i| if (i + 1) % 6 != 0 { "sliding_attention" } else { "full_attention" }).collect();
                        o.insert("layer_types".into(), json!(types));
                    }
                    None => {
                        o.insert("sliding_window".into(), Value::Null);
                        o.insert("layer_types".into(), json!(vec!["full_attention"; layers]));
                    }
                }
                let mut full = serde_json::Map::new();
                full.insert("rope_theta".into(), json!(theta));
                match get("rope.scaling.type").and_then(GValue::as_str) {
                    None | Some("none") => {
                        full.insert("rope_type".into(), json!("default"));
                    }
                    Some("linear") => {
                        let f = get("rope.scaling.factor").and_then(GValue::as_f64).ok_or_else(|| LowerError::bad("GGUF: linear rope without a factor"))?;
                        full.insert("rope_type".into(), json!("linear"));
                        full.insert("factor".into(), json!(f));
                    }
                    Some(t) => return Err(LowerError::not_lowerable(format!("GGUF gemma3 rope scaling `{t}` is not mapped"))),
                }
                o.insert(
                    "rope_parameters".into(),
                    json!({"sliding_attention": {"rope_type": "default", "rope_theta": 10000.0}, "full_attention": Value::Object(full)}),
                );
            }
            "phi3" => {
                // transformers' Phi-3 derives the head size (hidden / heads) and has no key for it.
                if head_dim != hidden / heads.max(1) {
                    return Err(LowerError::not_lowerable("GGUF phi3: a head size other than hidden / heads"));
                }
                o.remove("head_dim");
                o.insert("rope_theta".into(), json!(theta));
                o.insert("hidden_act".into(), json!("silu"));
                if rope_dim != head_dim {
                    o.insert("partial_rotary_factor".into(), json!(partial));
                }
                let sw = get("attention.sliding_window").and_then(GValue::as_u64).unwrap_or(0);
                o.insert("sliding_window".into(), if sw == 0 { Value::Null } else { json!(sw) });
                let orig = get("rope.scaling.original_context_length").and_then(GValue::as_u64).unwrap_or(ctx);
                o.insert("original_max_position_embeddings".into(), json!(orig));
                if let (Some(l), Some(s)) = (file.tensors.get("rope_factors_long.weight"), file.tensors.get("rope_factors_short.weight")) {
                    let long: Vec<f64> = file.tensor_f32(&l.name)?.data.iter().map(|v| *v as f64).collect();
                    let short: Vec<f64> = file.tensor_f32(&s.name)?.data.iter().map(|v| *v as f64).collect();
                    // The attention factor transformers derives from the context ratio must be the
                    // one the file carries.
                    let scale = ctx as f64 / orig as f64;
                    let want = if scale > 1.0 { (1.0 + scale.ln() / (orig as f64).ln()).sqrt() } else { 1.0 };
                    if let Some(af) = get("rope.scaling.attn_factor").and_then(GValue::as_f64)
                        && (af - want).abs() > 1e-6 * want
                    {
                        return Err(LowerError::not_lowerable(format!("GGUF phi3: attn_factor {af} is not LongRoPE's {want}")));
                    }
                    o.insert("rope_scaling".into(), json!({"rope_type": "longrope", "long_factor": long, "short_factor": short}));
                    consumed.insert(l.name.clone());
                    consumed.insert(s.name.clone());
                }
            }
            _ => {
                // qwen35: the hybrid gated-delta decoder.
                let interval = get("full_attention_interval").and_then(GValue::as_u64).unwrap_or(4) as usize;
                let (nk, nv) = (need_u("ssm.group_count")?, need_u("ssm.time_step_rank")?);
                let dk = need_u("ssm.state_size")?;
                let dv = need_u("ssm.inner_size")? / nv.max(1);
                let types: Vec<&str> =
                    (0..layers).map(|i| if (i + 1) % interval == 0 { "full_attention" } else { "linear_attention" }).collect();
                for (i, t) in types.iter().enumerate() {
                    let is_full = has(&format!("blk.{i}.attn_q.weight"));
                    if is_full != (*t == "full_attention") {
                        return Err(LowerError::not_lowerable(format!("GGUF qwen35: layer {i}'s tensors disagree with full_attention_interval {interval}")));
                    }
                }
                let sections: Vec<u64> = get("rope.dimension_sections")
                    .and_then(GValue::as_arr)
                    .map(|a| a.iter().filter_map(GValue::as_u64).collect())
                    .unwrap_or_else(|| vec![11, 11, 10, 0]);
                let sec: Vec<u64> = sections.iter().copied().take(3).collect();
                if sections.get(3).is_some_and(|s| *s != 0) {
                    return Err(LowerError::not_lowerable("GGUF qwen35: a fourth M-RoPE section"));
                }
                let gate_rows = file.tensors.iter().find(|(k, _)| k.ends_with(".attn_q.weight")).map(|(_, t)| t.dims[1] as usize);
                if gate_rows.is_some_and(|r| r != 2 * heads * head_dim) {
                    return Err(LowerError::not_lowerable("GGUF qwen35: attention without its output gate"));
                }
                o.insert("hidden_act".into(), json!("silu"));
                o.insert("attention_bias".into(), json!(false));
                o.insert("attn_output_gate".into(), json!(true));
                o.insert("full_attention_interval".into(), json!(interval));
                o.insert("layer_types".into(), json!(types));
                o.insert("linear_conv_kernel_dim".into(), json!(need_u("ssm.conv_kernel")?));
                o.insert("linear_key_head_dim".into(), json!(dk));
                o.insert("linear_value_head_dim".into(), json!(dv));
                o.insert("linear_num_key_heads".into(), json!(nk));
                o.insert("linear_num_value_heads".into(), json!(nv));
                o.insert(
                    "rope_parameters".into(),
                    json!({"mrope_interleaved": true, "mrope_section": sec, "rope_type": "default", "rope_theta": theta,
                           "partial_rotary_factor": partial}),
                );
            }
        }
        if let Some(rf) = file.tensors.get("rope_freqs.weight") {
            if arch != "llama" {
                return Err(LowerError::not_lowerable(format!("GGUF {arch}: rope_freqs.weight is not mapped")));
            }
            let got = file.tensor_f32(&rf.name)?.data;
            let hit = LLAMA3_ROPES.iter().find(|c| {
                let want = llama3_factors(head_dim, theta, **c);
                want.len() == got.len() && want.iter().zip(&got).all(|(a, b)| (a - b).abs() <= 1e-6 * a.abs().max(1.0))
            });
            let (factor, lo, hi, orig) =
                *hit.ok_or_else(|| LowerError::not_lowerable("GGUF rope_freqs.weight is not a Llama-3 factor set this mapping recognises"))?;
            o.insert(
                "rope_scaling".into(),
                json!({"rope_type": "llama3", "factor": factor, "low_freq_factor": lo, "high_freq_factor": hi,
                       "original_max_position_embeddings": orig}),
            );
            consumed.insert(rf.name.clone());
        }
        // Tensor names.
        let mut map: BTreeMap<String, Source> = BTreeMap::new();
        let mut map_expert: Vec<(String, String, usize)> = Vec::new();
        let mut put = |hf: String, g: String, rows: Option<Vec<usize>>, cols: Option<Vec<usize>>| {
            map.insert(hf, Source { gguf: g, rows, cols, expert: None });
        };
        put("model.embed_tokens.weight".into(), "token_embd.weight".into(), None, None);
        put("model.norm.weight".into(), "output_norm.weight".into(), None, None);
        if !tied {
            put("lm_head.weight".into(), "output.weight".into(), None, None);
        }
        let qwen35 = arch == "qwen35";
        let (nk, nv, dk, dv) = if qwen35 {
            let (nk, nv) = (need_u("ssm.group_count")?, need_u("ssm.time_step_rank")?);
            (nk, nv, need_u("ssm.state_size")?, need_u("ssm.inner_size")? / nv.max(1))
        } else {
            (0, 0, 0, 0)
        };
        // Qwen3.5 with more value heads than key heads: the value-head axis back to grouped order.
        let r = if qwen35 && nk > 0 { nv / nk } else { 1 };
        let tiled = qwen35 && r > 1;
        let head_axis = |d: usize| if tiled { Some(untile(nk, r, d)) } else { None };
        let qkv_rows = |extra: usize| {
            head_axis(dv).map(|v| {
                let qk = 2 * nk * dk;
                let mut p: Vec<usize> = (0..qk).collect();
                p.extend(v.iter().map(|i| qk + i));
                let _ = extra;
                p
            })
        };
        for l in 0..layers {
            let (b, m) = (format!("blk.{l}."), format!("model.layers.{l}."));
            let norm2 = match arch.as_str() {
                "gemma2" | "gemma3" | "qwen35" => "post_attention_norm",
                _ => "ffn_norm",
            };
            put(format!("{m}input_layernorm.weight"), format!("{b}attn_norm.weight"), None, None);
            put(format!("{m}post_attention_layernorm.weight"), format!("{b}{norm2}.weight"), None, None);
            if matches!(arch.as_str(), "gemma2" | "gemma3") {
                put(format!("{m}pre_feedforward_layernorm.weight"), format!("{b}ffn_norm.weight"), None, None);
                put(format!("{m}post_feedforward_layernorm.weight"), format!("{b}post_ffw_norm.weight"), None, None);
            }
            if arch == "phi3" {
                put(format!("{m}self_attn.qkv_proj.weight"), format!("{b}attn_qkv.weight"), None, None);
                put(format!("{m}self_attn.o_proj.weight"), format!("{b}attn_output.weight"), None, None);
                put(format!("{m}mlp.gate_up_proj.weight"), format!("{b}ffn_up.weight"), None, None);
                put(format!("{m}mlp.down_proj.weight"), format!("{b}ffn_down.weight"), None, None);
                continue;
            }
            for (hfp, gp, n) in [("q_proj", "attn_q", heads), ("k_proj", "attn_k", kv), ("v_proj", "attn_v", 0), ("o_proj", "attn_output", 0)] {
                for suffix in ["weight", "bias"] {
                    let g = format!("{b}{gp}.{suffix}");
                    if let Some(t) = file.tensors.get(&g) {
                        let rows = *t.shape().first().unwrap_or(&0);
                        let perm = (arch == "llama" && n > 0).then(|| unpermute(n, rows));
                        put(format!("{m}self_attn.{hfp}.{suffix}"), g, perm, None);
                    }
                }
            }
            if has(&format!("{b}attn_q_norm.weight")) {
                put(format!("{m}self_attn.q_norm.weight"), format!("{b}attn_q_norm.weight"), None, None);
                put(format!("{m}self_attn.k_norm.weight"), format!("{b}attn_k_norm.weight"), None, None);
            }
            if qwen35 && has(&format!("{b}attn_qkv.weight")) {
                let la = format!("{m}linear_attn.");
                put(format!("{la}in_proj_qkv.weight"), format!("{b}attn_qkv.weight"), qkv_rows(0), None);
                put(format!("{la}in_proj_z.weight"), format!("{b}attn_gate.weight"), head_axis(dv), None);
                put(format!("{la}in_proj_a.weight"), format!("{b}ssm_alpha.weight"), head_axis(1), None);
                put(format!("{la}in_proj_b.weight"), format!("{b}ssm_beta.weight"), head_axis(1), None);
                put(format!("{la}conv1d.weight"), format!("{b}ssm_conv1d.weight"), qkv_rows(0), None);
                // The file stores −exp(A_log); the binding reads it as is (`fix_binding`).
                put(format!("{la}A_log.neg_exp"), format!("{b}ssm_a"), head_axis(1), None);
                put(format!("{la}dt_bias"), format!("{b}ssm_dt.bias"), head_axis(1), None);
                put(format!("{la}norm.weight"), format!("{b}ssm_norm.weight"), None, None);
                put(format!("{la}out_proj.weight"), format!("{b}ssm_out.weight"), None, head_axis(dv));
            }
            if experts > 0 {
                let (moe, names): (&str, [&str; 3]) = if arch == "llama" {
                    ("block_sparse_moe", ["w1", "w3", "w2"])
                } else {
                    ("mlp", ["gate_proj", "up_proj", "down_proj"])
                };
                put(format!("{m}{moe}.gate.weight"), format!("{b}ffn_gate_inp.weight"), None, None);
                for (hfp, gp) in names.iter().zip(["ffn_gate_exps", "ffn_up_exps", "ffn_down_exps"]) {
                    for e in 0..experts {
                        map_expert.push((format!("{m}{moe}.experts.{e}.{hfp}.weight"), format!("{b}{gp}.weight"), e));
                    }
                }
                if arch == "qwen2moe" {
                    for (hfp, gp) in [("gate_proj", "ffn_gate_shexp"), ("up_proj", "ffn_up_shexp"), ("down_proj", "ffn_down_shexp")] {
                        put(format!("{m}mlp.shared_expert.{hfp}.weight"), format!("{b}{gp}.weight"), None, None);
                    }
                    put(format!("{m}mlp.shared_expert_gate.weight"), format!("{b}ffn_gate_inp_shexp.weight"), None, None);
                }
                continue;
            }
            for (hfp, gp) in [("gate_proj", "ffn_gate"), ("up_proj", "ffn_up"), ("down_proj", "ffn_down")] {
                put(format!("{m}mlp.{hfp}.weight"), format!("{b}{gp}.weight"), None, None);
            }
        }
        for (hf, g, e) in map_expert {
            map.insert(hf, Source { gguf: g, rows: None, cols: None, expert: Some(e) });
        }
        for (hf, s) in &map {
            if !file.tensors.contains_key(&s.gguf) {
                return Err(LowerError::weights(format!("GGUF: `{}` (for `{hf}`) is missing", s.gguf)));
            }
            consumed.insert(s.gguf.clone());
        }
        Ok(GgufModel { file, arch, config: cfg, map, consumed })
    }

    /// GGUF tensors this view does not map (reported as unread by the weight check).
    pub fn unmapped(&self) -> Vec<String> {
        self.file.tensors.keys().filter(|k| !self.consumed.contains(*k)).cloned().collect()
    }

    /// The quantisation this checkpoint's projections carry: for each Hugging Face module (a
    /// template over layers, `{L}`) whose tensors are block-quantised, the program layout over every
    /// layer's type (the finest group; an offset term if any layer's type has minimums; a column
    /// order when the columns are permuted). A module that is float in some layers and quantised in
    /// others is refused.
    pub fn quant_config(&self) -> Result<QuantConfig> {
        let mut per: BTreeMap<String, (Vec<GgmlType>, bool)> = BTreeMap::new();
        for (hf, s) in &self.map {
            let Some(module) = hf.strip_suffix(".weight") else { continue };
            let t = &self.file.tensors[&s.gguf];
            let dims = if s.expert.is_some() { t.dims.len() - 1 } else { t.dims.len() };
            if dims != 2 || module == "model.embed_tokens" || module.ends_with("conv1d") {
                continue;
            }
            let template = match module.strip_prefix("model.layers.") {
                Some(rest) => {
                    let (_, tail) = rest.split_once('.').unwrap_or(("", rest));
                    // One stored expert per module: `experts.{E}`.
                    let tail = match tail.split_once(".experts.") {
                        Some((a, b)) => match b.split_once('.') {
                            Some((n, c)) if n.bytes().all(|x| x.is_ascii_digit()) => format!("{a}.experts.{{E}}.{c}"),
                            _ => tail.to_string(),
                        },
                        None => tail.to_string(),
                    };
                    format!("model.layers.{{L}}.{tail}")
                }
                None => module.to_string(),
            };
            let e = per.entry(template).or_insert((Vec::new(), false));
            e.0.push(t.ty);
            e.1 |= s.cols.is_some();
        }
        let mut per_module = BTreeMap::new();
        for (m, (types, cols)) in per {
            let floats = types.iter().filter(|t| t.is_float()).count();
            if floats == types.len() {
                continue;
            }
            if floats > 0 {
                return Err(LowerError::not_lowerable(format!("GGUF: `{m}` is float in some layers and quantised in others")));
            }
            if let Some(t) = types.iter().find(|t| t.block().is_none()) {
                return Err(LowerError::not_lowerable(format!(
                    "GGUF: `{m}` is {} (Q8_0, Q4_0/1, Q5_0/1, Q4_K, Q5_K, Q6_K are read)",
                    t.name()
                )));
            }
            let group = types.iter().map(|t| t.group()).min().unwrap_or(32);
            let offset_term = types.iter().any(|t| t.has_min());
            per_module.insert(m, QLayout { group, order: cols, offset_term });
        }
        Ok(QuantConfig {
            fmt: QFormat::Gguf { layout: QLayout { group: 32, order: false, offset_term: false } },
            lm_head: false,
            skip: Vec::new(),
            only: None,
            per_module,
        })
    }

    /// The spec of this checkpoint: `config` through `hf_config`, then what GGUF stores
    /// differently — RMSNorm gains of the `1 + w` form stored as `1 + w` (Gemma, Qwen3.5: the spec
    /// multiplies by the stored value), and the quantised modules' layouts.
    pub fn spec(&self) -> Result<ArchSpec> {
        let mut spec = crate::parse_config(&self.config)?;
        if self.arch.starts_with("gemma") || self.arch == "qwen35" {
            let w = |n: &mut NormSpec| {
                if n.gain == Gain::OnePlusW {
                    n.gain = Gain::W;
                }
            };
            for ls in spec.layers.iter_mut() {
                match &mut ls.residual {
                    Residual::Sequential { pre_mixer, post_mixer, pre_ffn, post_ffn, .. } => {
                        for n in [pre_mixer, post_mixer, pre_ffn, post_ffn].into_iter().flatten() {
                            w(n);
                        }
                    }
                    Residual::Parallel { norm, ffn_norm } => {
                        w(norm);
                        if let Some(n) = ffn_norm {
                            w(n);
                        }
                    }
                    Residual::PostNorm { mixer_norm, ffn_norm } => {
                        w(mixer_norm);
                        w(ffn_norm);
                    }
                    Residual::Sandwich { pre_mixer, post_mixer, pre_ffn, post_ffn, .. } => {
                        for n in [pre_mixer, post_mixer, pre_ffn, post_ffn] {
                            w(n);
                        }
                    }
                    // No GGUF architecture stores hyper-connections.
                    Residual::HyperConnection { .. } => {}
                }
                if let crate::spec::Mixer::Attention(a) = &mut ls.mixer
                    && let Some(qk) = a.qk_norm.as_mut()
                {
                    w(&mut qk.norm);
                }
            }
            if let Some(n) = spec.final_norm.as_mut() {
                w(n);
            }
            spec.notes.push("GGUF: RMSNorm gains of the 1 + w form are stored as 1 + w and multiplied as stored".into());
        }
        let q = self.quant_config()?;
        if !q.per_module.is_empty() {
            crate::hf_config::attach_quant(&mut spec, q)?;
        }
        Ok(spec)
    }

    /// Bind the params as this file stores them: Qwen3.5's `A = −exp(A_log)` is stored as `A`, so
    /// the param reads it directly rather than through `−exp(log(−A))`.
    pub fn fix_binding(&self, binding: &mut crate::weights::Binding) {
        use crate::weights::{MapFn, Src};
        for s in binding.srcs.iter_mut() {
            if let Src::Map { src, f: MapFn::NegExp } = s
                && let Src::Tensor(t) = src.as_ref()
                && t.ends_with(".A_log")
            {
                *s = Src::Tensor(format!("{t}.neg_exp"));
            }
        }
    }

    /// `fidelity::prepare` for this file: its spec, lowered, and its binding fixed.
    pub fn prepare(&self, opts: &crate::lower::LowerOpts) -> Result<crate::fidelity::Prepared> {
        let mut prep = crate::fidelity::prepare_spec(self.spec()?, opts)?;
        self.fix_binding(&mut prep.binding);
        Ok(prep)
    }
}

fn permute_rows_cols(t: Tensor, rows: Option<&[usize]>, cols: Option<&[usize]>) -> Tensor {
    let r = t.shape.first().copied().unwrap_or(1);
    let c: usize = t.shape[1..].iter().product();
    let mut data = Vec::with_capacity(t.data.len());
    for i in 0..r {
        let src = rows.map_or(i, |p| p[i]);
        let row = &t.data[src * c..(src + 1) * c];
        match cols {
            None => data.extend_from_slice(row),
            Some(p) => data.extend(p.iter().map(|j| row[*j])),
        }
    }
    Tensor::new(t.shape, data)
}

impl TensorSource for GgufModel {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        let s = self.map.get(name)?;
        let sh = self.file.tensors.get(&s.gguf).map(GgufTensorInfo::shape)?;
        Some(if s.expert.is_some() { sh[1..].to_vec() } else { sh })
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let s = self.map.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let t = match s.expert {
            Some(e) => self.file.tensor_f32_expert(&s.gguf, e)?,
            None => self.file.tensor_f32(&s.gguf)?,
        };
        Ok(if s.rows.is_none() && s.cols.is_none() { t } else { permute_rows_cols(t, s.rows.as_deref(), s.cols.as_deref()) })
    }
    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.map.keys().cloned().collect();
        v.extend(self.unmapped());
        v
    }
    fn load_qweight(&self, name: &str) -> Result<QWeight> {
        let s = self.map.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let mut q = match s.expert {
            Some(e) => self.file.qweight_expert(&s.gguf, e)?,
            None => self.file.qweight(&s.gguf)?,
        };
        if let Some(p) = &s.rows {
            q = q.take_rows(p)?;
        }
        if let Some(p) = &s.cols {
            q = q.permute_cols(p)?;
        }
        Ok(q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_rows_come_back_to_hugging_face_order() {
        // 2 heads of 4: HF rows (h, t, j) ← GGUF rows (h, j, t).
        assert_eq!(unpermute(2, 8), vec![0, 2, 1, 3, 4, 6, 5, 7]);
    }

    #[test]
    fn k_quant_scales_and_minimums_unpack() {
        // Sub-blocks 0..3 read the low 6 bits of bytes 0..3 and 4..7; 4..7 combine nibbles of
        // bytes 8..11 with the top 2 bits of 0..3 / 4..7.
        let q = [0b1100_0001u8, 2, 3, 4, 0b0100_0101, 6, 7, 8, 0x9A, 0xBC, 0xDE, 0xF0];
        assert_eq!(scale_min_k4(0, &q), (1, 5));
        assert_eq!(scale_min_k4(4, &q), ((0x9A & 0xF) | (3 << 4), (0x9A >> 4) | (1 << 4)));
    }

    #[test]
    fn f32_metadata_reads_back_as_the_config_decimal() {
        assert_eq!(GValue::F32(1e-6).as_f64(), Some(1e-6));
        assert_eq!(GValue::F32(1e-5).as_f64(), Some(1e-5));
        assert_eq!(GValue::F32(500000.0).as_f64(), Some(500000.0));
    }
}
