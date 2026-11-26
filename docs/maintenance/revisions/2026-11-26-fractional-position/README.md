# MOE3/BMR1 — 全モデル1.0・50/20/20/5/5・モデル廃止とCourt49/51

2026-11-26（Asia/Tokyo）。**ADOPTED_DESIGN_NOT_ACTIVATED**。

今回の全ユーザー指示と最後の訂正を反映した設計改訂。新runtime接続・実BFT投票・資産burn・公開activationは実行していない。既存private MOE2 native実装と過去結果は保存する。

## 正本

1. [RFC0068](../../../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../../../adr/0236-fractional-position-miner-rewards.md) — 小数POSITION、全モデル統一、最終coinbase配分。
2. [RFC0069](../../../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../../../adr/0237-bft-model-retirement-and-role-bonds.md) — 恒久廃止、用途別bond、検証者保護、Court49/51。
3. [MOE3移行](../../../governance/fractional-position-economics-transition.md)／[BMR1移行](../../../governance/model-retirement-transition.md) — 旧資産・署名・claim・lotの保存と新profileの境界。
4. [MOE3方針](../../../design/palw/economics/moe3-economic-policy.json)／[BMR1方針](../../../design/palw/economics/bmr1-retirement-policy.json) — 機械可読契約。
5. [報酬算術](MATH.md)／[廃止・Court算術](RETIREMENT_MATH.md)／[検証記録](VALIDATION.md) — 参照コードの試験結果と限界。

## 最終配分

| 用途 | 原資Cの名目比率 | asset |
| --- | ---: | --- |
| Miner等の採掘報酬 | 50% | 購入したPOSITION |
| LP用POSITION購入 | 20% | 購入したPOSITIONをLPへ |
| 永久LPの通貨側 | 20% | MISAKA |
| Panel・Gate C | 合計5% | MISAKA |
| BFT継続報酬 | 5% | MISAKA |

70%分でPOSITIONを一回購入し、実購入原資比20:50でLPと採掘報酬へ分ける。整数ではF=P=floor(50C/1000)、A=J=floor(200C/1000)、D=C-F-P-A-J、B=D+J、L=floor(qJ/B)、M=q-L。端数のない単一モデル例ならLはqの2/7、Mは5/7。通貨端数はMiner購入原資Dへ、POSITION端数はMへ帰属する。

全モデルの係数1.0、genesisのPOSITION有効、fork特別区分なし、共通BFT管理。通常AMMのburn5%・登録者あり1%／なし0%は継承する。50%は購入原資の名目であり、POSITIONの時価や換金額の保証ではない。

## 制裁と発見報酬

BFT恒久廃止は対象poolの現存MISAKA、対象登録者/採掘bond、未確定採掘報酬を100%burn又は未発行取消する。Panel/Gate Cの検証bondは悪性の判別なしに保護し、正当に成立した検証報酬を支払う。個別不正はCourtで扱う。

Courtの実回収MISAKA Sは、49%を適格な不正発見・立証Panel/Gate Cへ合わせて配り、残り51%をburn。同一offenceの分割回収は累積Sから差分計算し、同じ資金を二度払わない。BFTモデル廃止には発見報酬を付けない。

## 再現

プロジェクトrootでPython標準ライブラリだけを使う。

```sh
PYTHONDONTWRITEBYTECODE=1 python3 docs/maintenance/revisions/2026-11-26-fractional-position/check_moe3.py
PYTHONDONTWRITEBYTECODE=1 python3 docs/maintenance/revisions/2026-11-26-fractional-position/check_retirement.py
python3 -m unittest discover -s docs/maintenance -p 'test_*.py' -v
python3 docs/maintenance/check_docs.py
```

報酬参照: [moe3_reference.py](moe3_reference.py)／[試験](test_moe3_reference.py)／[結果](math-check.json)。制裁参照: [retirement_reference.py](retirement_reference.py)／[試験](test_retirement_reference.py)／[結果](retirement-check.json)。

## 保存と限界

変更前bytesはbefore/へ保存し、入力ZIPのSHA-256と照合する。[入口統合](integration-changes.json)／[変更一覧](changed-existing-files.json)／[検証JSON](revision-validation.json)を参照する。元のnative、旧policy、過去evidenceと検査結果を新結果へ書き換えない。

参照コードは抽象算術・状態であり、実署名・BFT・Courtの正否・実UTXO・役務資格・native reorg/IBD・本番有効化を検査したものではない。入力ZIPの既存欠落リンクは入力との差分で報告し、baseline変更で隠さない。
