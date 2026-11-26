# ADR-0236 — 小数POSITION・全モデル1.0・50/20/20/5/5

- 日付: 2026-11-26（Asia/Tokyo、最終の割合指定を反映）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 承認: 今回のユーザーによる設計変更、小数対応、全モデル統一、支払asset、最終50/20/20/5/5の明示指示。
- 正本: [RFC0068](../rfc/0068-fractional-position-miner-rewards.md)
- 移行: [MOE3移行](../governance/fractional-position-economics-transition.md)
- 恒久廃止: [RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)
- 検証: [改訂記録](../maintenance/revisions/2026-11-26-fractional-position/README.md)

## 1. 最終決定

全モデルにPOSITIONを有効化し、係数1.0、同じ市場・報酬・BFT管理を適用する。genesisの差は登録者不在のみ。forkモデルの特別分類・特別経済、追加モデル専用の集約発行枠を廃止する。

同じ通常coinbase配分原資Cを、Miner等へのPOSITION報酬50%、LP用POSITION購入20%、MISAKA永久LP20%、Panel/Gate CのMISAKA報酬5%、BFTのMISAKA継続報酬5%へ分ける。Miner等の50%もPOSITIONを買う原資なので、合計70%を一度に購入へ使う。Panel/Gate Cは合わせて5%であり、それぞれ5%ではない。

同日の途中説明である80%購入・20%/60%POSITION、BFT10%、旧Panel率による可変枠及び途中の40/20/20/15/5は最新指定に置換する。旧MOE2及び以前の署名済みclaimは元の版で再生する。

## 2. 採用する具体化

1. POSITIONは小数8桁。IDは離散識別子のまま、数量だけを整数atomsで扱う。格納u128、二因子積u256、多因子比較はu512等でoverflowを拒否する。
2. CからF5%、P5%、A20%、J20%をfloorし、Dを残差50%とする。通貨端数はDへ、購入qからLPのLを引いたPOSITION端数はMiner等へ帰属する。
3. `B=D+J`で一回購入、`L=floor(qJ/B)`をLP、`M=q-L`をMiner等へ。端数なしならqの2/7と5/7。実予算比を使い、常時q*2//7へ置換しない。
4. 複数モデルでもCを一回分割する。A/Jは同じ配分、Dは2Jと名目10%と端数の追加予算Eを結合按分し、LP購入だけ正でMiner原資が0となる丸めを避ける。
5. P/Fと市場原資を別の認証予約へ結合する。市場dust・モデル廃止で正当に成立した検証役務のMISAKA支払権を消さない。
6. 通常売買はburn5%、登録者あり1%、元genesis等の正規な登録者不在は0%。内部報酬購入・LP・配布、直接POSITION転送は追加AMM fee0。
7. 全モデル1.0とするが、仕事価格、bond配分、共通発行上限、成熟、hold、個別claim検証は維持する。
8. genesis専用BFT免除と新FORK_DESIGNATEDを廃止する。旧originは来歴として保持し、model ID/rootの不変性は維持する。
9. 可逆burn-pauseは全モデル共通に、登録者認証又は専用BFT BurnResume承認と同じ規定burnで解除できる。独立safety stop・恒久廃止は解除しない。
10. 永久LPは通常の撤去・償還権を持たない。RFC0069のモデル恒久廃止では現存real MISAKAをburnする限定例外を設け、BFT又は他者への払出しにしない。
11. 旧whole残高は一回だけ10^8倍し、通貨額・署名・過去receiptを保持する。市場なしgenesis等は認証移行で一意生成し、旧市場をresetしない。

12. Court個別slashの実回収MISAKAは49%を正規の不正発見・立証Panel/Gate Cへ配り、残り51%をburnする。合計49%であり、BFTモデル廃止100%burnとは別原資・別権限とする。

## 3. 旧決定の置換

| 旧規範 | 新profileで置換するもの |
| --- | --- |
| ADR0090 | whole数量、decimals=0 |
| RFC0063／ADR0231 | 追加モデル限定、core市場なし・直接R、owner3% |
| RFC0064／ADR0232 | 30%購入・20%LP・残余MISAKA、価格比例LP、余剰POSITION burn |
| ADR0087／0089／0091／0095／0101／0114／0124／0177 | 数量、対象、fee、支払assetと原資の関連条項 |
| ADR0179／PRINCIPLESの二階層 | 1.0/0.3、追加モデル専用枠、fork昇格 |
| RFC0067／ADR0235 | genesis専用BFT免除、fork特別指定、origin別市場・報酬 |
| RFC0053／ADR0221 | genesis/forkのburn-pause免除、登録者だけのResume |

恒久廃止と用途別bondはADR0237に明示採択する。過去の測定・native/private MOE2・旧policyは保存し、過去の成功結果を新実装の証拠へ読み替えない。

## 4. 判断とレビュー

50%のPOSITION報酬をMISAKAのまま払う、20%LP購入を追加mintで賄う、Panel/Gate CをPOSITIONへ混ぜる、BFTへ5%を二重控除する方式は採用しない。購入数量の1/4又は20%をLPへ回す旧途中案も最終比率と一致しない。

固定LPの数量は購入後のreserve比率と一致しないため価格中立ではない。50%相当は購入原資の配分であり時価・換金額保証ではない。8桁でも極小量dustは残る。独立レビューで多因子overflow、複数モデルの端数、Panel/Gate C権利の別予約、既存停止状態の保存を確認する。

今回の参照モデルは算術・抽象journalである。新native、実BFT署名、役務資格・資金、移行、RPC/ABI、実reorg/IBD、有効化を完了したとはしない。
