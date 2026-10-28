# Independent review: decoded-original policy/projection extraction

Read-only against /tmp/g14-fp-policy-projection/policy-projection.patch and current palw_fp_objects_v3.rs. No repo edits/builds. No behavior-changing discrepancy found in the reviewed extraction.

## Order / refusals / callbacks

The subnet check and strict Borsh payload decode remain in the outer accepted-tx loop. Failed decode still records the same `(carrier id,"payload does not decode")` before ANY class_caps/verification callback. Accepted carrier order is unchanged. Only the decoded policy+object mapping is moved into the helper.

The moved sequence remains: first class_caps -> ruleset prompt/decode caps -> prefix stand-in/fences -> prefix-inherit class-safe -> constraint stand-in/fences -> class tokenizer -> stateless accepting rules -> Q24 decode controls -> held public-ID carrier limit -> carried-key signature -> second class_caps.derived_work -> Declared/Unpublished/TIR/legacy derived leaf check -> object fields. Each original `skipped(id,why); continue` becomes the exact same static reason returned, then the outer loop records it with the same tx id. Earliest-error precedence remains unchanged.

class_caps still runs twice on success and at a derived-work refusal, and once on earlier policy/signature refusals. The second callback still supplies ONLY derived_work; the class step_ladder used to count derives from the FIRST caps value, exactly as before. No caching/consolidation was inserted. The verifier runs once only after all earlier gates; its captured closure is borrowed, not copied or re-created. An extra reference level to payload/verifier is normal deref/Fn forwarding and introduces no policy operation or field clone.

All projected FreePromptCommitted fields retain their expressions: job_pin, cloned public_job, eval None, claim_id, class, executor bond/key, work leaves, prompt hash/count/IDs, executed decode count, trace/output/execution roots, DA chunk/retention, consumed prefix state. The outer carrier id remains tx.id. No new fields, price, fee, active-bond/current-key authority, or full-original admission is added.

## Test strength

The new `single_policy_projection_preserves_carrier_objects_rejections_and_caps_order` mostly compares the helper with an extractor that now calls that SAME helper. This is a wiring test, not independent before/after semantic evidence. Both paths could share a wrong rule or field map and still pass. The explicit success-calls==2 assertion independently pins that one callback count. Equality of early-error callback counts does not independently pin expected1, and the four cases do not assert an explicit expected result/reason per case. Current TFO four-byte input test demonstrates strict old decode refusal/callback0; it does not substitute for the actual full TFO native no-downgrade test.

If strengthening the added test, pin literal expected outcomes/callback/verifier counts for its four cases: Dormant+valid =>success/caps2/verify1; Active with V3 fixture =>stateless reason/caps1/verify0; bad signature=>signature reason/caps1/verify1; Unpublished=>published-profile reason/caps2/verify1. Independently pin a projected-object golden digest from a PRE-patch binary or use existing literal field assertions; do not compute the golden by calling the extracted helper twice.

## Existing independent regression filters

Core actual moved entry tests (old literal assertions / refusal behavior):
- `palw_fp_objects_v3::tests::the_walk_admits_one_job_version_per_side_of_the_decode_rules_fence`
- `palw_fp_objects_v3::tests::a_class_that_does_not_commit_q24_logits_is_offered_only_the_unit_free_controls`
- `palw_fp_objects_v3::tests::a_commitment_whose_signature_does_not_verify_creates_no_claim`
- `palw_fp_objects_v3::tests::the_commitment_signature_is_checked_over_the_claim_id_in_its_own_context`
- `palw_fp_objects_v3::tests::a_held_class_commitment_is_bounded_by_its_own_ladder_and_every_other_by_the_networks`
- `palw_fp_objects_v3::tests::under_the_held_regime_public_ids_past_one_transaction_are_skipped_by_name`
- `palw_fp_objects_v3::tests::an_accepted_commitment_becomes_one_object_carrying_its_leaves`
- `palw_fp_objects_v3::tests::every_skip_reason_is_named`
- `palw_fp_objects_v3::tests::only_this_subnetwork_is_read_and_order_is_preserved`
- `palw_fp_objects_v3::tests::ten_times_the_work_leaves_rides_below_the_fence_and_the_walk_refuses_it_past_it`
- `palw_fp_objects_v3::tests::the_walk_carries_the_prompt_length_and_the_prompt_itself`

Other old policy-family entry regressions:
- `palw_fp_prefix_v1::tests::the_walk_carries_the_state_past_the_fence_and_skips_the_claim_below_it`
- `palw_fp_constraint_job_v1::tests::the_walk_carries_a_constrained_claim_on_its_stand_in_and_skips_it_by_name_otherwise`
- `palw_freeprompt_v3::tests::golden_vector_ids_are_frozen` pins literal V3 job/claim/spend identifiers. It preserves underlying identity/wire behavior, but alone does not pin the moved object projection.
- existing `palw_gen_v5`/V4 wire vectors may pin lane codecs but not this ORIGINAL-specific helper; do not count them as direct extraction coverage unless their entry reaches it.

Real MLDSA/native path filters (consensus crate):
- `a_callers_prompt_on_a_registered_class_opens_a_claim_at_the_shipped_quantum` uses real MLDSA commitment, real verifier and shipped bundle extraction, then folded claim assertions.
- `palw_v2_a_derivation_rides_signed_by_the_claims_executor_and_is_dropped_otherwise` starts with a real signed original through this extraction; later negatives concern derived artefacts, so do not mislabel those as ORIGINAL signature-negative coverage.
- `palw_funded_original_real_native_inputs_preserve_fallback_and_refuse_incomplete_admission` is the current actual funded-carrier boundary/no-complete-install check, not an ordinary policy-helper success.

These tests plus an independent PRE/post exact-output comparison are stronger than the new mirror test. Keep the helper's documented scope: it does not decode an external carrier, establish current actor authority, reserve funds, supply full graph/profile or install a funded original. TFO must not be rewritten as legacy input merely to reach it.
