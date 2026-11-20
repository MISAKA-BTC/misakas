# RFC-0056 独立レビュー記録

- 日付: 2026-11-17
- 対象: [RFC-0056](../../../rfc/0056-global-claim-admission-difficulty.md)
- source基準: `MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14`
- 本文snapshot SHA-256: `13f2b9afb1d5476c89768b6dc11c069008fa388fa93a7e97e56c9b4be219a2ca`
- 設計状態: `PROPOSED_DESIGN_NOT_ACTIVATED`
- 判定: **対象snapshotの文書レビュー完了。native実装・実機性能・公開activationは未検証。**

## 1. 範囲と結論

全claim共通Dの判定位置、routeの非迂回性、既受理救済の免除、TrialSlotと実資本、集合・seed・資料readiness、canonical観測、整数更新、費用・失効・reorgを独立に確認した。整数計算の完全一致はユーザー前提として維持し、モデル単位のbenchmark・人手審査や計算正当性の暗号学的証明へ変更していない。

現mainの既存guardと今回の新規作業は[ソース監査](SOURCE_AUDIT.md)で区別した。Attemptには既存ticketがあり、FP receiptはFinal後、TFO1/TFO2には原子的予約がある。CAD1の全route接続が実装済みとは扱わない。以前のpreにおけるTFO3進捗や、TFO2内部wire version 3を今回mainのTFO3実装証拠にしない。

下記の指摘が修正された本文について、レビュー範囲に追加の重大な文書矛盾は検出しなかった。DがNode安全性を単独で保証する設計ではなく、RFC-0055のhardなflow/stock・全工程予約・local ingress/leaseが急増時にも上限を担うことを確認した。共通profileが実装・較正できること、正常受付が十分な頻度で成立すること、無条件の公平性・活性は、この文書レビューでは証明していない。

## 2. 指摘と是正

| ID | 初稿の問題・反例 | 現行本文で確認した是正 |
| --- | --- | --- |
| R01 | 正規reorgでslot/D/seedをundoすると記載しながら、試験D17が無条件の「履歴/slotの復活なし」を要求すると、正当な履歴再演まで拒否する。 | §11/D17で、通常cancel・再送・pruningによる復活禁止と、正規reorgで同checkpointの消費をundoして新canonical履歴で再判定することを分離。forkをまたぐ別seed抽選をCAD1だけで絶対禁止する保証も削除。 |
| R02 | batch分割で処理時刻、途中のcourt解放、資金状態まで変われば、同じ候補でも予約結果が変わり得る。 | D13を、同じclock/service cut・canonical prefix・外部state変化の位置を保つ合法分割へ限定。異なる時刻や解放を挟む分割まで同結果とはしない。 |
| R03 | `canonical_opportunity_sequence`の割当主体が曖昧だと、送信者がnonce同様にsequenceを変更して一回性を迂回できる。 | §4.2でnative資本台帳が資格・実費・lockとともに割り当てると明記。別sequenceは別の有資金trialであり、policy世代やoutpoint rolloverで元の消費済み機会を再発行しない。 |

並行レビュー・本文改訂で加わった次の点も確認した。

- §5は新CAD1 digestだけを32 bytesとし、既存native IDの幅・意味を切り詰めない。typed native reference、LP上界、field順を定めた。
- §6はseed確定から資料検査・伝播・canonical包含までの正の`G_ready_ms`を要求する。Start(e)直前のseed到着を個別候補の資料不供給責任にしない。
- bootstrapではD2とlow counterを先に初期化し、epoch 0/1の未開始targetをcomplete idleとして緩和へ利用しない。
- §9は固定搬送・ingress/control費をclaim負荷と分け、未来予約だけでなく消費済みflowとlive-stateのstock peakを見る。
- 最終の§9追記では、S_fixedの基礎controlとR_claimのclaim固有notice/court/終端/保持を二重控除しないことを確認した。unknown/disabled profileや供給ゼロshapeの提出だけでglobal unavailableを立てず、候補拒否とactive policy・認証bookの異常を分離した。

## 3. 確認した設計上の条件

以下の「確認」は規範の存在と節間の整合性の確認であり、native試験の合格ではない。

| 論点 | 確認内容 | 根拠 |
| --- | --- | --- |
| 重い処理より前のD | compact入口のサイズ・版・bounded認証・identity・TrialSlotを先に調べ、D不合格なら高価なgeometry/actor graph/plannerやPanel義務を作らない。前段費用も固定budgetで上限を持つ。 | §3.2/7/8 |
| 全route | Attempt、plain/non-TIR FP、TFP1、TFO1/2、evaluation、batch、quoteを分けて追跡。将来TFO3と未接続adapterはdefault deny。 | §3.1/13 |
| 既受理救済 | 適格notice、反証、応答、終端、保持を新D・新seed・新ExecCreditへ通し直さない。未知originalや責任拡張のcontinuation偽装は拒否する。 | §3.1/11/13 |
| 一回性 | 経済的機会をnative台帳に束縛し、結果・署名・route/model aliasでslotを増やさない。正当な別の有資金trialと、同一slotの再抽選を区別する。 | §4.2/5 |
| 費用と資本 | Preseal時点で実費、claim原資の上界、必要資本、control・保持・精算を確保。名目bond、将来収入、推測した低いeligible数を資源保証に使わない。 | §7/11 |
| seedと資料 | Dは収集前固定、集合freeze後の一意なbeacon、正のready guard、canonical資料rootを使用。seedが遅いtargetを止め、好きな次seedへ切り替えない。 | §5/6/7 |
| candidate処理 | rank順の有限cursor、実prefixでの資金・予約検査、原子的disposition/cursor更新。大候補の不成立後にも小候補を検査できる。 | §8 |
| qualified demand | 形式だけの自己申告やRPC件数でなく、eligibleで資格・資金・shape・内在的期限を確認した候補から導く。これは結果全体の正しさの認証ではない。 | §8/9.3 |
| NoCapacity | 固定system予約を維持した空claim-bookに同条件でfitする候補だけ、既存予約との競合を混雑と分類する。unsupported・資料欠落・planner不具合を混雑へ変換しない。 | §8 |
| coverageと観測 | 集合・seed・ready root・terminal disposition・meter・stock peakをcanonical cutで確定。未処理尾部や欠落epochを空需要として低負荷化しない。 | §8.1/9.3 |
| flow/stock負荷 | claim責任へ供給できる分母を用い、現在の未来予約、直前epochの消費flow、live-state peakの最大値を取る。局所RSS/CPUや解放済み未来予約の過去peakをそのまま合意値にしない。 | §9 |
| 更新境界 | previous D_(b+1)からD_(b+2)を更新。公表済みb/b+1を変えず、二重・逆順boundaryを拒否。unavailable優先、incompleteはlow禁止、D=1の整数丸めも明記。 | §6/10 |
| 失効・復旧 | 終端理由と使用済み費用を残し、返還・保持削除も有限予約。一回性・観測・bookを同journalで復元。hostだけの障害は共通Dへ投票しない。 | §9.3/11/13 |
| 経済・搬送との分離 | D合格からFinal、生成権、ExecCredit、追加の搬送permitを作らない。既存W/receipt/P_m/CCUを維持し、UAI1とCAD1を二重に掛けない。 | §0/2/13 |

## 4. 抽象検証の範囲

今回のCAD1については、ticket、集合・slot、candidate disposition、canonical観測、更新boundaryを結ぶ独立の抽象モデルが必要である。前RFC-0055の30試験を、新D経路の合格証拠に流用しない。

今回の[model-results.json](model-results.json)は、2026-11-17T06:43:14.068759+00:00の実行で**46件中46件成功、failure/error/skipは0件**と記録する。レビュー担当は結果を読み、RFC hashが冒頭の`13f2b9af…`、[script](check_difficulty_model.py) hashが`e73c68c3716f737f28b91361af77092481bca242493fc15a31dfc7af6be10269`で現物と一致することを確認した。途中の40件結果は追記前の本文snapshotを参照していたため、最終結果に読み替えて確認した。

これは合成費用・容量・料金・時刻・native参照bytesを使う抽象契約モデルである。native codec、署名、資本provenance、共通cost導出はfixture入力であり、RFC-0055の完全なactor graph・ClockBridge・court plannerも外部前提である。1万/5万model IDの試験はidentifier不変性を検査したもので、実registryやmodel負荷のbenchmarkではない。

本レビュー担当は抽象モデル・nativeテストを実行していない。今回の実行結果、合成fixtureと仮定、未実証事項は[改訂記録](README.md)と結果JSONを併読する。hash一致は対象ファイルの対応確認であり、scriptがRFCの全条項を検証する保証ではない。文書に受入条件が書かれていることと、その条件を試験が通ったことを区別する。

抽象試験が成功しても、次は別の証拠が必要である。

- native codecとtyped ID/署名scopeの同値性、実資本台帳のsequence・rollover・policy移行。
- 不正入力・不合格ticket・無料quoteが高価なdecode/geometry/plannerへ進まない実際の呼出順。
- canonical費用・stock peak・NoCapacity分類・coverageを全routeで正確に記録するmeterとjournal。
- beaconの未知性・bias・fork前提、RateClock/close frontier、seed guard内の実資料包含。
- 実機の同時負荷、allocator/queue/worker/storage所有権、復旧、実LLMからFinal・権利消費までの接続。

## 5. 残る実装・計測ゲート

| ID | 完了に必要な証拠 | 現在の判定 |
| --- | --- | --- |
| G01 | 全入口のcompact codecとcheap処理の上界。新D前に全model/trace/geometryを読む経路がなく、エラー・quoteもboundedであること。 | 新規native接続は未確認。既存byte capだけで全面合格にしない。 |
| G02 | TrialSlot/実資本/実費/高水位の認証状態と原子的lock。別route、別key/model、sequence変更、資本rollover、policy更新で無料試行が増えないこと。 | native台帳と全routeのfault/reorg試験が必要。 |
| G03 | freeze、seed anchor、ready guard、資料root、reserved AdmissionAdvance、closeの一意な実行。欠落時にincomplete/unavailableを正しく記録すること。 | 実transport・clock・canonical包含は未確認。 |
| G04 | qualified demand/NoCapacityのnative導出、固定system予約を含む空book比較、flow debit・stock peak・coverage集計の一致。 | toy boolean入力を実観測の完成と扱わない。全rejectも理由ごとに試験する。 |
| G05 | checked整数controller、公表target、bootstrap、欠落/重複boundary、policy移行、snapshot/IBD/reorgの一致。 | 同じ入力のpure式だけで全状態接続を合格にしない。 |
| G06 | eligible候補から既存original coordinatorへ同一claim/資金/責任を渡し、全合法継続を実prefixで予約。Final・権利消費は既存条件を再確認すること。 | D合格のみのmint、partial claim/book/escrow、quoteの予約権化を拒否する実装が必要。 |
| G07 | D最大・seed停止・新規受付停止中の唯一の適格正直者のnotice/反証/応答/終端。 | RFC-0055の役割別供給・原期限・原資・carrier条件を実profileで確認する。 |
| G08 | 最低Nodeで不正/不合格/有資金候補とDB/保持/復旧を混ぜた負荷。1万/5万登録、最大pending/nullifier/GC、全fork受信を含む有限性。 | CPU/RAM最低値、持続1 BPS、制御安定性・実利用率は未測定。 |

仕様上の約240〜360秒の反映遅れと整数gainだけでは、実際の収束、需要急増への追従、受付率、公平性を確定できない。有料で形式・資金を満たす負荷操作や、canonical化前の検閲もD単独では除去できない。これらを無条件保証へ読み替えず、hard budgetで被害を制限し、残る運用・配送前提を測定する。

## 6. 結論の使い方

現文書は、court容量を守るRFC-0055と役割を分け、**全claimの受付候補量を一つのDで調整し、合格後も既存の資金・全工程予約・Final条件を要求する**案として整合している。新Dは未実装で、共通capacity/checker/clock/料金profileと全経路接続が揃うまで公開有効化できない。

この記録は設計読解レビューの完了を示す。実装・性能・公開activationの完了宣言ではない。後続作業は[実装接続表](../../../design/palw/integration/claim-admission-difficulty-implementation-plan-2026-11-17.md)のgateで個別に確認する。
