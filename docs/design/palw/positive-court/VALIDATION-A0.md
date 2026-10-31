# A0 共通契約 r1 — 検証記録

2026-10-31。対象は `codex/positive-court-a0` の共通crate・docs。Node演算・Panel生成・native受付・支払の全体検証ではない。

| 検査 | 結果 | 証拠 |
| --- | --- | --- |
| `cargo test -p misaka-palw-positive-contract --offline` | 13 PASS、0 FAIL | [raw log](../../../evidence/positive-court-a0-2026-10-31/tests.log) |
| `cargo clippy -p misaka-palw-positive-contract --all-targets --offline -- -D warnings` | PASS | [raw log](../../../evidence/positive-court-a0-2026-10-31/clippy.log) |
| `cargo metadata --offline --no-deps --format-version 1` | member/default-member登録確認 | [summary](../../../evidence/positive-court-a0-2026-10-31/cargo-metadata-summary.json) |
| 文書検査 | 新規欠落0、既知43 | [raw log](../../../evidence/positive-court-a0-2026-10-31/docs-check.log) |
| 別エージェントの読み取りレビュー | 3指摘を修正し再確認。残る重大指摘なし | [record](../../../evidence/positive-court-a0-2026-10-31/readonly-review.txt) |

ソースのSHA-256は [snapshot](../../../evidence/positive-court-a0-2026-10-31/source-snapshot.json)。テストは実ML-DSA-87の役割・actor・署名scope、field改変、登録root・branch・ruleset・正規job/execution/inputの照合、旧形式回避、range/上限/切断/余剰bytes、全Panelで共有するcase identity、独立Pythonの固定bytes/ID vectorを確認した。

レビューで発見した問題と修正は、(1) Witnessのprofile/version照合、(2) payload長の検査迂回を防ぐprivate framing field、(3) 任意execution nonceによるcase予算更新を防ぐtrusted contextのjob/execution/Miner/input固定。反例は契約テストに含めた。current stateでのatomic commit再検証、同じcaseでの異なるResult拒否、dutyとrequestの一体化も接続契約へ反映した。

公開runtimeへの接続・有効化なし。既存Nodeが新しいnative objectを受理する変更はなく、`require_native_activation`は拒否する。旧G14の全回帰を実行済みとは報告しない。実LLMの負荷測定とcold/warm供給、原資・全consumer・restart/reorg/IBDはA1〜A7の結合時に確認する。
