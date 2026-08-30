# ADR-0064 — Trustless recovery from a total producer stop: the bond becomes usable in the block that registers it

Status: **Partially superseded by its own correction** (2026-08-30). It was written to answer the
one item ADR-0060 §12 left open after the audit withdrew the heartbeat lane. **It does not answer
it** — see the correction under §Decision, verified against the code and the reason the title is now
wrong. What it does deliver is landed and dormant: a bond becomes usable one chain block earlier,
and the pipeline stops disagreeing with the state machine about which state answers *is this bond
registered*. Facts A and B stand on their own.

## The question

If every bonded producer stops permanently, can the chain restart **without** an operator, a
checkpoint or a privileged key? Producing requires an attempt naming a registered bond; a bond is
registered by an object riding a transaction; a transaction needs a block; a block needs a
producer.

Four independent designs were generated and each red-teamed twice. Two facts decided it, and both
are worth more than the answer.

## Fact A — "the network was silent" is not a checkable predicate. This one is a theorem.

The heartbeat lane, and three of the four designs, gated a recovery right on demonstrated silence.
That cannot be done. The only wall clock in consensus bounds the **future**
(`check_block_timestamp_in_isolation`); the past is bounded by median-time-past, computed from the
branch's own headers. The other clock — DAA — advances only when blocks are produced, so in a total
stop it is frozen, which is the hostage clock ADR-0060 §1 is about.

So "no bonded block for S" evaluates to *"my selected parent's declared timestamp is S old"*, which
is true of **every fork rooted more than S ago on a perfectly healthy chain**. A branch cannot
witness the DAG's future. No depth bound or state read fixes this; it is what silence means.

**The design constraint that follows: the recovery right must be worthless to a fork, because you
cannot stop a fork from claiming it.**

## Fact B — a zero-weight lane hands fork choice to the block hash, for free

`palw_tip_weights_v1` returns `None` unconditionally on a V2 network, and its own comment records
that wiring the PALW order into the tip heap was a measured permanent wedge. Ordering therefore
falls to `SortableBlock` = `(blue_work, hash)`, and the deep-reorg comparator runs only for
candidates that are **not** chain-descendants of the sink — so it is not consulted on forward
progress.

Any lane whose blocks carry zero blue work makes every branch tie on work, and the largest hash
wins, at zero cost, forever. (This also settles the ε-versus-0 question the withdrawn lane left
open: 0 is not available.)

## Decision — no new lane. Move one lookup.

> On a `ConsensusV2` network past `palw_bootstrap_activation`, admission item 1's **bond lookup**,
> for the block's **own** attempt only, resolves against
> `bonds(parent state) ∪ { BondRegistered carried by this block's own mergeset }`.
>
> Every other item — class record, artifact root, class target, the `DerivedV1` pwu equality,
> epoch budget, class ticket, exposure ceiling, duplicate-attempt — keeps reading the parent state.

Equivalently: **a bond becomes usable by the block that accepts its registration, instead of by
that block's child.**

> ### CORRECTION (2026-08-30, against the implemented code) — this does NOT close the deadlock
>
> The sentence that stood here — *"a would-be producer mines one block that both carries their
> `BondRegistered` and makes an attempt under it; no existing producer is needed, so the deadlock is
> gone"* — is **false**, and it was false when written. Three facts, each read off the tree:
>
> 1. **A block's own body is never in its own mergeset.** `calculate_utxo_state` builds
>    `mergeset_acceptance_data` from `once(selected_parent) ++
>    consensus_ordered_mergeset_without_selected_parent` (`utxo_validation.rs:328-331`), which is
>    `mergeset_blues[1..] ++ mergeset_reds`. A block's transactions are accepted by a *later* block,
>    never by itself. So "this block's own mergeset" cannot contain a registration the same block
>    carries — the recovery block described above is unconstructible.
> 2. **A `ConsensusV2` network accepts exactly two lanes, and both require standing already.**
>    `accepts_algo_id` (`palw_mode_v2.rs:729-737`) admits the committed-attempt id — which must
>    decode an envelope naming a registered bond — and the free-prompt receipt id — which must
>    name a claim already certified at this chain point. The heartbeat lane is `false` since the
>    2026-08-30 audit.
> 3. Therefore **a party holding neither a bond nor a certified quantum cannot make any block at
>    all**, so its `BondRegistered` transaction can never reach a block body, so no later block can
>    accept it. Moving the lookup earlier does not help: the input it reads is empty for exactly the
>    party the ADR was written for.
>
> **What the change actually buys, and it is real.** On a chain that still has a producer, a bond
> becomes usable one block sooner — by the block that accepts the registration rather than by that
> block's child — and the pipeline stops disagreeing with the state machine about which state
> answers *is this bond registered* (see the next section, which was always the honest description
> of the change). That defect was real and is now closed. It is a narrowing, not a recovery
> mechanism.
>
> **What a real answer requires, stated so the next attempt does not repeat this one:** a block lane
> whose validity depends on nothing the chain has previously granted. That is what ADR-0060's
> heartbeat lane was, and it was withdrawn by the 2026-08-30 audit for four structural findings —
> so **trustless recovery from a total stop is an OPEN problem**, and this ADR does not solve it.
> Fact A below is unaffected: it is a theorem about what a chain can check, not about this fix.
>
> The staging item that would have caught this is in this document already — *"the bootstrap tool
> has been run on a real fleet, not reasoned about."* The fixture was never written, and the
> reasoning was wrong.

### This is not a new rule; it is the removal of a disagreement

`apply_palw_transition_v4` applies accepted objects at **step 3** and the block's own work at
**step 4**, and `apply_attempt` resolves the bond from the live fold. **The state machine already
accepts a block whose bond was registered in its own mergeset.** The only thing that refuses is the
pipeline pre-check, which resolves against the walk state at the selected parent. Two answers to
one question — the failure shape this project keeps meeting — and here it is the shape that stops
the chain.

The set is already built and already a consensus input: `palw_v2_bonds_declared_in_mergeset` feeds
`ctx.palw_v2_locked_bonds` on every chain block. The graft is to return the full records rather
than the outpoints.

### What the block is

*(Retitled: it is not a "recovery block" — see the correction above.)*

An **ordinary algo-6 block**. It carries a real attempt with a real inference, creates a real claim
with real escrow and exposure, earns the ordinary subsidy, adds `calc_work(bits)` like every other
block, and is refutable by a panel and a court. Nothing is relaxed except *when* its bond becomes
visible — and, corrected, that is one chain block earlier for a NEWCOMER JOINING A LIVE CHAIN: the
block that accepts the registration may attempt under it, instead of that block's child.

**The marginal cost of bonding-then-attempting versus pre-bonding is exactly zero** — same
collateral, same signature, same ticket, same inference, same ceiling. That is why it is not a cheap
permissionless mint, and it is a stronger argument than any rate limit, because there is no rate to
limit.

### Why it does not reproduce the four withdrawn findings

1. **Window poisoning** — no new lane and no new `bits`; there is no second bits population to
   average, so no fixed point can form.
2. **Weight parity** — no new work basis. The block's weight is `calc_work(bits)` in the ordinary
   currency, so Fact B never engages.
3. **Sibling width** — no slot rule is introduced, so there is no chain-versus-DAG gap. Width is
   bounded by what already bounds every producer: the collateral floor, one-key-one-bond, the
   exposure ceiling, the class ticket and the epoch budget.
4. **Horizon divergence** — depth ≤ 1 and no walk: the parent PALW state plus this block's own
   mergeset, both of which any node validating this block necessarily holds.

### The narrowing that must not be dropped

Move the **bond lookup** and nothing else. Inserting the *full* admission after the class retargets
would make admission item 6's strict `DerivedV1` equality compare against a post-retarget target
while the producer computed its pwu from the pre-retarget tip — so on a multi-class chain the first
block across every epoch boundary would be disqualified and the chain would stop every 1000 DAA.
testnet-11 runs three classes. A regression fixture pins this.

## What it costs

Supply, emission, coinbase rules, fork-choice weight, state root, ruleset id: **all unchanged**.
No version bump, no golden vectors, no re-mint. Node cost in normal operation is zero (the mergeset
extraction already runs; the admission call gains one `Option`).

**The real cost, stated plainly:** a restarted chain has a clock, a ledger and block production —
and **no finality** until six distinct operators exist, because `derive_panel_v2` refuses a short
draw. Until then every claim voids at `BindTimeout` and the worker carve of every block is
destroyed by don't-mint. Recovery restores liveness, not the economy. "The deadlock is closed" must
not be read as "the chain is back".

## The objection this exposes — and it is a P0 that exists TODAY

The red team's strongest attack is that the rescue procedure, run six times, manufactures a
`safe_frontier` on a private branch. The mechanism is real, and every link was verified here:

* post-genesis registration gates on `min_collateral_sompi` alone — **400,000 sompi (0.004 MSK)**,
  and the collateral is refundable;
* `write_bond(key, None)` has **no callers**: a bond never leaves the registry, retired or not;
* `registered_daa` is written and **read by no consensus gate anywhere** — there is no bond
  maturity and no soak;
* `PalwBondStatusV2` is `Active | Retiring`, so a bond is Active in the block that registers it;
* seat tickets are `H(anchor ‖ claim ‖ bond)` with both the anchor and the claim id
  attacker-influenced.

**But this proposal does not create it.** An attacker holding one bond registered at any time
before the fork point can already fork, carry sybil `BondRegistered` objects inside fork blocks
(they fold at the accepting block today, with no rule change), seat their own panels, self-license
and grow `safe_frontier`. `palw_fork_choice`'s stated invariant — *a fork nobody could see collects
no receipts, so it has no frontier* — **is already false for anyone who has ever paid 0.004 MSK**,
and the right is permanent and retroactive because bonds never leave.

That is a P0 in its own right and is tracked separately (seat maturity + frontier provenance —
`registered_daa` is precisely the unused field such a rule needs). This ADR must not ship as the
last word on it.

## Staging

* **Behind `Option<ForkActivation> palw_bootstrap_activation` on `Params`, TOP LEVEL** — never
  inside the V2 bundle. `consensus_identity_id` normalises top-level fences, so two builds that
  disagree only about a future height keep one identity and stay peers; a fence inside the bundle
  goes through `palw_ruleset_id_v2`, which is not normalised, and would be a deploy-day partition.
* **Ships dormant.** With the fence unset the pipeline passes `None` and behaviour is byte-identical
  to today: no fingerprint move, no re-mint.
* **Tests, in the real pipeline:** (1) fence armed — a self-bonded block validates and its successor
  is ordinary; (2) fence **off**, same DAG — the block is disqualified. A `const false` switch no
  fixture exercises is how all four heartbeat findings survived to the audit, so the switch is
  exercised in both positions; (3) an epoch-boundary block on a multi-class chain still validates
  (the §narrowing regression); (4) a self-bonded block merged into a *sibling's* chain is skipped
  and unpaid, deterministically.
* **Before switching it on:** the fence is armed at a future, *reachable* score while the network is
  still alive (a recovery rule fenced behind a score a dead chain cannot reach is withdrawn-finding
  1 in new clothes); and the bootstrap tool has been **run on a real fleet**, not reasoned about.

## Two things worth recording that nobody asked for

1. **The pipeline and the transition disagree today about which state answers "is this bond
   registered."** That disagreement is the deadlock, and it is a defect independent of this fix.
2. **`registered_daa` is a consensus field no consensus rule reads.** A recorded fact with no reader
   is either dead weight or a missing rule. Here it is the second.
