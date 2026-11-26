# UnifiedAdmissionV1 実装接続計画 — 2026-11-17

規範は[RFC-0055](../../../rfc/0014-exec-schedule-and-capacity.md)、決定は[旧ADR-0223](../../../rfc/0014-exec-schedule-and-capacity.md)。source基準はpre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398。本計画は新しい型・共通費用規則・実行予算の接続順と完了証拠を定める。設計状態はADOPTED_DESIGN_NOT_ACTIVATEDであり、以下の作業を実装済み・測定済みとは宣言しない。

最新指示に従い、claim/court件数、通常処理の固定30秒枠、one-winner、一claim一生成権、live permit四つ、十epoch更新は規範にしない。120秒は計画・集計・difficulty epochである。件数は共通bounded checkerの費用上界、共通容量、既存予約、各claimのshapeから導く。

## 1. 一つの完成経路

Studioまたは別の対応routeで得た同一結果・同一canonical work identityについて、次の一連の処理を接続する。

1. 初回traceと分解位置を固定し、共通checkerで扱えるclaim descriptorを導く。
2. 実費と保持を予算化したpresealを登録し、集合固定後の未知seedと事前固定Dで候補資格・順位を決める。
3. 保存cursorから有限の候補prefixを進め、実資金、既存route資格、全合法継続のflow/stock・期限・carrier予約が成立する分だけ原子的に受理する。
4. Node内部のpreflight/lease/meterを通し、通常検査、独立Panel、適格noticeと反証、防御、終端を元予約・原期限で処理する。
5. Final後に既存P_m、CCU、モデル生成権、有限ExecCredit/receiptの規則を適用し、共通carrier内で起源を再確認して消費する。
6. 精算、保持、archive/GC、journal責任を別々の有限条件で終える。

別claimのfixture、送信RPC成功、保存helper単体、恒久拒否・全Abortで、この経路の正常成功を代替しない。一originalと権利数・block数を固定対応させず、外側のtransport permit消費と内側の経済権利消費を別々に追跡する。

## 2. 接続対象と完了証拠

以下の既存接続先は調査・改修の起点であり、新版が現在そこへ接続済みという意味ではない。新規型名・domain・wire IDは登録前の既存APIとして公開しない。

| 段階 | 既存の主な接続先 | 新規作業 | 完了証拠 |
| --- | --- | --- | --- |
| policy / clock | consensus/core/src/config/params.rs、palw_mode_v2.rs | 版/hash、120秒epoch、RateClockとDAAのClockBridge、DISABLED gate | 旧履歴一致、不完全profile拒否、時計・strict deadline境界 |
| 共通形式 / descriptor | native program・trace・court codec / compiler経路 | kernel分解、初回trace固定、WellFormed / Init / Step / End、shape由来費用上界 | 最大合法shape、数値/接続/終端境界、未見modelの自動適合 |
| Node preflight / lease | codec、RPC/P2P入口、palw_accept.rs、worker / storage経路 | allocation前の全長/count/展開検査、process予算、所有権付きlease、失敗/undo費 | 巨大入力のallocation前拒否、同時decode制限、cancel/reorg後の実終了までlease保持 |
| 受付の同一性 | palw_state_v2/objects.rs、palw_funded_original_coordinator_fold_v1.rs | 全新規route共通のpreseal/target/cursor、仕事一回性、downgrade拒否 | 同一仕事の再署名・carrier交換・別routeで追加抽選なし |
| 難易度 | palw_work_target_v1.rs、palw_admission_v2.rs、palw_fp_admission_v3.rs | 旧意味を保存して共通Dを追加、資源利用率・需要・混雑・停止から整数更新 | b+2遅延適用、b/b+1不変、seed前固定、overflow・再演一致 |
| 実費 / 全工程予約 | palw_positive_contract_v2のtfp1_original経路、palw_protected_original_coordinator_v2.rs | 共通GlobalServiceBookV1、全actor/防御/回復の有限graph、全期限区間flow/stock、実資金 | 既存book不変の拒否、最後のnotice・最大合法継続、資金保存 |
| 候補順位 / planner | preseal索引、native admission coordinator | closed root/seed/basis、有限rank順走査、保存cursor、期限順の保守的fit | 小/大候補で受理数可変、大候補不成立後の小候補受付、planner自体の予算 |
| 原子的保存 | palw_funded_original_coordinator_fold_v1.rs、palw_state_v2/objects.rs | original/予約/実資金/actor/一回性/cursorの同一checkpoint | 各段階fault injection、部分保存なし、journal undo/replay一致 |
| court実行 | palw_accept.rs、court coordinator | actor/role/task/generation、predecessor、実行区間、全階層meter、once marker | 正当task成功、他人・別世代・重複の拒否、誤った適格証拠も実費計上 |
| Panel / Final | RFC17〜20のconsumer | 非同期全体検証、全期間notice、Q、反証優先、元期限 | 実LLM正常Final、不正反証、最後に到着する唯一正直者 |
| carrier | palw_execution_lane_v1.rs、native fold / processor | 全kind共通permit、有限L・schedule/fallback、serviceと経済資格の分離 | 全kind合算1 BPS、siblings一回消費、失効、creditゼロ・seed停止時の既存救済 |
| 経済権利 | claim / credit / receipt consumer | 既存P_m/CCU/権利規則を接続、起源・期限・取消を発行/予約/消費時に検査 | Dだけのmintなし、二重mint/spendなし、同batch反証優先、正規の生成権消費 |
| 保存 / 復旧 | snapshot / journal / IBD / expiry / GC索引 | 全新state認証、世代とlease、有限GC、差分/永続索引、移行book | 一万/五万model・最大pending/retention、cold/reorg/restart/IBD、book欠落時停止 |
| RPC / Studio | rpc/service/src/service.rs、quote / submit / worker経路 | target epoch、preseal/seed/rank/cursor、実vector・予約拒否、元identity保持 | 公開canonical readbackから同claimのFinal・消費・精算まで |

## 3. 実装順とgate

### P0. 旧意味と新しい型を分離する

新版policy、CommonCheckerProfileV1、NodeCapacityProfile、ClaimResourceDescriptor、GlobalServiceBookV1、UnifiedAdmissionV1、UnifiedCarrierV1を未有効の版として追加する。型/tag/domainを旧wireへ黙って割り当てず、codec registryとfingerprintへ登録する。旧署名bytes、旧W、Final後FP receipt、旧claim期限の再演を保存する。

gateは、不完全profile・未知版・未接続routeのdefault denyと旧履歴の不変である。文書採択をactivation fenceの設定根拠にしない。

### P1. 共通checkerと実行前上界を作る

共通の整数意味、kernel/複合演算の分解、有限の項数・source・認証・workspace上限を実装する。初回traceに全ての裁定対象と状態接続・終端を束縛し、異議後の再記録を拒否する。共通規格で表せる未知modelは認証済み構成・job geometryから自動判定し、個別benchmarkやmodel allowlistを入口へ足さない。

実行前の費用関数はparse/展開、署名/hash、算術、lookup、read/write、record、保持、失敗/undoを含む。空packetやwrapperにも非ゼロ固定費を置く。共通規格外は高価な処理に入る前にUnsupportedShape/UnsupportedKernel等として終了する。

gateは、最大合法入力と悪意入力で上界が導出でき、実行meterがその上界内に収まり、全体の正しいtrace関係に穴がないことである。p99や通常例の速さだけを最悪合法入力の上界としない。

### P2. OOMを防ぐpreflightとleaseを先に接続する

固定headerを有界に読み、checked length/count/depthと展開上界を検査する。process全体のparse/認証/decode予算を確保してから有界にoriginal/actor/予約へ対応付け、全階層予算を確認し、lease内で本体を展開・実行・保存する。

buffer、decode object、worker、I/O、clone、journal workspaceとleaseの所有権を一緒に移す。エラー、cancel、reorg、timeoutで二重返却せず、実workerとデータが終了した時点で一回だけ返却する。同期要求、未完成parts、candidate、再試行、ログ、planner、undoにも有限予算を置く。

gateは、巨大入力をallocation前に拒否できること、同時入力でもprocess上限を守ること、古い世代の処理が残っている間に同じ容量を再貸出ししないことである。local資源不足はbackpressure・保留・復旧へ進め、永久block-invalidやMinerの算術有罪としない。

### P3. 全期限区間・flow・stock・実資金を予約する

共通capacityからsystem/control/EXEC/復旧の先行予約と既受理責任を差し引く。claim descriptorから、全actorの合法な継続、防御、有限第三者、最後のnotice、最遅応答、InspectionPending、終端・精算、保持・GC、carrier/controlを有限task graphへ展開する。

各taskのrelease、finish deadline、predecessor、flow vector、stock期間をClockBridgeと共通profileから導く。既存予約を動かさずRFC-0055 §7.2の決定的順序で割り付ける。120秒総量のほか短い全期限区間とstockの全占有時点を検査する。nativeに確認できない枝の排他性を使って予約を減らさない。

gateは、一taskでも割付不能・planner予算超過・資金不足なら候補book全体を破棄し、既存book・escrowを変えないこと。全合法継続、Abortと精算、残保持を実原資で支えること。quoteのbookは権威にせず、actual prefixで同じnative計算を再実行する。

### P4. 全routeのD・preseal・有限候補処理を統一する

Attempt、FreePrompt、TFO3、atomic batchと公開quote/submitを同じ新規受付へ接続する。経路未接続時の旧入口fallbackを閉じる。既存bond・route資格・canonical-work・資本上限は追加の必要条件として維持する。

target eに対し、e−2中のpreseal、Start(e−1)の集合freeze、freeze後の未知seed、Start(e)での受付開始を実装する。epoch b開始時のD更新をtarget b+2へ適用し、b/b+1は変更しない。初期二target、seed不成立、eligibleゼロ、需要ゼロ、容量ゼロ、profile停止、overflowをそれぞれ試験する。

closed_preseal_root、seed、D、開始book/basisを固定し、eligible候補をrank順に一巡する。保存cursorからplanner/parse/lookup/書込み予算に入る有限prefixだけを処理する。candidateごとの実予約成功を次へ反映し、不成立理由を固定する。epoch終了時の未処理/未受理候補はExpiredとし、無料の別seed抽選へ持ち越さない。

gateは、受理数が実残資源に応じて変わり、固定one-winnerを使わず、候補集合・順位計算自体も有界であること。同一workの別route・再署名・nonce変更・carrier交換で抽選を増やせず、Dの遅れや最大値でもhard予約gateを越えられないこと。

### P5. court・Final・権利・carrierを同じfoldへ接続する

全時間帯の適格notice/Qを保護し、重いcourtは本人の予約taskと実行区間で行う。無効認証のpacketはingress費へ、適格主体の算術的に誤った証拠は本人の実行費へ計上する。再送・二重task・別actor・別generationで保護枠や支払を増やさない。

RFC-0055 §11の順序で、notice/court/terminal、Q/反証/取消、Final、経済権利の発行・予約・消費を処理する。local positiveで全体承認を代替しない。既存P_m、CCU、有限creditとモデル生成権を適用し、原受付から新しい均等権利や一claim一blockを作らない。待機・再予約で起源expiryを延長しない。

外側carrierは全kind共通1,000ms一本、profile由来の有限L・owner/fallback・包含条件とする。transport資格とservice予約を内側のModelBlockRight/ExecCreditから分離する。published scheduleを書き換えず、既存scheduleに整合する枠または未公開将来枠へ予約する。seed/schedule停止時も既受理救済の独立搬送を保つ。

gateは、同一canonical prefixでの資金・予約・permitの一回性、siblings merge時の再検査、同batchの反証優先、creditゼロ時の既存court、最遅readinessから検証・伝播・包含までの両期限である。持続1 BPSのためにcredit倍率を増やさない。

### P6. 保存・復旧・多数model・共通qualificationを通す

profile、clock、D/low counter、preseal集合/seed/cursor、一回性、carrier消費、資金、case、flow/stock、expiry/releaseをsnapshot/journalへ認証する。book欠落時に空bookで新規受付を再開しない。old generation workerの結果を新世代へ適用せず、leaseは実終了まで保持する。

毎blockの全model/全claim走査を必須にせず、認証永続索引、差分counter、期限索引、非空queueを使う。残存する全コピー・root再計算は最大state規模で予算化する。GC未完了diskを空き扱いせず、IBDのactive/staging DB、WAL、spill、復元workspaceと通常courtを合算する。

同じbinary/profileを最低機器で、checker、通常取引、全carrier、control、登録、DB/索引/root/journal、cold I/O、compaction、reorg/restart/IBDの同時混合負荷として測定する。一万/五万modelと最大pending/retentionを含める。未見modelを既存kernelから構成し、個別測定なしの自動受付と正常進行を確認する。

gateは、safe margin込みの同時供給点、上界とmeterの一致、保護期限、実LLM正常Final・不正反証・精算・既存権利消費である。共通profileのqualificationを、将来追加されるmodelの個別資格台帳へ変更しない。

### P7. 移行とactivationを別に記録する

旧義務を列挙し、旧署名・Final・既得権・原期限のまま新共通budgetへ収める移行bookを検証する。収容不能なら新profileを有効化せず旧契約を履行する。指定anchorで全新規routeを同時切替し、旧carrierへのdowngradeで新規責任を増やす経路を閉じる。

設計採択、文書検査、抽象モデル、native実装、実機性能、公開activationを別の証拠として記録する。実装・測定・移行gateが揃った後に別の明示activation recordへ進む。mergeや設定ファイルの存在を公開有効化としない。

## 4. 数値・profile・経済配分の境界

| 項目 | 扱い |
| --- | --- |
| 計画・集計・D epoch | 120,000ms。120 DAAへ読み替えない |
| 外側carrier供給 | 全kind共通で1,000msに一本。通常時の持続目標1 BPS |
| Dの更新時点 | epoch b開始時に計算しtarget b+2へ適用。RFC-0055 §6.5の整数更新を使用 |
| claim/court受理数 | 共通費用上界、全interval/flow/stock残量、期限と予約から導出。固定件数・one-winnerを置かない |
| 通常/negativeの時間帯 | taskのrelease/deadlineと予約で決める。固定30秒枠を置かない |
| carrier live期間L / fallback | 共通capacity/clock profileの較正済み有限欄。説明例の数を転記しない |
| kernel分割、decode、planner、horizon、queue | 共通形式・実行安全性の有限上限。件数無制限の意味にはしない |
| 機器性能・flow/stock・保持・第三者資格 | 完全profileの必須欄。欠落時に旧T12値や旧54claim/日を流用しない |
| 未知model | 同じ共通規格で表せればdescriptorを自動検査。個別benchmark・allowlist不要 |
| P_m / CCU / 有限権利 | 既存の適用版・配分・消費規則を維持。Dに二重掛けせず、端から端のP_m比例は保証しない |
| Runtime activation | 上記の構造決定や数値記述と別。未有効から共通qualification・移行・明示activationへ進む |

共通checker/schema/meter、storage、対象binary/機器/容量の変更は再qualificationする。同じ規格内のmodel ID・weights・graphの追加だけなら個別性能測定を要求しない。

## 5. 受入証拠と完了報告

RFC-0055 §16のV01〜V23をnative実装に結び付ける。特に次を同じ版で確認する。

- 小/大courtの混在で件数が変わり、CPUが余っていてもRAM・bytes・I/O・短いdeadline区間の超過は実行前に拒否する。
- 巨大length/展開/overflow、空packet、無効認証、parts/relayへの分割、planner入力、cancel/reorgで上界やleaseを迂回しない。
- 最後の適格notice、遅い唯一の正直者、他actorの最大合法caseを、原期限・本人予約・実資金内で処理する。
- 全新規route、seed後の集合変更、b+2更新境界、同一workの再抽選、stale quote、旧carrier downgradeを検査する。
- 原受付では権利をmintせず、同batch反証がFinal/予約/消費に先行し、全kind carrierの合計と一回性を守る。
- 一万/五万登録、最大pending/retention/GC、cold/reorg/restart/IBDで共通予算と復元を守り、未見modelに個別benchmarkを要求しない。
- 正常系・不正系それぞれで、Studio提出から起源claimの同一性を保って追跡する。正常系はPanel・Final・既存生成権消費・carrierのcanonical readbackまで、不正系は反証・成功停止・精算までを記録する。

測定報告にはbinary/profile/hardware/policy hash、全サンプル分母、失敗・未完了、空epoch/slot、stock/backlog、deadline miss、同時資源使用量を付ける。成功例だけを分母にせず、独立測定の最大値を同時に供給できる量として足し合わせない。

全拒否・全Abortしか確認できない段階は「安全に拒否できる段階」と記録する。正常Finalと既存権利の実消費、共通最低Node測定、旧義務の移行が未確認なら実用完成とはしない。公開稼働値やNode最低要件は、この計画の記述だけでは確定しない。
