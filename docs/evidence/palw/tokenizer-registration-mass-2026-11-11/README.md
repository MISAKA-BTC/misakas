# TKR1 signed carrier mass probe — 2026-11-11

実 tokenizer raw 2,104,556 bytes と Studio token table を使ったローカル搬送検査。公開 T12 の登録・送信・受理、数値 policy の性能認定ではない。固定テスト鍵と架空の network/class/policy/opener/期限/UTXO/料金を使用する。input の同一性・fixture 全値・コンパイル済み依存ライブラリの digest は [測定条件](notes.json) に記録する。**使用 rlib の正確な source commit は不明**。チェックアウト HEAD はビルド由来の証明として扱わない。

[ハーネス](main.rs) は別々の固定鍵で各 control と通常 UTXO transaction に署名し、TxScriptEngine の通常スクリプト検証を実行する。T12 shipped mass parameters を使い、非 coinbase P2PKH-MLDSA 1 入力・同じ script 1 出力で測定した。[初回結果](result.csv) と [再実行結果](replay.csv) は同一。raw 全体は 33 Part（32 × 65,536 bytes、最後 7,404 bytes）。

| Carrier | Payload bytes | Tx bytes | Compute mass | Transient mass | Storage mass |
| --- | ---: | ---: | ---: | ---: | ---: |
| Open | 5,451 | 12,942 | 23,652 | 51,768 | 4 |
| 最大 Part | 70,510 | 78,001 | 88,711 | 312,004 | 4 |
| 最終 Part | 12,378 | 19,869 | 30,579 | 79,476 | 4 |
| Finish | 5,182 | 12,673 | 23,383 | 50,692 | 4 |

最大値 312,004 は標準 mempool 上限 480,000、block 上限 500,000 内。空きはそれぞれ 167,996 / 187,996。複数入力・異なる出力・他取引を詰めた body の上限はこの結果で保証しない。TKR service work、予約容量、実料金、最大合法入力の CPU/RSS、block acceptance は別の検査を要する。

[コンパイルコマンド](compile.sh)・[再実行スクリプト](run.sh) は当時の一時 build path を記録しており、同じ rlib digest が必要。[コンパイル終了コード](compile.exit) は 0、[標準エラー](compile.stderr) は空。公開用採択前には、出荷対象 source commit からビルドした依存関係でも再検証する。
