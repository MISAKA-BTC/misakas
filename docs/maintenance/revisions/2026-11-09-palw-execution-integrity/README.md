# 2026-11-09 PALW実行正当性と一人の反証

入力: `misaka_docs_rfc20_exec_1bps_20261108(2).zip`。この改訂は入力のdocs全体を保持して更新する。
採択状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

ユーザーが求めた「一人の正直な検証者が適格な反証を示せば、他のPanelが結託していてもNodeの小さな計算で誤claimを無効化する」「必要な検証承認が成立しなければ成功させない」を、現行20冊のRFCに反映する。Miner本人や各Panelの物理的な実行行為を証明するという主張にはしない。

## 現行の入口

- [RFC20: 保証・状態遷移・成功gate](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)
- [RFC16: Minerの実行記録と全関係](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)
- [RFC17: 完全再実行の承認と局所positive](../../../rfc-v2/0017-noninteractive-positive-verification.md)
- [RFC18: 単独異議・取得後検査・局所化・責任](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)
- [RFC19: 有界なnative検査と局所反証の健全性・完全性](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md)
- [追加採択RFC-0049](../../../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)／[ADR-0217](../../../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)
- [公開裁定の方針v0.3](../../../design/palw/consensus/public-adjudication-policy-v0.3.md)

## 何を閉じた設計にするか

`WellFormed + Init + 全Step + End`を固定された実行の必要十分な関係へ結び付ける。型・codec・位置・read-from・state保存・演算・数値意味・制御・乱数・停止・出力・長さ・仕事会計を対象とし、各関係の偽を元Minerの初回commitに結び付いた有界資料で裁けるprofileだけを受理する。

一人の適格noticeをnative集合Qへ保存し、肯定票や別caseの棄却で消せなくする。完成反証は票数を使わずNodeが裁く。資料の受領と検査完了を分け、取得後の検査・反証を同じcase・原時計で継続する。共有資料のIDと申立人別のcase権利を分離し、他人の先着・defaultで正直な人の救済を失わせない。

同じcanonical batchの反証・未解決case・依存失効を、Finalだけでなく全credit consumerの前に反映する。同batchのpermitを先に消費して取消を逃がさない。batch以前のcanonical consumedは既存会計に従って保存し、この改訂だけで全取引を巻き戻さない。

## レビューと検査

[固定レビュー基準](REVIEW-CRITERIA.md)に基づき、執筆とは別のエージェントによるレビューと修正反復を実施した。初回FAILで見つかった4件と統合レビューの追加1件を修正し、再レビューは2系統とも**CONDITIONAL SPEC PASS、未解決設計BLOCKERなし**。[REVIEW.md](REVIEW.md)に各指摘と最終判定、[VALIDATION.md](VALIDATION.md)に文書保全5／5 PASS・有限抽象モデル16／16 PASS・変更前からの既存監査不整合を記録した。

再現コマンド（docsの親を作業ディレクトリにする）：

```sh
python -B docs/maintenance/revisions/2026-11-09-palw-execution-integrity/check_revision.py --json docs/maintenance/revisions/2026-11-09-palw-execution-integrity/check-results/final-revision.json
python -B docs/maintenance/revisions/2026-11-09-palw-execution-integrity/check_protocol_model.py --output docs/maintenance/revisions/2026-11-09-palw-execution-integrity/model-results.json
```

## 保持するものと残る実証

現行RFC20冊・ADR50冊、旧RFC／ADR本文、15冊の歴史付録、過去revision・実測・hash、モデル二階層、経済値、EXECの1 BPSの意味、暗号学的計算証明の不採用を保持する。新規採択記録は20冊とは別の履歴系列へ追加する。

一人保証には、正しい検証者の能動的参加、必要資料、資格と保護予約、期限内配送・canonical包含、native規則の正しい実施が必要である。全員共謀・全通信検閲・永久秘匿まで無条件に成功／無効化できるとは主張しない。

文書の論理レビューと有限抽象モデルが通っても、Rustのnative接続、実LLMの全関係の具体化、最低要件機材の最悪CPU／RAM／IO／包含、同一binaryの正常・反証・不供給経路、activationは別の未完了gateである。全profileを拒否するだけでは利用可能なPALWの完成とはしない。
