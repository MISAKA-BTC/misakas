# BCT2 plain Output consumer regression — 2026-11-11

Source `a7b678d79dbd7f2f4a6ace0fe92e33cd1a2fe3e5`; Node compile fix `9f0bd5a74`.
[Machine-readable record](summary.json) pins binaries and unmodified raw logs.

8 tests passed: SDK signing (3), funded UTXO transaction (1), CLI quote checks (1), RPC selector (1), rooted tokenizer/clock (1), funded Batch context (1). The consumer Cargo invocation selected no CLI/RPC tests because their names differ; `cli.log` and `rpc.log` record the explicit follow-up executions of those same compiled binaries. Zero-test integration targets are not counted.

Core tests used a 64 MiB worker stack and opt-level 0. The consumer build command used `CARGO_TARGET_DIR=/tmp/misaka-public-contract-test.fd7edK CARGO_INCREMENTAL=0 cargo --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' test --locked -p kaspa-pq-validator-core -p misaka-cli -p kaspa-rpc-service -p misaka-palw-sdk batch_plain_output -j2 -- --nocapture`. Core filters were `rooted_tokenizer_stop_material_pins_original_ids_rendering_and_eog` and `g14_native_funded_batch_candidate_v1`; the latter ran the same compiled core test executable. CLI/RPC filters were `plain_output_quote_requires_exact_subject_digest_and_live_slot` and `numerical_batch_open_selector_is_bounded_and_exclusive`.

`cargo check --locked -p kaspa-consensus --lib -j2` initially found a Node-only unresolved `serde_json` macro. The fix uses the existing JSON value conversion without adding a dependency; the repeated Node check passed under the same core profile. Both raw results are retained.

This verifies local request preparation, signature/funding integrity, parsing and existing core regressions. It does not exercise the complete BCT2 public carrier path or certify all-branch capacity, public activation, TFP1 acceptance, Final or receipt production.
