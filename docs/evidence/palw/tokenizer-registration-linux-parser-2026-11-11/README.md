# Linux tokenizer parser measurements

2026-11-11。`8a69231393ee636cc311f7ddaaca29afae185c43` の通常 release build を、6 vCPU Broadwell / RAM 約12 GB の既存ホストで実行した。全8ケース×5回が期待した成功または拒否で終了した。public policy の採択・有効化ではない。

| ケース | 期待結果 | 最小 ms | 最大 ms | peak RSS MiB |
| --- | --- | ---: | ---: | ---: |
| actual | 成功 | 201.81 | 250.16 | 18.93 |
| maximum_dense | 成功 | 814.00 | 1165.11 | 71.28 |
| last_leaf_mismatch | 拒否 | 764.28 | 953.15 | 71.15 |
| giant_string | 成功 | 85.47 | 105.09 | 42.14 |
| sparse_legal | 成功 | 337.23 | 382.38 | 2.25 |
| sparse_out_of_range | 拒否 | 0.01 | 0.06 | 2.25 |
| duplicate_key_last | 拒否 | 53.04 | 56.60 | 26.13 |
| many_keys | 成功 | 610.34 | 702.47 | 89.98 |

[機材と全コマンド](measurement.json)、[集計と build provenance](summary.json)、[build log](build.log)、各 `.jsonl` / `.time` / `.stderr` を保存する。ファイルの SHA-256 は集計で照合した。

入力生成は別プロセス。上表の時間は verifier の呼出し時間、RSS は入力読込みを含むプロセス全体のピークである。共有ホスト上の5サンプルであり、最悪遅延や p99 を保証しない。最大観測値は dense ケースの約1.165秒、RSS は key-heavy ケースの約90 MiB。native の署名・予約・復元・履歴保管・最大 mergeset の費用は含まないため、この表から公開1 BPS容量や登録料を決めない。

実入力は raw SHA-256 `9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c`。再生成・実行手順は[計測ツール](../../../../tools/tokenizer-registration-probe/README.md)を参照。
