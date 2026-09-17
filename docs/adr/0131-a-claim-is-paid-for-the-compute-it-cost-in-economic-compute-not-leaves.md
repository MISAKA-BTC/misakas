# ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves

* Status: **PROPOSED 2026-09-17; shadow implementation in progress** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule changes until a later fenced height; everything here first runs as node-local measurement.
* Operator's direction, in the operator's words: "次に見るべきは claim数ではなく `MSK / canonical compute`";
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
in the open, how many MSK each class earns per unit of compute it finalized and per unit it attempted.**

## 1. What testnet-11 pays today (read from its genesis objects and live node, 2026-09-17)

| class | canonical job | `pwu_per_inference` (leaves) | initial target | price at 7,001 |
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
* **Leaves are not compute.** A leaf is a tile of a node's output: a Q4 dense matmul, a GatedDeltaNet state
  update, a softmax row over the kv length and an MoE expert matmul each commit leaves at very different
  costs, and the canonical jobs above differ nine-fold in tokens.
* **The unit is whichever weight-bearing model class is heaviest**, and a class bears weight once its share is
  above zero — a class registered at 1 ‰ set testnet-11's unit and lowered every other class's pay.
* **What is already separate:** the 45 % domain cap and the parity rules shape execution slots only (ADR-0125,
  ADR-0130); pay is per `Final` claim. Execution-block fees go to the permit holder's payout — a future source of
  cross-class income differences, left for later.

## 2. Decisions

**Decision 1 — measure first: per-class economics over RPC (node-local).** For each class and each span: attempts
accepted, claims `Final` and voided, final rate; Σ `claim.pwu`, Σ static leaves, Σ economic compute (Decision 2);
producer MSK, panel MSK and execution-fee MSK actually paid; average class target and expected attempts; and
the ratios `F_m = paid MSK / Final compute`, `G_m = paid MSK / attempted compute` (voided claims included) and
`Gap = max(F) / min(F) − 1`, reported for the leaf basis and the compute basis side by side. A recorder follows
the chain and keys every row by claim id, so a reorg rewrites a row rather than adding one.

**Decision 2 — `EconomicComputeV1`: kernel-weighted compute, deterministic and hardware-independent.** From a
class's shape profile and a job context: per position, the multiply–accumulate count of every node — matmuls
by input × output and a dtype weight, attention by heads × head dim × the true kv length, GatedDeltaNet by its
state update, MoE by the router and the active experts only, elementwise ops by element count, the LM head where
logits are computed — summed with one versioned cost table (`PALW_ECONOMIC_COST_TABLE_V1`). Wall-clock time on
reference hosts calibrates the table and is never a consensus input. Shadow only: nothing on the block path reads it.

**Decision 3 — the reward basis, when it moves, is `expected attempts × draw compute + job compute`.** Expected
attempts come from the class target at the attempt (the same factor `claim.pwu` carries, read once, never
multiplied again); the draw is ADR-0117's one forward; the job is the canonical job executed once. Fork choice
keeps `claim.pwu` in leaves. The switch is a fence at a height the operator names after Decision 1 shows
`Gap` under 10 % on the compute basis (5 % once data suffices) and the calibration residuals are stated.

**Decision 4 — a panel is paid for verification compute, measured separately.** A seat replays sampled intervals
and opens trace material; its compute per claim is not the producer's. Classes carry `producer` and `panel`
compute separately before panel pay follows compute.

**Decision 5 — the unit is not the heaviest class.** Pricing by compute needs a rate, not a class to divide by:
a registration must not be able to move every other class's pay. The rate's derivation (a declared reference
job, or the span's compute-weighted average) is decided with Decision 3.

**Decision 6 — a new model earns under the rate only after a shadow period.** A class registers, produces and is
measured (compute, runtime on reference hosts, final rate, calibration residual) before its claims are priced by
compute.

## 3. Deferred

An execution-fee common pool; panel pay by verification compute (after Decision 4's measurement); a DA/availability
reward.

## 4. Status

* Decision 2's module and its measured table: in progress (`wip/adr0131-economic-compute`).
* Decision 1's recorder and RPC: after ADR-0130's M1/M2 merge (it reads their functions for ADR-0130's λ shadow).

## 5. Number hygiene

0131 was free when written; the next free number is 0132.
