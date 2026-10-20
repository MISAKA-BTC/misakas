# RFC-0026 F03 — 一経路と全共有分岐の byte 数を区別する

**状態:** 調査・条件付き導出。新 profile の採択・本番性能の測定・G14 の完成を意味しない。[A/A 詳細契約案](rfc26-service-shared-frontier-contract-2026-10-20.md) §5 の許容集合を閉じる際の確認事項。

## 対象と前提

対象は現行 `PalwTirPublicResponseV1::StepNode` の Borsh codec と、現行の root からの canonical な段階下降。`N` は original header の `step_leaf_count` であり、**context 長・token 数・パラメーター数ではない**。2M context の `N` を 2M としていない。

以下の下限が適用されるのは、全 canonical branch の末端 frontier が合法な共有開示要求として受理され、その要求を現行 codec で全て提供する責任を引き受ける場合だけ。将来 profile の合法要求集合、再利用、受付条件、最大敵対負荷がこの前提を満たすかは別に確定する。申立てを恣意的に非適格として外部者の経路を消す回避は採らない。

## ソースと確認

`consensus/core/src/palw_tir_court_v1.rs`:

- `PALW_TIR_STEP_NODE_DEPTH_V1 = 10`。
- `palw_tir_step_tree_height_v1` が original leaf 数から高さを導出する。
- `palw_tir_step_node_frontier_v1` が各回答の `(下位 level, first, end)` を導出する。
- 最後の段の全 frontier は、端数 node を含め、level 0 の original leaf nodes を重複・欠落なく分割する。

`consensus/core/src/palw_tir_public_source_v1.rs` の `StepNode` 回答は `frontier: Vec<Hash64>` と `siblings: Vec<Hash64>`。実 codec は各 frontier hash を 64 bytes で literal に送る。空の siblings を用いた shape-only fixture で `1 + 4 + 64k + 4` bytes と確認した。これは認証済み回答やモデル計算結果を作ったものではない。

release 試験 `palw_court_shared_frontier_volume` は上の geometry を `N = 2, 3, 1,024, 1,025, 65,537, 2,000,000` で確認し、codec を `k = 1, 2, 1,024` で serialize して確認した。全クラス・全 `u64` 入力の網羅試験ではない。

## 条件付きの payload 下限

既存 codec で上の全末端 frontier を 1 回ずつ送ると、frontier 値だけで `64N` bytes。siblings、vector prefix、制御、署名、step preimage、operand、再取得、IO、検証 work は含めていない。multiproof による重複 sibling の削減を、この literal frontier の全量まで削減したと扱わない。

| 条件付きの例 | 値 |
| --- | ---: |
| synthetic original leaf 数 `N` | 2,000,000 |
| `64N`（frontier 値だけ） | 128,000,000 B |
| 仮定する単一 claim 用の供給 `B` | 12,500 B/DAA |
| `ceil(64N / B)` | 10,240 DAA |
| original hard deadline の予算 | 10,000 DAA |

この `B` は RFC の例と同じ条件を置いた算術入力で、承認済みの capacity・測定された保証下限ではない。この表から実モデルの不可能性や実ネットワークの性能を断定しない。全量の codec 集計、実モデルの `N`、合法な全要求集合、実供給、物理的な block capacity は未被覆。

## 実装への帰結

1. 一人が一経路を降りる door の式と、全 owner 共謀下で共有開示を提供する資源予約を分ける。小さい packet fixture で scheduler が通っても F03 は閉じない。
2. 現行 owned plan は `fresh` の資源費用に `N` を含む。資本・work の上限を持つことと、全分岐が期限内に搬送されることは別。予約 adapter は profile の閉じた集合を実際に導出し、byte/work/IO/control/保持と残余期限を検査する。
3. 全量が収まると確認できるまで `OneOutsiderCapacityIncomplete` を解除しない。モデルの除外、上限の引き上げ、unknown 項のゼロ化を達成手段にしない。
4. 未達なら、その負荷で G14 未達と報告する。SC64、現行 4 分岐、16 分岐候補、先読み profile の数字を混ぜない。数値 fold 検査は維持する。
