# ADR-0232 — 30%買付・20%永久流動性・余剰Position burnを採択する

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)
- 現行会計: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)
- 共通移行: [経済profileの適用・移行](../governance/model-option-economics-transition.md)
- 前版: [RFC0063](../rfc/0063-additional-model-option-economics.md)／[ADR0231](0231-additional-model-option-economics.md)

## 背景

MOE1は追加モデルの対象worker報酬Rの50%でPositionを取得し、全量retireする規則だった。新しい方針では、Rの30%でPositionを買い、その一部とRの20%のBILIを永久流動性へ入れ、余剰Positionだけをburnする。残余は元の版付きworker配分へ進める。

既存市場はvirtual reserveを含む整数curveであり、旧seedは通常売却でも払い出せない下限だった。この20%を旧seedへ追加するだけでは、swapに使える流動性という新方針にならない。また、全取得Positionを最初にretireするとLPへ戻せない。金額・Position・永久権限制約を一つの遷移として改める。

## 決定

1. **MOE2を採択する。** wire、codec、ABI、fence、activation位置は未割当。旧claimと過去実装を新規則へ遡及変更しない。
2. **RをB/A/Dへ保存分割する。** `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A`。DのPanel等の配分は受理時planへ従い、全profileへ20%を追加しない。coreはB=A=0、Miner=Rを維持する。
3. **一つの整数kernelに固定する。** `X=V+r>0, U>=1`に対し、`X1=X+B`、`U1=ceil(XU/X1)`、`q=U-U1`、`ell=floor(AU1/X1)`、`burn=q-ell`。結果は`r2=r+B+A`、`U2=U1+ell`、`retired2=retired+burn`。Vと旧seed_lockedは不変。qは一時数量であり、先に全量retireしない。
4. **整数境界をそのまま適用する。** A<=Bからell<=q。余剰burn=0も正常であり、連続3:2から整数でもell/q<2/3と主張しない。例X100/U60/R10ではq=ell=1、burn=0。
5. **dustを成功へ変換しない。** B>0かつq=0、又はA>0かつell=0はLiquidityDustで全操作未commit。原有限期限内だけhold・再評価し、解消しなければ既定未成功終端。成功R/creditの先行発行、期限reset、donation、mint、手数料版変更、全額Minerへのfallbackを設けない。B=A=0は市場二legだけno-opでD=Rの元配分を維持する。A=0の成功買付はq全量burn、LP操作・receiptなし。
6. **永久LPを非償還native履歴として記録する。** A>0かつell>0を成功commitした時だけ、`LockedLiquidityReceipt`を`PERMANENT_PROTOCOL_LOCK`へ結合する。撤去、償還、transfer、fee請求、owner/admin鍵、期限解除を設けない。外部流通LP token又は公開LP入口を追加しない。
7. **永久lockとswapを区別する。** Aとellは通常swap用reserve/在庫へ一度だけ入り、receiptは追加assetでも第四の供給項でもない。旧seed_lockedとVを増やさず、通常swapを禁止する「同じcoinが永久に動かない」という意味にしない。
8. **永久LP付きモデルをcoreへ指定しない。** 成功receiptがあるmodel IDとcanonical artifact rootの履歴を認証する。同rootの再登録、owner変更、pruning、後継移植でcore指定禁止を迂回させない。永久LPをwinddownで解除しない。永久LPのない旧cohortだけを元条件でdrainし、core旧市場の解消条件へ接続する。
9. **入口と通常経済は維持する。** 追加登録と一意市場の生成は原子的。genesis/正式fork指定にはOptionを作らずR全額をMinerへ帰属させ、Panel等は別の事前原資で賄う。通常AMMはburn5%・owner側総額3%・net差分、直接移転はAMM手数料なし。内部買付・LPの追加BILI burn/owner支払は0。
10. **全工程を原子的に一回だけ決済する。** B/A流出、q/ell/burn、D、receipt、型制約、消費キーを同じnative遷移へ結合する。市場順序、quote・amount上限、整数qに対するminOut、価格影響、slippage、原期限を経済snapshotに固定する。B+Aも共通export capへ合算し、途中取引やreplay/IBDで増幅させない。

receiptのsettlement IDは固定claim文脈から事前決定する。前market参照とreceipt自身を除いた後market値を記録し、global post-state rootの自己参照を作らない。累積A/ellは拠出履歴であり、現在残高や担保価値ではない。成熟後の責任精算もLPの特別withdrawで行わず、元のbond/holdを維持する。noncanonical reorgはreceiptを含む原子遷移全体の巻戻しであり、私的な解除権とは区別する。

## 優先関係

本採択は、RFC0063・ADR0231・ADR0091の50%買上げと全量retire、ADR0090のLP記録不在、ADR0162の旧kernel・seedとの関係、永久LP保有市場のcore移行をRFC0064の範囲で置換する。pre-trade seedの元契約、通常売買、移転、core R100%、資本・発行・成熟・hold/exportの上限は継承する。

TB1/FOW1、全実行検証、一人の有効な反証、Court、旧claimの元期限と責任は変更しない。旧MOE1の原文・codec・fence・測定は版固有の履歴として保全する。

## 効果と限界

整数kernelは購入量の範囲でLPへ戻すPositionを決め、余剰burnと資金保存を一意にする。永久receiptに私的引出し権を与えず、市場内の流動性として利用する。

この算術は、価格上昇、holder利益、任意売却数量へのreal reserve十分性、先回り取引の不採算、LLM仕事費用の下界を証明しない。整数dustは活性制約として残る。最小仕事・quote・amount/slippage上限と有限終端を測定・実装し、既存市場への移行、native/EVM、一回性、reorg/restart/IBDを確認するまで未有効化とする。
