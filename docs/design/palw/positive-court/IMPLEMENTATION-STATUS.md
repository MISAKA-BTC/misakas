# Positive Court — A0の実装状態

2026-10-31。ユーザー指定のA0（共通仕様・統合）用workspace。

- branch: `codex/positive-court-a0`
- base: `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`
- 契約: [v2/r1](COMMON-CONTRACT.md)、[担当分担](WORKSTREAMS.md)、[vector](contract-vectors-r1.json)
- 履歴提案: [RFC-0044](../../../rfc/0044-positive-court-full-coverage.md) / [ADR-0212](../../../adr/0212-positive-court-without-model-prepossession.md)

## A0で追加したもの

独立した `misaka-palw-positive-contract` crateに、Miner結果とPanel証拠の別型、case/result/claim/witness/request/planのID、実ML-DSA-87の役割別認証、登録rootとbranch/rulesetの照合、有限資料契約、bounded framing、六系統の接続portを置いた。公開presetやnative wire tagは変更していない。native activationは拒否する。

これは演算検査器・証拠生成器・native受付・資金hold・支払・IBDの完成報告ではない。それらはA1〜A7の担当として [受入基準](WORKSTREAMS.md) を固定した。共通型の状態enumをnativeの合格権限として扱わない。

## 前workspaceとの関係

進行していた試作は元checkoutに保持し、hashを [記録](a0-workspace.json) した。元workspaceの未コミット作業・他チャットの変更をA0 branchへ一括コピーしなかった。試作のテスト結果をA0共通契約の統合成功へ流用しない。

## 検証記録

A0契約の13テスト、Clippy（warningsをerrorとして検査）、workspace登録確認が成功。[検証記録](VALIDATION-A0.md) とraw logを保存した。文書検査は既知43件、新規欠落0件。ZIP記載の42 Python参照テストは入力資料の検証であり、このRust contractやnative runtimeの完成を保証しない。実LLM性能の測定値はまだない。公開網有効化も行っていない。
