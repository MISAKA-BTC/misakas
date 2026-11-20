# PALW監査対応 — RFC-0056／0057改訂とRFC-0058

日付: 2026-11-17（Asia/Tokyo）  
状態: **PROPOSED_DESIGN_NOT_ACTIVATED**

## 今回の成果

ユーザーが提示した白書監査と追加対処案を、現在のsourceと既存の採択仕様に照らし、修正版RFCへまとめた。受付難易度、Court nonce、仕事の一回性、段階別抽選、合意資格の境界を一つの提案として接続する。

| 文書 | 今回の内容 |
| --- | --- |
| [RFC-0056](../../../rfc/0056-global-claim-admission-difficulty.md) | 経済TrialSlotだけでは同じ計算の再抽選を防げない点を修正。実計算WorkKey、完全Original、未来ticket、二層の受付時計へ接続 |
| [RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md) | Panelの未来乱数と元仕事をpositiveへ束縛。反証・資料要求・応答・終端は未来seedを待たない専用contextへ分離 |
| [RFC-0058](../../../rfc/0058-staged-work-commitments-and-consensus-eligibility.md) | owner事前固定、実job、Original固定後の抽選、Panel対象固定後の抽選、一回性、成熟、全consumerの現在状態再検査、証明の境界 |
| [ADR-0226](../../../adr/0226-staged-work-commitments-and-consensus-eligibility.md) | 変更理由、選ばない対策、既存原則との境界、BFT参照候補の無効状態 |
| [実装計画](../../../design/palw/integration/staged-work-implementation-plan-2026-11-17.md) | P0〜P9の接続先と受入条件。Studioから権利消費までのnative実装課題 |
| [脅威表](THREAT_MATRIX.md) | WB01〜WB36と統合シナリオ。防御、仮定、残るリスク、必要な証拠を分離 |
| [ソース監査](SOURCE_AUDIT.md) | 固定commitの根拠、現在実装への批判の補正、未実行の攻撃候補 |
| [独立レビュー](REVIEW.md) | R01〜R20の反例と修正。文書の整合と本番有効化を分離 |
| [検証記録](VALIDATION.md) | 抽象モデル、入力保全、旧検査の失敗との比較、未実施の検証 |

[RFC-0055](../../../rfc/0055-global-admission-court-calendar-and-one-bps.md)とADR-0223も依存資料として含め、元bytesを維持した。

## 最初に補正したこと

現在のattempt実装のC_attempt_zeroは、challenge欄だけをゼロにしたattempt全体であり、trace／output／execution rootを含む。従って、「推論結果を一切含まない位置情報だけでticketを計算する」という解釈は、現在mainに対する確認済みの攻撃とはしない。

その一方で、rootが入力にあることから、正しいrootの構成費用、本人の物理実行、安価な偽root候補の排除、beaconの偏り耐性、DAG合意安全性は導けない。これらを別の問題として修正・評価する。

既存sourceにもWorkIdの重複拒否、FP receiptの未来beacon、認証後の成熟がある。「すべて新設する」説明を避け、既存契約を新profileの全経路へつなぐ作業を記載した。監査の根拠はSOURCE_AUDITに固定commitリンクを付けた。

## 新しい順序

1. owner／beneficiary、実資金、job規則、stage予定を登録し、集合を閉じる。
2. R_jobから検証対象の実jobを導出する。
3. 元の完全Originalを一回だけ固定し、その集合を閉じる。
4. 未来のR_ticketで全モデル共通の受付Dを判定し、全工程を予約する。
5. 受理対象とpoolを閉じた後のR_panelでPanelを選ぶ。
6. 元rootに対する完全検査・局所court・適格反証・成熟を処理する。
7. 同batchの反証・取消を先に適用し、現在stateを再検査して用途別権利を発行・消費する。

各stageで同じWorkKeyを再抽選できない。R_jobをhashし直すだけで未来R_panelの未知性を作ったことにはしない。positiveはR_panelへ結合するが、negative／資料caseは新乱数を待たない。

120秒はdifficultyの制御epochのまま維持する。三段階と成熟を含む完了時間ではない。複数モデルは共通容量の範囲で並行でき、外側の全carrier合計1 BPSを目標にする。court件数を一件・三件・五件に固定していない。

## 保持する設計条件

整数完全一致、モデル非保有Node、全体を再計算するPanel、有界な局所checker、単独反証、モデル無審査登録と1.0／0.3、既存の経済上限を維持する。Nodeへ全モデル重み・全推論を要求せず、計算正当性の暗号学的証明を追加しない。

DNS-BFT廃止とPQ-onlyも、この提案で自動変更しない。threshold BLSは能力契約を説明するDISABLEDな参照候補である。具体的な委員会形成・DKG・鍵・fault model・方針適合が未確定のまま導入できない。

## 未証明・未実装として残ること

**この修正によってPALWのPoW成立を証明したわけではない。** 次の課題は、本仕様の文章や小さな状態モデルでは解決しない。

- 任意の登録モデルについて、cache、batch、簡約、零重み等を許した最安計算費用の下界と非償却性。
- 正直なPanelが全資料を取得し、実際に完全検査し、期限内に反証を包含・裁定できる実手順。
- 段階ごとに閉鎖前未知で、偏りや無料再試行を許さず、非循環なstable anchorを持つ実beacon。
- 認証遅延、取消、分断、reorg、pruning、既消費creditを含むDAGのcommon-prefix、収束、活性。
- Studio、全native carrier、DB／IBD、実LLM、最低Node、1万〜数万モデル、実1 BPSの統合実測。

抽象モデルには、正しい零重みtraceを少ない演算で構成できる例も入れた。これを不正として拒否するのでなく、「正しいtraceがあるので規範演算量分の費用を払った」という推論が成立しないことを記録する。

## 入力と保全

今回の文書基準は添付docs(3).zip（SHA-256: 4d6ee065a148aa2212dd650830bad79cba6894c9b849e314bfd56d9af50d7e40）のdocs 2,483ファイルである。前回の0055〜0057成果にのみ存在した44ファイルを追加し、結合入力2,527ファイルをsource-baselineへ固定した。添付の追加文書・証拠を古いmainのdocsで置き換えていない。

固定コード読取りはmain@2d8212b634880b0967fbf24b00b41936c2322c14。添付docs全体とコードsnapshotを同一だと扱わない。今回の添付に15ページ白書PDFはなく、ユーザーの監査文と設計メモを入力にした。

旧採択本文はmarked proposal pointer以外を保持し、歴史source、旧evidence、0055／0223、以前の三revisionの検証結果はbytesを保存した。0056／0057の改訂前bytesは次にも収録した。

- [prior-rfc-0056.source.txt](prior-rfc-0056.source.txt)
- [prior-rfc-0057.source.txt](prior-rfc-0057.source.txt)

以前の46件／49件のPASSはその時点の文書hashに結合した履歴であり、現在の改訂のPASSに読み替えない。今回のモデル結果は現在の0056／0057／0058とモデルscriptのhashへ結合する。

## 検査の再実行

パッケージのcurrentディレクトリ、又は更新済みrepositoryのrootから実行する。

~~~bash
python3 docs/maintenance/revisions/2026-11-17-work-binding/check_work_model.py
python3 docs/maintenance/revisions/2026-11-17-work-binding/check_revision.py
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

最初の二つは今回の抽象契約・入力保全を検査する。後の三つは、添付inputの時点で既存欠損や過去snapshot不一致によりFAILしている。公式baselineを失敗隠しのため更新していない。最終比較はVALIDATIONに記録する。

### 機械可読記録

[source-baseline.json](source-baseline.json)、[baseline-checks.json](baseline-checks.json)、[source-provenance.json](source-provenance.json)、[model-results.json](model-results.json)、[revision-results.json](revision-results.json)、[validation-results.json](validation-results.json)、[changes.json](changes.json)。

これらは文書と抽象契約の証拠であり、公開activationや理論安全性を承認するrecordではない。
