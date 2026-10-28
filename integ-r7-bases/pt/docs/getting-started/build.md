# ビルドする

対象release / commitを固定し、その `Cargo.toml` とtoolchainを確認します。以下は元資料からの例で、今回実行検証していません。

```sh
cargo build --release -p kaspad
cargo build --release -p misaka-cli --bin misaka
```

package名misaka-cliとbinary名misakaを区別します。EVM等のfeatureは対象releaseと一致させます。

OS別依存関係と旧worker構成は [整理前のビルド資料](../history/documentation/2026-10-17/getting-started/build.md)に保存しました。旧releaseの「latest」「live」やtestnet-11の値を現在の既定値としません。

[ノード接続](run-node.md) / [状態](../STATUS.md)。文書やcodeのmergeだけでは延期した画像・動画・改善を有効にしません。
