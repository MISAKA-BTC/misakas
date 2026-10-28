# G14 constrained service / receipt pagination checkpoint

2026-10-11。ユーザーの「一旦 push」に対応する途中時点の記録。G14 全域の保証は未完成で、activation の判断を変更しない。

## 変更

- wallet の FP commitment builder が v6 constraint job の isolation shape を検証できるようにした。送信 payload と署名は元の v6 claim のまま。height による admission fence は維持。
- False-Valid filer は DAA 差を block 数として扱わず、64 selected-chain blocks ずつ receipt 履歴を読む。未読 suffix がある間は watermark を進めず、読み切った元の tip の DAA を記録する。anchor が reorg で外れた場合は claim の開始点から読み直す。
- service fixture で、input answer を与える悪意ケースにも適用されていた honest-case assertion を、本来の honest case に限定した。producer と全 Panel の conviction 条件は維持。

## 検証と限界

wallet 49 tests、既存 filer 28 tests は PASS。receipt book/scan 12 tests と、plateau・欠損 suffix・reorg を扱う history test 1 件も PASS。normal node check は production pagination 変更後に PASS（6.53 s）。最後の test-only assertion 修正後の node test build は PASS（25.25 s）。大きな honest constraint job の actual service 1 件は pagination 修正前に PASS（369.18 s）。ログと source hash は `evidence/constraint-service-recipe.json` を参照。

先行サービス試験は wallet の v6 refusal、fixture の誤 assertion、producer conviction 後の全 Panel 追及 timeout を発見した。失敗ログを保持し、成功件数に数えていない。pagination 修正後の forbidden / premature stop / beaten token の actual service 3 件は、この checkpoint では実行中であり PASS を主張しない。実行中ログは途中時点の snapshot として保存する。

この fixture は独立 verifier がモデルを保有する ADR-0177 D7 の前提を維持し、合成 token table と手動の公開 answer 提供を使う。実ネットワーク RPC/P2P、producer の自動 answer scheduler、tiled acquisition、上限サイズの terminal proof carriage、全 active family の網羅、全域の時間・RAM・経済条件は未証明。現時点で「producer と全 Panel が共謀しても G14 全域で一人の公開 verifier が conviction できる」とは評価できない。
