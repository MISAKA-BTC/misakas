# RFC/ADR mission alignment audit — 2026-10-07

> **2026-10-10改定:** RFC05 の VM と RFC14 §16 の MISAKA Torrent は不採用理由だけの記述へ整理した。以下の snapshot・inventory は監査当時の版を記録する。現行の採用方針は各 RFC と索引を参照する。

## 結果

[ADR-0173](../0173-public-verifier-dispute-completeness-is-misaka-purpose.md)で、**普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeし、objective convictionまで完結すること**をMISAKAの中核目標として採択した。

RFC **15件**、ADR **91文書**を改定し、両indexを更新した。旧実装の記録と設計上の新しい優先条件を区別する。Panel=0はRFC14の全completion gatesとRFC15固有gateが成立するまで延期する。実装・runtime・activationは変更していない。

## 確認範囲と版

| 項目 | 対象 |
| --- | --- |
| 公開source | `MISAKA-BTC/misakas` main `43f0bcb362d37cba79414940f3fdd368d40d3377` |
| workspace runtime HEAD | `e060f614aa641f9b05e0fdc28725db4aeb5480d9`。公開mainのruntimeへ更新していない |
| 公開mainのRFC/ADRツリー | 182ファイル。recursiveなMarkdown・JSON・JSを含む |
| 追加のlocal文書 | RFC14、RFC15、旧filenameのADR0026。その他のlocal差分版も確認 |
| effective baseline | 185 paths、うちMarkdown 180。異なる版を含め195 file versionsを全文走査 |
| numbered RFC/ADR | RFC 15文書、ADR 159文書（158番号、ADR0026の2 filenameを含む） |
| workspace文書 | 既存18文書を選択版として保持。未配置の公開文書167件をreview snapshotへ取り込む |

全available file versionの内容を走査し、Panel/quorum、public prosecution、証拠取得・private state、admission、permission、sampling、activation/retention、VM/TEE信頼境界を抽出した。そこから規範的条項と関連設計を確認し、下表に各ファイルの処置を記録した。過去のbenchmark・STATUS-AUDIT・evidenceの値や旧署名方式の履歴を書き換えて最新結果にはしていない。この確認は形式証明や全実装の検証ではない。

workspaceにあった旧版を上流版で上書きしていない。特にlocal RFC9とADR0026旧filenameを保持した。両方の内容が異なる場合には選択版と公開版をそれぞれ確認し、共通の新しい方針をamendmentで優先した。sourceに実体のないADR0152/0160などの参照は未収録のままであり、本文を推測して補っていない。

## 衝突していた主要な設計条項

| 対象 | 旧記述の問題 | 改定 |
| --- | --- |
| ADR0077 Decision16 / PanelDa transport、ADR0044 | producer・Panel・既存courtのchallengerだけの閲覧権では外部の不正発見を塞ぎ得る | 必要materialの公開取得と認証、または別途承認された同等の公開証明経路を新profileの条件にする |
| ADR0084/0085/0086/0103/0111/0121、RFC6 | capture/FOLD/state/closeがproducerにしか生成できない可能性。leaf番号やcourt kernelだけでは不足 | non-seatの取得・局所化・dissection・terminal proof・非開示裁定を受入に追加 |
| ADR0069/0070/0075/0135/0145、RFC2/3/5/11/13 | catalog、certificate、source conformance、ready seatsだけでは外部訴追を証明しない | canonical最大profileのfresh public-bond E2Eを新しい報酬・weight条件に追加 |
| RFC2のstatic admission/self-test、RFC6のtitle/概要/summary、RFC7 G7 | static gateだけでadjudicable、dishonest seatは自分だけ害する、assigned seatの存在や既存courtで必ず裁けるという読み方、監査主体をbonded seatに限定 | static feasibilityと独立prosecutionを区別し、scope別の客観的責任へ修正。認証material取得・検査をlemmaの条件にし、普通の非Panel public bondへ主体を拡張 |
| ADR0098、ADR0124、ADR0147、RFC7/10 | fault ledgerで停止する旧経路、quorum・outsiderへの過剰な期待 | bounded courtへ進める設計、署名scope別の客観的責任、検出確率とconvictionの分離 |
| ADR0171/0172、RFC4/5/7/11 | public proof/localizerへの接続を通常checkerやkernelの存在から推論し得る | 通常の確率的検査を維持し、独立public prosecutionを別gateにする。VM/TEE/BFT authorityの撤回を維持 |
| ADR0144 | 有用なlocal inferenceという目的だけでは安全性の中核条件が明示されない | その用途を保ち、ADR173のpublic-verifier dispute completenessを中核目標として追加 |
| ADR0061、RFC12/14/15 | zero-seat genesisとPanel=0の混同、移行中のPanelを永久必須と読む余地 | 両者を区別。固定Panel撤去は全RFC14 gateとRFC15固有gate成立後のみ |

ADRの旧bodyを保持するのはADR indexに記載された履歴保存方式に従う。先頭bannerが衝突の存在と優先条項を知らせ、末尾のdated amendmentが実装方針・受入条件を改定する。RFCの将来設計は同じ優先条件を加え、上記の直接的な過大主張も修正した。

## コード確認と完成を主張しない理由

公開mainの以下の実装を照合した。リンクはimmutable source commitに固定しており、workspaceの旧runtimeにこのコードがあるとは扱わない。

| 位置 | この確認での意味 |
| --- | --- |
| [court opening](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_court_v2.rs#L333) / [ExecutorRefuted](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_offence_attribution_v1.rs#L1996) | 客観的証拠の土台。public materialから全profileの証拠を作れることは別問題 |
| [conviction state transition](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L23107) / [producer slash](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L26728) | licensed claimもvoid/slash、Finalは責任期間内のreversalを扱う。証拠生成とcollectible collateralを別に確認する必要がある |
| [non-seat operator DA](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L27) | rootsが開く自己整合的な偽traceはDA allegationに答え得る。DAだけで算術convictionが成立しない |
| [held prover gaps](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held.rs#L5) / [fresh verifier E2E limitations](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held_e2e.rs#L12) | held-8kのdense量、FOLD prefix、committed fused tileに独立生成のgap。liar instance由来ServedViewの代用をgate成功としない |
| [open-session refusal](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_offence_attribution_v1.rs#L2063) / [current basis](https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L1003) | RFC14の競合court処理とRFC15の別lifecycle移行が必要。現在のPanel条件を0へ設定するだけでは成立しない |

今回の変更は文書だけであり、Rustの追加試験やpublic-bond攻撃drillを実行していない。既存の小さいcourt/DA fixtureの成功は、そのmoduleの能力の証拠であり、RFC14全gateの達成、Panel=0、9B/2M全profileの安全性証明ではない。

## 検証と再現性

* 選択した185 baseline pathsをすべて下表とmachine inventoryへ対応付ける。各版のSHA-256、選択元、before/after、topic別処置、直接修正を記録する。
* dated amendment以外のADR本文が保持され、RFCの変更が記録した直接修正・banner・amendmentだけであることを比較する。
* 追加Markdown linkの存在、追加blockのfence、RFC14/15番号登録とnext-free16、ADR173の参照、Panel=0の禁止条件を確認する。既存の未収録Spec/ADRなどのbroken linkは新しい実装や架空本文で埋めない。

[Machine-readable inventory](0173-mission-alignment-inventory-2026-10-07.json)には全file versionのhashと処置を記録する。[Validation results](0173-mission-alignment-validation-2026-10-07.json)は文書整合性の検証結果であり、protocol completionの証拠ではない。

## 全ファイルの処置

| Baseline document / evidence | 選択版 | 処置 |
| --- | --- |
| [0001-network-isolation.md](../0001-network-isolation.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0002-mldsa65-p2pkh.md](../0002-mldsa65-p2pkh.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0003-lthash-utxo-accumulator.md](../0003-lthash-utxo-accumulator.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0004-utxo-commitment64.md](../0004-utxo-commitment64.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0005-mass-policy.md](../0005-mass-policy.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0006-rpc-wasm-sdk-types.md](../0006-rpc-wasm-sdk-types.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0007-layered-pow.md](../0007-layered-pow.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0008-hash64-consensus-identity.md](../0008-hash64-consensus-identity.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0009-dns-probabilistic-finality.md](../0009-dns-probabilistic-finality.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0010-validator-node-architecture.md](../0010-validator-node-architecture.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0011-validator-deployment-and-equivocation-safety.md](../0011-validator-deployment-and-equivocation-safety.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0012-mainnet-validator-sortition-commit-reveal.md](../0012-mainnet-validator-sortition-commit-reveal.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0013-validator-reward-distribution.md](../0013-validator-reward-distribution.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0014-coordinated-failover-protocol.md](../0014-coordinated-failover-protocol.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0015-remote-signer-hsm-protocol.md](../0015-remote-signer-hsm-protocol.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0016-stake-locked-bond-utxos.md](../0016-stake-locked-bond-utxos.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0017-all-active-staker-attestation.md](../0017-all-active-staker-attestation.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0018-quality-gated-stakescore-inclusion-economics.md](../0018-quality-gated-stakescore-inclusion-economics.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0019-mldsa87-migration.md](../0019-mldsa87-migration.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0020-selected-parent-evm-lane.md](../0020-selected-parent-evm-lane.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0021-palw-llm-pow.md](../0021-palw-llm-pow.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0022-pruned-ibd-evm-overlay-snapshot.md](../0022-pruned-ibd-evm-overlay-snapshot.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0023-base-three-lane-execution.md](../0023-base-three-lane-execution.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0024-verified-llm-token-weighted-bft.md](../0024-verified-llm-token-weighted-bft.md) | 公開main | 基盤/旧役割と整合、変更不要 |
| [0025-chain-participation-and-ibd-candidate-selection.md](../0025-chain-participation-and-ibd-candidate-selection.md) | local保持 | 基盤/旧役割と整合、変更不要 |
| [0026-current-palw-testnet11-architecture.md](../0026-current-palw-testnet11-architecture.md) | local保持 | banner＋規範的amendment、旧body保持 |
| [0026-palw-v2-runtime-separated-verification.md](../0026-palw-v2-runtime-separated-verification.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0027-palw-slash-unilateral-fraud-proofs.md](../0027-palw-slash-unilateral-fraud-proofs.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0028-palw-challenge-sampling-protocol.md](../0028-palw-challenge-sampling-protocol.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0029-palw-chain-carriage.md](../0029-palw-chain-carriage.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0030-palw-step-function-shape-profile.md](../0030-palw-step-function-shape-profile.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0031-palw-canonical-transcendentals.md](../0031-palw-canonical-transcendentals.md) | 公開main | 個別規則と整合、変更不要 |
| [0032-palw-fee-bond-escrow.md](../0032-palw-fee-bond-escrow.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0033-palw-credit-gate-wiring.md](../0033-palw-credit-gate-wiring.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0034-palw-execution-class-model-band-routing.md](../0034-palw-execution-class-model-band-routing.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0035-palw-public-testnet-strategy.md](../0035-palw-public-testnet-strategy.md) | 公開main | 個別規則と整合、変更不要 |
| [0036-palw-mainnet-activation-model.md](../0036-palw-mainnet-activation-model.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0037-palw-async-job-state-machine.md](../0037-palw-async-job-state-machine.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0038-palw-is-the-consensus-work.md](../0038-palw-is-the-consensus-work.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0039-palw-only-block-production.md](../0039-palw-only-block-production.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0040-palw-base-0-integer-arithmetic.md](../0040-palw-base-0-integer-arithmetic.md) | 公開main | 個別規則と整合、変更不要 |
| [0041-palw-pruning-proof-verification.md](../0041-palw-pruning-proof-verification.md) | 公開main | 個別規則と整合、変更不要 |
| [0042-palw-mainnet-candidate-ruleset.md](../0042-palw-mainnet-candidate-ruleset.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0043-palw-v2-state-root-ordering.md](../0043-palw-v2-state-root-ordering.md) | 公開main | 個別規則と整合、変更不要 |
| [0044-palw-free-prompt-receipts.md](../0044-palw-free-prompt-receipts.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0045-palw-class-economy-on-chain.md](../0045-palw-class-economy-on-chain.md) | 公開main | 個別規則と整合、変更不要 |
| [0046-palw-v2-consensus-object-carriage.md](../0046-palw-v2-consensus-object-carriage.md) | 公開main | 個別規則と整合、変更不要 |
| [0047-palw-a16-activation-tier.md](../0047-palw-a16-activation-tier.md) | 公開main | 個別規則と整合、変更不要 |
| [0049-palw-adjudication-contract.md](../0049-palw-adjudication-contract.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0050-palw-base0-residual-site.md](../0050-palw-base0-residual-site.md) | 公開main | 個別規則と整合、変更不要 |
| [0051-palw-metal-gguf-execution-family.md](../0051-palw-metal-gguf-execution-family.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0052-palw-qwen36-hybrid-class.md](../0052-palw-qwen36-hybrid-class.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0053-palw-one-execution-family.md](../0053-palw-one-execution-family.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0054-palw-share-follows-production.md](../0054-palw-share-follows-production.md) | 公開main | 個別規則と整合、変更不要 |
| [0055-palw-position-is-earned-not-declared.md](../0055-palw-position-is-earned-not-declared.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0056-palw-permissionless-class-admission-and-share-economy.md](../0056-palw-permissionless-class-admission-and-share-economy.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0057-palw-base0-runtime-acceleration.md](../0057-palw-base0-runtime-acceleration.md) | 公開main | 個別規則と整合、変更不要 |
| [0058-palw-merged-work-is-counted.md](../0058-palw-merged-work-is-counted.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0059-the-10b-premine-cap.md](../0059-the-10b-premine-cap.md) | 公開main | 個別規則と整合、変更不要 |
| [0060-the-liveness-doctrine.md](../0060-the-liveness-doctrine.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0061-zero-seat-genesis-and-right-sized-collateral.md](../0061-zero-seat-genesis-and-right-sized-collateral.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0062-data-availability-court.md](../0062-data-availability-court.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0063-operator-tooling-the-missing-half.md](../0063-operator-tooling-the-missing-half.md) | 公開main | 個別規則と整合、変更不要 |
| [0064-trustless-recovery-from-a-total-stop.md](../0064-trustless-recovery-from-a-total-stop.md) | 公開main | 個別規則と整合、変更不要 |
| [0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md](../0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md](../0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) | 公開main | 個別規則と整合、変更不要 |
| [0067-classes-are-chain-data-kernels-are-the-build.md](../0067-classes-are-chain-data-kernels-are-the-build.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0068-the-llm-primary-economy-and-the-floors-minimum.md](../0068-the-llm-primary-economy-and-the-floors-minimum.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0069-e2e-adjudicability-is-the-price-of-weight.md](../0069-e2e-adjudicability-is-the-price-of-weight.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0070-the-model-tiers-step-spaces-are-adjudicable.md](../0070-the-model-tiers-step-spaces-are-adjudicable.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0071-the-attempt-lanes-price-and-the-tickets-bound.md](../0071-the-attempt-lanes-price-and-the-tickets-bound.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0072-the-ticket-is-the-execution.md](../0072-the-ticket-is-the-execution.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0073-real-demand-work-bears-the-weight.md](../0073-real-demand-work-bears-the-weight.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0074-the-attempt-is-a-claim-drawn-by-the-chain.md](../0074-the-attempt-is-a-claim-drawn-by-the-chain.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0075-certification-is-a-consensus-object.md](../0075-certification-is-a-consensus-object.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md](../0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) | 公開main | 個別規則と整合、変更不要 |
| [0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md](../0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0078-what-was-made-from-it-is-committed-the-thing-never-rides.md](../0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md](../0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0080-the-answer-is-long-the-verified-unit-is-short.md](../0080-the-answer-is-long-the-verified-unit-is-short.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0081-long-context-the-input-is-a-state-chain.md](../0081-long-context-the-input-is-a-state-chain.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0082-the-close-is-flat-in-the-context.md](../0082-the-close-is-flat-in-the-context.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0083-the-difficulty-window-counts-only-rows-priced-by-bits.md](../0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) | 公開main | 個別規則と整合、変更不要 |
| [0084-the-ids-ride-the-capture-stays-home.md](../0084-the-ids-ride-the-capture-stays-home.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0085-the-close-is-assembled-from-what-was-served.md](../0085-the-close-is-assembled-from-what-was-served.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0086-the-opening-carries-the-fold-not-the-leaves.md](../0086-the-opening-carries-the-fold-not-the-leaves.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md](../0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | 公開main | 個別規則と整合、変更不要 |
| [0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | 公開main | 個別規則と整合、変更不要 |
| [0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | 公開main | 個別規則と整合、変更不要 |
| [0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md](../0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | 公開main | 個別規則と整合、変更不要 |
| [0091-the-reward-buys-the-pair-and-no-holder-is-paid.md](../0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) | 公開main | 個別規則と整合、変更不要 |
| [0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md](../0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md](../0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md](../0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | 公開main | 個別規則と整合、変更不要 |
| [0095-a-position-is-a-membership-not-an-income.md](../0095-a-position-is-a-membership-not-an-income.md) | 公開main | 個別規則と整合、変更不要 |
| [0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md](../0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md](../0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md](../0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md](../0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md](../0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | 公開main | 個別規則と整合、変更不要 |
| [0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md](../0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) | 公開main | 個別規則と整合、変更不要 |
| [0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md](../0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0105-a-heartbeat-never-turns-a-bonded-block-red.md](../0105-a-heartbeat-never-turns-a-bonded-block-red.md) | 公開main | 個別規則と整合、変更不要 |
| [0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md](../0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0107-a-share-grows-on-work-that-reached-final.md](../0107-a-share-grows-on-work-that-reached-final.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md](../0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md](../0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md](../0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md](../0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md](../0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md](../0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) | 公開main | 個別規則と整合、変更不要 |
| [0115-a-pending-transaction-is-announced-until-it-lands.md](../0115-a-pending-transaction-is-announced-until-it-lands.md) | 公開main | 個別規則と整合、変更不要 |
| [0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md](../0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0117-a-draw-is-one-forward.md](../0117-a-draw-is-one-forward.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md](../0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md](../0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md](../0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | 公開main | 個別規則と整合、変更不要 |
| [0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md](../0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md](../0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0123-the-epoch-progressively-releases-unused-class-budget.md](../0123-the-epoch-progressively-releases-unused-class-budget.md) | 公開main | 個別規則と整合、変更不要 |
| [0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md](../0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md](../0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md](../0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md](../0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0129-a-double-spend-needs-the-anchors-not-the-blocks.md](../0129-a-double-spend-needs-the-anchors-not-the-blocks.md) | 公開main | 個別規則と整合、変更不要 |
| [0130-bps1-is-hardened-before-it-is-widened.md](../0130-bps1-is-hardened-before-it-is-widened.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md](../0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md](../0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0134-testnet-11-subnetwork-census-2026-09-17.json](../0134-testnet-11-subnetwork-census-2026-09-17.json) | 公開main | 個別規則と整合、変更不要 |
| [0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md](../0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) | 公開main | 個別規則と整合、変更不要 |
| [0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md](../0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md](../0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) | 公開main | 個別規則と整合、変更不要 |
| [0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md](../0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0138-the-daa-score-is-the-anchors-clock.md](../0138-the-daa-score-is-the-anchors-clock.md) | 公開main | 個別規則と整合、変更不要 |
| [0139-the-execution-lanes-gas-is-one-budget-a-round.md](../0139-the-execution-lanes-gas-is-one-budget-a-round.md) | 公開main | 個別規則と整合、変更不要 |
| [0140-the-heartbeat-is-the-emergency-generator.md](../0140-the-heartbeat-is-the-emergency-generator.md) | 公開main | 個別規則と整合、変更不要 |
| [0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md](../0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md](../0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) | 公開main | 個別規則と整合、変更不要 |
| [0143-an-artifact-root-has-one-owner-on-the-chain.md](../0143-an-artifact-root-has-one-owner-on-the-chain.md) | 公開main | 個別規則と整合、変更不要 |
| [0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md](../0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0145-canonical-work-is-derived-and-admission-is-earned.md](../0145-canonical-work-is-derived-and-admission-is-earned.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md](../0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) | 公開main | 個別規則と整合、変更不要 |
| [0147-independence-is-drawn-not-declared.md](../0147-independence-is-drawn-not-declared.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0148-the-free-prompt-lane-prices-compute.md](../0148-the-free-prompt-lane-prices-compute.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0149-an-attempts-pwu-is-the-derivation.md](../0149-an-attempts-pwu-is-the-derivation.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0150-the-fingerprint-must-see-the-rule-not-only-the-height.md](../0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) | 公開main | 個別規則と整合、変更不要 |
| [0151-liveness-is-structural-collateral-covers-fraud.md](../0151-liveness-is-structural-collateral-covers-fraud.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0162-the-pair-opens-on-a-virtual-reserve.md](../0162-the-pair-opens-on-a-virtual-reserve.md) | 公開main | 個別規則と整合、変更不要 |
| [0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md](../0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md](../0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0166-verifier-unavailability-is-not-producer-fraud.md](../0166-verifier-unavailability-is-not-producer-fraud.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md](../0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md](../0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) | 公開main | 個別規則と整合、変更不要 |
| [0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md](../0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0170-the-seed-anchor-is-a-window-not-a-span.md](../0170-the-seed-anchor-is-a-window-not-a-span.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0171-probabilistic-constraint-checks-and-court-on-dispute.md](../0171-probabilistic-constraint-checks-and-court-on-dispute.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md](../0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | 公開main | banner＋規範的amendment、旧body保持 |
| [README.md](../README.md) | 公開main | 目的・優先順位・番号を更新 |
| [STATUS-AUDIT-2026-09-18.md](../STATUS-AUDIT-2026-09-18.md) | 公開main | 日付付き実測/evidenceを保持 |
| [STATUS-AUDIT-2026-09-19-llm-mining-reward.md](../STATUS-AUDIT-2026-09-19-llm-mining-reward.md) | 公開main | 日付付き実測/evidenceを保持 |
| [STATUS-AUDIT-2026-09-20-reward-reaudit.md](../STATUS-AUDIT-2026-09-20-reward-reaudit.md) | 公開main | 日付付き実測/evidenceを保持 |
| [0001-palw-inference-surface-gaps.md](../../rfc/0001-palw-inference-surface-gaps.md) | 公開main | 将来設計・受入条件を改定 |
| [0002-palw-tensor-ir.md](../../rfc/0002-palw-tensor-ir.md) | 公開main | 将来設計・受入条件を改定 |
| [0003-palw-generative-model-classes.md](../../rfc/0003-palw-generative-model-classes.md) | 公開main | 将来設計・受入条件を改定 |
| [0004-palw-model-improvement.md](../../rfc/0004-palw-model-improvement.md) | 公開main | 将来設計・受入条件を改定 |
| [0005-palw-ml-vm.md](../../rfc/0005-palw-ml-vm.md) | 公開main | 将来設計・受入条件を改定 |
| [0006-palw-layer-sharded-panels.md](../../rfc/0006-palw-layer-sharded-panels.md) | 公開main | 将来設計・受入条件を改定 |
| [0007-palw-verification-certificates-and-algebraic-checks.md](../../rfc/0007-palw-verification-certificates-and-algebraic-checks.md) | 公開main | 将来設計・受入条件を改定 |
| [0008-palw-claim-backed-consensus-blocks.md](../../rfc/0008-palw-claim-backed-consensus-blocks.md) | 公開main | 将来設計・受入条件を改定 |
| [0009-palw-remote-miner.md](../../rfc/0009-palw-remote-miner.md) | local保持 | 将来設計・受入条件を改定 |
| [0010-permissionless-palw-panel-and-claim-completion.md](../../rfc/0010-permissionless-palw-panel-and-claim-completion.md) | 公開main | 将来設計・受入条件を改定 |
| [0011-permissionless-model-and-long-context-onboarding.md](../../rfc/0011-permissionless-model-and-long-context-onboarding.md) | 公開main | 将来設計・受入条件を改定 |
| [0012-palw-only-consensus-and-native-evm-settlement.md](../../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) | 公開main | 将来設計・受入条件を改定 |
| [0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md](../../rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | 公開main | 将来設計・受入条件を改定 |
| [0014-panel-independent-fraud-prosecution.md](../../rfc/0014-panel-independent-fraud-prosecution.md) | local保持 | 将来設計・受入条件を改定 |
| [0015-panel-free-permissionless-verification.md](../../rfc/0015-panel-free-permissionless-verification.md) | local保持 | 将来設計・受入条件を改定 |
| [README.md](../../rfc/README.md) | 公開main | 目的・優先順位・番号を更新 |
| [0011-audit-coverage.mjs](../../rfc/evidence/0011-audit-coverage.mjs) | 公開main | 日付付き実測/evidenceを保持 |
| [0011-existing-coverage-audit.json](../../rfc/evidence/0011-existing-coverage-audit.json) | 公開main | 日付付き実測/evidenceを保持 |
| [0012-kernel-only-design-audit.md](../../rfc/evidence/0012-kernel-only-design-audit.md) | 公開main | 日付付き実測/evidenceを保持 |
| [0013-llama-t12-registration.json](../../rfc/evidence/0013-llama-t12-registration.json) | 公開main | 日付付き実測/evidenceを保持 |
| [0013-onboarding-tool-validation.json](../../rfc/evidence/0013-onboarding-tool-validation.json) | 公開main | 日付付き実測/evidenceを保持 |

新規ADR173、本report、machine inventory、validationは本baseline確認後の成果物であり、過去版の確認件数に混ぜていない。

## Main publication reconciliation — 2026-10-07

このreport・inventory・旧validationは、上記のworkspace review snapshotの記録である。mainへ反映する際は、公開main `43f0bcb362d37cba79414940f3fdd368d40d3377`に今回の文書差分だけを適用し、ローカルの古いruntimeや基盤ADR本文を持ち込まない。RFC9は公開mainの2026-10-06追記をすべて残し、今回のbanner/amendmentだけを追加した。RFC14 §16とRFC indexには、その後のMISAKA Transport・availability・reward条件の改定も含めた。

旧inventoryのbefore/after hashは選択したworkspace版の記録であり、公開mainの各file hashと同一とは限らない。公開用の変更範囲・history保存・新リンク・型とgateの整合・docs-only差分の検証は[publication validation](0173-main-publication-validation-2026-10-07.json)に記録する。コード統合・ネットワークactivationはこのpublicationに含まない。
