# EXEC・1 BPS追加改訂の検査結果

対象は直前の `misaka_docs_rfc20_20261108.zip` の1867ファイルから作成した追加改訂である。
[改訂内容](README.md) / [変更台帳](changes.json) / [現行RFC20冊](../../../rfc-v2/README.md)。

**今回入力に対する8項目の文書検査はすべてPASS。**
歴史snapshotに対する既存監査はFAILを残す。各判定の対象を以下に分けて記録する。
Rust build、native実装、実ノード、1 BPS・courtの性能測定、実モデルの完了、公開有効化は今回検証していない。

## 1. 今回入力からの保全・登録・リンク

実行:

~~~sh
python3 -B docs/maintenance/revisions/2026-11-08-exec-1bps/check_revision.py \
  --json docs/maintenance/revisions/2026-11-08-exec-1bps/check-results/current-revision.json
~~~

| 実行検査 | 結果 | 確認した範囲 |
| --- | --- | --- |
| 入力証拠と削除防止 | PASS | 1867入力ファイルをすべて保持。入力manifest・リンク問題一覧の固定hashを確認 |
| 現行目録と旧本文 | PASS | 現行RFC20冊・ADR50冊。旧RFC48原文、旧ADR211原文、統合ADR50冊のbytesを維持。新0048・0216だけを追加 |
| 歴史台帳と前回改訂 | PASS | 48保護台帳・snapshot、前回改訂フォルダ19ファイルのbytes・ファイル集合を維持 |
| 原付録 | PASS | 41原文に由来する89収録区間の割当・範囲・digestが一致 |
| adoption-map v2 | PASS | 49の完全パスと現行ownerが一致。従来48行の全値を保持。0048を新規採択記録として区別 |
| 後続採択登録 | PASS | RFC側16件、ADR側21件。0048・0216と前回採択を同時に参照可能 |
| parameter追加 | PASS | 旧52 entryの全値を保持。EXEC設計の3キーだけを追加し、合計55 entry |
| 入力比リンク回帰 | PASS | 974件→974件。キーと出現数を比較し、新規・増加は0件 |

[最終JSON](check-results/current-revision.json) /
[stdout](check-results/current-revision.txt) /
[入力manifest](input-manifest.json) /
[入力の974件](input-link-issues.json)。

新3キーは `exec_tx_target_bps_v2=1`、`exec_tx_slot_period_ms_v2=1000`、
`exec_tx_canonical_blocks_per_slot_v2=1` である。すべて新profileの設計値であり、runtime設定を書き換えていない。
一般の文書変更は[新旧digest台帳](changes.json)で追跡し、歴史原文・保護台帳のbytes保全と分けて評価する。

新checkerは前回checkerの固定hashを確認して共通処理を読み込む。
前回の検査器・入力・結果を書き換えず、今回の入力・件数・追加登録を別の改訂として検査する。
過去の[故障注入記録](../2026-11-08/check-results/revision-fault-injection.json)はそのまま保持し、今回の新しい試験結果とは数えない。

## 2. 既存check_docs.pyは入力時と同じFAIL

[今回の結果](check-results/current-docs.json) /
[stdout](check-results/current-docs.txt) /
[直前版の記録](../2026-11-08/check-results/current-docs.json)。

欠損・不正リンクは入力・更新版とも974件で、内訳はmissing-doc-file 972、missing-heading 2。
既存検査器の古いknown-link-issues台帳は43件だけを既知とし、残る931件をnewと表示する。
その931は今回作った問題の数ではない。今回入力比の増加は前節の0件で判定する。

既存検査器のfailure集合・出現数は直前版と同一で、948件の報告に追加・削除はない。
parameterの欠損参照も、入力と同じ5パスへの41参照である。
添付にない4つのevidence文書とdocs外のRustファイルを、実測したかのように作成していない。
外部URLとdocs外の参照先は、このオフライン検査の対象外である。

## 3. 既存RFC歴史snapshot監査は同じ7指摘

[今回stdout](check-results/current-rfc.txt) /
[直前版stdout](../2026-11-08/check-results/current-rfc.txt)。

RFC-0009・0011・0014・0023・0026・0037・0041の原文hash不一致が入力時から存在し、結果は同一。
これは古いsource-mapと直前版原文の差である。
今回それらの原文を変更していないことは、入力ファイルのdigest比較で別途確認した。
歴史source-mapや原付録のdigestを、今回の採択に合わせて更新していない。

## 4. 既存ADR歴史snapshot監査は84→86指摘

[今回JSON](check-results/current-adr.json) /
[stdout](check-results/current-adr.txt) /
[直前版JSON](../2026-11-08/check-results/current-adr.json)。

追加された2指摘は、次のファイルが古いmetadata snapshotから変更されたことを示す。

- `docs/governance/parameters.json`
- `docs/governance/parameter-register.md`

両ファイルへ追加した1 BPS設計パラメーターとその説明によるものである。
旧52 entryがすべて同じ値であり、新3キーだけが増えたことを今回のcheckerで検査した。
既存84指摘の集合・出現数は変わっていない。旧ADR原文や統合ADR50冊の新たな改変ではない。
この2変更を隠すために古いbaseline hashを置換していない。

## 5. 添付分析と意味のレビュー

[提供分析の記録](sources/README.md)に、今回の添付textをbytesのまま保存した。
添付とのSHA-256一致を確認した。添付内の外部実装、過去測定値、条件付きDAA算術は今回再測定していない。

RFC07・10・0048・ADR0216と改訂READMEを相互に読み合わせ、以下を確認した。

- 先行cut、proposal、後続seed・allocation root、ScheduleId、PermitIdの導出に自己参照を入れない。
- 生成資格のauthority、実際の収録根拠、EXEC attachmentを区別する。
- global slotの単一枠、期限付きcreditの予約・消費・返却・失効を同じ原資へ結び付ける。
- 遅延候補と四つの取引状態を分け、受理済みprefixとcanonical reorgを一貫して扱う。
- reorgで消費をundoしても、同じpermitで別payloadを署名しないjournalを保持する。

RFC05・18・19・20は、全合法継続の因果的な容量予約、cold outsider、EXECとの合算資源、
元のnotice／hard時計、資料不供給時の成功停止・default・残責任について読み合わせた。
レビューは文書間の整合確認であり、形式的安全性証明や実装の正しさの証明ではない。

## 6. 完了していない実装・実測gate

今回完了したのは設計文書の改訂である。
1 BPSの継続達成、credit供給率、schedule先行時間、clock／包含／閉鎖規則の実装、
実モデルのcourt完了、Nodeの最低資源内での混合負荷処理は未検証である。

有用な対象profileを受理して期限内に処理できることと、超過要求を安全に拒否できることを別に確認する。
資料の永久隠匿に対して算術反証の無条件な完成を約束せず、成功を与えない処理と客観的な提供義務違反を確認する。
そのfallbackとしてNodeにモデル本体・GPU・全推論を要求しない。
新version・clock profile・lead／horizon／grace・寿命・carry上限・共通順序・移行・性能の検証と公開有効化は、各担当RFCのgateに従う。

