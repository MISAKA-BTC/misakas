//! # `PALWTIR2` — a pipeline class's container (RFC-0003: the pipeline inventory)
//!
//! One file per generative class: the pipeline's canonical bytes, every program's (version 2), the
//! class as the registrant declares it, its tokenizer id, and every param tensor of every program.
//!
//! ```text
//! file   := "PALWTIR2" · version u16 LE · header_len u32 LE · header (borsh) · zeros to 64 · tensors
//! header := PalwTirContainerHeaderV2 { version, pipeline, programs, class, tokenizer_id, tensors, meta }
//! tensor := the instance's bytes, little-endian two's complement in the declared dtype (idx:
//!           unsigned), at a 64-byte-aligned absolute file offset — PALWTIR1's rule
//! ```
//!
//! * **Order.** Program `0`'s instances, then program `1`'s, …; within a program, PALWTIR1's
//!   inventory order over the program's DECLARED params (never an input: a version-2 program's
//!   inputs are the job's) — params in declaration order, each param's instances ascending. That is
//!   the order the consensus pipeline inventory (`kaspa_consensus_core::palw_gen_artifact_v1`) hashes
//!   them in, so a class's `artifact_root` streams from the file front to back.
//! * **What is identity and what is not.** As PALWTIR1: the container is not a consensus object. The
//!   class (its pipeline, programs, layouts, output, offers, tokenizer) is what a registration
//!   carries, and its weights are bound by the inventory root; `class` is the class's borsh as the
//!   registrant declares it (opaque here — this crate does not depend on the consensus crate, and
//!   the node compares it with the registered row), or empty; `meta` is provenance and enters
//!   nothing. [`super::file_digest_v1`] is the file's digest, as for PALWTIR1.
//! * **Two formats, two magics.** A PALWTIR1 reader ([`super::PalwTirContainerV1::open`], unchanged)
//!   refuses this file by its magic, and [`PalwTirContainerV2::open`] refuses a PALWTIR1 file by its.
//!
//! [`PalwTirContainerV2::open`] checks everything the header claims before a single tensor is
//! believed: every program decodes canonically (and so validates), the pipeline decodes over them,
//! the tensor table is exactly their instances in order with the declared byte lengths, and the
//! offsets are aligned, in order, non-overlapping and inside the file. The opened container is a
//! [`misaka_palw_tir::pipeline::PipelineParams`]: the node holds a class from it.

use super::{
    DigestWriter, PREFIX, PalwTirContainerError, Result, align, check_values, digest_state, param_instances_v1, tensor_bytes_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::pipeline::{PipelineParams, TirPipelineV1};
use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::{DType, ParamSource, Tensor, TirProgramV1};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const PALW_TIR_CONTAINER_MAGIC_V2: &[u8; 8] = b"PALWTIR2";
pub const PALW_TIR_CONTAINER_VERSION_V2: u16 = 2;
/// The most programs a pipeline container holds (a pipeline's stages name at most this many).
pub const PALW_TIR_CONTAINER_MAX_PROGRAMS_V2: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirTensorEntryV2 {
    /// The program's index in the pipeline's program list.
    pub program: u16,
    pub param: u16,
    pub layer: Option<u16>,
    /// Absolute file offset, a multiple of [`super::PALW_TIR_TENSOR_ALIGN_V1`].
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirContainerHeaderV2 {
    pub version: u16,
    /// The pipeline's canonical bytes (spec 04b §15.6).
    pub pipeline: Vec<u8>,
    /// Every program's canonical version-2 bytes, in the pipeline's program order.
    pub programs: Vec<Vec<u8>>,
    /// `borsh(PalwGenClassV1)` as the registrant declares it; empty when none is declared yet.
    pub class: Vec<u8>,
    pub tokenizer_id: [u8; 64],
    pub tensors: Vec<PalwTirTensorEntryV2>,
    /// Provenance (JSON by convention); enters no identity.
    pub meta: String,
}

/// **Program `k`'s param view**: its version-1 view with the declared params only (the appended
/// inputs cut) — the view whose instances the pipeline inventory lays out.
pub fn param_view_v2(program: &TirProgramV2) -> TirProgramV1 {
    let mut view = program.v1_view();
    view.params.truncate(program.params.len());
    view
}

/// **Every instance of a pipeline's params, in container (and inventory) order**:
/// `(program, param, layer)`.
pub fn pipeline_instances_v2(programs: &[TirProgramV2]) -> Vec<(u16, u16, Option<u16>)> {
    let mut out = Vec::new();
    for (k, p) in programs.iter().enumerate() {
        for (j, inst) in param_instances_v1(&param_view_v2(p)).into_iter().enumerate() {
            out.extend(inst.into_iter().map(|l| (k as u16, j as u16, l)));
        }
    }
    out
}

/// **Write a pipeline container**, one tensor at a time: `tensor(program, param, layer)` is asked
/// for each instance in container order and must return exactly its declared bytes. Returns the file
/// digest ([`super::file_digest_v1`]) of what was written.
pub fn write_container_v2(
    path: &Path,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    class: Vec<u8>,
    tokenizer_id: [u8; 64],
    meta: String,
    tensor: &mut dyn FnMut(u16, u16, Option<u16>) -> std::result::Result<Vec<u8>, String>,
) -> Result<[u8; 64]> {
    if programs.len() > PALW_TIR_CONTAINER_MAX_PROGRAMS_V2 {
        return Err(PalwTirContainerError::Program(format!(
            "{} programs exceed {PALW_TIR_CONTAINER_MAX_PROGRAMS_V2}",
            programs.len()
        )));
    }
    let program_bytes: Vec<Vec<u8>> = programs.iter().map(|p| p.encode()).collect();
    for (k, bytes) in program_bytes.iter().enumerate() {
        TirProgramV2::decode_canonical(bytes).map_err(|e| PalwTirContainerError::Program(format!("program {k}: {e}")))?;
    }
    let pipeline_bytes = pipeline.encode();
    TirPipelineV1::decode_canonical(&pipeline_bytes, programs)
        .map_err(|e| PalwTirContainerError::Program(format!("the pipeline: {e}")))?;
    let views: Vec<TirProgramV1> = programs.iter().map(param_view_v2).collect();
    let entries: Vec<PalwTirTensorEntryV2> = pipeline_instances_v2(programs)
        .into_iter()
        .map(|(k, j, l)| PalwTirTensorEntryV2 {
            program: k,
            param: j,
            layer: l,
            offset: 0,
            bytes: tensor_bytes_v1(&views[k as usize], j),
        })
        .collect();
    let mut header = PalwTirContainerHeaderV2 {
        version: PALW_TIR_CONTAINER_VERSION_V2,
        pipeline: pipeline_bytes,
        programs: program_bytes,
        class,
        tokenizer_id,
        tensors: entries,
        meta,
    };
    // Fixed-width table fields: the header's length does not depend on the offsets.
    let header_len = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?.len() as u64;
    if header_len > super::PALW_TIR_MAX_HEADER_BYTES_V1 as u64 {
        return Err(PalwTirContainerError::Header(format!("a header of {header_len} bytes")));
    }
    let mut at = align(PREFIX + header_len);
    for e in header.tensors.iter_mut() {
        e.offset = at;
        at = align(at + e.bytes);
    }
    let hb = borsh::to_vec(&header).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
    debug_assert_eq!(hb.len() as u64, header_len);
    let f = std::fs::File::create(path)?;
    let mut w = DigestWriter { inner: std::io::BufWriter::with_capacity(1 << 20, f), state: digest_state(), written: 0 };
    w.write_all(PALW_TIR_CONTAINER_MAGIC_V2)?;
    w.write_all(&PALW_TIR_CONTAINER_VERSION_V2.to_le_bytes())?;
    w.write_all(&(hb.len() as u32).to_le_bytes())?;
    w.write_all(&hb)?;
    for e in &header.tensors {
        let pad = e.offset - w.written;
        w.write_all(&vec![0u8; pad as usize])?;
        let d = &views[e.program as usize].params[e.param as usize];
        let bytes = tensor(e.program, e.param, e.layer).map_err(PalwTirContainerError::Bytes)?;
        if bytes.len() as u64 != e.bytes {
            return Err(PalwTirContainerError::Bytes(format!(
                "program {} `{}` (layer {:?}): {} bytes, declared {}",
                e.program,
                d.name,
                e.layer,
                bytes.len(),
                e.bytes
            )));
        }
        check_values(d.dtype, &bytes).map_err(|m| PalwTirContainerError::Bytes(format!("program {} `{}`: {m}", e.program, d.name)))?;
        w.write_all(&bytes)?;
    }
    w.inner.flush()?;
    let mut out = [0u8; 64];
    out.copy_from_slice(w.state.finalize().as_bytes());
    Ok(out)
}

/// **One program's params in an opened pipeline container** — a param source for the evaluator:
/// each fetch reads the instance from the file (the page cache makes repeated positions cheap).
#[derive(Debug)]
pub struct PalwTirContainerProgramV2 {
    path: PathBuf,
    decls: Vec<(DType, Vec<usize>)>,
    index: BTreeMap<(u16, Option<u16>), (u64, u64)>,
}

impl PalwTirContainerProgramV2 {
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
        let (dtype, shape) = self.decls.get(param as usize).ok_or(PalwTirContainerError::NoSuchTensor { param, layer })?;
        let bytes = self.read_tensor_bytes(param, layer)?;
        Tensor::from_le_bytes(*dtype, shape, &bytes).map_err(|e| PalwTirContainerError::Bytes(e.to_string()))
    }
}

impl ParamSource for PalwTirContainerProgramV2 {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.read_tensor(index, layer).ok()
    }
}

/// An opened pipeline container: the checked header, pipeline and programs, and where each tensor is.
#[derive(Debug)]
pub struct PalwTirContainerV2 {
    pub header: PalwTirContainerHeaderV2,
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub path: PathBuf,
    pub file_len: u64,
    sources: Vec<PalwTirContainerProgramV2>,
}

impl PalwTirContainerV2 {
    /// Read and check the header (no tensor is read).
    pub fn open(path: &Path) -> Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let file_len = f.metadata()?.len();
        let mut prefix = [0u8; PREFIX as usize];
        f.read_exact(&mut prefix).map_err(|_| PalwTirContainerError::Magic)?;
        if &prefix[0..8] != PALW_TIR_CONTAINER_MAGIC_V2 || u16::from_le_bytes([prefix[8], prefix[9]]) != PALW_TIR_CONTAINER_VERSION_V2
        {
            return Err(PalwTirContainerError::Magic);
        }
        let hl = u32::from_le_bytes(prefix[10..14].try_into().expect("4 bytes"));
        if hl > super::PALW_TIR_MAX_HEADER_BYTES_V1 || PREFIX + hl as u64 > file_len {
            return Err(PalwTirContainerError::Header(format!("a header of {hl} bytes in a file of {file_len}")));
        }
        let mut hb = vec![0u8; hl as usize];
        f.read_exact(&mut hb)?;
        let header = PalwTirContainerHeaderV2::try_from_slice(&hb).map_err(|e| PalwTirContainerError::Header(e.to_string()))?;
        if header.version != PALW_TIR_CONTAINER_VERSION_V2 {
            return Err(PalwTirContainerError::Header(format!("header version {}", header.version)));
        }
        if header.programs.is_empty() || header.programs.len() > PALW_TIR_CONTAINER_MAX_PROGRAMS_V2 {
            return Err(PalwTirContainerError::Header(format!("{} programs", header.programs.len())));
        }
        let programs = header
            .programs
            .iter()
            .enumerate()
            .map(|(k, b)| TirProgramV2::decode_canonical(b).map_err(|e| PalwTirContainerError::Program(format!("program {k}: {e}"))))
            .collect::<Result<Vec<_>>>()?;
        let pipeline = TirPipelineV1::decode_canonical(&header.pipeline, &programs)
            .map_err(|e| PalwTirContainerError::Program(format!("the pipeline: {e}")))?;
        // The table must be exactly the programs' instances, in container order.
        let want = pipeline_instances_v2(&programs);
        let got: Vec<(u16, u16, Option<u16>)> = header.tensors.iter().map(|e| (e.program, e.param, e.layer)).collect();
        if got != want {
            return Err(PalwTirContainerError::Table("the tensors are not the programs' param instances in inventory order".into()));
        }
        let views: Vec<TirProgramV1> = programs.iter().map(param_view_v2).collect();
        let mut sources: Vec<PalwTirContainerProgramV2> = views
            .iter()
            .map(|v| PalwTirContainerProgramV2 {
                path: path.to_path_buf(),
                decls: v.params.iter().map(|d| (d.dtype, d.shape.iter().map(|x| *x as usize).collect())).collect(),
                index: BTreeMap::new(),
            })
            .collect();
        let mut end = PREFIX + hl as u64;
        for e in &header.tensors {
            let view = &views[e.program as usize];
            let name = &view.params[e.param as usize].name;
            let decl = tensor_bytes_v1(view, e.param);
            if e.bytes != decl {
                return Err(PalwTirContainerError::Table(format!(
                    "program {} `{name}` (layer {:?}): {} bytes, declared {decl}",
                    e.program, e.layer, e.bytes
                )));
            }
            if !e.offset.is_multiple_of(super::PALW_TIR_TENSOR_ALIGN_V1) || e.offset < end {
                return Err(PalwTirContainerError::Table(format!(
                    "program {} `{name}` (layer {:?}): offset {} misaligned or overlapping",
                    e.program, e.layer, e.offset
                )));
            }
            end = e.offset.checked_add(e.bytes).filter(|x| *x <= file_len).ok_or_else(|| {
                PalwTirContainerError::Table(format!(
                    "program {} `{name}` (layer {:?}) runs past the end of the file",
                    e.program, e.layer
                ))
            })?;
            sources[e.program as usize].index.insert((e.param, e.layer), (e.offset, e.bytes));
        }
        Ok(Self { header, pipeline, programs, path: path.to_path_buf(), file_len, sources })
    }

    /// Program `k`'s params.
    pub fn program(&self, k: u16) -> Option<&PalwTirContainerProgramV2> {
        self.sources.get(k as usize)
    }

    /// One instance of program `k`'s param, as the evaluator's tensor.
    pub fn read_tensor(&self, k: u16, param: u16, layer: Option<u16>) -> Result<Tensor> {
        self.program(k).ok_or(PalwTirContainerError::NoSuchTensor { param, layer })?.read_tensor(param, layer)
    }
}

/// The container is the pipeline's params: program `k`'s from its slice of the file. A program index
/// past the pipeline's reads as a source with no tensors.
impl PipelineParams for PalwTirContainerV2 {
    fn params(&self, program: u16) -> &dyn ParamSource {
        static NONE: EmptySource = EmptySource;
        self.sources.get(program as usize).map_or(&NONE as &dyn ParamSource, |s| s as &dyn ParamSource)
    }
}

struct EmptySource;
impl ParamSource for EmptySource {
    fn param(&self, _: u16, _: Option<u16>) -> Option<Tensor> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::{PalwTirContainerV1, file_digest_v1};
    use super::*;
    use misaka_palw_tir::MapParams;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
    }

    struct Params(Vec<MapParams>);
    impl PipelineParams for Params {
        fn params(&self, program: u16) -> &dyn ParamSource {
            &self.0[program as usize]
        }
    }

    /// The golden toy VLM (two programs, `consensus-vectors/tir-v2/pipelines/toy-vlm.json`).
    fn toy_vlm() -> (TirPipelineV1, Vec<TirProgramV2>, Params) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v2/pipelines/toy-vlm.json");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the vector")).expect("json");
        let programs: Vec<TirProgramV2> = v["programs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| TirProgramV2::decode_canonical(&unhex(p["program_borsh_hex"].as_str().unwrap())).unwrap())
            .collect();
        let pipeline = TirPipelineV1::decode_canonical(&unhex(v["pipeline_borsh_hex"].as_str().unwrap()), &programs).unwrap();
        let params = Params(
            v["programs"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&programs)
                .map(|(pj, prog)| {
                    let mut m = MapParams::default();
                    for e in pj["params"].as_array().unwrap() {
                        let j = e["param"].as_u64().unwrap() as u16;
                        let d = &prog.params[j as usize];
                        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                        let layer = e["layer"].as_u64().map(|l| l as u16);
                        m.tensors.insert(
                            (j, layer),
                            Tensor::from_le_bytes(d.dtype, &shape, &unhex(e["le_hex"].as_str().unwrap())).unwrap(),
                        );
                    }
                    m
                })
                .collect(),
        );
        (pipeline, programs, params)
    }

    fn write(path: &Path, pipeline: &TirPipelineV1, programs: &[TirProgramV2], params: &Params) -> Result<[u8; 64]> {
        write_container_v2(path, pipeline, programs, vec![0xC1, 0xA5], [7u8; 64], "{\"x\":2}".into(), &mut |k, j, l| {
            params.params(k).param(j, l).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no ({k}, {j}, {l:?})"))
        })
    }

    #[test]
    fn a_pipeline_container_round_trips_program_major_and_serves_every_program() {
        let (pipeline, programs, params) = toy_vlm();
        let path = std::env::temp_dir().join(format!("palwtir2-{}.palwtir", std::process::id()));
        let digest = write(&path, &pipeline, &programs, &params).expect("written");
        assert_eq!(file_digest_v1(&path).expect("digest"), digest);
        let c = PalwTirContainerV2::open(&path).expect("opens");
        assert_eq!((c.pipeline.clone(), c.programs.clone()), (pipeline.clone(), programs.clone()));
        assert_eq!(
            (c.header.class.clone(), c.header.tokenizer_id, c.header.meta.as_str()),
            (vec![0xC1, 0xA5], [7u8; 64], "{\"x\":2}")
        );
        // Program-major, each program in its inventory order, every tensor aligned.
        let order: Vec<(u16, u16, Option<u16>)> = c.header.tensors.iter().map(|e| (e.program, e.param, e.layer)).collect();
        assert_eq!(order, pipeline_instances_v2(&programs));
        assert!(order.windows(2).all(|w| w[0].0 <= w[1].0), "program-major");
        assert_eq!(order.iter().map(|o| o.0).collect::<std::collections::BTreeSet<_>>().len(), programs.len(), "every program");
        for e in &c.header.tensors {
            assert_eq!(e.offset % super::super::PALW_TIR_TENSOR_ALIGN_V1, 0);
            let want = params.params(e.program).param(e.param, e.layer).unwrap();
            assert_eq!(c.read_tensor(e.program, e.param, e.layer).expect("tensor"), want);
            assert_eq!(c.params(e.program).param(e.param, e.layer), Some(want), "the PipelineParams view");
        }
        assert_eq!(c.params(99).param(0, None), None, "no such program");
        // A PALWTIR1 reader refuses it by its magic; this reader refuses a truncated or relabelled file.
        assert!(matches!(PalwTirContainerV1::open(&path), Err(PalwTirContainerError::Magic)));
        let mut bytes = std::fs::read(&path).expect("read");
        bytes.truncate(bytes.len() - 1);
        std::fs::write(&path, &bytes).expect("write");
        assert!(PalwTirContainerV2::open(&path).is_err(), "truncated");
        bytes[7] = b'1';
        std::fs::write(&path, &bytes).expect("write");
        assert!(matches!(PalwTirContainerV2::open(&path), Err(PalwTirContainerError::Magic)), "PALWTIR1's magic");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_table_that_is_not_the_programs_instances_is_refused() {
        let (pipeline, programs, params) = toy_vlm();
        let path = std::env::temp_dir().join(format!("palwtir2-table-{}.palwtir", std::process::id()));
        write(&path, &pipeline, &programs, &params).expect("written");
        let c = PalwTirContainerV2::open(&path).expect("opens");
        // The same bytes under a reordered table: refused before a tensor is read.
        let mut header = c.header.clone();
        header.tensors.swap(0, 1);
        let hb = borsh::to_vec(&header).unwrap();
        let mut bytes = std::fs::read(&path).expect("read");
        bytes[PREFIX as usize..PREFIX as usize + hb.len()].copy_from_slice(&hb);
        std::fs::write(&path, &bytes).expect("write");
        assert!(matches!(PalwTirContainerV2::open(&path), Err(PalwTirContainerError::Table(_))));
        // A tensor of the wrong length is not written.
        let r = write_container_v2(&path, &pipeline, &programs, vec![], [0u8; 64], String::new(), &mut |k, j, l| {
            let mut b = params.params(k).param(j, l).unwrap().to_le_bytes();
            b.push(0);
            Ok(b)
        });
        assert!(matches!(r, Err(PalwTirContainerError::Bytes(_))));
        let _ = std::fs::remove_file(&path);
    }
}
