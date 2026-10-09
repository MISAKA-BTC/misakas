# RFC-0001 inference surface / RFC-0003 generative classes — delivery record (workstream C3)

Branch `rfc1/c3-inference` (base `717064b16`). Owner: Agent C3. Scope: request/job construction, task-specific input serialization,
gateway ↔ worker execution, authenticated output carriage, streaming/status API, resume/error behaviour, and the gateway → commitment → submit
handoff. Not touched: frozen FP Job V4 semantics (penalty arithmetic and order, logit bias, stop rules, the Gumbel-max sampler, token accounting,
canonical normalization, the decode vectors), consensus rules/params/fingerprints, the dormant fences (`palw_fp_decode_rules`, `palw_gen_v1`,
`palw_fp_job_v5` — armed only in test-owned copies), `misaka-palw-challenge` (read-only; never an input to a generation seed R),
`misaka-palw-remote`, the rail's relay/redeem paths and `kaspad`.

Statuses (the matrix's): IMPLEMENTED_AND_TESTED · IMPLEMENTED_REFERENCE_ONLY · DORMANT_NOT_INTEGRATED · CODE_GAP · DESIGN_GAP ·
EXTERNAL_GATE_PENDING. A fixture PASS, an in-process worker or an RFC text is never production completeness; nothing here was run against a real
node, a real model weight or the t12 network. **No RFC is complete.**

## 1. Cases

| # | case | path | test (all in `misaka-palw-gateway/src`) | status |
|---|---|---|---|---|
| 1 | Chat request → committed claim (FP V3, the live lane) | `serve_connection` → admission → `prepare_request` → worker → bindings → outbox | `e2e::a_chat_request_becomes_a_queued_commitment_and_says_it_is_not_final` | IMPLEMENTED_AND_TESTED, in process, BASE-0 floor worker (`testkit::FloorWorker`); real family worker / node run: not done |
| 2 | …into a consensus fold (claim visible; receipt checked against the node's row; seat replay; licence) | gateway → fp-submit `plan_submission`/`execute_handoff` → extraction walk → `apply_palw_transition_v7` | `e2e_chain::a_v3_gateway_request_becomes_a_claim_the_fold_holds_…` | IMPLEMENTED_AND_TESTED in process on testnet-12's shipped params (l5 fixtures: fixture bond key, accepting broadcast, signature = the door's other test) |
| 3 | FP Job V4: gateway normalization → fence → fold, decode-only credit (D10), seat replay | same, fence test-armed at DAA 110 | `e2e_chain::a_v4_gateway_request_is_carried_only_past_the_test_armed_fence_…` | gateway side IMPLEMENTED_AND_TESTED; the fence itself DORMANT_NOT_INTEGRATED; gates G8–G10 (salted flag-day drill, release, activation) EXTERNAL_GATE_PENDING |
| 4 | Idempotency (`Idempotency-Key` + RFC 8785 request digest): same key+request → same claim, no second inference/commitment/charge; different request → 409; failed/cancelled request frees the key; survives restart; key never on disk | `idempotency` | `idempotency::*`, `e2e::a_retry_with_the_same_key_replays_…`, `…the_same_key_for_a_different_request_is_refused…`, `…a_streaming_retry_of_a_finished_request_is_replayed_as_a_stream…` | IMPLEMENTED_AND_TESTED |
| 5 | Cancellation on client disconnect: queued request never runs; mid-run request is drained and discarded — no commitment, no outbox file, no charge, trace removed, counters released, key freed | `serving::ClientLink`, `handle_chat` gates | `e2e::a_client_that_disconnects_mid_run_is_never_committed…`, `…a_streaming_client_that_hangs_up…`, `…a_queued_request_whose_client_left_is_not_run_at_all` | IMPLEMENTED_AND_TESTED; aborting a run already on the worker: closed by P1 (SMALL lane, §6) |
| 6 | `Retry-After` on every 503/429 (the queue-full 503 documented one and wrote none) | `serving::render_head` | `serving::a_503_and_a_429_always_carry_a_retry_after…`, `serving::the_connection_cap_503_reaches_the_client…`, `e2e::a_full_queue_answers_503_with_a_retry_after…` | fixed; IMPLEMENTED_AND_TESTED |
| 7 | Bounded queue as a CAS reservation released by drop | `serving::QueueGate` | `serving::the_queue_never_holds_more_than_its_cap…`, `…many_threads_never_push…` | IMPLEMENTED_AND_TESTED |
| 8 | Status `streaming → answered/committed → submitted → final/voided` (+ cancelled, misattributed); streamed chunks say `streaming`, `final:false`; `final`/`voided` only from a settled chain row via the rail's `ClaimTracker`, labelled `UNVERIFIED_REMOTE_STATE`; `GET /v1/requests/<id>` | `status`, `chain::observation_from_claim_reply` | `status::*`, `chain::a_claim_reply_is_folded_…`, `e2e::every_streamed_chunk_says_streaming…`, `e2e::the_status_route_reports_committed_then_submitted…` | mapping IMPLEMENTED_AND_TESTED; the wRPC call (`RpcChainSource::observe_claim`) is compiled, not run against a node (EXTERNAL_GATE_PENDING) |
| 9 | Producer offline → public evidence: material placed with dir/HTTP providers, every copy read back, BEFORE the commitment is queued; too few copies = answered, not committed; `PanelDa` never published | `evidence` over `misaka-palw-remote::transport` | `evidence::evidence_placed_with_a_directory_and_an_http_provider_outlives_the_producer`, `…a_provider_that_serves_other_bytes…`, `…the_gateway_publishes_before_it_commits…`, `…a_panel_da_*` | IMPLEMENTED_AND_TESTED with the reference HTTP provider; provider discovery stays DESIGN_GAP (config only); retention repair loop is C1's `RetentionMonitor` |
| 10 | Output receipt (claim id, output root, class id, decode-rule digest, bond, tokenizer, output-ids digest, shown-text digest, request digest, template id, job-context hash; txid added outside the seal); `GET /v1/receipts/<id>`; `verify_receipt` / `check_against_chain` | `receipt` | `receipt::*`, `e2e::the_receipt_route_serves_the_sealed_receipt…` | IMPLEMENTED_AND_TESTED; no gateway key (ADR-0079): authenticated by the chain, not signed |
| 11 | Binding audit (§2): a mutation test per field over four links + V5 + tensor job + execution roots + evidence | `binding` | `binding::*` | IMPLEMENTED_AND_TESTED; two gateway gaps found and fixed (§3) |
| 12 | VLM: deterministic integer preprocessing (`misaka.palw.image-preprocess.v1`, stretch/letterbox, bilinear Q16) with golden vectors from an independent Python implementation | `preprocess`, `tests/vectors/` | `preprocess::*` | IMPLEMENTED_AND_TESTED |
| 13 | VLM: preprocessing → V5 job (`input_root`) → vision stage → text stage → generated ids → commitment → seat Valid; `VlmReceiptV1` binds the preprocessing record | `vlm` | `vlm::*` (toy VLM class, in process) | DORMANT_NOT_INTEGRATED: `palw_fp_job_v5` unarmed everywhere, the fold does not open version-8 claims, the binary has no generative worker route, the gateway has no class tokenizer |
| 14 | Image generation: request → canonical `PalwGenJobV1` per image (held to the class by the chain's own acceptance) → `run_tensor` → canonical output + `output_root` → version-10 payload; seat Valid; R = requester's seed or a pure function of the request | `imagegen` | `imagegen::*` (SDK's tiny SD3 class, in process; skips if the fixture is not generated) | DORMANT_NOT_INTEGRATED; `POST /v1/images/generations` answers 501 by name (CODE_GAP: no generative worker process, no class tokenizer, no `--gen-class` configuration) |
| 15 | `/v1/embeddings` | local forward pass, no claim (unchanged) | existing | IMPLEMENTED_AND_TESTED as local serving; the claim form (RFC-0003 §II.3) is the tensor path of row 14's library, not this route |
| 16 | Audio / video understanding and generation | spec only | — | DESIGN_GAP |

## 2. Binding audit

Every fact a claim must pin → where it enters → the test that proves it. `ViaCanonicalForm` = bound only through the canonical field it is
turned into; `CheckedNotCommitted` = verified before a commitment exists, not itself a claim field; `NotBound` = **not in the claim**, with what
carries it instead. Generated from `binding::AUDIT` (`cargo test -p misaka-palw-gateway print_the_audit_table -- --ignored --nocapture`); a test
holds the table to the tests that exist, and the four-link tests destructure the job, request, decode config and commitment WITHOUT `..`, so
adding a field to any of them is a compile error until it has a row and a mutation.

| fact | where it enters | bound | proving test |
|---|---|---|---|
| user message text | chat template -> segments -> ids -> job.prompt_token_ids_hash | ViaCanonicalForm | `link_1_the_users_text_and_every_turn_around_it_reach_the_prompt_hash` |
| system / assistant turns, tools, tool_choice, response_format instruction | rendered into the turns before the template -> ids | ViaCanonicalForm | `link_1_the_users_text_and_every_turn_around_it_reach_the_prompt_hash` |
| chat template (built-in, sidecar) | the segments the template emits -> ids; its id is NOT a claim field | ViaCanonicalForm | `link_1_the_chat_template_changes_the_ids_and_its_id_is_not_committed` |
| max_tokens | request.decode_token_limit -> job.decode_token_limit | InClaim | `link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field` |
| temperature, seed (generation seed R for text) | request.temperature_q / sampling_seed -> job.temperature_q / sampling_seed | InClaim | `link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field` |
| repeat_penalty, frequency_penalty, presence_penalty, repeat_last_n, logit_bias, stop | request.decode (DecodeConfigV4, canonical) and request.stop_texts -> job.decode | InClaim | `link_1_the_sampling_and_decode_controls_each_reach_their_own_request_field` |
| network, class, executor bond, executor key, operator | gateway identity -> request -> job | InClaim | `link_1_identity_chain_anchor_and_privacy_enter_the_request` |
| anchor block / daa | chain facts at admission -> request -> job | InClaim | `link_1_identity_chain_anchor_and_privacy_enter_the_request` |
| privacy mode | gateway config -> request -> job.privacy_mode | InClaim | `link_1_identity_chain_anchor_and_privacy_enter_the_request` |
| job_nonce | fresh per request -> job.job_nonce (uniqueness only; no lottery meaning) | InClaim | `link_1_the_nonce_is_fresh_per_request_and_nothing_else_is_random` |
| model name, user, metadata, store, stream, stream_options, identity-valued sampling knobs | NOT in the claim: presentation or delivery only (reported in misaka.ignored_fields) | NotBound | `link_1_presentation_fields_are_not_bound_and_say_so` |
| every request field | validate_against_request refuses a result whose job differs from the request, field by field | CheckedNotCommitted | `link_2_a_result_that_changes_any_requested_field_is_refused` |
| tokenizer_id | job.tokenizer_id (worker-supplied, in the claim) — the gateway now holds it to the worker's manifest | CheckedNotCommitted | `link_2_the_jobs_tokenizer_is_the_manifests` |
| stop strings (spelled by the worker into job.decode.stop_sequences) | the worker's spelling; validate_against_request only caps the count, the gateway now requires at least one | CheckedNotCommitted | `link_2_a_worker_that_drops_the_requested_stop_strings_is_caught_by_the_gateway` |
| prompt ids <-> job.prompt_token_ids_hash | validate_against_request re-hashes the returned ids under the network's form | CheckedNotCommitted | `link_2_a_result_that_changes_any_requested_field_is_refused` |
| every FP job field | fp_job_id_v3 hashes the whole canonical job; the claim id hashes the commitment, which contains the job | InClaim | `link_3_every_job_field_changes_the_job_id_and_the_claim_id` |
| every DecodeConfigV4 field | inside the V4 job's borsh | InClaim | `link_3_every_decode_config_field_changes_the_job_id` |
| trace, output, schedule, execution roots; executed count; stop reason; work leaves; DA manifest root and chunk count | commitment fields; the sign gate refuses a commitment that differs from the result | InClaim | `link_4_every_commitment_field_changes_the_claim_id_and_the_sign_gate_refuses_a_forged_one` |
| trace_retention_daa | commitment field (a chain-time promise; the gate checks only that it is not already expired) | InClaim | `link_4_every_commitment_field_changes_the_claim_id_and_the_sign_gate_refuses_a_forged_one` |
| output token ids | output_root and execution_root (they hash the ids under the job context) | InClaim | `link_4_the_executed_roots_follow_the_prompt_and_repeat_for_the_same_job` |
| rendered answer text (what the user reads) | NOT in the claim: it is the tokenizer's rendering of the committed ids; the receipt carries its digest | NotBound | `link_4_the_rendering_is_not_a_claim_field_the_ids_are` |
| FP Job V5 images / source | PalwFpV5TailV1 inside the V5 job's borsh -> fp_job_id_v5 | InClaim | `gen_a_v5_job_binds_each_image_reference_and_the_source` |
| tensor job envelope, seed (R), image / embedding body | PalwGenJobV1 borsh -> gen_job_id_v1 | InClaim | `gen_every_tensor_job_field_changes_the_job_id` |
| pipeline stages and edges, state transitions, generated ids / output digest | stage roots -> step root -> execution root (with the job id, class id, leaf count) | InClaim | `gen_the_execution_roots_bind_every_stage_the_count_the_job_and_the_output` |
| raw picture bytes (before preprocessing) | NOT in the claim (resampling is outside consensus): PreprocessRecordV1.source_digest in the VLM receipt — a byte the sampler never reads is in neither the pixels nor the job | NotBound | `changing_the_picture_the_fit_or_the_prompt_moves_the_job_the_stage_roots_and_the_execution_root` |
| canonical pixels (slot size, tile length) | images[k].input_root inside the V5 job -> fp_job_id_v5 -> the claim id; the vision stage's root reads them | InClaim | `a_picture_of_another_size_is_preprocessed_run_through_both_stages_and_judged_valid` |
| preprocessing algorithm, fit, pad, placement | the PreprocessRecordV1 digest sealed in the VLM receipt; pixels produced another way are refused by the worker and the seat | CheckedNotCommitted | `the_receipt_binds_the_preprocessing_the_job_and_the_output_and_refuses_each_forgery` |
| image request: n, size, steps, guidance, prompt ids, negative ids | imagegen::plan -> PalwGenJobV1.body (image_index per image, steps, guidance_q, width/height, prompt hashes), each held to the class by palw_gen_job_resolve_class_v1 before it runs | InClaim | `what_the_class_does_not_offer_is_refused_before_anything_runs` |
| generation seed R (images) | PalwGenJobV1.seed — the requester's, or a pure function of the request digest; never the anchor, a nonce, the clock or a verification beacon | InClaim | `the_generation_seed_depends_on_the_request_alone_and_never_on_the_chain` |
| evidence / DA root | EvidenceManifestV1 header must equal the claim's roots, chunk count and retention | CheckedNotCommitted | `evidence_a_manifest_that_differs_from_the_claim_in_any_root_is_refused` |

## 3. Findings

| id | finding | severity | disposition |
|---|---|---|---|
| F1 | `validate_against_request` never compares the job's `tokenizer_id` (the request carries none; the chain's tokenizer rule is `Dormant`). A worker that stamped another tokenizer returned a result that bound its request. | gateway integrity | **fixed in the gateway** (`binding::check_result_against_manifest`, also class and context); hoisted into consensus-core by P2 (§6) |
| F2 | `validate_against_request` only CAPS the stop sequences a worker may add, so a worker that dropped the user's `stop` strings returned a result that bound its request, under a job where `stop` does nothing — while the response told the user it applied. | honesty of the response | **fixed in the gateway** (`binding::check_stop_texts_were_spelled`); hoisted into `validate_against_request` by P2 (§6) |
| F3 | The queue-full 503 documented `Retry-After` and wrote none. | serving | fixed |
| F4 | Without a key a client retry is a SECOND job: a fresh random `job_nonce` (a different claim id) for the same work, a second inference and budget charge, and a commitment the chain then refuses as `DuplicateWork` (`fp_work_id_v1 = (class, prompt hash, bond)`). | serving/economics | fixed by the idempotency key (opt-in per request) |
| F5 | The original request and the chat template id are not in any claim field; only the ids the template produced are. A third party can read what the model was shown, not which template made it. | by design | recorded; the receipt carries the request digest and template id; no consensus change proposed |
| F6 | The rendered text is not in the claim (the ids are); the user reads a tokenizer function of them. | by design | recorded; W5 (stream = rendering of committed ids) + the receipt's shown-text digest |
| F7 | A point-sampling resampler does not read every raw picture byte: an unsampled byte changes neither the canonical pixels nor the job. | by design | the preprocessing record carries the RAW picture's digest, so the receipt still tells the pictures apart |
| F8 | **`misaka-palw-fp-rail --evidence-out` has no `PanelDa` guard**: for a mode-2 claim `staged_prompt_ids` returns the worker's PRIVATE ids and they go into the material written to the evidence directory. If that directory is served by a provider anyone can GET (the reference `transport::server` serves every content-addressed path to any caller), the private prompt is disclosed. The gateway refuses `PanelDa` + public providers (at boot and per job); the rail does not. | privacy (C1's file) | **reported to C1/Lead, not edited** |
| F9 | A worker mid-run cannot be cancelled: the resident protocol has no cancel frame. A cancelled job finishes (≤ the decode cap) and is discarded; killing the worker instead would turn every dropped connection into a model re-map (an amplification attack). | resource | closed by P1 (SMALL lane, §6) |

## 4. Proposals (versioned amendments / decisions for the Lead — none changes frozen FP V4 semantics)

* **P1 — worker cancel frame.** A `Cancel` frame (or a bounded-decode-chunk protocol) in the `v3-serve` stream so a cancelled run frees its slot at the next
  token. Worker-protocol change (`misaka-palw-base0::fp_worker`, `palw_freeprompt_v3` frame enum), not consensus. Until then: drain and discard.
* **P2 — hoist F1/F2 into `validate_against_request`.** The two gateway checks belong in the consensus-core validator every caller shares (the rail's
  sign gate, a drill client). It changes no wire byte or id; it changes what a result is admitted for.
* **P3 — V5 commitment conventions.** `vlm::commitment_of` sets `output_root = 0` (RFC-0003 §I.3.3: a text pipeline's output is the ids, which the execution
  root hashes), one trace chunk, no DA manifest. The lane's retention/DA trio for a version-8 claim is unspecified (DESIGN_GAP) and must be decided before the V5 fold exists.
* **P4 — a tokenizer for generative classes.** Neither the gateway nor any generative worker tokenizes: VLM/image requests carry ids. A `Text` arm in the generative
  worker frame (as the FP worker has) is the missing piece.
* **P5 — evidence provider discovery** stays configuration (`--evidence-provider`); on-chain discovery is the existing DESIGN_GAP in `transport`.

## 5. Tests and how to run them (targeted only)

```
cargo test --offline -p misaka-palw-gateway            # 158 bin tests + rail 7 + drill 8 + dsl_da_election_gate 3, ~35 s (imagegen runs the SD3 pipeline)
cargo test --offline -p misaka-palw-gateway --bin misaka-palw-gateway e2e_chain      # the fold paths
```
Dev-dependencies added (tests only; `Cargo.lock` gains three edges): `misaka-palw-sdk`, `misaka-palw-tir-lower`, `misaka-palw-tir-artifact`.
The base0 tests (`fp_job_v4*`, `fp_job_v4_t12_e2e`) were not re-run: nothing under `misaka-palw-base0`, `consensus` or `kaspad` was edited.

## 6. SMALL lane (2026-10-09): proposals P1–P4 implemented

Nothing here changes FP Job V4 semantics or any consensus id; no fence was armed and no live testnet-12 id moved. Each item is a worker-protocol, validator
or dormant-door change, with its own tests.

**P1 — worker cancel frame (F9).** `PalwFpWorkerFrameV1::Cancelled { request_hash }` is appended (index 8; 0–7 unchanged) and the cancel request is the
frame `MPCX ‖ request_hash` (`PALW_FP_WORKER_CANCEL_MAGIC_V1`, `fp_worker_cancel_frame_v1`; no other framing can collide with it). The worker side:
`misaka-palw-base0::cancel` (a thread-local abort flag, one read in every decode loop after a token is reported — a seat, a court or a drill never sets
it), `fp_worker::ThreadedFrames` (the request stream read on its own thread so a cancel reaches a run in progress) and
`run_v3_serve_cancellable_v1` (a cancel for the request in flight stops the run at its NEXT token; exactly one `Cancelled` terminator; no result, no
retained trace, no commitment; the artifact stays resident; a cancel that arrives after the run ended is ignored; a stale cancel never names a later
request with the same bytes). The four decode loops (`base0_execute_streaming_select_capped_v1`, the A16 and Qwen3.6 streaming executors and the A16
stop-id loop of `PalwExecutionBackendV1`) check the flag. The gateway side: `worker_stream` polls `cancelled()` every `CANCEL_POLL_TOKENS` (4) tokens, writes the cancel ONCE, and
reads on to the terminator so the stream stays in step; `Cancelled` ends the run as `CANCELLED_BY_CLIENT`; a `Cancelled` nobody asked for, or naming another
request, is a broken stream and the worker is dropped. A worker build that does not know the magic refuses the frame as "not a v3 request": the gateway and
the worker ship from one tree, so the pair is not supported mixed (an older worker answers the cancel frame with a `Refused` the next request would read).
Tests: `consensus-core` `a_cancel_frame_names_one_request_and_collides_with_no_other_framing`; `base0` `a_cancel_stops_the_run_at_the_next_token_and_the_worker_serves_the_next_request_unchanged`
(the next request's result equals the same request on a fresh worker), `a_late_cancel_is_ignored_and_a_serial_source_skips_it`,
`the_threaded_source_orders_requests_consumes_cancels_and_forgets_them_per_request`; `gateway` `worker_stream::tests`.

**P2 — F1/F2 hoisted.** `PalwFpWorkerResultV3::validate_against_request` now also refuses a result whose job carries fewer stop sequences than
`max(asked, 1)` when the request carried `stop_texts` (`PalwFpV3Error::StopStringsNotSpelled`, F2), and `validate_against_manifest` /
`validate_against_request_and_manifest` hold the job's tokenizer, class and context to the manifest the same worker announced (`WorkerManifestMismatch`, F1).
No wire byte or id moves; what changes is what a result is admitted for, in the validator every caller shares (the gateway, the rail's sign gate, a drill
client). The gateway's two helpers are now thin `String` forms of them. Test: `the_hoisted_f1_and_f2_checks_are_in_the_shared_validator`.

**P3 — the version-8 commitment convention.** `palw_fp_v5_check_commitment_conventions_v1` / `palw_fp_v5_canonical_commitment_v1` /
`palw_fp_v5_validate_payload_canonical_v1`: a version-8 claim's `output_root`, `schedule_root` and `trace_manifest_root` are zero, its `trace_chunk_count` 1
and its `trace_retention_daa` 0 (the chain derives retention as for a tensor claim; a stated deadline would be a promise nothing reads), each refused by field
name (`CommitmentNotCanonical`). It is a SEPARATE door over the existing one (`palw_fp_v5_validate_payload_v1` and, with it, every FP Job V4 validator are byte
for byte what they were), reachable only with the `palw_fp_job_v5` fence in force — which is `None` on every preset, and which no node code consults yet (the
fold does not open version-8 claims) — so the convention rides the V5 fence rather than taking a fence of its own: wiring the door into the fold is that
fold's step. Test: `a_v5_commitment_follows_the_pipeline_claim_convention_through_its_own_door`.

**P4 — a `Text` arm in the generative worker's frame.** `PalwGenWorkerRequestV2` (frame = `MPGT ‖ borsh`; a V1 request, whose bytes are unchanged, never
begins with that magic) carries `PalwGenWorkerInputV1::{TokenIds, Text}`. With `Text` the job's `prompt_tokens` and `prompt_token_ids_hash` are left zero
and the WORKER — which holds the class's tokenizer through `GenPromptTokenizerV1`, refused unless its identity is the class row's — spells the prompt
(the class's forced prefix, then the encoding with control tokens NOT matched), completes the job and runs it, answering
`PalwGenWorkerAnswerV1::Completed { job, prompt_ids, binding }` (index 2, appended). The requester checks it with `gen_check_completed_job_v1` (the job is
the one it sent in every field but the two the worker fills; those two commit to the returned ids; the ids are the forced prefix and then user tokens, within
`max_prompt_tokens`). The `TokenIds` arm and the V1 frame mean what they always meant. A worker with no tokenizer refuses a `Text` request by name. What is
NOT here: a concrete tokenizer for a registered generative class, and the gateway route that sends the arm (`/v1/images/generations` still answers 501) —
both remain the lane that builds the generative worker process. Test: `a_text_request_is_spelled_by_the_worker_which_completes_the_job`.
