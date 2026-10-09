# RFC-0009: PALW Remote Client — node を持たないモデル登録・claim・非保管型の報酬回収

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。

* Status: Revised Draft, 2026-10-10後続改定 — off-chain model acquisition; model-bond coinbase allocation and conditional prosecution; historical implementation/measurements retain scope; new economics/implementation/activation pending.
* 改訂: 2026-10-06 — node-less モデル登録と登録者の GAS 負担を追加。仕様の追記であり、実装・有効化済みという意味ではない。
* 改訂: 2026-10-10後続改定 — §3.5は固定同一性・任意配布・モデル別拘束miner元本を扱う。MISAKA TorrentとSeeder報酬を廃止し、旧取得監査/TRDC/FPR gateを撤回する。新配分の実装・経済評価・activationは未完了。
* 対象: 現行 `testnet-12` のモデル/class 登録と PALW attempt / free-prompt claim。将来の [RFC-0008](0008-palw-claim-backed-consensus-blocks.md) の work-slice block は別途適合性を審査する。
* 関連: [ADR-0044](../adr/0044-palw-free-prompt-receipts.md)、[ADR-0124](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)、[ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)。

## 0. 結論

**miner 自身が full `kaspad` を立てずに claim し、正当に得た報酬を回収できる設計として進めてよい。** pool は必須ではない。ただし「署名済み claim + 任意 relay」だけでは目標に届かない。現在の free-prompt lane は Panel が producer の node から material を取得し、`Final` 後の winning quantum は**同じ bond の producer が receipt block に使う**。この二つを protocol 上で切り離す必要がある。

**モデル登録者も、自分の full node や Panel を立てずにモデルを追加できる経路を提供する。登録は無料ではなく、ユーザーが BILI の GAS と登録に必要な bond 資金を用意する。** ローカル/委託 builder が artifact と登録 object を作り、ユーザーが自分の鍵で登録 object と funding transaction を署名し、任意 node/relay が提出する。node-less はノード運用を不要にするだけで、登録の審査・署名・経済条件をなくさない。詳細は §3.3–§3.6 と [RFC11 §11.2](0011-permissionless-model-and-long-context-onboarding.md#112-exact-duplicates-must-be-idempotent-without-inventing-a-new-mandatory-registry)。

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
| `model add` は live registration terms、Active registrant bond、署名済み登録 object と有料 carrier を使う | builder/preflight、ユーザー署名/funding、relay、登録結果の追跡を分離する。ユーザー PC の node/Panel 起動は不要とし、GAS・burn・exposure は維持する。 |

`Final` claim の通常の escrow payout は既に bond の登録済み payout へ chain が支払う。これは block producer に渡す必要がない。free-prompt の **quantum receipt block** は別の報酬経路なので、同じ `Settlement` という語で混同しない。既存 execution-lane round の permit、fee payout、weight も本 RFC の receipt 変更だけで書き換えない。

## 2. 目標と非目標

1. miner に full node、公開受信 port、常時 material server、`Final` 後の常時 receipt producer を要求しない。
2. miner の鍵と bond、claim の帰属、登録済み payout は relay・provider・builder に渡さない。pool を必要条件にしない。
3. 現行の未来 anchor、Panel 選定、verdict、court、保留・`Final`・void、量子抽選、使用窓、chain 候補ごとの一意性を保持する。
4. material 欠落と不正計算の責任を、証明できる範囲で分ける。ローカル timeout だけを slash 証拠にしない。
5. 基礎 PoW を別人へ譲渡する仕組み、Panel node の撤廃、未検証の「trustless light client」、RFC-0008 の work-slice lane の有効化はこの RFC の対象外。
6. モデル登録者にも full node・Panel 起動を要求しない。登録 GAS の見積り、自己署名、任意 relay、accepted registry state の確認までを remote client の対象とする。登録のために推論 miner を起動する必要はないが、変換・pack・適合性検査など artifact 作成の計算は省略しない。

## 3. 署名済み claim と permissionless relay

### 3.1 free-prompt carrier

miner の wallet は現行の gateway/rail が作る commitment と同じ consensus object を組み立て、手数料 UTXO を選び、**完成した carrier transaction 全体**と PALW commitment に署名する。relay は raw bytes を既存の mempool 検証と gossip に渡すだけで、claim を自分のものにできない。payload 固有の署名だけで fee、change output、funding input の改変防止が成立したと見なさず、標準 tx 署名の sighash を監査する。

claim identity は network domain、class、job/input/output/trace roots、executor bond、登録済み payout、anchor と期限規則に結び付ける。これらを新たに wire へ足す必要がある場合は version と fence を付ける。relay ACK は inclusion ではない。client は carrier tx id、claim id、chain inclusion、Panel licence、challenge window、`Final`/void を個別に追跡し、reorg 後に依存関係を再検査する。同じ carrier の複数 relay への再送は同じ tx id として冪等に扱う。

### 3.2 ordinary attempt block

attempt は claim を載せる**block 自体が work**である。外部 node に未署名の template を作らせても、計算結果と header commitment に対する miner の署名・work identity は miner 側に残す。miner は完成した block を任意 node へ送信し、node は通常の候補検証をする。stale parent、誤った target、bond/class/fence 状態の偽装で高価な推論が無効になり得るため、remote template を単一 RPC の言い値で採用しない。後述の §6 の軽量検証を導入するまでは複数独立 node の一致、固定 checkpoint、鮮度上限、不一致時停止を最低条件とする。

**計算を開始する前の署名**や relay による header 改変を認めない。既存の attempt/receipt wire を暗黙に流用せず、必要な remote-template API と署名 digest を仕様化する。

### 3.3 node-less モデル登録: prepare → sign → relay → verify

モデル追加は独立した remote workflow とし、claim の提出や採掘開始を前提にしない。

1. **Prepare / preflight:** 登録者の PC または委託 worker で artifact、canonical manifest、class ID、inventory root、宣言した context と必要な適合性検査を完成させる。既存の SDK/gate を使い、network/genesis、ruleset/fence、判定 DAA、参照した chain state を固定する。外部 worker の成功表示だけで未検証 artifact を受理しない。HF URL や Torrent magnet だけでは登録 object にならない。
2. **Read terms / quote:** remote RPC の `getPalwRegistrationTerms` と bond/UTXO 状態を読み、実際の post-genesis 登録 object、carrier と費用明細を構築する。単一 RPC の古い terms を信用せず §6 の鮮度・照合・停止規則を適用する。dry-run の合格は将来の chain 受理を保証しない。
3. **Sign locally:** 現行の [`signed_class_registration`](../../misaka-cli/src/operator/model_add.rs) と SDK の署名対象・network domain を再利用する。登録者自身の Active bond の鍵で canonical object を署名し、wallet が funding inputs、fee、change を含む完成 carrier transaction を署名する。鍵・seed・wallet backup を worker、relay、サイト、VPS へ渡さない。builder の未署名 bytes はユーザー側で照合する。
4. **Relay:** 完成済み raw transaction を任意の node/relay へ渡す。relay は通常の mempool 検証・gossip を使い、登録者・owner・artifact・費用を改変できない。接続先を選べるものとし、特定 VPS、pool、サイト管理者の承認を登録条件にしない。
5. **Verify accepted state:** transaction inclusion と registry の accepted state を別々に追跡する。ユーザー PC に `kaspad` がなくても、署名した class/line/root と実際に採用された登録結果が一致することを確認できるようにする。

これらの prepare/build/quote/sign/submit/status 境界は SDK/CLI に分離し、Web UI も同じ実装を利用する。新しい API 名・wire field を既存 RPC に実装済みと扱わず、追加が必要なら schema/version と適合性 test を仕様化する。登録者の bond と carrier の fee payer は区別するが、**現行で Active registrant bond が必要な条件は維持**する。bond の設定も remote wallet の署名・提出で行え、ローカル Panel の稼働とは別である。

[RFC04](0004-palw-model-improvement.md)、[RFC05](0005-palw-ml-vm.md)、[RFC11](0011-permissionless-model-and-long-context-onboarding.md) の対象 profile と [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) の versioned Kernel 境界を守る。node-less 登録を admission の迂回路や Universal VM の導入理由にしない。未対応 Kernel、審査上限、未有効 fence は理由を返して停止する。

### 3.4 GAS は登録者が用意する: 固定登録費・carrier fee・担保を区別

**「node を立てない」≠「無料でモデルを追加できる」。ユーザーは署名前に必要な BILI を用意し、費用を確認する。** 本 RFC の登録 UI でいう GAS は登録操作のネットワーク費用であり、モデル登録を EVM transaction に変えたり、EVM の `gasLimit × gasPrice` を native carrier の料金へそのまま当てはめたりしない。

| 項目 | 支払元・扱い |
| --- | --- |
| 固定の登録価格 / burn | 現行 T12 の対象 fence 以降は `PALW_CLASS_REGISTRATION_BURN_SOMPI_V1` = **1 BILI**。受理・fold 時に registrant bond の collateral から burn する。wallet の carrier fee から引かれるわけではなく、返金される担保や fraud penalty とも別。対象 network/fence の現行規則を確認する。 |
| Carrier transaction fee | funding wallet の spendable BILI/UTXO から支払う。実際の carrier の mass、network fee policy、選択した priority を用いて見積もる。固定の登録価格と別であり、total GAS を常に 1 BILI と表示しない。 |
| Bond / registration exposure | Active bond の存在と残余 collateral を検査する。既存 reservation、live slashable locks、今回の exposure、burn 後の backing を同じ chain gate で確認する。GAS が払えても担保不足なら登録不可。 |
| その他の filing / 任意サービス | certification 等に追加 filing/rent が必要なら個別に表示する。activation pool、市場開設 deposit、worker/relay/seeding の任意サービス料金は登録 GAS と混ぜず、別承認とする。 |

根拠は [`palw_state_v2.rs`](../../consensus/core/src/palw_state_v2.rs) の登録 burn・exposure・live-lock gate と、[`model_add.rs`](../../misaka-cli/src/operator/model_add.rs) の費用提示・署名・carrier 提出経路である。本 RFC は新しい burn 額や免除規則を設けない。

quote には `network/genesis`、ruleset/fence、参照 tip/DAA と期限、class/root/object digest、登録 burn とその支払元、carrier mass/fee と wallet の change、必要 collateral/exposure、追加 filing の有無、ユーザーの最大支払額を含める。wallet は funding UTXO と bond の両方を確認し、不足額を分けて返す。**残高不足・quote 期限切れ・terms 変更・署名対象の差替えは、署名/提出前に停止する。** fee を増やす再構築は再見積り・再署名・再承認が必要で、relay に無制限の fee 変更権を与えない。

既定はユーザー自己負担とし、無料登録・自動スポンサーを装わない。スポンサーを提供する場合も明示的な署名済み条件に限定し、fee payer、bond signer、publisher、model-line owner を区別する。carrier fee の肩代わりだけで owner を取得できず、現行の bond burn がスポンサーへ移ることもない。所有者/支払元の新しい分離規則が必要なら別の合意更新とする。

### 3.5 Artifact配布の任意運用とmodel-bond配分 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

登録・README公開・モデル取得・Final/報酬をclientで別表示する。署名/rootを検証し、巨大weightsをcarrierへ載せず、
鍵を配信objectへ含めない。配布先のtelemetryを合意上のREADY/停止表示へ変換しない。
モデル配布契約/新配分登録をユーザーの見積りや署名なしに自動実行せず、投稿者VPSを唯一の保存先にしない。

### 3.6 状態追跡・再送・登録と readiness の分離

client は `prepared / preflight-passed / needs-gas-or-bond / signed / relay-accepted / tx-included / registration-accepted / refused / reorged` を区別し、raw tx ID、registration object ID、class/line/root と拒否理由を保持する。名称は client の表示状態案であり、新しい consensus lifecycle を追加するものではない。relay ACK、mempool 受理、carrier inclusion は登録完了の証拠にならず、fold が登録 object を拒否する場合はその理由を返す。

GAS の返金を保証しない。carrier が取り込まれ登録 object が拒否された場合でも transaction fee は消費され得る。同じ raw transaction を複数 relay に送る再送と、追加費用を払う新しい transaction を区別する。exact class の既存登録は accepted state と完全 identity を確認して再利用し、不要な再登録 fee を取らない。ただし Frozen/Dormant 等の lifecycle をそのまま表示し、再利用で凍結解除や再有効化の署名・予約条件を迂回しない。異なる context、program、tokenizer、root は exact duplicate と扱わない。reorg 後は registry と funding/bond 状態を再検査し、費用が発生する自動再提出をしない。

**登録受理、Panel readiness、最初の licensed/Final claim、報酬資格、ブロック生成、市場開設は別々の事実として表示する。** 本 workflow の完了は本人の署名と ownership に対応する accepted class/line を確認するところまでであり、Panel 起動・採掘・市場 deposit を暗黙に実行しない。

## 4. miner から独立した evidence 配信

### 4.1 manifest と取得

claim が現在 bind する `trace_root`、`output_root`、`execution_root`、`trace_chunk_count`、retention deadline に対応する material を、claim 提出前に複数 provider へ配置する。`EvidenceManifestV1` は network、claim id または衝突しない preclaim id、各 root、chunk の順序/length/hash、encoding、最大展開サイズ、保持 DAA を正規化して commit する。claim id が manifest hash を含む場合は循環参照にならない preclaim id を定義する。Panel は任意 provider から必要な chunk/opening を取り、root と claim binding を検証してから既存の再実行・court に渡す。provider の署名だけで material の正しさを判定しない。

provider は鍵、bond、報酬の代理人ではなく byte 配信者である。storage receipt には provider identity、manifest root、対象 chunk 集合、保持期限、network、署名を含める。必要な複製数、byte cap、同一運営者の重複、rate limit、discovery、料金は実測と攻撃予算から fence ごとに固定する。claim固有証拠の配信scopeをmodel weights取得に拡張しない。MISAKA Torrent/専用Seederは本設計から廃止する。

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
| A0: remote model registration | prepare/preflight・live terms/費用明細・ローカル署名・有料 carrier relay・accepted registry state の追跡を SDK/CLI/Web に分離する。Artifact/metadata は投稿者/peer の Torrent 配信とする。 | `kaspad`・Panel がないユーザー PC から、自己負担の GAS と Active bond で登録が本人の class/line/root として受理される。GAS 不足、担保不足、期限切れ quote、改変 relay/owner、二重提出、fold 拒否、reorg、VPS 再作成、seeder 停止を test し、誤課金・誤帰属・偽 readiness を起こさない。 |
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
- remote registration の quote/unsigned-object/署名/状態照会の API 境界、offline wallet が費用・identity を検証できる schema、複数 relay の retry と fee-change 承認。現行 burn・担保規則を変えずに実装できる部分と、新しい authority/fence が必要な部分の切り分け。

## 共通post-commit challengeとremote status — 2026-10-08

新しいKernel/model onboarding、claim/EXEC slice検査は[RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)だけを使う。remote clientもclass/policy、canonical commitment、qualifying future workとFinal/settlement provenanceからseed・queries・GKR transcriptを検証し、remote worker/operatorのseedを信頼しない。既存receipt quantum/ticketのbeacon規則とは別のversioned契約であり、今回の文書で再解釈しない。

RFC11 §17 / RFC13 §9のRegisteredDormant、WaitingRandomness、ConformancePassed、ActiveRewardableを明示し、beacon待ちはdurable idからresumeできる。artifact/layout/plan/implementation変更は古いevidenceを無効にする。source不足をheartbeat/BASE-0/EXEC hash・BFT/DNS署名で補わず、candidate/verificationだけをpending/既定deadline扱いにする。source/constraint/G14不足を「registered/active成功」と表示せず、fraud proofの客観性・担保・public material条件を維持する。runtime・activationは変更しない。

## Mission alignment amendment — 2026-10-07

node-less producerとrelayの分離は維持する。証拠の配布先をPanelだけに限定せず、普通のpublic bondがproducerの停止後も認証されたmaterialを取得して局所化・court提出できる責任と保持期間を定義する。複数RPCの一致は算術証明ではない。relayのHTTP失敗やmaterial未到達を自動slashの根拠にせず、規範的demand/disclosure/defaultで扱う。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

remote producerにも同じbond/DAAのclaim/block/reward/Final weight予算を適用する。クライアントの計算速度・model/PWU申告・再送で枠を増やさない。
readiness/RPCは予算残高、共通reuse_not_before、残存責任headroomとrulesetを公開し、予約額見積りと実際の合意creditを分ける。UIの表示だけで新会計を実装したことにしない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
