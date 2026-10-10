# ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **PROPOSED 2026-09-17; Decisions 1 and 2 IMPLEMENTED the same day, in shadow** on
  `feat/palw-exec-lane-and-validator-retirement` (§7). No consensus rule, parameter or fingerprint
  moves: everything here is node-local measurement until Decision 3 gets a height.
* **ADR-0144 alignment (2026-09-21).** Decisions 3–6 do not arm a scalar CCU until ADR-0146. See the end section.
* Operator's direction, in the operator's words: "次に見るべきは claim数ではなく `BILI / canonical compute`";
  "PWU = fork choice / consensus work、CCU = model間の経済価格 を分離"; "M4で何秒だったかを直接consensus値に
  しない — 時間は校正用データに留めます"; "DAA補正を二重に掛けないこと"; "Producer CCUとPanel CCUを分離";
  "45% domain capは報酬計算から切り離す"; "新モデル追加時に、shadow期間を必須にする"; "ここからモデル間の歪みを
  なくす方針で進める".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  Decision 6 (the work price this replaces), [0039](0039-palw-only-block-production.md) and
  [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) (per-class targets), [0117](0117-a-draw-is-one-forward.md) (a draw is one
  forward), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (the shadow-first discipline).

## 0. The sentence this ADR is

**A claim's pay is the network's rate times the economic compute the claim cost — the expected draw forwards
at its class's target plus its canonical job, each priced by a versioned, deterministic cost of the kernels it
runs — while fork choice keeps its leaves; and no rate, class or unit changes pay until the node has measured,
in the open, how many BILI each class earns per unit of compute it finalized and per unit it attempted.**

## 1. What testnet-11 pays today (read from its genesis objects and live node, 2026-09-17)

| class | canonical job | `pwu_per_inference` (leaves) | initial target | price at 6,001 |
|---|---|---|---|---|
| `PALW-BASE-0/rc` (floor) | 8 + 4 tokens, n_ctx 12 | 7,708 | MAX / 12,665 | unpriced (100 %) |
| `Qwen3.6-35B-A3B/graph-v3` | 7 + 2 tokens, n_ctx 8 | 2,685,360 | 0.6115 × MAX | 29.8 % |
| `Qwen/Qwen2.5-1.5B/graph-v5@512` | 63 + 2 tokens, n_ctx 512 | 6,630,544 | MAX (every forward wins) | 73.7 % |
| `Qwen/Qwen3.8-27B/graph-v3` (DAA 1,165, 1 ‰) | 7 + 2 tokens | 9,000,776 | — | 100 % (the unit) |

* **The basis is a static leaf count, not the claim's pwu.** `work_priced_escrow` prices by
  `palw_exposure_pwu_v1(class, claim.pwu)`, which for a `DerivedV1` class is `pwu_per_inference`. `claim.pwu`
  (the class target's expected attempts times the per-inference leaves) is fork-choice work and is not read.
  So there is no double count today; the expected-attempts factor is simply missing: a class drawn at
  0.61 × MAX pays ~1.64 forwards a claim and is paid as if it paid one.
* **Leaves are not compute, and the two model tiers are priced 1.86× apart per unit of the compute
  they ran.** A leaf is `elements / tile_len` of a node's output — the dense tier tiles at 128, the hybrid
  at 512 — so the same row commits four times the leaves on one class; a routed-expert row commits eight
  experts' matmuls as one row; and the canonical jobs differ nine-fold in tokens. Measured with Decision 2
  (§7): the dense tier runs 12,767 MAC-equivalents a leaf, the hybrid 7,846 (canonical against canonical,
  1.63×); and since the job an attempt runs is the prefill-only job (ADR-0117) while the price is the
  canonical job's leaves, the hybrid — which loses two of its nine tokens' decode calls to the rule where
  the dense tier loses two of sixty-five — is paid **86.4 % more per MAC-equivalent it ran** than the
  dense tier on today's basis. By the operator's line (5 % good, 10 % acceptable, 20 % a distortion) that
  is a distortion, and it is the basis's, not the mix's: it is the same at any claim ratio.
* **The unit is whichever weight-bearing model class is heaviest**, and a class bears weight once its share is
  above zero — a class registered at 1 ‰ set testnet-11's unit and lowered every other class's pay. Read
  in expected attempts the same rule is unusable: that class's target is 3,165 draws a claim, so an
  attempted-compute unit set by it would price both live tiers at 0 ‰ (§7).
* **Panels license nothing for the hybrid.** In the live window (§7) the hybrid's 500 sampled claims
  reached no licence; every one timed out and was redrawn. Its producers' `A_m` is therefore zero
  whatever the price says — the largest live distortion is panel liveness for the 33 GiB class, not a
  formula. The dense tier licensed 133 of 453 terminal claims (48 `Final`, 320 voided).
* **What is already separate:** the 45 % domain cap and the parity rules shape execution slots only (ADR-0125,
  ADR-0130); pay is per `Final` claim. Execution-block fees go to the permit holder's payout — a future source of
  cross-class income differences, left for later.

## 2. Decisions

**Decision 1 — measure first: per-class economics over RPC (node-local).** For each class and each span: attempts
accepted, claims `Final` and voided, final rate; Σ `claim.pwu`, Σ static leaves, Σ economic compute (Decision 2);
producer BILI, panel BILI and execution-fee BILI actually paid; average class target and expected attempts; and
the ratios `F_m = paid BILI / Final compute`, `G_m = paid BILI / attempted compute` (voided claims included) and
`Gap = max(F) / min(F) − 1`, reported for the leaf basis and the compute basis side by side. A recorder follows
the chain and keys every row by claim id, so a reorg rewrites a row rather than adding one.

**Decision 2 — `EconomicComputeV1`: kernel-weighted compute, deterministic and hardware-independent.** From a
class's shape profile and a job context: per position, the multiply–accumulate count of every node — matmuls
by input × output and a dtype weight, attention by heads × head dim × the true kv length, GatedDeltaNet by its
state update, MoE by the router and the active experts only, elementwise ops by element count, the LM head where
logits are computed — summed with one versioned cost table (`PALW_ECONOMIC_COST_TABLE_V1`). Wall-clock time on
reference hosts calibrates the table and is never a consensus input. Shadow only: nothing on the block path reads it.

**Decisions 3–6 — reward scalar (not adopted).** 単一の CCU/MAC-eq に報酬を切り替える計画は実装しない。class 間の実コストを同じ尺度で表せる根拠がなく、[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P4 の representation-neutral accounting を満たさないため。[ADR-0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) は係数表を採用していない。D1–D2 の shadow 計測は維持する。

## 7. Implementation record and measurements (2026-09-17)

**Built, in shadow.** `consensus/core/src/palw_economic_compute_v1.rs`: the cost table
(`PALW_ECONOMIC_COST_TABLE_V1`: a MAC-equivalent is one 8-bit-weight multiply–accumulate; attention over
the cache by heads × head dim × true kv length; the gated-delta recurrence at four MACs a state element;
norms 2, rotations 2, transcendentals 4, GLU 5, the convolution a tap a channel; wider weight dtypes at their
byte width), the per-node derivation from the profile's own node tables (a dense matmul is input width ×
output width; a routed-expert matmul is the concatenated row's active experts only, the down projection
block-diagonal over the router's `k`; the LM head where logits are computed), the closed form over exactly
the positions the leaf enumeration counts, the priced reward over `u128` measures, the three bases, the
unit, and the class census (`palw_class_census_v1`: registration numbers, target, expected attempts, claims
by phase, redraws, escrows). `getPalwClassEconomics` (op 185) hands a client the census with each class's
compute from its registered carriage or this build's ledger; `misaka palw economics` prices every class on
every basis with the same functions and prints `F_m`, `A_m`, the panel's rate per verification compute and
the gaps. Nothing on the block path reads any of it.

**The live classes, measured** (cost table v1; the job an attempt runs is the prefill-only job):

| class | canonical job | leaves | canonical MAC-eq | attempt job MAC-eq | MAC-eq / leaf | per token |
|---|---|---|---|---|---|---|
| `PALW-BASE-0/rc` | 8 + 4 | 7,708 | 30,504,896 | 21,657,728 | 3,957 | 2.5 M |
| `Qwen3.6-35B-A3B/graph-v3` | 7 + 2 | 2,685,360 | 21,070,759,296 | 18,055,200,736 | 7,846 | 2.34 G |
| `Qwen/Qwen2.5-1.5B/graph-v5@512` | 63 + 2 | 6,630,544 | 84,653,733,376 | 83,102,171,136 | 12,767 | 1.30 G |
| `Qwen/Qwen3.8-27B/graph-v3` | 7 + 2 | 9,000,776 | 198,712,432,640 | 172,919,123,392 | 22,077 | 22.1 G |

By kind, the hybrid's canonical job is 11.4 G dense projections, 8.1 G routed experts (eight of 256 at
2048 × 512, three matmuls, forty layers), 0.5 G recurrence, 1.0 G logits (2048 × 248,320 twice), 3 M
attention at kv ≤ 8; the dense tier's is 83.9 G dense (28 layers of 1536 × 8960 SwiGLU over 64 positions),
0.18 G attention, 0.47 G logits. The 27B is a one-expert mixture of 5120 × 17,408 over 64 layers.

**The draws a claim costs are not whole.** The fork-choice factor `palw_expected_attempts_v1` floors
`2¹²⁸ / (target + 1)` to whole inferences — right for `claim.pwu`, wrong for a price of attempts. At the live
targets (DAA 5,784: the dense tier at `2.276 × 10³⁸`, the hybrid at `3.396 × 10³⁸`) both classes read as one
expected attempt, while the dense tier draws **1.495** forwards a claim in expectation and the hybrid
**1.002** (`palw_expected_attempts_q32_v1`, Q32 fixed point, integer part the consensus factor at every
target; the 27B 3,165.96, the floor 26,403.64). The shadow's attempted basis reads the fraction
(`palw_attempted_compute_q32_per_claim_v1`); the census and op 185 carry both numbers.

**Per claim, at testnet-11's 6,001 escrow (3,200.85 MSK), the live targets and the live unit (the 27B),
producer 80 / panel pool 20; `F` per MAC-eq the `Final` claim's job cost, `A` per MAC-eq its draws cost in
expectation (MSK per 10⁹ MAC-eq):**

| basis | unit | Qwen3.6 price · MSK (producer / pool) | Qwen2.5 price · MSK (producer / pool) | `F₃₆` / `F₂₅` | gap `F` | `A₃₆` / `A₂₅` | gap `A` |
|---|---|---|---|---|---|---|---|
| leaves (in force) | 9,000,776 | 29.8 % · 954.96 (763.97 / 190.99) | 73.7 % · 2,357.95 (1,886.36 / 471.59) | 52.89 / 28.37 | **86.4 %** | 52.78 / 18.98 | **178 %** |
| economic job | 172.9 G | 10.4 % · 334.21 (267.37 / 66.84) | 48.1 % · 1,538.28 (1,230.62 / 307.66) | 18.51 / 18.51 | 0.0 % | 18.47 / 12.38 | **49.2 %** |
| economic attempted | 547 T | 0 % · 0.11 | 0 % · 0.73 | 0.01 / 0.01 | 49.2 % | 0.01 / 0.01 | 0.0 % |

The measured producer / pool split is 80 % / 20 % after pricing. The compared bases do not simultaneously equalize per-job and per-attempt cost. No reward scalar or calibration rate is selected from this shadow comparison; ADR-0144 P4 and ADR-0146 govern. The conservation result remains `producer + pool + burned == escrow`; the floor is paid whole on every measured basis.

**The live window** (1,311 distinct claims read from four bonds at DAA 5,774, before 6,001, when every
`Final` paid its whole 2,756.28 MSK escrow to the producer and no panel was paid): the dense tier's
`F` = 29,021 MSK per 10¹² MAC-eq and `A` = 1,748 (48 `Final` of 797 accepted; 320 voided at
`receipt_timeout`, 451 redrawn); the hybrid's `A` = 0 (500 accepted, none licensed, 282 redrawn); the floor's
window held no `Final`. The hybrid's gap is not a price: its panels do not license it.

**Calibration.** The operator's timings — the dense tier's A16 runtime at ~50 prefill / ~33 decode tokens a
second on an M4 Pro, the hybrid's canonical job at ~7.7 s on the reference host — put the dense tier at
~65 G MAC-eq/s and the hybrid at ~2.7 G/s: a twenty-fold gap this table does not predict (its per-token
cost ratio is 1.8). The hybrid's 33 GiB artifact is not resident on that host (ADR-0112's page faults), so
its timing is a host property, not the graph's; calibration needs a host where the artifact is resident,
and the routed experts' per-token weight traffic (~1 GB a token at batch one) is the term to measure there.

**Not built.** Decision 1's durable recorder of actual payments and a window beyond state retention. The former scalar reward fence, panel-compute pricing and calibration-rate implementation plans are not adopted: no justified scalar was found (ADR-0144/0146).

## 3. Deferred

An execution-fee common pool; panel pay by verification compute (after Decision 4's measurement); a DA/availability
reward.

## 4. Status

* Decisions 1 and 2: built in shadow (§7); the λ shadow of ADR-0130 Decision 8 is not yet read by op 185.
* Decisions 3–6: not adopted; shadow measurements alone do not authorize a reward scalar.

## 5. Number hygiene

0131 was free when written; the next free number is 0132.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P4: reward may change
only when verified work changes. Canonical work is a **vector**; a single scalar (MAC-eq, CCU, a
heaviest-class unit) has not been shown to track real cost. [ADR-0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)
records the coefficient search; no coefficient table or scalar was adopted.

* **Kept, in shadow.** Decisions 1 and 2: node-local measurement, PWU stays fork-choice work, time
  is calibration data, producer compute and panel compute are named separately. Op 185 may keep
  printing the census.
* **Decisions 3–6 do not arm a scalar CCU.** The reward basis, the panel-pay measurement as a
  consensus input, the rate, and a new model's shadow-then-price path wait on ADR-0146. Arming
  them first would freeze the arithmetic 0144/0145 already disqualified. The 2026-09-17 "operator's,
  after the shadow has run" schedule is withdrawn as a sufficient condition.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
