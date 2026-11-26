# ADR-0089 — native正本とEVMのMisakaOption転送・承認・決済

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](../governance/fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

- 改訂日: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。MOE2の設計改訂であり、native/EVM実装、測定、移行又は公開有効化の完了ではない。
- 共通正本: [RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md) / [ADR0232](0232-reward-buy-and-permanent-liquidity.md) / [RFC-v2-0011](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0011-bond-budgets-fees-and-payments.md)。
- 直前MOE1版の完全原本: [改訂前の原本](../maintenance/revisions/2026-11-20-permanent-liquidity/before/adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md.txt)。さらに古い原本への参照も保存原本に保持する。

本書の現行決定をMOE2へ適用する。MOE1の「50%全額でPosition購入・全取得分retire」を「30%購入・20%BILIと必要Positionの永久LP・余剰Positionのみburn」へ置換する。通常5%/3%、追加モデル限定、登録と市場の原子生成、直接転送、coreのMiner100%は維持する。旧claim・署名・決済・過去の試験結果はその版の記録であり、新規範で遡及再解釈しない。TB1/FOW1の実行・登録・完全検証・責任条件を免除しない。

## 1. 一つの資産と適用範囲

ModelPositionの資産正本はnative foldであり、EVM facadeが独立にmint又は残高を複製しない。MOE2の`ADDITIONAL`だけが対象であり、GENESIS/FORK_DESIGNATEDへはmarket・facadeのwrite・transfer・allowance・報酬購入・永久LPを開かない。coreを架空の追加lineでwrapして同じ市場へ接続しない。追加モデルの登録と一意市場の生成は同じnative遷移で完了し、EVMが後から別pairを作らない。実reserve初期値0は許され、任意seed/deepenを市場生成の前提にしない。登録済み追加モデルの市場欠落は状態不整合として拒否する。

既存のsystem address、旧ABI、過去のqueue・settlement記録は旧版の意味を持つ。新機能のwire/version/activationは別に固定し、予約済みselectorやaction tagを文書番号から割り当てない。以下は実装すべき新契約であり、旧`palw_model_evm`fenceをarmしただけで対応済みとはしない。

## 2. 読出しとauthority

registry、market quote、Position残高、allowance、model分類、economics policy、pending settlement、永久LP receiptと`hasPermanentLP`を、認証された同じnative状態から解決する。quoteは実行時の約定を保証せず、policy版・state anchor・手数料・最終受取額を示す。実行ではその版の`minOut`を再確認する。

holderはnetworkと認証namespaceを含むidentityである。EVM addressとnative ML-DSA identityを長さ・先頭byte・同じ表示文字列だけで同一視しない。EVMの20byte `transfer`宛先はそのlaneのcanonical holderへ写像する。native宛先を扱う拡張を設ける場合は、明示的なtyped recipientの別ABIとし、同じ単一台帳で移す。別chainの残高コピーを許可しない。

`msg.sender`が残高owner又は許可されたspenderであることを検査する。旧Decision 4の`msg.sender == tx.origin`及び`EXTCODESIZE(sender) == 0`を、新転送・承認の権限規則に継承しない。contract wallet、multisig、contract spenderも、同じ残高・allowance・revert規則を満たせば扱える。権限に`tx.origin`を使わない。

## 3. EVMの転送・承認interface

新profileには次の操作を設ける。完成したERC-20互換性を、文書だけから宣言しない。

```solidity
function balanceOf(address holder) external view returns (uint256);
function transfer(address to, uint256 value) external returns (bool);
function allowance(address owner, address spender) external view returns (uint256);
function approve(address spender, uint256 value) external returns (bool);
function transferFrom(address from, address to, uint256 value) external returns (bool);
event Transfer(address indexed from, address indexed to, uint256 value);
event Approval(address indexed owner, address indexed spender, uint256 value);
```

MOE2では以下を要求する。

1. `transfer`はcallerのavailable balanceを減らし、recipientを同量増やす。正の移転でreserve、retired units、BILI feeを変更しない。
2. `approve`は当該owner/spender/model/chain scopeのallowanceを指定値で上書きする。成功時は`Approval`を出す。nonzeroからnonzeroへの変更を一律拒否しない。先行消費との順序リスクに対しUIがzero resetを勧める場合も、それを隠れた合意要件にしない。
3. `transferFrom`はcallerのallowanceとfromのavailable balanceを実行時に検査し、転送と同じatomic遷移でallowanceを量だけ減らす。最大整数値を無限承認の特例にせず、使った分を減らす。owner自身の`transfer`にallowanceは不要。
4. 標準のゼロvalue転送は有効なtransferとして処理し、成功したtxに`Transfer`を出す。self-transferは残高を変えず、正の量なら利用可能額の検査を省略しない。`transferFrom`経由なら、selfでもspenderの許可量を消費する。zero/selfはmembership tenureを変更しない。
5. 不正又は未対応recipient、zero address宛の正のtransfer、coreモデル、権限不足、allowance不足、locked/reserved残高、overflowはrevertする。通常転送を任意burn入口として使わない。ゼロ量でも不正modelや権限domainを免除しない。
6. 成功した残高変更と`Transfer`、成功した承認と`Approval`は同じtx結果に属する。revertしたframe/txからevent、allowance消費、残高又はreceiverへの副作用を残さない。ERC-20のtransfer自体にはreceiver callbackを追加しない。

根拠となるinterfaceは[EIP-20](https://eips.ethereum.org/EIPS/eip-20)。`supportsInterface`の特定値だけで互換性を判定しない。旧facadeのmetadata・固定供給表示を新しいburn/retire意味へ黙って変更せず、供給viewの仕様を新ABIに固定する。`registered_supply`、curve在庫、holder残高、retired、circulatingを区別し、旧の固定totalSupply値から流通量を推測しない。永久LP receiptは譲渡・償還可能なLP tokenではなく拠出履歴であり、`balanceOf`、allowance、Position総供給、membershipへ別残高として加算しない。LPで戻したellはcurve在庫に一度だけ数える。

## 4. 同期の転送と非同期AMMを混同しない

旧ModelWriterはqueueを後段foldへ渡し、次blockでsettlementを表示する方式だった。**ActionQueuedは転送完了ではない。** その時点で標準transferの成功boolや成功`Transfer`を返す方式を採用しない。

新transfer/approve/transferFromは、認証済みnative残高・allowanceを基にした**EVM tx journal**で処理する。同じtx内の`balanceOf`/`allowance`は先行する成功writeを観測し、後続の支出・再入・revertも同じjournalを使う。native carrierとEVM txの処理順、journalの初期snapshot、accepted payloadとのbinding、native state rootへの一回適用をcodecと実行phaseで固定する。

そのprojectionを検証できない実装、親snapshotの古い残高だけで書込みを検査する実装、同じcreditをnative/EVMへ二重反映する実装はactivation不可。同期変更をnativeで後から黙って拒否してEVM成功receiptだけ残すことも不可である。

通常AMMの`buy`/`sell`は、明示的な追加APIとして既存のqueue/原資escrow/settlementを使うことができる。受理、約定、拒否、返還の状態を分け、EVM内でまだsettleしていない購入単位をtransfer可能残高に足さない。最終quoteはnativeの規定順で行い、買5% burn・3% owner・残差net、売のnet/minOutを同じ計算器で扱う。

## 5. 保存・gas・署名domain

MOE2の報酬購入B、永久LPのAとell、余剰burn、`LockedLiquidityReceipt(PERMANENT_PROTOCOL_LOCK)`、sticky属性、残余Dの支払権利はnativeの同一原子遷移で確定する。EVMが購入qを一時holder残高へ出したり、各段階の間に取引を挟んだりしない。receiptにはwithdraw/transfer/fee請求用selectorやowner/admin鍵を付けず、公開LP入口を新設しない。

`LiquidityDust`は成功receiptではなく、全工程未commitの有限保留状態である。RPC/EVM viewは元のdeadlineを示し、無期限待ち、donationへの付替え、旧MOE1全burn又はMiner全額へのfallbackを表示・実行しない。B=A=0は市場legのみno-op、A=0はLP操作・receiptなしとする。

nativeとEVMの残高・allowanceは同じ資産のprojectionであり、BILI bridgeのmint/burnとPosition転送を混ぜない。BILIによる買いは一度だけ資金を拘束し、売りのBILI支払はcoinbaseとEVM creditの一方だけで消費する。転送のみではBILI reserveやBILI supplyを変えない。

native署名はnetwork/model/asset版/送信者/recipient/量/nonce/expiryへ結合し、EVMではchain/lane、tx順、caller、call dataを検査する。reorg/restart/replayは同じjournal・operation ID・消費状態を復元する。allowanceは別model・別spender・別chainへ流用しない。

各操作は通常のgas/ネットワーク手数料を払うが、転送にAMMの8%を課さない。contract化したことを理由にownerへの追加徴収をしない。上限内のnamespace、allowance数、journal、logs、削除、IBD表現と料金を固定し、資源上限未確定のまま公開しない。

## 6. activationと受入

新profileに対応するnative/EVM双方の版、snapshot処理順、原子的commit・rollback、view、events、資源上限がそろうまで転送facadeは無効とする。旧selectorの`NonTransferable()`を新正常経路の実装として残さず、旧claim/旧blockの再実行だけへ保持する。

受入には、ゼロ・self・通常転送、approve上書き、使い切り、別spender、残高とallowanceの競合、contract wallet、revert、同tx read-after-write、native/EVM二重消費、reorg・IBD、core全入口拒否、永久LPの権限不存在、receiptと資産残高の非二重計上、LiquidityDustの無副作用と原期限、旧facadeの版境界を含める。古いテストのPASSはこの契約のPASSではない。
