# RFC-0012 — settlement policy proposal and the go / no-go list for arming it on testnet-12

Lane X12 · branch `rfc12/x12-dns-retirement` · written 2026-10-08 against the tree at that branch's head.
Companion: [implementation record](rfc-0012-implementation-record.md) (requirement → code → test → status).

**Status: a PROPOSAL. Nothing here is assigned.** `Params::palw_dns_retirement` is `None` on every preset and stays so. Whether RFC-0012
is armed at DAA 9,000 is the user's decision, after the evidence below. Words used exactly:

* **MEASURED** — printed by a test in this branch (named; reproduce with the command given). **DERIVED** — arithmetic on shipped
  constants. **PROPOSED** — a recommendation with its reason; not a fact. **GAP** — not known / not testable here.
* Nothing below is a proof of PALW common-prefix security. The `safe` / `finalized` tags are a *reader label* derived from the node's
  own selected chain; they add no tip-selection rule and no authority (RFC-0012 §3). The only thing standing between a minority and a
  reorg after the DNS veto is gone is the PALW comparator, exactly as it is today for everything the veto did not catch.
* DAA → wall clock: a DAA step needs one heartbeat slot, so **at least 120 s per DAA** (`target_time_per_block`; DERIVED). Every duration
  below is therefore a *lower bound* in time. Operational notes put the live rate near 150 s per DAA; that was not measured here.

## 0. A defect this work found (read first)

The dormant reader took REAL work evidence from claims **still held in the sink's state** whose `trace_retention_daa` had lapsed. On
testnet-12's rulesets a claim is retired from state at `Final + claim_retirement` (3,000 DAA) while its trace retention lapses at
`accepted + bind + receipt + challenge + court` (5,400 DAA). **MEASURED** on the real fold
(`rfc0012_native_evidence_fold`): a claim accepted at DAA 1,001 is `Final` at 1,124 (+123), is retired at 4,125 (3,001 after `Final`), and
its retention lapses at 6,401 — the old reader never saw it, at any of the 3,001 blocks it was in state. The slowest single-bind claim
retires at +4,320 against a retention of +5,400 (`rfc0012_evidence_window`, **DERIVED** from both rulesets). So an armed network would
have produced **no evidence for ordinary claims, ever**: `safe` and `finalized` null for good, with `stop = InsufficientDepth`.
Fail-closed, so not unsafe — but the feature would not have worked. Two further defects of the same reader: it walked the whole
selected chain from the sink to the pruning point with a database read per block on **every** virtual change (testnet-12's pruning point
stays near genesis for a long time), and it read genesis (which has no EVM result) as an execution gap, stopping at `MissingHistory`
on any chain whose pruning point is genesis.

What changed (branch): evidence is read from each chain block's PALW **delta** (the `Final` transition, the free-prompt spends, the
voids), kept per block in a memory-only cache, certified by an O(N + F) sweep; a conviction after `Final` retracts what it counted;
`latest` survives a failed certificate; genesis ends the executed chain. Details in the record.

## 1. What the parameters mean, in the code as it now stands

An **effect** is an executed selected-chain block (its EVM result is root-verified against its header). `safe` is the newest effect of
the longest *contiguous* prefix, oldest first, in which every effect satisfies, in this order of reporting:

1. **frontier** — the PALW settled frontier is on the branch and at or past the effect's blue score (`FrontierNotCovered`);
2. **lifecycle closed** — every claim accepted at or before the effect is `Voided`, or `Final` with its trace retention lapsed (or
   already retired), and no DA session names a claim at or before it (`OpenLifecycle`);
3. **evidence** — over the facts accepted at or after the effect (equal-DAA work counts only if its blue score is not earlier) and mature
   at the sink: no duplicated work identity (`DuplicateWork`), no overflow, at least **D** distinct anchors (`InsufficientDepth`),
   at least **W** unique work (`InsufficientWork`), no operator above `max_operator_permille` and no class above `max_class_permille`
   of the work (`ConcentratedWork`).

`finalized` is the validated pruning point when it is an executed ancestor of `safe`. A branch that abandons a previously published
`finalized` head is `FinalizedConflict`, sticky until a validated import.

**Evidence** is: REAL (non-floor) attempt claims at their `Final` transition, weighted by canonical `pwu`; and free-prompt slices at
the spend that consumed them, weighted by the network quantum. Floor, heartbeat, EXEC Round work and DNS add nothing. A fact is **mature**
at `max(trace_retention, Final + claim_retirement)` — a claim can no longer be convicted and its producer may drop the trace — and, for a
free-prompt slice, no earlier than the spend plus the execution-quantum maturity.

## 2. The constants everything below follows from

| Quantity | Value | Kind | Where |
|---|---|---|---|
| `window_bind` / `window_receipt` / `window_challenge` (applied) / `window_court` | 600 / 600 / 120 (1,200 unshortened) / 3,000 DAA | DERIVED | `PALW_RC_WINDOWS_V1`, short window from DAA 0 |
| `claim_retirement` | 3,000 DAA after the terminal | DERIVED | same |
| trace retention committed by a producer | `accepted + 5,400` DAA | MEASURED | `rfc0012_evidence_window`, fold test |
| attempt → `Final` with an honest quorum | **+123 DAA** | MEASURED | `rfc0012_native_evidence_fold` (accepted 1,001, Final 1,124) |
| claim never bound → `Voided` | +601 DAA | MEASURED | `…_measure_how_long_an_unverified_claim_holds…` |
| claim bound, no receipts → `Voided` | +1,203 DAA | MEASURED | same |
| claim retired from state | `Final + 3,001` | MEASURED | fold test |
| DNS slashing-evidence window (`evidence_window_blocks`) | 10,080 blocks | DERIVED | `DnsParams::at_two_minute_cadence` |
| `pwu` of one genesis claim: floor / class 2 / class 3 | 2,150,532 / 2,110,364,480 / 27,002,967,184 | MEASURED | `…_measure_the_work_a_genesis_claim_carries` |
| subsidy of one attempt block | 444,562,014,000 sompi (4,445.62 MSK) | MEASURED | x12 |
| native worker base (92 %) | 408,997,052,880 | MEASURED | x12 |
| a floor claim's escrow, before and after the fence | 320,084,650,080 (72 %), unchanged | MEASURED | x12 |
| never minted per floor attempt, post-fence (`base − escrow`) | 88,912,402,800 sompi (889.12 MSK) = 20 % | MEASURED | x12 |
| inclusion pool (8 %) | 35,564,961,120 | MEASURED | x12 |

## 3. How far behind the tip `safe` runs (the number integrators will feel)

* **Lifecycle closure** (every claim at or before the effect resolved): a `Final` claim is open for the lifecycle until its retention
  lapses *or it retires*, and it retires at `Final + 3,001`; so an effect cannot be closed before the claims around it have retired,
  **≥ 123 + 3,001 = 3,124 DAA after acceptance** (≥ 4.3 days). DERIVED / MEASURED.
* **Evidence maturity**: the work that buries an effect matures **5,400 DAA after its own acceptance** (≥ 7.5 days). DERIVED.
* **Together**: with the v1 rules, `safe` for an effect needs the sink to be at least **~5,400 DAA (≥ 7.5 days) past the work that buries
  it**, and the first ETH `safe` / `finalized` tag after activation arrives no earlier than that. Before then they are `null` (state and
  call queries error). A node that joins by pruned IBD starts with `latest = pruning point` and no `safe` / `finalized` until the same
  lag has elapsed after its pruning point.
* **One griefing claim** holds the prefix at its own blue score for at most its time to void: **+601** (never bound) or **+1,203**
  (bound, no receipts) DAA, MEASURED, and only for effects at or after it; `ReceiptTimeout` charges no producer (lane PL), so the stall
  is cheap but **bounded**, not permanent, and a newer claim never holds an older effect. An open DA session holds closure for at most
  `W_disclose` (the challenge window).

**PROPOSED decision for the user (D1):** v1 is deliberately conservative — it never counts work a late conviction could still reverse.
If a ~7.5-day `safe` is unacceptable for the bridge, the alternative is a *policy v2* that counts at `Final` (+123 DAA, ≈ 4 hours) and
closes the lifecycle at `Final`, **with retraction** (already implemented: a `Final → Voided` conviction removes the fact), at the price
that `safe` can retreat within the 3,000-DAA conviction window. That needs a committed policy field and a version bump; it is not in
this branch. Whichever is chosen must be said in the release notes in those words.

## 4. Proposed values (PROPOSED; none is final)

| Parameter | Proposed | Reason | Needs |
|---|---|---|---|
| maturity rule | v1 (above) | never counts reversible work | decision D1 |
| `settled_anchor_depth` **D** | **3** | with an operator cap of 400 ‰ at least three distinct operators must contribute, so three anchors is the least that cap can be met with | live operator census |
| `unique_mature_work` **W** | **6,331,093,440** (= D × 2,110,364,480, three of the smallest REAL class's claims) | a floor under which dust cannot certify; **the real W must come from a census**: `W = ⌈k × matured REAL work per 5,400-DAA window⌉` with `k` chosen so the label does not flap | census of REAL `pwu` per window on testnet-12 (GAP) |
| `max_operator_permille` | **400** | MEASURED table below: certifies 3-, 4- and 8-operator populations and a Zipf-8 (top share 36.8 %), refuses 1 and 2 operators | ≥ 3 operators with matured REAL work |
| `max_class_permille` | **1000** (off) until ≥ 3 REAL classes carry matured work, then 600 | testnet-12 has two REAL classes; with two, any cap under 500 can never be met and under 1000 binds the larger class | class census |
| `legacy_evidence_horizon_daa` | **10,080** | `evidence_window_blocks`; a DAA is at least one block, so 10,080 DAA is never shorter than the right it preserves. Exposure ends at `fence + 10,080` DAA (≥ 14 days) | — |

MEASURED, `rfc0012_measure_which_operator_caps_certify_which_distributions` (`ok` certifies, `X` refuses):

| operator cap (‰) | 300 | 400 | 500 | 600 | 800 | 1000 |
|---|---|---|---|---|---|---|
| 1 operator | X | X | X | X | X | ok |
| 2 equal | X | X | ok | ok | ok | ok |
| 60 / 40 | X | X | X | ok | ok | ok |
| 3 equal | X | ok | ok | ok | ok | ok |
| 4 equal / 8 equal | ok | ok | ok | ok | ok | ok |
| Zipf over 8 | X | ok | ok | ok | ok | ok |

**Caps are a speed bump, not a wall.** The unit is the registered `operator_id`; one entity can register k of them, each locking a
bond (13,000 MSK on testnet-12). The cap raises the cost of looking like several operators to k bonds; it does not identify entities.
The 8 genesis cards are 8 operator ids run by one fleet today — a census of *entities*, not ids, is part of the go / no-go.

## 5. Attacks, as far as this harness can say

* **Private maturation.** A branch with colluding Panel seats can license and finalize its own claims in **+123 DAA** (MEASURED), but
  none of that work is *counted* before **+5,400**, so a privately held fork cannot show D/W of mature work for ≥ 7.5 days, during which
  the honest chain adds its own. More importantly `safe` is read from the node's *selected* chain: a private branch can affect it only
  by winning the PALW comparator first. **The label does not protect against that; the comparator does, and its common-prefix
  security is not proven.** GAP: nothing here measures the comparator against a q-capacity adversary.
* **Concentration.** Table above. The failure mode is fail-closed: too tight a cap ⇒ `ConcentratedWork`, `safe` null, nothing wrong
  labelled safe.
* **Withholding.** Receipts withheld: ≤ +1,203 DAA per claim, bounded (§3). Evidence withheld (no REAL work produced): `safe` simply does
  not advance (`InsufficientWork`); the chain still progresses (`latest`) — no fallback to the sink or to heartbeats (x1: `safe` is
  `null`, never the legacy label, with zero Final work).
* **Spam to inflate depth.** One claim is one anchor however many blocks carry it; duplicated work identity is a stop (`DuplicateWork`)
  — by design a stop and not a de-duplication, so that a broken uniqueness invariant is loud. The fold refuses duplicates (B-4) so it is
  unreachable today.

## 6. Resource bounds (MEASURED unless marked)

* Row cache: 352 bytes per executed chain block (about 424 with its map entry): 100,000 blocks ≈ 42 MB plus the claim records of the few
  blocks that finalize or spend work.
* Cold rebuild after a restart: decode + extract 542 ns per mostly-empty delta (debug build, no disk; 3,122 real deltas) — disk reads
  dominate; **estimate** ≤ 10 s per 100,000 blocks on one SSD, not measured on the node.
* Per virtual change: one cache lookup per chain block plus one sweep. `certify_native_prefix_v1`: **83 ms for 160,000 effects and
  80,000 facts** (debug); the per-effect reference it replaced grew 30× for 4× the input (9 ms at 400 effects, 269 ms at 1,600) — at
  160,000 effects it would take about 45 minutes (extrapolated, not run). In a release build everything is roughly an order of magnitude faster. GAP: the
  full processor with a >100,000-block chain was not run (the EVM harness cannot build one; see the record).
* No unbounded rollback: nothing in the settlement path rewinds consensus state; the only retreat is the label, and a retreat below a
  published `finalized` is the sticky `FinalizedConflict`.

## 7. Trusted-sync assumptions (stated plainly)

1. A fresh node trusts the same pruning-point headers-proof it trusted before RFC-0012; the pruning point's EVM state is root-verified
   against its header before import. After the import `safe` / `finalized` are unavailable, never inferred.
2. Evidence older than the pruning point is not guessed: the pruning point's own delta is never read, so an archival node and a pruned
   node compute the same label.
3. At least one honest peer serves the headers-proof and the body chain; there is no DNS checkpoint to fall back on.
4. `finalized` means "the validated pruning point under a certified `safe` prefix" — protocol/economic finality under the stated
   assumptions, not a certificate and not mathematical finality.

## 8. Go / no-go for arming at testnet-12 DAA 9,000

> **Superseded framing (Lead, 2026-10-08 ~20:00).** There is no DAA-9,000 flag day. The next public release enables `palw_dns_retirement_v1` together
> with RFC-0008 / 0010 / 0011-K2 / 0014 / 0015, and ships only after all RFC implementation is complete. The list below is still the right content
> (the "9,000" is just the release height); the consolidated, sorted list of what is left — code with a next step each, the policy values with a
> recommended value and the trade-off, and external / ops — is **§11**.

**GO needs all of:**

1. The user decides D1 (§3) and the values in §4 are fixed from a **live census** (operators by entity, REAL classes, `pwu` per window).
   With fewer than three operators or the 400 ‰ top share broken the label will be `null` from day one — acceptable only if said so.
2. The release gate 4 drill (RFC §8) is run with the **shipped binary** on a private multi-node network that has *real* `Final` claims,
   deposits, withdrawals and a pruned join with EVM state. This branch proves the pieces in-process and names what it cannot reach (§9).
3. The fleet **and third-party seats** are upgraded before the height. Arming adds a fork-id gate fence
   (`rfc0012_fork_id_and_cost`, MEASURED): an upgraded node refuses an un-upgraded peer **from the height**, and before it they stay
   peers. The fork-id module's own notes record that builds older than its 2026-09-10 fix cut an upgraded peer off at connection time
   (six third-party seats within four minutes on testnet-11's second flag day); builds carrying the fix keep a peer whose next fence is
   lower than theirs until they pass it. Which builds the third-party seats run is not known here — treat it as a flag day for everyone.
4. Integrators are told, in these words: ETH `safe` / `finalized` are `null` for ≥ 7.5 days after activation and whenever evidence stops;
   `eth_getBalance` etc. at those tags **error**; `latest` is optimistic. A bridge or exchange keyed on `finalized` stops advancing.
5. The 20 % is understood: **88.9 billion sompi (889 MSK) per floor attempt block is never minted** from the fence on; the emission ceiling
   does not rise, a legacy claim keeps its escrow, and a REAL-class claim accepted after the fence escrows 92 %.
6. Ops runbook for `FinalizedConflict` exists (the record lists the signal and the resync; the page / pager is OPS).

**NO-GO if any of:** the fleet cannot be upgraded before the height; no `Final` has been demonstrated through the shipped binary with the
retirement armed; the census shows a single entity behind most REAL work and the user still wants a non-null `safe`; an exchange
integration depends on `finalized` within days.

**Risks to a live network if armed (all remain after the work above):**

* *Fork-choice change.* The DNS veto (measured in x2: `HardCheckpointReject` below the fence) is gone past it; the PALW comparator alone
  decides, and its common-prefix security is not proven.
* *DNS subsidy.* The 20 % validator share stops; validators that were being paid stop being paid. Their bonds exit (x11).
* *Rejected kinds.* 0x10 / 0x11 / 0x19 are refused in header context, UTXO context and templates (x11, x15). Anything still emitting them
  (an un-upgraded validator service) has its transactions dropped.
* *EVM heads.* `safe` / `finalized` unavailability (above). The `Pause` bridge policy ceases to mean anything.
* *Non-revertible binary.* After the fence an old binary accepts the first two blocks at or past it and disqualifies the third in the
  measured script (x6: `StatusDisqualifiedFromChain`; the block it stops at depends on when the first post-fence attempt is paid); reverting a node's binary after the height is a fork, not a rollback.
* *Wallet / explorer.* Done in this branch for the wallet, CLI, wRPC / gRPC and the first-party explorer; **EXTERNAL** work remains for
  third-party wallets, SDKs, EVM tooling that reads `safe` / `finalized`, and any explorer other than misakascan.

## 9. What this evidence does not cover (GAPs, repeated from the record)

No claim reaches `Final` through the processor with the EVM lane on (the harness clock stops at DAA 1); a real pruned IBD with EVM state
is not reachable in-process; market orders settle as `Refused MARKET_MISSING` (no seeded market, so nothing fills); live census data;
the comparator against a capacity adversary; the DNS "all-equivocating votes" case (the DNS state is simply not an input past the fence,
x2/x13, but no equivocation was crafted); release-build timings.

## 10. D1 — the maturity offset: where 5,400 comes from, what the alternatives would admit (lane X12b, 2026-10-08)

The user's position on D1: **do not shorten 5,400 now, and do not treat it as final.** RFC-0012 is outside the DAA-9,000 release. This section
is the evidence; it recommends no change. Sections 10.1 and 10.2 are reading and arithmetic on the tree at `9050b06bc`; 10.3 holds the tests.

### 10.1 The derivation (every term, its t12 value, where the code enforces it)

```
M_v1(claim)  =  max( t_acc + R ,  t_F + C )              palw_native_settlement_v1.rs  native_facts_of_block_v1
R            =  W_bind + W_receipt + W_challenge^max + W_court          = trace_retention − accepted
             =  A_DA + W_disclose        with  A_DA = W_bind + W_receipt + W_court
t12:   R = 600 + 600 + 1,200 + 3,000 = 5,400  = 4,200 + 1,200        C = claim_retirement = 3,000
       t_F − t_acc = F_off = bind_used + receipt_used + W_challenge^applied  = 123 (honest quorum, MEASURED)
       t_F + C = t_acc + 3,123 < t_acc + 5,400  ⇒  M_v1 = t_acc + 5,400   (the second term wins only if F_off > 2,400: a re-bound claim)
```

| Term | t12 | What it protects | Enforced at |
|---|---|---|---|
| `W_bind` | 600 | the panel must be bound by here, else `BindTimeout` | `sweep_deadlines` palw_state_v2.rs:33283 (`bind_timeout_reason`); shape `anchor_delay + max_beacon_gap < window_bind` palw_mode_v2.rs ≈1150 |
| `W_receipt` | 600 | the quorum must licence by here, else `ReceiptTimeout` | palw_state_v2.rs:33370 |
| `W_challenge` | 1,200 unshortened, **120 applied** | a licensed claim finalizes after `window_challenge_at(licensed_daa)`; but retention and the DA disclose window use the **unshortened** value | applied: palw_state_v2.rs:4955, `PALW_SHORT_CHALLENGE_WINDOW_DAA_V1`:1756; retention: palw_producer_v2.rs:437; disclose: palw_state_v2.rs:6795 (`W_disclose = window_challenge`, SA-3 `≥ 2 ×` the finality window, finality_depth = window_challenge/2 = 600, params.rs:3908) |
| `W_court` | 3,000 | the worst-case **honest prosecution** fits: `54 moves × 42 + 216 = 2,484` (2^26 ladder), `66 × 42 + 216 = 2,988` (2^32) | `palw_ladder_fits_window_court_v1` palw_context_ladder.rs:500; `validate_ruleset_shape` palw_mode_v2.rs:1299; `sweep_court_deadlines` palw_state_v2.rs:31175 |
| `R` itself | 5,400 | "the four windows a claim can be asked inside" — a promise shorter discards evidence before it can be asked for (ADR-0072 D8) | admission equality pin `check_palw_attempt_da_pins_v1` palw_admission_v2.rs:972,986; header stage pre_ghostdag_validation.rs:776,800 |
| DA gate | `now + W_disclose ≤ trace_retention` | an accusation must be answerable inside the retention | `DaOutsideRetention` palw_state_v2.rs:16587,30013; `CourtOutsideRetention` :30782 (a class's own court window can *extend* the claim's retention) |
| `C = claim_retirement` | 3,000 (`CLAIM_RETIREMENT == WINDOW_COURT`) | after it the claim record, its DA record, panel and derivations are gone: nothing can reverse the `Final` | `terminal_deadline_at_v1` palw_state_v2.rs ≈15470; `arm_retirement` :27916; `retire_claim` :27930; `reverse_convicted_final` :23921 returns when the claim is absent; palw_fp_devnet_v3.rs:198 |

**What 5,400 is.** It is the producer's *trace-retention duty*, equal to the last moment a data-availability session can conclude
(`accuse window 4,200 + disclose 1,200`), sized for the slowest single-path lifecycle with the **unshortened** challenge window. It is not
derived from the reversal horizon. For an ordinary claim the chain stops being able to reverse the `Final` at **`F + 3,001 = acceptance + 3,124`**
(retirement), so v1 waits **2,277 DAA longer than the chain's own reversal horizon**: 1,080 from counting the challenge window at 1,200 when 120
applies, and 1,197 from bind/receipt allowance a claim finalizing at +123 did not use. The reverse also holds: a re-bound claim (Final up to
+2,520) retires at +5,520, after 5,400, which is why v1 takes the `max`.

**Horizons that are *not* inside 5,400** (all t12, DAA after acceptance unless "F+" = after `Final`):

| Horizon | Value | Expression / site |
|---|---|---|
| claim lattice (a redraw, then court) | 6,600 | `2(W_bind+W_receipt)+W_challenge+W_court`, `palw_v2_claim_lattice_daa_v1` params.rs:3864 |
| pruning depth (blue-score units) | **74,920** with the class-verify cap (`2(600+16,000)+1,200+3,000+37,520`) | `palw_v2_pruning_depth_v1` params.rs:3839 — this, not M, bounds `finalized` (below) |
| max claim exposure | 7,200 | lattice windows `+ fp_abandon_hold 600`, `max_claim_exposure_daa` palw_fp_devnet_v3.rs:~104 |
| liability period | 6,900 | lattice `+ reorg_margin 300`, `liability_daa` palw_mode_v2.rs:1166; `withdrawal_delay 7,500 >` it |
| seat lock life after `Final` | F+1,000 | `PALW_FINAL_LOCK_LIFE_DAA_V1` palw_state_v2.rs:1784 (lane V02) |
| liability record / vesting-row clawback | F+3,000 | `persist_panel_liability` :22755 (`expiry = F + window_court`); palw_vesting_v1.rs:94 |
| record prune horizon (a late `PanelFalseValid` can still bind, slash-only, no reversal) | F+6,000 | `palw_panel_obligation_prunable_v1` palw_panel_var_v1.rs ≈367 |
| second clock (settled-anchor depth) on locks/rows | at most `+2 × window_court` past the DAA expiry | `palw_second_clock_holds_v1` palw_panel_var_v1.rs:261 |
| node finality depth (the reorg bound) | 600 blue score | `window_challenge/2`; a sink candidate that does not contain the finality point is refused by the DAG's own rule |
| draw-beacon reorg fringe | 300 | `reorg_margin_daa`, palw_mode_v2.rs:1077 |

There is **no parameter called "proof grace"** in this tree (the nearest names — `receipt_maturity` 400, `max_beacon_gap` 400,
`anchor_delay` 20 — concern the draw beacon, not evidence). RFC-0014 §7.3 / §16 says reward maturity and the model-lease `retained_liability_until`
must be aligned with the liability horizon; it is not armed here, and when it is, `M` must be re-derived against it (a lease that must be
retained "through the claim's dispute / liability horizon" is longer than today's 5,400).

### 10.2 The candidates: 5,400 / 3,000 / 1,000 / 600

Read as **`Final + X`** for the three shorter values (3,000 = `claim_retirement` = `window_court`; 1,000 = the seat lock life; 600 = the bind /
receipt / abandon-hold unit), with the closure rule moved to the same instant; v1's 5,400 is acceptance-relative (= `Final + 5,277`). Measured
from acceptance instead, subtract 123 from the shorter ones; nothing below changes in kind. Lag = `F_off (123) + X` (v1: `max(closure 3,124,
5,400)`), **before** the D anchors and the frontier cover, which add at least one more `Final` (+123) after the effect.

| Rule | `safe` lag | at 24 DAA/h (observed) | at 30 DAA/h (design, 120 s) | Windows still open when the evidence counts |
|---|---|---|---|---|
| **v1: 5,400 after acceptance** | 5,400 | 225 h = 9.4 d | 180 h = 7.5 d | none that can reverse; the slash-only record tail (F+3,000…F+6,000) |
| **`Final + 3,000`** | 3,124 | 130 h = 5.4 d | 104 h = 4.3 d | none that can reverse; the same slash-only tail |
| **`Final + 1,000`** | 1,123 | 47 h = 1.9 d | 37 h = 1.6 d | **2,000 DAA** of conviction / DA default / vesting clawback / record, the seats' locks just expired |
| **`Final + 600`** | 723 | 30 h = 1.3 d | 24 h = 1.0 d | **2,400 DAA** of the same; also shorter than one DA disclose window (1,200) and the seat lock |
| *(for reference) `Final + 6,000`* | 6,123 | 255 h | 204 h | none, including the slash-only tail |

`finalized` is **not** moved by any of these: it is the validated pruning point, at least the pruning depth (74,920 blue score on t12; blue score
grows at least as fast as DAA) behind the tip, which is months, not days. D1 is about `safe`.

**Why `Final + 3,000` is the only anchored value below 5,400.** Both ways the chain can take back a `Final` are *visible to the certificate* no
later than `F + 3,000`, by construction: the court (a prosecution opened at `Final` completes in ≤ 2,988 ≤ 3,000, `W_court`; **measured in 10.3:
the last block that still reverses a `Final` by conviction is the one at `F + 3,000`**), and data availability (a `Final` claim is accusable only
while it is held — **measured: the accusation is admitted in the block at `F + 3,000` and refused with `MissingClaim` from `F + 3,001`**). Retirement
removes the claim, so a later conviction can slash but cannot reverse (`reverse_convicted_final` returns). Counting evidence at `F + C` therefore
never counts work the chain can still un-count, so `safe` cannot retreat because of a conviction — the property v1 buys with 5,400. The rest of
v1's margin protects the producer's *unenforced* retention after retirement, which no chain rule can use, and the slash-only tail, which v1 does
not cover either (it ends at F+5,277, before F+6,000).

> **Correction (X12b, 2026-10-08), from the measurement in 10.3.** An earlier draft of this section said a DA session "is cut off when the claim
> retires, so a default lands by `F + 3,000`". That was an inference from reading `da_admission_v1`, and the real fold contradicts it: an open
> session **holds the claim in state past its retirement**, and the default lands `W_disclose + 1` blocks after the accusation — **as late as
> `F + 4,201`** for an accusation at `F + 3,000`. It does not change the conclusion for `Final + 3,000` (every admissible accusation is filed by
> `F + 3,000` and an open session is exactly what stops a fact from counting and keeps the lifecycle open, so the claim is never counted while a
> default can land — test `d1_e`), but it does change what a *reader* may assume: a `Final` claim can be voided up to `F + 4,201`
> (`≈ acceptance + 4,324`) by the DA channel, **after** the retirement point. v1's instant (`acceptance + 5,400`) is above that horizon too;
> `Final + 3,000` is below it and is safe only because the open session withholds the fact (and any future rule that counts evidence while no
> session is open must keep that property).

**What each shorter value admits, and what would have to change** (attack timing is demonstrated with the real fold in 10.3; profitability is
analysis, not measurement):

* **`Final + 1,000` / `+ 600`** count evidence while the claim can still be convicted or defaulted. Producer + every seat colluding can then
  produce `safe`-grade evidence from a fabricated execution that a public verifier — who needs up to 2,988 DAA to dissect — has no time to
  convict first; withheld material is counted though an accusation can still be filed (through `F + 3,000`) and its default can still land (through `F + 4,201`, 10.3) — the open session withdraws `safe` at the accusation, not at the default. The
  collateral design bar prices fraud gain against recoverable value; a *label* that an external bridge acts on is a **new gain term the bar
  does not price**. A conviction then retracts the evidence (implemented), so `safe` retreats, but whoever acted on it has already acted. Making
  these safe is **not a policy change**: `window_court` would have to fall to ≤ X (the ladder must shrink or the turn clock speed up),
  `claim_retirement`, the liability period, `withdrawal_delay`, the pruning lattice and the retention pin follow — all inside
  `palw_ruleset_id_v2`, i.e. a re-genesis-class change — or the label would need a stake-weighted discount and a consumer-visible retraction
  protocol that this branch does not specify.
* **`Final + 3,000`** is a policy-only change (a v2 rule: `matured = Final + claim_retirement`, drop the retention term; a committed policy field
  and a version bump), with three stated cautions: (0) **it has no slack** — the last block that reverses a `Final` is the block at `F + 3,000`,
  which is also the rule's own instant; it is closed only because the certificate reads the sink state *including* that block, and
  `Final + C + 1` (or `+ reorg_margin 300`) is the cheap way to stop depending on that equality (10.3, `d1_b`/`d1_d`); (1) a class's own model court window (`palw_model_court_window`) can outrun `window_court` and extends the claim's retention (palw_state_v2.rs ≈30790)
  after `Final` - **closed in wave 2 (C6, 12): the evidence conversion now reads the claim's live retention too, so the fact is mature at the later of the two**; (2) the slash-only tail (F+3,000…F+6,000) stays open exactly as under v1.
* **v1 (5,400)** needs nothing more; its cost is the lag and its slack.

### 10.3 Tests and measurements (X12b)

Everything here is printed by a test in the branch (named; reproduce in 10.3.6) on testnet-12's own `Params` and the fence still `None` on every
preset. Words as at the top of this file: **MEASURED** / **DERIVED** / **GAP**. This section recommends nothing; it is the evidence the user asked for.

#### 10.3.1 What the node now says about `safe` (item 4): `getPalwSettlement.nativeReadiness`

**No new op.** `getPalwSettlement` (op 182) gains one optional field. The snapshot still says *what* is safe and gives ONE stop reason; the
explanation says *which effect* holds `safe` back, *every* condition it still lacks (not only the first), which clocks are already running, and
whether the history the certificate needs is present at all. It decides nothing, no consensus path reads it, it is never persisted, and it is
computed from the SAME evaluation as the snapshot (`native_evaluate` returns both), so it cannot be about other evidence than the certificate.

| Field | Meaning |
|---|---|
| `generation`, `sinkDaa`, `sinkBlue` | the sink the explanation stands at (the response carries it only when it equals the snapshot's `generation`: a virtual change between the two reads drops it, the caller asks again) |
| `policy`, `maturity` | the numbers the certificate is held against; `maturity.rule = "v1"` with `claimRetirementDaa` and `quantumMaturityDaa` — **the rule in force, so a reader knows what "mature" means on this network** |
| `safe`, `safeLagDaa`, `safeLagBlue`, `stop` | the snapshot's, plus how far `safe` trails the newest executed block |
| `stoppedEarly` | weighing never happened: `missingHistory { gap, block }` (`gap` is one of `deltaNotRetained`, `executionGap`, `rootChainBroken`, `executionDisagreesWithHeader`, `headerUnreadable`, `parentUnreadable`, `executionRowUnreadable`, `stateUnavailable`, `paramsAbsent`, `persistedSnapshotIncompatible`, `reachabilityUnreadable`, `chainOpenEnded`) · `unexecuted` · `finalizedConflict` |
| `blocking` | the first effect that does not certify (the one right above `safe`): its `waits`, in the certificate's order, every one |
| `tip` | the same for the newest executed effect, when it is not the blocking one |
| `finalized` | the label, the pruning point, and `wait`: `noSafePrefix` · `pruningPointNotExecuted` · `pruningPointNotUnderSafe { safeBlue }` · `conflict` — **`finalized` waits for the pruning point, never for a maturity number** |
| `skipped` | evidence the node saw and did **not** count, by cause: `voided`, `baseClass`, `openDa`, `unpriced`, `bondNotHeld` (the bond a `Final` claim names has left the sink's state — the retention gap of the bond registry; its operator cannot be named and its work is never guessed) |

`waits[]` kinds (JSON `kind`): `frontierBehind { frontierBlue, effectBlue, frontierOnBranch }` · `openClaim { claim, stage, acceptedBlue,
retentionDaa, nextDeadlineDaa, waitDaa }` (`waitDaa` only for a `Final` claim, whose retention lapse is a clock; for any other stage the claim's own
panel / receipt / challenge / court clocks decide and **no end is promised**) · `openDaSession { claim, claimKnown, deadlineDaa, waitDaa }` (open court
on claim X; `claimKnown: false` = the dispute cannot be located and counts against every effect) · `waitingMaturity { facts, work, earliestMaturedDaa,
waitDaa, readyDaa }` (**waiting on maturity N DAA**: facts that would qualify exist and are not mature; `readyDaa` is the first DAA at which the facts
already on the chain meet the policy, `null` if even all of them would not) · `insufficientDepth { have, need }` · `insufficientWork { have, need }` ·
`concentratedWork { dimension, topPermille, capPermille }` · `duplicateWork` · `arithmeticOverflow` · `invalidPolicy` · and the early-stop kinds above.
At most 8 claims and 8 sessions are named per effect; the totals (`openClaimsTotal`, `openSessionsTotal`) are exact. `earliestReadyInDaa` is
`null` the moment any unmet condition needs an event that has not happened — it never promises that `safe` WILL advance.

**Wire.** wRPC response version 3 is written only when `nativeReadiness` is present (v1 and v2 bytes are unchanged otherwise, and a v2 body is the
byte prefix of a v3 one); the reader accepts 1 to 3. gRPC: `optional string nativeReadinessJson = 12`. JSON/wasm: key `nativeReadiness`, TypeScript
`INativeSafeReadinessV1`. It exists only past the retirement fence, so no shipped network's wire changes. CLI: `misaka palw settlement` prints the
lines under the settlement line (`--json` carries the whole document). The explorer (`contrib/misakascan-t12`) does not render it (GAP, small).

**Cost.** The snapshot path is `native_evaluate(..).snapshot` and pays nothing for the explanation (the material is moved out of the evaluation, not
recomputed). The explanation is built on demand and memoized per sink: one evaluation per virtual change, however often the RPC is called.

**Tests.** Core (`palw_native_readiness_v1`, 11 pass): the first stop-bearing wait of the blocking effect equals `certify_native_prefix_v1`'s stop on
400 generated chains; a promised time is exact and tight (certifies at `+d`, not at `+d-1`); evidence is re-read at the moment the last clock stops
(it is not monotone in time: a fact that matures later can break a cap — a test constructs it and the promise is withheld). Processor (`rfc12_r1`
to `r4`, with x0 to x15: 20 pass): agrees with the snapshot and names `frontierBehind`; names an unresolved claim with its stage and no clock; a deleted
delta row is `stoppedEarly = missingHistory { deltaNotRetained, block = that block }`, restored it is the ordinary explanation; unmatured work is
`waitingMaturity` with `readyDaa 500`, `earliestReadyInDaa = 500 - sinkDaa`, and flipping the same fact to mature certifies exactly that effect.
Wire: rpc-core 8, gRPC 2, CLI 6 pass.

A real answer (`rfc12_r4`, harness chain; the unmatured work was placed at a block through the named `cfg(test)` seam, see 10.3.2):

```json
"blocking": { "daa": 0, "blue": 1, "inSafePrefix": false,
  "waits": [ {"kind":"insufficientDepth","have":0,"need":1},
             {"kind":"insufficientWork","have":"0","need":"1"},
             {"kind":"waitingMaturity","facts":1,"work":"10","earliestMaturedDaa":500,"waitDaa":499,"readyDaa":500} ],
  "earliestReadyInDaa": 499, "evidence": {"anchors":0,"work":"0","maturedFacts":0,"pendingFacts":1,"pendingWork":"10"} }
```

and one that cannot promise (`rfc12_r1`): `waits = [ frontierBehind {frontierBlue 1, effectBlue 2}, openClaim {stage provisional, nextDeadlineDaa 600, waitDaa null} ]`,
`earliestReadyInDaa: null`.

#### 10.3.2 The attack harness: what is real, what is a seam

* **Real.** `apply_palw_transition_v7` on testnet-12's `Params`: a claim is accepted at DAA 1,001, bound, licensed by five seats, and `Final` at 1,124
  (+123); a conviction (`PanelFalseValid` with an executor equivocation) or a DA default reverses it; the sweep retires it at `Final + 3,001`. The
  extraction of evidence from the fold's own deltas, the fact conversion, the lifecycle closure (`native_open_from_v1`, for v1) and the certificate
  (`certify_native_prefix_v1`) are the shipped functions.
* **Seams, named.** (1) The chain is one block per DAA, `blue == DAA`, linear: no DAG, no fork choice, no PoW — **a fork race is not run** (the PALW
  common-prefix property is NOT proven here). (2) The fixture can only drive the floor claim without the class registry, so the fact is converted from
  the same record relabelled to a REAL class. (3) The policy is `D = 1, W = 1, no cap`, to isolate time; a real policy adds an anchor (`+123` DAA) per
  unit of `D`. (4) One claim, one effect that matters (the accepting block, DAA 1,001); effects exist at every DAA from 1,000. (5) The three
  alternative rules **exist only in `rfc0012_safe_maturity_attacks.rs`**: a function that re-times the fact the real conversion made and moves the
  lifecycle closure to the same instant (`Final + X`). v1 is the only rule in the tree. (6) Processor-level tests (`rfc12_r*`) place facts and a frontier
  through the `cfg(test)` seams of the x-matrix; the harness clock stops at DAA 1 to 2 (the EVM lane cannot be re-stamped), so "time" there is the fact's
  `matured_daa` against the sink DAA.

#### 10.3.3 Results (MEASURED)

**(a) Honest timeline** (`d1_a`): the first DAA at which `safe` covers the accepting block (claim accepted 1,001, `Final` 1,124, retired 4,125,
retention lapses 6,401; frontier after `Final` = blue 1,001):

| Rule | `safe` covers it at | = acceptance + | = `Final` + |
|---|---|---|---|
| v1 | 6,401 | 5,400 | 5,277 |
| `Final + 3,000` | 4,124 | 3,123 | 3,000 |
| `Final + 1,000` | 2,124 | 1,123 | 1,000 |
| `Final + 600` | 1,724 | 723 | 600 |

(each exact: not a DAA earlier.) v1's closing term is the retention, not the retirement.

**(b) The last block that still reverses a `Final` by conviction** (`d1_b`; the five seats signed `Valid`, a public verifier files the equivocation):
every conviction in a block from `Final + 1` to **`Final + 3,000`** voids the claim (`CourtFraud` in that block's delta); the block at `Final + 3,001`
folds but reverses nothing (the claim is retired in it); from `Final + 3,002` the claim is gone. **The last reversing block is `Final + claim_retirement`.**

**(c) The DA channel** (`d1_c`; `DefaultAccused` by a seat, 13 accusation DAAs from `Final + 5` to `Final + 3,001`): the accusation is **admitted through the
block at `Final + 3,000`** and refused with `MissingClaim` from `Final + 3,001`; every admitted session stays open `W_disclose + 1 = 1,201` blocks and
**holds the claim in state past its retirement**; the default lands at `accusation + 1,201` and voids the claim as `ProducerWithholding` (a withholding,
not a proof of fraud). So **the DA channel can reverse a `Final` as late as `Final + 4,201`** (`≈ acceptance + 4,324`), not by `Final + 3,000` as 10.2
first said (see the correction there).

**(d) Producer + every seat colluding** (`d1_d`): the conviction lands in the block at `Final + k`. Verdict per rule (`caught` = `safe` never counted the claim;
`retracted` = `safe` HAD covered the accepting block at the block before and the voided set takes it back in the block of the conviction; `late` = the claim
was already retired, nothing reverses):

| Rule | `caught` for `k` in | `retracted` for `k` in | `late` (not reversible) |
|---|---|---|---|
| v1 | 1 to 3,000 (all of them) | none | 3,001+ — the work then counts at `acceptance + 5,400` and is never taken back |
| `Final + 3,000` | 1 to 3,000 (all of them) | **none** (the last reversing block, `+3,000`, is the rule's own instant block) | 3,001+ — counted since `+3,000`, stays |
| `Final + 1,000` | 1 to 1,000 | **1,001 to 3,000 (2,000 blocks)** | 3,001+ |
| `Final + 600` | 1 to 600 | **601 to 3,000 (2,400 blocks)** | 3,001+ |

(the assertion in the test is stronger than the table: for every rule, "counted at the block before the conviction" is exactly "`k` is past the rule's
instant", checked at 18 values of `k`.) Two lines the table does not make visible: **`Final + 3,000` is closed with zero slack** (see the caution in 10.2),
and **a conviction after the retirement never retracts work under any rule** — a `late` conviction slashes, it does not reverse (`reverse_convicted_final`
finds no claim). That is a property of the fold, not of D1, and it is the same for every rule; it is also why a longer `M` cannot help for `k > 3,000`.

**(e) Withheld material** (`d1_e`, accusation at `Final + a`, `a` in 50 to 3,000): for **every** rule, **no block of the 1,201-block session counts the claim,
and none counts it after the default** (an open session removes the claim's fact and keeps the lifecycle open). A rule that had not yet counted the claim
(`a <= X`) never does; a rule that had (`a > X`, only `Final + 1,000` / `+ 600`) **loses `safe` at the accusation, not at the default**. So the DA channel is
fail-closed for all four rules; what differs is whether `safe` was published before the accusation arrived.

**(f) The node's own reorg bound** (`d1_f`): `finality_depth = window_challenge / 2 = 600` blue score; the node refuses a sink candidate that does not
contain its finality point (`sink_search`, `virtual_finality_point`). `safe` lags the accepting block by at least 5,400 / 3,123 / 1,123 / 723 DAA
(9.0 / 5.2 / 1.9 / 1.2 x the finality depth), before the `D` anchors. At 24 DAA/h that is 225 / 130 / 47 / 30 h; at 30 DAA/h 180 / 104 / 37 / 24 h.
**DERIVED, not measured on a real chain:** it assumes blue score grows at least `600 / lag` per DAA (0.83 for the shortest).

**(g) A private branch released late, at the node** (processor, real fork choice, harness chain). Existing evidence from the heartbeat lane, re-run here
(`hb_fork_choice_probe`): **`hb_probe_e` — PASS (322 s)**: with the claim not yet `Final` a heavier private branch (two attempts of its own) flips the victim and
the double spend lands; once the claim is `Final` (frontier = the anchor's blue score, 9; the private frontier is 0) the victim stays on the public chain
(`kept public` 284 times) however heavy the private branch (+2,097,720 vs +2,097,437 blue work) — the PALW comparator's first key refuses it. **The window in
which a branch without the claim can still win closes at `Final` (+123 DAA after acceptance), before any `safe` rule above counts the claim (+723 at the
earliest).** `hb_probe_d` (HB lane) — **PASS (629 s)**: only the finality depth stops a heartbeat-only reorg: with `X` **580** blue score deep (finality depth 600) the heavier private
branch flips the victim and the double spend lands; with `X` **620** deep the victim never leaves its chain (`kept public` 618 times, private +1,237 blue work vs public +620). What this does NOT show:
a private branch that carries its OWN `Final` claim with a frontier at least the honest one's (colluding panel signing on a branch nobody saw — a
capacity-security question, not a maturity one; **GAP**, nothing here bounds it).

**(h) Deep reorg across the would-be `safe` point at the node** (`rfc12_r5`, same chain, same private branch, two worlds): *mature at the release* (a rule whose
instant has passed): `safe` and `finalized` had been published at b0 (`finalized = safe`); the heavier branch takes the sink and **both labels are
withdrawn** with `stop = insufficientDepth` — **not** `finalizedConflict` (b0 is still an ancestor of the new sink; the alarm is for a reorg that abandons the
finalized block itself, x7). *Not yet mature*: nothing was published, nothing to withdraw (`safe = finalized = null` before and after). So **a published
`finalized` retreats when `safe` does; nothing latches it** — in this harness the pruning point is adjacent to `safe`. On a real chain `finalized` is a
pruning point at least 74,920 blue score behind the tip (10.1), and a conviction-driven retraction only moves `safe` back by the few effects the retracted
claim supported, so a retraction cannot reach `finalized`; a `finalizedConflict` needs a reorg deeper than the pruning depth, which the 600-blue finality
bound already refuses.

#### 10.3.4 What `safe` and `finalized` do under each value — the summary

| | v1 (5,400) | `Final + 3,000` | `Final + 1,000` | `Final + 600` |
|---|---|---|---|---|
| `safe` covers an ordinary claim's block at (honest, `D = 1`) | acceptance + 5,400 | + 3,123 | + 1,123 | + 723 |
| Conviction can still reverse the `Final` through | `Final + 3,000` | same | same | same |
| ... and the rule counts it from | `Final + 5,277` | `Final + 3,000` | `Final + 1,000` | `Final + 600` |
| `safe` retracted by a conviction | never | never (zero slack) | for 2,000 blocks of convictions | for 2,400 blocks |
| DA: counted while a session is open | never | never | never (loses `safe` at the accusation if it had it) | same |
| DA default can land as late as | `Final + 4,201` — before the rule's instant | `Final + 4,201` — after the instant, but the open session withholds the fact from the accusation on | same | same |
| late private branch | refused from `Final` (+123) by the comparator, and by the 600-blue bound after | same | same | same |
| `finalized` | pruning-point-driven (>= 74,920 blue behind); not moved by `M` | same | same | same |
| What else must change for it to be safe | nothing | a policy field + version (v2 rule), the zero-slack caution, the class model-court window | `window_court` <= 1,000 and everything keyed to it (re-genesis class), or a retraction protocol | the same, harder |

#### 10.3.5 What these tests do not show (GAP)

* **No fork race.** A fork's chance of winning is PALW capacity security; this branch proves nothing about common prefix. (g) shows the veto
  closes at `Final` for a branch that lacks the claim; it says nothing about a colluding branch that carries its own.
* One claim, `D = 1`, floor-relabelled, linear chain; a real policy lengthens every lag by `D x F_off` DAA.
* The fixture's honest-quorum `F_off = 123` is one lifecycle; a re-bound claim finalizes later (up to +2,520) and the v1 rule takes the `max` for it.
* The griefing cost of DA sessions is not measured: a session withholds the fact for 1,201 blocks; per-claim budgets (4 per seat over its life, 3 non-seat
  open at once, 16 non-seat in all) and the retention gate (`now + W_disclose <= trace_retention`) bound it; what it costs the accuser if refuted (the
  accuser is charged, SA-4) is the DA court's economics (ADR-0152), not exercised here.
* Pre-`Final` withholding (a claim that never finalizes) is `dos_repro_4` (another lane's); here the claim starts at `Final`.
* A class's own model-court window, which can extend a claim's retention, is not exercised.
* The explorer does not render `nativeReadiness`; no external SDK reads it.

#### 10.3.6 Reproduce

```
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus-core --lib palw_native_readiness_v1 palw_native_settlement_v1                  # 22
cargo test --offline -p kaspa-consensus-core --test rfc0012_safe_maturity_attacks --test rfc0012_native_evidence_fold -- --nocapture --test-threads=1   # 6 + 6
cargo test --offline -p kaspa-consensus --lib --features evm rfc12_                                                     # 20 (x0-x15, r1-r5)
cargo test --offline -p kaspa-consensus --lib --features evm hb_probe_e hb_probe_d -- --nocapture --test-threads=1       # existing (HB lane), 1 + 1 (322 s + 629 s)
RUST_MIN_STACK=33554432 cargo test --offline -p kaspa-rpc-core -p kaspa-grpc-core -p misaka-cli --lib --bins -- rfc0012 palw_settlement   # 8 + 2 + 6
```

## 11. What stands between `palw_dns_retirement_v1` and arming it on t12 (X12b, 2026-10-08)

The release enables RFC-0012 with RFC-0008, 0010, 0011-K2, 0014 and 0015, and ships after all RFC implementation is complete; D1 stays open and the user
decides the value from §10. Nothing below is decided by this lane. **Sizes:** S ≤ 1 day, M ≤ 1 week, L > 1 week of one lane. **Owners** are proposals.

### 11.1 Code (each with a concrete next step)

| # | What | Why it blocks | Next step | Owner / size |
|---|---|---|---|---|
| C1 | **A claim reaching `Final` through the processor with the EVM lane on**, so `safe` advances from the node's own deltas end to end | today the evidence is proven on the fold (§10.3, `d1_*`) and the processor on placed facts (`x8`–`x10`, `r1`–`r5`); the harness re-stamps templates after the build and the lane executes against the header timestamp (`p2_evm_twin.rs` header), so its clock stops at DAA 1 | give the harness's template builder the simulated stamp as an INPUT to the lane (so build == validate with a simulated clock) instead of re-stamping afterwards; then drive `live_life`'s claim (accepted 1,001 → `Final` 1,124) in the matrix and assert `snapshot.safe` moves at the rule's instant and `nativeReadiness` shows `waitingMaturity` counting down on the way. If the builder change is judged too close to consensus, do it on the C-drill network (11.3) with the shipped binary instead | X12 + the EVM-template owner · M |
| C2 | **A real pruned IBD with EVM state** | the import function is called directly (`x7`); a real join, its `latest = pruning point` start and the first `safe` after the lag are not exercised | a two-`Consensus` test through the IBD flow harness on a TEST copy of the params with shrunk pruning depth (the t12 derivation, 74,920 blue, keeps the pruning point at genesis in-process); assert the joined node's snapshot equals a full-replay node's once the lag has elapsed, and that the import clears a planted `FinalizedConflict` | IBD / protocol lane + X12 · L |
| C3 | **Market fills** | the model line has no seeded market, so both sells settle `Refused MARKET_MISSING`; nothing fills across the fence | seed a market through the Position route (lane MP fixtures) in the matrix harness and repeat `x1`/`x12` with a fill: the combined ledger must still move by exactly `coins + fees` | X12 + lane MP · M |
| C4 | **Re-measure D1 on the merged tree with 0008 / 0010 / 0011-K2 / 0014 / 0015 armed** | the horizons in §10.3 are properties of the fold as it is (`F_off = 123`, last reversing block `Final + 3,000`, DA default horizon `Final + 4,201`). Panel-free licensing (0015), permissionless panels (0010), public non-seat prosecution and the court/`Final` race (0014) and claim-backed slices (0008) can move every one of them, and 0014 §7.3 requires reward maturity and the lease `retained_liability_until` to be aligned with the liability horizon | run `rfc0012_safe_maturity_attacks` and `rfc12_r*` unchanged on the integration branch with those fences armed in the TEST `Params` (they read the windows from `Params`); any change in the `d1_b` / `d1_c` last-block numbers is a D1 input and the 10.2 table is re-issued | X12b · S to run, M to adapt |
| C5 | **The maturity rule, if D1 ≠ v1** | `Final + 3,000` needs a committed policy field | `NativeMaturityRuleV1 { V1, FinalPlus(offset) }` in `NativeFactRulesV1`, the lifecycle closure and `PalwSettlementPolicyV1::commitment_bytes` (version bump), `readiness.maturity.rule` reporting it, and the `Rule` enum of `rfc0012_safe_maturity_attacks.rs` promoted to the library with those tests unchanged. Must land BEFORE the release's fingerprint, because the policy id is committed in `PalwDnsRetirementV1::commitment_bytes`. Not needed if v1 stays | X12b · M |
| C6 | **A class's model-court window can extend a claim's retention** (`palw_model_court_window`, palw_state_v2.rs ≈ 30790) | with 0011-K2 / 0014 in the release, check whether any class window can outrun `window_court`; neither maturity rule reads an extension made after `Final` | grep + a fold test that sets a class window above `window_court` and asks `d1_b`/`d1_c`; if it can, the rule must read the class window or the fence must forbid it | X12b · S |
| C7 | **Preset + release wiring** | the fence is `None` on every preset and in no flag-day list | in the release branch only: set `palw_dns_retirement` on the t12 preset (activation, policy, horizon), add it to the fence list and `consensus_params_id` / fork-id inputs, update `rfc0012_fork_id_and_cost` (its TEST fence is 9,137 today) and `validate_palw_v2`'s tests; the policy id of the preset must equal the committed one (test); **requires `palw_fork_choice_rule_e_v1` (ADR-0175) armed at or below** the retirement height | Lead / integration · S |
| C8 | **Explorer renders `nativeReadiness`** | the reasons are served; only the CLI shows them | `contrib/misakascan-t12/app.js` + `tests/native-settlement.cjs`: show `maturity.rule`, `blocking.waits`, `finalized.wait`, `skipped` | X12b · S |
| C9 | **Release-build cost at 100k+ blocks** | §6 is a debug-build measurement and an extrapolation | a release-build benchmark of `certify_native_prefix_v1` on 160,000 effects with real fact density, and a cold rebuild of `NativeRowCache` from a devnet DB | X12b · S |
| C10 | **The DNS "all-equivocating votes" case past the fence** | x2/x13 show DNS is not an input; no equivocation was crafted | one matrix test with equivocating DNS attestations after the fence asserting no effect on the sink or the labels | X12 · S |
| C11 | **Decide whether a published `finalized` may retreat** | `rfc12_r5`: it retreats with `safe` and is not a `FinalizedConflict`; a latch would have to relax the "finalized is an ancestor of safe" API invariant | recommended: leave it (§10.3.3 h: on a real chain `finalized` is >= 74,920 blue behind and a retraction cannot reach it) and say so in the release notes; if the user wants the latch, a design note first | user decision, then X12 · M |

### 11.2 Policy values the user must choose (recommended value, trade-off)

| Value | Recommended | Trade-off |
|---|---|---|
| **D1: the maturity offset** | **keep v1 (5,400 after acceptance) for the first arming; re-decide on the live `nativeReadiness` data** | v1 needs no code, is above every reversal horizon measured (including `Final + 4,201`) and `safe` never retreats for a conviction; the price is a `safe` lag of ≥ 5,400 DAA (≥ 7.5 days at 30 DAA/h, 9.4 days at the observed 24). `Final + 3,000` is the only anchored shorter value (lag 3,124 DAA, 4.3 to 5.4 days): policy-only but needs C5, has zero slack, and depends on the open-session rule (§10.2). `Final + 1,000` / `+ 600` retract `safe` for 2,000 / 2,400 blocks of convictions and cannot be made safe by policy |
| **D (`settled_anchor_depth`)** | **3** | each unit adds ≈ 123 DAA (≈ 4 h at 30 DAA/h) to the lag; 1 would let one anchor (one operator's claim) certify; 3 is the least that a 400 ‰ operator cap can be met with |
| **W (`unique_mature_work`)** | **start at 6,331,093,440 (3 × the smallest REAL class's `pwu`), then set from the census: `⌈k × matured REAL work per 5,400-DAA window⌉`** | too high and `safe` is `null` (it flaps at the edge); too low and dust certifies. The floor is a placeholder, not a census |
| **`max_operator_permille`** | **400** | certifies 3-, 4- and 8-operator populations and a Zipf-8, refuses 1 and 2. With fewer than three entities of matured work `safe` is `null` from day one: either accept that and say so, or set 1000 until the census shows three. A cap is a speed bump (ids are not entities; the 8 genesis cards are one fleet), not a wall |
| **`max_class_permille`** | **1000 (off) until ≥ 3 REAL classes carry matured work, then 600** | with two classes any cap under 500 can never be met and any under 1000 binds the larger one; 0011-K2 onboarding may change the census before the release |
| **`legacy_evidence_horizon_daa`** | **10,080** | never shorter than the DNS evidence right it preserves (`evidence_window_blocks`); exposure to pre-fence DNS evidence ends at the fence + 10,080 DAA (≥ 14 days). Shorter ends that exposure sooner and cuts the right off earlier |
| **The activation height** | the Lead's, after the drill (11.3) is green | the fork-id gate refuses un-upgraded peers FROM the height (before it they stay peers); testnet-11's second flag day cut off six third-party seats within four minutes. Reverting a binary after the height is a fork |
| **Accepting the emission change** | RFC-0012's own design | from the fence, 88,912,402,800 sompi (889.12 MSK, 20 %) per floor attempt block is never minted; a REAL-class claim after the fence escrows 92 % |
| **What integrators are told** | in words: ETH `safe` / `finalized` are `null` for at least the lag after activation and whenever evidence stops; state and call queries at those tags error; `latest` is optimistic | a bridge or exchange keyed on `finalized` stops advancing until the first mature REAL work |

### 11.3 External and ops

* **The drill (release gate 4):** an adversarial multi-node run with zero DNS validators, on the **shipped binary**, crossing the fence, on a network that has real `Final` claims, deposits, withdrawals, market fills and a pruned join with EVM state — C1–C3 are its in-process rehearsals, not substitutes. A drill that does not cross the fence with the shipped binary does not count.
* **Fleet and third-party seats upgraded before the height** (fork-id), with the list of seat builds known; treat it as a flag day for everyone.
* **Wallets, SDKs, EVM tooling, explorers other than misakascan** that read `safe` / `finalized` or `getPalwSettlement` (wire v3 appears only past the fence). EXTERNAL.
* **DNS validator service** stopped or self-retiring; the 20 % validator share ends; their bonds exit (`x11`); notify validators.
* **Runbook and pager** for `FinalizedConflict` (the record §5 has the signal and the resync; the page is OPS).
* **Live census** (operators by entity, REAL classes, `pwu` per window) — the input for W and the caps; data gathering, not code.
* **Release notes** carrying the integrator wording above and the D1 choice in the user's words.

## 12. Wave 2: the code items C1-C11 of section 11 (lane X12c, branch `rfc12/x12-c-items`, 2026-10-08; built and run by X12N, 2026-10-09)

The coordinator asked for every code item of 11.1 to be implemented before the single full-activation release (no DAA-9,000 flag day; RFC-0012 ships with
RFC-0008 / 0010 / 0011-K2 / 0014 / 0015). **Status of this section: written by X12c while builds were on hold; built, fixed and run by X12N on 2026-10-09.
The "Result" column is what that run showed** (implementation record 7.1-7.3 has the commands and every binary's count). Nothing here moves a params /
schedule / identity id: every change is a `cfg(test)` seam, a test, a pure function or a log/RPC field that exists only past the dormant fence (plus one
line that makes the lead-cap parent policy read the same clock as the template builder, which is `unix_now()` outside a test).

| Item | What was written | Where | Result |
|---|---|---|---|
| **C1** a claim reaches `Final` through the processor with the EVM lane on | the builder reads a simulated clock (`template_clock` / `template_now`, `cfg(test)`) set by `T12Chain::arm_clock` on an EVM-active network, so the harness no longer re-stamps; `native_relabel_class` (`cfg(test)`) reads the floor claim as REAL work | `processor.rs`, `native_settlement.rs`, `t12_round_lane_e2e.rs`, `tests/rfc12_c_items.rs` (`c1_a`, `c1_b`, `c1_c` ignored: ~5,400 DAA) | **PASS** (`c1_a`, `c1_b`); `c1_c` (~5,400 DAA) is run once on the release candidate |
| **C2** a pruned join with EVM state | the joiner replays through P, loses everything a pruned joiner lacks (PALW tip and deltas, EVM header and state rows), and installs the PALW carriage and the EVM header + state through the node's own import functions; the blocks above P are taken and compared root for root | `tests/rfc12_c_items.rs` (`c2`) | **PASS**. **Partial by construction:** the UTXO set is the replayed one (the source's pruning UTXO set is advanced by the real pruning processor, which cannot move a pruning point on this clock), the overlay snapshot and the P2P messages are not exercised |
| **C3** market orders across the fence with ADR-0162 armed | ADR-0162 (`palw_model_virtual_v1`, dormant on every preset, the user has not decided it) armed at DAA 0 in a TEST copy of the params; an 80 MSK deposit, a 50 MSK buy and two one-unit sells (signed with `cast`, held to their fields) ride one payload across the fence; the combined ledger is asserted conserved on every block | `tests/rfc12_zero_dns_matrix.rs` (`c3`) | **PASS as restated; a FILL is a GAP.** X12c wrote the test to expect three fills; the run showed ADR-0162 Decision 5 at work: trading opens only at the class's approval (status `Active` and a registry lifecycle of exactly `Active`), and t12's genesis class is `Active` / `Prefetching`. So the buy reverts at the call and keeps its 50 MSK (gas only), the market exists (no more `MARKET_MISSING`) and both sells settle `Refused EXCEEDS_POSITION`, and the ledger conserves on every block. A fill end to end needs an approved class, which the floor-only harness cannot reach (record 7.2) |
| **C4** re-measure D1 with the other lanes armed | `measure_horizons` + tripwire `d1_g`: F_off, the last reversing conviction, the last DA accusation and the latest default asserted as RELATIONS under every lane configuration the fixture can arm (baseline, `palw_model_court_window`). **Not armable here, and why:** `panel_v3` (RFC-0010: a beacon and a draw policy), `kernel_route` / OPV (RFC-0014/0015: a class registry, the interim 50-DAA window), RFC-0008 slices, RFC-0011-K2 class registrations. **Measured against what is merged now; to be redone at integration** by adding a variant | `tests/rfc0012_safe_maturity_attacks.rs` (`d1_g`) | **PASS** on the merged tree (both configurations identical to the baseline); redo at integration |
| **C6** a class's own court window extends retention | `native_facts_and_skips_v1` now reads a claim's LIVE retention from the sink state too (a court opened after `Final` on a class with its own window extends `trace_retention_daa`, palw_state_v2.rs:30818); a fact is mature at the later of the recorded and the live value. Without a class window the two are equal, so nothing changes on any network that does not arm that fence | `palw_native_settlement_v1.rs`; `d1_h` | **PASS** |
| **C8** explorer renders `nativeReadiness` | reasons, clocks, gaps, skipped counts, the maturity rule, and the withdrawn-label alarm | `contrib/misakascan-t12/app.js`, `tests/native-settlement.cjs` | **PASS** (`node`, no cargo) |
| **C9** release-build cost at 100k+ blocks | designed, not run (12.9) | `tests/rfc0012_c9_release_cost.rs` (two `#[ignore]`d tests) | DESIGNED; compiles; not run (by order) |
| **C10** DNS "all validators equivocate" past the fence | `x16`: P is DNS-final on branch X, Q on branch Y, C on neither; the heavier PALW branch wins on all three with the same snapshot and heads; every (rollout stage x health) with an anchor on the abandoned incumbent leaves the outcome the anchorless one; an equivocation-evidence transaction citing a target at or past the fence is refused by name (`DnsLegacyEvidenceOutsideWindow`) | `tests/rfc12_zero_dns_matrix.rs` (`x16`) | **PASS** |
| **C11** may a published `finalized` retreat | decided: yes, to `null` only, never silently (12.7); implemented and tested | `native_settlement.rs`, `processor.rs`, CLI, explorer, `rfc12_r5` | **PASS** |
| **C5** the maturity rule v2 | skipped: the user has not chosen a D1 other than v1 | - | - |
| **C7** preset and release wiring | a ready-to-apply checklist (12.8); not applied | this document | - |


### 12.7 C11 - decided: a published `finalized` may be withdrawn; it is never replaced by a different block and never withdrawn silently

**Decision (X12c, for the user's veto): no latch.** Reasons, in the order they matter:

1. **What the label is.** `finalized` is "the validated pruning point under a certified `safe` prefix" - an *evidence-backed* label, recomputed at every virtual
   change. A latch would keep it standing after the evidence that justified it is gone (a conviction retracts the last work under it, a delta row is lost):
   authority without evidence, which is exactly what the design refuses everywhere else (missing evidence STOPS certification).
2. **A retreat can only be to `null`.** `finalized` is the pruning point, and the pruning point only advances along a chain; the label therefore never moves to an
   *older* or a different block. A reader that acts only on a non-null `finalized` and never walks its own view backwards cannot be misled by a retreat; the unsafe
   behaviour (the label jumping back) does not exist by construction.
3. **The dangerous case already has its own, stronger signal.** A reorg that abandons the published head is the sticky `finalizedConflict` (x7): `safe` and
   `finalized` withheld, an `ERROR` log, the CLI and explorer alarm, cleared only by a validated import.
4. **Reachability.** On a real chain `finalized` sits at least 74,920 blue score behind the tip (10.1). A retreat needs the certified prefix to fall *below* the pruning
   point: a conviction-driven retraction moves `safe` back by the few effects the retracted claim supported (hours of blocks, not 74,920), and a reorg deeper than the
   node's 600-blue finality depth is refused. The harness reaches the case only because its pruning point is adjacent to `safe` (`rfc12_r5`).
5. **A latch is not free.** It would relax the API's ordered-prefix invariant (`finalized` an ancestor of `safe`, enforced by `get_native_settlement_snapshot` and the
   ETH-tag consistency checks) and add persistent consensus-adjacent state.

**What was implemented so the retreat is never silent.** The virtual-change path (`update_evm_canonical_heads` -> `note_finalized_withdrawal`) compares the previous
snapshot's `finalized` with the new one's: when a published head is withdrawn and no conflict is recorded it logs `FINALIZED LABEL WITHDRAWN: <head> was published as
finalized and is still canonical, but at <sink> the evidence no longer certifies it (stop ...)` at `ERROR` level (the durable signal; a pager rule on that string is OPS),
and keeps the head in memory so `getPalwSettlement.nativeReadiness.finalized.withdrawnFrom` names it until a `finalized` is published again (a restart forgets the field,
not the log). The CLI prints it, the explorer renders it as an alarm. Tested: `rfc12_r5` (the withdrawn head is the published one; nothing is named when nothing was
published), core/rpc/gRPC/CLI/explorer round trips. **If the user prefers the latch**, it needs a design note first (it changes the snapshot's invariants), then
`native_evaluate` step 8 reads the previous snapshot - the data is already in hand.

### 12.8 C7 - the activation release: a ready-to-apply checklist (do NOT apply before the release)

Everything below is a release-branch edit; nothing here is applied in wave 2, and every step keeps the fence dormant until the height is chosen.

1. **Pick the height `H` and the policy** (the user, section 11.2): `D`, `W`, the caps, the horizon. Record them in one place: three `const`s next to the entry below.
2. **The entry** - in `consensus/core/src/palw_native_settlement_v1.rs`, in the pattern of `palw_audit_1004_v1::PALW_T12_AUDIT_1004_ENTRY`:
   ```rust
   pub const PALW_T12_SETTLEMENT_POLICY_V1: PalwSettlementPolicyV1 = PalwSettlementPolicyV1 { settled_anchor_depth: /* D */, unique_mature_work: /* W */,
       max_operator_permille: /* 400 */, max_class_permille: /* 1000 */ };
   pub const PALW_T12_LEGACY_EVIDENCE_HORIZON_DAA: u64 = 10_080;
   pub const PALW_T12_DNS_RETIREMENT_ENTRY: crate::config::params::PalwPostLaunchFenceV1 = crate::config::params::PalwPostLaunchFenceV1 {
       name: "palw_dns_retirement_v1",
       set: |params, at| {
           params.palw_dns_retirement = at.map(|activation| PalwDnsRetirementV1 {
               activation, settlement: PALW_T12_SETTLEMENT_POLICY_V1, legacy_evidence_horizon_daa: PALW_T12_LEGACY_EVIDENCE_HORIZON_DAA });
       },
   };
   ```
   `Some(never())` must collapse as every other entry does (`Params::sync_*`; the existing `Some(ForkActivation::never())` handling at params.rs ~6358 is the template). A test holds the name to `Params::palw_fences_v1()` (which already lists `"palw_dns_retirement_v1"`, params.rs:10414).
3. **Append it to the list** the release arms: `PALW_T12_INT13_FENCES_V1` (params.rs:21890) if the full-activation release reuses that list and its height (`PALW_T12_INT13_DAA`, now `None`), else a fresh list and constant. Prerequisites in force already: `ConsensusV2` (`validate_palw_v2` requires it), the EVM lane (genesis). Order: after `palw_audit_1004_v1`.
   **Hard prerequisite (Lead, 2026-10-09): `palw_dns_retirement_v1` requires `palw_fork_choice_rule_e_v1` (ADR-0175) armed at or below its height.** A release list that arms the retirement without it, or above it, is not shippable; `validate_palw_v2` refuses it (FINX). Order in the list: `palw_fork_choice_rule_e_v1` first. The RFC-0012 test fixtures that arm the retirement build their `Config` directly (validated without the retirement, which is set afterwards), so they do not depend on that fence.
4. **Re-pin the identities.** `consensus_params_id` and `consensus_schedule_id` move (params `5ee7fd8e...`, schedule `1678e073...` today): `grep -rln 5ee7fd8e` lists the `*_is_t12_only.rs` / `pruning_proof_strict_economic_fence` / `reorg_strict_win_fence` pins to update; the deploy kit's `contrib/t12-deploy-kit/fleet.env.example` `INT13_FLAG_DAY_DAA=` and the `t12_deploy_kit_constants` test that pins it to `PALW_T12_INT13_DAA`; the drill flag `--palw-drill-int13-at` already arms the whole list on a salted chain - use it for the dry run.
5. **The fork-id gate.** `rfc0012_fork_id_and_cost` arms a TEST fence at 9,137 and proves an upgraded node partitions from an un-upgraded one at the height; re-run it, and move its TEST height if the release height collides with it. The fence is visible to the fork id only if it is in the list BEFORE the build is cut ("once the int-13 build is cut this list is frozen", params.rs:21880).
6. **Gates, in order:** `cargo test -p kaspa-consensus-core --lib` (palw_native_*), `--test rfc0012_*`; `-p kaspa-consensus --features evm rfc12_` (x0-x16, r1-r5, c1-c3); `-p kaspa-rpc-core -p kaspa-grpc-core -p misaka-cli`; the explorer test; then the drill (policy proposal 11.3) on the SHIPPED binary crossing the fence. The `#[ignore]`d `rfc12_c1_c` (~5,400 DAA) and the C9 release-cost measurement are run once, on the release candidate.
7. **Words:** release notes carry the integrator wording of 11.2 and the D1 choice in the user's words; the `FINALIZED LABEL WITHDRAWN` and `FINALIZED CONFLICT` log strings go into the pager rules.

### 12.9 C9 - the release-build cost measurement: DESIGNED, NOT RUN

`consensus/core/tests/rfc0012_c9_release_cost.rs` (two `#[ignore]`d tests, compiled with the others so they cannot rot) and the node-level half described in its module
doc. Protocol: fleet-class hardware, release profile, an idle machine, three runs, the median of each figure recorded verbatim in the implementation record.
* **c9_a** - `certify_native_prefix_v1` at 10k / 40k / 160k / 640k / 1.28M effects in two shapes: *dense* (F = N/2, the stress shape of section 6) and *measured* (one REAL
  claim per 123 blocks, the fold's own `F_off`). PROPOSED gates: <= 40 ms at 160k dense, <= 5 ms at 160k measured, time growth within 1.15x of the input growth per step
  (a superlinear step at the top size says the sort or the anchor map dominates and the sweep needs chunking).
* **c9_b** - `native_safe_readiness_v1` on the same chains with 10,000 open claims and 100 sessions; PROPOSED gate <= 2x the sweep.
* **Node half (to write when run)** - a devnet database (the drill network, or the harness filled with N synthetic delta rows) opened fresh: the FIRST `native_evaluate`
  (every row a store read plus a delta decode) cold-cache and warm-cache, and the SECOND (all hits); RSS before and after (design number 352 B/row, 42 MB per 100k blocks).
  PROPOSED gates: cold first evaluation <= 30 s per 100k blocks on the fleet disk, warm per-change evaluation <= 50 ms. A node that cannot answer a virtual change within
  one block interval when warm is the failing case.
