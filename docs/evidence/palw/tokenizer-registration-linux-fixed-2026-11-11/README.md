# Fixed Linux native tokenizer replay — 2026-11-11

Source `098da5e04136e4e6c8050c0851bde81974990783` の全ソースを通常 release でビルドし、参照ホストで **25 プロセスすべて成功**した。binary SHA-256 は `ec865bddecf860369a199f2d8cadf23ce00242bfcccf5001da2c213cb45828a6`。実 tokenizer と 8 MiB raw の native candidate・5 pass、M=780 scanner を各5回実行した。これは公開数値の採択・有効化ではない。

| モード | 入力 | プロセス数 | harness 全体 秒（最小–最大） | 最大 peak RSS MiB |
| --- | --- | ---: | ---: | ---: |
| native-candidate-logical4g | actual | 5 | 9.51–10.20 | 51.99 |
| native-candidate-logical4g | maximum_raw | 5 | 54.60–56.79 | 96.79 |
| native-multipass-logical4g | actual | 5 | 3.10–3.32 | 47.53 |
| native-multipass-logical4g | maximum_raw | 5 | 39.62–40.45 | 97.45 |
| scanner-m780 | actual | 5 | 0.01–0.02 | 5.00 |

全 native replay は RAM/DB 各4 GiBの**論理上限**を指定した。物理 OS quota は設定していない。64 MiB worker stackを使う。共有ホストは6 vCPU Broadwell・RAM約12 GBで、既存ノードと併存し、default affinity/nice 10で実行した。5回の値をp99・最悪遅延・最小機種性能として扱わない。

candidateは受理済み親fixtureを基に、同じFinishを異なるcarrier識別子で780回処理するモデルである。carrierごと5 native pass・2 fee rollback、確定foldは1回、Complete/Finish slot消費は1回、残り779回は終端済みno-opで、導出keyはfixture構成上1個である。実DAG・UTXO検査・複数登録者の競合最悪ケースを含まない。実入力69操作、最大raw259操作、journal undo/redo・snapshot復元・期待値照合を含む全体時間であり、合意遷移1回や公開1 BPS能力の計測値ではない。

旧実装で論理4 GiBを枯渇させた最大rawのM=780ケースが、修正後は5回とも完了した。修正は、状態もjournal容量も増やさないFinish再試行から一時予約を返すもの。[修正前後の再現記録](../tokenizer-registration-native-candidate-2026-11-11/README.md)を維持する。過去の公開T12停止原因であると特定したものではない。

最大rawは8 MiBまでpaddingした小語彙fixtureで、最大語彙・EOG・key数を同時に満たすケースではない。scannerは不正形式と未解決Finishの重複を780 sourceとして数え、source重複とcandidate超過を拒否する2ケースを各プロセスで実行し、parserを呼ばない。

[集計と全ファイルdigest](summary.json)、[ソースとビルドの来歴](provenance.json)、[ビルドログ](build.log)、[実行スクリプト](measure.sh)、[runner](measure_linux.py)、[全体実行ログ](measurement.log)を保存した。各modeのmeasurement.jsonは入力bundle全ファイルのdigest、実行コマンド、前後のホスト観測を含む。各JSONL・time・stderrのdigestを再照合した。以前の[別ソース・2 pass測定](../tokenizer-registration-linux-native-2026-11-11/README.md)とは分離する。

公開TFP1 quote成功、対象claimの受付、Final、receipt block、mainへのpushはこの記録では未成立である。
