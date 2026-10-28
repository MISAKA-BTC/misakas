//! PALWQ361 runtime ↔ Source39 mirror differential driver (RFC-0002 §II.14,
//! "frontend and runtime connection", Qwen3.6-35B-A3B).
//!
//! The ORIGINAL runtime is base0's integer `Qwen36Engine` over the pinned
//! `.palwq36` artifact (integer A16 matmuls, expert routing, recurrent GDN and
//! convolution state). The mirror is the canonical Program39 that
//! `compile_runtime_q36_v40` derived from the same file. This driver feeds the
//! same token sequence position by position to both and compares every logit
//! lane exactly (i16 codes in i32 lanes vs the interpreter's i128 elements),
//! so integer arithmetic, routing and recurrent-state continuity are all
//! observed through the complete output row. It also records the original
//! runtime's per-site probe digest so a later site-level mapping can be added
//! without changing the measured transcript.
//!
//! The result is a measurement record with `status` `EQUAL_OVER_MEASURED` /
//! `MISMATCH` / `NOT_RUN`; it never says PASS, is not a cryptographic proof
//! (PRINCIPLES §8), and grants no class, Final or launch right.
use misaka_palw_kernel::{
    flat_job39::{CheckedJobMaximumV39, GenerationInputProviderV39, JobMaximumV39, MaterialManifestV39, RequestStopV39},
    flat_material39::{MaterialPageReaderV39, MaterialPageV39},
    flat_source39::{CheckedSourceV39, FlatSourceWireV39},
    hash::{Digest, object_id},
};
use misaka_palw_tir::{interp::RunState, interp_v39::InterpreterV39};
use std::path::Path;

struct NoMaterial;
impl MaterialPageReaderV39 for NoMaterial {
    fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
        Err("q36 differential: the decoder-only runtime mirror reads no material".into())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Q36DifferentialReportV40 {
    pub schema: &'static str,
    pub status: &'static str,
    pub artifact_root: String,
    pub source: String,
    pub program: String,
    pub real_parameter: String,
    pub tokens: usize,
    pub equal_positions: usize,
    pub first_mismatch: Option<(usize, usize, i128, i128)>,
    pub probe_transcript: String,
    pub logits_transcript: String,
    pub authority: &'static str,
}

/// `compiled` is a `compile_runtime_q36_v40` output directory (`source.borsh`
/// and `parameters/`). `tokens` is the exact original prompt (tokenizer-bound
/// elsewhere). Stops at the first differing lane; never skips a position.
pub fn run_q36_differential_v40(
    artifact: &Path,
    compiled: &Path,
    tokens: &[u32],
    workspace: u64,
    max_read_elements: usize,
) -> Result<Q36DifferentialReportV40, String> {
    if tokens.is_empty() || tokens.len() > 1 << 20 {
        return Err("q36 differential: a bounded non-empty original token sequence is required".into());
    }
    let wire = FlatSourceWireV39::from_bytes(&crate::native_audit_io_v32::read_bounded_file_v32(
        &compiled.join("source.borsh"),
        misaka_palw_kernel::flat_source39::SOURCE39_MAX_BYTES,
    )?)?;
    let source: CheckedSourceV39 = wire.check()?;
    if tokens.len() as u64 > source.original_context_max() as u64 || tokens.iter().any(|t| *t >= source.program().token_bound) {
        return Err("q36 differential: tokens exceed the source's original context/vocabulary".into());
    }
    let parameters = crate::full_component_v28::q36mirror40::SourceDirectoryV39::open(
        &source,
        &compiled.join("parameters"),
        workspace,
        max_read_elements,
    )?;
    let original = misaka_palw_base0::qwen36::open_artifact(artifact).map_err(|e| format!("{e:?}"))?;
    let engine = misaka_palw_base0::qwen36::Qwen36Engine::new(&original);
    let mut cache = misaka_palw_base0::qwen36::Qwen36Cache::new(&original.shape);
    // A decoder-only maximum over exactly these tokens; no material inputs.
    let manifest = MaterialManifestV39 { version: 39, source: source.root(), master: [0; 64], bindings: vec![] };
    let mut nonce = [0u8; 64];
    nonce[0] = 1;
    let request = JobMaximumV39 {
        version: 39,
        source: source.root(),
        requester: [1; 64],
        producer: [2; 64],
        nonce,
        stop: RequestStopV39::Fixed { positions: tokens.len() as u32, tokens: tokens.to_vec() },
        input_manifest: manifest.root(),
    };
    let maximum = CheckedJobMaximumV39::check(&source, &request, &manifest)?;
    let provider = GenerationInputProviderV39::derive(&source, &maximum, &NoMaterial)?;
    let interpreter = InterpreterV39::new(source.program()).map_err(|e| e.to_string())?;
    let mut state = RunState::default();
    let mut probes = Vec::with_capacity(tokens.len());
    let mut rows = Vec::with_capacity(tokens.len());
    let mut equal = 0usize;
    let mut first_mismatch = None;
    for (position, token) in tokens.iter().enumerate() {
        let (expected, probe) =
            engine.forward_token_probed(&mut cache, *token as usize, position).map_err(|e| format!("{e:?}"))?;
        let actual = interpreter
            .step_with_observer(&parameters, &provider, &mut state, *token, &mut |_| Ok(()))
            .map_err(|e| e.to_string())?;
        probes.push(object_id(b"misaka-palw/sdk/q36-runtime-probe-row/v40", &probe));
        rows.push(object_id(b"misaka-palw/sdk/q36-runtime-logit-row/v40", &expected));
        let mismatch = if actual.data.len() != expected.len() {
            Some((position, expected.len().min(actual.data.len()), expected.len() as i128, actual.data.len() as i128))
        } else {
            expected.iter().zip(&actual.data).position(|(e, a)| *e as i128 != *a).map(|lane| (position, lane, expected[lane] as i128, actual.data[lane]))
        };
        if let Some(m) = mismatch {
            first_mismatch = Some(m);
            break;
        }
        equal += 1;
    }
    let hex = crate::runtime_pack::commit::hex;
    Ok(Q36DifferentialReportV40 {
        schema: "misaka.palw.q36-runtime-mirror-differential.v40",
        status: if first_mismatch.is_some() { "MISMATCH" } else { "EQUAL_OVER_MEASURED" },
        artifact_root: original.artifact_root().to_string(),
        source: hex(&source.root()),
        program: hex(&source.program_root()),
        real_parameter: hex(&source.parameters().root()),
        tokens: tokens.len(),
        equal_positions: equal,
        first_mismatch,
        probe_transcript: hex(&object_id(b"misaka-palw/sdk/q36-runtime-probe-transcript/v40", &probes)),
        logits_transcript: hex(&object_id(b"misaka-palw/sdk/q36-runtime-logit-transcript/v40", &rows)),
        authority: "measurement-record-only; not PASS; no class, Final or launch right",
    })
}
