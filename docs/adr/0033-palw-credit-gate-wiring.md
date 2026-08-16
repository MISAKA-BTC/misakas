# ADR-0033: The credit gate, wired — how `credit(C)` becomes a consensus fact

Status: **Accepted (design; activates nothing, and cannot be activated until its stated
preconditions are met).** ADR-0028 §1 defined `credit(C)` as a predicate. This ADR decides
**where it is evaluated, what state it reads, how it survives reorgs, and what happens the
moment it says yes** — the B14 wiring, specified so a future Stage-2 release implements one
design rather than inventing three.
Date: 2026-08-16
Relates to: ADR-0028 §1 (the predicate), §4 (the economics, with the 2026-08-16 leverage
amendment), ADR-0029 (the Stage-1 carriage and store the gate reads), ADR-0027 §6 (the stage
ladder), ADR-0032 (fee/bounty value flows), `docs/palw-class-activation-gate-status.md` (the
§12 ledger this promotion answers to), the capability credit walk in
`consensus/src/pipeline/virtual_processor/processor.rs` (the template).

## Preconditions — none of this may ship before all of them hold

1. Stage-1 carriage live: dedicated subnetwork ids, stateless validators, and the
   accept/revert/backfill store (ADR-0029 Stage 1).
2. ADR-0028 §4e's leverage remedy **chosen and encoded** — either the per-validator credited-job
   cap or a fractional `base(C)` (the B15 finding; the current parameters violate
   `max_leverage ≤ 1` by ~10⁴×, so wiring credit without this mints against nothing).
3. §12 gate items 2, 4, 5, 10, 11-external and 12-exercise met (`palw-class-activation-gate-status.md`).
4. A Stage-1 soak with zero unexplained class freezes.

## Decision

### 1. Where the gate is evaluated

`credit(C)` is evaluated **in the virtual processor's chain walk, at the block whose accepted
DAA score first reaches `challenge_close_daa(C)`** — never at commitment time, never by a
timer. The walk already visits every accepted transaction of every chain block (the capability
credit walk does exactly this); the PALW gate is a second consumer of the same walk over the
Stage-1 carriage store.

Consequences of that placement, all deliberate:

* **The gate is a pure function of chain state**, so every node computes the same answer at the
  same block — the property that lets credit be consensus rather than telemetry.
* **A job's credit is decided once**, at a specific block, and is thereafter history.
* Wall-clock never enters. Under a stall, DAA stalls, and every deadline stalls with it
  (ADR-0028 §3's stall rule) — the gate simply is not reached.

### 2. What it reads

Exactly four facts, all already carried or derivable:

| fact | source |
| --- | --- |
| the commitment `C` and its `daa(C)` | carriage store (kind 0x01), accepted-DAA-indexed |
| assigned panel at the anchor | `select_replay_panel_v1` over the bonded set at `daa(C)+Δ_bind` — derived, not stored |
| attestations against `C` | carriage store (kind 0x02), filtered to panel members, root-equal, on-time |
| refutations against `C` | carriage store (kind 0x05/0x06), any accepted one, adjudicated |

No off-chain input, no oracle, no timer. The panel is *derived* rather than stored precisely so
a stored panel cannot drift from the rule that produced it.

### 3. The predicate, verbatim from ADR-0028 §1

```
credit(C) ⟺ W_challenge(C) closed
          ∧ ≥1 assigned attestation with an independently recomputed root equal to C's
          ∧ no accepted refutation against C
```

Zero attestations ⇒ credit 0. The panel is never shrunk to make a job creditable, and a
refutation accepted at any point inside the window voids credit regardless of attestation
count. A refutation accepted **after** the window still convicts (slash is not window-bound)
but does not retroactively revoke credit — that asymmetry is deliberate and is why the
Stage-0 ledger counts "credited-and-later-refuted" as its own tail metric.

### 4. What "yes" does

`credit(C) = true` makes the job's `base(C)` (and the `q · ρ_v · base(C)` attester share)
**mintable in the coinbase of the crediting block**, subject to §4's caps. Concretely: the
crediting block's coinbase gains PALW outputs; a node validating that block recomputes the
gate from its own state and rejects a coinbase claiming credit the gate does not grant.

This is the ONLY consensus-visible effect. Per ADR-0027 §7's standing rule, no PALW outcome —
pass, fail, dispute, freeze, credit — touches block validity beyond its own coinbase claim,
fork choice, or any past block.

### 5. Reorg behavior

The gate follows the chain walk, so it inherits the store's accept/revert discipline: a
reverted chain block un-does its carriage inserts, and any credit decision made at that block
is un-made with it. Because the decision is a pure function of the state at that block, a
re-org that changes which transactions were accepted before `challenge_close` may legitimately
change the answer — and every node will change it identically. `Δ_bind` keeps the *anchor*
settled (ADR-0028 §2); `finality < W_challenge` (§3) keeps the *decision* inside the finality
horizon, so a credited job cannot be un-credited by a reorg deeper than finality.

### 6. Class freeze interaction

A frozen class credits nothing (`palw-class-activation-gate-status.md` §2): the gate reads
`class_active ∧ ¬class_frozen` from the registry before anything else, and a zero
`credited_ceiling` makes `credit(C) = 0` through the ceiling arithmetic itself with no
special case. The emergency rollback is therefore *inside* this gate, not bolted beside it.

## Consequences

* One design to implement, with its preconditions written down — the wiring cannot be
  "started early" without visibly violating item 2 or 3 above.
* The gate is a second consumer of the Stage-1 store and the existing chain walk; it adds no
  new state machine and no new consensus surface beyond the coinbase claim it authorizes.
* Because the gate is where the ceiling and the freeze are read, §12's rollback exercise and
  B15's leverage remedy both land here — which is why neither may be deferred past this ADR's
  implementation.
