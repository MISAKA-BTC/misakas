//! # `misaka-palw-tir-artifact` — the `PALWTIR1` container (RFC-0002 Phase F, F3)
//!
//! One file per IR artifact (`docs/design/palw/tir/phase-f-integration.md` §2.10): the program's
//! canonical bytes, its declared commitment layout and tokenizer id, and every param tensor.
//!
//! ```text
//! file   := "PALWTIR1" · version u16 LE · header_len u32 LE · header (borsh) · zeros to 64 · tensors
//! header := PalwTirContainerHeaderV1 { version, program, layout, tokenizer_id, tensors, meta }
//! tensor := the instance's bytes, little-endian two's complement in the declared dtype (idx:
//!           unsigned), at a 64-byte-aligned absolute file offset
//! ```
//!
//! * **Order.** Tensors appear in INVENTORY order: params in declaration order, each param's
//!   instances ascending (`None` for a global; every layer whose scheduled block reads a per-layer
//!   param). That is the order the consensus inventory (`kaspa_consensus_core::palw_tir_artifact_v1`)
//!   hashes them in, so a streamed root reads the file front to back.
//! * **Alignment.** Every tensor starts at a multiple of 64 bytes, so a mapped file serves `i16`,
//!   `i32` and `i64` params in place (the node's typed evaluator, design §2.10 "zero-copy params").
//! * **What is identity and what is not.** The container is NOT a consensus object. The program
//!   (through `graph_ir_root`), the layout, the tokenizer id and the tensors (through the inventory
//!   root) are what a class id commits to; the container's own digest ([`file_digest_v1`]) is what a
//!   `.palwmanifest` sidecar binds to the file; `meta` is provenance for humans (calibration set,
//!   scales, converter) and enters nothing.
//!
//! [`PalwTirContainerV1::open`] checks everything the header claims against the program before a
//! single tensor is believed: the program decodes canonically and validates, the tensor list is
//! exactly the program's instances in inventory order with the declared byte lengths, and the
//! offsets are aligned, in order, non-overlapping and inside the file.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Ref, Tensor, TirProgramV1};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// **`PALWTIR2`**: a pipeline class's container (RFC-0003) — N programs and one tensor table in the
/// pipeline inventory's order. A PALWTIR1 reader refuses it by its magic.
pub mod v2;
pub use v2::{
    PALW_TIR_CONTAINER_MAGIC_V2, PALW_TIR_CONTAINER_VERSION_V2, PalwTirContainerHeaderV2, PalwTirContainerProgramV2,
    PalwTirContainerV2, PalwTirTensorEntryV2, pipeline_instances_v2, write_container_v2,
};

pub const PALW_TIR_CONTAINER_MAGIC_V1: &[u8; 8] = b"PALWTIR1";
pub const PALW_TIR_CONTAINER_VERSION_V1: u16 = 1;
/// Every tensor starts at a multiple of this.
pub const PALW_TIR_TENSOR_ALIGN_V1: u64 = 64;
/// A header larger than this is refused before it is read (a program is ≤ 256 KiB, a layout and a
/// tensor table are small; the cap keeps a hostile file from sizing an allocation).
pub const PALW_TIR_MAX_HEADER_BYTES_V1: u32 = 64 << 20;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirTensorEntryV1 {
    pub param: u16,
    pub layer: Option<u16>,
    /// Absolute file offset, a multiple of [`PALW_TIR_TENSOR_ALIGN_V1`].
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirContainerHeaderV1 {
    pub version: u16,
    /// The program's canonical bytes (spec 04b §4).
    pub program: Vec<u8>,
    /// `borsh(PalwTirLayoutV1)` as the registrant declares it (design §2.3); empty when no layout is
    /// declared yet (a lowerer's output before the tool derives a default one).
    pub layout: Vec<u8>,
    pub tokenizer_id: [u8; 64],
    pub tensors: Vec<PalwTirTensorEntryV1>,
    /// Provenance (JSON by convention); enters no identity.
    pub meta: String,
}

#[derive(thiserror::Error, Debug)]
pub enum PalwTirContainerError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a PALWTIR1 container (magic or version)")]
    Magic,
    #[error("container header: {0}")]
    Header(String),
    #[error("the embedded program is refused: {0}")]
    Program(String),
    #[error("tensor table: {0}")]
    Table(String),
    #[error("param {param} (layer {layer:?}) is not in this container")]
    NoSuchTensor { param: u16, layer: Option<u16> },
    #[error("tensor bytes: {0}")]
    Bytes(String),
}

pub type Result<T> = std::result::Result<T, PalwTirContainerError>;

/// The instances of every param in inventory order: `[None]` for a global; for a per-layer param
/// every layer whose scheduled block references it, ascending. (The same rule as the consensus
/// inventory's `palw_tir_param_instances_v1`; the SDK's tests pin the two together.)
pub fn param_instances_v1(p: &TirProgramV1) -> Vec<Vec<Option<u16>>> {
    let mut readers: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); p.params.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for n in &b.nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r
                    && let Some(set) = readers.get_mut(*j as usize)
                {
                    set.insert(bi as u8);
                }
            }
        }
    }
    p.params
        .iter()
        .enumerate()
        .map(|(j, d)| {
            if d.per_layer {
                p.schedule.layers.iter().enumerate().filter(|(_, k)| readers[j].contains(k)).map(|(l, _)| Some(l as u16)).collect()
            } else {
                vec![None]
            }
        })
        .collect()
}

/// Bytes of one instance of param `j`.
pub fn tensor_bytes_v1(p: &TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
}

fn align(x: u64) -> u64 {
    x.div_ceil(PALW_TIR_TENSOR_ALIGN_V1) * PALW_TIR_TENSOR_ALIGN_V1
}

/// The fixed prefix: magic, version, header length.
const PREFIX: u64 = 8 + 2 + 4;

/// **Write a container**, one tensor at a time: `tensor(param, layer)` is asked for each instance
/// in inventory order and must return exactly its declared bytes. Returns the file digest
/// ([`file_digest_v1`]) of what was written.
pub fn write_container_v1(
    path: &Path,
    program: &TirProgramV1,
    layout: Vec<u8>,
    tokenizer_id: [u8; 64],
    meta: String,
    tensor: &mut dyn FnMut(u16, Option<u16>) -> std::result::Result<Vec<u8>, String>,
) -> Result<[u8; 64]> {
    misaka_palw_tir::validate::validate(program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
    let program_bytes = program.encode();
    let instances = param_instances_v1(program);
    // The table's size does not depend on the offsets (fixed-width fields), so lay it out with
    // zeros first, then fill the offsets in.
    let mut entries: Vec<PalwTirTensorEntryV1> = Vec::new();
    for (j, inst) in instances.iter().enumerate() {
        for l in inst {
            entries.push(PalwTirTensorEntryV1 { param: j as u16, layer: *l, offset: 0, bytes: tensor_bytes_v1(program, j as u16) });
        }
    }
    let mut header = PalwTirContainerHeaderV1 {
        version: PALW_TIR_CONTAINER_VERSION_V1,
        program: program_bytes,
        layout,
        tokenizer_id,
        tensors: entries,
        meta,
    };
    let header_len = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?.len() as u64;
    let mut at = align(PREFIX + header_len);
    for e in header.tensors.iter_mut() {
        e.offset = at;
        at = align(at + e.bytes);
    }
    let hb = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
    debug_assert_eq!(hb.len() as u64, header_len);
    let f = std::fs::File::create(path)?;
    let mut w = DigestWriter { inner: std::io::BufWriter::with_capacity(1 << 20, f), state: digest_state(), written: 0 };
    w.write_all(PALW_TIR_CONTAINER_MAGIC_V1)?;
    w.write_all(&PALW_TIR_CONTAINER_VERSION_V1.to_le_bytes())?;
    w.write_all(&(hb.len() as u32).to_le_bytes())?;
    w.write_all(&hb)?;
    for e in &header.tensors {
        let pad = e.offset - w.written;
        w.write_all(&vec![0u8; pad as usize])?;
        let bytes = tensor(e.param, e.layer).map_err(PalwTirContainerError::Bytes)?;
        if bytes.len() as u64 != e.bytes {
            let name = &program.params[e.param as usize].name;
            return Err(PalwTirContainerError::Bytes(format!(
                "`{name}` (layer {:?}): {} bytes, declared {}",
                e.layer,
                bytes.len(),
                e.bytes
            )));
        }
        check_values(program.params[e.param as usize].dtype, &bytes)
            .map_err(|m| PalwTirContainerError::Bytes(format!("`{}`: {m}", program.params[e.param as usize].name)))?;
        w.write_all(&bytes)?;
    }
    w.inner.flush()?;
    let mut out = [0u8; 64];
    out.copy_from_slice(w.state.finalize().as_bytes());
    Ok(out)
}

/// Every element of a param lies in its dtype by construction of the byte width, except nothing:
/// `i8`/`i16`/`i32`/`i64` and `idx` (unsigned 32) cover every bit pattern of their width. `i128` is
/// never a param (NF-7). Kept as a function so the rule has one place.
fn check_values(dtype: DType, bytes: &[u8]) -> std::result::Result<(), String> {
    if dtype == DType::I128 {
        return Err("an i128 param".into());
    }
    if !bytes.len().is_multiple_of(dtype.width()) {
        return Err(format!("{} bytes is not a whole number of {}", bytes.len(), dtype.name()));
    }
    Ok(())
}

struct DigestWriter<W: Write> {
    inner: W,
    state: blake2b_simd::State,
    written: u64,
}

impl<W: Write> Write for DigestWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.state.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Key of [`file_digest_v1`].
pub const PALW_TIR_FILE_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/tir/container-file/v1";

fn digest_state() -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_FILE_DIGEST_DOMAIN_V1).to_state()
}

/// **The file's digest** — BLAKE2b-512 keyed with [`PALW_TIR_FILE_DIGEST_DOMAIN_V1`] over every byte
/// of the file, streamed. What a `.palwmanifest` binds to the file; not an identity.
pub fn file_digest_v1(path: &Path) -> Result<[u8; 64]> {
    let mut f = std::fs::File::open(path)?;
    let mut st = digest_state();
    let mut buf = vec![0u8; 1 << 20];
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

/// An opened container: the checked header and program, and where each tensor is.
#[derive(Debug)]
pub struct PalwTirContainerV1 {
    pub header: PalwTirContainerHeaderV1,
    pub program: TirProgramV1,
    pub path: PathBuf,
    pub file_len: u64,
    index: BTreeMap<(u16, Option<u16>), (u64, u64)>,
}

impl PalwTirContainerV1 {
    /// Read and check the header (no tensor is read).
    pub fn open(path: &Path) -> Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let file_len = f.metadata()?.len();
        let mut prefix = [0u8; PREFIX as usize];
        f.read_exact(&mut prefix).map_err(|_| PalwTirContainerError::Magic)?;
        if &prefix[0..8] != PALW_TIR_CONTAINER_MAGIC_V1 || u16::from_le_bytes([prefix[8], prefix[9]]) != PALW_TIR_CONTAINER_VERSION_V1
        {
            return Err(PalwTirContainerError::Magic);
        }
        let hl = u32::from_le_bytes(prefix[10..14].try_into().expect("4 bytes"));
        if hl > PALW_TIR_MAX_HEADER_BYTES_V1 || PREFIX + hl as u64 > file_len {
            return Err(PalwTirContainerError::Header(format!("a header of {hl} bytes in a file of {file_len}")));
        }
        let mut hb = vec![0u8; hl as usize];
        f.read_exact(&mut hb)?;
        let header = PalwTirContainerHeaderV1::try_from_slice(&hb).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
        if header.version != PALW_TIR_CONTAINER_VERSION_V1 {
            return Err(PalwTirContainerError::Header(format!("header version {}", header.version)));
        }
        let program = TirProgramV1::decode_canonical(&header.program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
        misaka_palw_tir::validate::validate(&program).map_err(|e| PalwTirContainerError::Program(e.to_string()))?;
        // The table must be exactly the program's instances, in inventory order.
        let want: Vec<(u16, Option<u16>)> = param_instances_v1(&program)
            .into_iter()
            .enumerate()
            .flat_map(|(j, inst)| inst.into_iter().map(move |l| (j as u16, l)))
            .collect();
        let got: Vec<(u16, Option<u16>)> = header.tensors.iter().map(|e| (e.param, e.layer)).collect();
        if got != want {
            return Err(PalwTirContainerError::Table("the tensors are not the program's param instances in inventory order".into()));
        }
        let mut index = BTreeMap::new();
        let mut end = PREFIX + hl as u64;
        for e in &header.tensors {
            let name = &program.params[e.param as usize].name;
            let decl = tensor_bytes_v1(&program, e.param);
            if e.bytes != decl {
                return Err(PalwTirContainerError::Table(format!(
                    "`{name}` (layer {:?}): {} bytes, declared {decl}",
                    e.layer, e.bytes
                )));
            }
            if !e.offset.is_multiple_of(PALW_TIR_TENSOR_ALIGN_V1) || e.offset < end {
                return Err(PalwTirContainerError::Table(format!(
                    "`{name}` (layer {:?}): offset {} misaligned or overlapping",
                    e.layer, e.offset
                )));
            }
            end = e.offset.checked_add(e.bytes).filter(|x| *x <= file_len).ok_or_else(|| {
                PalwTirContainerError::Table(format!("`{name}` (layer {:?}) runs past the end of the file", e.layer))
            })?;
            index.insert((e.param, e.layer), (e.offset, e.bytes));
        }
        Ok(Self { header, program, path: path.to_path_buf(), file_len, index })
    }

    /// Where an instance's bytes are: `(offset, bytes)`.
    pub fn locate(&self, param: u16, layer: Option<u16>) -> Option<(u64, u64)> {
        self.index.get(&(param, layer)).copied()
    }

    /// One instance's bytes, read from the file.
    pub fn read_tensor_bytes(&self, param: u16, layer: Option<u16>) -> Result<Vec<u8>> {
        let (off, len) = self.locate(param, layer).ok_or(PalwTirContainerError::NoSuchTensor { param, layer })?;
        let mut f = std::fs::File::open(&self.path)?;
        f.seek(SeekFrom::Start(off))?;
        let mut v = vec![0u8; len as usize];
        f.read_exact(&mut v)?;
        Ok(v)
    }

    /// One instance as the evaluator's tensor.
    pub fn read_tensor(&self, param: u16, layer: Option<u16>) -> Result<Tensor> {
        let d = &self.program.params[param as usize];
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        let bytes = self.read_tensor_bytes(param, layer)?;
        Tensor::from_le_bytes(d.dtype, &shape, &bytes).map_err(|e| PalwTirContainerError::Bytes(e.to_string()))
    }
}

/// The container is a param source for the reference evaluator: each fetch reads the instance
/// from the file (the page cache makes repeated positions cheap).
impl misaka_palw_tir::ParamSource for PalwTirContainerV1 {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.read_tensor(index, layer).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Interpreter, MapParams, TensorType};

    fn program() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
        let table = pb.param("embed.table", DType::I8, &[16, 4], false);
        let w = pb.param("blk.w", DType::I8, &[4, 4], true);
        let m = pb.param("blk.m", DType::I64, &[4], true);
        let head = pb.param("head.w", DType::I16, &[16, 4], false);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let row = b.gather(table, Ref::Input(0), 0, 0);
            let row = b.cast(row, DType::I32);
            b.finish(&[row])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let acc = b.matmul(w, x, DType::I64);
            let acc = b.reshape_fixed(acc, &[4]);
            let y = b.mul(acc, m, DType::I128);
            let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
            let y = b.clamp(y, -30_000, 30_000, DType::I32);
            b.finish(&[y])
        };
        let post = {
            let mut b = pb.block("post", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.reshape_fixed(l, &[16]);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.commit(l);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        pb.finish(pre, vec![layer, layer], post, logits)
    }

    fn params(p: &TirProgramV1) -> MapParams {
        let mut out = MapParams::default();
        for (j, inst) in param_instances_v1(p).into_iter().enumerate() {
            let d = &p.params[j];
            for l in inst {
                let n: usize = d.shape.iter().map(|x| *x as usize).product();
                let data: Vec<i128> =
                    (0..n).map(|i| (((i * 37 + j * 11 + l.map_or(0, |l| l as usize) * 5) % 200) as i128) - 100).collect();
                let data: Vec<i128> = data.into_iter().map(|v| v.clamp(d.dtype.min_value(), d.dtype.max_value())).collect();
                out.tensors.insert(
                    (j as u16, l),
                    Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).expect("in range"),
                );
            }
        }
        out
    }

    #[test]
    fn a_container_round_trips_and_runs_like_its_params() {
        let p = program();
        let mp = params(&p);
        let path = std::env::temp_dir().join(format!("palwtir1-{}.palwtir", std::process::id()));
        let digest = write_container_v1(&path, &p, vec![1, 2, 3], [7u8; 64], "{\"x\":1}".into(), &mut |j, l| {
            Ok(mp.tensors[&(j, l)].to_le_bytes())
        })
        .expect("written");
        assert_eq!(file_digest_v1(&path).expect("digest"), digest);
        let c = PalwTirContainerV1::open(&path).expect("opens");
        assert_eq!(c.program, p);
        assert_eq!((c.header.layout.clone(), c.header.tokenizer_id, c.header.meta.as_str()), (vec![1, 2, 3], [7u8; 64], "{\"x\":1}"));
        for e in &c.header.tensors {
            assert_eq!(e.offset % PALW_TIR_TENSOR_ALIGN_V1, 0);
            assert_eq!(c.read_tensor(e.param, e.layer).expect("tensor"), mp.tensors[&(e.param, e.layer)]);
        }
        // The reference evaluator gives the same logits from the file as from memory.
        let i = Interpreter::new(&p).expect("valid");
        let a = i.run(&mp, &[3, 1, 4, 1, 5]).expect("runs");
        let b = i.run(&c, &[3, 1, 4, 1, 5]).expect("runs from the file");
        assert_eq!(a.iter().map(|s| &s.logits).collect::<Vec<_>>(), b.iter().map(|s| &s.logits).collect::<Vec<_>>());
        // A flipped header byte or a truncated file is refused.
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.truncate(bytes.len() - 1);
        std::fs::write(&path, &bytes).expect("write");
        assert!(PalwTirContainerV1::open(&path).is_err(), "truncated");
        bytes[0] = b'X';
        std::fs::write(&path, &bytes).expect("write");
        assert!(matches!(PalwTirContainerV1::open(&path), Err(PalwTirContainerError::Magic)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_tensor_of_the_wrong_length_is_not_written() {
        let p = program();
        let path = std::env::temp_dir().join(format!("palwtir1-bad-{}.palwtir", std::process::id()));
        let r = write_container_v1(&path, &p, vec![], [0u8; 64], String::new(), &mut |j, _| {
            Ok(vec![0u8; tensor_bytes_v1(&p, j) as usize + 1])
        });
        assert!(matches!(r, Err(PalwTirContainerError::Bytes(_))));
        let _ = std::fs::remove_file(&path);
    }
}
