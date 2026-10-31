# HFから抽出したモデルの登録確認 — 2026-10-31

**全重み・最大文脈長2,048の登録、保存、別node再生、および3実装のconformanceがPASS。公開配備の宣言ではない。** [RFC-v2-0002](../../rfc-v2/0002-tensor-ir-and-shared-query-data.md) / [RFC-v2-0003](../../rfc-v2/0003-model-onboarding-and-artifacts.md) に沿って、既存の汎用TIR経路を実checkpointで確認する。

## 抽出と対象

Hubのtext-generation一覧のダウンロード順200件を保存し、公開・非gated・safetensorsパラメーター数が2億以下の13件から、保存した乱数seedで選んだ。最初の抽出は `shibing624/macbert4csc-base-chinese` で、BertForMaskedLMのCausalLM adapterがなく、変換前に拒否された。この結果は残す。

続いて同じsnapshotのCausalLM 8件を同じseedでshuffleし、先頭の `EleutherAI/gpt-neo-125m` を検査する。revisionは `21def0189f5705e2521767faed922f1f15e7d7db`。[抽出記録](hf-registration-2026-10-31/sampling-record.json)。これは資源上限を明示した1モデルの抽出であり、Hub全体の登録率・任意モデル対応の実証ではない。

## 確認できた範囲

- 固定revisionの9ファイルを取得し、長さとpublisherのLFS SHA-256／git blob identityを照合した。[取得manifest](hf-registration-2026-10-31/checkpoint-source-manifest.json)。モデルのPythonやpickleを実行しない。
- 全重みを600個の整数tensor、約166.1 MiBのArtifactへ変換した。元モデルが宣言する最大文脈長2,048・attention window 2,048で生成した。64-positionとwindow未指定の探索runは停止し、成功に数えない。
- 2,048のlayoutはshipped testnet-12のDAA 17400におけるadmission v10でADMISSIBLE。[layout記録](hf-registration-2026-10-31/declare-layout-full.log)。実際の稼働nodeの有効化状態とは別である。
- 実ArtifactからSDKでprogram・layout・tokenizer・全tensorのinventory rootとclass IDを導出した。[受入fixture](hf-registration-2026-10-31/gpt-neo-full-fixture.json)。synthetic rootやshape-only fixtureではない。

native carrier／mempool／block／保存済みregistry／別node再生の受入試験はPASS（1 test、5.06秒）。[受入結果](hf-registration-2026-10-31/native-acceptance.json) / [ログ](hf-registration-2026-10-31/native-acceptance.log)。private harnessはtestnet-12 launch規則と既存IR fenceのgenesis有効化を使い、上記のshipped DAA 17400での静的admission判定とは区別する。公開networkの資金・鍵・設定は変更しない。

追加の[変換差分測定](hf-registration-2026-10-31/fidelity.json)では、校正入力の4位置でtop-1一致率1.0、平均KL 0.0251169だった。型付き整数backendとツール内float referenceの比較であり、独立したHF Transformers実装との比較・holdout評価・2,048全域の精度確認ではない。登録資格審査や階層変更には使わない。

参照・型付きbackend・独立実装の3実装でconformanceがPASSした。[pack](hf-registration-2026-10-31/pack.json) / [生成・照合ログ](hf-registration-2026-10-31/conformance-build.log)。1 job、実際のprompt 3 token＋decode 2 tokenの5位置を検査した。packが生成したdeclared ArtifactそのものをSDKで再度読み直し、同じclass ID・rootでnative受入試験を再実行してPASSした。

[使用binaryのpin](hf-registration-2026-10-31/tools.json)。変換・静的admissionは既存のdebug toolを使い、SDK exporterとnative consensus testは今回buildした。全toolを単一commitからbuildしたという主張はしない。

## 実装と再現

HFの全checkpoint取得の不足を `scripts/models/fetch_hf_checkpoint.py` で補った。取得前の総容量上限、revision固定、破損・途中切れ・不正pathの拒否を6試験で確認した。登録判断は既存のconsensus admissionへ戻し、モデル別allowlistや新しい審査・階層変更を追加しない。

[実行手順](../../../scripts/models/README.md)。SDKの `export-tir-registration-fixture` は実Artifactを読み直し、consensusのopt-in test `g14_downloaded_hf_artifact_registration_mined_and_replayed` へ渡す。テストは鍵の署名・carrier・mempool・テンプレートのblock収録・永続registryと読出し・独立nodeのroot一致を確認する。

登録、変換忠実度、claimごとのSecurityCertified、採掘、報酬、公開配備は別である。今回の試験だけでこれらを同時に成功としない。計算の暗号学的証明やモデル単位の資格審査は導入しない。

### 今回の最大文脈長の生成条件

`calibration.json` は証拠directoryに保存した数値token列を使用した。先行runの校正統計を `--stats-in` で固定し、`--context 2048 --max-window 2048 --declare testnet-12:max-context=2048 --prompts 1 --prefill 4 --decode 2` を指定した。conformanceはこの1 jobの5位置を検査する。2,048全位置を再実行したという意味ではなく、admissionが宣言scope全体を扱えることと、短い実行jobの一致を分けている。

生成物の内容を再登録する場合も、既存classの上書きは行わず、接続先のchain domain・規則・bond・資金・署名を改めて正しく指定する。新しいモデル審査・昇格制度・計算証明は追加していない。

## 保存先

モデルの取得データ・Artifact・runtime pack・実行logは `/Users/wata/Downloads/misaka-hf-registration-20261031` に保存した。使用する登録用Artifactは `gpt-neo-full.palwtir.testnet-12.palwtir`、packは `gpt-neo-full-pack/`。公開networkへの送信は実施していない。
