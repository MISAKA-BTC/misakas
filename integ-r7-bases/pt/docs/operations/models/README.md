# モデルの変換・固定登録

先に[モデル階層](../../spec/palw/model-tiers.md)を読みます。登録時から固定0.3、年次指定でのみ1.0、旧モデル資格審査なしという採択済み改定を優先します。

[変換・追加runbook](palw-add-a-model-runbook.md) / [SDK](palw-model-onboarding-sdk.md) / [モデル要求](model-requests.md)。旧testnetの説明・コマンドを含むため対象releaseで確認します。

[ADR-0075系のfamily/class認証資料](../../history/operations/model-certification/palw-certify-a-new-model.md)は版依存の保存資料です。kernel conformance、旧モデル資格審査、claimのGate Cを同一視しません。

RFC-0004の改善protocolはmainnet後へ延期・無効です。普通の固定登録・不変性まで停止せず、画像・動画profileを利用可能にする入口も開きません。[正式方針](../../governance/post-mainnet-scope.md)。
