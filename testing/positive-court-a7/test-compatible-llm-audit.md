# A7 — existing trained artifact / claim-profile audit

2026-10-31。**確認した既存の学習済み4候補には、今回のA1 all-node-commit i32 profileで直ちに実LLM計測できるものを確認できなかった。** 登録資格・モデル品質・tierの審査ではなく、claim checkerが対応する計算profileとの静的適合性だけを照合した。

新規ダウンロード、変換、program/commit flag変更、登録、ビルド、推論は行っていない。既存PALWTIR1のheader/programを有界に読み、Borshの宣言schemaを全fieldのEOF一致まで走査した。weight本体は読まず、native証拠のartifact root/class IDは既存記録から引用した。したがってweight/file全体の再認証ではない。実Rust checkerの拒否コード、実LLM throughput/CPU/RSS、cold搬送は **NOT_RUN**。

## 必要条件と具体的阻害点

`CoveragePlan::derive` はprogramの全blockについて未commit nodeが一つでもあれば初期profileを拒否する。tiled i32 schemeとI32 logitsも必須。TIR v1のI64/I128中間値はcommittableではないため、未commit flagを一括trueへ書き換えて同じ登録classとして扱うことはできない。今回その変更も新規変換・登録も行わない。これはA1/A0への対応範囲の事実の引継ぎであり、凍結設計の変更案ではない。

countsはblock宣言のunique node数で、全layer/positionへ展開した仕事量ではない。512のlogits tile自体は4096を割り、現TIR layoutで許容されるため拒否理由にしていない。

| 既存候補 | 全node | commit | 未commit | 未commit中I64/I128 | logits | 結果 |
|---|---:|---:|---:|---:|---|---|
| EleutherAI/gpt-neo-125m | 473 | 23 | 450 | 287 | I32 | profile条件不適合 |
| HuggingFaceTB/SmolLM2-135M | 308 | 17 | 291 | 176 | I32 | profile条件不適合 |
| HuggingFaceTB/SmolLM2-360M | 308 | 17 | 291 | 176 | I32 | profile条件不適合 |
| EleutherAI/pythia-70m (wide Q24) | 324 | 15 | 309 | 193 | I64 wide | profile条件不適合 |
| trl-internal-testing/tiny-Qwen2ForCausalLM-2.5 | 258 | 17 | 241 | 151 | I32 | profile条件不適合、実LLM証拠に不使用 |

GPT-Neo/SmolLMのlogits schemeはA1のtiled i32 schemeと一致するが、all-node-commit条件を満たさない。Pythiaのwide artifactはlogits dtype/schemeも対象外。Rustの入口で先にcanonical decode/binding確認に失敗する場合もあるので、実行していない特定のRust error enumを断定しない。

## 既存のファイル・記録上のroots

**EleutherAI/gpt-neo-125m**

- 既存artifact: `/Users/wata/Downloads/misaka-hf-registration-20261031/gpt-neo-full.palwtir.testnet-12.palwtir`
- program SHA-256 (実headerから): `3cab5cab672a096dc1ce33abaee6f6a38f637d3ed816ec1be8cc19813ac862ef`
- header SHA-256: `9c1bcd658dbc6ee1a2c9b8fb0f9a597aa7d1f90242234b758beb70fc008ecbc9`
- 記録上のclass ID: `07c7a10a741c0f2e92dbfa82174dbec890f33c895bcdcc6462de54cc2fc5813c4d916b07dd1c841066b0173bc67b820044319c78722b9839282b2fa02ce094ac`
- 記録上のartifact root: `dabe6aaa926bccd5143fe34c5b78ba258809527bcaf7a1a497873d36c40c4bdbde1a87b8868274b30a7fe56a63f2f78972de84d139e05f8cc3722cd5f20b0ffc`
- 固定revision: `21def0189f5705e2521767faed922f1f15e7d7db`、記録上max context: `2048`
- 記録: `/Users/wata/Downloads/MISAKA-testnet/docs/evidence/models/hf-registration-2026-10-31/native-acceptance.json`

**HuggingFaceTB/SmolLM2-135M**

- 既存artifact: `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-135M/run/model.palwtir.testnet-12.palwtir`
- program SHA-256 (実headerから): `744225e2c41ebb11a741f86e9109f49cae28b3a0b50a07d266685779872cf6df`
- header SHA-256: `0767360d41d7fe101e9b28d369b272a4bf3b9b35ba5f36363c7c4cc107d8c3e3`
- 記録上のclass ID: `7a2616b4b132946d21b1d396c0a82f69a144fb7f692394314efb35e3f8bff1f231b857f174e1165ad92c65ce742e1bcc57dfb11e1e5eab02b67b8cf87436c625`
- 記録上のartifact root: `8e31580dc81f6548b9e440c9b6d45fe63bff1d42ed2dbb4b342bdb58fc4590f0b8210f10a2a23e6c352c866a4313004b547ec363997e5757d2bce31a6838eec4`
- 固定revision: `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`、記録上max context: `8192`
- 記録: `/Users/wata/Downloads/MISAKA-testnet/docs/evidence/models/trained-llm-cohort-2026-10-31/HuggingFaceTB__SmolLM2-135M/native.json`

**HuggingFaceTB/SmolLM2-360M**

- 既存artifact: `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/HuggingFaceTB__SmolLM2-360M/run/model.palwtir.testnet-12.palwtir`
- program SHA-256 (実headerから): `4e28b26f43de39c6ea6549dd37d196d7a2da8c7d2a7a6095dad13381534eafe3`
- header SHA-256: `83c2c01b0b10107fe37423a6682ce82e0ca6c55ee86c2747e3f658e6412eaa1c`
- 記録上のclass ID: `ad6c9866ee15de51ad18c4c83c595dfafd646a931cc18db77b8bbe627b8e9f555dd87f3efb81943d098ff32729d9c5c645a88571f65b5e007061234fa5489476`
- 記録上のartifact root: `041029123feed837f46db580ee1b89092e0c1700b4cc033c9089f50afd587be75ceab1f985700042ee934194f244fafbd9e4940299621a8c3e2b09529dd8ec12`
- 固定revision: `f8027fd0eaeea54caa13c31d31b9fdc459c38b49`、記録上max context: `8192`
- 記録: `/Users/wata/Downloads/MISAKA-testnet/docs/evidence/models/trained-llm-cohort-2026-10-31/HuggingFaceTB__SmolLM2-360M/native.json`

**EleutherAI/pythia-70m (wide Q24)**

- 既存artifact: `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/EleutherAI__pythia-70m/run-wide-q24-v2/model.palwtir.private-wide-q24.palwtir`
- program SHA-256 (実headerから): `1c5dba053682afd2a53b5649da96150860463a35ae35505aa0c114e425e145b9`
- header SHA-256: `201d7b8edd61c76fc116feb80cc7872e9d4e233c0e0c1895ba5a0297060792f4`
- 記録上のclass ID: `356fbf891ef8ff98cc008c5187c7105bbc9eb32a53225a54361493c96fdc75f3af9d7acb1789a4fa9866771442fa1a55a8217371278bf88130ece5d2e4af91e1`
- 記録上のartifact root: `427a912e722f173d1d0fe4b02dac4148709e9a70d245c3012768a5962fa3f6736ea9cb921b8824b9712a1f718f5dc389c00c1d81fa9c8f552fd358c96ba10ef4`
- 固定revision: `a39f36b100fe8a5377810d56c3f4789b9c53ac42`、記録上max context: `2048`
- 記録: `/Users/wata/Downloads/MISAKA-testnet/docs/evidence/models/trained-llm-cohort-2026-10-31/EleutherAI__pythia-70m/wide-q24-v2/native-retry-native.json`

**trl-internal-testing/tiny-Qwen2ForCausalLM-2.5**

- 既存artifact: `/Users/wata/Downloads/misaka-hf-registration-round2-20261031/final-run/model.palwtir.testnet-12.palwtir`
- program SHA-256 (実headerから): `c453a497208dd9cd4ffd4151ec2ad9611f016985e1c03a5564fde3cf727a2746`
- header SHA-256: `3b3c733465b7f1e980f0cb4f78b19fb2e0b2a2392d704794679b26c81371bf40`
- 記録上のclass ID: `2d966d22fd9e808b91fbdf4f498ab9242f65ce647559704aa04eb5d0f9c64ad3df83b0f817e7c0abe6c303844425f582800eeb2adee93b399c87d028b8946d86`
- 記録上のartifact root: `963c172b3a90e86e66c67ae211c9f0eb6fcfd8084dbb6182cd3a5be6737ea6f5a09ee3320cba95f191cec0276acf53efed4c87de0463c1e33251a2dfd28e9186`
- 固定revision: `ce8d0bf270b28c8fab026ced69fe8aa14b0f0eda`、記録上max context: `32768`
- 記録: `/Users/wata/Downloads/MISAKA-testnet/docs/evidence/models/hf-registration-round2-2026-10-31/native.json`

## 再現・限界

機械可読の[監査結果](test-compatible-llm-audit.json)に、実測header/program SHA、参照した既存JSONのSHA、checker source SHAと必要条件を保存した。例として各候補の最初の未commit nodeはpre blockのGatherで、後続にもI64/I128 nodeが多数ある。

確認対象は既存trained cohort（SmolLM2 135M/360M、Pythia70M）、既存GPT-Neo125M登録実験、および比較用tinyQwenの5artifact。Downloads内の関連ディレクトリのファイル一覧も確認したが、全ファイルシステムを網羅した不存在証明ではない。tinyQwenを実LLMの代用にしない。既存の3実装conformance・private登録成功を、このPositive Court profileの対応や本番認定へ合算しない。

公開有効化、model tier、登録規則、claim単位の認証条件は変更していない。今回の結論は、確認した既存入力で実LLM計測を完遂できた、というものではない。
