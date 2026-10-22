# Applied handover candidates — verification run 1 (Claude, 2026-10-22)

Applied verbatim on top of retry3-green `c1470683b`: `test-indices/family-test-indices.patch`, `preclaim-combined/combined.patch`, then `preclaim-role-catalog/preclaim-role-catalog.patch` (each applycheck0), plus one test-side precondition: the numerical test asserts the opaque cover roots are strictly ascending by leaf before its binary search. Committed as `1d5c760be` (file contents identical to the frozen hashes in verification.json).

Runner = retry3 runner with new output dir and three added core filters (`original_aggregate_draft_requires_native_variant_and_never_grants_reservation`, `g14_native_funded_candidate_fixture_generator_v1` with `MISAKA_G14_NATIVE_FIXTURE_DIR` unset so it writes nothing, and the whole `palw_tir_public_fold_v1::` module) and two added native regressions (`lg14a_legacy_filer_e2e::`, `rfc12_x14_a_reorg_across_a_deposit_claim_and_the_fence_undoes_both_ledgers_together`).

Result: 41 steps, all exit0; 181 unique `ok` test lines, 0 failures; frozen source and native fixture unchanged. The numerical join selection now finishes in 21.59 s (277 s in retry3) — a test-side wall time on this machine, not a protocol cost.

Scope: component/regression evidence only. The candidates remain preclaim authority/requirement derivations with typed-missing contracts; no funded admission, escrow, fee/capital body, activation, pre integration or T-item completion follows.
