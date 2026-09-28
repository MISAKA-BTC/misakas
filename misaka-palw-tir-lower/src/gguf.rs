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

/// A GGUF checkpoint seen as the Hugging Face model it was converted from.
pub struct GgufModel {
    pub file: GgufFile,
    pub arch: String,
    /// The Hugging Face `config.json` of the model (`hf_config::parse_config` reads it).
    pub config: Value,
    /// Hugging Face tensor name → (GGUF tensor name, the Hugging Face rows' order in it).
    map: BTreeMap<String, (String, Option<Vec<usize>>)>,
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

/// `{arch}.*` metadata keys each architecture's mapping reads; any other `{arch}.*` key is a
/// refusal (it may change the math). `general.*`, `tokenizer.*` and `quantize.*` are provenance.
const ARCH_KEYS: &[&str] = &[
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
    "attn_logit_softcapping",
    "final_logit_softcapping",
    "attention.sliding_window",
];

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
            other => {
                return Err(LowerError::not_lowerable(format!(
                    "GGUF architecture `{other}` has no mapping (llama, qwen2, qwen3, gemma and gemma2 do)"
                )));
            }
        };
        if file.meta.get("split.count").and_then(GValue::as_u64).is_some_and(|n| n > 1) {
            return Err(LowerError::not_lowerable("a split GGUF (merge the shards first)"));
        }
        let pre = format!("{arch}.");
        for k in file.meta.keys() {
            if let Some(rest) = k.strip_prefix(&pre)
                && !ARCH_KEYS.contains(&rest)
            {
                return Err(LowerError::not_lowerable(format!("GGUF metadata `{k}` is not mapped (it may change the math)")));
            }
        }
        let get = |k: &str| file.meta.get(&format!("{pre}{k}"));
        let need_u = |k: &str| -> Result<usize> {
            get(k).and_then(GValue::as_u64).map(|v| v as usize).ok_or_else(|| LowerError::bad(format!("GGUF: no `{pre}{k}`")))
        };
        let hidden = need_u("embedding_length")?;
        let layers = need_u("block_count")?;
        let ffn = need_u("feed_forward_length")?;
        let heads = need_u("attention.head_count")?;
        let kv = get("attention.head_count_kv").and_then(GValue::as_u64).map_or(heads, |v| v as usize);
        let head_dim = get("attention.key_length").and_then(GValue::as_u64).map_or(hidden / heads.max(1), |v| v as usize);
        if get("attention.value_length").and_then(GValue::as_u64).is_some_and(|v| v as usize != head_dim) {
            return Err(LowerError::not_lowerable("GGUF: value heads of another size than the key heads"));
        }
        if get("rope.dimension_count").and_then(GValue::as_u64).is_some_and(|v| v as usize != head_dim) {
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
        let has = |n: String| file.tensors.contains_key(&n);
        let tied = !has("output.weight".into());
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
            "rope_theta": theta,
            "max_position_embeddings": ctx,
            "vocab_size": vocab,
            "tie_word_embeddings": tied,
        });
        let o = cfg.as_object_mut().expect("object");
        match arch.as_str() {
            "llama" => {
                o.insert("attention_bias".into(), json!(has("blk.0.attn_q.bias".into())));
                o.insert("mlp_bias".into(), json!(false));
                o.insert("hidden_act".into(), json!("silu"));
            }
            "qwen3" => {
                o.insert("attention_bias".into(), json!(has("blk.0.attn_q.bias".into())));
                o.insert("hidden_act".into(), json!("silu"));
            }
            "qwen2" => {
                o.insert("hidden_act".into(), json!("silu"));
            }
            "gemma" => {
                o.insert("hidden_activation".into(), json!("gelu_pytorch_tanh"));
            }
            _ => {
                // gemma2: llama.cpp scales the queries by 1/√head_dim, except for the 27B (46
                // layers, 1/√(hidden/heads)) — the GGUF carries no query_pre_attn_scalar.
                let qpas = if layers == 46 { hidden / heads } else { head_dim };
                o.insert("hidden_activation".into(), json!("gelu_pytorch_tanh"));
                o.insert("query_pre_attn_scalar".into(), json!(qpas));
                for (k, hk) in [("attn_logit_softcapping", "attn_logit_softcapping"), ("final_logit_softcapping", "final_logit_softcapping")] {
                    if let Some(v) = get(k).and_then(GValue::as_f64) {
                        o.insert(hk.into(), json!(v));
                    }
                }
                let sw = get("attention.sliding_window").and_then(GValue::as_u64).ok_or_else(|| LowerError::bad("GGUF gemma2: no sliding window"))?;
                o.insert("sliding_window".into(), json!(sw));
            }
        }
        // Rotary scaling.
        match get("rope.scaling.type").and_then(GValue::as_str) {
            None | Some("none") => {}
            Some("linear") => {
                let f = get("rope.scaling.factor").and_then(GValue::as_f64).ok_or_else(|| LowerError::bad("GGUF: linear rope without a factor"))?;
                o.insert("rope_scaling".into(), json!({"rope_type": "linear", "factor": f}));
            }
            Some(t) => return Err(LowerError::not_lowerable(format!("GGUF rope scaling `{t}` is not mapped"))),
        }
        let mut consumed = std::collections::BTreeSet::new();
        if let Some(rf) = file.tensors.get("rope_freqs.weight") {
            let got = file.tensor_f32(&rf.name)?.data;
            let hit = LLAMA3_ROPES.iter().find(|c| {
                let want = llama3_factors(head_dim, theta, **c);
                want.len() == got.len() && want.iter().zip(&got).all(|(a, b)| (a - b).abs() <= 1e-6 * a.abs().max(1.0))
            });
            let (factor, lo, hi, orig) = *hit.ok_or_else(|| {
                LowerError::not_lowerable("GGUF rope_freqs.weight is not a Llama-3 factor set this mapping recognises")
            })?;
            o.insert(
                "rope_scaling".into(),
                json!({"rope_type": "llama3", "factor": factor, "low_freq_factor": lo, "high_freq_factor": hi,
                       "original_max_position_embeddings": orig}),
            );
            consumed.insert(rf.name.clone());
        }
        // Tensor names.
        let mut map: BTreeMap<String, (String, Option<Vec<usize>>)> = BTreeMap::new();
        let mut put = |hf: String, g: String, perm: Option<Vec<usize>>| {
            map.insert(hf, (g, perm));
        };
        put("model.embed_tokens.weight".into(), "token_embd.weight".into(), None);
        put("model.norm.weight".into(), "output_norm.weight".into(), None);
        if !tied {
            put("lm_head.weight".into(), "output.weight".into(), None);
        }
        let gemma2 = arch == "gemma2";
        for l in 0..layers {
            let (b, m) = (format!("blk.{l}."), format!("model.layers.{l}."));
            put(format!("{m}input_layernorm.weight"), format!("{b}attn_norm.weight"), None);
            for (hfp, gp, n) in [("q_proj", "attn_q", heads), ("k_proj", "attn_k", kv), ("v_proj", "attn_v", 0), ("o_proj", "attn_output", 0)] {
                for suffix in ["weight", "bias"] {
                    let g = format!("{b}{gp}.{suffix}");
                    if let Some(t) = file.tensors.get(&g) {
                        let rows = *t.shape().first().unwrap_or(&0);
                        let perm = (arch == "llama" && n > 0).then(|| unpermute(n, rows));
                        put(format!("{m}self_attn.{hfp}.{suffix}"), g, perm);
                    }
                }
            }
            if arch == "qwen3" {
                put(format!("{m}self_attn.q_norm.weight"), format!("{b}attn_q_norm.weight"), None);
                put(format!("{m}self_attn.k_norm.weight"), format!("{b}attn_k_norm.weight"), None);
            }
            if gemma2 {
                put(format!("{m}post_attention_layernorm.weight"), format!("{b}post_attention_norm.weight"), None);
                put(format!("{m}pre_feedforward_layernorm.weight"), format!("{b}ffn_norm.weight"), None);
                put(format!("{m}post_feedforward_layernorm.weight"), format!("{b}post_ffw_norm.weight"), None);
            } else {
                put(format!("{m}post_attention_layernorm.weight"), format!("{b}ffn_norm.weight"), None);
            }
            for (hfp, gp) in [("gate_proj", "ffn_gate"), ("up_proj", "ffn_up"), ("down_proj", "ffn_down")] {
                put(format!("{m}mlp.{hfp}.weight"), format!("{b}{gp}.weight"), None);
            }
        }
        for (hf, (g, _)) in &map {
            if !file.tensors.contains_key(g) {
                return Err(LowerError::weights(format!("GGUF: `{g}` (for `{hf}`) is missing")));
            }
            consumed.insert(g.clone());
        }
        Ok(GgufModel { file, arch, config: cfg, map, consumed })
    }

    /// GGUF tensors this view does not map (reported as unread by the weight check).
    pub fn unmapped(&self) -> Vec<String> {
        self.file.tensors.keys().filter(|k| !self.consumed.contains(*k)).cloned().collect()
    }

    /// The quantisation this checkpoint's projections carry: for each Hugging Face module (a
    /// template over layers, `{L}`) whose tensors are block-quantised, the program layout over every
    /// layer's type (the finest group; an offset term if any layer's type has minimums). A module
    /// that is float in some layers and quantised in others is refused.
    pub fn quant_config(&self) -> Result<QuantConfig> {
        let mut per: BTreeMap<String, Vec<GgmlType>> = BTreeMap::new();
        for (hf, (g, _)) in &self.map {
            let Some(module) = hf.strip_suffix(".weight") else { continue };
            let t = &self.file.tensors[g];
            if t.dims.len() != 2 || module == "model.embed_tokens" {
                continue;
            }
            let template = match module.strip_prefix("model.layers.") {
                Some(rest) => {
                    let (_, tail) = rest.split_once('.').unwrap_or(("", rest));
                    format!("model.layers.{{L}}.{tail}")
                }
                None => module.to_string(),
            };
            per.entry(template).or_default().push(t.ty);
        }
        let mut per_module = BTreeMap::new();
        for (m, types) in per {
            let floats = types.iter().filter(|t| t.is_float()).count();
            if floats == types.len() {
                continue;
            }
            if floats > 0 {
                return Err(LowerError::not_lowerable(format!("GGUF: `{m}` is float in some layers and quantised in others")));
            }
            if let Some(t) = types.iter().find(|t| t.block().is_none()) {
                return Err(LowerError::not_lowerable(format!("GGUF: `{m}` is {} (Q8_0, Q4_0/1, Q5_0/1, Q4_K, Q5_K, Q6_K are read)", t.name())));
            }
            let group = types.iter().map(|t| t.group()).min().unwrap_or(32);
            let offset_term = types.iter().any(|t| t.has_min());
            per_module.insert(m, QLayout { group, order: false, offset_term });
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
    /// differently — Gemma's norm gains as `1 + w` (multiplied as stored), and the quantised
    /// modules' layouts.
    pub fn spec(&self) -> Result<ArchSpec> {
        let mut spec = crate::parse_config(&self.config)?;
        if self.arch.starts_with("gemma") {
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
                }
            }
            if let Some(n) = spec.final_norm.as_mut() {
                w(n);
            }
            spec.notes.push("GGUF: Gemma's norm gains are stored as 1 + w and multiplied as stored".into());
        }
        let q = self.quant_config()?;
        if !q.per_module.is_empty() {
            crate::hf_config::attach_quant(&mut spec, q)?;
        }
        Ok(spec)
    }
}

impl TensorSource for GgufModel {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        let (g, _) = self.map.get(name)?;
        self.file.tensors.get(g).map(GgufTensorInfo::shape)
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let (g, perm) = self.map.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let t = self.file.tensor_f32(g)?;
        Ok(match perm {
            None => t,
            Some(p) => {
                let cols: usize = t.shape[1..].iter().product();
                let mut data = Vec::with_capacity(t.data.len());
                for r in p {
                    data.extend_from_slice(&t.data[r * cols..(r + 1) * cols]);
                }
                Tensor::new(t.shape.clone(), data)
            }
        })
    }
    fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.map.keys().cloned().collect();
        v.extend(self.unmapped());
        v
    }
    fn load_qweight(&self, name: &str) -> Result<QWeight> {
        let (g, perm) = self.map.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let q = self.file.qweight(g)?;
        match perm {
            None => Ok(q),
            Some(p) => q.take_rows(p),
        }
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
