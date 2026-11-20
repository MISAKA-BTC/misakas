# 入力・番号・検証の来歴

2026-11-20（Asia/Tokyo）。この改訂はユーザーが提供した文書とTB1 bundleを対象にした。

| 入力 | SHA-256 | 用途 |
| --- | --- | --- |
| misaka_docs_latest (2).zip | `3cfb64e7eb0d5406d0df631d0386f8f91f152ba0e4119d29fee8ac6e2e74d4e8` | 元の全文書一式。今回の変更前検査の基準。 |
| Misaka_PALW_TB1_Design_Bundle(1).zip | `f94c88ea686fb8899decef808b856209750f1f8900f5dd7811fee81d7e9c5697` | 今回指定された完全BGW/PQ方式、参照fixture、レビューの原本。 |

TB1入力は543,307 bytes、ZIP内39ファイル。入力の提案RFC0060は既存MOR1と衝突したため、統合版のRFC0061/ADR0229へ割当てた。FOW1の共通関数・仕事契約はRFC0062/ADR0230とした。native wire IDやactivation heightの割当を意味しない。

[入力TB1原本](upstream/Misaka_PALW_TB1_Design_Bundle.zip)は、当時のDRAFT表示、番号、レビュー、fixture出力を含めbyte一致で保持する。本文を統合する際に追加・変更したROLE/SCHEDULE/条件付きADMISSION、DrawPolicy区別、全consumer移行は、元の29テストで検証済みとしない。

## 変更前検査

元文書へ今回の変更を加える前に三つの既存checkerを実行した。最初からcheck_docsは947件、RFC integrityは7件、ADR integrityは86件の失敗を報告していた。

詳細は[baseline summary](checker-baseline/summary.json)、[docs](checker-baseline/docs.json)、[RFC](checker-baseline/rfc.json)、[ADR](checker-baseline/adr.json)を参照する。未同梱evidence参照や既存source整合性の失敗を、今回の設計変更が原因であると混同しない。一方で、今回増えた不整合はbaselineへ移して隠さない。

## 検査の意味

文書リンク・番号・保護された原文bytes・新規回帰の検査と、参照fixtureの実行を行う。これらは本番の完全BGW、PQ暗号、native consensus、LLM費用下界の証明ではない。[最終検証結果](VALIDATION.md)に、それぞれの範囲を記録する。
