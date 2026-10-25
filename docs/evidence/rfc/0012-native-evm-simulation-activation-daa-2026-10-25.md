# RFC-0012: simulation requires actual L1 activation metadata

Status: coordinated node no-run compilation and all three focused tests passed.

The actual node's head and historical simulation environment builders previously read the selected executed L1 header's DAA with `get_header(...).map(...).unwrap_or(0)`. A returned header-read error therefore became successful DAA zero and selected F003's handler registration with invented activation metadata. With a finite positive fence it could disable an already active handler; with fence zero it could continue active despite unavailable metadata. Public fences are unchanged by this correction.

Both `head_snapshot_and_env` and `snapshot_and_env_at` now enter the same private `with_executed_l1_daa` continuation. It reads exactly the previously resolved executed L1 identity and reports a server-unavailable error before environment construction when the API fails. It never substitutes the sink/latest block or zero. Successful headers retain the existing `daa_score >= f003_mldsa_verify_activation_daa_score` comparison, including genuine DAA zero. Actual consumers are `eth_call`, `estimate_gas` and head-based `trace_evm_candidate`. Other environment fields and historical market-view limitations retain their prior semantics.

Three tests in `kaspad/src/eth_rpc_activation_daa_tests.rs` drive the production continuation with a typed `ConsensusApi` reader: exact L1 identity and zero downstream calls on missing/read-error headers; genuine zero and fence zero/MAX/below/at/above boundaries; and real `kaspa_evm::sim::simulate_call` F003 registration. The latter uses malformed non-value-bearing calldata: the actual active F003 handler returns a 32-byte ABI false, whereas its inactive empty account returns no bytes. No signature, accepted work or model input is manufactured. Package `kaspad`, feature `evm`, filter `eth_rpc::activation_daa_tests`.

## Limits

These are native-provider metadata/API-boundary and real local simulator controls, not native EVM chain activation, historical-signature replay or a campaign. The underlying native `Consensus::get_header` still contains its status-independent header-store `.optional().unwrap()`; failures that panic internally are not repaired here. No public policy, activation height, K3 input or certification authority changes. UNKNOWN acceptance gates remain UNKNOWN.

## Observed verification

The coordinated node EVM/private no-run build passed in **5m41s**, logged in `/tmp/misaka-rfc-predecessor-activation-node-build-20261025.log`. Its immutable binary is `/tmp/misaka-rfc-predecessor-activation-node-20261025`, SHA-256 `790d8234099697d71c348f31aa494de728fac862f1f90e635c3d3c6bfc2f09c5`. All 14 captured referenced Rust source hashes remained unchanged. Filter `eth_rpc::activation_daa_tests` passed **3/3 in 0.01s**, logged in `/tmp/misaka-rfc-activation-daa-node-tests-20261025.log`. This records the metadata boundary and real local F003 handler controls only; it does not classify unrelated fixture results as passing or establish an actual native EVM campaign.
