# Public wide replay prompt form

The saved Studio request decodes to the literal UTF-8 prompt `Hello`.
The worker returns `[19556]`. The matching raw tokenizer produces the same
sequence via the existing `QwenTokenizer::encode_without_specials`.
The native result validator accepts `MerkleV1` and rejects `Flat`. The
same local claim is reproduced with the original quote's liability clock.
Input evidence and reproduction are in
`/tmp/t12-studio-prompt-evidence-20261111.json`.

## Current claim route

The exact worker request names the native legacy-width tiled logits scheme.
The legacy replay adapter below is the same-claim priority. The wide
section remains an independent caller audit.

## Expected form source

- Native G14 quote/fold already resolves native rules in
  `consensus/core/src/palw_tir_public_fold_v1.rs:501` and retains accepted
  IDs under `native.prompt_form` at line 1266. The node constructs
  `PalwTirPublicAllocationRulesV1.prompt_form` via its rules resolver in
  `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs:3225`.
  Supply the exact chosen form to future accepted linkage/replay metadata
  from that same path. Preserve class overrides if the native caller applies
  `palw_prompt_ids_form_of_class_v1`; do not duplicate the choice in the Panel.
- `PalwPublicTirBindingHeaderV1` / `PalwJobContextV2` hold one commitment
  slot, and the TFO3 companion plus current draw context do not explicitly
  encode a native form. Cold/drawn consumers receive a job/class and cannot
  obtain an authoritative form solely from those arguments. Require an
  explicit expected form from the native resolver/accepted link owner. A
  checked manifest's codec must match it; its codec cannot choose the form.
  Never try both forms on a verification path. No change to any accepted
  job, claim, header or signature is needed for prompt form alone.
- G14 `PanelSourceSetV2` already carries a typed `self.prompt_form` set in
  its constructor at `positive_panel_sources.rs:920`; use it in its
  replay method, preserving its existing expected-source/identity checks.
  `TirMinerJobV2.prompt_form` already feeds the producer input material
  and public text checker at `positive_miner_v2_inference.rs:171,178,373`.

## Wide-only patch boundary

1. Introduce explicit-form entry points for the shared public replay,
   cold replay and drawn replay-and-sign routines
   (`misaka/palw/remote/src/positive_panel_sources.rs:237,284,458`).
   Forward the same value through all three; the G14 source set's method
   forwards its stored value at line 983. Keep old Flat entry points as
   forwarding wrappers where API compatibility is necessary. At line 497,
   use the form in existing `fetch_public_replay_inputs_v1`; native
   manifest codec, count, canonical chunk size and commitment checks already
   exist in `check_public_input_tokens_v1` (lines 761–783).
2. For the exec chain, add `capture_one_pass_with_prompt_form_v2`, with the
   old `capture_one_pass_v2` delegating with Flat so existing standalone
   callers/struct literal syntax remain compatible. Carry the form in the
   returned `WidePanelCaptureV2` so a proof-builder attempt still checks
   its independent `prep.prompt_form()` against the actual capture form.
   This is the exact check at `positive_wide_material_v2/proof.rs:99`.
   Change capture's native ID check at `capture.rs:737`. Independently
   pass it to the retained-output runner, which currently checks Flat again
   at `run_wide_q24_v2.rs:416`. Expose an internal explicit-form
   retained-output method; keep the other runner APIs Flat. Ensure that the
   single-run work identity remains the unchanged job context hash.
3. Replace the producer's Flat-only refusal at
   `positive_miner_v2_inference.rs:245` only together with both exec
   layers. Forward the existing typed `job.prompt_form`, preserve exact
   text re-encoding and signed output/trace checks. Other owned-artifact
   Flat-only forward/measurement callers are separate and can keep their
   existing behavior.
4. Budget the new form before either hash. Current
   `prompt_token_ids_root_v1` builds ordered tile hashes then a step tree.
   For n IDs and t=ceil(n/32) valid tiles, it hashes n*4+16*t
   bytes in tile preimages, 72*t in indexed leaves, up to 128*(t-1)
   in internal nodes and 72 in the final prompt root. A conservative
   separately checked scratch bound for the live vectors is
   64*(2*t+ceil(t/2)), plus the existing writer temporary envelope or a
   separately justified bound for those tiny temporaries. Verify the fixed
   domain/hasher accounting convention when converting this byte count to
   the existing work policy. The capture and runner each hash; count
   both in their own admissible owner envelopes. Make the runner quote
   form-aware before `max_requested_bytes` and handoff checks, and extend
   capture's `Charge::Hash` at line 726 and its scratch guard. Preserve
   all Flat quotes. Acquisition/tokenizer CPU/IO/RAM still need the
   separately reserved replay budget, including its prompt commitment check.
5. In the existing remote signed-draw fixture, exercise successful replay
   under each expected form with its matching original and signed manifest,
   and cross-form refusal before signer invocation. Cover a ragged
   multi-tile case at capture/runner level, a wrong digest, and a reservation
   too small for Merkle scratch. Compare roots using the existing native
   execution/capture contract within each form. Different form implies a
   different committed job context; do not expect identical roots across
   form substitutions.

This is a source-inspection plan. The offline result shows tokenizer and
commitment compatibility for the saved claim. Wide replay and native
acceptance remain separate checks. In particular, the drawn legacy wide
route currently requires a stateless wide-logits program with committed nodes
and the cold dependency material, independently of the prompt form. Verify
the actual registered Studio program and protected relation plan before using
that wide route for the ordinary TFP1 case. G14 material/DA, same-claim
paired quote/fold and a public replay or Final have not been observed here.

## Priority patch for the exact legacy tiled TFP1

The stored native request names `e0532a21…f5a1`. Existing scheme
constructors confirm that this is `tiled_logits_scheme_id_v1`. It differs
from both Flat logits and `palw_logits_q24_v2::scheme_id`. The saved worker
result authenticates the request hash. This confirms the request scheme and
the MerkleV1 prompt IDs form as separate observations. Existing G14
`original_plan_mode_v4` refuses the known wide Q24 row at line 523;
`TirClassRunnerWideQ24V2::new` requires it. A wide runner change is outside
the shortest same-claim path.

1. Obtain the typed accepted pairing/link context when it exists. The
   adapter needs exact FP `PalwFreePromptCommitmentV3`, literal
   `PalwPublicTirBindingHeaderV1`, native class/registered artifact,
   expected prompt form, exact funded case scope, and resource limits.
   Reuse the paired Source/claim/header validation. Validate the class's
   explicit legacy tiled scheme before runner construction, and require
   the companion's aggregate Trace subject and output to match native
   roots. Avoid resolving job selection from companion-only metadata.
2. Factor the format-neutral parts of
   `remote/src/positive_panel_sources.rs::run_cold_public_wide_replay_v1`:
   signed companion and manifest checks, program acquisition, cold
   initial/dependencies when the native context proves cold, and
   authenticated artifact inventory. `fetch_public_wide_artifact_v1`
   at line 355 is already parameter-width-neutral: it checks manifest,
   exact program/class/inventory, Borsh bounds, canonical operands and
   inventory root before opening the assembled artifact. Factor/reuse
   it with its legacy caller preserved, and independently enforce the
   accepted container binding if the native profile needs the literal
   container. Existing `PanelSourceSetV2::assemble_artifact` is the
   alternative when the same authenticated G14 inventory is already held.
   `fetch_public_replay_inputs_v1(..., expected_form)` checks exact
   prompt UTF-8, tokenizer/raw ID, token count, canonical manifest codec
   and native ID commitment. For TFO3 additionally compare manifest
   prompt bytes with the accepted carrier's literal UTF-8 (or reuse the
   same validated hash/opening). `fetch_public_replay_output_v1` already
   uses the ordinary native output commitment. Keep each acquisition
   within booked and locally reserved bounds.
3. Reuse the actual existing legacy deterministic runner through
   `PalwExecutionBackendV1::tir_own_step_source_need_v1` and
   `open_tir_own_step_source_v1`. The production G14 owner invokes them at
   `kaspad/src/palw_tir_public_owner.rs:885–897`, with a memory ledger
   and blocking replay. The backend already checks the exact header,
   FP job ID/class/tokenizer/prompt pins, decode count and the expected
   form (`tir/exec/src/node/backend.rs:290`). It reuses the native
   FP selector (`:318`) with the original job's V4 or V3 rule and
   does not default to attempt-greedy. `open_tir_own_step_source_v1`
   (`:1943`) calls `OwnStepSource::build`
   (`tir/exec/src/node/own_source.rs:305`), which runs
   `TirClassRunnerV1::stream_v1` once and recomputes aggregate execution,
   legacy tiled logits root, step root/count, output and generated IDs
   using the exact header context. Its disk tree exposes bounded step/row
   queries and its output roots come from the actual replay. Hold both
   measured RAM and the quoted temporary disk owner during this call.
   `replay_tir_generated_ids_v1` (`backend.rs:1896`) is an existing
   narrower full-row path, used by the fraud filer, that needlessly
   materializes rows when the already-streamed source is available.
4. Match the complete replay result and accepted context. Compare exact
   IDs to authenticated output material, output root, aggregate execution,
   logits root and step count/root to the native header/FP, and apply
   the original stop semantics already used by G14
   `owner_suspicion_v1` (`kaspad/src/palw_tir_public_owner.rs:137`).
   If retained FP trace-manifest identity is part of the protected
   relation, derive it from the same streamed row roots with the
   original native `fp_trace_manifest_v3` algorithm. The current own
   source exposes the authenticated row tree, so this may need a
   separate row-root visitor/streaming manifest adapter. Keep the FP
   manifest identity distinct from the public Trace material entry.
   Compare/authenticate the public Trace index with the independently
   computed header. Factor `check_public_replay_trace_v1` so it can
   read exact bounded chunks from `own.leaf_hashes_v1` instead of
   copying a complete hash vector. The existing trace opener checks
   aggregate binding, ranges, encoded tile bounds and leaf preimage.
   Mismatch localization must compare original/own authenticated ranges
   and produce native G14 evidence; acquisition exhaustion is incomplete.
5. The public manifest consumer/signer currently hardcodes
   `stateless_wide_replay_plan_v1`. Its identity string explicitly
   commits `wide-tir/flat/all-nodes-committed/stateless/cold-v1`
   (`misaka/palw/positive-contract/src/public_one_honest.rs:18`).
   Preserve this identity for existing wide routes. Use a distinct
   adopted/explicit native TFP1 relation plan on the paired route,
   supply the same plan to draw, manifest request and full replay scope,
   and add the native legacy TFP1 ExecutionRules material codec/producer
   binding instead of calling `public_wide_tir_rules_material_v1`.
   Reuse existing drawn key/scope and delayed signature checks from
   `replay_and_sign_drawn_public_wide_v1` only through a profile-specific
   success result. G14 owner's matching replay alone is not a drawn
   approval or consensus credit.
6. First test the legacy adapter using existing independent step-source
   parity tests (`own_source.rs:531`) and real cold artifact/manifest
   fixtures. In the saved Studio case reproduce the exact stored native
   FP header/registered program, and test paired prompt, MerkleV1, wrong
   form/codec/source, authenticated trace opening, output and roots
   against the preserved saved worker claim. This offline adapter test
   does not establish positive Final or public activation/qualification.
