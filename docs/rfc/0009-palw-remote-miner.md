# RFC-0009: PALW Remote Miner — node を持たない claim と非保管型の報酬回収

* Status: Draft, 2026-10-04 — design only; activation height、fingerprint、wire format は未決定。
* 対象: 現行 `testnet-12` の PALW attempt / free-prompt claim。将来の [RFC-0008](0008-palw-claim-backed-consensus-blocks.md) の work-slice block は別途適合性を審査する。
* 関連: [ADR-0044](../adr/0044-palw-free-prompt-receipts.md)、[ADR-0124](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)、[ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)。

## 0. 結論

**miner 自身が full `kaspad` を立てずに claim し、正当に得た報酬を回収できる設計として進めてよい。** pool は必須ではない。ただし「署名済み claim + 任意 relay」だけでは目標に届かない。現在の free-prompt lane は Panel が producer の node から material を取得し、`Final` 後の winning quantum は**同じ bond の producer が receipt block に使う**。この二つを protocol 上で切り離す必要がある。

推奨する最終形は次のとおり。

```text
miner: 計算・鍵・bond・署名
  ├─ 完成済み claim/carrier または署名済み attempt block → 任意 node/relay
  ├─ content-addressed evidence → 複数の DA provider
  └─ quantum の公開 redemption authorization → 任意の block builder

Panel → DA provider から material を取得して検証
chain → claim の executor bond に帰属を固定
receipt block → 誰が組み立てても miner の payout に報酬、builder に明示的な手数料
```

miner は chain の必要情報を取得・検証し、計算と署名が終われば PC を止められる。一方、node、Panel、DA、block builder はネットワーク上で稼働し続ける。relay と DA はインフラの機能であり、既存の PALW Panel seat の権限を得ない。

## 1. 現行実装と解くべき結合

`main` の公開ネットワークは `testnet-12`。旧 `testnet-11` の [free-prompt 運用文書](../testnet11-free-prompt-mining.md)は、障害の具体例として有用だが、現在のパラメータ表として使わない。

| 現行経路 | node-less 化に必要な変更 |
| --- | --- |
| free-prompt gateway の出力は rail が署名・funding・carrier 提出して初めて claim になる | miner 側 wallet が完成済み carrier を署名し、任意 node が検証・中継する。既存の `misaka-palw-fp-rail` を分離の起点にする。 |
| Panel は claim の material/opening を producer 側の node から pull する | claim に bind された material を、miner から独立した provider が保持・配信する。Panel の認証と court は同じ bytes/roots を読む。 |
| `Final` は free-prompt の quantum を使える状態にするが、単体ではその receipt-block 報酬を払わない | quantum の抽選・使用窓・一回限りの spend を維持しつつ、block builder と claim owner を分離する。 |
| [`ProducerNotExecutor`](../../consensus/core/src/palw_fp_admission_v3.rs) が receipt block の `producer_bond == claim.bond` を要求する | versioned receipt で executor bond と builder bond を別フィールドにし、報酬帰属を executor に固定する。 |
| ordinary attempt は計算済み work の block を producer が作る | remote template と完成済み block の提出を提供する。attempt の work identity と署名者は miner のまま維持する。 |

`Final` claim の通常の escrow payout は既に bond の登録済み payout へ chain が支払う。これは block producer に渡す必要がない。free-prompt の **quantum receipt block** は別の報酬経路なので、同じ `Settlement` という語で混同しない。既存 execution-lane round の permit、fee payout、weight も本 RFC の receipt 変更だけで書き換えない。

## 2. 目標と非目標

1. miner に full node、公開受信 port、常時 material server、`Final` 後の常時 receipt producer を要求しない。
2. miner の鍵と bond、claim の帰属、登録済み payout は relay・provider・builder に渡さない。pool を必要条件にしない。
3. 現行の未来 anchor、Panel 選定、verdict、court、保留・`Final`・void、量子抽選、使用窓、chain 候補ごとの一意性を保持する。
4. material 欠落と不正計算の責任を、証明できる範囲で分ける。ローカル timeout だけを slash 証拠にしない。
5. 基礎 PoW を別人へ譲渡する仕組み、Panel node の撤廃、未検証の「trustless light client」、RFC-0008 の work-slice lane の有効化はこの RFC の対象外。

## 3. 署名済み claim と permissionless relay

### 3.1 free-prompt carrier

miner の wallet は現行の gateway/rail が作る commitment と同じ consensus object を組み立て、手数料 UTXO を選び、**完成した carrier transaction 全体**と PALW commitment に署名する。relay は raw bytes を既存の mempool 検証と gossip に渡すだけで、claim を自分のものにできない。payload 固有の署名だけで fee、change output、funding input の改変防止が成立したと見なさず、標準 tx 署名の sighash を監査する。

claim identity は network domain、class、job/input/output/trace roots、executor bond、登録済み payout、anchor と期限規則に結び付ける。これらを新たに wire へ足す必要がある場合は version と fence を付ける。relay ACK は inclusion ではない。client は carrier tx id、claim id、chain inclusion、Panel licence、challenge window、`Final`/void を個別に追跡し、reorg 後に依存関係を再検査する。同じ carrier の複数 relay への再送は同じ tx id として冪等に扱う。

### 3.2 ordinary attempt block

attempt は claim を載せる**block 自体が work**である。外部 node に未署名の template を作らせても、計算結果と header commitment に対する miner の署名・work identity は miner 側に残す。miner は完成した block を任意 node へ送信し、node は通常の候補検証をする。stale parent、誤った target、bond/class/fence 状態の偽装で高価な推論が無効になり得るため、remote template を単一 RPC の言い値で採用しない。後述の §6 の軽量検証を導入するまでは複数独立 node の一致、固定 checkpoint、鮮度上限、不一致時停止を最低条件とする。

**計算を開始する前の署名**や relay による header 改変を認めない。既存の attempt/receipt wire を暗黙に流用せず、必要な remote-template API と署名 digest を仕様化する。

## 4. miner から独立した evidence 配信

### 4.1 manifest と取得

claim が現在 bind する `trace_root`、`output_root`、`execution_root`、`trace_chunk_count`、retention deadline に対応する material を、claim 提出前に複数 provider へ配置する。`EvidenceManifestV1` は network、claim id または衝突しない preclaim id、各 root、chunk の順序/length/hash、encoding、最大展開サイズ、保持 DAA を正規化して commit する。claim id が manifest hash を含む場合は循環参照にならない preclaim id を定義する。Panel は任意 provider から必要な chunk/opening を取り、root と claim binding を検証してから既存の再実行・court に渡す。provider の署名だけで material の正しさを判定しない。

provider は鍵、bond、報酬の代理人ではなく byte 配信者である。storage receipt には provider identity、manifest root、対象 chunk 集合、保持期限、network、署名を含める。必要な複製数、byte cap、同一運営者の重複、rate limit、discovery、料金は実測と攻撃予算から fence ごとに固定する。特定の transport や `misaka-torrentd` が現リポジトリにあると仮定しない。

**新profileの適用境界（2026-10-06）:** 既存profileへの配信は既存replay/courtを維持する。
将来の大型claimは[RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)・
[RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)の小さいconstraint検査へ認証済み証拠を渡し、
異常時のみ有界exact courtを使う。manifestはclass/kernel/plan/suiteと証拠をbindし、proof/openingの配信・保持費用も
計上する。[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)のKernel更新境界を
remote executionで迂回せず、未対応演算をVMで処理した結果を有効なclaimとしない。

### 4.2 可用性と責任

storage receipt は**保管の約束**であり、将来の全 Panel からの取得可能性の証明ではない。初期段階では claim の従来の producer 責任を残し、外部配信を node 不要化の運用改善として試す。miner を配信責任から外す fence は、provider 側の bond と客観的な不履行証明が動いた後に限る。

その fence では、Panel/court が特定 chunk を chain 上で challenge し、provider は deadline 内に hash に合う opening を提出できる。無応答なら provider の担保が対象となる。複数 provider が共通障害で沈んだ場合の claim は無報酬または失効とし、既に正しく計算した miner の fraud slash と区別する。偽 root・不正計算は miner の責任に残る。**単一 Panel の timeout、ローカル fetch 失敗、provider の事前署名だけでは slash しない。** 現行の producer withholding / `DefaultAccused` 経路を変更する際は、旧 claim と新 claim の責任境界を version/fence で分け、同一 failure に二重 slash をしない。

この客観的 court を用意できないなら「miner は送信後に必ず PC を切れて、配信事故の slash リスクもない」とは約束しない。その場合は miner 自身が provider を手配して責任を持つ段階で止める。

## 5. executor と receipt block builder の分離

### 5.1 公開 redemption authorization

free-prompt `Final` の quantum は従来どおり beacon、target、使用窓で抽選し、一つの canonical history で一度だけ使える。新しい `ReceiptSpendV4` は `(network, claim_id, quantum_index, beacon, executor_bond, builder_bond, fee_rule, expiry)` を区別する。miner は claim 提出時または `Final` 後に、**任意の適格 builder が redemption できる**と署名する。future beacon を先取りして署名しないため、署名対象は beacon の値ではなく beacon の決定規則と quantum 範囲にしてもよい。どちらにするかを wire spec で固定する。

builder は自分の block header にその authorization と quantum を載せて署名する。検証 node は claim が `Final`、quantum が勝利し未使用、beacon と target が candidate chain 上で正しい、期限内、executor の署名と bond が一致、builder が適格、coinbase が規定どおりであることを検査する。現行 `ProducerNotExecutor` はこの version にだけ適用しない。旧 `V3` receipt は従来どおり受理または fence で終了し、意味を黙って変えない。

### 5.2 報酬と誘因

receipt block の miner leg は claim の bond record の `payout_payload` へ支払う。builder は**事前に規定された上限内の inclusion fee**を受け取る。fee は同じ receipt-block 報酬から分け、総発行・Panel leg・reserve・既存 maturity を増やさない。payout と fee の出力順・端数・重複 spend・reorg 巻戻しは全 node が同じ値を計算する。builder に利益がなければ任意 redemption は liveness を持たないので、fee 上限と市場試験は activation gate とする。

単なる `Settlement { claim_id }` transaction に置き換えない。現行 receipt block は quantum ticket による lottery、consensus weight、coinbase、使用窓を一体で担うため、transaction だけの支払はそれらの規則を失う。builder-independent な **block carriage** と bond-bound payout が現行経済を最も小さく変更する。

## 6. miner client の chain 検証

node-less は「chain を検証しない」を意味しない。miner が必要とするのは network/genesis、finality checkpoint、selected parent、DAA/epoch、beacon、target、bond/exposure、class と model artifact、fence、claim phase、funding UTXO の正しい view である。複数 RPC の一致は運用上の防御であり、共謀・同一 backend・reorg への暗号学的な保証ではない。

検証可能な軽量 client を名乗るには、checkpoint の更新規則、DAG/pruning/finality proof、これらの状態を commit する root と inclusion proof、reorg 時の巻戻し、証明が得られないときの停止を実装・監査する必要がある。header だけで bond/registry/fence を証明できると仮定しない。Phase 1 は複数 node、署名済み checkpoint、鮮度上限、停止規則を client に明示し、Phase 2 で必要な state commitment/proof を追加する。

## 7. 導入順と受け入れ条件

| 段階 | 内容 | 合格条件 |
| --- | --- | --- |
| A: remote claim | rail の署名/funding と node の提出を分離し、attempt の remote template と完成 block relay を実装。複数 RPC と状態監視を入れる。 | miner PC に `kaspad` がなくても claim が本人の bond で chain に入り、改変 relay・stale template・二重提出・reorg で誤帰属しない。material 配信責任は従来どおり。 |
| B: independent DA | manifest、複数 provider、Panel の root 検証を実装。まず既存責任下で試験し、客観的 provider challenge/court 後に責任を移す。 | miner PC を落としても Panel/court が期限内に material を取得・裁定できる。取得障害と fraud の結果が全 node で一致する。 |
| C: public receipt redemption | V4 spend、builder fee、bond payout、旧 V3 との fence 境界を実装。 | miner PC を `Final` 後も止めたまま、他者の block が winning quantum を一度だけ使い、報酬が miner に入る。builder の取り分以外の供給・weight・Panel 報酬は現行の上限を超えない。 |
| D: verified light client | 必要な state commitment/proof と checkpoint 運用を実装。 | 悪意ある RPC、矛盾する fork、古い anchor/bond/class/target を client が拒否する。 |

各 consensus 変更には独立した version、activation fence、fingerprint、old/new 境界、reorg・IBD・pruning・mergeset の test vector と rollback 手順を用意する。既存 `testnet-12` の有効な claim を RFC の merge だけで変更しない。特に B と C が終わるまで「claim 後に miner PC を切っても報酬まで保証」と宣伝しない。

## 8. 未決事項

- 現行 T12 の全 class/form（PublicDa、PanelDa、held）でどの material が必要か、外部 provider が現行 Panel/court へ同じ bytes を渡せるか。
- provider bond と challenge の費用、response deadline、保存期間、同時障害時の claim 失効規則。
- V4 の producer/builder fee の原資・上限・競争規則と、base-layer block template への正確な carriage。
- remote client の proof に必要な state commitment、checkpoint の配布/更新、検証資源。
- RFC-0008 の work-slice block が実装された場合の同一 work の二重 credit と public redemption の扱い。
