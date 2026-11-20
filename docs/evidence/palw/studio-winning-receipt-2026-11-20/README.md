# Studio claim の隔離 T12 観測

2026-11-20、`codex/public-one-honest-vertical` の `3f117d246` から作った Linux ノードで、登録済み DAA 317 のチェックポイントを再開した。3 ピアは同一ホスト上の独立プロセスで、専用 genesis と履歴時計を使った。ノード実行ファイルの SHA-256 は `a954b1ae693a3ea4ed9a337d61c6d68d43c0679a9cf25ae475956f1c6bed621b`。追加統合した AN1・CAD1・CPN1・SWC1 は未採択案として保持し、この実行には既存の protected TFO3 受付を用いた。

## 受付

- Studio が登録済みモデルを検出し、prompt の実行で 85,938 work leaves を記録した。[Studio 応答](studio-run.json)
- Studio job と署名済み rail 取引を claim ID、output root、実行 root で対応づけた。`studio-evidence.json` は照合に使う項目の抜粋。[Studio と rail の対応](studio-evidence.json)
- ノードは carrier 取引 `3a989cfc…a54f08de` を、ブロック `62182f1b…66303ebd` から選択鎖ブロック `c643963c…bb93873` の ordinal 1 で受理した。DAA 318、blue score 638。受付時点の phase は `provisional`、Gate C は `pending`、21 量子の消費数は 0。完全な識別子、両ヘッダー、取引本体は RPC 観測に記録した。[受付観測](original.json)
