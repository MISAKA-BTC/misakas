# MISAKA ドキュメント

## 2026-11-26 MWA1 — モデル別仕事量・報酬・EXEC credit

**PROPOSED_DESIGN_NOT_ACTIVATED**。[RFC0070](rfc/0070-model-work-accounting-and-exec-credit.md)／[ADR0238](adr/0238-model-work-accounting-and-exec-credit.md)をモデル別仕事量認定の設計案として追加する。FOW1一試行一Ticket・事前固定targetを維持し、共通meter・ModelWorkProfile・当選原資の認定仕事量比例・有限EXEC creditを接続する案。BFTは任意倍率を付けず、旧MOE2・genesis管理免除を復活させない。MOE3全モデル1.0・50/20/20/5/5、BMR1の49/51個別slashと100%廃止burn、独立Gate Cを維持する。共通carrier 1 BPS、台帳確定600秒は設計目標。120秒計画epoch・原期限・公開fence・QF1 REVIEW_HOLDは変更しない。詳細式の採択、実装、実測、公開有効化は別工程。


<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTによるモデル廃止を可逆Pauseと区別した恒久終端にする。モデル廃止と個人の有罪判定を分け、次のscopeだけを処理する。

| 対象 | BMR1のモデル廃止時 | 個人不正との関係 |
| --- | --- | --- |
| 当該モデルの永久LP内MISAKA | 認証済みactual reserveを100%burn | V・receipt累計・別pool残高を実資産に数えない |
| 当該モデルの登録者責任bond | 対象の責任lotを100%burn | 無関係なwallet/別model資産へ拡張しない |
| 当該モデルのmining bond | MINING用途lotを100%burn | 同じ人がPanelを兼任しても用途を分ける |
| 当該モデルの未確定mining報酬 | 100%失効/burn | 未発行予約失効と実資産burnを二重計上しない |
| 全Panel/Gate C verification bond | **一律にBFT廃止burnから保護** | 不正者も同じ保護。個人不正はCourtの個別slashで処理 |
| 正当に成立した検証報酬 | MISAKAで支払を維持 | 廃止を理由に成立済み役務対価を横取りしない |
| 他モデルの資産・bond・報酬 | 影響なし | 同一address・同一参加者という理由で混ぜない |

兼任者は用途別lotで管理する。MINING6,000/PANEL4,000なら前者6,000だけをburn、検証用途4,000は保護する。同じ元本の重複担保と責任中の用途変更を禁止し、廃止直前の付替えでburn又は保護を迂回しない。

通常の永久LP withdraw/redeem・管理者引出し禁止を維持し、BMR1 canonical廃止だけにactual MISAKA reserveの宛先なし消却を認める。**BFTは没収を受け取らず、廃止bountyは0**。継続5%を廃止件数や没収額に連動させない。RETIREDを通常Resume/BurnResumeで戻さず、台帳finality・fork choice・任意残高処分の権限を追加しない。

旧「BFTは停止だけ」「永久LPを一切burnできない」等は、この明示的な廃止scopeだけで置換する。旧claim/旧profileの義務・履歴・wire・実装/過去試験を保全し、BMR1 native接続・移行実行・公開activationを今回完了扱いしない。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](adr/0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](governance/fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](adr/0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](governance/model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](governance/qwen-fow1-profile-transition.md)。Qwen3-8Bの具体的FOW1実行をQF1として追加した。最初にRFC0066を参照する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・反復費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。採掘資格・モデル保存登録・市場状態を分ける。今回の入口はRFC0065と現行RFC-v2-0003/0020である。

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

<!-- AN1-DRAFT-20261118-BEGIN -->
## 2026-11-18 PALW-AN1 — Panel＋Courtを維持する草案

**DRAFT_NOT_ACTIVATED**。アドレス・nonceを実入力tokenへ結合し、登録と元claimの二段階固定、完全再実行、局所反証、一回性を具体化した。計算validity proofは導入せず、通常LLM実費の下界を成立済みとしない。[RFC-0059](rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](adr/0227-palw-an1-input-binding-and-panel-court.md) / [変更・検証](maintenance/revisions/2026-11-18-an1/README.md)。関連する追補はRFC-0056〜0058を参照してください。
<!-- AN1-DRAFT-20261118-END -->


> **未完gate:** 大きな資料の供給・取得後検査を有界なnative根拠で客観終結するprofile別手順は、具体化できていない場合 **PROFILE PROCEDURE NOT ESTABLISHED** として有効化不可である。最低承認数1や公開方針だけでは、この手順の成立を証明しない。実装・実測の未完と区別し、18 §2.4／20 §4.3のgateを維持する。

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[採択時の統合RFC第20章](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC credit・事前公開schedule・通常時1 BPSの目標とcourt容量は [RFC-0048](rfc/0048-exec-credit-schedule-and-one-bps.md) と [ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md) を参照してください。設計採択、実装、測定、有効化を区別します。

設計文書は [RFC](rfc/README.md) と [ADR](adr/README.md) から参照できます。
>
> **2026-10-31 の統合履歴:** 当時は RFC 15本・ADR 50本に整理しました。旧番号の本文・参照を保持しています。[取り込み記録](maintenance/v2-integration-2026-10-31.md)。

**使う・運用する・仕組みを理解する・変更を提案する。目的から選んでください。**

> **採用済み設計は原則凍結。** 設計変数・パラメーターの承認された調整を除き、通常は変更しません。重大な問題がある場合に限り、**新規 RFC と新規 ADR の両方**をレビュー・明示的採択して変更できます。[変更管理](governance/design-freeze.md)
>
> **初期 mainnet の対象外:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は、**mainnet 後に延期し、当面は無効**とします。mainnet 開始による自動有効化はありません。[延期・無効化方針](governance/post-mainnet-scope.md)

2026-11-08 のclaim設計は [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) に採択を記録しています。状態は **ADOPTED_DESIGN_NOT_ACTIVATED** です。[状態](STATUS.md)

新しい claim 版では、**Node はモデル本体を取得・保持せず、全推論・GPU を要求しません**。モデル未共有とモデル内の Miner／Panel の全結託を想定し、Miner の資料供給、独立選出 Panel の非対話 positive、不一致時の対話 negative、検証成立後の成功 Final を定義します。global pool の独立選出と正直な検証者の前提は残ります。[設計の入口](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md) / [公開裁定の方針 v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md)。

## 目的別の入口

| 目的 | 最初に読む |
| --- | --- |
| ビルド・ノード接続・採掘 | [はじめに](getting-started/README.md) |
| ノード・モデル・RPC を運用 | [運用](operations/README.md) |
| PALW の claim・court・bond・fence を CLI で読む・object を出す | [PALW CLI ガイド](guides/palw-cli.md) |
| 全体の構成を理解 | [アーキテクチャ](architecture/overview.md) |
| 固定する構造・責務とモデル別DAG並列生成を確認 | [PALW Architecture Specification](architecture/palw-specification.md) |
| 採用済み規則と適用条件を確認 | [仕様](spec/README.md) |
| API・設定・manifest を参照 | [リファレンス](reference/README.md) |
| 採用理由・技術設計・未決事項を読む | [ADR](adr/README.md) / [RFC](rfc/README.md) / [新設計の採択 ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) / [詳細設計](design/README.md) |
| 試験・監査・証拠・履歴をたどる | [testing](testing/README.md) / [audit](audit/README.md) / [evidence](evidence/README.md) / [history](history/README.md) |

## 設計を読む順序

[PRINCIPLES](PRINCIPLES.md) → [全体像](architecture/overview.md) → [PALW のトピック別仕様](spec/README.md) → 必要な ADR / RFC。
モデル階層と旧審査廃止は RFC-0017 / ADR-0179 の採択済み改定を優先します。`α=1.5`・7日平均（PRINCIPLESと後続承認記録に従う）・bond 予算・claim 認証は今回撤回していません。

## 文書を更新する人へ

[凍結規程](governance/design-freeze.md) / [パラメーター台帳](governance/parameter-register.md) / [文書の役割](governance/document-lifecycle.md) / [テンプレート](governance/templates/README.md)。

[2026-11-08 の変更・反映手順](maintenance/revisions/2026-11-08/README.md) / [今回の検査結果](maintenance/revisions/2026-11-08/VALIDATION.md) / [番号台帳](maintenance/numbering.md)。

[以前の整理・反映手順](maintenance/REORGANIZATION.md) / [旧パスから探す](maintenance/path-map.md) / [以前の検査結果と残存課題](maintenance/validation-report.md)。
