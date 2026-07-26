# ADR-0043 — G6 valid-sibling flood の bounded 化: consensus-validity sibling bound を採用する

- **Status:** Accepted(設計方針の確定。閾値の凍結は多機実測後 — §Consequences)
- **Date:** 2026-07-26
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

## Definition of done

- [ ] `MAX_DIRECT_CHILDREN_PER_PARENT` ヘッダ検証規則(v4 gated)+ RuleError + 単体テスト
- [ ] g6_measurement harness を「規則あり」で再実行し p99 書き込みが bounded であることを確認
- [ ] 多機実測(serial/concurrent flood、long-soak)→ 閾値凍結(外部)
- [ ] 独立レビュー(外部)
