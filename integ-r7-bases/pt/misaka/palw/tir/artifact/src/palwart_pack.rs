//! **Packing PALW artifacts into the one `PALWART3` container** (genesis 8, user decision 2026-10-15).
//!
//! Every genesis model ships as one `PALWART3` artifact ([`crate::palwart_v3`]). This module fills it,
//! and depends on nothing heavier than this crate (no consensus crate, no converter), so the packer
//! builds and runs whatever state the node crates are in:
//!
//! * [`pack_legacy_v3`] carries an artifact a legacy converter wrote — `PALWB0A1`/`PALWB0A2` (BASE-0
//!   A16, `qwen25-convert --a16`), `PALWTIR1` (`palw-tir-convert`) or `PALWQ361` (the Qwen3.6 integer
//!   runtime) — into the container's weight section, without touching a weight byte, and keeps its
//!   legacy identity as the section's inner root:
//!   - A16: the `artifact_digest` the file declares in its last 64 bytes (the BASE-0 reader refuses a
//!     file whose recomputed digest differs; `misaka_palw_sdk::palwart_deep` recomputes it);
//!   - TIR: the consensus inventory root, computed here by [`tir_inventory_root_twin_v1`] — a twin of
//!     `kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1` (leaf
//!     `misaka-palw/artifact/leaf/v1`, node `misaka-palw/artifact/node/v1`, odd nodes promoted),
//!     pinned equal to the consensus function by `misaka-palw-sdk/tests/palwart_pack.rs`, as this
//!     crate's `param_instances_v1` is pinned to its consensus twin;
//!   - PALWQ361: the legacy file digest.
//! * [`pack_common_v3`] adds the tokenizer, the chat template, the source metadata, the license texts
//!   and the calibration inputs, read as DATA from the checkpoint directory (nothing there is run).
//! * The manifest (source repository and revision pin, license, attribution, conversion) is supplied
//!   by the caller (the genesis pipeline) and written canonically; it is part of the root.
//!
//! The writer streams: a segment hook is called with every finished segment (the CLI's
//! `--on-segment` uploads it and deletes it), so peak disk is the legacy input plus one segment.

use crate::palwart_v3::{
    self as v3, PalwArtProgramV3, PalwArtSegmentHookV3, PalwArtWriterV3, PalwArtWrittenV3, PalwArtifactFormatV3, dtype, encoding,
    inner_scheme, program_kind, section_kind,
};
use crate::{PalwTirContainerV1, param_instances_v1, tensor_bytes_v1};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Tokenizer files carried when the checkpoint has them (data only).
pub const TOKENIZER_FILES_V3: &[&str] = &[
    "tokenizer.json",
    "tokenizer_config.json",
    "special_tokens_map.json",
    "added_tokens.json",
    "vocab.json",
    "merges.txt",
    "tokenizer.model",
];
/// Chat templates (a Jinja text, never evaluated here).
pub const CHAT_TEMPLATE_FILES_V3: &[&str] = &["chat_template.jinja", "chat_template.json"];
/// The checkpoint's configuration, kept for anyone re-deriving the program. The image/video/audio processor configs are not
/// carried: the genesis scope is text generation only (`palwart_v3::PALWART_OUT_OF_SCOPE_FILES_V3`).
pub const SOURCE_METADATA_FILES_V3: &[&str] =
    &["config.json", "generation_config.json", "model.safetensors.index.json", "configuration.json"];
/// License and notice files shipped by the source repository.
pub const SOURCE_LICENSE_FILES_V3: &[&str] = &["LICENSE", "LICENSE.txt", "LICENSE.md", "NOTICE", "NOTICE.txt", "USE_POLICY.md"];
/// The largest metadata file read (a tokenizer.json is tens of MiB).
pub const MAX_SMALL_FILE_V3: u64 = 256 << 20;

/// The consensus artifact-tree domains (`kaspa_consensus_core::palw_artifact`), for the twin.
pub const TWIN_LEAF_DOMAIN_V1: &[u8] = b"misaka-palw/artifact/leaf/v1";
pub const TWIN_NODE_DOMAIN_V1: &[u8] = b"misaka-palw/artifact/node/v1";
/// `PALW_TIR_ROW_PIECE_BYTES_V1`: the most bytes one leaf holds.
pub const TWIN_ROW_PIECE_BYTES_V1: u64 = 32_768;

/// What a pack needs besides the legacy artifact.
#[derive(Clone, Debug)]
pub struct PalwArtPackInputsV3 {
    /// The checkpoint directory the tokenizer, templates, configs and license files are read from.
    pub source_dir: Option<PathBuf>,
    /// The caller's manifest (a JSON object): source repo, revision, files, license, attribution,
    /// conversion. Written canonically, with the container and weight records added.
    pub manifest: serde_json::Value,
    pub family: u16,
    pub out_dir: PathBuf,
    pub stem: String,
    pub segment_bytes: u64,
    /// Extra license texts (the Apache-2.0 text for a source without one, a NOTICE).
    pub license_files: Vec<PathBuf>,
    /// Calibration statistics and token files (a rebuild's inputs).
    pub calibration_files: Vec<PathBuf>,
    /// Provenance (not identity).
    pub provenance: serde_json::Value,
}

/// The weight section a pack wrote.
#[derive(Clone, Debug)]
pub struct PalwArtWeightsV3 {
    pub legacy_format: PalwArtifactFormatV3,
    pub section_kind: u16,
    pub inner_root_scheme: u16,
    pub inner_root: [u8; 64],
    pub inner_leaf_count: u64,
    pub bytes: u64,
}

/// What [`pack_legacy_v3`] wrote.
#[derive(Clone, Debug)]
pub struct PalwArtPackedV3 {
    pub written: PalwArtWrittenV3,
    pub weights: PalwArtWeightsV3,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn s<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// A regular file (a symlink is followed; its target must be a regular file). Refuses anything else.
fn regular_file(p: &Path) -> Result<Option<u64>, String> {
    match std::fs::metadata(p) {
        Ok(m) if m.is_file() => Ok(Some(m.len())),
        Ok(_) => Err(format!("{}: not a regular file", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

#[cfg(unix)]
fn pread(f: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.read_exact_at(buf, at)
}

#[cfg(not(unix))]
fn pread(f: &std::fs::File, buf: &mut [u8], at: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut f = f.try_clone()?;
    f.seek(SeekFrom::Start(at))?;
    f.read_exact(buf)
}

// ---- the twin of the consensus TIR inventory root ------------------------------------------------

fn twin_node(l: &[u8; 64], r: &[u8; 64]) -> [u8; 64] {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(TWIN_NODE_DOMAIN_V1).to_state();
    st.update(l);
    st.update(r);
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// `artifact_leaf_parts_v1`'s twin: the position and the length, then the bytes.
pub fn twin_leaf_v1(tensor_name: &str, layer: Option<u16>, row_start: u32, bytes: &[u8]) -> [u8; 64] {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(TWIN_LEAF_DOMAIN_V1).to_state();
    st.update(&(tensor_name.len() as u32).to_le_bytes());
    st.update(tensor_name.as_bytes());
    st.update(&layer.map_or(u32::MAX, u32::from).to_le_bytes());
    st.update(&row_start.to_le_bytes());
    st.update(&(bytes.len() as u32).to_le_bytes());
    st.update(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// `PalwArtifactMerkleFrontierV1`'s twin: one peak per level, odd nodes promoted.
#[derive(Clone, Debug, Default)]
pub struct TwinFrontierV1 {
    peaks: Vec<Option<[u8; 64]>>,
    count: u64,
}

impl TwinFrontierV1 {
    pub fn push(&mut self, leaf: [u8; 64]) {
        let mut carry = leaf;
        let mut level = 0;
        loop {
            if level == self.peaks.len() {
                self.peaks.push(None);
            }
            match self.peaks[level].take() {
                Some(left) => {
                    carry = twin_node(&left, &carry);
                    level += 1;
                }
                None => {
                    self.peaks[level] = Some(carry);
                    break;
                }
            }
        }
        self.count += 1;
    }
    pub fn leaf_count(&self) -> u64 {
        self.count
    }
    pub fn root(&self) -> Option<[u8; 64]> {
        let mut acc: Option<[u8; 64]> = None;
        for peak in self.peaks.iter().flatten() {
            acc = Some(match acc {
                None => *peak,
                Some(right) => twin_node(peak, &right),
            });
        }
        acc
    }
}

/// One row of param `j` (an axis-0 slice at rank ≥ 2, the whole tensor below).
pub fn twin_row_bytes_v1(p: &misaka_palw_tir::TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    if d.shape.len() >= 2 {
        d.shape[1..].iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
    } else {
        tensor_bytes_v1(p, j)
    }
}

/// **The TIR inventory root of a `PALWTIR1` file**, by the consensus rule (see the module docs):
/// params in declaration order, each param's instances ascending, every row cut into pieces of at
/// most 32 KiB, each piece one leaf. Refuses what the consensus function refuses (an instance over
/// 4 GiB, more than `u32::MAX` leaves, no leaf). Returns `(root, leaf_count)`.
pub fn tir_inventory_root_twin_v1(c: &PalwTirContainerV1) -> Result<([u8; 64], u64), String> {
    let p = &c.program;
    let file = std::fs::File::open(&c.path).map_err(|e| format!("{}: {e}", c.path.display()))?;
    let mut frontier = TwinFrontierV1::default();
    let mut buf = vec![0u8; TWIN_ROW_PIECE_BYTES_V1 as usize];
    for (j, inst) in param_instances_v1(p).into_iter().enumerate() {
        let j = j as u16;
        let name = &p.params[j as usize].name;
        let (t, r) = (tensor_bytes_v1(p, j), twin_row_bytes_v1(p, j));
        if t > u32::MAX as u64 {
            return Err(format!("`{name}`: an instance of {t} bytes (the inventory addresses at most 4 GiB)"));
        }
        if t == 0 || r == 0 {
            continue;
        }
        for layer in inst {
            let (off, bytes) = c.locate(j, layer).ok_or_else(|| format!("`{name}` layer {layer:?} is not in the file"))?;
            if bytes != t {
                return Err(format!("`{name}` layer {layer:?}: {bytes} bytes, declared {t}"));
            }
            let mut row = 0u64;
            while row + r <= t {
                let mut at = 0u64;
                while at < r {
                    let len = (r - at).min(TWIN_ROW_PIECE_BYTES_V1);
                    let piece = &mut buf[..len as usize];
                    pread(&file, piece, off + row + at).map_err(|e| format!("{}: {e}", c.path.display()))?;
                    frontier.push(twin_leaf_v1(name, layer, (row + at) as u32, piece));
                    at += len;
                }
                row += r;
            }
        }
    }
    let n = frontier.leaf_count();
    if n > u32::MAX as u64 {
        return Err(format!("{n} leaves exceed the inventory's u32 count"));
    }
    let root = frontier.root().ok_or("the program declares no param leaf")?;
    Ok((root, n))
}

/// `palw_tir_graph_ir_root_v1`'s twin: BLAKE2b-512 keyed `misaka-palw/tir/graph-ir-root/v1`.
pub fn twin_graph_ir_root_v1(program_bytes: &[u8]) -> [u8; 64] {
    let h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tir/graph-ir-root/v1").hash(program_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.as_bytes());
    out
}

// ---- packing -----------------------------------------------------------------------------------

/// **Pack a legacy artifact**, then the common sections, then the manifest. Returns the written
/// index (with its root) and the weight section's record.
pub fn pack_legacy_v3(legacy: &Path, inputs: &PalwArtPackInputsV3, hook: PalwArtSegmentHookV3<'_>) -> Result<PalwArtPackedV3, String> {
    if !inputs.manifest.is_object() {
        return Err("the manifest must be a JSON object".into());
    }
    let format = v3::sniff_format_v3(legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
    if !matches!(
        format,
        PalwArtifactFormatV3::Base0A16V2
            | PalwArtifactFormatV3::Base0V1
            | PalwArtifactFormatV3::TirV1
            | PalwArtifactFormatV3::PalwQ361
    ) {
        return Err(format!(
            "{}: {} is not packed (supported: PALWB0A1/PALWB0A2, PALWTIR1, PALWQ361)",
            legacy.display(),
            format.name()
        ));
    }
    let mut w = PalwArtWriterV3::create(&inputs.out_dir, &inputs.stem, inputs.segment_bytes, hook).map_err(s)?;
    let (program, layout, legacy_meta, weights) = match format {
        PalwArtifactFormatV3::TirV1 => pack_tir_v1_v3(&mut w, legacy, inputs.family)?,
        PalwArtifactFormatV3::PalwQ361 => pack_file_v3(&mut w, legacy, inputs.family, format)?,
        _ => pack_file_v3(&mut w, legacy, inputs.family, format)?,
    };
    pack_common_v3(&mut w, inputs)?;
    let manifest = manifest_bytes_v3(&inputs.manifest, inputs.family, &weights);
    if v3::manifest_declares_text_scope_v3(&manifest) {
        v3::check_text_scope_v3(&program, w.sections())?;
    }
    let mut prov = inputs.provenance.clone();
    if !prov.is_object() {
        prov = serde_json::json!({});
    }
    prov["packer"] = serde_json::json!(format!("palwart pack {}", env!("CARGO_PKG_VERSION")));
    prov["legacy_file"] = serde_json::json!({
        "name": legacy.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "format": format.name(),
        "bytes": std::fs::metadata(legacy).map(|m| m.len()).unwrap_or(0),
    });
    let written = w.finish(manifest, program, layout, legacy_meta, v3::canonical_json_v3(&prov).into_bytes()).map_err(s)?;
    Ok(PalwArtPackedV3 { written, weights })
}

/// The manifest as written: the caller's object plus the container and weight records, canonical.
pub fn manifest_bytes_v3(manifest: &serde_json::Value, family: u16, w: &PalwArtWeightsV3) -> Vec<u8> {
    let mut m = manifest.clone();
    m["palwart"] = serde_json::json!({ "format": "PALWART3", "version": v3::PALWART_VERSION_V3 });
    m["weights"] = serde_json::json!({
        "legacy_format": w.legacy_format.name(),
        "section": section_kind::name(w.section_kind),
        "family": v3::family::name(family),
        "inner_root_scheme": w.inner_root_scheme,
        "inner_root": hex(&w.inner_root),
        "inner_leaf_count": w.inner_leaf_count,
        "bytes": w.bytes,
    });
    v3::canonical_json_v3(&m).into_bytes()
}

type Packed = (PalwArtProgramV3, Vec<u8>, String, PalwArtWeightsV3);

/// A legacy file carried whole (A16 or PALWQ361), streamed: its file digest is computed on the way;
/// an A16 file's declared `artifact_digest` (its last 64 bytes) is its inner root.
fn pack_file_v3(w: &mut PalwArtWriterV3<'_>, legacy: &Path, family: u16, format: PalwArtifactFormatV3) -> Result<Packed, String> {
    let len = regular_file(legacy)?.ok_or_else(|| format!("{}: absent", legacy.display()))?;
    let mut f = std::fs::File::open(legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
    let (packed, _) = pack_reader_v3(w, &mut f, len, &legacy.display().to_string(), family, format, None)?;
    Ok(packed)
}

/// A legacy A16 / PALWQ361 file carried whole from any reader (a local file, or an HTTPS stream that never lands on disk): its
/// SHA-256 and its legacy digest are computed on the way, and with `expect_sha256` the bytes must be the pinned ones — checked
/// before the caller can finish the index. Returns the section record and the SHA-256.
fn pack_reader_v3(
    w: &mut PalwArtWriterV3<'_>,
    f: &mut dyn Read,
    len: u64,
    label: &str,
    family: u16,
    format: PalwArtifactFormatV3,
    expect_sha256: Option<[u8; 32]>,
) -> Result<(Packed, [u8; 32]), String> {
    use sha2::{Digest as _, Sha256};
    let a16 = matches!(format, PalwArtifactFormatV3::Base0A16V2 | PalwArtifactFormatV3::Base0V1);
    if a16 && len < 8 + 64 {
        return Err(format!("{label}: {len} bytes cannot hold a BASE-0 artifact"));
    }
    let (kind, enc, entry, prog) = if a16 {
        (section_kind::BASE0_A16, encoding::BASE0_A16_FILE, "base0.a16.palwart", program_kind::BASE0_A16_ENGINE)
    } else {
        (section_kind::PALWQ361_INT, encoding::PALWQ361_FILE, "qwen36.palwq36", program_kind::PALWQ361_ENGINE)
    };
    let mut st = v3::legacy_file_digest_state_v3();
    let mut sha = Sha256::new();
    w.begin_section(kind, family, section_kind::name(kind)).map_err(s)?;
    w.begin_entry(entry, None, dtype::BYTES, vec![len], enc, len).map_err(s)?;
    let mut buf = vec![0u8; 8 << 20];
    let mut got = 0u64;
    let mut tail = Vec::with_capacity(64);
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("{label}: {e}"))?;
        if n == 0 {
            break;
        }
        if got + n as u64 > len {
            return Err(format!("{label}: more than the declared {len} bytes"));
        }
        st.update(&buf[..n]);
        sha.update(&buf[..n]);
        w.write(&buf[..n]).map_err(s)?;
        got += n as u64;
        tail.extend_from_slice(&buf[..n]);
        if tail.len() > 64 {
            tail.drain(..tail.len() - 64);
        }
    }
    if got != len {
        return Err(format!("{label}: {got} of {len} bytes (changed while read, or the stream ended)"));
    }
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&sha.finalize());
    if let Some(want) = expect_sha256
        && want != sha256
    {
        return Err(format!("{label}: SHA-256 {} is not the pinned {}", hex(&sha256), hex(&want)));
    }
    w.end_entry().map_err(s)?;
    let mut file_digest = [0u8; 64];
    file_digest.copy_from_slice(st.finalize().as_bytes());
    let (scheme, root) = if a16 {
        let mut d = [0u8; 64];
        d.copy_from_slice(&tail);
        (inner_scheme::BASE0_ARTIFACT_DIGEST_V1, d)
    } else {
        (inner_scheme::LEGACY_FILE_DIGEST_V1, file_digest)
    };
    w.end_section(scheme, root, 0).map_err(s)?;
    let program = PalwArtProgramV3 { kind: prog, ..PalwArtProgramV3::none() };
    let weights = PalwArtWeightsV3 {
        legacy_format: format,
        section_kind: kind,
        inner_root_scheme: scheme,
        inner_root: root,
        inner_leaf_count: 0,
        bytes: len,
    };
    Ok(((program, Vec::new(), String::new(), weights), sha256))
}

/// A legacy file arriving as a stream (genesis 8: #1's and #8's published legacy files, read over HTTPS and never stored).
pub struct LegacyStreamV3<'r> {
    pub reader: &'r mut dyn Read,
    /// The file's length (the registry's).
    pub len: u64,
    /// Its name (provenance only).
    pub name: String,
    /// The LFS id the bytes must have; the index is not written otherwise. `None` for a file a converter is writing into the
    /// pipe now (genesis #8: `qwen36-convert --measured-in … --out FIFO`), which has no published id yet — its identity is the
    /// container root, and the converter checked its own input.
    pub sha256: Option<[u8; 32]>,
}

/// **Pack a streamed legacy file** (A16 or PALWQ361; a `PALWTIR1` needs random access and is refused). The weight entry is the
/// first entry of the data, at logical offset 0, so every segment but the last is an exact byte slice of the legacy file — which
/// lets a content-defined-chunking store (Hugging Face Xet) deduplicate the upload against the legacy file already there.
pub fn pack_legacy_stream_v3(
    src: LegacyStreamV3<'_>,
    inputs: &PalwArtPackInputsV3,
    hook: PalwArtSegmentHookV3<'_>,
) -> Result<PalwArtPackedV3, String> {
    if !inputs.manifest.is_object() {
        return Err("the manifest must be a JSON object".into());
    }
    let mut magic = [0u8; 8];
    src.reader.read_exact(&mut magic).map_err(|e| format!("{}: {e}", src.name))?;
    let format = PalwArtifactFormatV3::of_magic(&magic);
    if !matches!(format, PalwArtifactFormatV3::Base0A16V2 | PalwArtifactFormatV3::Base0V1 | PalwArtifactFormatV3::PalwQ361) {
        return Err(format!("{}: {} is not packed from a stream (supported: PALWB0A1/PALWB0A2, PALWQ361)", src.name, format.name()));
    }
    let mut w = PalwArtWriterV3::create(&inputs.out_dir, &inputs.stem, inputs.segment_bytes, hook).map_err(s)?;
    let mut whole = std::io::Cursor::new(magic).chain(src.reader);
    let ((program, layout, legacy_meta, weights), sha256) =
        pack_reader_v3(&mut w, &mut whole, src.len, &src.name, inputs.family, format, src.sha256)?;
    pack_common_v3(&mut w, inputs)?;
    let manifest = manifest_bytes_v3(&inputs.manifest, inputs.family, &weights);
    if v3::manifest_declares_text_scope_v3(&manifest) {
        v3::check_text_scope_v3(&program, w.sections())?;
    }
    let mut prov = inputs.provenance.clone();
    if !prov.is_object() {
        prov = serde_json::json!({});
    }
    prov["packer"] = serde_json::json!(format!("palwart pack {} (streamed)", env!("CARGO_PKG_VERSION")));
    prov["legacy_file"] = serde_json::json!({ "name": src.name, "format": format.name(), "bytes": src.len, "sha256": hex(&sha256) });
    let written = w.finish(manifest, program, layout, legacy_meta, v3::canonical_json_v3(&prov).into_bytes()).map_err(s)?;
    Ok(PalwArtPackedV3 { written, weights })
}

fn pack_tir_v1_v3(w: &mut PalwArtWriterV3<'_>, legacy: &Path, family: u16) -> Result<Packed, String> {
    let c = PalwTirContainerV1::open(legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
    // The legacy identity: the consensus inventory root over the legacy file (the twin, pinned).
    let (root, leaves) = tir_inventory_root_twin_v1(&c)?;
    let file = std::fs::File::open(legacy).map_err(|e| format!("{}: {e}", legacy.display()))?;
    w.begin_section(section_kind::TIR_INVENTORY, family, "tir-inventory").map_err(s)?;
    let mut buf = vec![0u8; 8 << 20];
    let mut total = 0u64;
    for (j, inst) in param_instances_v1(&c.program).into_iter().enumerate() {
        let j = j as u16;
        let d = &c.program.params[j as usize];
        let code = dtype::of_tir(d.dtype).ok_or_else(|| format!("param `{}`: dtype {:?} is never a param", d.name, d.dtype))?;
        for layer in inst {
            let (off, bytes) = c.locate(j, layer).ok_or_else(|| format!("param `{}` layer {layer:?} is not in the file", d.name))?;
            let shape: Vec<u64> = d.shape.iter().map(|x| *x as u64).collect();
            w.begin_entry(&d.name, layer.map(u32::from), code, shape, encoding::TIR_INSTANCE_LE, bytes).map_err(s)?;
            let mut at = 0u64;
            while at < bytes {
                let n = ((bytes - at) as usize).min(buf.len());
                pread(&file, &mut buf[..n], off + at).map_err(|e| format!("{}: {e}", legacy.display()))?;
                w.write(&buf[..n]).map_err(s)?;
                at += n as u64;
            }
            w.end_entry().map_err(s)?;
            total += bytes;
        }
    }
    w.end_section(inner_scheme::TIR_INVENTORY_ROOT_V1, root, leaves).map_err(s)?;
    let program = PalwArtProgramV3 {
        kind: program_kind::TIR_V1,
        bytes: c.header.program.clone(),
        kernel_id: twin_graph_ir_root_v1(&c.header.program),
        kernel_version: 0,
        tokenizer_id: c.header.tokenizer_id,
    };
    let weights = PalwArtWeightsV3 {
        legacy_format: PalwArtifactFormatV3::TirV1,
        section_kind: section_kind::TIR_INVENTORY,
        inner_root_scheme: inner_scheme::TIR_INVENTORY_ROOT_V1,
        inner_root: root,
        inner_leaf_count: leaves,
        bytes: total,
    };
    Ok((program, c.header.layout.clone(), c.header.meta.clone(), weights))
}

/// The common sections: tokenizer, chat template, source metadata, license, calibration. Each file
/// is read as bytes; a section with no file is not written.
pub fn pack_common_v3(w: &mut PalwArtWriterV3<'_>, inputs: &PalwArtPackInputsV3) -> Result<(), String> {
    let src = inputs.source_dir.as_deref();
    let present = |names: &[&str]| -> Result<Vec<(String, PathBuf)>, String> {
        let mut out = Vec::new();
        if let Some(dir) = src {
            for n in names {
                let p = dir.join(n);
                if let Some(len) = regular_file(&p)? {
                    if len > MAX_SMALL_FILE_V3 {
                        return Err(format!("{}: {len} bytes exceeds the metadata cap", p.display()));
                    }
                    out.push((n.to_string(), p));
                }
            }
        }
        Ok(out)
    };
    let text_encoding = |name: &str| if name.ends_with(".json") { encoding::JSON } else { encoding::UTF8_TEXT };

    let tok = present(TOKENIZER_FILES_V3)?;
    if !tok.is_empty() {
        w.begin_section(section_kind::TOKENIZER, v3::family::NONE, "tokenizer").map_err(s)?;
        for (n, p) in &tok {
            let enc = if n == "tokenizer.model" { encoding::RAW_LE } else { text_encoding(n) };
            w.put_file(n, enc, p).map_err(s)?;
        }
        w.end_section(inner_scheme::NONE, [0; 64], 0).map_err(s)?;
    }

    // Chat template: a template file, else the `chat_template` of tokenizer_config.json, as text.
    let tpl = present(CHAT_TEMPLATE_FILES_V3)?;
    let mut embedded: Option<String> = None;
    if tpl.is_empty()
        && let Some(dir) = src
        && let Some(len) = regular_file(&dir.join("tokenizer_config.json"))?
        && len <= MAX_SMALL_FILE_V3
    {
        let bytes = std::fs::read(dir.join("tokenizer_config.json")).map_err(s)?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            match v.get("chat_template") {
                Some(serde_json::Value::String(t)) => embedded = Some(t.clone()),
                Some(other @ serde_json::Value::Array(_)) => embedded = Some(other.to_string()),
                _ => {}
            }
        }
    }
    if !tpl.is_empty() || embedded.is_some() {
        w.begin_section(section_kind::CHAT_TEMPLATE, v3::family::NONE, "chat-template").map_err(s)?;
        for (n, p) in &tpl {
            w.put_file(n, text_encoding(n), p).map_err(s)?;
        }
        if let Some(t) = embedded {
            w.put_entry(
                "tokenizer_config.json#chat_template",
                None,
                dtype::BYTES,
                vec![t.len() as u64],
                encoding::UTF8_TEXT,
                t.as_bytes(),
            )
            .map_err(s)?;
        }
        w.end_section(inner_scheme::NONE, [0; 64], 0).map_err(s)?;
    }

    let meta = present(SOURCE_METADATA_FILES_V3)?;
    if !meta.is_empty() {
        w.begin_section(section_kind::SOURCE_METADATA, v3::family::NONE, "source-metadata").map_err(s)?;
        for (n, p) in &meta {
            w.put_file(n, text_encoding(n), p).map_err(s)?;
        }
        w.end_section(inner_scheme::NONE, [0; 64], 0).map_err(s)?;
    }

    // License: the source's own files, then the ones the caller adds.
    let mut lic: Vec<(String, PathBuf)> =
        present(SOURCE_LICENSE_FILES_V3)?.into_iter().map(|(n, p)| (format!("source/{n}"), p)).collect();
    for p in &inputs.license_files {
        let len = regular_file(p)?.ok_or_else(|| format!("{}: absent", p.display()))?;
        if len > MAX_SMALL_FILE_V3 {
            return Err(format!("{}: too large for a license text", p.display()));
        }
        let n = p.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{}: no file name", p.display()))?;
        lic.push((format!("added/{n}"), p.clone()));
    }
    if !lic.is_empty() {
        w.begin_section(section_kind::LICENSE, v3::family::NONE, "license").map_err(s)?;
        for (n, p) in &lic {
            w.put_file(n, encoding::UTF8_TEXT, p).map_err(s)?;
        }
        w.end_section(inner_scheme::NONE, [0; 64], 0).map_err(s)?;
    }

    if !inputs.calibration_files.is_empty() {
        w.begin_section(section_kind::CALIBRATION, v3::family::NONE, "calibration").map_err(s)?;
        for p in &inputs.calibration_files {
            regular_file(p)?.ok_or_else(|| format!("{}: absent", p.display()))?;
            let n = p.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{}: no file name", p.display()))?;
            w.put_file(n, text_encoding(n), p).map_err(s)?;
        }
        w.end_section(inner_scheme::NONE, [0; 64], 0).map_err(s)?;
    }
    Ok(())
}

/// A summary of an opened artifact (what `palwart inspect` prints).
pub fn summary_json_v3(a: &v3::PalwArtV3) -> serde_json::Value {
    let h = &a.header;
    let manifest: serde_json::Value = serde_json::from_slice(&h.manifest).unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "format": "PALWART3",
        "version": h.version,
        "root": a.root_hex(),
        "index": a.index_path.display().to_string(),
        "data_bytes": h.data_bytes,
        "segment_bytes": h.segment_bytes,
        "segments": h.segments.iter().enumerate().map(|(k, sg)| serde_json::json!({
            "file": v3::segment_file_name_v3(&a.stem, k as u32),
            "bytes": sg.bytes,
            "sha256": hex(&sg.sha256),
        })).collect::<Vec<_>>(),
        "program": {
            "kind": h.program.kind,
            "bytes": h.program.bytes.len(),
            "kernel_id": hex(&h.program.kernel_id),
            "kernel_version": h.program.kernel_version,
            "tokenizer_id": hex(&h.program.tokenizer_id),
        },
        "sections": h.sections.iter().map(|sc| serde_json::json!({
            "kind": section_kind::name(sc.kind),
            "family": v3::family::name(sc.family),
            "name": sc.name,
            "inner_root_scheme": sc.inner_root_scheme,
            "inner_root": hex(&sc.inner_root),
            "inner_leaf_count": sc.inner_leaf_count,
            "entries": sc.entries.len(),
            "bytes": sc.entries.iter().map(|e| e.bytes).sum::<u64>(),
        })).collect::<Vec<_>>(),
        "layout_bytes": h.layout.len(),
        "manifest": manifest,
    })
}

/// **Re-assemble the legacy file** a section carries: the A16 / PALWQ361 bytes as they were, or a
/// `PALWTIR1` container written from the TIR section (same program, layout, tokenizer id, meta and
/// instances: byte-identical to the file that was packed). Lets every legacy tool run unchanged.
pub fn extract_legacy_v3(a: &v3::PalwArtV3, out: &Path) -> Result<PalwArtifactFormatV3, String> {
    let (si, sec) = a.weights().ok_or("the artifact has no weight section")?;
    if out.exists() {
        return Err(format!("{} exists", out.display()));
    }
    match sec.kind {
        section_kind::BASE0_A16 | section_kind::PALWQ361_INT => {
            if sec.entries.len() != 1 {
                return Err("a legacy-file section holds exactly one entry".into());
            }
            let mut f =
                std::fs::OpenOptions::new().write(true).create_new(true).open(out).map_err(|e| format!("{}: {e}", out.display()))?;
            a.stream_entry(si, 0, 8 << 20, &mut |b| f.write_all(b).map_err(|e| e.to_string())).map_err(s)?;
            f.sync_all().map_err(s)?;
            Ok(PalwArtifactFormatV3::of_magic(&sec_magic(a, si)?))
        }
        section_kind::TIR_INVENTORY => {
            if a.header.program.kind != program_kind::TIR_V1 {
                return Err("a TIR section without a TIR v1 program".into());
            }
            let program = misaka_palw_tir::TirProgramV1::decode_canonical(&a.header.program.bytes).map_err(s)?;
            let params = v3::PalwArtTirParamsV3::bind(a, &program).map_err(s)?;
            let mut buf = vec![0u8; 8 << 20];
            crate::write_container_v1_streamed(
                out,
                &program,
                a.header.layout.clone(),
                a.header.program.tokenizer_id,
                a.header.legacy_meta.clone(),
                &mut |j, l, sink| {
                    let bytes = tensor_bytes_v1(&program, j);
                    let mut at = 0u64;
                    while at < bytes {
                        let n = ((bytes - at) as usize).min(buf.len());
                        params.read_instance_range(j, l, at, &mut buf[..n]).map_err(|e| e.to_string())?;
                        sink.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                        at += n as u64;
                    }
                    Ok(())
                },
            )
            .map_err(s)?;
            // The extracted file must root to what the section claims.
            let c = PalwTirContainerV1::open(out).map_err(s)?;
            let (root, leaves) = tir_inventory_root_twin_v1(&c)?;
            if root != sec.inner_root || leaves != sec.inner_leaf_count {
                return Err("the re-assembled PALWTIR1 file does not root to the section's inner root".into());
            }
            Ok(PalwArtifactFormatV3::TirV1)
        }
        other => Err(format!("section kind {} has no legacy form", section_kind::name(other))),
    }
}

fn sec_magic(a: &v3::PalwArtV3, si: usize) -> Result<[u8; 8], String> {
    let e = &a.header.sections[si].entries[0];
    let mut m = [0u8; 8];
    if e.bytes < 8 {
        return Err("a legacy entry shorter than its magic".into());
    }
    a.read_at(e.offset, &mut m).map_err(s)?;
    Ok(m)
}

/// **Verify an artifact** without the consensus crate: the index (structure and root, checked by
/// `open`), every present segment's SHA-256, and — when every segment is present — every entry digest,
/// the TIR section's inventory root (twin) and kernel id, the A16 section's declared digest against
/// the embedded file's trailer, the PALWQ361 section's file digest. The A16 `artifact_digest`
/// recomputation needs the BASE-0 decoder: `misaka_palw_sdk::palwart_deep`.
pub fn verify_v3(a: &v3::PalwArtV3) -> Result<serde_json::Value, String> {
    let missing = a.verify_segments(false).map_err(s)?;
    let mut out = serde_json::json!({ "root": a.root_hex(), "index": "ok", "segments_missing": missing });
    if !missing.is_empty() {
        out["entries"] = serde_json::json!("not checked: segments absent");
        return Ok(out);
    }
    a.verify_entries().map_err(s)?;
    out["entries"] = serde_json::json!("ok");
    if v3::manifest_declares_text_scope_v3(&a.header.manifest) {
        v3::check_text_scope_v3(&a.header.program, &a.header.sections)?;
        out["scope"] = serde_json::json!("ok (text generation only: no image, audio, video or MTP component)");
    }
    let Some((si, sec)) = a.weights() else { return Ok(out) };
    match sec.kind {
        section_kind::TIR_INVENTORY => {
            let program = misaka_palw_tir::TirProgramV1::decode_canonical(&a.header.program.bytes).map_err(s)?;
            let params = v3::PalwArtTirParamsV3::bind(a, &program).map_err(s)?;
            let mut frontier = TwinFrontierV1::default();
            let mut buf = vec![0u8; TWIN_ROW_PIECE_BYTES_V1 as usize];
            for (j, inst) in param_instances_v1(&program).into_iter().enumerate() {
                let j = j as u16;
                let name = &program.params[j as usize].name;
                let (t, r) = (tensor_bytes_v1(&program, j), twin_row_bytes_v1(&program, j));
                if t == 0 || r == 0 {
                    continue;
                }
                for layer in inst {
                    let mut row = 0u64;
                    while row + r <= t {
                        let mut at = 0u64;
                        while at < r {
                            let len = (r - at).min(TWIN_ROW_PIECE_BYTES_V1);
                            let piece = &mut buf[..len as usize];
                            params.read_instance_range(j, layer, row + at, piece).map_err(s)?;
                            frontier.push(twin_leaf_v1(name, layer, (row + at) as u32, piece));
                            at += len;
                        }
                        row += r;
                    }
                }
            }
            if frontier.root() != Some(sec.inner_root) || frontier.leaf_count() != sec.inner_leaf_count {
                return Err("the TIR section's inventory root differs from its inner root".into());
            }
            if twin_graph_ir_root_v1(&a.header.program.bytes) != a.header.program.kernel_id {
                return Err("the program's kernel id is not its graph-IR root".into());
            }
            out["inner_root"] = serde_json::json!("ok (TIR inventory root)");
        }
        section_kind::BASE0_A16 => {
            let e = &sec.entries[0];
            let mut tail = [0u8; 64];
            a.read_at(e.offset + e.bytes - 64, &mut tail).map_err(s)?;
            if tail != sec.inner_root {
                return Err("the A16 section's inner root is not the embedded file's declared digest".into());
            }
            out["inner_root"] =
                serde_json::json!("ok (declared artifact_digest; recompute with palw-class / misaka_palw_sdk::palwart_deep)");
        }
        section_kind::PALWQ361_INT => {
            let mut st = v3::legacy_file_digest_state_v3();
            a.stream_entry(si, 0, 8 << 20, &mut |b| {
                st.update(b);
                Ok(())
            })
            .map_err(s)?;
            if st.finalize().as_bytes() != sec.inner_root {
                return Err("the PALWQ361 section's file digest differs from its inner root".into());
            }
            out["inner_root"] = serde_json::json!("ok (legacy file digest)");
        }
        _ => {}
    }
    Ok(out)
}
