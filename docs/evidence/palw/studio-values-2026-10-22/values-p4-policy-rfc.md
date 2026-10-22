# Lane P4 — V12・R1〜R7 の値（2026-10-22）

- **状態:** 決定した値は「**決定済み、独立検証待ち**」（decided, awaiting independent verification）。fence の武装、arming record・measured record の pin、出荷 preset の activation 高さの変更は、どれも行っていない（Lead が独立検証のあとで行う）。`parameter-register.md` / `parameters.json` も編集していない。
- **根拠:** ユーザーの値決定の委任（2026-10-22、[作業指示 (2)](../../../design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md)）、[16000 の判断表](../../../design/palw/integration/g14-t12-16000-decisions-2026-10-22.md)、[rfcgap-a](../rfc-gaps-2026-10-22/a.md)、ADR-0176/0177、RFC-0006/0007/0008/0010/0016/0002、PRINCIPLES §8〜§10。
- **branch:** push 先 `studio/values-p4-policy-rfc`（local `values/p4-policy-rfc`、base `g14/integration-r7` 34d68d7e2、のち 193c2c65b（558619700 を含む）を merge）。コードは新しい module `consensus/core/src/palw_values_p4_v1.rs`（どの preset からも読まれない）、`allocation_v3.rs` の epoch 0 の拒否の修正、`tests/adr0177_allocation_v3.rs` の 7 日化。
- **機材:** `sysctl hw.model hw.memsize hw.ncpu` = `Mac13,1`（Apple M1 Max の Mac Studio）、34,359,738,368 B（32 GiB）、10。最低要件ハードウェアではない。
- **前提の禁止事項:** 計算の暗号学的証明は使わず提案もしない（PRINCIPLES §8）。モデル単位の検証・審査・昇格は提案しない（§9）。下の hash は公開 seed の導出であり、計算の証明ではない。

## 0. まとめ

| # | 値 | 決定 | 拘束条件（binding constraint） | 状態 |
|---|---|---|---|---|
| V12-a | `palw_model_bond_allocation_v1` の policy | V3、`epoch_daa = 720`、`seasoning_epochs = 1`、`α = 1.5`（`alpha_halves = 3`）、7 日平均（5,040 DAA、n = 7）、`verifier_capital = true`、`block_opportunity = true`、`max_models_per_bond = 16` | α・7 日・miner+verifier・P_m の適用はユーザー決定。epoch は「7 日を整数日に割る最も粗い epoch＝1 日 hold」、seasoning は `validate` の下限、16 は genesis 8 + 登録 8 | 決定済み・独立検証待ち。**fence は武装不可のまま**（ADR-0177 D6 とユーザーの有効化前条件は値ではない） |
| V12-b | `palw_bond_budget_v1` の policy（V3） | 構造だけ決定: V3、hold = 720 DAA/日・Panel=1 1 日・Panel=0 1 日、`window_daa = 720`、`round_rights = ExecutionCap`、`weight_value = Unknown` | 日あたりの率 `q, b, r, f`・burst・同時責任上限・`H_L`・export cap は ADR-0176 Stage 1（実ノードで ≥ 2,160 DAA の正直 miner 実測）が無いと決まらない | **未決定（実測なし）**。コードには入れていない |
| V12-c | `palw_material_income_v3` | 値は無い（activation だけの fence） | 拒否理由は値ではなく「signed material/TTL と consumer lifecycle の検証未了」、かつ `palw_reporter_income_v2` が武装不可 | 値の対象外。16000 の前提として**満たせない** |
| R1-a | RFC-0006 配分曲線 | `A_m = c_m · S̄_m^1.5`（α = 1.5）。2026-10-12 の `S_m·μ(S_m)` 案は採らない | ユーザー決定 2026-10-14（ADR-0177 改定、CLAUDE.md） | 決定済み（V12-a と同じ値） |
| R1-b | RFC-0006 cell closure を武装するか | **武装する方針**（拒否だけを足す fence）。blame path の closure mode は実装済みの `for_blame_path`（consumer v2 前は各経路の歴史的な答えのまま、v2 後は全経路同じ）を採る | PRINCIPLES §10.4-2（court で正直な seat を slash させない）。武装は record の 4 要件（suite・node E2E・この review・跨ぎ drill）が揃ってから | 判断は決定。record は未作成（E2E・drill 未実施） |
| R1-c | RFC-0006 draw（R6-DRAW 6–7）と price（R6-PRICE 4） | 未決定 | V3 draw と segment pricing v2 は N3 第二錠（独立 bootstrap の検証）で 16000 に載らない。price の 4 値は release ハードウェアでの実測が要る | 未決定（16000 では不要） |
| R2-a | RFC-0007 bootstrap profile | **固定 seed による bootstrap**（ユーザー決定 2026-10-17）。`BootstrapExact`（`palw_beacon_bootstrap_v1`）は使わない、承認済み profile は空のまま | ユーザー決定 | 決定済み |
| R2-b | 固定 seed の値 | `seed = H("MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-SEED/NETWORK/V1", network_domain)`（network 名 + genesis hash）。testnet-12 の値は §3.2 | 運営者が seed を選べない（特定の checker bond を引く seed を選ばせない）こと、network ごとに異なること | 決定済み・独立検証待ち（関数として実装、policy には未設定） |
| R2-c | `PalwClaimChallengePolicyV2` の資源上限 6 値、`d_src`、source cap、source fee floor、`k/D/delay/retry`、Q・G・誤り配分、`N_max`、R7-ECON 8 の 2 問 | 未決定 | court 操作の実測（M-C）、beacon policy（R3）、外部 128-bit review が入力。`d_src` は式 `c_ab·T_w/T_ab − carrier` が決まっているが、入力が未決定の beacon・受付料の項 | 未決定 |
| R3 | RFC-0010 本番 Panel beacon | 未決定・未承認 | RFC-0007 §VI.2「k, D, delay, retry はこの文書では選ばない」、承認は外部の bias/withholding/P0-10 review が条件 | 未決定（外部 gate） |
| R4 | RFC-0016 measured record とクラス別 DA/court 負荷表 | 空のまま | 3 段階の導出（隔離 calibration devnet、4,320 DAA/run）と class ごとの実測が無い | 未決定（実測なし） |
| R5 | RFC-0006/0008 measured record | 空のまま | 実測（取得・検査・court 費用）と独立 review の digest が要る。`palw_settlement_once_v1` は open blocker（RFC08-2）付き | 未決定（実測なし） |
| R6 | TIR frame の費用 record | 空のまま | lane P1 の `frame_measure.rs` の実測が branch `values/p1-measure`（4e6df6ad4）に無い — harness だけで measurements.md も実測値も無い（P1 の commit 文: build は disk 30 GB 未満で停止）。既存の Huihui-2B 測定は frame が `Incomplete` | 未決定（実測なし） |
| R7 | PALWQ361 の logit scale と f32 参照窓 | この機材では決められない | 35B MoE の f32 長系列実行が要り、32 GiB では足りない。既存の実測も無い | 決定不能（この機材） |
| R8 | RFC-0016 本番値・クラス別負荷・§14 record | epoch 720 DAA、`ε = 0`、`k = 1`、V11 = 移行しない。ほかは未決定、record は空 | 3 段階導出・M-C・M-DA が無い | 一部決定（doc のみ） |
| R9 | tier κ・c・catalog edition・RFC-0019 U-3/U-4/U-6 | c = 300‰（§9、測定対象でない）。**κ は未決定**（分母を縮めないため登録 tier が正規 tier の発行を希釈できる反例、§6a.2） | 構造の判断が先 | 未決定 |
| R10 | RFC-0018 U-1〜U-8 | U-1 validator の順、U-2 通常分割、U-3 認証まで保留、U-6 不採用、U-7 承認推奨。**U-5 は未決定**（47/32 は f = 0.25 で正直な claim の 46% が認証失敗、§6a.3）。U-4・U-8 は実測待ち | U-2・U-3 は実装（GAP-C4・C3）が要る | 一部決定（doc のみ） |
| R11 | RFC-0008・RFC-0012 の名前付き parameter | min providers 2、`W_budget` 720、legacy evidence horizon 10,080、wallet margin 1,000 承認。ほかは未決定 | 実測・census | 一部決定（doc のみ） |

**16000 への影響（重要）:** r7-fence-16000 の一覧のうち、`palw_settlement_once_v1`（#24、`palw_bond_budget_v1` が前提）・`palw_model_tier_v1`（#32、`palw_model_bond_allocation_v1` が前提）・`palw_monetary_material_v1`（#34、`palw_material_income_v3` が前提）は、この lane の結果では前提が満たせない。V12-b は実測が無く、V12-a は値が決まっても validator が値以外の理由で拒否し、V12-c は値の対象外である。したがって、16000 の一覧からこの 3 つ（とそれに依存するもの: #25 `palw_native_liability_horizon_v1` は #24 に依存）を外すか、Stage 1 実測と各 validator の非値ブロッカーの解消を先に行う必要がある。

## 1. V12 — 前提 fence の policy

### 1.1 V12-a `palw_model_bond_allocation_v1`（ADR-0177 D3〜D5、RFC-0015 §8.5.5）

- **定義元:** ADR-0177（2026-10-10 改定、2026-10-14 決定、2026-10-16 の 7 日化）、`consensus/core/src/palw_bond_budget_v1/allocation_v3.rs`。
- **式:** `S_m(k) = C_miner_claim + C_verifier_claim`（bond ごとに重複を除き capital で頭打ち）、`S̄_m = (1/n) Σ_{k=e−n+1}^{e} S_m(k)`、`n = span / epoch`、`A_m = c_m · ⌊S̄_m^α⌋`、`P_m = A_m / ΣA`。block 機会 `≤ ⌊accrued · A_m/ΣA⌋`、coinbase `R_m = ⌊R_PALW · A_m/ΣA⌋`（1 回だけ適用）。
- **入力:** ユーザー決定（α = 1.5、7 日 = 5,040 DAA、miner + claim 拘束 verifier capital、P_m を block 機会と coinbase に）。`PALW_BOND_HOLD_DAA_PER_DAY_V1 = 720`（120 s slot の名目換算）。t12 の実測ペース中央値 184 s/DAA（[2026-10-19 観測](../../../measurements/2026-10-19-t12-passive-daa-blocks.md)）。
- **決定値:**

| 項 | 値 | 理由 | 比較した候補 |
|---|---|---|---|
| `epoch_daa` | 720 | 5,040 を整数日に割る最も粗い epoch で、両 mode の共通 hold（1 日）と同じ長さ。hold が明けて引き出した capital は次の snapshot で消える | 360（n = 14、履歴行が 2 倍、平均の更新は半日ごと）／1,680（n = 3、平均が最大 2.3 日遅れ、新モデルの立ち上がりが遅い）／5,040（n = 1、平均ではなく単一 snapshot になり 7 日平均の意味が消える） |
| `seasoning_epochs` | 1 | `validate` の下限。平均は capital × 時間に線形なので、seasoning を長くしても借りた capital の増幅は防げない（そもそも増幅しない、§1.1 監査）。長くすると正直な bond が遅れるだけ | 2〜7（正直な bond の待ちが 1 日ずつ増える。安全上の利得なし） |
| `max_models_per_bond` | 16 | genesis 8 モデル全部 + 登録 8。α > 1 では分割は重みを減らす（`x^α + y^α ≤ (x+y)^α`）ので、上限は資源の上限だけ。1 entry ≈ 72 B、16 で ≈ 1.2 kB（観測した block の tx bytes 最大 111,949 B に対して小さい） | 8（genesis 8 を全部覆うと登録モデルに割けない）／64（fold の 1 assignment あたりの仕事が 4 倍、利得なし） |

- **余裕:** 7 日平均は 5,040 DAA。t12 の 184 s/DAA では壁時計で約 10.7 日（遅い時計は期間を長くするだけで、短くしない）。
- **経済監査:**
  - *攻撃者（capital を一時的に借りて配分を取る）:* 1 epoch だけ Active な capital C は、7 epoch にわたり C/7 として数えられる。平均の総和は `C × 1 epoch` に等しく、増幅はない（テスト `p4_seven_epoch_average_never_amplifies_borrowed_capital...`）。さらに capital は bond として 1 日 hold され、claim に拘束された分だけが数えられる（待機 bond・登録料・Position は数えない）。必要資本は「欲しい配分 × 期間」そのもので、利得は配分の比例分（固定予算の分け前）を超えない。発覚時は claim の担保が slash される（配分は bond の Q/B/R/F 上限を上げない — D5）。
  - *攻撃者（モデルを分割・自作自演）:* α = 1.5 の超線形性で、同じ capital を複数モデルに割ると重みは減る。登録モデルは c_m = 0.3（§9、ADR-0179）。
  - *正直な参加者:* 新しい bond は assign から最大 2 epoch（seasoning）で数え始め、さらに 6 epoch で平均が満ちる（テストでは epoch 10 に参加したモデルが epoch 18 で既存モデルと等しい）。待ちは最大 8 epoch ≈ 8 × 720 DAA。必要資本は自分の bond だけ。
- **保証:** 配分の合計は予算を超えない（`Σ ⌊R·A_m/ΣA⌋ ≤ R`）。借りた capital は capital × 時間を超えて数えられない。分割・合流で得をしない（α ≥ 1）。
- **保証しないこと:** 新モデルが α = 1.5 で飢えないこと（ユーザーの有効化前条件、未評価）。閉鎖モデルの実効検出確率（RFC-0006 ALL-MODEL 6、ADR-0177 D7）。bond-without-checking を含む verifier 参加・責任・hold・報酬の規則（ユーザーの有効化前条件、未定義）。ADR-0177 D6 の open/closed 評価（validator が名前で拒否）。
- **依存:** `palw_bond_budget_v1`（同じ高さ以下、D5）。`palw_model_tier_v1`（#32）はこの fence が前提。RFC-0016 V+ の block rights は `P_m` を読む。
- **コード:** `palw_model_allocation_policy_decided_v1()`（値は既存の `decided_unapproved_v3()` と一致。こちらは「決定済み・独立検証待ち」の印付き）。

### 1.2 V12-b `palw_bond_budget_v1`（ADR-0176、V3 hold policy）

- **定義元:** ADR-0176（2026-10-15/16 改定）、[cap 導出と測定計画](../../../design/palw/economics/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md)、`palw_bond_budget_v1.rs` / `hold.rs`。
- **決定した構造（コードの validator がすでに要求しているもの、またはユーザー決定）:** `version = 3`、`hold = {daa_per_day 720, panel_one_days 1, panel_zero_days 1}`、`window_daa = 720`、`round_rights = ExecutionCap{…}`（fee-only Round は `B_max` に数えない — 2026-10-11 判断、validator が要求）、`weight_value = Unknown`（Final weight の価値は未知。`F_max` だけで有界化）。
- **決められない項と理由:**

| 項 | 式・拘束 | 足りない入力 |
|---|---|---|
| 日あたりの `q, b, r, f`（`claims_per_unit` など） | 「正直な参照 miner がほぼ上限まで使い（TooLow でも TooHigh でもない）、偽造は同じ上限に当たる」— Stage 1 → Stage 2 の sweep で `Feasible` 領域 | Stage 1（Qwen2.5-1.5B A16、最小 producer bond、≥ 2,160 DAA の実ノード走行）。隔離ネットワークも参照 miner も無い。既存記録 `docs/evidence/palw/g14-measure-2026-10-18/rfc0016/summary.json` も NOT RUN |
| `burst_window_daa`・`burst_factor_permille` | DAG 並列度と提出パターンの実測 | 同上（p95 TBD） |
| `concurrent_liability_permille` | `λ·a·H` + 余裕 | λ（日あたり受理数）と H（実際の hold）が Stage 1 の出力 |
| `liability_hold_daa`（`H_L`） | claim の責任期間（challenge + court + grace）を覆う | X3 の hard deadline（V10、別 lane）。`hard = accepted + 5,520 + Δ` の Δ が未実測 |
| `export_cap_permille` | 攻撃者の正味損 `0.51·K`（報告者 49% を差し引いた回収）> 輸出額 `e·K` + 節約計算 `S` ⇒ `e < 0.51 − S/K` | `S/K`（節約した計算費用と予約 K の比）が Stage 1 の出力。構造上の上限 510 は等号で、PRINCIPLES §10.4-1 の「上回る」を満たす余裕が無いので採らない |

- **経済監査（構造だけの範囲）:** hold を両 mode 1 日に揃えたことで、速い偽造と正直な計算は同じ日上限に当たる（§10.1）。mode を替えても容量は戻らない（長期・日・burst の 3 view が同じ ledger）。日上限の値そのものの監査は Stage 1 の後でないとできない。
- **結論:** 値は**未決定**。「仮値で合格させない」に従い、コードには入れない。`unapproved_probe_v1` / `hold_baseline_unapproved_v1`（1 単位の率）は提案ではなく、引き続き武装不可。

### 1.3 V12-c `palw_material_income_v3`

- `Params::palw_material_income_v3` は `Option<ForkActivation>` で、policy の値を持たない。validator の拒否理由は「signed material/TTL と全 consumer lifecycle の検証が未了」で、値ではない。前提の `palw_reporter_income_v2` も「monetary schema v2 と legacy cash の移行が休眠」で拒否される。
- この lane で決める値は無い。`palw_monetary_material_v1`（#34）はこの fence を前提とするため、16000 には載らない。

## 2. R1 — RFC-0006

### 2.1 配分曲線

ALL-MODEL 5 の「Allocation curve: USER DECISION REQUIRED（暫定 α = 1.5、2026-10-12 の `S_m·μ(S_m)` 案は未決）」は、2026-10-14 のユーザー決定（`PALW_ALLOCATION_ALPHA_HALVES_DECIDED_V1 = 3`、`validate_v3` が α ≠ 1.5 を拒否）と CLAUDE.md の `A_m = c_m · S̄_m^1.5` で閉じている。μ 案は採らない。値は V12-a と同じ。

### 2.2 cell closure（`palw_cell_coverage_closure_v1`）を武装するか

- **判断:** 武装する方針。理由: この fence は「被覆できない class の新規登録・新規 claim を拒否し、fence 後に受理した claim の court close を binding だけで有罪にする」拒否側の規則で、Panel の false-valid 経路を閉じる（REQ-CELL-1/1b/2）。fence 前に受理した claim は旧規則のまま（REQ-CELL-3）。
- **open question（blame path の closure mode）の決定:** 実装済みの `PalwCellClosureModeV1::for_blame_path` を採る。consumer v2（`palw_memory_routing_court_v2`）が claim を治める場合は全経路が同じ `MemoryRoutingV2`、そうでなければ各経路は歴史的な答え（round 4 で closure を使っていた経路は `Legacy`、`ExecutorRefuted` と terminal carriage は `Off`）を保つ。
  - 理由: claim を受理したときに適用された規則と、後でその claim の blame を決める規則を一致させる。後から厳しい mode を Panel の false-valid 経路にだけ遡って適用すると、受理時の規則では正しく署名した seat を有罪にしうる（PRINCIPLES §10.4-2 に反する）。
  - 16000 では `palw_memory_routing_court_v2` に measured record が無く（R5、`PALW_ROUTE_COST_*_US_V2` も `None`）、consumer v2 は治めない見込みなので、実際の mode は `Legacy` / `Off`。
- **正直な参加者への影響:** 被覆できない class では新規 claim が拒否され、その class の正直な miner の新規収入は止まる（非 fraud、slash なし）。genesis 8 のうちどの class が拒否されるかは、record 要件 2（node E2E）で確かめる必要がある。この lane では確認していない。
- **武装の条件:** `PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1` の 4 件。3 件目（この review）は本節を evidence にできる。1・2・4 件目（suite、node E2E、出荷 binary の跨ぎ drill）は未実施。record list は pin していない。

### 2.3 draw（R6-DRAW 6–7）と price（R6-PRICE 4）

- V3 draw・segment pricing v2・probabilistic cells は N3 の第二錠（`PALW_N3_GATED_REWARD_FENCES_V1`）で、独立 bootstrap が検証されるまで休眠。16000 には載らない。
- R6-PRICE の 4 値（最悪 cell の公開 material bytes、最悪 cell の fresh-verifier 時間、shard ごとの court 容量、non-seat watcher の報酬）は「release ハードウェアでの実測が要る」と RFC が明記。実測は無い。**未決定。**
- R6-DRAW の retry 上限・strata 数・class seat と outsider・retained population 上限、protocol-107 の transport 上限: 式が無く、V3 draw 自体が N3 で止まるため、16000 では決める必要が無い。**未決定。**

## 3. R2 — RFC-0007

### 3.1 bootstrap profile

- ユーザー決定（2026-10-17）: 最初の承認済み paid Final ができるまでは、genesis・年次フォークで固定した seed で checker を抽選し、以後は通常の beacon（v6 の seal-reveal）。承認済み paid claim を beacon source に含める。起動期間の claim は抽選が事前に読めるため、EXEC credit を与えない（`palw_exec_v2_selected_approval_v1` の credit 判定）。
- したがって `PalwBeaconBootstrapPolicyV1`（BootstrapExact、kernel inner 73）は使わない。`approved_bootstrap_entropy_profiles_v1()` は空のまま、`palw_beacon_bootstrap_v1` は武装しない。

### 3.2 固定 seed の値

- **式:** `seed = object_id("MISAKA/PALW/PAID-CLAIM-BOOTSTRAP-SEED/NETWORK/V1", network_domain)`、`network_domain = Params::palw_rfc0608_network_domain_v1()`（network 名と genesis hash から）。
- **testnet-12 の値:** `a208a453de3cf93f8433020101a15d1f94c8c2d79f91827803f515f8752cacdf112e2fdf306a62b70f117ec70427646f461707d26995b6de4cbcb5497ba54fc0`（`palw_t12_shipped_params()` から計算、§8 のテスト出力）。genesis hash か network 名が変われば値も変わる。
- **選んだ理由:** どんな固定 seed も公開され予測可能なので、安全性は seed の値に依らない。違いは「誰が選んだか」だけで、運営者が任意に選べる seed は、自分の checker bond が引かれる seed を探す余地を残す。network から決定的に導けば、その余地が無い。また claim ごとの抽選 `paid_claim_bootstrap_seed_v2(seed, context, statement)` は context（chain genesis を含む）にも束縛されるので、network をまたいだ再利用もできない。
- **経済監査:**
  - *攻撃者:* bootstrap 期間は抽選が予測できるので、producer は statement（job nonce など）を変えてオフラインで試行し、共謀 checker が引かれる抽選を選べる。checker の重みのうち共謀分を q、席数を m とすると、1 回で全席を取る確率は q^m だが、試行は安価なので、bootstrap 期間の抽選には選出の独立性が**まったく無い**ものとして扱う。得られるもの: その claim の承認（EXEC credit なし）と、payer がいればその fee。1 件の承認で bootstrap 窓が閉じる。
  - *抑止:* その claim は claim 単位の court（Gate C、G14: Panel 外の公開 bonded verifier 1 人が公開 material で有罪にできる）と bond slash の対象のまま。payer の損失の上限は fee で、producer の担保が slash されれば、PRINCIPLES §10.4-1 の条件（slash 損 > 利得）は担保の大きさで決まる。
  - *正直な参加者:* bootstrap 窓は最初の承認済み paid Final で閉じるので、待ちは 1 件分。EXEC credit が無いことが正直な bootstrap producer の唯一の損。
- **保証:** seed は 0 でなく、network ごとに異なり、誰も選べない。
- **保証しないこと:** bootstrap 期間の抽選の偏りの無さ（設計上ゼロ）。窓を閉じる最初の承認済み Final が共謀で作られた場合、その claim が以後の beacon source に入る。seal-reveal v6 の no-grinding 論証（`collect_paid_seal_reveal_seed_v6` の doc）が、1 つの source が攻撃者のものでも成り立つかは、独立検証で確かめること（**反証を試みるべき点**）。
- **コード:** `palw_paid_claim_bootstrap_seed_decided_v1(network_domain)` と `Params::palw_paid_claim_bootstrap_seed_decided_v1()`。`PalwClaimChallengePolicyV2` には入れていない（fence の policy を組むのは Lead）。

### 3.3 未決定の値

| 値 | 定義元 | 式・拘束 | 足りない入力 |
|---|---|---|---|
| `maximum_classes / jobs / live_claims / enrollments_per_job / stream_blocks / rounds` | R7-SUBJECT 6 | 不変条件（非 0、rounds ≤ 64 ほか）。生きている claim の上限は、全部が不正で全部検出された場合にも court 窓内で裁けること（court cap 42 session、検出 90〜99%） | court 操作ごとの時間（M-C）と court session の所要 DAA。`maximum_rounds` は NonInteractive の初期契約なら 1 で足りるが、反復 audit round で検出率を上げる設計（`1 − (1−p)^r`）との関係が未整理なので決めない |
| `lifetime_review_scope`・`source_independence_evidence`・`economic_evidence` | R7-SUBJECT 6 | 外部 review の digest | 外部 review（値ではない） |
| `d_src` | R7-SOURCE 6、econ B12 | `d* = c_ab·T_w/T_ab − carrier`（parity。これより高い deposit は何も買わない） | `T_w = anchor + 2W + 1`、`T_ab`、`c_ab = f_adm + D·β_d` は未決定の beacon（R3）と受付料の項。28.6 BILI は interim の項での値で、採らない |
| producer/global の同時 source 上限、source fee floor | R7-SOURCE 6 | source-Final slack ≥ challenge + court + grace（不変条件） | 実測と beacon |
| `work_count_k`・`settlement_depth_D`・`anchor_delay_slots`・`retry_limit` | §VI.2、R7-SOURCE 6 | 「この文書では選ばない」。bias の上限は review | §VI.8 item 2 の分析（外部） |
| Q・G・retry/subject・staged-beacon ceiling・誤り配分 | R7-RANDOM 7 | 実効 128-bit の累積勘定 | 外部 review |
| `N_max` | R7-ECON 5 | seal 窓あたりの sampled attempt 上限 | 被害（遅延の価値 ν）の実測 |
| deferred 解放の Panel/buyback 分割、unavailable beacon の扱い | R7-ECON 8 | — | `palw_model_verification_gate_v1` の判断。この gate はモデル・epoch ごとの attestation で配分補助を止めるもので、PRINCIPLES §9（モデル単位の検証は不採用）との関係を Lead が確かめるべき。この lane では値を決めず、武装も提案しない |

## 4. R3 — RFC-0010 本番 Panel beacon

- `approved_panel_beacon_policies_v1()` は空。承認は RFC-0007 §VI.8 item 2（reorg・withholding・last-mover・candidate grinding の bias）と RFC-0010 P0-10 の外部 review が条件で、`k, D, delay, retry` は RFC が「選ばない」と明記している。
- 参考の上限（決定ではない）: 最後の寄与者が withhold で veto できる場合、攻撃者が選べる出力は高々 `retry_limit + 1` 通りで、特定の席を取る確率は最大で `retry_limit + 1` 倍になる。`D` は reorg で source を差し替える再抽選を防ぐため、t12 の観測で finality が封印されるまでの深さ（静穏時 7〜10 h）を下回らないことが条件になる。どちらも外部 review の入力で、ここでは値を決めない。
- `palw_permissionless_panel_v1` はほかにも「R-core+・panel economy・audit_2026_09_23・RFC-0014 の gate」で拒否される。**未決定（外部 gate）。**

## 5. R4〜R6 — measured record

| record | 置き場 | 必要な実測 | 現状 |
|---|---|---|---|
| RFC-0016 values | `PALW_RFC0016_MEASURED_RECORDS_V1` | §3.6/§8.6 の 3 段階（正直な最小容量 → sweep → 敵対・境界）。隔離 calibration devnet（`palw_probabilistic_constraints_v1`・bond budget・allocation を武装）で 4,320 DAA/run | 実行環境が無い（`g14-measure-2026-10-18/rfc0016/summary.json`: NOT RUN）。`tilt_k = 1000` だけユーザー決定 |
| RFC-0016 class load | `PALW_RFC0016_CLAIM_LOADS_V1` | class ごとの `da_bytes_per_claim`・`court_units_per_claim`（出荷 binary） | UNMEASURED（`claim-loads.json`）。court を要する class は producer の trace 保持を 4W + 開示窓以上にするユーザー決定（2026-10-17）があり、保持 bytes/DAA の計上はその期間で行うこと |
| RFC-0006/0008 | `PALW_RFC0608_MEASURED_RECORDS_V1` | fence ごとの policy root と、取得・検査・court 費用の実測、独立 review | 実測なし。`palw_settlement_once_v1` は open blocker（RFC08-2）付きで record があっても武装できない |
| TIR frame | `PALW_TIR_FRAME_COST_RECORDS_V1` | `frame_measure.rs`（lane P1）の実モデル測定 | P1 の `values/p1-measure`（4e6df6ad4）は harness だけで、実測値は無い。既存の Huihui-2B 測定は `frame_upper*` が `Incomplete`（`NativeWitnessCeiling required 2,382,851 > native 2,250,000`）で record の入力にならない |

いずれも「実測からだけ埋める」規則に従い空のまま。record list の pin はこの lane の担当外でもある。

## 6. R7 — PALWQ361 の logit scale と f32 参照窓

- 35B MoE（Qwen3.6-35B-A3B）を f32 で長系列実行する必要がある。f32 の重みだけで約 35 × 10^9 × 4 B ≈ 140 GB で、この 32 GiB の機材では実行できない。
- 既存の実測（`qwen36-window-compare` の出力、`genesis10_pipeline.py` の `fit_plan` に入れる窓）はリポジトリに無い。
- **この機材では決定不能。** fit guard は窓と scale が無いと拒否する（`genesis10_pipeline.py`）ので、PALWQ361 の変換はそのまま止まる。

## 6a. R8〜R11（2026-10-22 追加分、558619700）

### 6a.1 R8 — RFC-0016 の本番値・クラス別負荷・§14 arming record

| 値 | 決定 | 理由・足りない入力 |
|---|---|---|
| V1 V+ 採用、`k = 1`（tilt off） | 決定済み（ユーザー D-V1・V2） | — |
| V5 epoch の長さ | **720 DAA**（決定済み・独立検証待ち） | rights の epoch を配分の epoch（V12-a、720）と同じにする。違う長さだと 1 つの rights epoch の中で `P_m` が 2 回変わり、`⌊(N_exp−Ent)·A_m/ΣA⌋` の発行と再発行の基準がずれる。7 日平均 5,040 DAA を割り切る |
| V3 `ε` | **0**（ADR-0179） | tier fence の下では entrant が存在しない（ADR-0179「`ε = 0`、κ が唯一の上限」） |
| V11 coinbase 経路の Panel 移行 | **しない**（ユーザー決定、`PALW_RFC0016_COINBASE_PANEL_MIGRATION_V1 = false`） | — |
| V2 σ・φ・pace、V3 `S_entry`・1 モデル上限、V4、V6、V7、V8、V9、V10 | **未決定** | §3.6 の 3 段階導出（隔離 calibration devnet、4,320 DAA/run、参照 miner）が無い。V4 は V12-b の Stage 1 と同じ入力 |
| クラス別負荷 `PALW_RFC0016_CLAIM_LOADS_V1` | **空** | M-C・M-DA の実測が無い（P1 の `values/p1-measure` 4e6df6ad4 は harness だけ）。court を要する class の保持は 4W + 開示窓以上（ユーザー決定 2026-10-17）で計上すること |
| §14 arming record `PALW_RFC0016_ARMING_RECORDS_V1` | **空** | §14 の各 conjunct（§12 の試験の独立 review 付き PASS、実ノード E2E、ADR-0177 の二段評価の再計算）は未実施。値の record（`PALW_RFC0016_MEASURED_RECORDS_V1`）も空 |

`PalwRfc0016ValuesV1` は 18 項を 1 組で持つため、上の決定済みの項だけをコードに入れることはしない（残りが placeholder になる）。

### 6a.2 R9 — モデル tier（κ、c、catalog edition、RFC-0019 U-3/U-4/U-6）

- **c = 300‰:** ユーザー決定（PRINCIPLES §9、ADR-0179）。「c の実測 record」は §9 と両立しない（c は審査・測定で動かない固定値）。record の 1 件目の要件（`…the user's approval of c and κ…, with their measured record`）のうち c については「ユーザー決定 2026-10-15、測定なし（§9 により測定の対象でない）」を evidence とすべき。モデル単位の検証は提案しない。
- **κ: 未決定 — 先に解くべき反例がある。** `roll_epoch_v3_tiered` は登録 tier の重みを `κ·ΣA` に縮めるが、分母の `ΣA` は縮める前の値のまま（`palw_model_tier_cap_factor_ppm_v1` の doc、ADR-0179 5「縮めた分は発行しない、再配分もしない」）。このため:
  - 例: 正規 tier の `A = 1`、登録 tier の `A = 100`（大きな capital を登録モデルの claim に拘束）→ `ΣA = 101`。登録 tier は κ に縮むが、正規 tier の取り分は `1/101 ≈ 1%` に下がる。残りは発行されない。
  - つまり登録モデルに capital を積む者は、自分の取り分（≤ κ）に関係なく、**正規モデルの miner の発行を希釈できる**。費用は capital の拘束（と claim の担保）だけで、登録モデルが非公開でも Gate C で credit が止まるだけで、`S_m`（claim に拘束された capital）は数えられる。κ の値はこの希釈を止めない。
  - 分母を縮めた後の重みにすれば、正規 tier の取り分は `≥ 1 − κ` で下から抑えられる。ただしこれは ADR-0179 の構造の変更で、§11 の手続き（RFC + ADR）が要る。値ではないので、この lane では変更しない。**Lead・ユーザーの判断事項。**
  - κ の候補: 300‰（設計値）、230‰（`κ/(1−κ) = 0.3`、縮めた分母のもとで「登録 tier 全体 ≤ 正規 tier 全体の 0.3 倍」になる値）。どちらを選ぶかは分母の扱いで意味が変わるため、上の判断の後で決める。
- **最新 catalog edition:** genesis 8 の edition 0 の root は、実変換（PALWART3 → HF Misakachain）が NOT_RUN のため確定していない。record は edition と root を名指すので、**未決定**。
- **RFC-0019 U-3（manifest の署名者集合と k-of-n、独立 builder 数）:** 署名者の人選は値ではなく、ユーザーの判断事項。**未決定。**
- **U-4（edition の中身と H）:** モデル 8 個はユーザー決定（2026-10-15）。root・kernel version・plan id は変換の後。**未決定。**
- **U-6（tier fence の有効化）:** Lead・ユーザー。record の要件 2〜8（試験 PASS、R-1〜R-8 drill、署名済み manifest、readiness 報告）は未実施。

### 6a.3 R10 — RFC-0018 U-1〜U-8

| U | 決定 | 理由・経済監査 |
|---|---|---|
| U-1 有効化と順序 | **validator の前提順のとおり**（kernel route → Gate C → tier、r7-fence-16000 の #5・#29・#32）。rule E v2 と DNS 退役は rule E（ADR-0178）が未決定なので 16000 に入れない | 順序は各 validator が要求している。`palw_model_tier_v1` は V12-a/b の前提が満たせないので、現状 16000 には載らない（§0） |
| U-2 発行由来 leg の保留後の支払い先 | **通常の Final と同じ分割**（Panel share・buyback・vesting）。producer への 1 leg にしない | 認証を経た claim だけ Panel seat の取り分が消えると、seat が検証する誘因が下がり、producer は認証待ちを通すだけで通常より多く受け取る。認証の有無で受取人が変わらないことが中立。**実装が要る（GAP-C4）** — 値ではないので、この lane ではコードを変えていない。R7-ECON 8 の同じ問いにも同じ答えを当てる |
| U-3 free-prompt の認証前 spend の coinbase carve | **認証まで保留、認証で支払い、失敗で名付けない（発行しない）** | 未認証 claim は決済を進めない（CLAUDE.md、ADR-0179 6）。今の実装は weight だけ止めて carve は払うので、未認証の仕事に発行が出る。正直な producer の損は待ち（承認期限 2 日 + 抽選期限 3 日の設計値）だけ。**実装が要る（GAP-C3）** |
| U-4 DAG 幅 schedule | **未決定** | block 伝播の実測が要る。`palw_dag_width_schedule_v1` は probe しか無く、16000 から除外済み |
| U-5 `m`/`k` | **未決定 — 設計値 47/32 は liveness の反例がある** | 下の表。誤認証確率 `P(Bin(m,f) ≥ k)` と、攻撃者が承認を出さない（withhold）ときの正直な claim の認証確率 `P(Bin(m,(1−f)·a) ≥ k)`（a = 正直な検証者の応答率 0.9 と仮定）。47/32 では f = 0.25 で正直な claim の 46% が認証に失敗し、その発行由来 leg は焼却される（§7.4）。f = 0.25 の攻撃者は費用なしで正直な発行の約半分を止められる。前提 f（正直な割合の明示的前提）はユーザーが置く値なので、f を決めてから `k` を選ぶ。認証失敗後に再抽選があるか（あれば liveness は改善）も確かめること |
| U-6 veto | **採らない（v = 0）** | 抽選された 1 人の否定で正直な claim を止められるので、U-5 の liveness 問題を悪化させる |
| U-7 credit なし同士の tie を GHOSTDAG 順に | **承認を推奨**（規則の承認なので最終は Lead・ユーザー） | 既存の合意順序で決定的。hash 順など producer が grind できる順序より安全 |
| U-8 公開 check 経路に固定する class と計測記録 | **未決定** | class ごとの実測が要る |

U-5 の計算（`python3 -I`、二項分布の上側確率、a = 0.9）:

| m/k | f = 0.10 誤認証 / 正直 | f = 0.20 | f = 0.25 | f = 1/3 |
|---|---|---|---|---|
| 47/32（設計値） | 1.6e-21 / 0.989 | 1.3e-12 / 0.779 | 6.4e-10 / 0.535 | 1.2e-6 / 0.163 |
| 47/28 | 1.0e-16 / 1.000 | 3.2e-9 / 0.977 | 5.2e-7 / 0.904 | 2.0e-4 / 0.586 |
| 47/26 | 1.5e-14 / 1.000 | 9.6e-8 / 0.995 | 8.9e-6 / 0.971 | 1.6e-3 / 0.790 |
| 47/24 | 1.6e-12 / 1.000 | 2.1e-6 / 0.999 | 1.1e-4 / 0.994 | 9.0e-3 / 0.918 |
| 63/36 | 3.1e-20 / 1.000 | 9.9e-11 / 0.996 | 5.8e-8 / 0.968 | 8.9e-5 / 0.725 |
| 95/56 | 1.3e-31 / 1.000 | 1.0e-16 / 0.998 | 2.4e-12 / 0.969 | 2.8e-7 / 0.626 |

`k/m` は `f`（安全）と `(1−f)·a`（liveness）の間に置く必要がある。`m` を増やすと両方が改善するが、費用は認証 1 件あたり `m` 回の再実行で、network 全体の抽選上限（設計値 4,096/日）からは 1 日に `⌊4,096/m⌋` 件（47 で 87 件、95 で 43 件）しか認証できない。この上限と t12 の claim 率の比較も、決定の前に要る。

### 6a.4 R11 — RFC-0008・RFC-0012 の名前付き parameter

| parameter | 決定 | 理由 |
|---|---|---|
| RFC-0008 R8-LANE 3 `PalwMaterialSessionMinProviders` | **2**（決定済み・独立検証待ち。コードは既に構造下限 2） | 1 provider は material 保持の単一障害点。上げる根拠になる provider 不履行率の実測が無いので、構造下限で正直な session の成立を優先する。攻撃者は 2 provider を自分で用意できるので、これは可用性の下限であって独立性の保証ではない |
| R8-BUDGET 6 `W_budget` | **720 DAA**（= 両 mode 共通の 1 日 hold） | 2026-10-16 のユーザー決定で budget の window は 1 日 hold（V3）になり、単一 window（V2、280..436、drill 暫定 400）は validator が mechanics 専用として拒否する。条件 `L ≤ W_budget` は対象 profile ごとに確かめる |
| R8-BUDGET 4 `ExecutionCap` の資本比率 | 未決定 | V12-b と同じ Stage 1 の入力 |
| R8-IDENTITY 3 `L_auth`・履歴上限 | 未決定（v1 構造 profile 100,000 DAA / 100,000 行のまま） | release の memory・latency の実測が要る |
| R8-WORK 6 `ExecWorkReleaseResourceBounds` | 未決定 | 同上 |
| R8-WINDOW 6/7（Panel lifetime V2、monetary material policy） | 未決定 | review と実測。monetary material は `palw_material_income_v3` が前提で 16000 に載らない |
| R8-ECON 3 `capital_rate`・`C_upper`・`M_net_min` | 未決定 | class 別の実測費用 |
| RFC-0012 R12-SETTLE 6 `legacy_evidence_horizon_daa` | **10,080**（式: ≥ DNS の `evidence_window_blocks`） | 導出値。保持すべき証拠窓を下回らない最小値 |
| R12-SETTLE 6 `D`・`W`・集中度上限・`M_maturity` | 未決定（drill 暫定のまま） | `W` と集中度は census が要る。`M_maturity` 5,400 はユーザーが 2026-10-08 に「比較と攻撃試験なしで短縮も最終扱いもしない」とした。`D` は `D ≥ ⌈1000 / operator_cap⌉` に従う（cap 400 なら 3）ので cap と一緒に決める。RFC-0012（DNS 退役）は 16000 の一覧に無い |
| R12-OPS 4 release 観測上限、100k block latency 上限 | 未決定 | 実測 |
| R12-OPS 7 wallet の 1,000 DAA margin | **1,000 DAA を承認**（client 側の確認、consensus 外） | t12 の 184 s/DAA で約 51 時間。退役の予定を知ってから wallet が更新するまでに 2 日以上ある。consensus の値ではない |
| R12-MONEY split・fee schedule | 未決定 | RFC 自身が「ADR-0177 D4 の claim escrow 配分が未決なので推奨しない」とする |

## 7. 独立検証への依頼（反証を試みてほしい点）

1. V12-a: epoch 720・seasoning 1 で、assign → 引き出し → 再 assign の繰り返しや epoch 境界の操作で、capital × 時間を超えて平均に数えさせる経路が無いか。
2. V12-a: 16 entries の上限で、1 bond が 16 モデルに薄く割って新モデルの立ち上がりを妨げる（分母を膨らませる）経路が無いか（α > 1 では分割した側も損をするはず）。
3. R2-b: bootstrap 窓を閉じる最初の承認済み Final が共謀で作られたとき、以後の seal-reveal seed に偏りが残らないか。
4. R9: 登録 tier による正規 tier の希釈（§6a.2）が、`S_m` に数える capital の条件（claim 拘束、7 日平均、1 日 hold）で実際に安く実行できるか。
5. R10 U-5: 認証失敗後の再抽選の有無と、withhold した検証者への罰則の有無（あれば liveness の数字が変わる）。
6. R1-b: `for_blame_path` の歴史的な答えのうち、`Off` の経路で、fence 後の被覆できない class の不正が罰せられないまま残る場合が無いか（court close 側の REQ-CELL-2 で捕まるはず）。

## 8. テスト（実行した結果）

機材は冒頭のとおり（Mac13,1、32 GiB、10 cores）。`CARGO_TARGET_DIR` 共有、`nice -n 10`、`-j4`、`--release`、対象 crate だけ。

| コマンド | 結果 |
|---|---|
| `cargo test --release -p kaspa-consensus-core --lib -- palw_values_p4_v1`（初回） | 4 passed / **1 failed**: `p4_decided_allocation_policy_validates_and_carries_the_user_terms` が `allocation_v3.rs:103` の「0 で割る余り」で panic。`epoch_daa = 0` の V3 policy を `validate` が拒否せず panic していた（`validate_v3` の剰余が `validate` の 0 検査より先）。**既存の不具合**: 不正な policy の `Params` 検証が拒否ではなく panic になる |
| 修正（`validate_v3` で剰余の前に `epoch_daa == 0` を拒否）後、`… --lib -- palw_values_p4_v1 palw_bond_budget_v1 --nocapture` | **88 passed, 0 failed**（P4 の 5 件 + bond budget module の既存 83 件）。testnet-12 の seed を出力: `a208a453de3cf93f8433020101a15d1f94c8c2d79f91827803f515f8752cacdf112e2fdf306a62b70f117ec70427646f461707d26995b6de4cbcb5497ba54fc0` |
| `… --lib -- palw_kernel_route_selected_approval_tests palw_cell_coverage_closure palw_claim_challenge` | 7 passed, 0 failed（cell closure の REQ-CELL 7 件。selected approval は module 名が違い、この filter では選ばれなかった） |
| `… --lib -- selected_approval_tests palw_claim_challenge_fold` | 8 passed, 0 failed（bootstrap seed の per-claim 束縛と probe の `None` を含む） |
| `cargo test --release -p kaspa-consensus-core --test adr0177_allocation_v3`（30 日 → 7 日の訂正後、merge 前の tree） | 5 passed, 0 failed |
| 558619700 を含む `g14/integration-r7`（193c2c65b）を merge した後: `… --lib --test adr0177_allocation_v3 -- palw_values_p4_v1 palw_bond_budget_v1 selected_approval_tests palw_cell_coverage_closure the_ miner_ block_ rfc0017_gap_e1` | lib: **2,692 passed, 8 failed**。`the_` の filter が広すぎて 2,700 件ほどが選ばれた。P4・bond budget・cell closure・selected approval の 103 件はすべて ok。失敗した 8 件はこの lane の変更と無関係な module（`fork_id_v1` 2 件: 「`palw_tir_container_identity_v1` … this probe cannot set — add it here」、`palw_lifecycle_objects_v2` a2u: 「TirDesignatedEventV1 (tag 245) sits in exactly one row of PALW_A2_KIND_FENCE_TABLE_V1」left 0、`palw_court_model_scope_routes_v1` 2 件（`DemandExpired` と `RootNotCommitted` の不一致ほか）、`palw_ruling_fences_v1` X3、`palw_state_v2` claim_local_rewards / r5_f02_wire）。integration-r7 側の回帰と見られ、この lane では直していない（Lead へ報告） |
| merge 後に `… --test adr0177_allocation_v3` だけ | 5 passed, 0 failed |

P4 のテスト（`consensus/core/src/palw_values_p4_v1.rs`）の内容:

- 成功経路: 決定した allocation policy が `validate` を通る（n = 7、α = 1.5、epoch 720）。16 entries の assign が通る。7 epoch 平均の engine（`roll_epoch_v3`）で、1 epoch だけ借りた 700 は epoch 2〜8 に 100 ずつ、総和 700（増幅なし）。epoch 10 に参加したモデルは epoch 18 で既存モデルと等しい。履歴行は 1 モデル 7 行以下。testnet-12 の seed は 0 でなく決定的で、mainnet と異なり、`PalwClaimChallengePolicyV2::validate` を通る。
- 拒否経路: α = 1 / 2、30 日平均、verifier capital なし、block 機会なし、5,040 を割らない epoch（1,000）、epoch 0、seasoning 0、entries 0、average なし、17 entries の assign。`validate_palw_model_bond_allocation_v1` は budget なしで「requires palw_bond_budget_v1」、budget ありでも「cannot be armed」（非値ブロッカー）、不正な policy は「policy is invalid」で先に拒否。seed を入れた paid-claim fence は review evidence と record が無いので拒否。出荷 t12 preset は両 fence とも `None` のまま。

`consensus/core/tests/adr0177_allocation_v3.rs` は 2026-10-16 の 7 日化の前の 30 日を assert していた（`average_epochs() == Some(30)`、7 日を不正とする assert、epoch 7 を不正とする assert — 5,040 は 7 で割れるので今は正しい値）。ユーザー決定に合わせて 7 日に直し、epoch の不正例を 1,000 に、epoch 0 の拒否を追加した（緩めた assert は無い）。
