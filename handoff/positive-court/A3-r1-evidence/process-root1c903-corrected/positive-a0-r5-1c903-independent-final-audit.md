# r5 同一候補 runtime の独立監査

**1c90300a4a8ff8f65b13e70c18c37856d9f0145f: 86 PASS、0 failed。ignored subprocess helper 1件は除外。追加runtimeを実行せず、Root原ログを独立監査した。**

| scope | raw成功件数 |
|---|---:|
| `positive_wide_material_v2` | 8 |
| `wide_quote_noalloc` | 1 |
| `run_wide_q24_v2` | 7 |
| `wide_capture_v2` | 7 |
| `positive_full_relation` | 11 |
| `positive_expanded_plan` | 5 |
| `cell_ipc_v2` | 6 |
| `cell` | 12 |
| `misaka_palw_tir_exec` / `node::cell_ipc_v2::` | 8 |
| `kaspa_consensus_core` / `palw_positive_check_v2::full_relation` | 10 |
| `kaspa_consensus_core` / `palw_native_resource_v1::tir::tests::public_vec_capacity_envelope_keeps_literal_minimum_and_overflow_boundaries` | 1 |
| `kaspa_consensus_core` / `palw_native_resource_v1::tir::tests::private_vec_request_preserves_capacity_byte_and_total_overflow_order` | 1 |
| `palw_tir_step_space` | 4 |
| `misaka_palw_tir_artifact` / `bounded_v1::tests::` | 5 |

同じ target/test 名の重複は0。exec 65とCore/geometry/artifact 21を同じcandidateの別scopeとして計上した。Core unit binaryは一度のbuildからfull_relation10と2つのVec exactfilterを実行した。前source ce016のPanel8成功＋quote1失敗、9193のE0308 compile failure/runtime0、9193のleaf1成功は86件へ加算しない。

## 独立照合

全raw summary・成功test名・source内宣言・nonzero・runtime引数・exit codeを確認。binary/runner/plan/log/native archiveの現在bytes/SHAが記録と一致し、各runnerの前後照合とも一致した。exec source pin 4798件、Core側 4898件は現在fileと一致し、git1c903のtracked sourceからの差分/未追跡追加は0。Core側には独立position-walkが読むroot consensus-vectors JSON100件も含む。編集中handoff Markdownはpin対象外である。

既存 `/Users/wata/.codex/worktrees/positive-court-a0/target`、cargo offline/locked、Core opt0/debug0/test mode、CARGO_BUILD_JOBS2、Rayon2、testthreads1、RUST_MIN_STACK/RUSTFLAGS unsetを確認。23,048,496-byte librocksdb.aと46,784-byte libsnappy.aはhash一致。Core unit/geometryの最終rustcだけ `-- -l c++`、C++コンパイル拒否guardと512MiB free-space閾値で自分のprocess groupだけ停止するguardを維持した。任意hostの最小stack/RSS保証ではない。

## 失敗原本・修正の限界

ce016 quote失敗binaryのgzipはcompressed bytes/SHAに加え、boundedメモリ展開した3,769,120 bytes/SHAを原binary記録へ照合した。実行ファイルへ復元した・再実行したとは扱わない。9193 compile failureのraw hashとE0308診断、別source leaf rawの1件成功も照合し、過去の失敗を最新PASSに書き換えない。

running_totalの変更はmultiplicity Vecをvalidated MAX_BLOCKS=16の256B固定配列に置換し、既存loop/order/演算を維持する。既存独立position-walk・layout拒否・旧4B/Idx bytes・builder/bindingのgeometry4は今回rawで成功。public from_programは既に検証済みprogram/info/layoutを渡す前提を持ち、任意malformed public DTOの新たな安全性は保証しない。

## 結果の範囲

ここで確認したのは固定candidateのlocal runtime回帰であり、全workspace試験や実LLM最大context・nativeモデル登録/署名/UTXO・Assignment/r1 Result・role/funding/P・合意creditの実装完了ではない。strict Clippyは別Root実行の監査PENDINGとし、runtime86へ加算しない。詳細・exact binary/SHA/コマンドは同名JSONを参照。
