# G14 — class 単位の検出率 policy(2026-10-17、branch `integ/g14-detection-policy`)

外部監査(`g14-external-audit-2026-10-17/rfc1_20_audit_ja.md` §5、§7 P1 の 4 項、§8 B)への処置。
ユーザー前提(10-17):不正 claim の見逃しを 1〜10% 許容する。検出率 90〜99% を、**明示した不正集合に対して**満たす。

**休眠。** fence `palw_detection_policy_v1` は全 preset で `None`。fingerprint には Some-only で入る(各 policy の id を高さの横に書く)。
`Some(never())` は `None` に畳まれる。武装すると `validate_palw_detection_policy_v1` が拒否する。
fence より前は何も読まない。legacy の draw(`palw_fp_interval_draw_v1`、`k = 4`、domain、golden vector `[645, 20, 372, 227]`)は変えていない。

計算の正しさの暗号学的証明は使わない(CLAUDE.md、PRINCIPLES §8)。
ここにあるのは、抜き取り再実行、決定論的な再計算、経済的抑止、court だけである。
hash と既存の challenge stream(`misaka_palw_challenge::seed` の rejection sampling と Floyd の非復元抽出)は、乱数と同一性のためだけに使う。

実装: `consensus/core/src/palw_detection_policy_v1.rs`。
試験: `consensus/core/tests/g14_detection_policy.rs`、`consensus/core/tests/g14_detection_corpus.rs`。
node 側: `kaspad/src/palw_fp_seat.rs`(`palw_fp_seat_draw_under_policy_v2`)、`kaspad/src/palw_panel.rs`(seat の draw 切替、opening 保持上限、funnel)、`kaspad/src/palw_economics.rs`(funnel の計数)。

## 1. policy の値(`PalwDetectionPolicyV1`)

1 つの class と kernel/TIR 版(`class_id`、`kernel_version`、`route`)に結び付く consensus 値である。`id()` は全 field を domain 付きで hash する。

| field | 意味 |
|---|---|
| `method` | `IntervalReplay`(seat の区間再実行、ADR-0077 D8 / ADR-0082 D9)または `FullReexecution`(claim 全体の再実行と root 比較) |
| `n_rule` | N の導出規則。chain の数だけを使う(§2) |
| `n_max` | claim が持てる N の上限。上限を超える claim は拒否する |
| `fault_model` | `D_min(N) = max(d_min_abs, ⌈N·d_min_ppm/10^6⌉) ∧ N`。ε を約束する不正集合は「検出可能な不正区間が D_min 以上の claim」 |
| `epsilon` | 許す見逃し(有理数、分母 ≤ 10^9) |
| `k_max` | 1 round で引ける区間数の上限 |
| `shadow_rate_ppm` | policy による無作為の全再実行の割合(§4.4) |
| `economics` | 1 区間の正直な計算費用 c、class の 1 区間あたり担保、検査被覆率 q、徴収率、margin、固定利得 G の有無 |
| `resources` | opening の bytes、claim あたりの取得 bytes・再実行 unit の上限、検査者の処理速度、取得遅延、同時 claim 数、opening 保持上限、court/DA が 1 claim に強制できる opening 数と bytes |
| `deadlines` | 検査期限、round の間隔、round 数 |
| `localisation` | `IntervalLeafCourt`(seat の Fault → leaf court の bisection)または `RootDescentElementCourt`(seg_detect の降下 → element court) |

## 2. N は価格に照合した chain の数から導く

- `DecodeCalls { checkpoint_interval }`: `N = max(1, ⌈(decode − 1)/ci⌉)`。base0 の call 幾何(`Base0FpIntervalGeometryV1::from_chain_facts_v1`)と同じ式。
- `HeldPositions { width }`: `N = max(1, ⌈(prompt + decode − 1)/width⌉)`。held 幾何(`from_chain_facts_held_v1`)と同じ式。
- `PricedLeaves { leaves_per_interval }`: `N = ⌈work_leaves / leaves_per_interval⌉`。

claim が自分の仕事に値段を付けている場合(`work_leaves ≠ 0`)、2 つの数を運ぶ context の価格が `work_leaves` と一致しなければ拒否する(`NotAtTheClaimsPrice`)。
これは `palw_fp_seat_draw_at_the_claims_price_v1` の防御を policy 側にも持たせたものである。N を producer や配られた material が申告する余地はない。
seat は、自分の backend の区間数が policy の N と一致しない場合、どちらの空間でも引かずに duty を断る(`GeometryMismatch`、funnel の loss に記録する)。

## 3. k_min(整数演算)

1 人の正直な検査者が、検査した claim の K 個の異なる区間を一様に引くとき、

```text
miss(N, D, K) = C(N−D, K) / C(N, K)      (K > N−D なら 0)
```

target は抜き取り検査だけで課す: `q · (1 − miss(N, D_min, K)) ≥ 1 − ε`。q は検査被覆率の下限である(G14 の前提では、正直な主体が全 claim を検査するので 10^6 ppm)。
shadow は ε に数えない。

`palw_detection_miss_bounds_v1` は miss を 10^21 の固定小数で `[lo, hi]` として持つ(各因子を lo は切り捨て、hi は切り上げ)。
比較は `hi·den ≤ num·S` なら成立、`lo·den > num·S` なら不成立とする。どちらでもない(固定小数で決められない厳密な同値)場合は不成立と扱う。
したがって `palw_detection_k_min_v1` の答えは、厳密な最小値か、その 1 つ上であり、下回ることはない。
N ≤ 40 の全 (N, D) と 4 つの ε で全列挙と照合した。3,280 件中 21 件が同値で 1 つ上になった。
監査の N = 1000 表(D = 1000/500/100/10/1 の 90%・99% の最小 k、k = 4 の検出率)も整数演算で再現した。

## 4. 疎な不正への経済的抑止

### 4.1 利得が省いた区間に比例する場合: 拘束は D = N

D 区間を省いた利得を `D·c`、有罪時に徴収する担保を S、検出確率を `p(D) = q·(1 − miss(N, D, K))` とする。
すべての D で期待値上の不利を求めると、条件は `S ≥ max_D D·c / p(D)` になる。

`p(D+1) − p(D) = q·miss(D)·K/(N−D)` で、隣り合う増分の比は `(N−D−K)/(N−D−1) ≤ 1` である。
よって p は凹で `p(0) = 0`。`p(D)/D` は非増加で、`D/p(D)` の最大は `D = N`(`p = q`)にある。

```text
S_required = margin · N · c / (q_eff · q_collect)
```

claim 全体の正直な費用を被覆率で割った額が、K によらず、すべての D の省略を抑止する。
K が効くのは検出率 ε の側であり、この上限ではない。

監査の依頼文にある `S ≥ C_claim·(N/k)·margin` は D = 1 の値である。利得が D に比例するなら、それは拘束にならない(`d1_proportional_sompi` として記録だけする)。
N ≤ 40 の全 (N, K, D) で `D/p(D) ≤ N` と凹性を全列挙で確かめた(`the_proportional_bound_binds_at_d_equals_n_for_every_k`)。
policy の要求額では、すべての D で `D·c ≤ p(D)·S` が厳密な有理数で成り立ち、1 sompi 少なければ受理しないことも確かめた。

報酬 R が検出より前に払われている場合(監査 §4: Final 後に liability 期間が続く経路)でも、上の不等式は S だけで成り立つ。R の没収は当てにしていない。
正直な採掘が赤字の場合(R < N·c)は、比較相手が「採掘しない」になる。それでも `(1−p)R − (N−D)c − pS ≤ R − N·c < 0` となり、偽造は不利である。

### 4.2 利得が省略量に比例しない場合(選択出力)

1 区間を偽るだけで固定の利得 G が得られる場合(例: 答えの選択を偽る)、拘束は D = 1 になる:

```text
S_required(G) = margin · G · N / (K_tot · q_eff · q_collect)
```

policy が `fixed_gain_sompi = Some(G)` を宣言した場合だけ、この上限を要求する。
宣言しない場合、policy はこの種の不正を抜き取りで覆うとは主張しない。その覆いは次の既存経路である。

- **legacy route(BASE-0 A16、PALWQ361)**: `--palw-challenge` の watchdog。免許された全 claim を再実行し、再現できなければ dispute を開く(`kaspad/src/palw_panel.rs`、config `challenge`)。
  見つかった不一致は leaf court が決定論的に裁く。
- **kernel / TIR route**: `misaka_palw_kernel::seg_detect`(claim 全体を再実行して segment root を比べ、⌈log₂ S⌉ 回の probe で最初の分岐位置まで降り、element court へ出す)。
  これを `kaspad/src/palw_kernel_verifier.rs` が node の service として動かす(公開 bond、公開資料だけ、Panel の賛成を要しない)。
- **shadow**: `palw_detection_is_shadowed_v2` が、round 0 の anchor(commit 時点で producer が知らない)から `shadow_rate_ppm` の割合の claim を選ぶ。選ばれた claim は全再実行の対象になる。
  被覆率は `q_eff = q + r − q·r` になり、§4.1 の S を下げる。ε には数えない。

### 4.3 class を拒否する条件

- 武装時(`validate_armable_v1`): N = 1、n_max 未満の 2 の冪、n_max のそれぞれで、class 自身の担保規則(`collateral_per_interval_sompi · N`)が plan を通らなければ、policy は武装できない。
- claim の受理(`palw_detection_admit_claim_v1`): fence より後に、policy のない class、担保が S_required に届かない claim、資源に収まらない claim は受理しない。

## 5. round をまたぐ被覆(v2 の draw)

- seed は `H(DRAW_DOMAIN_V2 ‖ network ‖ anchor ‖ claim ‖ class ‖ job ‖ policy_id ‖ round)`。stream の label は `(Segment, checker scope, relation = round, repetition = 0)`。checker は `Seat(i)` か `PublicBond(rate key)`。
- round r の draw は、round < r で同じ checker が引いた区間を除いた残りから、`distinct_indices`(Floyd、rejection sampling)で k 個を引き、残りの中の順位を区間番号へ写す。重複は構成上起こらず、二重に数えることもない。
- anchor は claim の受理 DAA より後、かつ前の round の anchor より後でなければ拒否する(`AnchorNotPostCommit`)。これは seed.rs の staged round の規則と同じ形である。新しい beacon は作らない(round 0 は panel anchor、以降は後の slot の chain block)。
- 何も保存しない。restart、reorg、IBD の後も、同じ anchor から同じ集合を作り直す。round 2 の anchor が reorg で変われば、round 0 と 1 はそのまま、round 2 以降だけが作り直される(試験済み)。
- 新しい domain の golden vector は `[211, 748, 819, 858]`。legacy の golden vector は同じ試験の中で不変であることを確かめている。

## 6. k と資源の連動

`plan_v1` は k と一緒に次を確かめる。足りなければ class または claim を拒否する。opening を黙って落とすことはしない。

- 取得 bytes: `K_tot · opening_bytes ≤ max_claim_fetch_bytes`
- 再実行: `K_tot · units ≤ max_claim_replay_units`
- 保持: `k_round · claims_in_flight ≤ retention_pairs_cap`(legacy の 512 では、k ≈ 44 を 32 claim 分持てないので拒否される。試験済み)
- court/DA: `k_round ≤ court_openings_per_claim`、`k_round · opening_bytes ≤ da_bytes_per_claim`。引いた区間はすべて強制開示できなければならない。
- 期限: 1 round に要する時間 `fetch_daa + ⌈k_round·units / checker_units_per_daa⌉` が round 間隔以下で、`(rounds−1)·spacing + round_daa ≤ inspection_deadline_daa`

node 側の `pool_interval_opening` は、上限を `palw_detection_retention_pairs_v1`(fence のない間は 512 のまま、武装された policy の最大 `retention_pairs_cap` まで上げる)から取る。
新しい pair を入れる余地がなければ `false` を返し、caller が funnel の `capacity_refusal` に数え、warn を出す。

## 7. genesis family の値(設計値、未測定)

`palw_detection_genesis_design_v1` が route ごとの雛形を返す。class id は呼び出し側が渡す(root を発明しない)。

| family | route | 方式 | N rule の候補 | ε / D_min | 1 claim の最大 k(probe N = 1〜32,768) |
|---|---|---|---|---|---|
| #1 Dolphin3.0-Qwen2.5-1.5B | BASE-0 A16 | IntervalReplay | HeldPositions{4} または DecodeCalls{1} | 1% / 10%(最低 1) | 44 |
| #8 Qwen3.6-35B-A3B | PALWQ361 | IntervalReplay | HeldPositions{16}(例) | 1% / 10% | 44 |
| #2〜#6、#10 | TIR v1 | FullReexecution | PricedLeaves{1}(例) | 検査した claim は全数(miss 0)。被覆率 q が target | N(全区間) |

- 区間方式の k は、D_min を割合で決めているので N が大きくても 44 程度で頭打ちになる。小さい N では D_min が 1 なので全数に近い検査になる(例: N = 10 で k = 10)。
- legacy の k = 4 は、この不正集合(10% 以上の区間が不一致)に対して 99% に届かない。N = 1000、D = 100 のとき k = 4 の検出率は 34.4%。
- **未測定**(武装を拒否する理由): 1 区間の費用 c(sompi)、各 family の opening bytes(row bytes、32K context での frontier)、1 区間の再実行 CPU、検査者の処理速度、TIR family の全再実行時間と、それに基づく現実の被覆率 q。
  表の bytes、unit、throughput は形を確かめるための値であり、測定値ではない。
- #10(Llama 70B)は e2e で裁定できないので、どの方式でも SecurityCertified にならない。policy を置いても何も与えない。
- D_min より疎な不正(D = 1 の行の偽り、選択出力、区間末尾の cache の偽り)の検出率は 1 回の検査で k/N である(§8 の N = 64、k = 30 で約 47%。N = 32,768、k = 44 なら約 0.13%)。
  これは target の外であり、§4.1 の抑止(省略量に比例する利得)と §4.2 の全再実行・court で覆う。

## 8. 不正 corpus(fixture model、実経路)

`g14_detection_corpus.rs` は決定論的な整数モデルを使う。8 幅の持続 cache(token embedding の累積で、後のすべての row が読む。KV cache と同じ働き)、量子化 hidden(round-half-up shift、i16 飽和)、16 logits である。
seat は実際の seam(`PalwExecutionBackendV1::verify_fp_interval_opening`)を ADR-0082 D9 の手順で実装する。prompt と commit 済み id から checkpoint を自分で再計算して比べ、区間の全 row と decode 関係を厳密に再実行する。
draw は v2、k は policy の plan(N = 64、D_min = 7、k = 30、D_min での miss ≤ 8,660 ppm)、検査ループは `palw_detection_inspect_v2` である。
各不正で ground-truth の D を数え、600 個の独立な post-commit anchor で検出率を測り、厳密な超幾何値と比べた(許容 4σ + 1/T)。

| 不正 | D/64 | 実測 | 解析 | 区分 |
|---|---:|---:|---:|---|
| 全面偽造 | 64 | 1.0000 | 1.0000 | 不正集合(99%) |
| 1 row(中央) | 1 | 0.4950 | 0.4688 | D_min 未満 |
| 選択出力の偽り(中央) | 1 | 0.5200 | 0.4688 | D_min 未満 |
| 区間先頭 row / 末尾 row | 1 / 1 | 0.4600 / 0.4683 | 0.4688 | D_min 未満 |
| 先頭の cache 偽り | 64 | 1.0000 | 1.0000 | 不正集合 |
| 末尾の cache 偽り | 1 | 0.4683 | 0.4688 | D_min 未満 |
| prefill 最後の row / decode 最初の選択 | 1 / 1 | 0.4667 / 0.4733 | 0.4688 | D_min 未満 |
| 中央の cache(K/V)偽り | 39 | 1.0000 | 1.0000 | 不正集合 |
| 量子化 half-even(全体)/ 切り捨て(全体) | 45 / 64 | 1.0000 | 1.0000 | 不正集合 |
| overflow(wrap、全体) | 33 | 1.0000 | 1.0000 | 不正集合 |
| 量子化 half-even(最後の区間だけ) | 0 | 0 | 0 | commit 値が変わらず不正ではない |
| 別 job の row を 1 区間 / 別 model の row を 1 区間 | 1 / 1 | 0.4667 / 0.4950 | 0.4688 | D_min 未満 |
| 整合的な借用 trace(別 prompt の正直な実行) | 64 | 1.0000 | 1.0000 | 不正集合 |
| 開示拒否 8/64 | 8 | 0.9983 | 0.9959 | 不認証(DA/default 経路) |
| 開示拒否 全部 | 64 | 1.0000 | 1.0000 | 不認証 |
| 別 claim の認証済み opening を 8 区間で配る | 8 | 0.9933 | 0.9959 | 不認証(binding が合わない) |

- round: 4 round × 8 の異なる区間(計 32)で D = 1 の検出 0.5233(解析 0.5)。同じ 8 区間を 4 回引き直すと 0.1283(解析 0.125)で、被覆は増えない。
- 正直な claim: 4 model × 25 claim の全区間で、正しい seat と等価な別実装の seat はどちらも 1 度も flag しなかった。
  誤った丸め・切り捨て・wrap の seat は正直な claim をその場で 13,754 回 flag した。それらの局所化された leaf は、すべて参照の裁定(class の固定意味論で、claim 自身の commit から再計算する)が棄却した。
  検証者の実装差が producer の不正にはならない。
- 参照 seat が局所化した leaf は、どの不正でも参照の裁定が棄却しなかった(本物の不正である)。

**これは fixture model での policy 算術と seat の契約の確認である。** genesis family の実 kernel、bytes、CPU の測定ではない(§7)。
grinding、選択的 abort、複数 bond、operator の相関は、anchor が commit 後に決まること(draw を予測できない)に依存する。これらは測っていない。

## 9. 実効率の funnel(node 側)

`PalwDetectionFunnelV1`: eligible → checked → detected → localized → filed → accepted → convicted / defaulted → collected → invalidated。
loss(capacity_refusal、fee_shortfall、missing_material、deadline_expiry、unassigned、policy_refused、geometry_mismatch)は段とは別に数え、抜き取りの見逃しに混ぜない。
`is_consistent()` は段の順序(各段 ≤ 前段)を確かめる。`effective_rate_ppm()` は invalidated/eligible(監査 §5.3 の積を、仮定ではなく実測で出す)。

seat は `palw_panel.rs` の区間 duty で eligible、checked、detected、localized(leaf または range が名指された Fault)、missing_material、policy_refused、geometry_mismatch を claim ごとに 1 回ずつ数える。
保持上限で落ちた opening は capacity_refusal に数える。`PalwNodeTelemetryV1::detection_snapshot(class)` で読める。
filed 以降の段(court の受理、有罪、default、徴収、失効)はまだ配線していない。RPC(op 185)にも出していない。RPC は wire 形式の変更が要り、安くないためである。

## 10. 残り

1. **状態遷移での claim 受理**: `palw_detection_admit_claim_v1` は純関数として入れた。palw_state_v2 の claim 受理(`reserved`、`work_leaves`、受理 DAA)には未配線。fence は武装を拒否するので、現状で効く経路はない。
2. **複数 round の seat**: seat は round 0 だけを引く。`rounds > 1` の policy では、後の round の anchor を読む必要がある。それがない間は、引き足りないまま certify せず、duty を断る。public verifier の再検査・panel の redraw への round の割当も未配線。
3. **kernel/TIR route での policy の利用**: `palw_kernel_verifier.rs` は claim を全数再実行する設計で、shadow 選択と被覆率 q の実測は入れていない。
4. **court/DA の予算の実連動**: policy は宣言された `court_openings_per_claim`、`da_bytes_per_claim` を k と照合する。実際の court 容量コード(別 lane: G-07、owner admission)がこの値を読むようにする必要がある。
5. **同時 claim 数の強制**: plan は `k·claims_in_flight ≤ retention` を示すが、seat の scheduler が `claims_in_flight` を超えて duty を取らないようにする配線はない(落ちた opening は数えられ、黙っては消えない)。
6. **funnel の後段と RPC**: filed〜invalidated の計数(court lane、fold の結果)と RPC への公開。
7. **実測**: §7 の未測定値をすべて実 artifact で測る。その後で、各 genesis class の policy 値(class id 付き)を決め、ユーザーが承認する。

## 11. 実行した試験(この branch)

- `cargo test -p kaspa-consensus-core --test g14_detection_policy`: 18 passed。
- `cargo test -p kaspa-consensus-core --test g14_detection_corpus`: 5 passed。
- kaspad の変更の確認と t12 の指紋 pin の結果は、commit message と §11 の追記に書く。
