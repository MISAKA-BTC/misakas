//! # `PALWART3` — the one versioned PALW artifact container (genesis 8, user decision 2026-10-15)
//!
//! Every genesis model ships in this one container. Before it there were three unrelated files:
//! `.palwart` (`PALWB0A1`/`PALWB0A2`, the BASE-0 A16 classes), `.palwtir` (`PALWTIR1`/`PALWTIR2`,
//! RFC-0002 TIR programs) and `.palwq36` (`PALWQ361`, the Qwen3.6 integer runtime). Those stay
//! readable as LEGACY formats, under their own magics and their own identities; this container never
//! reinterprets one of their ids. It is a new format with a new magic and an explicit version, and
//! the legacy identity of what it carries is kept as a section's `inner_root`.
//!
//! ```text
//! index    := "PALWART3" · version u16 LE (= 3) · header_len u32 LE · header (borsh) · root [64]
//! segments := <stem>.<k:05>.palwseg, k = 0, 1, …   (the data stream cut every `segment_bytes`)
//! data     := entry bytes, each at a 64-byte aligned logical offset, sections in order,
//!             zero padding between entries, nothing after the last entry
//! ```
//!
//! * **Why an index and segments.** A converter streams: one input shard in, one output segment out,
//!   the segment uploaded and deleted, the next shard fetched. No step holds the artifact, so peak
//!   disk is about one input shard plus one segment. The index is written last; it names every
//!   segment's length and SHA-256 (the Hugging Face LFS object id, so a remote copy is checked
//!   without downloading it).
//! * **What it holds.** A version header; a canonical manifest (source repository and revision pin,
//!   license, attribution, conversion route and options); the program (TIR program bytes and their
//!   kernel id, or the engine an A16/PALWQ361 section runs on); per-architecture weight sections
//!   (A16, PALWQ361 integer, TIR inventory for the GDN hybrid / Gemma 4 / gpt-oss / Llama families,
//!   an MXFP4-derived exact-integer kind reserved); the tokenizer, the chat template, the source
//!   metadata, the license texts and the calibration inputs.
//! * **Identity.** [`palwart_root_v3`] — BLAKE2b-512 keyed `misaka-palw/palwart/v3/root` over the
//!   version, the manifest, the program record and every section's kind, family, name, inner root
//!   and entry table (names, dtypes, shapes, encodings, lengths, and each entry's BLAKE2b-256). It
//!   does NOT cover offsets, segment cuts, the declared layout or the provenance: those are storage
//!   and history, not content. This root is "the .palwart root" a genesis catalog entry names.
//! * **Not a proof.** The root authenticates bytes and their structure (PRINCIPLES §8: hashes for
//!   identity only). Whether a computation over these bytes was done correctly is decided by
//!   sampled re-execution and the deterministic court, never by this file.
//!
//! [`PalwArtV3::open`] checks the whole header before any entry is believed: version, caps, the
//! canonical offset of every entry, the segment cut, and the trailing root against a recomputation.

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const PALWART_MAGIC_V3: &[u8; 8] = b"PALWART3";
pub const PALWART_VERSION_V3: u16 = 3;
/// Every entry starts at a multiple of this logical offset.
pub const PALWART_ALIGN_V3: u64 = 64;
/// A header larger than this is refused before it is read.
pub const PALWART_MAX_HEADER_BYTES_V3: u32 = 64 << 20;
/// The default segment: small enough that one input shard plus one segment fits a laptop disk.
pub const PALWART_DEFAULT_SEGMENT_BYTES_V3: u64 = 2 << 30;
pub const PALWART_MIN_SEGMENT_BYTES_V3: u64 = 1 << 20;
/// Below the Hugging Face per-file ceiling (50 GB).
pub const PALWART_MAX_SEGMENT_BYTES_V3: u64 = 40 << 30;
/// The most sections and entries a header may declare (a hostile index cannot size an allocation).
pub const PALWART_MAX_SECTIONS_V3: usize = 256;
pub const PALWART_MAX_ENTRIES_V3: usize = 1 << 20;
pub const PALWART_ROOT_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/root";
pub const PALWART_ENTRY_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/entry";
pub const PALWART_ENTRIES_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/entries";
/// The index file's extension and the segment files' suffix.
pub const PALWART_INDEX_EXT_V3: &str = "palwart";
pub const PALWART_SEGMENT_EXT_V3: &str = "palwseg";

/// Section kinds. Codes are never reused; a reader that meets an unknown code still verifies the
/// root but must refuse to execute the artifact.
pub mod section_kind {
    /// The BASE-0 A16 class: one entry, the legacy `PALWB0A2` file bytes; inner root = its
    /// `artifact_digest` (scheme [`super::inner_scheme::BASE0_ARTIFACT_DIGEST_V1`]).
    pub const BASE0_A16: u16 = 1;
    /// The Qwen3.6 PALWQ361 integer runtime: one entry, the legacy `PALWQ361` file bytes.
    pub const PALWQ361_INT: u16 = 2;
    /// A TIR program's params: one entry per instance in INVENTORY order; inner root = the
    /// consensus TIR inventory root (scheme [`super::inner_scheme::TIR_INVENTORY_ROOT_V1`]).
    pub const TIR_INVENTORY: u16 = 3;
    /// Reserved: MXFP4 blocks/scales decoded to exact integer codes (gpt-oss native MXFP4).
    pub const MXFP4_EXACT_INT: u16 = 4;
    pub const TOKENIZER: u16 = 16;
    pub const CHAT_TEMPLATE: u16 = 17;
    pub const SOURCE_METADATA: u16 = 18;
    pub const LICENSE: u16 = 19;
    /// The calibration statistics and token sequences a rebuild needs (reproducibility).
    pub const CALIBRATION: u16 = 20;

    pub fn name(code: u16) -> &'static str {
        match code {
            BASE0_A16 => "base0-a16",
            PALWQ361_INT => "palwq361-int",
            TIR_INVENTORY => "tir-inventory",
            MXFP4_EXACT_INT => "mxfp4-exact-int",
            TOKENIZER => "tokenizer",
            CHAT_TEMPLATE => "chat-template",
            SOURCE_METADATA => "source-metadata",
            LICENSE => "license",
            CALIBRATION => "calibration",
            _ => "unknown",
        }
    }

    /// The weight-bearing kinds (one of them is the model).
    pub fn is_weights(code: u16) -> bool {
        matches!(code, BASE0_A16 | PALWQ361_INT | TIR_INVENTORY | MXFP4_EXACT_INT)
    }
}

/// Architecture families a weight section belongs to.
pub mod family {
    pub const NONE: u16 = 0;
    pub const QWEN2_DENSE: u16 = 1;
    pub const QWEN35_GDN_HYBRID: u16 = 2;
    pub const QWEN35_GDN_HYBRID_MOE: u16 = 3;
    pub const GEMMA4_UNIFIED: u16 = 4;
    pub const GPT_OSS_MOE: u16 = 5;
    pub const LLAMA_DENSE: u16 = 6;

    pub fn name(code: u16) -> &'static str {
        match code {
            NONE => "none",
            QWEN2_DENSE => "qwen2-dense",
            QWEN35_GDN_HYBRID => "qwen35-gdn-hybrid",
            QWEN35_GDN_HYBRID_MOE => "qwen35-gdn-hybrid-moe",
            GEMMA4_UNIFIED => "gemma4-unified-dense",
            GPT_OSS_MOE => "gpt-oss-moe",
            LLAMA_DENSE => "llama-dense",
            _ => "unknown",
        }
    }

    pub fn from_name(s: &str) -> Option<u16> {
        Some(match s {
            "none" => NONE,
            "qwen2-dense" => QWEN2_DENSE,
            "qwen35-gdn-hybrid" => QWEN35_GDN_HYBRID,
            "qwen35-gdn-hybrid-moe" => QWEN35_GDN_HYBRID_MOE,
            "gemma4-unified-dense" => GEMMA4_UNIFIED,
            "gpt-oss-moe" => GPT_OSS_MOE,
            "llama-dense" => LLAMA_DENSE,
            _ => return None,
        })
    }
}

/// How a section's `inner_root` was computed (the LEGACY identity of what the section carries).
pub mod inner_scheme {
    /// No inner root: the section is identified by its entry table alone.
    pub const NONE: u16 = 0;
    /// `Base0ArtifactV1::artifact_digest()` of the embedded `PALWB0A2` file.
    pub const BASE0_ARTIFACT_DIGEST_V1: u16 = 1;
    /// [`super::legacy_file_digest_v3`] of an embedded legacy file (BLAKE2b-512 keyed
    /// `misaka-palw/palwart/v3/legacy-file` over every byte): used for `PALWQ361`, whose registered
    /// inventory root depends on a shape profile chosen at registration, not on the file alone.
    pub const LEGACY_FILE_DIGEST_V1: u16 = 2;
    /// `palw_tir_inventory_root_v1` over the section's instances (with its leaf count).
    pub const TIR_INVENTORY_ROOT_V1: u16 = 3;
}

/// What the program record names.
pub mod program_kind {
    pub const NONE: u16 = 0;
    /// The BASE-0 A16 engine (the program is the engine; `bytes` empty).
    pub const BASE0_A16_ENGINE: u16 = 1;
    /// The PALWQ361 integer engine (`bytes` empty).
    pub const PALWQ361_ENGINE: u16 = 2;
    /// A canonical `TirProgramV1`; `kernel_id` = its graph-IR root.
    pub const TIR_V1: u16 = 3;
    /// A `TirPipelineV1` over `TirProgramV2`s (reserved for pipeline classes).
    pub const TIR_V2_PIPELINE: u16 = 4;
}

/// Entry dtypes (the element type of `shape`).
pub mod dtype {
    /// Opaque bytes (a file, a text); `shape` = `[bytes]`.
    pub const BYTES: u8 = 0;
    pub const I8: u8 = 1;
    pub const I16: u8 = 2;
    pub const I32: u8 = 3;
    pub const I64: u8 = 4;
    /// TIR `idx` (unsigned 32).
    pub const U32: u8 = 5;
    pub const BF16: u8 = 6;
    pub const F16: u8 = 7;
    pub const F32: u8 = 8;
    pub const U16: u8 = 9;
    pub const U8: u8 = 10;

    pub fn width(code: u8) -> Option<u64> {
        Some(match code {
            BYTES | I8 | U8 => 1,
            I16 | BF16 | F16 | U16 => 2,
            I32 | U32 | F32 => 4,
            I64 => 8,
            _ => return None,
        })
    }

    pub fn of_tir(d: misaka_palw_tir::DType) -> Option<u8> {
        use misaka_palw_tir::DType as T;
        Some(match d {
            T::I8 => I8,
            T::I16 => I16,
            T::I32 => I32,
            T::I64 => I64,
            T::Idx => U32,
            T::I128 => return None,
        })
    }
}

/// Entry encodings.
pub mod encoding {
    /// Little-endian elements of `dtype` (two's complement for signed integers).
    pub const RAW_LE: u16 = 0;
    /// The legacy `PALWB0A2` (or `PALWB0A1`) file, byte for byte.
    pub const BASE0_A16_FILE: u16 = 1;
    /// The legacy `PALWQ361` file, byte for byte.
    pub const PALWQ361_FILE: u16 = 2;
    /// One TIR param instance as a `PALWTIR1` container stores it.
    pub const TIR_INSTANCE_LE: u16 = 3;
    pub const UTF8_TEXT: u16 = 4;
    pub const JSON: u16 = 5;
    /// Reserved: MXFP4 (E2M1 x2 nibbles, E8M0 group scales) decoded to exact integer codes.
    pub const MXFP4_EXACT_INT_V1: u16 = 6;
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtEntryV3 {
    pub name: String,
    /// The layer of a per-layer instance (TIR), `None` for a global or a file.
    pub layer: Option<u32>,
    pub dtype: u8,
    pub shape: Vec<u64>,
    pub encoding: u16,
    /// Logical offset into the data stream, a multiple of [`PALWART_ALIGN_V3`].
    pub offset: u64,
    pub bytes: u64,
    /// BLAKE2b-256 keyed [`PALWART_ENTRY_DOMAIN_V3`] over the entry's bytes.
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtSectionV3 {
    pub kind: u16,
    pub family: u16,
    pub name: String,
    pub inner_root_scheme: u16,
    pub inner_root: [u8; 64],
    pub inner_leaf_count: u64,
    pub entries: Vec<PalwArtEntryV3>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtProgramV3 {
    pub kind: u16,
    /// The canonical program bytes (TIR), or empty (an engine).
    pub bytes: Vec<u8>,
    /// The TIR graph-IR root, or zero for an engine named by `kind`.
    pub kernel_id: [u8; 64],
    /// The catalog's `kernel_version` (0: the legacy engine of `kind`).
    pub kernel_version: u16,
    /// The tokenizer id the class binds (TIR `tokenizer_id_of`, or BASE-0's commitment).
    pub tokenizer_id: [u8; 64],
}

impl PalwArtProgramV3 {
    pub fn none() -> Self {
        Self { kind: program_kind::NONE, bytes: Vec::new(), kernel_id: [0; 64], kernel_version: 0, tokenizer_id: [0; 64] }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtSegmentV3 {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtHeaderV3 {
    pub version: u16,
    /// Canonical JSON (sorted keys, no insignificant whitespace): identity.
    pub manifest: Vec<u8>,
    pub program: PalwArtProgramV3,
    pub sections: Vec<PalwArtSectionV3>,
    pub data_bytes: u64,
    pub segment_bytes: u64,
    pub segments: Vec<PalwArtSegmentV3>,
    /// A declared commitment layout (`borsh(PalwTirLayoutV1)`), or empty: not identity.
    pub layout: Vec<u8>,
    /// A legacy container's own provenance string (`PALWTIR1` meta), kept so the legacy file can
    /// be re-assembled byte for byte: not identity.
    pub legacy_meta: String,
    /// Provenance JSON (converter build, host, times): not identity.
    pub provenance: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum PalwArtErrorV3 {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a PALWART3 index (magic or version)")]
    Magic,
    #[error("header: {0}")]
    Header(String),
    #[error("layout: {0}")]
    Layout(String),
    #[error("the root does not reproduce: the index says {declared}, the header gives {recomputed}")]
    Root { declared: String, recomputed: String },
    #[error("segment {index}: {why}")]
    Segment { index: u32, why: String },
    #[error("entry `{name}`: {why}")]
    Entry { name: String, why: String },
    #[error("writer: {0}")]
    Writer(String),
    #[error("segment hook: {0}")]
    Hook(String),
}

pub type ResultV3<T> = std::result::Result<T, PalwArtErrorV3>;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn align_v3(x: u64) -> u64 {
    x.div_ceil(PALWART_ALIGN_V3) * PALWART_ALIGN_V3
}

/// The entry digest state: BLAKE2b-256 keyed [`PALWART_ENTRY_DOMAIN_V3`].
pub fn entry_digest_state_v3() -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(32).key(PALWART_ENTRY_DOMAIN_V3).to_state()
}

/// The digest of an entry's bytes.
pub fn entry_digest_v3(bytes: &[u8]) -> [u8; 32] {
    let mut st = entry_digest_state_v3();
    st.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// Key of [`legacy_file_digest_v3`].
pub const PALWART_LEGACY_FILE_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/legacy-file";

/// The streaming state of [`legacy_file_digest_v3`].
pub fn legacy_file_digest_state_v3() -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(PALWART_LEGACY_FILE_DOMAIN_V3).to_state()
}

/// BLAKE2b-512 keyed [`PALWART_LEGACY_FILE_DOMAIN_V3`] over a whole legacy file (streamed).
pub fn legacy_file_digest_v3(path: &Path) -> std::io::Result<[u8; 64]> {
    let mut f = std::fs::File::open(path)?;
    let mut st = legacy_file_digest_state_v3();
    let mut buf = vec![0u8; 8 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        st.update(&buf[..n]);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    Ok(out)
}

fn h64(domain: &[u8], bytes: &[u8]) -> [u8; 64] {
    let h = blake2b_simd::Params::new().hash_length(64).key(domain).hash(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.as_bytes());
    out
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// The root of one section's entry table: names, layers, dtypes, shapes, encodings, lengths and
/// digests, in the CANONICAL order — sorted by (name bytes, layer), never the file order and never the
/// offsets. A packed `PALWTIR1` (inventory order) and a streamed conversion (production order) of the
/// same tensors therefore have the same root.
pub fn palwart_entries_root_v3(entries: &[PalwArtEntryV3]) -> [u8; 64] {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(PALWART_ENTRIES_DOMAIN_V3).to_state();
    st.update(&(entries.len() as u64).to_le_bytes());
    let mut order: Vec<&PalwArtEntryV3> = entries.iter().collect();
    order.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()).then(a.layer.cmp(&b.layer)));
    let mut buf = Vec::with_capacity(256);
    for e in order {
        buf.clear();
        put_str(&mut buf, &e.name);
        match e.layer {
            None => buf.push(0),
            Some(l) => {
                buf.push(1);
                buf.extend_from_slice(&l.to_le_bytes());
            }
        }
        buf.push(e.dtype);
        buf.extend_from_slice(&(e.shape.len() as u32).to_le_bytes());
        for d in &e.shape {
            buf.extend_from_slice(&d.to_le_bytes());
        }
        buf.extend_from_slice(&e.encoding.to_le_bytes());
        buf.extend_from_slice(&e.bytes.to_le_bytes());
        buf.extend_from_slice(&e.digest);
        st.update(&buf);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// **The `.palwart` root** — the identity of a `PALWART3` artifact (see the module docs for what it
/// covers and what it does not).
pub fn palwart_root_v3(h: &PalwArtHeaderV3) -> [u8; 64] {
    let mut buf = Vec::with_capacity(1024);
    buf.extend_from_slice(&h.version.to_le_bytes());
    buf.extend_from_slice(&h64(b"misaka-palw/palwart/v3/manifest", &h.manifest));
    let p = &h.program;
    buf.extend_from_slice(&p.kind.to_le_bytes());
    buf.extend_from_slice(&h64(b"misaka-palw/palwart/v3/program", &p.bytes));
    buf.extend_from_slice(&p.kernel_id);
    buf.extend_from_slice(&p.kernel_version.to_le_bytes());
    buf.extend_from_slice(&p.tokenizer_id);
    buf.extend_from_slice(&(h.sections.len() as u32).to_le_bytes());
    for s in &h.sections {
        buf.extend_from_slice(&s.kind.to_le_bytes());
        buf.extend_from_slice(&s.family.to_le_bytes());
        put_str(&mut buf, &s.name);
        buf.extend_from_slice(&s.inner_root_scheme.to_le_bytes());
        buf.extend_from_slice(&s.inner_root);
        buf.extend_from_slice(&s.inner_leaf_count.to_le_bytes());
        buf.extend_from_slice(&palwart_entries_root_v3(&s.entries));
    }
    h64(PALWART_ROOT_DOMAIN_V3, &buf)
}

/// The index path of `<dir>/<stem>.palwart`.
pub fn index_path_v3(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.{PALWART_INDEX_EXT_V3}"))
}

/// Segment `k`'s file name.
pub fn segment_file_name_v3(stem: &str, k: u32) -> String {
    format!("{stem}.{k:05}.{PALWART_SEGMENT_EXT_V3}")
}

/// Check the header's structure (everything but the root and the bytes).
pub fn check_header_v3(h: &PalwArtHeaderV3) -> ResultV3<()> {
    if h.version != PALWART_VERSION_V3 {
        return Err(PalwArtErrorV3::Header(format!("version {}", h.version)));
    }
    if h.sections.len() > PALWART_MAX_SECTIONS_V3 {
        return Err(PalwArtErrorV3::Header(format!("{} sections", h.sections.len())));
    }
    if !(PALWART_MIN_SEGMENT_BYTES_V3..=PALWART_MAX_SEGMENT_BYTES_V3).contains(&h.segment_bytes) {
        return Err(PalwArtErrorV3::Header(format!("segment_bytes {}", h.segment_bytes)));
    }
    if std::str::from_utf8(&h.manifest).is_err() {
        return Err(PalwArtErrorV3::Header("the manifest is not UTF-8".into()));
    }
    let total: usize = h.sections.iter().map(|s| s.entries.len()).sum();
    if total > PALWART_MAX_ENTRIES_V3 {
        return Err(PalwArtErrorV3::Header(format!("{total} entries")));
    }
    // The canonical layout: each entry at the aligned end of the previous one.
    let mut end = 0u64;
    for s in &h.sections {
        for e in &s.entries {
            let at = align_v3(end);
            if e.offset != at {
                return Err(PalwArtErrorV3::Layout(format!("`{}` at {}, the canonical offset is {at}", e.name, e.offset)));
            }
            let w = dtype::width(e.dtype)
                .ok_or_else(|| PalwArtErrorV3::Entry { name: e.name.clone(), why: format!("unknown dtype {}", e.dtype) })?;
            let elems = e
                .shape
                .iter()
                .try_fold(1u64, |a, d| a.checked_mul(*d))
                .ok_or_else(|| PalwArtErrorV3::Entry { name: e.name.clone(), why: "shape overflows".into() })?;
            if elems.checked_mul(w) != Some(e.bytes) {
                return Err(PalwArtErrorV3::Entry {
                    name: e.name.clone(),
                    why: format!("{} bytes, the shape {:?} of width {w} gives {elems}×{w}", e.bytes, e.shape),
                });
            }
            end = at.checked_add(e.bytes).ok_or_else(|| PalwArtErrorV3::Layout("offset overflow".into()))?;
        }
    }
    if end != h.data_bytes {
        return Err(PalwArtErrorV3::Layout(format!("the entries end at {end}, data_bytes is {}", h.data_bytes)));
    }
    let want = h.data_bytes.div_ceil(h.segment_bytes);
    if h.segments.len() as u64 != want {
        return Err(PalwArtErrorV3::Layout(format!("{} segments, the cut gives {want}", h.segments.len())));
    }
    for (k, s) in h.segments.iter().enumerate() {
        let k = k as u64;
        let len = if k + 1 < want { h.segment_bytes } else { h.data_bytes - k * h.segment_bytes };
        if s.bytes != len {
            return Err(PalwArtErrorV3::Segment { index: k as u32, why: format!("{} bytes, the cut gives {len}", s.bytes) });
        }
    }
    Ok(())
}

/// What a finished segment is, handed to the segment hook.
#[derive(Clone, Debug)]
pub struct PalwArtSegmentDoneV3 {
    pub index: u32,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: [u8; 32],
}

/// Called with every segment once it is complete and synced. It may upload the file and delete it;
/// the writer never reads a closed segment again.
pub type PalwArtSegmentHookV3<'a> = Box<dyn FnMut(&PalwArtSegmentDoneV3) -> std::result::Result<(), String> + 'a>;

/// **A segment hook that runs a program** (the genesis pipeline's uploader): `command[0]` with the
/// rest as arguments, and `PALWART_SEGMENT_PATH`, `PALWART_SEGMENT_INDEX`, `PALWART_SEGMENT_BYTES`,
/// `PALWART_SEGMENT_SHA256` (hex) in the environment. A non-zero exit stops the conversion. With
/// `delete`, the segment file is removed once the program has succeeded (it has been uploaded and
/// verified). An empty command only logs.
pub fn command_segment_hook_v3(command: Vec<String>, delete: bool) -> PalwArtSegmentHookV3<'static> {
    Box::new(move |d: &PalwArtSegmentDoneV3| -> std::result::Result<(), String> {
        eprintln!("palwart: segment {} ({} bytes, sha256 {}) {}", d.index, d.bytes, hex(&d.sha256), d.path.display());
        let Some((prog, args)) = command.split_first() else {
            return if delete { Err("deleting segments without a program to hand them to would lose them".into()) } else { Ok(()) };
        };
        let status = std::process::Command::new(prog)
            .args(args)
            .env("PALWART_SEGMENT_PATH", &d.path)
            .env("PALWART_SEGMENT_INDEX", d.index.to_string())
            .env("PALWART_SEGMENT_BYTES", d.bytes.to_string())
            .env("PALWART_SEGMENT_SHA256", hex(&d.sha256))
            .status()
            .map_err(|e| format!("{prog}: {e}"))?;
        if !status.success() {
            return Err(format!("{prog} exited {status} on segment {}", d.index));
        }
        if delete {
            std::fs::remove_file(&d.path).map_err(|e| format!("{}: {e}", d.path.display()))?;
        }
        Ok(())
    })
}

struct OpenSegment {
    file: std::io::BufWriter<std::fs::File>,
    path: PathBuf,
    sha: Sha256,
    written: u64,
}

struct OpenEntry {
    entry: PalwArtEntryV3,
    remaining: u64,
    state: blake2b_simd::State,
}

/// What [`PalwArtWriterV3::finish`] wrote.
#[derive(Clone, Debug)]
pub struct PalwArtWrittenV3 {
    pub index_path: PathBuf,
    pub root: [u8; 64],
    pub header: PalwArtHeaderV3,
}

/// **The streaming writer.** Sections and entries are written in order; the data stream is cut into
/// segment files as it grows, each handed to the hook when complete. Nothing is held but the open
/// segment's buffer and the header table.
pub struct PalwArtWriterV3<'a> {
    dir: PathBuf,
    stem: String,
    segment_bytes: u64,
    hook: PalwArtSegmentHookV3<'a>,
    seg: Option<OpenSegment>,
    segments: Vec<PalwArtSegmentV3>,
    at: u64,
    sections: Vec<PalwArtSectionV3>,
    open_section: Option<PalwArtSectionV3>,
    open_entry: Option<OpenEntry>,
}

impl<'a> PalwArtWriterV3<'a> {
    /// A writer of `<dir>/<stem>.palwart` and its segments. Refuses to overwrite an existing index or
    /// segment (a converter that resumes starts over in a fresh directory: the output is a function
    /// of the inputs, so the segments it re-makes are the same bytes).
    pub fn create(dir: &Path, stem: &str, segment_bytes: u64, hook: PalwArtSegmentHookV3<'a>) -> ResultV3<Self> {
        if !(PALWART_MIN_SEGMENT_BYTES_V3..=PALWART_MAX_SEGMENT_BYTES_V3).contains(&segment_bytes) {
            return Err(PalwArtErrorV3::Writer(format!(
                "segment_bytes {segment_bytes} outside [{PALWART_MIN_SEGMENT_BYTES_V3}, {PALWART_MAX_SEGMENT_BYTES_V3}]"
            )));
        }
        if stem.is_empty() || stem.contains('/') || stem.contains('\\') || stem.starts_with('.') {
            return Err(PalwArtErrorV3::Writer(format!("stem `{stem}`")));
        }
        std::fs::create_dir_all(dir)?;
        let index = index_path_v3(dir, stem);
        if index.exists() {
            return Err(PalwArtErrorV3::Writer(format!("{} exists", index.display())));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
            segment_bytes,
            hook,
            seg: None,
            segments: Vec::new(),
            at: 0,
            sections: Vec::new(),
            open_section: None,
            open_entry: None,
        })
    }

    /// Logical bytes written so far (entries and padding).
    pub fn data_bytes(&self) -> u64 {
        self.at
    }

    fn close_segment(&mut self) -> ResultV3<()> {
        let Some(mut s) = self.seg.take() else { return Ok(()) };
        s.file.flush()?;
        let f = s.file.into_inner().map_err(|e| PalwArtErrorV3::Io(e.into_error()))?;
        f.sync_all()?;
        drop(f);
        let mut sha = [0u8; 32];
        sha.copy_from_slice(&s.sha.finalize());
        let index = self.segments.len() as u32;
        self.segments.push(PalwArtSegmentV3 { bytes: s.written, sha256: sha });
        let done = PalwArtSegmentDoneV3 { index, path: s.path, bytes: s.written, sha256: sha };
        (self.hook)(&done).map_err(PalwArtErrorV3::Hook)
    }

    fn emit(&mut self, mut buf: &[u8]) -> ResultV3<()> {
        while !buf.is_empty() {
            if self.seg.is_none() {
                let k = self.segments.len() as u32;
                let path = self.dir.join(segment_file_name_v3(&self.stem, k));
                let f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| {
                    PalwArtErrorV3::Writer(format!("{}: {e} (a segment is never overwritten; start in an empty directory)", path.display()))
                })?;
                self.seg = Some(OpenSegment { file: std::io::BufWriter::with_capacity(8 << 20, f), path, sha: Sha256::new(), written: 0 });
            }
            let s = self.seg.as_mut().expect("just opened");
            let n = ((self.segment_bytes - s.written) as usize).min(buf.len());
            s.file.write_all(&buf[..n])?;
            s.sha.update(&buf[..n]);
            s.written += n as u64;
            self.at += n as u64;
            buf = &buf[n..];
            if s.written == self.segment_bytes {
                self.close_segment()?;
            }
        }
        Ok(())
    }

    pub fn begin_section(&mut self, kind: u16, family: u16, name: &str) -> ResultV3<()> {
        if self.open_section.is_some() {
            return Err(PalwArtErrorV3::Writer("a section is already open".into()));
        }
        if self.sections.len() >= PALWART_MAX_SECTIONS_V3 {
            return Err(PalwArtErrorV3::Writer("too many sections".into()));
        }
        self.open_section = Some(PalwArtSectionV3 {
            kind,
            family,
            name: name.to_string(),
            inner_root_scheme: inner_scheme::NONE,
            inner_root: [0; 64],
            inner_leaf_count: 0,
            entries: Vec::new(),
        });
        Ok(())
    }

    /// Start an entry of exactly `bytes` bytes (checked against `shape` × the dtype's width).
    pub fn begin_entry(
        &mut self,
        name: &str,
        layer: Option<u32>,
        dtype_code: u8,
        shape: Vec<u64>,
        encoding_code: u16,
        bytes: u64,
    ) -> ResultV3<()> {
        if self.open_section.is_none() {
            return Err(PalwArtErrorV3::Writer(format!("entry `{name}` outside a section")));
        }
        if self.open_entry.is_some() {
            return Err(PalwArtErrorV3::Writer(format!("entry `{name}` while another is open")));
        }
        let w = dtype::width(dtype_code).ok_or_else(|| PalwArtErrorV3::Writer(format!("`{name}`: dtype {dtype_code}")))?;
        let elems = shape.iter().try_fold(1u64, |a, d| a.checked_mul(*d)).unwrap_or(u64::MAX);
        if elems.checked_mul(w) != Some(bytes) {
            return Err(PalwArtErrorV3::Writer(format!("`{name}`: {bytes} bytes for shape {shape:?} × {w}")));
        }
        let pad = align_v3(self.at) - self.at;
        if pad > 0 {
            self.emit(&[0u8; PALWART_ALIGN_V3 as usize][..pad as usize])?;
        }
        self.open_entry = Some(OpenEntry {
            entry: PalwArtEntryV3 {
                name: name.to_string(),
                layer,
                dtype: dtype_code,
                shape,
                encoding: encoding_code,
                offset: self.at,
                bytes,
                digest: [0; 32],
            },
            remaining: bytes,
            state: entry_digest_state_v3(),
        });
        Ok(())
    }

    /// Append bytes to the open entry.
    pub fn write(&mut self, buf: &[u8]) -> ResultV3<()> {
        let e = self.open_entry.as_mut().ok_or_else(|| PalwArtErrorV3::Writer("write outside an entry".into()))?;
        if buf.len() as u64 > e.remaining {
            return Err(PalwArtErrorV3::Writer(format!("`{}`: more bytes than declared", e.entry.name)));
        }
        e.remaining -= buf.len() as u64;
        e.state.update(buf);
        self.emit(buf)
    }

    pub fn end_entry(&mut self) -> ResultV3<[u8; 32]> {
        let e = self.open_entry.take().ok_or_else(|| PalwArtErrorV3::Writer("no entry is open".into()))?;
        if e.remaining != 0 {
            return Err(PalwArtErrorV3::Writer(format!("`{}`: {} bytes short", e.entry.name, e.remaining)));
        }
        let mut entry = e.entry;
        entry.digest.copy_from_slice(e.state.finalize().as_bytes());
        let digest = entry.digest;
        self.open_section.as_mut().expect("checked in begin_entry").entries.push(entry);
        Ok(digest)
    }

    /// A whole entry from memory.
    pub fn put_entry(
        &mut self,
        name: &str,
        layer: Option<u32>,
        dtype_code: u8,
        shape: Vec<u64>,
        encoding_code: u16,
        data: &[u8],
    ) -> ResultV3<[u8; 32]> {
        self.begin_entry(name, layer, dtype_code, shape, encoding_code, data.len() as u64)?;
        self.write(data)?;
        self.end_entry()
    }

    /// An opaque file as one entry (`dtype` bytes, `shape` `[len]`), streamed from disk.
    pub fn put_file(&mut self, name: &str, encoding_code: u16, path: &Path) -> ResultV3<[u8; 32]> {
        let len = std::fs::metadata(path)?.len();
        self.begin_entry(name, None, dtype::BYTES, vec![len], encoding_code, len)?;
        let mut f = std::fs::File::open(path)?;
        let mut buf = vec![0u8; 8 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.write(&buf[..n])?;
        }
        self.end_entry()
    }

    pub fn end_section(&mut self, scheme: u16, inner_root: [u8; 64], inner_leaf_count: u64) -> ResultV3<()> {
        if self.open_entry.is_some() {
            return Err(PalwArtErrorV3::Writer("end_section with an entry open".into()));
        }
        let mut s = self.open_section.take().ok_or_else(|| PalwArtErrorV3::Writer("no section is open".into()))?;
        s.inner_root_scheme = scheme;
        s.inner_root = inner_root;
        s.inner_leaf_count = inner_leaf_count;
        self.sections.push(s);
        Ok(())
    }

    /// Close the last segment and write the index. `manifest` must already be canonical JSON.
    pub fn finish(
        mut self,
        manifest: Vec<u8>,
        program: PalwArtProgramV3,
        layout: Vec<u8>,
        legacy_meta: String,
        provenance: Vec<u8>,
    ) -> ResultV3<PalwArtWrittenV3> {
        if self.open_section.is_some() || self.open_entry.is_some() {
            return Err(PalwArtErrorV3::Writer("finish with a section or an entry open".into()));
        }
        self.close_segment()?;
        let header = PalwArtHeaderV3 {
            version: PALWART_VERSION_V3,
            manifest,
            program,
            sections: std::mem::take(&mut self.sections),
            data_bytes: self.at,
            segment_bytes: self.segment_bytes,
            segments: std::mem::take(&mut self.segments),
            layout,
            legacy_meta,
            provenance,
        };
        check_header_v3(&header)?;
        let root = palwart_root_v3(&header);
        let hb = borsh::to_vec(&header).map_err(|e| PalwArtErrorV3::Header(e.to_string()))?;
        if hb.len() > PALWART_MAX_HEADER_BYTES_V3 as usize {
            return Err(PalwArtErrorV3::Header(format!("a header of {} bytes", hb.len())));
        }
        let index_path = index_path_v3(&self.dir, &self.stem);
        let tmp = self.dir.join(format!(".{}.{PALWART_INDEX_EXT_V3}.tmp", self.stem));
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(PALWART_MAGIC_V3)?;
            f.write_all(&PALWART_VERSION_V3.to_le_bytes())?;
            f.write_all(&(hb.len() as u32).to_le_bytes())?;
            f.write_all(&hb)?;
            f.write_all(&root)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &index_path)?;
        Ok(PalwArtWrittenV3 { index_path, root, header })
    }
}

/// Parse an index's bytes: the header and the declared root, after the structural check and the
/// root recomputation.
pub fn parse_index_v3(bytes: &[u8]) -> ResultV3<(PalwArtHeaderV3, [u8; 64])> {
    if bytes.len() < 14 || &bytes[0..8] != PALWART_MAGIC_V3 || u16::from_le_bytes([bytes[8], bytes[9]]) != PALWART_VERSION_V3 {
        return Err(PalwArtErrorV3::Magic);
    }
    let hl = u32::from_le_bytes(bytes[10..14].try_into().expect("4 bytes"));
    if hl > PALWART_MAX_HEADER_BYTES_V3 {
        return Err(PalwArtErrorV3::Header(format!("a header of {hl} bytes")));
    }
    let end = 14usize + hl as usize;
    if bytes.len() != end + 64 {
        return Err(PalwArtErrorV3::Header(format!("an index of {} bytes, the header gives {}", bytes.len(), end + 64)));
    }
    let header = PalwArtHeaderV3::try_from_slice(&bytes[14..end]).map_err(|e| PalwArtErrorV3::Header(e.to_string()))?;
    check_header_v3(&header)?;
    let mut declared = [0u8; 64];
    declared.copy_from_slice(&bytes[end..]);
    let recomputed = palwart_root_v3(&header);
    if recomputed != declared {
        return Err(PalwArtErrorV3::Root { declared: hex(&declared), recomputed: hex(&recomputed) });
    }
    Ok((header, declared))
}

/// An opened `PALWART3` artifact: the checked header, its root, and where its segments live.
#[derive(Clone, Debug)]
pub struct PalwArtV3 {
    pub header: PalwArtHeaderV3,
    pub root: [u8; 64],
    pub index_path: PathBuf,
    /// The directory the segments are read from (by default the index's).
    pub data_dir: PathBuf,
    pub stem: String,
}

impl PalwArtV3 {
    /// Open `<dir>/<stem>.palwart`; segments are looked up beside it.
    pub fn open(index: &Path) -> ResultV3<Self> {
        let dir = index.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        Self::open_with_data(index, &dir)
    }

    /// Open an index whose segments live in `data_dir`.
    pub fn open_with_data(index: &Path, data_dir: &Path) -> ResultV3<Self> {
        let meta = std::fs::metadata(index)?;
        if meta.len() > PALWART_MAX_HEADER_BYTES_V3 as u64 + 14 + 64 {
            return Err(PalwArtErrorV3::Header(format!("an index of {} bytes", meta.len())));
        }
        let bytes = std::fs::read(index)?;
        let (header, root) = parse_index_v3(&bytes)?;
        let file = index.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let stem = file
            .strip_suffix(&format!(".{PALWART_INDEX_EXT_V3}"))
            .ok_or_else(|| PalwArtErrorV3::Header(format!("{}: not a .{PALWART_INDEX_EXT_V3} file name", index.display())))?
            .to_string();
        Ok(Self { header, root, index_path: index.to_path_buf(), data_dir: data_dir.to_path_buf(), stem })
    }

    pub fn root_hex(&self) -> String {
        hex(&self.root)
    }

    pub fn segment_path(&self, k: u32) -> PathBuf {
        self.data_dir.join(segment_file_name_v3(&self.stem, k))
    }

    /// The manifest as text (canonical JSON).
    pub fn manifest_text(&self) -> &str {
        std::str::from_utf8(&self.header.manifest).unwrap_or("")
    }

    /// The first section of `kind`.
    pub fn section(&self, kind: u16) -> Option<(usize, &PalwArtSectionV3)> {
        self.header.sections.iter().enumerate().find(|(_, s)| s.kind == kind)
    }

    /// The weight-bearing section (exactly one is expected in a model artifact).
    pub fn weights(&self) -> Option<(usize, &PalwArtSectionV3)> {
        self.header.sections.iter().enumerate().find(|(_, s)| section_kind::is_weights(s.kind))
    }

    /// Read `out.len()` bytes at logical offset `offset`, across segment boundaries.
    pub fn read_at(&self, offset: u64, out: &mut [u8]) -> ResultV3<()> {
        let end = offset.checked_add(out.len() as u64).filter(|e| *e <= self.header.data_bytes).ok_or_else(|| {
            PalwArtErrorV3::Layout(format!("bytes {offset}+{} past the data's {}", out.len(), self.header.data_bytes))
        })?;
        let sb = self.header.segment_bytes;
        let mut at = offset;
        let mut done = 0usize;
        while at < end {
            let k = (at / sb) as u32;
            let within = at % sb;
            let n = ((sb - within) as usize).min(out.len() - done);
            let path = self.segment_path(k);
            let f = std::fs::File::open(&path).map_err(|e| PalwArtErrorV3::Segment { index: k, why: format!("{}: {e}", path.display()) })?;
            read_exact_at(&f, &mut out[done..done + n], within)
                .map_err(|e| PalwArtErrorV3::Segment { index: k, why: format!("{}: {e}", path.display()) })?;
            done += n;
            at += n as u64;
        }
        Ok(())
    }

    /// Stream an entry in pieces of at most `piece` bytes, checking its digest at the end. The sink
    /// sees every byte before the check: a caller that must not act on unverified bytes buffers.
    pub fn stream_entry(
        &self,
        section: usize,
        entry: usize,
        piece: usize,
        sink: &mut dyn FnMut(&[u8]) -> std::result::Result<(), String>,
    ) -> ResultV3<()> {
        let e = self
            .header
            .sections
            .get(section)
            .and_then(|s| s.entries.get(entry))
            .ok_or_else(|| PalwArtErrorV3::Entry { name: format!("{section}/{entry}"), why: "no such entry".into() })?;
        let mut st = entry_digest_state_v3();
        let mut buf = vec![0u8; piece.max(1)];
        let mut at = 0u64;
        while at < e.bytes {
            let n = ((e.bytes - at) as usize).min(buf.len());
            self.read_at(e.offset + at, &mut buf[..n])?;
            st.update(&buf[..n]);
            sink(&buf[..n]).map_err(|why| PalwArtErrorV3::Entry { name: e.name.clone(), why })?;
            at += n as u64;
        }
        if st.finalize().as_bytes() != e.digest {
            return Err(PalwArtErrorV3::Entry { name: e.name.clone(), why: "the bytes do not match the entry digest".into() });
        }
        Ok(())
    }

    /// One entry, read whole and checked.
    pub fn read_entry(&self, section: usize, entry: usize) -> ResultV3<Vec<u8>> {
        let mut out = Vec::new();
        self.stream_entry(section, entry, 8 << 20, &mut |b| {
            out.extend_from_slice(b);
            Ok(())
        })?;
        Ok(out)
    }

    /// An entry by section kind and name.
    pub fn find_entry(&self, kind: u16, name: &str) -> Option<(usize, usize)> {
        self.header
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == kind)
            .find_map(|(si, s)| s.entries.iter().position(|e| e.name == name).map(|ei| (si, ei)))
    }

    /// Hash every segment present in `data_dir` against the index (SHA-256 and length). Returns the
    /// indices of segments that are absent (a streamed upload deletes them locally).
    pub fn verify_segments(&self, require_all: bool) -> ResultV3<Vec<u32>> {
        let mut missing = Vec::new();
        for (k, s) in self.header.segments.iter().enumerate() {
            let k = k as u32;
            let path = self.segment_path(k);
            let mut f = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && !require_all => {
                    missing.push(k);
                    continue;
                }
                Err(e) => return Err(PalwArtErrorV3::Segment { index: k, why: format!("{}: {e}", path.display()) }),
            };
            let len = f.metadata()?.len();
            if len != s.bytes {
                return Err(PalwArtErrorV3::Segment { index: k, why: format!("{len} bytes, the index says {}", s.bytes) });
            }
            let mut sha = Sha256::new();
            let mut buf = vec![0u8; 8 << 20];
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                sha.update(&buf[..n]);
            }
            if sha.finalize().as_slice() != s.sha256 {
                return Err(PalwArtErrorV3::Segment { index: k, why: "SHA-256 differs from the index".into() });
            }
        }
        Ok(missing)
    }

    /// Check every entry's digest (all segments must be present).
    pub fn verify_entries(&self) -> ResultV3<()> {
        for (si, s) in self.header.sections.iter().enumerate() {
            for ei in 0..s.entries.len() {
                self.stream_entry(si, ei, 8 << 20, &mut |_| Ok(()))?;
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn read_exact_at(f: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.read_exact_at(buf, at)
}

#[cfg(not(unix))]
fn read_exact_at(f: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut f = f.try_clone()?;
    f.seek(SeekFrom::Start(at))?;
    f.read_exact(buf)
}

/// What a file is, by its first eight bytes. The legacy formats are named, never converted in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwArtifactFormatV3 {
    /// This container's index.
    PalwArtV3,
    /// `PALWB0A1` — BASE-0, no A16 store (legacy `.palwart`).
    Base0V1,
    /// `PALWB0A2` — BASE-0 with the A16 store (legacy `.palwart`).
    Base0A16V2,
    /// `PALWTIR1` — a TIR program container (legacy `.palwtir`).
    TirV1,
    /// `PALWTIR2` — a TIR pipeline container (legacy `.palwtir`).
    TirV2,
    /// `PALWTIRS` — a composite's adapter section (legacy).
    TirSection,
    /// `PALWQ361` — the Qwen3.6 integer runtime (legacy `.palwq36`).
    PalwQ361,
    /// None of the above.
    Unknown,
}

impl PalwArtifactFormatV3 {
    pub fn of_magic(m: &[u8]) -> Self {
        match m.get(0..8) {
            Some(b"PALWART3") => Self::PalwArtV3,
            Some(b"PALWB0A1") => Self::Base0V1,
            Some(b"PALWB0A2") => Self::Base0A16V2,
            Some(b"PALWTIR1") => Self::TirV1,
            Some(b"PALWTIR2") => Self::TirV2,
            Some(b"PALWTIRS") => Self::TirSection,
            Some(b"PALWQ361") => Self::PalwQ361,
            _ => Self::Unknown,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::PalwArtV3 => "PALWART3 (unified container v3)",
            Self::Base0V1 => "PALWB0A1 (legacy BASE-0 .palwart)",
            Self::Base0A16V2 => "PALWB0A2 (legacy BASE-0 A16 .palwart)",
            Self::TirV1 => "PALWTIR1 (legacy TIR .palwtir)",
            Self::TirV2 => "PALWTIR2 (legacy TIR pipeline .palwtir)",
            Self::TirSection => "PALWTIRS (legacy composite section)",
            Self::PalwQ361 => "PALWQ361 (legacy Qwen3.6 .palwq36)",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_legacy(self) -> bool {
        !matches!(self, Self::PalwArtV3 | Self::Unknown)
    }
}

/// Sniff a file's format from its magic.
pub fn sniff_format_v3(path: &Path) -> std::io::Result<PalwArtifactFormatV3> {
    let mut f = std::fs::File::open(path)?;
    let mut m = [0u8; 8];
    let mut got = 0;
    while got < 8 {
        let n = f.read(&mut m[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(PalwArtifactFormatV3::of_magic(&m[..got]))
}

/// **Canonical JSON**: object keys sorted by their UTF-8 bytes, no insignificant whitespace, the
/// `serde_json` number and string spelling. Independent of the `preserve_order` feature.
pub fn canonical_json_v3(v: &serde_json::Value) -> String {
    fn go(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::String((*k).clone()).to_string());
                    out.push(':');
                    go(&m[*k], out);
                }
                out.push('}');
            }
            serde_json::Value::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    go(x, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut out = String::new();
    go(v, &mut out);
    out
}

/// A TIR section as a param source: instance `(param, layer)` is the entry named by the program's
/// param `param` with that layer. The node's evaluator runs from it as from a `PALWTIR1` file.
pub struct PalwArtTirParamsV3<'c> {
    art: &'c PalwArtV3,
    section: usize,
    decls: Vec<(misaka_palw_tir::DType, Vec<usize>, String)>,
    index: std::collections::BTreeMap<(u16, Option<u16>), usize>,
}

impl<'c> PalwArtTirParamsV3<'c> {
    /// Bind the artifact's TIR section to `program`: every instance of the program must be exactly one
    /// entry (by param name and layer) with the declared byte length, and nothing else. The ORDER is
    /// the writer's: a packed `PALWTIR1` is in inventory order, a streamed conversion in production
    /// order (occurrence by occurrence); both are deterministic, and the entry table (order included)
    /// is in the root. The inner root is always the inventory-order root.
    pub fn bind(art: &'c PalwArtV3, program: &misaka_palw_tir::TirProgramV1) -> ResultV3<Self> {
        let (section, s) = art
            .section(section_kind::TIR_INVENTORY)
            .ok_or_else(|| PalwArtErrorV3::Header("no TIR inventory section".into()))?;
        let mut by_name: std::collections::BTreeMap<&str, u16> = std::collections::BTreeMap::new();
        for (j, d) in program.params.iter().enumerate() {
            if by_name.insert(d.name.as_str(), j as u16).is_some() {
                return Err(PalwArtErrorV3::Header(format!("two params named `{}`", d.name)));
            }
        }
        let want: std::collections::BTreeSet<(u16, Option<u16>)> = crate::param_instances_v1(program)
            .into_iter()
            .enumerate()
            .flat_map(|(j, inst)| inst.into_iter().map(move |l| (j as u16, l)))
            .collect();
        let mut index = std::collections::BTreeMap::new();
        for (ei, e) in s.entries.iter().enumerate() {
            let j = *by_name
                .get(e.name.as_str())
                .ok_or_else(|| PalwArtErrorV3::Entry { name: e.name.clone(), why: "not a param of the program".into() })?;
            let l = match e.layer {
                None => None,
                Some(l) => Some(u16::try_from(l).map_err(|_| PalwArtErrorV3::Entry { name: e.name.clone(), why: format!("layer {l}") })?),
            };
            let bytes = crate::tensor_bytes_v1(program, j);
            if !want.contains(&(j, l)) || e.bytes != bytes || e.encoding != encoding::TIR_INSTANCE_LE {
                return Err(PalwArtErrorV3::Entry {
                    name: e.name.clone(),
                    why: format!("layer {l:?}: not an instance of the program, or not {bytes} bytes of TIR instance"),
                });
            }
            if index.insert((j, l), ei).is_some() {
                return Err(PalwArtErrorV3::Entry { name: e.name.clone(), why: format!("layer {l:?} twice") });
            }
        }
        if index.len() != want.len() {
            return Err(PalwArtErrorV3::Header(format!(
                "the TIR section has {} of the program's {} instances",
                index.len(),
                want.len()
            )));
        }
        let decls = program.params.iter().map(|d| (d.dtype, d.shape.iter().map(|x| *x as usize).collect(), d.name.clone())).collect();
        Ok(Self { art, section, decls, index })
    }

    /// One instance's bytes, digest-checked.
    pub fn read_instance(&self, param: u16, layer: Option<u16>) -> ResultV3<Vec<u8>> {
        let ei = *self
            .index
            .get(&(param, layer))
            .ok_or_else(|| PalwArtErrorV3::Entry { name: format!("param {param}"), why: format!("layer {layer:?} absent") })?;
        self.art.read_entry(self.section, ei)
    }

    /// A byte range of one instance (no digest check: the caller re-roots what it reads).
    pub fn read_instance_range(&self, param: u16, layer: Option<u16>, start: u64, out: &mut [u8]) -> ResultV3<()> {
        let ei = *self
            .index
            .get(&(param, layer))
            .ok_or_else(|| PalwArtErrorV3::Entry { name: format!("param {param}"), why: format!("layer {layer:?} absent") })?;
        let e = &self.art.header.sections[self.section].entries[ei];
        if start + out.len() as u64 > e.bytes {
            return Err(PalwArtErrorV3::Entry { name: e.name.clone(), why: "range past the instance".into() });
        }
        self.art.read_at(e.offset + start, out)
    }
}

impl misaka_palw_tir::ParamSource for PalwArtTirParamsV3<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<misaka_palw_tir::Tensor> {
        let (dtype, shape, _) = self.decls.get(index as usize)?;
        let bytes = self.read_instance(index, layer).ok()?;
        misaka_palw_tir::Tensor::from_le_bytes(*dtype, shape, &bytes).ok()
    }
}
