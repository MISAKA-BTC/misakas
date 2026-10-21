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
| 係争の hard deadline | `a + 10,000` | `a`(申立てからではない。再申立てで reset しない) | 同上 |
| 各手番の応答期限 | 手番ごと。hard deadline とは別の制約 | 手番の開始 | 同上 D。値は未決 |
| door ≤ 3,200 | 対象 profile の**性能目標** | `a` | RFC-0024・0025(Draft)。主契約ではない。3,120 → 3,200 は明示承認の対象 |
| 混雑延長 `E_i` | **hard deadline の内側**での予約調整に限る:`a + 3,200 + E_i ≤ a + 10,000` | `a` | 本書の整理。RFC-0025 §4.3 の `E_max` は `≤ 6,800` に読み替える。それを超える延長は別の将来方針で、採用していない |

- 10,000 を維持するので、**10,000 の内側で処理できる負荷だけを original claim の受付で引き受ける**。受け付けた後に、正当な court だけを容量不足で捨てない。例:64 KiB × 10,000 件を court 用 25,000 B/DAA で運ぶと、転送だけで 26,215 DAA かかる(RFC-0025 の仮定による算術)。この負荷は 10,000 の内側に入らないので、受付の段階で抑制する。
- `10,000 − 3,120 = 6,880` は予算の差であり、最後に受理できる位置には等号・排他的境界の余裕を別に入れる。遅い申立て・毎回合法な最遅応答・最後の証拠・収録・回復の全部が残余に収まることを、機械的に検査する(T06)。
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
