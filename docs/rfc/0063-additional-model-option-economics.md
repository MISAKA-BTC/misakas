# RFC-0063 — 追加モデル限定MisakaOption・50%買上げ・5%/3%手数料・直接移転

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](../governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。旧seedのpre-trade制限と市場の公開LP禁止は、新しい報酬専用の永久流動性操作とは区別する。
<!-- MOE2-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。経済規範の採択であり、native/EVM実装、検証、既存市場の移行、公開有効化の完了ではない。
- 論理profile: **MOE1**（MisakaOption Economics）。wire値、object tag、fence、activation heightは未割当。
- 判断記録: [ADR-0231](../adr/0231-additional-model-option-economics.md)
- 現行会計の正本: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)
- 接続: [モデル階層04](../rfc-v2/0004-model-tiers-and-annual-catalog.md)、[会計07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、[Final20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)、[TB1](0061-palw-internal-threshold-beacon.md)、[FOW1](0062-palw-function-output-and-work-admissibility.md)

## 0. 採択する経済契約

MisakaOption / ModelPositionを有効にする対象は**追加登録モデルだけ**である。genesisおよび正式なsoftfork・年次fork指定モデルでは市場、発行、売買、移転、allowance、membership等のMisakaOption機能を有効にしない。

| 項目 | ADDITIONAL | GENESIS / FORK_DESIGNATED |
| --- | --- | --- |
| MisakaOption市場・残高・直接移転 | MOE1有効下の追加登録と同時に市場を生成 | 無効 |
| 認証済み対象worker報酬Rの買上げ | 500‰、整数切捨て | 0 |
| 買上げ後の対象報酬 | Rから買上げを引いた残額。指定された報酬planで分割 | **Rの100%をMinerへ帰属** |
| RからのPanel/Verifier/owner控除 | 残額について明示された役務planの範囲。ownerへの直接legは0 | **すべて0** |
| 通常AMM売買 | BILI grossに対しburn 50‰、owner 30‰、残額がnet | 対象なし |
| 報酬買上げの追加BILI burn / owner支払 | 0 / 0。全買上げ額をreserveへ | 対象なし |
| 直接送金・受取・譲渡 | 許可。AMM売買手数料なし、通常network feeのみ | 対象なし |

買上げと通常売買を同じfee経路へ流さない。直接移転を、別名のAMM売買として課金しない。本RFCはモデル階層係数1.0/0.3、既存の総発行枠、仕事上限、成熟・export上限、Panel＋Court、TB1/FOW1の適格性を免除しない。

## 1. 金額・原資・分類

### 1.1 Rの定義

`R`は一つのcanonical claimに帰属する、適用中の仕事価格付け・発行上限によって確定する**買上げおよびPanel配分前のworker報酬**である。価格付け前の全escrow、network全体の報酬、取引手数料、provider fee、Court fee、返還金、holderの保有額はRではない。

仕事単位、二階層係数、資本配分、報酬上限を既存の規則で一度だけ適用し、その結果のRを本RFCで配分する。100%又は50%の規則を理由に新たなRを発行したり、tier係数を二度掛けたりしない。Rは最小通貨単位の非負整数であり、成功条件未成立のclaimには成功worker報酬を発行しない。

**GENESIS/FORK_DESIGNATEDの100%は、Rの帰属先をすべてMinerにする規則**である。旧ADR-0124のPanel poolをRから控除して80%をMinerへ渡す意味ではない。成熟、責任期間、bond、export capによる保留は別に維持し、即時・無条件支払を約束しない。

### 1.2 正規のモデル分類

`ModelEconomicsClass`はnativeの登録identityと有効なcatalogから解決する論理分類であり、callerが任意に指定するenumではない。

1. `GENESIS`: 固定model IDとartifact rootがgenesis editionに一致する。
2. `FORK_DESIGNATED`: genesis以外で、有効化済みの正式catalogが同じmodel IDとartifact rootを指定する。softforkおよび既存制度の年次fork指定を含み、正式指定の1.0モデルを追加モデル扱いにしない。
3. `ADDITIONAL`: 有効な追加登録であり、上記二集合のいずれにも属さない。
4. 不明なidentity、root不一致、未認証catalog、未知版は拒否する。ADDITIONAL又はGENESISへ推測で分類しない。

単なるticker、API表示、登録者の自己申告、現在の数値係数だけで分類しない。報酬を別model/line/pairへ付け替えて制度を選択することも認めない。正式指定が後から有効になる場合は§10の移行を先に完了する。

### 1.3 追加登録と市場生成は一つの原子遷移

MOE1下では、ADDITIONALのモデル登録が成功した時点で、そのmodel IDに一意に対応するMisakaOption市場が存在する。

\[
\operatorname{ModelRegistered}(m,\mathrm{ADDITIONAL})
\Longrightarrow \exists!\,\operatorname{MarketFor}(m).
\]

modelの登録、pair identity、規範の初期virtual reserve・実reserve・Position在庫・owner参照を**同じnative遷移で初期化**する。実reserveは登録規則に従う実入金額であり、外部seedがない場合は0でよい。仮想reserveを実入金と偽らず、規範曲線の初期状態を再現する。登録の一部が失敗した場合はmodelと市場の両方をrollbackし、どちらかだけ残さない。reorg/IBDでもこの原子性を維持する。

別の市場作成取引、外部seed、後続open、採掘前のmarket-ready待機を必要としない。seed/deepenは、存在する市場への任意の流動性操作であり、市場の存在条件でも50%買上げを切り替える条件でもない。GENESIS/FORK_DESIGNATEDの登録・正式指定からは市場を生成しない。

登録済みADDITIONALで市場が欠落していれば、正常な「未準備」状態ではなく、state invariant違反、不正transition又は未解消の移行不整合である。その状態を受理せず、Minerへ全額支払うfallbackにも変換しない。市場の存在は、FOW1の採掘適格性やclaimの正当性の証明とは別である。

## 2. Snapshotと時点

各claimの受理時に、次の意味を持つ`EconomicsSnapshot`を認証native recordへ一意に結合する。これは論理契約であり、既存署名codecに新fieldを暗黙追加したことを意味しない。

```text
EconomicsSnapshot = {
  network_id, economics_policy_id,
  model_id, artifact_root, catalog_root, catalog_edition,
  model_class, pricing_policy_id, reward_plan_id,
  pair_id_or_none, owner_rule_ref,
  reward_amount_basis, admission_anchor,
  original_timing_and_reservation_refs
}
```

ADDITIONALの`pair_id_or_none`は登録時に同時生成した一意のpairを指し、Noneを許さない。GENESIS/FORK_DESIGNATEDではNoneとし、別model又は別lineのpairで代替しない。

FOW1ではsnapshotへの参照と必要な規則を試行文脈へ入れ、TB1 EXECの公開前固定へ結合する。Rの額はそのsnapshotが規定する価格付けに従って確定し、恣意的な後付けをしない。支払先、pair、配分順、端数、原期限、予約も同じ契約に従う。

通常のAMM売買、transfer、approve、transferFromは、その取引を実行するcanonical状態の有効MOE1とモデル分類を確認する。quote時点の古い分類・fee表示だけで後続取引を受理しない。既受理claimの義務は署名済みsnapshotで処理するが、将来の正式指定の有効化前に旧additional義務をdrainするため、指定後にそのモデルの新しいOption操作を継続する経路にはしない。

## 3. モデル別の報酬分配

### 3.1 ADDITIONAL

\[
B=\left\lfloor\frac{500R}{1000}\right\rfloor,\qquad D=R-B.
\]

`B`は当該claimが認証されたmodel/lineのpairへの買上げ額、`D`は残余worker報酬である。Bはholder、owner、protocol treasuryへの現金配当ではない。

残余DのProducer/Panelおよび既存の適法なprefix worker等への配分は、snapshotに固定した`reward_plan_id`で決める。ほかの報酬legを持たず、旧ADR-0124の20% Panel poolを採るplanなら、

\[
P=\left\lfloor\frac{200D}{1000}\right\rfloor,\qquad M=D-P.
\]

本改訂で全profileへ20%を自動採用しない。採用するplanは役務資格、未使用poolの帰属、期限、slashとの関係を完全指定し、費用だけを別名にして二重に支払わない。planが不明なら新規の有償claimを受理しない。

### 3.2 GENESIS / FORK_DESIGNATED

\[
B=0,\qquad \operatorname{OwnerFromR}=0,\qquad
\operatorname{PanelFromR}=0,\qquad M=R.
\]

Option市場が存在しないから偶然全額になるfallbackではなく、正規分類による本来の配分である。Rからowner、holder、Panel、Verifier、prefix export worker、TB1役務又は別のreserveへ振り替えない。Rを減らす「特別手数料」「未配分pool」「準備金」も設けない。必要なprefix供給等の役務も§4の別原資へ接続する。

### 3.3 1,000 BILIの例

| model class | 買上げB | RからPanel pool | Miner M | 合計 |
| --- | ---: | ---: | ---: | ---: |
| ADDITIONAL、残余の20% poolを採用するplan | 500 | 100 | 400 | 1,000 |
| GENESIS | 0 | 0 | 1,000 | 1,000 |
| FORK_DESIGNATED | 0 | 0 | 1,000 | 1,000 |

この表はRからの配分を示す。別原資の検証役務費は表のRへ加算も減算もしない。

## 4. 検証役務の資金と受付

MinerへRの100%を帰属させても、全実行再検証、資料供給、TB1、Court、原期限、能動的な正直Panelの必要性は変わらない。特にGENESIS/FORK_DESIGNATEDでは、これらの役務費を**Rとは別の、受理前に資金と同意を確保した有限escrow**から支払う。

別原資は、既存のfee契約に適合する利用者/sponsorの入金、既存残高による明示同意、別に採択された有限budget等である。本RFCだけを根拠に新規発行・自動課税・無資金の将来補填を行わない。R自体又は同じ未発行Rを二重に担保化し、見かけ上だけ別口座から支払うことも禁止する。

各役務のpayer、provider、scope、上限、lock、原deadline、返還・解除条件を固定する。資金、供給資料、容量又は同意が欠ける場合は新claimの受付を停止する。Panel無報酬又は検証免除で不足を埋めない。未成功claimでも既に発生した正当な役務費は、その別契約に従って精算し、成功報酬Rを便宜発行しない。

ADDITIONALでも、D内poolと別feeの支払元・義務IDを区別する。同じ義務への二重払いを防ぎ、追加fee同意が必要な場合は受理前に済ませる。

## 5. 報酬による自動買上げ

MOE1のADDITIONAL有償claimは、登録時に同時生成された一意のpairへ結合する。§1.3の登録不変条件を検査し、必要な決済資源・役務原資・原期限を受理前に予約する。市場の後付け作成、外部seed又は準備完了を待つ独立gateは設けない。**市場欠落を正常分岐としてBを0へ戻し、MinerへR全額を払う旧fallbackは廃止する。** 一般登録には市場生成を含むが、FOW1適格性と成功claimの検証は別に必要である。

成功と支払資格が成立し、既存の成熟・hold・export制約を満たした原子的決済で、Bの全額を当該pairの実BILI reserveへ計上する。追加BILI burnとowner直接支払はともに0。pairの規範曲線・整数丸めにより買い上げたPositionをretireし、holderへ分配しない。仮想reserveを実BILI入金又は支払可能資金として扱わない。

retireは市場在庫から再販売不能な区分への移動であり、BILIの5% burnとは別である。既存の会計上の総供給とcirculating/retiredの区別を明示し、native/RPC/EVMで異なる値を返さない。原単位丸めでretire量が0になっても、Bをownerへ送ったりMinerへ返したりせず、全額reserveへの入金として記録する。

登録済み市場の新規user buyの停止だけで、既受理のbuyback義務を消さない。既受理義務を決済できる資源と権利を保持する。検証・原期限等の成功条件を満たして正規の原子決済を完了できなければ、規定された未成功終端と原資精算へ進める。無期限Pending、期限reset、成功報酬/creditだけの先行発行は認めない。市場rowの欠落・不整合そのものは正しいstateとして採用せず、通常のclaim終端分岐へ混ぜない。

取引順序はcanonical規則に従う。buybackを反映する場所・slot・順序を実装profileで固定し、送信者の任意により二重適用又は順序替えしない。買上げ額の増加は、先回り売買や価格操作を自動的に防止せず、将来価格の上昇も保証しない。

## 6. 通常AMM売買の5% / 3% / 残額

gross BILI額をgとする。通貨は最小単位で計算する。

\[
\operatorname{Burn}(g)=\left\lfloor\frac{50g}{1000}\right\rfloor,\quad
\operatorname{Owner}(g)=\left\lfloor\frac{30g}{1000}\right\rfloor,\quad
\operatorname{Net}(g)=g-\operatorname{Burn}(g)-\operatorname{Owner}(g).
\]

netを別途`floor(920g/1000)`で計算せず、保存則が成立する残額とする。92%は端数のない名目比率である。乗算に十分な幅のchecked整数を用い、overflow、負数、範囲外、二重debitを拒否する。

- **買い:** userのgross入金gを上式で分け、netを実reserveへ入れる。Position出力とminOutはnetに基づく曲線計算で確認する。
- **売り:** 入力Positionから曲線が定めるgross BILI gを実reserveから引き、burn・owner・売却者netへ分ける。minOutは売却者が実際に受け取るnetで確認する。
- **owner:** 取引時点の認証済みモデルowner規則に従う。30‰はownerと、その既存の適法なcontributor内部分配を含む**owner側総額**であり、owner3%に別のcontributor割合を加算しない。内訳の全受取額と端数はこの総額内で保存する。Position受取人がモデルownerになるわけではない。owner又は内部受取人の宛先が不明・不正な状態を、無断の追加burn又は第三者支払へ置き換えず拒否する。
- **手数料版:** 同じcanonical取引時点のMOE1をすべてのnative/EVM/RPC consumerが使用する。旧50/50のV2や旧50/10のV1を名前だけで上書きしない。

g=1,000 BILIならburn=50、owner=30、net=920である。買い・売りの各grossがそれぞれ計算基礎となり、往復費用を単純な固定8%又は16%と保証しない。

## 7. 直接送金・譲渡・受取・allowance

ADDITIONALの有効なModelPositionについて、保有者の直接transfer、他者からの受取、明示allowanceに基づくtransferFromを許す。approve/allowanceも状態の一部とする。モデルごとのnative残高が正本であり、EVMに別の独立残高を作らない。

`transfer(from,to,u)`は認証済み送信権限、正しいmodel/pair、非負整数u、残高、利用可能量を確認し、fromをu減らしtoをu増やす。market reserve、market在庫、retired量、BILI供給、owner feeを変更しない。直接移転には5%/3%のAMM手数料を課さず、通常のcarrier/network feeだけを適用する。

注文・旧清算等に有効にlockされたunitsは利用可能残高から除く。allowanceは残高のlockでも超過残高の生成でもない。Positionの送金によってBILI bond、miner/Panel/TB1の役務lease、署名責任、slash対象を移転できない。Positionを新たなconsensus bondやweightにする機能も導入しない。

EVMのtransfer/approve/transferFromを成功として返す経路は、native正本と同じtransaction journalで残高・allowance・logを原子的に扱い、失敗/revert時に全て戻す。queueへ積んだだけで送金成功を返し、その後nativeで失敗する経路を同じ機能にしない。read-after-write、二重実行拒否、owner/spender別nonce・認証、zero transfer、self transfer、0 allowance、allowance減算、イベント、reorg/IBDを定義・確認してから有効化する。具体ABIの規範は[ADR-0089](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)を参照する。

直接移転の対価を別に払う相対売買は、通常AMM売買ではないため5%/3%を自動徴収しない。チェーンが贈与と相対売買を識別し、全ての経済的売買へ8%を徴収できるとは主張しない。

## 8. Membershipと利用権

ModelPositionは依然として採掘報酬の直接配当請求権ではない。新しい保有者の認証済み残高に、定義されたmembershipと保有量tierを対応させる。転送元が以前の残高証明を出し続けて同じunitの新しい利用権を二重に取得できないよう、model、holder、認証anchor、state版、期限と利用済み権利を結合する。

すでに適法に完了したサービスは、後のtransferだけで巻き戻さない。一方、transferを繰り返して利用quotaや同じ期間の権利を無制限にresetしない。具体サービスの権利scopeと消費規則は元のgrant契約に従い、経済的配当、BILI rebate、owner権、consensus投票権を自動付与しない。

GENESIS/FORK_DESIGNATEDはMisakaOptionのmembership対象ではない。モデルの通常利用権を失わせるという意味ではなく、それぞれの非Optionサービス契約に従う。

## 9. 保存則・原子性・取消

本規則は既存のcoinbase/escrow/market会計を新規発行で補わない。少なくとも次を同じnative遷移で確認する。

1. ADDITIONALでは`R = B + D`、Dの各支払と未使用poolの総和はD。GENESIS/FORK_DESIGNATEDでは`R = M`かつRからの他のlegは0。
2. 通常売買では`g = burn + owner + net`。reserveの出入とsink/withhold/payout/burnの資金を一対一に対応させる。
3. Positionは`market_units + sum(holder_units) + retired_units = accounted_supply`。transferはholder間だけを更新し、retireされたunitsを再び送金可能にしない。
4. 同じclaimの`RewardSettlementKey`、buyback、Miner/Panel支払権利、pool残高、消費recordを原子化し、再送・別carrier・別API・reorg replayで増幅しない。
5. 買上げreserveへの流出も既存export上限へ合算する。名目配分を満たすためにhold、成熟、資本上限を外さない。枠不足は事前受付・予約又は原契約の有限精算に接続し、比率変更で吸収しない。
6. 旧Final、取引決済確定、責任期間を同一視しない。既存の取消・slash・依存・reorg規則を継承し、経済式だけでDAG安全性や既に確定した第三者取引の自動rollbackを保証しない。

誤計算、二重支払、無権限transfer等は客観的証拠に従う。正しい計算を安く行ったことや、許可された相対売買だけでslashしない。FOW1の反復費用検査を、50%買上げや手数料の高さで代替しない。

## 10. 有効化と既存残高の移行

`MOE1Activation`にはnetwork、経済policy、対象型、catalog root、各codec/ABI、claim受付境界、通常取引の適用位置、旧義務のdrain期限と根拠を含める。具体値を本書から推測してruntimeへ適用しない。

- 境界より前に受理したclaimは、その受理時の署名・価格付け・経済snapshot・原期限・責任で終える。新しい50%を遡及控除しない。
- 境界以後の新claimと通常取引にはMOE1を適用する。旧fee・非譲渡codec・買上げスキップによる新規成功fallbackを作らない。
- ADDITIONALの既存残高・retired量・実reserve・元seed・既存支払権利は保存し、移行の前後で保存則を照合する。移行時点で全対象に§1.3の一意市場不変条件を確認する。旧snapshotに欠落があれば、明示したmigration検証・修復を有効化前に完了し、正常な後付け市場作成待ちへ移さない。過去の非譲渡残高を二重発行せず、新しい移転機能から同じnative残高を扱う。
- **GENESIS/FORK_DESIGNATEDにはOption市場・残高・allowance・未清算義務を持ち越さない。** 旧状態にそれらがある場合は、明示した移行計画で既存権利と資金を清算し、その完了を新経済規則のactivation gateにする。未完了なら該当切替は有効化しない。
- coreモデルの旧市場をsell-onlyとして新profileで残すこと、未知価格で保有者を強制償還すること、無同意の没収、reserve不足を新規発行で埋めることを本RFCから許可しない。旧履歴のread-only表示は保持できる。
- ADDITIONALが将来FORK_DESIGNATEDになる場合も、正式指定が経済上有効になる前に旧additional claim・pair・holder/allowance・役務義務のdrainを完了する。未完了の正式指定を先に適用し、後で100%例外を作らない。

直接移転、native/EVM原子実行、全consumer、資源・支払原資、既存残高移行、復旧を実装・検証するまでMOE1は未有効化である。各stageの実装・測定・採択・activationを別に記録する。

## 11. 旧条項の置換

| 対象 | MOE1で置換する規則 |
| --- | --- |
| ADR0087 Decision4/5・旧市場開設条件 | 追加登録と市場生成は原子的。追加モデルだけ、通常売買5%/3%、直接移転許可 |
| ADR0089・ADR0101 | transfer/approve/transferFrom/allowanceの一律拒否を撤回。native正本の原子的EVM対応と受取人membership |
| ADR0091 Decision1/3 | 追加モデルの買上げ500‰。pair不在で全額Minerへ戻すfallbackを廃止。coreは本来100%Miner |
| ADR0095 | サービスmembershipは維持し、移転後の保有者に権利を結合。直接配当権にはしない |
| ADR0114 | 旧50/50の売買feeを新50/30へ。旧versionの意味は保存 |
| ADR0124 Decision1/6 | 追加モデルは買上げ後のDへ版付きPanel pool。coreではRからPanel控除0 |
| ADR0177 | モデルownerの通常売買収入は3%。Rからのowner直接配当0を維持 |
| ADR0179・旧ADR0162のcore sell-only | 追加モデル限定を強化。coreの旧市場・残高はactivation前に清算 |
| ADR-v2-0021/0023・RFC-v2-0011 | 現行著述をこの分類・比率・送受信・原資区分へ更新 |

TB1/FOW1は実行の適格性、乱数、Ticket、全検証、一回会計の規範を維持する。その「既存経済値を維持」という旧一般句は、本RFCで明示した経済値と対象範囲に限り後続改訂を受ける。反復費用検査や完全BGWの未完成を、本経済改訂によって解決済みとはしない。

## 12. 有効化前の受入条件

GENESIS/FORK_DESIGNATED/ADDITIONALと未知分類、追加登録・市場初期化の原子成功/失敗、重複市場・市場欠落の不変条件違反拒否、R=0/1/奇数/最大値、通常売買の端数・overflow、owner更新、既存市場のpauseと旧義務、原期限、別原資不足、snapshotをまたぐactivation、将来指定、全二重支払、transfer/allowance/revert、locked units、membership再利用、reorg/restart/IBDを確認する。

市場操作・先回り・自己保有者による採掘と売買の組合せ、手数料を課さない相対取引の影響、holder集中、最悪時の役務資金も評価対象である。保存則や例示計算の成功だけで経済均衡・価格上昇・投資収益・攻撃の不採算を証明したとはしない。
