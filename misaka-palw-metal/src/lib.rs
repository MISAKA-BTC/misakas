//! **Family M: the Metal/GGUF execution backend** (ADR-0051 step 2).
//!
//! # Why this crate links nothing
//!
//! The inference happens in `misaka-palw-worker`, which is statically linked against a *pinned*
//! llama.cpp build and therefore only builds where that tree exists. `kaspad` must keep building
//! on a Linux x86 host with no llama.cpp and no Metal — the deterministic floor is the liveness
//! anchor and it must never depend on a GPU toolchain — so the family boundary is a **process
//! boundary**, not a link boundary. This crate spawns the worker and speaks the framed-Borsh
//! protocol that already exists (`--mode v2-legs-job`), so a node without a worker simply has no
//! Family-M backend rather than failing to compile.
//!
//! That is also the ADR-0026 shape: runtime separated from verification, the runtime addressed by
//! its measured identity rather than by its source.
//!
//! # What is committed, and what is not re-derived here
//!
//! Everything. The worker computes `full_logits_trace_root_v2`, the output commitment, the
//! activation and checkpoint legs and the composite `committed_execution_root`; this crate
//! *transports* them. It deliberately recomputes none of it: a second implementation of the
//! commitment on this side would be a second thing to disagree with the runtime, which is the
//! defect class this repository keeps finding ("correspondence defects are found by round trips").
//! What it does check is the round trip it can afford — that the result answers the request it
//! sent, by `request_hash` and `job_id`.
//!
//! # No court, by construction
//!
//! [`PalwExecutionBackendV1`]'s court methods are left at their defaults, which answer `None` and
//! `Err`. That is not an omission to fill in later: a Family-M dispute has no arithmetic terminal,
//! because the family verifies within a tolerance and a tolerance cannot separate "lied by ε" from
//! "rounded by ε". `PalwExecutionFamilyV1::MetalGguf.is_court_adjudicable()` is `false` and this
//! backend is the reason.

use kaspa_consensus_core::palw_backend::{
    PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionFamilyV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_legs::PalwLegsJobResultV1;
use kaspa_consensus_core::palw_v2::{
    PALW_V2_MAX_FRAME_BYTES, PalwJobEnvelopeV2, PalwJobModeV2, decode_framed_borsh, job_request_hash_v2, read_framed, write_framed,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub mod material;
pub use material::{PalwMetalMaterialV1, metal_material_decode_v1, metal_material_encode_v1};

/// Everything a Family-M class pins (ADR-0051 Decision 2), as this node holds it.
///
/// The chain's registration carries the same values; a node whose worker reports a different
/// identity is running a different class and must not produce for this one. Checking that is
/// [`MetalBackend::check_runtime_identity`], and it is a startup check rather than a per-block one
/// because a runtime does not change under a running process.
#[derive(Clone, Debug)]
pub struct MetalClassPinsV1 {
    /// Hugging Face-style model id, for logs. Never dispatch.
    pub model_id: String,
    /// The worker binary that owns the pinned runtime. Its own sha256 is inside the runtime
    /// manifest the worker reports, so naming a path here is naming a candidate, not a trust.
    pub worker_path: PathBuf,
    /// The identity the CHAIN registered. The worker's reported manifest hash must equal it.
    pub runtime_manifest_hash: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub tokenizer_id: Hash64,
    /// The canonical job: how many prompt tokens are fed and how many are generated.
    pub prefill_tokens: u32,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,
    /// The class's vocabulary, for deriving a prompt from an anchor.
    pub vocab_size: u32,
    /// The network domain the envelope is bound to.
    pub network_id: Vec<u8>,
}

/// The Family-M backend: a client of one pinned worker, bound to one class.
pub struct MetalBackend {
    pins: MetalClassPinsV1,
}

/// Domain for deriving a Family-M job's prompt from its anchor. Separate from the integer
/// family's, because two families deriving the same prompt from the same anchor would be two
/// classes doing identical work — which the share table would then pay for twice.
pub const PALW_METAL_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/metal/job-prompt/v1";

impl MetalBackend {
    pub fn new(pins: MetalClassPinsV1) -> Self {
        Self { pins }
    }

    pub fn pins(&self) -> &MetalClassPinsV1 {
        &self.pins
    }

    /// **Does this node's worker run the class the chain registered?**
    ///
    /// Asks the worker for its measured identity and compares the manifest hash. A node that
    /// produced without this would sign attempts claiming a runtime it is not running — and the
    /// whole family rests on the runtime being the pinned one, since nothing downstream re-derives
    /// the arithmetic.
    ///
    /// Run at startup. It costs one process spawn and no inference.
    pub fn check_runtime_identity(&self) -> Result<(), String> {
        let out = Command::new(&self.pins.worker_path)
            .args(["--mode", "v2-manifest"])
            .output()
            .map_err(|e| format!("cannot run the worker at {}: {e}", self.pins.worker_path.display()))?;
        if !out.status.success() {
            return Err(format!("the worker refused to report its manifest: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        // The JSON is display-only by the worker's own doc — the canonical identity is the hash —
        // so this reads exactly the one field that IS canonical and ignores the rest.
        let reported = json_hex_field(&text, "runtime_manifest_hash_v2")
            .ok_or("the worker's manifest has no runtime_manifest_hash_v2")?;
        let want = hex_of(&self.pins.runtime_manifest_hash);
        if reported != want {
            return Err(format!(
                "this worker is runtime {reported}, and the class is registered at {want} — a different runtime is a different class"
            ));
        }
        Ok(())
    }

    /// One `--mode v2-legs-job` round trip.
    fn run_worker(&self, envelope: &PalwJobEnvelopeV2) -> Result<PalwLegsJobResultV1, String> {
        let payload = borsh::to_vec(envelope).map_err(|e| format!("the envelope does not encode: {e}"))?;
        let expected_request_hash = job_request_hash_v2(&payload);

        let mut child = Command::new(&self.pins.worker_path)
            .args(["--mode", "v2-legs-job"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot spawn the worker: {e}"))?;

        // One frame in, then EOF. Both pipes are drained on threads BEFORE the wait: llama's
        // model-load chatter alone can fill an OS pipe buffer, and a worker blocked writing stderr
        // while we block reading stdout is a deadlock with no error message. (The agent learned
        // this the same way; the comment is kept because the shape is not obvious from the code.)
        {
            let mut stdin = child.stdin.take().ok_or("the worker's stdin was not piped")?;
            write_framed(&mut stdin, &payload).map_err(|e| format!("cannot write the job frame: {e}"))?;
            stdin.flush().map_err(|e| format!("cannot flush the job frame: {e}"))?;
        }
        let mut stdout = child.stdout.take().ok_or("the worker's stdout was not piped")?;
        let mut stderr = child.stderr.take().ok_or("the worker's stderr was not piped")?;
        let err_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
            buf
        });
        let frame = read_framed(&mut stdout, PALW_V2_MAX_FRAME_BYTES);
        let status = child.wait().map_err(|e| format!("the worker did not exit: {e}"))?;
        let log = err_thread.join().unwrap_or_default();

        if !status.success() {
            // The worker writes NOTHING to stdout on any failure — partial results never leave the
            // process — so a non-zero exit is the whole story and stderr is where it is told.
            return Err(format!("the worker refused the job ({status}): {}", String::from_utf8_lossy(&log).trim()));
        }
        let frame = frame.map_err(|e| format!("the worker's reply frame is unreadable: {e:?}"))?;
        let result: PalwLegsJobResultV1 =
            decode_framed_borsh(&frame).map_err(|e| format!("the worker's reply does not decode: {e:?}"))?;

        // The round trip, checked. Not the commitments — recomputing those here would be a second
        // implementation to disagree with the runtime — but the identity of the ANSWER: this reply
        // must be to the request we sent.
        if result.result.request_hash != expected_request_hash {
            return Err("the worker answered a different request than the one sent".to_string());
        }
        if result.result.job_id != envelope.job_id {
            return Err("the worker's reply names a different job".to_string());
        }
        if result.binding.full_logits_trace_root != result.result.projection.full_logits_trace_root {
            return Err("the worker's binding and projection disagree about the trace root".to_string());
        }
        Ok(result)
    }

    /// The envelope for one job, from the class's pins and a derived prompt.
    fn envelope_for(&self, job: &PalwJobContextV2, prompt: &[usize]) -> PalwJobEnvelopeV2 {
        PalwJobEnvelopeV2 {
            version: kaspa_consensus_core::palw_v2::PALW_JOB_WIRE_VERSION_V2,
            network_id: self.pins.network_id.clone(),
            job_id: job.job_id,
            job_nullifier: job.job_nullifier,
            mode: PalwJobModeV2::Execute,
            model_profile_id: self.pins.model_profile_id,
            runtime_manifest_hash: self.pins.runtime_manifest_hash,
            runtime_class_id: self.pins.runtime_class_id,
            shape_profile_id: self.pins.shape_profile_id,
            trace_scheme_id: self.pins.trace_scheme_id,
            cu_ruleset_id: self.pins.cu_ruleset_id,
            execution_seed: job.execution_seed,
            prompt_token_ids: prompt.iter().map(|t| *t as u32).collect(),
            exact_decode_tokens: self.pins.exact_decode_tokens,
            max_context_tokens: self.pins.max_context_tokens,
            assignment_id: job.assignment_id,
            assignment_epoch: 0,
            deadline_unix_ms: 0,
        }
    }
}

impl PalwExecutionBackendV1 for MetalBackend {
    fn family(&self) -> PalwExecutionFamilyV1 {
        PalwExecutionFamilyV1::MetalGguf
    }

    fn model_id(&self) -> &str {
        &self.pins.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        if self.pins.vocab_size == 0 {
            return Err("a class with an empty vocabulary has no job".to_string());
        }
        // Derived, never chosen — the same rule the integer family follows and for the same
        // reason: an executor that picks its own prompt can search for an input whose output it
        // likes, and that search is free where running the model is not.
        let mut prompt = Vec::with_capacity(self.pins.prefill_tokens as usize);
        let mut counter = 0u64;
        while prompt.len() < self.pins.prefill_tokens as usize {
            let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_METAL_DOMAIN_JOB_PROMPT).to_state();
            h.update(anchor.as_byte_slice());
            h.update(&counter.to_le_bytes());
            for word in h.finalize().as_bytes().chunks_exact(8) {
                if prompt.len() == self.pins.prefill_tokens as usize {
                    break;
                }
                let v = u64::from_le_bytes(word.try_into().expect("chunks_exact(8)"));
                prompt.push((v % self.pins.vocab_size as u64) as usize);
            }
            counter += 1;
        }
        let job = PalwJobContextV2 {
            version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.pins.network_id.clone(),
            job_id: anchor,
            job_nullifier: anchor,
            assignment_id: anchor,
            execution_seed: anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32"),
            model_profile_id: self.pins.model_profile_id,
            runtime_manifest_hash: self.pins.runtime_manifest_hash,
            runtime_class_id: self.pins.runtime_class_id,
            shape_profile_id: self.pins.shape_profile_id,
            trace_scheme_id: self.pins.trace_scheme_id,
            cu_ruleset_id: self.pins.cu_ruleset_id,
            tokenizer_id: self.pins.tokenizer_id,
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(
                &prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(),
            ),
            declared_prefill_tokens: self.pins.prefill_tokens,
            exact_decode_tokens: self.pins.exact_decode_tokens,
            max_context_tokens: self.pins.max_context_tokens,
        };
        Ok((job, prompt))
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let envelope = self.envelope_for(job, prompt);
        let result = self.run_worker(&envelope)?;
        let material = metal_material_encode_v1(&PalwMetalMaterialV1 {
            job: job.clone(),
            prompt_token_ids: prompt.iter().map(|t| *t as u32).collect(),
            projection: result.result.projection.clone(),
            binding: result.binding.clone(),
        })
        .map_err(|e| format!("the material does not encode: {e}"))?;
        Ok(PalwExecutionOutcomeV1 {
            trace_root: result.binding.full_logits_trace_root,
            output_root: result.result.projection.output_commitment,
            execution_root: result.binding.committed_execution_root,
            // **One chunk, and it is the whole material.** A manifest claiming more chunks than
            // the producer retains would be a retention promise it cannot keep; the integer floor
            // makes the same choice for the same reason.
            trace_manifest_root: metal_trace_manifest_root_v1(&material),
            trace_chunk_count: 1,
            material,
        })
    }

    /// **A seat re-runs the job and compares roots — exactly** (ADR-0051 Decision 4, implemented
    /// as full replay rather than spot replay, and the reason is a measurement).
    ///
    /// Decision 4 proposed sampling `m` positions and teacher-forcing each, because replaying a
    /// whole generation was assumed too expensive to ask of a seat. On the class this family
    /// launches with it is not: the canonical job (8 prefill / 4 decode) runs in **2.75 s** on an
    /// M4 Pro. Full replay is simpler, and it is *stronger* than the sampled rule in two ways —
    /// it checks every position instead of `m` of them, and it compares **exactly** rather than
    /// within a tolerance, which the same measurement licenses: four runs of one anchor on one
    /// machine are byte-identical, `gemm_trace_root` included.
    ///
    /// The sampled, tolerant rule is not discarded — it is what a class needs whose generations
    /// are long enough that replay costs more than a seat will pay, or whose panel spans device
    /// generations where ε > 0. Both are properties of a class, so both belong in its
    /// registration, and neither is this class.
    ///
    /// A seat that cannot run the worker answers `Unverifiable` and files nothing. That is not a
    /// mismatch: the producer may be perfectly honest and this seat merely unequipped, and the
    /// receipt lane already separates "I could not check" from "this does not match".
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Ok(decoded) = metal_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // Cheap checks first: a material that does not even claim to be this claim's execution is
        // refused before a GPU is woken.
        if decoded.binding.committed_execution_root != claim.execution_root
            || decoded.binding.full_logits_trace_root != claim.trace_root
            || decoded.projection.full_logits_trace_root != decoded.binding.full_logits_trace_root
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        // The prompt must be the one the carried job commits to — otherwise a seat would replay a
        // prompt the producer chose after the fact.
        if kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&decoded.prompt_token_ids)
            != decoded.job.prompt_token_ids_hash
        {
            return PalwMaterialVerdictV1::Mismatch;
        }
        let prompt: Vec<usize> = decoded.prompt_token_ids.iter().map(|t| *t as usize).collect();
        match self.execute(&decoded.job, &prompt) {
            // Ran it, and got what the chain holds. This is the only path to `Valid`.
            Ok(mine) if mine.execution_root == claim.execution_root && mine.trace_root == claim.trace_root => {
                PalwMaterialVerdictV1::Matches
            }
            // Ran it, and got something else. The producer committed to an execution this
            // runtime does not reproduce — which on a same-device panel is a lie, and across
            // device generations may not be. The family never convicts either way: no quorum
            // forms, the claim voids, and the escrow is burned.
            Ok(_) => PalwMaterialVerdictV1::Mismatch,
            // Could not run it at all.
            Err(_) => PalwMaterialVerdictV1::Unverifiable,
        }
    }
}

/// The DA manifest root over one chunk of material.
pub fn metal_trace_manifest_root_v1(material: &[u8]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/metal/trace-manifest/v1").to_state();
    h.update(&(material.len() as u64).to_le_bytes());
    h.update(material);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn hex_of(h: &Hash64) -> String {
    h.as_byte_slice().iter().map(|b| format!("{b:02x}")).collect()
}

/// Pulls one `"key":"hex"` out of the worker's display JSON without a JSON dependency. The value
/// is hex by construction (every field this reads is a hash), so the scan is exact rather than
/// lenient: anything that is not `"<key>":"<hex>"` yields `None` and the caller refuses.
fn json_hex_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    let value = &rest[..end];
    value.chars().all(|c| c.is_ascii_hexdigit()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins() -> MetalClassPinsV1 {
        MetalClassPinsV1 {
            model_id: "test/model".into(),
            worker_path: PathBuf::from("/nonexistent/worker"),
            runtime_manifest_hash: Hash64::from_u64_word(1),
            model_profile_id: Hash64::from_u64_word(2),
            runtime_class_id: Hash64::from_u64_word(3),
            shape_profile_id: Hash64::from_u64_word(4),
            trace_scheme_id: Hash64::from_u64_word(5),
            cu_ruleset_id: Hash64::from_u64_word(6),
            tokenizer_id: Hash64::from_u64_word(7),
            prefill_tokens: 8,
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
            vocab_size: 248_320,
            network_id: b"misaka-metal-test".to_vec(),
        }
    }

    /// **The family answers the one question that decides everything downstream.**
    #[test]
    fn family_m_is_never_court_adjudicable() {
        let b = MetalBackend::new(pins());
        assert_eq!(b.family(), PalwExecutionFamilyV1::MetalGguf);
        assert!(!b.family().is_court_adjudicable());
        // The court methods answer honestly rather than plausibly: a `None` prefix state loses a
        // rung, which is the correct outcome for a party that cannot substantiate its execution,
        // and an `Err` terminal refuses to produce evidence the family cannot support.
        assert!(b.bisect_prefix_state(b"anything", 0).is_none());
        assert!(b.refutation_for_index(b"anything", 0).is_err());
        assert!(b.execute_with_injected_fault(&b.job_for_anchor(Hash64::from_u64_word(1)).unwrap().0, &[], 0).is_err());
    }

    /// The anchor decides the prompt, and a different anchor is a different job — the property
    /// that stops an executor searching for an input whose output it likes.
    #[test]
    fn the_anchor_decides_the_job() {
        let b = MetalBackend::new(pins());
        let (ja, pa) = b.job_for_anchor(Hash64::from_u64_word(1)).expect("job");
        let (jb, pb) = b.job_for_anchor(Hash64::from_u64_word(2)).expect("job");
        assert_eq!(pa.len(), 8, "the prefill count is the class's");
        assert_ne!(pa, pb);
        assert_ne!(ja.prompt_token_ids_hash, jb.prompt_token_ids_hash);
        assert!(pa.iter().all(|t| *t < 248_320), "every token is inside the class's vocabulary");
        // The job is a pure function of the anchor: two derivations agree.
        assert_eq!(pa, b.job_for_anchor(Hash64::from_u64_word(1)).unwrap().1);
    }

    /// **Family M's job derivation is its own.** Sharing the integer family's domain would let two
    /// classes derive identical work from one anchor, and the share table would pay for both.
    #[test]
    fn the_prompt_domain_is_not_the_integer_familys() {
        assert_ne!(PALW_METAL_DOMAIN_JOB_PROMPT, b"misaka-palw/base0/rc-job-prompt/v1");
    }

    /// A worker that is not there is an error, not a panic and not a silent fallback: a node whose
    /// backend quietly did nothing would look exactly like a node whose class is idle.
    #[test]
    fn a_missing_worker_is_reported() {
        let b = MetalBackend::new(pins());
        let err = b.check_runtime_identity().expect_err("there is no worker at that path");
        assert!(err.contains("cannot run the worker"), "{err}");
    }

    /// **The real thing, on real Metal** — ignored because it needs the pinned worker, the pinned
    /// GGUF and an Apple GPU, none of which exist in CI.
    ///
    /// ```text
    /// MISAKA_PALW_WORKER=target/release/palw-worker \
    /// MISAKA_PALW_GGUF=<pinned>.gguf \
    /// cargo test -p misaka-palw-metal --lib metal_end_to_end -- --ignored --nocapture
    /// ```
    ///
    /// It asserts the two things a wiring test cannot fake: that a Family-M execution produces the
    /// four roots an attempt carries, and that the SAME anchor produces the same roots twice —
    /// which is the family's whole premise, measured rather than assumed.
    #[test]
    #[ignore]
    fn metal_end_to_end_produces_an_attempt_and_reproduces_it() {
        let Ok(worker) = std::env::var("MISAKA_PALW_WORKER") else {
            eprintln!("set MISAKA_PALW_WORKER (and MISAKA_PALW_GGUF) to run this");
            return;
        };
        // The class's pins come from the worker itself here — a registration would carry them, and
        // reading them back is exactly the startup check a producer does before it may produce.
        let manifest = Command::new(&worker).args(["--mode", "v2-manifest"]).output().expect("the worker runs");
        assert!(manifest.status.success(), "{}", String::from_utf8_lossy(&manifest.stderr));
        let doc = String::from_utf8_lossy(&manifest.stdout);
        let field = |k: &str| -> Hash64 {
            let hex = json_hex_field(&doc, k).unwrap_or_else(|| panic!("the manifest has no {k}"));
            let mut b = [0u8; 64];
            for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate().take(64) {
                b[i] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap();
            }
            Hash64::from_bytes(b)
        };
        let pins = MetalClassPinsV1 {
            model_id: "pinned/worker-reported".into(),
            worker_path: PathBuf::from(&worker),
            runtime_manifest_hash: field("runtime_manifest_hash_v2"),
            model_profile_id: field("model_profile_id"),
            runtime_class_id: field("runtime_class_id"),
            shape_profile_id: field("shape_profile_id_v2"),
            trace_scheme_id: field("trace_scheme_id_v2"),
            cu_ruleset_id: field("cu_ruleset_id_v2"),
            tokenizer_id: field("tokenizer_id_v2"),
            prefill_tokens: 8,
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
            vocab_size: 248_320,
            network_id: b"misaka-palw-rc".to_vec(),
        };
        let backend = MetalBackend::new(pins);
        backend.check_runtime_identity().expect("the worker is the runtime its own manifest names");

        let anchor = Hash64::from_u64_word(0x5EA_51DE);
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let started = std::time::Instant::now();
        let a = backend.execute(&job, &prompt).expect("Family M executes on this machine");
        eprintln!(
            "metal execute: {:?}  trace_root={}…  execution_root={}…  material={}B",
            started.elapsed(),
            &hex_of(&a.trace_root)[..16],
            &hex_of(&a.execution_root)[..16],
            a.material.len()
        );
        assert_ne!(a.trace_root, Hash64::default());
        assert_ne!(a.execution_root, Hash64::default());
        assert_ne!(a.output_root, Hash64::default());
        assert_eq!(a.trace_chunk_count, 1);
        assert!(!a.material.is_empty());

        // A seat's transport check passes against the roots this run committed, and fails against
        // somebody else's.
        let claim = PalwClaimRootsV1 { execution_root: a.execution_root, trace_root: a.trace_root };
        let seat_started = std::time::Instant::now();
        assert_eq!(
            backend.verify_material(&a.material, claim),
            PalwMaterialVerdictV1::Matches,
            "a seat re-running the job must reproduce the roots the producer committed"
        );
        eprintln!("seat verify (full replay): {:?}", seat_started.elapsed());
        let other = PalwClaimRootsV1 { execution_root: Hash64::from_u64_word(0xBAD), ..claim };
        assert_eq!(backend.verify_material(&a.material, other), PalwMaterialVerdictV1::Mismatch);

        // **The premise, measured.** Same anchor, same job, same machine: same roots.
        let b = backend.execute(&job, &prompt).expect("the second run runs");
        assert_eq!(b.trace_root, a.trace_root, "Metal did not reproduce its own trace root");
        assert_eq!(b.execution_root, a.execution_root, "Metal did not reproduce its own execution root");
        assert_eq!(b.output_root, a.output_root);
        eprintln!("metal reproducibility: two runs of one anchor agree on all four roots");
    }

    #[test]
    fn the_manifest_field_scan_is_exact() {
        let doc = r#"{"a":"zz","runtime_manifest_hash_v2":"00ff1234","b":2}"#;
        assert_eq!(json_hex_field(doc, "runtime_manifest_hash_v2").as_deref(), Some("00ff1234"));
        assert_eq!(json_hex_field(doc, "a"), None, "a non-hex value is not a hash and is refused");
        assert_eq!(json_hex_field(doc, "missing"), None);
    }
}
