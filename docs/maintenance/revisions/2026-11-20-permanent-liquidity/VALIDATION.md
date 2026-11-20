# MOE2改訂検証記録

2026-11-20（Asia/Tokyo）。対象は設計文書、整数kernelの証明と参照検査、旧原本の保全である。**MOE2はADOPTED_DESIGN_NOT_ACTIVATED**。native/EVM実装・network activation・全状態の経済活性を検証したものではない。

## 1. 改訂範囲

MOE1の既存2,817ファイルをすべて元の相対パスに保持した。既存75ファイルを改訂し、2,742ファイルはbyte同一、削除は0。改訂した全75ファイルについて、変更前の完全原本を`before/`へ保存した。改訂前後のSHA-256は[changes.json](changes.json)に記録する。

中心となる新正本は[RFC0064](../../../rfc/0064-reward-buy-and-permanent-liquidity.md)と[ADR0232](../../../adr/0232-reward-buy-and-permanent-liquidity.md)。ADR0087・0089・0091・0095・0101・0114・0124・0177・0179、ADR-v2-0021〜0023、RFC-v2-0011の現行決定を改訂し、登録・市場・会計・移行・索引の参照を接続した。既存の番号や統合原条文を削除・再番号付けしていない。

現行目録はADR228件、RFC65件、ADR-v2 50件、RFC-v2 20件である。新文書番号をwire ID又はnetwork activationと扱わない。

## 2. 算術と参照遷移

整数kernelの一般的な証明は[MATH_PROOF.md](MATH_PROOF.md)、有限ケースの実行結果は[liquidity-math-check.json](liquidity-math-check.json)に記録する。

| 検査 | 件数 | 結果 |
| --- | ---: | --- |
| 小整数の全探索 | 401,600 | PASS |
| 広い整数範囲の検査 | 10,000 | PASS |
| 64系列の混合遷移 | 16,384操作 | PASS |

確認対象は`ell<=q`、BILI・Position保存、余剰のみburn、curve product非減少、LP部分の価格丸め、原子的dust・期限・receipt・再実行の参照会計である。有限テストを全入力の数学的証明や本番実装の実証に読み替えない。

旧seed_lockedへ20%のAを足すと資金のある正常sellを拒否する反例、購入量の固定2/3をLPへ戻すと比率が変わる反例、整数丸めで余剰burnが0になる例も保存した。

## 3. 保存性

[check_preservation.py](check_preservation.py)を直前MOE1との比較で実行し、[preservation-check.json](preservation-check.json)はPASS、差分違反0だった。

| 保護対象 | 件数 | 結果 |
| --- | ---: | --- |
| RFC-v2原条文収録以降のsuffix | 15 | byte同一 |
| ADR_SOURCEブロック | 204 | byte同一 |
| TB1/FOW1保護ブロック・単独マーカー | 195＋2 | byte同一／出現数維持 |
| MOE1保護ブロック | 49 | byte同一 |
| history・evidence・audit関連 | 1,446ファイル | byte同一 |
| 過去revision全体 | 380ファイル | byte同一 |
| 旧checker | 3ファイル | byte同一 |
| source-manifest・既知リンクbaseline | 11＋1ファイル | byte同一 |

上の分類には重複がある。合計を異なるファイル数として扱わない。旧MOE1の原文は履歴として保持し、現行MOE2の優先節と本文改訂によって適用範囲を分けた。

## 4. 既存checkerの結果を隠さない

旧checkerと基準を改変せず実行した。元のMOE1にも既存FAILがあり、今回の改訂をもって全体PASSとは報告しない。

| 既存checker | MOE1の診断数 | MOE2の診断数 | 差分 |
| --- | ---: | ---: | --- |
| maintenance/check_docs.py | 947 | 947 | 新規診断なし |
| rfc-v2/check_integrity.py | 7 | 7 | 新規診断なし |
| adr-v2/check_integrity.py | 150 | 162 | 授権済み本文・旧原本のchecksum比較12件 |

追加12件は、現行ADR-v2のMOE2適用節7件と、ADR0090へのMOE2優先節追加に関する旧本文・行範囲・Status・appendix・baselineの比較5件である。いずれも今回の実差分・変更前完全原本へ結び付けて分類した。旧checksumを新しい本文に合わせて書き換えていない。

改訂中に見つかったADR0087の旧見出し参照は互換anchorを残して修復した。最終比較の**説明のつかない新規診断は0**。ログ・比較は[checker-comparison.json](checker-comparison.json)及び`checker-after/`に保存する。[revision-check.json](revision-check.json)のPASSは、この改訂範囲・保存性・参照算術を意味する。

## 5. 有効化前に残る範囲

- whole Positionでは、正のAでもell=0が起こる。特にU<=2ではこのkernelで正のペアLPが成立しない。これは市場未準備ではなく既存市場の整数数量の境界である。元の有限期限内の全体未commit、未成功終端を規定し、全状態でのLP成立は主張しない。
- LiquidityDust自体は計算不正・slash証拠ではない。追加mint、retired復活、片側donation、成功報酬だけの先行発行へ変換しない。
- 永久receiptは引出し不能な拠出履歴であり、現在残高・資産価値・引出し下限ではない。swap用資産を別lock残高として二重計上しない。
- 永久LP記録済みmodel ID／artifact rootはcore指定不可であり、alias・pruning・後継移植で解除しない。永久LPのない旧権利の移行だけを元契約で扱う。
- quote・金額・価格影響・minOut・決済期限の具体的profile、native整数幅、receipt codec、EVM整合、再起動・IBD・reorg、移行と役務原資を実装検証する必要がある。
- 将来価格、利益、全量売却可能性、MEV不採算、DAG安全性、LLM計算費用の下界をこの改訂で証明したとは扱わない。
