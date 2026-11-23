# RFC-0058 — 脅威・残る前提・受入条件

日付: 2026-11-17（Asia/Tokyo）  
状態: **PROPOSED_DESIGN_NOT_ACTIVATED**  
規範案: [RFC-0058](../../../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)  
実装計画: [staged-work implementation plan](../../../design/palw/integration/staged-work-implementation-plan-2026-11-17.md)  
コード監査基準: `main@2d8212b634880b0967fbf24b00b41936c2322c14`

## 1. 読み方と結論の境界

本表は、ユーザーが提示した理論監査と追加対処案を、既存契約・追加作業・残る前提・受入条件へ変換したもの。原PDFのページや数式がworkspaceで確認できていない場合、引用された監査文章から旧実装の脆弱性成立を断定しない。wire入力、値の生成依存、canonical性を固定sourceで確認する。

「既存」は規範または固定sourceの足場があることを意味し、RFC-0058の新経路が接続済みという意味ではない。「追加」は未採択の接続案である。「条件付き」は明示したhonest actor・乱数・同期・包含等の前提が必要である。「未成立」はこの文書の作成では解消していない。

全modelの整数実行が完全一致することは前提とする。それでも、正しい結果の検証、当選の予測困難性、計算費用の下界、第三者への実際の資料供給、DAGの全体安全性は別の命題である。

SWC1の順序は、経済TrialSlotとowner/funding/beneficiaryを持つ`WorkOpportunity`の事前固定、`R_job`、実jobの完全実行、元rootを含む`OriginalId`の固定、`R_ticket`、admission cohort固定、`R_panel`である。WorkOpportunityはRFC-0058のJobRegistration／OpportunityIdの経済試行概念であり、独立した新wire IDではない。`ComputationalWorkKey`は実model/program/content/rulesと実入力・初期状態・範囲から導き、外装を除外する。以下のWorkKey／Originalはこの区別に従い、推論途中に新たなsegment乱数方式を追加する意味ではない。

## 2. Ticket、仕事identity、乱数

| ID / 深刻度 | 攻撃または誤った主張 | 対策・既存契約との接続 | 残る前提・限界 | 必要な受入証拠 |
| --- | --- | --- | --- | --- |
| WB01 / Critical候補 | 推論前にticket全入力を安価に作り、当選候補だけLLM実行 | 新版ticketを正規の完全実行commitとWorkKeyへ束縛し、段階commitの依存を固定 | 旧式のfield名だけでは先読み可否を判定できない。任意rootをhashできることと、有効rootを得られることを分ける | 固定wireの値生成graph、実行前情報だけでは有効当選を作れない条件、shortcut試験 |
| WB02 / High | 同じ実行から署名・nonce・padding・木形状を変えて多くのticketを作る | 正規trace encoding、immutable work identity、一回の権利、可変wrapperをticketの仕事入力から除外 | 決定論的算術だけでは証拠bytesの正規形は決まらない | 等価encoding、別署名、leaf順序、空field、別carrierでの同一性vector |
| WB03 / High | 同じ仕事を別model ID・別claim・別relay・別登録で再包装して二重credit | WorkKey、Original、receipt、消費identityの用途を分け、work重複をnative stateで処理 | 別modelの意味的同値を一般に判定できるとは主張しない。共通profileに従う | 仕事identity再利用、複数包装、batch・cancel・再送の一回会計 |
| WB04 / High | 後続challengeを先に知り、前段commitを都合よく選ぶ | 各canonical cutにR_job/R_ticket/R_panelのcontextを束縛 | 現行beaconの偏り・公開拒否・branch選別の評価が別に必要 | opportunity/Original/cohort各固定cutとseedの前後境界、異なるbranch、複数候補seed、stage省略 |
| WB05 / High | 最終結果を見る前にPanel集合を知り、不正が通る実行を選ぶ | Panel選出のR_panelを最終実行commitとadmission cohort固定後へ束縛 | 抽選の独立性と、安価な再試行機会を制約する必要がある | 最終root変更、複数Original、seed公開後の取消・再提出・資本付替え |
| WB06 / High | 一回の`f^m`だけで全期間の安全性を主張 | pool、抽出法、distinct主体、敵対比率、累積機会数を明記 | union boundは独立性なしでも使えるが、適応的選別を含めた各機会の確率上界が必要。無制限候補なら有限Nを仮定できない | native試行台帳からN上界を導き、サンプリング法に合う失敗確率を評価 |
| WB07 / High | BFT committeeのfault boundをglobal Panelへそのまま流用 | 同一母集団・同一fault単位と証明できない限り転用不可 | stake比率、人数、復元抽出位置数は互換ではない | pool snapshot、membership、stake/headcountの対応、重複draw |
| WB08 / High | seed障害・bootstrap停止をゼロseedや自由なblock hashで回避 | 原clock・原予算内の回復または未検証終了。既存caseは予約で継続 | serviceとbeaconがclaim自身の未認証workに依存すると循環が残る | 初回claim、全新規受付停止、seed停止、再開、既存反証継続 |

## 3. Panel、nonce、資料供給、court

| ID / 深刻度 | 攻撃または誤った主張 | 対策・既存契約との接続 | 残る前提・限界 | 必要な受入証拠 |
| --- | --- | --- | --- | --- |
| WB09 / High | 他PanelのCourtを再署名し、別の義務を履行したことにする | 0057の元claim/root・主体・round・role・scope・nonceをstateから再導出 | 公開資料の再利用や外注まで排除できない | 別主体、別round、別root、鍵rotation、同主体重複draw、relay差替え |
| WB10 / High | 正しい局所証拠だけをコピーしてFullReplay承認する | full scope承認とLocalHoldsを区別。元rootへの有効反証を全positiveより優先 | 正しい元実行なら、省略した事実だけをnonceで立証できない | 正しい部分を持つ誤った全traceに対し、正直な一人が反証しscope付き責任だけ適用 |
| WB11 / High | 「正直な身元が一人」を「一人が完全検査して反証できる」と同義にする | PublicOneHonestの能動的完全検査・資料取得・期限内包含・裁定を明記 | 身元の正直さだけでは稼働、資料、費用、時間を保証しない | cold Panel一人、他全員非協力、供給・費用・包含を含む成功と反証E2E |
| WB12 / High | 全Panelのnonce challenge完了を必要として非署名者に拒否権を与える | 最低完全承認数1と全選出者の責務を両立し、他人の未完で新Qを作らない | 正常完了は既存の全case終結とone-honest前提に依存 | 正しいclaim、正直者一人のみ承認、他全員沈黙でも元期限内に成功 |
| WB13 / High | 正直者だけへ資料を隠す／小chunkと別人のACKで受領扱いにする | 0018 §2.4の当人・全scope・残検査時間・包含機会を確認 | 有界な客観的手順が未確立ならPROFILE PROCEDURE NOT ESTABLISHED。署名やbeaconだけでは解消しない | 選択的秘匿、部分応答、偽ACK、別人の正常受領、虚偽未着の識別手順 |
| WB14 / High | 資料の遅い応答でnotice窓を閉じ、その後の検査を新規異議として拒否 | 元MaterialRequestと同じCaseIdの取得後検査・局所化・反証を継続 | 元の有限予算と検査時間内に収まらないなら成功不可 | 最後の合法要求、窓後資料取得、元scope内反証、残時間不足のAbort |
| WB15 / High | 最大合法の虚偽異議でcourtと包含容量を消費 | 0055の全branch・全actor・最後の合法要求の予約、有限case/turn/bytes/work | 難易度だけでは既受理責任を保護できない。包含serviceの前提が必要 | 他全員最大合法caseと正直者caseを同時進行し、原期限と予算を検査 |
| WB16 / High | 高D・新seed・positive多数で既受理反証を止める | 0056を新規責任受付に限定。完成反証は元権利・予約で処理 | 無限の未認証packet受信そのものは消せず、local資源上限が必要 | D最大、seed停止、ingress洪水下でも適格な旧caseが進む混合負荷 |
| WB17 / High | 不正packetにより被害者のQ・nonce・budget・手番を消費 | bounded precheck、主体認証、期待scope、原子的受理。invalidは当該packetのみ拒否 | parser、quote、index更新の前段費用も上界が必要 | invalid-first/valid-later、重複parts、署名失敗、巨大長、heap/CPU計測 |
| WB18 / Medium | 不応答・開示失敗・根不一致だけで計算不正slashを付ける | ComputationRefuted、QualifiedDeliveryDefault、ServiceInterrupted、AbortedUnverifiedを分ける | 帰責は署名済み義務・客観的機会・元責任窓に限定 | 通信障害、悪意provider、正直challenger、LocalHolds署名者、relayerの分類 |

## 4. Final、credit、DAG、永続化

| ID / 深刻度 | 攻撃または誤った主張 | 対策・既存契約との接続 | 残る前提・限界 | 必要な受入証拠 |
| --- | --- | --- | --- | --- |
| WB19 / High | TicketEligibleまたは手続きFinalだけで信用・合意weightを与える | 0020/0048の認証・成熟・依存・資本・取消gateを新版全consumerへ接続 | 旧Finalと新版論理phaseを同じwireに読み替えない | 全consumer読取一覧、未認証sourceのmint・weight・depth・participation・settled anchorが0 |
| WB20 / High | 同batchの反証より先にEXECをconsumeして保全対象にする | 0020 §5.2の反証/Q/取消優先をmint・reserve・schedule・consume・feeまで適用 | 過去batchのconsumed保全と現在候補を区別 | body内順序の全排列、provisional先行、旧permit、同batch依存取消 |
| WB21 / High | Final後取消を、全過去取引の無条件rollbackまたは取消無効果と解釈 | 未消費失効、consumed provenance、通常reorg、settlement policyを別管理 | 不正仕事の過去fork-choice影響の安全性は別のDAG証明義務 | 未使用・予約・既消費混在、source取消、reorg、取引効果の一貫したundo/replay |
| WB22 / High | immutable snapshotだけを見て、現在のQ／失効を無視して消費 | historical cutと現在native eligibilityの両方を検査 | snapshot認証は将来の取消不存在を保証しない | 古いquote/permit/index、取消generation変更、同batchhold、再開後再検査 |
| WB23 / High | maturity期限と訴追期限の境界の隙間で信用を確定 | notice/response/adjudication/liability/retention/maturityのinclusive/exclusiveを同版で固定 | 現legacyのterminal court制限と新Final後訴追経路は別に接続が必要 | 直前・同値・直後、同DAA複数batch、overflow、case継続、依存責任翌tick |
| WB24 / High | 無期限取消を許しながら、pruned/retired floorより絶対戻らないと主張 | 取消可能性、成熟、floor、pruningを一つの版付き権利へ整合 | 期限後証拠の扱いと定理の安全性範囲を明記する必要 | late conviction、floor更新、archive欠損、pruned IBD、旧権利保全 |
| WB25 / High | seed／draw／verdictだけを別branchから再利用する | work・case・once・予約・credit・consumerをcanonical単位でundo/replay | reorgでgrindingが絶対不可能になるとは主張しない | partition/rejoin、深いreorg、重複再包含、古いworker、restart/IBD |
| WB26 / High | 反証やbootstrapのserviceを自身の未発行信用で支払う循環 | 0020 §11Aと0055の先行control/service予約を新版stageへ接続 | 空network・不十分work供給・seed供給の活性条件は別に示す | 初回完了、credit枯渇、全新規受付停止、既存caseの原期限内終了 |
| WB27 / Critical未証明 | frontier再計算とmaturityがあるのでDAG安全性は証明済みとする | fork-choice全入力、認証遅延、取消、分断、pruningを固定して別定理を要求 | common-prefix、収束、活性は本改訂では未成立 | 正式な前提・定理／反例解析、攻撃simulation、実network試験を区別して提出 |

## 5. 費用、model増加、authorityの変更

| ID / 深刻度 | 攻撃または誤った主張 | 対策・既存契約との接続 | 残る前提・限界 | 必要な受入証拠 |
| --- | --- | --- | --- | --- |
| WB28 / Critical未証明 | 正しいtraceがあるので規範FLOPs分の費用を必ず払ったとする | 最安の有効trace構築費用を対象に、cache/batch/簡約/事前計算/外注を分析 | 全modelの計算費用下界・非償却性は未成立。局所反証で正しい高速化を罰せない | 複数claim・複数model・前計算を含む費用/成功率と限界の報告 |
| WB29 / High | 新modelを1万個追加するとmodelごとに同じglobal枠が増える | 0055の共通資源、0056の共通D、元workと資本の一回性 | 登録の無審査・1.0/0.3は維持。登録件数から性能を推定しない | 1万/5万model、burst、cold miss、index/GC、全model走査を含む最低Node測定 |
| WB30 / High | 1 BPSを達成するため成熟・court・反証を省略 | 外側carrierの目標とclaim検証/settlementを分け、原予約を優先 | 供給不足・seed停止・資源不足では空slotがある。無理なBPS補填はしない | 全候補分母、空slot、未完了、backlog、credit在庫、期限超過を含む持続測定 |
| WB31 / High | BFT beacon案を既存承認として導入し、DNS-BFTを別名で復活 | PRINCIPLESと0010の退役を維持。別の例外採択がないadapterはdisabled | randomness authorityと取引settlement authorityは別変更。今回自動採択なし | 未採択adapterを全入口で拒否、DNS veto/署名quorumが新資格を与えないこと |
| WB32 / High | beacon実装のためPQ-onlyを破り未承認署名を追加 | 承認されたPQC scheme/domainのみ。未割当adapterは不使用 | PQ署名の有無だけではunbiased beaconや計算正当性を証明しない | scheme/鍵/署名domain/codec/最大bytes、malformed署名とdowngradeの拒否 |
| WB33 / High | 欠ける費用下界をmodel審査や係数変更で解決済みにする | 無審査登録、固定二階層、claim共通profileの境界を維持 | 一般modelの安価な特殊構造は理論評価に残る | 未見modelが同じ入口を通り、個別benchmark/取得認定が隠れた資格にならない |
| WB34 / Medium | docs検査や抽象モデルのPASSをruntime・実測・理論証明へ昇格 | 設計／native接続／E2E／容量測定／理論／activationの証拠を分離 | ファイルの整合は実装の安全性を示さない | 各成果物に対象source/binary/policyと未達項目、実行した検査だけを記録 |
| WB35 / High | R_jobをhash metadataへ書くだけで同じ実推論から多数の新仕事を得る | R_jobをprofileが定める実job入力・初期状態・実行へ結合し、ComputationalWorkKeyへ実内容を反映 | 異なる実入力でも共有prefixや簡約が可能で、非償却性は別問題 | 同じ元job、R_job差替え、計算に影響しないmetadata、実入力が変わらない包装を検査 |
| WB36 / High | 経済TrialSlotだけ変え、同じ仕事の所有者や受益者を後付けで選び直す | WorkOpportunityのowner/funding/beneficiaryをR_job前に固定し、仕事とOriginalの対応をnativeに保持 | WorkOpportunityと計算内容の同一性は別。実資本の一回性も独立に必要 | 同一slot別鍵、署名差替え、payer/beneficiary変更、別opportunityでの同じWorkKey再利用 |

## 6. 全体に適用する必須invariant

1. **同一仕事:** 同じ正規実行から、carrier・署名・nonce・再提出・取消を用いて追加の仕事、ticket機会、信用、報酬を作れない。合法に別試行となる場合は、その元資本・試行枠・費用・scopeをnativeが区別する。
2. **認証前の成功停止:** ticket適格、局所pass、資料公開宣言だけでFinal・信用・合意資格を作らない。成功判定は現在のnative stateから導く。
3. **単独反証:** 適格な元scopeの有効反証は、正しい局所証拠・肯定多数・新D・新seedより優先する。反証自体が常に可能であるとの仮定は別記する。
4. **有限の資源:** 新しいstage／nonce／modelを追加しても、元のglobal予約を増やせない。既受理責任の終端と正直者の救済を原時計で処理する。
5. **受領の区別:** materialの同一性・保存署名・個別chunkの認証から、特定の検証者の完全検査機会を推定しない。
6. **取消と一回性:** 同batchの失効を先に反映し、未消費を止める。過去consumedを再発行せず、canonical reorgは関連状態を一体で復元する。
7. **未証明を残す:** 実費下界、資料の客観的終結手順、DAG定理が未成立なら、その状態を明記してprofileの公開有効化判断に渡す。

## 7. 最小の統合受入シナリオ

### A. 不正元実行とコピー承認

同じWorkKey／Originalについて、Minerが誤った完全traceをcommitしticketを得る。悪意Panelは正しい局所資料を利用してFullReplay承認を出す。正直Panel一人が元実行全体を検査し、本人challenge集合外を含む有効な局所反証を提出する。Nodeは元rootを照合してRefutedとし、同batchの成功・mint・consumeを防ぐ。新しい成功資格が残らず、署名scopeに対応する既存の責任だけを一回適用する。

### B. 正常元実行と非協力多数

正しい整数実行、coldな正直Panel一人、残り全員不署名、さらに最大合法の虚偽caseを組み合わせる。資料・検査・包含・裁定を原予約と元期限内で満たし、正常Final、成熟、正規credit、schedule、permitの一回消費まで到達する。全拒否・全Abortはこの試験の成功にならない。

### C. 選択的資料秘匿

他Panelへの供給は成功させ、正直者一人へ必要scopeを隠す。URL、別人のACK、数個のchunk、beacon署名があっても、当人のinspection prerequisitesを成立させない。適格なMaterialRequestを保護し、元scopeの検査継続へ進むか、成功不可の未検証Abortへ終える。算術不整合未立証なら計算有罪にしない。

### D. 遅延反証と複数consumer

同じsourceから未使用credit、予約permit、過去batchでconsumedの権利が存在する状態で、scope付きhold／反証を受理する。未消費は停止・取消、過去消費はprovenanceを保持し、同batchの新消費は拒否する。その後のcanonical reorgでは関連履歴を一体復元する。frontier、weight、participation、settlement fact、pruningが同じ版を読む。

### E. 全model負荷とservice継続

1万/5万modelの登録stateを背景に、invalid packet、適格候補、quote、全stage、継続court、通常取引、GC、restartを混合する。全model共通Dが新規責任を絞り、既存caseの反証・終端・返還を妨げず、Nodeの上限を守る。空slot・未完了・stock・backlogを含めて外側1 BPSの実績を報告する。

## 8. 固定source・既存規範の参照

- [RFC-v2-0010](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0010-dag-consensus-and-native-settlement.md): PALW authority、DNS-BFT退役、included/provisional/BASE accepted/settledの区別。
- [RFC-v2-0018](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0018-interactive-negative-court-and-defaults.md): MaterialRequest、取得後検査、InspectionPrerequisitesEstablished、客観default。
- [RFC-v2-0020](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md): 同batch反証優先、成功停止、全consumerの取消、Final後権利、非循環。
- [RFC-0050](../../../rfc/0050-public-materials-and-one-honest-panel.md): 資料手順未確立時のprofile有効化禁止。
- [palw_native_settlement_v1.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_native_settlement_v1.rs): canonical snapshot、maturity、依存責任、settlement fact。
- [palw_security_certification_v1.rs](https://github.com/MISAKA-BTC/misakas/blob/2d8212b634880b0967fbf24b00b41936c2322c14/consensus/core/src/palw_security_certification_v1.rs): 旧認証の前提、frontier再計算、floor、pruning rollback bound。

本表は新しいtest runnerの実行結果を含まない。各受入条件の達成は、対応する固定binary・policy・観測記録で後から検証する。
