# RFC-0036 — 認証付き PALW state map と行単位アクセス

Status: **Proposed / Review Required** (2026-10-22)。[ADR-0203](../adr/0203-authenticated-palw-state-map-and-row-access.md)。30 GB pruned node 方針（ユーザー決定 2026-10-22）の state lane が起案した。純ライブラリと dormant fence の実装だけを伴う。本番採択・pre 統合・fence の武装・配備・state root 形式の変更は承認されていない。どの network の state root・判定も変えない。

## 1. 問題

PALW の各 collection は state root に「全行を key 順に連結した 1 本の逐次 hash」（`palw_collection_root_of_entries_v1`）として入る。1 行の存在・不在を示す最小の証明は collection 全体である（[palw_state_proof_v1.rs](../../consensus/core/src/palw_state_proof_v1.rs)、O(行数)）。したがって検証する node は全 PALW state を手元に持つ以外に方法がなく、30 GB の上限を「現在必要な state を全量保持」で超えうる。加えて state root は毎 block 全表を再 hash する（[在庫 §4 #42](../design/palw/storage/state-inventory.md)）。

必要なもの:

1. 行の **存在**（値つき）・**不在**・**範囲の完全性**・**更新（旧値 → 新 root）** を、行数に対して対数的な大きさで示す commitment。
2. table・schema version・親 state root・branch で domain 分離され、他の表・版・親・分岐の行が通らないこと。
3. 証明の大きさと検証の仕事が、受け取る前に上限で抑えられること。
4. 「取得できない」が「存在しない」に化けないこと。供給者の不在・ローカル取得失敗は sync の停止であって block の無効化ではないこと。
5. cache の追い出し・pruning・再起動が永続登録や未解決責任を失わせないこと。

暗号はハッシュ（keyed BLAKE2b）と Merkle commitment だけを使う。これは取得したデータの同一性の認証であり、計算の正しさの証明ではない（[PRINCIPLES §8](../PRINCIPLES.md)）。

## 2. 提案 — 二分 Patricia Merkle trie と state directory

### 2.1 鍵の符号化

生の鍵 bytes を 1 byte につき `1` + 8 bit、末尾に `0` の bit 列にする。**prefix-free**（ある鍵の符号が他の鍵の符号の真の接頭辞にならない）かつ **順序保存**（符号の bit 辞書順 = 生 bytes の辞書順、接頭辞が先）。表の型付き鍵は順序保存の bytes に写す（hash 鍵はそのまま 64 B、bond outpoint は txid 64 B ‖ index big-endian 4 B、kernel aux は table byte ‖ key）。

### 2.2 表（table）

- 葉: `H_leaf(spec ‖ len(key) ‖ key ‖ len(value) ‖ value)`。
- 枝: `H_branch(spec ‖ prefix_len ‖ prefix(packed, 0 詰め) ‖ left ‖ right)`。枝は配下の鍵が共有する**絶対** prefix を commit し、prefix の次の bit で 0 側・1 側に分ける。形は鍵集合だけで決まる（書き込み順に依らない）。
- 空: `H_empty(spec)`。表 root: `H_table(spec ‖ count ‖ trie_root)`。
- `spec = (table_id, schema_version, max_key_len, max_value_len)` を全 node hash に入れる。domain は node 種別ごとの keyed BLAKE2b 鍵。

### 2.3 証明

1 つの形（pruned trie を preorder で並べる: `Opaque(hash)` / `Branch(prefix)` / `Leaf(key, value)` と行数）ですべてを表す。検証者は範囲の集合を与え、次を確かめる:

- 開かれた node は親が固定した prefix（親 prefix ‖ 方向 bit）の下にある。
- **完全性**: 範囲の鍵を含みうる部分木（`subtree_may_intersect_v1`、証明者と検証者が同じ関数を使う）は `Opaque` であってはならない。
- 再計算した表 root が commit 値と一致する。

成功すれば、範囲内の任意の鍵について「値つきで存在」または「不在」が**事実**として答えられる。範囲外の鍵は `NotCovered`（未証明）であり、不在ではない。

- 存在 / 不在 = 1 点範囲。大きさ ≤ `2 · (9 · max_key_len + 1) + 1` node（hash 鍵 n 行で ≈ `2 log2 n`）。
- 範囲完全性 = 範囲の境界 2 本の路と範囲内の葉。
- **更新**: 書く鍵すべての 1 点範囲を旧 root に対して検証し、各書き込みの旧値が証明値と一致することを確かめ、pruned trie 上で挿入・削除・上書きを行って新 root を計算する（完全性により必要な node はすべて開いている。削除は兄弟の部分木を絶対 prefix のまま持ち上げる）。
- **stream**: 表全体を `[位置, end)` の chunk（各 ≤ `max_rows` 行、最後以外は ≥ 1 行）で順に読み、最後に読んだ行数が commit された count と一致することを確かめる。

### 2.4 state level

`state_root = H_state(format_version ‖ parent_state_root ‖ branch_anchor ‖ directory_root)`。directory は同じ構造の表（`table_id = u32::MAX`）で、鍵 `table_id ‖ schema_version`（big-endian）→ 値 表 root。行の証明は header・directory の 1 点証明・表の証明の 3 つ。読み手は pin（state root と、それが chain すべき親 root・branch anchor）を持ち、別分岐・別親・別形式・別表・別版の証明を拒否する。

change set は親 header・触る表の directory 証明・表ごとの書き込みと更新証明・子の branch anchor であり、検証者は親 pin だけから子 root を計算する（表は初回の書き込みで作られ、空になっても directory から消さない）。

### 2.5 上限

表は key / value の上限を宣言し（hash に入る。ハード上限 512 B / 1 MiB）、検証者は証明の node 数と bytes を hash の前に検査する。1 点証明・change set の上限は `AuthProofLimitsV1::for_points` で導ける。

## 3. 行アクセス層（dormant）

- `PalwRowSourceV1`: 親 root に pin した `read_row` / `read_range`。答えは `Present` / `Absent` / `Unavailable(理由)` の 3 種で、型付き読みは unavailable を `Err` にし、決して `Ok(None)` にしない。
- `PalwRowOverlayV1` / `PalwRowChangeSetV1`: 書き込みは変更だけを持ち、旧値は pin 下の読みで固定する（読めない行への書き込みは拒否）。change set は表ごとの `RowUpdateV1` になり、そのまま 2.3 の更新証明に渡る。`inverse()` が reorg の巻き戻し。
- `PalwCachedRowSourceV1`: (root, table, key) をキーとする LRU（byte 予算）。検証済みの存在 / 不在だけを持ち、unavailable は持たない。追い出しは次の答えの取得元を変えるだけで答えは変えない。
- `PalwFullStateRowSourceV1`: 現在の全量 in-memory state への adapter。既存 fixture 8 種で行・probe・範囲・型付き読みが state の getter と同一（試験）。
- `PalwProvenRowSourceV1`: 供給者から行と証明を取り、pin で検証する pruned backend。沈黙 → `SupplierUnavailable`、虚偽 → `ProofRejected`、directory が表の不在を証明 → 全鍵 `Absent`（証明済み）。

## 4. 採らない案

- **疎 Merkle tree（鍵の hash で配置）**: 不在・更新は示せるが、生の鍵順の範囲完全性（期限索引・prefix 範囲）を示せない。
- **整列した葉の静的平衡 Merkle tree**: 挿入・削除で葉の位置がずれ、更新が O(n) 再 hash になる。
- **Merkle treap / B-tree**: 形が書き込み履歴や優先度に依存するか、攻撃者が鍵を grind して深さを伸ばせる。Patricia trie の深さは鍵長で上から抑えられる。
- **現 root に行 index を足す**: 逐次 hash は 1 行の証明を表全体より小さくできない。
- 計算の正しさの暗号学的証明は対象外（PRINCIPLES §8）。

## 5. 互換と有効化

- 新しい state root は**新 root 版**であり、`Params::palw_state_row_map_v1`（全 preset `None`、Some-only hash、武装は検証で拒否）の後でだけ意味を持つ。現在の root・delta・wire・fork id はバイト同一。
- 有効化には: 本 RFC と ADR の採択、旧 root からの移行規則（fence 高での一括構築と、その root を header が commit する方法）、row 証明の wire（IBD lane の stream / generation 形式と整合）、全表操作の変換（§6）、node 側 fetch / cache / 予算の統合、fence を跨ぐ drill が必要。
- 移行中も旧 root の検証は変えない。新 root を shadow で計算・比較する段階を挟める（判定に使わない）。

## 6. 全表操作 — consensus の意味に触れるもの（別途 RFC が要る）

[在庫 §4](../design/palw/storage/state-inventory.md#4-全表操作per-block--per-decisionと分類) の 46 件のうち、R（行 lookup）と I（索引による完全範囲）は rooted 索引を足して同じ結果を返せば意味不変。次は判定や上限の意味に触れるので、本 RFC では扱わず個別に起案する:

1. **Panel draw の適格集合**（`palw_panel_v2.rs` の `palw_panel_bonds_judging_v1`、census、`draw_seats`）: 取得した勝者 1 行は「より良い候補が隠されていない」を示さない。現行規則のまま pruned 検証するには適格 bond 表の完全 stream（S）が要り、per-block の取得量は bond 数に比例する。上限を持たせるには累積 stake の認証木を O(log n) で降下する抽選への変更が要り、**抽選結果が変わる**。
2. kernel route ledger の全行 flush / 全 bond 再同期（#6〜8）、RFC-0016 の容量集計（#12）、network room / audit backlog / court pool の集計（#31, #35, #37）、reporter commitment の最大値（#24）: 増分カウンタ・最大値を rooted にする変更。結果同一性の証明と試験が条件。
3. 派生索引の全件再構築（#40）と state root の全量再 hash（#42）: 本 RFC の新 root 版と索引の rooted 化で解消する。
4. fork choice の bonds 走査（#43, #45）: 表 root を registry root として使う同値性。

## 7. 試験（実装済み、dormant）

`consensus/core/src/palw_state_auth_map_v1/tests.rs`、`palw_state_row_access_v1/tests.rs`、`palw_state_v2/tests/row_access_v1.rs`。符号の順序保存・prefix-free（全対）、完全性判定の健全性（小宇宙で網羅）、書き込み順非依存の root、BTreeMap との一致、存在・不在・範囲・prefix 範囲、証明の大きさ上限、更新の新 root 一致、stream、state change set の子 root 一致。**拒否されなければならない敵対例**: 行の欠落（葉を hash に畳む・削除する・枝ごと畳む）、偽の不在（隣の証明・鍵の改名・行を除いた map の証明・値の改竄・空証明）、範囲の省略（短い範囲の証明を長い範囲に、2 範囲中 1 つだけ）、配置の偽装（子の入れ替え・末尾の余分・切り詰め・汚れた padding・深すぎる prefix）、上限超過（node 数・bytes・鍵長）、別表・別版・別上限、別分岐・別親・親の行・別形式・directory にない版、古い旧値・旧値の偽装・未証明の書き込み・二重書き込み・別 root、stream の chunk 飛ばし・過大主張・空 chunk・行数超過。
