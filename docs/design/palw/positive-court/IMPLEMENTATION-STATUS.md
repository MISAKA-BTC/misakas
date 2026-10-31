# Positive Court — A0の実装状態

2026-10-31。ユーザー指定のA0（共通仕様・統合）用workspace。

- branch: `codex/positive-court-a0`
- base: `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`
- 契約: [v2/r1](COMMON-CONTRACT.md)、[担当分担](WORKSTREAMS.md)、[vector](contract-vectors-r1.json)
- 採択した実装方針: [RFC-0044](../../../rfc/0044-positive-court-full-coverage.md) / [ADR-0212](../../../adr/0212-positive-court-without-model-prepossession.md)。具体的な本番profile・公開有効化は別。

## A0で追加したもの

独立した `misaka-palw-positive-contract` crateに、Miner結果とPanel証拠の別型、case/result/claim/witness/request/planのID、実ML-DSA-87の役割別認証、登録rootとbranch/rulesetの照合、有限資料契約、bounded framing、六系統の接続portを置いた。署名付きframe認証、循環しないpayer funding scope、176番object・aux151・P subjectの予約、固定row valueとtyped readerを追加した。[native契約](NATIVE-CONTRACT-R1.md)を正本とする。公開有効化は行っていない。

これはnative受付・実資金hold・支払・IBDの完成報告ではない。A1/A3の実演算検査・Panel生成経路、A2初期有限応答、A5会計/Node bridgeを選択統合した。各候補の検証を全機能が同じcandidateで通った実績へ合算しない。共通型の状態enumをnativeの合格権限として扱わない。[担当別の現状](../../../../handoff/positive-court/A0-integration-status.md)を参照。

## 前workspaceとの関係

進行していた試作は元checkoutに保持し、hashを [記録](a0-workspace.json) した。元workspaceの未コミット作業・他チャットの変更をA0 branchへ一括コピーしなかった。試作のテスト結果をA0共通契約の統合成功へ流用しない。

## 検証記録

初期A0契約13テストの記録は保持し、row追加後の共通契約29テストとClippy成功を別snapshotで保存した。[検証記録](VALIDATION-A0.md)を参照。文書検査は既知43件、新規欠落0件。ZIP記載の42 Python参照テストは入力資料の検証であり、このRust contractやnative runtimeの完成を保証しない。実LLM性能の測定値はまだない。公開網有効化も行っていない。
