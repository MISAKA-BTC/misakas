# PALW spec — 05. Canonical work

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/work.md](../../design/palw/work.md). The code is the truth, and disagreements are
> listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (the ADR-0145 bundle, the work target
and the economic payout, all from genesis)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P4 (representation-neutral), P5 (efficiency keeps its profit), P7 (usage never
changes the unit price).

How much work an inference was. Work is a **vector derived from the registered graph and the execution
facts**, never a number a registrant or a miner declares. The same derivation prices both lanes. One
network-wide work target `W` buys one unit of work from any model, so a model's share of blocks is an
output, not an input.

## 5.1 The CanonicalWorkVector

- **PALW-WK-1 (the vector).** Canonical work MUST be the vector `PalwCanonicalWorkVectorV1`, derived by
  the chain from the class's registered graph and the run's execution facts:

  | Component | Unit |
  | --- | --- |
  | `dense_matmul` | MAC-eq |
  | `routed_expert_matmul`, counting only the active experts, a graph fact | MAC-eq |
  | `attention_prefill` | MAC-eq |
  | `attention_decode` | MAC-eq |
  | `recurrence` | MAC-eq |
  | `normalization` | MAC-eq |
  | `other_verified_ops` | MAC-eq |
  | `weight_traffic_bytes`, derived from the artifact's real block layout | bytes |
  | `kv_read_bytes` | bytes |
  | `kv_write_bytes` | bytes |

  No component MAY be declared by a registrant or a miner.
- **PALW-WK-2 (one derivation).** An attempt and a free-prompt claim of the same run MUST derive the
  same vector. Only new positions count: a reused prefix is an execution fact committed by prefix state
  (11 §11.4), never a claim the producer makes.
- **PALW-WK-3 (representation neutrality).** No quantity that feeds reward or weight MAY change with
  `tile_len`, serialization, commitment splitting, map iteration order, a profile's shape, or any
  self-reported cost. *Code:* the property tests in `core/palw_reward_properties_v1.rs`.

**Sources:** ADR-0145 §2–§6, ADR-0144 P4. **Code:** `core/palw_canonical_work_v1.rs`
(`PalwCanonicalWorkVectorV1`), `core/palw_derived_v1.rs`.

## 5.2 From vector to units

- **PALW-WK-4 (no scalar table).** A dimension MAY be priced by a coefficient only if the arbitrage the
  coefficient permits stays within the bound. The search measured a bound of 1.000000×, so no
  coefficient table exists and no scalar compute unit (CCU) is armed (`palw_arbitrage_search_v1`).
  Most dimensions need no coefficient at all.
- **PALW-WK-5 (the provisional scalar).** Until a coefficient passes that test, weight and pricing use
  the derivation's provisional scalar: the attempt lane's weight derivation. Past
  `palw_canonical_work`, the free-prompt price reads it too.
- **PALW-WK-6 (shadow is not normative).** Economic compute (kernel-weighted compute,
  `palw_economic_compute_v1.rs`) and the
  per-class economics census (op 185, `misaka palw economics`) are node-local measurements. No
  consensus rule reads them.

**Sources:** ADR-0146 §2–§4 and §9; ADR-0131 D1–D2 (shadow; D3–D6 not to be armed); ADR-0144 §6.
**Code:** `core/palw_arbitrage_search_v1.rs`, `core/palw_economic_compute_v1.rs`,
`core/palw_economics_ledger_v1.rs`.

## 5.3 pwu

- **PALW-WK-7 (an attempt's pwu is the derivation).** Past `palw_canonical_work`, an attempt's `pwu` MUST
  equal `palw_attempt_derived_pwu_v1(effective class target, derived draw)`: the expected attempts a
  win costs at the target, times the derived work of the draw the class runs. Any other value is refused
  (`PwuClaimNotDerived`), and so is a class with no derived draw (`PwuUnderivable`). The weight is
  `claim.pwu` (`palw_claim_canonical_weight_v1`). Nothing a registrant declares
  (a `pwu_per_inference`, a leaf count, a `tile_len`) multiplies it. The floor keeps its row's
  derivation.
- **PALW-WK-8 (a free-prompt claim's pwu).** Past `palw_canonical_work` and `palw_fp_derived_work`, a
  free-prompt claim's credit is its run's canonical work over new positions
  (`fp_derive_credited_compute_v1`). It is split into `n = min(⌊C/Q⌋, 2^16)` equal quanta, where `Q` is
  the network quantum (the floor's derived draw over the ruleset's divisor). `pwu = n × ⌊C/n⌋`. At
  `Final` a claim retires `pwu/quanta × spent.len()` of safe weight. The declared `work_leaves` must
  still equal the graph's leaf count, because the court walks leaves, but nothing is paid in leaves.

**Sources:** ADR-0149, ADR-0148 §2, ADR-0145 §5; superseding the declared-pwu paths of ADR-0045 D1,
ADR-0072 D5 and ADR-0074 D5. **Code:** `core/palw_pwu.rs`, `core/palw_fp_admission_v3.rs`
(`fp_derive_credited_compute_v1`, `palw_fp_spend_weight_v1`).

## 5.4 The work target W

- **PALW-WK-9 (one target).** Past `palw_work_target`, a class MUST have no target of its own. At a
  block with work target `W`, a model class's attempt wins with ticket
  `T_m = MAX · min(1, CCU_m / W)`, where `CCU_m` is its derived work per draw
  (`palw_work_ticket_target_v1`). Nothing a class does moves another class's odds.
- **PALW-WK-10 (walking W).** At every epoch boundary,
  `W ← max(W₀, clamp(W · model_blocks / expected_blocks, ÷4, ×4))`, walked over model blocks only, with
  `W₀ = escrow / rate`.
- **PALW-WK-11 (the floor is the residual).** The floor class draws against its own target, walked to
  fill the cadence that model blocks leave. A floor block carries transactions and fees, and no escrow.
- **PALW-WK-12 (share is a reader's number).** A class's share of blocks is computed by readers (op 185
  and 186, the CLI) from `Final`s. It is not state, and no consensus rule reads it.
- **PALW-WK-13 (the budget replaces caps).** A claim is admitted only within its class's verification
  room (10 PALW-CO-36). The lifecycle gates admission and never prices it (03 PALW-CL-11).
- **PALW-WK-14 (free-prompt pooled target).** Free-prompt quanta draw against one pooled receipt target
  (`fp_pooled_receipt_key_v1`), walked on the whole receipt lane and clamped so that a quantum never
  gets better odds than a forward of the same compute at `W`.
- **PALW-WK-15 (epoch budgets that remain).** Where an epoch budget still applies (the floor, and
  pre-work-target networks), unused budget is released progressively (`palw_epoch_budget_release`,
  ADR-0123), and the boundary budget follows `palw_epoch_boundary_budget`.

**Sources:** ADR-0137 D1–D8 (superseding share-as-input: ADR-0054, 0076, 0107, 0135 D5), ADR-0148 §2
items 2–5, ADR-0123, ADR-0045 D2. **Code:** `core/palw_work_target_v1.rs`, `core/palw_class_daa.rs`,
`core/palw_fp_admission_v3.rs`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis: `palw_canonical_work`, `palw_fp_derived_work`, `palw_admission_independence` (armed together or not at all), `palw_work_target` with the single lottery, the epoch budgets |
