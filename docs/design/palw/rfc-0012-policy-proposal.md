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

**Why `Final + 3,000` is the only anchored value below 5,400.** Both ways the chain can take back a `Final` close at the same instant by
construction: the court (a prosecution opened at `Final` completes in ≤ 2,988 ≤ 3,000, `W_court`) and DA (a `Final` claim is accusable only while it
is held and its vesting row is unmatured — `da_admission_v1` palw_state_v2.rs:≈16560, released by the DAA clock at `F + 3,000` — and a session
is cut off when the claim retires, so the last accusation that can default is at `retirement − W_disclose = F + 1,800` and a default lands by `F + 3,000`). Retirement removes the claim, so a later
conviction can slash but cannot reverse (`reverse_convicted_final` returns). Counting evidence at `F + C` therefore never counts work the chain
can still un-count, so `safe` cannot retreat because of a conviction — the property v1 buys with 5,400. The rest of v1's margin (2,277 DAA) protects
the producer's *unenforced* retention after retirement, which no chain rule can use, and the slash-only tail, which v1 does not cover either (it
ends at F+5,277, before F+6,000).

**What each shorter value admits, and what would have to change** (attack timing is demonstrated with the real fold in 10.3; profitability is
analysis, not measurement):

* **`Final + 1,000` / `+ 600`** count evidence while the claim can still be convicted or defaulted. Producer + every seat colluding can then
  produce `safe`-grade evidence from a fabricated execution that a public verifier — who needs up to 2,988 DAA to dissect — has no time to
  convict first; withheld material (DA default needs a session opened by `F + 1,800`) is counted though a default could still land. The
  collateral design bar prices fraud gain against recoverable value; a *label* that an external bridge acts on is a **new gain term the bar
  does not price**. A conviction then retracts the evidence (implemented), so `safe` retreats, but whoever acted on it has already acted. Making
  these safe is **not a policy change**: `window_court` would have to fall to ≤ X (the ladder must shrink or the turn clock speed up),
  `claim_retirement`, the liability period, `withdrawal_delay`, the pruning lattice and the retention pin follow — all inside
  `palw_ruleset_id_v2`, i.e. a re-genesis-class change — or the label would need a stake-weighted discount and a consumer-visible retraction
  protocol that this branch does not specify.
* **`Final + 3,000`** is a policy-only change (a v2 rule: `matured = Final + claim_retirement`, drop the retention term; a committed policy field
  and a version bump), with two stated cautions: (1) it ignores a class's own model court window (`palw_model_court_window`), which can outrun
  `window_court` and extends the claim's retention (palw_state_v2.rs ≈30790) — neither rule reads an extension made *after* `Final`, so if that
  fence is armed the class window must be read; (2) the slash-only tail (F+3,000…F+6,000) stays open exactly as under v1.
* **v1 (5,400)** needs nothing more; its cost is the lag and its slack.

### 10.3 Tests and measurements

*(filled in by the attack tests and the readiness RPC below)*
