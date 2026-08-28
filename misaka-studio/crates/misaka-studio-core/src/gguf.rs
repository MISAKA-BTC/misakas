//! A **header-only GGUF reader**.
//!
//! Everything a model list needs — architecture, parameter count, layer count, trained context,
//! attention shape, quantization — is in the first few megabytes of a GGUF. The weights are the
//! other 20 GB. So this reads the header and stops: listing forty models must not mean touching
//! forty gigabytes of disk.
//!
//! It is also the difference between a real model card and a guess. `Llama-3.2-3B-Q4_K_M.gguf`
//! tells you three things, two of which are conventions people break; the header tells you the
//! same three plus the twelve the filename never carried, and it cannot be renamed into a lie.
//!
//! # Refusing rather than guessing
//!
//! Every length in this format is attacker-controlled — a model file arrives over the network
//! from a repository this app does not own. A `u64` string length read straight into a `Vec`
//! allocation is how a 300-byte file becomes an out-of-memory kill, so every count is bounded
//! ([`MAX_KV_COUNT`], [`MAX_STRING_BYTES`], [`MAX_ARRAY_LEN`], [`MAX_TENSOR_COUNT`]) and a file
//! that exceeds a bound is rejected with the offset that did it. Truncation is reported the same
//! way: an unexpected EOF names where it happened, not "invalid file".

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::Path;

/// GGUF's magic, little-endian: the ASCII bytes `G G U F`.
pub const GGUF_MAGIC: [u8; 4] = *b"GGUF";

/// Metadata keys are counted in thousands even for the largest models; a million is a bound
/// nothing legitimate approaches and no allocation can be built from.
pub const MAX_KV_COUNT: u64 = 1 << 20;
/// A single metadata string. Chat templates are the large ones (tens of kilobytes); 64 MiB is
/// far above anything real and far below anything that hurts.
pub const MAX_STRING_BYTES: u64 = 64 << 20;
/// Token vocabularies are the big arrays — ~256 k entries for current models. A million allows
/// generous growth; beyond it, the file is not describing a vocabulary.
pub const MAX_ARRAY_LEN: u64 = 1 << 20;
/// A 405B model has on the order of 1 k tensors.
pub const MAX_TENSOR_COUNT: u64 = 1 << 20;

/// How many elements of a metadata array are kept in memory.
///
/// The vocabulary is the reason this exists: `tokenizer.ggml.tokens` is a quarter of a million
/// strings, the UI shows none of them, and holding them per model turns a model list into
/// hundreds of megabytes. The count is always exact; the values are a preview.
pub const ARRAY_PREVIEW_LEN: usize = 8;

/// One metadata value, in the GGUF type system.
///
/// `Array` carries its true `len` and a bounded `preview` — see [`ARRAY_PREVIEW_LEN`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    String(String),
    Array { element_type: u32, len: u64, preview: Vec<GgufValue> },
}

impl GgufValue {
    /// Widen any integer variant to `u64`. Signed values that are negative return `None` rather
    /// than wrapping — a negative layer count is a corrupt file, not a very large one.
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            GgufValue::U8(v) => Some(v as u64),
            GgufValue::U16(v) => Some(v as u64),
            GgufValue::U32(v) => Some(v as u64),
            GgufValue::U64(v) => Some(v),
            GgufValue::I8(v) => u64::try_from(v).ok(),
            GgufValue::I16(v) => u64::try_from(v).ok(),
            GgufValue::I32(v) => u64::try_from(v).ok(),
            GgufValue::I64(v) => u64::try_from(v).ok(),
            GgufValue::Bool(v) => Some(v as u64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            GgufValue::F32(v) => Some(v as f64),
            GgufValue::F64(v) => Some(v),
            _ => self.as_u64().map(|v| v as f64),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            GgufValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// The parsed header of one GGUF file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GgufMetadata {
    pub version: u32,
    pub tensor_count: u64,
    /// Every metadata key/value in the file, in key order.
    pub kv: BTreeMap<String, GgufValue>,
    /// Exact parameter count: the sum of every tensor's element count, from the tensor index.
    ///
    /// Not an estimate from file size — that would divide by an assumed bits-per-weight and be
    /// wrong for every mixed-precision quantization, which today is all of the good ones.
    pub parameter_count: u64,
    /// `ggml_type` → how many tensors carry it. The honest answer to "what quantization is
    /// this?" for a mixed file, and the fallback when `general.file_type` is absent.
    pub tensor_types: BTreeMap<u32, u64>,
    /// Bytes the header occupies — where the tensor data begins.
    pub data_offset: u64,
}

impl GgufMetadata {
    /// Read the header of the GGUF at `path`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let display = path.display().to_string();
        let file = std::fs::File::open(path).map_err(|e| Error::io(&display, e))?;
        Self::from_reader(BufReader::with_capacity(1 << 20, file), display)
    }

    /// Read a header from any reader. Split out so tests can build files in memory.
    pub fn from_reader(reader: impl Read, path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let mut r = Cursor { inner: reader, pos: 0, path };

        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if magic != GGUF_MAGIC {
            return Err(Error::NotGguf { path: r.path, found: magic });
        }

        let version = r.u32()?;
        if !(2..=3).contains(&version) {
            return Err(Error::UnsupportedGgufVersion { path: r.path, version });
        }

        let tensor_count = r.bounded_u64("tensor_count", MAX_TENSOR_COUNT)?;
        let kv_count = r.bounded_u64("metadata_kv_count", MAX_KV_COUNT)?;

        let mut kv = BTreeMap::new();
        for _ in 0..kv_count {
            let key = r.string()?;
            let value = r.value()?;
            // A duplicate key is a malformed file, but the last-wins behaviour of every reader in
            // the ecosystem is what the model was tested against; matching it beats being right
            // alone.
            kv.insert(key, value);
        }

        let mut parameter_count: u64 = 0;
        let mut tensor_types: BTreeMap<u32, u64> = BTreeMap::new();
        for _ in 0..tensor_count {
            let _name = r.string()?;
            let n_dims = r.u32()?;
            if n_dims > 8 {
                return Err(r.malformed(format!("tensor has {n_dims} dimensions; GGML allows at most 8")));
            }
            let mut elements: u64 = 1;
            for _ in 0..n_dims {
                let dim = r.u64()?;
                elements = elements.saturating_mul(dim);
            }
            let ggml_type = r.u32()?;
            let _offset = r.u64()?;
            parameter_count = parameter_count.saturating_add(elements);
            *tensor_types.entry(ggml_type).or_insert(0) += 1;
        }

        Ok(GgufMetadata { version, tensor_count, kv, parameter_count, tensor_types, data_offset: r.pos })
    }

    /// `general.architecture` — `llama`, `qwen3`, `gemma3`, … The prefix every shape key uses.
    pub fn architecture(&self) -> Option<&str> {
        self.kv.get("general.architecture").and_then(GgufValue::as_str)
    }

    pub fn name(&self) -> Option<&str> {
        self.kv.get("general.name").and_then(GgufValue::as_str)
    }

    /// Look up a key that is namespaced under the architecture, e.g. `qwen3.block_count`.
    pub fn arch_key(&self, suffix: &str) -> Option<&GgufValue> {
        let arch = self.architecture()?;
        self.kv.get(&format!("{arch}.{suffix}"))
    }

    fn arch_u64(&self, suffix: &str) -> Option<u64> {
        self.arch_key(suffix).and_then(GgufValue::as_u64)
    }

    /// Transformer layers. Drives both the KV-cache estimate and GPU offload planning.
    pub fn block_count(&self) -> Option<u64> {
        self.arch_u64("block_count")
    }

    pub fn embedding_length(&self) -> Option<u64> {
        self.arch_u64("embedding_length")
    }

    /// The context the model was **trained** for. A runtime may be asked for less (to save
    /// memory) and sometimes for more (with RoPE scaling); this is the honest default.
    pub fn context_length(&self) -> Option<u64> {
        self.arch_u64("context_length")
    }

    pub fn head_count(&self) -> Option<u64> {
        self.arch_u64("attention.head_count")
    }

    /// KV heads — the number that actually sizes the cache. Grouped-query attention makes this
    /// much smaller than [`Self::head_count`], and using the wrong one overstates the memory a
    /// long context needs by up to 8x.
    pub fn head_count_kv(&self) -> Option<u64> {
        self.arch_u64("attention.head_count_kv").or_else(|| self.head_count())
    }

    /// Experts in a mixture-of-experts model; `None` for a dense one.
    pub fn expert_count(&self) -> Option<u64> {
        self.arch_u64("expert_count").filter(|&n| n > 0)
    }

    /// `general.file_type`, the `LLAMA_FTYPE` enum the converter stamped in.
    pub fn file_type(&self) -> Option<u32> {
        self.kv.get("general.file_type").and_then(GgufValue::as_u64).map(|v| v as u32)
    }

    /// The Jinja chat template, when the model ships one. Its presence is what lets the Studio
    /// apply the model's own prompt format instead of a house style that fits nothing.
    pub fn chat_template(&self) -> Option<&str> {
        self.kv.get("tokenizer.chat_template").and_then(GgufValue::as_str)
    }

    /// Per-token KV-cache bytes at f16: `2 (K and V) * layers * kv_heads * head_dim * 2 bytes`.
    ///
    /// `head_dim` comes from `embedding_length / head_count` unless the file states it, which
    /// newer architectures (where the two are decoupled) do.
    pub fn kv_cache_bytes_per_token(&self) -> Option<u64> {
        let layers = self.block_count()?;
        let kv_heads = self.head_count_kv()?;
        let head_dim = match self.arch_u64("attention.key_length") {
            Some(d) => d,
            None => self.embedding_length()? / self.head_count()?.max(1),
        };
        Some(2 * layers * kv_heads * head_dim * 2)
    }
}

/// A position-tracking reader. The position is the whole point: an error at byte 41 in a
/// truncated file is diagnosable, "unexpected end of file" is not.
struct Cursor<R: Read> {
    inner: R,
    pos: u64,
    path: String,
}

impl<R: Read> Cursor<R> {
    fn malformed(&self, reason: impl Into<String>) -> Error {
        Error::MalformedGguf { path: self.path.clone(), offset: self.pos, reason: reason.into() }
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.inner.read_exact(buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => self.malformed(format!("file ends mid-value; {} more bytes needed", buf.len())),
            _ => Error::Io { path: self.path.clone(), source: e },
        })?;
        self.pos += buf.len() as u64;
        Ok(())
    }

    fn u8(&mut self) -> Result<u8> {
        let mut b = [0u8; 1];
        self.read_exact(&mut b)?;
        Ok(b[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let mut b = [0u8; 2];
        self.read_exact(&mut b)?;
        Ok(u16::from_le_bytes(b))
    }

    fn u32(&mut self) -> Result<u32> {
        let mut b = [0u8; 4];
        self.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    fn u64(&mut self) -> Result<u64> {
        let mut b = [0u8; 8];
        self.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }

    fn bounded_u64(&mut self, what: &str, max: u64) -> Result<u64> {
        let v = self.u64()?;
        if v > max {
            return Err(self.malformed(format!("{what} is {v}, above the {max} this reader accepts")));
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String> {
        let len = self.bounded_u64("string length", MAX_STRING_BYTES)?;
        let mut buf = vec![0u8; len as usize];
        self.read_exact(&mut buf)?;
        // Lossy on purpose. A single bad byte in one metadata string is not a reason to refuse a
        // 20 GB model the runtime will load happily.
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    fn value(&mut self) -> Result<GgufValue> {
        let ty = self.u32()?;
        self.value_of_type(ty)
    }

    fn value_of_type(&mut self, ty: u32) -> Result<GgufValue> {
        Ok(match ty {
            0 => GgufValue::U8(self.u8()?),
            1 => GgufValue::I8(self.u8()? as i8),
            2 => GgufValue::U16(self.u16()?),
            3 => GgufValue::I16(self.u16()? as i16),
            4 => GgufValue::U32(self.u32()?),
            5 => GgufValue::I32(self.u32()? as i32),
            6 => GgufValue::F32(f32::from_bits(self.u32()?)),
            7 => GgufValue::Bool(self.u8()? != 0),
            8 => GgufValue::String(self.string()?),
            9 => {
                let element_type = self.u32()?;
                if element_type == 9 {
                    return Err(self.malformed("nested arrays are not a GGUF value"));
                }
                let len = self.bounded_u64("array length", MAX_ARRAY_LEN)?;
                let mut preview = Vec::new();
                for i in 0..len {
                    let v = self.value_of_type(element_type)?;
                    if (i as usize) < ARRAY_PREVIEW_LEN {
                        preview.push(v);
                    }
                }
                GgufValue::Array { element_type, len, preview }
            }
            10 => GgufValue::U64(self.u64()?),
            11 => GgufValue::I64(self.u64()? as i64),
            12 => GgufValue::F64(f64::from_bits(self.u64()?)),
            other => return Err(self.malformed(format!("unknown metadata value type {other}"))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a GGUF header in memory, so the parser is tested against bytes rather than
    /// against a 4 GB file nobody can put in a repository.
    #[derive(Default)]
    struct Builder {
        kv: Vec<u8>,
        kv_count: u64,
        tensors: Vec<u8>,
        tensor_count: u64,
    }

    impl Builder {
        fn string(out: &mut Vec<u8>, s: &str) {
            out.extend_from_slice(&(s.len() as u64).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }

        fn kv_string(mut self, key: &str, value: &str) -> Self {
            Self::string(&mut self.kv, key);
            self.kv.extend_from_slice(&8u32.to_le_bytes());
            Self::string(&mut self.kv, value);
            self.kv_count += 1;
            self
        }

        fn kv_u32(mut self, key: &str, value: u32) -> Self {
            Self::string(&mut self.kv, key);
            self.kv.extend_from_slice(&4u32.to_le_bytes());
            self.kv.extend_from_slice(&value.to_le_bytes());
            self.kv_count += 1;
            self
        }

        fn kv_str_array(mut self, key: &str, values: &[&str]) -> Self {
            Self::string(&mut self.kv, key);
            self.kv.extend_from_slice(&9u32.to_le_bytes());
            self.kv.extend_from_slice(&8u32.to_le_bytes());
            self.kv.extend_from_slice(&(values.len() as u64).to_le_bytes());
            for v in values {
                Self::string(&mut self.kv, v);
            }
            self.kv_count += 1;
            self
        }

        fn tensor(mut self, name: &str, dims: &[u64], ggml_type: u32) -> Self {
            Self::string(&mut self.tensors, name);
            self.tensors.extend_from_slice(&(dims.len() as u32).to_le_bytes());
            for d in dims {
                self.tensors.extend_from_slice(&d.to_le_bytes());
            }
            self.tensors.extend_from_slice(&ggml_type.to_le_bytes());
            self.tensors.extend_from_slice(&0u64.to_le_bytes());
            self.tensor_count += 1;
            self
        }

        fn build(self, version: u32) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&GGUF_MAGIC);
            out.extend_from_slice(&version.to_le_bytes());
            out.extend_from_slice(&self.tensor_count.to_le_bytes());
            out.extend_from_slice(&self.kv_count.to_le_bytes());
            out.extend_from_slice(&self.kv);
            out.extend_from_slice(&self.tensors);
            out
        }
    }

    fn sample() -> Vec<u8> {
        Builder::default()
            .kv_string("general.architecture", "qwen3")
            .kv_string("general.name", "Qwen3 4B Instruct")
            .kv_u32("general.file_type", 15)
            .kv_u32("qwen3.block_count", 36)
            .kv_u32("qwen3.embedding_length", 2560)
            .kv_u32("qwen3.context_length", 262_144)
            .kv_u32("qwen3.attention.head_count", 32)
            .kv_u32("qwen3.attention.head_count_kv", 8)
            .kv_u32("qwen3.attention.key_length", 128)
            .kv_str_array("tokenizer.ggml.tokens", &["a", "b", "c"])
            .tensor("token_embd.weight", &[2560, 151_936], 12)
            .tensor("blk.0.attn_q.weight", &[2560, 4096], 12)
            .build(3)
    }

    #[test]
    fn reads_shape_and_parameters_from_the_header() {
        let md = GgufMetadata::from_reader(sample().as_slice(), "sample.gguf").expect("parses");
        assert_eq!(md.version, 3);
        assert_eq!(md.architecture(), Some("qwen3"));
        assert_eq!(md.name(), Some("Qwen3 4B Instruct"));
        assert_eq!(md.block_count(), Some(36));
        assert_eq!(md.context_length(), Some(262_144));
        assert_eq!(md.head_count_kv(), Some(8));
        assert_eq!(md.file_type(), Some(15));
        assert_eq!(md.parameter_count, 2560 * 151_936 + 2560 * 4096);
        assert_eq!(md.tensor_types.get(&12), Some(&2));
    }

    /// A vocabulary is a quarter of a million strings. The count must survive; the strings must
    /// not, or a model list is measured in gigabytes.
    #[test]
    fn arrays_keep_their_length_but_not_their_contents() {
        let md = GgufMetadata::from_reader(sample().as_slice(), "sample.gguf").expect("parses");
        match md.kv.get("tokenizer.ggml.tokens").expect("array is present") {
            GgufValue::Array { element_type, len, preview } => {
                assert_eq!(*element_type, 8);
                assert_eq!(*len, 3);
                assert_eq!(preview.len(), 3);
            }
            other => panic!("expected an array, got {other:?}"),
        }
    }

    /// GQA: 8 KV heads, not 32. Sizing the cache off `head_count` would overstate a 262 k
    /// context by 4x and tell a 32 GB machine it cannot run a model it runs comfortably.
    #[test]
    fn kv_cache_is_sized_from_kv_heads() {
        let md = GgufMetadata::from_reader(sample().as_slice(), "sample.gguf").expect("parses");
        assert_eq!(md.kv_cache_bytes_per_token(), Some(2 * 36 * 8 * 128 * 2));
    }

    #[test]
    fn a_non_gguf_file_is_named_as_one() {
        let err = GgufMetadata::from_reader(b"not a model".as_slice(), "notes.txt").unwrap_err();
        assert!(matches!(err, Error::NotGguf { .. }), "got {err}");
    }

    #[test]
    fn a_truncated_file_reports_where_it_ended() {
        let full = sample();
        let err = GgufMetadata::from_reader(&full[..40], "cut.gguf").unwrap_err();
        match err {
            Error::MalformedGguf { offset, .. } => assert!(offset > 0, "the offset locates the truncation"),
            other => panic!("expected a malformed-file error, got {other}"),
        }
    }

    /// The allocation guard: a 30-byte file claiming a 2^60-entry vocabulary must be refused
    /// before anything is allocated, not after the OOM killer arrives.
    #[test]
    fn an_absurd_count_is_refused_before_allocating() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&GGUF_MAGIC);
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&(u64::MAX / 2).to_le_bytes());
        let err = GgufMetadata::from_reader(bytes.as_slice(), "hostile.gguf").unwrap_err();
        assert!(matches!(err, Error::MalformedGguf { .. }), "got {err}");
    }

    #[test]
    fn version_one_is_refused_with_a_reason() {
        let bytes = Builder::default().build(1);
        let err = GgufMetadata::from_reader(bytes.as_slice(), "old.gguf").unwrap_err();
        assert!(matches!(err, Error::UnsupportedGgufVersion { version: 1, .. }), "got {err}");
    }
}
