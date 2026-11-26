# モデル・実行契約

[BFTのみのモデル追加](../../../rfc/0007-bft-model-admission.md) · [TIR/runtime](../../../rfc/0002-tensor-ir-kernels-and-runtime.md) · [QF1](../../../rfc/0005-qf1-qwen3-execution-profile.md)

モデル名だけでは登録せず、不変artifact、program、tokenizer、数値profile、入力adapter、Readout、trace/checkerを結合します。kernel解析・自動変換は技術互換を調べるもので、採用資格を与えません。

正式追加は共通技術互換と正規BFT YESだけを使います。BFTのYES/NOに費用定理・品質証明・測定証拠の提出を必須にしません。G-COLL/G-COST/G-USEのUNKNOWNはそのまま表示し、未知kernelや個別claim不正を免除しません。

- [採用policy](mad2-admission-policy.json)
- [任意分析policy](mas1-admission-policy.json)
- [QF1 instance契約](qf1-qwen3-8b-profile.json)
- [QF1 codec・参照計算](qf1/README.md)

genesisを含む全モデルの共通経済・管理はMOE3に従います。profile名やdomain中の既存`v1`はwire識別子であり、RFC体系の版分割ではありません。
