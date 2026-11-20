# 2026-11-17 — 全claim共通の受付難易度CAD1

状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。ユーザー依頼に基づく具体化済みの設計案。正式採択・native実装・性能測定・公開activationを完了扱いにしない。

## 主題と成果物

前回のRFC-0055はNodeへ提出するcourtの資源上界と全工程予約を扱う。今回のRFC-0056は、全新規claimが重い検証責任を作る前に候補量を絞るD_admitを独立して規定する。採択新profileでは55§6をCAD1へ詳細化し、二つの受付Dを同時には掛けない。

| 文書 | 内容 |
| --- | --- |
| [RFC-0056](../../../rfc/0056-global-claim-admission-difficulty.md) | ticket、TrialSlot、凍結、seed/ready guard、全route、観測、整数controller、精算・復旧 |
| [ADR-0224](../../../adr/0224-global-claim-admission-difficulty.md) | 提案の理由、維持する規則、棄却構成、採択/実装/activationの境界 |
| [実装接続表](../../../design/palw/integration/claim-admission-difficulty-implementation-plan-2026-11-17.md) | mainの実在入口からの接続順と完了証拠 |
| [ソース監査](SOURCE_AUDIT.md) | 現行W・FP receipt・atomic予約・court budgetの存在と不足 |
| [レビュー](REVIEW.md) | 是正した反例と実装/実機に残る条件 |
| [検証記録](VALIDATION.md) | 抽象試験、文書保全、既存checkerの前後比較 |

## 重要な規則

- D_admit、REALのW、Final後receipt/経済資格、外側1 BPSを別state・別目的として扱う。
- raw RPC数、未資金・不正packet、host CPU/RAMを難易度入力にしない。
- 現在のfuture book、canonical消費flow、live-state stock peakを共通claim用容量へ正規化する。
- NoCapacityは、資金・資格・shape・期限が有効で空claim-bookなら収容可能な候補の既予約競合に限定する。
- incompleteな観測はlowにしない。確定したhigh根拠があれば増加できる。
- 実行機会と結果commitを分離し、同一slotの結果/trace/route/model名差替えで再抽選しない。
- 次Dは120秒境界bのcollection前にb+2へ公表する。既受理反証・応答・終端は新Dを免除し、元予約で継続する。
- Dの追従遅れ・最大値・不運なticket集中時も、RFC-0055のhard上限で新規受理を制限する。

## sourceと先行成果物

基準は添付misakas-main(1).zipとGitHub main@2d8212b634880b0967fbf24b00b41936c2322c14。[照合記録](source-provenance.json)にarchive hash、tree、対象3,582 blobと追加RPC 1 blobの一致を保存した。[source-baseline.json](source-baseline.json)は元mainのdocs 1,946ファイルと既存parameter 56 recordの基準である。

同会話で提供済みのRFC-0055/ADR-0223等13ファイルは、元のbytesで併録した。これらの基準はpre@c3da7c6eであり、今回mainのruntime進捗へ読み替えない。先行revisionのcheckerや試験hashは当時のsnapshotの記録として保持する。今回の保全判定は下記の新checkerを使う。

旧numbered RFC/ADR・歴史付録・元mainの旧baselineは変更しない。現行20 RFC／50 ADRの主題別冊数を維持し、今回の文書はpost-consolidation記録として登録する。

## 再現コマンド

~~~bash
python3 docs/maintenance/revisions/2026-11-17-admission-difficulty/check_difficulty_model.py --stdout-only
python3 docs/maintenance/revisions/2026-11-17-admission-difficulty/check_revision.py --stdout-only
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

[抽象モデル](check_difficulty_model.py) / [保存結果](model-results.json) / [文書結果](revision-results.json) / [既存checker比較](validation-results.json) / [変更台帳](changes.json)。

抽象モデルはnativeの上界式・planner・beacon・DBを実装し直したものではない。対応するfixture/adaptorの前提を結果に記録する。1万/5万model IDの試験は共通D/予算がmodel数で増えない性質を検査し、実registry性能や実LLMの負荷試験を代替しない。旧RFC-0055の30試験を新Dの合格証拠へ転用しない。

