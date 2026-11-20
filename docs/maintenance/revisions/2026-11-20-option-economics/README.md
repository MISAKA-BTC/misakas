# MOE1経済設計改訂・再配布記録

日付: **2026-11-20（Asia/Tokyo）**。状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

この改訂は、追加モデルだけにMisakaOption / ModelPositionを適用し、対象worker報酬の50%買上げ、通常売買の5% burn・3% owner配分、直接移転を採択する。GENESISと正式softfork・年次fork指定モデルにはOption市場を作らず、対象worker報酬Rの100%をMinerに帰属させる。

**追加モデルの登録と市場生成は同じ原子的なnative状態遷移である。** 登録済みなのに市場を作っていない正常状態、市場準備待ち、外部seed待ちは設けない。登録の一部が失敗すればmodelと市場の双方をrollbackする。登録済み追加モデルの市場欠落は状態不変条件違反であり、買上げ免除やMinerへの全額払いに変換しない。

## 正本と読む順序

1. [RFC-v2-0011 会計・手数料・支払](../../../rfc-v2/0011-bond-budgets-fees-and-payments.md) — 現行入口の本文改訂。
2. [RFC-0063](../../../rfc/0063-additional-model-option-economics.md) — 分類、原子登録、snapshot、配分、transfer、移行の統合規範。
3. [ADR-0231](../../../adr/0231-additional-model-option-economics.md) — 採択理由と置換範囲。
4. [経済移行契約](../../../governance/model-option-economics-transition.md) — 旧条項との優先順位、旧claim、既存市場、実装・有効化条件。
5. [変更対象と旧記載の扱い](SUPERSESSION.md)、[検証結果](VALIDATION.md)。

## 採択したルール

| 項目 | ADDITIONAL | GENESIS / FORK_DESIGNATED |
| --- | --- | --- |
| MisakaOption市場 | 登録と同時に一意市場を生成 | 生成しない・有効にしない |
| 対象worker報酬Rの買上げ | floor(500R/1000) | 0 |
| Miner等への残余 | 残額を当該版のreward planで配分 | MinerにRの100% |
| RからのPanel・owner等の控除 | 明記された残余planのみ。owner直接legは0 | 0 |
| 検証・資料供給等の役務 | 残余pool又は別の事前同意・資金契約 | Rと別の有限・事前資金契約 |
| 通常AMM売買 | burn 5%、owner側合計3%、残額net | 対象なし |
| 内部buyback | 追加BILI burn 0、owner直接支払0、全額reserve、Positionをretire | 対象なし |
| 直接送受信・譲渡 | 許可。AMM手数料なし、通常network feeのみ | 対象なし |

Rは、モデル係数・価格付け・上限を適用した後、買上げとPanel配分の前に確定するworker報酬である。network全体の全収入ではない。100%は帰属率であり、認証・成熟・hold・export上限の免除ではない。

R=1,000 BILIの場合、GENESISとFORK_DESIGNATEDはMiner=1,000。ADDITIONALは買上げ500、残余500になる。旧20% Panel poolを残余へ適用するplanの例ならPanel=100、Miner=400。20%を全profileの共通定数として新設していない。

通常AMMのgrossが1,000 BILIならburn50、owner30、net920。整数端数は `net = gross - burn - owner` とし、netまで独立に切り捨てて通貨を失わせない。owner30‰にはcontributorへの内部配分も含み、3%に上乗せしない。

## 市場生成、送金、membership

市場の登録時初期化にはmodel/pair identity、owner参照、規範のvirtual reserve、実reserve、Position在庫を含める。実入金がなければ初期実reserveは0でよい。外部seed/deepenは既存市場への任意の流動性操作であり、市場生成の前提ではない。仮想reserveを実BILIとして支払うことはできず、市場の存在は任意額の売却保証でもFOW1採掘適格性の証明でもない。

transfer、approve、allowance、transferFromを追加モデルの単一native残高へ接続する。EVMは同一transaction journalで残高・allowance・ログを更新し、revert時には一緒に戻す。受付queueへ積んだことを送金完了として返さない。署名・spender認証を検査し、Position移転でbond、ExecCredit、既往claim責任、登録owner権は移転させない。

membershipは受取後の正規残高へ対応させる。保有期間はtier閾値ごとに管理し、少額受取で既存期間を消したり、送信者の長期保有期間を引き継いだり、古い少量保有を上位tierの待機完了に転用したりしない。

## 元資料、変更方法、履歴

再配布の元は、この会話の作業領域にある直前の完成版 `misaka_docs_TB1_FOW1_20261120.zip`（2,702ファイル）である。

```text
SHA-256: 2c7491f244244857850ce93a828c709d164c7265de4edeb8fab810702dc351fe
```

文面中に言及された別名の未取得ZIPを確認済みとは扱わない。この版は直前のTB1/FOW1改訂を引き継ぎ、その上にユーザーが指定した経済改訂と、登録時の市場同時生成の訂正を加えたもの。

原ADR0087・0089・0091・0095・0101・0114・0124・0177・0179は、対象Decisionを現行規範として全面改訂した。ADR-v2-0021/0023、RFC-v2-0011の著述本文も更新した。ADR0162と登録・階層・claim・Panel・Court・Final・EVM・予算の隣接入口には、適用範囲と優先正本を明示した。

各変更前ファイルを `before/<元のdocs相対パス>.txt` にそのまま保存した。例えば旧ADR0087は `.md.txt` であり、内容のbytesは元Markdownと同一である。拡張子の追加は、歴史上の相対リンクを移動先から新しい現行文書として走査しないためである。

RFC-v2の旧原条文、ADR-v2の`ADR_SOURCE` payload、旧測定・証拠・履歴・過去改訂は保全する。履歴に残る5%買上げ、owner5%、非譲渡、seed待ち等は新profileの現行規範ではない。旧source-manifestやcheckersを新仕様に合わせて書き換え、昔の検査を成功したようには見せない。

## 実装と有効化の境界

これは設計文書の改訂であり、native/EVM実装、ネットワーク有効化、具体的な旧市場清算、検証役務の資金供給を完了した報告ではない。activationには実装・相互運用・rollback/IBD・サービス原資・旧cohort処理が必要となる。

旧core市場を持つcohortは、新profileの有効化又は正式core指定の前に既存権利と義務をwinddownする。無断没収、無原資補填、coreの恒久的sell-only市場を新規範の例外として残さない。既受理claimを新比率へ遡及変更せず、元snapshot・原期限に従って処理する。

TB1の用途別乱数、FOW1のLLM関数由来採掘値と費用適格性、正直Panel・Court・資料・合意条件は変更しない。経済比率や市場同時生成から、Qwen等の費用下界やPoWの完成を新たに証明したとは主張しない。

## 検査ファイル

- [会計方針の構造化記録](../../../design/palw/economics/moe1-economic-policy.json)
- [整数会計検査](check_accounting.py) / [結果](accounting-check.json)
- [旧記載インベントリ](economic-legacy-inventory.json) / [統合範囲監査](economic-scope-audit.json)
- [独立最終レビュー](economic-final-independent-review.json)
- [保全検査](check_preservation.py) / [結果](preservation-check.json)
- [変更前manifest](baseline-manifest.json) / [変更一覧](changes.json)
- [改訂範囲検査](check_revision.py) / [結果](revision-check.json)
- [新旧checker比較](checker-comparison.json)

検査の範囲と未解消の旧診断は[検証結果](VALIDATION.md)に示す。
