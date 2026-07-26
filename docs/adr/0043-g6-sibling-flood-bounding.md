# ADR-0043 — G6 valid-sibling flood の bounded 化: consensus-validity sibling bound を採用する

- **Status:** Amended 2026-07-26 — **当初の (B) sibling-count validity rule は soundness 検証で棄却**し、
  (A) per-header reindex-cost bound を正とする(§Amendment を参照)。閾値凍結は多機実測後
- **Date:** 2026-07-26 (amended same day)
- **Supersedes / amends:** `docs/palw-public-value-header-v4-antispam.md` §G6(measurement-only)を
  設計決定で補完。ADR-0040 §remediation の G6 記述を実装方針に落とす
- **Consumes:** ADR-0041(public/value mainnet の StopShip 解除条件)

## Context

G6 は「**valid** な sibling ヘッダの洪水」で、コストの本体は stamp(v4 anti-spam が課す料金)では
払われない **reachability reindex** にある: 親の trailing `u64` interval が枯渇すると
`add_tree_block → reindex_intervals` が走り、`propagate_interval` が既存の全子 interval を書き換え、
`split_exponential` が子容量を使い切るため **次の sibling が再び reindex を誘発**する。
per-header 書き込み量は O(reindex される reachability 部分木) であり定数でない
(M1-Max 実測: p99 ≈ 1,037 batch ops、うち ~983 が reachability —
`docs/palw-public-value-header-v4-antispam.md:229-240`)。

`PalwSpamParams` の 8-band stamp ramp は「同一 selected past を共有する任意個の sibling」にも
base_stamp_bits を課すが(`palw_antispam.rs:135`)、stamp は**支払い**であって**上限**ではない。
攻撃者が stamp を払い続ける限り reindex コストは他の全ノードに externalize される。
sampled-window ratio 方式は gameable として既に**棄却済み**(ADR-0040:1325-1329)。

選択肢は台帳(`adr-palw-public-value-activation-readiness.md:30`)の言う通り二つ:
(A) bounded-reachability/allocation の再設計、(B) consensus-validity な sibling bound。

## Decision

**(B) consensus-validity sibling bound を採用する。**(A) は棄却する。

1. **規則(要旨):** 1 つの親ブロック P に対し、P を direct parent に含む**受理可能な**ヘッダ数を
   consensus 規則で `MAX_DIRECT_CHILDREN_PER_PARENT` に上限する。上限超過の子ヘッダは
   **ヘッダ検証で拒否**(`RuleError`)— mergeset/blue には一切入らない。
   正直なマイナーは同親に多数の子を作る理由がない(テンプレートは新 tip に移る)ため、
   正常運転への影響は無い。境界値は多機実測(§Consequences)で凍結するが、
   設計上の初期候補は **64**(= `increase_max_block_parents(64)` と同桁の余裕)。
2. **なぜ (A) でないか:** reachability の interval allocation を「有界書き込み」に再設計するのは
   コア到達可能性データ構造の全面改修であり、探索空間が広くレビュー面積が大きい。
   さらに (A) 単独では攻撃者は依然として O(bound 内) の reindex を**無数の親**に対して
   繰り返せる(コストが分散するだけ)。(B) は攻撃面そのもの(同親 sibling の無限供給)を
   consensus で閉じ、(A) の恩恵(reindex の稀少化)を副次的に得る。
3. **stamp ramp は併存させる。** (B) は「上限」、stamp は「上限内の料金」。両者は直交する。
4. **検証コストの位置:** 親ごとの子カウントはヘッダ受理時に children 集合
   (relations store が既に保持)から O(1)/O(log) で判定できる。新しいストアは要らない。

## Consequences

- **ヘッダ有効性規則の追加 = ハードフォーク境界。** ADR-0041 の Header-v4 re-genesis に同乗させる
  (v4 genesis から規則が効く)。既存 v3 閉域ネットには適用しない(regenesis しない chain の
  歴史に遡及しない)。
- **G6 の gate 遷移:** `VerifierExists`(measurement-only)→ 規則実装+単体/e2e で `Bounded`、
  その後 **多機 serial/concurrent flood + long-soak 再実測**で閾値
  (`MAX_DIRECT_CHILDREN_PER_PARENT` と stamp ramp)を凍結して `Closed`。
  再実測は外部(複数実機)であり in-session では閉じない。
- 正直な限界: (B) は「1 親あたり」の上限であり、攻撃者が**多数の異なる親**へ 1 子ずつ
  ばら撒く形の flood は stamp ramp + 通常の PoW/難易度が既存どおり律速する
  (これは G6 の定義外で、既存の hash-lane 経済で bounded)。
- fork-choice・GHOSTDAG の性質(k-cluster 等)は不変 — 上限は受理前のヘッダ検証で、
  受理後の色付け規則には触れない。

## Amendment (2026-07-27) — (B) は unsound、(A) per-header reindex-cost bound を正とする

### なぜ (B) sibling-count validity rule を棄却したか(soundness)

ヘッダ有効性規則は「**ヘッダ自身とその committed past cone の決定的関数**」でなければならない。
`MAX_DIRECT_CHILDREN_PER_PARENT` はこれを満たさない — 「親 P が既に k 個の受理済み direct child を
持つ」は P の **children 集合 = H の anticone** の性質であり、H の past cone のどこにも commit
されていない。帰結は三つ、いずれも致命的:

1. **到着順分岐 → 恒久 split。** ノード X が c₁..c₆₄ を先に見て c₆₅ を拒否し、ノード Y が c₆₅ を
   先に見て別の子を拒否すると、両者は同一ヘッダの有効性で恒久に不一致になる(ヘッダ有効性は
   `StatusInvalid` としてキャッシュされ再評価されない)。c₆₅ の上に積まれた全ブロックが片側でのみ
   valid になり、これは fork ではなく **view split**(合意の定義不能)である。
2. **hash 順で決定化しても単調性が壊れる。**「hash 最小の 64 個が勝つ」等の順序無依存化は、後着の
   low-hash sibling が**既に valid とされたヘッダを遡及的に invalid 化**することを要求する。
   pipeline は validity の取り消しを表現できない(reachability/GHOSTDAG データは受理時に確定済み)し、
   仮に表現できても low-hash 子を withhold して後出しする grind 攻撃に、正直ノードの中間ブロックを
   まとめて無効化する新しい攻撃面を渡すだけである。
3. **IBD で再導出不能。** 歴史同期ノードは 64 個超の children 集合を一括で観測する。どの部分集合が
   「当時受理可能だったか」は元の到着順の関数だが、到着順は chain に記録されない。

(ADR-0040:1325-1329 が sampled-window ratio を棄却した理由と同族 — 観測値が到着/配置順に依存する
規則は consensus に置けない。)

### (A) の実体 — consensus 規則ではなく allocation policy

コスト増幅の機構は §Context の通り: (i) `split_exponential` が re-tile 時に親の子容量を**使い切り**
trailing 余白がゼロになるため、次の sibling が**即座に**次の reindex を誘発する; (ii) 通常挿入は
`remaining.split_half()` で余白を半減させるため、余白は log₂ 回しか挿入を吸収しない。

reachability interval の layout は**ノードローカルで非合意対象**(header に commit されず、既に
到着順依存)であるため、割当 policy の変更は **hard fork ではなく、v4 gate も不要で、全ネットに
適用できる**。修正は二点:

1. **re-tile 時の trailing reserve。** `propagate_interval`/re-tile の子割当を
   「Σsubtree + surplus/2」までに留め、surplus の半分を trailing 余白として残す
   (各子は自 subtree サイズ以上を保証されたまま)。現行は surplus/2 = 0 なので次 sibling が
   即 reindex — reserve があれば次の re-tile までの挿入回数が挿入列に対して超幾何に伸びる。
2. **flood-regime の挿入割当。** 親の children 数 n が `SIBLING_FLOOD_ALLOC_THRESHOLD`(64 —
   正直な並行幅の数倍)を超えたら、新規子の割当を `remaining/2` から `remaining/(2n)`(最小 1)に
   切り替える。調和級数的消費により、reserve R は ~log(R) 回ではなく実質無制限
   (R·√(n/k) < 2k まで、R≈2^60 で天文学的)の挿入を吸収する。正直なトポロジ(n ≤ 数十)は
   従来の split_half のまま。

### 保証(honest)

- happy path の per-header reachability 書き込み = O(1) 行(現行と同じ)。
- 同一親 sibling flood に対し: 最初の re-tile 後、reserve+調和割当により追加 re-tile は実質発生せず、
  **ネットワークの総書き込みは受理ヘッダ数に線形**(amortized O(1)/header)。攻撃者の stamp 支払いと
  防御側コストが定数比になり、G6 の「支払いは有限・externalized コストは超線形」という非対称が消える。
- 残る単発最悪ケース: 深い subtree flood が祖先 re-tile を誘発した場合の O(reindex される subtree)
  1 回分 — 発生間隔が幾何級数以上に伸びるため amortize され、dynamic-array と同型の契約。
  これは (B) でも消えなかった(bound 内 reindex は許容されていた)。
- fork-choice/GHOSTDAG/ヘッダ有効性は**一切不変**。stamp ramp は従来どおり併存(§Decision 3)。

## Definition of done(amended)

- [x] 割当 policy 実装: re-tile trailing reserve + flood-regime 挿入割当(`interval.rs`/`reindex.rs`/
  `tree.rs`)+ reachability 不変量の単体テスト(既存 property tests green、15/15)
- [x] g6_measurement harness を「bounded allocator あり」で再実行し、1,000-sibling flood の
  per-header 書き込みが O(1)(p99 ≈ 定数)であることを確認 — 2026-07-27 M1 Max: total ops p99
  1,037 → **16**、reachability ops p99 1,023 → **2**、data writes p99 → **1**(max 79/65/64 は
  64-閾値交差時の単発 re-tile)。gate: Measurement → **Bounded**
- [ ] 多機実測(serial/concurrent flood、long-soak)→ 閾値凍結(外部)
- [ ] 独立レビュー(外部)
