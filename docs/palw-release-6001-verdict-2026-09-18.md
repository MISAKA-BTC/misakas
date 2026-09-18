# The 6,001 release: what was gated, what was measured, and the verdict

**Release candidate.** The bundle audited as `05169552` plus the fixes that audit and its own
re-audits produced. Fingerprint `48d1e31feefce318ae2596bfa0083a83ec9c232437f96d77c0d0c1ce42776de4`;
schedule `1150, 1900, 2150, 2400, 3500, 4000, 6300, 6301, 6400, 6501, 6900`.

This is the third report on this bundle and the only one that decides. The first
([the pre-arming audit](palw-audit-2026-09-18-6001.md)) found two Critical and six High and said
NOT SAFE. The second ([the DAA-clock audit](palw-daa-clock-audit-2026-09-18.md)) is why ADR-0138
exists. This one checks the gates the operator set on the frozen candidate and answers one question:
does arming at 6,001 introduce a new Critical or High?

## 1. What changed after the candidate was frozen, and why each change was allowed

The freeze forbade new features. Five changes landed; none is one.

| commit | what | why it is not a feature |
|---|---|---|
| `ccde5885` | the heartbeat re-entered the DAA clock; the four release gates | fixes a freeze the candidate would have shipped |
| `4006bfee` | the DNS leak's evidence window covers the DAA span it is decided over | fixes a leak that could never fire |
| `d8efa139` | a V2 possession proof is no wider than the challenge names | the diff audit's one new High |
| `38f977e2` | a heartbeat ticks only where nothing `bits` priced was merged | fixes a double-count the previous fix created |
| `e31b7357` | the 390 M ceiling measured, ADR-0139 §3a | a measurement and an `#[ignore]`d bench; no rule moves |

Two more are drill and documentation only: `867cb476` and `f5c77d07` give the drill a hash lane and a
heartbeat lane, `1c4cb36d` and `17d22232` bring the operator pages and the ADR in line with the rule
that actually shipped. No consensus rule, constant or fence moves in any of the four.

## 2. The three holes this bundle's own fixes opened, and how each was found

Every one was found by re-auditing a fix rather than the bundle. This is the pattern worth naming:
a fix that closes a real hole is the most likely place for the next one, because it is the code
nobody has yet read adversarially.

* **The DAA freeze.** The first cut of ADR-0138 exempted every lane `bits` does not price, the
  heartbeat included. A chain whose hash lane stops still beats, so its score would have frozen
  while blocks kept coming — every fence, deadline, retention and leak window with it. Found live:
  the registry drill sat at virtual DAA 20 with 183 blocks accepted.
* **The heartbeat double-count.** Counting the heartbeat unconditionally is the opposite error: on a
  chain whose priced lane runs, a beat beside every tick paces the clock twice as fast. The shipped
  rule is per mergeset — a heartbeat gives back one exemption only where nothing priced was merged.
  The justification first written for this bullet was itself wrong, and reading it again is what
  uncovered §6: it argued that testnet-11's parents are algo-3 blocks and therefore take
  `heartbeat_interval_ms`'s 120-second recovery branch. The chain says the opposite. The parents are
  algo 6, the branch taken is the NOMINAL one hour, and the danger was never a fast clock.
* **The DNS leak's units.** ADR-0128 decides the leak in DAA and walked its evidence in blue. Once
  the DAA clock slowed, the blue-bounded walk reached back fewer DAA than the leak is decided over,
  and no bond would ever have been leaked.

## 3. The operator's gates

**ADR-0138 re-audited against every DAA increment site.** `internal_calc_daa_score` is the only
arithmetic that produces a score, and both callers — the template's
`calc_daa_score_and_mergeset_non_daa_blocks` and validation's `block_daa_window` → `calc_daa_score` —
end in it, so a header claiming otherwise is refused by `check_difficulty_and_daa_score`. The
exemption is arithmetic on the score alone and not a `mergeset_non_daa` membership, so the coinbase
still pays exempt blocks and the PALW fold still folds their claims.

**The difficulty retarget and the DAA clock now count the same blocks.** `calculate_difficulty_bits`
has excluded the heartbeat, receipt and attempt lanes since ADR-0083 and ADR-0132 S. Until ADR-0138
the DAA score counted all three. That disagreement between the two clocks IS the window shrink the
clock audit measured; past 6,001 both count only what `bits` prices. A heartbeat header carries the
global `bits` and its own constant target is applied at validation, so an unpriced row never pollutes
the average target either.

**What is still counted in blue, and what that means.** Finality (360), merge depth (30), pruning
(~900) and the DNS attestation epoch (100) are blue-score windows. The model lane still paces blue,
so their wall-clock length is about half what the anchor cadence alone would give. None is a
chain-split risk, finality moves in the safe direction, and the numbers are in §9 of the clock audit.
This is named here because "we fixed the DAA, so the time problems are gone" is the wrong conclusion
to draw from ADR-0138, and it is the one most easily drawn.

**The registry fence on the consensus side.** A class registered before the fence has no entry in
`genesis_works`. At activation `step_model_registry` writes it an explicit inert row: `Registered`,
zero work, admission zero, so its permille returns to the room at that very boundary. The row is the
same on every node because "the build cannot describe this class" is a fact about the build, not
about the host. On the node side a registration is held until the fence is scheduled (`05169552`).
Neither path leaves a class in limbo.

**ADR-0133 §11.3's units.** `verification_window_spans` is in spans and `window_receipt()` is in DAA;
the comparison multiplies by `span_daa` before comparing. Pinned at the boundary by
`adr0133_the_window_fits_the_deadline_in_daa_at_deadline_minus_one_deadline_and_plus_one`, and the
lifecycle step is pinned separately: a class whose window does not fit is HELD from every state and
stays HELD.

**ADR-0139's burst.** The rule is pinned by unit tests — one budget per distinct round, two permits
of one round buy one, a missing round buys none, the ceiling reached exactly at 120 rounds and
saturating beyond. What those do not say is what a block AT the ceiling costs, so it was measured.

## 4. The 390 M ceiling, measured

`o13_bench_ceiling_block_apply_and_reorg`, 2026-09-18, one chain block filled to 389,991,000 gas with
18,571 transfers to fresh addresses — the most transactions the ceiling admits and the worst case for
state growth. n = 30.

| phase | min | p50 | p95 | max |
|---|---|---|---|---|
| execute the 390 M gas | 2,166 ms | 2,205 ms | 3,635 ms | 3,680 ms |
| commit (`state_root` + snapshot) | 13.5 ms | 14.3 ms | 25.4 ms | 28.2 ms |
| **apply** | **2,180 ms** | **2,219 ms** | **3,653 ms** | **3,701 ms** |
| reseed from the 18,572-account post-state | 1.3 ms | 1.6 ms | 2.5 ms | 6.4 ms |

The p95 and max columns come from a host that was compiling at the same time, so the gap from p50 is
this workload's load sensitivity, 1.7×, not a tail of the rule. Against a 120-second anchor slot a
ceiling block costs 1.8 % quiet and 3.1 % loaded. Thirty of them — the whole merge depth — re-execute
in about 67 seconds, inside one slot. The block carries 1.97 MB and leaves 1.71 MB of new state,
which is 1.23 GB a day at one ceiling block a slot: ADR-0139 §3's estimate, measured rather than
multiplied. State growth remains the binding cost and is the line to re-measure before the budget
moves.

## 5. The drill, and what it uncovered

The registry drill on this candidate did not reach its first step. It failed the way a good drill
should: by being a network shaped like the one it rehearses for.

Past `palw_anchor_clock` a devnet whose only producers are PALW lanes has no clock at all, so the
first run froze at virtual DAA 20 with 183 blocks accepted. The fix for the drill was to give it a
hash miner, so that something `bits` prices exists — and the miner refused to start:

> this network mines with PoW algo 6 (a ConsensusV2 inference lane) and this miner cannot search it
> — a nonce there is won by running the class's pinned model … Stopping rather than searching a hash
> target no header of this algo is graded by.

That refusal is not a devnet quirk. The template declares `bundle.algorithm_id`, and
`PalwRulesetV2::validate` requires that to be the committed attempt id on **every** ConsensusV2
network. testnet-11 is one.

## 6. The blocker: testnet-11 has no `bits`-priced producer

Measured from the chain with `scripts/misaka-t11-lane-walk.py`, which walks the selected parent back
from the sink over the node's JSON wRPC and counts `powAlgoId`:

| window | algo 6 (PALW attempt) | algo 8 (heartbeat) | algo 3 (hash anchor) |
|---|---|---|---|
| last 60 selected-chain blocks (4.58 h) | 60 — **100 %** | 0 | **0** |
| last 300 selected-chain blocks (41.5 h) | 261 — 87 % | 39 — 13 % | **0** |

The DAA-clock audit's first version recorded the exact opposite — "300 / 300 are algo 3" — and this
ADR bundle was built on top of that. The claim was never checked against the chain until now. It is
corrected in §2 of that audit, with the script that produced the correction.

**Severity: Critical.** Past the fence `palw_lane_advances_daa_v1` is true only for a `bits`-priced
lane, and on testnet-11 that set is empty. The only remaining tick is the heartbeat stand-in, and
`heartbeat_interval_ms` answers the NOMINAL one-hour interval whenever the selected parent is a
PALW-v2 block, which on this chain it always is.

| | today | after arming |
|---|---|---|
| DAA cadence | 275 s | 3,600 s at best; **none** across a stretch like the last 4.58 h |
| fence 6,100 | — | ~100 h instead of 3.3 |
| fence 6,900 | — | ~37 days |
| challenge window, 120 DAA | 4 h nominal | ~5 days |
| `T_leak`, 5,040 DAA | 7 days nominal | ~7 months |

And the chain's whole clock would rest on one unbonded heartbeat producer on one host. The
per-mergeset stand-in rule is still correct and still strictly safer than either alternative, but it
does not rescue this: a one-hour beat gives a one-hour DAA.

## 7. What closed the blocker, and the two rules that left the day with it

**§3c was not enough, and neither was the fix after it.** The heartbeat's interval, the miner's
yield and the template's slot stamp were each a real defect and each was fixed; the drill froze
anyway, twice more, because the rule they all rested on measured a beat against its SELECTED PARENT,
and every new chain block replaces that parent. A block that advanced no clock was moving the next
opportunity to advance it.

[ADR-0142](adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) replaces that
rule with a consensus clock cursor: a heartbeat consumes a slot, the slot advances in whole
intervals, and nothing else moves it. The slot rule retires behind the same fence, because its job
was bounding the lane's width and the cursor bounds the clock instead — so a beat may be minted
whenever its producer can pay for it, and it earns a DAA only where a slot is open. The reference
the cursor stands for is **derived, not stored**: the block at the selected parent's score with the
lowest blue score, read from the DAA window every node already builds, so a pruned node and one that
joined by pruning proof answer exactly as an archival one.

**So the single lottery and the anchor clock left the 6,301 day.** They arm together — one setter,
so they can never be spelled at two heights — and `validate_palw_v2` now refuses the anchor clock
without ADR-0142's cursor. The preset therefore ships with both dormant, and the build enforces it
rather than trusting whoever arms them next. Everything else keeps the day: the work target, the
registry, the payout, the panel economy, the execution lane, the DNS gate and the short challenge
window. The dependency runs one way — the lottery needs the work target, never the reverse.

The heights moved once more for an unrelated reason: the tip reached 6,000 while the release was
held, so the whole day moved up 300. Compatibility boundary 6,300, PALW upgrade day 6,301,
Verification and readiness V2 at 6,400, compute-overlay retirement at 6,501; ADR-0120's 6,900 stays.
Fingerprint `48d1e31feefce318ae2596bfa0083a83ec9c232437f96d77c0d0c1ce42776de4`, read from a running
node.

## 8. Verdict

_(issued when the drill on this build completes; see §10 for the run)_

### The verdict this replaces

**NOT SAFE TO ARM AT 6001**, issued against the bundle before ADR-0138 §3c. Kept because the
reasoning is what produced the fix, and because a verdict that is quietly deleted once it becomes
inconvenient is worth nothing the next time one is issued.

One new Critical, found by measuring the live chain rather than reading the design. Nothing else in
the bundle was implicated: the consensus suites passed, the one red was the dependency-boundary test
that is red on `main` too, and the 390 M ceiling measured comfortably inside a slot. The blocker was
`palw_anchor_clock` alone — and because `Params::set_palw_single_lottery` arms the lottery and the
clock together, the preset could not have shipped with one and not the other.

The decision it handed the operator was where a PALW-only network's clock comes from: give such a
network a priced lane, which is a params change because ConsensusV2 templates always name the attempt
id, or make the heartbeat the designed clock. §7 is the second of those, done as a rule about what
the lane yields to rather than as a constant.

## 9. What is deliberately NOT in this build

**ADR-0132 S's single lottery and ADR-0138's anchor clock.** Dormant, and the build refuses to arm
either without ADR-0142's cursor. §7 has the reason.

**ADR-0142 itself is built, tested and drilled — and armed nowhere.** It ships as code and as a
document so the next release starts from a rule that has been exercised rather than from a draft.
Before it is armed it wants a reorg drill and a drill at testnet-11's own cadence, which §8 of that
ADR states and §11 here makes a standing gate.

**ADR-0140 and ADR-0141** ship as documents only. Their implementation — the seams, the protocol-time
view, the deadline registry, the shadow evaluation and the counters — was written and then reverted
on the operator's call: the clock's design question is not part of this release.

## 10. The drill on this build

Run in testnet-11's exact lane composition — PALW producers, one bondless heartbeat miner, no hash
lane — with the anchor clock, the cursor and the registry all armed at DAA 20, and held to that
composition by `LANE_PROFILE`.

## 11. The gate this run adds to every future release

Two liveness bugs landed at the same fence in one day, and the full suite was green through both.
That is not a testing gap that more unit tests close: each rule was right per block, and what broke
was what happened to the chain when a fence retired the thing that had been pacing it.

The gate is now written into the launch runbook §5c and enforced by the drill itself. No build that
arms a fence is rolled out until a drill has **crossed** that fence, with the **target network's lane
composition**, and the drill's own clock gate has passed. The lane composition is read from the live
chain rather than assumed, and a mismatch stops the run.

## 12. The lesson worth keeping

Every finding in this report and the two before it came from checking a claim against the thing it
describes — the code, then the running chain. This one was different in kind: the false claim was
**mine**, written into an audit four hours earlier, and every later document inherited it without
anyone re-reading the chain. A measurement recorded in prose stops being a measurement. The script
is now in the repository so the next reader runs it instead of quoting it.
