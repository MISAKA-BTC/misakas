# PALW実行正当性改訂 — 検査結果

状態: **文書保全PASS、独立レビューCONDITIONAL SPEC PASS、有限抽象モデル16／16 PASS。実装・実LLM・実資源測定はNOT ESTABLISHED、有効化はNOT ACTIVATED。** 検査日: 2026-11-09。

## 入力時点の既存監査

原ZIPを別の読み取り用treeへ展開し、変更前に実行した結果を[check-results](check-results/)へ保存した。

| 検査 | 入力時点の結果 |
| --- | --- |
| 前回EXEC改訂の専用checker | PASS、8項目、0 errors |
| maintenance/check_docs.py | FAIL。ローカルリンク問題974件（欠損文書972、欠損見出し2）、報告failure 948 |
| rfc-v2/check_integrity.py | FAIL。既存の旧snapshot hash差分7件 |
| adr-v2/check_integrity.py | FAIL。既存86件 |

これらの問題を隠すために旧known-link台帳、snapshot hash、過去checker、過去結果を変更しない。新しい0049／0217の追加によって前回の件数固定checkerが拒否する場合も、前回の履歴を改変せず、今回の入力基準で差分を測る。

## 今回の保全と回帰

`check_revision.py`は原ZIPの1885入力ファイル、許可編集範囲外1846ファイル、15の歴史付録、PRINCIPLES§13以外、過去revision、既存49 mapping行、現行20冊／50冊、新採択0049／0217の登録を検査する。原リンク問題974件からの新規・増加を別に数える。

最終結果は[final-revision.json](check-results/final-revision.json)。**5／5検査群PASS、0 errors**。入力基準は[input-manifest.json](input-manifest.json)と[input-link-issues.json](input-link-issues.json)。原入力ファイルの削除なし、許可範囲外1846ファイルの変更なし、現行RFC20冊／ADR50冊を維持した。

| 検査群 | 結果 |
| --- | --- |
| 原入力との照合・削除検出 | PASS |
| 歴史・既存証拠・改訂対象外の原則とパラメータの保全 | PASS |
| 現行冊数と追加採択記録 | PASS |
| mapping・番号registry・indexの整合 | PASS |
| 原入力からのローカルリンク回帰 | PASS、新規・増加0 |

元の共通監査は今回も既存不整合を報告する。[final-docs.json](check-results/final-docs.json)は報告failure 948、リンク問題974（旧known台帳の分類でknown 43／new 931）、[final-adr.json](check-results/final-adr.json)は既存86 errors、RFC旧snapshot監査は既存7件である。入力時点との新規エラー差分は0であり、文書全体の既存監査をすべてgreenにしたという意味ではない。

## 仕様の独立レビュー

[REVIEW.md](REVIEW.md)に指摘・修正・再判定を保存した。執筆に参加していない2系統のreviewerが固定基準で審査し、初回FAILのB1〜B4と統合指摘B5を修正した。再レビューは両者とも**CONDITIONAL SPEC PASS、未解決設計BLOCKERなし**。この判定を「実装済み」「実LLM安全」「測定完了」「有効化済み」へ読み替えない。

## 有限抽象モデル

[check_protocol_model.py](check_protocol_model.py)は、小さな決定論的プログラム、Merkleで認証する局所記録、期限とcase状態、肯定票・反証・creditの競合を有限に探索する。[model-results.json](model-results.json)に列挙数、反例、故意に安全規則を外したmutantの検出、保証外の例を記録する。

**16／16検査群PASS、0 failures。** 結果に記録したRFC16／18／19／20のsource SHA-256は、この検査対象本文と一致する。

| 検査対象 | 結果 |
| --- | --- |
| 数値trace | 16,875件 |
| metadata組合せ | 34,992件 |
| 合計 | 51,867件。正しい54件、偽51,813件すべてにモデル内の有界な局所反証を構成 |
| 関係coverage | WellFormedness、Init、算術、read-from、state保存、停止・出力・長さ・仕事を含む11種 |
| 同batchイベント順 | 120順列で同じ結果 |
| Final gate | 768組合せ |
| 安全規則を故意に外したmutant | 14種すべてを反例で検出 |
| 保証外の前提破れ | 全員共謀、正直な一人の全通信検閲の2例を期待される反例として記録 |

上記16群は、認証とscope、notice／hard／Finalの境界、case隔離、責任分類、一度だけの経済効果、保護予約、取得後のInspectionPending、CaseId／MaterialIdの分離、元raw record違反と外側packet不正の区別、同batchの全consumerの状態参照を含む。局所証拠の最大3 record／9 sibling hashはこのtoy modelの値であり、実LLMの上限や性能値ではない。

これは独立に作成した仕様モデルであり、Rustコードとの同値性証明、LLM全演算の実証、暗号安全性の証明、実帯域・メモリ・CPU測定ではない。全員共謀や正直な一人の全通信が検閲される例は、無条件安全性のPASSにせず保証外の反例として残す。

## 実装と有効化

**NOT ESTABLISHED / NOT ACTIVATED。** 添付はdocsであり、今回の作業はRFCと関連文書の変更である。対応するnative実装・全relation familyの具体checker・実LLM・混合負荷・restart／reorg／IBD・実包含・担保精算の試験は実施していない。これらは20 §11の有効化gateとして明示したまま残す。
