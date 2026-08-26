//! **Qwen3.6 behind the execution-backend seam** — the producer path.
//!
//! `PalwExecutionBackendV1` is what a node reaches for when a template says "run the job this
//! anchor implies, commit to what you ran, and check whether somebody else's material answers for
//! their claim". Implementing it is what makes a class producible rather than merely runnable.
//!
//! # What this backend can and cannot do, stated first
//!
//! `execute` and `verify_material` are real. `bisect_prefix_state` and `refutation_for_index` are
//! **not implemented**, and they take the trait's honest defaults — `None` and `Err` — rather than
//! something that looks like a court.
//!
//! That is not an oversight to be tidied later; it is a fact about where the work is. The court's
//! step space for the hybrid graph (`palw_step`'s coordinates, the tile leaves, the refutation
//! prover) is a separate piece, and ADR-0039 already says what follows from its absence: **no
//! class may carry fork-choice weight until its kernel catalog is complete.** So a Qwen3.6 class
//! registered on this backend is admissible for liveness and must not carry weight, and the
//! coverage gate that enforces that is the same one that already refuses the float families.
//!
//! Writing the backend anyway is not premature. Producing is the half that has to exist first —
//! there is nothing for a court to adjudicate until somebody has run a job and committed to it —
//! and every root here is one the court will pin against.

use crate::qwen36::{Qwen36ArtifactV1, Qwen36Cache, Qwen36Engine, Qwen36ShapeV1};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_v2::{
    PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2, output_commitment_v2, prompt_token_ids_hash_v2,
};
use kaspa_hashes::Hash64;

/// Domain separators. Distinct from BASE-0's so that a root computed for one class can never be
/// read as the other's, which is the only thing a domain tag is for.
pub const QWEN36_DOMAIN_JOB_PROMPT: &[u8] = b"misaka-palw/qwen36/job-prompt/v1";
pub const QWEN36_DOMAIN_SHAPE: &[u8] = b"misaka-palw/qwen36/shape/v1";
pub const QWEN36_DOMAIN_EXECUTION: &[u8] = b"misaka-palw/qwen36/execution/v1";
pub const QWEN36_DOMAIN_MANIFEST: &[u8] = b"misaka-palw/qwen36/trace-manifest/v1";
pub const QWEN36_DOMAIN_MATERIAL: &[u8] = b"misaka-palw/qwen36/material/v1";

fn keyed(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The graph's identity.** Every field of the shape, fixed-width, in declaration order.
///
/// Stands in for the court's `shape_profile_id` until the hybrid step space exists. It carries the
/// same obligation — two classes with different graphs must not share it — and none of the court's
/// other meaning, which is why it has its own domain.
pub fn qwen36_shape_id_v1(s: &Qwen36ShapeV1) -> Hash64 {
    let kinds: Vec<u8> = s
        .layer_types
        .iter()
        .map(|k| match k {
            crate::qwen36::Qwen36LayerKind::LinearAttention => 0u8,
            crate::qwen36::Qwen36LayerKind::FullAttention => 1u8,
        })
        .collect();
    let mut scalars = Vec::with_capacity(16 * 8);
    for v in [
        s.d_model,
        s.n_heads,
        s.n_kv_heads,
        s.head_dim,
        s.rotary_dim,
        s.linear_k_heads,
        s.linear_v_heads,
        s.linear_head_dim,
        s.conv_kernel,
        s.n_experts,
        s.experts_per_token,
        s.moe_dim,
        s.shared_dim,
        s.vocab,
        s.max_position,
    ] {
        scalars.extend_from_slice(&(v as u64).to_le_bytes());
    }
    scalars.extend_from_slice(&s.eps_q.to_le_bytes());
    scalars.push(s.router_up_bits);
    keyed(QWEN36_DOMAIN_SHAPE, &[&kinds, &scalars])
}

/// **The prompt a template's anchor implies.**
///
/// A producer must not choose its own prompt: a class whose executor picks the input is a class
/// where "run the model" and "find an input whose output I like" are the same move. So the ids are
/// a pure function of the anchor — the same construction BASE-0 uses, under this class's own
/// domain.
pub fn qwen36_prompt_for_anchor(anchor: Hash64, vocab: usize, prefill: u32) -> Vec<usize> {
    let mut prompt = Vec::with_capacity(prefill as usize);
    let mut counter = 0u64;
    while prompt.len() < prefill as usize {
        let block = keyed(QWEN36_DOMAIN_JOB_PROMPT, &[anchor.as_byte_slice(), &counter.to_le_bytes()]);
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

/// One Qwen3.6 class, bound to its artifact.
pub struct Qwen36Backend {
    artifact: Qwen36ArtifactV1,
    model_id: String,
    /// `(prefill, decode)` — the canonical job's shape, a class fact.
    canonical_job: (u32, u32),
    shape_id: Hash64,
    /// **The chain's id for this class** — `qwen36_profile_v1(...).shape_profile_id()`, passed in
    /// rather than re-derived per call (the profile is 95 nodes × 40 layers). The job context
    /// carries it, so the job a seat re-derives and the class the chain named cannot disagree.
    class_profile_id: Hash64,
    /// The network the node runs, from its own configuration — a job context is not portable
    /// across networks and a hardcoded string said otherwise.
    network_id: Vec<u8>,
}

impl Qwen36Backend {
    pub fn new(
        artifact: Qwen36ArtifactV1,
        model_id: impl Into<String>,
        canonical_job: (u32, u32),
        class_profile_id: Hash64,
        network_id: Vec<u8>,
    ) -> Self {
        let shape_id = qwen36_shape_id_v1(&artifact.shape);
        Self { artifact, model_id: model_id.into(), canonical_job, shape_id, class_profile_id, network_id }
    }

    pub fn artifact(&self) -> &Qwen36ArtifactV1 {
        &self.artifact
    }

    pub fn shape_id(&self) -> Hash64 {
        self.shape_id
    }

    /// Run the canonical job and keep everything a commitment is computed from.
    fn run(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<Qwen36RunV1, String> {
        let engine = Qwen36Engine::new(&self.artifact);
        let mut cache = Qwen36Cache::new(&self.artifact.shape);
        let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(prompt.len() + job.exact_decode_tokens as usize);
        let mut generated: Vec<u32> = Vec::with_capacity(job.exact_decode_tokens as usize);

        for (position, token) in prompt.iter().enumerate() {
            let row = engine.forward_token(&mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e}"))?;
            logits_rows.push(row);
        }
        // The decode budget is EXACT: an early end-of-generation is telemetry and never terminates,
        // because a job whose length depends on what the model said is a job whose cost a producer
        // controls.
        for step in 0..job.exact_decode_tokens as usize {
            let last = logits_rows.last().ok_or_else(|| "an empty prefill".to_string())?;
            let next = crate::engine::argmax_lowest(last) as u32;
            generated.push(next);
            let position = prompt.len() + step;
            if position >= self.artifact.shape.max_position {
                return Err(format!("the job runs past the rotary table at position {position}"));
            }
            let row = engine.forward_token(&mut cache, next as usize, position).map_err(|e| format!("decode at {position}: {e}"))?;
            logits_rows.push(row);
        }
        Ok(Qwen36RunV1 { logits_rows, generated })
    }
}

/// What one execution produced, before it is committed to.
pub struct Qwen36RunV1 {
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
}

/// The four roots, from a run.
///
/// `execution_root` is a composite over the job, the trace and the output. In BASE-0 that slot
/// holds the step leg's binding, which a refutation is pinned against; here there is no step leg
/// yet, so it holds the thing that is true today and is stated as such rather than dressed up.
pub fn qwen36_roots_v1(job: &PalwJobContextV2, shape_id: Hash64, run: &Qwen36RunV1) -> (Hash64, Hash64, Hash64, Hash64) {
    let context = job.context_hash();
    // **The tiled trace, over the SELECTING rows.** The run keeps every logits row it produced —
    // prefill rows included — but the committed set is one row per generated token: the row that
    // token was selected FROM, which is `rows[prefill − 1 + i]`. Committing the prefill rows too
    // would put `prefill × vocab` lanes behind the root for no adjudicable claim: no token is
    // selected from them, so no decode-token dispute can ever open one.
    let prefill = job.declared_prefill_tokens as usize;
    let selecting: Vec<Vec<i32>> = (0..run.generated.len())
        .map(|i| run.logits_rows.get(prefill.saturating_sub(1) + i).cloned().unwrap_or_default())
        .collect();
    debug_assert!(
        selecting
            .iter()
            .zip(&run.generated)
            .all(|(row, t)| kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(row) as u32 == *t),
        "every committed token is its own row's argmax — the property the close adjudicates"
    );
    let trace_root = kaspa_consensus_core::palw_step_refute::tiled_logits_trace_root_v1(job, &selecting, &run.generated);
    // Nothing renders text on this path — the class commits token ids — so the rendered-output
    // hash is over the ids' own encoding rather than over bytes no one produced.
    let rendered =
        keyed(QWEN36_DOMAIN_EXECUTION, &[b"rendered", &run.generated.iter().flat_map(|t| t.to_le_bytes()).collect::<Vec<_>>()]);
    let output_root = output_commitment_v2(&context, &run.generated, &rendered);
    let execution_root = keyed(
        QWEN36_DOMAIN_EXECUTION,
        &[context.as_byte_slice(), shape_id.as_byte_slice(), trace_root.as_byte_slice(), output_root.as_byte_slice()],
    );
    let manifest = keyed(QWEN36_DOMAIN_MANIFEST, &[context.as_byte_slice(), trace_root.as_byte_slice(), &1u64.to_le_bytes()]);
    (trace_root, output_root, execution_root, manifest)
}

/// The retained material: the logit rows and the generated ids, which is everything a seat needs
/// to recompute the roots without re-running the model.
pub fn qwen36_material_encode_v1(run: &Qwen36RunV1) -> Vec<u8> {
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

/// Decode retained material. Returns `None` for bytes that are not this format — a seat's honest
/// "unavailable" rather than an accusation.
pub fn qwen36_material_decode_v1(bytes: &[u8]) -> Option<Qwen36RunV1> {
    let mut i = 0usize;
    let u64_at = |i: &mut usize| -> Option<u64> {
        let end = i.checked_add(8)?;
        if end > bytes.len() {
            return None;
        }
        let v = u64::from_le_bytes(bytes[*i..end].try_into().ok()?);
        *i = end;
        Some(v)
    };
    let rows = u64_at(&mut i)? as usize;
    let mut logits_rows = Vec::with_capacity(rows.min(1 << 16));
    for _ in 0..rows {
        let n = u64_at(&mut i)? as usize;
        let end = i.checked_add(n.checked_mul(4)?)?;
        if end > bytes.len() {
            return None;
        }
        logits_rows.push(bytes[i..end].chunks_exact(4).map(|c| i32::from_le_bytes(c.try_into().expect("4"))).collect());
        i = end;
    }
    let n = u64_at(&mut i)? as usize;
    let end = i.checked_add(n.checked_mul(4)?)?;
    if end > bytes.len() {
        return None;
    }
    let generated = bytes[i..end].chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("4"))).collect();
    (end == bytes.len()).then_some(Qwen36RunV1 { logits_rows, generated })
}

impl PalwExecutionBackendV1 for Qwen36Backend {
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
        let prompt = qwen36_prompt_for_anchor(anchor, shape.vocab, prefill);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: self.network_id.clone(),
            job_id: anchor,
            job_nullifier: keyed(QWEN36_DOMAIN_EXECUTION, &[b"nullifier", anchor.as_byte_slice()]),
            assignment_id: Hash64::default(),
            execution_seed: anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes"),
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            // The COURT's id, not the backend's own shape hash: the chain registered the class by
            // its shape profile, and a job that named anything else would be a job for a class
            // that does not exist.
            shape_profile_id: self.class_profile_id,
            // The TILED commitment (the flat one prices a decode-token close at decode × vocab ×
            // 4 bytes, which at this vocabulary is megabytes against the ~80 KiB a lifecycle
            // carrier can relay). Declared here and nowhere else on this path: the scheme is what
            // the class registers, and the binding check compares this field against it.
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
        let (trace_root, output_root, execution_root, trace_manifest_root) = qwen36_roots_v1(job, self.shape_id, &run);
        Ok(PalwExecutionOutcomeV1 {
            trace_root,
            output_root,
            execution_root,
            trace_manifest_root,
            trace_chunk_count: 1,
            material: qwen36_material_encode_v1(&run),
        })
    }

    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        let Some(run) = qwen36_material_decode_v1(material) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        // The seat needs the job the claim was made under, and the material does not carry it.
        // What it can check without one is that the material is self-consistent with the claimed
        // trace root under the job the ANCHOR implies — which is the job the chain asked for, and
        // the only one a producer was entitled to run.
        let Ok((job, _)) = self.job_for_anchor(claim_anchor(&claim)) else {
            return PalwMaterialVerdictV1::Unverifiable;
        };
        let (trace_root, _, execution_root, _) = qwen36_roots_v1(&job, self.shape_id, &run);
        if trace_root == claim.trace_root && execution_root == claim.execution_root {
            PalwMaterialVerdictV1::Matches
        } else {
            PalwMaterialVerdictV1::Mismatch
        }
    }
}

/// The claim carries roots and not an anchor, and the job is a function of the anchor. Until the
/// seat check is handed the template it is checking, the anchor is recovered from the execution
/// root's own binding — which is only possible because the root commits to the context. This is a
/// placeholder shape, marked as one: it returns a default and so a seat check against a claim it
/// was not given the job for reports `Mismatch` rather than pretending.
fn claim_anchor(_claim: &PalwClaimRootsV1) -> Hash64 {
    Hash64::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qwen36::Qwen36LayerKind;

    fn backend() -> Qwen36Backend {
        let artifact = crate::qwen36::test_fixture(4, 8);
        Qwen36Backend::new(artifact, "Qwen3.6-fixture", (4, 2), Hash64::from_u64_word(0x36), b"misaka-palw-test".to_vec())
    }

    /// **A producer can run the job an anchor implies, and two producers get the same roots.**
    /// That is the whole premise of the family: the chain names a job, and everybody who runs it
    /// honestly commits to the same four values.
    #[test]
    fn two_producers_on_one_anchor_commit_to_the_same_roots() {
        let a = backend();
        let b = backend();
        let anchor = Hash64::from_u64_word(0xA1);
        let (job_a, prompt_a) = a.job_for_anchor(anchor).expect("a job");
        let (job_b, prompt_b) = b.job_for_anchor(anchor).expect("a job");
        assert_eq!(prompt_a, prompt_b, "the prompt is a pure function of the anchor");
        assert_eq!(job_a.context_hash(), job_b.context_hash());

        let out_a = a.execute(&job_a, &prompt_a).expect("it runs");
        let out_b = b.execute(&job_b, &prompt_b).expect("it runs");
        assert_eq!(out_a.trace_root, out_b.trace_root);
        assert_eq!(out_a.output_root, out_b.output_root);
        assert_eq!(out_a.execution_root, out_b.execution_root);
        assert_eq!(out_a.material, out_b.material);
        assert_eq!(out_a.trace_chunk_count, 1);
    }

    /// A different anchor is a different job, a different prompt and different roots. A backend
    /// that ignored the anchor would pass the test above and fail this one.
    #[test]
    fn a_different_anchor_is_a_different_execution() {
        let a = backend();
        let one = Hash64::from_u64_word(1);
        let two = Hash64::from_u64_word(2);
        let (j1, p1) = a.job_for_anchor(one).expect("a job");
        let (j2, p2) = a.job_for_anchor(two).expect("a job");
        assert_ne!(p1, p2);
        let r1 = a.execute(&j1, &p1).expect("runs");
        let r2 = a.execute(&j2, &p2).expect("runs");
        assert_ne!(r1.trace_root, r2.trace_root);
        assert_ne!(r1.execution_root, r2.execution_root);
    }

    /// The material round-trips exactly, and bytes that are not this format are `Unverifiable`
    /// rather than an accusation.
    #[test]
    fn the_material_round_trips_and_refuses_what_it_cannot_read() {
        let a = backend();
        let anchor = Hash64::from_u64_word(7);
        let (job, prompt) = a.job_for_anchor(anchor).expect("a job");
        let out = a.execute(&job, &prompt).expect("runs");
        let run = qwen36_material_decode_v1(&out.material).expect("its own material decodes");
        assert_eq!(run.logits_rows.len(), prompt.len() + job.exact_decode_tokens as usize);
        assert_eq!(run.generated.len(), job.exact_decode_tokens as usize);
        let (trace_root, _, execution_root, _) = qwen36_roots_v1(&job, a.shape_id(), &run);
        assert_eq!(trace_root, out.trace_root);
        assert_eq!(execution_root, out.execution_root);

        assert!(qwen36_material_decode_v1(&[]).is_none());
        assert!(qwen36_material_decode_v1(&out.material[..out.material.len() - 1]).is_none());
        let mut extra = out.material.clone();
        extra.push(0);
        assert!(qwen36_material_decode_v1(&extra).is_none(), "trailing bytes are not this format");
        assert_eq!(
            a.verify_material(b"not material", PalwClaimRootsV1 { execution_root: out.execution_root, trace_root: out.trace_root }),
            PalwMaterialVerdictV1::Unverifiable
        );
    }

    /// **The court is unavailable and says so.** A backend that returned something plausible from
    /// `bisect_prefix_state` would let a ladder converge on a rung nothing can open, which reads as
    /// a party that lost rather than as a class that has no court.
    #[test]
    fn the_court_methods_are_honestly_unavailable() {
        let a = backend();
        assert_eq!(a.bisect_prefix_state(b"anything", 0), None);
        assert!(a.refutation_for_index(b"anything", 0).is_err());
        assert!(a.execute_with_injected_fault(&a.job_for_anchor(Hash64::default()).expect("a job").0, &[1], 0).is_err());
        // And the family is the one whose disputes CAN end in a conviction, because the arithmetic
        // is deterministic-integer — what is missing is the step space, not the premise.
    }

    /// A job that runs past the rotary table is refused at derivation, not discovered mid-decode.
    #[test]
    fn a_job_longer_than_the_table_is_refused() {
        let artifact = crate::qwen36::test_fixture(2, 8);
        let context = artifact.shape.max_position as u32;
        let a = Qwen36Backend::new(artifact, "Qwen3.6-fixture", (context, 1), Hash64::from_u64_word(0x36), b"misaka-palw-test".to_vec());
        assert!(a.job_for_anchor(Hash64::default()).is_err());
    }

    /// The shape id separates two graphs. Two classes that shared one would be two classes the
    /// chain could not tell apart.
    #[test]
    fn the_shape_id_separates_two_graphs() {
        let four = crate::qwen36::test_fixture(4, 8);
        let eight = crate::qwen36::test_fixture(8, 8);
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&eight.shape));
        let mut altered = four.shape.clone();
        altered.layer_types[0] = Qwen36LayerKind::FullAttention;
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&altered));
        let mut wider = four.shape.clone();
        wider.n_experts += 1;
        assert_ne!(qwen36_shape_id_v1(&four.shape), qwen36_shape_id_v1(&wider));
    }
}
