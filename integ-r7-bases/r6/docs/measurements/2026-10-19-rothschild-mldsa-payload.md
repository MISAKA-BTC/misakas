# 測定: rothschild の ML-DSA-87 payload tx(2026-10-19)

## 冒頭
- 測定日時: 2026-10-19
- source: branch release/t12-16000、commit は下記 commit 参照(rothschild の変更を含む)
- ビルド条件: `cargo check -p rothschild --tests` と `cargo test -p rothschild --bin rothschild`(dev profile、build lock 下)
- 測った host: なし(ノードは未起動。稼働中の t12 には何も送っていない)
- 負荷・試行回数: なし

## 結果
- コンパイル: 成功(rothschild のエラー・警告なし。出力された警告はすべて他 crate)
- unit test(`payload_bound_is_the_cli_maximum`): 1 件合格

## 測れなかった項目
- 隔離 devnet での 17,408 B payload tx の mempool 受理・block 受理: 未実施。devnet ノードの起動、資金の mature 化、mining の準備が必要で、今回は行っていない。
- tx 1 本の bytes と mass: 未測定(受理試験の tx から取る必要がある)。
- 既存の署名経路との比較: 未実施。

## 注意
- 署名は validator-core の `sign_native_multi` と同じ primitives(`calc_mldsa87_signature_hash`、`MLDSA87_TX_CONTEXT`)を使うが、関数自体は rothschild 内に複製している。payload 引数を validator-core に入れる修正は別途必要。
