# Linux native registration replay measurements

2026-11-11。core `8a6923139` と計測ツール `4d0aee958` の通常 release binary を、6 vCPU Broadwell / RAM約12 GBの参照ホストで実行した。実入力と最大 raw、各5回・2種類の論理上限設定の **計20プロセスすべてが期待した root・fee・journal と一致**した。公開数値の採択・有効化ではない。

| 論理上限 | 入力 | harness 全体 秒（最小〜最大） | 最大 peak RSS MiB | Finish cold ms（最小〜最大） |
| --- | --- | ---: | ---: | ---: |
| 未有効 | actual | 2.34〜2.66 | 44.27 | 218.99〜253.60 |
| 未有効 | maximum_raw | 29.48〜30.24 | 76.58 | 76.19〜79.16 |
| RAM/DB各4 GiB | actual | 2.38〜2.52 | 47.14 | 226.05〜235.63 |
| RAM/DB各4 GiB | maximum_raw | 29.67〜31.29 | 96.69 | 76.27〜113.61 |

実入力は69操作、最大 raw は259操作。それぞれ2 session分の受理済み Part を復元し、第二の Finish で Complete、第一を Superseded にする。1操作につきfee仮実行と確定foldの2パス、fee巻き戻し1回、journal undo/redoを1回含む。最大5パス・2巻き戻しやM=780候補の測定は別である。

全体秒数には fixture の期待値照合と追加の状態root計算が含まれる。合意処理1回の時間や公開1 BPSの能力には読み替えない。最大rawは8 MiBの空白padding付き小語彙入力で、最大語彙・EOGを同時に満たす入力ではない。物理quotaは未設定、共有ホスト上の5回でp99や最悪遅延を保証しない。

[集計・制約](summary.json)、[source provenance](build-provenance.json)、各mode配下のmeasurement.json・JSONL・time・stderrを保存する。先行する[parser測定](../tokenizer-registration-linux-parser-2026-11-11/README.md)、[実nodeの物理DB測定](../tokenizer-registration-physical-db-actual-2026-11-11/README.md)とは範囲を分ける。
