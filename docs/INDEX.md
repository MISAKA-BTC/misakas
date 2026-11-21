# 文書索引（互換入口）

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](governance/qwen-fow1-profile-transition.md)。Qwenの実行位置・数値・Readout・TicketはRFC0066に具体化した。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・反復費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。追加モデルの安全性入口、有限の非報酬評価、停止・復帰の担当文書を追加する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。
<!-- MOE2-ADOPTED-20261120-END -->


<!-- MOE1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — 追加モデル限定MisakaOption経済MOE1

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済の正本をRFC0063/ADR0231とRFC-v2-0011へ追加する。旧sourceの率・非譲渡・core売却継続は新経済profileの選択肢ではない。

追加モデル登録と一意な市場の作成を同じ原子遷移とし、通常の登録済み・市場未作成状態やseed待ちを設けない。genesis・有効softfork指定モデルは市場を作らず、価格付け・共通上限処理後の対象worker報酬RをMinerへ100%配分する。追加モデルのみ50%買上げ、通常AMMはburn5%・owner総額3%・net残差、直接送受信はAMM手数料なしとする。

[RFC0063](rfc/0063-additional-model-option-economics.md) / [ADR0231](adr/0231-additional-model-option-economics.md) / [適用と移行](governance/model-option-economics-transition.md)。新経済profileではこの後続採択を優先する。旧率・旧署名・原文引用・過去の実装試験はその版の記録として保持し、新規経路へfallbackしない。TB1/FOW1の正当性・乱数・Court・適格性条件を免除しない。
<!-- MOE1-ADOPTED-20261120-END -->

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

新しい採掘profileの規範入口をRFC0061/0062へ変更する。旧文書のAccepted/Draft表示や旧beaconの語は旧版の状態を表し、新profileの選択肢ではない。

[TB1の正本](rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

<!-- MOR1-DRAFT-20261120-BEGIN -->
## 2026-11-20 PALW-MOR1 — モデル登録時の試行別認証出力

**DRAFT_NOT_ACTIVATED**。対象profileの登録時に、別の採掘試行の認証出力が天文学的に小さい確率でしか重複しない構成を要求する。正式出力は回答y・試行ID・256bit実行識別値。完全context・実beacon・三Rootへ結合し、同一試行の再現性と別試行の非重複性を定義する。AN1の実入力・Ticket・Panel＋Court・一回会計を維持し、通常LLM費用の下界を成立済みとしない。

[RFC-0060](rfc/0060-model-registration-and-attempt-output-binding.md)／[ADR-0228](adr/0228-model-registration-and-attempt-output-binding.md)／[変更・検証](maintenance/revisions/2026-11-20-model-output-binding/README.md)。新しい不変profileへの適用草案であり、旧登録・過去claim・稼働ルールへ遡及適用しない。現行20 RFC／50 ADRの統合構造を維持する。
<!-- MOR1-DRAFT-20261120-END -->

<!-- AN1-DRAFT-20261118-BEGIN -->
## 2026-11-18 PALW-AN1 — Panel＋Courtを維持する草案

**DRAFT_NOT_ACTIVATED**。アドレス・nonceを実入力tokenへ結合し、登録と元claimの二段階固定、完全再実行、局所反証、一回性を具体化した。計算validity proofは導入せず、通常LLM実費の下界を成立済みとしない。[RFC-0059](rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](adr/0227-palw-an1-input-binding-and-panel-court.md) / [変更・検証](maintenance/revisions/2026-11-18-an1/README.md)。0056〜0058の追補と現行20冊の担当案内を追加し、旧採択・過去証拠・50 ADRの統合構成を保持する。
<!-- AN1-DRAFT-20261118-END -->


## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC creditは[RFC07](rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

旧 RFC 索引が参照していた `docs/INDEX.md` を案内として復旧しました。総合入口は [README](README.md)、完全一覧は [カタログ](maintenance/catalog.md)。

## 1. 利用・運用

[はじめに](getting-started/README.md) / [運用](operations/README.md) / [リファレンス](reference/README.md)。

## 2. 設計と適用状態

[PRINCIPLES](PRINCIPLES.md) / [状態](STATUS.md) / [仕様](spec/README.md)。

[PALW Architecture Specification](architecture/palw-specification.md) — 0〜9章の統合仕様と、モデル＝minerを前提とするDAG並列ブロック生成。

[2026-11-08 採択設計](rfc-v2/0020-verification-gated-final-and-adversarial-models.md) / [公開裁定の方針 v0.2](design/palw/consensus/public-adjudication-policy-v0.2.md) — 新しい claim 版について、Node のモデル非保有、未共有・モデル内全結託の脅威、非対話 positive、対話 negative、検証成立後の Final を定義します。実装・測定・有効化の完了とは別です。

<a id="3"></a>
## 3. RFC・ADR・仕様の更新

採用済み設計は原則凍結。重大問題時には新規 RFC と新規 ADR の両方、レビュー、明示的採択、対象仕様の改定、試験・移行と別途の有効化判断を必要とします。[変更管理](governance/design-freeze.md)

現行の設計文書: [RFC 20本（RFC-v2-0001〜0020）](rfc-v2/README.md) / [統合 ADR 50本](adr-v2/README.md)。新設計の採択記録 [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) は統合後の追加文書です。[RFC対応表](rfc-v2/MAPPING.md) / [ADR対応表](adr-v2/MAPPING.md) / [今回の変更記録](maintenance/revisions/2026-11-08/README.md) / [今回の検査結果](maintenance/revisions/2026-11-08/VALIDATION.md)。
