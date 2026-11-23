# ADR-0229 — 全対象乱数源をPALW内部閾値beaconへ統合する

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](../governance/model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。非報酬評価の独立MODEL_AUDITと、別権限のMODEL_STOPを追加する。台帳finality用BFT及び旧BLS乱数候補は復活させない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0061](../rfc/0061-palw-internal-threshold-beacon.md)
- 関連: [RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md)、[RFC-0060](../rfc/0060-model-registration-and-attempt-output-binding.md)、[RFC-0062](../rfc/0062-palw-function-output-and-work-admissibility.md)

## 背景と今回の明示指示

ユーザーは前回のRFC/ADR監査で指摘された保証対象・登録・Ticket・再利用・移行の修正を指示し、乱数の発生方法を添付 `Misaka_PALW_TB1_Design_Bundle(1).zip` へ変更することを明示した。この指示を、新profileの**設計要件の採択**として記録する。runtime変更、全暗号合成の証明、公開activationの完了とはしない。

添付のTB1 RFC-0060は旧snapshotに対する提案番号だった。既存のMOR1 RFC-0060を保持し、正式なTB1統合先をRFC-0061とする。ADR-0229は未使用番号であり本記録へ割り当てる。native wire tag、suite/profile ID、activation fenceは別の有効化記録で確定する。添付原文・レビュー・参照結果は当時の状態として保全する。

## 判断1 — 乱数役務の中核

TB1-BGW-PQを選ぶ。完全BGW VSS、PQ認証私信、署名付きshare点の公開復元certificateから構成する。`n=3f+1`、私的復元`t=f+1`、公開certificate`q=2f+1`を区別する。全qualified dealerの固定集合Qを復元し、無応答者を後から除外して別の乱数にしない。

ML-DSA-65、ML-KEM-768と認証・鍵確認・AES-256-GCMを組み合わせるchannel、SHA3-512 context、SHAKE256 seedというsuite要件を採用する。具体wire・KDF・nonce永続化・標準訂正・全分岐・合成の検証は未完であり、PQという名称だけで完成認定しない。

保持者はshare役務だけを担う。BFT validator finality、DNS-final、BLS長期閾値署名、Groth16、LLMのvalidity proofを導入しない。nativeが供給する共通履歴・安定性・包含は別の前提であり、holder多数決へ移譲しない。

## 判断2 — 用途を閉じる

| 用途 | 決定 |
| --- | --- |
| EXEC | 全C/nonce候補固定後の独立秘密から実行入力を導出 |
| PANEL | Original/pool/DrawPolicy固定後の別秘密から選出 |
| SCHEDULE | 認証済みcredit集合と未来slot規則を固定して独立秘密から割当 |
| ROLE | 前epoch保持者の独立secretで次rosterを選出。自己依存なし |
| ADMISSION | seed順位を持つ別の新control profileに限る独立用途。AN1/FOW1へ追加抽選しない |
| Court個別challenge・生成sampling | 正規PANEL又はEXEC/sから登録済み規則で派生。未来乱数と偽称しない |
| Ticket | 実行依存の固定式。第三の未来Ticket beaconを設けない |
| negative・資料case・Court・終端 | 新beacon待ちを課さない |

全用途の生成方式をTB1へ統一することと、全経路へ新しい抽選を追加することを区別する。AN1/FOW1の候補処理順は実行依存Ticketとslotの固定順を維持する。旧受付beaconを必要とする別control profileだけがADMISSIONを使う。

添付の「前PANELから次rosterを派生」は、公開前に次候補も閉じた場合に限定された方式だった。本統合では独立ROLEへ置換し、SCHEDULE/ROLE/ADMISSIONの追加sessionを未実証の検査範囲として明記する。添付29テストを追加用途の検証済み根拠にしない。

Panelの重み・復元/非復元抽選・担当人数・予備順は固定DrawPolicyへ置く。乱数源変更だけで既存policyを暗黙変更せず、m枠をm人の独立主体とみなさない。

## 判断3 — 最初のshare、停止、曝露を扱う

freezeはcertificate完成時刻ではなく、**最初の正直share放出より前**にnativeの安定条件を満たさなければならない。全員同時受信や、公開開始後の先行計算利益を保証しない。

旧seed、block/header hash、work-fold、seal-reveal、BLS、別round/suite/keyへの自動fallbackを禁止する。公開済み秘密はreorgで未公開へ戻せない。曝露journal、別freezeへの再公開拒否、影響attemptの未成功終端、新材料・新登録による明示回復を要求する。cold importで別forkの漏洩不存在が証明されたとは扱わない。

公開certificateは同じ多項式に乗るq点と全Qを要求する。holderは自分のshareへ署名し、最初の候補多項式へ投票を固定しない。単なる最初のt点や未成立の候補polyとの不一致でslashしない。

## 判断4 — LLM関数の保証と分離する

TB1は入力の事前予測・抽選操作に対する条件付きの乱数契約である。LLM関数の採掘値が非衝突であること、別試行に十分な新規計算が必要なこと、通常GPU時間・電力を支払ったこと、正直Panelの存在は証明しない。

FOW1の関数出力・費用条件はRFC-0062に従う。MOR1のreceiptをLLM関数の評価困難性へ読み替えず、TB1を導入した事実だけで費用gateを省略しない。

## 判断5 — 置換範囲と旧履歴

新TB1対象profileについて、未指定又は旧work-derivedのEXEC/PANEL source、seed付きschedule、必要な受付順位をRFC-0061のpurposeへ置換する。旧SWC1の追加R_ticketと旧BFT/BLS候補は新AN1/FOW1に採用しない。旧署名済みclaim・paid lock・schedule・creditは、その元codec・原期限・責任でdrainする。

ADR-0180のPALW-only/no-BFTは維持する。ADR-0185の旧paid-lock乱数は新sourceの代替にしない。ADR-0226のBLS候補DISABLED、ADR-0227の未指定beacon、ADR-0228の実beacon参照は、新profileの範囲で本RFCへ接続する。旧原文を当時からTB1だったかのように改ざんしない。

移行fence、対象登録、全consumer、旧新受付境界、DB/cold import/replayを閉じる。切替後の新規対象が旧routeへfallback/downgradeして成功する経路を拒否する。切替により既受理negative/Court/終端の処理機会や元予約を失わせない。

## 実装・有効化と未解決条件

採択したのは添付方式を統合した設計要件である。完全BGW/PQ通信、native broadcast/phase、量子・適応・並行合成、bootstrap/fault bound、全用途の容量、control進行、深いreorg/cold import、全consumerの移行は未完である。

添付の29件参照テストと103,152算術ケースは別の検査であり、本番暗号・本番ネットワークの実証ではない。ROLE/SCHEDULE等の追加用途を含むRFC-0061 §14の全gateと、FOW1の独立gateが揃うまで公開有効化しない。失敗時は未成功終端と既受理救済を維持し、別の乱数方式で成功へ補完しない。
