# Hugging Faceモデルの登録準備

規範入口は [RFC-v2-0002](../../docs/rfc-v2/0002-tensor-ir-and-shared-query-data.md) と [RFC-v2-0003](../../docs/rfc-v2/0003-model-onboarding-and-artifacts.md)。登録は不変なIdentityの記録であり、モデル資格審査・階層昇格・claim認証の代替ではない。

`fetch_hf_checkpoint.py` は完全commit SHAを指定したHF checkpointのデータだけを取得する。全計画の容量上限を重み取得前に検査し、各fileの長さとLFS SHA-256（通常のgit fileはblob SHA-1）を照合する。これはデータの同一性確認である。失敗時は `source-manifest.json` の `complete` がfalseのまま残る。出力先には新しいdirectoryを使う。

```sh
python3 scripts/models/fetch_hf_checkpoint.py \
  --repo OWNER/MODEL --revision FULL_40_CHARACTER_COMMIT_SHA \
  --out /absolute/path/checkpoint --max-bytes 2000000000

palw-class preflight --network testnet-12 /absolute/path/checkpoint --depth headers --json
palw-class pack build --model /absolute/path/checkpoint \
  --out /absolute/path/model.palwtir --pack /absolute/path/runtime-pack \
  --calib /absolute/path/calibration-tokens.json \
  --repo OWNER/MODEL --revision FULL_40_CHARACTER_COMMIT_SHA \
  --context 64 --max-window 64 --declare testnet-12:max-context=64
palw-class pack verify /absolute/path/runtime-pack --model /absolute/path/checkpoint --json
```

64は受入試験用の明示したプロファイル。全重みを変換するが、元モデルの最大文脈長の対応を意味しない。異なるscopeのArtifact・class IDを同じものとして扱わない。必要なcontext、calibration、資源境界を対象モデルに合わせて指定し、変換・pack検証・登録・claim認証を別々に確認する。

署名・登録には既存の `misaka model add --artifact <declared.palwtir> --pack <pack> --export-bundle <file> --payer-address <addr>` → `model sign` → `model submit`、または `misaka palw tir-registration` → `palw submit-object` を使用する。接続先の規則・有効化・bond・資金・署名者が必要であり、この取得ツールは送信しない。

## 実Artifactでのprivate受入試験

以下はテスト用の鍵・private genesisで実際のcarrier、mempool、block、保存済みregistry、別node再生を確認する。公開networkへの登録・有効化や採掘・報酬・SecurityCertifiedを成功と数えない。

```sh
cargo run --locked --offline -p misaka-palw-sdk --example export-tir-registration-fixture -- \
  /absolute/path/declared.palwtir /absolute/path/fresh-fixture.json
RUST_MIN_STACK=67108864 MISAKA_HF_REGISTRATION_FIXTURE=/absolute/path/fresh-fixture.json \
MISAKA_HF_REGISTRATION_REPORT=/absolute/path/fresh-report.json \
  cargo test --locked --offline -p kaspa-consensus --lib \
  g14_downloaded_hf_artifact_registration_mined_and_replayed -- --ignored --nocapture
```

取得ツールの検査: `python3 -m unittest discover -s scripts/models -p 'test_*.py' -v`。

## 一括実行

`onboard_hf_checkpoint.py` は固定revisionのconfigから宣言contextを読み、**全重み取得前**のheader/shape preflight、容量上限付き取得、全重み変換、3実装conformance、layout宣言を順に実行する。sourceのcontextを推測・縮小しない。曖昧なcontext、非対応の演算や資源超過は失敗として残す。calibration token列は利用者が指定する。

```sh
python3 scripts/models/onboard_hf_checkpoint.py \
  --repo OWNER/MODEL --revision FULL_40_CHARACTER_COMMIT_SHA \
  --calib /absolute/path/calibration.json --out /absolute/path/fresh-run \
  --palw-class /absolute/path/palw-class \
  --fixture-exporter /absolute/path/export-tir-registration-fixture \
  --native-test-bin /absolute/path/compiled-consensus-test
```

後半2引数を指定するとprivate native受入試験まで実行する。指定しない場合、最終状態は `PREPARED_NOT_REGISTERED`、登録は `NOT_RUN`。公開networkへの送信・ユーザー鍵による署名は行わない。

`run.json` に段階ごとのargv・exit code・実行時間とtool/script/calibrationのhash、宣言ArtifactのSHA-256を保存する。全段階が成功し、native結果のclass/root/context/Artifact SHA-256とpackが一致した場合だけ登録をPASSとする。失敗段階のstdout/stderrを残し、timeout時は子process groupを停止する。既存directoryは再利用せず、古い成功結果の混入を防ぐ。

このdriverは既存のadmission・登録実装を接続する。モデル資格審査、tier変更、新kernelの有効化、claim認証の免除は追加しない。

実際の学習済みモデルを扱う場合は `--stream` を指定できる。converterに加え、3実装のconformanceでもtensorを必要時に読み、複数の全量i128展開を避ける。全重み・sourceのcontext・3実装の比較は維持する。[学習済み3モデルの検証記録](../../docs/evidence/models/trained-llm-cohort-2026-10-31.md)。

## 採択済み wide Q24 profile の私設試験

学習済みcheckpointでも自然単位logitがi32 Q24の範囲を超える場合がある。`--logits-bits 64` は別schemeのi64 Q24を明示し、clip・温度変更・context短縮を行わない。driverはこの場合 `private-wide-q24` という別rulesetで宣言し、native試験にも明示的な私設fenceを渡す。公開testnet-12の規則を変更せず、そのnetworkで登録成功したとは報告しない。既定値32・過去packのbytesは維持する。

`--hf-reference /absolute/path/hf-reference.json` で公式実装の同一checkpointのlogitsを照合できる。元の失敗directoryを上書きせず、新directoryで再試験する。

私設native試験の未最適化consensus binaryは大きいasync frameを持つため、driverはtest subprocessの `RUST_MIN_STACK` を未指定時64 MiBにする（既存release検証scriptと同じ）。指定済み値は保持し、run.jsonに記録する。これはテストスレッドの設定で、モデル・context・court予算・合意の上限は変更しない。
