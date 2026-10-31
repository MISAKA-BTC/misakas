# HFモデル登録経路の一括実行と追加抽出 — 2026-10-31

規範入口: [RFC-v2-0002](../../rfc-v2/0002-tensor-ir-and-shared-query-data.md) / [RFC-v2-0003](../../rfc-v2/0003-model-onboarding-and-artifacts.md)。[前回のGPT-Neo-125Mの記録](hf-registration-2026-10-31.md)に続く確認。

## 実装

[一括driver](../../../scripts/models/onboard_hf_checkpoint.py)を追加した。取得前のheader/shape preflight → 固定revision・容量上限・file identity付き取得 → streaming変換・runtime pack・3実装conformance → 元モデルの最大contextでのlayout宣言 → 任意指定のprivate native受入試験を接続する。

contextは元configの明示値を使用し、推測や縮小はしない。対応していない演算や資源上限で失敗すればその段階で止める。calibrationは入力データとして固定する。最終的なnative結果はpackのclass ID・inventory root・context、および実ArtifactのSHA-256と一致し、mempool／登録block／保存済みregistry／独立node再生の全段階が成功している必要がある。

各段階のargv・stdout/stderr・exit code・所要時間、tool/script/calibrationのhashを `run.json` に記録する。既存出力を再利用せず、timeoutではprocess groupを止める。native引数を省略した場合は `PREPARED_NOT_REGISTERED` / 登録 `NOT_RUN` と表示する。

[使用手順](../../../scripts/models/README.md)。モデル資格審査・tier変更・新kernel有効化・claim認証の免除は導入しない。

## 抽出条件

前回保存したHub上位200件のsnapshotから、公開・CausalLM・safetensorsパラメーター数2千万以下、前回のGPT-Neoを除外した候補を用いた。抽出前に生成したseed `2125632823003917137` で選び、[候補・revision・結果](hf-registration-round2-2026-10-31/selection.json)を保存した。

対象は `trl-internal-testing/tiny-Qwen2ForCausalLM-2.5`、revision `ce8d0bf270b28c8fab026ced69fe8aa14b0f0eda`、sourceの宣言contextは **32,768**。小型の検証用モデルであり、学習済み大型モデルの精度・運用実績やHub全体の対応率の根拠にはしない。

## 確認結果

**最終runはPASS。** 取得前preflight、取得、pack生成、fixture導出、native登録・別node再生の5段階が成功した。最終runの結果は [run.json](hf-registration-round2-2026-10-31/run.json)、[native受入結果](hf-registration-round2-2026-10-31/native.json)、[pack](hf-registration-round2-2026-10-31/pack.json) に記録する。

別途、前回拒否されたBertForMaskedLMの固定revisionでもdriverを実行した。CausalLM経路のpreflightでexit 2となり、checkpoint directoryを作らず停止した。[失敗記録](hf-registration-round2-2026-10-31/rejected-run.json)。取得や登録の成功には数えない。

driverと取得処理の11試験（source contextの推測拒否、誤ったclass/root/contextの拒否、非zero exit、timeout、既存出力拒否、取得上限・破損検出等）はPASS。[試験ログ](hf-registration-round2-2026-10-31/tests.log)。

## 範囲

native受入試験は前回と同じprivate testnet-12 launch規則＋IR fenceのテストharnessで、テスト鍵による実carrier・mempool・block・永続registry・別node再生を扱う。公開networkへの送信、現行公開presetでの有効化、producer claimのSecurityCertified・Final・報酬は今回の成功範囲に含めない。

宣言context 32,768全域の静的admissionと、短いconformance jobの実行一致は別である。全context全位置の再実行や、独立HF Transformersとの出力忠実度評価を完遂したという意味ではない。変換・登録の標準経路を接続した確認であり、RFCのHub全体／90% coverage目標を達成したとの宣言でもない。

データ・Artifact・runtime pack・全logは `/Users/wata/Downloads/misaka-hf-registration-round2-20261031/` に保存する。
