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
