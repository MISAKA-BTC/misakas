# 検証記録 — RFC-0056 受付難易度

対象: [RFC-0056](../../../rfc/0056-global-claim-admission-difficulty.md) / [ADR-0224](../../../adr/0224-global-claim-admission-difficulty.md)。

状態はPROPOSED_DESIGN_NOT_ACTIVATED。実装・実機・公開activationを文書や抽象試験の結果から推定しない。

## 検証対象

- 添付mainとGitHubの基準commit: 2d8212b634880b0967fbf24b00b41936c2322c14。
- [source照合](source-provenance.json)、[保全基準](source-baseline.json)、[変更台帳](changes.json)。
- [独立レビュー](REVIEW.md)、[実コードの入口監査](SOURCE_AUDIT.md)。
- [新しい難易度モデル](check_difficulty_model.py)、[モデル結果](model-results.json)、[文書結果](revision-results.json)。

## 結果

| 検査 | 結果 | 解釈 |
| --- | --- | --- |
| CAD1抽象モデル | 46 PASS / 0 failure / 0 error / 0 skip | 実SHA-256 ticket、Preseal資金・TrialSlot、凍結・seed・ready、prefix、canonical観測、D更新を接続した契約モデル |
| 今回の文書保全checker | PASS | 元1,946文書のうち1,916はbyte一致、30件は許可した統合変更。削除0 |
| 先行RFC-0055資料 | 13件byte一致 | 別枝起点の前回成果をcompanionとして保持。今回の実装済み機能とは扱わない |
| 統合文書・parameter | 20 RFC / 50 ADR維持、旧56項目不変＋新14提案項目 | 新0056/0224はPROPOSED。旧本文は17件の追記pointerを除去して復元可能 |
| 今回の文書のローカル参照 | 保全checkerの対象参照がすべて解決 | external URL、旧companion全体、限定parser外の構文は別範囲 |
| 既存check_docs.py | 947 failures → 947 failures | 追加・削除したfailureは0。missing-doc-file 971、missing-heading 2は前後同数 |
| 既存RFC-v2 integrity | 7 failures → 7 failures | 同一の歴史source不一致7件。新たなfailureなし |
| 既存ADR-v2 integrity | 86 errors → 86 errors | error内容の多重集合が完全一致。新たなerrorなし |

抽象モデルの実行は2026-11-17 15:43:14 JST。本文・scriptのhashが保存済み結果と一致することを保全checkerと独立レビューでも確認した。前回RFC-0055の30試験は今回46件へ合算していない。

元mainでも既存の文書checkerは参照切れ・歴史snapshotの不整合で失敗している。`check_docs.py`自身の古いknown-issues baselineでは930件が「新規」と表示されるが、その930件も今回の変更前から存在し、今回の前後差は0件である。checkerの失敗947件にはリンク以外のfailureも含むため、missing target数とは同一の指標ではない。

旧baseline、旧checker、旧番号本文、歴史appendixを変更して合格扱いにはしていない。比較の件数・failure fingerprint・正確な差集合は[前後比較](validation-results.json)へ記録した。今回の保全PASSは既存全checkerのPASSを意味しない。

## 再現手順

新資料を適用したrepositoryのrootで実行する。

~~~sh
python3 docs/maintenance/revisions/2026-11-17-admission-difficulty/check_difficulty_model.py --stdout-only
python3 docs/maintenance/revisions/2026-11-17-admission-difficulty/check_revision.py --stdout-only
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

上記の`--stdout-only`は保存結果を書き換えず再検査する。抽象モデルをこのoptionなしで実行すると時刻付き`model-results.json`を書き換えるため、その場合は今回の変更台帳のhashも正規の変更として更新してから保全検査を行う。

基準mainと比較する場合は、同じdocs-only範囲と同じcommandで比較する。repository全体のREADMEやdocs外のファイルを追加した実行では対象範囲が変わるため、上記の947/86という件数をそのまま適用しない。

## 未実証

nativeの全入口、codec/meter/shapeの完全性、既存W/receiptとの複合loop、beacon/clockと実包含、実資金・永続DB/journal、実LLMの同一claim経路、最低Nodeの混合負荷、長期の制御安定性、実時間1 BPSは別の検証を要する。今回の成果物を実Nodeが落ちないことの実測証拠にしない。

