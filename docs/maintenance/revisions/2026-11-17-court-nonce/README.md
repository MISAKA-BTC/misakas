# 2026-11-17 — Court nonceとPanel固有チャレンジ CPN1

状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。設計案、抽象契約検査、native実装、実機、公開activationを分ける。

## 成果物

| 文書 | 内容 |
| --- | --- |
| [RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md) | native義務・nonce・署名subject、個別challenge、共有資料、受理/計数、責任、復旧 |
| [ADR-0225](../../../adr/0225-court-nonce-and-panel-challenges.md) | 提案理由、凍結仕様との境界、採らない構成 |
| [実装接続表](../../../design/palw/integration/court-nonce-implementation-plan-2026-11-17.md) | 現実の入口・owner・P0〜P6と完了証拠 |
| [ソース監査](SOURCE_AUDIT.md) | 元mainのbinding・shared material・nonce・beacon・onceとCPN1の不足 |
| [レビュー](REVIEW.md) | native seed幅、plan循環、受理後計数、chunk認証を含む本文レビュー |
| [検証記録](VALIDATION.md) | 抽象モデル・文書保全・標準checker前後差と限界 |

## 本提案の意味

Courtの再利用防止は、元claim・actor・義務・round・stepに結び付く提出物の一回性である。正しい認証資料の共有を禁止しない。同じ資料が別Panelの必要対象にも使える場合は、その本人の署名・nonce・資格と局所条件を別に検査する。

positiveの個別challengeはその人の完全承認の計数条件であり、全員提出待ちのFinal条件ではない。共通coverage、最低1件承認のPublicOneHonest、全選出者の完全検証責務、全体検査/異議期間、Q解消と単独反証を維持する。negativeはsample外の不正も扱い、新seedを待たせない。

nonceは元trace/leaf・数値計算へ混ぜない。既存の64-byte seedは全体をNRで包み、新しい32-byte nonceへ結合する。署名されたChunkRootと完成BodyDigestを両方検証する。nonce・署名・局所openingは本人の独立全体実行を証明しない。

## 基準と保全

コードの基準は添付ZIPとGitHub main@2d8212b634880b0967fbf24b00b41936c2322c14。[source照合](source-provenance.json)は前回3,582＋RPC 1 blobの記録を参照し、今回追加の6 blobを新たに照合した。

文書差分の基準は、同会話で提供したRFC-0056までの完成overlay。[source-baseline.json](source-baseline.json)はその1,975ファイルを固定し、[integration-baseline.json](integration-baseline.json)は構造的な追記を検査するための元metadata bytesを保持する。前回0055/0056の本文・ADR・試験結果・歴史revisionは変更しない。現行20 RFC/50 ADRには提案のpointerを追記するだけであり、採択済み本文を黙って改定しない。

[変更台帳](changes.json) / [今回の文書結果](revision-results.json) / [標準checker比較](validation-results.json)。

## 再検査

以下は保存結果を変更せずに実行する。

~~~sh
python3 docs/maintenance/revisions/2026-11-17-court-nonce/check_revision.py --stdout-only
python3 docs/maintenance/revisions/2026-11-17-court-nonce/check_nonce_model.py --stdout-only
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

[モデル](check_nonce_model.py) / [保存結果](model-results.json)。抽象モデルはnativeの署名・選出・資本・beacon安全性・全工程planner・実機負荷を代替しない。保存結果を再生成した場合は、正規の変更としてmanifestのhashも更新する。

旧revisionのcheckerは当時のsnapshotを検査するものとして保持し、この新revisionの合格/未有効化の判断へ読み替えない。標準checkerは元入力の歴史不整合と参照切れで失敗し得るため、旧baselineを変更せずに前後差を記録する。
