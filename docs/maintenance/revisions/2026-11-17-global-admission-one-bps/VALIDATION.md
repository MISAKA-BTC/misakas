# 検証記録 — 2026-11-17 全モデル共通受付

この記録は文書・抽象契約・元preからの増分に限定する。Rust/native、実Node、Studio、実LLM、最低ハードウェア、実時間1 BPS、公開activationは未検証である。

## 1. 対象と基準

- source: pre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398。
- 設計: [RFC-0055](../../../rfc/0055-global-admission-court-calendar-and-one-bps.md) / [ADR-0223](../../../adr/0223-global-admission-court-calendar-and-one-bps.md)。
- sourceのdocsテキスト1,367 blobを元Git SHAと照合したうえで編集した。[source-baseline.json](source-baseline.json) と [changes.json](changes.json) を分けて保存した。
- 最終本文と抽象scriptのhash、試験時刻・ケース・scope: [model-results.json](model-results.json)。
- 文書増分の機械結果: [revision-results.json](revision-results.json)。既存checkerの前後比較: [validation-results.json](validation-results.json)。
- 独立レビューと残条件: [REVIEW.md](REVIEW.md)。

## 2. 結果

| 検査 | 今回の結果 | 意味 |
| --- | --- | --- |
| 抽象契約モデル | 30 / 30 成功。失敗・error・skipは0 | 有限モデルの反例・境界。nativeの実装/性能証明ではない |
| revision専用チェック | 7群で文書保全・登録・params・新規リンク・モデル記録を照合 | 完全な結果はrevision-results.json。runtime非実行 |
| 既存check_docs.py | 基準preと今回の両方がFAIL。今回追加の失敗0 | 旧checker基準外の参照問題931件は前後同数。旧index staleの2項目は再生成で解消 |
| 既存RFC-v2 check_integrity.py | 両方FAIL、同じ7項目 | 元commitに既にある歴史source hash不整合。新規の不整合0 |
| 既存ADR-v2 check_integrity.py | 両方FAIL、同じ86項目 | 元commitに既にあるsource/歴史baseline等の不整合。新規の不整合0 |

check_docs.pyの参照集計は前後ともmissing-doc-file 972、missing-heading 2。そのうち43 occurrenceは旧checkerの既知allowlist、残り931は元preにも存在した。これを今回作成した参照切れとは扱わず、全体checkerが合格したとも報告しない。外部URLとdocs外sourceリンクは、この文書checkerの検証範囲外である。

旧baseline/manifest/checkerを変更して緑にしていない。上表の既存checkerの終了コードはすべて1のまま保存し、増分の合否と別に記録した。

## 3. 抽象モデルで検査した範囲

固定件数を撤去し、実vectorと期限により受付量が変わること、未知model IDを増やしても予算が増えないことを確認した。同じ試験用容量で小証拠10件/重証拠2件になる例、異種vectorの合算、120秒総量内でも最後の1msへ集中すれば不成立となる例を含む。

preflightとlease、複数actorの救済、無効packetによる他人の予約の非消費、原子的rollback、同batch反証優先、整数D/二epoch先公表、bootstrap、可変Lのcarrier総寿命も検査した。

試験用容量・10件/2件・Lの例を、本番court数や物理性能の採択値にしない。Pythonの抽象構造・deepcopyをnativeのメモリ安全性や永続DBの証明へ昇格しない。一万/五万modelの例はID非依存の会計則を検査し、実registryのスケール性能を測っていない。

## 4. 独立した保全照合

旧番号付きRFC55本、旧番号付きADR218本、ADR-v2本文50冊は元preのbytesと一致する。RFC-v2の18冊はタイトル直後の新版入口追記のみで旧本文を完全保持し、0001/0004は変更しない。PRINCIPLESは新版入口と§15、parameter-registerは末尾追記で旧本文を保持した。

既存parameter 56 recordの内容・順序・metadataは完全一致し、新規14 recordのみ追加した。既存registryのdocumentsとpost_consolidation配列prefixも保持した。変更はdocs内に限定し、Rust・runtime設定・activationの変更はない。

## 5. 実装・実機の残条件

共通checkerの意味保存・構造上界とmeter一致、nativeのpreflight/lease所有、全新規routeとStudio/Panel/権利消費の同一claim通し、全合法継続のplanner/ClockBridge、最大registryと保持/GC/復旧混合負荷、旧責任移行、実包含が必要である。

cold Panelの取得・全体再実行、provider供給と役割間競合も確認する。局所checkerが軽いだけでPanel readinessを保証せず、全拒否・全Abortだけで実用完成にしない。[実装計画](../../../design/palw/integration/unified-admission-implementation-plan-2026-11-17.md)の各完了証拠を満たすまで有効化しない。
