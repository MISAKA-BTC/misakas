# TFP1とPublicOriginalのnative保存リンク — 2026-11-11

[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)で維持したclaim固有資料の接続作業。モデル重みの独自配信やHF取得不能の認定を追加しない。[前段のtokenizer資料codec](../tokenizer-material-codec-2026-11-11/README.md)を、別claimの資料契約やcaseで使用しないための保存基盤である。

## 実装範囲

新しいkernel aux table 204に、native claim/Source、FP allocation/rules/resource、PublicOriginal、受理carrier、予約と義務の識別情報を固定長1,050 bytesで保存する。これはaux table番号であり、既存のlifecycle wire tag 204の意味を変更しない。

実TFO3 pairingからだけ作れる専用のprepared値をwriterが要求する。署名済みcompanion単体では保存できない。同じclaim・同じ受理block/DAAでも、native escrowのbirth outpointに一致しない別carrierを拒否する。初期保存時は未消費escrowも照合し、保存後の読込みは決済で変化する現在outpointに依存しない。

汎用aux writerからの注入・上書き・削除は拒否し、専用writerはメモリとjournalの予約後に一度だけ書く。journalの適用/巻戻し、snapshotの有界preflightと復元が新行を扱う。読込み時にnative frameとPublicOriginal admissionの対応を再確認する。

リンクの存続中は参照元claimと資料を先にGCしない。現在のholdは終結に伴う有界release/archiveへ未接続であり、この状態で公開有効化しない。

## 検証

コードcommit `1c6b34697`。既存shared target、core libのopt-level 0、incremental無効、`-j2`でビルドした。対象のTFO3 pairing/native link試験1件、native writerの関連回帰13件、合計14件が成功した。

同一claimを持つ別carrier、署名済みcompanion単体、異なる受理block、汎用writerからの注入を拒否する。専用writerの冪等性、資源不足時の状態不変、journalの適用/巻戻し、snapshot preflight/復元、参照claimの欠落・受理block改変・リンク削除の拒否を確認した。関連回帰には旧PublicOriginal保存と不正な保存行・deltaの拒否を含む。

検証結果は[機械可読記録](summary.json)、[対象テスト](tests.log)、[関連回帰](regressions.log)、[source SHA-256](sources.sha256)に記録する。[初回のコンパイル失敗](initial-build.log)も保持する。

試験のnative frame・PublicOriginal admission・escrowはfixtureである。実署名とliteral TFO3 pairingを使うが、実UTXO支払を伴う公開受付の証明ではない。

文書検査は[比較結果](docs-validation.json)、[docsログ](docs-check.log)、[RFCログ](rfc-integrity.log)、[ADRログ](adr-integrity.log)に記録する。既知不備を隠すbaseline変更はしない。

## 未接続の範囲

production coordinatorのTFO3は引き続き完全profile未設定として拒否する。frame・予約book・escrow・PublicOriginalの同一checkpointでの実受理、リンクからdraw/requestへ続く保護caseの権限導出、tokenizer資料の外側wireと実支払writer、有界release/archiveは未完成。リンク行だけをcaseの実行権限として扱わない。

公開T12のquote成功・同一claim受付・Final・当選時receipt、burn停止制度の公開有効化、main pushの達成ではない。
