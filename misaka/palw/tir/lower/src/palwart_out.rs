//! **A streaming conversion written straight into a `PALWART3` artifact** (genesis 8, 2026-10-15).
//!
//! The classic converter spools every tensor instance into a chunk store and then assembles a
//! `PALWTIR1` file in inventory order: peak disk about twice the artifact, on top of the checkpoint.
//! Here each instance goes, as it is produced, into the container's TIR section and from there into a
//! segment file, and every finished segment is handed to the segment hook (the genesis pipeline uploads
//! it, verifies it and deletes it). With the lazy shard source (`weights::lazy`) the input is streamed
//! too: peak disk is about one input shard plus one output segment.
//!
//! * **The same tensors.** The sink receives exactly the bytes the chunk sink would (the same
//!   `materialise_stream`), so the TIR section holds the instances a `PALWTIR1` conversion holds;
//!   `palwart extract-legacy` re-assembles that `PALWTIR1` file byte for byte (the provenance string the
//!   classic converter would write is kept as the container's `legacy_meta`).
//! * **The same inner root.** Each instance's leaves (32 KiB row pieces, the consensus leaf rule, the
//!   twin `misaka_palw_tir_artifact::palwart_pack::twin_leaf_v1`) are hashed as its bytes pass and kept
//!   (64 bytes a leaf); at the end they are folded in INVENTORY order, giving the consensus inventory
//!   root a `PALWTIR1` of the same tensors has.
//! * **The same root.** The entry table enters the container root in canonical (name, layer) order,
//!   so this artifact and one packed from the equivalent `PALWTIR1` have the same `.palwart` root.

use crate::error::{LowerError, Result};
use crate::lower::{StreamMaterialised, TensorSink};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::palwart_pack::{
    PalwArtPackInputsV3, PalwArtWeightsV3, TWIN_ROW_PIECE_BYTES_V1, TwinFrontierV1, manifest_bytes_v3, pack_common_v3,
    twin_graph_ir_root_v1, twin_leaf_v1, twin_row_bytes_v1,
};
use misaka_palw_tir_artifact::palwart_v3::{
    PalwArtProgramV3, PalwArtWriterV3, PalwArtWrittenV3, PalwArtifactFormatV3, canonical_json_v3, command_segment_hook_v3, dtype,
    encoding, inner_scheme, program_kind, section_kind,
};
use misaka_palw_tir_artifact::{param_instances_v1, tensor_bytes_v1};
use std::collections::BTreeMap;

/// Where a streamed conversion writes, and what it hands each segment to.
#[derive(Clone, Debug)]
pub struct PalwartOutV1 {
    /// The container's out dir, stem, segment size, manifest, family, extra license and calibration
    /// files, provenance, and the checkpoint directory its tokenizer/template/metadata are read from.
    pub inputs: PalwArtPackInputsV3,
    /// The segment program and its arguments (empty: segments stay on disk).
    pub on_segment: Vec<String>,
    /// Remove each segment once the program has succeeded.
    pub delete_after_hook: bool,
}

struct Cur {
    param: u16,
    layer: Option<u16>,
    name: String,
    row: u64,
    total: u64,
    pos: u64,
    piece: Vec<u8>,
    leaves: Vec<[u8; 64]>,
}

/// The [`TensorSink`] into a `PALWART3` TIR section (see the module docs).
pub struct PalwartSink<'a, 'h> {
    w: &'a mut PalwArtWriterV3<'h>,
    program: &'a TirProgramV1,
    cur: Option<Cur>,
    leaves: BTreeMap<(u16, Option<u16>), Vec<[u8; 64]>>,
    bytes: u64,
}

impl<'a, 'h> PalwartSink<'a, 'h> {
    pub fn new(w: &'a mut PalwArtWriterV3<'h>, program: &'a TirProgramV1) -> Self {
        Self { w, program, cur: None, leaves: BTreeMap::new(), bytes: 0 }
    }

    /// Fold the instances' leaves in inventory order: `(root, leaf_count, bytes)`. Every instance of
    /// the program must have been produced exactly once.
    pub fn finish(self) -> Result<([u8; 64], u64, u64)> {
        if self.cur.is_some() {
            return Err(LowerError::eval("internal: the conversion ended inside an instance"));
        }
        let mut leaves = self.leaves;
        let mut frontier = TwinFrontierV1::default();
        for (j, inst) in param_instances_v1(self.program).into_iter().enumerate() {
            for l in inst {
                let v = leaves.remove(&(j as u16, l)).ok_or_else(|| {
                    LowerError::eval(format!("internal: `{}` layer {l:?} was never produced", self.program.params[j].name))
                })?;
                for leaf in v {
                    frontier.push(leaf);
                }
            }
        }
        if let Some(((j, l), _)) = leaves.iter().next() {
            return Err(LowerError::eval(format!("internal: an instance outside the program: param {j} layer {l:?}")));
        }
        let n = frontier.leaf_count();
        if n > u32::MAX as u64 {
            return Err(LowerError::bad(format!("{n} leaves exceed the inventory's u32 count")));
        }
        let root = frontier.root().ok_or_else(|| LowerError::bad("the program declares no param leaf"))?;
        Ok((root, n, self.bytes))
    }
}

fn w_err(e: impl std::fmt::Display) -> LowerError {
    LowerError::Io(e.to_string())
}

impl TensorSink for PalwartSink<'_, '_> {
    fn begin(&mut self, param: u16, layer: Option<u16>, bytes: u64) -> Result<()> {
        if self.cur.is_some() {
            return Err(LowerError::eval("internal: an instance began inside another"));
        }
        let d = self.program.params.get(param as usize).ok_or_else(|| LowerError::eval(format!("internal: param {param}")))?;
        let want = tensor_bytes_v1(self.program, param);
        if bytes != want {
            return Err(LowerError::eval(format!("internal: `{}`: {bytes} bytes, declared {want}", d.name)));
        }
        if bytes > u32::MAX as u64 {
            return Err(LowerError::bad(format!(
                "`{}`: an instance of {bytes} bytes (the inventory addresses at most 4 GiB)",
                d.name
            )));
        }
        let code =
            dtype::of_tir(d.dtype).ok_or_else(|| LowerError::eval(format!("`{}`: dtype {:?} is never a param", d.name, d.dtype)))?;
        let shape: Vec<u64> = d.shape.iter().map(|x| *x as u64).collect();
        self.w.begin_entry(&d.name, layer.map(u32::from), code, shape, encoding::TIR_INSTANCE_LE, bytes).map_err(w_err)?;
        self.cur = Some(Cur {
            param,
            layer,
            name: d.name.clone(),
            row: twin_row_bytes_v1(self.program, param),
            total: bytes,
            pos: 0,
            piece: Vec::with_capacity(TWIN_ROW_PIECE_BYTES_V1 as usize),
            leaves: Vec::new(),
        });
        Ok(())
    }

    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        self.w.write(bytes).map_err(w_err)?;
        let c = self.cur.as_mut().ok_or_else(|| LowerError::eval("internal: a push outside an instance"))?;
        if c.pos + bytes.len() as u64 > c.total {
            return Err(LowerError::eval(format!("internal: `{}`: more bytes than declared", c.name)));
        }
        if c.row == 0 {
            c.pos += bytes.len() as u64;
            return Ok(());
        }
        let mut rest = bytes;
        while !rest.is_empty() {
            let piece_start = c.pos - c.piece.len() as u64;
            let target = TWIN_ROW_PIECE_BYTES_V1.min(c.row - piece_start % c.row);
            let n = ((target - c.piece.len() as u64) as usize).min(rest.len());
            c.piece.extend_from_slice(&rest[..n]);
            c.pos += n as u64;
            rest = &rest[n..];
            if c.piece.len() as u64 == target {
                c.leaves.push(twin_leaf_v1(&c.name, c.layer, piece_start as u32, &c.piece));
                c.piece.clear();
            }
        }
        Ok(())
    }

    fn end(&mut self) -> Result<()> {
        let c = self.cur.take().ok_or_else(|| LowerError::eval("internal: an end outside an instance"))?;
        if c.pos != c.total || !c.piece.is_empty() {
            return Err(LowerError::eval(format!("internal: `{}`: {} of {} bytes", c.name, c.pos, c.total)));
        }
        self.w.end_entry().map_err(w_err)?;
        self.bytes += c.total;
        if self.leaves.insert((c.param, c.layer), c.leaves).is_some() {
            return Err(LowerError::eval(format!("internal: `{}` layer {:?} produced twice", c.name, c.layer)));
        }
        Ok(())
    }
}

/// What [`write_palwart_v1`] wrote.
#[derive(Clone, Debug)]
pub struct PalwartReportV1 {
    pub written: PalwArtWrittenV3,
    pub inner_root: [u8; 64],
    pub leaves: u64,
    pub tensor_bytes: u64,
    pub resid_scale: f64,
    pub logits_scale: f64,
    pub quant_inexact: usize,
    pub stats: crate::lower::StreamStats,
}

/// **Materialise `prep` into a `PALWART3` artifact**: the TIR section streamed through
/// [`PalwartSink`], then the common sections, then the manifest and the program record. `meta` is the
/// provenance string the classic converter writes into a `PALWTIR1` (kept as `legacy_meta`).
#[allow(clippy::too_many_arguments)]
pub fn write_palwart_v1(
    prep: &crate::fidelity::Prepared,
    loader: &dyn crate::float_ref::stream::OccParams,
    calib: &BTreeMap<String, crate::float_ref::SiteStat>,
    policy: &crate::quant::QuantPolicy,
    stream: &crate::lower::StreamOpts,
    out: &PalwartOutV1,
    tokenizer_id: [u8; 64],
    meta: &dyn Fn(&StreamMaterialised) -> serde_json::Value,
    progress: &dyn Fn(usize, usize),
    validate_source: &dyn Fn() -> Result<()>,
) -> Result<PalwartReportV1> {
    let program = &prep.lowered.program;
    let hook = command_segment_hook_v3(out.on_segment.clone(), out.delete_after_hook);
    let mut w = PalwArtWriterV3::create(&out.inputs.out_dir, &out.inputs.stem, out.inputs.segment_bytes, hook).map_err(w_err)?;
    w.begin_section(section_kind::TIR_INVENTORY, out.inputs.family, "tir-inventory").map_err(w_err)?;
    let mut sink = PalwartSink::new(&mut w, program);
    let m = crate::lower::materialise_stream(&prep.lowered, &prep.hl, loader, calib, policy, stream, &mut sink, progress)?;
    let (root, leaves, tensor_bytes) = sink.finish()?;
    // The source vouches for every byte it served (a streamed remote file: its whole-file SHA-256 against the pin) BEFORE the
    // index — the artifact's identity — is written. Segments already handed off carry no identity without it.
    validate_source()?;
    w.end_section(inner_scheme::TIR_INVENTORY_ROOT_V1, root, leaves).map_err(w_err)?;
    pack_common_v3(&mut w, &out.inputs).map_err(LowerError::Io)?;
    let weights = PalwArtWeightsV3 {
        legacy_format: PalwArtifactFormatV3::TirV1,
        section_kind: section_kind::TIR_INVENTORY,
        inner_root_scheme: inner_scheme::TIR_INVENTORY_ROOT_V1,
        inner_root: root,
        inner_leaf_count: leaves,
        bytes: tensor_bytes,
    };
    let manifest = manifest_bytes_v3(&out.inputs.manifest, out.inputs.family, &weights);
    let program_bytes = program.encode();
    let record = PalwArtProgramV3 {
        kind: program_kind::TIR_V1,
        kernel_id: twin_graph_ir_root_v1(&program_bytes),
        bytes: program_bytes,
        kernel_version: 0,
        tokenizer_id,
    };
    // The genesis scope (text generation only): a program with a job input (`input.image_rows`, …) or a vision / audio / MTP
    // component is refused by name before the index — the artifact's identity — exists.
    if misaka_palw_tir_artifact::palwart_v3::manifest_declares_text_scope_v3(&manifest) {
        misaka_palw_tir_artifact::palwart_v3::check_text_scope_v3(&record, w.sections()).map_err(LowerError::not_lowerable)?;
    }
    let legacy_meta = meta(&m).to_string();
    let mut prov = out.inputs.provenance.clone();
    if !prov.is_object() {
        prov = serde_json::json!({});
    }
    prov["converter"] = serde_json::json!(format!("palw-tir-convert {} (streamed into PALWART3)", env!("CARGO_PKG_VERSION")));
    let written = w.finish(manifest, record, Vec::new(), legacy_meta, canonical_json_v3(&prov).into_bytes()).map_err(w_err)?;
    Ok(PalwartReportV1 {
        written,
        inner_root: root,
        leaves,
        tensor_bytes,
        resid_scale: m.resid_scale,
        logits_scale: m.logits_scale,
        quant_inexact: m.quant_inexact,
        stats: m.stats,
    })
}
