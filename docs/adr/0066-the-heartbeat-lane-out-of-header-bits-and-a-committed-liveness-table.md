# ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory

Status: **Decisions 1, 2 and 4's fence LANDED (2026-08-31), both dormant. Decision 3 and Decision
4's committed table remain PROPOSED.** Supersedes the *implementation* of ADR-0060 Decisions 1–2
and Decision 4; the doctrine those decisions state is unaffected. Both features still ship OFF —
but they ship off behind **fences that can be armed**, which is the difference this ADR was written
to make. See "What landed" at the foot of this document for exactly what is and is not in the tree.

**Neither part is a re-mint. Both must be activations, and both must be fenced at TOP LEVEL.**

> **Arming and closures after the Status line (index reconciliation, 2026-09-02).** "Both
> dormant … ship OFF" was the 2026-08-31 state. [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)
> arms `Params::palw_heartbeat` from genesis on testnet-11 (Relaunch 5 onward) and devnet, closes
> Decision 3 (attempt blue work leaves `calc_work(bits)` — `Params::palw_attempt_work`, a 2²⁰
> constant against ε = 1) and closes F3a (width bound: at most four heartbeats per mergeset, or any
> number forming one chain — its F5 amendment). Decision 4's fence `Params::palw_inactivity_leak`
> is still `None` everywhere and the committed table is still unbuilt. The sentence "a V2 network's
> doctrine is to re-mint rather than schedule" is the testnet practice, not the doctrine: consensus
> changes ship by activation (mainnet), and [ADR-0072](0072-the-ticket-is-the-execution.md) §3 records
> the activation shape. Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: Decision 4's table must be verified from the pruning-point snapshot or no leak is computed; the leak is monotone with hysteresis; it never lowers the denominator below `min_active_validators`; `t_leak_daa` enters the identity raw before arming.

## Why the first implementation failed, sorted by cause

The audit recorded four findings. Sorting them by *mechanism* rather than by symptom is what makes
the redesign tractable, because only two of them are about `bits`:

| # | finding | cause |
|---|---|---|
| F1 | the lane can price the bonded lane off its own chain, permanently | **`bits`** |
| F2 | ε = 1 is not small against a V2 block worth 2 | the shared **blue-work** scale |
| F3a | sibling heartbeats share one POV, so nothing bounds their width | independent |
| F3b | the retarget can never rise above its floor | **`bits`** |
| F4 | the evidence walk terminates on a node-local fact | independent |

F1 is the fatal one and it is purely a consequence of the price living in the field the global
difficulty window averages. A V2 network runs at `MAX_DIFFICULTY_TARGET` because the class lottery,
not the hash target, is its throttle — so a window of heartbeat rows demands work 33,554,432 and no
bonded block can re-enter it. The fixed point is a heartbeat-only chain recoverable only by re-mint:
the self-feeding refusal ADR-0060 was written to abolish, reintroduced by its own remedy.

## Decision 1 — the lane gets its own algorithm id, and its price never touches `bits`

Four carriers were considered. Three are unusable:

* **A new header field** works and moves no genesis hash (the `palw_state_root` precedent shows a
  double-gated field leaves every existing preimage byte-identical), but it changes `Header`, the
  p2p conversion and the RPC model, and old binaries compute a different identity for the new
  headers. Cost without a matching benefit.
* **`header.palw_commitment`** is **disqualifying**: it is excluded from every PoW digest, so a
  cadence datum carried there is post-PoW and one solved header mints unbounded sibling identities.
  A cadence datum must be pre-PoW.
* **The coinbase payload** is body-level, and bodies arrive after headers. A price that cannot gate
  header relay makes header spam free.

**So: a new `POW_ALGO_ID_HEARTBEAT_V1`, whose target is a network constant rather than
`header.bits`.** Heartbeat headers carry the global expected bits like every other lane, so they
enter the difficulty window as ordinary rows and F1 and F3b both disappear — not as a tuning, but
because the quantity that fed back on itself is gone.

The current file argues against a new id ("it would need a new finalizer arm for identical bytes").
That arm is ~8 lines, and it buys three things: every rule stops being triple-gated on
`(id == 3) && ConsensusV2 && CONST`; a hash-network algo-3 solution can never be replayed as a
heartbeat, because the Layer-0 digest binds `pow_algo_id`; and the id becomes the fence's own
observable.

## Decision 2 — the slot rule is one block deep, and the evidence walk is deleted

F4 is not a tuning problem. `heartbeat_evidence`'s walk terminates on `Err(get_header) => break`,
which is a **node-local** fact: an archival node never hits it and a pruned node hits it at its own
pruning point. Two honest nodes then compute different verdicts for the same header — a partition
along the `--archival` flag.

A retarget is what needs ancestor evidence. With Decision 1 there is no retarget, so the slot rule
becomes a function of the selected parent alone: *is this heartbeat far enough after the parent's
timestamp*. `consensus/src/processes/heartbeat_evidence.rs` is deleted, not bounded.

F3a — sibling width — is **not** fixed by this and must be stated: the slot rule bounds the chain,
not the DAG. What bounds width is the price, which is now a fixed target rather than a floor the
retarget cannot leave.

## Decision 3 — ε stops competing with a V2 block's work

F2 is independent of `bits` and survives Decisions 1–2 untouched: ε = 1 against a bonded block worth
2 is parity at `ghostdag_k = 1`. The fix is that a V2 attempt block's blue work must not be
`calc_work(bits)` — the attempt lane's throttle is the class lottery, so its work should reflect the
inference it carries, not the hash target it did not need.

**This is the expensive decision.** It moves `header.blue_work` on every V2 block, so it needs its
own soak, its own golden vectors and a re-derivation of the pruning proof's level argument. It is
separable from Decisions 1–2 and should be staged after them.

## Decision 4 — the inactivity leak needs committed per-validator state

The leak must know how long EACH validator has been inactive. Today that lives in node memory
(`last_attestation_daa_by_validator`), which cannot be right: a value that decides block validity
and is not committed is a value two nodes can disagree about with no way to notice.

It becomes a per-validator table inside the DNS overlay's committed state, rooted into
`overlay_commitment_root` and riding `PruningPointOverlaySnapshot` so a pruned IBD imports it under
the existing trustless gate. Below the fence, and with an empty table, the root is byte-identical —
**no genesis hash moves on any preset**. `component_digests` gains a third digest, or the triage
line loses the ability to localise a divergence to the new half.

Two constraints that are easy to get wrong:

* **Entries for validators with no surviving bond must be dropped at capture**, or the table grows
  without bound and a from-genesis node disagrees with an importing one.
* **The branch-comparison prohibition stays structural.** `dns_reorg_outcome` must keep passing an
  empty leak view: a committed table does not fix the quorum-intersection problem, because a
  candidate branch commits its *own* table.

## The trap in the constant, and why both fences must be top level

`inactivity_leak_daa` lives inside `DnsParams`, which `consensus_params_id` hashes as one raw borsh
blob while `for_each_fence` deliberately does not visit it. Both halves are individually correct.
Together they mean **changing that constant from `u64::MAX` to a live value moves
`consensus_identity_id` and disconnects the first upgrading operator from every un-upgraded peer
immediately** — the deploy-day partition the identity split exists to prevent.

So the leak cannot be armed by editing that constant. It needs a top-level
`Option<PalwInactivityLeakV1 { activation, t_leak_daa }>` — ADR-0065 D1's exact shape, for its exact
reason: `for_each_fence` visits only the activation, the collapse takes the whole `Option`, and the
duration reaches `consensus_params_id` raw. `DnsParams.inactivity_leak_daa` is then retired at
`u64::MAX` permanently rather than reused.

The same applies to the heartbeat lane. Its current shape — a `const bool` that changes block
validity and moves no fingerprint — is **code-only masquerading as a rule**, which is a silent fork.
It becomes a top-level `Option<PalwHeartbeatParamsV1>`; the new algo id, the work constant and the
slot rule may not go into the V2 bundle, because the bundle's fences live under
`palw_ruleset_id_v2`, and a V2 network's doctrine is to re-mint rather than schedule.

## What this costs, honestly

**Contained (days):** deleting `heartbeat_evidence` and reshaping the slot rule (~120 lines out, ~30
in, two call sites); forcing heartbeat blocks to carry no chain position; relocating the leak fence
out of `DnsParams`; retiring `PALW_HEARTBEAT_LANE_ENABLED` for a params predicate (7 call sites).

**Multi-week:** the new algo id (finalizer arm, `check_algo_id_known` and its pruning-proof
implications, `accepts_algo_id`, `required_algo_id_for_mode`, the miner, the template adapter, and
the fingerprint fixtures for all five presets); Decision 3's blue-work change; and Decision 4's
fourth snapshot component with its apply/revert pair, reorg-symmetry tests, pruning-point capture
filter and pruned-IBD import tests.

Nothing here should be landed as a switch flip. ADR-0060 §12 already listed "what a correct
heartbeat lane needs" and "correct activation needs PERSISTED per-validator last-attestation state";
landing either without recording which of the four findings it closes, and how, would repeat exactly
the failure that produced them.

## Consequences

* Both features stay OFF until their fences exist and are armed. No preset fingerprint moves while
  they are `None`, so this ADR costs nothing to adopt and everything to arm carelessly.
* F3a (sibling width) is **not** closed by Decisions 1–2 and is recorded as open.
* The heartbeat lane remains the only known answer to trustless recovery from a total producer
  stop — ADR-0064's correction establishes that its own remedy does not close that, so this ADR is
  on the critical path for a property the chain currently does not have.


## What landed, 2026-08-31 — and what did not

**Landed. Decisions 1 and 2, in full, plus a fence for Decision 4.**

*The lane's price left `header.bits`.* `POW_ALGO_ID_HEARTBEAT_V1 = 8` exists, `check_algo_id_known`
admits it, and `StateLayer0::new` substitutes `Uint512::MAX >> PALW_HEARTBEAT_WORK_LOG2` for the
target on that id. A heartbeat header carries the GLOBAL expected bits like every other lane's, so
heartbeat rows enter the difficulty window as ordinary rows. F1 and F3b are gone as arithmetic. The
substitution is in the one place every PoW path shares — ordinary validation, the pruning proof,
trusted import — so no caller can price the lane by forgetting to. The tag arm is algo-3's, shared
rather than copied, and the two lanes cannot borrow each other's solutions because the Layer-0
digest binds `pow_algo_id`.

*The evidence walk is deleted.* `consensus/src/processes/heartbeat_evidence.rs` is gone, both call
sites with it. `check_heartbeat_slot` now takes `(selected_parent_timestamp, selected_parent_algo_id,
header_timestamp)` — one header the caller already holds. F4, the `--archival` partition, cannot
recur because there is nothing left to walk.

*The ramp has two steps, not three.* The middle step asked "how long has the bonded lane been
silent", which is ancestor evidence — F4 in one question. One block deep distinguishes exactly two
states and they are the two that matter: the chain is producing (nominal hour) or it is not
(recovery cadence).

*Both features became fences.* `Params::palw_heartbeat: Option<PalwHeartbeatV1 { activation,
work_log2 }>` replaces the `const bool`; `Params::palw_inactivity_leak: Option<PalwInactivityLeakV1
{ activation, t_leak_daa }>` replaces `DnsParams.inactivity_leak_daa`, which is now retired at
`u64::MAX` permanently and pinned there by a test. Both are wired at all seven identity sites and
are `None` on every preset, so **no shipped fingerprint moved**.

*One defect found while landing it, recorded because the shape recurs.* `algo_id_carries_no_chain_position`
answers two questions at once — block LEVEL and blue WORK — because for the receipt lane both
answers are the same. For the heartbeat they are NOT: level must be zero (a constant target makes a
lucky solve indistinguishable from a hard one, so `calc_level_from_pow_512` would read luck as
hierarchy) but blue work must be ε, not zero (the regime the lane exists for is total collapse,
where every block is a heartbeat and zero-work branches all tie). Folding the heartbeat into the
shared predicate made the ghostdag zero-arm return before the ε-arm was reached, so the lane weighed
nothing and a collapsed chain could not order its own branches. Split into
`algo_id_derives_no_block_level`, and pinned by a test that asserts the two predicates DISAGREE for
this id.

*The integration test runs now.* `palw_heartbeat_blocks_tick_the_clock_and_weigh_epsilon` used to
`return` early because the lane was a `const bool` set to false — a test that could only be run by
rebuilding the binary, which was also the only way to run the feature. It arms the fence and
executes: the adapter, the ε credit, both slot intervals, the fee-only coinbase, and the assertion
that carries this ADR — **the heartbeat's `bits` equal the bits the ordinary template already had**.

**Not landed, and not started.**

*Decision 3 (ε against a V2 block's work).* Unchanged and still correct: on a V2 preset
`calc_work(0x207fffff) = 2`, so a heartbeat is worth half a bonded block and `ghostdag_k = 1` makes
that parity. The fix moves `header.blue_work` on every V2 block and needs its own soak, its own
golden vectors and a re-derivation of the pruning proof's level argument. Deliberately staged.

*Decision 4's committed per-validator table.* The FENCE landed; the TABLE did not. And one
correction to this ADR's own text, made after reading the code rather than the design: it says the
per-validator state "lives in node memory (`last_attestation_daa_by_validator`), which cannot be
right: a value that decides block validity and is not committed is a value two nodes can disagree
about with no way to notice." The premise is wrong. The table is built by
`last_attestation_daa_by_validator(&contributions, &epoch_anchor_daa)` from the contributions
`collect_stake_contributions_v2` gathers on a walk from the tip, bounded by
`dns_params.stake_score_window_blue_score` — a CONSENSUS parameter, identical on every node. So it
is already a pure function of (tip, params), reorg-safe by the same argument `recompute_epoch_tallies`
uses, and two nodes holding the same tip compute the same table. The "node memory" this ADR feared
is not there.

What IS still worth committing is narrower, and it is the half this ADR got right: a node that
starts from a pruning point has no window to walk. `PruningPointOverlaySnapshot` is how the other
overlay components cross that boundary, and the leak table has no equivalent — so a pruned-IBD node
cannot reconstruct it, and if the leak were armed it would judge liveness from an empty table.
Closing that still needs the fourth snapshot component, the capture-time filter for validators with
no surviving bond, and the pruned-IBD import tests; it no longer needs the apply/revert pair this
ADR budgeted for, because there is no incremental state to revert. Because the leak ships dormant,
nothing depends on it today.

*F3a (sibling width).* Still open, as this ADR recorded. The slot rule bounds the chain, not the
DAG. What bounds width is the price, which is now a fixed 2²⁴ per block rather than a floor a
retarget could never leave — better, but not a bound.

**The standing warning is unchanged, and SA-4 below makes it legible rather than removing it.**
Arming either fence is a coordinated change: `work_log2` and `t_leak_daa` reach
`consensus_params_id` but not the fence visitor, so two operators scheduling one height with
different values share an identity, peer, and disagree the moment the fence fires. Since SA-4 they
also announce different `consensus_schedule_id`s, which is what the `flow_context` warning prints —
so the disagreement is visible before it fires instead of inferable after. For the heartbeat price
there is a second lock — the binary refuses to start if the fence names a `work_log2` it does not
implement — because that value has a second source in `StateLayer0`. `t_leak_daa` has no second
source and therefore no such lock.

## Security amendment (2026-09-02) — Decision 4's committed table, before it is built

**SA-1 — The table is part of the pruning-point's authenticated snapshot, and a node that cannot
verify it computes no leak.** A pruned-IBD node derives quorum denominators from a table it did not
compute; if that table is not committed and verified, archival and pruned nodes exclude different
validators and the finality overlay partitions along the `--archival` flag — the F4 shape this ADR
deleted from the heartbeat lane. Fail closed: no verified table ⇒ full denominator ⇒ no leak.

*As built (2026-09-03).* The self-computed half is in
`VirtualStateProcessor::palw_leak_table_provenance`, and it asks the WALK: it iterates the same
backward chain the table is built from and answers `SelfComputed` only if every header from the tip
to past the far edge of `stake_score_window_blue_score` is one this node holds. The decision itself
is the pure `dns_finality::leak_table_provenance_from_walk_v1`, unit-tested.

Two corrections to what an earlier draft of this amendment said, because both were wrong in the
dangerous direction. First, the check must not be "is the consensus pruning point a window below the
tip": that value is derived from the chain, so it is identical on an archival node and a pruned one
and cannot see the node-local divergence SA-1 exists to close. Second, **arming the leak on a pruned
fleet is not a no-op.** A pruned node in steady state holds every header above a pruning point far
below its tip, so it covers the window, answers `SelfComputed`, and leaks — correctly, and exactly
as its archival peers do. `Unverified` is for a node that genuinely cannot reach the far edge
(mid-IBD, or a store truncated above the window). The verified-IMPORT half remains unbuilt: the
fourth `PruningPointOverlaySnapshot` component this decision budgets for does not exist, so such a
node stays on the full denominator rather than guessing.

**SA-2 — The leak is monotone with hysteresis.** Exclusion after `t_leak_daa` of silence;
re-inclusion only when the validator's fresh attestation is itself final. A validator flapping
around the threshold must not swing the quorum on a block of its choosing.

**SA-3 — The leak never lowers the denominator below `min_active_validators`.** If it would,
finality halts (no certificates) rather than continuing as a small-quorum overlay. ADR-0060
Decision 4 accepted double-finality risk under a long partition; it did not accept a two-validator
quorum, and the floor must be a rule.

**SA-4 — A value that rides a fence must be VISIBLE to the operator comparing two builds.** Two
builds that peer with different `t_leak_daa` and then diverge on finality is the
`inactivity_leak_daa` accident under a new name.

*As built (2026-09-03), and this is a decision that had to be made rather than a bug that had to be
fixed.* The amendment's original wording — put the value in `consensus_identity_id` raw, past the
fence visitor — was implemented and then withdrawn, because it buys the wrong half of a trade that
cannot be had both ways:

* `consensus_identity_id` is a **gate**: `flow_context` refuses a peer whose identity differs.
* The rolling deploy needs `id(None) == id(Some(H, v))` — a build that schedules a fence must peer
  with a build that has never heard of it. The value-in-the-identity rule needs
  `id(Some(H, v₁)) != id(Some(H, v₂))`. For any equality-compared fingerprint those two are
  contradictory by transitivity. One of them has to go.

The rolling deploy is what stays, for two reasons and both are recorded in `params.rs`:

1. **It is a procedure this project has executed.** The ADR-0068 Phase-1 rollout of 2026-09-01
   scheduled `palw_heartbeat` (work_log2 24, max_per_mergeset 4) and `palw_attempt_work`
   (work_log2 20) at DAA 5,000 with the live testnet-11 tip at 1,746, and rolled the fleet host by
   host *because* "a scheduled fence is normalised out of `consensus_identity_id`". Under a gate on
   the value, the FIRST host to restart is disconnected from every host that has not — a partition
   thousands of DAA before the fence, on a fleet with no way to converge except stopping all of it
   at once.
2. **For the leak specifically it would undo the reason the fence exists.** `palw_inactivity_leak`
   was lifted out of `DnsParams` precisely because that field could only be armed by a flag day, and
   the moment a network needs finality to self-heal after validator loss is the moment it cannot
   coordinate one. A gate on `t_leak_daa` puts the flag day back.

So the value goes in **`consensus_schedule_id`** — reported, never gated — which is the answer
ADR-0065 D1's `window_daa` already gave and which `params.rs` documents at that call site. That is
the id `flow_context` prints in the warning it already emits whenever two peers' params ids differ
and their identities agree, and folding the value in is what makes that warning able to name the
disagreement it is the only defence against; without it, the two builds SA-4 is about printed two
identical schedule ids, which says "these agree" about the one thing they do not.

**The rule, stated so it can be applied to the next such value without re-litigating this:** a value
that rides a fence goes in `consensus_params_id` (it always did) and in `consensus_schedule_id`
(SA-4). It goes in `consensus_identity_id` only through the fence being ACTIVE AT GENESIS, where two
builds disagree about block 1 and the handshake is right to refuse. Nothing scheduled belongs in the
gate — not the height, and not the value beside it. This covers `palw_heartbeat.{work_log2,
max_per_mergeset}`, `palw_attempt_work.{work_log2, ticket_bucket_log2}`,
`palw_inactivity_leak.{t_leak_daa, reentry_final_depth_daa}`, `palw_bond_maturity.window_daa`, and
the beacon fold width `k`, whose placement in `consensus_schedule_id` is therefore correct as
shipped.

What this does NOT buy, said plainly: arming a value-carrying fence is still a **coordinated**
change. Two operators who schedule one height with different values will peer, sync, and diverge
when it fires. The warning is the whole of the defence, and the schedule id is what makes the
warning true.
