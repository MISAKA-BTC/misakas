//! **A GGUF reader, and the two k-quant dequantizers Qwen3.6 is stored in.**
//!
//! The checkpoint the goal names is a 24 GiB GGUF: `Q4_K` for most projections, `Q6_K` for the
//! value and down projections and the output head, `F32` for the norms, `F16` for two scalars.
//! Converting it is the only path to running the model — the bf16 safetensors are 70 GiB and the
//! machine that has to do the conversion does not have that.
//!
//! # Offline, and float on purpose
//!
//! This is conversion, not execution. ADR-0040 Decision B pins the class's scales at
//! REGISTRATION, so what reads a checkpoint may use float and what runs one may not — the same
//! boundary `convert.rs` sits on. Nothing here is on the block-validation path.
//!
//! # Ranged reads
//!
//! A tensor is addressed by an offset into the data section, so a converter can fetch one tensor
//! over HTTP without the other 24 GiB. [`GgufDirectory`] parses the header alone and reports each
//! tensor's byte range; the caller decides how the bytes arrive.

use std::collections::BTreeMap;

/// The `ggml` type tags this reader knows. Everything else is refused by name rather than
/// silently mis-read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GgufType {
    F32,
    F16,
    Q8_0,
    Q4K,
    Q5K,
    Q6K,
}

impl GgufType {
    fn from_tag(tag: u32) -> Option<Self> {
        match tag {
            0 => Some(Self::F32),
            1 => Some(Self::F16),
            8 => Some(Self::Q8_0),
            12 => Some(Self::Q4K),
            13 => Some(Self::Q5K),
            14 => Some(Self::Q6K),
            _ => None,
        }
    }

    /// Bytes for `n` values. The k-quants are super-blocks of 256, so `n` must be a whole number
    /// of them — llama.cpp guarantees that for every tensor it writes. `Q8_0` blocks are 32.
    pub fn bytes_for(&self, n: usize) -> Option<usize> {
        match self {
            Self::F32 => Some(n * 4),
            Self::F16 => Some(n * 2),
            Self::Q8_0 => n.is_multiple_of(32).then(|| n / 32 * 34),
            Self::Q4K => n.is_multiple_of(256).then(|| n / 256 * 144),
            Self::Q5K => n.is_multiple_of(256).then(|| n / 256 * 176),
            Self::Q6K => n.is_multiple_of(256).then(|| n / 256 * 210),
        }
    }
}

#[derive(Debug)]
pub enum GgufError {
    NotGguf,
    UnsupportedVersion(u32),
    Truncated(&'static str),
    UnknownType { tensor: String, tag: u32 },
    BadShape(String),
    MissingTensor(String),
    ShortData { tensor: String, want: usize, got: usize },
}

impl std::fmt::Display for GgufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotGguf => write!(f, "not a GGUF file"),
            Self::UnsupportedVersion(v) => write!(f, "GGUF version {v} is not read by this build"),
            Self::Truncated(w) => write!(f, "the header ends inside {w}"),
            Self::UnknownType { tensor, tag } => write!(f, "tensor {tensor} has ggml type {tag}, which this reader does not decode"),
            Self::BadShape(t) => write!(f, "tensor {t} has a shape this reader cannot use"),
            Self::MissingTensor(t) => write!(f, "no tensor named {t}"),
            Self::ShortData { tensor, want, got } => write!(f, "tensor {tensor} needs {want} bytes and {got} were supplied"),
        }
    }
}

impl std::error::Error for GgufError {}

/// One tensor's entry: where it is and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GgufTensor {
    pub name: String,
    /// GGUF's dimension order, fastest-varying first. A `[2048, 8192]` matrix is 8192 rows of
    /// 2048, which is the row-major layout a projection reads.
    pub dims: Vec<u64>,
    pub kind: GgufType,
    /// Absolute byte offset in the file.
    pub offset: u64,
    pub bytes: usize,
}

impl GgufTensor {
    pub fn elements(&self) -> usize {
        self.dims.iter().product::<u64>() as usize
    }
    /// The byte range to fetch, inclusive of the start and exclusive of the end.
    pub fn range(&self) -> (u64, u64) {
        (self.offset, self.offset + self.bytes as u64)
    }
}

/// A parsed header: the metadata that is not an array, and every tensor's location.
///
/// Array metadata is skipped rather than kept — the vocabulary alone is 248,320 strings, and a
/// converter that wants the tokenizer reads the tokenizer file.
#[derive(Debug, Default)]
pub struct GgufDirectory {
    pub metadata: BTreeMap<String, GgufValue>,
    pub tensors: BTreeMap<String, GgufTensor>,
    pub data_start: u64,
}

/// A scalar metadata value. Arrays are recorded by length only.
#[derive(Clone, Debug, PartialEq)]
pub enum GgufValue {
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
    Str(String),
    ArrayLen(usize),
    /// A retained string array. Kept only for the keys [`KEEP_ARRAYS`] names — the vocabulary is
    /// 248,320 strings and every other array in the file is metadata nobody reads.
    Strings(Vec<String>),
    /// A retained integer array, for the token-type table.
    Ints(Vec<i64>),
}

/// Metadata keys whose arrays are kept rather than counted. The tokenizer lives in the GGUF for
/// this checkpoint — the repository ships no `tokenizer.json` — so it is the one array a loader
/// that wants to turn text into ids cannot skip.
pub const KEEP_ARRAYS: [&str; 3] = ["tokenizer.ggml.tokens", "tokenizer.ggml.merges", "tokenizer.ggml.token_type"];

impl GgufValue {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(v) => Some(*v),
            Self::I64(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(v) => Some(*v),
            Self::U64(v) => Some(*v as f64),
            Self::I64(v) => Some(*v as f64),
            _ => None,
        }
    }
    pub fn as_strings(&self) -> Option<&[String]> {
        match self {
            Self::Strings(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_ints(&self) -> Option<&[i64]> {
        match self {
            Self::Ints(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(v) => Some(v),
            _ => None,
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], GgufError> {
        let end = self.i.checked_add(n).ok_or(GgufError::Truncated(what))?;
        if end > self.b.len() {
            return Err(GgufError::Truncated(what));
        }
        let out = &self.b[self.i..end];
        self.i = end;
        Ok(out)
    }
    fn u32(&mut self, w: &'static str) -> Result<u32, GgufError> {
        Ok(u32::from_le_bytes(self.take(4, w)?.try_into().expect("4")))
    }
    fn u64(&mut self, w: &'static str) -> Result<u64, GgufError> {
        Ok(u64::from_le_bytes(self.take(8, w)?.try_into().expect("8")))
    }
    fn string(&mut self, w: &'static str) -> Result<String, GgufError> {
        let n = self.u64(w)? as usize;
        Ok(String::from_utf8_lossy(self.take(n, w)?).into_owned())
    }
    /// One typed value. Arrays consume their elements and report only a length unless `keep`.
    fn value(&mut self, tag: u32, keep: bool) -> Result<GgufValue, GgufError> {
        Ok(match tag {
            0 => GgufValue::U64(self.take(1, "u8")?[0] as u64),
            1 => GgufValue::I64(self.take(1, "i8")?[0] as i8 as i64),
            2 => GgufValue::U64(u16::from_le_bytes(self.take(2, "u16")?.try_into().expect("2")) as u64),
            3 => GgufValue::I64(i16::from_le_bytes(self.take(2, "i16")?.try_into().expect("2")) as i64),
            4 => GgufValue::U64(self.u32("u32")? as u64),
            5 => GgufValue::I64(i32::from_le_bytes(self.take(4, "i32")?.try_into().expect("4")) as i64),
            6 => GgufValue::F64(f32::from_le_bytes(self.take(4, "f32")?.try_into().expect("4")) as f64),
            7 => GgufValue::Bool(self.take(1, "bool")?[0] != 0),
            8 => GgufValue::Str(self.string("string")?),
            9 => {
                let element = self.u32("array type")?;
                let n = self.u64("array length")? as usize;
                if !keep {
                    for _ in 0..n {
                        self.value(element, false)?;
                    }
                    return Ok(GgufValue::ArrayLen(n));
                }
                if element == 8 {
                    let mut out = Vec::with_capacity(n.min(1 << 20));
                    for _ in 0..n {
                        out.push(self.string("array string")?);
                    }
                    GgufValue::Strings(out)
                } else {
                    let mut out = Vec::with_capacity(n.min(1 << 20));
                    for _ in 0..n {
                        out.push(match self.value(element, false)? {
                            GgufValue::U64(v) => v as i64,
                            GgufValue::I64(v) => v,
                            GgufValue::F64(v) => v as i64,
                            GgufValue::Bool(v) => v as i64,
                            _ => return Err(GgufError::Truncated("array element")),
                        });
                    }
                    GgufValue::Ints(out)
                }
            }
            10 => GgufValue::U64(self.u64("u64")?),
            11 => GgufValue::I64(i64::from_le_bytes(self.take(8, "i64")?.try_into().expect("8"))),
            12 => GgufValue::F64(f64::from_le_bytes(self.take(8, "f64")?.try_into().expect("8"))),
            other => return Err(GgufError::UnknownType { tensor: "<metadata>".into(), tag: other }),
        })
    }
}

/// Parse a GGUF header. `bytes` must cover the header; the data section may be absent, which is
/// the point — a 24 GiB file's directory is the first few tens of megabytes.
pub fn parse_directory(bytes: &[u8]) -> Result<GgufDirectory, GgufError> {
    let mut r = Reader { b: bytes, i: 0 };
    if r.take(4, "magic")? != b"GGUF" {
        return Err(GgufError::NotGguf);
    }
    let version = r.u32("version")?;
    if version != 3 {
        return Err(GgufError::UnsupportedVersion(version));
    }
    let n_tensors = r.u64("tensor count")? as usize;
    let n_kv = r.u64("metadata count")? as usize;

    let mut metadata = BTreeMap::new();
    for _ in 0..n_kv {
        let key = r.string("metadata key")?;
        let tag = r.u32("metadata type")?;
        let keep = KEEP_ARRAYS.contains(&key.as_str());
        metadata.insert(key, r.value(tag, keep)?);
    }

    let mut entries = Vec::with_capacity(n_tensors);
    for _ in 0..n_tensors {
        let name = r.string("tensor name")?;
        let n_dims = r.u32("tensor rank")? as usize;
        let mut dims = Vec::with_capacity(n_dims);
        for _ in 0..n_dims {
            dims.push(r.u64("tensor dimension")?);
        }
        let tag = r.u32("tensor type")?;
        let offset = r.u64("tensor offset")?;
        entries.push((name, dims, tag, offset));
    }

    // The data section is aligned; `general.alignment` overrides the default of 32.
    let alignment = metadata.get("general.alignment").and_then(|v| v.as_u64()).unwrap_or(32).max(1);
    let data_start = (r.i as u64).next_multiple_of(alignment);

    let mut tensors = BTreeMap::new();
    for (name, dims, tag, offset) in entries {
        let kind = GgufType::from_tag(tag).ok_or_else(|| GgufError::UnknownType { tensor: name.clone(), tag })?;
        let elements: u64 = dims.iter().product();
        let bytes = kind.bytes_for(elements as usize).ok_or_else(|| GgufError::BadShape(name.clone()))?;
        tensors.insert(name.clone(), GgufTensor { name, dims, kind, offset: data_start + offset, bytes });
    }
    Ok(GgufDirectory { metadata, tensors, data_start })
}

/// IEEE-754 half to `f32`. Written out rather than taken from a crate because it is eleven lines
/// and this is the only place the format appears.
fn f16(bits: u16) -> f32 {
    let sign = ((bits >> 15) & 1) as u32;
    let exponent = ((bits >> 10) & 0x1F) as u32;
    let mantissa = (bits & 0x3FF) as u32;
    // The subnormal case is written as arithmetic rather than as bit surgery. The first draft
    // renormalized the mantissa into an f32 exponent field and landed one power of two low —
    // 2^-25 where the smallest half subnormal is 2^-24 — which is the kind of error that reads
    // correctly and produces weights that are quietly half of what they should be. A subnormal
    // half is exactly `mantissa · 2^-24`, and saying so leaves nothing to get wrong.
    if exponent == 0 {
        let magnitude = mantissa as f32 * (1.0 / 16_777_216.0);
        return if sign == 1 { -magnitude } else { magnitude };
    }
    let out = match exponent {
        0x1F => (sign << 31) | (0xFF << 23) | (mantissa << 13),
        _ => (sign << 31) | ((exponent + 127 - 15) << 23) | (mantissa << 13),
    };
    f32::from_bits(out)
}

/// The six-bit scale/min pair for sub-block `j` of a `Q4_K` super-block.
///
/// llama.cpp packs eight `(scale, min)` pairs into twelve bytes: the first four sub-blocks take
/// six low bits from bytes 0..8, and the last four take four bits from bytes 8..12 plus two high
/// bits borrowed from the first eight. Transcribed rather than reasoned about — this is the one
/// place where "close" produces plausible weights that are quietly wrong.
fn q4k_scale_min(scales: &[u8; 12], j: usize) -> (u8, u8) {
    if j < 4 {
        (scales[j] & 63, scales[j + 4] & 63)
    } else {
        let d = (scales[j + 4] & 0xF) | ((scales[j - 4] >> 6) << 4);
        let m = (scales[j + 4] >> 4) | ((scales[j] >> 6) << 4);
        (d, m)
    }
}

/// Dequantize `Q4_K`: 256 values per 144-byte super-block, eight sub-blocks of 32 with their own
/// six-bit scale and minimum.
pub fn dequantize_q4k(data: &[u8], elements: usize) -> Result<Vec<f32>, GgufError> {
    let blocks = elements / 256;
    if !elements.is_multiple_of(256) || data.len() < blocks * 144 {
        return Err(GgufError::ShortData { tensor: "<q4_k>".into(), want: blocks * 144, got: data.len() });
    }
    let mut out = Vec::with_capacity(elements);
    for b in 0..blocks {
        let block = &data[b * 144..(b + 1) * 144];
        let d = f16(u16::from_le_bytes([block[0], block[1]]));
        let dmin = f16(u16::from_le_bytes([block[2], block[3]]));
        let scales: [u8; 12] = block[4..16].try_into().expect("12");
        let qs = &block[16..144];
        for j in 0..8 {
            let (sc, m) = q4k_scale_min(&scales, j);
            let (scale, min) = (d * sc as f32, dmin * m as f32);
            // Sub-blocks come in pairs sharing 32 bytes: the even one takes the low nibbles and
            // the odd one the high nibbles.
            let half = &qs[(j / 2) * 32..(j / 2) * 32 + 32];
            for byte in half {
                let q = if j.is_multiple_of(2) { byte & 0xF } else { byte >> 4 };
                out.push(scale * q as f32 - min);
            }
        }
    }
    Ok(out)
}

/// Dequantize `Q5_K`: 256 values per 176-byte super-block — the `Q4_K` layout (eight sub-blocks
/// of 32 with six-bit scale/min pairs) plus one high bit per value in a 32-byte plane, so the
/// quant runs 0..31 instead of 0..15. Transcribed from llama.cpp's `dequantize_row_q5_K`: the
/// high-bit plane is addressed by a mask that shifts two positions per 64-value pair, exactly as
/// the low nibbles alternate.
pub fn dequantize_q5k(data: &[u8], elements: usize) -> Result<Vec<f32>, GgufError> {
    let blocks = elements / 256;
    if !elements.is_multiple_of(256) || data.len() < blocks * 176 {
        return Err(GgufError::ShortData { tensor: "<q5_k>".into(), want: blocks * 176, got: data.len() });
    }
    let mut out = Vec::with_capacity(elements);
    for b in 0..blocks {
        let block = &data[b * 176..(b + 1) * 176];
        let d = f16(u16::from_le_bytes([block[0], block[1]]));
        let dmin = f16(u16::from_le_bytes([block[2], block[3]]));
        let scales: [u8; 12] = block[4..16].try_into().expect("12");
        let qh = &block[16..48];
        let ql = &block[48..176];
        let (mut u1, mut u2) = (1u8, 2u8);
        for pair in 0..4 {
            let (sc1, m1) = q4k_scale_min(&scales, 2 * pair);
            let (sc2, m2) = q4k_scale_min(&scales, 2 * pair + 1);
            let (d1, min1) = (d * sc1 as f32, dmin * m1 as f32);
            let (d2, min2) = (d * sc2 as f32, dmin * m2 as f32);
            let low = &ql[pair * 32..pair * 32 + 32];
            for l in 0..32 {
                let q = (low[l] & 0xF) as f32 + if qh[l] & u1 != 0 { 16.0 } else { 0.0 };
                out.push(d1 * q - min1);
            }
            for l in 0..32 {
                let q = (low[l] >> 4) as f32 + if qh[l] & u2 != 0 { 16.0 } else { 0.0 };
                out.push(d2 * q - min2);
            }
            u1 <<= 2;
            u2 <<= 2;
        }
    }
    Ok(out)
}

/// Dequantize `Q8_0`: 32 values per 34-byte block — an `f16` scale and 32 signed bytes.
pub fn dequantize_q8_0(data: &[u8], elements: usize) -> Result<Vec<f32>, GgufError> {
    let blocks = elements / 32;
    if !elements.is_multiple_of(32) || data.len() < blocks * 34 {
        return Err(GgufError::ShortData { tensor: "<q8_0>".into(), want: blocks * 34, got: data.len() });
    }
    let mut out = Vec::with_capacity(elements);
    for b in 0..blocks {
        let block = &data[b * 34..(b + 1) * 34];
        let d = f16(u16::from_le_bytes([block[0], block[1]]));
        for l in 0..32 {
            out.push(d * block[2 + l] as i8 as f32);
        }
    }
    Ok(out)
}

/// Dequantize `Q6_K`: 256 values per 210-byte super-block, sixteen sub-blocks of 16 with an
/// eight-bit signed scale, and six-bit quants split across a low nibble and a high pair.
pub fn dequantize_q6k(data: &[u8], elements: usize) -> Result<Vec<f32>, GgufError> {
    let blocks = elements / 256;
    if !elements.is_multiple_of(256) || data.len() < blocks * 210 {
        return Err(GgufError::ShortData { tensor: "<q6_k>".into(), want: blocks * 210, got: data.len() });
    }
    let mut out = vec![0f32; elements];
    for b in 0..blocks {
        let block = &data[b * 210..(b + 1) * 210];
        let ql = &block[0..128];
        let qh = &block[128..192];
        let scales = &block[192..208];
        let d = f16(u16::from_le_bytes([block[208], block[209]]));
        // llama.cpp's loop: two halves of 128, each producing four groups of 32.
        for half in 0..2 {
            let (ql, qh) = (&ql[half * 64..half * 64 + 64], &qh[half * 32..half * 32 + 32]);
            let sc = &scales[half * 8..half * 8 + 8];
            for l in 0..32 {
                let low = ql[l] as i32;
                let high = ql[l + 32] as i32;
                let h = qh[l] as i32;
                let q = [
                    ((low & 0xF) | ((h & 3) << 4)) - 32,
                    ((high & 0xF) | (((h >> 2) & 3) << 4)) - 32,
                    ((low >> 4) | (((h >> 4) & 3) << 4)) - 32,
                    ((high >> 4) | (((h >> 6) & 3) << 4)) - 32,
                ];
                for (g, value) in q.iter().enumerate() {
                    let index = b * 256 + half * 128 + g * 32 + l;
                    out[index] = d * sc[g * 2 + l / 16] as i8 as f32 * *value as f32;
                }
            }
        }
    }
    Ok(out)
}

/// Dequantize whatever a tensor holds, given exactly its bytes.
pub fn dequantize(tensor: &GgufTensor, data: &[u8]) -> Result<Vec<f32>, GgufError> {
    if data.len() < tensor.bytes {
        return Err(GgufError::ShortData { tensor: tensor.name.clone(), want: tensor.bytes, got: data.len() });
    }
    let n = tensor.elements();
    match tensor.kind {
        GgufType::F32 => Ok(data[..n * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("4"))).collect()),
        GgufType::F16 => Ok(data[..n * 2].chunks_exact(2).map(|c| f16(u16::from_le_bytes(c.try_into().expect("2")))).collect()),
        GgufType::Q8_0 => dequantize_q8_0(data, n),
        GgufType::Q4K => dequantize_q4k(data, n),
        GgufType::Q5K => dequantize_q5k(data, n),
        GgufType::Q6K => dequantize_q6k(data, n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_precision_round_trips_the_values_that_matter() {
        assert_eq!(f16(0x0000), 0.0);
        assert_eq!(f16(0x3C00), 1.0);
        assert_eq!(f16(0xBC00), -1.0);
        assert_eq!(f16(0x4000), 2.0);
        assert!((f16(0x3555) - 0.333_252).abs() < 1e-5);
        // A subnormal, which the renormalizing branch exists for.
        assert!((f16(0x0001) - 5.960_464e-8).abs() < 1e-12);
        assert!(f16(0x7C00).is_infinite());
        assert!(f16(0xFC00).is_infinite() && f16(0xFC00) < 0.0);
        assert!(f16(0x7E00).is_nan());
    }

    /// The block sizes are the format's, and getting one wrong shifts every tensor after it.
    #[test]
    fn the_block_sizes_are_the_formats() {
        assert_eq!(GgufType::Q4K.bytes_for(256), Some(144));
        assert_eq!(GgufType::Q6K.bytes_for(256), Some(210));
        assert_eq!(GgufType::Q4K.bytes_for(2048 * 8192), Some(2048 * 8192 / 256 * 144));
        assert_eq!(GgufType::F32.bytes_for(7), Some(28));
        // A k-quant length that is not a whole super-block is refused rather than rounded.
        assert_eq!(GgufType::Q4K.bytes_for(255), None);
    }

    /// A hand-built super-block, decoded. The scale packing is the part that produces plausible
    /// but wrong weights when it is close rather than exact, so it is checked against values
    /// computed by hand rather than against another implementation.
    #[test]
    fn a_q4k_super_block_decodes() {
        let mut block = vec![0u8; 144];
        // d = 1.0, dmin = 0.0
        block[0..2].copy_from_slice(&0x3C00u16.to_le_bytes());
        block[2..4].copy_from_slice(&0x0000u16.to_le_bytes());
        // Sub-block 0: scale 1, min 0. Sub-block 1: scale 2, min 0.
        block[4] = 1;
        block[5] = 2;
        // Every quant nibble = 3 in the first 32 bytes, so sub-block 0 is 3·1 and sub-block 1 is
        // 3·2 (they share the same 32 bytes, low nibbles then high nibbles).
        for byte in block[16..48].iter_mut() {
            *byte = 0x33;
        }
        let values = dequantize_q4k(&block, 256).expect("one super-block");
        assert_eq!(values.len(), 256);
        assert!(values[..32].iter().all(|v| (*v - 3.0).abs() < 1e-6), "sub-block 0 is scale 1 times quant 3");
        assert!(values[32..64].iter().all(|v| (*v - 6.0).abs() < 1e-6), "sub-block 1 is scale 2 times quant 3");
        assert!(values[64..].iter().all(|v| v.abs() < 1e-6), "the untouched sub-blocks are zero");
    }

    /// The six-bit scale packing, which is the one piece that cannot be reasoned out from the
    /// struct definition.
    #[test]
    fn the_q4k_scale_packing_matches_upstream() {
        let mut scales = [0u8; 12];
        // Sub-block 0 takes the low six bits of byte 0; its min the low six of byte 4.
        scales[0] = 0b11_101010;
        scales[4] = 0b01_010101;
        assert_eq!(q4k_scale_min(&scales, 0), (0b101010, 0b010101));
        // Sub-block 4 takes the low nibble of byte 8 plus the top two bits of byte 0, and its min
        // the high nibble of byte 8 plus the top two of byte 4.
        scales[8] = 0b0110_1001;
        assert_eq!(q4k_scale_min(&scales, 4), (0b1001 | (0b11 << 4), 0b0110 | (0b01 << 4)));
    }
}
