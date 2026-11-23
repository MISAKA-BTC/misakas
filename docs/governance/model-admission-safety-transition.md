# MAS1 — モデル追加時の検査とBFT安全停止の適用・移行

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


2026-11-21改訂。新規採掘のG-COSTには、現世代の固定条件での反復費用検査を適用する。未検査はUNKNOWN、再現された安価な正規TicketはFAILとし、合格の範囲と未検査条件を記録する。[RFC-0062 §4.3](../rfc/0062-palw-function-output-and-work-admissibility.md#g-cost-review)


<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](qwen-fow1-profile-transition.md)。QF1をMAS1の具体審査対象として追加する。実行形式の具体化だけで未知の費用gateを解除しない。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md) / [ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)
- 基礎: [TB1/FOW1移行](palw-tb1-fow1-transition.md) / [MOE2移行](model-option-economics-transition.md)

## 1. 今回の権限と優先関係

ユーザーの明示指示により、安価な予測・再利用を使うモデルの採掘を防ぐ追加検査と、自動判定不能な危険に対する限定BFT停止を採択する。本契約は、MAS1を適用する新しいFOW1採掘資格の範囲で、旧「モデル単位の検査不採用」「品質審査なし」「BFTを一切使わない」の包括的表現を置換する。

一般登録の自由、モデルIDの不変性、モデル非保有Node、Panel＋Court、計算validity proof不使用、TB1の乱数源、MOE2、既存の署名済み責任は維持する。停止担当は台帳finality・fork choice・reorg・既確定資産への権限を持たない。

## 2. 入口の一本化

UI/SDK/RPC/native登録・import/repair・snapshot/IBD・miner・Panel・Final・reward・credit・weight・permit・受賞権利消費は、RFC0065の同じEffectiveMiningAllowedを読む。別API、owner変更、同一artifactの別名、burn Resume、fork指定で安全停止を回避させない。元genesisはBFT管理・停止の対象外であり、後付けGENESIS指定を拒否する。

MAD2では有効なAUTO又はBFT YESがない状態は採掘不可。自動審査のUNKNOWN/採用判断FAILを保持したまま、技術互換とq YESでBFT承認できる。停止と個別claim条件は共通であり、全自動gate PASSをBFTへ再要求しない。

追加モデルのModelRegisteredは市場生成と原子的に成立する。MAS1の未適格は「市場が未準備」という状態ではない。市場取引、既確定残高、永久LPの意味を安全停止で変更しない。

## 3. 非報酬評価とBFTの接続

候補・suiteをfreezeした後の独立TB1-MODEL_AUDITで評価入力を作る。提出結果固定後の別TB1-PANELで検証担当を選ぶ。評価は非報酬であり、未適格モデルが自己の採掘資格や停止rosterを作る循環を持たない。control容量と予算を事前予約する。

自動UNKNOWNはAUTO経路で保留する。技術互換なら明示的なMODEL_ADMISSION proposalを作りYES/NOへ進む。稼働モデルの停止は別のMODEL_STOPであり、採用票と混同しない。

停止rosterは事前確定TB1 holder rosterに明示されたMODEL_STOP権限とし、n=3f+1、q=2f+1、別fault前提を持つ。TB1 share証明と停止署名は別domainである。PublicOneHonestからBFT故障上限を推定しない。

## 4. 停止・解除と適用順序

MAS1用の新しいreducerは、安全停止・反例・rule失効を同batchの成功sweep、Final、発行、権利消費より前に反映する。旧profileの再生順は変えない。停止中も元期限・資料・Court・返還・責任精算を継続する。

artifact/profileの解除は現在の採用basis、現在rosterのq署名、正確なStopId集合・control recordへの束縛を必要とする。AUTOは現行全PASS、BFTは共通技術条件と現在YESを使う。kernel/checker停止はどちらでも免除せず、採用定理だけの失効からBFTへ切り替える場合も明示proposalと停止scopeの確認を必要とする。

## 5. activationと旧責任

1. 本改訂は設計採択のみ。公開設定・配備・支払の変更ではない。
2. 共通checker・実行契約・roster・fault前提・codec・容量を固定する。AUTOはTheoremRuleとsuiteを必要とし、BFTはMODEL_ADMISSIONの正規YESを必要とする。BFTへ定理・測定証拠を要求しない。
3. 一つのバイナリで受付から停止・復帰・Final・権利消費まで接続し、restart/reorg/IBDで状態一致を検証する。Qwen用定理が未完成なら合格モデルとして扱わない。
4. 明示activation recordの境界以後の対象profileに適用する。既登録モデルの新規資格にも検査を行い、過去登録を自動PASSにしない。
5. 境界前に正規受理された旧claimは固定したlegacy drainと元期限で終える。旧仕事の新世代への付替えや二重計上を禁止する。
6. 条件が未完成、BFTが未構築、又は適格モデル集合が空なら、新profileのactivationは成立しない。旧乱数・旧採掘へfallbackせず、MAD2の正規BFT採用を旧MAS1の仮PASSへ読み替えない。

## 6. 残余リスクの公表

MAS1は、認識済みの危険を排除し、不明な新規候補を閉じ、後発の危険へ停止手段を与える。全ての安い算法の自動検出、一般LLMの費用証明、未知入力での品質、停止までの被害ゼロを保証しない。

停止の認証と危険の正しい理解を区別する。BFTによる誤停止・停止遅延、資料不足・検閲、native合意前提、suiteの過学習と登録候補選別を実装・運用の評価対象として保持する。
