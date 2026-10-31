# 置換・撤回・混合採択の適用境界

**大きい番号や新しい日付だけで、古い文書全体を無効にしません。** 明示された対象の節・network・claim cohort・fenceについてだけ後続改定を適用します。表は重要な読み違いを防ぐ編集索引であり、全原文に含まれる全ての競合を機械的に解決した証明ではありません。

| 主題 | 優先・区別の範囲 | 原資料 | 新しい主担当 |
| --- | --- | --- | --- |
| 署名とaddress | 旧0002の65／32-byte例 → 旧0019の87／64-byte移行 | [旧ADR-0002](../adr/0002-mldsa65-p2pkh.md) [旧ADR-0019](../adr/0019-mldsa87-migration.md) | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) |
| LtHash | 旧0003内部の初期16-bit案 → 同文書の32-bit監査改定 | [旧ADR-0003](../adr/0003-lthash-utxo-accumulator.md) | [ADR-v2-0003](0003-utxo-accumulator-and-commitment.md) |
| PALW合意 | 旧0037 Decision 1 → 0038 → 0039の明示改定範囲。0007のLayer 0は撤回しない | [旧ADR-0007](../adr/0007-layered-pow.md) [旧ADR-0037](../adr/0037-palw-async-job-state-machine.md) [旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md) [旧ADR-0039](../adr/0039-palw-only-block-production.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| Family M | 旧0051 → 0053の単一family | [旧ADR-0051](../adr/0051-palw-metal-gguf-execution-family.md) [旧ADR-0053](../adr/0053-palw-one-execution-family.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| 旧hash-floor | 旧0036 Decision 4、0038 W4/W6などの対象範囲 → 0039 | [旧ADR-0036](../adr/0036-palw-mainnet-activation-model.md) [旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md) [旧ADR-0039](../adr/0039-palw-only-block-production.md) | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| 回答／prefill分割 | 旧0080／0081の撤回部分 → 0082。維持されたtransport／prompt IDsは残す | [旧ADR-0080](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) [旧ADR-0081](../adr/0081-long-context-the-input-is-a-state-chain.md) [旧ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md) | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) |
| step ladder | 旧0092のclock budget → 0103のheld方式ではcarrier budget、0119のclass ladder | [旧ADR-0092](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) [旧ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) [旧ADR-0119](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | [ADR-v2-0018](0018-long-context-measurement-and-admission.md) |
| mutable model | 旧0088等の更新系 → 0175の登録不変性。画像・動画・改善protocolは0182の延期 | [旧ADR-0088](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) [旧ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md) [旧ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) | [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) |
| 入手保証 | 旧0173-publicのモデル配布強制部分 → 0177で撤回。claim資料の義務は維持 | [旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) |
| モデル二階層 | 旧0177内の候補 → 同文書の明示確定 → 0179の係数・審査廃止 | [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) [旧ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) |
| 認証とFinal | 旧0177本文の非介入一般論 → 2026-10-15の合意credit改定。取得不能だけでslashしない | [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) |
| Position手数料 | 旧0087の旧料率 → 0114のfenced料率。0177は直接採掘配当を否定 | [旧ADR-0087](../adr/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) [旧ADR-0114](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | [ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md) |
| 市場seed | 0090／0094 → 0120 → 0162の各版・fence。異なるreserveを同時適用しない | [旧ADR-0090](../adr/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) [旧ADR-0094](../adr/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) [旧ADR-0120](../adr/0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) [旧ADR-0162](../adr/0162-the-pair-opens-on-a-virtual-reserve.md) | [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) |
| genesis bond | 0061のzero-seat選択肢 → 0209の新genesisに対するbondゼロ方針 | [旧ADR-0061](../adr/0061-zero-seat-genesis-and-right-sized-collateral.md) [旧ADR-0209](../adr/0209-genesis-bond-free-bootstrap.md) | [ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md) |
| PanelDa | 旧の長期非公開前提 → 0206の期限付き開示。model全weights開示ではない | [旧ADR-0206](../adr/0206-panel-da-mandatory-disclosure-g14-route.md) | [ADR-v2-0039](0039-data-availability-court-and-panel-da.md) |
| feeと役務 | 0207の通常verifier／provider fee-onlyは採択。役務有限化A/Bは未採択のまま | [旧ADR-0207](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) | [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) |
| 依存prefix | 0186の独立進行と0208の受理済み生成元identity。期限延長や旧claim改変はしない | [旧ADR-0186](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) [旧ADR-0208](../adr/0208-authenticated-prefix-work-dependencies.md) | [ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md) |
| state rootとtransport | 0203は新root形式案。0204は既存flat rootを保つstream案。相互の自動置換ではない | [旧ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md) [旧ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md) | [ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md) |
| 設計承認 | 実装許可、隔離統合許可、採択、実測値承認、network有効化、公開配備を分ける | [旧ADR-0181](../adr/0181-design-freeze-and-controlled-change.md) [旧ADR-0194](../adr/0194-dormant-g14-integration-and-service-schema.md) [旧ADR-0207](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) |

## 判断が一つの文書に混在する場合

通常verifier fee・provider fee-onlyの採択は、provider役務の有限化A/B案まで採択したことを意味しません。Court候補のA案を詳細化・隔離実装する許可も、新しい本番料金・公開wire・既存claim移行・activationの許可とは別です。

重大変更の判断が必要になったら[旧ADR-0181](../adr/0181-design-freeze-and-controlled-change.md)の手続きを使います。この再編では不足している採択を補わず、未決を未決のまま残します。
