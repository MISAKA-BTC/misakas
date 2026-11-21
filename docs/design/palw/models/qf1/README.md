# QF1の具体profileと小規模参照

正本は[RFC0066](../../../../rfc/0066-qwen3-fow1-execution-and-readout.md)、判断は[ADR0234](../../../../adr/0234-qwen3-fow1-execution-and-readout.md)。状態は**ADOPTED_DESIGN_NOT_ACTIVATED**。

| ファイル | 用途 |
| --- | --- |
| [profile JSON](../qf1-qwen3-8b-profile.json) | architecture、入力、StateMap、数値型、Readout、未完成gateを機械可読で固定 |
| [trace-schema.md](trace-schema.md) | Init/値/選択/Stop/hash chunk/Endのlogical byte layout |
| [qf1_reference.py](qf1_reference.py) | 位置・Readout・Merkle・Ticket・BLAKE2b chunk stateの小規模参照 |
| [check_qf1_reference.py](check_qf1_reference.py) | 境界・型・コピー・chain改竄の否定例を含む検査 |
| [conformance-vectors.json](conformance-vectors.json) | 同じ位置規則と公開synthetic tensorによる再現用値 |

```bash
python3 docs/design/palw/models/qf1/check_qf1_reference.py
```

参照は標準Pythonだけで動く。整数化済みQwen、上流tokenizer、native登録/Panel/Court、TB1/BFT、実性能・品質を実行しない。s/rootsを引数に与えられることは、元claim又はbeaconの認証を済ませたことを意味しない。TRACE treeのleaf内容はcodec試験ではopaqueであり、正しい実行traceの受理試験ではない。

Readout testの値は`((j+1)*257 + layer*31 + channel*17) mod 65535 - 32767`で作った架空tensorで、Qwen推論結果ではない。37×4096のshape、little-endian、hashの順序・chunk境界を調べるためだけに使う。非衝突性や費用の定理の根拠にしない。

実モデル登録では、未入力のdigest・program・scale・tokenizer等を埋めて全gateを通す必要がある。UNKNOWNはMAS1の採掘不可・BFT risk reviewへ接続する。小規模検査の合格でこのgateを解除しない。
