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
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
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
    /// **The class's graph, not just its id.** The id is `profile.shape_profile_id()`, so holding
    /// the profile cannot disagree with the class this backend claims to serve — and the
    /// free-prompt path needs the graph itself: the step space, the checkpoint layout and the
    /// state chunk map the class declares all come from here.
    profile: PalwShapeProfileV3,
    class_profile_id: Hash64,
    canonical_job: (u32, u32),
}

impl Qwen25A16Backend {
    pub fn new(
        artifact: std::sync::Arc<Base0ArtifactV1>,
        network_id: Vec<u8>,
        profile: PalwShapeProfileV3,
        canonical_job: (u32, u32),
    ) -> Self {
        let shape_id = artifact.artifact_digest();
        let class_profile_id = profile.shape_profile_id();
        Self { artifact, model_id: "PALW-QWEN25-A16".to_string(), network_id, shape_id, profile, class_profile_id, canonical_job }
    }

    /// The class's graph, for callers that need it directly — the same reason `Base0Backend`
    /// exposes its own: the trait's job is the verbs, not the shape.
    pub fn profile(&self) -> &PalwShapeProfileV3 {
        &self.profile
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

    fn execute_free_prompt(
        &self,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwFpRunV1, String> {
        use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;
        use kaspa_consensus_core::palw_state_chunk_map as map;

        if job.prompt_tokens as usize != prompt_tokens.len() {
            return Err(format!("the job declares {} prompt tokens and {} were supplied", job.prompt_tokens, prompt_tokens.len()));
        }
        let vocab = self.artifact.shape.vocab;
        if let Some(bad) = prompt_tokens.iter().find(|t| **t >= vocab) {
            return Err(format!("token {bad} is outside this class's vocabulary of {vocab}"));
        }

        // What the derivation asks for. `shape_profile_id` is the class; the rest are the values
        // this family's job contexts carry, taken from the same profile rather than invented.
        let class = PalwFpClassFactsV3 {
            model_profile_id: self.shape_id,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: self.shape_id,
            shape_profile_id: self.class_profile_id,
            cu_ruleset_id: Hash64::default(),
        };
        // A declared budget, decoded exactly: the count and the stop reason are known before the
        // run, and the context builder enforces the pairing rather than trusting this.
        let shape = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
        };
        // Built BEFORE the run and run under: `palw_fp_execution_root_v3` recomputes the court's
        // root from this context, so an execution carried out under any other one commits a root
        // nobody can reproduce.
        let ctx = palw_fp_job_context_v3(job, &class, &shape, &self.network_id).map_err(|e| format!("{e:?}"))?;

        let engine = A16Engine::new(&self.artifact).map_err(|e| format!("the artifact is not an A16 class: {e:?}"))?;
        let mut cache = A16Cache::new(self.artifact.shape.n_layers);
        // **The correspondence ADR-0049 Decision F requires, checked before a single leaf is filled.**
        //
        // "No worker may commit a step leg for a class whose profile does not name every narrowing
        // the engine performs." `Base0Engine` exposes `plan()` and `base0_check_graph_v1` enforces
        // it; `A16Engine` has no plan and there is no A16 counterpart, so nothing establishes the
        // correspondence for this family. Measured, the per-layer and post tables agree exactly —
        // and the pre table does not: the engine records the embedding gather AND the requant that
        // lifts it onto the A16 stream, while the profile declares only the gather. A requant is a
        // narrowing, which is precisely what Decision F is about.
        //
        // The consequence is not cosmetic. Decision F states it: a producer that ran anyway would
        // commit to arithmetic the court recomputes differently and be convicted for performing it
        // correctly. So this refuses, and names the node rather than surfacing an `UnknownSlot`
        // from three frames down. Closing it means the class declaring that node — which moves the
        // shape profile id, and therefore registers a different class.
        let probe = engine
            .forward_token_traced(&mut A16Cache::new(self.artifact.shape.n_layers), prompt_tokens[0], 0)
            .map_err(|e| format!("probing the graph: {e:?}"))?
            .1;
        let (declared_pre, recorded_pre) = (self.profile.pre_nodes.len(), probe.pre.len());
        let declared_attn = self.profile.attn_nodes.len();
        let recorded_attn = probe.attn.first().map(Vec::len).unwrap_or(0);
        if recorded_pre != declared_pre || recorded_attn != declared_attn {
            return Err(format!(
                "this class's registered graph does not name every narrowing its engine performs (ADR-0049 Decision F): pre \
                 declares {declared_pre} node(s) and the engine records {recorded_pre} — the embedding gather and the requant \
                 that lifts it onto the A16 stream, of which only the gather is declared; per-layer declares {declared_attn} \
                 against {recorded_attn} recorded. Committing a step leg under this profile would commit arithmetic the court \
                 recomputes differently."
            ));
        }

        let leaf_count = kaspa_consensus_core::palw_step::step_leaf_count(&self.profile, &ctx).map_err(|e| format!("{e:?}"))?;
        let mut capture = crate::legs::Base0StepCaptureV1::new(leaf_count).map_err(|e| format!("{e:?}"))?;
        let checkpoint_profile = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
        let mut checkpoints = crate::legs::Base0CheckpointCaptureV1::new(&ctx, &self.profile, &checkpoint_profile);

        let prefill = prompt_tokens.len();
        let mut logits_rows: Vec<Vec<i32>> = Vec::with_capacity(job.decode_token_limit as usize);
        let mut generated: Vec<u32> = Vec::with_capacity(job.decode_token_limit as usize);

        // Call 0 — prefill. Logits leaves exist only at its LAST position; the earlier rows predict
        // tokens the prompt already contains, and pushing them would place steps this class's step
        // space does not have.
        // **This family keeps a logits row per PREFILL position, not only the last one.**
        // `qwen25_a16_roots_v1` indexes `prefill - 1 + i` to check that every committed token is
        // its own row's argmax, so a vector built the floor's way — last prefill row, then the
        // decodes — makes that check read the wrong rows. The capture still drops the Post rows at
        // every position but the last, because those are steps this class's step space does not
        // have; the two conventions are about different objects and both are followed here.
        for (position, token) in prompt_tokens.iter().enumerate() {
            let (logits, trace) =
                engine.forward_token_traced(&mut cache, *token, position).map_err(|e| format!("prefill at {position}: {e:?}"))?;
            let mut rows = crate::legs::a16_captured_rows_v1(&trace);
            if position + 1 != prefill {
                rows.retain(|r| r.table != kaspa_consensus_core::palw_step::PalwStepTableV1::Post);
            }
            capture.push_call(&self.profile, &ctx, 0, position as u32, &rows).map_err(|e| format!("{e:?}"))?;
            logits_rows.push(logits);
        }
        let last = logits_rows.last().ok_or_else(|| "an empty prefill".to_string())?;
        let mut next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(last) as u32;
        generated.push(next);

        for call in 1..job.decode_token_limit as usize {
            let cache_position = prefill + call - 1;
            let (logits, trace) = engine
                .forward_token_traced(&mut cache, next as usize, cache_position)
                .map_err(|e| format!("decode at {cache_position}: {e:?}"))?;
            let rows = crate::legs::a16_captured_rows_v1(&trace);
            // The COORDINATE's position is 0 in every decode call — each call has one position —
            // while the cache position is absolute. Conflating them lands every decode row on top
            // of the first one's.
            capture.push_call(&self.profile, &ctx, call as u32, 0, &rows).map_err(|e| format!("{e:?}"))?;
            next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits) as u32;
            generated.push(next);
            logits_rows.push(logits);
            if call as u32 == checkpoints.next_covered_decode_call() {
                // Through the CACHE's own serializer, at the width the class declares. Under a map
                // that cannot describe this state — which is what this class declares today — this
                // refuses, and the run fails here rather than committing a checkpoint that opens
                // to a state it never held.
                let geometry = checkpoints.next_geometry().map_err(|e| format!("{e:?}"))?;
                let mut chunks = Vec::with_capacity(geometry.chunk_count() as usize);
                for index in 0..geometry.chunk_count() {
                    let entry = map::integer_kv_state_chunk_entry_v1(&geometry, index)
                        .ok_or_else(|| format!("the map has no chunk {index}"))?;
                    chunks.push(cache.state_chunk_bytes_v1(&entry).ok_or_else(|| {
                        format!(
                            "this cache does not fit the state map the class declares (chunk {index}, {} bytes per row)",
                            entry.row_bytes
                        )
                    })?);
                }
                checkpoints.push_chunks(chunks).map_err(|e| format!("{e:?}"))?;
            }
        }

        let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
        let checkpoints = checkpoints.finish(decode_calls / checkpoint_profile.checkpoint_interval).map_err(|e| format!("{e:?}"))?;
        let tiles = capture.finish().map_err(|e| format!("{e:?}"))?;
        // **This class's own trace scheme, not the floor's.** Its `trace_scheme_id` is the tiled
        // logits one; committing `base0_logits_trace_root_v1` here would file a root under a scheme
        // the class does not declare, and the court dispatches on the registered lane.
        let run = Qwen25A16RunV1 { logits_rows, generated: generated.clone() };
        let (trace_root, output_root, _, trace_manifest_root) = qwen25_a16_roots_v1(&ctx, self.shape_id, &run);
        let activation_leg_root = crate::produce::base0_activation_leg_root_v1(&ctx);
        let binding =
            crate::legs::base0_binding_from_capture_v1(&self.profile, &ctx, &tiles, &checkpoints, trace_root, activation_leg_root)
                .map_err(|e| format!("{e:?}"))?;
        let (checkpoint_leg_root, step_leg_root) = crate::legs::base0_leg_roots_from_binding_v1(&binding);

        Ok(kaspa_consensus_core::palw_backend::PalwFpRunV1 {
            outcome: PalwExecutionOutcomeV1 {
                trace_root,
                output_root,
                execution_root: binding.committed_execution_root,
                trace_manifest_root,
                trace_chunk_count: 1,
                material: qwen25_a16_material_encode_v1(&run),
            },
            facts: PalwFpRunFactsV3 {
                full_logits_trace_root: trace_root,
                activation_leg_root,
                checkpoint_leg_root,
                step_leg_root,
                ..shape
            },
            output_token_ids: generated,
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

#[cfg(test)]
mod free_prompt_tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use crate::engine_a16::derived_a16_store;
    use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, palw_fp_execution_root_v3, palw_fp_job_context_v3};
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptJobV3,
    };
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v1, qwen25_a16_profile_v2};
    use kaspa_consensus_core::palw_state_chunk_map as map;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const NETWORK: &[u8] = b"misaka-palw-rc";

    /// A class small enough to run in a unit test, built from ONE geometry so the artifact and the
    /// profile cannot describe different models — which is the failure a hand-written pair invites.
    fn class(map_id: Hash64) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
        class_from(map_id, false)
    }

    /// `corrected` builds the class `qwen25_a16_profile_v2` describes: the pre table names the
    /// embed-lift requant and the state map is the four-byte one.
    fn class_from(map_id: Hash64, corrected: bool) -> (std::sync::Arc<Base0ArtifactV1>, PalwShapeProfileV3) {
        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let mut profile = if corrected {
            qwen25_a16_profile_v2(geometry).expect("a valid corrected A16 profile")
        } else {
            qwen25_a16_profile_v1(geometry).expect("a valid A16 profile")
        };
        if !corrected {
            profile.state_chunk_map_id = map_id;
        }
        let shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique");
        (std::sync::Arc::new(artifact), profile)
    }

    fn job(profile: &PalwShapeProfileV3, prompt_tokens: u32, decode: u32) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: profile.shape_profile_id(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 4242,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(0x71),
            prompt_tokens,
            decode_token_limit: decode,
            max_context_tokens: profile.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        }
    }

    /// **The corrected class runs a caller's prompt and commits a root the derivation recomputes.**
    ///
    /// This is the property everything else was for: a language model's own execution, under a
    /// class whose graph names what the engine does and whose state map is the width the cache
    /// holds, producing a free-prompt commitment a court could recompute. The two defects that
    /// stopped it were measured, not guessed, and correcting either one moves the class id — so
    /// this is a class to register, and the test is what says it would work once registered.
    #[test]
    fn the_corrected_a16_class_commits_the_root_the_derivation_recomputes() {
        let (artifact, profile) = class_from(map::integer_kv_state_chunk_map_id_v2(), true);
        let digest = artifact.artifact_digest();
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let job = job(&profile, prompt.len() as u32, 3);

        let run = backend.execute_free_prompt(&job, &prompt).expect("the corrected class runs a caller's prompt");

        let class_facts = PalwFpClassFactsV3 {
            model_profile_id: digest,
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: digest,
            shape_profile_id: profile.shape_profile_id(),
            cu_ruleset_id: Hash64::default(),
        };
        let ctx = palw_fp_job_context_v3(&job, &class_facts, &run.facts, NETWORK).expect("the finished run implies a context");
        assert_eq!(
            palw_fp_execution_root_v3(&ctx, &run.facts),
            run.outcome.execution_root,
            "the derivation and the run must agree, or the court convicts the honest"
        );

        // All four legs measured — four zero roots would satisfy the equality above and mean
        // nothing — and the answer, which is the other half of the one inference.
        assert_ne!(run.facts.full_logits_trace_root, Hash64::default());
        assert_ne!(run.facts.step_leg_root, Hash64::default());
        assert_ne!(run.facts.checkpoint_leg_root, Hash64::default());
        assert_eq!(run.facts.stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
        assert_eq!(run.output_token_ids.len(), job.decode_token_limit as usize);

        // And it is a different class from the one testnet-11 carries, which is the cost.
        let (_, registered) = class_from(map::integer_kv_state_chunk_map_id_v1(), false);
        assert_ne!(profile.shape_profile_id(), registered.shape_profile_id());
    }

    /// **A16 refuses the free-prompt path, and the refusal names why — under either map.**
    ///
    /// This test was first written to assert the opposite: give the class a state map that fits its
    /// cache and the round trip should close. It does not, and the reason is a second gap the first
    /// one was hiding. ADR-0049 Decision F requires a class's profile to name every narrowing its
    /// engine performs; `Base0Engine` exposes `plan()` and `base0_check_graph_v1` enforces it.
    /// `A16Engine` has no plan and there is no A16 counterpart. Measured, the per-layer and post
    /// tables agree exactly — and the pre table does not: the engine records the embedding gather
    /// AND the requant that lifts it onto the A16 stream, while the profile declares only the
    /// gather. A requant is a narrowing, which is what Decision F is about.
    ///
    /// Both maps are exercised because the graph gap fires first: giving this class a state map
    /// that fits its cache would not make it adjudicable.
    #[test]
    fn a16_refuses_the_free_prompt_path_until_its_graph_is_reconciled() {
        for map_id in [map::integer_kv_state_chunk_map_id_v2(), map::integer_kv_state_chunk_map_id_v1()] {
            let (artifact, profile) = class(map_id);
            let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
            let prompt: Vec<usize> = vec![3, 9, 17, 33];
            let job = job(&profile, prompt.len() as u32, 3);
            let error = match backend.execute_free_prompt(&job, &prompt) {
                Err(e) => e,
                Ok(_) => panic!("a class whose graph does not name what its engine computes must not commit a step leg"),
            };
            assert!(error.contains("registered graph"), "the refusal names the gap: {error}");
            assert!(error.contains("requant"), "and the node it is missing: {error}");
        }
    }

    /// **Under the map the class declares TODAY, the same run refuses.**
    ///
    /// `integer_kv_state_chunk_map_id_v1` describes one byte per element and this cache holds
    /// `i32`. The refusal is the whole point: the alternative implementation truncates, passes
    /// every downstream check, and commits a checkpoint that opens to a state the producer never
    /// had.
    #[test]
    fn a16_refuses_to_commit_under_a_map_that_cannot_describe_its_cache() {
        let (artifact, profile) = class(map::integer_kv_state_chunk_map_id_v1());
        let backend = Qwen25A16Backend::new(artifact, NETWORK.to_vec(), profile.clone(), (4, 2));
        let prompt: Vec<usize> = vec![3, 9, 17, 33];
        let job = job(&profile, prompt.len() as u32, 3);

        let error = match backend.execute_free_prompt(&job, &prompt) {
            Err(e) => e,
            Ok(_) => panic!("a one-byte map cannot describe an i32 cache, and committing anyway is the defect"),
        };
        assert!(error.contains("registered graph") || error.contains("state map"), "the error names the defect it hit first: {error}");
    }

    fn artifact_digest_of(backend: &Qwen25A16Backend) -> Hash64 {
        backend.artifact.artifact_digest()
    }
}
