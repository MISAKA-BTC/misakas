# ADR-0068: The LLM-primary economy — the floor retires to the doctrine's minimum

- Status: Accepted; **Phase 1 implemented AND drilled 2026-09-01**. F3a and F2 are closed in
  code (fences shipped OFF everywhere but devnet, which arms them as the standing drill
  network); the deterministic drill (`the_heartbeat_clock_sweeps_a_stopped_chain_back_to_life`)
  releases the block-600 wedge in 4,300 clock ticks; the live two-node drill
  (`scratchpad hb-drill/REPORT.md` of the implementing session) PROVED the lane on real
  processes — a zero-bond devnet born and bootstrapped over heartbeats, the nominal hour held
  to the second twice, 120 s recovery cadence, ε = 1 vs 2²⁰ priced exactly as declared, and
  unattended producer re-entry twice. Drill findings: **F1** (producer/heartbeat-miner SIGTERM
  hang — fixed on this branch, the panel's `tick()` copied to both), **F2** (no cadence pacing
  at genesis-permissive targets — first-epoch mint runaway on fresh nets, Phase 2 item),
  **F3** (a dying producer accepts its own block after its P2P server stops — narrowed by the
  F1 fix, residual noted), **F4** (the V2 assembly violates upstream pruning depth-consistency,
  and LIVE testnet-11 does at 6,600 = 11 × 600 — devnet fixed, t11's correction rides the
  Phase 2 flag day), **F5** (a heartbeat CHAIN deeper than `max_per_mergeset` strands
  permanently once a heavier bonded fork puts it in the anticone — **closed the same day by
  amending the width rule**: at most four heartbeats, or any number provided they form ONE
  chain (sorted by blue score, every adjacent pair ancestor-related). A chain is the lane
  doing its job through a long outage, already rate-priced by the slot ladder; width —
  siblings, and the tree shape that would fool a chain-HEAD count — is what F3a is about, and
  the pairwise total order refuses both. The rule's semantic version rides the fingerprint
  label (`palw_heartbeat/width-chain-exempt-v2`), so only armed presets moved: t11's
  scheduled fingerprint 05df4e5e… → 0533c8ee…, devnet's 47ba789e… → 280db9e1…. The drill's
  exact strand shape is the regression test).
  **F6 (the launch audit's finding, fixed before the train left):** the heartbeat MINER
  consulted `should_mine`, which folds in `is_nearly_synced` — and the sync-rate escape that
  clause leans on itself expires once the finality point is over three finality durations old.
  So the longer a chain had been stopped (or the older a fresh re-genesis's timestamp), the
  more firmly the clock refused to start: the ADR-0060 §1 self-referential hostage, wearing a
  mining-heuristic costume, masked in the drill by `--enable-unsynced-mining` on the H node.
  The miner now holds only on what a stalled chain can still supply — peer connectivity, the
  chain-participation gate, and the transitional-IBD check — and its first tick is what makes
  every unmodified producer's `should_mine` pass flag-free again.
  Phase 0 is operations and began the same day. **Phase 2 is implemented on this branch
  (`palw-adr0068-phase2`, the Relaunch 5 identity `d7510c7a…`)**: the floor reserve
  500‰ → 20‰ (one field, every V2 network), genesis shares 489/489 so the floor lands
  exactly on its reserve, both fences armed from genesis (mainnet's assembly arms the same
  way the day its card is pinned), and F4's depth-consistency nudge moved into the shared
  assembly. Note the measured correction to Phase 0's own text: the LIVE reserve was 500‰
  (`BASE_CLASS_RESERVE_PERMILLE`, which the bundle builder writes over the struct default of
  100), so the walk could never pass the half-table on the running chain — the 90% figure was
  only ever reachable through this train. Execution = `docs/testnet11-relaunch5-runbook.md`,
  gated on the Phase 1 fleet deploy and the operator's re-genesis go (every host wipes;
  seat 5 re-keys off the lost 160.16 host, returning unmanned seats to ≤ 2).
- Date: 2026-09-01
- Depends on: ADR-0045 (class economy: block-denominated budgets, share table),
  ADR-0054 (share follows production), ADR-0058 (merged work is counted),
  ADR-0060 (the liveness doctrine), ADR-0064 (trustless recovery), ADR-0066 (heartbeat out of
  bits — findings F2 and F3a, which this ADR closes)
- Amends: testnet-11's de-facto floor-majority cadence (share table 550/200/250) and
  `min_base_class_share_permille = 100`.

> **Deployed (index reconciliation, 2026-09-02).** The Status above records Phase 2 as
> "implemented on this branch … gated on the operator's re-genesis go". The go was given: Relaunch
> 5 shipped the LLM-primary table from genesis (`fe8a7284`), and testnet-11 has run it through
> Relaunch 5c, 5d and 5e — floor reserve 20‰, genesis shares 489/489/22, heartbeat lane and
> attempt-work constant armed from genesis (`palw_rc_arm_phase1`; the Phase 1 sentence "fences
> shipped OFF everywhere but devnet" is the pre-train state). What later ADRs added on top of this
> table: [ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (a class holds that share
> only while its family is certified end to end), [ADR-0072](0072-the-ticket-is-the-execution.md)
> (the lottery priced in inferences, so F2's constant is compared against a per-inference draw) and
> [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) (each class's
> attempt-lane target seeded from its own share and work — the 5d measurement that a shared seed
> gave the floor 98.5 % of blocks against this table). Map: [`README.md`](README.md).

## 1. Goal

Rewards AND block production come from LLM computation by default; base (model-free)
computation exists only for the minimum the liveness doctrine requires. End-state on a
three-class network:

| axis | floor (BASE-0) | LLM classes | mechanism |
|---|---|---|---|
| time (clock) | — | — | heartbeat lane (armed): ε-weight, fee-only, ramps on silence |
| blocks / issuance | 2% | 98% | share table (blocks × the same subsidy carve) |
| chain weight | ~0.01% | ~99.99% | pwu (unchanged) |

## 2. What was already true (measured, not designed here)

* **Issuance follows block counts.** The subsidy carve is class-independent (ADR-0042 D10:
  a carve of the fixed emission, never an addition) and the epoch budgets are "blocks, never
  pwu" (`derive_epoch_budgets_v2`). The share table is therefore THE issuance lever, and pwu
  is chain weight — security, reorg depth, slash exposure — not pay.
* **Liveness does not need the floor's share.** Three shipped mechanisms carry it: the census
  denominator (an idle class's budget redistributes), S-04 (over-budget blocks are merged but
  unpaid — the floor can advance DAA past a dying epoch at zero issuance cost), and the
  heartbeat lane (ADR-0060 D1 / ADR-0066) once armed.
* **Emergency full pay needs no new rule.** If every LLM class dies, the next epoch's census
  hands the floor ~the whole block budget at full pay — the ambulance wage emerges from
  ADR-0045 arithmetic.

## 3. The three phases

**Phase 0 — walk the table, don't fork it (operations, no consensus change).** Run the LLM
producers persistently; ADR-0054 walks the floor 550‰ → 100‰ (its current minimum) as LLM
production sustains. The floor producer keeps running through its unpaid over-budget tail —
that tail IS its liveness readiness. *(Begun 2026-09-01: node0 restored to the QWEN36
producer class; C's seat 2 doubles as the QWEN25-A16 producer, because node-b — the class's
producer and panel seat 5 — is dark, which is also why the void rate is elevated: three dark
seats where ADR-0065 assumed at most two.)*

**Phase 1 — arm the clock before shrinking the reserve.** Close ADR-0066's two leftovers in
code (§4), then arm `palw_heartbeat` and `palw_attempt_work` on testnet-11 **by a rolling
preset update**: both fences follow the fence discipline — a scheduled activation keeps
`consensus_identity_id`, so old and new builds stay peers until the fence fires, and the
locked values (`work_log2`, `max_per_mergeset`) refuse to start a binary that does not
implement them. Run the bondless heartbeat miner (one flag) on ≥ 2 independent hosts. Then
the drill, on a devnet clone: kill every producer AND the floor; verify the ramp
(1 h → 120 s), re-entry transactions riding heartbeats, epoch closure, census hand-back, and
unattended recovery. **Phase 2 is gated on this drill passing.**

**Phase 2 — the floor's minimum drops 100‰ → 20‰ (coordinated re-mint).**
`min_base_class_share_permille` is inside the V2 bundle and therefore inside the identity, so
this is testnet-11's next re-mint train, not a fence. With the clock armed, the 10% reserve
defends nothing the census + heartbeat do not already defend; 20‰ (the grant floor admits
≥ 1‰) keeps the floor as a permissionless entry ramp (one floor block funds ~10⁵ minimum
collaterals), the artifact-less KAT class for the dispute machinery, and the census's
expansion seed. Mainnet preset: genesis share table {floor 20‰, LLM 980‰},
`min_base_class_share_permille = 20`, both fences armed from genesis.

**The floor is never withdrawn.** ADR-0053 measured what withdrawal costs; the floor is the
one class with no artifact to lose and the one class every seat verifies by construction. It
shrinks; it does not leave.

## 4. Phase 1's two closures (implemented on this branch)

### F2 — the attempt lane's blue work leaves `calc_work(bits)`

`Params::palw_attempt_work` (`PalwAttemptWorkV1 { activation, work_log2 }`): under the fence
an algo-6 block's blue work is the constant `1 << PALW_ATTEMPT_BLUE_WORK_LOG2` (2²⁰), maxed
with `level_work`, at every proof level (threaded through the pruning-proof managers the same
way ε is). The audit's finding: on a V2 preset the ambient bits price every bonded block at
2 — parity with two ε = 1 sibling heartbeats for ~280 kH/s.

**A constant, deliberately NOT the envelope's claimed pwu.** The claim is verified against
class state, class state lives on the selected chain, and GHOSTDAG holds only the header. A
claim-derived work would let a shape-valid header that never faces the lottery mint
fork-choice weight with a number; the constant keeps the spam/honest ratio exactly where it
is today while fixing the one ratio F2 is about. Per-class weight (QWEN36 vs floor) is not
this layer's job either — that is the pwu-verified PALW chain weight (`safe(C)`, ADR-0058),
which only counts what the chain actually checked. The pruning-proof level argument is
undisturbed in structure: per-block work is still a constant across the lane, only larger,
and level assignment (digest zeros) never depended on it.

### F3a — sibling heartbeat width is bounded where `mergeset_size_limit` lives

`PalwHeartbeatV1` grows `max_per_mergeset` (locked to
`PALW_HEARTBEAT_MAX_PER_MERGESET = 4`): a valid block's mergeset — selected parent included —
holds at most four heartbeat blocks, **or any number of them provided they form ONE chain**
(F5's amendment: sorted by blue score, every adjacent pair ancestor-related; a blue-score tie
is never ancestor-related and fails). `RuleError::MergeSetTooManyHeartbeats`, checked beside
`check_mergeset_size_limit`; POV-independent, no walk, no window — over the flat bound it
costs one reachability query per adjacent pair. The template builder decides the same
predicate over its accumulated set (`heartbeat_set_admissible`) and chunks sibling floods:
four this block, the rest against later blocks' fresh mergesets — while an outage CHAIN of
any depth is absorbed whole, which is what un-strands the drill's F5 shape. A chain is
rate-priced by the slot ladder already; width — siblings, and the tree shape that would fool
a chain-HEAD count — is what this bound is for, and the pairwise total order refuses both.

What the bound does NOT claim: relay/storage of never-merged siblings stays priced by the
2²⁴ header cost, as before. The bound removes their *consensus* influence — DAA, mergesets,
blue sets — which is what "unbounded valid blocks at a permanently fixed price" was about.

## 5. Considered and rejected

* **Claim-derived attempt work** — rejected above (weight minted by a number).
* **Per-class pay discount** (`base_class_pay_permille`, don't-mint the remainder): at a 20‰
  share the floor is already 2% of issuance; a second lever on the same axis buys < 2pp of
  purity for a new consensus mechanism and audit surface. One lever per axis.
* **Zero-pay floor**: an unpaid backbone is an assigned duty (re-centralizes to staff), and
  the entry ramp dies. The heartbeat lane is the correct zero-pay liveness instrument.
* **Removing BASE-0**: the heartbeat is fee-only and ε-weight — on a young chain it pays
  ~nothing (no faucet role) and weighs nothing (no weight-flow through an outage); and the
  dispute machinery loses its CI-runnable class.

## 6. Invariants to verify at each step

* I1 (seat capacity): at 98% LLM cadence ≈ 1 claim / 2 min sustained; seat artifact rollout
  precedes each share step (the 2026-08-28 slash incident generalizes: at LLM-majority
  cadence, seat coverage is chain-critical).
* I2 (escrow float): ~all issuance becomes Final-gated. Document miner cashflow; collateral
  derivation already prices claim lifetime.
* I3 (mid-epoch death): floor budget exhausts in a dying epoch → unpaid floor blocks + the
  heartbeat ramp close the epoch; census restores paid floor cadence next epoch. Drill it.
* I4 (finality during outage): weight-flow pauses on a heartbeat-only chain; the inactivity
  leak (ADR-0060 §6 / ADR-0066 D4) covers the overlay. No new rule.
* I5 (walk rate): 550 → 100 takes multiple epochs under the retarget clamp — fine on t11;
  new networks write the end-state table into genesis instead of walking to it.
