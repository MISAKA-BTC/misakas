# 2026-11-08改訂の検査結果

対象: 今回添付されたdocs(2).zipから作成した文書更新版。
[変更記録](README.md) / [現行RFC20本](../../../rfc-v2/README.md) /
[変更ファイル台帳](changes.json)。

**今回入力に対する文書保全・目録・対応表・リンク回帰の7検査はPASS。**
旧snapshotを基準にする既存監査はFAILを残す。
それらを同じ判定にまとめず、以下に範囲と理由を記録する。
Rustのbuild、native接続、実ノード、実推論、資源計測、E2E、activationの試験は今回実施していない。

## 1. 今回入力を基準にした検査

実行:

~~~sh
python3 docs/maintenance/revisions/2026-11-08/check_revision.py   --json docs/maintenance/revisions/2026-11-08/check-results/current-revision.json
~~~

| 検査 | 結果 | 確認した範囲 |
| --- | --- | --- |
| 入力ファイルの存在 | PASS | 1840ファイルをすべて保持。削除・消失なし |
| 現行RFCの目録 | PASS | RFC-v2-0001〜0020の20冊、ファイル名・ID・registryが一致 |
| 旧番号原文・ADR冊子 | PASS | 旧RFC48ファイル、旧ADR210ファイル、統合ADR50冊のbytesが今回入力と一致 |
| 歴史台帳・snapshot | PASS | 48の保護対象ファイルのbytesを保持。既知リンクbaselineを更新していない |
| RFC原付録 | PASS | 41原文からの89収録区間の割当・範囲・digestを維持 |
| 現行担当表 | PASS | 48の旧RFC完全パス、入力hash、owner、関連先、人間用対応表が一致 |
| 後続採択の登録 | PASS | 基準日後のRFC7件・ADR6件と新ADR0215を登録。新RFC16〜20もADR側から参照可能 |
| 入力に対するリンク回帰 | PASS | 新規・増加した問題は0件。既存問題は1540→974件 |

表は7個の実行検査の一部を読みやすく分けて表示している。
[機械可読の最終結果](check-results/current-revision.json)が検査名と詳細を持つ。
[入力manifest](input-manifest.json)と[入力リンク問題](input-link-issues.json)のhashも検査器に固定している。
一般の既存ファイル変更44件は、承認された文書改訂として新旧digestを記録し、
旧番号原文・付録・歴史台帳の保全とは別に評価する。

1540→974の減少566件は、主に自動生成catalogの参照先を実際の添付ファイル集合へ再生成した結果である。
添付にない実験ログやevidenceを復元したという意味ではない。

## 2. 既存check_docs.py

| 実行対象 | 終了コード | 検出した欠損・不正リンク |
| --- | --- | --- |
| 変更前の添付 | 1（FAIL） | 1540件 = missing-doc-file 1538 + missing-heading 2 |
| 更新版 | 1（FAIL） | 974件 = missing-doc-file 972 + missing-heading 2 |

[変更前JSON](check-results/input-docs.json) /
[更新版JSON](check-results/current-docs.json) /
[更新版stdout](check-results/current-docs.txt)。

この既存検査器のknown-link-issues台帳は43件だけを既知としているため、
入力時点から存在する参照の多くも「new link issue」と表示される。
更新版の931件というnew_issue_occurrencesは、この古い内部台帳に対する値であり、
今回の改訂で作ったリンク切れの件数ではない。今回入力との差分は前節の0件で判定する。

パラメーター台帳から参照する欠損は、入力・更新版とも同じ5パスへの41参照である。
内訳は添付にない4つのevidence文書と、docs-onlyパッケージ外の1つのRustファイル。
今回これらの実測資料を作成・推定したり、既存baselineへ一括追加したりしていない。

## 3. 既存RFC snapshot監査

実行: python3 docs/rfc-v2/check_integrity.py。入力・更新版とも、元の7原文hashの不一致を検出する。

対象はRFC-0009・0011・0014・0023・0026・0037・0041。
この不一致は2026-10-29のsource-mapと今回添付の原文の間に、編集前から存在する。
今回の更新でそれら7原文を変更していないことは、1節の入力digest比較で確認した。

目録検査はユーザーの採択に従い20冊へ更新したが、歴史source-mapと89のfragment digestは更新していない。
最終結果に追加のリンク不備・付録改変・目録不一致はない。

[入力stdout](check-results/input-rfc.txt) / [更新版stdout](check-results/current-rfc.txt)。

## 4. 既存ADR snapshot監査

添付の元検査器はrepository直下READMEを無条件に読むため、docsだけの入力ではFileNotFoundErrorで停止する。
[元の停止記録](check-results/input-adr-original.txt)を保存した。

更新版の検査器は、供給されていないrepository READMEを範囲外と明示し、
RFC冊数を現行registryから確認する。原文・appendixのhash gateは維持する。
この同じ検査器で変更前を評価すると50指摘、更新版は84指摘となる。

- 入力にすでにあった、旧ADR-0171・0204・0209の元snapshotとの相違を保持している。対応する複数のhash・行数・statusチェックが別々に報告される。
- 旧RFC7原文と以前のmetadata snapshotの不一致も入力由来である。
- 後続文書の未登録と旧RFC冊数の2指摘は、registry更新により解消した。
- 今回の採択による現行RFC先頭節、索引、PRINCIPLES、文書検査器などの変更に対して、旧metadata snapshot監査の指摘が36件増える。同じファイルが二つの旧inventoryから報告される場合がある。

増えた36件の分類はExisting RFC files 17件、Unchanged baseline files 19件であり、
旧番号原文・統合ADR50冊の新たな破損ではない。
この採択変更を古い編集snapshotへ混入させるためのhash更新はしていない。
現在の入力からの保全と変更は、新しい検査器・採択記録・変更台帳で追跡する。

[変更前を同じ検査器で評価した結果](check-results/input-adr-with-overlay-support.json) /
[更新版JSON](check-results/current-adr.json) /
[更新版stdout](check-results/current-adr.txt)。

## 5. 検査器への故障注入

隔離したコピーに対して、具体的な2ケースだけを実行した。本体の文書は変更していない。

| 入れた不備 | 期待結果・実結果 |
| --- | --- |
| 旧RFC-0042の原文bytesを変更 | 原文保全がFAIL |
| 既存の同じ壊れたリンクを1回増やす | 同一キーの出現数1→2を検出してFAIL |

[故障注入結果](check-results/revision-fault-injection.json)。
この検査は意図した不備を検出することの確認であり、checker全体の形式証明ではない。

## 6. 意味のレビューと検査の限界

RFC16・17・20の相互参照、18・19・20の脅威・帰責・資源条件、
ADR0215とPRINCIPLESの限定的な優先関係を別途読み合わせた。
Miner自身の資料隠匿・破棄の免責禁止、局所検査対象と承認位置の分離、
global寡占とモデル内寡占の区別、遅い公開の時間予算、Finalとの循環回避を反映した。

文書検査のPASSは、モデルの非保有を実装が満たしたこと、最悪ケースが3,120／10,000 DAAに収まること、
実行traceを一回で取得できること、正直なglobal pool比率、実ネットワークの安全性を保証しない。
新profileの具体的な実装・測定・有効化gateはRFC20に残している。
外部URLとdocs外のコードの存在・正しさは、このオフラインのリンク検査では検証していない。
