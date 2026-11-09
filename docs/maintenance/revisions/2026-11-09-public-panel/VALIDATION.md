# 公開資料・モデル未保有Panel — 検査結果

状態: **独立レビュー2系統CONDITIONAL SPEC PASS、文書保全5／5 PASS、新有限モデル14／14 PASS、前回有限モデルの回帰16／16 PASS。** 検査日: 2026-11-09。実装・実LLM・実資源・有効化は検証していない。

## 判定を分ける

[REVIEW.md](REVIEW.md)の2系統の判定はCONDITIONAL SPEC PASSである。PROFILE PROCEDURE、IMPLEMENTATION、MEASUREMENTの未確立は別々に残す。今回の作業はdocs改訂であり、Rustのruntime変更や本番activationではない。

## 保全と既存監査

[input-manifest.json](input-manifest.json)は、今回添付1,885ファイルと前回受理改訂1,911ファイルの由来を固定する。新checkerは前回受理版の全1,911ファイル、許可変更範囲外1,870ファイル、15歴史付録、PRINCIPLES §13外、既存50 mapping行、旧55パラメーター行、台帳の旧prefix、20冊／50冊と追加0050／0218を検査する。

元のリンク問題974件、旧RFC snapshot監査7件、旧ADR監査86件を消すために、過去のknown台帳・snapshot・checkerを変更しない。今回の新規差分を別に報告する。

### 文書保全

[final-revision.json](check-results/final-revision.json)は5検査群すべてPASS、0 errors。許可対象41ファイルを変更し、範囲外1,870ファイルと過去のrevisionを保持した。

| 検査 | 結果 |
| --- | --- |
| 継承入力1,911ファイルの存在・削除検査 | PASS、削除なし |
| 許可範囲外・15歴史付録・PRINCIPLES §13外・旧パラメーターの保全 | PASS |
| 現行20冊／50冊と追加0050／0218 | PASS |
| 既存50 mapping行・registry・index、新0050本文hash | PASS |
| 入力からのローカルリンク回帰 | PASS、974→974、新規・増加0 |

旧`parameters.json`の55行は変えず、最低完全承認1の設計行を1行追加した。`parameter-register.md`は旧prefixをbytesで保持して追記した。これはruntime設定の変更ではない。

### 共通監査の前後比較

継承した前回完成版に対してbaseline監査を実行し、今回版でも同じ共通監査を実行した。

| 監査 | 入力baseline | 今回final | 新規エラー |
| --- | --- | --- | --- |
| 前回改訂の専用checker | 5群PASS、0 errors | 前回記録は保存。今回の変更は新checkerで判定 | 対象版を混同しない |
| maintenance/check_docs.py | FAIL、failure 948、リンク問題974 | FAIL、同じfailure 948とリンク問題974 | 0 |
| rfc-v2/check_integrity.py | FAIL、旧snapshot差分7 | FAIL、同じ7件 | 0 |
| adr-v2/check_integrity.py | FAIL、既存86 errors | FAIL、同じ86件 | 0 |

根拠: [baseline-prior-revision.json](check-results/baseline-prior-revision.json)、[baseline-docs.json](check-results/baseline-docs.json)／[final-docs.json](check-results/final-docs.json)、[baseline-rfc.txt](check-results/baseline-rfc.txt)／[final-rfc.txt](check-results/final-rfc.txt)、[baseline-adr.json](check-results/baseline-adr.json)／[final-adr.json](check-results/final-adr.json)。共通監査の既存FAILを全体greenと報告しない。

## 有限抽象モデル

新仕様の最低承認1、cold取得、全検査窓、個別case、検査前提、客観default、旧版閾値分離、atomic batchと一回性を有限モデルで検査する。toyの資料scopeは限定され、大容量モデルの公開可用性の証明にはならない。未知のprofile機構を拒否する条件と、定めたtoy機構の下で正常仕事が完了する条件を分ける。

実行: `python3 -B docs/maintenance/revisions/2026-11-09-public-panel/check_public_panel_model.py`。
結果: [public-panel-model-results.json](public-panel-model-results.json)。新規14群と、前回スクリプトを変更せず再実行した16群がすべてPASS。前回の結果ファイルを書き換えていない。

| 新規モデルの対象 | 結果 |
| --- | --- |
| cold開始、唯一の正直者の承認で正常完了 | 9通り、他の選出者の署名0 |
| 悪意承認下の一人反証 | 18通り |
| 資料応答後の同case継続 | 正常Finalと不正Refutedの両方 |
| 全scopeの検査前提の導出 | 384組合せ |
| 資料不足・quota消尽・遅延供給 | 6ケースと残時間不足のAbort |
| 虚偽caseと不署名下の条件付き完了 | 18通り、各回最大4虚偽case |
| 旧75／41と新最低1の署名版・scope | 版分離、無効scope等5種を拒否 |
| Final条件と時間 | 640組合せ |
| atomic batch | 無効形6種、scope変更2種、一leaf反証で全体無効 |
| case隔離・一回決済 | 別申立人の2case、6回の決済再試行、再包装の重複拒否 |
| 同canonical処理単位の順序 | 120順列で結果一致 |
| 保証を失う部分集合・分担 | 部分集合9反例、分担1反例を記録 |
| 安全規則を故意に外した変異 | 11種類すべてを検出 |

11変異は、早期1票Final、全員ACK、Q無視、署名版無視、一部資料を全資料と扱う、quotaを配送完了にする、配送だけでcaseを閉じる、申立人case統合、部分batch credit、任意部分集合への検証委任、旧承認による後付けleaf受入である。全員共謀と正直者の全通信検閲では、不正Finalが起き得る保証外の反例も保持する。

このモデルの検査前提は自己申告boolではなく、小さい既知scopeの全認証資料・本人の予約機会・原時計から導出する。ただし最大3leaf／41scalarという説明用transcriptであり、実LLMの全資料をNodeへ送る提案や、有効化可能な大資料供給profileの実証ではない。未知の大資料機構はUnsupportedProfileとして拒否する。結果内のRFC16〜20のsource SHA-256は検査時の本文と一致している。

## 未完gate

大資料の供給・検査機会を有界に客観終結する手順、全LLM関係、native実装、最低要件機材での測定、bootstrap・報酬式の未確定事項は残る。有限モデルやリンク検査が通っても有効化可能とは判定しない。
