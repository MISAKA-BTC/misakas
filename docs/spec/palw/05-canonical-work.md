# PALW spec — 05. Canonical work

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions.

**Purpose.** This chapter defines how much work an inference was, the quantity reward and weight are
computed from. Work is a **vector derived from the registered graph and the execution facts**, never
a number a registrant or miner declares. It is converted by protocol rules that no participant sets.
The same derivation prices both lanes. This chapter also defines the network-wide work target `W`,
which buys one unit of work from any model. A model's share of blocks is an output of this chapter's
rules, not an input.

**Principles served:** P4 (representation-neutral), P5 (efficiency keeps its profit), P7 (usage never
changes the unit price).

## 5.1 The CanonicalWorkVector

- [ ] Define the vector and its components (dense compute, routed-expert compute, attention prefill
  and decode, KV read and write, …), and the facts each component is derived from. *Sources:* 0145 §4,
  0144 §5. *Code:* `core/palw_canonical_work_v1.rs` `PalwCanonicalWorkVectorV1`.
- [ ] **One derivation for both lanes.** An attempt and a free-prompt claim of the same work derive the
  same vector. *Sources:* 0145 §5, 0148, 0149. *Code:* `core/palw_derived_v1.rs`.
- [ ] Cache reuse is an execution fact, never a claim. How it enters the vector is specified with the
  prefix state in chapter 11. *Sources:* 0145 §6.

## 5.2 From vector to units: the coefficient rule

- [ ] **No scalar coefficient table.** A dimension is priced only if the arbitrage its coefficient
  would allow stays under the bound. The search found a bound of 1.000000×, so no scalar is armed.
  *Sources:* 0146 §2–§4 and §9. *Code:* `core/palw_arbitrage_search_v1.rs`.
- [ ] A scalar compute unit (CCU) MUST NOT be armed. The economic-compute measures are shadow only and
  not normative. *Sources:* 0131 D3–D6 (withheld by 0144 and 0146), 0132 §6. *Code:*
  `core/palw_economic_compute_v1.rs`, `core/palw_economics_ledger_v1.rs` (shadow, read by op 185 and
  `misaka palw economics`).
- [ ] The invariants P4 and P5, stated as testable properties: splitting, re-serialising or re-tiling
  a commitment leaves reward unchanged. *Code:* `core/palw_reward_properties_v1.rs`.

## 5.3 pwu

- [ ] An attempt's pwu **is** the derivation, and the weight reads it directly. Nothing multiplies it.
  *Sources:* 0149 (supersedes the declared `pwu_per_inference` path of 0045 D1, 0072 D5 and
  0074 D5). *Code:* `core/palw_pwu.rs`.
- [ ] A free-prompt claim's pwu (`pwu/quanta × spent.len()` at Final) priced by the same derivation.
  *Sources:* 0148, 0144 §1. *Code:* `core/palw_state_v2.rs` (the Final-work iterator
  `final_work_iter`).

## 5.4 The work target W

- [ ] A block buys one unit of work from any model. One network-wide target `W` prices a unit, and a
  class's share is a result of production, not a lottery input. *Sources:* 0137 (supersedes 0054,
  0076, 0107 and 0135 D5 past `palw_work_target`). *Code:* `core/palw_work_target_v1.rs`.
- [ ] Issuance per unit (what `W` implies for subsidy), linked to chapter 10. *Sources:* 0137 §8.
- [ ] Per-class epoch budgets that remain (the boundary budget, and the progressive release of unused
  budget), and how they compose with `W`. *Sources:* 0045 D2, 0123. *Code:* fences
  `palw_epoch_boundary_budget` and `palw_epoch_budget_release`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis: the ADR-0145 bundle (`palw_canonical_work`, `palw_fp_derived_work`, `palw_admission_independence`, armed together or not at all), the work target, the epoch budgets |

**Design:** `design/palw/work.md`.
