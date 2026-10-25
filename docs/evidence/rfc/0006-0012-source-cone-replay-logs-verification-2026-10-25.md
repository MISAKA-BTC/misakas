# Source/Cone owners and EVM logs/replay verification

Status: six-package no-run build PASS; 29 dedicated parents and 23 related regression groups PASS. The combined selection contains 186 distinct parent test functions. Isolated children are inclusive, and overlapping dedicated/regression filters are not counted twice. These are local implementation checks, not the fifteen real campaign gates.

## Reproducible build identity

The initial no-run attempt failed after 346.67 seconds with two E0425 diagnostics: the new EVM test leaf omitted its `writer_address` import. Its source/patch and log are preserved in `/tmp/misaka-rfc-replay-log-cone-wave-source-20261025/record.json` and `/tmp/misaka-rfc-replay-log-cone-wave-build-20261025.log`. No failed attempt is counted as a passing build.

Retry1 passed in 186.13 seconds (Cargo 3m05), with all 40 source/config identities unchanged from its pre-build snapshot. Its authoritative snapshot/patch is `/tmp/misaka-rfc-replay-log-cone-wave-source-retry1-20261025/record.json`; log is `/tmp/misaka-rfc-replay-log-cone-wave-build-retry1-20261025.log`. The snapshot includes every changed/untracked Rust file and the relevant manifests, lockfile and activation source. Only the missing test import changed after the initial failure.

The command was `cargo test -p kaspa-consensus-core -p kaspa-consensus -p kaspad -p misaka-palw-base0 -p kaspa-evm -p kaspa-eth-rpc --lib --features kaspa-consensus/evm,kaspad/evm,kaspad/private-integration-config --no-run --config 'profile.test.package.kaspa-consensus-core.opt-level=0'`. Target was `/tmp/misaka-rfc06-12-build-20261024`; dev/test debug=0, incremental=0, RUST_MIN_STACK=67108864 and TMPDIR=/private/tmp. Repository profiles were unchanged.

Actual Cargo-generated executables were copied before runtime or another Cargo build. The pinned record is `/tmp/misaka-rfc-replay-log-cone-pins-20261025.json`:

| Crate | Immutable executable | SHA-256 |
| --- | --- | --- |
| kaspa_consensus | `/tmp/misaka-rfc-replay-log-cone-kaspa_consensus-retry1-20261025` | `7587d6e681c2baaf26085438adbbbef58b51501b7c23cd418e6c748175d6d9db` |
| kaspa_consensus_core | `/tmp/misaka-rfc-replay-log-cone-kaspa_consensus_core-retry1-20261025` | `183bc992d49ac925319fb1bb2c4c9dc0b2df2d053cba15d2873e85fe2c221824` |
| kaspa_eth_rpc | `/tmp/misaka-rfc-replay-log-cone-kaspa_eth_rpc-retry1-20261025` | `ce6f1ffcda88b9187fca10636507ea10d880b6fcd8afde11a60661052d4d435c` |
| kaspa_evm | `/tmp/misaka-rfc-replay-log-cone-kaspa_evm-retry1-20261025` | `132ca5c463c7c69ebd964a54c723be0d30e68f02f0a121bd174686c8516534b6` |
| kaspad_lib | `/tmp/misaka-rfc-replay-log-cone-kaspad_lib-retry1-20261025` | `bb12019f6374f2ae9f6cf2fc7ad847f6af5d200ce98306c2fefe0b588b353340` |
| misaka_palw_base0 | `/tmp/misaka-rfc-replay-log-cone-misaka_palw_base0-retry1-20261025` | `efd84652fd10053009fc9b2b8f26c4eaa8ef0247d290f2c273b17f7a637c28f4` |

## Actual runtime scope

Each row ran its exact filter with `--nocapture --test-threads=1`, checked nonzero expected parent count and exit0. Times below are final parent harness durations; child duration is inclusive. The machine-readable records include commands, actual pins, logs, wall times and expected/observed counts at `/tmp/misaka-rfc-replay-log-cone-focused-results-20261025.json` and `/tmp/misaka-rfc-replay-log-cone-regression-results-20261025.json`.

| Group | Filter | Parents | Seconds | Result |
| --- | --- | ---: | ---: | --- |
| validation | `source_validation_` | 2 | 0.45 | PASS |
| writer | `source_identity_writer_borsh_layout_` | 1 | 0.12 | PASS |
| replay-core | `replay_market_` | 2 | 0.01 | PASS |
| cone-inputs | `tir_cone_inputs_owned_` | 2 | 0.14 | PASS |
| cone-derived | `tir_cone_derived_owned_` | 1 | 0.06 | PASS |
| logs-native | `evm_logs_native_` | 5 | 0.19 | PASS |
| logs-node | `exact_hash_logs_node_` | 3 | 0.01 | PASS |
| logs-dispatch | `exact_hash_logs_dispatch_` | 4 | 0.00 | PASS |
| replay-native | `replay_market_native_` | 3 | 0.62 | PASS |
| replay-node | `replay_market_rpc_` | 2 | 0.00 | PASS |
| replay-evm | `accepted_market_replay_` | 4 | 0.11 | PASS |
| source | `palw_tir_public_source_v1::tests` | 7 | 0.03 | PASS |
| continuation | `continuation_source_owned` | 3 | 1.55 | PASS |
| ids | `complete_ids_owned` | 3 | 0.18 | PASS |
| harvest | `tir_owner_harvest_` | 2 | 0.06 | PASS |
| arithmetic | `tir_arithmetic_scheduler` | 11 | 0.00 | PASS |
| source-worker | `tir_source_worker_` | 3 | 0.17 | PASS |
| cone | `tir_cone_owned_consumer_` | 4 | 0.69 | PASS |
| owner-replay | `tir_owner_replay_` | 2 | 0.00 | PASS |
| eth-rpc | `eth_rpc::` | 28 | 0.02 | PASS |
| receipt | `evm_receipt_native_` | 4 | 0.07 | PASS |
| canonicality | `native_canonicality_` | 3 | 0.03 | PASS |
| reader | `evm_native_reader_` | 3 | 0.03 | PASS |
| trace | `trace::tests` | 14 | 0.05 | PASS |
| sim | `sim::tests` | 1 | 0.04 | PASS |
| retained-market | `historical_market_retained` | 4 | 0.01 | PASS |
| public-source | `public_row_owned_source` | 1 | 0.21 | PASS |
| response | `palw_tir_public_source_v1::response_native_v2::tests` | 5 | 0.04 | PASS |
| finished | `finished_material_owned_` | 2 | 1.03 | PASS |
| view-core | `historical_evm_view_` | 4 | 0.02 | PASS |
| view-native | `historical_market_native_api_` | 4 | 0.58 | PASS |
| candidate | `candidate_market_` | 3 | 0.02 | PASS |
| eth-dispatch-all | `(whole kaspa-eth-rpc library)` | 31 | 0.27 | PASS |
| executor | `executor::tests` | 28 | 1.33 | PASS |

## Evidence boundaries

Source controls use actual signed Capture/Owner rows and actual Borsh growth. Cone controls use original-source authenticated identity material and the real planning/worker/copy helpers, not accepted model execution. Native replay controls use actual accepted BASE parent/child blocks; separately, real signed EVM calls execute against explicit hypothetical market-window/settlement inputs to reproduce accepted receipts and prefix counters. Exact-hash native log controls use actual accepted L1 branches with explicit stored EVM rows, not claims that those logs executed. See the implementation records for detailed contracts and limitations:

- [Source validation and writer requests](0006-tir-source-validation-transients-2026-10-25.md)
- [Cone inputs and derived owners](0029-cone-input-derived-native-owners-2026-10-25.md)
- [Exact block-hash log consumer](0012-exact-blockhash-log-consumer-2026-10-25.md)
- [Accepted market replay context](0012-accepted-market-replay-context-2026-10-25.md)

Public activation remains None; normal selected-source/V6 paid bootstrap refusal, unapproved economics, frozen model tiers and claim-level selected-verifier approvals are unchanged. K3 formal material is uncreated; formal native39 settlement mapping is unformulated; P2P staging readiness refusal, whole-model costs, actual bridge/fill and all fifteen real campaign gates remain open/UNKNOWN. No narrow local result promotes a gate to PASS. This record adds no deployment or public runtime measurement.
