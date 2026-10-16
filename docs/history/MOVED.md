# MOVED — where the docs/ root files went (2026-10-16 reorganization)

> docs/ の root に平置きされていた 101 個の文書ファイル(`README.md`・`PRINCIPLES.md` を除く)のうち 93 個を、用途別のディレクトリへ `git mv` で移しました。**文書の本文・決定・履歴・番号は変えていません**(変えたのは、移動に伴う相対リンクとパス表記だけです)。`docs/adr/`・`docs/rfc/`・`docs/design/palw/`(既存のファイル)・`docs/spec/palw/` の既存ファイルは動かしていません。

## 読み方と注意

- **old path → new path** の対応表は次のとおりです。`git log --follow` で履歴を追えます。
- 参照は、リポジトリ全体(`md`・`rs`・`py`・`sh`・`toml`・`yml`・`js`・`html` など)で更新しました。Markdown の相対リンクは、移動した側とされた側の両方を新しい位置から計算し直しています。`docs/<name>.md` という形の文字列(コメント・CLI の案内文・スクリプトの既定値)も新しいパスに置き換えました。
- **更新しなかったもの:** (1) `*.json` と `*.patch` の記録(`docs/**/evidence/*.json`、`docs/design/palw/tir/evidence/rfc02-actual-outsider9-final-workspace-preimage.json`、`docs/wiki/wiki-t12-sync.patch` など)。当時の状態の記録で、ハッシュと対になっているため旧パスのままです。(2) ファイル名だけの言及(パスを含まない「`palw-registry-map.md` を見よ」のような文)。ファイル名は変わっていないので、検索すれば見つかります。
- `https://github.com/MISAKA-BTC/misakas/blob/main/docs/...` の URL(`contrib/misakascan-t12/`、`docs/wiki/` など)も新しいパスに書き換えました。**この branch が `main` に入るまでは、その URL は旧パスの `main` を指すため 404 になります。** misakascan の配備済みページは、merge 後に再配備が必要です。
- 移動したファイルは、読み物としての分類です。**現行か過去かの判断がつかないものは動かしていません**(下の「動かさなかったもの」)。

### audit/palw — PALW の監査・検証判定

| old path | new path |
| --- | --- |
| `docs/palw-mainnet-audit-2026-08-28.md` | [`docs/audit/palw/palw-mainnet-audit-2026-08-28.md`](../audit/palw/palw-mainnet-audit-2026-08-28.md) |
| `docs/palw-mainnet-audit-2026-08-30.md` | [`docs/audit/palw/palw-mainnet-audit-2026-08-30.md`](../audit/palw/palw-mainnet-audit-2026-08-30.md) |
| `docs/palw-mainnet-audit-2026-09-05.md` | [`docs/audit/palw/palw-mainnet-audit-2026-09-05.md`](../audit/palw/palw-mainnet-audit-2026-09-05.md) |
| `docs/palw-mainnet-audit-2026-09-06.md` | [`docs/audit/palw/palw-mainnet-audit-2026-09-06.md`](../audit/palw/palw-mainnet-audit-2026-09-06.md) |
| `docs/palw-mainnet-audit3-2026-08-29.md` | [`docs/audit/palw/palw-mainnet-audit3-2026-08-29.md`](../audit/palw/palw-mainnet-audit3-2026-08-29.md) |
| `docs/palw-mainnet-readiness-audit-2026-08-22.md` | [`docs/audit/palw/palw-mainnet-readiness-audit-2026-08-22.md`](../audit/palw/palw-mainnet-readiness-audit-2026-08-22.md) |
| `docs/palw-mainnet-reaudit-2026-08-29.md` | [`docs/audit/palw/palw-mainnet-reaudit-2026-08-29.md`](../audit/palw/palw-mainnet-reaudit-2026-08-29.md) |
| `docs/palw-audit-2026-09-18-6001.md` | [`docs/audit/palw/palw-audit-2026-09-18-6001.md`](../audit/palw/palw-audit-2026-09-18-6001.md) |
| `docs/palw-critical-audit-2026-08-19-ja.md` | [`docs/audit/palw/palw-critical-audit-2026-08-19-ja.md`](../audit/palw/palw-critical-audit-2026-08-19-ja.md) |
| `docs/palw-only-v4-audit-2026-08-17-ja.md` | [`docs/audit/palw/palw-only-v4-audit-2026-08-17-ja.md`](../audit/palw/palw-only-v4-audit-2026-08-17-ja.md) |
| `docs/palw-external-audit-2026-08-21.md` | [`docs/audit/palw/palw-external-audit-2026-08-21.md`](../audit/palw/palw-external-audit-2026-08-21.md) |
| `docs/palw-daa-clock-audit-2026-09-18.md` | [`docs/audit/palw/palw-daa-clock-audit-2026-09-18.md`](../audit/palw/palw-daa-clock-audit-2026-09-18.md) |
| `docs/palw-algo4-forgery-audit-2026-08-16.md` | [`docs/audit/palw/palw-algo4-forgery-audit-2026-08-16.md`](../audit/palw/palw-algo4-forgery-audit-2026-08-16.md) |
| `docs/palw-release-6001-verdict-2026-09-18.md` | [`docs/audit/palw/palw-release-6001-verdict-2026-09-18.md`](../audit/palw/palw-release-6001-verdict-2026-09-18.md) |
| `docs/palw-rc-launch-blockers-2026-08-21.md` | [`docs/audit/palw/palw-rc-launch-blockers-2026-08-21.md`](../audit/palw/palw-rc-launch-blockers-2026-08-21.md) |
| `docs/palw-base0-runtime-review-reconciliation-2026-08-26.md` | [`docs/audit/palw/palw-base0-runtime-review-reconciliation-2026-08-26.md`](../audit/palw/palw-base0-runtime-review-reconciliation-2026-08-26.md) |
| `docs/ambient-pol-binary-audit-2026-08-15.md` | [`docs/audit/palw/ambient-pol-binary-audit-2026-08-15.md`](../audit/palw/ambient-pol-binary-audit-2026-08-15.md) |

### audit/evm — EVM の監査 scope

| old path | new path |
| --- | --- |
| `docs/audit-scope-evm-rpc.md` | [`docs/audit/evm/audit-scope-evm-rpc.md`](../audit/evm/audit-scope-evm-rpc.md) |

### evidence/palw — 実測・drill・bench の記録

| old path | new path |
| --- | --- |
| `docs/palw-aarch64-class-determinism-2026-08-20.md` | [`docs/evidence/palw/palw-aarch64-class-determinism-2026-08-20.md`](../evidence/palw/palw-aarch64-class-determinism-2026-08-20.md) |
| `docs/palw-algo4-crosshost-determinism-2026-08-16.md` | [`docs/evidence/palw/palw-algo4-crosshost-determinism-2026-08-16.md`](../evidence/palw/palw-algo4-crosshost-determinism-2026-08-16.md) |
| `docs/palw-algo4-difficulty-economics-2026-08-16.md` | [`docs/evidence/palw/palw-algo4-difficulty-economics-2026-08-16.md`](../evidence/palw/palw-algo4-difficulty-economics-2026-08-16.md) |
| `docs/palw-base0-depth-measurement-2026-08-21.md` | [`docs/evidence/palw/palw-base0-depth-measurement-2026-08-21.md`](../evidence/palw/palw-base0-depth-measurement-2026-08-21.md) |
| `docs/palw-legs-capture-measurement-2026-08-15.md` | [`docs/evidence/palw/palw-legs-capture-measurement-2026-08-15.md`](../evidence/palw/palw-legs-capture-measurement-2026-08-15.md) |
| `docs/palw-llm-pow-block-generation-2026-08-16.md` | [`docs/evidence/palw/palw-llm-pow-block-generation-2026-08-16.md`](../evidence/palw/palw-llm-pow-block-generation-2026-08-16.md) |
| `docs/palw-qwen25-block-generation-blocked-2026-08-22.md` | [`docs/evidence/palw/palw-qwen25-block-generation-blocked-2026-08-22.md`](../evidence/palw/palw-qwen25-block-generation-blocked-2026-08-22.md) |
| `docs/palw-second-class-weight-2026-08-26.md` | [`docs/evidence/palw/palw-second-class-weight-2026-08-26.md`](../evidence/palw/palw-second-class-weight-2026-08-26.md) |
| `docs/palw-economic-parameters-2026-08-16.md` | [`docs/evidence/palw/palw-economic-parameters-2026-08-16.md`](../evidence/palw/palw-economic-parameters-2026-08-16.md) |
| `docs/palw-economy-studio-drill-2026-09-20.md` | [`docs/evidence/palw/palw-economy-studio-drill-2026-09-20.md`](../evidence/palw/palw-economy-studio-drill-2026-09-20.md) |
| `docs/palw-stage0-fleet-replay-bench-2026-08-16.md` | [`docs/evidence/palw/palw-stage0-fleet-replay-bench-2026-08-16.md`](../evidence/palw/palw-stage0-fleet-replay-bench-2026-08-16.md) |

### security/palw — 脅威モデル

| old path | new path |
| --- | --- |
| `docs/palw-rc-threat-model.md` | [`docs/security/palw/palw-rc-threat-model.md`](../security/palw/palw-rc-threat-model.md) |

### history/testnet-12 — testnet-12 の regenesis・lane・事故記録

| old path | new path |
| --- | --- |
| `docs/testnet-12-regenesis-2026-09-23.md` | [`docs/history/testnet-12/testnet-12-regenesis-2026-09-23.md`](../history/testnet-12/testnet-12-regenesis-2026-09-23.md) |
| `docs/t12-panel-seed-2026-09-25.md` | [`docs/history/testnet-12/t12-panel-seed-2026-09-25.md`](../history/testnet-12/t12-panel-seed-2026-09-25.md) |
| `docs/palw-rc-t12-seat-deadlock-2026-08-22.md` | [`docs/history/testnet-12/palw-rc-t12-seat-deadlock-2026-08-22.md`](../history/testnet-12/palw-rc-t12-seat-deadlock-2026-08-22.md) |

### design/evm — EVM の設計(撤回済みを含む)

| old path | new path |
| --- | --- |
| `docs/misaka-evm-design-v0.4.md` | [`docs/design/evm/misaka-evm-design-v0.4.md`](../design/evm/misaka-evm-design-v0.4.md) |
| `docs/misaka-evm-optimization-design-v0.1.md` | [`docs/design/evm/misaka-evm-optimization-design-v0.1.md`](../design/evm/misaka-evm-optimization-design-v0.1.md) |
| `docs/misaka-evm-state-backend-design-v0.1.md` | [`docs/design/evm/misaka-evm-state-backend-design-v0.1.md`](../design/evm/misaka-evm-state-backend-design-v0.1.md) |
| `docs/misaka-pq-deposit-strict-design-v0.1.md` | [`docs/design/evm/misaka-pq-deposit-strict-design-v0.1.md`](../design/evm/misaka-pq-deposit-strict-design-v0.1.md) |
| `docs/misaka-base-3lane-execution-design-v0.1.md` | [`docs/design/evm/misaka-base-3lane-execution-design-v0.1.md`](../design/evm/misaka-base-3lane-execution-design-v0.1.md) |
| `docs/misaka-compute-token-program-design-v0.1.md` | [`docs/design/evm/misaka-compute-token-program-design-v0.1.md`](../design/evm/misaka-compute-token-program-design-v0.1.md) |

### design/consensus — PQ・hash identity の設計

| old path | new path |
| --- | --- |
| `docs/kaspa-pq-design-mldsa87.md` | [`docs/design/consensus/kaspa-pq-design-mldsa87.md`](../design/consensus/kaspa-pq-design-mldsa87.md) |
| `docs/hash64-migration-inventory.md` | [`docs/design/consensus/hash64-migration-inventory.md`](../design/consensus/hash64-migration-inventory.md) |

### design/palw — 旧 PALW の設計・計画・撤回済み設計(既存の design/palw/ に追加。既存ファイルは動かしていない)

| old path | new path |
| --- | --- |
| `docs/palw-base0-ptq-pipeline-scope.md` | [`docs/design/palw/palw-base0-ptq-pipeline-scope.md`](../design/palw/palw-base0-ptq-pipeline-scope.md) |
| `docs/palw-full-logits-trace-v2-design.md` | [`docs/design/palw/palw-full-logits-trace-v2-design.md`](../design/palw/palw-full-logits-trace-v2-design.md) |
| `docs/palw-private-prompts-design-2026-09-05.md` | [`docs/design/palw/palw-private-prompts-design-2026-09-05.md`](../design/palw/palw-private-prompts-design-2026-09-05.md) |
| `docs/palw-practical-runtime-plan-2026-08-26.md` | [`docs/design/palw/palw-practical-runtime-plan-2026-08-26.md`](../design/palw/palw-practical-runtime-plan-2026-08-26.md) |
| `docs/palw-road-to-mainnet-2026-08-21.md` | [`docs/design/palw/palw-road-to-mainnet-2026-08-21.md`](../design/palw/palw-road-to-mainnet-2026-08-21.md) |
| `docs/palw-two-class-plan-2026-08-21.md` | [`docs/design/palw/palw-two-class-plan-2026-08-21.md`](../design/palw/palw-two-class-plan-2026-08-21.md) |
| `docs/palw-position-benefits-consumer-p-b2-2026-09-24-ja.md` | [`docs/design/palw/palw-position-benefits-consumer-p-b2-2026-09-24-ja.md`](../design/palw/palw-position-benefits-consumer-p-b2-2026-09-24-ja.md) |
| `docs/palw-round-exec-additional-acceptance-2026-10-10.md` | [`docs/design/palw/palw-round-exec-additional-acceptance-2026-10-10.md`](../design/palw/palw-round-exec-additional-acceptance-2026-10-10.md) |
| `docs/misaka-palw-slash-protocol-design-v0.1.md` | [`docs/design/palw/misaka-palw-slash-protocol-design-v0.1.md`](../design/palw/misaka-palw-slash-protocol-design-v0.1.md) |
| `docs/misaka-palw-pow-detailed-design-v0.1-ja.md` | [`docs/design/palw/misaka-palw-pow-detailed-design-v0.1-ja.md`](../design/palw/misaka-palw-pow-detailed-design-v0.1-ja.md) |
| `docs/misaka-palw-ota-secure-update-design-v0.1-ja.md` | [`docs/design/palw/misaka-palw-ota-secure-update-design-v0.1-ja.md`](../design/palw/misaka-palw-ota-secure-update-design-v0.1-ja.md) |
| `docs/misaka-palw-vps-canonical-worker-design-v0.1-ja.md` | [`docs/design/palw/misaka-palw-vps-canonical-worker-design-v0.1-ja.md`](../design/palw/misaka-palw-vps-canonical-worker-design-v0.1-ja.md) |
| `docs/misaka-audit-emission-v0.2-design.md` | [`docs/design/palw/misaka-audit-emission-v0.2-design.md`](../design/palw/misaka-audit-emission-v0.2-design.md) |
| `docs/palw-fp-on-registered-classes.md` | [`docs/design/palw/palw-fp-on-registered-classes.md`](../design/palw/palw-fp-on-registered-classes.md) |
| `docs/palw-fp-wiring-atomicity.md` | [`docs/design/palw/palw-fp-wiring-atomicity.md`](../design/palw/palw-fp-wiring-atomicity.md) |
| `docs/palw-qwen25-class-phase0.md` | [`docs/design/palw/palw-qwen25-class-phase0.md`](../design/palw/palw-qwen25-class-phase0.md) |
| `docs/palw-model-fit-held-2026-09-11.md` | [`docs/design/palw/palw-model-fit-held-2026-09-11.md`](../design/palw/palw-model-fit-held-2026-09-11.md) |
| `docs/palw-seat-coverage-2026-09-10.md` | [`docs/design/palw/palw-seat-coverage-2026-09-10.md`](../design/palw/palw-seat-coverage-2026-09-10.md) |
| `docs/palw-shard-plan-2026-09-10.md` | [`docs/design/palw/palw-shard-plan-2026-09-10.md`](../design/palw/palw-shard-plan-2026-09-10.md) |

### operations/models — モデルの追加・認証・運用

| old path | new path |
| --- | --- |
| `docs/palw-add-a-model-runbook.md` | [`docs/operations/models/palw-add-a-model-runbook.md`](../operations/models/palw-add-a-model-runbook.md) |
| `docs/palw-certify-a-new-model.md` | [`docs/operations/models/palw-certify-a-new-model.md`](../operations/models/palw-certify-a-new-model.md) |
| `docs/palw-model-onboarding-sdk.md` | [`docs/operations/models/palw-model-onboarding-sdk.md`](../operations/models/palw-model-onboarding-sdk.md) |
| `docs/model-requests.md` | [`docs/operations/models/model-requests.md`](../operations/models/model-requests.md) |
| `docs/misaka-palw-model-adjudicability-guide-v0.1-ja.md` | [`docs/operations/models/misaka-palw-model-adjudicability-guide-v0.1-ja.md`](../operations/models/misaka-palw-model-adjudicability-guide-v0.1-ja.md) |
| `docs/palw-public-testnet-classes-runbook.md` | [`docs/operations/models/palw-public-testnet-classes-runbook.md`](../operations/models/palw-public-testnet-classes-runbook.md) |
| `docs/palw-step-space-deployment-notes.md` | [`docs/operations/models/palw-step-space-deployment-notes.md`](../operations/models/palw-step-space-deployment-notes.md) |
| `docs/palw-extension-envelope.md` | [`docs/operations/models/palw-extension-envelope.md`](../operations/models/palw-extension-envelope.md) |

### operations/evm — EVM backend と Ethereum tooling

| old path | new path |
| --- | --- |
| `docs/misaka-evm-flat-backend-runbook-v0.1.md` | [`docs/operations/evm/misaka-evm-flat-backend-runbook-v0.1.md`](../operations/evm/misaka-evm-flat-backend-runbook-v0.1.md) |
| `docs/connecting-ethereum-tooling.md` | [`docs/operations/evm/connecting-ethereum-tooling.md`](../operations/evm/connecting-ethereum-tooling.md) |

### operations/pq — PQ 検証

| old path | new path |
| --- | --- |
| `docs/kaspa-pq-mldsa87-verification-runbook.md` | [`docs/operations/pq/kaspa-pq-mldsa87-verification-runbook.md`](../operations/pq/kaspa-pq-mldsa87-verification-runbook.md) |

### operations/validator

| old path | new path |
| --- | --- |
| `docs/validator-runbook.md` | [`docs/operations/validator/validator-runbook.md`](../operations/validator/validator-runbook.md) |

### operations/mainnet — mainnet readiness と certification

| old path | new path |
| --- | --- |
| `docs/mainnet-readiness.md` | [`docs/operations/mainnet/mainnet-readiness.md`](../operations/mainnet/mainnet-readiness.md) |
| `docs/mainnet-palw-certification-runbook.md` | [`docs/operations/mainnet/mainnet-palw-certification-runbook.md`](../operations/mainnet/mainnet-palw-certification-runbook.md) |

### operations/testnet-12 — 現行 network の参加・launch 文書

| old path | new path |
| --- | --- |
| `docs/testnet12-join-mining.md` | [`docs/operations/testnet-12/testnet12-join-mining.md`](../operations/testnet-12/testnet12-join-mining.md) |
| `docs/t12-launch-2026-09-25.md` | [`docs/operations/testnet-12/t12-launch-2026-09-25.md`](../operations/testnet-12/t12-launch-2026-09-25.md) |
| `docs/t12-rcore-launch-checklist.md` | [`docs/operations/testnet-12/t12-rcore-launch-checklist.md`](../operations/testnet-12/t12-rcore-launch-checklist.md) |

### operations/tools — probe・address check

| old path | new path |
| --- | --- |
| `docs/node-liveness-probe.md` | [`docs/operations/tools/node-liveness-probe.md`](../operations/tools/node-liveness-probe.md) |
| `docs/misaka-probe-usage-ja.md` | [`docs/operations/tools/misaka-probe-usage-ja.md`](../operations/tools/misaka-probe-usage-ja.md) |
| `docs/misaka-probe-dev-handoff-ja.md` | [`docs/operations/tools/misaka-probe-dev-handoff-ja.md`](../operations/tools/misaka-probe-dev-handoff-ja.md) |
| `docs/misaka-miner-address-check-ja.md` | [`docs/operations/tools/misaka-miner-address-check-ja.md`](../operations/tools/misaka-miner-address-check-ja.md) |

### operations — release・パラメータ上書き

| old path | new path |
| --- | --- |
| `docs/override-params.md` | [`docs/operations/override-params.md`](../operations/override-params.md) |
| `docs/release-process.md` | [`docs/operations/release-process.md`](../operations/release-process.md) |

### reference/palw — registry map・artifact 記録

| old path | new path |
| --- | --- |
| `docs/palw-registry-map.md` | [`docs/reference/palw/palw-registry-map.md`](../reference/palw/palw-registry-map.md) |
| `docs/palw-derived-artifacts.md` | [`docs/reference/palw/palw-derived-artifacts.md`](../reference/palw/palw-derived-artifacts.md) |
| `docs/qwen25-a16-2m-held-artifact.md` | [`docs/reference/palw/qwen25-a16-2m-held-artifact.md`](../reference/palw/qwen25-a16-2m-held-artifact.md) |
| `docs/qwen36-2m-held-artifact.md` | [`docs/reference/palw/qwen36-2m-held-artifact.md`](../reference/palw/qwen36-2m-held-artifact.md) |

### reference/evm — 互換性 matrix・wallet profile

| old path | new path |
| --- | --- |
| `docs/evm-differences-from-ethereum.md` | [`docs/reference/evm/evm-differences-from-ethereum.md`](../reference/evm/evm-differences-from-ethereum.md) |
| `docs/ethereum-rpc-compat-matrix.md` | [`docs/reference/evm/ethereum-rpc-compat-matrix.md`](../reference/evm/ethereum-rpc-compat-matrix.md) |
| `docs/misaka-evm-wallet-profile-v1.md` | [`docs/reference/evm/misaka-evm-wallet-profile-v1.md`](../reference/evm/misaka-evm-wallet-profile-v1.md) |

### reference — components manifest

| old path | new path |
| --- | --- |
| `docs/components-manifest.md` | [`docs/reference/components-manifest.md`](../reference/components-manifest.md) |

### spec — kaspa-pq の仕様(`spec/palw/` の章は動かしていない)

| old path | new path |
| --- | --- |
| `docs/kaspa-pq-spec.md` | [`docs/spec/kaspa-pq-spec.md`](../spec/kaspa-pq-spec.md) |

### testing — test plan

| old path | new path |
| --- | --- |
| `docs/test-plan-kaspa-pq.md` | [`docs/testing/test-plan-kaspa-pq.md`](../testing/test-plan-kaspa-pq.md) |

## 動かさなかったもの(docs/ root に残した 8 個)

| file | 理由 |
| --- | --- |
| [`docs/archival.md`](../archival.md) | calibration corpus: scripts/genesis/genesis_registry_build.py, scripts/genesis/genesis8_models.json and tests/hf-onboarding/onboard.sh read docs/archival.md by path (pinned git blob), and docs/evidence/hf-onboarding/*/conversion.json record it |
| [`docs/crescendo-guide.md`](../crescendo-guide.md) | calibration corpus, same as archival.md (docs/crescendo-guide.md is pinned by path and blob in the genesis and hf-onboarding tooling) |
| [`docs/misaka-prea-design-v1.1.md`](../misaka-prea-design-v1.1.md) | calibration corpus: tests/hf-onboarding/models.json and docs/evidence/hf-onboarding/*/model.json name docs/misaka-prea-design-v1.1.md as an input document |
| [`docs/palw-stage0-shadow-drill-runbook.md`](../palw-stage0-shadow-drill-runbook.md) | a Stage-0 runbook (ADR-0029 §5); may still be runnable, so not classified as history |
| [`docs/palw-freeprompt-gateway.md`](../palw-freeprompt-gateway.md) | operator guide for the free-prompt gateway (ADR-0044); currency after ADR-0077/0144 unknown |
| [`docs/palw-court-round-trip-drill.md`](../palw-court-round-trip-drill.md) | drill / acceptance record; reads like both a runbook and evidence |
| [`docs/palw-class-activation-gate-status.md`](../palw-class-activation-gate-status.md) | a status ledger opened 2026-08-16; unknown whether still maintained |
| [`docs/palw-claim-capacity-map-2026-09-26.md`](../palw-claim-capacity-map-2026-09-26.md) | paired with the data directory palw-claim-capacity-map-2026-09-26/ ("committed beside this file"); kept together in place |

root に残るのは上の表のほか、`README.md`・`PRINCIPLES.md`・`STATUS.md` です。root のディレクトリ(`capacity/`・`evidence-qwen36-model-gate/`・`explorer/`・`extension-manifests/`・`fp-job-v4/`・`model-manifests/`・`openai-surface/`・`palw-claim-capacity-map-2026-09-26/`・`serving/`・`tools/`・`transcription-sources/`・`wiki/`)は動かしていません。`extension-manifests/`・`openai-surface/` は Rust の試験が実行時にパスで読むため動かせません。

## `docs/wiki/` と `getting-started/` / `operations/` の重複候補(何も削除していません。判断は後で)

各行の割合は、行単位の一致(25 字以上の行)の割合で、wiki 側の行のうち相手に同じ行がある割合 / 相手側の行のうち wiki に同じ行がある割合です。

| wiki のページ | 重なる文書 | 一致 |
| --- | --- | --- |
| `wiki/Testnet-11-Verification-Participation-JA.md` | [`history/testnet11-verification-participation-ja.md`](testnet11-verification-participation-ja.md) | 49% / 55% — 実質的に同じ内容の写し |
| `wiki/Testnet-11-Operator-UI-JA.md` | [`history/testnet11-join-mining.md`](testnet11-join-mining.md) | 33% / 17% |
| `wiki/README.md` | [`getting-started/validator.md`](../getting-started/validator.md)、[`history/README-2026-10-12.md`](README-2026-10-12.md) | 46% / 12%、46% / 1% — 旧 root README の写しの部分 |
| `wiki/Quick-Start.md` | [`operations/testnet-12/testnet12-join-mining.md`](../operations/testnet-12/testnet12-join-mining.md) | 26% / 4% — testnet-12 の build と参加手順 |
| `wiki/Testnet-12-Operator-UI-JA.md` | [`operations/testnet-12/testnet12-join-mining.md`](../operations/testnet-12/testnet12-join-mining.md) | 21% / 3% |
| `wiki/Operations-Notes.md` | [`operations/testnet-12/t12-launch-2026-09-25.md`](../operations/testnet-12/t12-launch-2026-09-25.md) | 18% / 9% |
| `wiki/Adding-a-Model-JA.md` | [`operations/models/palw-add-a-model-runbook.md`](../operations/models/palw-add-a-model-runbook.md) | 行は一致しないが、ページ自身が「詳しい手順の正本は runbook」と述べる要約 |
| `wiki/EVM-Pruned-Node.md` | [`operations/evm/misaka-evm-flat-backend-runbook-v0.1.md`](../operations/evm/misaka-evm-flat-backend-runbook-v0.1.md) | 行は一致しないが、ページが runbook を正本として参照する要約 |

`wiki/` は GitHub wiki の写し(`wiki/README.md` に同期方法がある)なので、重複の扱いは wiki 側の運用と合わせて決める必要があります。`getting-started/build.md`・`run-node.md`・`validator.md`・`testing.md` は旧 root README(2026-10-12 まで)から移した手順で、testnet-11 の例を含みます(各ファイルの冒頭に注記があります)。
