# 置換・撤回・混合採択の適用境界

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](../governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。
<!-- MOE2-ADOPTED-20261120-END -->


<!-- MOE1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — 追加モデル限定MisakaOption経済MOE1

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済の正本をRFC0063/ADR0231とRFC-v2-0011へ追加する。旧sourceの率・非譲渡・core売却継続は新経済profileの選択肢ではない。

追加モデル登録と一意な市場の作成を同じ原子遷移とし、通常の登録済み・市場未作成状態やseed待ちを設けない。genesis・有効softfork指定モデルは市場を作らず、価格付け・共通上限処理後の対象worker報酬RをMinerへ100%配分する。追加モデルのみ50%買上げ、通常AMMはburn5%・owner総額3%・net残差、直接送受信はAMM手数料なしとする。

[RFC0063](../rfc/0063-additional-model-option-economics.md) / [ADR0231](../adr/0231-additional-model-option-economics.md) / [適用と移行](../governance/model-option-economics-transition.md)。新経済profileではこの後続採択を優先する。旧率・旧署名・原文引用・過去の実装試験はその版の記録として保持し、新規経路へfallbackしない。TB1/FOW1の正当性・乱数・Court・適格性条件を免除しない。
<!-- MOE1-ADOPTED-20261120-END -->

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

新しい採掘profileの規範入口をRFC0061/0062へ変更する。旧文書のAccepted/Draft表示や旧beaconの語は旧版の状態を表し、新profileの選択肢ではない。

[TB1の正本](../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

> **2026-11-08追加採択:** EXEC creditは[RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

**大きい番号や新しい日付だけで、古い文書全体を無効にしません。** 明示された対象の節・network・claim cohort・fenceについてだけ後続改定を適用します。表は重要な読み違いを防ぐ編集索引であり、全原文に含まれる全ての競合を機械的に解決した証明ではありません。

## 2026-11-08 採択の適用範囲

[PRINCIPLES](../PRINCIPLES.md) の明示改定と [ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) に基づき、新しい claim / profile は [RFC-v2-0016〜0020](../rfc-v2/README.md) および [公開裁定の方針 v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md) に従います。設計状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。wire・fence・有効化高さは未割当であり、旧署名済み claim を新版に読み替えません。

| 論点 | 新しい claim 版で優先する規則 | 保持する境界 |
| --- | --- | --- |
| Node の責務 | モデル本体の取得・保持、全推論、GPU を要求しない。確定状態と上限付き局所証拠で裁定 | 重いモデル検証は Miner／Panel。認証付きの必要な重み断片は証拠として扱える |
| 正常時の肯定検証 | Miner が trace と資料の供給責任を固定し、独立選出 Panel が非対話 positive を作る | 一つの局所計算の成立や署名だけを、全計算の正しさへ昇格させない |
| Final と認証 | 結果固定後に検証し、必須局所 positive と独立肯定承認の成立で `CertificationReady`、公開異議・依存条件等の成立後に成功 Final | 従来の `Final` 後認証は旧版の履歴。旧 claim の状態・責任・署名の意味は不変 |
| 未共有・秘匿・寡占 | モデル内 Miner／Panel の全結託を想定し、global pool の独立選出と資料取得を要求 | global な正直者の前提は残る。モデル登録時の可用性審査や全 Node へのモデル配布を復活させない |
| 取得不能と責任 | 資料不足・肯定検証未成立なら成功 Final 不可。計算不正と適格な提供義務違反を区別 | private timeout や Panel の未受領申告だけで計算不正として slash しない |

統合 ADR 50本は判断履歴・共通制約の参照先として保持し、ADR-0215 をその原文に混ぜません。モデル二階層、暗号学的な計算正当性証明を使わない方針、初期 mainnet の延期機能、既存の原時計・責任境界は、明示的に改定された範囲を除いて維持します。

## 従来の採択と置換の索引

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
