# testnet-12 regenesis — deployment record, 2026-09-23

> **状態（2026-09-24 更新）: 新しい genesis `d73dbf44…` で起動し直す準備中（`f6cc9576…` は
> replay 分離で置き換えた。下の「replay 分離」）。consensus params fingerprint と出荷 binary の
> sha256 は未確定（TBD）。** 別 session が実装中の DoS 監査修正
> （#1〜#4、#5 の forfeiture 側、#6〜#8、#11〜#14）が merge されると consensus 側が動き、
> fingerprint も変わる。merge 後の release build で下表の TBD を埋めてから配備する。
>
> 初回配備の chain（genesis `a8cabac4…`）とは互換性が無い。premine が動いたので genesis block
> そのものが変わっている。旧 t12 の datadir で起動すると startup の genesis-mismatch guard で拒否
> される。消さずに `.<日付>-regenesis-bak` などへ退避すること（初回配備の時と同じ扱い）。
>
> **初回配備の chain は genesis から heartbeat しか流れていなかった。** 09-23 の route matrix で
> 見ると 1,231 block のうち 1,230 が algo 8（heartbeat）、attempt は 0 だった。原因は 3 つ重なって
> いた。全ホストが `--palw-producer-class=74c67e63…`（2M 行）を指定していたが、この行は担保の単位、
> root、working set の問題で生産できなかった。floor を動かすホストが無かった。座席も止まっていた。
> それでも起動は「heartbeat が刻んだ」ことで成功と判定されていた。この経緯から、新 genesis の
> 起動判定には lane の内訳（下の「起動判定」）を使う。

## 新 genesis（2026-09-24 の replay 分離以降）

| | |
|---|---|
| genesis hash | `d73dbf44dbae3522c05de7aada567f9448221bc5c832393c9eb2996698e91aba2e638f0eea1f89bccbf72268750bb62deb1e958440136ca748727747f230fe18`（`f6cc9576…` を置き換え） |
| genesis timestamp | `1788220800000`（2026-09-01T00:00:00Z）。t11 などが共有していた参照 timestamp `1748390400000` ではない |
| hash_merkle_root | `5ef04d1b9a6cb09e728a970d2e0b75a0142ecacc4a373c9b719e47d6a0d2058cb048fa240e13f1bbb094dd4e195919ea041db2ee338f21de34364f885317f805`（初回配備と同じ。coinbase marker `misaka-palw-t12` も同じ） |
| utxo commitment | `12df48ae4c1b4a181e8746b2d888d55a12cd7d651f582f51e95d41c8c51b56a88fb027d40c3e94a433a7253d61c9cb202f2c5c89bbde112ca1c4f90ceae5478b` |
| premine txid | `5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669`（`premine_txid_for(testnet-12)`。index は従来どおり: collateral 0〜7、main wallet 40、fee float 41〜48） |
| community txid | `e3d638e58827755bdc78b362b499495c9607275b50a3ec2e1941f4ea75d38eda1c010f7796c637bd0b343d811a729291869c4658f36d6ca8d959baf8ed85064b`（`testnet12_community_txid()`、index 0〜15） |
| consensus params fingerprint | **TBD**（DoS 修正の merge 後に確定。起動ログの `Consensus params fingerprint: … (network testnet-12)` 行と照合） |
| release binary | **TBD**（`kaspad` / `misaka` / seeder の sha256。merge 後の release build） |
| fence schedule | `1000`。ADR-0065 D1 の bond maturity window で、他の rule はすべて DAA 0 で武装している（`palw_t12_arm_every_rule_from_genesis`）。起動ログの `Consensus fence schedule:` 行でも確かめる |
| premine | 33 outputs、合計はちょうど 10B MSK。内訳は collateral 8 + fee float 8 + **community 16（758M）** + main wallet |
| bond collateral | **939,063.21001040 MSK / seat**。8 seats で 7,512,505.68 MSK（cap の 0.0751 %） |
| genesis bond cards | card 0〜6 は `PALW_RC_GENESIS_BONDS` 0〜6 のまま（鍵は変わらない）。**card 7 は testnet-12 専用に鍵を替えた**（下記） |
| classes | BASE-0 floor + dense Qwen2.5-1.5B graph-v7 @8,192 + @2,097,152。**hybrid 行は無い**（下記） |
| P2P port | 26311（t11 と同じ。運用者の決定） |

### replay 分離（2026-09-24、ユーザー決定）

旧 card（`f6cc9576…`）の premine は、すべての network と同じ sentinel txid（ASCII
`misaka-premine`）の上にあった。私設 testnet-12 も同じ binary 系列から同じ bond 鍵 0〜7 で作られて
いる。ML-DSA の sighash は使う outpoint には署名するが、network にも genesis にも署名しない。
そのため私設 chain で署名した float や collateral の spend が、そのまま公開 testnet-12 でも有効だった。

* **premine txid を network ごとに分けた。** `premine_txid_for(testnet-12)` は
  `BLAKE2b-512(key = "misaka-premine-txid/v1", sentinel ‖ "testnet-12" ‖ PALW_T12_PREMINE_SALT)`。
  他の network（testnet-11、testnet-10、devnet、simnet、mainnet）は sentinel のままで、genesis と
  fingerprint は動かない（`palw_the_release_did_not_move`、`every_genesis_commits_to_the_premine_this_build_mints`、
  `test_genesis_hashes` がそれを確かめる）。
* **community txid も同じ方法で分けた**（`testnet12_community_txid()`）。`misaka-t12-community`
  sentinel は私設 chain と共有していたため。
* **genesis timestamp も変えた**（2026-09-01T00:00:00Z）。commitment とは別に、block hash が参照
  timestamp を共有する chain と一致しないようにするため。過去の日付なのは、wall clock より未来の
  genesis だと最初の block が「too far into the future」で拒否されるため。
* **運用上の影響: index は同じだが txid が変わる。** bond の identity（`PalwBondKeyV2`）は
  outpoint なので、testnet-12 の bond は `5e0d5f1b…:<index>` になる。host の起動コマンドに
  `--palw-producer-bond=<sentinel>:<i>` や `--palw-fee-outpoint=<sentinel>:<41+i>` と書いてある場合は、
  新しい txid に書き換える必要がある。
* 私設 chain の tx を公開 chain に流しても、名前の違う outpoint を使うので無効になる。
  `t12_shares_no_premine_outpoint_or_genesis_with_the_sentinel_chains` と
  `the_fleets_premine_indices_are_unchanged_on_t12s_own_txid`（consensus/core/tests/t12_regenesis.rs）
  がこれを固定している。

### genesis の行

| model id | class id | inventory root | artifact（sidecar） | bytes | flat artifact digest |
|---|---|---|---|---|---|
| `Qwen/Qwen2.5-1.5B/graph-v7@8192` | `ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01` | `88096dc177826d880c1c5fca4ec93cffe5ab51af108ed169a8e03cd4726308f91263f79f81904b043327bfa277e3558b1656f14259f6dd33603a9f91871aae20` | `qwen25-1.5b-a16-8k.palwart`（`consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest`） | 1,799,359,436 | `f4af38d9…` |
| `Qwen/Qwen2.5-1.5B/graph-v7@2097152` | `74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a` | `f63af2c46b3816f6a16c168a130d28ec95b0ca5107da76d4abe24e4d9396e65c80d6d83014c47855e2394af20ce3333a59359fd3eb06090fdc7bfd502f75c7c2` | `qwen25-1.5b-a16-2m.palwart`（`…/qwen25-1.5b-a16-2m.palwmanifest`） | 2,868,906,956 | `b5baca63…` |

* どちらの root も commit 済みの sidecar から `class_manifest_const_v1` で読んでいる。手で打った
  定数は無い。右端の flat digest は、初回配備で root の代わりに誤って pin した値そのもの
  （`b5baca63…`）。**root と digest は別物**なので、ここでは並べて載せている。
* どちらの artifact も `Qwen/Qwen2.5-1.5B-Instruct` を `qwen25-convert --a16 --n-ctx <幅>` で変換
  したもの（2M の変換元は `docs/qwen25-a16-2m-held-artifact.md` 参照）。8k の sidecar は
  5.104.81.23 で測定し、`palw-class manifest --check` で再導出して一致を確かめた。
* **運用上意味があるのは 8k 行。** 2M 行は fleet のホストで attempt 1 回に 1 週間かかる。8k の
  attempt は 1,023 位置の prefill が約 290 s で、全席の replay に要るメモリは ≈ 3.37 GiB
  （perm-drill の実測）。
* 2M 行は、hybrid 行があった時の登録スロットを引き継ぐために 2 番目に置いてある
  （`PALW_T12_GENESIS_HELD_ROWS`）。

**hybrid 行を入れていない理由。** graph-v7 の held map は GDN の畳み込みを `(2·k_dim + v_dim)·heads`
で集める。これが engine の窓と一致するのは key と value の head 数が等しい場合だけで、
Qwen3.6-35B-A3B は 16 と 32 で一致しない。そのため held hybrid の attempt はすべて最初の recurrence
checkpoint（prefill の位置 15、`ConvIsNotTheGeometrys`）で失敗する。genesis に入れても誰も生産
できない class になる。map を直せば、hybrid 行は登録トランザクション 1 本で追加できる。ただし
genesis 後の登録なので `Candidate` から始まり、admission audit を待つことになる（join 手順書の
§ lifecycle を参照）。

### card 7 を作り直した

旧 card 7 の鍵はどの fleet ホストにも無い。そのまま testnet-12 に載せると、誰も署名できない登録
座席が 1 つ残る。model class を動かすには ready seat が 7 つ要るので、この欠けは致命的になる。
そこで 5.104.81.23 で `misaka key gen` を使い、bond 鍵と operator 鍵を新しく作った
（`/etc/misaka/t12/t12-bond-7.key`、`t12-operator-7.key`、0600、ホスト外には一度も出していない）。
それを同じホスト上の `palw-rc-genesis --emit-row` で `PALW_T12_GENESIS_BOND_7` にした。この
コマンドが出力するのは公開値だけ。payout は bond 鍵自身のアドレスなので、premine index 48 の
float は card 7 に署名する鍵で使える。testnet-11 の table には手を入れていない
（`testnet_12s_registry_is_seven_carried_cards_and_one_new_key`）。

bond と float の対応は `premine_outpoint(i)` ↔ `41 + i` のまま（card 0 ↔ 41 … card 7 ↔ 48）。

## mainnet 想定の bond（2026-09-24、ユーザー決定）

| | testnet-12 | 備考 |
|---|---|---|
| producer（miner）floor | **13,000 MSK**（`PALW_MAINNET_MIN_COLLATERAL_SOMPI`。mainnet の値も 10,000 → 13,000） | bundle の `min_collateral_sompi`。testnet-11 と devnet は 0.004 MSK のまま |
| panel seat floor | **130,000 MSK**（producer floor の 10 倍、`palw_panel_collateral_floor_v1`） | |
| readiness に要る空き担保 | 39,000 MSK（floor × 3） | genesis card 939,063.21 MSK はすべての floor を満たす |
| option A での floor claim 1 本の予約 | **3,200.95 MSK**（escrow 3,200.85 + weight 0.11） | 13,000 MSK の bond（上限 500 ‰ = 6,500 MSK）には **2 本**入り、3 本は入らない。依頼時の想定は「ちょうど 1 本」だったが、実測では 2 本（ちょうど 1 本にするには floor を 6,401.91〜12,803.82 MSK 未満にする必要がある）。`t12_mainnet_assumed_bonds.rs` が実測値を固定 |
| DNS validator bond | **20,000,000 MSK 以上** | `PALW_T12_DNS_PARAMS`（`PRODUCTION_DNS_PARAMS` から導出。production 自体は不変） |
| DNS finality の起動条件 | **validator 6 以上、active stake 120,000,000 MSK 以上** | production は 12 validator。validator 数 × bond の関係は production と同じ |
| unbonding period | 10,083 block = **約 14 日 6 分**（120 s cadence） | `at_two_minute_cadence` で変換。10 bps の 14 日ではない |
| coinbase の long maturity | 600 DAA（約 20 時間） | Decision A（coinbase は DAA だけで成熟）のため testnet-11 の値を維持 |

DNS set は production から次の点だけ変えている。validator 数・bond・stake の 3 値、120 s cadence への
窓の変換、coinbase long maturity（600）、それに `required_work_depth`（testnet-11 の値。production の
値は 10 bps の kHeavyHash 用で、PALW chain では何年も届かず DNS 確認が永久に起きないため）。
それ以外（`required_stake_depth`、`min_anchor_attesters` = 2、報酬、stake preference 無効、VLT inert）は
production のまま。testnet-11・mainnet・devnet の fingerprint は動かない（`palw_the_release_did_not_move` と
preset の fingerprint pin で確認）。

## 担保: option A（監査 U2 に対する運用者の決定）

ConsensusV2 の preset はすべて `deflationary_phase_daa_score = 0` なので、genesis 期の claim の
escrow は block 1 が実際に払う subsidy になる。444,562,014,000 sompi に worker carve の 720‰ を
掛けて **3,200.84650080 MSK / claim**。旧 card はどの block も払わない
`pre_deflationary_phase_base_subsidy`（370,468,345 sompi）で計算していたので、1,200 倍小さかった。
card の値付けどおり、exposure horizon 中の floor claim 7,201 本を同時に持てる seat を作ろうと
すると 46,627,776.51 MSK / seat になり、参加できる人がいなくなる。そこで運用者は次のように決めた。

* **runtime は claim ごとに「escrow + weight」（不正で得られる利益の全額）を bond に予約する。**
  escrow は claim の `accepted_daa` に紐づく別の項として持つ（`escrow_backed_exposure_from_daa`、
  `palw_audit_2026_09_23` の state-params 側の写し）。予約は受理時に行い、Final か void で解放し、
  block と一緒に巻き戻る。整合性検査でも再導出される。
* **没収される額は void の種類で変わる。** CourtFraud の void は reserved と escrow の両方を没収する。
  `ProducerWithholding` の void（DA court が default を確定）と、2 回目の `ReceiptTimeout` も
  weight + escrow を没収する（`b38356fe`、監査 #10）。`BindTimeout` と `NoCapablePanel` は
  producer に課金しない。
* **admission の上限は `collateral × 500‰`。** producer 側の余裕計算も escrow を含めて数える。
  したがって bond が同時に持てる claim の数は担保に比例する。1 本あたりに要る担保は次のとおり。

| class | exposure pwu | 不正利益 / claim | 同時保有数（card） | 担保 / seat | **同時 1 本あたりの担保** |
|---|---|---|---|---|---|
| BASE-0 floor | 7,708 | 3,200.84689 MSK | 64（`PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1`） | 409,708.40 MSK | ≈ 6,401.69 MSK |
| Qwen2.5 @8,192 | 494,320,046 | 3,225.56250 MSK | 4（in-flight cap ×4） | 25,804.50 MSK | ≈ 6,451.13 MSK |
| Qwen2.5 @2,097,152 | 1,194,858,841,364 | 62,943.78857 MSK | 4（同上） | 503,550.31 MSK | ≈ 125,887.58 MSK |
| | | | | **939,063.21001040 MSK** | |

`PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI = 93_906_321_001_040`。`t12_bond_collateral_matches_the_card`
が出荷 card からこの値を再導出し、ずれがあれば build を落とす。これまでの値の変遷（60,088.18 →
516,429.80 → 939,063.21）と、それぞれが間違っていた理由は `consensus/core/src/config/premine.rs`
の doc comment と ADR-0151 の Addendum（2026-09-23 夜）に書いてある。

**まだ閉じていないこと:** runtime が予約するのは renormalise 後の exposure だが、それが与える fork
weight は raw の MAC-eq で、その差は 5,620 倍ある（ADR-0151 D1 の weight 側の半分）。

## 前回この記録を書いてから変わったこと（`7327c4e0..5a559459`）

| commit | 内容 |
|---|---|
| `8c61a3d8`…`46f2592b`、`0da84e44`…`254463b1` | `feat/kv-codec` の merge。held class の attempt を fold し、memory bracket を出し、residency は「share − attempt」から切り出す。hybrid tier にも resource profile を付けた |
| `b5c5f324` / `5d90a18f` / `cca52e92` / `3644c1b3` | 2026-09-23 経済監査の修正を 1 つの fence（`Params::palw_audit_2026_09_23`、t12 だけ `Some(0)`）の裏にまとめた。C-1 cross-block execution replay、C-2 execution credit の単位と quanta 上限、C-3 seat lock の単位、C-4 attention geometry など。state schema は v21 |
| `b791b460` | **regenesis card**: option A、genesis 行を dense 8k + 2M に、card 7 を作り直し。genesis hash `a8cabac4…` → `f6cc9576…` |
| `6aee2530` | wallet: `NetworkParams::from` に suffix 12 の分岐が無く、t12 では wallet 操作がすべて panic していたのを修正 |
| `ff07d744` | CLI: `misaka key pubkey` を追加。ネットワークを指定しない場合の既定を testnet-12 に変更（対象は operator 系コマンドのみ） |
| `faf80a4e` | add-a-model runbook を 09-23 の実測に合わせて修正 |
| `b38356fe` | 監査 #9（attempt の上限を live state で判定する）と #10（withholding でも escrow を没収する） |
| `5a559459` | route matrix #1〜#8: lane watch と producer の起動拒否、実行 lane の windowed mint、Valid lock を満たせない bond を抽選から外す、`E-MODEL-NOT-ADMITTING`、RPC の v2/v3 フィールド。あわせて監査 #5 の escrow 側（計算量で値付けする free-prompt claim の `rights_reserved`）と B1（`--palw-register-bond` が operator-possession 署名を付けるようにした） |

**B1 がこの regenesis で一番効く修正。** t12 は `palw_operator_id_unique` を DAA 0 で武装している。
修正前の `--palw-register-bond` は署名を 1 つしか付けなかったので、carrier の tx は mine されても
中の `BondRegistered` はすべてのノードで捨てられていた。つまり新しい鍵は誰も bond operator に
なれなかった（perm-drill で確認）。`5a559459` からは 2 つ目の署名（operator-possession proof）が
付く。

## 起動判定（新 genesis）

初回配備の反省から、「heartbeat が刻む」は成功条件に含めない。

1. **lane の内訳を見る。** どの ConsensusV2 ノードも、selected chain の直近 600 block を 60 s ごとに
   lane 別に数え、各 chain block が merge した algo-10 round block も数える（round block は selected
   chain に乗らないので mergeset から数える。仕事には数えない）。algo 9 は ADR-0072 の
   execution-priced attempt で、どの preset でも未武装なので常に 0。ノードがほぼ同期していて、
   最新 30 block（heartbeat 120 s で 1 時間）に PALW の仕事が 1 つも無ければ、
   `[palw-lane-watch] no PALW work block in the newest … — the chain is running on its clock alone`
   を ERROR で出す（10 分ごとに `still:` として繰り返す）。同期中は判定しない。同じ内訳は
   `getPalwNodeStatus` v3 の `laneMix` / `laneAlarm` でも読める。
2. **floor の経路を最後まで通す。** Claim → PanelBound → Final → quanta → permit → algo-10 → payout。
3. **8k 行が `Prefetching` から `Probation` に進む。** 異なる operator の ready seat が 7 つ要り、
   各 seat の `--palw-host-memory-share` は 3.5 GiB 以上。genesis card は 8 枚なので、7 枚が 8k seat
   を立てるか、第三者の bond が加わる必要がある。registry が数え始めるのは DAA 30（grace）から。
4. **第三者が bond を登録できる。** `--palw-register-bond` で登録し、`misaka bond status` で
   `REGISTERED` を確かめる。

producer 側も、確実に生産できない class（artifact を 1 つも持たない、または genesis class を全 artifact
の sidecar が別 root で載せている）を指定されたら、attempt lane を止める（stdout と log に理由を出し、
ERROR 行は `— production disabled` で終わる。状態は `disabled`）。ノード・seat・RPC・receipt lane は
動き続ける（exit はしない。systemd の `Restart=` で crash loop になるため）。30 分生産できなかった場合は
`NOT PRODUCING for N min — holding: …` を ERROR で出す。

round producer は、ticket が並ぶ span の schedule が見えた時点で、自 bond の ticket のうち過去の round
（median time より後）も含めて古い順に全部署名する。schedule が見えるのは opener の上に次の chain block
が届いてからなので、現在 round だけ署名していた版では窓の先頭が失われていた。残る損失は、次の span に
view が移った時点でまだ先にある ticket（窓の末尾）。

## 配備の順序（予定）

DoS 修正の merge → release build（上表の TBD を埋める）→ drill（出荷する binary で、fence 1000 を
跨ぐまで）→ 4 ホストへ配備 → seeder 切替 → 告知（告知は運用者が行う）。一般参加者向けの手順は
[`testnet12-join-mining.md`](testnet12-join-mining.md)。

---

## 経緯（2026-09-23 昼までの状態欄、原文のまま残す）

> 1. **担保の単位** — genesis の carve が「クラス自身の declared leaves」単位、runtime の予約は「floor 正規化後の導出値」で 44.25× 差。全 producer が `holding: the bond's exposure ceiling leaves no room for another claim` で `produced=0`。担保 60,088.18 → 516,429.80 MSK/seat（`2bd134ec`）。
> 2. **class root** — pin した値が operand-inventory root ではなく flat `artifact_digest`（t11 と同じ事故の再発）。全 seat が `holds no artifact whose registered root form is b5baca63…`。
>
> 対処（項目 1〜5 完了、6 進行中）: inventory root の streaming 化（`216641a4`）／`ArtifactDigest`・`InventoryRoot`・`ClassId` の型分離＋`.palwmanifest`＋`palw-class manifest`（`ea7ad7df`）／runtime が sidecar を読み `--palw-verify-class-manifest` で fail-closed（`26030bc1`、`2f588a58`）／genesis の手書き定数を廃止し commit した manifest を `const fn` で読む（`077d4c7f`、root `b5baca63…`→`f63af2c4…`）／per-host メモリ予算 `--palw-host-memory-budget` / `--palw-host-node-count`（`a0350833`）／phase 別メモリ分解＋60 s 周期行（`b9b5ed07`、`e40dfcd2`）／1-seat acceptance が暴いた OOM 経路 3 つの修正（`427a1b8d`、`e40dfcd2`）。
>
> **genesis ブロックは不変**（root は PALW bundle の `genesis_objects` にあり header/premine に入らない）— *この一文は `b791b460` までの話。option A と card 7 で premine が動き、genesis block も動いた（上記）。* params fingerprint は `fb8f378d…` → `c746f07c…`（担保）→ `30848c6b…`（root）→ `bcfbf2a3…`（t11 で genesis 有効だった `palw_unavailable_abstains` が t12 で dormant に退行していたのを武装、`t12_arms_every_fence_t11_armed` が台帳として常駐）→ `f66bf139…`（RC family 5 本目 `PALW-QWEN36-V6`。t12 の genesis fp 認定集合を「card が登録した class」から導出。`court_e2e_root` は全 RC bundle に入るので t11 の fingerprint も `33bdff0b…` へ動く）→ `88a9aee8…`（held hybrid 行の root 事故 — 同型 3 度目。`qwen36.palwq36.palwmanifest` を commit、`t12_genesis_roots_are_all_read_from_committed_manifests` が genesis の全登録行を sidecar と突き合わせる）→ 監査 fence・option A・route matrix を経て **現在は TBD**。
>
> **engine 側（`feat/kv-codec` を `5451aac2` で merge、fingerprint 不変）**: A16-KV-i16 は無損失の再パックとして出荷既定、i8 は名指し拒否。2M attempt の working set 14.84 → 7.84 GiB、fleet 上では ~11.6 GiB（paged KV は未着手）。役割別 resource profile、node-local 予約台帳、S1 partial seat の prefix 再実行、telemetry。`--palw-host-memory-share`（`4e05f85d`）で per-process share を明示可能。

---

## 初回配備の記録（genesis `a8cabac4…`、2026-09-22/23）

以下は初回配備時点の記録で、書き換えていない。**新 genesis では genesis hash、担保、class 行、
card 7 が違う**（上記）。

Build `de857a71` (`kaspad v1.1.0-de857a71`), a release build of `feat/testnet-12-regenesis`.

### The network (first deployment)

| | |
|---|---|
| genesis hash | `a8cabac47b96fe30d9675ce08a355f62e6d57aac84865b0d6295c024c6d8ff61fb2d93943952e8da4c36ff87ea7f17f6c8de643fa7b02ecf7512892d590777dd` — **superseded by `f6cc9576…`, itself superseded by `d73dbf44…` (replay separation, 2026-09-24)** |
| params fingerprint | `fb8f378df6373455e0c14d08c835184b86717ae3fc983039f9ded633be7fa38d` at first deploy; the live public nodes later ran `c746f07c…` |
| fence schedule | **`1000`** — one height, ADR-0065 D1's bond-maturity window (t11's was `1150, 1900, 2150, 2400, 3500, 4000, 6900, 7100, 7101, 7200, 7301, 8000, 2125000`) |
| rule manifest | `… palw_work_target=1 palw_independence=1` (digest `9def81a1…`) |
| premine | 33 outputs, exactly 10B MSK: 8 collateral + 8 fee floats + **16 community (758M)** + main wallet |
| bond collateral | 60,088.18407600 MSK a seat at first deploy, then 516,429.79663480 (`2bd134ec`) — **now 939,063.21001040** |
| classes | BASE-0 floor + held Qwen3.6 `e108e736…`@512 + held Qwen2.5 `74c67e63…`@2,097,152 — **now floor + dense @8,192 + @2,097,152** |
| P2P port | 26311 — unchanged from t11, by the operator's decision |

### Hosts (first deployment)

| host | unit | appdir | listen | bond / fee |
|---|---|---|---|---|
| 169.58.232.113 | `misaka-t12-node` | `.t12` | 0.0.0.0:26311 | 6 / 47 |
| 169.58.39.220 (ibm) | `misaka-t12-node1` | `.t12b` | 0.0.0.0:26321 | 1 / 42 |
| 5.104.81.23 | `misaka-t12-seat2..5` | `.t12`, `.t12b`, `.t12c`, `.t12e` | 127.0.0.1:26311/21/31/41 | 2..5 / 43..46 |
| 95.111.236.186 | seeder only | — | — | — |

Seeders, all four: `misaka-dnsseeder-t12` (`--network-id testnet-12`), binary `1174b965…`.

**The t11 units, launch scripts and datadirs are all still in place.** Rollback is: stop the t12 unit,
`systemctl enable --now` the t11 one.

### What the switch did NOT change, and why that matters

Every PALW flag on every host carried across untouched — `--palw-producer-class=74c67e63…`, the 2M
class artifact, `--palw-producer-key=/etc/misaka/t12/t12-bond-N.key`, `--palw-producer-bond=…:N`,
`--palw-fee-outpoint=…:4N`, and all ports. testnet-12 re-used the same eight genesis bond cards, the
same premine sentinel txid and indices, and registered the very class the fleet was already mining.
Only the network's name, its genesis and its datadir changed. `the_fleets_premine_outpoints_are_unchanged`
pins the bond↔float pairing (1→42 … 6→47) so a future change cannot break the submitters silently.

*In hindsight (route matrix, 2026-09-23): carrying `--palw-producer-class=74c67e63…` across unchanged
is what left the chain with no producible class and no floor producer. On the new genesis a producer
names a class it can produce (the floor, or the 8k row with its artifact) — and a producer that names
one it cannot now refuses to start.*

### Three things this deployment caught

1. **The old DNS seeder advertises the wrong port on testnet-12.** Measured, not assumed:
   `misaka-dnsseeder --network-id testnet-11` health-checks anchors on `:26311`, and the same binary
   with `testnet-12` checks `:26411` — the `None | Some(_)` fallback in the pre-regenesis
   `NetworkId::default_p2p_port`. Switching the seeders' flag without replacing the binary would have
   pointed every new user at a dead port. The rebuilt seeder answers `:26311` for both.
2. **Stale August `testnet-12` datadirs, with consensus data.** `/root/.t12*` existed on 5.104.81.23
   (64M + 48M + 53M) and 95.111.236.186 (87M) from the internal relaunches the suffix was minted for
   in August. Moved aside as `.aug2026-regenesis-bak-<ts>`, never deleted. Starting on one would have
   meant either a genesis-mismatch refusal or, worse, a silent resume of a dead chain. **The same
   applies to every first-deployment `.t12*` datadir now.**
3. **A plain `SIGTERM` can leave kaspad hung after it has closed its listeners.** Observed in the
   drill: the node logged `SIGTERM - shutting down…`, stopped its P2P/gRPC/wRPC servers, and then sat
   in state `S` for 19 minutes ignoring both SIGTERM and SIGINT — a process that looks alive and
   serves nothing. The live units are already protected (`KillSignal=SIGINT` + `TimeoutStopSec`, which
   escalates to SIGKILL), and the switch confirmed it: **no hung t11 node was left on any host.** A
   bare `kill` in a script is not protected.

### Drill evidence (first deployment)

Same binary as shipped. `consensus/tests/palw_t12_liveness.rs` and
`consensus/core/tests/t12_economic_safety_drill.rs` cover the rule-level half; this is the live half.

* **boot** — both nodes loaded the genesis, peered, and `[palw-heartbeat-miner] heartbeat #1 … the
  clock ticked` inside 35 s. "**bondless** heartbeat lane (ADR-0060), fee-only" is ADR-0151 D3 in the
  node's own words: the lane that carries this chain's clock takes no bond, so no collateral figure
  can stop it.
* **restart** on the same datadir — ticks continued, 0 errors.
* **IBD** — a third node with no datadir accepted 8 blocks and started minting, 0 errors.
* **partition** — with its only peer gone, the survivor kept ticking (76 beats alone). This is the
  liveness property the collateral reduction rests on, observed under partition.
* **join as a user would** — a fresh node with no `--addpeer` and no `--connect` queried the four DNS
  seeders, got addresses from seeder1 and seeder3 (seeder2/seeder4 are not delegated in public DNS),
  connected to **both** public nodes (`169.58.232.113:26311` and, once peer exchange gave it the port,
  `169.58.39.220:26321`), accepted blocks via relay and tracked the tip (work 21 → 23), with 0 network
  mismatches.

*What this evidence did not show, and the new genesis must: every item above is heartbeat-lane
evidence. None of it involved a PALW attempt block. See "起動判定" above.*

**Still owed** (ADR-0151 §4): a forced reorg, and the class-at-cap states on a fleet that is actually
producing model blocks.

### Operational notes (first deployment)

* **External t11 users are now refused, by design.** ibm logged
  `handshake failed … Network mismatch - local: misaka-testnet-12, remote: misaka-testnet-11` from
  three Japanese IPs within minutes of the switch. The network-id gate is working; those participants
  need this build. **An announcement is the operator's to make.** The new genesis refuses first-
  deployment t12 nodes the same way (genesis mismatch), so it needs its own announcement.
* Only `169.58.232.113:26311` is reachable at the default P2P port. ibm's node listens on 26321, so a
  client that picks it from a seeder answer and dials the default port fails and must retry.
* `/root/misakas-stale-consensus-diagnosis` is still running a t11 fixture node on 5.104.81.23 on its
  own ports and datadir. It belongs to another session's work and was left alone.

### Fleet state at the end of the switch (first deployment)

| host | node | seeder | blocks | peers | fatal |
|---|---|---|---|---|---|
| 169.58.232.113 | active | active | 26 | 3 | 0 |
| 169.58.39.220 (ibm) | active | active | 40 | 10 | 0 |
| 5.104.81.23 | 4 seats active | active | — | — | 0 |
| 95.111.236.186 | (seeder only) | active | — | — | — |
