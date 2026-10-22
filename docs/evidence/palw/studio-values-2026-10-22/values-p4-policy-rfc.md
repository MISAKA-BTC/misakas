# Lane P4 — V12・R1〜R7 の値（2026-10-22）

- **状態:** 決定した値は「**決定済み、独立検証待ち**」（decided, awaiting independent verification）。fence の武装、arming record・measured record の pin、出荷 preset の activation 高さの変更は、どれも行っていない（Lead が独立検証のあとで行う）。`parameter-register.md` / `parameters.json` も編集していない。
- **根拠:** ユーザーの値決定の委任（2026-10-22、[作業指示 (2)](../../../design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md)）、[16000 の判断表](../../../design/palw/integration/g14-t12-16000-decisions-2026-10-22.md)、[rfcgap-a](../rfc-gaps-2026-10-22/a.md)、ADR-0176/0177、RFC-0006/0007/0008/0010/0016/0002、PRINCIPLES §8〜§10。
- **branch:** `values/p4-policy-rfc`（base `g14/integration-r7` 34d68d7e2）。コードは新しい module `consensus/core/src/palw_values_p4_v1.rs` だけ（どの preset からも読まれない）。
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
| R6 | TIR frame の費用 record | 空のまま | lane P1 の `frame_measure.rs` の実測が branch `values/p1-measure` に未公開（2026-10-22 時点で origin に無い）。既存の Huihui-2B 測定は frame が `Incomplete` | 未決定（実測なし） |
| R7 | PALWQ361 の logit scale と f32 参照窓 | この機材では決められない | 35B MoE の f32 長系列実行が要り、32 GiB では足りない。既存の実測も無い | 決定不能（この機材） |

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
| TIR frame | `PALW_TIR_FRAME_COST_RECORDS_V1` | `frame_measure.rs`（lane P1）の実モデル測定 | P1 の `values/p1-measure` は 2026-10-22 時点で origin に無い。既存の Huihui-2B 測定は `frame_upper*` が `Incomplete`（`NativeWitnessCeiling required 2,382,851 > native 2,250,000`）で record の入力にならない |

いずれも「実測からだけ埋める」規則に従い空のまま。record list の pin はこの lane の担当外でもある。

## 6. R7 — PALWQ361 の logit scale と f32 参照窓

- 35B MoE（Qwen3.6-35B-A3B）を f32 で長系列実行する必要がある。f32 の重みだけで約 35 × 10^9 × 4 B ≈ 140 GB で、この 32 GiB の機材では実行できない。
- 既存の実測（`qwen36-window-compare` の出力、`genesis10_pipeline.py` の `fit_plan` に入れる窓）はリポジトリに無い。
- **この機材では決定不能。** fit guard は窓と scale が無いと拒否する（`genesis10_pipeline.py`）ので、PALWQ361 の変換はそのまま止まる。

## 7. 独立検証への依頼（反証を試みてほしい点）

1. V12-a: epoch 720・seasoning 1 で、assign → 引き出し → 再 assign の繰り返しや epoch 境界の操作で、capital × 時間を超えて平均に数えさせる経路が無いか。
2. V12-a: 16 entries の上限で、1 bond が 16 モデルに薄く割って新モデルの立ち上がりを妨げる（分母を膨らませる）経路が無いか（α > 1 では分割した側も損をするはず）。
3. R2-b: bootstrap 窓を閉じる最初の承認済み Final が共謀で作られたとき、以後の seal-reveal seed に偏りが残らないか。
4. R1-b: `for_blame_path` の歴史的な答えのうち、`Off` の経路で、fence 後の被覆できない class の不正が罰せられないまま残る場合が無いか（court close 側の REQ-CELL-2 で捕まるはず）。

## 8. テスト（実行した結果）

機材は冒頭のとおり（Mac13,1、32 GiB、10 cores）。`CARGO_TARGET_DIR` 共有、`nice -n 10`、`-j4`、`--release`、対象 crate だけ。

| コマンド | 結果 |
|---|---|
| `cargo test --release -p kaspa-consensus-core --lib -- palw_values_p4_v1`（初回） | 4 passed / **1 failed**: `p4_decided_allocation_policy_validates_and_carries_the_user_terms` が `allocation_v3.rs:103` の「0 で割る余り」で panic。`epoch_daa = 0` の V3 policy を `validate` が拒否せず panic していた（`validate_v3` の剰余が `validate` の 0 検査より先）。**既存の不具合**: 不正な policy の `Params` 検証が拒否ではなく panic になる |
| 修正（`validate_v3` で剰余の前に `epoch_daa == 0` を拒否）後、`… --lib -- palw_values_p4_v1 palw_bond_budget_v1 --nocapture` | **88 passed, 0 failed**（P4 の 5 件 + bond budget module の既存 83 件）。testnet-12 の seed を出力: `a208a453de3cf93f8433020101a15d1f94c8c2d79f91827803f515f8752cacdf112e2fdf306a62b70f117ec70427646f461707d26995b6de4cbcb5497ba54fc0` |
| `… --lib -- palw_kernel_route_selected_approval_tests palw_cell_coverage_closure palw_claim_challenge` | 7 passed, 0 failed（cell closure の REQ-CELL 7 件。selected approval は module 名が違い、この filter では選ばれなかった） |
| `… --lib -- selected_approval_tests palw_claim_challenge_fold` | 8 passed, 0 failed（bootstrap seed の per-claim 束縛と probe の `None` を含む） |
| `cargo test --release -p kaspa-consensus-core --test adr0177_allocation_v3`（30 日 → 7 日の訂正後） | 下の追記を参照 |

P4 のテスト（`consensus/core/src/palw_values_p4_v1.rs`）の内容:

- 成功経路: 決定した allocation policy が `validate` を通る（n = 7、α = 1.5、epoch 720）。16 entries の assign が通る。7 epoch 平均の engine（`roll_epoch_v3`）で、1 epoch だけ借りた 700 は epoch 2〜8 に 100 ずつ、総和 700（増幅なし）。epoch 10 に参加したモデルは epoch 18 で既存モデルと等しい。履歴行は 1 モデル 7 行以下。testnet-12 の seed は 0 でなく決定的で、mainnet と異なり、`PalwClaimChallengePolicyV2::validate` を通る。
- 拒否経路: α = 1 / 2、30 日平均、verifier capital なし、block 機会なし、5,040 を割らない epoch（1,000）、epoch 0、seasoning 0、entries 0、average なし、17 entries の assign。`validate_palw_model_bond_allocation_v1` は budget なしで「requires palw_bond_budget_v1」、budget ありでも「cannot be armed」（非値ブロッカー）、不正な policy は「policy is invalid」で先に拒否。seed を入れた paid-claim fence は review evidence と record が無いので拒否。出荷 t12 preset は両 fence とも `None` のまま。

`consensus/core/tests/adr0177_allocation_v3.rs` は 2026-10-16 の 7 日化の前の 30 日を assert していた（`average_epochs() == Some(30)`、7 日を不正とする assert、epoch 7 を不正とする assert — 5,040 は 7 で割れるので今は正しい値）。ユーザー決定に合わせて 7 日に直し、epoch の不正例を 1,000 に、epoch 0 の拒否を追加した（緩めた assert は無い）。
