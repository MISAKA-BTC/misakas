# MISAKA RFC v2 — 現行20本

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。QF1は01/02/03/05/06/07/16〜20へ具体的な入力・数値・Readout・検査条件を追加する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用定理・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。0002/0003/0005/0006/0007/0009/0010/0011/0016〜0020がMAS1のkernel・登録・評価・停止・成功境界を担当する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

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

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](../maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](../maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](../maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

## 追加採択 — EXECの1 BPSとcourt容量

**現行RFCは引き続き20本。** ユーザーの追加指示により、
[07](0007-claims-exec-and-answer-accounting.md)へ期限付きExecCredit台帳、
[10](0010-dag-consensus-and-native-settlement.md)へ事前公開schedule・明示ScheduleId・現在slot生成と1 BPS目標を反映した。
生成権の参照と取引履歴への接続先を分離し、通常tipの伸長で割当を変えない。

1 BPSはEXEC_TX全体の通常時目標であり、1,000 ms slotあたり最大一つの正規blockを使う。
credit・予定表・producer・検証／包含容量の条件を付け、1秒決済や1 DAA/秒とは扱わない。
[05](0005-bounded-resources-and-long-context.md)・[18](0018-interactive-negative-court-and-defaults.md)・
[19](0019-model-free-node-and-bounded-evidence.md)・[20](0020-verification-gated-final-and-adversarial-models.md)には、
添付分析のcourt待機・搬送予算の膨張を解消する全分岐予約・共有・並列化・実モデルの受入条件を追加した。

[追加採択RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md) /
[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md) /
[今回の変更](../maintenance/revisions/2026-11-08-exec-1bps/README.md) /
[今回の検査](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。

改訂日: 2026-11-08 / 入力: docs(2).zip / 新設計: **ADOPTED_DESIGN_NOT_ACTIVATED**。

**普段読む・更新するRFCを0001〜0020の20本に整理する。**
ユーザーが採択した、モデルを持たないNode、モデル非共有・Miner／Panel寡占の脅威、
非対話positive・対話negative・検証先行Finalを16〜20で定義し、01〜15の適用範囲も更新した。
これは設計採択と文書更新であり、新profileの実装・測定・公開有効化の完了ではない。

[旧番号原文・追加採択の対応](MAPPING.md) / [版の境界・反映方法](MIGRATION.md) /
[採択ADR](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) /
[変更記録](../maintenance/revisions/2026-11-08/README.md) /
[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)。

## 先に固定する設計条件

- **Nodeはモデル本体を取得・保持せず、全推論、GPU、モデル実行環境、Panel workerを必須にしない。** court・cold start・IBD・再起動でも同じ条件とする。Nodeは上限内の認証付き局所断片を検査する。
- **モデル共有やモデル保有者内の正直な参加者を仮定しない。** 悪意ある運営者がモデルを隠し、そのモデルのMiner・通常Panel・providerを全て支配していても、そこだけの承認で成功Finalにしない。
- Minerが元traceと結果を署名して固定し、global poolから独立に選ばれた検証者が資料を取得して全再実行する。canonical traceのrootと宣言出力が一致すれば、自分の記録からpositiveを一括作成できる。
- 正常positiveに中間値を順次照合する対話を要求しない。不一致は対話negativeで局所化し、完成済み反証は直接提出も認める。
- 資料不足・検証未完了なら、反証を作れなくても成功Finalと仕事creditを与えない。計算不正、提供義務違反、未検証終了は別の結果である。
- 小さなpositiveと承認署名だけで、全global検証者の共謀まで防げるとは主張しない。global抽選と誠実性の前提、Nodeの局所検査、資料供給義務の保証範囲を分ける。

## 20本の構成

| RFC | 担当する範囲 | 今回の位置づけ |
| --- | --- | --- |
| [01](0001-inference-and-generative-jobs.md) | 推論・生成Job・入出力 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [02](0002-tensor-ir-and-shared-query-data.md) | TIR・Kernel・共有Queryデータ | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [03](0003-model-onboarding-and-artifacts.md) | モデル登録・変換・Artifact | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [04](0004-model-tiers-and-annual-catalog.md) | モデル二階層・年次カタログ | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [05](0005-bounded-resources-and-long-context.md) | 資源上限・長文脈メモリ・計測 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [06](0006-panels-and-probabilistic-verification.md) | Panel参加・抽選・確率的検証・認証Receipt | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [07](0007-claims-exec-and-answer-accounting.md) | Claim・EXEC・仕事量・回答会計 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [08](0008-claim-dependencies-and-liability.md) | Claim依存・Prefix生成元・未解決責任 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [09](0009-remote-client-and-sdk.md) | Remote Client・SDK・非保管型操作 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [10](0010-dag-consensus-and-native-settlement.md) | DAG合意・EXECスケジュール・Native EVM決済 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [11](0011-bond-budgets-fees-and-payments.md) | Bond予算・配分・Fee同意・決済会計 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [12](0012-authenticated-state-storage-and-ibd.md) | 認証State・保存・Pruning・IBD | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [13](0013-permissionless-bootstrap-and-release.md) | 公開Bootstrap・Genesis・有効化とRelease接続 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [14](0014-court-master-contract.md) | Court統合契約 — 受付・対話・裁定・資金・責任終了 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [15](0015-freeze-deferred-rejected-and-governance.md) | 設計凍結・延期・不採用・未採択・変更管理 | 基礎契約・旧profileを保持し、新profileの適用範囲を追加 |
| [16](0016-miner-execution-commit-and-material-supply.md) | Minerの実行commitとclaim固有の資料供給 | 2026-11-08採択の新profileを定義 |
| [17](0017-noninteractive-positive-verification.md) | 非対話positiveと独立再実行の承認 | 2026-11-08採択の新profileを定義 |
| [18](0018-interactive-negative-court-and-defaults.md) | 対話negative・局所裁定・提供義務違反 | 2026-11-08採択の新profileを定義 |
| [19](0019-model-free-node-and-bounded-evidence.md) | モデルを持たないNodeと有界な証拠 | 2026-11-08採択の新profileを定義 |
| [20](0020-verification-gated-final-and-adversarial-models.md) | 検証先行Finalと秘匿・寡占を前提にした安全条件 | 2026-11-08採択の新profileを定義 |

## 読む順序と責務

先行した検証設計の変更は **20（脅威・成功条件）→19（Nodeの境界）→16（元claim・供給）→17（positive）→18（negative）** の順で読む。
jobと数値実行は01・02、登録と階層は03・04、資源は05、参加は06、仕事会計は07、
依存は08、APIは09、合意接続は10、資金は11、保存とIBDは12、releaseは13、共通courtは14、変更管理は15を参照する。

新profileの成功は、ResultCommitted → VerificationPending → CertificationReady → Finalの順である。
CertificationReadyは事前認証の成立を表し、それだけでは仕事報酬や合意creditを与えない。
不一致・適格な公開異議・資料要求が未解決なら保留し、期限までに肯定条件が成立しなければ未検証終了となる。

Nodeの承認条件と局所検査は19・20、Panelの独立再実行責任は17に置く。
同じ規範を各冊で別々に定義しない。通常Panelとglobal認証者を同じrole名だけで同一視しない。

## 歴史と現行の区別

以前の旧番号RFC48ファイル（0001〜0047、0042は二つ）を元の完全パスで保持し、今回のRFC-0048を加えた49ファイルを索引化する。
ADRの統合50本も保持し、新しい採択記録ADR-0215をpost-consolidation記録として登録する。
旧番号のbody・wire ID・署名domain・fence・digestを一括置換しない。

2026-10-29の41原文→15冊の統合は、[source-map.json](source-map.json)と89収録区間に残る。
現在の49の旧番号原文・採択記録→20冊の編集担当は[adoption-map.json](adoption-map.json)にある。
前者は歴史snapshot、後者は現在の担当表であり、同じ台帳として書き換えない。
旧0044のPositive Court R2全被覆検査、旧Gate CのFinal後認証もその版の意味を保持する。

新しい規則の優先範囲は[20](0020-verification-gated-final-and-adversarial-models.md)と
[PRINCIPLES §13](../PRINCIPLES.md#model-free-adversarial-verification)に限定する。
[公開裁定の方針v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)が新profileの方針である。
二階層、既存経済パラメーター、計算validity proofの禁止、media・モデル改善の延期は維持する。

## 検証と有効化

今回の再検査は、20冊の構成、原文と歴史付録の保持、対応表、追加リンクの回帰を対象とする。
runtimeの完成、未測定の性能、稼働ネットワークの安全性を文書検査で保証しない。
既存入力にある欠損リンクと旧snapshotのハッシュ不一致は[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)で別に報告する。

新profileの署名形式・native接続・抽選policy・資源割当・cold資料取得・最悪時のnegative・
reorg・決済の検証が揃ってから、既存手順でactivationを判断する。
以後の通常追記は20冊の担当先へ集約し、設計凍結の例外手続きが必要な変更は新規RFC＋ADRと明示採択で扱う。

2026-11-11追加採択: [RFC-0052](../rfc/0052-hugging-face-model-acquisition-and-claim-materials.md)／[ADR-0220](../adr/0220-hugging-face-model-acquisition-and-claim-materials.md)。HFを新profileの必須モデル公開元にし、claim固有資料の供給と分離する。登録・定期取得検査と期限、資料不足の成功停止を維持する。通常の20 RFC／50 ADRと過去snapshotは維持し、公開activationは未完。

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。
