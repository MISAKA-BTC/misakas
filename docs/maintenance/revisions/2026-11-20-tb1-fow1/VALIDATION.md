# TB1・FOW1改訂の検証

2026-11-20（Asia/Tokyo）。対象は文書整合性・過去bytesの保全と添付参照モデルの検査である。本番BGW、PQ暗号、native実装、Qwenの費用定理や公開activationの検証ではない。

## 結果

| 検査 | 結果 | 解釈 |
| --- | --- | --- |
| 添付TB1 fixture | 29 / 29成功 | 参照モデルの状態・復元・certificate構造等。完全BGW/PQ/nativeを実装していない。 |
| 独立算術検査 | 103,152ケースで偽多項式の支持数がqに届かない | 小さい有限体上の列挙。上の29テストや本番暗号の件数に合算しない。 |
| 原文・旧証跡の保全 | 1,847ファイルSHA-256一致 | 入力から変更しない原RFC/ADR、履歴、既存改訂記録等。既に存在した不整合を修復済みとする結果ではない。 |
| 現行冊の旧本文 | 20 RFC-v2＋50 ADR-v2を保全 | 新適用節を歴史payloadの外へ追加。追加節と挿入用空行を除く本文のhash一致を確認。 |
| 番号付き文書の対応 | 359件すべて収録 | 元355件と今回の4件。既存の欠番・重複番号を保持。 |
| 旧乱数関連記述の対応 | 元536文書・5,252箇所を収録 | 元path・行・節・引用・SHAと新profileの適用区分を保持。意味論・暗号の自動証明ではない。 |
| check_docs | **FAIL、947→947** | 失敗集合に新規差分なし。旧evidence未同梱等の既存問題を維持。 |
| RFC integrity | **FAIL、7→7** | 失敗集合に新規差分なし。変更前の原文不整合を維持。 |
| ADR integrity | **FAIL、86→92** | 既存86件を保持。追加6件は今回明示したscope追補による旧「不変」判定。 |
| 今回専用の改訂検査 | **PASS** | 上記保全、全件台帳、exactな6追補、差分集合、参照コード原本一致を確認。旧checkerのFAILをPASSへ読み替えない。 |

ログとJSON: [docs](validation/docs.json)、[RFC](validation/rfc.json)、[ADR](validation/adr.json)、[今回専用検査](validation/revision-check.json)、[fixture](validation/fixture.stdout.log)、[算術](validation/arithmetic.stdout.log)。変更前は[checker-baseline](checker-baseline/summary.json)。

## ADR checkerが追加検出した6件

旧checkerは2026-10月の不変ファイル集合も検査するため、以下の今回の採択追補を検出した。

| 対象 | 今回追加した内容 |
| --- | --- |
| governance/design-freeze.md | 明示採択した新profileの優先関係 |
| governance/document-lifecycle.md | 新規方式と歴史文書の適用範囲 |
| spec/palw/17-model-improvement.md | 延期・旧方式を新profileで復活させない範囲 |
| spec/palw/18-layer-sharded-panels.md | 新TB1/FOW1の選出・完全検証を優先する範囲 |
| spec/palw/18-verification-certificates.md | 旧certificateを新資格へ自動昇格しない範囲 |
| spec/palw/claim-certification.md | FOW1適格性・TB1・PublicOneHonestをFinalへ接続する範囲 |

[adoption-integrity.json](adoption-integrity.json)は、この6pathだけに対しbefore/after SHA、追補hash、marker数と位置、旧本文hashを固定する。6件とも追補と挿入用空行以外の旧本文は一致する。独立の読み取りレビューでも一致を確認した。任意のファイルからmarkerを除去すれば免除できる仕組みにはしない。

旧ADR checker、source-manifest、過去post-integration変更記録、元baselineは変更していない。追加6件を旧baselineへ移して隠したり、旧checker全体を合格と表示したりしない。

## 統合レビュー

TB1担当とFOW1担当を分離し、別担当の読み取りレビューで接続を確認した。静的なモデルdescriptorと動的な試行C、RoundRefとFreezeRecord、32/64byteの型幅、z/ReadoutとRootの非循環性、外部安全性仮定とartifact適合前提の区別を修正した。

EXEC/PANELの別秘密・公開前固定、SCHEDULE/ROLEの分離、追加Ticket・FOW1用ADMISSION抽選の不採用、旧受理Courtに新beacon待ちを加えないこと、旧入口の閉鎖とdrainの優先関係について、未解消の文書統合上のblockerは報告されなかった。このレビューは安全性定理・本番実装の認定ではない。

追加の現行入口レビューは[followup-scope-audit.json](followup-scope-audit.json)へ記録し、優先17入口に追補した。測定報告・旧wire vector・鍵seed・試験PRNGは新beaconへ書き換えない。

## 再実行

展開後の`docs`を含むディレクトリから実行できる。旧checkerの失敗は上表どおり残る。

```sh
python3 docs/maintenance/check_docs.py --root . --json-out docs/maintenance/revisions/2026-11-20-tb1-fow1/validation/docs.json
python3 docs/adr-v2/check_integrity.py --root . --json docs/maintenance/revisions/2026-11-20-tb1-fow1/validation/adr.json
python3 docs/rfc-v2/check_integrity.py
python3 docs/maintenance/revisions/2026-11-20-tb1-fow1/check_revision.py
```

fixtureの再実行は`docs/reference/palw-tb1/v1`を作業ディレクトリとし、`python3 test_beacon_model.py`と`python3 bgw_certificate_check.py`を使用する。今回専用検査は同梱ログ・結果も参照する。RFC checkerを再実行して結果を更新する場合は、そのstdout/stderrを同じvalidation内の対応ログへ保存する。

## 未検証の範囲

完全BGWの全phase、PQ認証private channel、悪意holderの実署名、native broadcast/包含/reorg、全purposeの容量・曝露journal・cold recovery、具体LLMのG-COLL/G-COST・有界適合checker、公開activationは未完成である。これらは本改訂の文書検査・fixtureから保証されない。
