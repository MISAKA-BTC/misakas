# RFC0042 R1 → R2 差分

| 項目 | R1 | R2 |
|---|---|---|
| Miner commit | 全traceも必須 | 結果・計算条件が必須。新全traceは不要 |
| 証拠trace構築 | Minerのtrace固定に依存 | Panelが再計算して自分で構築 |
| 乱数前の固定 | Miner trace | Panel witness。健全性の事前固定は維持 |
| 対話 | material搬送が中心 | 不足data要求→有限応答→Panel計算→提出 |
| 資源 | 主にNode総量 | Miner追加分・Panel生成・Node検査を別予約/実測 |
| 帰責 | claim/trace一体 | Miner resultとPanel witnessを分離 |
| 不提出 | coverage不足 | Miner data / Panel evidence / Node checksの三台帳 |
| 偽witness | Miner署名traceの矛盾を想定 | Panel不正はまずwitness拒否。Miner有罪は独立exact反証 |
| 実装 | 新規全範囲Court | 新しい軽量結果commit＋Panel proverを明示追加 |

## 維持する規則

五つの安全条件、全relation被覆、モデルの必要情報の開示、全員共謀でも署名だけでは通さないこと、Gate C等とのAND、未検証の無信用、年次カタログ/1.0/0.3、暗号学的validity proof不採用は維持する。

## 変更しない／主張しないこと

Panelへ計算を移しただけでNode検査や総bytesが小さくなるとはしない。Miner本人のGPU使用履歴・実FLOPsの証明ではない。旧runtimeから重い処理を除去済みではない。新しいPositive証拠に必要なため、モデルを最後まで完全秘匿できるとはしない。

## 導入

パッケージのRFCとADRは同名の前案を置換する。`rfc042-r2.patch`は前案の文書・参照モデルを導入済みのtree用。`rfc042-r2-from-upload.patch`は今回の元ZIP用。両方を順番に適用しない。

root README/VALIDATION、evidenceは配布説明であり、repository rootを上書きしない。別branchの変更・番号予約があれば`git apply --check`の結果と差分を確認し、無理に適用しない。docsのindexは統合時に既存generatorで更新する。現行入口はRFC-v2-0014、ADR-v2-0038のまま。
