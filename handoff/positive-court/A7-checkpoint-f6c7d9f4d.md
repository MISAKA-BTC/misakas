# A7 — f6c7d9f4d の軽量追跡確認（2026-10-31）

固定checkpoint `f6c7d9f4d` を親と独立担当でread-only確認した。Cargo実行・追加target作成・production編集なし。A0から次の全candidate指定を受ける前の静的確認であり、新たな実行PASSではない。

## A4の指摘修正

前回の[815b7ca1c追跡](A7-A4-fix-review-815b7ca1c.md)で残ったfuture-frame P2は、`17990b173` で静的に解消した。checkpointの対象2ファイルは同修正と同一。

`coordinator/contract.rs:415–432` はsingle relation、`require_next`、progress存在確認の後にだけCase wire予算と時計を変更する。未受理future frameと不適格headerのコピー再送はCaseを変えない。受理済みframeのACKも現在のauthority/署名を確認してから無課金で返す。現在位置の署名済み不正算術は課金後にcheckerへ渡り、候補拒否と費用保持を維持する。

`positive_court_a4_integration.rs:605–657` の追加negativeは、同じ正規署名済future packetの反復拒否、全counterと時計の不変、その後の同じpacketの正順受理・完了を含み、反例を直接検査する内容。523–547行は不正算術の費用保持・次attemptへの累積を検査する。A7自身のruntime確認は次の固定候補で行う。

P1の事前role認可は維持されている。追加の具体的な残存不具合は今回の範囲では確認していない。

## 次のrunner更新条件

次の全candidate指定後、有効化境界は `cargo test -p kaspa-consensus-core --test positive_court_fence_v2` を明示する。checkpointではwrapperが既存public試験を `checks` moduleとして実行し、8個の `#[test]` がある。

- target単位で成功8件以上・失敗0を要求する。
- policy root、released presets、profile fingerprint、pre-fork peer identity、全profile field、無効caps、ruleset binding、raw tag opaqueの8試験が集合として欠けないことを確認する。
- 現在のlib `positive2_` はprivate visitorの1件だけであり、8件の代替にしない。
- 実行対象・source集合・HEAD・両lock・実行binary・logを新しい同一候補で固定し、古い成功件数を合算しない。

A2 rawjournal/tokenizer/logical-source NoOpの残る統合と、A0の全candidate指定を待つ。native176 writer/header/profile、実ネットワーク、実LLMの成功は認定しない。A7の歴史的243件は `bd9623914`、証拠commit `eeb32005c` のまま保持する。
