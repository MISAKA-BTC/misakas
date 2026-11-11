# 対象TFP1に束縛した資料case — 2026-11-11

[native保存リンク](../tfp1-native-original-link-2026-11-11/README.md)を、受理済みのPublicOriginal・Panel抽選・署名付き資料要求へ接続する。[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)のclaim固有資料の実装であり、Minerにモデル重み全量の配信を要求しない。

## 変更

実取引のTFO3 pairingから再構成した値を、保存済みclaim/Source・元companion・literal FP digest・profile/resource/義務・admission予約へ照合する。署名companion単体、同一claimを載せた別carrier、未受理のdraw/requestは使用できない。初期保存と違い、後続caseの読込みは現在のescrow outpointを固定しない。

その上で既存のdraw/request検査を行い、native claim/Source、case ID、選出bondの予約rankを一つの型で返す。claimやscopeを要求者の入力からそのまま採用しない。新profile専用scopeは署名前に固定されるclaim/Source/OriginalId/profile/resource/義務から導き、caseごとのscopeはdraw/rank/caseを加えて導く。受理blockや署名bytesを元scopeへ混ぜず、旧共有contextや先着noticeのpacket IDを読み替えない。

結果はそのbranchの読込み結果であり、持ち運べるwire証明ではない。native writerは現在のprefixから再導出し、物理的な予約枠・費用・期限を別途確認する必要がある。

## 検証

コードcommit `026fb6335`。既存shared target、core libのopt-level 0、incremental無効、`-j2`でビルドした。対象1件とnative writer回帰13件、計14件が成功した。未受理draw/request・署名companion単体・別carrier・改変署名の拒否、caseの巻戻し後の拒否、snapshot復元後の一致を確認した。初回はテストのmove済み変数参照でコンパイルが失敗し、移動前にcase IDを取得するよう修正した。

[対象テスト](tests.log)、[関連回帰](regressions.log)、[初回コンパイル失敗](initial-build.log)、[source hash](sources.sha256)、[機械可読記録](summary.json)を保存する。テスト内で実ML-DSA署名を検証する対象はcompanionと資料要求である。FPのnative受理frame・資金・抽選はfixtureであり、FP署名の実UTXO検証や公開T12での受理を証明しない。

文書検査は[比較結果](docs-validation.json)、[docs](docs-check.log)、[RFC](rfc-integrity.log)、[ADR](adr-integrity.log)に記録する。

## 残件

資料要求の権限照合までを接続した。production coordinatorでの完全なTFO3受理、全保護caseの物理予約と実費、tokenizer応答の外側wire/native保存、case終結と有界archive、公開quote/受付/Final/当選時receipt、main pushは未達。現在のAPIを呼ぶだけで資料到着や実再計算の成立を扱わない。
