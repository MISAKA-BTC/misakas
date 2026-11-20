# 検証結果

この記録はPALW-MOR1の文書改訂に関するものである。native実装、モデル実行、費用下界、ネットワーク運用の検証を意味しない。

## 更新前の状態

| 必須検査 | 更新前exit | 更新前の失敗 |
| --- | --- | --- |
| docs/maintenance/check_docs.py | 1 | failure entries 947 |
| docs/rfc-v2/check_integrity.py | 1 | FAIL行7 |
| docs/adr-v2/check_integrity.py | 1 | errors86 |

更新前の失敗を既知として消すためのbaseline変更は行わない。更新後は、各エラーの文字列と重複回数に新規増分がないかを比較する。

## 最終比較

**新規の報告エラーは0件。既存エラーの文字列と重複回数も完全に一致した。** checkerそのものの全件成功ではなく、入力資料からの新規regressionがないという結果である。

| 比較対象 | 更新前 | 更新後 | 追加 | 削除 |
| --- | --- | --- | --- | --- |
| docs failure entries | 947 | 947 | 0 | 0 |
| docs new-link occurrences | 930 | 930 | 0 | 0 |
| RFC FAIL行 | 7 | 7 | 0 | 0 |
| ADR errors | 86 | 86 | 0 | 0 |

三検査は既存問題により更新後もexit 1である。docsの既存failure entriesは906種類の新規link keyと41件のparameter source欠落を含む。RFCの既存7件は過去source bytes不一致であり、ADRの既存86件も入力時点から存在する。今回の設計追加と関係しない既存問題を修正・抑制してはいない。

完全なstdout、stderr、JSON、exit statusを`checker-baseline/`と`checker-post-final/`へ収録した。[Counter比較の集計](document_check_delta.json)で、エラーの種類だけでなく重複回数も確認できる。

初回の更新後検査では、索引未登録2件と、RFC-v2末尾への参照追加が履歴断片へ含まれる問題2件を検出した。索引を追加し、RFC-v2参照を履歴節の前へ移動して解消した。初回結果は`checker-post-initial/`へ残し、修正履歴を隠していない。checkerやsource manifestの期待値を変えて解消したものではない。

## 参照例の検査

[check_mor1_reference.py](check_mor1_reference.py)の**8テスト群が成功、失敗0・エラー0**。結果と具体的な符号化例は[mor1_reference_results.json](mor1_reference_results.json)に収録した。

確認したのは、同一入力の再現性、完全C・beacon・s・三Rootへの束縛、強制的に同じsとした場合のCによる区別、不正な長さ・型・framingの拒否、同じ回答文を持つ別試行の出力形式、偽Rootでもrho自体は作れるという限界、birthday上界の正確な有理数計算である。

このscriptのcodecとsigma schemeは**試験用**であり、native wireの割当ではない。C内部のcanonical性は前提として与える。有限個の非衝突例を暗号学的な確率保証の実測とはせず、RFC0060 §8の証明と区別する。Qwen実行、全trace検査、Panel/Court、Final、一回会計、費用下界はこのscriptでは実装していない。

## 保存性・変更範囲

[source_preservation.json](source_preservation.json)で、入力2,606ファイルのうち2,586ファイルのbytes完全一致、意図した20ファイルの限定変更、元ファイルの欠損0を確認した。

RFC-v2の履歴節全体、ADR-v2の元本文と履歴、registryの元documents・adoption、既存catalogレコードを保持した。番号付きの現行冊はRFC20冊、ADR50冊のままである。元ファイルの改変範囲外に、今回のRFC・ADR・改訂記録・参照例と検証ログを新規追加した。

## 再実行

docsを含む展開先を作業ディレクトリとし、次を個別に実行する。最初の三検査は上記の既存問題を再報告し得る。

```bash
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
python3 docs/maintenance/revisions/2026-11-20-model-output-binding/check_mor1_reference.py
```

## 未検証

本番codec、native登録経路、Miner／Panel／Nodeの接続、実Qwenの再現性と品質、資料取得とCourt活性、fork/reorgを含む合意、通常LLM費用の下界は未検証・未実装のまま区別する。設計状態は**DRAFT_NOT_ACTIVATED**であり、今回の文書検査を採択・有効化の記録へ昇格させない。
