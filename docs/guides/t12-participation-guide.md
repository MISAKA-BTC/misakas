# testnet-12 参加の手引き(G14 flag day 以降)

対象: testnet-12 に参加する人。full node 運用者、node を持たない miner、job の投稿者、モデルの登録者、外部の告発者、wallet の保有者。
最終確認: 2026-10-27、branch `claude/ux-consistency`(基点 `ffabfcb55`)。

- コマンドは、この版の実装に実在するものだけを書く。
- 「追加中」と書いたものは、lane UX-A(状態・エラー表示)、UX-B(job 投稿・告発・ingress・wallet)、PANELDA(開示)が実装中である。まだ使えない。
- 設計前提は [PRINCIPLES](../PRINCIPLES.md)。CLI の読み取り系の詳細は [palw-cli.md](palw-cli.md)。

## 0. 高さと、その高さで変わること

**G14 flag day は DAA 17,000**(`PALW_T12_G14_17000_DAA_V1`、2026-10-27 ユーザー決定で 19,000 から変更)。
依頼文の「16,600」は現行 preset と一致しない。この手引きは preset の値に従う。

DAA 17,000 で、次の一覧(`PALW_T12_G14_17000_FENCES_V1`)が一緒に武装される。

| 区分 | fence | 参加者への影響 |
|---|---|---|
| Panel=1 kernel slice | `palw_kernel_route_panel_one_v1`、`palw_probabilistic_constraints_v1`、`palw_provider_court_v1`、`palw_bond_budget_v1`、`palw_claim_hard_deadline_v1`、`palw_settlement_once_v1` | kernel route の claim を Panel 1 席で受理する。claim 担保は 1,080 MSK |
| Gate C | `palw_independently_work_certified_v1` | 未認証の claim は fork choice・決済を進めない(全モデル共通) |
| 6 family | `palw_legacy_public_filer_v1`、`palw_legacy_held_da_v2`、`palw_staged_public_court_v1`、`palw_staged_tir_route_v1`、`palw_tir_public_conviction_v1`、`palw_court_outsider_lane_v1`、`palw_court_clock_v2`、`palw_court_capacity_v1` ほか | Legacy・FP 制約/prefix/tokenizer/selection などの公開告発経路が開く |
| PanelDa | `palw_panel_da_disclosure_v1` | 非公開プロンプトの claim は、期限までの開示が必須 |
| 全体 | `palw_g14_route_required_v1` | 経路が武装していない family は新規報酬なし |

int-13 の 4 本は 15,500 のまま変わらない。

**自分の node が実際に何を武装しているかは、必ず node に聞く。**

```sh
misaka palw fences --armed-only
misaka palw fences --list g14At17000     # G14 flag day の一覧(この版で view に追加)
misaka palw status                       # 武装状況・full activation・未採択の値
misaka palw not-adopted                  # 合意が typed-missing で持つ値
```

## 1. full node 運用者

| 手順 | コマンド |
|---|---|
| 初期設定(目的を選ぶ対話) | `misaka init` |
| node の起動・停止・状態 | `misaka node start` / `stop` / `status` |
| 不足の診断 | `misaka doctor` |
| 出荷 binary の fence 表を確認 | `misaka palw fences` |
| ログ | `misaka logs` |

- Panel の席の義務(receipt、replay、readiness、DA 応答、自動 filer)は、席の identity(`--palw-producer-key` と `--palw-producer-bond`)を持つ node で常に動く。
  - `--palw-panel` は廃止済みで、何もしない。
  - 自動 filer の carrier には `--palw-fee-outpoint` が要る。
- court の有料 ingress は全 bonded filer で常時有効。緊急停止は `--palw-disable-court-paid-ingress`。
- evidence provider のディレクトリを読ませるときは `--palw-evidence-provider-dir <dir,dir>`(§6)。
- 告発を優先したい公開 claim は `--palw-public-claim <128hex>`(最大 16)。

## 2. node を持たない miner(RFC-0009)

binary は `palw-remote-miner`。複数の node から読み、意見が割れたら止まる。

| 運用 | 指定 | 安全分類 |
|---|---|---|
| A: 自分の full node | `--own-node` | full node と同じ |
| B: 検証付き remote(推奨の既定) | `--rpc a,b,c --checkpoint <daa>:<hash> --verify-headers [--fork-choice-issuer … --fork-choice-attestations <dir>]`、bond は `--pin <hash>` で証明 | `VERIFIED_REMOTE`(L2 は `palw_fork_choice_commitment_v1` 武装前は未検証のまま) |
| C: 任意の pool・未検証 | `--accept-unverified-state UNVERIFIED_REMOTE` などと明示 | `UNVERIFIED_REMOTE`。full node 相当とは言わない |

```sh
palw-remote-miner --network testnet-12 --rpc nodeA:17210,nodeB:17210,nodeC:17210 \
  --bond <txid>:<index> --class <class id> --artifact-root <root> --pay-address <addr> \
  --key-file <seed> --checkpoint <daa>:<block hash> --pin <block hash> \
  --executor-cmd <program> --state-dir <dir>
```

- executor はこの binary に含まれない。`--executor-cmd` に自分の実行器を渡す。
- material は `--state-dir/material/` に残る。evidence provider に預けるまで、この機械は止めない。
- bond の担保や状態は `misaka bond …`、`misaka palw bond-detail <txid:index>` で見る。

## 3. job の投稿者

### 3.1 kernel route(Panel=1)

- 現状は node の flag で投稿する: `kaspad … --palw-kernel-model <artifact> --palw-kernel-post-job <tok,tok,..>:<max_new_tokens> --palw-fee-outpoint <outpoint>`。
- kernel claim は B = F = 0(合意重みなし)。支払いは job escrow から、Gate C の認証後に一度だけ。
- **追加中(UX-B):** `misaka` からの job 投稿と状態確認。

### 3.2 free prompt(FP)

```sh
misaka palw gen-claim --artifact <PALWTIR2> --request <job.json> …   # commitment tx と material を書く(提出はしない)
misaka palw fp-submit --tx <claim>.commitment-tx.borsh --yes
misaka palw claim-detail <claim>     # op 225
misaka palw claim <claim>            # 旧い読み取り(op 225 が無い node 向け)
```

- job JSON の `"privacy": "panel"` で PanelDa(§3.3)になる。`"public"` は ids を commitment に載せる。

### 3.3 PanelDa(非公開テキストプロンプト)

- prompt は `prompt_token_ids_hash` で固定される。service 中に中身を見るのは席だけ。
- **開示の期限:** 受理から 1,040 DAA 以内に、誰でも token ids を開示できる(tag 233、署名不要)。その後、外部者の notice 窓が 2,080 DAA 続く。値は目安。
- 期限までに開示がないと `PanelDaDisclosureDefault` で claim は void、deposit は burn、支払いはない。計算の有罪にはならない。
- 開示までは報酬も credit もない(Final floor が期限 + 1)。
- **追加中(PANELDA):** 開示 object を作る CLI。それまでは SDK で object を作り、`misaka palw submit-object --object <file> --yes` で出す。

## 4. モデルの登録者

| 手順 | コマンド |
|---|---|
| artifact の事前検査 | `misaka model preflight <artifact>` |
| TIR 登録 object の作成 | `misaka palw tir-registration …` → `misaka palw submit-object` |
| kernel class の登録(kernel route) | `kaspad --palw-kernel-register-class <.register-class.bin>`(`--palw-kernel-register-class-dry-run` で確認) |
| 登録済みの一覧・係数 | `misaka model list` / `misaka palw model-show` |

- genesis と年次フォークで指定されたモデルは係数 1.0。後から登録したモデルは、登録した時点から 0.3 に固定される。モデル単位の審査はない。
- claim 単位の検証(court、Gate C)は全モデルで行われる。

## 5. 外部の告発者

| 手順 | コマンド |
|---|---|
| claim と court の状態 | `misaka palw claim-detail <claim>`、`misaka palw court-state --claim <claim>` |
| DA 告発 | `misaka palw da-accuse …` → `misaka palw submit-object` |
| shard 告発 | `misaka palw shard-accuse …` |
| legacy dispute の予約・解除 | `misaka palw build dispute-reserve` / `dispute-release`(`palw_legacy_public_filer_v1` 未武装なら exit 10) |
| court の close | `misaka palw court-close …` |
| 自動の公開 filer | full node の席 duty。優先 claim は `--palw-public-claim` |

- 未採択の値に依存する builder(`funded-original`、`court-ingress`、`material-intake`、`d2-demand`、`position-close`)は、値が採択されるまで exit 10 で拒否する。採択されると `misaka palw not-adopted` の行が `adopted` に変わる。
- PanelDa の claim は、開示後に公開 FP 経路で告発できる。
- **追加中(UX-B):** 6 family それぞれの告発 object を作る CLI と、ingress 手数料の見積り。

## 6. evidence provider

material を代わりに保管し、Panel と外部者に配る。

```sh
palw-evidence serve  --listen <addr> --root <dir> --max-store-gb <N> --network-domain <hex>
palw-evidence publish --claim <claim> --file <material> --providers <a,b> …
palw-evidence status|fetch|repair --claim <claim> --providers <a,b> --min-copies <N>
```

- node 側は `--palw-evidence-provider-dir` で読む。manifest は claim の on-chain root と照合され、全 chunk が検査される。producer からの取得は fallback として残る。
- provider court は `palw_provider_court_v1`(DAA 17,000)で有効になる。

## 7. wallet の保有者

| 手順 | コマンド |
|---|---|
| 鍵の作成・取込み・address | `misaka key gen` / `import` / `address` |
| UTXO・送金・統合 | `misaka wallet list` / `send` / `consolidate` |
| bond の確認・引退 | `misaka bond …` |
| 報酬・vesting | `misaka rewards`、`misaka palw vesting` |
| EVM lane | `misaka evm balance` / `deposit-lock` / `claim` / `refund` / `send` |
| モデル position | `misaka position …`、`misaka palw model-buy` / `model-sell` |

- **追加中(UX-B):** wallet 側の PALW 操作の整理。**追加中(UX-A):** 状態とエラーの表示の統一。

## 8. 終了コード

[palw-cli.md §4](palw-cli.md) と同じ。exit 10 は安全のための拒否(未採択の値、休眠中の fence)。
