# 公開裁定 P4 — 測定したプロファイル

- 状態: 2026-11-07。P4 は未了である。debug の全フレームドアと、そのあと別の公開救済測定がある。どちらも測定クラスの行にはしていない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空のままである。出荷 preset は武装しない。公開 T12 は主張しない。
- 関係: [公開裁定の方針 v0.1](public-adjudication-policy-v0.1.md)、[RFC-0047](../../../rfc/0047-batch-court-time-contract.md)
- 配置: この文書は記録である。`palw_batch_court_v1` も出荷 preset もここから武装しない。

## 探したハーネス

TIR / TFP1 の実クラスをオフラインで測る入口は `misaka/palw/tir/exec/tests/frame_measure.rs` にある。実クラスの試験は `tir_frame_operations_on_a_real_class` で、`#[ignore]` であり、環境変数 `MISAKA_TIR_FRAME_MEASURE_PALWTIR` が指す `PALWTIR1` ファイルを開く。同じファイルの `door_breakdown_on_a_real_class`、`read_set_on_a_real_class`、`structural_read_window_on_a_real_class`、`program_structure_dump_on_a_real_class`、`closure_bound_on_a_real_class` も同じ入力を要求する。

リポジトリ内に `*.palwtir` と `*.palwart` はない。

`tir_frame_operations_are_measured_on_every_corpus_class` はツリー内の小さな corpus からコンテナを作って動く。その記録の `status` は、arming evidence ではなく、このホストとこのビルドの壁時計であり、genesis 級クラスは測っていない、と書いている。公開救済の最悪ケースが 6,880 DAA に収まるかは出力しない。その試験は P4 の数を得るために動かしていない。

`misaka/palw/tir/exec/src/bin/tir-exec-bench.rs` の実コンテナ経路も `--container` の `PALWTIR1` を要求する。幾何を合成する経路は重みの再実行ではないので動かしていない。

`testing/positive-court-a7/src/ordinary_forward_measurement.rs` は SmolLM2-360M、SmolLM2-135M、Pythia-70M の外部アーティファクトを SHA-256 でピンする。それらのファイルはこのツリーにない。`misaka/palw/tir/exec/examples/positive-court-llm.rs` は、測定ヘッダにモデル重みがないと書いている。

## 探したディレクトリ

名前 `*.palwtir` / `*.palwart` を、次の範囲だけで探した。ディスク全体は走査していない。モデルはダウンロードしていない。

- `/Users/wata/Downloads`
- `/Users/wata/Documents`
- `/Users/wata/Desktop`
- `/Users/wata/data`
- `/Users/wata/xgen`
- `/Users/wata/MISAKA-CORE-v10-final`
- `/Users/wata/misaka-t11-genesis-keys`
- `/Users/wata/misakas-3of3`
- `/Users/wata/.cache/huggingface`
- `/Users/wata/.ollama`
- `/Users/wata/.lmstudio`（`/Users/wata/.lmstudio-home-pointer` の指す先。ディレクトリは無い）
- `/Users/wata/pret12`（無い）

ヒットは `/Users/wata/Downloads` だけである。`PALWTIR1` が 239 個、`PALWART3` が 1 個（5,433 バイト）、`PALWB0A2` が 1 個（`dolphin3.0-qwen25-1.5b-a16-32k-precise.palwart`、1,811,942,348 バイト）。後者二つは `PALWTIR1` ではない。Qwen の `PALWTIR1` は無かった。

リポジトリ内の Qwen 資料は重みではない。`misaka/palw/tir/lower/tests/fixtures/genesis8/huihui-qwen35-2b/fixture.json` は `weights_read: false` で、ピンした shard の `file_bytes` は 4,548,221,488 である。同ディレクトリの `model.safetensors-00001-of-00001.safetensors` は 76,656 バイトで、ヘッダだけである。`genesis8/llama33-70b-abliterated/` の shard もヘッダ断片である。`misaka/palw/tir/lower/tests/fixtures/` の他の safetensors と gguf は lowering 用の小さな fixture であり、TFP1 の実クラスコンテナではない。

## 走ったコマンド

レイアウトの無い `model.palwtir` は、コンテナとしては `PALWTIR1` で開いたあと、クラスを作れずに落ちた。

```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/run/model.palwtir \
cargo test -p misaka-palw-tir-exec --features node --test frame_measure -- --ignored --nocapture tir_frame_operations_on_a_real_class
```

エラーは `frame_measure.rs:768` の `called Result::unwrap() on an Err value: "the container declares no layout"`。test result は `FAILED`。1.49s。プロファイルは `test`（debug）。`--release` は、`target/release` が無く、ボリュームの空きが約 14 GiB からコンパイル中に約 6 GiB まで減ったため、使っていない。

レイアウト 98 バイトを持つ同じモデルの declared ファイルで、同じ debug 試験バイナリを再実行した。

```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir \
MISAKA_TIR_FRAME_MEASURE_OUT=/tmp/misaka-tir-frame-measure-real-smollm2-360m-declared.json \
target/debug/deps/frame_measure-5927d487c6080dc6 --ignored --nocapture tir_frame_operations_on_a_real_class
```

`test tir_frame_operations_on_a_real_class ... ok`。`finished in 31.94s`。印刷された `profile` は `debug`。`host` は `os=macos`、`arch=aarch64`、`label` は空。`source_commit` は空。`record` は `misaka-palw/tir-frame-measure-real/v1`。

- artifact: `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir`
- class_id: `ad6c9866ee15de51ad18c4c83c595dfafd646a931cc18db77b8bbe627b8e9f555dd87f3efb81943d098ff32729d9c5c645a88571f65b5e007061234fa5489476`
- inventory_root: `041029123feed837f46db580ee1b89092e0c1700b4cc033c9089f50afd587be75ceab1f985700042ee934194f244fafbd9e4940299621a8c3e2b09529dd8ec12`

## ツールが印刷した数

JSON に `cpu` という欄は無い。印刷されたナノ秒は次である。

| 欄 | ns |
| --- | ---: |
| `open_ns` | 35388041 |
| `inventory_root_ns` | 1355631000 |
| `execute.ns` | 4411131833 |
| `answer_cold.ns` | 12176552236 |
| `answer_cold.authenticate_ns` | 281913992 |
| `preparation.publish_ns` | 136619625 |
| `preparation.warm_ns` | 11700700944 |
| `preparation_pass.pass_ns` | 866261708 |
| `frame[upper 6].encode_and_hash_ns` | 141375 |
| `frame[upper 257].encode_and_hash_ns` | 3154083 |

JSON に resident RAM の欄は無い。印刷されたバイト数は次である。

| 欄 | bytes |
| --- | ---: |
| `execute.capture_bytes` | 13469666 |
| `bounds_upper6.prepared_disk_bytes` | 1487565500 |
| `preparation.retained_bytes` | 19607341 |
| `answer_cold.answer_bytes_max` | 16925 |
| `step_run.answer_bytes_max` | 261205 |
| `step_run.bound_bytes` | 4219413 |
| `frame[upper 6].frame_bytes` | 84905 |
| `frame[upper 257].frame_bytes` | 2921707 |

印刷された I/O バイトは次である。

| 欄 | io_bytes |
| --- | ---: |
| `preparation.publish_io_bytes` | 39217620 |
| `preparation_pass.pass_io_bytes` | 12649034680 |
| `preparation_pass.budget_io_bytes` | 647961843556 |
| upper 6 `owner_lane.io_bytes` | 1296003569912 |
| upper 6 `custodian_lane.io_bytes` | 1295923687112 |
| upper 257 `owner_lane.io_bytes` | 1299345333712 |
| upper 257 `custodian_lane.io_bytes` | 1295923687112 |

証拠バイトの数は印刷されていない。`filing` は文字列 `UNMEASURED`。`step_check` は文字列 `UNMEASURED: no in-memory params for this class (real-model run)`。

6,880 という数も、公開救済の最悪ケースが 6,880 DAA に収まるかという真偽も、この debug 全フレームの実行は印刷していない。印刷された `door_daa` は upper 6 で 829744、upper 257 で 23741483。どちらも 6,880 より大きい。ツール自身の `door_fits_horizon` は両方 `false` で、その比較先 `liability_horizon_daa` は 3120 である。この節のドアは、設計がフォールバックとして拒む全フレーム経路である。公開救済の最悪ケースではない。

## 公開救済の測定

同じ `early-declared.palwtir` に対する `public_remedy_on_a_real_class` である。一つの局所 primitive（commit ノードごと、および state / history の最後の葉のうち、順序つきドアが最大のもの）、その共有資料、公開提出、回復の再検査を測る。64 回の再試行と 306,108 DAA の局所化スパンは項に入っていない。上の debug 全フレームドアとは別の実行である。

```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir \
MISAKA_TIR_FRAME_MEASURE_OUT=/tmp/misaka-public-remedy.json \
cargo test -p misaka-palw-tir-exec --features node --offline --test frame_measure -- --ignored --nocapture public_remedy_on_a_real_class
```

`test public_remedy_on_a_real_class ... ok`。`finished in 11.13s`。印刷された `profile` は `debug`。stderr は `kinds 17`。`record` は `misaka-palw/public-remedy-measure/v1`。`step_leaves` は 45097。`kinds_measured` は 17。`kinds_refused` は 0。`cpu_ns_per_daa` は 171428571428。`allowed_daa` は 6880。`class_id` は `ad6c9866ee15de51ad18c4c83c595dfafd646a931cc18db77b8bbe627b8e9f555dd87f3efb81943d098ff32729d9c5c645a88571f65b5e007061234fa5489476`。

選ばれた葉は `commit:post:33:Clamp`、`leaf` 45096。ツールが印刷した数は次である。

| 欄 | 値 |
| --- | ---: |
| `cpu_ns` | 1094223000 |
| `primitive_cpu_ns` | 764632000 |
| `recovery_cpu_ns` | 329591000 |
| `peak_rss_bytes` | 609337344 |
| `evidence_bytes` | 1070720 |
| `io_bytes` | 1038080 |
| `shared_material_bytes` | 1047935 |
| `submission_bytes` | 22785 |
| `shared_material_daa` | 16 |
| `primitive_daa` | 1 |
| `submission_daa` | 9 |
| `recovery_daa` | 1 |
| `ordered_public_door_daa` | 27 |
| `door_fits_6880` | true |

順序つき公開ドアは 27 で、6,880 を超えていない。切り詰めていない。時計版 5、`collect_until`、`material_until` は変えていない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` には行を入れてない。一覧が空なので、このクラスは測定クラスとして受理されない。6,880 を超えたための拒否ではない。P4 は閉じない。

## G14 の batch 予算

`palw_g14_batch_replay_budget_estimate_v1`（`consensus/core/src/palw_g14_values_v1.rs`）の batch 予算は暫定見積のままである。`max_elements` 2、`max_terms` 3、`max_target_requests` 1、`max_source_reads` 1。測定した Qwen 再実行ではない。この文書もその関数も、その四つの数を測定値へ書き換えていない。

## 測定記録のないプロファイル

`consensus/core/src/palw_public_adjudication_measured_class_v1.rs` の `PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空である。debug 全フレームの実行も、公開救済の実行も、その配列へ写していない。`admit_measured_class_v1` は、その一覧に測定 digest のある記録がないプロファイルを `NoMeasurementRecord` で拒む。出荷 preset はこの一覧を読まない。

## 敵対的な公開救済の測定。この class は受理しない。

これは、方針が名付ける敵対的な公開救済の測定である。対象は、資料の保留、最後に合法な最初の notice、最も遅い応答、公開提出、回復である。この class は受理しない。

同じ `early-declared.palwtir` に対する `public_remedy_adversarial_on_a_real_class` である。試験は `#[ignore]` で、`MISAKA_TIR_FRAME_MEASURE_PALWTIR` が要る。種類ごとの最後の葉を、一つの葉の束としてではなく、保留、最も遅い応答、公開提出、回復の各項の最大として順に結ぶ。`notice_offset_daa` は 3120。306,108 は項に入っていない。6,880 へ切り詰めていない。

```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/early-declared.palwtir \
MISAKA_TIR_FRAME_MEASURE_OUT=/tmp/misaka-public-remedy-adversarial.json \
cargo test -p misaka-palw-tir-exec --features node --offline --test frame_measure -- --ignored --nocapture public_remedy_adversarial_on_a_real_class
```

`test public_remedy_adversarial_on_a_real_class ... ok`。`finished in 11.56s`。印刷された `profile` は `debug`。`host` は `os=macos`、`arch=aarch64`。stderr は `kinds 17`。`record` は `misaka-palw/public-remedy-adversarial-measure/v1`。`step_leaves` は 45097。`kinds_measured` は 17。`kinds_refused` は 0。`cpu_ns_per_daa` は 171428571428。`notice_offset_daa` は 3120。`allowed_daa` は 6880。`class_id` は `ad6c9866ee15de51ad18c4c83c595dfafd646a931cc18db77b8bbe627b8e9f555dd87f3efb81943d098ff32729d9c5c645a88571f65b5e007061234fa5489476`。`class_admitted` は false。`retired_localization_span_used` は false。`door_clipped_to_6880` は false。

ツールが印刷した数は次である。

| 欄 | 値 |
| --- | ---: |
| `withholding_cpu_ns` | 953294000 |
| `withholding_ready_daa` | 1 |
| `shared_material_bytes` | 1047935 |
| `shared_material_daa` | 16 |
| `shared_material_kind` | `commit:post:33:Clamp` |
| `slowest_response_cpu_ns` | 992666000 |
| `slowest_response_daa` | 1 |
| `slowest_response_kind` | `commit:attn+mlp:109:Reshape` |
| `submission_bytes` | 30401 |
| `submission_daa` | 9 |
| `submission_kind` | `commit:attn+mlp:159:Reshape` |
| `recovery_cpu_ns` | 328882000 |
| `recovery_daa` | 1 |
| `recovery_kind` | `commit:post:33:Clamp` |
| `evidence_bytes` | 1070720 |
| `io_bytes` | 1038080 |
| `peak_rss_bytes` | 608518144 |
| `ordered_public_door_daa` | 28 |
| `door_fits_6880` | true |

順序つき公開ドアは 28 で、6,880 を超えていない。保留のオフチェーン時間は `withholding_ready_daa` 1 であり、ゼロではない。切り詰めていない。306,108 へ戻していない。`PALW_PUBLIC_MEASURED_CLASS_RECORDS_V1` は空のままである。一つの debug コンテナは class の受理記録ではない。P4 は閉じない。
