# 2026-11-09 公開資料・モデル未保有Panelの改訂

採択状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

ユーザーが提示した方針を、現行RFC20冊と関連規範に反映した。保証の前提は、各claimの独立選出Panelに能動的な正直者が少なくとも一人いることである。その人はモデルを事前に保有せず、Minerが公開供給する認証付き資料を取得して全体を検証する。Nodeはモデル全量を取得・実行せず、小さなnative証拠を裁定する。

## 入力と継承

今回の添付`misaka_docs_rfc20_exec_1bps_20261108(2).zip`は1,885ファイルで、前回改訂元とSHA-256が一致した。前回ユーザー指示で完成した[実行整合性改訂](../2026-11-09-palw-execution-integrity/README.md)の1,911ファイルを継承し、B1〜B5の修正と採択・レビュー履歴を戻さずに今回の変更を加えた。[input-manifest.json](input-manifest.json)に添付と継承版のhash、継承全ファイルのhashを固定した。

## 規範の入口

- [RFC20: 一人前提・最低承認1・Final・正常完了・atomic batch](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)
- [RFC16: 公開検証パッケージとMiner供給責務](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)
- [RFC17: 全体検証・最低1件の承認・検査窓](../../../rfc-v2/0017-noninteractive-positive-verification.md)
- [RFC18: 資料要求・検査前提・case終結](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)
- [RFC19: 上限付き整数区間と全kernelの局所反証](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md)
- [RFC05: cold取得と全合法係争の容量](../../../rfc-v2/0005-bounded-resources-and-long-context.md)／[RFC12: 状態更新と復元](../../../rfc-v2/0012-authenticated-state-storage-and-ibd.md)
- [採択RFC0050](../../../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR0218](../../../adr/0218-public-materials-and-one-honest-panel.md)／[公開裁定v0.4](../../../design/palw/consensus/public-adjudication-policy-v0.4.md)

## 今回の変更

重み・構造・program・prompt／token列／tokenizer・数値規則・RNG・初期KV等の依存・初回traceを、独立Panelと適格第三者が所有者の個別許可なしに取得・認証・再配布できる範囲へ定義した。モデル事前保有、Miner推薦、取得成功者だけへの再選出を検証参加条件にしない。

新profile`PublicOneHonest`のlogicalな最低完全承認数を`minimum_full_replay_approvals = 1`とし、機械可読台帳にも設計記録として追加した。1署名直後のFinal、全員ACKの別名復活、人数m=1への読み替えを認めない。全検査・異議窓、Q、局所計画、依存、資金、原期限を残す。

資料不足でも算術反証なしに適格要求を開始でき、供給後もInspectionPendingを保持する。全scopeの検査前提を客観化できないまま、quota消尽・少数chunk・他人receipt・時間経過で要求者をdefaultにしない。正常claimへの最大合法な虚偽caseを終結する条件を、安全な未検証Abortとは別に定義した。

有限atomic batchは同Miner・同model・同profileの封印済みleaf群を単一originalとして受理する任意経路とした。全leaf検証、一leaf反証で全体Refuted、部分creditなし、既受理original吸収禁止とする。cache・batchは固定費を共有するが、全検証仕事量や最悪の係争費用を消さない。

旧75／41、旧4,096抽選位置／日、旧署名・経済値・既存の期限、EXEC1 BPSの意味、20冊／50冊、旧原文・過去のレビュー証拠を維持する。新m、B、帯域、受付量、batch上限は未測定値を作らず、有効化前の確定を要求する。

## レビューと検査

[固定基準](REVIEW-CRITERIA.md)による執筆後の独立レビューは、2系統とも**CONDITIONAL SPEC PASS**。指摘されたatomic会計の表現、defaultの参照・節順、未完gateの入口表示を修正して確認した。[REVIEW.md](REVIEW.md)に判定と留保、[VALIDATION.md](VALIDATION.md)に保全・旧監査差分・有限モデルの結果を分けて記録する。

文書保全は5／5 PASS、新しい有限モデルは14／14 PASS、前回モデルの回帰は16／16 PASS。元のリンク問題974件・旧RFC監査7件・旧ADR監査86件は保持し、今回の新規エラー差分は0である。

再現コマンド（docsの親を作業ディレクトリにする）：

```sh
python3 -B docs/maintenance/revisions/2026-11-09-public-panel/check_revision.py --json docs/maintenance/revisions/2026-11-09-public-panel/check-results/final-revision.json
python3 -B docs/maintenance/revisions/2026-11-09-public-panel/check_public_panel_model.py
```

## 未完gateを分離する

**PROFILE PROCEDURE NOT ESTABLISHED:** 大きな公開資料の虚偽未着申告と選択的秘匿を、小さなnative根拠で区別し、全scopeの検査機会を有界に客観終結する具体手順が未確立のprofileは有効化できない。これは単なるコード・性能測定のTODOではない。最低承認1や公開方針だけで解決したとは扱わない。

**IMPLEMENTATION / MEASUREMENT NOT ESTABLISHED:** 実LLMの全関係とnative checker、資料・Q・承認・決済の同一binaryでの接続、唯一正直者のcold試験、最悪係争・混合負荷・包含・復元は未実証である。抽象モデルや文書レビューのPASSで置き換えない。全profile拒否だけを利用可能なPALW完成としない。

全員共謀、正直者の全通信の永久検閲、合意側がnative規則を実施しない状況を無条件に保証せず、Miner本人の物理的な計算・非外注・実FLOPsも証明対象にしない。
