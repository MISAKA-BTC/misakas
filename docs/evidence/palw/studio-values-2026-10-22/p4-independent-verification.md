# Lane P4 の値 — 独立検証（2026-10-22）

- **対象:** [values-p4-policy-rfc.md](values-p4-policy-rfc.md) の決定値 V12-a、R1-a/R1-b、R2-a/R2-b、R8、R9（c）、R10（U-1/U-2/U-3/U-6/U-7）、R11（4 値）と `consensus/core/src/palw_values_p4_v1.rs`。
- **検証者:** 値を選んでいない独立の agent。worktree `MISAKA-p4-verify`、branch `values/p4-verify`、base `g14/integration-r7` 49d65113e。
- **方法:** 各値について、その値が最も有利にする攻撃者の戦略を作り、実際の定数で利得と必要資本を計算した。正直な参加者が成り立つかと、他の決定値・時計との整合も確かめた。計算 script は `python3 -I`（scratchpad の `p4calc.py`）。一時的な integration test（`consensus/core/tests/zz_p4_independent_verify_tmp.rs`）で engine の挙動を確かめ、commit 前に削除した。product code は変えていない。
- **使った定数:** escrow `E` = 3,200.85 MSK/claim（subsidy 4,445.62 MSK/DAA の 72%）。1 epoch = 720 DAA ⇒ PALW 予算は 2,304,612 MSK/epoch、7 日平均の窓（5,040 DAA）で 16,132,284 MSK。DAA の進みは t12 実測中央値 184 s（名目 120 s。1 epoch は壁時計 36.8 h、名目 24 h）。court clock v2: notice（追及開始の締切）`a + 3,120`、hard `a + 10,000`（`palw_court_policy_v1`）。bond の退出遅延 `withdrawal_delay_daa` は court の最長経路以上（bundle の interlock。R-core+ fixture では 12,900）。RFC-0018 の設計値: 承認期限 2 日、抽選期限 3 日、**G14 公開告発 window 7 日**（`PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1`）。
- **前提の禁止事項:** 計算の暗号学的証明は使わない（PRINCIPLES §8）。モデル単位の検証は提案しない（§9）。§9 の凍結値（1.0/0.3、α = 1.5、7 日平均）は再審しない。報告するのは、凍結値どうしの整合と、それを実装した値の不備だけである。

## 0. 判定

| 値 | 判定 | 要点 |
|---|---|---|
| V12-a `epoch_daa = 720` | **CONFIRMED**（新しい所見 F1 あり） | 境界での借入は増幅されない。ただし epoch ごとに機会がリセットされるため、`P_m < 1/720` のモデルには 1 ブロックも出ない（F1） |
| V12-a `seasoning_epochs = 1` | **CONFIRMED** | 退出遅延が支配する。短期の借入の効率は ≤ 1/15 |
| V12-a `α = 1.5` / n = 7 / `verifier_capital` | **CONFIRMED** | ユーザー決定と整合。集中を促す性質を定量化した（§1.3） |
| V12-a `block_opportunity = true` | **ADJUST（R8 と矛盾）** | `validate_v3` は `false` を拒否し、RFC-0016 V+ は `true` を拒否する。V12-a と R8（V+）は同時に武装できない（F2）。下の §1.4 |
| V12-a `max_models_per_bond = 16` | **CONFIRMED** | 安全上の上限ではない（bond は分割できる）。資源の上限で、16 で足りる |
| R1-a 曲線 `c_m·S̄^1.5` | **CONFIRMED** | ユーザー決定 |
| R1-b cell closure の武装方針と `for_blame_path` | **CONFIRMED（方針）** | fence 後に被覆できない claim は受理で拒否されるので、`Off` 経路に罰されない不正は作れなかった。武装 record の 4 要件は未達のまま |
| R2-a 固定 seed の bootstrap | **CONFIRMED** | ユーザー決定。bootstrap 中の抽選は statement の grinding で完全に操作できる（文書のとおり）。報酬は EXEC credit なしで有界 |
| R2-b `seed = H(domain, network_domain)` | **CONFIRMED**（説明を訂正） | 「運営者が選べない」は起動前の genesis grinding では偽。ただし statement の grinding が支配するので、実害はない。v6 seal-reveal の source は Final を要さないので、bootstrap を閉じた claim が偏りを残さない |
| R8 epoch 720 | **ADJUST** | rights epoch は bundle の `state.epoch_length` で、t12 では **1,000**。720 % 1,000 ≠ 0 なので、V+ の validator が拒否する。1,000 の倍数で 5,040 を割り切る数は無いので、t12 の bundle のままでは、どの 7 日 V3 allocation とも V+ を組めない。bundle の `epoch_length` を 720 にする fence（ruleset id が変わる）か、独立した rights epoch が要る（§4.1） |
| R8 `ε = 0` | **ADJUST** | ε = 0 を正当化できるのは `palw_model_tier_v1` の下だけで、その下ではコードが既に 0 に固定している。P4 §0 では tier fence が 16000 に載らないので、tier なしの V+ では新規モデルの経路が 0 になり、F1 の崖がそのまま残る。訂正: `ε` は tier fence の値ではなく、「tier fence が同じ高さ以下にあるときだけ 0」。そうでなければ未決定（V3 の ≤ 20‰ から実測で選ぶ） |
| R8 `k = 1` | **CONFIRMED** | k > 1 は G2 を最大 k² 崩す |
| R8 V11 移行なし | **CONFIRMED** | 攻撃面は増えない |
| R9 `c = 300‰` | **CONFIRMED** | §9 の凍結値。κ による希釈（§6a.2）は定量化した。claim なしでも実行できる（F3） |
| R10 U-1 | **CONFIRMED** | |
| R10 U-2 通常の分割 | **CONFIRMED** | 受取人が認証経路に依らないことが中立 |
| R10 U-3 認証まで保留 | **CONFIRMED（決定）／監査を訂正** | 正直な待ちは「2 + 3 日」ではなく ≥ Final + 告発 window 7 日（5,040 DAA）。court の notice 締切（`a + 3,120`）と告発 window（Final + 5,040）は別の時計で整合していない（F5） |
| R10 U-6 veto なし | **CONFIRMED** | f = 0.10 で正直な claim の 99.3% を止められる |
| R10 U-7 GHOSTDAG 順 | **CONFIRMED（推奨）** | |
| R11 min providers 2 | **CONFIRMED** | どの値でも Sybil の provider を防げない。最小値が正直な費用を最小にする |
| R11 `W_budget = 720` | **CONFIRMED（値）／条件は不成立** | RFC-0008 の「`L ≤ W_budget`」は court clock v2 の claim（L は最大 10,000）では成り立たない。安全性は `H_L ≥` hard 経路（V12-b、未決定）に依存する（F6） |
| R11 legacy evidence horizon 10,080 | **ADJUST（network ごと）** | t12 では正しい（DNS の `evidence_window_blocks` = 10,080、U = 10,083 ≥ R + E）。mainnet の DNS window は 12,096,000（10 bps）で、10,080 では 16.8 分しかない。値は「その network の `dns_params.evidence_window_blocks`」とし、validator に ≥ を検査させる（今は ≠ 0 だけ） |
| R11 wallet margin 1,000 | **CONFIRMED（t12）／説明を訂正** | 名目 120 s では 33 h で、「2 日以上」は偽。必要なのは署名から取り込みまでの余裕なので、t12 では足りる。network ごとの DAA 速度に依存する（10 bps では 100 s） |

## 1. V12-a `palw_model_bond_allocation_v1`

### 1.1 境界での借入（epoch・seasoning）

- **engine の動き（`allocation_v3.rs`、`palw_bond_budget_fold_v1.rs:2296–2352`）:** epoch の roll の瞬間に、bond ごとの snapshot を 1 つ取る。対象は `capital_of = (collateral, status == Active)` で、seasoning を済ませた assign と、生きている claim の seat exposure を、bond の元本で pro-rata に clip したもの。`Retiring` の bond は roll の時点で 0 になる。assign は `pending_since + 1 + seasoning` の roll で有効になる。減らす変更はすぐ効き（`min(effective, pending)`）、移す変更は 2 epoch のあいだ 0 になる。
- **最も有利な攻撃:** epoch e の snapshot だけに数えさせる。必要な拘束は、epoch e−2 の末に assign した時点から roll e まで Active（≈ 721 DAA）であることと、その後 `Retiring` で退出遅延のあいだ拘束されること（≥ court 経路、≥ 10,000 DAA）。

| 退出遅延 | 1 snapshot（平均重み 1 epoch 分）に要る拘束 | 効率 |
|---|---|---|
| 0（仮） | 721 DAA | 0.999 |
| 10,000 | 10,721 DAA | 0.067 |
| 12,900 | 13,621 DAA | 0.053 |

  正直な連続拘束は効率 ≈ 1 なので、借入は 15〜19 倍不利で、増幅はしない。元の監査は「1 日 hold で引き出す」と書いたが、bond の退出を決めるのは budget の hold ではなく `withdrawal_delay_daa` で、結論はより強くなる。
- **seasoning を 2 以上にする利得:** 無い（借入は上の表で既に不利）。正直な bond が 1 epoch ずつ遅れるだけ。**CONFIRMED。**
- **epoch の長さ:** 360/1,680/5,040 と比べて、借入の効率はどれも同じ（退出遅延が支配する）。違いは F1 の崖の高さ（§1.2）と平均の分解能だけ。

### 1.2 F1（新しい所見）— epoch ごとのリセットで `P_m < 1/720` は 0 ブロックになる

- `model_blocks_available_v1` は `⌊accrued·A_m/ΣA⌋ − reserved` を返す。accrued は epoch の開始から 1 DAA あたり 1,000,000 unit で、1 ブロックは 1,000,000 unit（`PALW_BUDGET_BLOCK_UNIT_V1`）。epoch をまたいで繰り越さない。RFC-0016 V+ の `Rt_m = ⌊(N_exp − Ent)·A_m/ΣA⌋` も同じ形をしていて、σ による再発行の上限も `Rt_m·(1+σ) = 0` になる。
- **したがって、`P_m < 1/720 = 0.139%` のモデルは epoch 全体で 1 ブロックも受理されず、coinbase も 0 になる。** 比例して減るのではなく、崖になる（一時 test `p4v_block_opportunity_cliff_at_one_over_720` で engine の値を確認。§4）。
- 資本に直した閾値（`A = c·S^1.5`）:

| 比較の相手 | c = 1.0 | c = 0.3（登録） |
|---|---|---|
| 支配的な 1 モデル | 自分の資本 ≥ 1.25% | ≥ 2.78% |
| 同額の genesis 8 モデル（各 S_g） | ≥ 0.050·S_g（genesis 全体の 0.62%） | ≥ 0.111·S_g（1.39%） |

  立ち上がりの途中は `S̄ = (k/7)·S` なので、最初の 1 epoch に数えられるには最終の `P_m` が崖の 18.5 倍（2.6%）必要になる。
- epoch の長さを変えると崖は動く（360: 0.278%、1,680: 0.060%、5,040: 0.020%）が、5,040 にすると n = 1 で 7 日平均が消える。**値の問題ではなく構造の問題**（端数の accrual を繰り越すか、`ε` の entrant 経路を残すか）で、ユーザーの有効化前条件「新モデルが α = 1.5 で飢えないこと」に直接かかわる。epoch 720 は CONFIRMED とし、F1 は Lead・ユーザーへの報告事項とする。

### 1.3 α = 1.5 と分割・集中

- 同じ capital を k モデルに分けると、重みは `k^{1−α}` 倍（16 分割で 1/4）になる。分割は分母も小さくする。`max_models_per_bond` は安全上の上限ではない（同じ所有者が bond を何本でも作れる）。資源の上限として 16 で足りる。**CONFIRMED。**
- 集中: 1 単位の capital あたりの取り分は `R·S_m^{0.5}/ΣA` で、`√S_m` に比例する。資本 4 倍のモデルの miner は 1 単位あたり 2 倍を得るので、資本が 1 モデルへ集まる方向に動く。これは ADR-0177 が認めている性質（「α = 1.5 は資本集中を強める」）で、新規モデルは 1 単位あたり `√(S_new/S_big)` 倍の取り分になる（1% なら 0.1 倍、登録 tier ならさらに 0.3 倍）。凍結値なので判定は変えない。

### 1.4 F2 — V12-a `block_opportunity = true` と R8（V+）は同時に武装できない

- `PalwModelAllocationPolicyV1::validate_v3`（`allocation_v3.rs`）は `!average.block_opportunity` を拒否する。P4 の test も `false` が拒否されることを assert している。
- `Params::validate_palw_model_block_rights_v1`（`palw_rfc0016_v1/values.rs:545`）は、`allocation.policy.average.is_some_and(|a| a.block_opportunity)` を「`P_m` を 2 回ブロックへ掛ける: どちらかだけを武装」として拒否する。
- V2 の allocation（average なし）なら V+ は通るが、V2 には 7 日平均も α の固定も無く、ユーザー決定を満たさない。
- **結論:** 決定した組（V12-a の V3 + `block_opportunity = true`、R8 の V+ 採用・epoch 720）は、どの高さでも同時に武装できない。一時 test `p4v_decided_allocation_and_rfc0016_vplus_cannot_both_arm` で実際の validator の拒否を確認した（§4）。
- **ADJUST:** `P_m` をブロックへ一度だけ適用するという決定（RFC-0015 §8.5.5）は、V+ が武装されているときは V+ が担う。V12-a は「V+ が同じ高さ以下にあるときは `block_opportunity = false`、そうでなければ `true`」としなければならない。これには `validate_v3` を「V+ と組むときは `false` を許す」に変える必要があり、値だけでは直らない（コードの変更、Lead の判断）。

### 1.5 F3 — miner 側の資本は claim に拘束されていない（監査の記述の誤り）

- 元の監査は「claim に拘束された分だけが数えられる（待機 bond・登録料・Position は数えない）」と書いた。**miner 側については誤り。** `S_m` の miner 部分は、Active な bond の assign（seasoning 済み）を元本で clip したもので、claim は 1 件も要らない（`palw_allocation_snapshot_v3`）。これは ADR-0177 D3（「割り当てた適格な拘束元本」）のとおりだが、PRINCIPLES §10.2 の「miner の claim bond の総額（Final まで返らない担保）」とは食い違う。claim 拘束が要るのは verifier 側（seat exposure、ADR-0177 C5）だけである。
- **帰結（claim がなく、slash の危険もない希釈）:** 使われなかったモデルの枠は発行されない（ADR-0177 D4、RFC-0016「失効、発行しない」）。そのため、claim を出さない assign は、ほかの全モデルの発行をその分だけ焼く。正直な資本 G が genesis 8 モデルに等分されているとき:

| 攻撃者が置く資本 X | tier なし | 登録モデル（c = 0.3、κ の分母は縮めない） |
|---|---|---|
| 正直な発行の 25% を焼く | 0.240 G | 0.536 G |
| 正直な発行の 50% を焼く | 0.500 G | 1.116 G |

  費用は X の機会費用だけ（slash も DA 費用もない）。一時 test `p4v_idle_assignment_counts_and_dilutes_without_any_claim` で、claim を出さない bond が 400（G = 800 の半分）を 1 モデルに assign すると、epoch の半分を取ることを確認した。V+ の σ 再発行（calibration の初期値 0.25）は、正直なモデルに最大 25% を戻すだけである。
- R9 の κ の反例（§6a.2）は、この経路で **claim なしに** 実行できる。§7 の依頼 4 への答えは「安い。claim 拘束の条件は miner 側には無い」。
- 値の訂正ではない（ADR-0177 D3 の構造）。Lead・ユーザーへの報告事項: miner 側も「生きている claim の予約に拘束された元本」に限るか、未使用の枠を `A_m` 加重で再配分するか。どちらも §11 の手続きが要る。

## 2. R1・R2

### 2.1 R1-b（cell closure、`for_blame_path`）

- `for_mirrors` は `accepted_daa ≥ fence` で決まり、遡及しない。`for_blame_path` は、consumer v2 の下では全経路を同じ mode にし、それ以外では round 4 で closure を使っていた経路（Panel false-valid、fault site、V1 contradiction）を `Legacy` に、`ExecutorRefuted` と terminal carriage を `Off` に保つ。
- 反証の試み: fence 後に被覆できない class の claim は、受理の時点で拒否される（REQ-CELL-1/1b）。`Off` の経路に残るのは、fence 前に受理された claim（REQ-CELL-3、旧規則）だけである。fence 後の claim で、`Off` のために罰されない不正は作れなかった。正直な seat が後から厳しい mode で有罪になる経路も無い（受理時の mode を固定するため）。**CONFIRMED（方針）。** 武装 record の要件 1・2・4（suite、node E2E、跨ぎ drill）は未実施のまま。

### 2.2 R2-a / R2-b（bootstrap seed）

- **network_domain の入力:** `palw_network_domain_v2_for(net.to_string(), genesis.hash)`。network 名か genesis が違えば seed も違う。drill の genesis は drill marker を含むので、t12 とは別になる。
- **seed の grinding:** 起動前の network（mainnet）では、genesis の作成者が timestamp などを変えて genesis hash を、つまり seed を選べる。「運営者が seed を選べない」は、起動前については偽である。ただし、1 claim の抽選は `paid_claim_bootstrap_seed_v2(seed, context, commitment_root)` で、producer は statement（job nonce）を自由に、ほぼ費用なしで grind できる。seed の選択で得られるものは、statement の grinding で既に得られる。候補は「producer の operator を除く全 Active bond」で、operator は bond ごとの自己申告なので Sybil で除外を避けられる。したがって、bootstrap 中の選出の独立性は seed と無関係にゼロである（文書の認識どおり）。
- **bootstrap と v6 の関係:** `collect_paid_seal_reveal_seed_v6` が `Some` なら常にそれを使う。bootstrap seed は、v6 が `None` で、かつ承認済み paid Final がまだ無いときだけ使われる（`palw_claim_challenge_fold_v2.rs:617–622`）。
  - 攻撃者は source を 1 つ withhold して v6 を `None` にでき、窓が開いている間は自分の claim を bootstrap 経路へ回せる。さらに、自分の共謀 claim を最初の承認済み paid Final にして、窓を閉じられる。
  - 窓を閉じた claim は v6 の入力にならない（v6 の source は Final を要さない、「sealed after the target」だけ）。**したがって、閉じた後の seal-reveal に bootstrap の偏りは残らない**（§7 の依頼 3 への答え）。
  - v6 自身の前提「mix の少なくとも 1 source が正直」は、k（R3、未決定）と Sybil source の費用で決まる。R2-b の値とは独立である。
- 得られるもの: bootstrap の lock は EXEC credit を持たず、payer の損は fee まで、producer は claim 単位の court と slash の対象のまま。**R2-a・R2-b とも CONFIRMED**（R2-b の「選べない」は「選んでも意味がない」に訂正）。

## 3. R8〜R11

### 3.1 R8

- **epoch 720:** rights epoch は `bundle.state.epoch_length()`（admission の budget predicate と共有する PALW の state epoch）で、allocation の `epoch_daa` はその整数倍でなければならない（`values.rs:566`）。t12 の bundle は 1,000 なので、720 は通らない（§4.1、ADJUST）。
- **ε = 0:** §0 の表のとおり ADJUST。根拠の ADR-0179（「tier の下では entrant がいない」）は tier fence の下でしか成り立たず、その下ではコードが既に `entrant_reserve_permille = 0` に固定している（`palw_rfc0016_fold_v1.rs:143`）。tier fence（#32）は 16000 に載らないので、V+ を先に武装すると、ε = 0 は新規モデルの唯一の経路を消し、F1 の崖（`P_m < 1/720` で 0）がそのまま残る。
- **k = 1、V11:** CONFIRMED。

### 3.2 R9 c = 300‰

凍結値。CONFIRMED。κ は未決定のまま。F3 の表のとおり、分母を縮めない限り、登録 tier の資本 1.116 G で正規 tier の発行が半分になる。攻撃者の受け取りは κ（最大 30%）に頭打ちされ、残り 20% は発行されない。資本 1 単位あたりの利回りは、攻撃者 0.27·R/G に対し正直 0.5·R/G で、攻撃者に利益は無い。それでも、外部の動機があれば claim なしで実行できる希釈である。

### 3.3 R10

- **U-3:** 認証は「m-of-k の承認」と「G14 公開告発 window（`PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1` = 7 日）が Final から経過」の両方を要る（RFC-0018 §7 item 4）。正直な producer の待ちは ≥ Final + 5,040 DAA で、t12 の 184 s/DAA で約 11 日（名目 7.1 日）。元の監査の「2 + 3 日」は過小である。
- **F5（時計の不整合）:** court clock v2 は、新しい追及を `a + 3,120` までしか開始させない（`palw_claim_pursuit_may_start_v2`）。一方、Gate C の告発 window は Final + 5,040 まで「異議なし」を待つ。`a + 3,120` から Final + 5,040 までの約 2,000 DAA 以上は、追及を始められないのに認証を待つだけの区間になる。安全性は増えず、正直な発行由来 leg の保留が延びるだけである。どちらの時計を正にするかは Lead の判断事項（告発 window = `min(prosecution, notice cutoff − Final)`、または notice cutoff の延長）。U-3 の決定自体は PRINCIPLES（未認証の仕事に発行しない）に従うので CONFIRMED。
- **U-6:** veto `v = 1` で m = 47 のとき、f = 0.05 で 91.0%、f = 0.10 で 99.3% の claim が止まる。CONFIRMED。
- **U-2・U-1・U-7:** CONFIRMED。U-5 の表は再計算で一致した（47/32、f = 0.25、a = 0.9 で、正直な認証確率 0.535、誤認証 6.4e-10）。

### 3.4 R11

- **min providers 2:** 2 provider の共謀 = producer が Sybil の provider を 2 つ用意すること。3 でも 8 でも同じ Sybil で満たせるので、値を上げても独立性は買えない。保持をやめた provider は DA/default の責任を負う（bonded の鍵）。値を上げると正直な session の費用だけが増える。CONFIRMED。
- **F6 — `W_budget = 720`:** 値は `hold.window_daa(PanelZero) = 1·720` でコードが強制し、ユーザーの 1 日 hold と一致する。RFC-0008 R8-BUDGET 6 の条件「`L ≤ W_budget`」の `L` は、ECON §5e.4 の interim kernel の寿命（280〜301 DAA）である。court clock v2 の claim は hard `a + 10,000` まで生きうるので、条件は成り立たない。ADR-0176 D4 は、枠の回復（`d + W`）と責任の担保（`terminal + H_L`、同時責任上限）を分けている。だから `W = 720` が安全かどうかは `H_L ≥` hard 経路と同時責任上限で決まり、これは V12-b で未決定である。`W_budget = 720` は単独では武装できない、と記録すべきである。
- **legacy evidence horizon:** `legacy_dns_evidence_allowed_v1` は、retirement の後 `daa − activation ≤ horizon`、かつ両 target が activation より前のときだけ DNS の equivocation 証拠を受ける。DNS 自身の window と組み合わせると、horizon ≥ `evidence_window_blocks` で旧規則を削らない。
  - t12/testnet（`TESTNET_DNS_PARAMS`）: window 10,080、`unbonding_period_blocks` 10,083 ≥ R(3) + E。直前に equivocate して退出した validator も、10,080 以内なら slash できる。**t12 では正しい。**
  - mainnet（`PRODUCTION_DNS_PARAMS`、10 bps）: window 12,096,000。10,080 では 16.8 分で証拠を締め切り、retirement の直前の equivocator が 14 日の unbond を経て逃げる。
  - validator（`params/validation.rs:1054`）は `== 0` だけを拒否する。**ADJUST:** horizon は network ごとに `= dns_params.evidence_window_blocks`（t12 10,080 / mainnet 12,096,000 / devnet 300）とし、validator に `≥` を検査させる。
- **wallet margin 1,000:** client の preflight は、retirement が sink から 1,000 DAA 以内なら retired kind を拒否する。要るのは「署名から取り込みまでの遅延」を上回る余裕で、t12 では名目 33 h / 実測 51 h なので十分である。元の説明の「2 日以上」は名目の速度では偽。誤って署名した tx は retirement 後に無効になるだけで、資金は失わない。CONFIRMED（t12）。10 bps の network では 100 s になるので、壁時計で定めるべきである。

## 4. 一時 test の結果

`CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-g14/target misaka-heavy-lock.sh cargo test --release -p kaspa-consensus-core --test zz_p4_independent_verify_tmp -- --nocapture`（file は commit 前に削除）:

4 passed, 0 failed（release、2026-10-22）。出力:

```
units over a whole epoch: P=1/720-eps -> 999999, P=1/720 -> 1000000 (one block = 1000000)
honest 8 models (800 capital): 360 blocks/epoch; idle 400 on one model: 360 blocks/epoch
t12 bundle.state.epoch_length() = 1000
V+ with the decided V3 allocation: Invalid("palw_model_block_rights_v1 and the allocation's block_opportunity would apply P_m to blocks twice: arm one")
```

- `p4v_block_opportunity_cliff_at_one_over_720`: `P_m = 1/720 − ε` では epoch 全体で 999,999 unit で、1 ブロック（1,000,000）に届かない（F1）。
- `p4v_idle_assignment_counts_and_dilutes_without_any_claim`: claim を 1 件も出さない bond の assign 400 が、正直な 800（8 モデル）と同じ 360 ブロック/epoch を取る（F3）。
- `p4v_decided_allocation_and_rfc0016_vplus_cannot_both_arm`: 決定した V3 policy に `block_opportunity = false` を入れると `validate` が拒否し、`true` のままでは V+ の validator が拒否する（F2）。
- `p4v_t12_bundle_epoch_length`: §4.1。

### 4.1 t12 の bundle `epoch_length`

**t12 の `bundle.state.epoch_length()` は 1,000 DAA である。** RFC-0016 V+ の rights epoch はこの値を読む（`palw_rfc0016_fold_v1.rs:114`）。validator は `allocation.epoch_daa % rights_epoch == 0` を要求し（`palw_rfc0016_v1/values.rs:566`）、calibration では `rights_epoch == 720` を要求する。

- 720 % 1,000 ≠ 0 なので、t12 の bundle のままでは、決定した allocation（720）の上に V+ を武装できない。
- `validate_v3` は epoch が 5,040 を割り切ることを要求するが、1,000 の倍数で 5,040 を割り切る数は無い。したがって、t12 の bundle では、どの V3 の 7 日 allocation とも V+ を組めない。
- **R8 の「epoch 720」は、bundle の state `epoch_length` を 720（またはその約数）に変えることを含意する。** `epoch_length` は ruleset id・fingerprint に入る consensus の値で、admission の budget predicate、`claim_retirement_daa > epoch_length` の検査（`params/validation.rs:1031`）などが共有している。fence での切り替えが要る。または、rights epoch を独立した parameter にする（コードの変更）。
- 判定を **ADJUST** に改める: R8 の epoch 720 は「rights epoch（= bundle の `epoch_length`）を 720 にする fence と同時に」でなければ成り立たない。値だけでは不十分である。

## 5. Lead への依頼（値以外）

1. **F2:** `validate_v3` を「V+（`palw_model_block_rights_v1`）が同じ高さ以下なら `block_opportunity = false` を要求し、そうでなければ `true`」へ変える。今のままでは V12-a と R8 は同時に武装できない。
2. **F1:** epoch 内で端数の accrual を繰り越さないため、`P_m < 1/720` は 0 ブロックになる。新モデルの飢餓という有効化前条件の評価に含める。
3. **F3:** miner 側の `S_m` が claim なしの assign を数えること（PRINCIPLES §10.2 との食い違い）と、焼却型の希釈（X = 0.5 G で 50%）。
4. **F5:** Gate C の告発 window（Final + 5,040）と court clock v2 の notice 締切（`a + 3,120`）の不整合。
5. **R8:** t12 の bundle `epoch_length` は 1,000 なので、V+ は epoch 720 の allocation と組めない。bundle の epoch を 720 にする fence か、独立した rights epoch が要る。
6. **R11:** legacy evidence horizon を network ごとの導出値にし、validator の検査を `≥ evidence_window_blocks` にする。
