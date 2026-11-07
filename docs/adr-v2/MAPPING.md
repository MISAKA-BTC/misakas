# 旧204 ADR → 新50 ADR 完全対応表

主担当は各旧ファイルにつき一つだけです。全行をその主担当の原条文欄へ収録し、節単位の部分撤回は新ADR-50の台帳と各文書の境界から参照します。番号だけが同じ別ファイルは統合時にも別のsource keyです。

## 旧文書から探す

| 旧原文（完全ファイル名） | 新しい主担当 | 原文行 | 状態宣言 |
| --- | --- | --- | --- |
| [0001-network-isolation.md](../adr/0001-network-isolation.md) | [ADR-v2-0001](0001-network-identity-and-fingerprint.md) | L1–L134 | [原文宣言の記録](STATUS-LEDGER.md#source-0001-network-isolation) (L12–L14) |
| [0002-mldsa65-p2pkh.md](../adr/0002-mldsa65-p2pkh.md) | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) | L1–L172 | [原文宣言の記録](STATUS-LEDGER.md#source-0002-mldsa65-p2pkh) (L12–L15) |
| [0003-lthash-utxo-accumulator.md](../adr/0003-lthash-utxo-accumulator.md) | [ADR-v2-0003](0003-utxo-accumulator-and-commitment.md) | L1–L199 | [原文宣言の記録](STATUS-LEDGER.md#source-0003-lthash-utxo-accumulator) (L12–L14) |
| [0004-utxo-commitment64.md](../adr/0004-utxo-commitment64.md) | [ADR-v2-0003](0003-utxo-accumulator-and-commitment.md) | L1–L169 | [原文宣言の記録](STATUS-LEDGER.md#source-0004-utxo-commitment64) (L12–L14) |
| [0005-mass-policy.md](../adr/0005-mass-policy.md) | [ADR-v2-0004](0004-mass-carriage-and-transaction-relay.md) | L1–L267 | [原文宣言の記録](STATUS-LEDGER.md#source-0005-mass-policy) (L12–L15) |
| [0006-rpc-wasm-sdk-types.md](../adr/0006-rpc-wasm-sdk-types.md) | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) | L1–L289 | [原文宣言の記録](STATUS-LEDGER.md#source-0006-rpc-wasm-sdk-types) (L12–L15) |
| [0007-layered-pow.md](../adr/0007-layered-pow.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L391 | [原文宣言の記録](STATUS-LEDGER.md#source-0007-layered-pow) (L12–L13, L261–L261) |
| [0008-hash64-consensus-identity.md](../adr/0008-hash64-consensus-identity.md) | [ADR-v2-0001](0001-network-identity-and-fingerprint.md) | L1–L307 | [原文宣言の記録](STATUS-LEDGER.md#source-0008-hash64-consensus-identity) (L12–L20) |
| [0009-dns-probabilistic-finality.md](../adr/0009-dns-probabilistic-finality.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0009-dns-probabilistic-finality) (L5–L5) |
| [0010-validator-node-architecture.md](../adr/0010-validator-node-architecture.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0010-validator-node-architecture) (L5–L5) |
| [0011-validator-deployment-and-equivocation-safety.md](../adr/0011-validator-deployment-and-equivocation-safety.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0011-validator-deployment-and-equivocation-safety) (L5–L5) |
| [0012-mainnet-validator-sortition-commit-reveal.md](../adr/0012-mainnet-validator-sortition-commit-reveal.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0012-mainnet-validator-sortition-commit-reveal) (L5–L5) |
| [0013-validator-reward-distribution.md](../adr/0013-validator-reward-distribution.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0013-validator-reward-distribution) (L5–L5) |
| [0014-coordinated-failover-protocol.md](../adr/0014-coordinated-failover-protocol.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0014-coordinated-failover-protocol) (L5–L5) |
| [0015-remote-signer-hsm-protocol.md](../adr/0015-remote-signer-hsm-protocol.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0015-remote-signer-hsm-protocol) (L5–L5) |
| [0016-stake-locked-bond-utxos.md](../adr/0016-stake-locked-bond-utxos.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0016-stake-locked-bond-utxos) (L5–L5) |
| [0017-all-active-staker-attestation.md](../adr/0017-all-active-staker-attestation.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0017-all-active-staker-attestation) (L5–L5) |
| [0018-quality-gated-stakescore-inclusion-economics.md](../adr/0018-quality-gated-stakescore-inclusion-economics.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0018-quality-gated-stakescore-inclusion-economics) (L5–L5) |
| [0019-mldsa87-migration.md](../adr/0019-mldsa87-migration.md) | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) | L1–L159 | [原文宣言の記録](STATUS-LEDGER.md#source-0019-mldsa87-migration) (L12–L13) |
| [0020-selected-parent-evm-lane.md](../adr/0020-selected-parent-evm-lane.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L171 | [原文宣言の記録](STATUS-LEDGER.md#source-0020-selected-parent-evm-lane) (L14–L24) |
| [0021-palw-llm-pow.md](../adr/0021-palw-llm-pow.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0021-palw-llm-pow) (L5–L5) |
| [0022-pruned-ibd-evm-overlay-snapshot.md](../adr/0022-pruned-ibd-evm-overlay-snapshot.md) | [ADR-v2-0012](0012-state-root-pruning-and-snapshots.md) | L1–L240 | [原文宣言の記録](STATUS-LEDGER.md#source-0022-pruned-ibd-evm-overlay-snapshot) (L12–L14) |
| [0023-base-three-lane-execution.md](../adr/0023-base-three-lane-execution.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L7 | [原文宣言の記録](STATUS-LEDGER.md#source-0023-base-three-lane-execution) (L5–L5) |
| [0024-verified-llm-token-weighted-bft.md](../adr/0024-verified-llm-token-weighted-bft.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0024-verified-llm-token-weighted-bft) (L5–L5) |
| [0025-chain-participation-and-ibd-candidate-selection.md](../adr/0025-chain-participation-and-ibd-candidate-selection.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L89 | [原文宣言の記録](STATUS-LEDGER.md#source-0025-chain-participation-and-ibd-candidate-selection) (L12–L14) |
| [0026-current-palw-testnet11-architecture.md](../adr/0026-current-palw-testnet11-architecture.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L64 | [原文宣言の記録](STATUS-LEDGER.md#source-0026-current-palw-testnet11-architecture) (L15–L15) |
| [0026-palw-v2-runtime-separated-verification.md](../adr/0026-palw-v2-runtime-separated-verification.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L379 | [原文宣言の記録](STATUS-LEDGER.md#source-0026-palw-v2-runtime-separated-verification) (L15–L33) |
| [0027-palw-slash-unilateral-fraud-proofs.md](../adr/0027-palw-slash-unilateral-fraud-proofs.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L309 | [原文宣言の記録](STATUS-LEDGER.md#source-0027-palw-slash-unilateral-fraud-proofs) (L15–L29) |
| [0028-palw-challenge-sampling-protocol.md](../adr/0028-palw-challenge-sampling-protocol.md) | [ADR-v2-0032](0032-sampling-beacon-and-receipts.md) | L1–L436 | [原文宣言の記録](STATUS-LEDGER.md#source-0028-palw-challenge-sampling-protocol) (L24–L42) |
| [0029-palw-chain-carriage.md](../adr/0029-palw-chain-carriage.md) | [ADR-v2-0004](0004-mass-carriage-and-transaction-relay.md) | L1–L273 | [原文宣言の記録](STATUS-LEDGER.md#source-0029-palw-chain-carriage) (L15–L27) |
| [0030-palw-step-function-shape-profile.md](../adr/0030-palw-step-function-shape-profile.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L325 | [原文宣言の記録](STATUS-LEDGER.md#source-0030-palw-step-function-shape-profile) (L15–L27) |
| [0031-palw-canonical-transcendentals.md](../adr/0031-palw-canonical-transcendentals.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L88 | [原文宣言の記録](STATUS-LEDGER.md#source-0031-palw-canonical-transcendentals) (L12–L19) |
| [0032-palw-fee-bond-escrow.md](../adr/0032-palw-fee-bond-escrow.md) | [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) | L1–L149 | [原文宣言の記録](STATUS-LEDGER.md#source-0032-palw-fee-bond-escrow) (L15–L25) |
| [0033-palw-credit-gate-wiring.md](../adr/0033-palw-credit-gate-wiring.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0033-palw-credit-gate-wiring) (L5–L5) |
| [0034-palw-execution-class-model-band-routing.md](../adr/0034-palw-execution-class-model-band-routing.md) | [ADR-v2-0015](0015-permissionless-model-registration.md) | L1–L644 | [原文宣言の記録](STATUS-LEDGER.md#source-0034-palw-execution-class-model-band-routing) (L15–L40) |
| [0035-palw-public-testnet-strategy.md](../adr/0035-palw-public-testnet-strategy.md) | [ADR-v2-0011](0011-release-activation-and-flag-days.md) | L1–L199 | [原文宣言の記録](STATUS-LEDGER.md#source-0035-palw-public-testnet-strategy) (L14–L19) |
| [0036-palw-mainnet-activation-model.md](../adr/0036-palw-mainnet-activation-model.md) | [ADR-v2-0011](0011-release-activation-and-flag-days.md) | L1–L184 | [原文宣言の記録](STATUS-LEDGER.md#source-0036-palw-mainnet-activation-model) (L15–L16) |
| [0037-palw-async-job-state-machine.md](../adr/0037-palw-async-job-state-machine.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L349 | [原文宣言の記録](STATUS-LEDGER.md#source-0037-palw-async-job-state-machine) (L15–L19, L94–L101) |
| [0038-palw-is-the-consensus-work.md](../adr/0038-palw-is-the-consensus-work.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L806 | [原文宣言の記録](STATUS-LEDGER.md#source-0038-palw-is-the-consensus-work) (L17–L20) |
| [0039-palw-only-block-production.md](../adr/0039-palw-only-block-production.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L391 | [原文宣言の記録](STATUS-LEDGER.md#source-0039-palw-only-block-production) (L15–L20) |
| [0040-palw-base-0-integer-arithmetic.md](../adr/0040-palw-base-0-integer-arithmetic.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L456 | [原文宣言の記録](STATUS-LEDGER.md#source-0040-palw-base-0-integer-arithmetic) (L12–L15) |
| [0041-palw-pruning-proof-verification.md](../adr/0041-palw-pruning-proof-verification.md) | [ADR-v2-0012](0012-state-root-pruning-and-snapshots.md) | L1–L449 | [原文宣言の記録](STATUS-LEDGER.md#source-0041-palw-pruning-proof-verification) (L12–L18) |
| [0042-palw-mainnet-candidate-ruleset.md](../adr/0042-palw-mainnet-candidate-ruleset.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L678 | [原文宣言の記録](STATUS-LEDGER.md#source-0042-palw-mainnet-candidate-ruleset) (L15–L16) |
| [0043-palw-v2-state-root-ordering.md](../adr/0043-palw-v2-state-root-ordering.md) | [ADR-v2-0012](0012-state-root-pruning-and-snapshots.md) | L1–L213 | [原文宣言の記録](STATUS-LEDGER.md#source-0043-palw-v2-state-root-ordering) (L12–L19) |
| [0044-palw-free-prompt-receipts.md](../adr/0044-palw-free-prompt-receipts.md) | [ADR-v2-0027](0027-claim-lifecycle-and-liability.md) | L1–L552 | [原文宣言の記録](STATUS-LEDGER.md#source-0044-palw-free-prompt-receipts) (L17–L19) |
| [0045-palw-class-economy-on-chain.md](../adr/0045-palw-class-economy-on-chain.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L207 | [原文宣言の記録](STATUS-LEDGER.md#source-0045-palw-class-economy-on-chain) (L12–L21) |
| [0046-palw-v2-consensus-object-carriage.md](../adr/0046-palw-v2-consensus-object-carriage.md) | [ADR-v2-0004](0004-mass-carriage-and-transaction-relay.md) | L1–L201 | [原文宣言の記録](STATUS-LEDGER.md#source-0046-palw-v2-consensus-object-carriage) (L12–L19) |
| [0047-palw-a16-activation-tier.md](../adr/0047-palw-a16-activation-tier.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L102 | [原文宣言の記録](STATUS-LEDGER.md#source-0047-palw-a16-activation-tier) (L22–L24) |
| [0049-palw-adjudication-contract.md](../adr/0049-palw-adjudication-contract.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L438 | [原文宣言の記録](STATUS-LEDGER.md#source-0049-palw-adjudication-contract) (L15–L17) |
| [0050-palw-base0-residual-site.md](../adr/0050-palw-base0-residual-site.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L191 | [原文宣言の記録](STATUS-LEDGER.md#source-0050-palw-base0-residual-site) (L12–L14) |
| [0051-palw-metal-gguf-execution-family.md](../adr/0051-palw-metal-gguf-execution-family.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L7 | [原文宣言の記録](STATUS-LEDGER.md#source-0051-palw-metal-gguf-execution-family) (L5–L5) |
| [0052-palw-qwen36-hybrid-class.md](../adr/0052-palw-qwen36-hybrid-class.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L258 | [原文宣言の記録](STATUS-LEDGER.md#source-0052-palw-qwen36-hybrid-class) (L15–L17) |
| [0053-palw-one-execution-family.md](../adr/0053-palw-one-execution-family.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L323 | [原文宣言の記録](STATUS-LEDGER.md#source-0053-palw-one-execution-family) (L15–L18) |
| [0054-palw-share-follows-production.md](../adr/0054-palw-share-follows-production.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L137 | [原文宣言の記録](STATUS-LEDGER.md#source-0054-palw-share-follows-production) (L18–L20) |
| [0055-palw-position-is-earned-not-declared.md](../adr/0055-palw-position-is-earned-not-declared.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L97 | [原文宣言の記録](STATUS-LEDGER.md#source-0055-palw-position-is-earned-not-declared) (L15–L19) |
| [0056-palw-permissionless-class-admission-and-share-economy.md](../adr/0056-palw-permissionless-class-admission-and-share-economy.md) | [ADR-v2-0015](0015-permissionless-model-registration.md) | L1–L287 | [原文宣言の記録](STATUS-LEDGER.md#source-0056-palw-permissionless-class-admission-and-share-economy) (L15–L19) |
| [0057-palw-base0-runtime-acceleration.md](../adr/0057-palw-base0-runtime-acceleration.md) | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) | L1–L153 | [原文宣言の記録](STATUS-LEDGER.md#source-0057-palw-base0-runtime-acceleration) (L12–L14) |
| [0058-palw-merged-work-is-counted.md](../adr/0058-palw-merged-work-is-counted.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L132 | [原文宣言の記録](STATUS-LEDGER.md#source-0058-palw-merged-work-is-counted) (L15–L20) |
| [0059-the-10b-premine-cap.md](../adr/0059-the-10b-premine-cap.md) | [ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md) | L1–L101 | [原文宣言の記録](STATUS-LEDGER.md#source-0059-the-10b-premine-cap) (L14–L19) |
| [0060-the-liveness-doctrine.md](../adr/0060-the-liveness-doctrine.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L338 | [原文宣言の記録](STATUS-LEDGER.md#source-0060-the-liveness-doctrine) (L17–L35) |
| [0061-zero-seat-genesis-and-right-sized-collateral.md](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) | [ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md) | L1–L128 | [原文宣言の記録](STATUS-LEDGER.md#source-0061-zero-seat-genesis-and-right-sized-collateral) (L17–L23) |
| [0062-data-availability-court.md](../adr/0062-data-availability-court.md) | [ADR-v2-0039](0039-data-availability-court-and-panel-da.md) | L1–L632 | [原文宣言の記録](STATUS-LEDGER.md#source-0062-data-availability-court) (L25–L25) |
| [0063-operator-tooling-the-missing-half.md](../adr/0063-operator-tooling-the-missing-half.md) | [ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md) | L1–L240 | [原文宣言の記録](STATUS-LEDGER.md#source-0063-operator-tooling-the-missing-half) (L17–L17) |
| [0064-trustless-recovery-from-a-total-stop.md](../adr/0064-trustless-recovery-from-a-total-stop.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L240 | [原文宣言の記録](STATUS-LEDGER.md#source-0064-trustless-recovery-from-a-total-stop) (L14–L19) |
| [0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) | [ADR-v2-0031](0031-panel-independence-and-unavailability.md) | L1–L448 | [原文宣言の記録](STATUS-LEDGER.md#source-0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else) (L21–L28) |
| [0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L279 | [原文宣言の記録](STATUS-LEDGER.md#source-0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table) (L12–L15) |
| [0067-classes-are-chain-data-kernels-are-the-build.md](../adr/0067-classes-are-chain-data-kernels-are-the-build.md) | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) | L1–L761 | [原文宣言の記録](STATUS-LEDGER.md#source-0067-classes-are-chain-data-kernels-are-the-build) (L17–L34) |
| [0068-the-llm-primary-economy-and-the-floors-minimum.md](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L207 | [原文宣言の記録](STATUS-LEDGER.md#source-0068-the-llm-primary-economy-and-the-floors-minimum) (L15–L64) |
| [0069-e2e-adjudicability-is-the-price-of-weight.md](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L498 | [原文宣言の記録](STATUS-LEDGER.md#source-0069-e2e-adjudicability-is-the-price-of-weight) (L15–L28) |
| [0070-the-model-tiers-step-spaces-are-adjudicable.md](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L206 | [原文宣言の記録](STATUS-LEDGER.md#source-0070-the-model-tiers-step-spaces-are-adjudicable) (L15–L21) |
| [0071-the-attempt-lanes-price-and-the-tickets-bound.md](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | L1–L278 | [原文宣言の記録](STATUS-LEDGER.md#source-0071-the-attempt-lanes-price-and-the-tickets-bound) (L15–L21) |
| [0072-the-ticket-is-the-execution.md](../adr/0072-the-ticket-is-the-execution.md) | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | L1–L365 | [原文宣言の記録](STATUS-LEDGER.md#source-0072-the-ticket-is-the-execution) (L15–L23) |
| [0073-real-demand-work-bears-the-weight.md](../adr/0073-real-demand-work-bears-the-weight.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L331 | [原文宣言の記録](STATUS-LEDGER.md#source-0073-real-demand-work-bears-the-weight) (L21–L26) |
| [0074-the-attempt-is-a-claim-drawn-by-the-chain.md](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md) | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | L1–L214 | [原文宣言の記録](STATUS-LEDGER.md#source-0074-the-attempt-is-a-claim-drawn-by-the-chain) (L23–L35) |
| [0075-certification-is-a-consensus-object.md](../adr/0075-certification-is-a-consensus-object.md) | [ADR-v2-0032](0032-sampling-beacon-and-receipts.md) | L1–L375 | [原文宣言の記録](STATUS-LEDGER.md#source-0075-certification-is-a-consensus-object) (L17–L26) |
| [0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) | [ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md) | L1–L215 | [原文宣言の記録](STATUS-LEDGER.md#source-0076-the-attempt-lanes-seed-is-the-retargets-equilibrium) (L18–L28) |
| [0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | [ADR-v2-0024](0024-inference-input-output-contract.md) | L1–L611 | [原文宣言の記録](STATUS-LEDGER.md#source-0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try) (L15–L37) |
| [0078-what-was-made-from-it-is-committed-the-thing-never-rides.md](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) | [ADR-v2-0025](0025-context-and-input-commitments.md) | L1–L470 | [原文宣言の記録](STATUS-LEDGER.md#source-0078-what-was-made-from-it-is-committed-the-thing-never-rides) (L15–L28) |
| [0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md](../adr/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | [ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md) | L1–L469 | [原文宣言の記録](STATUS-LEDGER.md#source-0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host) (L15–L20) |
| [0080-the-answer-is-long-the-verified-unit-is-short.md](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0080-the-answer-is-long-the-verified-unit-is-short) (L5–L5) |
| [0081-long-context-the-input-is-a-state-chain.md](../adr/0081-long-context-the-input-is-a-state-chain.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0081-long-context-the-input-is-a-state-chain) (L5–L5) |
| [0082-the-close-is-flat-in-the-context.md](../adr/0082-the-close-is-flat-in-the-context.md) | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) | L1–L1003 | [原文宣言の記録](STATUS-LEDGER.md#source-0082-the-close-is-flat-in-the-context) (L15–L44) |
| [0083-the-difficulty-window-counts-only-rows-priced-by-bits.md](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) | [ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md) | L1–L115 | [原文宣言の記録](STATUS-LEDGER.md#source-0083-the-difficulty-window-counts-only-rows-priced-by-bits) (L12–L23) |
| [0084-the-ids-ride-the-capture-stays-home.md](../adr/0084-the-ids-ride-the-capture-stays-home.md) | [ADR-v2-0025](0025-context-and-input-commitments.md) | L1–L470 | [原文宣言の記録](STATUS-LEDGER.md#source-0084-the-ids-ride-the-capture-stays-home) (L15–L31) |
| [0085-the-close-is-assembled-from-what-was-served.md](../adr/0085-the-close-is-assembled-from-what-was-served.md) | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) | L1–L215 | [原文宣言の記録](STATUS-LEDGER.md#source-0085-the-close-is-assembled-from-what-was-served) (L15–L24) |
| [0086-the-opening-carries-the-fold-not-the-leaves.md](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md) | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) | L1–L308 | [原文宣言の記録](STATUS-LEDGER.md#source-0086-the-opening-carries-the-fold-not-the-leaves) (L17–L21) |
| [0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | [ADR-v2-0021](0021-positions-and-membership.md) | L1–L394 | [原文宣言の記録](STATUS-LEDGER.md#source-0087-a-position-is-bought-from-the-curve-and-sold-back-to-it) (L18–L25) |
| [0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) | L1–L509 | [原文宣言の記録](STATUS-LEDGER.md#source-0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing) (L20–L48) |
| [0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L574 | [原文宣言の記録](STATUS-LEDGER.md#source-0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand) (L24–L39) |
| [0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) | L1–L217 | [原文宣言の記録](STATUS-LEDGER.md#source-0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole) (L18–L23) |
| [0091-the-reward-buys-the-pair-and-no-holder-is-paid.md](../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) | [ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md) | L1–L326 | [原文宣言の記録](STATUS-LEDGER.md#source-0091-the-reward-buys-the-pair-and-no-holder-is-paid) (L18–L32) |
| [0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L296 | [原文宣言の記録](STATUS-LEDGER.md#source-0092-the-ladder-is-minted-once-and-the-clock-is-what-binds) (L15–L25) |
| [0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) | L1–L410 | [原文宣言の記録](STATUS-LEDGER.md#source-0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing) (L15–L31) |
| [0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) | L1–L215 | [原文宣言の記録](STATUS-LEDGER.md#source-0094-a-seed-is-paid-in-as-many-transactions-as-it-takes) (L14–L19) |
| [0095-a-position-is-a-membership-not-an-income.md](../adr/0095-a-position-is-a-membership-not-an-income.md) | [ADR-v2-0021](0021-positions-and-membership.md) | L1–L334 | [原文宣言の記録](STATUS-LEDGER.md#source-0095-a-position-is-a-membership-not-an-income) (L20–L40) |
| [0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) | [ADR-v2-0024](0024-inference-input-output-contract.md) | L1–L685 | [原文宣言の記録](STATUS-LEDGER.md#source-0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed) (L15–L36) |
| [0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L462 | [原文宣言の記録](STATUS-LEDGER.md#source-0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token) (L15–L35) |
| [0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) | [ADR-v2-0031](0031-panel-independence-and-unavailability.md) | L1–L326 | [原文宣言の記録](STATUS-LEDGER.md#source-0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else) (L15–L37) |
| [0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) | [ADR-v2-0030](0030-sharded-verification-and-model-measurement.md) | L1–L371 | [原文宣言の記録](STATUS-LEDGER.md#source-0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard) (L15–L42) |
| [0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | [ADR-v2-0015](0015-permissionless-model-registration.md) | L1–L425 | [原文宣言の記録](STATUS-LEDGER.md#source-0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard) (L15–L39) |
| [0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | [ADR-v2-0021](0021-positions-and-membership.md) | L1–L207 | [原文宣言の記録](STATUS-LEDGER.md#source-0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves) (L12–L34) |
| [0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) | L1–L252 | [原文宣言の記録](STATUS-LEDGER.md#source-0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity) (L12–L29) |
| [0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) | [ADR-v2-0025](0025-context-and-input-commitments.md) | L1–L807 | [原文宣言の記録](STATUS-LEDGER.md#source-0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm) (L15–L55) |
| [0105-a-heartbeat-never-turns-a-bonded-block-red.md](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L558 | [原文宣言の記録](STATUS-LEDGER.md#source-0105-a-heartbeat-never-turns-a-bonded-block-red) (L12–L32) |
| [0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) | [ADR-v2-0017](0017-model-streaming-and-memory-bounds.md) | L1–L246 | [原文宣言の記録](STATUS-LEDGER.md#source-0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model) (L15–L30) |
| [0107-a-share-grows-on-work-that-reached-final.md](../adr/0107-a-share-grows-on-work-that-reached-final.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L167 | [原文宣言の記録](STATUS-LEDGER.md#source-0107-a-share-grows-on-work-that-reached-final) (L23–L34) |
| [0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | [ADR-v2-0032](0032-sampling-beacon-and-receipts.md) | L1–L521 | [原文宣言の記録](STATUS-LEDGER.md#source-0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote) (L23–L43) |
| [0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) | [ADR-v2-0027](0027-claim-lifecycle-and-liability.md) | L1–L289 | [原文宣言の記録](STATUS-LEDGER.md#source-0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause) (L17–L33) |
| [0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L395 | [原文宣言の記録](STATUS-LEDGER.md#source-0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation) (L15–L28) |
| [0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) | [ADR-v2-0039](0039-data-availability-court-and-panel-da.md) | L1–L310 | [原文宣言の記録](STATUS-LEDGER.md#source-0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge) (L15–L33) |
| [0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) | [ADR-v2-0017](0017-model-streaming-and-memory-bounds.md) | L1–L436 | [原文宣言の記録](STATUS-LEDGER.md#source-0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact) (L15–L32) |
| [0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) | [ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md) | L1–L125 | [原文宣言の記録](STATUS-LEDGER.md#source-0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height) (L14–L28) |
| [0115-a-pending-transaction-is-announced-until-it-lands.md](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md) | [ADR-v2-0004](0004-mass-carriage-and-transaction-relay.md) | L1–L86 | [原文宣言の記録](STATUS-LEDGER.md#source-0115-a-pending-transaction-is-announced-until-it-lands) (L14–L24) |
| [0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) | [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | L1–L177 | [原文宣言の記録](STATUS-LEDGER.md#source-0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width) (L15–L26) |
| [0117-a-draw-is-one-forward.md](../adr/0117-a-draw-is-one-forward.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L260 | [原文宣言の記録](STATUS-LEDGER.md#source-0117-a-draw-is-one-forward) (L15–L32) |
| [0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L285 | [原文宣言の記録](STATUS-LEDGER.md#source-0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form) (L15–L35) |
| [0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | L1–L244 | [原文宣言の記録](STATUS-LEDGER.md#source-0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are) (L15–L30) |
| [0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) | L1–L78 | [原文宣言の記録](STATUS-LEDGER.md#source-0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height) (L14–L25) |
| [0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) | [ADR-v2-0017](0017-model-streaming-and-memory-bounds.md) | L1–L296 | [原文宣言の記録](STATUS-LEDGER.md#source-0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders) (L15–L34) |
| [0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md](../adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) | [ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md) | L1–L990 | [原文宣言の記録](STATUS-LEDGER.md#source-0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id) (L17–L29) |
| [0123-the-epoch-progressively-releases-unused-class-budget.md](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L62 | [原文宣言の記録](STATUS-LEDGER.md#source-0123-the-epoch-progressively-releases-unused-class-budget) (L12–L16) |
| [0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) | L1–L450 | [原文宣言の記録](STATUS-LEDGER.md#source-0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies) (L17–L48) |
| [0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L432 | [原文宣言の記録](STATUS-LEDGER.md#source-0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time) (L17–L33) |
| [0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays) (L5–L5) |
| [0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L100 | [原文宣言の記録](STATUS-LEDGER.md#source-0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms) (L15–L26) |
| [0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L9 | [原文宣言の記録](STATUS-LEDGER.md#source-0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate) (L5–L5) |
| [0129-a-double-spend-needs-the-anchors-not-the-blocks.md](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L139 | [原文宣言の記録](STATUS-LEDGER.md#source-0129-a-double-spend-needs-the-anchors-not-the-blocks) (L14–L28) |
| [0130-bps1-is-hardened-before-it-is-widened.md](../adr/0130-bps1-is-hardened-before-it-is-widened.md) | [ADR-v2-0011](0011-release-activation-and-flag-days.md) | L1–L280 | [原文宣言の記録](STATUS-LEDGER.md#source-0130-bps1-is-hardened-before-it-is-widened) (L17–L30) |
| [0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L190 | [原文宣言の記録](STATUS-LEDGER.md#source-0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves) (L17–L29) |
| [0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L371 | [原文宣言の記録](STATUS-LEDGER.md#source-0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price) (L17–L28) |
| [0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md](../adr/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) | [ADR-v2-0034](0034-verification-spans-and-service-clock.md) | L1–L468 | [原文宣言の記録](STATUS-LEDGER.md#source-0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself) (L17–L31) |
| [0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L135 | [原文宣言の記録](STATUS-LEDGER.md#source-0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes) (L12–L24) |
| [0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | [ADR-v2-0015](0015-permissionless-model-registration.md) | L1–L438 | [原文宣言の記録](STATUS-LEDGER.md#source-0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle) (L15–L31) |
| [0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) | [ADR-v2-0017](0017-model-streaming-and-memory-bounds.md) | L1–L120 | [原文宣言の記録](STATUS-LEDGER.md#source-0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it) (L12–L13) |
| [0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L723 | [原文宣言の記録](STATUS-LEDGER.md#source-0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input) (L17–L24) |
| [0138-the-daa-score-is-the-anchors-clock.md](../adr/0138-the-daa-score-is-the-anchors-clock.md) | [ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md) | L1–L142 | [原文宣言の記録](STATUS-LEDGER.md#source-0138-the-daa-score-is-the-anchors-clock) (L12–L15) |
| [0139-the-execution-lanes-gas-is-one-budget-a-round.md](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L113 | [原文宣言の記録](STATUS-LEDGER.md#source-0139-the-execution-lanes-gas-is-one-budget-a-round) (L12–L16) |
| [0140-the-heartbeat-is-the-emergency-generator.md](../adr/0140-the-heartbeat-is-the-emergency-generator.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L248 | [原文宣言の記録](STATUS-LEDGER.md#source-0140-the-heartbeat-is-the-emergency-generator) (L12–L15) |
| [0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | L1–L136 | [原文宣言の記録](STATUS-LEDGER.md#source-0141-can-an-inference-be-the-ticket-without-a-hash-lottery) (L15–L19) |
| [0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) | [ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md) | L1–L358 | [原文宣言の記録](STATUS-LEDGER.md#source-0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot) (L12–L18, L259–L262) |
| [0143-an-artifact-root-has-one-owner-on-the-chain.md](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md) | [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) | L1–L240 | [原文宣言の記録](STATUS-LEDGER.md#source-0143-an-artifact-root-has-one-owner-on-the-chain) (L20–L23) |
| [0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L298 | [原文宣言の記録](STATUS-LEDGER.md#source-0144-palw-pays-for-the-inference-you-were-going-to-run-anyway) (L15–L17) |
| [0145-canonical-work-is-derived-and-admission-is-earned.md](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L308 | [原文宣言の記録](STATUS-LEDGER.md#source-0145-canonical-work-is-derived-and-admission-is-earned) (L15–L19) |
| [0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) | [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | L1–L293 | [原文宣言の記録](STATUS-LEDGER.md#source-0146-a-coefficient-is-justified-by-the-arbitrage-it-permits) (L12–L17) |
| [0147-independence-is-drawn-not-declared.md](../adr/0147-independence-is-drawn-not-declared.md) | [ADR-v2-0031](0031-panel-independence-and-unavailability.md) | L1–L226 | [原文宣言の記録](STATUS-LEDGER.md#source-0147-independence-is-drawn-not-declared) (L15–L18) |
| [0148-the-free-prompt-lane-prices-compute.md](../adr/0148-the-free-prompt-lane-prices-compute.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L214 | [原文宣言の記録](STATUS-LEDGER.md#source-0148-the-free-prompt-lane-prices-compute) (L15–L19) |
| [0149-an-attempts-pwu-is-the-derivation.md](../adr/0149-an-attempts-pwu-is-the-derivation.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L160 | [原文宣言の記録](STATUS-LEDGER.md#source-0149-an-attempts-pwu-is-the-derivation) (L15–L16) |
| [0150-the-fingerprint-must-see-the-rule-not-only-the-height.md](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) | [ADR-v2-0001](0001-network-identity-and-fingerprint.md) | L1–L187 | [原文宣言の記録](STATUS-LEDGER.md#source-0150-the-fingerprint-must-see-the-rule-not-only-the-height) (L12–L14) |
| [0151-liveness-is-structural-collateral-covers-fraud.md](../adr/0151-liveness-is-structural-collateral-covers-fraud.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L383 | [原文宣言の記録](STATUS-LEDGER.md#source-0151-liveness-is-structural-collateral-covers-fraud) (L17–L29) |
| [0152-account-stake-staged-reserve-and-vested-rewards.md](../adr/0152-account-stake-staged-reserve-and-vested-rewards.md) | [ADR-v2-0035](0035-bond-budget-and-shared-reservations.md) | L1–L94 | [原文宣言の記録](STATUS-LEDGER.md#source-0152-account-stake-staged-reserve-and-vested-rewards) (L12–L22) |
| [0153-2m-verification-sharding-plan.md](../adr/0153-2m-verification-sharding-plan.md) | [ADR-v2-0030](0030-sharded-verification-and-model-measurement.md) | L1–L78 | [原文宣言の記録](STATUS-LEDGER.md#source-0153-2m-verification-sharding-plan) (L5–L13) |
| [0154-testnet-12-flag-day-daa-750.md](../adr/0154-testnet-12-flag-day-daa-750.md) | [ADR-v2-0011](0011-release-activation-and-flag-days.md) | L1–L71 | [原文宣言の記録](STATUS-LEDGER.md#source-0154-testnet-12-flag-day-daa-750) (L5–L13) |
| [0155-testnet-12-flag-day-daa-1300.md](../adr/0155-testnet-12-flag-day-daa-1300.md) | [ADR-v2-0011](0011-release-activation-and-flag-days.md) | L1–L50 | [原文宣言の記録](STATUS-LEDGER.md#source-0155-testnet-12-flag-day-daa-1300) (L5–L10) |
| [0160-claim-capacity-separation.md](../adr/0160-claim-capacity-separation.md) | [ADR-v2-0035](0035-bond-budget-and-shared-reservations.md) | L1–L1389 | [原文宣言の記録](STATUS-LEDGER.md#source-0160-claim-capacity-separation) (L5–L29) |
| [0161-a-claim-is-not-an-emission.md](../adr/0161-a-claim-is-not-an-emission.md) | [ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md) | L1–L126 | [原文宣言の記録](STATUS-LEDGER.md#source-0161-a-claim-is-not-an-emission) (L5–L11) |
| [0162-the-pair-opens-on-a-virtual-reserve.md](../adr/0162-the-pair-opens-on-a-virtual-reserve.md) | [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) | L1–L425 | [原文宣言の記録](STATUS-LEDGER.md#source-0162-the-pair-opens-on-a-virtual-reserve) (L18–L33) |
| [0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) | [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) | L1–L87 | [原文宣言の記録](STATUS-LEDGER.md#source-0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed) (L15–L20) |
| [0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) | [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | L1–L438 | [原文宣言の記録](STATUS-LEDGER.md#source-0165-the-floor-is-a-reserve-and-the-work-carries-the-clock) (L17–L17) |
| [0166-verifier-unavailability-is-not-producer-fraud.md](../adr/0166-verifier-unavailability-is-not-producer-fraud.md) | [ADR-v2-0031](0031-panel-independence-and-unavailability.md) | L1–L125 | [原文宣言の記録](STATUS-LEDGER.md#source-0166-verifier-unavailability-is-not-producer-fraud) (L17–L21, L123–L123) |
| [0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md](../adr/0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md) | [ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md) | L1–L185 | [原文宣言の記録](STATUS-LEDGER.md#source-0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker) (L17–L19) |
| [0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L311 | [原文宣言の記録](STATUS-LEDGER.md#source-0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor) (定型宣言なし) |
| [0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | L1–L34 | [原文宣言の記録](STATUS-LEDGER.md#source-0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission) (L12–L12) |
| [0170-the-seed-anchor-is-a-window-not-a-span.md](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md) | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | L1–L147 | [原文宣言の記録](STATUS-LEDGER.md#source-0170-the-seed-anchor-is-a-window-not-a-span) (L15–L19) |
| [0171-probabilistic-constraint-checks-and-court-on-dispute.md](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md) | [ADR-v2-0033](0033-probabilistic-checks-and-exact-court.md) | L1–L127 | [原文宣言の記録](STATUS-LEDGER.md#source-0171-probabilistic-constraint-checks-and-court-on-dispute) (L15–L15) |
| [0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) | L1–L204 | [原文宣言の記録](STATUS-LEDGER.md#source-0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm) (L15–L16) |
| [0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L160 | [原文宣言の記録](STATUS-LEDGER.md#source-0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation) (L14–L19) |
| [0173-public-verifier-dispute-completeness-is-misaka-purpose.md](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L232 | [原文宣言の記録](STATUS-LEDGER.md#source-0173-public-verifier-dispute-completeness-is-misaka-purpose) (L14–L21, L217–L217) |
| [0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) | [ADR-v2-0001](0001-network-identity-and-fingerprint.md) | L1–L57 | [原文宣言の記録](STATUS-LEDGER.md#source-0174-token-name-misaka-ticker-bili-address-prefix-unchanged) (L12–L12) |
| [0175-registered-models-are-permanently-immutable.md](../adr/0175-registered-models-are-permanently-immutable.md) | [ADR-v2-0015](0015-permissionless-model-registration.md) | L1–L103 | [原文宣言の記録](STATUS-LEDGER.md#source-0175-registered-models-are-permanently-immutable) (L12–L12) |
| [0176-bond-bounds-palw-production-rewards-and-final-weight.md](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) | [ADR-v2-0035](0035-bond-budget-and-shared-reservations.md) | L1–L228 | [原文宣言の記録](STATUS-LEDGER.md#source-0176-bond-bounds-palw-production-rewards-and-final-weight) (L5–L8, L156–L156, L166–L166, L182–L182) |
| [0177-model-bond-allocation-without-availability-consensus.md](../adr/0177-model-bond-allocation-without-availability-consensus.md) | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) | L1–L285 | [原文宣言の記録](STATUS-LEDGER.md#source-0177-model-bond-allocation-without-availability-consensus) (L5–L10, L206–L207) |
| [0178-fork-choice-heals-a-partition-on-bonded-participation.md](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) | [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | L1–L284 | [原文宣言の記録](STATUS-LEDGER.md#source-0178-fork-choice-heals-a-partition-on-bonded-participation) (L12–L16) |
| [0179-model-tiers-genesis-annual-1-0-registered-0-3.md](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) | L1–L109 | [原文宣言の記録](STATUS-LEDGER.md#source-0179-model-tiers-genesis-annual-1-0-registered-0-3) (L7–L9) |
| [0180-palw-only-consensus-no-bft-validators.md](../adr/0180-palw-only-consensus-no-bft-validators.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) | L1–L26 | [原文宣言の記録](STATUS-LEDGER.md#source-0180-palw-only-consensus-no-bft-validators) (L5–L11) |
| [0181-design-freeze-and-controlled-change.md](../adr/0181-design-freeze-and-controlled-change.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L22 | [原文宣言の記録](STATUS-LEDGER.md#source-0181-design-freeze-and-controlled-change) (L3–L8) |
| [0182-defer-media-and-model-improvement-until-post-mainnet.md](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L30 | [原文宣言の記録](STATUS-LEDGER.md#source-0182-defer-media-and-model-improvement-until-post-mainnet) (L3–L10) |
| [0183-g14-court-capacity-panel-floor-retention-detection.md](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L23 | [原文宣言の記録](STATUS-LEDGER.md#source-0183-g14-court-capacity-panel-floor-retention-detection) (L3–L8) |
| [0184-native-liability-horizon-panel-record-and-single-certified-leg.md](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md) | [ADR-v2-0027](0027-claim-lifecycle-and-liability.md) | L1–L27 | [原文宣言の記録](STATUS-LEDGER.md#source-0184-native-liability-horizon-panel-record-and-single-certified-leg) (L3–L8) |
| [0185-paid-lock-randomness-bootstrap-and-paid-sources.md](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md) | [ADR-v2-0032](0032-sampling-beacon-and-receipts.md) | L1–L33 | [原文宣言の記録](STATUS-LEDGER.md#source-0185-paid-lock-randomness-bootstrap-and-paid-sources) (L3–L8) |
| [0186-hold-unresolved-liability-advance-independent-claims.md](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) | [ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md) | L1–L31 | [原文宣言の記録](STATUS-LEDGER.md#source-0186-hold-unresolved-liability-advance-independent-claims) (L3–L6) |
| [0187-g14-native-public-filing-and-post-final-mining-accusation.md](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L59 | [原文宣言の記録](STATUS-LEDGER.md#source-0187-g14-native-public-filing-and-post-final-mining-accusation) (L3–L8) |
| [0188-d2-x3-arm-records-composition-and-pause-credit.md](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | L1–L93 | [原文宣言の記録](STATUS-LEDGER.md#source-0188-d2-x3-arm-records-composition-and-pause-credit) (L3–L14) |
| [0189-tir-court-read-set-door-options.md](../adr/0189-tir-court-read-set-door-options.md) | [ADR-v2-0041](0041-long-context-court-decomposition.md) | L1–L58 | [原文宣言の記録](STATUS-LEDGER.md#source-0189-tir-court-read-set-door-options) (L3–L6) |
| [0190-tir-door-sparse-fetch-designated-answers-tenure-split.md](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md) | [ADR-v2-0041](0041-long-context-court-decomposition.md) | L1–L122 | [原文宣言の記録](STATUS-LEDGER.md#source-0190-tir-door-sparse-fetch-designated-answers-tenure-split) (L3–L9) |
| [0191-tir-dispute-local-decomposition-long-context.md](../adr/0191-tir-dispute-local-decomposition-long-context.md) | [ADR-v2-0041](0041-long-context-court-decomposition.md) | L1–L252 | [原文宣言の記録](STATUS-LEDGER.md#source-0191-tir-dispute-local-decomposition-long-context) (L3–L13) |
| [0192-court-service-opportunity-and-shared-pursuit.md](../adr/0192-court-service-opportunity-and-shared-pursuit.md) | [ADR-v2-0042](0042-court-service-and-versioned-clock.md) | L1–L53 | [原文宣言の記録](STATUS-LEDGER.md#source-0192-court-service-opportunity-and-shared-pursuit) (L3–L6) |
| [0193-miner-account-bond-for-panel-incentives.md](../adr/0193-miner-account-bond-for-panel-incentives.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | L1–L156 | [原文宣言の記録](STATUS-LEDGER.md#source-0193-miner-account-bond-for-panel-incentives) (L3–L9) |
| [0193-sequential-witness-filing-clock.md](../adr/0193-sequential-witness-filing-clock.md) | [ADR-v2-0042](0042-court-service-and-versioned-clock.md) | L1–L23 | [原文宣言の記録](STATUS-LEDGER.md#source-0193-sequential-witness-filing-clock) (L3–L3) |
| [0194-dormant-g14-integration-and-service-schema.md](../adr/0194-dormant-g14-integration-and-service-schema.md) | [ADR-v2-0042](0042-court-service-and-versioned-clock.md) | L1–L66 | [原文宣言の記録](STATUS-LEDGER.md#source-0194-dormant-g14-integration-and-service-schema) (L3–L3) |
| [0195-rooted-court-response-eligibility-history.md](../adr/0195-rooted-court-response-eligibility-history.md) | [ADR-v2-0043](0043-court-eligibility-and-initial-notice.md) | L1–L46 | [原文宣言の記録](STATUS-LEDGER.md#source-0195-rooted-court-response-eligibility-history) (L3–L6) |
| [0196-owner-independent-shared-tir-query-data.md](../adr/0196-owner-independent-shared-tir-query-data.md) | [ADR-v2-0044](0044-shared-tir-query-data.md) | L1–L76 | [原文宣言の記録](STATUS-LEDGER.md#source-0196-owner-independent-shared-tir-query-data) (L3–L6) |
| [0197-court-end-effects-and-pending-liability-archive.md](../adr/0197-court-end-effects-and-pending-liability-archive.md) | [ADR-v2-0046](0046-court-close-and-liability-archive.md) | L1–L60 | [原文宣言の記録](STATUS-LEDGER.md#source-0197-court-end-effects-and-pending-liability-archive) (L3–L3) |
| [0198-shared-court-initial-notice-boundary.md](../adr/0198-shared-court-initial-notice-boundary.md) | [ADR-v2-0043](0043-court-eligibility-and-initial-notice.md) | L1–L24 | [原文宣言の記録](STATUS-LEDGER.md#source-0198-shared-court-initial-notice-boundary) (L3–L3) |
| [0199-shared-court-terminal-material-and-nonblocking-ingress.md](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md) | [ADR-v2-0045](0045-terminal-material-and-readiness.md) | L1–L25 | [原文宣言の記録](STATUS-LEDGER.md#source-0199-shared-court-terminal-material-and-nonblocking-ingress) (L3–L3) |
| [0200-d2-readiness-root-opening-availability.md](../adr/0200-d2-readiness-root-opening-availability.md) | [ADR-v2-0045](0045-terminal-material-and-readiness.md) | L1–L25 | [原文宣言の記録](STATUS-LEDGER.md#source-0200-d2-readiness-root-opening-availability) (L3–L3) |
| [0201-archived-close-qualified-financial-discharge.md](../adr/0201-archived-close-qualified-financial-discharge.md) | [ADR-v2-0046](0046-court-close-and-liability-archive.md) | L1–L42 | [原文宣言の記録](STATUS-LEDGER.md#source-0201-archived-close-qualified-financial-discharge) (L3–L3) |
| [0202-court-original-funded-contract-and-role-consent.md](../adr/0202-court-original-funded-contract-and-role-consent.md) | [ADR-v2-0047](0047-funded-original-and-role-authorization.md) | L1–L35 | [原文宣言の記録](STATUS-LEDGER.md#source-0202-court-original-funded-contract-and-role-consent) (L3–L3) |
| [0203-authenticated-palw-state-map-and-row-access.md](../adr/0203-authenticated-palw-state-map-and-row-access.md) | [ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md) | L1–L35 | [原文宣言の記録](STATUS-LEDGER.md#source-0203-authenticated-palw-state-map-and-row-access) (L3–L3) |
| [0204-ibd-bounded-row-stream-and-storage-generations.md](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md) | [ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md) | L1–L47 | [原文宣言の記録](STATUS-LEDGER.md#source-0204-ibd-bounded-row-stream-and-storage-generations) (L3–L5) |
| [0205-a-decoded-answer-is-not-paid-twice.md](../adr/0205-a-decoded-answer-is-not-paid-twice.md) | [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | L1–L30 | [原文宣言の記録](STATUS-LEDGER.md#source-0205-a-decoded-answer-is-not-paid-twice) (L3–L5) |
| [0206-panel-da-mandatory-disclosure-g14-route.md](../adr/0206-panel-da-mandatory-disclosure-g14-route.md) | [ADR-v2-0039](0039-data-availability-court-and-panel-da.md) | L1–L68 | [原文宣言の記録](STATUS-LEDGER.md#source-0206-panel-da-mandatory-disclosure-g14-route) (L3–L7) |
| [0207-finite-fee-consent-and-provider-payment-bounds.md](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) | [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) | L1–L34 | [原文宣言の記録](STATUS-LEDGER.md#source-0207-finite-fee-consent-and-provider-payment-bounds) (L3–L3) |
| [0208-authenticated-prefix-work-dependencies.md](../adr/0208-authenticated-prefix-work-dependencies.md) | [ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md) | L1–L21 | [原文宣言の記録](STATUS-LEDGER.md#source-0208-authenticated-prefix-work-dependencies) (L3–L3) |
| [0209-genesis-bond-free-bootstrap.md](../adr/0209-genesis-bond-free-bootstrap.md) | [ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md) | L1–L17 | [原文宣言の記録](STATUS-LEDGER.md#source-0209-genesis-bond-free-bootstrap) (L3–L3) |

## 新文書から探す

### ADR-v2-0001 — ネットワーク分離・Hash64・識別子・fingerprint

[統合本文を開く](0001-network-identity-and-fingerprint.md) / [旧ADR-0001](../adr/0001-network-isolation.md)、[旧ADR-0008](../adr/0008-hash64-consensus-identity.md)、[旧ADR-0150](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md)、[旧ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)

### ADR-v2-0002 — 耐量子署名・アドレス・RPC／WASM／SDK型

[統合本文を開く](0002-post-quantum-signatures-and-interfaces.md) / [旧ADR-0002](../adr/0002-mldsa65-p2pkh.md)、[旧ADR-0006](../adr/0006-rpc-wasm-sdk-types.md)、[旧ADR-0019](../adr/0019-mldsa87-migration.md)

### ADR-v2-0003 — UTXO accumulatorと64-byte commitment

[統合本文を開く](0003-utxo-accumulator-and-commitment.md) / [旧ADR-0003](../adr/0003-lthash-utxo-accumulator.md)、[旧ADR-0004](../adr/0004-utxo-commitment64.md)

### ADR-v2-0004 — Mass・DoS上限・合意object搬送・再送

[統合本文を開く](0004-mass-carriage-and-transaction-relay.md) / [旧ADR-0005](../adr/0005-mass-policy.md)、[旧ADR-0029](../adr/0029-palw-chain-carriage.md)、[旧ADR-0046](../adr/0046-palw-v2-consensus-object-carriage.md)、[旧ADR-0115](../adr/0115-a-pending-transaction-is-announced-until-it-lands.md)

### ADR-v2-0005 — PALW合意構造・実行family・旧PoWからの移行

[統合本文を開く](0005-palw-consensus-architecture.md) / [旧ADR-0007](../adr/0007-layered-pow.md)、[旧ADR-0026（current-palw-testnet11-architecture）](../adr/0026-current-palw-testnet11-architecture.md)、[旧ADR-0026（palw-v2-runtime-separated-verification）](../adr/0026-palw-v2-runtime-separated-verification.md)、[旧ADR-0037](../adr/0037-palw-async-job-state-machine.md)、[旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md)、[旧ADR-0039](../adr/0039-palw-only-block-production.md)、[旧ADR-0042](../adr/0042-palw-mainnet-candidate-ruleset.md)、[旧ADR-0053](../adr/0053-palw-one-execution-family.md)、[旧ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md)

### ADR-v2-0006 — Fork choice・参加重み・merged work・settlement

[統合本文を開く](0006-fork-choice-participation-and-settlement.md) / [旧ADR-0025](../adr/0025-chain-participation-and-ibd-candidate-selection.md)、[旧ADR-0055](../adr/0055-palw-position-is-earned-not-declared.md)、[旧ADR-0058](../adr/0058-palw-merged-work-is-counted.md)、[旧ADR-0127](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md)、[旧ADR-0129](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md)、[旧ADR-0178](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md)

### ADR-v2-0007 — EVM／EXEC実行lane・anchor・gas budget

[統合本文を開く](0007-evm-exec-lane-and-anchors.md) / [旧ADR-0020](../adr/0020-selected-parent-evm-lane.md)、[旧ADR-0089](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)、[旧ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)、[旧ADR-0139](../adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md)、[旧ADR-0168](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md)、[旧ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md)

### ADR-v2-0008 — DAA・difficulty・retarget・合意時計

[統合本文を開く](0008-daa-difficulty-and-consensus-clock.md) / [旧ADR-0076](../adr/0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)、[旧ADR-0083](../adr/0083-the-difficulty-window-counts-only-rows-priced-by-bits.md)、[旧ADR-0138](../adr/0138-the-daa-score-is-the-anchors-clock.md)、[旧ADR-0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)

### ADR-v2-0009 — Heartbeat・BASE floor・停止回復・liveness

[統合本文を開く](0009-heartbeat-floor-and-liveness.md) / [旧ADR-0060](../adr/0060-the-liveness-doctrine.md)、[旧ADR-0064](../adr/0064-trustless-recovery-from-a-total-stop.md)、[旧ADR-0066](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)、[旧ADR-0105](../adr/0105-a-heartbeat-never-turns-a-bonded-block-red.md)、[旧ADR-0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md)、[旧ADR-0151](../adr/0151-liveness-is-structural-collateral-covers-fraud.md)、[旧ADR-0165](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md)

### ADR-v2-0010 — Genesis発行上限・bondゼロ起動

[統合本文を開く](0010-genesis-emission-and-bond-free-bootstrap.md) / [旧ADR-0059](../adr/0059-the-10b-premine-cap.md)、[旧ADR-0061](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md)、[旧ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md)

### ADR-v2-0011 — 公開testnet・release・有効化・flag day

[統合本文を開く](0011-release-activation-and-flag-days.md) / [旧ADR-0035](../adr/0035-palw-public-testnet-strategy.md)、[旧ADR-0036](../adr/0036-palw-mainnet-activation-model.md)、[旧ADR-0130](../adr/0130-bps1-is-hardened-before-it-is-widened.md)、[旧ADR-0154](../adr/0154-testnet-12-flag-day-daa-750.md)、[旧ADR-0155](../adr/0155-testnet-12-flag-day-daa-1300.md)

### ADR-v2-0012 — 状態root・pruning proof・snapshot同期

[統合本文を開く](0012-state-root-pruning-and-snapshots.md) / [旧ADR-0022](../adr/0022-pruned-ibd-evm-overlay-snapshot.md)、[旧ADR-0041](../adr/0041-palw-pruning-proof-verification.md)、[旧ADR-0043](../adr/0043-palw-v2-state-root-ordering.md)

### ADR-v2-0013 — TIR算術・tile・BASE-0・決定論

[統合本文を開く](0013-tir-arithmetic-and-determinism.md) / [旧ADR-0030](../adr/0030-palw-step-function-shape-profile.md)、[旧ADR-0031](../adr/0031-palw-canonical-transcendentals.md)、[旧ADR-0040](../adr/0040-palw-base-0-integer-arithmetic.md)、[旧ADR-0047](../adr/0047-palw-a16-activation-tier.md)、[旧ADR-0050](../adr/0050-palw-base0-residual-site.md)、[旧ADR-0052](../adr/0052-palw-qwen36-hybrid-class.md)、[旧ADR-0116](../adr/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)

### ADR-v2-0014 — Versioned kernel・拡張境界・runtime backend

[統合本文を開く](0014-versioned-kernels-and-backends.md) / [旧ADR-0057](../adr/0057-palw-base0-runtime-acceleration.md)、[旧ADR-0067](../adr/0067-classes-are-chain-data-kernels-are-the-build.md)、[旧ADR-0102](../adr/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)、[旧ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)

### ADR-v2-0015 — Permissionlessモデル登録・class profile・不変性

[統合本文を開く](0015-permissionless-model-registration.md) / [旧ADR-0034](../adr/0034-palw-execution-class-model-band-routing.md)、[旧ADR-0056](../adr/0056-palw-permissionless-class-admission-and-share-economy.md)、[旧ADR-0100](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)、[旧ADR-0135](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md)、[旧ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)

### ADR-v2-0016 — Artifact所有・モデルlineage・adapter

[統合本文を開く](0016-artifact-ownership-and-model-lineage.md) / [旧ADR-0088](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)、[旧ADR-0143](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md)、[旧ADR-0163](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md)

### ADR-v2-0017 — モデルstreaming・mmap・weight／captureメモリ上限

[統合本文を開く](0017-model-streaming-and-memory-bounds.md) / [旧ADR-0106](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md)、[旧ADR-0112](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)、[旧ADR-0121](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md)、[旧ADR-0136](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md)

### ADR-v2-0018 — 長文脈profile・公開計測・context有効化

[統合本文を開く](0018-long-context-measurement-and-admission.md) / [旧ADR-0070](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md)、[旧ADR-0092](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)、[旧ADR-0097](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)、[旧ADR-0110](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)、[旧ADR-0118](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)、[旧ADR-0119](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)

### ADR-v2-0019 — モデル二階層・年次指定・非共有リスクの境界

[統合本文を開く](0019-model-tiers-and-annual-catalog.md) / [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)、[旧ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md)

### ADR-v2-0020 — Class share・epoch配分・経済係数の変遷

[統合本文を開く](0020-class-share-and-allocation-evolution.md) / [旧ADR-0045](../adr/0045-palw-class-economy-on-chain.md)、[旧ADR-0054](../adr/0054-palw-share-follows-production.md)、[旧ADR-0068](../adr/0068-the-llm-primary-economy-and-the-floors-minimum.md)、[旧ADR-0107](../adr/0107-a-share-grows-on-work-that-reached-final.md)、[旧ADR-0123](../adr/0123-the-epoch-progressively-releases-unused-class-budget.md)、[旧ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)、[旧ADR-0146](../adr/0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)

### ADR-v2-0021 — Position・curve・membership・非譲渡性

[統合本文を開く](0021-positions-and-membership.md) / [旧ADR-0087](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)、[旧ADR-0095](../adr/0095-a-position-is-a-membership-not-an-income.md)、[旧ADR-0101](../adr/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md)

### ADR-v2-0022 — 流動性seed・分割入金・virtual reserve

[統合本文を開く](0022-liquidity-seeding-and-virtual-reserves.md) / [旧ADR-0090](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)、[旧ADR-0094](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)、[旧ADR-0120](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md)、[旧ADR-0162](../adr/0162-the-pair-opens-on-a-virtual-reserve.md)

### ADR-v2-0023 — モデル報酬の送金先・owner leg

[統合本文を開く](0023-model-reward-routing-and-owner-leg.md) / [旧ADR-0091](../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)、[旧ADR-0114](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md)

### ADR-v2-0024 — 推論Job・prompt・構造化出力・入口契約

[統合本文を開く](0024-inference-input-output-contract.md) / [旧ADR-0077](../adr/0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)、[旧ADR-0096](../adr/0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)

### ADR-v2-0025 — Context／入力state・commitment・取得単位

[統合本文を開く](0025-context-and-input-commitments.md) / [旧ADR-0078](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)、[旧ADR-0084](../adr/0084-the-ids-ride-the-capture-stays-home.md)、[旧ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)

### ADR-v2-0026 — 正規仕事量・PWU・推論対価・回答の二重計上防止

[統合本文を開く](0026-canonical-work-and-answer-accounting.md) / [旧ADR-0073](../adr/0073-real-demand-work-bears-the-weight.md)、[旧ADR-0117](../adr/0117-a-draw-is-one-forward.md)、[旧ADR-0131](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md)、[旧ADR-0132](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md)、[旧ADR-0144](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)、[旧ADR-0145](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md)、[旧ADR-0148](../adr/0148-the-free-prompt-lane-prices-compute.md)、[旧ADR-0149](../adr/0149-an-attempts-pwu-is-the-derivation.md)、[旧ADR-0205](../adr/0205-a-decoded-answer-is-not-paid-twice.md)

### ADR-v2-0027 — Free-prompt claim・lock・native責任期間

[統合本文を開く](0027-claim-lifecycle-and-liability.md) / [旧ADR-0044](../adr/0044-palw-free-prompt-receipts.md)、[旧ADR-0109](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md)、[旧ADR-0184](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md)

### ADR-v2-0028 — Attempt・ticket・抽選・seed anchor

[統合本文を開く](0028-attempt-ticket-and-seed-anchor.md) / [旧ADR-0071](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md)、[旧ADR-0072](../adr/0072-the-ticket-is-the-execution.md)、[旧ADR-0074](../adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md)、[旧ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md)、[旧ADR-0170](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md)

### ADR-v2-0029 — Remote client・miner操作・署名・host sandbox

[統合本文を開く](0029-remote-client-mining-and-host-sandbox.md) / [旧ADR-0063](../adr/0063-operator-tooling-the-missing-half.md)、[旧ADR-0079](../adr/0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)、[旧ADR-0122](../adr/0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md)

### ADR-v2-0030 — Shard検証・登録時計測・2M実装計画

[統合本文を開く](0030-sharded-verification-and-model-measurement.md) / [旧ADR-0099](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)、[旧ADR-0153](../adr/0153-2m-verification-sharding-plan.md)

### ADR-v2-0031 — Panel独立性・coverage・取得不能と不正の区別

[統合本文を開く](0031-panel-independence-and-unavailability.md) / [旧ADR-0065](../adr/0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)、[旧ADR-0098](../adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)、[旧ADR-0147](../adr/0147-independence-is-drawn-not-declared.md)、[旧ADR-0166](../adr/0166-verifier-unavailability-is-not-producer-fraud.md)

### ADR-v2-0032 — 検証抽選・beacon・認証receipt・paid lock乱数

[統合本文を開く](0032-sampling-beacon-and-receipts.md) / [旧ADR-0028](../adr/0028-palw-challenge-sampling-protocol.md)、[旧ADR-0075](../adr/0075-certification-is-a-consensus-object.md)、[旧ADR-0108](../adr/0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md)、[旧ADR-0185](../adr/0185-paid-lock-randomness-bootstrap-and-paid-sources.md)

### ADR-v2-0033 — 確率的constraint検査とexact courtの分担

[統合本文を開く](0033-probabilistic-checks-and-exact-court.md) / [旧ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md)

### ADR-v2-0034 — 検証span・class別service時計・starvation境界

[統合本文を開く](0034-verification-spans-and-service-clock.md) / [旧ADR-0133](../adr/0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)

### ADR-v2-0035 — Bond・責任予算・共有担保・段階reserve

[統合本文を開く](0035-bond-budget-and-shared-reservations.md) / [旧ADR-0152](../adr/0152-account-stake-staged-reserve-and-vested-rewards.md)、[旧ADR-0160](../adr/0160-claim-capacity-separation.md)、[旧ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)

### ADR-v2-0036 — Claim容量拡張・固定発行予算・breaker

[統合本文を開く](0036-capacity-scaling-and-emission-budget.md) / [旧ADR-0161](../adr/0161-a-claim-is-not-an-emission.md)、[旧ADR-0167](../adr/0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md)

### ADR-v2-0037 — Fee escrow・Panel報酬・verifier／provider同意

[統合本文を開く](0037-fee-escrow-and-role-consent.md) / [旧ADR-0032](../adr/0032-palw-fee-bond-escrow.md)、[旧ADR-0124](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)、[旧ADR-0207](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md)

### ADR-v2-0038 — G14公開訴追・adjudicability・裁定保証の前提

[統合本文を開く](0038-g14-public-prosecution-and-completeness.md) / [旧ADR-0027](../adr/0027-palw-slash-unilateral-fraud-proofs.md)、[旧ADR-0049](../adr/0049-palw-adjudication-contract.md)、[旧ADR-0069](../adr/0069-e2e-adjudicability-is-the-price-of-weight.md)、[旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[旧ADR-0183](../adr/0183-g14-court-capacity-panel-floor-retention-detection.md)、[旧ADR-0187](../adr/0187-g14-native-public-filing-and-post-final-mining-accusation.md)、[旧ADR-0188](../adr/0188-d2-x3-arm-records-composition-and-pause-credit.md)

### ADR-v2-0039 — DA court・claim資料請求・PanelDa期限付き開示

[統合本文を開く](0039-data-availability-court-and-panel-da.md) / [旧ADR-0062](../adr/0062-data-availability-court.md)、[旧ADR-0111](../adr/0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)、[旧ADR-0206](../adr/0206-panel-da-mandatory-disclosure-g14-route.md)

### ADR-v2-0040 — TIR終端証拠・fold opening・局所裁定

[統合本文を開く](0040-tir-terminal-proof-and-local-opening.md) / [旧ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md)、[旧ADR-0085](../adr/0085-the-close-is-assembled-from-what-was-served.md)、[旧ADR-0086](../adr/0086-the-opening-carries-the-fold-not-the-leaves.md)、[旧ADR-0093](../adr/0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)

### ADR-v2-0041 — 長文脈court・sparse read set・dispute分解

[統合本文を開く](0041-long-context-court-decomposition.md) / [旧ADR-0189](../adr/0189-tir-court-read-set-door-options.md)、[旧ADR-0190](../adr/0190-tir-door-sparse-fetch-designated-answers-tenure-split.md)、[旧ADR-0191](../adr/0191-tir-dispute-local-decomposition-long-context.md)

### ADR-v2-0042 — Court処理機会・共有訴追・versioned clock

[統合本文を開く](0042-court-service-and-versioned-clock.md) / [旧ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)、[旧ADR-0193（sequential-witness-filing-clock）](../adr/0193-sequential-witness-filing-clock.md)、[旧ADR-0194](../adr/0194-dormant-g14-integration-and-service-schema.md)

### ADR-v2-0043 — Court応答資格履歴・initial notice境界

[統合本文を開く](0043-court-eligibility-and-initial-notice.md) / [旧ADR-0195](../adr/0195-rooted-court-response-eligibility-history.md)、[旧ADR-0198](../adr/0198-shared-court-initial-notice-boundary.md)

### ADR-v2-0044 — Owner非依存の共有TIR query・認証data

[統合本文を開く](0044-shared-tir-query-data.md) / [旧ADR-0196](../adr/0196-owner-independent-shared-tir-query-data.md)

### ADR-v2-0045 — 終端material受付・非独占提出・readiness opening

[統合本文を開く](0045-terminal-material-and-readiness.md) / [旧ADR-0199](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md)、[旧ADR-0200](../adr/0200-d2-readiness-root-opening-availability.md)

### ADR-v2-0046 — Court終了効果・未解決責任archive・金融解除

[統合本文を開く](0046-court-close-and-liability-archive.md) / [旧ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md)、[旧ADR-0201](../adr/0201-archived-close-qualified-financial-discharge.md)

### ADR-v2-0047 — Court originalの実原資・予約・役割別署名

[統合本文を開く](0047-funded-original-and-role-authorization.md) / [旧ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)

### ADR-v2-0048 — Claim依存関係・独立進行・prefix生成元

[統合本文を開く](0048-claim-dependencies-and-prefix-origin.md) / [旧ADR-0186](../adr/0186-hold-unresolved-liability-advance-independent-claims.md)、[旧ADR-0208](../adr/0208-authenticated-prefix-work-dependencies.md)

### ADR-v2-0049 — 認証state map・IBD row stream・世代別保存

[統合本文を開く](0049-authenticated-state-map-and-row-stream.md) / [旧ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md)、[旧ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)

### ADR-v2-0050 — 設計凍結・延期・撤回・不採用・未採択・変更管理

[統合本文を開く](0050-freeze-deferred-rejected-and-governance.md) / [旧ADR-0009](../adr/0009-dns-probabilistic-finality.md)、[旧ADR-0010](../adr/0010-validator-node-architecture.md)、[旧ADR-0011](../adr/0011-validator-deployment-and-equivocation-safety.md)、[旧ADR-0012](../adr/0012-mainnet-validator-sortition-commit-reveal.md)、[旧ADR-0013](../adr/0013-validator-reward-distribution.md)、[旧ADR-0014](../adr/0014-coordinated-failover-protocol.md)、[旧ADR-0015](../adr/0015-remote-signer-hsm-protocol.md)、[旧ADR-0016](../adr/0016-stake-locked-bond-utxos.md)、[旧ADR-0017](../adr/0017-all-active-staker-attestation.md)、[旧ADR-0018](../adr/0018-quality-gated-stakescore-inclusion-economics.md)、[旧ADR-0021](../adr/0021-palw-llm-pow.md)、[旧ADR-0023](../adr/0023-base-three-lane-execution.md)、[旧ADR-0024](../adr/0024-verified-llm-token-weighted-bft.md)、[旧ADR-0033](../adr/0033-palw-credit-gate-wiring.md)、[旧ADR-0051](../adr/0051-palw-metal-gguf-execution-family.md)、[旧ADR-0080](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md)、[旧ADR-0081](../adr/0081-long-context-the-input-is-a-state-chain.md)、[旧ADR-0126](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md)、[旧ADR-0128](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md)、[旧ADR-0134](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md)、[旧ADR-0173（a-seat-holds-a-root-not-a-class-and-possession-gates-activation）](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md)、[旧ADR-0181](../adr/0181-design-freeze-and-controlled-change.md)、[旧ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md)、[旧ADR-0193（miner-account-bond-for-panel-incentives）](../adr/0193-miner-account-bond-for-panel-incentives.md)
