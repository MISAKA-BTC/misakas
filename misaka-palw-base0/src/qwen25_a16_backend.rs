//! **The A16 dense tier's execution backend** — what a node runs when the chain names the
//! Qwen2.5 A16 class.
//!
//! The tier already had an engine (`engine_a16`) and a fidelity story (Qwen2.5-1.5B is FAITHFUL
//! against its float reference and answers "The capital of France is Paris."). What it did not
//! have was a way to turn a running model into a CLAIM: an anchor-derived job, four committed
//! roots, and material a seat can check. That is this file, and it is deliberately the same shape
//! as `qwen36_backend` — a producer, a seat and a court reach an execution through three verbs, and
//! two families that answered them differently would be two protocols.
//!
//! # What it commits, and why it is the tiled scheme
//!
//! `full_logits_trace_root` is [`tiled_logits_trace_root_v1`] over the SELECTING rows — one row per
//! generated token, the row that token was chosen from. The class registers the tiled scheme
//! because at vocabulary 151,936 a flat pin row is 607,744 bytes against a carrier that holds
//! 81,920: a flat commitment would be a class whose every decode-token dispute is inadmissible.
//! The producer building the scheme the class registers is not an implementation detail — a class
//! that commits what it cannot produce mints and then makes no blocks.

use crate::artifact::Base0ArtifactV1;
use crate::engine_a16::{A16Cache, A16Engine};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_v2::{
    PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2, prompt_token_ids_hash_v2,
};
use kaspa_hashes::Hash64;

pub const QWEN25_A16_DOMAIN_EXECUTION: &[u8] = b"misaka-palw/qwen25-a16/execution/v1";
pub const QWEN25_A16_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/qwen25-a16/job-prompt/v1";
pub const QWEN25_A16_DOMAIN_MANIFEST: &[u8] = b"misaka-palw/qwen25-a16/manifest/v1";

fn keyed(domain: &'static [u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// One execution: every logits row it produced, and the tokens it selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qwen25A16RunV1 {
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
}

/// **The prompt an anchor implies.** A producer must not choose its own input — a class whose
/// executor picks the prompt is a class where "run the model" and "search for an input whose
/// output I like" are the same move — so the ids are derived from the anchor and the vocabulary.
pub fn qwen25_a16_prompt_for_anchor(anchor: Hash64, vocab: usize, prefill: u32) -> Vec<usize> {
    let mut prompt = Vec::with_capacity(prefill as usize);
    let mut counter = 0u64;
    while prompt.len() < prefill as usize {
        let block = keyed(QWEN25_A16_DOMAIN_JOB_PROMPT, &[anchor.as_byte_slice(), &counter.to_le_bytes()]);
        for word in block.as_byte_slice().chunks_exact(8) {
            if prompt.len() == prefill as usize {
                break;
            }
            let v = u64::from_le_bytes(word.try_into().expect("chunks_exact(8)"));
            prompt.push((v % vocab.max(1) as u64) as usize);
        }
        counter += 1;
    }
    prompt
}

/// The four roots, from one run — the same decomposition every family uses, because the header
/// does not know which family produced them.
pub fn qwen25_a16_roots_v1(job: &PalwJobContextV2, shape_id: Hash64, run: &Qwen25A16RunV1) -> (Hash64, Hash64, Hash64, Hash64) {
    let context = job.context_hash();
    let prefill = job.declared_prefill_tokens as usize;
    let selecting: Vec<Vec<i32>> =
        (0..run.generated.len()).map(|i| run.logits_rows.get(prefill.saturating_sub(1) + i).cloned().unwrap_or_default()).collect();
    debug_assert!(
        selecting
            .iter()
            .zip(&run.generated)
            .all(|(row, t)| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32 == *t),
        "every committed token is its own row's argmax — the property the close adjudicates"
    );
    let trace_root = kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(job, &selecting, &run.generated);
    let rendered =
        keyed(QWEN25_A16_DOMAIN_EXECUTION, &[b"rendered", &run.generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()]);
    let output_root = output_commitment_v2(&context, &run.generated, &rendered);
    let execution_root = keyed(
        QWEN25_A16_DOMAIN_EXECUTION,
        &[context.as_byte_slice(), shape_id.as_byte_slice(), trace_root.as_byte_slice(), output_root.as_byte_slice()],
    );
    let manifest = keyed(QWEN25_A16_DOMAIN_MANIFEST, &[context.as_byte_slice(), trace_root.as_byte_slice(), &1u64.to_le_bytes()]);
    (trace_root, output_root, execution_root, manifest)
}

pub fn qwen25_a16_material_encode_v1(run: &Qwen25A16RunV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + run.logits_rows.iter().map(|r| r.len() * 4 + 8).sum::<usize>());
    out.extend_from_slice(&(run.logits_rows.len() as u64).to_le_bytes());
    for row in &run.logits_rows {
        out.extend_from_slice(&(row.len() as u64).to_le_bytes());
        for v in row {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&(run.generated.len() as u64).to_le_bytes());
    for t in &run.generated {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

pub fn qwen25_a16_material_decode_v1(bytes: &[u8]) -> Option<Qwen25A16RunV1> {
    let mut i = 0usize;
    let mut u64_at = |i: &mut usize| -> Option<u64> {
        let end = i.checked_add(8)?;
        if end > bytes.len() {
            return None;
        }
        let v = u64::from_le_bytes(bytes[*i..end].try_into().ok()?);
        *i = end;
        Some(v)
    };
    let rows = u64_at(&mut i)? as usize;
    // A length prefix is an allocation instruction from a stranger: every count is checked against
    // the bytes actually present before a vector is reserved.
    if rows > bytes.len() {
        return None;
    }
    let mut logits_rows = Vec::with_capacity(rows.min(1024));
    for _ in 0..rows {
        let len = u64_at(&mut i)? as usize;
        let end = i.checked_add(len.checked_mul(4)?)?;
        if end > bytes.len() {
            return None;
        }
        logits_rows.push(bytes[i..end].chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
        i = end;
    }
    let count = u64_at(&mut i)? as usize;
    let end = i.checked_add(count.checked_mul(4)?)?;
    if end > bytes.len() {
        return None;
    }
    let generated = bytes[i..end].chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    i = end;
    if i != bytes.len() {
        return None; // trailing bytes are a different encoding, not this one
    }
    Some(Qwen25A16RunV1 { logits_rows, generated })
}

pub struct Qwen25A16Backend {
    artifact: std::sync::Arc<Base0ArtifactV1>,
    model_id: String,
    network_id: Vec<u8>,
    /// The artifact's own digest — what the class's roots are taken over.
    shape_id: Hash64,
    /// The CHAIN's class id (the shape profile's), which the job must name.
    class_profile_id: Hash64,
    canonical_job: (u32, u32),
}

impl Qwen25A16Backend {
    pub fn new(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        class_profile_id: Hash64,
        canonical_job: (u32, u32),
    ) -> Self {
        let shape_id = artifact.artifact_digest();
        Self { artifact, model_id: "PALW-QWEN25-A16".to_string(), network_id, shape_id, class_profile_id, canonical_job }
    }

    fn run(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<Qwen25A16RunV1, String> {
        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut cache = A16Cache::new(self.artifact.shape.n_layers);
        let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(prompt.len() + job.exact_decode_tokens as usize);
        let mut generated: Vec<u32> = Vec::with_capacity(job.exact_decode_tokens as usize);
        for (position, token) in prompt.iter().enumerate() {
            let row = engine.forward_token(&mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e:?}"))?;
            logits_rows.push(row);
        }
        // EXACT, never early: a job whose length depends on what the model said is a job whose cost
        // the producer controls.
        for step in 0..job.exact_decode_tokens as usize {
            let last = logits_rows.last().ok_or_else(|| "an empty prefill".to_string())?;
            let next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(last) as u32;
            generated.push(next);
            let position = prompt.len() + step;
            let row = engine.forward_token(&mut cache, next as usize, position).map_err(|e| format!("decode at {position}: {e:?}"))?;
            logits_rows.push(row);
        }
        Ok(Qwen25A16RunV1 { logits_rows, generated })
    }
}

impl PalwExecutionBackendV1 for Qwen25A16Backend {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        let (prefill, decode) = self.canonical_job;
        let shape = &self.artifact.shape;
        if prefill as usize + decode as usize >= shape.max_position {
            return Err(format!(
                "the canonical job needs {} positions and the table covers {}",
                prefill as usize + decode as usize,
                shape.max_position
            ));
        }
        let prompt = qwen25_a16_prompt_for_anchor(anchor, shape.vocab, prefill);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.network_id.clone(),
            job_id: anchor,
            job_nullifier: keyed(QWEN25_A16_DOMAIN_EXECUTION, &[b"nullifier", anchor.as_byte_slice()]),
            assignment_id: Hash64::default(),
            execution_seed: anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes"),
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            trace_scheme_id: kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&ids),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: shape.max_position as u32,
        };
        Ok((ctx, prompt))
    }

    fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        let run = self.run(job, prompt)?;
        let (trace_root, output_root, execution_root, trace_manifest_root) = qwen25_a16_roots_v1(job, self.shape_id, &run);
        Ok(PalwExecutionOutcomeV1 {
            trace_root,
            output_root,
            execution_root,
            trace_manifest_root,
            trace_chunk_count: 1,
            material: qwen25_a16_material_encode_v1(&run),
        })
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Some(run) = qwen25_a16_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // Recomputed under the job the claim's ANCHOR implies — the job the chain asked for, and
        // the only one a producer was entitled to run. A claim with no anchor has no block to bind
        // to, and a capture verified without one is re-usable by anyone who mines a fresh block, so
        // that case is `Unverifiable` rather than a guess.
        if claim.anchor == Hash64::default() {
            return PalwMaterialVerdictV1::Unverifiable;
        }
        let Ok((job, _)) = self.job_for_anchor(claim.anchor) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        let (trace_root, _, execution_root, _) = qwen25_a16_roots_v1(&job, self.shape_id, &run);
        if trace_root == claim.trace_root && execution_root == claim.execution_root {
            PalwMaterialVerdictV1::Matches
        } else {
            PalwMaterialVerdictV1::Mismatch
        }
    }
}
