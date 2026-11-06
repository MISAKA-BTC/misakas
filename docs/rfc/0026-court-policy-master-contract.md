# RFC-0026 — court の主契約:時計・帰責・責任期間・置換関係の一本化

- **状態:** Draft / Review Required。2026-10-19、RFC 全件整合性監査([0026-assets/audit-report.md](0026-assets/audit-report.md)、指摘 F01〜F11)を受けて、ユーザー指示により作成。
- **役割:** court に関する RFC-0008・0009・0014・0018・0023・0024・0025 の記述が食い違って見える箇所について、**どの値がどの時計に当たるか、どの条項がどれを置き換えるか**を 1 か所にまとめる「current court policy」。本書は新しい値を決めない。ユーザーが採用した方針(RFC-0014 2026-10-19 改定)を主契約とし、それ以外は Draft・未決として区別する。
- **非変更:** ZK・SNARK/STARK・zkVM・PCS による計算証明は使わない(PRINCIPLES §8)。モデル単位の審査はしない(PRINCIPLES §9)。オンチェーン対話の方針は撤回しない(監査の結論)。
- **有効化:** 本書の条項はどれも休眠。§9 の未決条件(とくに P0 の F02・F03)が閉じるまで、関連 fence は武装できない。

## 1. 監査の結論(要約)

設計の骨格は一貫している。ただし「オンチェーン対話にしたので、期限まで応答が収録されなければ安全に slash できる」という保証はまだ完成していない。11 項目は稼働中のバグの発見ではなく、RFC 自身が未確定と認めている条件、複数 RFC の統合の不整合、目標と既存形式の非両立を分けたものである。

| ID | 優先度 | 本書での扱い |
|---|---|---|
| F01 | P1 | §2 の時計表で一本化 |
| F02 | P0 | §4 に帰責の規範を置く。具体的な判定規則は未決(§9) |
| F03 | P0 | §5 に要件を置く。具体的な経路は未決(§9) |
| F04 | P1 | §6 で profile ごとの 4 役割を必須化 |
| F05 | P1 | §7.1 で SC64 を現行形式の未達目標と明記 |
| F06 | P1 | §7.2 で数値 fold 検査の維持を明記 |
| F07 | P1 | §3.3 で最大延長を受付時の契約に含める |
| F08 | P1 | §4.3 で終了理由ごとの効果表を要求 |
| F09 | P2 | §2.2 で境界表を要求 |
| F10 | P1 | §8 で実装状態を区別 |
| F11 | P2 | §8 の置換表と、各 RFC からの参照注記 |

## 2. 時計(F01・F09)

### 2.1 主契約

`a` を original claim の受付 DAA(merged block の header ではなく、accepting chain block の DAA。RFC-0019)とする。

| 時計 | 値 | 起点 | 根拠と強さ |
|---|---|---|---|
| 通常の申立て受付の締切 | `a + 3,120` | `a` | RFC-0014 2026-10-19 改定(ユーザー採用) |
| 係争の hard deadline | `a + 10,000`（期限内に有効な申立てを受理した claim） | `a`（再申立てで reset しない） | 同上 |
| 各手番の応答期限 | 手番ごと。hard deadline とは別の制約 | 手番の開始 | 同上 D。値は未決 |
| door ≤ 3,200 | 対象 profile の**性能目標** | `a` | RFC-0024・0025(Draft)。主契約ではない。3,120 → 3,200 は明示承認の対象 |
| 混雑延長 `E_i` | **hard deadline の内側**での予約調整に限る:`a + 3,200 + E_i ≤ a + 10,000` | `a` | 本書の整理。RFC-0025 §4.3 の `E_max` は `≤ 6,800` に読み替える。それを超える延長は別の将来方針で、採用していない |

- 10,000 を維持するので、**10,000 の内側で処理できる負荷だけを original claim の受付で引き受ける**。受け付けた後に、正当な court だけを容量不足で捨てない。例:64 KiB × 10,000 件を court 用 25,000 B/DAA で運ぶと、転送だけで 26,215 DAA かかる(RFC-0025 の仮定による算術)。この負荷は 10,000 の内側に入らないので、受付の段階で抑制する。
- `10,000 − 3,120 = 6,880` は予算の差であり、最後に受理できる位置には等号・排他的境界の余裕を別に入れる。遅い申立て・毎回合法な最遅応答・最後の証拠・収録・回復の全部が残余に収まることを、機械的に検査する(T06)。TFP1 の一括対話候補で使う比較式と予約の境界は §11.1・§11.4 に定める。
- 3,120・10,000 は bond 全額の無条件返還時点でも、全証拠の削除時点でもない(§3.3)。

### 2.2 境界表(F09。値は未決、形式だけ固定)

notice、root、各 round、choice、最終 fragment、`collect_until`、hard deadline、default sweep、refund / bond exit のそれぞれについて、次を表にして実装・試験で共有する:起点、単位(合意 DAA)、受理区間(閉/開)、等号、同一 block 内の処理順(canonical ordering)、reorg 後の再計算。RFC-0014 10-12 改定の `d <= deadline` は例示であり、`collect_until` は排他的(`daa < collect_until` で受理、既存 fold も `daa >= collect_until` を拒否)。両者は別の時計なので矛盾ではないが、同じ「期限」という語で置き換えない。

## 3. 事前予約と責任期間(F01・F07)

### 3.1 受付時の予約

original claim を報酬対象として受け付ける前に、hard deadline までに必要な検証・開示・最終裁定・保持・回復の資源(bytes、検証 work、IO、保持 byte-DAA、control 数)と、料金・担保を予約する。指定の burst で全 claim が同時に争われる場合を評価する。確率的 overbooking は、全員共謀時の最悪保証として表示しない。

### 3.2 混雑と過負荷

予約は合意状態から決定論的に導出し、後から来た申立てで先行の予約を動かさない。入りきらない新しい経済的責任は受付で抑制する(RFC-0025 §7.3)。既存の有効な申立ての期限・権利を消して帳尻を合わせない。

### 3.3 署名済み依存契約の内側に収める(F07)

- 新しい original claim の受付時に、**最大の係争期間(`a + 10,000`)と必要な後続責任を、すべての依存契約(EXEC 依存 worker、provider lease、kernel liability、custody pledge)が既に覆っていること**を検査する。覆わない claim は受け付けない。
- 延長のたびに被告の新しい同意を取り直す設計にしない(被告が同意を拒んで訴追を止められるため)。最大延長は最初から署名・担保・資金に含める。署名済みの元 frame は書き換えず、versioned な effective terms を導出する(RFC-0025 §6.1)。
- RFC-0008 R8-WINDOW(依存 path の最短の署名済み horizon を越えて延長しない)は維持する。本条はその horizon を受付時に hard deadline まで取らせることで両立させる。
- bond の解放は、claim、EXEC 依存、provider、post-Final の公開告発 window(RFC-0018)、Gate C、既払・未払の費用、monetary material の保持(RFC-0008:最後の実支払 + W_mat)の**最大の必要期間**を同時に満たしてから。同じ bond が負う別 claim の責任は残る。
- 旧 claim には旧 ruleset を適用する。

## 4. 無応答の帰責(F02、P0)

### 4.1 規範

チェーン上の「期限まで有効な応答がない」は、次の 2 つで同じ履歴になり得る。

```text
A:Miner が応答しなかった
B:Miner は有効な応答を署名・送信したが、配送や収録を妨害された
```

署名は発言の帰属を認証するが、未収録の送信や受信時刻は認証しない。したがって、**チェーン上の不在だけから A だけを処罰し B だけを救済する判定は作れない**。無応答による default は、次の 4 つが揃ったときに限る。

1. accepted な義務、応答権限(誰の鍵か)、必要 bytes / work、予約、期限、適格な応答の完了条件が、合意状態にある。
2. 「予定された正規の処理機会」が、合意状態から決定論的に計算でき、その機会が与えられた。
3. 合意で観測できない個別の通信妨害は、明示した inclusion / network 前提として扱い、その前提の外側を保証しない。
4. 前提が崩れた場合の扱い(Final 保留、担保と証拠の保持、復旧)を、無応答 default とは別の経路として定める。

- `service_ok` のようなフラグを置くだけでは解決しない。その根拠がローカル回線測定、Panel の申告、mempool の観測なら、全ノードで同じ判定にならない。
- 有効な応答が来たときだけ時計を進める案は採らない(永久沈黙で時計も止まる)。逆に、clock や heartbeat で DAA が進むこと自体は応答機会の証明ではない(RFC-0008)。
- Miner 自身の DDoS の申告だけでは延長しない(RFC-0014 E)。

### 4.2 DDoS 防御の層

| 層 | 対策 |
|---|---|
| 受理後の有料 court 負荷 | 容量予約と事前費用(challenger bond、少額の応答 fee、敗訴側の負担) |
| 受理前の不正 codec・重複 message・接続負荷 | 有界な受信 queue、長さ検査、署名検証前後の work 上限、peer 処理の制限。**少額 fee ではこの層を防げない** |

### 4.3 終了理由ごとの効果(F08)

各終了理由(計算不正の確定、開示・応答義務違反、申立ての棄却、Panel の放棄、service 前提の崩壊、hard deadline 到達)について、claim の credit、court の終了 / 復旧、担保、証拠、共有 frontier、pruning への効果を別の field で表にする。

- 未解決を「解決済み」に書き換えない。期限だけで有効な Final にしない(RFC-0025 §6.3)。
- RFC-0023 段階 1(共有 frontier を保ち `L_class` で上限)と段階 2(独立進行、別 fence・未完)を区別する。段階 2 が未有効なら、その独立進行を段階 1 の可用性の説明に借りない。service 障害中も `L_class` 以内という保証は要求しない(RFC-0023 自身がネットワーク停止中の前進を保証しない)。

## 5. 外部検証者 1 人の訴追経路(F03、P0)

- 共謀者が owner 枠(既存の上限 8)をすべて取って合法的に動き続けても、外部の正直な検証者が訴追できなければならない。「外部検証者用に 1 枠残す」だけでは、その枠を Sybil が取れるので解決しない(RFC-0025 §7.1)。
- 外部検証者が**完成した直接証拠をまだ持っていない**場合が要点である。「完成済みの直接反証はいつでも受理する」だけでは、局所化に必要な開示・対話に入れない。
- 要件:original claim の受付時に、許容する敵対的分岐を含む責任容量を予約する。争点・root・範囲・手番に対する開示を共用できるようにし、**先着した申立人に claim の訴追経路を独占させない**。完成済みの直接反証の優先経路と、まだ開示を要する適格な訴追の経路を両方示す。どこまでが保証対象の受付で、どこからが過負荷拒否かを明示する。
- 申立て fee と bond だけで、この条件が満たされたとは判定しない。未証明なら `OneOutsiderCapacityIncomplete` として武装を拒否する(RFC-0025 §7.2)。

## 6. 署名者・資料提供者の役割(F04)

RFC-0009 の「計算と署名が終われば Miner は PC を止められる」は、既存の TIR court で root claim と各 round に Miner の claim bond の鍵の署名が要ることと、そのままでは両立しない。provider は bytes を持っていても Miner の署名を作れず、第三者 relay は作成済みの署名を運べるだけで、将来の適応的な応答を新しく署名する権限はない。

- profile ごとに、**資料を保持する者・新しい応答を署名する者・投稿を中継する者・不履行の責任者**を明記する。
- 現行の署名方式を保つなら、court の義務が終わるまで利用可能な court signer を残す。PC の完全停止も保つなら、限定された応答権限の委任、または第三者が追加署名なしで提出できる認証済みの終了証拠の経路を、別仕様として採択・検証する。資金移動用の鍵を provider に渡すことを解決策にしない。
- 追加署名なしの直接証拠や認証済みの終了証拠で閉じる profile は、別に扱ってよい。

## 7. 証拠量の目標(F05・F06)

### 7.1 SC64(64 KiB)は現行形式の未達目標

既存の各手番の署名(ML-DSA-87、4,627 B)を残したまま全履歴を絞ると、root + 履歴対話の署名は `1 + 2R` 個になる(tile = 16)。

| 条件 | R | 署名数 | 署名だけの bytes |
|---|---:|---:|---:|
| 32K・4 分岐 | 6 | 13 | 60,151 |
| 2M・4 分岐 | 9 | 19 | **87,913**(> 65,536) |
| 32K・16 分岐(仮定) | 3 | 7 | 32,389 |
| 2M・16 分岐(仮定) | 5 | 11 | 50,897 |

監査の読み取りでは、出荷 t12 preset の分岐数は 4(`review_heldnode_arity.rs` の期待値)。2M・4 分岐では、申立て・部分値・operand・opening・最終証拠を足す前に署名だけで 64 KiB を超える。これは算術上の下限で、実 proof の測定ではない。**SC64 は現行 profile の達成値ではなく、認証方式・投稿数を含めて別 profile として検証する目標**とする。現行 profile・16 分岐候補・先読み profile を別々に名付け、分岐数・tile 幅・context・署名数・各 body・最終証拠の最大値を 1 つの codec 集計にする。最後の約 12 KB や fragment 17,408 B を総量に流用しない。

### 7.2 数値 fold 検査は削らない

RFC-0024 本文(B)の各 round の数値 fold 検査を、現行の規範として維持する。付録 C の commitment(hash)中心の局所化は**未採用の研究・設計候補**である。親 100・子 40 と 50 がすべて正しく認証されていても `40 + 50 ≠ 100` という集約の不正は残り、最終 leaf だけを調べる方式はこれを見落とす。採用するなら、任意の内部の親子 fold 不正を局所的に反証できる証拠と、その root への認証を定義し、完結性を示してからにする。

## 8. 実装状態と置換表(F10・F11)

### 8.1 実装状態の区別

| 機能 | 状態 |
|---|---|
| `palw_court_capacity_v1`(court 専用 block 予算・予約枠・期限順の割当・新規 claim 受付制限) | **別 branch の実装**(`release/t12-16000` に存在。`pre` には未統合)。休眠、値は仮。新しい提出保証が実装で閉じたとは言えない |
| 履歴 tile 二分探索・1 位置化・tag 245 登録簿 | 別 branch(`g14/door-design`)、休眠、単体試験のみ |
| X3 の固定期限・open session による Final 保留 | 別 branch `e27771b96`（起点 `05fcf7b5c`）。合法な浅い reorg の予約/session rollback と復元を release node 試験で確認。関連 suite 33 passed / R-D2-NODE 2 ignored、t12 同一性 26 passed。未統合・有効化なし。詳細は [記録](../evidence/palw/g14-completion-2026-10-20/README.md) |
| F02/F03 未完による武装拒否 | 別 branch の実装 `62ac3c55f`。readiness record による迂回を拒否。release 部品・回帰試験 29 件通過、実経路・敵対的統合試験は未完、有効化なし |
| §2/§3.3 の時計・依存 horizon 導出部品 | 別 branch の実装 `62ac3c55f`。state/fold 未接続。原子的期限更新・T06/T09 完了を意味しない |
| F02/F03 予約 scheduler 部品 | 別 branch `0336b6cea`、release 部品・回帰 32 件通過。全分岐の導出、供給会計、block admission、T04/T05 は未接続・未検証。有効化なし |
| F02 供給会計部品 | 別 branch `0ea164766`、release 部品・回帰 36 件通過。同一 DAA/skip/予約 quota を検査。state/fold/codec/snapshot、期限補正と Final hold は未接続。有効化なし |
| ADR-0194 A の隔離統合候補 | `codex/rfc14-g14-integration-candidate` の `b9c6d0656` で main/X3 の選択部分を統合。選択 joint release 93件。pre未統合・公開採択なし・有効化なし。[範囲と後続記録](../evidence/palw/g14-integration-candidate-2026-10-20/README.md) |
| rooted service v2 / native duty clock | 隔離候補 `705b27616` までで key/row 12/18・13/19、native duty/prerequisite clock、journal/保持・default拒否を接続。選択64件。実供給・原子的な全分岐受付・帰責の完成ではない |
| owner非依存の rooted query/data | 隔離候補 `b980f5788`、候補key/row 14/20〜16/22。build済み、選択69件の限定敵対的fixture検証済み（全8owner占有、原データ改変拒否、undo/reload等）。有料lifecycle/Ready応答/全分岐予約/T05は未完、有効化なし。[Draft RFC-0029](0029-owner-independent-shared-tir-query-data.md) |
| Ready照合・永続signed stream/EOF | 隔離候補 `33738ea26` 後、候補key/row17/23・18/24。build・限定敵対的fixture検証86件、non-test check成功。[source hash・生ログ・範囲](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-signed-shared-stream-native-adapter)。共有chunkのcheckpoint再コピーとArc preflight会計不一致を修正。公開lifecycle・完全予約/fee・fresh duty・実供給・T05/G14は未完、有効化なし |
| 共有データとliteral signerの明示的応答の分離 | 隔離候補 `fa1104eed` 後、候補key/row19/25。署名を受理済みquery block/DAAと認証済みデータへ束縛し、実データ前提のresource起点とreceiptを分離。選択89件・non-test check・生成物一致成功。[範囲とhash](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#explicit-fresh-data-response-native-candidate)。data replyのみで、全semantic duty/default・完全予約/fee・実供給・node T05は未完、有効化なし |
| 本書 §2〜§7 の残りの規範 | 設計だけ。F02/F03 の選択肢は [ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)（Proposed） |

上表の旧 branch の件数は各checkpointの履歴であり、隔離候補の件数に加算しない。「設計だけ / 別 branch 実装 / 統合済み / build 済み / 敵対的試験済み / 有効化済み」を別々に記し、設計メモ・別 branch のコード・toy test を混ぜて「完了」と表示しない。

### 8.2 置換表

| 事項 | 現行規範(稼働 t12) | 採用済みの次期方針 | 未採用・Draft |
|---|---|---|---|
| 申立て受付 | X3 固定期限(RFC-0014 10-12 改定) | `a + 3,120`(RFC-0014 10-19) | — |
| 係争の hard deadline | X3 `hard_deadline` | `a + 10,000`、異議のある claim だけ(RFC-0014 10-19) | — |
| door | `maximum_door_daa = 3,120`(validation の上限) | — | ≤ 3,200(RFC-0024・0025、要承認) |
| 混雑延長 | なし | — | `E_i`(RFC-0025)。本書 §2.1 で hard deadline の内側に限定 |
| 証拠量 | 現行形式 | — | SC64・32 KiB close(RFC-0025、§7.1 で現行形式は未達) |
| 局所化 | 各 round の数値 fold | — | hash 中心(RFC-0024 付録 C、§7.2) |
| TFP1 の一括対話 | 各要求・応答を既存の規則で受理 | §2.1 の二段階期限 | Batch-Pipelined Court（§11、別 ruleset の候補） |
| 無応答の帰責 | 応答義務のある側が期限超過で負け(既存仕様) | 収録機会に結びつける(RFC-0014 10-19 E) | 判定規則の具体化(§4、未決) |

旧本文は履歴として残し、各 RFC から本書を参照する注記を付ける。正式な採択には、RFC-0021 と凍結規程どおり、対応する ADR と条項の置換節が要る。

## 9. 未決(武装を拒否する条件)

| 優先度 | 未決事項 |
|---|---|
| P0 | F02:正規の処理機会の具体的な計算、合意で観測できる service 違反の定義、inclusion / network 前提の範囲 |
| P0 | F03:全 owner 枠の共謀占有下で、未完成の局所化段階から外部 1 人が訴追できる具体的な経路 |
| P1 | F04:profile ごとの court signer の可用性(または委任・追加署名不要の終了証拠) |
| P1 | F05:SC64 を採るか、現行形式に別の上限を承認するか |
| P1 | F07:依存契約が受付時に `a + 10,000` を覆うことの検査の実装 |
| P1 | F08:終了理由ごとの効果表 |
| P2 | F09:境界表の値 |

修正の順序:F01・F02・F03 → F04・F07・F08 → F05・F06 → F09・F10・F11。期限の数値や少額 fee だけを先に変えても、提出妨害による不当な timeout 処罰は解決しない。

## 10. 統合受入試験

監査の T01〜T15([0026-assets/audit-report.md](0026-assets/audit-report.md) §7)を、関連 fence の有効化条件とする。主なもの:正直な Miner と誤った challenger(T01)、Miner の永久沈黙(T02)、偽署名・Panel の架空の完了申告(T03)、正常 tx flood・有料 court・不正 codec flood の分離測定(T04)、全 owner 枠の共謀占有下の外部 1 人(T05)、最後の受付時刻での notice と最遅応答(T06)、境界 −1 / 一致 / +1(T07)、Miner の PC 停止後の court(T08)、最短の依存 lease と同じ bond の複数 claim(T09)、内部 fold だけの不正(T10)、1〜10,000 件の最終段だけ不正(T11)、10,000 DAA への集中(T12)、最古 claim の service 失敗と独立 claim(T13)、reorg・restart・pruning・pruned IBD(T14)、新旧 fence 境界(T15)。

fixture での `close_cone_acquit` の軽さ(`docs/measurements/2026-10-19-court-verify-cpu.md`、`release/t12-16000`)や、中央値の小ささは、これらの最悪 case・転送・収録・復旧の代わりにならない。

## 11. TFP1 の一括対話（Batch-Pipelined Court）

**状態** 2026-11-03 の実装候補。二段階の期限は RFC-0014 の採用方針に従い、本節で TFP1 の一括対話に適用する。初回異議の境界は RFC-0031 A を使う。有効化後に新 ruleset で受理した original claim に適用し、旧 claim は受付時の ruleset で再現する。公開 t12 の既存 TFP1 受付では、資料取得から最終提出までの見積もりに `maximum_door_daa = 3,120` を適用する。新 ruleset の 3,120 DAA は初回異議の受付窓であり、有効に開始した係争は original claim の受付から 10,000 DAA の内側で処理する。[RFC-0014](0014-panel-independent-fraud-prosecution.md)

### 11.1 申立てと対話の起点

`a` は original claim の実 accepting-chain DAA、`d` は各操作の実 accepting-chain DAA とする。

| 段階 | 候補の DAA 条件 | 受理時の処理 |
|---|---|---|
| Initial notice | `a ≤ d ≤ a + 3,120` | 適格者が対象の original、署名、原資、後続の開示や検査に進める具体的な根拠を示す。Node は検出の根拠と係争の開始を記録する。裁定と slash は §4 の条件を別途満たす。 |
| 後続の要求・応答・証拠収録 | `d ≤ a + 10,000` かつ `d < collect_until`。各操作の締切と先行条件も適用する | 受理済みの notice と同じ original・ruleset・profile に結び付け、受理済みの資料から Node が独立に裁定する。 |
| 事前予約のサービス枠 | `a + 3,121 ≤ d < min(a + 10,000, collect_until) − recovery_daa` | 最遅 notice の翌 DAA から配分し、§11.4 の回復枠と境界余裕を確保する。 |

境界値は checked arithmetic で求め、overflow は受付拒否とする。排他的な `collect_until` の最大値は `a + 10,000 + 1` とし、資料保持は少なくとも `collect_until` までを覆う。個別 Demand の応答も `d < response_deadline_daa` を満たす。同一 DAA 内の順序は実際の受理順で判定する。Initial notice と、受理済み notice を参照する Batch Open は別の操作であり、Open、後続 Demand、再送、再申立て、owner 交代で `a` や hard deadline を更新しない。[RFC-0031](0031-shared-court-initial-notice-boundary.md)

境界試験では `a + 3,120` の適格な notice を受理し、`a + 3,121` の新規 notice を拒否する。同じ `a + 3,121` でも、既に受理した notice に基づく後続操作には表の後続条件を適用する。`a + 10,000` は外側の期限であり、候補の rooted Open は `collect_until = a + 10,000` を使うため、その DAA の応答は収録窓の条件を満たさない。事前予約はさらに早い位置に終端提出と回復枠を確保する。

有効な notice の受理時には、開示義務、証拠収録窓、資料保持、Final と報酬の保留、bond の解放条件を原子的に適用する。notice がない claim は通常の認証・決済・保持条件に従う。元の claim の受付時に、署名済みの依存契約が最大係争期間とその後の保持・回復を覆うことを検査する。裁定後も §3.3 の残存義務に従い、実効期限で Miner／Provider の保存と Node の GC を制御する。異議受付期間が満了しても、Gate C に必要な独立席の承認やその他の Final 条件は引き続き個別に検査する。

Batch 専用の共有 context では、Node は初回 notice の受理時に、同じ original と scope に属する対応経路の Open 枠を照合する。notice は係争開始だけを記録する。資料要求は受理済みの notice ID に束縛した Batch Open で開始し、同一 notice の再送は同じ受付にまとめる。後続操作は Batch の予約を使用し、旧共有経路の資料搬送枠は割り当てない。reorg で notice が消えた branch では、その notice を参照する Open と後続状態も無効とする。正規の別署名者による共有訴追経路には §5 の条件を適用する。

### 11.2 役割と対話

1. Miner は通常の推論を実行し、モデル・入力・出力・実行規則と必要な実行 commitment を固定する。係争時は認証に必要な既存資料を、受理済み要求の範囲で返す。
2. Panel は再計算して争点と依存関係を特定する。同じ前提から取得できる資料を一括 Demand にし、前段の裁定や応答によって決まる要求は後続の手番に分ける。資料が揃ったら局所反証を構築する。正常側の証拠を提出する profile では、その profile が要求する全範囲を Panel が構築する。
3. 署名権限のある Miner／Provider は、同じ要求内で独立した part を、前の part の収録を待たず上限内で送る。共有可能な資料は元の source と要求範囲に認証する。資料の受理と、特定署名者の応答義務の履行を別に記録する。[RFC-0029](0029-owner-independent-shared-tir-query-data.md)
4. Node はモデルの事前コピーや Panel の私有 DB に依存せず、受理した資料を認証して数値関係を独立検査する。独立して認証できる part は収録後に順次検査する。全体認証が必要な範囲は、全 part の収録と認証が揃った時点で検査済みにする。局所的な `NoFaultFound` は調べた争点の結果として記録する。正常側の必要範囲が欠けた場合は未検証とし、範囲の検査が完了した場合も Gate C の独立した抽選席による認証条件と所定の Final・決済条件を適用する。計算不一致、資料不足、処理機会の不足、義務違反の帰責を §4 に従って区別する。

### 11.3 実装契約

| 部分 | 要件 |
|---|---|
| Tokenizer | クラスの登録者が署名した PALWART3 binding を受理し、`palw_class_semantics_v1` が有効な登録経路で、厳密な parser、EOF、key の一意性、全 token 表、rendering、EOG を Node が有界に検証する。登録時の規則版、raw tokenizer と token 表の root、artifact・binding を含む認証済み記録を受理済み状態へ結び付ける。claim は元の受付点で有効な記録を参照し、入力・出力に固有の関係を検査する。初回の検証・署名・保存を登録経路で計上し、restart・reorg・IBD で同じ記録を復元する。この記録を前提とする profile は、受付点に binding または記録がない場合や文脈が違う場合に拒否する。この検査の対象は claim の検証に使う tokenizer 資料の同一性と形式である。 |
| Owner | `material_retention_daa`、`service_quantum_daa`、`response_deadline_daa`、`idle_grace_daa` を分離する。応答待ちの owner は要求・期限・予約を保持して実行枠を解放し、有効な応答で公平な再入場の対象になる。実行枠と応答待ち予約を別に制限し、native の取得行と同じ branch の原子的更新で記録する。待ち時間は実際の scheduler と予約から算出する。外部の適格な申立人が全 owner 枠の共謀占有下でも利用できる共有経路を §5 に従って検証する。 |
| TIR 局所化 | 演算・tile・reduction の境界を元の source に認証し、行列積の部分和や attention の最大値・総和・重み付き集約などを小さな決定論的検査単位にする。パラメータに加え、動的 operand、親子の数値 fold、選択に必要な比較、opening と終端を計上する。局所反証の費用と正常側の全範囲の費用は別に測る。 |
| Batch 搬送と復元 | Demand は case、元 source と ruleset に束縛した要求 root、canonical な範囲・長さ・認証方式、part 上限、総 bytes 上限、応答期限、予約を固定する。Node は認証方式を元資料の形式から決定する。Response は case・要求 root・範囲・part index・payload を署名者と opening に結び付ける。受信順に処理しても canonical な範囲・slot で検査結果を確定する。正当な重複で進捗・予約を二重計上せず、競合・改変は拒否する。受理済み・検査済み・未取得の範囲と残り予算は branch が認証する checkpoint から復元する。範囲全体の認証を要する形式では、元資料の全体検査に成功した時点でのみ全 slot を更新し、部分的な検査済み状態の復元を拒否する。送受信キューを件数と bytes の両方で制限し、Node の処理容量に応じた backpressure を Panel の証拠生成まで伝える。 |

現在の rooted tokenizer 候補では、同じ branch で登録記録の DAA と blue score がともに claim の受付点より前であることを要求する。受付の見積もりでは、クラス未登録、binding 未受理、semantics 未有効、認証済み記録不足、対話予算超過を区別する。公開 t12 の SmolLM2 事前確認では、登録の見積もりが通っても受理済み登録がなく、semantics も未有効だった。[公開 t12 の事前確認](../evidence/oom-recovery-continuation-2026-11-03.md#palw-clock-builder-and-tfp1-parallel-follow-up--0806-utc)

登録経路は、クラスの登録、登録者が署名した binding の受理、tokenizer 資料の公開と EOF までの検査、認証済み記録の確定の順に進める。登録者の署名は binding に必要となる。記録が確定した後、claim の提出者は自分の適格な executor bond を用いる。公開 t12 の DAA 19,615 の再試験では、試験用クラスが未登録で、既登録の TIR 2 クラスにも受理済みの binding がなかった。これらは時間見積もりとは独立した受付条件であり、当該試験では Studio は carrier を提出していない。

#### 署名付き Batch 要求の開始・応答・再開

候補の lifecycle carrier は `BCT1` の識別子を使い、`Open`、`Respond`、対応 profile の終端提出 `Stop` を区別する。Node は範囲数、part の長さ、署名長を割当て前に検査し、未定義の版・操作や余分なバイトを拒否する。この形式は新 ruleset の候補であり、公開 chain での受付は §11.5 の条件を満たした後に有効化する。

- **Open。** 初回 notice を出した適格な署名者が、ネットワーク、元の claim と scope、受理済み notice ID、要求番号、署名者、canonical な要求 root に署名する。Node は対象 branch の notice と現在の bond 適格性、元の claim に予約済みの開始枠と実収録時刻を照合する。ひとつの開始枠に保持する要求はひとつとし、同一要求の再送で予約や期限を増やさない。受理済みの要求と root を branch の状態に保存する。
- **事前予約。** 応答枠は元の claim と scope、要求番号、canonical な範囲の位置、Begin・Part・Finish の段階で識別する。元の claim の受付では、後から決まる notice、要求 root、応答者、資料ハッシュを使わず、各位置の最大応答を予約する。Open の際は、全要求範囲の枠、符号化サイズ、割当順序、draft 期限を照合する。実際の notice と要求番号を要求 root に結び付ける。
- **Respond。** Begin・Part・Finish の署名は、ネットワーク、元の claim と scope、受理済み要求 root、資料の座標、応答者、操作内容に結び付ける。Node は carrier の要求番号から受理済み要求を読み、Begin の宣言または Part・Finish が参照する draft を使って予約枠を導出する。元の claim が所有する枠、要求の期限、署名を照合してから状態を更新する。
- **再開。** 同じ要求 root と範囲にひとつの draft を持つ。最初に受理した Begin が応答者、長さ、資料ハッシュを固定する。同一宣言の再送は同じ draft に対応し、競合する宣言は拒否する。応答者の競合や無応答から別経路へ移る profile では、代替要求の枠も元の claim の受付時に予約し、元の hard deadline 内で扱う。受理済み draft と part は、同じ要求 root に束縛した draft ID で復元する。要求、署名、draft の取り違えを拒否する。先行送信された part はキュー上限と収録時の依存条件に従う。受信だけで義務の履行や検査完了を記録せず、完成後に共有資料を再利用する場合も、元資料への認証と応答者固有の義務の履行を個別に扱う。
- **Stop。** 現在の候補は制約付き EOG の局所裁定を扱う。Node は同じ branch の元の claim・source、受理済み `FullIds`、受付点で有効な tokenizer 記録、予約済みの終端枠、署名付き intent、EOG opening を照合して局所再計算する。結果の receipt には source、提出資料の digest、実収録点、保持期限、見つかった場合は不一致位置を記録する。不一致が見つからない場合の記録は当該争点に限る。credit、slash、Final、報酬に反映するには §4 と §11.2 の条件を満たす終端遷移を別途検証する。
- **会計。** 予約は carrier の識別子・操作種別・要求番号を含む実際の符号化サイズを使う。資料保管料は同じ carrier の native 手数料枠から配分し、要求番号、要求 root、署名済み操作、実収録点、金額を照合する。同じ手数料を別の費目に重複計上しない。

#### 応答枠の参照と提出

`getPalwCourtState` は claim、要求番号 `batchOrdinal`、資料種別 `batchTarget` を受け、選択 branch の同じ snapshot から `palwBatchResponse` を返す。要求番号は 64 bit の値を失わないよう十進文字列で送る。応答には tip、claim、scope、要求番号、資料種別、要求 root、draft ID、Begin・Part・Finish の予約枠、保持中の受理済み part を含める。Studio は view の版と種別、claim、要求番号、資料種別、tip の形式を照合し、取得元とともに表示用に返す。この参照は署名・収録・裁定の状態を進めない。[RPC](../evidence/oom-recovery-continuation-2026-11-03.md#batch-response-reads-through-the-existing-court-rpc) [Studio](../evidence/oom-recovery-continuation-2026-11-03.md#studio-batch-response-observation)

数値争点の手動経路では、operator RPC の `getPalwCourtState` に `batchOpen` を指定して、選択 tip に結び付いた `palwNumericalBatchOpen` 見積もりを取得する。`misaka palw batch-open` は、元の claim の予約位置と受理済み notice に束縛した要求 root をローカルで再計算し、署名と原資付き carrier を組み立てる。数値争点向けの Open と、制約付き EOG の Stop に先立つ専用 `FullIds` 要求は、用途と予約を区別する。Stop の予約では、`FullIds` の Finish より後に Node 検査の枠を置き、その後に終端提出の枠を置く。終端提出時には同じ claim の受理・認証済み `FullIds` を照合する。[数値 Open の見積もり](../evidence/oom-recovery-continuation-2026-11-03.md#operator-rpc-for-numerical-batch-open-quotes) [手動 Open](../evidence/oom-recovery-continuation-2026-11-03.md#sign-and-fund-a-numerical-batch-open)

手動の `misaka palw batch-respond` は、ローカル資料から Begin・Part・Finish のいずれか一つを、登録済み bond 鍵で署名し、手数料を賄う UTXO を使って組み立てる。再開時は受理済み Begin の応答者と資料ハッシュを照合する。送信を指定した場合は、観測した fold DAA が予約枠に一致し、再取得した tip と見積もりが一致したときだけ送信する。Node が実収録点で検査した結果を受理の根拠とする。[手動応答の実装と検証範囲](../evidence/oom-recovery-continuation-2026-11-03.md#sign-and-fund-a-booked-batch-response)

再開時の view では、同じ branch が対象の認証済み資料を既に保持しているかも確認できる。資料が揃っていれば、再開処理は重複する Begin・Part と未使用の応答枠の再予約を省く。既存 draft の Finish は所定の条件で処理し、署名者固有の応答義務は別に判定する。ブロック候補の選定では、fence の有効後に認証済みの選択親状態から Open の notice と開始枠、Respond の要求・draft・応答枠を事前照合する。枠外の carrier は候補への収録を見送り、mempool での保持は既存の規則に従う。候補への選定、mempool 受信、RPC 見積もりはいずれも、実収録点で行う署名・会計・資料・裁定の検査から独立する。[資料の再利用](../evidence/oom-recovery-continuation-2026-11-03.md#batch-resume-recognizes-already-authenticated-material) [候補の選定](../evidence/oom-recovery-continuation-2026-11-03.md#template-selection-checks-rooted-batch-open-and-respond-slots)

#### TIR の局所要求とネイティブ認証

Panel は、元の実行 source に結び付いた争点 root の入力境界から、必要な step leaf を導出する。同じ leaf への要求をまとめ、連続する範囲を既存の `StepRun { first, count }` に写す。離れた範囲は別の要求にする。各区間は元資料の規則が定める最大 leaf 数と、ネイティブ応答サイズの上限を使って求めた最大 part 数の両方に収める。要求数の上限を超える計画は予約に使えない。

各区間の要求 ID は、元の claim、資料の割当、適用規則・profile、source、canonical な要求位置に結び付ける。受理済みの区間を Node が認証した後、裁定側は元の leaf と opening を取り出して既存の局所裁定を実行できる。区間をまたぐ場合も、同じ source に認証された端点を使う。要求 ID を導出した時点では、署名者への応答義務や応答の受理は成立しない。

数値裁定の候補では、各必要範囲の認証済み Finish を先行条件とし、数値検査と終端提出を別々の枠に予約する。数値検査 intent は、ネットワーク、元の claim と scope、受理済み notice、選択された leaf、適用する native policy、申立人、数値検査と終端提出の予約 ID に署名する。Node は同じ notice の署名者の適格性と、元の claim が所有する検査枠の実収録 DAA・サイズを照合する。実収録 DAA は現時点の予約から判定するため、認められた枠の再割当てで署名対象は変わらない。

Node は選択した leaf に必要な受理済み資料だけを読み出し、元の実行に対する既存の TIR 局所裁定を、有界なメモリと検査量で実行する。一致と不一致を局所裁定結果として区別し、資料不足・改変・資源上限超過は検査エラーとする。終端処理には、同じ branch の検査結果を元の claim と予約に結び付けて保存し、reorg で同じ単位に巻き戻す契約が必要となる。数値 replay の分離試験では正しい leaf の一致、改変した leaf の不一致、bytes・work 上限での拒否を確認した。数値 intent の carrier 受理、結果の保存、終端処理の統合検証は未完了である。[数値 replay の実装記録](../evidence/oom-recovery-continuation-2026-11-03.md#replay-the-selected-numerical-leaf-with-the-existing-tir-checker)

予約では、選択した区間ごとのネイティブ応答上限、必要な認証と制御枠、step 以外の依存資料、後続手番と回復を計上する。同じ root 内の要求の重複除去は、別 root や別の署名者の義務をまとめる根拠にはならない。局所反証の一例で得た使用量を、全許容分岐や正常側の全範囲の上限として使用しない。

### 11.4 時計と資源の予算

CPU work、bytes、IO、メモリ、保持 byte-DAA、control 枠を個別に予約する。DAA は先行操作の成立を待つ時間と、予約済み資源で処理・収録できるまでの時間を表す。各 part の検査費用は資源予算に計上し、独立した part に個別の応答待ち時間を加えない。先行する結果に依存する手番には、その結果の確定と後続の合法な最遅応答を計上する。同時処理を見込むときは、共有する帯域・CPU・IO と並行時の供給を計測して、全許容分岐で成立する予約に限って重複を除く。受付で使う時間上限は、許容する各分岐について、先行関係と共有資源の競合を反映した予約上の最遅完了点から求める。工程の重複を見込まない場合の保守的な見積もりは次式で表し、実際の受付には下記の予約検査も適用する。

```text
必要 DAA = 申立てまでの経過 DAA
         + 準備 + owner 待機 + 直列手番
         + ceil(搬送 bytes / 保証搬送 bytes per DAA) + Node 検査
         + 終了収録 + 再送・回復 + 1（T06 の排他的境界余裕）
必要 DAA <= 10,000
```

最後の有効な申立て `a + 3,120` では、差の `6,880 DAA` に境界余裕を含めるため、対話・収録・回復に配分できるのは最大 `6,879 DAA`。設計比較用の仮配分（準備 500、owner 800、要求・応答 1,400、搬送 2,400、検査 1,600、終了収録 1,200、再送・回復 1,100）は合計 9,000 DAA。最遅 notice を入れると境界余裕込みで 12,121 DAA となるため、期限超過の比較例に位置付ける。受付時に最遅開始を含む全合法分岐を予約できない profile は有効化を拒否する。見積もりは算出した必要量をそのまま返し、受付上限を別に示す。

段階ごとの見積もりには、初回登録と claim ごとの費用、owner が実行枠を持つ時間と応答待ち、範囲ごとの搬送・再送量、Node の検査量、終端収録、回復を別々に表示する。統合試験では見積もりを予約表と実収録に突き合わせ、最も遅い許容分岐とその律速資源を記録する。

事前予約の候補では、§11.1 のサービス窓で Open・最大長の Begin/Part/Finish・Node 検査・終端提出の枠をまとめて確保する。同じ要求内で独立した part は、予約済みの合計容量に収まる場合に同じ DAA を割り当てられる。後続手番は、入力を確定させる先行枠の後に置く。代替応答を後から開く場合は別の要求番号を使い、前の応答機会を予約上の先行条件にする。Node は応答の実受理を別に検査する。全枠と順序を検査した後、元の claim のサービス行に所有者と依存関係を一緒に記録する。

元の claim の受付時に、Node は登録済み資料と元の実行から全許容分岐の予約を導出し、役割別同意、実原資、容量、保持を同じ branch で原子的に確定する。RFC-0035 の契約を適用し、呼出側の部分 graph、仮単価、供給未確認の枠で受付を成立させない。応答待ちによる実行枠の解放は、予約済みの応答機会や応答者の義務を解除しない。[RFC-0035](0035-court-original-funded-contract-and-role-consent.md)

終端提出枠の排他的な上限を `e`、正の回復予算を `r` とすると、候補の必要量は `e − a + r` DAA であり、`min(10,000, collect_until − a)` 以下を要求する。修復によって収録が後ろに移る場合に備え、枠の最終 DAA `e − 1` までを見積もりに含める。最後の有効な notice と同じ DAA を、後続サービスの供給として計上しない。原子的な予約が成立しなければ、元の claim の受付を拒否する。

Begin と Finish の予約は、実際に割り当てる DAA で個別の draft の期限も満たす。期限は Begin の収録 DAA に適用する保存期間、係争期限、実効保持期限の最小値とし、Finish はその値より前に置く。Begin・全 part・Finish の依存関係と期限を一括で検査し、どれかが成立しない予約は確定しない。

### 11.5 実装・検証の順序

同じ SmolLM2 frame、class、artifact、入力で旧見積もり `8,782,776 DAA` と上限 `3,120 DAA` を再現する。内訳は tokenizer raw/key が `8,418,376`、表の rendering が `16,392`、inventory/semantics opening が `150`。残る `347,858 DAA` には owner 待機 `60,000`、取得 `287,806`、準備 `16`、その他の開始・収録 `36` が含まれる。これらは fixture EOF を使ったオフライン見積もりである。[SmolLM2 計測記録](../evidence/oom-recovery-continuation-2026-11-03.md#smollm2-tokenizer-court-quote-and-rendering-check--0538-utc)

1. Tokenizer の初回費用と claim ごとの費用を分け、同じ登録済み資料の再利用と、未登録・未 binding・semantics 未有効・受付と同じ DAA の登録記録・別 branch・改変の拒否を確認する。
2. Scheduler と見積もりを対にして、正直な申立人の待ち時間を全枠占有・応答の最遅到着・再入場で測る。
3. 対応演算と全許容分岐の資料上限を測る。数箇所で約 7.5 KB だったパラメータ証拠に加え、動的 operand、認証情報、全対象 root の最大量を計測する。
4. 実ノードで署名付き Open から Finish までを通し、未受理 notice、異なる要求番号・root、署名者の取り違え、保管料の不足を拒否する。先行送信、逆順、重複、欠落、再送、上限到達、backpressure、restart・reorg・pruned IBD を測る。予約量と実際の搬送・検査・収録を突き合わせ、T06 の最遅申立てでも hard deadline に収める。
5. 同じ候補で正常・不正・未提出・収録機会不足の裁定、信用・会計・保持の復元、同時 Court の混雑を §10 に従って確認する。最遅 notice と各手番の最遅応答、代替経路、回復枠を含め、最大合法負荷の下で各 profile の期限と資源予算を検証する。公開検証は同じ claim ID で Studio からの受理、独立席、Panel 応答、Node 裁定、Gate C、Final、PFS4、報酬を個別に照合する。

作業ブランチの候補には tokenizer 記録の導出、owner の rooted scheduler、TIR の有界な parameter 解析と認証済み StepRun の取り込み、Batch の受信・検査・checkpoint 部品がある。分離試験では、最遅 notice と明示的な回復枠を持つ合成 graph の予約結果が 10,000 DAA になり、回復を 1 DAA 増やすと予約を原子的に拒否することを確認した。Processor は UTXO 検証済みの BCT1 を受理順に Court へ渡す。制約付き EOG で不一致を確定した Stop は、署名済み資料を保持した receipt と同じ original に対する既存の `CourtFraud` 処理を一つの rollback 境界で更新する。不一致が見つからない場合は争点単位の結果を記録する。これらは候補コードと分離試験の範囲である。[最大枠の分離試験](../evidence/oom-recovery-continuation-2026-11-03.md#maximum-batch-reservation-at-the-latest-notice) [Processor の実装範囲](../evidence/oom-recovery-continuation-2026-11-03.md#processor-integration-of-accepted-batch-carriers) [Stop の会計と保存](../evidence/oom-recovery-continuation-2026-11-03.md#batch-stop-fault-settlement-and-archive-cause)

保持済み SmolLM2 の 15,252 leaf と 810 数値 root を走査し、1 root 当たり最大 10 範囲、1 範囲当たり最大 265,397 bytes を計測した。この上限から作った単一 root の搬送シナリオは、空の予約表、仮の検査・終端費用、回復 1 DAA の条件で、最遅 notice を含めて元の受付から 3,315 DAA を要した。全許容分岐の裁定、役割別義務、同時 claim の負荷を含む 10,000 DAA の受入判定は未完了。[全 root の走査](../evidence/oom-recovery-continuation-2026-11-03.md#measure-every-numerical-root-of-the-retained-smollm2-execution) [単一 root の搬送](../evidence/oom-recovery-continuation-2026-11-03.md#measure-the-legacy-frontier-and-the-numerical-delivery-reservation)

Batch 専用 context の候補コードは、同じ original が所有する数値 Open または Stop 用 FullIds の将来枠を確認し、旧共有経路の追加要求と資料受付を閉じる。notice の経路と対応する Open 枠の拘束、元の claim の本番受付からの context の生成、実ノードの一連の境界試験を有効化までに確認する。現時点では予約枠の分離試験まで実施し、notice から Open・Finish までを含む native fixture は未実行である。[Batch 専用 notice の実装記録](../evidence/oom-recovery-continuation-2026-11-03.md#separate-the-batch-initial-notice-from-legacy-delivery)

有効化には、実モデルの全許容分岐を元の claim から導出し、原資・処理枠・保持を受付時に予約できる native profile が必要となる。署名付き Open からの Panel 自動処理、正常側の全必要範囲と Gate C の認証、終端状態と会計の統合検証、実ノードの最大負荷検証、公開経路の完走も確認する。Batch carrier と手数料は共通の独立した `palw_batch_court_v1` fence に従い、配布設定では休眠とする。

## 2026-10-21 追加隔離checkpoint — original parameter footprint / archive S1

original全numerical leafのparameter関係を登録program/layout/headerから一度の解析で導出する部品と、native時限資格＋元署名/native v5によるarchive当該declaration名目責任の一回解消を追加した。84 unique selected pass、core全tests/非test consensus lib型検査exit0。全graph・役割契約・実UTXO原資・有料公開入口・実供給/default・全金融精算/pruning・実model door・T01–T15は引き続き未完である。追加部品や選択数をこの主契約全体の達成へ置換しない。[raw logと厳密な範囲](../evidence/palw/g14-completion-2026-10-21/s1-and-original-cover.md)。
