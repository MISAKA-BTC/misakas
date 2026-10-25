# RFC-0012 candidate market handler parity — 2026-10-25

Status: focused and related runtime controls PASS at the immutable retry2 checkpoint.

`trace_candidate_tx` accepted an `EthCallEnv` containing the exact PALW view and market fences, but registered the precompile seam with `None` market handlers. A market registry call could therefore appear to execute successfully against an empty account with empty output, while `eth_call` against the same state returned its ABI data. This was an existing candidate-trace defect; no accepted market fill is established by fixing it.

The simulator and candidate tracer now call one `market_handlers_for_env_v2` setup function. It preserves the simulator's existing behavior: reject simultaneous legacy and retained views even below the fence; register no market doors when dormant; select the raw legacy handler for a raw view; use the existing retained handler constructor and propagate its typed local capacity refusal for an owned view. Candidate setup precedes snapshot reconstruction, signed transaction decoding and revm execution. Handler clones in the precompile callback retain the same view and handler setup leases. No consensus executor or accepted-transaction replay path is changed.

Three dedicated parent controls in `trace::candidate_market_tests_v2` were executed:

- Real signed candidate calls to chainDaa/classCount/lineCount run through revm and the actual registry precompile. Their output matches `simulate_call` and legacy-view candidate tracing, retains the call frame output, and has the actual 32-byte ABI shape. Dormant fences produce the existing empty-account behavior.
- Ambiguous views refuse before an invalid raw transaction can be decoded, under active and dormant fences.
- An isolated child arms a real native capacity limit after constructing the fixture view. With all remaining quota held, retained handler setup returns NativeResource before invalid-transaction decoding. After release, the same candidate path executes, and all setup capacity is available again. This checks the handler setup lifetime; the input fixture view was created before quota arming. Existing retained view/handler lifetime tests separately cover charged input ownership. The child is part of its parent count.

These controls use locally constructed genesis views and signed simulated EVM calls. They do not establish an accepted market fill, bridge transfer, selected-verifier approval, native39 maturity or formal RFC12 settlement mapping, K3 execution, or any real campaign gate. Accepted-transaction trace replay still has a separate historical market-context contract to implement. Whole-trace memory, snapshot allocation, and complete model costs are outside this bounded setup change. Public activation, bootstrap refusals, economic values and frozen model tiers are unchanged.

## Verified checkpoint

The coordinated five-package no-run build passed in 276.60 seconds (Cargo summary 4m36s), with all 29 recorded Rust/config identities matching before binary capture. Source record: `/tmp/misaka-rfc-material-authority-wave-source-retry2-20261025/record.json`; build log: `/tmp/misaka-rfc-material-authority-wave-build-retry2-20261025.log`.

EVM immutable binary: `/tmp/misaka-rfc-material-authority-kaspa_evm-retry2-20261025`, SHA-256 `e258ddb8d9f840e8a807400fde6415d75871fef0d5e9d8645d079fff41940ba5`. Candidate market filter **3/3 PASS**, 0.04 seconds harness / 0.61 seconds controller wall; the isolated child is inclusive. Exact log: `/tmp/misaka-rfc-material-authority-candidate-20261025.log`; controller: `/tmp/misaka-rfc-material-authority-focused-results-20261025.json`.


The same immutable EVM binary passed existing trace14/14 (0.05s), simulator1/1 (0.03s), and retained-market4/4 (0.01s). Logs are `/tmp/misaka-rfc-material-authority-trace-regression-20261025.log`, `/tmp/misaka-rfc-material-authority-sim-regression-20261025.log`, and `/tmp/misaka-rfc-material-authority-retained-market-regression-20261025.log`; controller `/tmp/misaka-rfc-material-authority-regression-results-20261025.json`. These retain accepted replay reconciliation, activation/failure controls and the existing retained setup quota/last-callback lifetime tests. They do not add accepted market execution to the new candidate fixture's scope.

The coordinated build first failed in the separate material lane: an incorrect contract-level grant accessor (E0599, initial 39.83s), then test observations consuming two reservation builders twice (E0382, retry1 241.03s). Those logs/source records remain under the initial and retry1 wave paths. The minimal fixes precede the successful retry2; they are not failed candidate runtime results. No source was modified during a running build. The focused21 parents and related15 groups across all lanes are local software controls, not the fifteen real RFC-0012 campaign gates.
