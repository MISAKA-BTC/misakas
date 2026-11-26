# 全claim受付難易度 — 実装接続計画（2026-11-17）

- 規範案: [RFC-0056](../../../rfc/0014-exec-schedule-and-capacity.md)
- 決定案: [旧ADR-0224](../../../rfc/0014-exec-schedule-and-capacity.md)
- 容量・継続保護: [RFC-0055](../../../rfc/0014-exec-schedule-and-capacity.md)
- source基準: `MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14`
- 実入口の根拠: [SOURCE_AUDIT.md](https://github.com/MISAKA-BTC/misakas/blob/f839ef0dc0307b58ed6cc280e631b1e1d34f733c/docs/maintenance/revisions/2026-11-17-admission-difficulty/SOURCE_AUDIT.md)
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED。以下は接続する作業と完了条件であり、実装済み・測定済みの報告ではない。**

## 1. 完成させる経路

新profileの公開claimについて、route adapterの上位にある共通入口から、次を同じidentityで追跡する。

1. boundedなcompact入力とnative資格を確認し、実資本の一回の実行機会をTrialSlotへ割り当てる。
2. Presealの入口実費・原資上界・必要資本・control/保持を確保し、targetと初期結果commitを固定する。
3. freeze後の一意なseedと事前公表Dでticketを判定し、eligible資料をguard内にcanonical化する。
4. 保存cursorの次prefixだけを処理し、eligible候補に限って予算内のgeometry/actor graph/plannerを実行する。
5. actual prefixの資格・資金・全工程予約を確認し、既存original coordinatorへ原子的に引き継ぐ。
6. 結果分類、canonical費用、stock peak、現在のfuture bookから観測を閉じ、D_(b+2)を更新する。
7. 既受理claimは新Dと独立してPanel/court/Final/精算へ進み、既存権利の条件を満たしてから消費する。

Minerは結果commitを作るための整数計算を先に行い得る。CAD1はNodeが引き受ける新規検証責任を調整する入口であり、Minerが実行前に必ず当落を知る仕組みではない。D合格だけで結果の正しさ、Final、権利、外側carrier permitを作らない。

## 2. 実在する入口と差込み方針

下表のsource pathは固定mainの調査起点である。新CAD1 APIが現在そこへ存在するという意味ではない。

| route | 現行の主な接続先 | 新profileで必要な差込み | 完了gate |
| --- | --- | --- | --- |
| Attempt / PAV3 | `consensus/src/pipeline/virtual_processor/processor/palw.rs`の`palw_v2_check_attempt_admission_waiving_v1`、`palw_admission_v2.rs`、`pow_layer0.rs` | 新formのcompact参照でtarget/Preseal/slotを有界に照合し、高価なmaterial・geometry処理より前にCAD1を判定。既存署名・pins・class/W資格は残す。 | 閉じた新form・不合格Dは新責任を作らず、既存履歴・W意味・BASE救済を変更しない。 |
| 通常 / non-TIR FreePrompt | `palw_fp_objects_v3.rs`のextractor/projector、`palw_state_v2/objects.rs`の`FreePromptCommitted` | 新compact carrierとeligible後payloadを区別し、全body decode・仕事量導出・original準備を済ませてからDを見る順序を避ける。 | plain FPも同gateを通り、別subnetwork/wrapper/再署名で新規入口を迂回しない。 |
| TFP1 Plain | `palw_tir_public_fold_v1.rs::prepare_paired_original_v1`、`OriginalAdmissionV1::Plain` | PlainをFundedの免除経路にしない。CAD1後に同一literal originalを既存のframe準備へ渡す。 | Funded分岐だけにgateを置く誤接続を検出する。 |
| TFO1 / TFO2 Funded | `palw_tir_funded_original_envelope_v1.rs` / `_v2.rs`、`palw_funded_original_coordinator_fold_v1.rs` | bounded transport/authからCAD1へ進み、通過後にactor catalog、全branch、資金scope、service bookを準備。実install時にもactual bookを確認。 | 元claim・payer・source・scope・一回性を保持し、失敗時にpartial claim/escrow/bookを残さない。 |
| evaluation / 無報酬claim | `objects.rs`のeval早期分岐、`palw_improve_eval_fold_v1` | 公開で新責任を作る場合は早期returnより上で同gateを適用。既存の機能無効化は維持。 | 「無報酬だから免除」「eval handlerだから未接続」を許さない。 |
| 複数original / atomic batch | batchの各original adapter、funded coordinatorのBatch install | 各originalが固有の有資金TrialSlot/Preseal/判定を持つ。外側batch一つのticketで全件を通さない。 | 全内側originalの検査、rank/prefix、資金・予約の原子性、部分失敗の処分が一致。 |
| quote / preview | `rpc/service/src/service.rs::get_palw_tir_original_quote_call`、`palw_accept.rs::palw_tir_public_minimum_rent_v1` | 無料は有界な型/範囲確認まで。詳細plannerはeligibleな資金付きPresealと署名済み費用scopeへ束縛。basis付きcacheも有限にする。 | 不合格payload・basis変更・並列quoteで無制限の高価な処理を呼べない。quoteを予約権としてinstallしない。 |
| 将来TFO3 / 未登録route | 今回mainに独立TFO3 carrierは未確認 | adapter接続までdefault deny。TFO2内部wire v3をTFO3と誤認しない。 | 将来追加時に同gateへ接続する。CAD1のためにTFO3自体を今回新設することは必須にしない。 |
| 既受理notice / court / response / terminal | 原claimの保護control・court・資金・service経路 | 元予約内で継続し、新D・新seed・新ExecCreditを要求しない。 | D最大・seed停止中にも救済を通す。未知originalや責任拡張の偽装は拒否。 |

## 3. P0 — 旧意味と新policyを分離する

CAD1の型・domain・codec・policy hash・TargetState・TrialUse・observationを未有効profileで追加する。既存native IDの幅と意味を維持し、RFC-0056 §5のtyped native referenceと唯一のfield順を実装する。wire/tag/署名番号はregistryへ正式に新設し、旧番号や旧signature subjectへ黙って割り当てない。

既存W/class target、FP receipt、P_m/CCU/tier、Final、WorkId、元claimの期限と資金をCAD1で再定義しない。RFC-0055の旧UAI1 DとCAD1を二つの抽選として掛け合わせず、新profileではCAD1がその一つのDを所有する。新規機能の無効状態と、旧履歴のreaderは両立させる。

**gate:** 旧snapshot・署名bytes・block/claim identityの再演が不変。不完全profile、未知codec、未接続adapter、旧carrier downgradeから新責任を作れない。新profileの存在や文書mergeをactivationと扱わない。

## 4. P1 — compact入口と全routeの判定順を接続する

固定header、総bytes/count/depth/展開上界をchecked arithmeticで確認し、global ingressのparse/auth/memory leaseを取得する。compact参照だけでnetwork/policy/target/producer/TrialSlot/Presealを照合できる形にする。高価なtrace、model geometry、actor graph、全工程plannerへ到達する前に、事前公表Dと唯一のseedでticketを判定する。

その前段も無償・無制限にはしない。署名・hash・lookup・エラー・log・decode失敗・再試行・quote cache・snapshot参照の上界を共通profileで定める。単に既存大payloadのdecode後へ`if eligible`を追加して終えない。未知modelには既存の共通kernel/schemaを使い、個別benchmarkやmodel名allowlistを導入しない。

新規責任か既受理continuationかはnative状態遷移で判定する。kind文字列や報酬有無だけで免除しない。

**gate:** 表の全新規routeで、不合格D・未知target・未資金Presealから高価な処理へ進まないことを実呼出順で確認する。不正署名、大length、巨大count、wrapperの重層化、全経路のerror/undoを含め、allocation前の拒否とprocess全体の上限を確認する。

## 5. P2 — TrialSlot、資金、Presealを原子的に受理する

native資本台帳が既存の資格・実行機会・費用・lockとともにsequenceを割り当てる。クライアントの任意sequenceはnonceとして採用しない。network・元資本outpoint・元機会sequenceの使用状態をpolicy世代をまたいで保存し、資本rolloverやoutpoint名変更で使った機会を再発行しない。

最初のPresealでtarget、model/program/weights、入力・生成条件、output/trace、shape、既存WorkId、資金・同意・期限を束縛する。署名bytes変更、別route/model alias、結果差替えで同じslotから複数ticketを得られないようにする。別の正当な仕事は別の有資金trialとして扱えるが、同じ資本・原資を二重lockしない。

入口実費、claim原資の安全上界、必要資本、Presealの記録・凍結・hash/索引・全候補の最悪訪問費・返還・保持を確保する。seed前にeligible数を期待値で見積もってcontrol予約を削らない。高水位/nullifierと索引・pruningの費用もstockへ入れる。

**gate:** 各失敗点で部分slot/資金/Presealを残さない。全候補が合格しても、割当済みcontrolとRFC-0055のhard budgetを超えない。再送はcanonical debitを増やさず、local再受信費も有限。費用保存・一回返還・高水位がcancel/GC/rolloverで崩れない。

## 6. P3 — freeze、seed、ready cutと有限cursorを接続する

target eはe−2で収集、Start(e−1)でfreeze、freeze後の指定beaconからseedを得て、Start(e)でready rootを閉じる。D_eはe−2の収集開始処理より先に公表・固定する。seed anchorの選び直しや、候補のtarget移動・無料carryを許さない。

共通clock/control profileから、最大予約量のeligible payloadを検査・伝播・包含できる正のG_ready_msを与える。seed cutを満たさなければtargetをunavailableとし、遅いseedを個別候補の資料不供給責任へ変えない。canonical資料rootに束縛し、hostの受信時刻で合否を分けない。

rankで一巡する索引と保存cursorを持ち、予約済みserviceに次prefixのAdmissionAdvanceを接続する。候補を好きな順で抜き出せない。全候補訪問・close・精算を予算化し、clock飛越し・carrier欠落・未処理尾部を完全な空集合にしない。

bootstrapはepoch 0/1の新規admissionを空とし、target 2収集前にD2とlow_runを初期化する。Observation[0]/[1]はbootstrap-unavailableかつincompleteであり、未開始をidle decayへ使わない。

**gate:** freeze境界、seed cut直前/直後、ready不足、seed不成立、空集合、eligibleゼロ、未処理尾部、重複/欠落Advanceで一意な結果。既受理court/controlは継続する。同一結果を要求するbatch分割試験は、clock/service cutと外部state変化の位置を保持する。

## 7. P4 — eligible候補と既存atomic originalを接続する

eligibleかつ資料commitが一致する候補だけ、予算内のintrinsic geometry・actor catalog・plannerへ進める。共通規格、実資格・実資金、元期限、predecessor、資料供給、全合法継続を検査する。内在的に不成立、資金不足、unsupported、MissingMaterial、planner不具合を単なるNoCapacityへ変換しない。

NoCapacityは、固定system予約を維持する空claim-bookに、同じgeometry/readiness/clock/資金ならfitすることを確認した上で、既存予約との競合として記録する。この比較自体の費用も予約する。

実際の受理は現在canonical prefixのbookで行う。mainの`prepare_paired_original_v1`、`admit_original_v1`、`reserve_for_claim`、`write_claim`、`install_original_admission_v1`等へ同一literal claimと資金を渡し、original/TrialUse/資金/book/actor/disposition/cursorを同checkpointでcommitする。既存actor/資金/service条件をD合格で代替しない。previewで作った古いbookをinstallしない。

**gate:** NoCapacity全拒否とInvalidQualification全拒否が異なる観測になる。大候補不成立後の小候補受付、siblings、同一slot再送、stale quote、途中faultで一回性・資金保存・原子性を確認する。失敗・rollbackの実処理費もboundedであり、無料の無限再試行を作らない。

## 8. P5 — canonical観測から一つのDを更新する

capacityから固定system・最低通常取引・ingress/control・復旧を除いたclaim責任の供給を分母にする。外側carrier一本の固定費とco-packできる内側payload容量を同じ件数として二重に控除しない。

S_fixedへ入れるのは基礎control等の先行枠であり、claim固有のnotice/court/終端/保持はR_claimに計上する。preseal/controlとclaim責任の区分をpolicyに固定し、同じ費用を両方で差し引かない。

各canonical操作のcost debitとlive-state上界を記録し、現在の全未来予約U_future、直前epochの消費flow U_spent、同epochのstock peak U_stockを算出する。Uはその最大であり、同じ仕事の三重加算ではない。cancelで解放した未来予約を消しても、実費・stock peak・既に確定したNoCapacityは消さない。

qualified_demand、known_congestion、observation_complete、unavailableをnativeなdispositionとcanonical rootから導く。RPC成功数、host CPU/RAM、自己申告値、未認証packet数を合意入力にしない。観測穴はlowを禁止し、確定済みのhigh根拠があれば難化できる。seed/profileのunavailableはhighより優先する。

global unavailableはactive policyと認証済み共通stateから導く。提出者がunknown/disabled profileや供給ゼロshapeを指定しただけなら当該候補を拒否し、全体停止へ昇格させない。

Start(b)でb−1をcloseした直後、bの処理より前に、previous D_(b+1)からD_(b+2)をchecked整数で計算する。b/b+1は変更しない。D=1の丸め、D_max、ゼロ供給、欠落/重複/逆順boundary、policy generationを明示的に処理する。

**gate:** RFC-0056 D01〜D20を、外部から与えたtoy booleanだけでなく実native状態から再現する。完了・解放済み高負荷が偽lowにならず、未処理尾部がidle decayを起こさず、固定BASE/controlだけではclaim highを作らない。集計・索引更新自体にも有限な費用がある。

## 9. P6 — 復旧・実機負荷・既受理救済・activation

policy/TargetState、Preseal/TrialUse、資本高水位、集合/seed/ready root、cursor/disposition、cost/stock/coverage、D schedule/low counter、資金とbookをsnapshot/journalで認証する。bookや観測が復元できないNodeは、新規受付を停止・保留して同期する。空の観測や空の予約を捏造しない。

通常cancel/再送/pruningによる一回性の復活は禁じる。正規reorgは同checkpointをundoし、新canonical履歴で再判定する。旧世代workerの結果やleaseを新世代へ混ぜず、実worker/buffer/I/O終了前にlocal容量を返さない。cross-forkのseed biasや無料再抽選をCAD1だけで絶対排除したとは主張しない。

最低Nodeでは、未認証、不合格、適格・有資金候補、quote、通常取引、全carrier、既存court、DB/index/journal、cold I/O、restart/reorg/IBD/GCを混ぜて測定する。登録1万/5万と最大pending/retention/nullifierを含め、全model走査や全state cloneに依存しないこと、残る費用が採択profile内であることを確認する。個別modelごとのbenchmarkを要求せず、共通profileと未見modelの自動適合を確認する。

実LLMの同一claimについてStudio等の送信からCAD1、元資金・book、Panel全体検証、正常Final、不正反証、精算・既存権利消費まで追う。高D・seed停止・新規受付停止中にも最後の適格正直者の救済を維持する。全拒否・全Abortだけを実用完了の代替にしない。

**gate:** 上界・実meter・資金保存・原期限が一致し、未完了分母、backlog、stock、空slot、待ち時間、制御遅れを含む報告を作る。W/receipt/CAD1/carrierのloopを同時に評価する。設計採択、文書検査、抽象試験、native試験、実機、移行、公開activationを別々の証拠にする。

移行では既受理義務を元期限で守り、旧targetをdrainする。旧義務を収容できない、共通profile未完成、入口が一つでも迂回可能な状態では新profileを有効化しない。指定anchor・D_initial・開始target・counter resetを別のactivation recordへ明記し、通常のmergeや私的profileの成功を公開有効化と扱わない。

## 10. 完了報告の最低内容

- source/binary、policy/checker/capacity/料金/clock/beacon、hardwareと測定記録のhash。
- routeごとのcompact入力からoriginal/資金/bookへの追跡と、未接続routeのdefault-deny確認。
- TrialSlot/Presealの一回性、全terminal理由、実費・返還・残lock、epochの全分母・未処理数。
- D入力の根拠root、消費flow、stock peak、future book、coverage、targetごとの公表値と再演一致。
- 実負荷の最大値・失敗・未完了・空slot・backlog・期限超過。成功例やchecker単体の速度だけを報告しない。
- 既受理救済、正常Final・権利消費、反証と精算、再起動/reorg/IBD、旧義務移行のそれぞれの結果。

この計画には本番コード変更も実機達成値も含まれない。新Dの完成は上記gateの証拠で判定する。
