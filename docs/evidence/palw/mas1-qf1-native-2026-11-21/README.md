# MAS1・QF1 改訂の統合と隔離試験

2026-11-21 に受領した `misaka_docs_MOE2_MAS1_QF1_20261120.zip` の SHA-256 は `95f460df28fd5a8f7aad2ce50f12f0c20d7cbb15a3de3958523f1de58ac41303`。展開した3,094ファイルがすべてリポジトリの同じパスとバイト一致した。統合前の台帳にあった Studio 実行記録は対応する[隔離試験の記録](../studio-winning-receipt-2026-11-20/README.md)に保存した。

最新版の改訂検査は入力3,012件、変更51件、履歴保存51件を照合した。さらに同梱の変更前ファイルを SHA-256 で検査して MAS1 版を復元し、その改訂検査も実施した。QF1 の Python 参照検査は57項目成功。過去の原本と照合する RFC・ADR の全体検査には配布版と同じ7件・157件の指摘が残る。文書全体の strict 検査にも配布原本から引き継いだ指摘が残っている。

## QF1

RFC-0066 の入力試験は Qwen3-8B の tokenizer をリビジョン `b968826d9c46dd6066d109eabc6255188de91218` で固定した。ファイルの SHA-256 は `aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4`。固定した3区間と通常の入力文 `2 + 2?` を個別に符号化した。元の Studio モデルの byte-token 入力とは署名対象の domain と instance が異なる。隔離 QF1 登録の model/program・資金参照は試験値で、実際の QF1 インスタンスの登録を意味しない。[固定した tokenizer](https://huggingface.co/Qwen/Qwen3-8B/blob/b968826d9c46dd6066d109eabc6255188de91218/tokenizer.json)

Readout の試験は配布版の合成テンソルを用いる。1回答位置について36層の第二残差と最終 norm に相当する37本の各4,096値を順に符号化する。303,141 バイトを75個の chunk に分け、前後の BLAKE2b 状態、counter、最終フラグを照合する。四つの root の順は X、Y、Z、TRACE。合成 TRACE を用いた Ticket は符号化の比較値である。入力試験と合成テンソル試験は別の実行である。MAS1 の G-USE 評価には専用の TB1 MODEL_AUDIT と後続の PANEL が必要になる。

## MAS1

新規採掘の 6項目の判定 について、FAIL は REJECTED、UNKNOWN は REVIEW_HOLD とする。未解決 case が原期限を迎えると REJECTED_UNRESOLVED に移る。隔離版の MODEL_STOP reader は元の roster、case、停止証明の順序と対象を選択鎖のブロック本文で照合する。登録者が選んだ候補を権限の根拠にせず、この試験では専用署名鍵と許可された roster の digest をあらかじめテスト設定で固定する。

3署名は隔離 fixture の異なる鍵で生成する。3ピアは同じ署名をそれぞれ検査する。対象者4人のうち故障・共謀が最大1人という BFT 前提は試験値。実ネットワークの roster 選出、evidence 資料の可用性、停止と解除の永続 reducer、実インスタンスの G-USE・G-COLL・G-COST、公開有効化とは別の検証である。

## 実行結果

同じ native バイナリで二つの隔離チェーンを試験した。TB1 チェーンでは3プロセスが同じ EXEC 証明書から QF1 入力111 token を導出した。独立した Hugging Face tokenizers 0.23.3 と各ピアの全111 ID が一致した。同じ登録集合の従来形式を選んだ Studio モデルは回答 token 95 を返した。QF1 の新規採掘登録は REVIEW_HOLD で拒否された。

MOE2 チェーンでは3プロセスが市場の生成、異なる鍵による3署名の MODEL_STOP、停止後も市場が存在することを確認した。2署名、重複 slot、別 descriptor は拒否された。より重い分岐に切り替えた後は、各プロセスが元の停止証明の選択鎖所属を再検査し、適用を拒否した。[バイナリの SHA-256・各ピアの観測値・回帰試験](observations.json)

## 再現

リポジトリのルートから実行する。3プロセスで実 tokenizer を照合する際は、上記のファイルのパスを `MISAKA_PRIVATE_QF1_TOKENIZER` に指定する。併走する従来の Studio モデル推論には [TB1 の実行記録](../tb1-moe2-native-2026-11-21/README.md)に記載した `MISAKA_PRIVATE_TB1_TIR_ARTIFACT` を指定する。

```sh
export CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=target/protected-core-tests RUST_MIN_STACK=67108864
python3 -B docs/maintenance/revisions/2026-11-20-qwen-fow1/check_revision.py
python3 -B docs/design/palw/models/qf1/check_qf1_reference.py
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_qf1
cargo +1.93.0 test --locked --offline -p kaspa-consensus-core --lib palw_mas1
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib native_withdrawal_halts_an_exposed_holder -- --nocapture
cargo +1.93.0 test --locked --offline -p kaspa-consensus --features evm --lib private_moe2_three_process_registration -- --nocapture
```
