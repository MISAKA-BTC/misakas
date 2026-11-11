# Tokenizer資料codecと署名付き断片 — 2026-11-11

コードcommit `207a1f303`。[前段の予約計算](../plain-tokenizer-batch-reservation-2026-11-11/README.md)から、実際のcanonical資料bytes・署名付き内側envelope・断片の復元と意味検査までを接続した。公開T12のnative受理や成功Finalの記録ではない。[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)で維持したclaim資料と同一性確認を実装し、HF取得不能の証明・再試行裁定は追加しない。

## 実装した範囲

- 入力ID列はnative prompt commitmentへ照合する。EOGと生成順のrendering openingは、受理済みFullIds行のexact rows root/ID列と、元claimの登録済みtokenizerへ照合する。body hash一致だけでは完成資料にならない。個数・長さ・ID・Merkle経路・末尾を確保前に検査し、返却資料はheap予約のownerを保持する。
- 内側envelopeはnetwork・claim・scope・branch・case・request root・ordinal/range・資料種別・応答者・操作を新signature contextで固定する。借用decoderはPartや署名をコピーせず、巨大なlength prefixでallocationしない。旧`MaterialTargetV1`のタグ・意味を変更しない。
- Beginが固定した長さ・body hash・応答者・scopeからdraft IDを導き、全Partの正確なoffsetと最終断片長、Finishの同一draftを確認する。欠落・重複・並べ替え・別応答者・別case・hash不一致を拒否してcanonical bodyを復元する。復元だけでは署名検査済み、支払済み、受理済みとは扱わない。
- tokenizer用予約は内側envelopeの実長と外側prefix/tagの5 bytesを下限として計上する。これは外側タグの割当ではない。既存の数値/FullIds経路を維持し、旧の短いMaterialTarget幅で新資料を見積もらない。入力・出力の両decoded operandとscratchをworkspace下限に含め、復元とtyped decodeの二度のbody hashをwork下限へ加算する。完全な署名・native DB・runtime calibrationの上限確定ではない。

メモリ確保容量が要求と異なる場合は`NativeResource`として拒否し、資料不正や計算不正に分類しない。検証は同一性と既存の決定論的照合であり、モデル計算の暗号学的な正しさの証明ではない。

## 検証と証跡

core libを既存shared target、opt-level 0、incremental無効、`-j2`でビルドし、最終source 7本のhashが開始時から変わっていないことを確認した。

| 検査 | 結果 |
| --- | --- |
| native登録fixtureから入力/出力bodyを作り、実ML-DSA署名Begin/Part/Finish、復元、typed照合 | 1件成功 |
| Batch既存回帰と新codec/署名/複数断片の試験 | 33件成功 |
| 登録・native context境界 | 1件成功 |
| 受付pin・旧quote境界 | 1件成功 |
| 合計 | 36件成功、0失敗 |

新body試験はFlat/Merkle入力、不正なcount/tag/長さ/末尾、反復生成ID、NoneとSome(empty)、path形状を検査する。署名試験はnetworkと全scope座標の改変を拒否し、実encoded長と予約下限を照合する。複数断片の試験は欠落・重複・並べ替え・別応答者・本文変更を拒否する。

native統合試験の登録・FullIds行・要求・資金はfixtureである。受理済みFullIds行がなければ、off-stateで作った認証IDだけでは出力bodyを使えないことを検査した。実UTXO支払、公開Open、保存/巻戻し、公開claimの受理を検証したと解釈しない。

[最終build/fixture](tests.log)、[Batch回帰](batch-regressions.log)、[登録](registration.log)、[受付](intake.log)、[source SHA-256](sources.sha256)、[binary SHA-256](binary.sha256)、[機械可読記録](summary.json)を保存した。[初回build/fixture](initial-test.log)も成功したが、[当時source](initial-sources.sha256)は最終版と異なる。最終版ではallocation拒否の分類とworkspace/work下限をレビュー後に修正した。

文書検査は[比較結果](docs-validation.json)、[docsログ](docs-check.log)、[RFCログ](rfc-integrity.log)、[ADRログ](adr-integrity.log)に記録する。既存不備を隠すbaseline変更はしない。

## 次に必要なnative接続

1. `rooted_drawn_batch_case_v1`が確認したadmission/draw/pending requestと、`reserve_protected_v2`のrank予約を結ぶ専用demand rowを保存する。旧`CourtBatchDemand(claim, ordinal)`や先着notice排他を保護caseの代わりにしない。論理caseと、元から予約された物理packet namespaceを区別する。実TFP1/PublicOriginalの永続的な紐付けがなければ受理しない。
2. 外側タグ・新contextを割り当て、`ValidatedBatchCarrierV1`からnative writerへ、実UTXO費用・Active署名者・予約DAA・長さ/期限を検証して渡す。Begin rentをRound/template/coinbaseと同じ控除へ接続する。同じ予約枠への別Beginをdraft IDの違いだけで許可しない。物理intakeの署名/不正候補/duplicate予算も計上する。
3. 受理済みPartだけを復元し、本codecで認証してから完成行の保存とdraft解放を同じbounded journalで行う。Outputでは実FullIds Finishの先行を確認する。2つのcaseの隔離、wrong signer/slot/fee/bodyの原子的拒否、再編によるdraft/Part/charge復元を確認する。

外側wire/native writer・全Selection・資料未着時の回復と終結・全保護枠・公開profileは未接続。公開T12のquote成功/受付/Final/当選時receipt、main pushは未達のまま。今回のinner codecを既存public decoderが受理するようには変更していない。
