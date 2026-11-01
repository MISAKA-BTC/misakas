# A7 startup境界の追記

source `e328aae6273589b06a8a6daca88ce9499e02f5f5`。A0の追加確認に対応し、artifact open・nested header decode・compile・parameter instance map・weight identity setupの割当前上限は未確立、runtime quoteの対象外とRust/driver JSON・[使用条件](../../REAL-LLM-REPRO.md)に明記した。

子process groupのwall timeoutは起動処理を含む。Python親側のsource/input hashは対象外。Darwin `time -l` RSSは起動処理を含む全寿命の観測で、段階別RSSではなく、強制RSS上限も設定しない。これらはstartup readerの割当前admissionやnative leaseを代替しない。

独立静的レビュー、rustfmt、Python syntax、差分checkを確認。更新sourceのdry-runは3モデルとも入力エラー0、source前後一致、外枠観測・quote・推論はNOT_RUN。[原manifest](preflight/manifest.json)、[command/hash/review記録](report.json)。以前の12件のpure/mock guard結果は旧固定sourceの記録のまま保持し、今回のruntime結果へ加算していない。

Cargo・build・型検査・推論・RSS計測・timeout実験は未実施。A0のheavy-build holdを維持し、初回の実行許可後はcompile→quote-onlyの順で進める。native/credit/独立expected rootsもNOT_RUN。
