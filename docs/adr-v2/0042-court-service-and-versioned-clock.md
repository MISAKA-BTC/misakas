# ADR-v2-0042 — Court処理機会・共有訴追・versioned clock

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

Courtの処理機会、共有訴追、複数候補統合のclock・namespaceをまとめる。

## 2. 統合した判断の読み方

### 2.1

署名者の沈黙と配送・収録機会の欠如を区別し、空きblockやDAA進行だけから個人のdefaultを導かない。共有frontierと実供給の設計判断を保存する。[旧ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)

### 2.2

sequential witnessのaccepted cursor依存をfiling clockで扱う案は、単なるbyte capacityの増加と区別する。[旧ADR-0193（sequential-witness-filing-clock）](../adr/0193-sequential-witness-filing-clock.md)

### 2.3

隔離候補内のintegration／service schema検討は、公開wire採択・pre統合・有効化の承認ではない。[旧ADR-0194](../adr/0194-dormant-g14-integration-and-service-schema.md)

## 3. 維持する境界・後続改定

同じ旧番号0193でもMiner Account Bond案とは別文書。推奨案の詳細化承認と、本番規則の採択・実測値承認を分離する。

## 4. 隣接する判断との分担

response eligibilityは[ADR-v2-0043](0043-court-eligibility-and-initial-notice.md)、material入口は[ADR-v2-0045](0045-terminal-material-and-readiness.md)、金融と保持は[ADR-v2-0046](0046-court-close-and-liability-archive.md)・[ADR-v2-0047](0047-funded-original-and-role-authorization.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0192-court-service-opportunity-and-shared-pursuit.md](../adr/0192-court-service-opportunity-and-shared-pursuit.md) | L1–L53 | L3–L6 |
| [0193-sequential-witness-filing-clock.md](../adr/0193-sequential-witness-filing-clock.md) | L1–L23 | L3–L3 |
| [0194-dormant-g14-integration-and-service-schema.md](../adr/0194-dormant-g14-integration-and-service-schema.md) | L1–L66 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0192-court-service-opportunity-and-shared-pursuit"></a>

<details>
<summary>0192-court-service-opportunity-and-shared-pursuit.md — 原文 L1–L53</summary>

[原文を開く](../adr/0192-court-service-opportunity-and-shared-pursuit.md) · SHA-256: `a384287d1cf5704e14a4788bcc828c3e931ef71ba333ad5ecbeaefe69fe67ded`

<!-- ADR_SOURCE_BEGIN:0192-court-service-opportunity-and-shared-pursuit.md -->
~~~~~~~~markdown
# ADR-0192 — court の正規処理機会と共有訴追経路

- **Decision:** Proposed / Review Required (2026-10-20)。採択・統合・有効化を意味しない。
- **Scope:** [RFC-0026](../rfc/0026-court-policy-master-contract.md) §4・5、監査 F02・F03。
- **Source:** `7fe073ec2`。別 branch の scheduler・tag 245 はこの snapshot に未統合。
- **Supersession:** 採択前は既存の期限・署名・責任・wire を置換しない。採択後も旧 claim は旧 ruleset。

## 詳細仕様化の指示

2026-10-20、ユーザーは推奨 A/A の詳細仕様化を指示。[詳細契約案](../design/palw/integration/rfc26-service-shared-frontier-contract-2026-10-20.md)を追加した。数値・配送前提・wire・状態遷移の採択や武装承認とは区別する。

## 判断が必要な理由

チェーン上の応答不在だけでは、沈黙と個別配送妨害を区別できない。block の空き容量や進んだ DAA だけを「送信機会」とすると、同じ履歴の正直な署名者を処罰し得る。全 owner 枠の占有下で完成済み直接反証だけを受理しても、外部者が必要な開示に到達できることにはならない。fee や空き枠の予約だけでは両方を閉じない。

## F02 の選択肢

| 案 | 合意で記録するもの | 保証と限界 |
| --- | --- | --- |
| A: 予約サービス機会 + 明示的 inclusion 前提 | claim/争点/root/範囲/手番/責任鍵、必要 bytes/work、予約割当、予定 DAA と実供給、完了条件 | 各 accepting block の割当から機会を再計算。予約が提供され、かつ明示的な配送・収録前提の内側でのみ無応答 default。個別検閲をチェーンから識別できるとは主張しない |
| B: 無応答は非 credit の保留・復旧へ送る | 同じ義務と割当、未処理量、保持責任、復旧状態 | 計算不正の直接反証は裁定可能。無応答だけでは slash しない。永久沈黙時の担保保持と frontier 停滞が残り、無条件の有限返還は保証しない |

**提案:** A を仕様化する。ただし予約の供給を定義する受理規則と inclusion 前提はユーザー採択まで未決。正規機会なしは slash せず、hard deadline の内側で期限補正し Final を保留。hard deadline を越える時計 reset や自動無罪にしない。範囲外では未解決のまま担保・証拠を保持する。

A のレビューでは少なくとも、DAA ごとの複数 block / 無 block、通常 tx と court 予約の優先順位、署名検証前の無効入力予算、応答が入らなかった予約枠の供給認定、同一 block の最終応答と sweep の順序を固定する。ローカル回線・mempool・Panel の申告を入力にしない。供給ゼロでも絶対時計は進み、永久沈黙で時計を止めない。

## F03 の選択肢

| 案 | 経路 | 保証と未決 |
| --- | --- | --- |
| A: claim 共通の局所化 frontier | original acceptance で敵対的分岐を含む資源を予約。owner に依存しない争点/root/範囲/手番の要求を誰でも出し、指定 custodian の認証済み回答を共有する | owner の先着・上限 8 から独立。ただし許容分岐集合、重複統合、異なる合法 choice のスケジュール、最大 bytes/work と admission 検査が必要 |
| B: 有界な全開示を original acceptance で予約 | 外部者が必要な全公開資料を共有取得し、ローカルで局所化して直接反証 | owner 枠の争奪を除けるが全開示費用・保持・長 context の容量は未実証。上限引き上げや未知量のゼロ置換は不可 |

**提案:** A を tag 245 の共有開示を土台に仕様化する。claim ごとの最初の予約を共謀者が消費しても正直な争点が残ることを、争点 identity と許容する全分岐の容量から示す。「外部者 1 枠を残す」「reserve queue がある」だけで合格にしない。T05 は未完成証拠で入った外部者について、全 owner 占有・合法な最遅応答・埋め直し・同 root 重複を同時に試す。

## 現時点の実装境界

`palw_g14_gates_v1` に `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` の拒否を追加。通常 network では形式の正しい readiness record を渡しても未完経路を武装できない。既存の隔離 G14 test genesis は未完経路の試験を継続できる。preset・identity・wire・旧 claim の fold は変更しない。

`palw_court_policy_v1` は original accepting-chain DAA から 3,120 / 10,000、保持、依存 horizon、同じ bond の最大責任を checked arithmetic で導出する部品。state/fold 未接続であり、期限の原子的な状態更新や T06/T09 の完了とは表示しない。notice 等号・turn duration は選んでいない。

## 採択後の受入条件

T02–T05、T06–T09、T13–T15 を同じ release binary で実施。予約・effective terms・保留を reorg/restart/pruned IBD から同じ状態に戻す。t12 同一性試験を通す。測定値、derived_upper、未被覆を分離する。tag 230 の codec、応答 fee、turn duration、SC64 profile はこの ADR で決めない。

## 追補(2026-10-26、lane F03b2): F03 A の未決 2 点の暫定決定

**暫定。ユーザーが変更し得る。** 2026-10-22 の値決定委任(目安可、根拠と見直し条件を台帳に)により統括が決定。

1. **サイズでは拒否しない(目安、改訂 2):** 最大 B bytes の座標は `⌈B/34,000⌉` packet(V1)として、original acceptance で予約した容量から差し引く。work の目安は class の op_work(V11 `[5,13,1,11]`)として台帳に記録するが、op ごとの実測がないため単独では執行しない。M-C 実測で置き換える。
2. **容量:** original acceptance で `Q = Σ_i ⌈maxB_i/34,000⌉`(全分岐の全座標)を計算し、`Q ≤ c·W` を満たさなければ original の受付を拒否する(状態変更なし)。原因は生産側の誤りであり、conviction 時には拒否しない。
3. **重なる合法 choice:** 同じ座標は統合する。異なる choice は応答期限の早い順、同順位は座標 id の昇順で処理する。shared request の期限はすべて h なので、fold 内では到着順(block 内では object 順)になる。fold は request 行(version 2)に `SharedServiceSlotV1 {packets, start, end}` を記録し、claim の前回 slot の直後から連続して割り当てる。h より前に終わらない slot は「予約容量なし」として拒否する。間断なく処理した場合の k 番目の終了 DAA は `a + ⌈Σ_{j≤k} p_j / c⌉ − 1 ≤ a + ⌈Q/c⌉ − 1 < h` で、各座標の行は 1 つだけなので、共謀者の先取りや埋め直しで job は増えない。
4. 実装: `palw_court_shared_frontier_budget_v1.rs`、`palw_court_shared_tir_state_v1.rs`(v2 行と packet 計算)、`palw_court_shared_tir_fold_v1.rs`(slot の記録)。version 1 の行は byte 単位で不変で、version 2 は末尾に slot を付加する。
~~~~~~~~
<!-- ADR_SOURCE_END:0192-court-service-opportunity-and-shared-pursuit.md -->

</details>

<a id="source-0193-sequential-witness-filing-clock"></a>

<details>
<summary>0193-sequential-witness-filing-clock.md — 原文 L1–L23</summary>

[原文を開く](../adr/0193-sequential-witness-filing-clock.md) · SHA-256: `8f2f2503eb9b1145045dfd35a97cbff11ab273b272151fbf748e063249ccf72e`

<!-- ADR_SOURCE_BEGIN:0193-sequential-witness-filing-clock.md -->
~~~~~~~~markdown
# ADR-0193: sequential witness filing の versioned clock

Status: **Proposed / Review Required** (2026-10-20)。採択・統合・武装・配備ではない。RFC-0026 の補助案。正本は source snapshot `7fe073ec2:docs/rfc/0026-court-policy-master-contract.md` にあり、この古い X3 branch へ RFC 一式を統合したものではない。

## 問題と根拠

`kaspad/src/palw_tir_public_filer_scheduler.rs` は staged witness の各 part を、現在の accepted `TirWitness.received.len()` の offset から作る。offset が受理で進むまで次の part は送らない。現行 clock version 1 の `tir_public_filing_daa_v1` は固定 2 inclusion margin と全 move の byte inclusion だけを加える。高い byte capacity はこの accepted-cursor の直列依存を解消しない。これはノードの current pipeline に対して filing allowance 単独では直列待ちを計上しないという問題であり、実測されたネットワーク遅延の報告ではない。実 plan は `max(collection_daa, filing_daa)` を用いるため、十分に長い collection window が追加項を覆う場合もある。全ての既存 plan の期限が不足すると主張するものではない。

## 選択肢

A（現時点の推奨）: **version 2 を明示して既存の直列 pipeline を覆う。** `terminal.envelope.maximum_moves` を保守的な carried-move 上限とし、`max(2, maximum_moves) × inclusion_margin` と既存の全 byte inclusion を加える。旧 version 1 の数式、serialization、plan 再導出は保持する。新 clock は既存の実 plan 導出が呼ぶため、collect/material/bond の各値と resource identity に反映する。既存 maximum door を越えた plan は拒否し、上限を広げない。

B: filer と mempool/native preflight を実際に package/prefetch 対応させ、全 part を各受理待ちなしで送り、canonical な順序で受理する。そこで初めてその pipeline の測定と故障条件に対応した別 profile を検証する。現行コードのまま、将来の batch を clock の前提に置かない。

## 候補実装と採択条件

A の候補は `palw_tir_frame_cost_v1` に実装。struct の serialized field を増減せず、明示した `clock.version = 2` で dispatch する。version 1 の既存 ID を新意味へ読み替えない。全 preset と pinned frame-cost records は変更しない。record list は空のまま、本番 profile の承認はない。version 1 の記録を version 2 へ使い回すことも一致検査で拒否する。

この候補の synthetic fixture を、network inputs・CPU/memory・inclusion capacity の実測や actual door ≤ 3,120 の証拠にしない。新 profile の actual carrier bytes、body parts、slowest supported node、入出力・保持・capital の各費用、署名済み依存 horizon、3,120 / 10,000 の規則、fair inclusion と供給会計、全 owner 共謀時の経路がそろうまで武装を拒否する。clock V2 だけで G14 完成とはしない。

## 範囲外・残件

owner queue の再入場/tenure、全許容分岐、局所化の起点と cone、grant-ready など各 carrier の実サイズ、全体の予約/通常 tx budget 分離、response deadline の補正、default attribution、終了理由ごとの保持/解除、restart/pruned IBD はこの修正で閉じない。実 pipeline を batch 化する最適化も残る。計算の正しさの暗号学的証明やモデル単位の審査は追加しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0193-sequential-witness-filing-clock.md -->

</details>

<a id="source-0194-dormant-g14-integration-and-service-schema"></a>

<details>
<summary>0194-dormant-g14-integration-and-service-schema.md — 原文 L1–L66</summary>

[原文を開く](../adr/0194-dormant-g14-integration-and-service-schema.md) · SHA-256: `a0f8dde80cf72fefd73fd1303cde130176222f3e96aa9d1f6f199ba74d81319d`

<!-- ADR_SOURCE_BEGIN:0194-dormant-g14-integration-and-service-schema.md -->
~~~~~~~~markdown
# ADR-0194 — 休眠 G14 統合候補と service namespace

Status: **Proposed / Review Required** (2026-10-20)。利用者は A 案の隔離候補内での実装・検証を承認済み。公開 schema 採択・pre 統合・武装・配備は未承認。

## 判断の対象

独立した候補を同じ native build で接続するため、**新しい隔離 branch の中だけで**統合と検証を進める案。`pre`、既存 source branch、公開 network、稼働中 t12 は変更しない。未知の費用・帯域・inclusion 前提や 3,120 / 10,000 を採択済みとして補わない。

- main 候補: `codex/rfc14-g14-completion` / `56f7c95a9`。service policy/book/rooted hold・recovery/custody、共有 query 集合、native/legacy node 試験、正本 RFC と利用者追加の Account Bond 文書。
- X3 候補: `codex/rfc14-x3-reorg-clock` / `0c3cb9f93`。新しい batched/de-stacked cost/clock、owner/terminal/carrier 処理、instance-local reorg 試験時計、休眠 transport 会計、Proposed filing clock V2。
- 共通祖先: `f99e86f83ec069bdd3162208226b513522d693d2`。祖先からの差分は main 165 files、X3 381 files（共有 query commit 前の観測、ログを含む）。単純な「数ファイルの merge」として扱わない。

## 実際の衝突

両 head の `consensus/core/src/palw_staged_court_state_v1.rs` を比較すると、末尾追加の Borsh enum が同じ ordinal を別用途に使う。

| Namespace | main 未採択候補 | X3 |
| --- | --- | --- |
| key tag 11 | `CourtService`（unit key） | `TirContainer(Hash64)` |
| row tag 17 | `CourtService` | `TirContainer` |

main の service format は休眠候補で production writer がない。これを理由に衝突を黙殺したり、X3 の既存 tag を service の意味へ読み替えたりしない。直接 concatenation、既存 tag の reorder、shared scanner/heap schema の片側だけの変更は禁止する。

## 選択肢

### A — 推奨: X3 namespace を保ち、service を別 version として末尾追加

新しい `codex/rfc14-g14-integration-candidate` を X3 head から作り、main の service/query 変更を選別して移植する。X3 の既存 key 0..11 / row 0..17 は保持。service の新しい候補を未使用の key 12 / row 18 として末尾に追加し、service payload version 2 と独立した profile identity にする。これは新候補内での実装・試験案で、公開 schema 採択ではない。

旧 main の未採択 service v1 snapshot を、そのまま X3/新候補の snapshot として読み込む経路を作らない。source format/chain identity を区別し、旧候補を自動 import しない。必要なら後で別仕様の明示的 offline converter を設計・承認する。新 service reader は自分の version を厳密に検査する。X3 の既存 snapshot/row/tag と production chain の互換性、source format の混同拒否を試験する。

Borsh、typed heap、allocation-free scanner、state/context/journal/carriage、deadline/hold、native writer を一緒に更新する。profile/rules/accepting context を実 state から接続し、旧 claim の処理を変えない。main 側の v1 候補は元 branch/history に残し、同じ番号を新意味へ静かに再利用しない。

### B — 別の service table/namespace へ分離

既存 staged enum を増やさず、独立した versioned service table と key domain を導入する。別 table の root、codec、journal、heap/scanner、IBD/保持を全て検証する必要がある。未使用 table ID の調査・予約と追加費用が必要で、今ここで仮 ID を置かない。

### C — 分離を維持する

統合せず、各 branch の調査・部品検証を続ける。旧 main の legacy clock の不成立を X3 全体の不可能性として扱わない。分離した green tests を同じ build の G14 完成へ合算しない。

## 隔離候補へ持ち込む範囲

A の場合も branch 全体を無条件 merge しない。main の service policy/book/context guards、hold/recovery、query universe と対応テストを X3 native API に接続する。X3 の source/cost/clock、carrier cost、signed custody、native accepting order と照合し、main の古い clock/owned 実装で上書きしない。

正本 RFC-0024/0025/0026 と Account Bond RFC-0027/ADR 文書は、元資材の bytes/digest を保って取り込む。shared index/number registry は再生成・調停する。別内容の ADR-0193（Account Bond と filing clock）は全文パスで識別し、既存主文書を勝手に改名しない。Account Bond の新規則実装・料金採択をこの統合に混ぜない。

新 branch 内でも real-network fence は休眠、record list は未承認のまま。新 profile の費用・物理供給・全合法対話 graph・hard/依存 horizon が不成立なら受付/武装を拒否し、上限・モデル範囲・identity pin を変えて成功扱いにしない。tag 230 の A/B 判断はこの ADR で代行しない。

## 統合候補の受入条件

1. X3 の既存 serialized tags、t12 identity、params/schedule IDs の保存を同じ release build で確認する。期待値を repin しない。
2. 新 version の service codec/heap/scanner/context/journal/carriage を一致させる。旧未採択 service v1 と X3 container の意味を混同しない。
3. 同じ build で native default 拒否、Final/担保/証拠 hold、回復、正当な fraud 裁定、signed custody、共有 query、実署名 relay と funded node relay/reorg を再検証する。
4. query から署名者/turn/fragment/実資源の完全な義務を導出し、original admission、block supply、owner 非依存の public ingress へ接続する。部品だけの成功で T05 としない。
5. source 別の実測・synthetic fixture・derived bound・未被覆を分けて記録する。T01–T15、R-D2-NODE、tag 230、実モデル door ≤ 3,120 の残件を省略しない。

## 承認が必要な根拠

[引き継ぎ §6](../design/palw/integration/codex-handover-court-rfc14-2026-10-20.md) は「fence の武装、統合、配備は行わず、ユーザーの承認を待つこと。」と指示している。ADR-0192 A/A の詳細仕様化の指示は統合承認ではない。この文書と source 比較までは実施し、branch を混ぜる操作はまだ行っていない。

ここで求める判断は **A を隔離候補 branch 内で実装・検証してよいか**。承認されても pre への merge、公開 schema/費用/inclusion 前提の採択、武装、配備の承認へ広げない。

## 利用者の判断 — 2026-10-20

利用者回答「A：隔離候補で実装・検証（推奨）」により A の作業を開始する。X3 の既存 tag を保持し、service は別 version / namespace とする。承認範囲は新しい専用 branch の実装・検証であり、pre 統合・武装・配備・本番規則の採択を含まない。上記の未実施・承認要求の記述は判断前の経緯である。
~~~~~~~~
<!-- ADR_SOURCE_END:0194-dormant-g14-integration-and-service-schema.md -->

</details>

