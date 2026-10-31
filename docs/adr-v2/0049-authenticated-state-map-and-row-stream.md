# ADR-v2-0049 — 認証state map・IBD row stream・世代別保存

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0012](../rfc-v2/0012-authenticated-state-storage-and-ibd.md)

## 1. この文書の役割

行単位で認証する新state形式と、既存形式を有界に転送するIBD方式を同じ入口から区別する。

## 2. 統合した判断の読み方

### 2.1

認証state map案はtable trieとdirectoryを用い、存在・不在・範囲・更新をroot／親／branchへpinする。UnavailableをAbsentと同じものにしない。[旧ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md)

### 2.2

IBD row stream案は既存flat commitmentを保つtransportの変更であり、上の新root形式と同じ変更ではない。最終import gateと全root再計算・整合検査を省かない。[旧ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)

### 2.3

protocol versionの交渉、旧peerへのfallback、node-local世代保存・journal・VERIFIED・undoを分離する。[旧ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)

## 3. 維持する境界・後続改定

2案を一つにまとめたからといって新state rootへの移行が採択されたことにはならない。30 GB目標はこの統合で実測達成した値ではない。

## 4. 隣接する判断との分担

従来のroot／pruningは[ADR-v2-0012](0012-state-root-pruning-and-snapshots.md)、依存責任の復元対象は[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)、正式仕様は[RFC-v2-0012](../rfc-v2/0012-authenticated-state-storage-and-ibd.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0203-authenticated-palw-state-map-and-row-access.md](../adr/0203-authenticated-palw-state-map-and-row-access.md) | L1–L35 | L3–L3 |
| [0204-ibd-bounded-row-stream-and-storage-generations.md](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md) | L1–L47 | L3–L5 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0203-authenticated-palw-state-map-and-row-access"></a>

<details>
<summary>0203-authenticated-palw-state-map-and-row-access.md — 原文 L1–L35</summary>

[原文を開く](../adr/0203-authenticated-palw-state-map-and-row-access.md) · SHA-256: `c98c20f2d73905bfd5e24ab5c42e328c0aa177c1f45c1801de0441933db07f91`

<!-- ADR_SOURCE_BEGIN:0203-authenticated-palw-state-map-and-row-access.md -->
~~~~~~~~markdown
# ADR-0203 — 認証付き PALW state map と行単位アクセス

Status: **Proposed / Review Required** (2026-10-22)。[RFC-0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md) に具体化。30 GB pruned node 方針（ユーザー決定 2026-10-22）の state lane。純ライブラリと dormant fence の実装のみ。本番採択・pre 統合・武装・配備・state root 形式の変更は承認されていない。

## 問題

PALW の collection root は表の全行を逐次 hash する。行 1 つの存在・不在の最小証明が表全体になり、検証 node は全 state を保持するしかない。state root は毎 block 全表を再 hash する。pruned node が 30 GB 内で検証を続けるには、行単位で取得して検証できる commitment と、「取得できない」を「存在しない」と区別する読み取り層が要る。

## 決定（提案）

1. state の各表を **二分 Patricia Merkle trie**（prefix-free・順序保存の鍵符号、枝は絶対 prefix を commit、全 node hash に table id・schema version・上限を含む）で commit する。1 つの pruned-trie 証明で **存在・不在・範囲完全性・更新（旧値 → 新 root）** を示し、chunk 化で表全体の **認証 stream** にする。
2. 表 root を **directory**（同じ構造）にまとめ、`state_root = H(format ‖ parent_state_root ‖ branch_anchor ‖ directory_root)` とする。読み手は pin（root・親 root・branch anchor）で検証し、別分岐・別親・別表・別版の行を拒否する。
3. 証明の node 数・bytes・鍵長に上限を課し、hash 前に検査する。
4. consensus の行読みを **親 root に pin した `PalwRowSourceV1`** に寄せ、答えを `Present / Absent / Unavailable` の 3 種にする。書き込みは変更だけの change set。cache は (root, table, key) の検証済み答えだけを持ち、追い出しは canonical state に影響しない。
5. 新 root は新しい root 版であり、`Params::palw_state_row_map_v1` の後でのみ意味を持つ。現行の root・delta・wire・fork id は変えない。

## 選択肢

| 案 | 不在 | 範囲完全性 | 更新 | 深さの上限 | 判断 |
| --- | --- | --- | --- | --- | --- |
| A: 二分 Patricia Merkle trie（本提案） | ○ | ○（生鍵順） | O(深さ) | 鍵長で有界 | 推奨 |
| B: 疎 Merkle tree（鍵 hash 配置） | ○ | ×（hash 順） | O(256) | 固定 | 期限索引・prefix 範囲を示せない |
| C: 整列葉の平衡 Merkle tree | ○ | ○ | 挿入・削除で O(n) | log n | 更新が高価 |
| D: Merkle treap / B-tree | ○ | ○ | O(深さ) | 履歴・grind 依存 | 形が書き込み履歴に依存 |
| E: 現行（逐次 hash） | 表全体 | 表全体 | 表全体 | — | 30 GB 方針に合わない |

## 結果

- 今回: `palw_state_auth_map_v1`（map・証明・検証・更新・stream・state directory）と `palw_state_row_access_v1`（trait・overlay / change set・bounded cache・full-state adapter・proven backend・fence gate）を純ライブラリとして追加し、敵対例を含む試験を付けた。consensus 経路は未接続。fence は全 preset `None`、Some-only hash、武装は検証で拒否。
- 在庫と全表操作の分類、親 state の全量 copy の経路と change-set 化の段階計画は [state-inventory.md](../design/palw/storage/state-inventory.md)。
- 抽選（Panel draw）・集計規則など判定結果に触れる変換は本 ADR に含めない（RFC-0036 §6 の別起案）。

## 承認の範囲

本 ADR が求めるのは方式（A）の Review である。採択・state root の移行規則・wire・武装高・配備・全表操作の規則変更は含まない。未採択の間も、意味不変の段階（overlay による clone 削減、索引の増分化、tip の差分保持）は PRINCIPLES §11 の「意味不変の実装・最適化」として進められる。
~~~~~~~~
<!-- ADR_SOURCE_END:0203-authenticated-palw-state-map-and-row-access.md -->

</details>

<a id="source-0204-ibd-bounded-row-stream-and-storage-generations"></a>

<details>
<summary>0204-ibd-bounded-row-stream-and-storage-generations.md — 原文 L1–L47</summary>

[原文を開く](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md) · SHA-256: `ae580e9c62a410454c1e31980998c5bd53eabacdb64fd100d23a07f13a36fbb3`

<!-- ADR_SOURCE_BEGIN:0204-ibd-bounded-row-stream-and-storage-generations.md -->
~~~~~~~~markdown
# ADR-0204 — IBD の有界 authenticated row stream と世代番号付き保存

Status: **Proposed / Review Required** (2026-10-22)。[RFC-0037](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md)。
設計: [ibd-generations.md](../design/palw/storage/ibd-generations.md)。設計所有者の明示的な採択まで、新 P2P message の
登録・protocol version の割当・有効化・配備は行わない。

## 問題

pruning point の PALW state は全量受信・全量 decode の後でしか header の root と照合されず、一時領域とメモリが
state の大きさに比例する。G14 storage の上限(pruned node ≤ 30 GB、IBD spill 384 MiB × 2)の下では、受信を
有界にし、偽の state を途中で止め、検証済みのものだけを原子的に昇格させる必要がある。

## 決定(提案)

1. **consensus は変えない。** state root(ADR-0043 §2 の flat commitment)をそのまま使い、preimage を先に照合し、
   collection ごとに逐次 hash して preimage の該当 64 byte と照合する row stream を P2P 転送形式として追加する。
2. stream の照合は「早期中断と有界 disk」のためで、**最終判定は既存の import ゲート**(`restore_native_carriage_v1`
   + 全 root 再計算 + `assert_internal_consistency*`)のまま。root、または root に一致する stream だけでは検証済みに
   しない。
3. 新 message は新 protocol version の交渉下でのみ使い、旧 peer には従来の 106/107 carriage 転送を続ける。
4. node-local の世代保存(最大 2 世代、fsync + rename の journal、`VERIFIED` marker、undo log、切替をまたぐ巻き戻し)を
   受け皿とする。これは node-local で、本 ADR の採択を待たず既定 off / 既定不変で実装済み。

## 代替案

| 案 | 内容 | 不採用(または後回し)の理由 |
| --- | --- | --- |
| A(本提案) | flat root のまま preimage 先行・collection 単位検証 | consensus 不変。粒度は collection 単位(行単位ではない) |
| B | 木構造の state commitment で行単位の membership/non-membership proof | consensus 変更(新 state-root version)。state lane の RFC で扱う。採択されれば A の第 1 段を置き換えられる |
| C | 現行のまま(全量受信後に照合) | 上限内で state の成長に追従できない |

## 影響

- consensus fingerprint・header・fork choice・Panel 抽選: 変更なし。
- P2P: 新 message(休眠)。旧 peer の動作は変わらない。
- 帰責: preimage/collection の不一致は peer、容量・I/O は local(停止・再試行、block を無効にしない)。

## 承認の範囲

求めるのは A の方式の採択可否。protocol version 番号、message 名、chunk 大きさ、移行期間、phase 2(全体検査の
bounded-memory 化)の採択は含まない。

## 記録

- 2026-10-22: 受信側 verifier(`consensus/core/src/palw_row_stream_v1.rs`)と node-local 世代保存
  (`database/src/storage_generations/`)を実装・テスト。wire 未接続。番号: ADR-0204 / RFC-0037
  (ADR-0203 / RFC-0036 は同日の state lane(authenticated state)用に空けた)。
~~~~~~~~
<!-- ADR_SOURCE_END:0204-ibd-bounded-row-stream-and-storage-generations.md -->

</details>

