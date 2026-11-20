# MOE2 — 30%購入・20%永久流動性・50%残余

2026-11-20（Asia/Tokyo）。**ADOPTED_DESIGN_NOT_ACTIVATED**。

直前MOE1の50%全購入・全retireを、**30%購入→20%BILIと購入Positionの必要分を同じ市場へ永久LP拠出→余剰だけburn→残余50%を報酬planで分配**へ変更した。追加モデル限定、市場の登録時原子生成、coreの市場なし・対象R100%Miner、通常AMM5%/3%、直接送受信を維持する。

## 読む順序

1. [ADR0091](../../../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) — 報酬の中心規則。
2. [新ADR0232](../../../adr/0232-reward-buy-and-permanent-liquidity.md)／[新RFC0064](../../../rfc/0064-reward-buy-and-permanent-liquidity.md) — 採択と固定kernel。
3. [RFC-v2-0011](../../../rfc-v2/0011-bond-budgets-fees-and-payments.md)／[経済移行契約](../../../governance/model-option-economics-transition.md)。
4. [数学証明と反例](MATH_PROOF.md)／[検証記録](VALIDATION.md)。

## 実際に固定した構成

B=floor(300R/1000)、A=floor(200R/1000)、D=R-B-A。R=1,000なら300/200/500で、旧残余20%Panel planの例はPanel100・Miner400。LPの20%とPanelの20%を重複計上しない。

既存の仮想reserve/whole Positionへ接続するため、LP量は購入後の比率からfloor(A·U1/(X+B))とする。購入qを超えないことは整数でも証明できる。購入分の2/3を機械的に戻す方法や、最低1/3のburnを整数に保証する方法は採らない。

永久LPはnativeの引出し・償還・転送・LP fee請求不能な記録。外部流通LP tokenや公開LP入口は新設しない。実資産は通常swapに使い、Aを旧seed_lockedへ加えない。累積拠出は現在残高・担保価値ではない。

whole-unit dustは全体未commitのまま元の有限期限内だけ保留し、解消しなければ既定未成功終端へ進む。追加mint、retired復活、片側donation、成功報酬だけの先行支払は禁止。全価格・在庫・報酬で毎回正のLPが作れる保証はない。

永久LP記録済みのmodel IDはcore指定不可とする。永久LPをwinddownで解約する例外や、同rootの別名再登録・後継移植での迂回は認めない。

## 元資料と保全

元はこの会話で作成した `misaka_docs_MOE1_20261120.zip`、SHA-256 `ea6d8aed855815c213a3b2fb06e061945ad9296892281bdead4283a84b159c57`、既存2,817ファイルである。変更前原本は本改訂の `before/<元のdocs相対パス>.txt` へexact bytesで保存する。

旧埋込原条文、TB1/FOW1とMOE1の採択ブロック、旧測定・過去revisionを保全し、旧source-manifestやchecker基準を更新してPASSを装わない。

- [基準manifest](baseline-manifest.json) / [変更一覧](changes.json)
- [数学検査](check_liquidity_math.py) / [結果](liquidity-math-check.json)
- [改訂・保全結果](revision-check.json)
- [旧checker比較](checker-comparison.json)
- [MOE2構造化方針](../../../design/palw/economics/moe2-economic-policy.json)

この納品は設計文書であり、native/EVM実装、network activation、MEV安全性、全状態での経済活性やLLM-PoW費用定理の完成ではない。quote・量・価格影響上限、元期限、receipt/reorg/IBD、移行、役務原資を実装・検査してから有効化する。
