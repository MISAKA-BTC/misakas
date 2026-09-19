# Re-audit — is every 2026-09-19 finding rooted out? (2026-09-20)

**Method.** Four agents (accounting, admission, FreePrompt/E2E, red-team) re-verified every
non-refuted finding against the integrated implementation, reading fences at RUNTIME from
`palw_rc_shipped_params()` rather than from preset constants — the process error that turned a
refuted claim into a false CRITICAL last round. Then the fixes, and then the same counterexamples
re-run against them.

**The single most important fact, and it is not a finding.** Every fence this work adds is `None`
on every shipped preset. Nothing below has changed testnet-11's behaviour by one bit. The correct
classification for the closed items is therefore **IMPLEMENTED BEHIND A DORMANT BUNDLE**, not FIXED
ON THE NETWORK, and the gap between those two words is a flag day this document does not schedule.

---

## 1. Verdicts

| # | Finding | Today (t11) | Past the bundle | Evidence |
|---|---|---|---|---|
| F1 | fork-choice weight is a number the registrant chooses | STILL EXPLOITABLE | **CLOSED** | weight per executed MAC-eq **24,572× → 1.000000×** across the whole declaration space |
| F2 | the free-prompt lane's work is a self-report | STILL EXPLOITABLE | **CLOSED** | the declaration is a comparand, never an input; a tampered one is refused by name |
| F3 | a class can be judged entirely by its own registrant | STILL EXPLOITABLE | **CLOSED** | ADR-0147: the outsider seat is drawn from the BASE-class population and its `Valid` is a veto |
| F4 | a class admitted at a ladder its own legal jobs exceed | CONFIRMED | **CLOSED** | the gate compares the deepest LEGAL job; +18.4 % under-count gone |
| F5 | pay per unit of compute falls like 1/model width | STILL EXPLOITABLE | **CLOSED** | weight **1.000000×** and pay **899,999,999 sompi/G-MAC-eq** on all four classes |
| (a) | the ceiling and the ledger price one claim differently | not reachable | **CLOSED** | one expression, and the floor's reservation is byte-identical across the fence |
| 5 | a lifecycle stage moves a class's PRICE | latent | **CLOSED** | the bundle arms registry and work target at ONE height |
| 6 | one registration divides every incumbent's pay by 6.94 | **LIVE** | **CLOSED** | the denominator is the chain's work target, not a statistic over the registered set |
| 7 | a bought `Final` grandfathers `Active` at 1,000 ‰ | LIVE | **CLOSED** | a bought class opens at `Probation`, 50 ‰, and earns the rest |
| KV | the cache credit is directional, per-bond, and dies with its claim | latent | **CLOSED** | credit is a pure function of (this prompt, the prompts committed in this chain) |
| (b) | `palw_canonical_per_draw_v1` reads only `model_lifecycles` | — | **REFUTED** | `step_model_registry` backfills every class; the ordering guard makes rows precede claims |
| — | the cancellation in `palw_claim_canonical_pwu_v1` can be desynced | — | **REFUTED** | `pwu_rule` is written once; re-registration forces the same graph, hence the same count |
| — | the `is_none_or` branch admits a row-less class to the unit | — | **REFUTED** | unreachable on the block path; the window is under one span |
| — | `ZeroQuanta` is a remote block-invalidation DoS | — | **REFUTED** | `palw_v2_accepted_objects` drops the object and the block stands |

Two findings from the first round stay refuted and were not resurrected: the 13.4 % dilution claim
and the 13.7× MAC gap (both filed from `#[cfg(test)]` fixtures or stale preset constants).

---

## 2. What is still open, stated rather than closed

**The window before the bundle can arm.** F1 is live today, `claim.pwu` needs no fence, and the
bundle cannot arm below the registry — the derivation reads a registry row, and arming it over an
empty table would keep the declared basis while announcing it had not. That window is exposure the
schedule cannot remove.

**A cold re-run is credited only its extension.** The chain cannot distinguish a producer that
re-ran a committed prompt from scratch from one that held the KV cache. ADR-0145 §6's prefix-STATE
commitment can, and it moves the object's wire. Until then the conservative direction is the one
that never pays for compute that may not have happened.

**MAC-equivalents carry no memory-traffic term.** Two classes running equal arithmetic against
unequal residency cost their operators unequally — on this fleet, RAM against disk. That is a
difference between classes in COST, not a lever a registrant can pull in PRICE, which is why it is
an open design question (ADR-0146 §6) and not an arbitrage.

**`prompt_mode` is not read by the fold.** The chain cannot tell a user's inference from the
network's own synthetic job and prices them identically. This is deliberate: pricing them
differently would be consensus judging whether a prompt was real, which ADR-0144 forbids. It is
also the honest limit of "the user's own inference becomes the work".

**The tooling half of permissionless.** The consensus gate reads the carriage and has no table to
miss a model from. `misaka model add` resolves its argument against a compiled-in ledger, so a user
who cannot build the node cannot use that command. The escape hatch is ADR-0108's extension
submission with an inline profile.

**`palw_artifact_root_ownership` is commented out on the shipped card**, and the bundle now refuses
to arm without it. A build that arms the economy must arm ADR-0143 with it.

---

## 3. Reward invariants, re-scored

| | invariant | 2026-09-19 | now, past the bundle |
|---|---|---|---|
| i | surface prompt inflation must not raise reward | VIOLATED | HELD |
| ii | no reward for compute not performed | VIOLATED | HELD |
| iii | more non-useful work must not mean more profit | VIOLATED | HELD |
| iv | efficiency allowed, shortcut arbitrage bounded | VIOLATED | HELD (measured at 1.000000×) |
| v | no model self-report in reward | VIOLATED | HELD |
| vi | difficulty not set from miner-controlled metadata | VIOLATED | HELD |
| vii | a class's declared worst case must bound every legal job | VIOLATED | HELD |
| viii | collateral reserved must scale with the weight bought | VIOLATED | HELD |
| ix | the quantity deciding fork choice must be one nobody can declare | VIOLATED | HELD |
| x | a claim's price must be recomputable without trusting any executor | HELD | HELD |
| xi | a class's economic profile must be attested, not estimated | UNTESTED | HELD (derived from the graph) |

---

## 4. Gates

Property tests, reorg/IBD and the end-to-end path are in-tree and green. The one gate that is not a
test is the drill: the repo's first working rule is that a build which arms a fence does not ship
without a drill that CROSSES it, and `ECONOMY_AT` now exists so one can.

**No height is proposed here.** ADR-0144 §6 item 0 and this repo's working rule both say the same
thing: the height is chosen after the evidence, not before.
