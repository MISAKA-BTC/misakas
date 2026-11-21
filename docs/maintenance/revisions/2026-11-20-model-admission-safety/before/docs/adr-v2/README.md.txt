# ADR-v2 — 現行の閲覧・更新先50本

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

<!-- MOR1-DRAFT-20261120-BEGIN -->
## 2026-11-20 PALW-MOR1 — モデル登録時の試行別認証出力

**DRAFT_NOT_ACTIVATED**。対象profileの登録時に、別の採掘試行の認証出力が天文学的に小さい確率でしか重複しない構成を要求する。正式出力は回答y・試行ID・256bit実行識別値。完全context・実beacon・三Rootへ結合し、同一試行の再現性と別試行の非重複性を定義する。AN1の実入力・Ticket・Panel＋Court・一回会計を維持し、通常LLM費用の下界を成立済みとしない。

[RFC-0060](../rfc/0060-model-registration-and-attempt-output-binding.md)／[ADR-0228](../adr/0228-model-registration-and-attempt-output-binding.md)／[変更・検証](../maintenance/revisions/2026-11-20-model-output-binding/README.md)。新しい不変profileへの適用草案であり、旧登録・過去claim・稼働ルールへ遡及適用しない。現行20 RFC／50 ADRの統合構造を維持する。
<!-- MOR1-DRAFT-20261120-END -->

<!-- AN1-DRAFT-20261118-BEGIN -->
## 2026-11-18 PALW-AN1 — Panel＋Courtを維持する草案

**DRAFT_NOT_ACTIVATED**。アドレス・nonceを実入力tokenへ結合し、登録と元claimの二段階固定、完全再実行、局所反証、一回性を具体化した。計算validity proofは導入せず、通常LLM実費の下界を成立済みとしない。[RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md) / [変更・検証](../maintenance/revisions/2026-11-18-an1/README.md)。0056〜0058の追補と現行20冊の担当案内を追加し、旧採択・過去証拠・50 ADRの統合構成を保持する。
<!-- AN1-DRAFT-20261118-END -->


<!-- SWC1-PROPOSAL-BEGIN -->
> **2026-11-17 監査対応の設計案:** [RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md) / [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md) とRFC-0056・0057の追補は、実行commit、仕事の一回性、JOB／TICKET／PANELの段階別抽選、用途別の成熟・取消接続を扱う。状態は **PROPOSED_DESIGN_NOT_ACTIVATED**。既存原則・採択済み本文・旧claim・runtimeをこの参照で変更しない。新BFT乱数候補は無効、PoW費用下界とDAG定理は未証明である。[変更と検証](../maintenance/revisions/2026-11-17-work-binding/README.md)を参照。
<!-- SWC1-PROPOSAL-END -->

<!-- CPN1-PROPOSAL-BEGIN -->
## 2026-11-17 Court nonceとPanel固有チャレンジ — 設計案

本追記は **PROPOSED_DESIGN_NOT_ACTIVATED** への参照であり、既存採択・署名・runtime・activationを変更しない。[RFC-0057](../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../adr/0225-court-nonce-and-panel-challenges.md)。

nonceはnativeなclaim/actor/義務へ結合し、共有する局所証拠・本人の完全承認・支払い一回性を分ける。前回0055/0056の資源・受付難易度と接続する。個別challengeは本人の承認条件であり、全員待ち・sample外反証の拒否・独立全体再実行の証明へ変更しない。

[改訂記録](../maintenance/revisions/2026-11-17-court-nonce/README.md) / [ソース監査](../maintenance/revisions/2026-11-17-court-nonce/SOURCE_AUDIT.md) / [検証記録](../maintenance/revisions/2026-11-17-court-nonce/VALIDATION.md)。20 RFC／50 ADRと旧資料を保持する。
<!-- CPN1-PROPOSAL-END -->

<!-- CAD1-PROPOSAL-BEGIN -->
## 2026-11-17 全claim共通の受付難易度 — 設計案

[RFC-0056](../rfc/0056-global-claim-admission-difficulty.md) / [ADR-0224](../adr/0224-global-claim-admission-difficulty.md) は、新originalの高コスト処理前に全model共通Dを適用する具体案である。原子予約・courtのhard容量は前回提供した [RFC-0055](../rfc/0055-global-admission-court-calendar-and-one-bps.md) / [ADR-0223](../adr/0223-global-admission-court-calendar-and-one-bps.md) を参照する。採択される新profileでは55§6のDをCAD1へ詳細化し、二重に掛けない。

基準はmain@2d8212b634880b0967fbf24b00b41936c2322c14。0055関連13ファイルは同会話の先行提供物をbytesのまま併録したもので、元mainに存在した/実装済みという意味ではない。通常編集先20 RFC／50 ADRと旧原文を保持する。今回の0056/0224は **PROPOSED_DESIGN_NOT_ACTIVATED**。既存採択状態、runtime、wire、fenceを変更しない。

[改訂記録](../maintenance/revisions/2026-11-17-admission-difficulty/README.md) / [ソース監査](../maintenance/revisions/2026-11-17-admission-difficulty/SOURCE_AUDIT.md) / [検証記録](../maintenance/revisions/2026-11-17-admission-difficulty/VALIDATION.md)。以前のrevisionはその時点のsnapshotに対する記録であり、現在mainの完成証拠へ読み替えない。
<!-- CAD1-PROPOSAL-END -->

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](../maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](../maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC creditは[RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

**2026-10-31 の統合では204本を50の主題別文書へ整理しました。** 正式な識別子は`ADR-v2-0001`〜`ADR-v2-0050`です。この50本を維持し、旧ADRの判断履歴を失わせず、読む・更新する場所を統合しています。統合自体による仕様の再採択と、後日の明示的な新規採択は区別します。

[完全対応表](MAPPING.md) · [現行 RFC 20本との対応](RFC-CROSSWALK.md) · [凍結・延期等の台帳](0050-freeze-deferred-rejected-and-governance.md) · [原文の状態宣言](STATUS-LEDGER.md) · [優先関係](PRECEDENCE.md) · [導入方法](MIGRATION.md) · [統合時の検査結果](VALIDATION.md)

## 2026-11-08 の追加採択

[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) は、新しい [RFC-v2-0016〜0020](../rfc-v2/README.md) の明示採択を記録する **post-consolidation 文書**です。ADR-v2 の51冊目を追加したり、元の204本に混ぜたりしません。今回の状態は **ADOPTED_DESIGN_NOT_ACTIVATED** であり、実装・測定・network activation の完了を示しません。

新しい claim / profile の規範入口は [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) と [公開裁定の方針 v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)。Node はモデル本体・全推論・GPU を要求されず、未共有モデルとモデル内 Miner／Panel の全結託を想定します。正常時は非対話 positive、不一致時は negative の対話で局所化し、必要資料・肯定検証が成立しなければ成功 Final へ進めません。global pool の独立選出と正直な検証者の前提は残ります。[今回の変更記録](../maintenance/revisions/2026-11-08/README.md) / [今回の検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)。

## 読み方

各文書の先頭に統合した判断、その後に後続改定と責任分担、最後に原条文を置きます。原条文は旧ファイルのbytesを保持した折り畳み欄です。**主題の統合と設計の採択は別**で、Draft／ProposedをAcceptedへ、Acceptedを実装・配備済みへ変更していません。

新しい説明・参照は必ず`ADR-v2-xxxx`と名前空間を付けます。`ADR-0014`などの旧番号は意味を変えません。旧番号の衝突3組も完全ファイル名で区別します。

## 50本の一覧

| 新ADR | テーマ | 主担当の旧文書数 | 関連RFC-v2 |
| --- | --- | ---: | --- |
| [ADR-v2-0001](0001-network-identity-and-fingerprint.md) | ネットワーク分離・Hash64・識別子・fingerprint | 4 | [13](../rfc-v2/0013-permissionless-bootstrap-and-release.md), [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) | 耐量子署名・アドレス・RPC／WASM／SDK型 | 3 | [09](../rfc-v2/0009-remote-client-and-sdk.md), [13](../rfc-v2/0013-permissionless-bootstrap-and-release.md) |
| [ADR-v2-0003](0003-utxo-accumulator-and-commitment.md) | UTXO accumulatorと64-byte commitment | 2 | [12](../rfc-v2/0012-authenticated-state-storage-and-ibd.md) |
| [ADR-v2-0004](0004-mass-carriage-and-transaction-relay.md) | Mass・DoS上限・合意object搬送・再送 | 4 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md), [09](../rfc-v2/0009-remote-client-and-sdk.md), [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0005](0005-palw-consensus-architecture.md) | PALW合意構造・実行family・旧PoWからの移行 | 9 | [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0006](0006-fork-choice-participation-and-settlement.md) | Fork choice・参加重み・merged work・settlement | 6 | [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) | EVM／EXEC実行lane・anchor・gas budget | 6 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md), [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0008](0008-daa-difficulty-and-consensus-clock.md) | DAA・difficulty・retarget・合意時計 | 4 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md), [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0009](0009-heartbeat-floor-and-liveness.md) | Heartbeat・BASE floor・停止回復・liveness | 7 | [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md), [13](../rfc-v2/0013-permissionless-bootstrap-and-release.md) |
| [ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md) | Genesis発行上限・bondゼロ起動 | 3 | [13](../rfc-v2/0013-permissionless-bootstrap-and-release.md) |
| [ADR-v2-0011](0011-release-activation-and-flag-days.md) | 公開testnet・release・有効化・flag day | 5 | [13](../rfc-v2/0013-permissionless-bootstrap-and-release.md) |
| [ADR-v2-0012](0012-state-root-pruning-and-snapshots.md) | 状態root・pruning proof・snapshot同期 | 3 | [12](../rfc-v2/0012-authenticated-state-storage-and-ibd.md) |
| [ADR-v2-0013](0013-tir-arithmetic-and-determinism.md) | TIR算術・tile・BASE-0・決定論 | 7 | [02](../rfc-v2/0002-tensor-ir-and-shared-query-data.md) |
| [ADR-v2-0014](0014-versioned-kernels-and-backends.md) | Versioned kernel・拡張境界・runtime backend | 4 | [02](../rfc-v2/0002-tensor-ir-and-shared-query-data.md), [03](../rfc-v2/0003-model-onboarding-and-artifacts.md) |
| [ADR-v2-0015](0015-permissionless-model-registration.md) | Permissionlessモデル登録・class profile・不変性 | 5 | [03](../rfc-v2/0003-model-onboarding-and-artifacts.md) |
| [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) | Artifact所有・モデルlineage・adapter | 3 | [03](../rfc-v2/0003-model-onboarding-and-artifacts.md) |
| [ADR-v2-0017](0017-model-streaming-and-memory-bounds.md) | モデルstreaming・mmap・weight／captureメモリ上限 | 4 | [05](../rfc-v2/0005-bounded-resources-and-long-context.md) |
| [ADR-v2-0018](0018-long-context-measurement-and-admission.md) | 長文脈profile・公開計測・context有効化 | 6 | [03](../rfc-v2/0003-model-onboarding-and-artifacts.md), [05](../rfc-v2/0005-bounded-resources-and-long-context.md) |
| [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) | モデル二階層・年次指定・非共有リスクの境界 | 2 | [04](../rfc-v2/0004-model-tiers-and-annual-catalog.md), [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0020](0020-class-share-and-allocation-evolution.md) | Class share・epoch配分・経済係数の変遷 | 7 | [04](../rfc-v2/0004-model-tiers-and-annual-catalog.md), [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0021](0021-positions-and-membership.md) | 追加モデルPosition・membership・直接移転と永久LP | 3 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0022](0022-liquidity-seeding-and-virtual-reserves.md) | 登録時市場生成・reserve・永久流動性 | 4 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md) | 30%購入・20%永久LP・余剰burnとcore報酬 | 2 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0024](0024-inference-input-output-contract.md) | 推論Job・prompt・構造化出力・入口契約 | 2 | [01](../rfc-v2/0001-inference-and-generative-jobs.md) |
| [ADR-v2-0025](0025-context-and-input-commitments.md) | Context／入力state・commitment・取得単位 | 3 | [01](../rfc-v2/0001-inference-and-generative-jobs.md), [08](../rfc-v2/0008-claim-dependencies-and-liability.md) |
| [ADR-v2-0026](0026-canonical-work-and-answer-accounting.md) | 正規仕事量・PWU・推論対価・回答の二重計上防止 | 9 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md), [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0027](0027-claim-lifecycle-and-liability.md) | Free-prompt claim・lock・native責任期間 | 3 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md) |
| [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) | Attempt・ticket・抽選・seed anchor | 5 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md) |
| [ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md) | Remote client・miner操作・署名・host sandbox | 3 | [09](../rfc-v2/0009-remote-client-and-sdk.md) |
| [ADR-v2-0030](0030-sharded-verification-and-model-measurement.md) | Shard検証・登録時計測・2M実装計画 | 2 | [03](../rfc-v2/0003-model-onboarding-and-artifacts.md), [06](../rfc-v2/0006-panels-and-probabilistic-verification.md) |
| [ADR-v2-0031](0031-panel-independence-and-unavailability.md) | Panel独立性・coverage・取得不能と不正の区別 | 4 | [06](../rfc-v2/0006-panels-and-probabilistic-verification.md) |
| [ADR-v2-0032](0032-sampling-beacon-and-receipts.md) | 検証抽選・beacon・認証receipt・paid lock乱数 | 4 | [06](../rfc-v2/0006-panels-and-probabilistic-verification.md), [10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| [ADR-v2-0033](0033-probabilistic-checks-and-exact-court.md) | 確率的constraint検査とexact courtの分担 | 1 | [06](../rfc-v2/0006-panels-and-probabilistic-verification.md), [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0034](0034-verification-spans-and-service-clock.md) | 検証span・class別service時計・starvation境界 | 1 | [06](../rfc-v2/0006-panels-and-probabilistic-verification.md) |
| [ADR-v2-0035](0035-bond-budget-and-shared-reservations.md) | Bond・責任予算・共有担保・段階reserve | 3 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md) | Claim容量拡張・固定発行予算・breaker | 2 | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md), [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) | Fee escrow・Panel報酬・verifier／provider同意 | 3 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md) |
| [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) | G14公開訴追・adjudicability・裁定保証の前提 | 7 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0039](0039-data-availability-court-and-panel-da.md) | DA court・claim資料請求・PanelDa期限付き開示 | 3 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) | TIR終端証拠・fold opening・局所裁定 | 4 | [02](../rfc-v2/0002-tensor-ir-and-shared-query-data.md), [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0041](0041-long-context-court-decomposition.md) | 長文脈court・sparse read set・dispute分解 | 3 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0042](0042-court-service-and-versioned-clock.md) | Court処理機会・共有訴追・versioned clock | 3 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0043](0043-court-eligibility-and-initial-notice.md) | Court応答資格履歴・initial notice境界 | 2 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0044](0044-shared-tir-query-data.md) | Owner非依存の共有TIR query・認証data | 1 | [02](../rfc-v2/0002-tensor-ir-and-shared-query-data.md), [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0045](0045-terminal-material-and-readiness.md) | 終端material受付・非独占提出・readiness opening | 2 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0046](0046-court-close-and-liability-archive.md) | Court終了効果・未解決責任archive・金融解除 | 2 | [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0047](0047-funded-original-and-role-authorization.md) | Court originalの実原資・予約・役割別署名 | 1 | [11](../rfc-v2/0011-bond-budgets-fees-and-payments.md), [14](../rfc-v2/0014-court-master-contract.md) |
| [ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md) | Claim依存関係・独立進行・prefix生成元 | 2 | [08](../rfc-v2/0008-claim-dependencies-and-liability.md) |
| [ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md) | 認証state map・IBD row stream・世代別保存 | 2 | [12](../rfc-v2/0012-authenticated-state-storage-and-ibd.md) |
| [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) | 設計凍結・延期・撤回・不採用・未採択・変更管理 | 24 | [15](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) |

## Court・TIR・会計の境界

Courtの共通契約は[RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)、2026-11-08 採択の新しい claim 版に固有の境界は[RFC-v2-0016〜0020](../rfc-v2/README.md)を参照します。ADRは判断理由を探せるよう38〜47に分け、TIRの算術・データ形式は13・14・40・44、共通会計は19〜23・26・35〜37が担当します。shared queryの形式はRFC-v2-0002、共通の応答義務・期限・defaultはRFC-v2-0014、新版の不一致処理と義務違反の区別は[RFC-v2-0018](../rfc-v2/0018-interactive-negative-court-and-defaults.md)を参照します。

## 保存したもの／変更していないもの

2026-10-31 の統合時には、旧204本・48,236行の原文、旧41 RFC、新15 RFCを保存しました。元snapshotのruntimeコードはこの文書overlayに含めず、修正していませんでした。その統合作業では新15 RFCの全ファイルを既存パッケージから変更せず、合意設定・wire ID・署名domain・高さ・報酬係数・G14の保証範囲に新しい決定を加えていません。この記録は統合時点の範囲であり、現行20 RFCや2026-11-08の追加採択を否定するものではありません。

この50本は現在の整理単位です。重大な新規判断に新規RFCとADRを要求する旧ADR-0181の手続きを、50という数を保つために省略してはいけません。詳細は[変更手順](MIGRATION.md)を参照してください。

## 統合基準日後の追加文書

並行作業で追加された [ADR-0210](../adr/0210-all-model-g14-panel-one.md) / [RFC-0042](../rfc/0042-all-model-g14-panel-one.md) は、入力snapshotの204 ADR／41 RFCに含まれない追加文書として保持する。統合時の50 ADR／15 RFCへ原文を混ぜず、採択・実装・有効化の状態は各文書の記録を参照する。`registry.json` の `post_consolidation_documents` からもたどれる。旧番号の意味は変えない。

2026-10-31に採択された学習済みLLMの出力表現の追加は [RFC-0043](../rfc/0043-wide-q24-llm-output.md) / [ADR-0211](../adr/0211-wide-q24-llm-output.md)。旧i32 Q24・既存schemeを保持し、別のi64 Q24 profileを追加する。公開有効化は別判断。入力原本へ混ぜず、post-consolidation文書として読む。

2026-11-11追加採択: [RFC-0052](../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)／[ADR-0220](../adr/0220-hugging-face-model-acquisition-and-claim-materials.md)。HFを新profileの必須モデル公開元にし、claim固有資料の供給と分離する。登録・定期取得検査と期限、資料不足の成功停止を維持する。通常の20 RFC／50 ADRと過去snapshotは維持し、公開activationは未完。

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。
