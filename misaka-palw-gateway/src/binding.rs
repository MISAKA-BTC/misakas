//! **The binding audit** (RFC-0001 + RFC-0003 delivery, task 1): for every fact a claim must pin, WHERE it enters the job or the
//! commitment — and a test per field that fails if changing it would NOT change the claim.
//!
//! > A request that only succeeded in the UI is not a verifiable claim. Every claim binds the user's input, the canonical token
//! > input, the task and profile, the model and class, the tokenizer, the generation seed, the decode config, the output, the
//! > pipeline stages and edges, the state transitions and the evidence root — so that nothing uncommitted can be changed later.
//!
//! The audit is a chain of four links, and each link has a test that is exhaustive over its fields by construction (a struct
//! destructured WITHOUT `..`, so adding a field to the job, the request or the commitment is a compile error here until the new field
//! is given a row and a mutation):
//!
//! | link | what it guarantees | enforced by | test |
//! |---|---|---|---|
//! | 1. gateway input → `PalwFpWorkerRequestV3` | every thing a person or the chain controls lands in a named request field | [`crate::prepare_request`] | `link_1_*` |
//! | 2. request field → job field | the worker cannot answer a different request than it was sent | `PalwFpWorkerResultV3::validate_against_request` + [`check_result_against_manifest`] | `link_2_*` |
//! | 3. job → job id → claim id | changing any job field (decode config included) changes the claim | `fp_job_id_v3`, `fp_claim_id_v3` | `link_3_*` |
//! | 4. result → commitment | the commitment is the execution, field by field | `signable_claim_id` (the sign gate) | `link_4_*` |
//!
//! plus the generative path (`gen_*` rows: FP Job V5, the tensor job, the execution roots) and the evidence manifest. Rows marked
//! **not bound** are findings in the plain sense: the input exists, it is NOT in the claim, and the table says what carries it instead.

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerManifestV1, PalwFpWorkerRequestV3, PalwFpWorkerResultV3};

/// One audited fact: what it is, where it enters, and the test that proves it.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // `enters` is the audit's prose: read by `audit_markdown` (the record document is generated from it) and by humans.
pub struct BindingRow {
    pub fact: &'static str,
    /// Where it enters the job / commitment / execution root, or what carries it when it does not.
    pub enters: &'static str,
    /// The test in this file that proves the row (the audit test asserts it exists).
    pub test: &'static str,
    pub bound: Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// In the job id (and so the claim id): changing it changes the claim.
    InClaim,
    /// Bound transitively: the fact is canonicalised into a bound field (the ids, the decode config).
    ViaCanonicalForm,
    /// Checked by the gateway before a commitment exists, but not itself a claim field.
    CheckedNotCommitted,
    /// NOT in the claim. The row says what holds it instead.
    NotBound,
}

/// **The audit table.** Every row names a test below; `the_audit_table_names_only_tests_that_exist` holds the table to that.
pub const AUDIT: &[BindingRow] = &[
    // ---- link 1: gateway input -> worker request ----
    BindingRow { fact: "user message text", enters: "chat template -> segments -> ids -> job.prompt_token_ids_hash", test: "link_1_the_users_text_and_every_turn_around_it_reach_the_prompt_hash", bound: Bound::ViaCanonicalForm },
    BindingRow { fact: "system / assistant turns, tools, tool_choice, response_format instruction", enters: "rendered into the turns before the template -> ids", test: "link_1_the_users_text_and_every_turn_around_it_reach_the_prompt_hash", bound: Bound::ViaCanonicalForm },
    BindingRow { fact: "chat template (built-in, sidecar)", enters: "the segments the template emits -> ids; its id is NOT a claim field", test: "link_1_the_chat_template_changes_the_ids_and_its_id_is_not_committed", bound: Bound::ViaCanonicalForm },
    BindingRow { fact: "max_tokens", enters: "request.decode_token_limit -> job.decode_token_limit", test: "link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field", bound: Bound::InClaim },
    BindingRow { fact: "temperature, seed (generation seed R for text)", enters: "request.temperature_q / sampling_seed -> job.temperature_q / sampling_seed", test: "link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field", bound: Bound::InClaim },
    BindingRow { fact: "repeat_penalty, frequency_penalty, presence_penalty, repeat_last_n, logit_bias, stop", enters: "request.decode (DecodeConfigV4, canonical) and request.stop_texts -> job.decode", test: "link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field", bound: Bound::InClaim },
    BindingRow { fact: "network, class, executor bond, executor key, operator", enters: "gateway identity -> request -> job", test: "link_1_identity_chain_anchor_and_privacy_enter_the_request", bound: Bound::InClaim },
    BindingRow { fact: "anchor block / daa", enters: "chain facts at admission -> request -> job", test: "link_1_identity_chain_anchor_and_privacy_enter_the_request", bound: Bound::InClaim },
    BindingRow { fact: "privacy mode", enters: "gateway config -> request -> job.privacy_mode", test: "link_1_identity_chain_anchor_and_privacy_enter_the_request", bound: Bound::InClaim },
    BindingRow { fact: "job_nonce", enters: "fresh per request -> job.job_nonce (uniqueness only; no lottery meaning)", test: "link_1_the_nonce_is_fresh_per_request_and_nothing_else_is_random", bound: Bound::InClaim },
    BindingRow { fact: "model name, user, metadata, store, stream, stream_options, identity-valued sampling knobs", enters: "NOT in the claim: presentation or delivery only (reported in misaka.ignored_fields)", test: "link_1_presentation_fields_are_not_bound_and_say_so", bound: Bound::NotBound },
    // ---- link 2: request -> job (the worker is never trusted about what it was asked) ----
    BindingRow { fact: "every request field", enters: "validate_against_request refuses a result whose job differs from the request, field by field", test: "link_2_a_result_that_changes_any_requested_field_is_refused", bound: Bound::CheckedNotCommitted },
    BindingRow { fact: "tokenizer_id", enters: "job.tokenizer_id (worker-supplied, in the claim) — the gateway now holds it to the worker's manifest", test: "link_2_the_jobs_tokenizer_is_the_manifests", bound: Bound::CheckedNotCommitted },
    BindingRow { fact: "stop strings (spelled by the worker into job.decode.stop_sequences)", enters: "the worker's spelling; validate_against_request only caps the count, the gateway now requires at least one", test: "link_2_a_worker_that_drops_the_requested_stop_strings_is_caught_by_the_gateway", bound: Bound::CheckedNotCommitted },
    BindingRow { fact: "prompt ids <-> job.prompt_token_ids_hash", enters: "validate_against_request re-hashes the returned ids under the network's form", test: "link_2_a_result_that_changes_any_requested_field_is_refused", bound: Bound::CheckedNotCommitted },
    // ---- link 3: job -> claim id ----
    BindingRow { fact: "every FP job field", enters: "fp_job_id_v3 hashes the whole canonical job; the claim id hashes the commitment, which contains the job", test: "link_3_every_job_field_changes_the_job_id_and_the_claim_id", bound: Bound::InClaim },
    BindingRow { fact: "every DecodeConfigV4 field", enters: "inside the V4 job's borsh", test: "link_3_every_decode_config_field_changes_the_job_id", bound: Bound::InClaim },
    // ---- link 4: result -> commitment ----
    BindingRow { fact: "trace, output, schedule, execution roots; executed count; stop reason; work leaves; DA manifest root and chunk count", enters: "commitment fields; the sign gate refuses a commitment that differs from the result", test: "link_4_every_commitment_field_changes_the_claim_id_and_the_sign_gate_refuses_a_forged_one", bound: Bound::InClaim },
    BindingRow { fact: "trace_retention_daa", enters: "commitment field (a chain-time promise; the gate checks only that it is not already expired)", test: "link_4_every_commitment_field_changes_the_claim_id_and_the_sign_gate_refuses_a_forged_one", bound: Bound::InClaim },
    BindingRow { fact: "output token ids", enters: "output_root and execution_root (they hash the ids under the job context)", test: "link_4_the_executed_roots_follow_the_prompt_and_repeat_for_the_same_job", bound: Bound::InClaim },
    BindingRow { fact: "rendered answer text (what the user reads)", enters: "NOT in the claim: it is the tokenizer's rendering of the committed ids; the receipt carries its digest", test: "link_4_the_rendering_is_not_a_claim_field_the_ids_are", bound: Bound::NotBound },
    // ---- the generative path ----
    BindingRow { fact: "FP Job V5 images / source", enters: "PalwFpV5TailV1 inside the V5 job's borsh -> fp_job_id_v5", test: "gen_a_v5_job_binds_each_image_reference_and_the_source", bound: Bound::InClaim },
    BindingRow { fact: "tensor job envelope, seed (R), image / embedding body", enters: "PalwGenJobV1 borsh -> gen_job_id_v1", test: "gen_every_tensor_job_field_changes_the_job_id", bound: Bound::InClaim },
    BindingRow { fact: "pipeline stages and edges, state transitions, generated ids / output digest", enters: "stage roots -> step root -> execution root (with the job id, class id, leaf count)", test: "gen_the_execution_roots_bind_every_stage_the_count_the_job_and_the_output", bound: Bound::InClaim },
    // ---- the vision-language path (RFC-0003 §II.4) ----
    BindingRow { fact: "raw picture bytes (before preprocessing)", enters: "NOT in the claim (resampling is outside consensus): PreprocessRecordV1.source_digest in the VLM receipt — a byte the sampler never reads is in neither the pixels nor the job", test: "changing_the_picture_the_fit_or_the_prompt_moves_the_job_the_stage_roots_and_the_execution_root", bound: Bound::NotBound },
    BindingRow { fact: "canonical pixels (slot size, tile length)", enters: "images[k].input_root inside the V5 job -> fp_job_id_v5 -> the claim id; the vision stage's root reads them", test: "a_picture_of_another_size_is_preprocessed_run_through_both_stages_and_judged_valid", bound: Bound::InClaim },
    BindingRow { fact: "preprocessing algorithm, fit, pad, placement", enters: "the PreprocessRecordV1 digest sealed in the VLM receipt; pixels produced another way are refused by the worker and the seat", test: "the_receipt_binds_the_preprocessing_the_job_and_the_output_and_refuses_each_forgery", bound: Bound::CheckedNotCommitted },
    // ---- the evidence ----
    BindingRow { fact: "evidence / DA root", enters: "EvidenceManifestV1 header must equal the claim's roots, chunk count and retention", test: "evidence_a_manifest_that_differs_from_the_claim_in_any_root_is_refused", bound: Bound::CheckedNotCommitted },
];

/// The audit table as the record document's Markdown (`cargo test -p misaka-palw-gateway print_the_audit_table -- --nocapture --ignored`).
#[allow(dead_code)]
pub fn audit_markdown() -> String {
    let mut out = String::from("| fact | where it enters | bound | proving test |\n|---|---|---|---|\n");
    for row in AUDIT {
        out.push_str(&format!("| {} | {} | {:?} | `{}` |\n", row.fact, row.enters, row.bound, row.test));
    }
    out
}

/// **The worker's result against the worker's own manifest** — the one link `validate_against_request` cannot make, because the
/// request carries no tokenizer. The job's `tokenizer_id` is inside the claim but NO consensus rule compares it to anything (the
/// registration carries no tokenizer identity, module doc of `palw_freeprompt_v3`), so a worker that stamped another tokenizer would
/// produce a perfectly well-formed claim whose ids were read under a tokenizer the class does not name. The gateway holds the
/// result to the manifest the worker announced at boot before it builds a commitment on it.
pub fn check_result_against_manifest(result: &PalwFpWorkerResultV3, manifest: &PalwFpWorkerManifestV1) -> Result<(), String> {
    let job = &result.job;
    if job.tokenizer_id != manifest.tokenizer_id {
        return Err(format!(
            "the worker's result names tokenizer {} where its own manifest names {}: the prompt ids were not read under the class's tokenizer",
            job.tokenizer_id, manifest.tokenizer_id
        ));
    }
    if job.class_id != manifest.class_id {
        return Err("the worker's result is for a different class than its own manifest".to_string());
    }
    if job.max_context_tokens > manifest.n_ctx {
        return Err(format!(
            "the worker's result claims a context of {} where its manifest registers {}",
            job.max_context_tokens, manifest.n_ctx
        ));
    }
    Ok(())
}

/// **The stop strings a request asked for must have been spelled into the job.** `validate_against_request` lets the worker ADD
/// stop sequences (it spells each requested string with the class's tokenizer, which the gateway does not hold) and checks only
/// that it added no more than it was asked to — so a worker that silently DROPPED every spelled stop sequence returns a result that
/// binds its request, with a job under which the user's `stop` does nothing, while the response tells the user it applied. The
/// strings cannot be re-spelled here; what can be checked is the floor of the count.
pub fn check_stop_texts_were_spelled(request: &PalwFpWorkerRequestV3, result: &PalwFpWorkerResultV3) -> Result<(), String> {
    if request.stop_texts.is_empty() {
        return Ok(());
    }
    let asked = request.decode.as_ref().map_or(0, |d| d.stop_sequences.len());
    let have = result.job.decode.as_ref().map_or(0, |d| d.stop_sequences.len());
    // At least one spelled sequence beyond the ones the request already carried (two strings may spell one sequence, and a string may
    // spell one the request already named, so the floor is one new sequence only when none of the strings coincided with an old one —
    // the gateway sends no ids of its own, so `asked` is zero and the floor is exactly one).
    if have < asked.max(1) {
        return Err(format!(
            "the request asked for {} stop string(s) and the worker's job carries {have} stop sequence(s): the strings were not spelled into the job",
            request.stop_texts.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_CANONICAL, PALW_FP_V3_VERSION, PALW_FP_V4_VERSION,
        PalwFpStopReasonV3, PalwFpWorkerRequestV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3, fp_claim_id_v3, fp_job_id_v3,
        fp_worker_request_hash_v3,
    };
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_pq_validator_core::palw_fp_sign_gate::{FpSignGateError, signable_claim_id};

    use super::*;
    use crate::testkit::{FloorWorker, certified_facts, config, identity, temp_dir};
    use crate::{JobRunner, PreparedJob, chain, serving, surface};

    const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::MerkleV1;

    fn worker() -> (FloorWorker, std::path::PathBuf) {
        let dir = temp_dir("binding");
        (FloorWorker::new(&dir.join("traces"), FORM), dir)
    }

    fn body(content: &str) -> serde_json::Value {
        serde_json::json!({ "messages": [{ "role": "user", "content": content }], "max_tokens": 4 })
    }

    /// The gateway's own `prepare_request` for `body` under `facts`/`cfg`/`id`.
    fn prepare(
        cfg: &crate::Config,
        id: &crate::Identity,
        w: &FloorWorker,
        facts: &chain::ChainFacts,
        body: &serde_json::Value,
    ) -> Result<PreparedJob, String> {
        let (_, admitted, _) = surface::parse_and_admit_with(&serde_json::to_vec(body).unwrap(), facts, |c| {
            cfg.sidecar.as_ref().map(|s| s.apply_defaults(c, facts))
        })?;
        crate::prepare_request(cfg, id, JobRunner::manifest(w), facts, &admitted, admitted.sampling)
    }

    /// The names of the request fields that differ (the nonce always does, and is reported by its own test).
    fn request_diff(a: &PalwFpWorkerRequestV3, b: &PalwFpWorkerRequestV3) -> BTreeSet<&'static str> {
        // Destructured WITHOUT `..`: a new request field is a compile error here until it has a name in this diff.
        let PalwFpWorkerRequestV3 {
            version, network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce,
            decode_token_limit, max_context_tokens, privacy_mode, prompt_mode, sampling_seed, temperature_q, input, model_profile_id,
            runtime_manifest_hash, runtime_class_id, shape_profile_id, trace_scheme_id, decode, stop_texts, constraint,
        } = a;
        let mut changed = BTreeSet::new();
        macro_rules! cmp {
            ($($f:ident),* $(,)?) => { $( if *$f != b.$f { changed.insert(stringify!($f)); } )* };
        }
        cmp!(
            version, network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce,
            decode_token_limit, max_context_tokens, privacy_mode, prompt_mode, sampling_seed, temperature_q, input, model_profile_id,
            runtime_manifest_hash, runtime_class_id, shape_profile_id, trace_scheme_id, decode, stop_texts, constraint,
        );
        changed
    }

    fn changed(a: &PreparedJob, b: &PreparedJob) -> BTreeSet<&'static str> {
        let mut set = request_diff(&a.request, &b.request);
        set.remove("job_nonce");
        set
    }

    fn set(names: &[&'static str]) -> BTreeSet<&'static str> {
        names.iter().copied().collect()
    }

    // ------------------------------------------------------------------------------------------------------------------
    // Link 1 — gateway input -> request
    // ------------------------------------------------------------------------------------------------------------------

    #[test]
    fn link_1_the_users_text_and_every_turn_around_it_reach_the_prompt_hash() {
        let (w, dir) = worker();
        let (mut cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        cfg.max_prompt_bytes = 4096; // the tool list below is far larger than the floor's passthrough prompt
        let base = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        // The user's text: one changed letter is another prompt.
        let other = prepare(&cfg, &id, &w, &facts, &body("ho")).unwrap();
        assert_eq!(changed(&base, &other), set(&["input"]), "the user's text is the request's input, and only that");
        assert_ne!(base.plan.segments, other.plan.segments);
        // A system turn, an assistant turn, tools, tool_choice and a response_format each render into the turns BEFORE the template:
        // they change the ids and nothing else.
        let with = |extra: serde_json::Value| {
            let mut b = body("hi");
            for (k, v) in extra.as_object().unwrap() {
                b[k] = v.clone();
            }
            prepare(&cfg, &id, &w, &facts, &b)
        };
        let mut sys = body("hi");
        sys["messages"] = serde_json::json!([{ "role": "system", "content": "s" }, { "role": "user", "content": "hi" }]);
        assert_eq!(changed(&base, &prepare(&cfg, &id, &w, &facts, &sys).unwrap()), set(&["input"]), "a system turn");
        let mut asst = body("hi");
        asst["messages"] = serde_json::json!([{ "role": "user", "content": "hi" }, { "role": "assistant", "content": "a" }, { "role": "user", "content": "hi" }]);
        assert_eq!(changed(&base, &prepare(&cfg, &id, &w, &facts, &asst).unwrap()), set(&["input"]), "an assistant turn");
        let tools = with(serde_json::json!({ "tools": [{ "type": "function", "function": { "name": "f", "parameters": {"type": "object"} } }] })).unwrap();
        assert_eq!(changed(&base, &tools), set(&["input"]), "a tool list is text in the system turn");
        let format = with(serde_json::json!({ "response_format": { "type": "json_object" } })).unwrap();
        assert_eq!(changed(&base, &format), set(&["input"]), "an advisory response_format is an instruction in the system turn");
        // The same request twice is the same request (the gateway is deterministic apart from the nonce).
        let again = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        assert_eq!(changed(&base, &again), BTreeSet::new());
        assert_ne!(base.request.job_nonce, again.request.job_nonce);
    }

    #[test]
    fn link_1_the_chat_template_changes_the_ids_and_its_id_is_not_committed() {
        let (w, dir) = worker();
        let (mut cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let base = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        // The same request under a template that wraps the user's turn: other segments, so other ids.
        let sidecar = cfg.sidecar.as_mut().unwrap();
        sidecar.template.as_mut().unwrap().user.suffix = vec![misaka_palw_base0::sidecar::TemplatePieceV1::Text { text: "!".into() }];
        sidecar.template_ids = Some(("test/other/v1", "test/other-tools/v1"));
        let wrapped = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        assert_eq!(changed(&base, &wrapped), set(&["input"]));
        // FINDING (recorded in the audit table): the template's id is NOT in the request, the job or the commitment — only the ids it
        // produced are. A third party holding the ids and the tokenizer can read what the model was shown; it cannot tell which
        // template made it. The receipt carries the template id and a digest of the canonical request for exactly this reason.
        assert_ne!(base.plan.template_id, wrapped.plan.template_id);
        let diff = request_diff(&base.request, &wrapped.request);
        assert!(!diff.iter().any(|f| f.contains("template")), "no request field carries a template id");
    }

    #[test]
    fn link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field() {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(true));
        let base = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        assert_eq!(base.request.version, PALW_FP_V4_VERSION, "past the decode-rules fence every job is V4 (the no-op config when nothing was asked)");
        let with = |k: &str, v: serde_json::Value| {
            let mut b = body("hi");
            b[k] = v;
            prepare(&cfg, &id, &w, &facts, &b).unwrap()
        };
        let seeded = |extra: &[(&str, serde_json::Value)]| {
            let mut b = body("hi");
            b["temperature"] = serde_json::json!(0.5);
            b["seed"] = serde_json::json!("11".repeat(32));
            for (k, v) in extra {
                b[*k] = v.clone();
            }
            prepare(&cfg, &id, &w, &facts, &b).unwrap()
        };
        assert_eq!(changed(&base, &with("max_tokens", serde_json::json!(5))), set(&["decode_token_limit"]));
        let sampled = seeded(&[]);
        assert_eq!(changed(&base, &sampled), set(&["temperature_q", "sampling_seed"]), "temperature and seed are the sampler's inputs (R for text)");
        assert_eq!(changed(&sampled, &seeded(&[("temperature", serde_json::json!(0.75))])), set(&["temperature_q"]));
        assert_eq!(changed(&sampled, &seeded(&[("seed", serde_json::json!("22".repeat(32)))])), set(&["sampling_seed"]));
        // The five decode-config families each land in `decode` (and the stop STRINGS in `stop_texts` for the class's tokenizer).
        for (what, k, v) in [
            ("repeat_penalty", "repeat_penalty", serde_json::json!(1.5)),
            ("frequency_penalty", "frequency_penalty", serde_json::json!(0.5)),
            ("presence_penalty", "presence_penalty", serde_json::json!(0.5)),
            ("logit_bias", "logit_bias", serde_json::json!({ "104": 5.0 })),
        ] {
            let got = changed(&base, &with(k, v));
            assert_eq!(got, set(&["decode"]), "{what}");
        }
        let stopped = with("stop", serde_json::json!(["x"]));
        assert_eq!(changed(&base, &stopped), set(&["stop_texts"]), "stop STRINGS travel as text: the class's tokenizer spells them into the job's decode config");
        // The window is a decode field only while a penalty is active.
        let windowed = {
            let mut b = body("hi");
            b["repeat_penalty"] = serde_json::json!(1.5);
            let one = prepare(&cfg, &id, &w, &facts, &b).unwrap();
            b["repeat_last_n"] = serde_json::json!(8);
            (one, prepare(&cfg, &id, &w, &facts, &b).unwrap())
        };
        assert_eq!(changed(&windowed.0, &windowed.1), set(&["decode"]), "repeat_last_n is penalty_window");
        // Below the fence the same controls are REFUSED by name rather than silently dropped: a job carrying them would be a V3 job
        // that cannot say them.
        let dormant = certified_facts(false);
        let mut asks = body("hi");
        asks["temperature"] = serde_json::json!(0.5);
        assert!(prepare(&cfg, &id, &w, &dormant, &asks).err().expect("refused").contains("palw_fp_decode_rules"));
    }

    #[test]
    fn link_1_identity_chain_anchor_and_privacy_enter_the_request() {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let base = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        let r = &base.request;
        assert_eq!((r.network_domain, r.class_id, r.executor_bond, &r.executor_pubkey, r.operator_id), (id.network_domain, id.class_id, id.executor_bond, &id.executor_pubkey, id.operator_id));
        assert_eq!((r.anchor_block, r.anchor_daa, r.privacy_mode, r.prompt_mode), (facts.anchor_block, facts.anchor_daa, PALW_FP_PRIVACY_PUBLIC_DA, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER));
        let m = JobRunner::manifest(&w);
        assert_eq!((r.model_profile_id, r.runtime_manifest_hash, r.runtime_class_id, r.shape_profile_id, r.trace_scheme_id, r.max_context_tokens), (m.model_profile_id, m.runtime_manifest_hash, m.runtime_class_id, m.shape_profile_id, m.trace_scheme_id, m.n_ctx));
        // Each one moves its own field.
        let mut id2 = identity(&w.profile);
        id2.network_domain = Hash64::from_u64_word(1);
        assert_eq!(changed(&base, &prepare(&cfg, &id2, &w, &facts, &body("hi")).unwrap()), set(&["network_domain"]));
        let mut id2 = identity(&w.profile);
        id2.executor_bond = TransactionOutpoint::new(TransactionId::from_u64_word(2), 7);
        assert_eq!(changed(&base, &prepare(&cfg, &id2, &w, &facts, &body("hi")).unwrap()), set(&["executor_bond"]));
        let mut id2 = identity(&w.profile);
        id2.executor_pubkey = vec![1; 32];
        assert_eq!(changed(&base, &prepare(&cfg, &id2, &w, &facts, &body("hi")).unwrap()), set(&["executor_pubkey"]));
        let mut id2 = identity(&w.profile);
        id2.operator_id = Hash64::from_u64_word(3);
        assert_eq!(changed(&base, &prepare(&cfg, &id2, &w, &facts, &body("hi")).unwrap()), set(&["operator_id"]));
        let mut id2 = identity(&w.profile);
        id2.class_id = Hash64::from_u64_word(4);
        assert_eq!(changed(&base, &prepare(&cfg, &id2, &w, &facts, &body("hi")).unwrap()), set(&["class_id"]));
        let mut f2 = certified_facts(false);
        f2.anchor_block = Hash64::from_u64_word(5);
        assert_eq!(changed(&base, &prepare(&cfg, &id, &w, &f2, &body("hi")).unwrap()), set(&["anchor_block"]));
        let mut f2 = certified_facts(false);
        f2.anchor_daa += 1;
        assert_eq!(changed(&base, &prepare(&cfg, &id, &w, &f2, &body("hi")).unwrap()), set(&["anchor_daa"]));
        // Privacy mode is the operator's, and mode 2 is refused where the chain has not armed it (before any inference).
        let mut c2 = config(&dir);
        c2.privacy_mode = PALW_FP_PRIVACY_PANEL_DA;
        let mut armed = certified_facts(false);
        armed.panel_da_armed = true;
        assert_eq!(changed(&base, &prepare(&c2, &id, &w, &armed, &body("hi")).unwrap()), set(&["privacy_mode"]));
        assert!(prepare(&c2, &id, &w, &facts, &body("hi")).err().expect("refused").contains("palw_panel_da"));
    }

    #[test]
    fn link_1_the_nonce_is_fresh_per_request_and_nothing_else_is_random() {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let a = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        let b = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        assert_ne!(a.request.job_nonce, b.request.job_nonce, "uniqueness: a fresh nonce per request");
        assert_eq!(request_diff(&a.request, &b.request), set(&["job_nonce"]), "and it is the ONLY field that differs between two identical requests");
        // Two requests are two claims (the nonce is in the job id); the claim id of ONE request is a function of that job alone.
        assert_ne!(a.request.job_nonce, [0u8; 32]);
    }

    #[test]
    fn link_1_presentation_fields_are_not_bound_and_say_so() {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let base = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        for (k, v) in [
            ("model", serde_json::json!("whatever-the-client-calls-it")),
            ("user", serde_json::json!("alice")),
            ("metadata", serde_json::json!({ "trace": "x" })),
            ("store", serde_json::json!(true)),
            ("stream", serde_json::json!(true)),
            ("stream_options", serde_json::json!({ "include_usage": true })),
            ("top_p", serde_json::json!(1.0)),
        ] {
            let mut b = body("hi");
            b[k] = v;
            let got = prepare(&cfg, &id, &w, &facts, &b).unwrap_or_else(|e| panic!("{k}: {e}"));
            assert_eq!(changed(&base, &got), BTreeSet::new(), "{k} is presentation or delivery: it changes nothing the claim contains");
        }
        // And a knob that asks for something this lane has no rule for is refused, never silently dropped.
        let mut b = body("hi");
        b["top_p"] = serde_json::json!(0.5);
        assert!(prepare(&cfg, &id, &w, &facts, &b).is_err());
    }

    // ------------------------------------------------------------------------------------------------------------------
    // Link 2 — request -> job
    // ------------------------------------------------------------------------------------------------------------------

    /// A real request/result pair: V3 (rules dormant) or V4 (armed, with a sampled and controlled decode).
    fn run_pair(v4: bool) -> (FloorWorker, std::path::PathBuf, PalwFpWorkerRequestV3, PalwFpWorkerResultV3) {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(v4));
        let mut b = body("hi");
        if v4 {
            b["temperature"] = serde_json::json!(0.5);
            b["seed"] = serde_json::json!("11".repeat(32));
            b["repeat_penalty"] = serde_json::json!(1.5);
            b["stop"] = serde_json::json!(["x"]);
        }
        let prepared = prepare(&cfg, &id, &w, &facts, &b).unwrap();
        let result = w.run(&prepared.request, FORM, &mut |_, _| {}, &|| false).expect("the floor runs it");
        (w, dir, prepared.request, result)
    }

    fn hash_of(request: &PalwFpWorkerRequestV3) -> Hash64 {
        fp_worker_request_hash_v3(&borsh::to_vec(request).unwrap())
    }

    #[test]
    fn link_2_a_result_that_changes_any_requested_field_is_refused() {
        for v4 in [false, true] {
            let (_w, _dir, request, result) = run_pair(v4);
            let h = hash_of(&request);
            result.validate_against_request(&request, h, FORM).expect("the honest result binds its request");
            // Every field of the returned job that the request fixed, mutated: refused by name.
            let mutations: Vec<(&str, Box<dyn Fn(&mut PalwFpWorkerResultV3)>)> = vec![
                ("request_hash", Box::new(|r| r.request_hash = Hash64::from_u64_word(9))),
                ("job.network_domain", Box::new(|r| r.job.network_domain = Hash64::from_u64_word(9))),
                ("job.class_id", Box::new(|r| r.job.class_id = Hash64::from_u64_word(9))),
                ("job.executor_bond", Box::new(|r| r.job.executor_bond = TransactionOutpoint::new(TransactionId::from_u64_word(9), 1))),
                ("job.executor_pubkey", Box::new(|r| r.job.executor_pubkey = vec![9; 32])),
                ("job.operator_id", Box::new(|r| r.job.operator_id = Hash64::from_u64_word(9))),
                ("job.anchor_block", Box::new(|r| r.job.anchor_block = Hash64::from_u64_word(9))),
                ("job.anchor_daa", Box::new(|r| r.job.anchor_daa += 1)),
                ("job.job_nonce", Box::new(|r| r.job.job_nonce[0] ^= 1)),
                ("job.decode_token_limit", Box::new(|r| r.job.decode_token_limit += 1)),
                ("job.max_context_tokens", Box::new(|r| r.job.max_context_tokens -= 1)),
                ("job.privacy_mode", Box::new(|r| r.job.privacy_mode = PALW_FP_PRIVACY_PANEL_DA)),
                ("job.prompt_mode", Box::new(|r| r.job.prompt_mode = PALW_FP_PROMPT_MODE_CANONICAL)),
                ("job.sampling_seed", Box::new(|r| r.job.sampling_seed[0] ^= 1)),
                ("job.temperature_q", Box::new(|r| r.job.temperature_q += 1)),
                ("job.prompt_token_ids_hash", Box::new(|r| r.job.prompt_token_ids_hash = Hash64::from_u64_word(9))),
                ("job.prompt_tokens", Box::new(|r| r.job.prompt_tokens += 1)),
                ("prompt_token_ids", Box::new(|r| r.prompt_token_ids[0] ^= 1)),
                ("job.version", Box::new(|r| r.job.version = PALW_FP_V3_VERSION + 100)),
                ("decode_tokens_executed", Box::new(|r| r.decode_tokens_executed = 0)),
                ("stop_reason", Box::new(|r| r.stop_reason = if r.stop_reason == PalwFpStopReasonV3::ExactBudgetReached { PalwFpStopReasonV3::EndOfGeneration } else { PalwFpStopReasonV3::ExactBudgetReached })),
                ("trace_manifest_root", Box::new(|r| r.trace_manifest_root = Hash64::default())),
                ("trace_chunk_count", Box::new(|r| r.trace_chunk_count += 1)),
                ("output_token_ids length", Box::new(|r| r.output_token_ids.push(0))),
            ];
            for (what, mutate) in mutations {
                let mut forged = result.clone();
                mutate(&mut forged);
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "v4={v4}: a result with a changed {what} must be refused");
            }
            if v4 {
                // The decode config: the request's fields verbatim; the worker may ADD spelled stop sequences and nothing else.
                let mut forged = result.clone();
                forged.job.decode.as_mut().unwrap().repeat_penalty_q += 1;
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "repeat_penalty_q");
                let mut forged = result.clone();
                forged.job.decode.as_mut().unwrap().frequency_penalty_q += 1;
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "frequency_penalty_q");
                let mut forged = result.clone();
                forged.job.decode.as_mut().unwrap().penalty_window += 1;
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "penalty_window");
                let mut forged = result.clone();
                forged.job.decode.as_mut().unwrap().logit_bias.push((1, 1));
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "logit_bias");
                // (A DROPPED spelled stop sequence is the one mutation `validate_against_request` lets through: see the next test.)
                let mut forged = result.clone();
                forged.job.decode = Some(DecodeConfigV4::NOOP);
                assert!(forged.validate_against_request(&request, h, FORM).is_err(), "a different decode config entirely");
            }
        }
    }

    #[test]
    fn link_2_a_worker_that_drops_the_requested_stop_strings_is_caught_by_the_gateway() {
        let (_w, _dir, request, result) = run_pair(true);
        assert!(!request.stop_texts.is_empty());
        check_stop_texts_were_spelled(&request, &result).expect("the honest worker spelled the string into the job");
        let mut dropped = result.clone();
        dropped.job.decode.as_mut().unwrap().stop_sequences.clear();
        // THE FINDING: the request binding accepts it (the worker may add up to len(stop_texts) sequences, and zero is "up to"),
        assert!(dropped.validate_against_request(&request, hash_of(&request), FORM).is_ok(), "the gap: the cap is an upper bound only");
        // ... under a job where the user's `stop` does nothing. The gateway's floor catches it.
        assert!(check_stop_texts_were_spelled(&request, &dropped).unwrap_err().contains("not spelled"));
        // A request with no stop strings is unaffected.
        let (_w2, _d2, plain, plain_result) = run_pair(false);
        check_stop_texts_were_spelled(&plain, &plain_result).unwrap();
    }

    #[test]
    fn link_2_the_jobs_tokenizer_is_the_manifests() {
        let (w, _dir, request, result) = run_pair(false);
        let manifest = JobRunner::manifest(&w);
        check_result_against_manifest(&result, manifest).expect("the honest result names the worker's tokenizer");
        // THE FINDING: `validate_against_request` does not look at the tokenizer — the request carries none — and the chain's
        // tokenizer rule is dormant. A result stamped with another tokenizer binds its request perfectly well...
        let mut forged = result.clone();
        forged.job.tokenizer_id = Hash64::from_u64_word(0xBAD);
        assert!(forged.validate_against_request(&request, hash_of(&request), FORM).is_ok(), "the gap this row closes: unchecked by the request binding");
        // ... and the gateway now refuses it before a commitment is built on it.
        let err = check_result_against_manifest(&forged, manifest).unwrap_err();
        assert!(err.contains("tokenizer"), "{err}");
        // Its two siblings: another class, a wider context than the class registers.
        let mut other_class = result.clone();
        other_class.job.class_id = Hash64::from_u64_word(1);
        assert!(check_result_against_manifest(&other_class, manifest).unwrap_err().contains("class"));
        let mut wide = result.clone();
        wide.job.max_context_tokens = manifest.n_ctx + 1;
        assert!(check_result_against_manifest(&wide, manifest).unwrap_err().contains("context"));
    }

    // ------------------------------------------------------------------------------------------------------------------
    // Link 3 — job -> job id -> claim id
    // ------------------------------------------------------------------------------------------------------------------

    fn commitment_of(result: &PalwFpWorkerResultV3) -> PalwFreePromptCommitmentV3 {
        result.to_commitment(result.job.anchor_daa + 500_000)
    }

    #[test]
    fn link_3_every_job_field_changes_the_job_id_and_the_claim_id() {
        for v4 in [false, true] {
            let (_w, _dir, _request, result) = run_pair(v4);
            let base = commitment_of(&result);
            // Exhaustive: a field added to the job is a compile error until it gets a mutation below.
            let PalwFreePromptJobV3 {
                version, network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce,
                tokenizer_id, prompt_token_ids_hash, prompt_tokens, decode_token_limit, max_context_tokens, privacy_mode, prompt_mode,
                sampling_seed, temperature_q, decode, tail,
            } = &base.job;
            let _ = (version, network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce, tokenizer_id, prompt_token_ids_hash, prompt_tokens, decode_token_limit, max_context_tokens, privacy_mode, prompt_mode, sampling_seed, temperature_q, decode, tail);
            let mutations: Vec<(&str, Box<dyn Fn(&mut PalwFreePromptJobV3)>)> = vec![
                ("version", Box::new(|j| j.version += 1)),
                ("network_domain", Box::new(|j| j.network_domain = Hash64::from_u64_word(9))),
                ("class_id", Box::new(|j| j.class_id = Hash64::from_u64_word(9))),
                ("executor_bond.txid", Box::new(|j| j.executor_bond = TransactionOutpoint::new(TransactionId::from_u64_word(9), j.executor_bond.index))),
                ("executor_bond.index", Box::new(|j| j.executor_bond.index += 1)),
                ("executor_pubkey", Box::new(|j| j.executor_pubkey[0] ^= 1)),
                ("operator_id", Box::new(|j| j.operator_id = Hash64::from_u64_word(9))),
                ("anchor_block", Box::new(|j| j.anchor_block = Hash64::from_u64_word(9))),
                ("anchor_daa", Box::new(|j| j.anchor_daa += 1)),
                ("job_nonce", Box::new(|j| j.job_nonce[31] ^= 1)),
                ("tokenizer_id", Box::new(|j| j.tokenizer_id = Hash64::from_u64_word(9))),
                ("prompt_token_ids_hash", Box::new(|j| j.prompt_token_ids_hash = Hash64::from_u64_word(9))),
                ("prompt_tokens", Box::new(|j| j.prompt_tokens += 1)),
                ("decode_token_limit", Box::new(|j| j.decode_token_limit += 1)),
                ("max_context_tokens", Box::new(|j| j.max_context_tokens += 1)),
                ("privacy_mode", Box::new(|j| j.privacy_mode ^= 3)),
                ("prompt_mode", Box::new(|j| j.prompt_mode ^= 1)),
                ("sampling_seed", Box::new(|j| j.sampling_seed[0] ^= 1)),
                ("temperature_q", Box::new(|j| j.temperature_q += 1)),
            ];
            for (what, mutate) in mutations {
                let mut forged = base.clone();
                mutate(&mut forged.job);
                assert_ne!(fp_job_id_v3(&base.job), fp_job_id_v3(&forged.job), "v4={v4}: {what} must be inside the job id");
                assert_ne!(fp_claim_id_v3(&base), fp_claim_id_v3(&forged), "v4={v4}: {what} must be inside the claim id");
            }
            // The V4 job's decode tail is in the id; a V3 job has none, and gaining one is another job.
            let mut with_tail = base.clone();
            with_tail.job.decode = Some(with_tail.job.decode.clone().unwrap_or(DecodeConfigV4::NOOP));
            if !v4 {
                assert_ne!(fp_job_id_v3(&base.job), fp_job_id_v3(&with_tail.job), "a V3 job that gains a decode tail is another job");
            }
        }
    }

    #[test]
    fn link_3_every_decode_config_field_changes_the_job_id() {
        let (_w, _dir, _request, result) = run_pair(true);
        let base = commitment_of(&result);
        let decode = base.job.decode.clone().expect("a V4 job");
        // Exhaustive over `DecodeConfigV4`.
        let DecodeConfigV4 { repeat_penalty_q, penalty_window, frequency_penalty_q, presence_penalty_q, logit_bias, stop_sequences } = &decode;
        let _ = (repeat_penalty_q, penalty_window, frequency_penalty_q, presence_penalty_q, logit_bias, stop_sequences);
        let mutations: Vec<(&str, Box<dyn Fn(&mut DecodeConfigV4)>)> = vec![
            ("repeat_penalty_q", Box::new(|d| d.repeat_penalty_q += 1)),
            ("penalty_window", Box::new(|d| d.penalty_window += 1)),
            ("frequency_penalty_q", Box::new(|d| d.frequency_penalty_q += 1)),
            ("presence_penalty_q", Box::new(|d| d.presence_penalty_q += 1)),
            ("logit_bias", Box::new(|d| d.logit_bias.push((900, 1)))),
            ("stop_sequences", Box::new(|d| d.stop_sequences.push(vec![999]))),
        ];
        for (what, mutate) in mutations {
            let mut forged = base.clone();
            mutate(forged.job.decode.as_mut().unwrap());
            assert_ne!(fp_job_id_v3(&base.job), fp_job_id_v3(&forged.job), "{what}");
            assert_ne!(fp_claim_id_v3(&base), fp_claim_id_v3(&forged), "{what}");
        }
        // And the canonical form is enforced: a non-canonical config is refused before it can have two spellings.
        let mut bad = decode.clone();
        bad.penalty_window = 0;
        assert!(bad.validate_canonical().is_err(), "penalties on with window 0 is not canonical");
    }

    // ------------------------------------------------------------------------------------------------------------------
    // Link 4 — result -> commitment
    // ------------------------------------------------------------------------------------------------------------------

    #[test]
    fn link_4_every_commitment_field_changes_the_claim_id_and_the_sign_gate_refuses_a_forged_one() {
        let (_w, _dir, _request, result) = run_pair(false);
        let base = commitment_of(&result);
        signable_claim_id(&base, &result).expect("the honest commitment passes the gate");
        let PalwFreePromptCommitmentV3 {
            job, trace_root, output_root, schedule_root, execution_root, decode_tokens_executed, stop_reason, work_leaves,
            trace_manifest_root, trace_chunk_count, trace_retention_daa,
        } = &base;
        let _ = (job, trace_root, output_root, schedule_root, execution_root, decode_tokens_executed, stop_reason, work_leaves, trace_manifest_root, trace_chunk_count, trace_retention_daa);
        // (field, mutation, whether the gate can compare it to the result)
        let mutations: Vec<(&str, Box<dyn Fn(&mut PalwFreePromptCommitmentV3)>, bool)> = vec![
            ("job", Box::new(|c| c.job.job_nonce[0] ^= 1), true),
            ("trace_root", Box::new(|c| c.trace_root = Hash64::from_u64_word(9)), true),
            ("output_root", Box::new(|c| c.output_root = Hash64::from_u64_word(9)), true),
            ("schedule_root", Box::new(|c| c.schedule_root = Hash64::from_u64_word(9)), true),
            ("execution_root", Box::new(|c| c.execution_root = Hash64::from_u64_word(9)), true),
            ("decode_tokens_executed", Box::new(|c| c.decode_tokens_executed += 1), true),
            ("stop_reason", Box::new(|c| c.stop_reason = PalwFpStopReasonV3::EndOfGeneration), true),
            ("work_leaves", Box::new(|c| c.work_leaves += 1), true),
            ("trace_manifest_root", Box::new(|c| c.trace_manifest_root = Hash64::from_u64_word(9)), true),
            ("trace_chunk_count", Box::new(|c| c.trace_chunk_count += 1), true),
            // The one field with no counterpart in the result: a chain-time promise.
            ("trace_retention_daa", Box::new(|c| c.trace_retention_daa += 1), false),
        ];
        for (what, mutate, comparable) in mutations {
            let mut forged = base.clone();
            mutate(&mut forged);
            if what == "stop_reason" && forged.stop_reason == base.stop_reason {
                forged.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
            }
            assert_ne!(fp_claim_id_v3(&base), fp_claim_id_v3(&forged), "{what} must be inside the claim id");
            if comparable {
                assert!(signable_claim_id(&forged, &result).is_err(), "the sign gate must refuse a commitment whose {what} is not the execution's");
            } else {
                // The gate's only check on the promise: it is not already broken at the anchor.
                assert!(signable_claim_id(&forged, &result).is_ok());
                forged.trace_retention_daa = forged.job.anchor_daa;
                assert!(matches!(signable_claim_id(&forged, &result), Err(FpSignGateError::RetentionAlreadyExpired { .. })));
            }
        }
    }

    #[test]
    fn link_4_the_executed_roots_follow_the_prompt_and_repeat_for_the_same_job() {
        let (w, dir) = worker();
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let a = prepare(&cfg, &id, &w, &facts, &body("hi")).unwrap();
        let run = |r: &PalwFpWorkerRequestV3| w.run(r, FORM, &mut |_, _| {}, &|| false).expect("runs");
        let first = run(&a.request);
        let again = run(&a.request);
        let strip = |mut r: PalwFpWorkerResultV3| {
            r.model_load_ms = 0;
            r.execute_ms = 0;
            r
        };
        assert_eq!(strip(first.clone()), strip(again), "the same job is the same roots, bit for bit");
        // Another prompt: every root that follows the execution moves. (A fresh nonce alone would move them too — the job id is in
        // the context — so the request is rebuilt with the SAME nonce.)
        let other = {
            let mut o = prepare(&cfg, &id, &w, &facts, &body("ho")).unwrap().request;
            o.job_nonce = a.request.job_nonce;
            run(&o)
        };
        assert_ne!(first.trace_root, other.trace_root);
        assert_ne!(first.output_root, other.output_root);
        assert_ne!(first.execution_root, other.execution_root);
        assert_ne!(first.schedule_root, other.schedule_root, "the schedule hangs off the job context, which holds the prompt hash");
        // The nonce alone moves them too: the job id is in the context.
        let renonced = {
            let mut o = a.request.clone();
            o.job_nonce[0] ^= 1;
            run(&o)
        };
        assert_ne!(first.execution_root, renonced.execution_root);
    }

    #[test]
    fn link_4_the_rendering_is_not_a_claim_field_the_ids_are() {
        let (_w, _dir, _request, result) = run_pair(false);
        let base = commitment_of(&result);
        // FINDING: the user reads `rendered`; the claim commits `output_token_ids` (through output_root / execution_root). Rendering is
        // the class tokenizer's function of the ids, and NO field of the commitment holds it — changing `rendered` changes no claim
        // field. What keeps it honest is the gateway's W5 check (the streamed bytes ARE the rendering of the committed ids) and the
        // receipt, which carries a digest of what was shown beside the ids.
        let mut shown_differently = result.clone();
        shown_differently.rendered = b"something else entirely".to_vec();
        assert_eq!(commitment_of(&shown_differently), base, "the rendered text is not in the commitment");
        assert!(signable_claim_id(&base, &shown_differently).is_ok(), "nor does the sign gate look at it");
    }

    // ------------------------------------------------------------------------------------------------------------------
    // The generative path
    // ------------------------------------------------------------------------------------------------------------------

    #[test]
    fn gen_a_v5_job_binds_each_image_reference_and_the_source() {
        use kaspa_consensus_core::palw_fp_job_v5::{PalwFreePromptJobV5, fp_job_id_v5};
        use kaspa_consensus_core::palw_gen_class_v1::{PalwGenImageInputRefV1, PalwGenSourceRefV1};
        let (_w, _dir, _request, result) = run_pair(true);
        let v4 = result.job.clone();
        assert_eq!(v4.version, PALW_FP_V4_VERSION);
        let image = |n: u8| PalwGenImageInputRefV1 { input_root: Hash64::from_u64_word(u64::from(n)), h: 4, w: 4 };
        let base = PalwFreePromptJobV5 { v4: v4.clone(), images: vec![image(1), image(2)], source: Some(PalwGenSourceRefV1 { token_ids_hash: Hash64::from_u64_word(7), tokens: 3 }) };
        let id = fp_job_id_v5(&base);
        let mutations: Vec<(&str, Box<dyn Fn(&mut PalwFreePromptJobV5)>)> = vec![
            ("images[0].input_root", Box::new(|j| j.images[0].input_root = Hash64::from_u64_word(99))),
            ("images[0].h", Box::new(|j| j.images[0].h += 1)),
            ("images[1].w", Box::new(|j| j.images[1].w += 1)),
            ("image order", Box::new(|j| j.images.swap(0, 1))),
            ("an image added", Box::new(|j| j.images.push(image(3)))),
            ("an image dropped", Box::new(|j| {
                j.images.pop();
            })),
            ("source.token_ids_hash", Box::new(|j| j.source.as_mut().unwrap().token_ids_hash = Hash64::from_u64_word(8))),
            ("source.tokens", Box::new(|j| j.source.as_mut().unwrap().tokens += 1)),
            ("source dropped", Box::new(|j| j.source = None)),
            ("embedded V4: prompt hash", Box::new(|j| j.v4.prompt_token_ids_hash = Hash64::from_u64_word(5))),
            ("embedded V4: decode config", Box::new(|j| j.v4.decode.as_mut().unwrap().repeat_penalty_q += 1)),
            ("embedded V4: seed", Box::new(|j| j.v4.sampling_seed[0] ^= 1)),
            ("embedded V4: class", Box::new(|j| j.v4.class_id = Hash64::from_u64_word(5))),
        ];
        for (what, mutate) in mutations {
            let mut forged = base.clone();
            mutate(&mut forged);
            assert_ne!(fp_job_id_v5(&forged), id, "{what} must be inside the V5 job id");
        }
        // A V5 job is named under its own domain: it is never confusable with the V4 job it embeds.
        assert_ne!(id, fp_job_id_v3(&v4));
    }

    fn tensor_job() -> kaspa_consensus_core::palw_gen_job_v1::PalwGenJobV1 {
        use kaspa_consensus_core::palw_gen_job_v1::*;
        PalwGenJobV1 {
            version: PALW_GEN_JOB_VERSION_V1,
            envelope: PalwJobEnvelopeV1 {
                network_domain: Hash64::from_u64_word(1),
                class_id: Hash64::from_u64_word(2),
                executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(3), 0),
                executor_pubkey: vec![4; 8],
                operator_id: Hash64::from_u64_word(5),
                anchor_block: Hash64::from_u64_word(6),
                anchor_daa: 7,
                job_nonce: [8; 32],
                privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
                prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            },
            seed: [9; 32],
            body: PalwGenBodyV1::Image(PalwGenImageBodyV1 {
                prompt_token_ids_hash: Hash64::from_u64_word(10),
                prompt_tokens: 3,
                negative_token_ids_hash: Hash64::from_u64_word(11),
                negative_tokens: 2,
                guidance_q: 16,
                image_index: 0,
                sampler_id: Hash64::from_u64_word(12),
                steps: 4,
                width: 16,
                height: 16,
                output: 1,
            }),
        }
    }

    #[test]
    fn gen_every_tensor_job_field_changes_the_job_id() {
        use kaspa_consensus_core::palw_gen_class_v1::PalwGenImageInputRefV1;
        use kaspa_consensus_core::palw_gen_job_v1::*;
        let base = tensor_job();
        // The envelope, exhaustively.
        let PalwJobEnvelopeV1 { network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce, privacy_mode, prompt_mode } = &base.envelope;
        let _ = (network_domain, class_id, executor_bond, executor_pubkey, operator_id, anchor_block, anchor_daa, job_nonce, privacy_mode, prompt_mode);
        let PalwGenBodyV1::Image(image) = &base.body else { unreachable!() };
        let PalwGenImageBodyV1 { prompt_token_ids_hash, prompt_tokens, negative_token_ids_hash, negative_tokens, guidance_q, image_index, sampler_id, steps, width, height, output } = image;
        let _ = (prompt_token_ids_hash, prompt_tokens, negative_token_ids_hash, negative_tokens, guidance_q, image_index, sampler_id, steps, width, height, output);
        let mut mutations: Vec<(&str, Box<dyn Fn(&mut PalwGenJobV1)>)> = vec![
            ("version", Box::new(|j| j.version += 1)),
            ("seed (R)", Box::new(|j| j.seed[0] ^= 1)),
            ("envelope.network_domain", Box::new(|j| j.envelope.network_domain = Hash64::from_u64_word(99))),
            ("envelope.class_id", Box::new(|j| j.envelope.class_id = Hash64::from_u64_word(99))),
            ("envelope.executor_bond", Box::new(|j| j.envelope.executor_bond.index += 1)),
            ("envelope.executor_pubkey", Box::new(|j| j.envelope.executor_pubkey[0] ^= 1)),
            ("envelope.operator_id", Box::new(|j| j.envelope.operator_id = Hash64::from_u64_word(99))),
            ("envelope.anchor_block", Box::new(|j| j.envelope.anchor_block = Hash64::from_u64_word(99))),
            ("envelope.anchor_daa", Box::new(|j| j.envelope.anchor_daa += 1)),
            ("envelope.job_nonce", Box::new(|j| j.envelope.job_nonce[0] ^= 1)),
            ("envelope.privacy_mode", Box::new(|j| j.envelope.privacy_mode = PALW_FP_PRIVACY_PUBLIC_DA)),
            ("envelope.prompt_mode", Box::new(|j| j.envelope.prompt_mode = PALW_FP_PROMPT_MODE_CANONICAL)),
        ];
        macro_rules! body_field {
            ($($name:literal => |$b:ident| $edit:expr),* $(,)?) => {
                $( mutations.push(($name, Box::new(|j| { if let PalwGenBodyV1::Image($b) = &mut j.body { $edit } }))); )*
            };
        }
        body_field! {
            "image.prompt_token_ids_hash" => |b| b.prompt_token_ids_hash = Hash64::from_u64_word(99),
            "image.prompt_tokens" => |b| b.prompt_tokens += 1,
            "image.negative_token_ids_hash" => |b| b.negative_token_ids_hash = Hash64::from_u64_word(99),
            "image.negative_tokens" => |b| b.negative_tokens += 1,
            "image.guidance_q" => |b| b.guidance_q += 1,
            "image.image_index" => |b| b.image_index += 1,
            "image.sampler_id" => |b| b.sampler_id = Hash64::from_u64_word(99),
            "image.steps" => |b| b.steps += 1,
            "image.width" => |b| b.width += 1,
            "image.height" => |b| b.height += 1,
            "image.output" => |b| b.output += 1,
        }
        for (what, mutate) in mutations {
            let mut forged = base.clone();
            mutate(&mut forged);
            assert_ne!(forged.id(), base.id(), "{what} must be inside gen_job_id_v1");
        }
        // The embedding body: its input (text ids or one image reference), pooling, width and output kind.
        let embedding = |input: PalwGenEmbeddingInputV1| {
            let mut j = base.clone();
            j.body = PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 { input, pooling: 1, dims: 4, output: 3 });
            j
        };
        let text = embedding(PalwGenEmbeddingInputV1::Text { token_ids_hash: Hash64::from_u64_word(1), tokens: 3 });
        let image_ref = |n: u64| embedding(PalwGenEmbeddingInputV1::Image(PalwGenImageInputRefV1 { input_root: Hash64::from_u64_word(n), h: 4, w: 4 }));
        assert_ne!(text.id(), image_ref(1).id());
        assert_ne!(image_ref(1).id(), image_ref(2).id(), "the image's input_root is the image");
        let mut pooled = image_ref(1);
        if let PalwGenBodyV1::Embedding(b) = &mut pooled.body {
            b.pooling = 2;
        }
        assert_ne!(pooled.id(), image_ref(1).id(), "the pooling");
        let mut wide = image_ref(1);
        if let PalwGenBodyV1::Embedding(b) = &mut wide.body {
            b.dims = 8;
        }
        assert_ne!(wide.id(), image_ref(1).id(), "the output width");
        // And the encoding is canonical: a job has exactly one byte string.
        assert_eq!(PalwGenJobV1::decode_canonical(&base.encode()).unwrap(), base);
    }

    #[test]
    fn gen_the_execution_roots_bind_every_stage_the_count_the_job_and_the_output() {
        use kaspa_consensus_core::palw_fp_job_v5::PalwFreePromptJobV5;
        use kaspa_consensus_core::palw_gen_close_v1::{PalwGenStepBindingV1, PalwGenTensorBindingV1};
        use kaspa_consensus_core::palw_gen_worker_v1::PalwGenClaimRootsV1;
        let roots = PalwGenClaimRootsV1 {
            step_root: Hash64::default(),
            stage_roots: vec![Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3)],
            generated: vec![5, 6, 7],
            output_root: None,
        };
        // ---- the tensor claim ----
        let tensor = PalwGenTensorBindingV1::of(&tensor_job(), &roots, 100, Hash64::from_u64_word(50));
        assert_eq!(tensor.execution_root(), tensor.committed_execution_root);
        let PalwGenTensorBindingV1 { version, job, stage_roots, step_leaf_count, output_root, committed_execution_root } = &tensor;
        let _ = (version, job, stage_roots, step_leaf_count, output_root, committed_execution_root);
        let tensor_mutations: Vec<(&str, Box<dyn Fn(&mut PalwGenTensorBindingV1)>)> = vec![
            ("job", Box::new(|b| b.job.seed[0] ^= 1)),
            ("class", Box::new(|b| b.job.envelope.class_id = Hash64::from_u64_word(77))),
            ("a stage root (stage 0)", Box::new(|b| b.stage_roots[0] = Hash64::from_u64_word(77))),
            ("a stage root (the last stage)", Box::new(|b| b.stage_roots[2] = Hash64::from_u64_word(77))),
            ("a stage dropped", Box::new(|b| {
                b.stage_roots.pop();
            })),
            ("stage order", Box::new(|b| b.stage_roots.swap(0, 1))),
            ("the leaf count", Box::new(|b| b.step_leaf_count += 1)),
            ("the output digest", Box::new(|b| b.output_root = Hash64::from_u64_word(77))),
        ];
        for (what, mutate) in tensor_mutations {
            let mut forged = tensor.clone();
            mutate(&mut forged);
            assert_ne!(forged.execution_root(), tensor.committed_execution_root, "tensor: {what} must change the execution root a claim committed");
        }
        // ---- the text pipeline (FP Job V5) claim ----
        let (_w, _dir, _request, result) = run_pair(true);
        let v5 = PalwFreePromptJobV5 {
            v4: result.job.clone(),
            images: vec![kaspa_consensus_core::palw_gen_class_v1::PalwGenImageInputRefV1 { input_root: Hash64::from_u64_word(1), h: 4, w: 4 }],
            source: None,
        };
        let text = PalwGenStepBindingV1::of(&v5, &roots, 100);
        assert_eq!(text.execution_root(), text.committed_execution_root);
        let text_mutations: Vec<(&str, Box<dyn Fn(&mut PalwGenStepBindingV1)>)> = vec![
            ("the job (an image reference)", Box::new(|b| b.job.images[0].input_root = Hash64::from_u64_word(77))),
            ("the job (the prompt)", Box::new(|b| b.job.v4.prompt_token_ids_hash = Hash64::from_u64_word(77))),
            ("a stage root", Box::new(|b| b.stage_roots[1] = Hash64::from_u64_word(77))),
            ("the leaf count", Box::new(|b| b.step_leaf_count += 1)),
            ("a generated id", Box::new(|b| b.generated[0] ^= 1)),
            ("generated ids appended", Box::new(|b| b.generated.push(0))),
            ("generated ids dropped", Box::new(|b| {
                b.generated.pop();
            })),
        ];
        for (what, mutate) in text_mutations {
            let mut forged = text.clone();
            mutate(&mut forged);
            assert_ne!(forged.execution_root(), text.committed_execution_root, "text pipeline: {what} must change the execution root");
        }
    }

    // ------------------------------------------------------------------------------------------------------------------
    // The evidence
    // ------------------------------------------------------------------------------------------------------------------

    #[test]
    fn evidence_a_manifest_that_differs_from_the_claim_in_any_root_is_refused() {
        use kaspa_consensus_core::palw_evidence_v1::{ClaimRoots, EvidenceManifestV1, ManifestLimits};
        let (_w, _dir, _request, result) = run_pair(false);
        let c = commitment_of(&result);
        let chunks = vec![vec![1u8; 64], vec![2u8; 32]];
        let manifest = EvidenceManifestV1::build(c.job.network_domain, &c.job.executor_bond, &c.job.job_nonce, c.trace_root, c.output_root, c.execution_root, c.trace_chunk_count, c.trace_retention_daa, &chunks);
        manifest.validate_shape(&ManifestLimits::default()).expect("shape");
        let claim = ClaimRoots {
            network_domain: c.job.network_domain,
            trace_root: c.trace_root,
            output_root: c.output_root,
            execution_root: c.execution_root,
            trace_chunk_count: c.trace_chunk_count,
            retention_deadline: c.trace_retention_daa,
        };
        manifest.verify_claim_binding(&claim).expect("the honest manifest agrees with its claim");
        let ClaimRoots { network_domain, trace_root, output_root, execution_root, trace_chunk_count, retention_deadline } = claim;
        let _ = (network_domain, trace_root, output_root, execution_root, trace_chunk_count, retention_deadline);
        let forged: Vec<(&str, ClaimRoots)> = vec![
            ("network_domain", ClaimRoots { network_domain: Hash64::from_u64_word(9), ..claim }),
            ("trace_root", ClaimRoots { trace_root: Hash64::from_u64_word(9), ..claim }),
            ("output_root", ClaimRoots { output_root: Hash64::from_u64_word(9), ..claim }),
            ("execution_root", ClaimRoots { execution_root: Hash64::from_u64_word(9), ..claim }),
            ("trace_chunk_count", ClaimRoots { trace_chunk_count: claim.trace_chunk_count + 1, ..claim }),
            ("retention (the claim promises longer than the manifest)", ClaimRoots { retention_deadline: claim.retention_deadline + 1, ..claim }),
        ];
        for (what, other) in forged {
            assert!(manifest.verify_claim_binding(&other).is_err(), "a claim with a different {what} must not accept this manifest");
        }
        // A served chunk is bound by hash: another byte is refused.
        manifest.verify_chunk(0, &chunks[0]).unwrap();
        let mut bad = chunks[0].clone();
        bad[0] ^= 1;
        assert!(manifest.verify_chunk(0, &bad).is_err());
        assert!(manifest.verify_chunk(1, &chunks[0]).is_err(), "and a chunk is bound to its index");
    }

    // ------------------------------------------------------------------------------------------------------------------
    // The table itself
    // ------------------------------------------------------------------------------------------------------------------

    #[test]
    #[ignore = "prints the record document's table"]
    fn print_the_audit_table() {
        println!("{}", audit_markdown());
    }

    #[test]
    fn the_audit_table_names_only_tests_that_exist() {
        let source = std::include_str!("binding.rs");
        // Rows may be proven in a sibling module's tests (the vision-language path lives in `vlm`).
        let siblings = [source, std::include_str!("vlm.rs"), std::include_str!("preprocess.rs")];
        for row in AUDIT {
            assert!(
                siblings.iter().any(|s| s.contains(&format!("fn {}(", row.test))),
                "the audit row `{}` names a test that does not exist: {}",
                row.fact,
                row.test
            );
        }
        // Every link-numbered test is in the table (a test the table forgot is a binding nobody reads).
        let in_table: BTreeSet<&str> = AUDIT.iter().map(|r| r.test).collect();
        for line in source.lines() {
            if let Some(name) = line.trim().strip_prefix("fn ").and_then(|l| l.split('(').next())
                && (name.starts_with("link_") || name.starts_with("gen_") || name.starts_with("evidence_"))
            {
                assert!(in_table.contains(name), "test {name} proves a binding the audit table does not list");
            }
        }
        // The findings are rows, not footnotes: the not-bound ones are named.
        let not_bound: Vec<_> = AUDIT.iter().filter(|r| r.bound == Bound::NotBound).collect();
        assert_eq!(not_bound.len(), 3, "presentation fields, the rendered text and the raw picture are what a claim does not contain");
        let _ = serving::CANCELLED_BY_CLIENT;
    }
}
