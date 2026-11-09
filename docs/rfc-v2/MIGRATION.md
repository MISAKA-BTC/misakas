# 現行20本への移行と版の境界

## 2026-11-09公開資料・一人前提の限定移行

[0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[0218](../adr/0218-public-materials-and-one-honest-panel.md)により、公開資料を選出後に取得するPanel、一人の全体検証、k_min=1と全検査・異議窓、正常claimの条件付き完了を新profileへ追加する。入口は[20](0020-verification-gated-final-and-adversarial-models.md)と[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。旧75／41・旧claim・旧PositiveChecked・wire・fence・runtime設定を変更しない。

今回添付は先行0049／0217改訂の入力と同じbytesであり、同会話で採択・提供した先行改訂を継承して編集した。先行改訂1911ファイルに対するmanifestを[今回の記録](../maintenance/revisions/2026-11-09-public-panel/README.md)へ保存する。現行20 RFC／50 ADR、旧原文、15冊の歴史付録、先行revisionの検査・hashを保持する。

今回の保全・登録・リンク回帰は `docs/maintenance/revisions/2026-11-09-public-panel/check_revision.py`、結果は[今回の検査](../maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。過去checkerの件数・許可編集範囲を最新状態に書き換えず、過去検査は当時の対象だけを示す。以下の先行改訂説明にある「今回」「現在」はその改訂時点の記録として読む。

## 2026-11-09の限定移行

[0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)の追加採択は新claim/profileに限定する。通常編集先20冊、既存旧番号原文、41原文の89歴史断片、ADR50冊、2026-11-08の両改訂フォルダを保持する。

現在の入口は[20](0020-verification-gated-final-and-adversarial-models.md)と[公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md)。旧署名済みclaim・旧PositiveChecked・旧Gate C・既存wire/fenceを読み替えない。旧v0.1/v0.2と採択記録は履歴として保持し、新しい規則が優先する範囲をPRINCIPLES§13と0049に限定する。

今回入力に対する保全・登録・リンク検査は `docs/maintenance/revisions/2026-11-09-palw-execution-integrity/check_revision.py` を使う。過去checkerは過去の件数・採択を検査するため、その結果を今回のruntime完成判定にしない。以下の過去統合の説明は当時の記録である。

> **追加改訂の現行入口:** [EXEC・1 BPSの変更](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。以下の前回改訂・過去snapshotの記録は履歴として読む。実装・有効化の完了記録ではない。

改訂日: 2026-11-08。入力は今回添付された docs(2).zip。
[現行索引](README.md) / [変更記録](../maintenance/revisions/2026-11-08/README.md) /
[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)。

## 今回の採択と編集先

現行RFCを0001〜0020の20本とし、16〜20を新設、01〜15の適用範囲と入口を更新する。
ADRの統合50本は保持し、[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)を新しい採択記録として登録する。
48の旧番号RFCと入力時点の210の旧番号ADRは、添付時のbytesを保持する。
旧番号0042の重複は完全なファイル名で識別し、改名や番号の流用で解消しない。

採択は新しいclaim版の設計に限る。Miner originalのtrace帰責、normal positive、
negative、Nodeの検査範囲、pre-Final認証を新しい版で接続する。
旧R2の全被覆検査と新しいglobal承認を同じ証明として扱わず、旧claimを遡及解釈しない。
既存のFinal後認証に「認証済みでないとFinal不可」を足す実装は循環待ちになるため禁止する。

## 反映手順

ZIPのdocsディレクトリは今回の入力に対する更新済み一式である。実repositoryの別作業が進んでいれば、
[変更ファイル台帳](../maintenance/revisions/2026-11-08/changes.json)と差分を照合してmergeする。
この文書更新はRustコード、preset、genesis、height、wire、署名domain、稼働nodeを変更しない。

実装時は20の未完了gateに従い、新しい型、native入口、資金・状態遷移、
public remedy、cold Node、reorg／再起動を接続・測定する。
具体的な抽選・資源policyとactivationは別途確定する。
採択済み設計を再び未採択案へ戻すという意味ではない。

## 再検査

~~~sh
python3 docs/maintenance/revisions/2026-11-08/check_revision.py
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

最初は今回入力に対する原文保持・20本構成・48ファイル対応・追加リンクの回帰を検査する。
残りは既存監査であり、今回入力以前の欠損リンクや旧snapshotとの不一致も検出する。
歴史manifestや既存の欠損リンクbaselineを書き換えてPASSにしていない。
[検査結果](../maintenance/revisions/2026-11-08/VALIDATION.md)に終了コードと範囲を分けて記録する。

## 歴史的な15本統合の記録

以下は2026-10-29の旧統合記録である。当時の入力・15冊・41原文という数は歴史値であり、
現在の編集先20本と48の参照原文を表す値ではない。


## 1. このパッケージに含めた変更

新しい閲覧・編集の入口を`docs/rfc-v2/`の15本へ移す。`docs/rfc/`の番号付き41本は原文を変更せず、互換参照・判断履歴として残す。旧README／INDEX／TOPICSは歴史索引であることを明示し、新しい入口へ案内する。

独立した新技術RFCを41本足したのではない。現行の主題別RFCは15本、旧41本は原文証跡である。ADR、Spec、PRINCIPLES、パラメーター台帳は判断資料として保全し、編成だけで権威を入れ替えない。

`source-map.json`は原文の全行を一度ずつ15本へ割り当てる。Markdown見出しの階層と相対リンク以外は収録内容を変えない。元の41本もSHA-256で照合する。日付付きの実装記録や旧い案はそのまま残るが、現状保証ではない。

## 2. 反映手順

`misakas-pre(5).zip`と同じ内容のcheckoutを対象に、ZIP内の`docs/`をrepo直下へ重ねる。別branchに新しい文書がある場合は上書きせず、差分をmergeする。コードのファイルや合意設定をこの作業で変更しない。

```sh
python3 docs/rfc-v2/check_integrity.py
```

新しい文書を確認した後、通常の文書変更としてcommitする。既存のCI／文書自動生成ツールが旧`docs/rfc/*.md`を数える場合は、アクティブな冊子一覧を`docs/rfc-v2/registry.json`から読むように別途接続する。旧番号でADR／test／sourceを追う用途は旧索引を使い続ける。

## 3. 変更しないもの

Rust、Cargo、P2P message ID、codec tag、署名domain、fence名、height、genesis、release.json、既存権利、台帳の値を変更しない。runtimeのcompile／E2Eはこの文書編集の検査に含めない。

旧RFCパスは動かさないため、既存コードやADRの参照先を壊さない。原文中の相対リンクは統合収録の際に旧ファイルの場所へ向け直し、旧§番号が新章番号へ誤って結び付かないようにする。

## 4. 整理中に分離した意味の違い

| ID | 論点 | この編集での処置 |
| --- | --- | --- |
| D01 | 旧READMEに次RFC=0027、古い台帳に0023等が残るが、入力には0041まである | 現行15本の新registryを追加。旧台帳は日付付き履歴。他branchの次番号を断定しない |
| D02 | Courtの3,120と3,200 | 受付締切と性能目標を14で別欄にした。数値統一はしていない |
| D03 | 30日平均と後続の7日平均 | 11では現行台帳／PRINCIPLESの7日を案内。原文の30日は過去記録として保持 |
| D04 | 登録tierのκ、縮小後分母・縮小分発行について台帳自身がADRとの不整合を記録 | 11に未解決として明示。数式・発行を独断で変更していない |
| D05 | 旧15のPanel=0休眠と共通bond制度 | modeは15、共通の採用規則は11へ分けた |
| D06 | 旧39のfee-only採択と有限役務A/B | feeは11、A/Bは15。A/Bを採択済みにしていない |
| D07 | Draft／隔離実装と本番採択 | 旧30〜38等の元状態を維持。未採択を恒久不採用にも変更していない |
| D08 | 旧い計算validity proof・モデル審査・Seeder案 | 15の既存の禁止／廃止台帳を優先し、原文の残存を復活の根拠にしない |

原文・ADRの意味に残る食い違いは設計責任者の採択記録で解消する。新しい文書番号が小さい／大きいという理由で優先しない。

## 5. 運用

15本それぞれを、その主題の編集先として使う。Courtの新規細分RFCを増やす代わりに14の該当章へ、TIRのquery/data変更は02へ追記する。ただし凍結例外に必要な変更提案とADRの新規発行手続きは維持する。

凍結・延期・不採用の台帳と、実装済み・試験済み・有効化済みの記録は別に管理する。この整理版だけでmainnet readinessを判定しない。
