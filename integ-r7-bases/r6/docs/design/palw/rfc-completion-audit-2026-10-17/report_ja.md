# RFC01–20 完成条件に対するコード不足監査（RFC04・05除外）

監査日: 2026-10-17。対象は新しく添付された `misakas-pre(1).zip`。RFC本文だけでなく末尾の改定、実コードのconsumer/caller、最新の残件記録を照合した。

## 0. 結論と対象の同一性

**完成を阻む具体的な実装欠落・接続不足がある。特に、FP receiptのGate C前の生成/支払い、RFC16のDA/court制約の情報落ち、PALWART3 identity、native責任期限、公開追及の容量・時計は、単なる「テストしていない」問題ではない。**

新旧ZIPのSHA-256はともに `c13be519c006ac707d2c3f2ae1dcb468b50695844121f1e4c6e0321b92e9f22e`。ファイルの追加・削除・変更はなく、今回のZIPにはユーザーが説明した別branchのcourt容量/detection-policy統合済み変更は入っていない。`palw_court_capacity_v1.rs` と `g14_drill_factory.rs` はこの展開物に存在しない。別branchの出来栄えを本ZIPから判定しない。

**前提:** 関連fenceは有効とみなす。`None`やarming recordの空リストそれ自体だけを未実装欠陥に数えない。ただし、fenceを有効にしても消えない未接続、明示的Unsourced拒否、必要な測定値の未設定は残件とする。新規コードは書き換えていない。

**スコープ:** RFC04/05は監査対象外。genesisは現行8モデル・text generation・32,768 context。画像/音声/動画などの全RFC拡張完成と、genesisを出すための必須条件を分離した。撤回済みモデル審査・配布PoR等を復活させない。#10の実court資源測定の免除を、新しい必須試験に戻さない。

## 1. 判定の意味と検証の限界

- **不合格（具体的欠落）:** 必要な分岐がない、consumerが呼ばない、対象を拒否する、または数式の反例がある。
- **受入未完:** コードの存在は確認したが、仕様が要求する実ノード/実artifact条件の試験が未実装又は未完。コード欠落を見つけたという主張とは分ける。
- **本体に追加欠落を特定せず:** 確認した現行要件とコードは対応する。全MUSTを実行合格した意味でも、未確認箇所にバグがない証明でもない。
- **提案/範囲外:** 未採用の提案、延期済み最適化、text-only genesisの対象外。通常のrelease blockerと合算しない。

今回実行したPython drill unit testsは **129 passed, 176 subtests passed**。これはdriverと判定器の試験で、kaspad起動やRust consensus testではない。cargo/rustcが本環境に無いため、Rustビルド・test実行・実32K推論・fence越えnode drillは未実行。既存記録の赤い試験数を「今回失敗した本数」とは表示しない。

ソース行番号と原文は [source_evidence.md](source_evidence.md)。file SHAは [source_manifest.json](source_manifest.json)。独立した算術/静的チェックは [static_checks.json](static_checks.json) と [reproduce_static_checks.py](reproduce_static_checks.py)。提案した受入試験は、既存で合格した試験ではない。

## 2. 18 RFCの完成判定

| RFC | 判定 | 完成を阻む項目／残る確認 |
|---|---|---|
| 01 | 本体の追加欠落は未特定／受入未完 | 01-A。01-Bは提案・最適化で別枠。 |
| 02 | 不合格：identityとruntimeの接続不足 | 02-A/B。PALWART3登録/court、FP semantics注入。 |
| 03 | 全profile未完成／text genesisは別 | 03-A。Audio/Video job lifecycle。 |
| 06 | 不合格：独立filerの接続不足 | 06-A。held GDN checkpointのDA→告発自動化。 |
| 07 | 不合格：event/fixture/測定未完 | 07-A/B。07-Cはstagedへの拡張範囲。 |
| 08 | 不合格：受理/支払いが正常終端しない分岐 | 08-A/B。08-CのseamなしE2E。 |
| 09 | 不合格：Rule E競合でremote L2停止 | 09-A。operational minerのV2/V3 reader化。 |
| 10 | 独自primitive欠落未特定／統合未完 | 10-Aと07-A/B、14-B/C。 |
| 11 | 全scopeの完成条件未達 | 11-A。full-task/full-context censusと該当profile。 |
| 12 | 不合格：必要read/failpoint/CLI不足 | 12-A。Rule E header rootは接続済み。 |
| 13 | 不合格：長系列比較tool/実artifact受入不足 | 13-A/B。short/toy evidenceでは32Kを満たさない。 |
| 14 | 不合格：検出→裁定→権利失効が全経路で閉じない | 14-A–Fと18-A。 |
| 15 | 本体predicateに追加欠落未特定／依存未完 | 15-A。Panel=0の実資源・支払い・告発。 |
| 16 | 不合格：未接続＋容量計算の具体的不整合 | 16-A/B/C。DA/court minが供給量で失われる。 |
| 17 | 現行本体の静的要件は対応／統合受入は未実行 | 17-A。旧審査の再実装は不要。 |
| 18 | 不合格：FP生成/支払いgateとnative後訴追不足 | 18-A/B、14-F。 |
| 19 | モデル追加editionの受入未実装 | 19-A。R7/R8。edition0 switchとは別。 |
| 20 | 現行最適化実装はある／実規模受入未完 | 20-A。attentionタイル・KV diskを必須不足としない。 |



## 3. 修正優先順位

第一に **18-A（FP生成/支払いgate）・16-B（資源制約情報落ち）・08-A/B（責任に対応する受理/支払い）** を閉じる。次に **02-A/B、06-A、14-B/C/D/F、16-A** の実経路接続と追及容量を共同で修正する。**14-Aの検査kを先に増やすだけでは、証拠取得/court処理の飽和を悪化させ得る。** その後07/09/10/12/13/19のconsumer・tool・drillを揃え、同一commitで正常系と不正系を実行する。

各不足は以下のIDで一意に管理し、複数RFCにまたがる同じ欠落は重複した実装タスクにしない。P0は主要な安全性・正常完遂を阻む項目、P1は完全な受入/運用/校正の項目、P2は拡張/提案。P0という表示は、全項目が即時に盗難へつながる脆弱性という意味ではない。

## 4. RFC別の具体的不足と完成条件


### RFC01 — 本体の追加欠落は未特定／受入未完


#### 01-A [P1] decode・prefixの実経路受入を完了させる

**分類:** 受入試験の未完。**対象:** 現行要件。関連RFC: 14。

**満たすべき要件:** RFC01 §6(2)–(5)、mission amendment: produce/replayのbit一致、公開反証、prefix/batch同値、fence越えFinal。

**不足位置:** `docs/rfc/0001-palw-inference-surface-gaps.md:370–377; consensus/core/src/palw_step_refute.rs:5959,5969; consensus/core/src/palw_state_v2/transition_builder.rs:5792–5793; misaka/palw/base0/src/fp_worker.rs:1256`

**何が成立しないか:** constraint courtやrender自体が無い、という旧記録は現コードに合わない。一方、drill producerが存在することだけで、改定後の全fence・実class・公開告発・Final/支払いの組合せを合格にできない。

**必要な変更:** 既存misaka-palw-rfc1-drill-claimとdrill shellを使い、decode rules/constraint2/prefix/inheritのマトリクスを現ノードへ接続。非対応classは名前付き拒否を維持する。

**終了条件・追加試験:** 正しいtokenは第三者反証されない。mask/penaltyを無視したtokenは他panelの票なしで裁定される。fresh=resume、single=batch、fence前後のstate root一致を同一試験で検査する。

**根拠:** [S01](source_evidence.md#s01), [S36](source_evidence.md#s36)。


#### 01-B [P2] 整数seedとprefix計算削減は本体欠落と分ける

**分類:** 採用前提案・最適化。**対象:** 提案・後続最適化。genesis必須欠陥には計上しない。関連RFC: なし。

**満たすべき要件:** RFC01 §7.3残(最適化・凍結後)、2026-10-12 W1は明示的な提案。

**不足位置:** `misaka/palw/gateway/src/surface.rs:124–134; misaka/palw/gateway/src/main.rs:1168–1193; RFC01:410–426`

**何が成立しないか:** 整数seedはdecimal文字列へ変換された後、64-hexを要求する経路で拒否される。prefix leafの共有は実装済みだが、計算省略とseat cacheまで完成したという評価はできない。これは新規報酬の安全性が破綻したという指摘ではない。

**必要な変更:** W1を正式採用する場合はle64(n+1)とoverflow拒否を一箇所に実装し、既存hex仕様との区別を固定。prefixは承認済みinheritance-safe classだけでexecutor短縮とメモリ上限付きcacheを実装。

**終了条件・追加試験:** seed境界0/最大値/overflowのgolden vector、cold/warmの出力・leaf root・課金同値、cache破棄後の再現を検査。

**根拠:** [S01](source_evidence.md#s01)。


### RFC02 — 不合格：identityとruntimeの接続不足


#### 02-A [P0] PALWART3の配布identityを登録とcourtへ貫通させる

**分類:** 未接続。**対象:** 現行要件。関連RFC: 11, 13, 17, 19。

**満たすべき要件:** genesis8 catalogの現行要求: class artifact rootはPALWART3 container rootを拘束し、courtはその下のinventoryを認証する。

**不足位置:** `consensus/core/src/palw_genesis8_catalog_v1.rs:23–25; misaka/palw/sdk/src/tir_layout.rs:827–837; misaka/palw/sdk/src/tir_registration.rs:145–148; consensus/core/src/palw_gdn_court_v2.rs:110–111`

**何が成立しないか:** 通常TIR entryはinventory_rootをartifact_rootにし、登録はその値を署名する。catalogが期待するcontainer identityと既存の登録/court identityが一致する契約になっていない。root型名が同じでも同じ対象ではない。

**必要な変更:** container版・inner inventory root・manifest等の拘束をversioned registration/class identityへ追加。PALWART3 membership openingを検証してからinner operand proofを検証する共通readerをSDK、node、court、remoteに接続。旧class rootの解釈は変更しない。

**終了条件・追加試験:** 正しいcontainerで登録→実claim→第三者court。manifestだけ/inner rootだけ/operandだけの変更と別containerからの移植を拒否。restart/IBD/reorgで同じclass/rootを復元。

**根拠:** [S02](source_evidence.md#s02), [S16](source_evidence.md#s16)。


#### 02-B [P0] TIR FP backendに登録済みsemanticsを渡す

**分類:** 未接続。**対象:** 現行要件。関連RFC: 14。

**満たすべき要件:** RFC02 Phase H: FP job/frameのruntime/model/CU semanticsはclass registrationと一致する。

**不足位置:** `misaka/palw/tir/exec/src/node/backend.rs:460–465 (with_fp_class_facts_v1)`

**何が成立しないか:** 注入APIはあるが、対象production treeを走査した出現は定義1件のみ。nodeが登録semanticsをこのAPIへ渡す接続は見当たらない。デフォルトと登録値が偶然一致するfixtureでは検出できない。

**必要な変更:** chainのPalwClassSemanticsV1を読み、backend作成時にnetwork IDとFP class factsを注入。class切替・reorg・restart時の更新も同じreaderを使う。

**終了条件・追加試験:** 非デフォルトmanifest/rulesetを持つIR classを通常登録し、FP claimのcontext/frame一致とFinal/支払いを実ノードで確認。1フィールドだけ異なるframeは拒否する。

**根拠:** [S03](source_evidence.md#s03), [S22](source_evidence.md#s22)。


### RFC03 — 全profile未完成／text genesisは別


#### 03-A [P2] Audio/Videoのjob bodyから終端までがない

**分類:** 未実装。**対象:** RFC03全機能の不足。text-only genesisの必須障害ではない。関連RFC: 11。

**満たすべき要件:** RFC03のAudio/Video出力形式・裁定・保存・支払いを含む全profile完成。

**不足位置:** `consensus/core/src/palw_gen_job_v1.rs:139–158 (PalwGenBodyV1); palw_gen_step_v1.rs:30; RFC03:2214–2282`

**何が成立しないか:** job bodyはImage/Embedding/Headのみ。Audio/Videoの名前やlowererがあっても、canonical jobをdecodeして実行・公開反証・決済する完成経路ではない。video等ではleaf上限の扱いも必要。

**必要な変更:** 各profileのversioned body、PCM/codec/frame順序等のoutput拘束、input/output DA、局所court、費用・retention・payoutを追加。multi-codebookは既存single-codebookへ流用せず版を分ける。

**終了条件・追加試験:** profileごとに正しい出力のFinal/支払い、時系列順/codec/header/1区間の改ざん、DA沈黙、予算超過をテスト。単なるfrontend smokeを合格にしない。

**根拠:** [S04](source_evidence.md#s04)。


### RFC06 — 不合格：独立filerの接続不足


#### 06-A [P0] held GDN checkpoint不正を追う自動DA filer

**分類:** 未接続。**対象:** 現行要件。関連RFC: 02, 14。

**満たすべき要件:** RFC06の独立局所化: producerの非公開heapなしに状態遷移を公開資料から裁定へ運ぶ。

**不足位置:** `consensus/core/src/palw_gdn_court_v2.rs:30–40,91–115; docs/design/palw/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md:119–123`

**何が成立しないか:** GDN verdict/libraryの証拠構築はあるが、kaspadでsessionを開き回答を集める自動filerが未作成と記録され、対応proof-kindのdaemon接続も見つからない。Q36 held routeのcheckpoint不正を自動追及できるという保証が途切れる。TIR-native GDNとは別経路。

**必要な変更:** checkpoint mismatch→gdn_transition_demands/conv-window demands→署名DA session→回答検証→GDN accusation→courtをservice/outboxへ接続。沈黙は適格DA defaultへ送る。

**終了条件・追加試験:** 個々の局所stepだけは整合する偽checkpointを置く。全seat共謀・外部bond1つ・cold cacheから、通常回答と沈黙の両方で実担保減額まで確認。

**根拠:** [S05](source_evidence.md#s05)。


### RFC07 — 不合格：event/fixture/測定未完


#### 07-A [P1] PanelLicensed Finalの正しいevent attribution

**分類:** 未接続。**対象:** 現行要件。関連RFC: 10, 18。

**満たすべき要件:** RFC07 source provenance/selected approvalと依存・非循環性の拘束。

**不足位置:** `consensus/core/src/palw_kernel_route_v1.rs:843–875 (finals_read_v1)`

**何が成立しないか:** PaidClaimV2のselected-approved eventは接続済み。非paidのPanelLicensedはseed/epochをrouteが知らずevent=None。PanelLicensedをPanelIndependentに偽装して穴埋めしてはいけない。Panel drawの契約が拒否するsourceは、接続後も拒否しなければならない。

**必要な変更:** 認証済みPanel assignment、seed、epoch、round、dependent profileをroute receipt/eventへ拘束し、正しいFinalPathを出す。利用consumerごとのsource exclusionを保持する。

**終了条件・追加試験:** 正のselected-approved paid sourceだけを使うfixtureへ移行。timeout-only/未承認/自己依存sourceではbeacon不成立。PanelLicensed eventのseed/epoch差し替えを拒否。

**根拠:** [S06](source_evidence.md#s06), [S12](source_evidence.md#s12)。


#### 07-B [P1] m/k・検査費用・fixtureを現approval規則へ揃える

**分類:** 未測定・試験未完。**対象:** 現行要件。関連RFC: 18。

**満たすべき要件:** RFC07の検査被覆・条件付き誤差・実参加前提、およびGate Cの抽選承認。

**不足位置:** `docs/design/palw/remaining-work-2026-10-16.md:13–21,42–47; palw_kernel_route_v1.rs:859–868`

**何が成立しないか:** 旧OPV-onlyの約45件は最新規則ではbeacon源にならないという移行記録。これは45件の本番バグを今回再現したという意味ではない。m/kを未測定のまま固定しても、1正直検証主体の処理量や実効検出率は導けない。

**必要な変更:** fixtureはassignされた正のquorumとbound source/roundを含む共通builderへ変更。検査する全関係、fault model、cold-fetch/CPU/DA/courtの実測からpolicy値を固定。

**終了条件・追加試験:** fixtureの修正前後に未承認sourceを通す抜け道がないことを確認。抽選quorum成立率と外部の検出・裁定率は別に測る。全suiteを同一commitで再実行。

**根拠:** [S06](source_evidence.md#s06), [S35](source_evidence.md#s35)。


#### 07-C [P2] staged paid claimには現trap保証を拡張できない

**分類:** 対象外分岐が明示。**対象:** 現行non-staged限定を超える拡張。自動的にgenesis blockerとはしない。関連RFC: 08。

**満たすべき要件:** 現T6 trap実装はnon-staged paid V2限定。stagedまで同じlazy-verifier耐性を主張する場合の追加要件。

**不足位置:** `consensus/core/src/palw_paid_claim_trap_v1.rs:7–12,291–303`

**何が成立しないか:** interactive=trueのtrap revealは明示的に拒否される。通常の詐欺証拠による裁定まで無いという意味ではないが、stagedにもtrapによる誘因保証が実装済みとは言えない。

**必要な変更:** stagedを保証対象にする場合、roundごとのcommit-before-challenge、秘密fault、receipt/reveal期限、fee holdとsigner liabilityをversioned transcriptへ追加。現限定を維持するならcoverage表に除外を明示。

**終了条件・追加試験:** 前roundを後から変更、future seed先読み、途中中断、最終roundだけ不正のケースを試験する。

**根拠:** [S07](source_evidence.md#s07)。


### RFC08 — 不合格：受理/支払いが正常終端しない分岐


#### 08-A [P0] native claimの署名付き責任期限が未定義

**分類:** 受理経路の欠落。**対象:** 現行要件。関連RFC: 02, 14, 15。

**満たすべき要件:** RFC08 combined liability predicate: 資金・最大責任期限・資料TTLを認証済み入力から導出して受理する。

**不足位置:** `consensus/core/src/palw_settlement_liability_fold_v1.rs:456–462; palw_bond_budget_fold_v1.rs:2509,2571,2638,2741`

**何が成立しないか:** native funded/text/job/miningの対象分岐はfence後にUnsourcedで拒否する。これは安全側の拒否であり、不正が通る脆弱性ではないが、全native正常系が完成していることとも両立しない。

**必要な変更:** 各canonical objectにsigned hard deadlineとmaterial TTLを加え、予約K/R、bond hold、retire deadlineを導出。builder/RPC/producer/node同じ関数で認証する。拒否ガードを削って通す修正は禁止。

**終了条件・追加試験:** 正常native claimが受理→Final→支払い。hold不足/TTL不足/署名改ざん/期限直前の告発は正しく拒否又は完遂。kernel側admissionも同じpolicy/court予算を通す。

**根拠:** [S08](source_evidence.md#s08)。


#### 08-B [P0] Unsourced feeが零額終端される

**分類:** 支払い経路の欠落。**対象:** 現行要件。関連RFC: 07, 14, 15。

**満たすべき要件:** RFC08 exactly-once settlementと支払いに対応する有限の責任保全。

**不足位置:** `consensus/core/src/palw_monetary_material_fold_v1.rs:788–809; palw_kernel_route_fold_v1.rs:953–973,1064–1089; palw_settlement_liability_fold_v1.rs:349–379`

**何が成立しないか:** provider fee、非対応drawn verifier fee、beacon source feeで責任のsourceが未定義。predicateは拒否時にzero-amount markerを作るため、単なる後払い待機ではなく当該obligationが閉じられる。Paid V2 trap対象のfee holdは既に実装されており一律欠落ではない。

**必要な変更:** 各legに実際のclaim/reservation、署名期限、資料TTLを結び、保留・解除・default・返金を有限の状態遷移にする。未成立時にretryすべき債務と取消済み債務を区別し、別consumerによる二重払いを防ぐ。

**終了条件・追加試験:** 正直provider/selected verifier/sourceの報酬が実UTXOへ一度だけ支払われる。途中convictionでは所定保留が没収される。同じobligationをkernel tick/EXEC/物理nodeが消費しても二重払いしない。

**根拠:** [S09](source_evidence.md#s09)。


#### 08-C [P1] EXEC REAL admissionをtest seamなしで通す

**分類:** 本番経路の受入不足。**対象:** 現行要件。関連RFC: 12, 14。

**満たすべき要件:** RFC08/14のnativeノード端から端: 通常の登録・readiness・per-bond制約を経てEXECが成立する。

**不足位置:** `consensus/src/pipeline/virtual_processor/processor.rs:293–305 (exec_v2_test_admit_class_v1)`

**何が成立しないか:** cfg(test)のREAL route admission hookはregistry/seating/per-bond shareの一部を迂回する。これ自体は本番バイパスではない。ただしこのhookを使ったgreenだけで通常ノード経路を完成認定できない。

**必要な変更:** 通常のclass registration、family certification、seat readiness、bond shareを準備するnode fixtureを作り、test hookなしで同じEXEC試験を実行する。

**終了条件・追加試験:** 正常EXEC Final/支払いと、不正EXEC root/prefix双方のproducer・covering signerへの実slash、restart/IBD/reorg後の一致を確認。

**根拠:** [S10](source_evidence.md#s10), [S24](source_evidence.md#s24)。


### RFC09 — 不合格：Rule E競合でremote L2停止


#### 09-A [P1] remote L2 minerをV2/V3 committed leafへ更新する

**分類:** 未接続。**対象:** 現行要件。関連RFC: 12, 18。

**満たすべき要件:** RFC09 L2の現在のfork-choice規則に対応した検証とfail-closed signing。

**不足位置:** `misaka/palw/remote/src/miner.rs:510–564; remote/src/l2.rs:313–321,940–944`

**何が成立しないか:** 新opening verifierは存在するが、運用minerはVec<PalwForkChoiceOpeningV1>→verify_fork_choice_v1。Rule Eが有効な競合ではLeafV1InsufficientでSTOPする。誤forkを採用するバグではなく、現在の規則下でnode-less minerが選択できない未完成経路。

**必要な変更:** RemoteNode/RPC opening取得、attestation、candidate chain window、comparatorをfenceで選ばれるV2/V3 viewへ移行。native nodeと同じroot reader/comparatorを使い、旧V1fallbackで新規則を推測しない。

**終了条件・追加試験:** Rule E/Gate C有効の2forkを実RPCから取得し、full nodeとremoteの選択一致。改ざんopening/欠落provenanceでは署名しない。単一chainの既存動作を退行させない。

**根拠:** [S11](source_evidence.md#s11), [S15](source_evidence.md#s15)。


### RFC10 — 独自primitive欠落未特定／統合未完


#### 10-A [P1] permissionless panelの正常完遂を新beacon契約で確認する

**分類:** 統合受入不足。**対象:** 現行要件。関連RFC: 07, 14。

**満たすべき要件:** RFC10のpublic discovery/auto binding/replacement/finalizationと非循環source。

**不足位置:** `consensus/core/src/palw_panel_v3_fold_v1.rs:153–164,504–544; docs/design/palw/remaining-work-2026-10-16.md:19`

**何が成立しないか:** production foldと告発中のreceipt clock停止は実装済みで、reference-only評価は誤り。残るのは07-A/Bによるsource接続・fixture移行と、飽和/blackout下の実service完遂。新たなpermissionless primitive欠落は今回特定していない。

**必要な変更:** paid selected approvalを本当に成立させたsourceからauto binding/replaceまで動かす。手動Panel injectionを使わない。告発中のclock pause、無人口時の非有罪終了を保持する。

**終了条件・追加試験:** 正常claimはFinal/支払い。検証者追加・離脱・beacon blackout・DA告発中の再抽選でも不正claimが中立終了して担保を逃がさず、正常claimが永久停止しない。

**根拠:** [S06](source_evidence.md#s06), [S12](source_evidence.md#s12)。


### RFC11 — 全scopeの完成条件未達


#### 11-A [P1] full-task/full-context onboardingの成功率契約を満たす

**分類:** 範囲未完成・受入不足。**対象:** 現行要件。関連RFC: 02, 03, 06, 13, 14, 20。

**満たすべき要件:** RFC11現行§18: D_completeは全task・提供する全contextでon-chain registration、片側95%下限90%以上。未対応architectureも分母に残す。

**不足位置:** `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md:684–719; misaka/palw/sdk/src/census/onboarding.rs:90–143`

**何が成立しないか:** FrontendRequired/KernelExtension/ProfileRequired/NotRunを区別するコードはある。Audio/Video profile欠落、実32K artifact/registration/court未完を除外して成功率を作ることは仕様違反。8つのtext-only genesis成功だけでもRFC11全体の達成にはならない。

**必要な変更:** 未対応profile/architectureを明示し、02/03/06/13/14の該当経路を閉じる。pinned censusから実変換→登録→実task/context→必要なFinalまで駆動する証拠を揃える。既に廃止されたmodel審査は復活させない。

**終了条件・追加試験:** D_all/D_complete/mining-funded Finalを別集計。short prompt、shape-only、toy、NotRunをfull successへ昇格させない。成功率と信頼区間を生の対象一覧から再計算可能にする。

**根拠:** [S13](source_evidence.md#s13), [S16](source_evidence.md#s16)。


### RFC12 — 不合格：必要read/failpoint/CLI不足


#### 12-A [P1] reserve read・WAL failpoints・refund操作を実装する

**分類:** 運用API・受入実装不足。**対象:** 現行要件。関連RFC: 08, 09。

**満たすべき要件:** RFC12受入checkbox 3/5/7: import crash recovery、bridge exactly-once、DNS retirement reserve整合。

**不足位置:** `scripts/drills/misaka-rfc12-acceptance.py:62–78,985–1000`

**何が成立しないか:** driverが必要とするblock別reserve残高、WAL barrier間の正確なcrash point、認証snapshotからのpaid request生成、outpoint UTXO readがない。deposit-refund CLIもなく、外部command未指定だと明示BLOCKED。これはbridge本体が必ず誤動作するという主張ではない。

**必要な変更:** read APIとprivate-test failpoint、署名付きrefund CLI/paid request builderを実装。driverはaddress残高の近似だけでなく同じoutpointの遷移を判定する。

**終了条件・追加試験:** 各WAL barrier前後でkill/restartし二重credit/消失なし。timeout deposit refundは1回だけ、reorg/IBDで同一。reserveの期首+流入−流出=期末をブロックごとに照合。

**根拠:** [S14](source_evidence.md#s14)。


### RFC13 — 不合格：長系列比較tool/実artifact受入不足


#### 13-A [P1] legacy A16/PALWQ361の長系列window comparator

**分類:** 未実装。**対象:** 現行要件。関連RFC: 02, 11, 20。

**満たすべき要件:** RFC13再現可能なonboarding evidence、現行genesis全モデル32K。

**不足位置:** `docs/design/palw/genesis8-status.md:185–197; docs/rfc/evidence/0013-onboarding-tool-validation.json:19,58–59,93,101–109`

**何が成立しないか:** #8のqwen36-run --referenceは8-token prompt用で、長系列の途中windowをf32と比較する道具が未作成。#1のFAITHFULも57-token範囲。これを32Kの意味保存と数えることはできない。

**必要な変更:** legacy engineのf32/intを同じ全prefix stateで進め、複数絶対位置windowを逐次比較するtoolを追加。window外を切り捨てて短contextへ変えるのではなく、logits保持だけを制限する。

**終了条件・追加試験:** 冒頭・中盤・末尾でtop-1/KL等の採用済み基準を満たす。state/position/rope差し替えを検出。短いfixtureのPASSと32K実測を別の証拠種別で記録する。

**根拠:** [S16](source_evidence.md#s16), [S17](source_evidence.md#s17)。


#### 13-B [P1] 実genesis artifactを再生成して配布identityまで検証する

**分類:** 成果物再現・受入未完。**対象:** 現行要件。関連RFC: 02, 11, 19, 20。

**満たすべき要件:** RFC13のlayout/index/runtime packの再現性と登録・実行 parity。

**不足位置:** `docs/design/palw/remaining-work-2026-10-16.md:51–59; docs/rfc/evidence/0013-onboarding-tool-validation.json:58–59,93,109,182`

**何が成立しないか:** 保存記録にはartifact喪失と未変換/fit停止が残る。既存tool evidenceもpack_fully_verified=false、registration_submitted=false、node parity未確立。これはbyte-levelツールが全て未実装という意味ではない。

**必要な変更:** 固定source revision・stats・manifestから対象artifactを再生成しsha256/root一致を確認。runtime packの全segment検証と02-Aのcontainer登録を通す。失敗/中断したfitをindex公開成功と扱わない。

**終了条件・追加試験:** 独立作業環境で同じ固定入力から同じartifact/index/root/opening。破損segment/truncated packは拒否。実ノード登録と実classの32K claimが同じidentityを使う。

**根拠:** [S16](source_evidence.md#s16), [S17](source_evidence.md#s17), [S35](source_evidence.md#s35)。


### RFC14 — 不合格：検出→裁定→権利失効が全経路で閉じない


#### 14-A [P0] 固定4区間を対象不正に対応した検出policyへ置き換える

**分類:** policy未実装。**対象:** 現行要件。関連RFC: 01, 07, 16, 18。

**満たすべき要件:** ユーザー目標: 区間検証による見逃し1–10%を許容するが、定義した不正についてその水準に達する。

**不足位置:** `consensus/core/src/palw_fp_interval_v1.rs:25–28,39–70; kaspad/src/palw_fp_seat.rs:391–428`

**何が成立しないか:** 固定k=4ではfault密度が低い場合、90–99%検出を満たさない。同じclaim/seat/beaconで再実行しても同じ区間。新court-capacity/detection-policyモジュールはZIPに存在しない。全TIR/kernelがこの4区間方式だという意味ではない。

**必要な変更:** class/profileごとにN,k,故障モデル,目標miss率、検査bytes/CPU、coverage、期限をcommitに拘束。新roundはcommit後challengeとし選択grindingを防ぐ。検出/取得/court予算をclaim admission、kernel admission、Final schedulerと共有する。

**終了条件・追加試験:** dense/sparse/1区間不正を別々に注入し、実測率と理論値を比較。正直1検証主体で受理claim量を処理できる。未検査/UnverifiableをValidに変換しない。

**根拠:** [S18](source_evidence.md#s18), [S35](source_evidence.md#s35)。


#### 14-B [P0] 共有court枠の飽和を越えて追及を完遂する

**分類:** 容量制御不足。**対象:** 現行要件。関連RFC: 06, 10, 15, 16。

**満たすべき要件:** RFC14 G07: 適格な外部告発が他claimによる枠占有だけで期限を失わない。

**不足位置:** `consensus/core/src/palw_state_v2/objects.rs:2002–2010; da_sessions.rs:752–760; consensus/core/tests/g14_close_legacy.rs:539–564`

**何が成立しないか:** 共有court session上限でCourtSessionsAtCapacityになる残件テストがある。対象claimのFinalは予約で保留されるため、これだけで攻撃者への報酬取得は示していない。ただし発見→客観裁定→slashの完遂が保証されない。

**必要な変更:** 実測court moves/block、期限付き公平scheduler、占有時間rent、claim admission総量の整合。連続再充填攻撃でも既に受理した正当追及へ処理枠を確保する。単に42を大きくしない。

**終了条件・追加試験:** 満杯+継続再充填の中で正直1bondの不正追及が責任期限前に完了。正常claimもFinal/支払い。計算/メモリ/DAの最大値、返金と重複slashなしを検査。

**根拠:** [S19](source_evidence.md#s19), [S22](source_evidence.md#s22)。


#### 14-C [P0] 全activeのTIR owner占有とnative owner枠

**分類:** 取得経路の容量不足。**対象:** 現行要件。関連RFC: 02, 06, 15。

**満たすべき要件:** RFC14: 公開資料を必要とする外部追及に、期限内の取得経路を用意する。

**不足位置:** `consensus/core/src/palw_tir_public_fold_v1.rs:618–639; palw_native_text_fold_v29.rs:1160; palw_flat_execution_court_fold_v11.rs:107–110`

**何が成立しないか:** TIRはidle回収があるが8owner全てactiveなら新Open拒否。native v11はjobのretained seal/discoveryを合計4枠、v29はlifetime ownerを4枠に制限する。v39には回収があり、全nativeを未実装とはしない。証拠を既に持つdirect filingまで一律遮断されるとの主張ではない。

**必要な変更:** public witnessの再利用/共同取得、bounded ownershipの終了条件、transfer/reclaim、追及優先度を実装。新ownerが必要な経路と不要なdirect proofを別に容量会計する。

**終了条件・追加試験:** 全ownerを実際に進行させ続ける敵対ケースでも、正しい外部追及に取得か等価の公開証拠経路がある。idleだけのテストで代用しない。

**根拠:** [S20](source_evidence.md#s20), [S21](source_evidence.md#s21)。


#### 14-D [P0] TIR frame cost/clockを有限の責任予算へ合わせる

**分類:** 実用上限を満たさない価格・時計。**対象:** 現行要件。関連RFC: 02, 08, 15, 16。

**満たすべき要件:** RFC14 T-N1/公開追及と、ユーザーの正常時約1日目標。

**不足位置:** `docs/design/palw/g14-completeness-proof-2026-10-16.md:978–1080; docs/design/palw/remaining-work-2026-10-16.md:23–40`

**何が成立しないか:** 最新記録でもoutside ownerのOpen/collect期限とrentが実用的な責任期間から大きく外れる。古い算術overflow/二重計上の修正は入っており、それと残る高すぎるcost/clockを混同しない。記録の例示corpus値は今回実測した全32Kモデル値ではない。

**必要な変更:** 現実のcustodian最大数、各move/fragment/close/replay、cold fetch/inclusionを測り、preparation・acquisition・court・liabilityの上限を1つのdeadline式で閉じる。正常解放と係争中の必要担保を分離。T-N1実ノードharnessへ02-Bを接続。

**終了条件・追加試験:** 通常IR FPが登録→Active→frame grant→検証→Final→実支払い。責任期限直前の不正は担保退出前に裁定。正常時24h目標を実ノードの進行時間で検査し、停止/検閲時の条件は別途明示。

**根拠:** [S22](source_evidence.md#s22), [S03](source_evidence.md#s03)。


#### 14-E [P1] Panel Active/Resolvedの二段階担保

**分類:** 資本効率の未実装。**対象:** 保全バグの未修正ではなく、効率・時間目標の残件。関連RFC: 16。

**満たすべき要件:** ユーザーの約1日目標と、G13責任期限までの担保保全を両立する設計。

**不足位置:** `docs/design/palw/remaining-work-2026-10-16.md:27,33; G14 completeness proof:880,903`

**何が成立しないか:** G13のlockを責任期限まで伸ばす修正は既に入っている。残るActive/Resolved分離は未実装という最新記録。単純にFinal+1000で全解放へ戻すとlate convictionの徴収を失う。

**必要な変更:** 新規work用のactive capacityと、Final後も必要なresolved liabilityを別台帳にし、R_newの計算と実slash残高を整合。長い係争と正常claimの資本再利用を別条件にする。

**終了条件・追加試験:** 多数の正常Finalの後も必要late liabilityが残る。新claim/退出で二重利用されない。late convictionで実担保を徴収し、同時に正常claimの資本効率を測る。

**根拠:** [S22](source_evidence.md#s22), [S35](source_evidence.md#s35)。


#### 14-F [P0] native v39 mining Final後を対象にするcourt

**分類:** 告発経路の欠落。**対象:** 現行要件。関連RFC: 18。

**満たすべき要件:** RFC14のFinal後責任期間と、RFC18 Gate Cの未解決告発/権利失効。

**不足位置:** `consensus/core/src/palw_security_certification_fold_v1.rs:489–492`

**何が成立しないか:** native側はMaster Finalがstandingで、native mining claimを名指すchain accusationがないためaccusation_pending=false固定。component jobのFinal前courtがあるだけでは、Master Final後の不正を同じ主張で追及できない。

**必要な変更:** mining claim→component execution/root/有責bondの認証linkとretained evidenceを用意し、post-Final告発、pending状態、verdict、budget/receipt/credit失効へ接続。未確定のnegative veto人数の変更で客観裁定を代用しない。

**終了条件・追加試験:** Master Final後・責任期限内に外部1bondが不正を局所化し、producer担保減額と該当receipt/報酬/credit失効。境界直前後と再構築・reorgを試験。

**根拠:** [S23](source_evidence.md#s23)。


### RFC15 — 本体predicateに追加欠落未特定／依存未完


#### 15-A [P0] Panel=0を実用資源・全liabilityで完遂させる

**分類:** 依存経路の未完。**対象:** 現行要件。関連RFC: 08, 14, 16。

**満たすべき要件:** RFC15 kernel OPVの公開検証・Challengeable/FinalRevocable/Settled区別。V2Attempt全体をPanel不要にする仕様ではない。

**不足位置:** `consensus/core/src/palw_kernel_settlement_v1.rs; docs/design/palw/rfc-0015-implementation-status-2026-10-15.md:16–19,130–134`

**何が成立しないか:** RFC15独自のmode binding/settlement predicateは追えており、新しいライフサイクル欠落は今回特定していない。しかしcourt枠、公開取得、fee期限、検証予算が未完なので、単にPanel=0の単体testがあるだけでは実用completionにならない。kernel B=F=0のserviceを勝手に物理mining権利と読み替えない。

**必要な変更:** 08-A/B・14-B/C/D・16-A/Bを閉じ、通常registrationとfunded jobを使うPanel=0のnode acceptanceを作る。RPC/SDK/UIはsettlementと検査のassuranceを同じpredicateで表示する。

**終了条件・追加試験:** Panel不在・正しいpublic verifier1つ・実profile最大負荷で、正常Final/支払いとpre/post-Final不正の実slash。no-challengeを算術証明と表示しない。

**根拠:** [S26](source_evidence.md#s26), [S09](source_evidence.md#s09), [S19](source_evidence.md#s19)。


### RFC16 — 不合格：未接続＋容量計算の具体的不整合


#### 16-A [P0] capacity attestationをproduction admissionへ取り込む

**分類:** 未接続。**対象:** 現行要件。関連RFC: 07, 14, 15, 18。

**満たすべき要件:** RFC16 V8: compute/DA/court/memoryと有効期限付き証拠で検証能力を評価し同じhardwareを二重計上しない。

**不足位置:** `consensus/core/src/palw_rfc0016_v1/capacity.rs:240–336; palw_rfc0016_fold_v1.rs:286–314`

**何が成立しないか:** attestation型・署名message・expiry checkerはあるが、供給導出の呼出は同ファイルのtestsのみ。実foldは一律shipped_per_spanをready bondへ配る。実際の機械能力に基づくadmissionではない。

**必要な変更:** 署名付きcarrier、fold/state/snapshot、expiry/revoke/read APIを追加し、rfc0016_capacity_read_v1を実attestation/resource allocationへ変更。quote/claim/kernel OPV/court admissionが同じ予約を読む。

**終了条件・追加試験:** 能力低下/期限切れ/虚偽or欠落証拠で新規受理量を抑制。同じverifierが複数modelを宣言してもbudgetは増えない。reorgで期限/供給/予約が復元。

**根拠:** [S27](source_evidence.md#s27), [S28](source_evidence.md#s28)。


#### 16-B [P0] DA/courtボトルネックがsupply変換で失われる

**分類:** 算術上の情報落ち。**対象:** 現行要件。関連RFC: 14, 15, 18。

**満たすべき要件:** RFC16 V8: 能力はcompute・DA・courtの最小、memoryはfit条件。

**不足位置:** `consensus/core/src/palw_rfc0016_v1/capacity.rs:300–311,318–336,32–55`

**何が成立しないか:** claims_per_spanはminを返すがsupply側は0かだけを見てcompute_ccu_per_span全量を戻す。単一model、compute=1000、DA=50、court=3000、claim cost=(10,10,1000)なら許されるのは3件。しかし後段は1000 CCUを配り、10件=100 CCUの需要を受理可能と判定する。現在未接続でも、16-Aだけ実施するとこの不整合を運用へ持ち込む。

**必要な変更:** per-verifier/per-modelにcompute/DA/courtの多資源制約を残した割当へ変更する。単一modelの最低条件はcapacity<=min-resource claims×claim compute。複数modelで単なる個別min合算をして共有DA/courtを二重計上しない。

**終了条件・追加試験:** 添付の提案Rust回帰testとPython独立再計算。DAだけ/ courtだけ/ memoryだけ不足、異なる負荷の複数model、期限切れを通す。合計割当は全資源で各verifierの上限以内。

**根拠:** [S28](source_evidence.md#s28)。


#### 16-C [P1] 測定値をproduction recordへ固定する

**分類:** 本番parameter未完成。**対象:** 現行要件。関連RFC: 07, 14, 18。

**満たすべき要件:** RFC16 measured-value gateと3段階の係数・容量決定。

**不足位置:** `consensus/core/src/palw_rfc0016_v1/values.rs:79–100,184–204`

**何が成立しないか:** production valuesはk=1以外0のplaceholder、measured recordsは空。activation=None自体は今回の欠陥に数えないが、fenceが有効という仮定だけで必要な非ゼロ容量/queue/OPV予算が生まれるわけではない。calibration開始値は本番承認値ではない。

**必要な変更:** 実traceでσ、安全係数、queue長/TTL、verifier share、OPV rate等を導出し、evidence/review digest付きrecordを固定。時間目標・14-A policyと共同で設定する。k=1によるtilt無効は現方針なら維持。

**終了条件・追加試験:** ゼロ能力・過負荷・回復、境界DAA、複数classを同時稼働。校正用envが本番設定を黙って上書きしないことを試験。

**根拠:** [S29](source_evidence.md#s29)。


### RFC17 — 現行本体の静的要件は対応／統合受入は未実行


#### 17-A [P1] tier係数の一度だけ適用を統合試験で閉じる

**分類:** 現行本体に追加欠落を特定せず。**対象:** 本体の静的対応を確認。ビルド/全受入合格を宣言する項目ではない。関連RFC: 02, 19。

**満たすべき要件:** 現行RFC17本文冒頭のみ: genesis/annual=1.0、他登録=0.3即時固定、review撤廃、allocationで一回だけ係数適用、登録全体cap。

**不足位置:** `consensus/core/src/palw_model_tier_v1.rs:42–48; palw_bond_budget_v1/allocation_v3.rs:300–311; palw_model_eligibility_fold_v1.rs:34–41; palw_rfc0016_fold_v1.rs:138–143`

**何が成立しないか:** 係数、旧審査のunreachable化、entrant reserve無効化は現仕様に対応。旧RFCの試験・renewalが無いことを欠陥としない。残る外部identityは02-A、年次適用と実モデル切替は19-A。これらと実行未確認を別に残す。

**必要な変更:** 新しいtierアルゴリズム追加は不要。既存testsへannual activation×budget×rights×coinbaseの同一chain統合ケースを加え、旧model review経路が復活しないことを固定する。

**終了条件・追加試験:** 一般登録直後0.3、時間だけでは昇格せず、掲載editionのallocation rollで1.0。genesis1.0保持。登録model分割で全体capを回避しない。報酬側で係数を二重適用しない。

**根拠:** [S30](source_evidence.md#s30)。


### RFC18 — 不合格：FP生成/支払いgateとnative後訴追不足


#### 18-A [P0] FP receiptをGate C前に使わせない

**分類:** 支払い・生成gateの欠落。**対象:** 現行要件。関連RFC: 08, 14。

**満たすべき要件:** RFC18 Gate C: 発行報酬・合意creditを未認証claimへ与えない。ユーザー目標の不正claim由来の生成/報酬拒否。

**不足位置:** `consensus/core/src/palw_fp_admission_v3.rs:181–188; palw_state_v2.rs:19481–19498,19574–19586; consensus/src/pipeline/virtual_processor/processor/palw.rs:10044–10075,4289–4300; processes/coinbase.rs:227–232`

**何が成立しないか:** 通常FP V3/V4 receiptはFinalを確認するがSecurityCertifiedを要求しない。foldはGate C未認証のweightを0にするだけでquantumを消費する。receipt blockのworker報酬は別coinbase経路にある。convicted rootの後続使用拒否は既にあるが、認証前の使用/支払いと同じではない。

**必要な変更:** receipt pre-check、template/RPCのspendable表示、fold、blue/red/selected-parent coinbase entitlementに共通Gate C predicateを通す。未認証は使用を拒否するか、明示された別escrow状態へ保留し、weightだけの0で済ませない。

**終了条件・追加試験:** FinalだがGate C pending/no-population/rejectedのclaimで有効ticketと署名を作ってもreceipt生成/coinbaseを得られない。certified後は正常支払い。late convictionとsibling/reorgを含む。

**根拠:** [S31](source_evidence.md#s31)。


#### 18-B [P1] held issuance releaseを所定の分配へ接続する

**分類:** 分配仕様・実装不足。**対象:** 現行要件。関連RFC: 08, 16。

**満たすべき要件:** RFC18 U-2/報酬分配: producer/Panel share/buyback/vestingの扱いを決定し、合意の発行総額を保存。

**不足位置:** `consensus/core/src/palw_security_certification_fold_v1.rs:362–403; docs/design/palw/rfc-0018-implementation-status-2026-10-15.md:40–41`

**何が成立しないか:** 保留報酬は現在row.payout_payloadへの単一leg。horizonまでのrelease延期は既に実装済みで、即時払いの旧指摘は取り下げる。ただしPanel share等の分配を含めたRFC全体完了には未決・未実装が残る。

**必要な変更:** U-2の配分をversioned policyとして確定し、既存reward split/vesting/settlement-onceへ接続。二重配賦・新たな無担保報酬を作らない。仕様未決なら勝手な比率で実装済みにしない。

**終了条件・追加試験:** 各payoutの受取人・合計額・保留期限・没収・丸め・重複consumer・reorgを照合。sum(outputs)が当該holdを超えない。

**根拠:** [S32](source_evidence.md#s32), [S25](source_evidence.md#s25)。


### RFC19 — モデル追加editionの受入未実装


#### 19-A [P1] R-7 model-add / R-8 local-llmを実装する

**分類:** 受入driver未実装。**対象:** 現行要件。関連RFC: 13, 17。

**満たすべき要件:** RFC19 §11/§12.1: annual新規model ID/root追加のみ、旧モデル保持、実推論をH前後で確認。

**不足位置:** `docs/rfc/0019-annual-model-upgrade.md:320–327,406; scripts/drills/rfc0019_rolling_upgrade_drill.py:391`

**何が成立しないか:** driver choicesはrolling/switch/stale/boundary/restart/livenessのみ。catalog editionのswitchが通っても新旧実モデルの推論成功を証明しない。R7/R8は実際に新モデルを追加するeditionから必須で、edition0同梱fenceの初回試験と区別する。

**必要な変更:** 固定旧/新class IDとartifact root、実gateway/workerを起動する2つのdrillを追加。/healthと/v1/models、実prompt response/commitment、registry/tierを同じ証拠へ保存。

**終了条件・追加試験:** 旧ID/rootを上書きせず新IDだけ追加。H後のallocation rollから新0.3→1.0、旧1.0。前後で旧/新の非空推論を確認し他modelへのfallbackをFAIL。報酬を主張するなら実claim/Gate Cも確認。

**根拠:** [S33](source_evidence.md#s33)。


### RFC20 — 現行最適化実装はある／実規模受入未完


#### 20-A [P1] 32Kのbit一致・メモリ・時間を実artifactで測る

**分類:** 実規模受入未完。**対象:** 現行要件。関連RFC: 02, 11, 13。

**満たすべき要件:** RFC20 §2/§3/§5.5/§7: consume-on-read、位置run、spill、streaming fidelityの意味同一性と8K/16K/32K計測。

**不足位置:** `misaka/palw/tir/lower/tests/position_blocks.rs:125; docs/rfc/0020-long-context-calibration-memory.md:128–157`

**何が成立しないか:** carry消費/spill/runの実装と7architecture toy参照テストはあるが、記録は実モデル8K/16K/32Kのpeak RAM/時間を未計測とする。32K drift-windowsはTIR用で、legacy #1/#8の不足は13-A。

**必要な変更:** ram-accountingと既存position_blocks、fidelity --exec --drift-windowsを実artifact pipelineの合否へ接続。run=1/32、RAM/spill、cold/warmでstats/logits/root同一を記録し、予算超過は公開前に拒否。

**終了条件・追加試験:** 8K/16K/32Kの最大live bytes、RSS、I/O、時間、全系列を保持したままのwindow品質を取得。メモリ低下の推計だけでPASSにしない。#10の実court資源測定免除を勝手に撤回しない。

**根拠:** [S34](source_evidence.md#s34), [S16](source_evidence.md#s16)。



## 5. 新規に特定したRFC16の反例（16-B）

例示値は本番測定値ではなく、関数を反証するための入力である。

| 資源 | verifierの1 span能力 | 1 claimの負荷 | 受理できるclaim数 |
|---|---:|---:|---:|
| Compute | 1,000 CCU | 10 CCU | 100 |
| DA | 50 bytes | 10 bytes | 5 |
| Court | 3,000 units | 1,000 units | 3 |

memoryがfitするなら、真の上限は `min(100, 5, 3) = 3` 件。

`palw_capacity_claims_per_span_v1` は3を返す。しかし `palw_capacity_supply_from_attestations_v1` は3を量として保存せず「0でないから可」とし、`compute_ccu_per_span = 1000` を供給へ戻す。1 classのwater-fillは1000全部を配る。10 claimの需要=100 CCUは `100 <= 1000` で通る一方、courtは3件しか処理できない。

これは **既存helperをproductionへつなぐだけでは直らない**。`rfc0016_capacity_regression.rs` は既存capacity.rsのtest moduleへ追加する提案テストで、今回Rustとしては未実行。Pythonによる対応算術の再計算は添付結果のとおり。不整合の結論は単一classで成立し、異なるclassのCCU換算の議論に依存しない。複数classの修正はcompute/DA/courtすべての共有上限を保存する必要がある。

## 6. G14の検出policyの終了条件（14-A）

総区間N、不正を検出可能な区間D、重複なし検査kとして、不正集合が抽選前に固定され、一様抽選かつ不正区間を検査すれば必ず不一致が出るなら、

`P_miss = C(N-D,k) / C(N,k)`。

N=1000、k=4の独立再計算は次のとおり。

| D | 検出率 |
|---:|---:|
| 1 | 0.40% |
| 10 | 約3.95% |
| 100 | 約34.43% |
| 500 | 約93.79% |
| 1000 | 100% |

「10〜100回に1回の見逃しを許容する」は、どの不正集合/fault modelに対してかを定義して初めて検査kへ落とせる。同じ入力の再検査は独立試行ではない。単一不正区間を90%検出したいなら、この方式ではk/N>=0.9が必要になる。全演算をせずに全区間へ誤りをばらまく不正と、1つだけ不正な状態遷移を作る不正は分ける。

また、**検出率と処罰完遂率は別**。監視対象に入る確率、区間検出、cold fetch成功、告発収録、court期限内終了、担保残存/実徴収を個別に計測する。これらは独立とは限らず、根拠なしに単純積を信頼性として表示しない。低い検出率を、court容量不足や発見後の証拠取得失敗の言い換えに使わない。

## 7. 過去の評価から取り下げる欠落

**Rule E header rootの未接続:** 現在 `processor/palw.rs:8049` はV2/V3を選ぶ共通root readerを呼んでいる（S15）。残るのはremote運用consumer側（09-A）である。

**kernel interim signerが常にslashされない:** 現在のG14 lane closureは署名者lockの予約・conviction時徴収へつながっている（S24）。以前のkernel-only testのassertをnative fold後の挙動へ一般化しない。

**G13 lockが必ずFinal+1000で失効:** 新しいclaimのlock lifetimeを責任期限へ延ばす修正が最新記録にある。残るのはActive/Resolvedの効率（14-E）であり、同じ欠落ではない。

**Gate Cで認証された瞬間に常に保留報酬が支払われる:** 現在は対象claimについてhorizonまで保留する（S25）。それでも通常FP receiptのcoinbase経路は別で、18-Aは残る。

**RFC01 constrained decode court/renderなし:** 現在のproof arm・refuter・rendererがある（S36）。古いRFC追記はそのまま残件にしない。

**TIR FPそのものが未実装／古いframe算術不具合が全て未修正:** Phase Hと複数修正は入っている。残件はruntime facts注入、価格/時計、本番harness等へ限定する（S03/S22）。

**RFC20 attentionタイル/chunk observer/KV diskがgenesisの必須未実装:** 現行仕様はcausal textで全N×N行列を作らず、§5.2–5.4を今は実装しないと明示。KV diskも必要時まで延期（S34）。実装を増やすこと自体を完成の条件にしない。

## 8. 最終受入マトリクス（追加・統合する試験）

これは今回実行した試験一覧ではなく、上記不足を閉じるための終了条件。

| ID | 条件 | 必須観測 |
|---|---|---|
| AC-01 | 正常claim、全関連fence、実登録・実profile、seamなし | Finalだけでなく実支払い、必要lock、期限後解放 |
| AC-02 | 不正claim、全selected panel共謀、外部正直1bond | 公開証拠、客観verdict、実collateral減額、対象権利/credit失効 |
| AC-03 | dense/sparse/1区間不正、固定済みcommit | 検出率、未検査率、false positive、必要bytes/CPU |
| AC-04 | court上限飽和＋継続再充填 | 受理追及の期限内終了、正常Final進行、資源上限 |
| AC-05 | TIR全8owner active、native owner満杯 | 新規の適格追及の取得/等価証拠経路、無期限占有防止 |
| AC-06 | Gate C pending/rejected/no populationのFP Final | 有効ticketでもreceipt使用・coinbase取得不可 |
| AC-07 | native Master Final後に不正発覚 | post-Final告発、実徴収、receipt/credit失効 |
| AC-08 | verifier能力制約とattestation expiry | compute/DA/court各制約を全class合算で守る |
| AC-09 | PALWART3 container/inner/rootを各々改ざん | registration・node・court・SDKが同じidentityで拒否 |
| AC-10 | Rule E fork競合、V3 leaf | full nodeとremoteの選択一致、欠落証拠ではSTOP |
| AC-11 | provider/checker/source正直feeと不正fee | 一度だけ正常支払い、責任期限保持、不正時所定没収 |
| AC-12 | snapshot/restart/IBD/reorg、WAL各境界 | 残高・予約・Final・pending court・payout root一致 |
| AC-13 | 年次追加edition、旧新実モデル | 追加のみ、tier適用、H前後実推論、fallbackなし |
| AC-14 | 8K/16K/32K、run=1/32、RAM/spill | stats/logits/root同一、品質窓、peak RAM/I/O/時間 |

正常claimの失敗を告発側だけの試験で隠さず、AC-01を他の不正試験と同じbinary/parameterで毎回実行する。ユーザーが報告した「G 正直claimのFinal/支払い1件失敗」の実ログはこのZIPには提示されていないため、その失敗原因を08-B等と断定していない。08-Bは独立にコードから特定した支払い未完分岐である。

## 9. 総合判定

**全18 RFCの完全完成とは判定しない。** ただし「未認定」だけで終わらせず、上記の具体的位置・修正・終了条件で作業を分解した。RFC01/10/15/17のように今回独自の新規コード欠落を特定していないものへ、架空の欠陥を追加してはいない。それらも共有経路・実行受入が未完なら、その依存を明記してrelease判定から分ける。

最重要なのは、**不正を発見してslashするコードがあること**と、**claim受理→検査→証拠取得→客観裁定→実徴収→関連生成/報酬/weightの拒否が、各有効経路で端から端まで成立すること**を区別する点である。現状は後者に具体的な断絶が残る。

### 2026-10-19 — Codex RFC08/09/10/12 継続状況

- `origin/integ/validation-r2` は `8478ec5dcb6e9d3ad6ba4ef0fd046ca28e898e21` に進んでおり、確認時点で作業 branch `codex/rfc08-09-10-12` の HEAD `1b90a67c89f4e8e2417d851932dfffb3224bfee5` の祖先だったため、merge は Already up to date だった。
- RFC09 は既記録の実 gRPC/wRPC socket fork 比較 **3/3**、remote miner の欠落・改ざん opening 時に署名しない試験 **1/1** が根拠。今回の追加試験ではない。64 bond 超の end-to-end と正の certified-credit fork は未検証。
- RFC08: `native_text_service_uses_the_original_reservation_only_through_an_authenticated_horizon` **1/1**、`native_liability_hold` **12/12**。source-H helper は署名済み deadline と元の prepaid 量を変えず、従来 API の deadline 制約も維持する。これは限定試験で、text29/funded11/mining39 の post-Final H admission、court/subject retention、保留・返金経路は未完。08-B は未完了。08-C と RFC10 も未検証。
- RFC12: private-only paid-v4 profile の service span を承認済みの 1,321 DAA とし、dispute/material tail は 3,000/2,000 DAA のまま。公開 preset/identity は変更しない。node/CLI/probe helper の各 build は成功。誤った helper を指定した試行は node を起動していない。次の試行は node 起動中に停止してしまい、`/tmp/misaka-rfc12-approved-bootstrap-v2-result.json` に FAIL として保持。120秒は factory の上限で node 全体の上限ではないと訂正し、新 salt/new appdir で1時間の workflow 上限を使う実行を開始した。A/B は起動済みだが registration/binding、session、cold reopen、full acceptance の結果はまだない。
- 既記録の RFC12 EVM tests **60 passed / 0 failed / 1 ignored**、Python controls **133/133** は維持。署名付き返金 CLI の compile/startup 試験は実 broadcast を証明しない。受入スクリプト、reserve-flow 再構成、crash/refund/reorg の acceptance は未完了。
- ユーザー指定の検証形式: 同一の候補 binary・private testnet で複数 node/claim を動かし、正常系と攻撃系を別 scenario として一度の統合 campaign にまとめる。現行 `scripts/drills/misaka-rfc12-acceptance.py` は network を維持する一方、honest paid-work claim は 1 件で、通常経路から不正 root/prefix を送る攻撃 scenario がない。統合 campaign plan/実行結果はまだない。
- 修正した RFC12 private bootstrap v3 は fresh salt `d6de5084b5b35dacaa080adca7aaabe2db9c2e526b05d88ec77db3fd94583a2f`、paid-v4、fresh appdir で継続中。A/B/JOIN は起動・接続したが `/tmp/misaka-rfc12-approved-bootstrap-v3-result.json` は未生成。registration/binding・session・cold reopen・統合 acceptance の PASS 根拠には数えない。
