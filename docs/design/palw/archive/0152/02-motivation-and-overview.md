> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 539–748 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): v2 changes (history), §1 Motivation and the binding operator decisions (§1.5), §2 Decision overview.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## v2 changes (history, kept for traceability)

v2 applied v1's review (X1–X30). The review of v2 confirmed as closed: X1, X3, X4, X5, X6, X8, X10 (as text),
X11 and X13. X2, X9 and X12 were partly closed and are finished in v3 (F3/F8/F11, F15, F13/D6). X7 is closed
with a residual. The v2 list itself:

* **X1.** A conviction binds only the claim it names and that claim's own signers (V-5).
* **X2.** Escrow released at licence only on a full-service quorum or coverage licence, never after a redraw (SR-1).
* **X3.** Burns keyed on the vesting row, not the claim record (V-1, V-5).
* **X4.** Maturity is step 3d, after 3c (V-4).
* **X5.** Maturity is never refused by the market queue (V-7).
* **X6.** S4′ (ConflictingPermit) never runs #8 (V-5).
* **X7.** A post-licence DA default is charged at the CourtFraud tier; any Valid signer may refute (§3.6).
* **X8/X9.** The duty converts into the lock; the fold licenses on the backed subset (L-4, SR-6).
* **X10.** No charge for panel silence. **Superseded at launch by D1** (§3.2 SR-5).
* **X11.** Only `withholding_strikes` is kept as slash memory.
* **X12.** No escalation, ejection status or tombstone; Eq kept whole in v2. **v3 caps Eq on t12 (D6).**
* **X13.** Rows halt during a licence halt; locks still release (V-4(b), V-8).
* **X14.** 2M `c = 1`, held to Final; the 2M row marked NOT satisfying the invariant.
* **X15–X18.** Prerequisites, the fence-aware reload, isolation, counters.
* **X19–X30.** Detection preconditions, B-3 horizon, duty not slashable, door mix, waits, strikes, tests,
  capital pricing, B-2 text, K nominal, the latch, stop-not-skip.

---

## 1. Motivation, measured

§1.1–§1.3 are v2's text; their line references are at `50565f55`.

### 1.1 What a floor claim holds today

Under option A, a floor claim reserves its whole fraud gain on its producer's bond from acceptance
until Final:

| term | MSK | source |
|---|---|---|
| escrow `E` | **3,200.8465** | subsidy 444,562,014,000 sompi (`params.rs:15188`) × carve 720‰ (`claim_escrow_reservation_v1`, `palw_state_v2.rs:1644`) |
| weight `w` | 0.1075 (0.0004 in the one-attempt fold fixture) | `palw_admission_v2.rs:611-622` |
| **reservation** | **3,200.95** | `palw_claim_bond_reservation_v1` (`palw_state_v2.rs:2590`; at `e93be0f2` STATE:2765) |

At 500‰ (`palw_fp_devnet_v3.rs:420`), a 13,000 MSK bond (`palw_fp_devnet_v3.rs:902`) holds two
concurrent floor claims.

**The normal hold is 142–147 DAA:** anchor 20 (`processor.rs:8377`); licence 1–6 DAA after the bind; Final at
licence + 121 (`window_challenge_at` = 120, `palw_state_v2.rs:1195`, and the sweep breaks at `deadline >= daa`,
`:15519`); release at Final (`:12503`). That is 4.7–4.9 h at 120 s/DAA and 7.9–8.2 h at the measured ~200 s/DAA.

### 1.2 The reservation protects the interval that needs it least

At Final the escrow is named a payout (`:12578-12595`), minted by the next coinbase (`processor.rs:5197`), and
spendable after Decision A's maturity. `reverse_convicted_final` ((dos) `:10516`) does not return payouts, so
post-Final recovery is only the seat lock: 3 × 1,173.69 MSK. The producer's 3,200.95 prices only pre-licence
failures (a failed fraud attempt, P0-10 fake-root occupancy, withholding). It is double margin on L→F and covers
nothing after Final.

### 1.3 What the lock costs

The Valid lock is 97% of a floor claim's capital × time: 1,173.69 per signer, held 3,121 DAA. A 130,000 MSK seat
holds 110 locks. The eight genesis seats license at most 0.41–0.68 claims/DAA. The two ledgers do not see each
other, so commitments reach 150% of posted collateral (finding 12, `dos_l5_4b`).

### 1.4 Why pure no-reserve fails, and what attribution adds

An attempt that never licenses costs its producer only what was reserved against it **and actually charged**.
At a reservation of D = 6.5 MSK, P0-10 lane capture costs 4,680 MSK/day (today 2.30M/day), the undetected-fraud
threshold P* falls from 0.50 to 6.1%, and withholding amplification rises to about 250×.

The second review showed that "charged" was the weak word in v2. Under v2's X10 a failed attempt was charged
only if it was **attributed**, and three attribution paths did not exist: a borrowed-root claim answers every DA
accusation with genuine material (F1), the execution-proving gate refuses every block-lane claim (F2), and the DA
court cannot convict an offender that holds its data (F3). So v3 does two things:

* It **keeps today's charge** (RT#2's forfeit, D1) until attribution is proven by fold tests on real
  producer-built claims.
* It **builds the attribution paths** (§3.10, §3.11), and only then, by a later fence, moves the charge from
  "timed out" to "attributed" (X10).

### 1.5 Operator decisions (binding)

**v1 decisions, 2026-09-24:**
1. R-core+.
2. The honest reward waits about F + 3,000 DAA plus the second clock.
3. Release at licence.
4. Rows mature on the lock's two clocks, which is not Decision A.
5. Values: producer floor 13,000 MSK; seat floor 130,000 MSK; exposure ceiling 500‰; `window_challenge_at`
   120; court 3,000; withdrawal 7,500; admission audit every 100 DAA (`params.rs:14260`).
6. Principles: cumulative is kept separate from concurrent; the bond is standing stake; slashing is
   action-based; throughput caps apply instead of bond caps; **max definitely-extractable profit ≤
   definitely-recoverable value**, given conviction in its window.

**v2 decisions, 2026-09-24:** the slash rates (as amended); the duty's λ term; ready-seat concurrency for floor
and 8k; heartbeat blocks MUST carry conviction objects; X2 held to Final when a seat filed Withheld or
Unavailable; S1 strikes only on DA-confirmed withholding; no S5(a)/(b), no tombstones; `c_2M = 1` held to Final;
a ~10% reporter reward through the vesting path.

**v3 decisions, 2026-09-24 (given to both sessions; they override v2 and the review):**
1. **X10.** Keep the current RT#2 slash (#10's full forfeit) until F1, F2 and F3 are GREEN. A temporary cap is
   acceptable if the full slash is too much, never a free timeout. X10 arms later, by fence, only after the
   mandated order completes.
2. **Doors.** On t12, S3 Valids never count toward the quorum; S3 is audit and fraud detection only. A quorum
   Valid is a full replay, or an S1 receipt with sufficient coverage. S2 may remain as a fast path, but never
   as the basis for final economic weight. Keep the doors enabled and recount k. Exclude sample-only signers
   from post-Final lock slashing.
3. **The DA court redesign goes into this ADR**: multiple indices; no single-session monopoly (sessions keyed by
   `(claim, accuser)`, a per-claim cap well above 1, more challenges allowed for a period); challenge after
   licence and after Final (against unmatured vesting rows); pause credit; Incapable ≠ served.
4. **m ≥ 3**, not 1. Safety must not rest on S2/S3 detection probability. Prefer "several independent pieces of
   evidence required" over raising the producer minimum; the producer floor stays 13,000 MSK.
5. **Reporter reward:** commit–reveal, earliest matching commitment paid, rooted with its DAA. A refuted
   accusation costs ≥ r·S, capped (for example at `min_collateral`). The reward base is the actual collected
   debit: 0 for a bond past its withdrawal gate or already released. The consumed offence records the actual
   debit, and `R = ⌊r·max(0, debit − X)⌋`.
6. **Eq** gets a t12-only cap, for example `min(C, k·G)`, until runtime nondeterminism (Q1) is settled. The
   mainnet value is a separate decision.
7. **2M** stays conservative until ADR-0153: keep the RT#2 slash, no S0, and `c_2M = 1` held to Final.

**Launch policy (operator):** complete R-core+ and launch t12. The long soak and the drills run on public t12
after launch. The regenesis gate is F1–F4 GREEN, the fold/reorg/restart tests, and a short drill (§8.3). v2's
9-day T10 becomes a post-launch observation program with live pass criteria (§8.4).

**v3.1 decisions, 2026-09-24 (operator):**
1. **F1-M (a).** Model-class fraud — an 8k or 2M relabel, and a forged output on tiled/A16 decode — must be
   attributable **at launch**, inside the regenesis gate. The launch keeps model-class production: no floor-only
   launch and no model class held back at start. (T-2(b)'s hold to Final is a rule of the panel room, not a production
   hold; after post-edit 5 it applies to C7, the 2M row, only.)
2. **F1c.** Garbage logits on an honest step tree (`LogitsNotStepOutput`) is inside the gate.
3. **Q4. The panel draw becomes stake-weighted before launch** (§3.14), reversing ADR-0124 D5 / ADR-0130's one ticket
   per operator on t12, instead of accepting v3's residual (an undetected colluding segment pair EV-positive at about 20
   uniform Sybil operators, 2.60M MSK).
4. **Q1–Q3 and Q5–Q8 take v3's defaults** (§9).

**v3.1 post-edit decisions, 2026-09-24 (operator, relayed by the audit session; table "v3.1 post-edits"):**
1. **SEAT-0** (the seat-only material check, ADDENDUM §4-bis.10) is patched on the **live** t12 fleet after a drill: the
   fingerprint must stay `c746f07c`, hosts roll one at a time, and the operator confirms the window first. It also goes
   into the launch line (PE-4).
2. **Kimi-kernel classes are refused at admission** under `palw_offence_attribution` (J-5, PE-4).
3. **The FP `output_root` rendered rule is unified**, so `OutputMismatch` (10) covers FP model claims; FP roots and
   `misaka-palw-derive` move once, at the regenesis (J-5, PE-4).
4. **The canonical job is fixed to `(n_ctx/8 − 1, 2)`** for model classes under the fence; registrants lose the choice
   (J-5, PE-4).
5. **SEAT-R ships in the same binary as F2's fence** (Q-7, PE-3).
6. **U1:** the escrow is held to Final only for C7 (2M); 8k releases per SR-1 (SR-1 cond. 4, PE-12).
7. **U2:** ejection is the capital predicates **plus a producer floor gate** (posted collateral ≥ 13,000 MSK to
   produce), amending B-4; an honest 13k producer that takes one S0′ tops up before producing again (B-4, PE-12).
   The decision names no top-up mechanism and the code has none, so today "tops up" means re-registering under a new
   bond key and operator identity; an in-place top-up is open (§9.3 Q11).
8. **U3:** an FP claim convicted after Final charges its executor a producer action tier capped at `m × G_fp` (§3.6,
   PE-12).

**Agreed with the audit session, 2026-09-24:** the vesting row copies `job_identity`, `free_prompt`, `trace_root` and
`segment_count`; `ProducerWithholding` stays a contradiction against `Valid` signers only when the withholding is
DA-confirmed, never against `Sampled`, `Incapable` or `Unavailable` filers, and at the S tier capped by m = 3 (`3 × G`);
the kaspad filer is Phase 2. The audit's spec (`f2f1_spec.md`) is authoritative for F1/F2 names, types and
discriminants.

**Decisions of 2026-09-24 taken after v3.1 (operator; recorded by the integration amendments):**
1. **Held attention (4-ter; §3.9).** 8k is made attributable by A-held (C1–C5, object 57) inside the launch gate.
   Partial-mask signers are not bound by a dissection or by a DA default at launch (no v22 field; IA-11). An attention
   lie found after Final moves to Phase 2.
2. **2M is closed at launch (U-D1)**, attempt and FP alike. It opens only at a flag day that installs a measured row,
   and only if it is attributable and an honest seat can prove a lie inside the deadline. U-D2…U-D10 take the
   recommended options (§3.9). This supersedes 4-ter's "(C): accept 2M under an economic bound" for the launch.
3. **Deadlines split by kind** (R response, C compute-bearing verification, E economic liability, K capacity). Only C
   is derived per class, and E never releases before C can conclude (4-quater, §3.9).
4. **P-1:** t12's pruning depth is fixed at the regenesis at ≈ 74,920 DAA (D_cap 16,000) **without waiting for M12**.
5. **Drills and measurements after launch.** The SEAT-0 drill, M5, M5b, M9, M12 and the 8k real-weight timing drill
   run on public t12 after launch; implementation goes first (§8.4).
6. **A court default (S-4 deviation 2; IA-9).** A court default is charged the forfeit plus S2's action, so silence is
   never cheaper than losing, and writes no `CourtConviction` record. IA-9's other S-4 rules are not operator
   decisions: the kind-3 record's contents are S-4's implementation, the live-lock rule is the audit's #12, and the
   9-entry strike bound is a correction of v3.1's arithmetic.

---

## 2. Decision overview

| pillar | one line | rules |
|---|---|---|
| Bond | one slashable account; exit bounded (the DA lattice included); no ejection status or tombstone; a producer floor gate (U2: 13,000 MSK posted to produce); Eq capped on t12 | B-1…B-5 |
| Staged reservation | `w + E` until licence; `w` after a licence where every seat carried a `Valid` and `basis_k ≥ 2` (or a completing supplementary receipt by `min(L + 60, bound + window_receipt)`); otherwise `w + E` to Final, and always for C7 (2M, U1); the free-prompt abandon hold kept | SR-1…SR-10 |
| Charging at launch | the second failed panel forfeits the reservation (D1); a first failed panel redraws, uncharged. X10's attributed-only charging arms later by fence. 2M never gets S0 for a producer-attributable void (D7) | SR-5 |
| Attribution (F1/F2) | the audit's spec: one adjudicator `palw_check_panel_false_valid_v2` binding by root (`PanelFalseValidV2 = 3`); the claim records `job_identity`; identity checks J1–J5 and contradictions 9–13 (F1-M and F1c included; 13 = `PromptNotAnchored`); `ExecutorRefuted = 4`; `ClaimUnderSession` on an open court only; model-class admission pinned (canonical job, head predicate, no Kimi kernel); the shard court's one move bound | J-1…J-8 |
| Panel draw | stake-weighted: one key `L/W` per eligible operator (W = its one bond's posted collateral, capped at 1,000,000 MSK), the smallest keys sit; the outsider too, the admission jury not; drawn only in its anchor block, the first attempt block at or past the slot (IA-1a), on the state acceptance reads; no bind below 875‰ of the base weight eligible, the executor's capped weight counted on both sides (fail closed; IA-1b); the rate room counts effective ready operators | SW-1…SW-10 |
| DA court (F3) | sessions per `(claim, accuser)`; drawn units inside the committed run; live, licensed and Final-with-row claims; pause credit for seat sessions; a session re-keys the row and keeps the locks live | DA-1…DA-9, DL-1 |
| Quorum counting (F4) | `Sampled` never counts and never serves; `basis_k` recounted per segment over every counted `Valid`; S2 never reaches Final alone and never ticks the second clock; located faults bind only the signers that replayed them | Q-1…Q-7 |
| Vesting rows | `Final` writes rows; rows mature on the lock's clocks, not in a licence halt, not while a DA session is open (implied by the re-key); burned by row | V-1…V-8 |
| Heartbeat carriers | conviction, DA and reveal objects ride heartbeat blocks | H-1 |
| Seat lock | `lock = palw_seat_lock_required_v2(G_res, k') + ⌈100‰·E_v/k'⌉`, `k' = max(basis_k, 2)`, the recount's k, not the door's; as armed, `s` at its cap `5%·E` (IA-3, IA-5) | L-1…L-4 |
| One ledger | `palw_bond_committed_v1 ≤ 500‰`; duty `min(max(λ, lock_2), ⌊(w+E)/seats⌋)` converts into the lock; accuser exposure on the free half; one invariant `committed + accuser ≤ C` at every gate (IA-2) | A-1…A-6 |
| Action slash | attributable acts only (plus RT#2 until X10), capped at `3 × G`; Eq `min(C, 3·G)` on t12; an FP executor convicted after Final takes `min(25%·C, 3·G_fp)` (U3); a court default is charged as a fraud (IA-9); one funnel opens and closes every conviction (S-4) | §3.6 |
| Reporter reward | commit–reveal on execution-proving convictions (and Eq); only a proven conviction opens a reward, never a court default (IA-10); a DA default pays its earliest defaulted accuser; 10% of the actual collected debit less X; not an incentive against a rational offender | §3.6 R |
| Throughput caps | cadence; the rate room of `e93be0f2`; C7 (2M) held to Final and to its static cap (`f8c91f19`, re-keyed on C7 by post-edit 5; 8k is released at licence); the unminted-reward ceiling | T-1…T-4 |

**Values** (t12 genesis rows, MSK; v3 script):

| symbol | meaning | floor | 8k | 2M |
|---|---|---|---|---|
| `E` | escrow | 3,200.85 | 3,200.85 | 3,200.85 |
| `w` | weight reservation (8k fold-measured, ×20 attempts) | 0.1075 | 494.32 | 59,742.94 |
| `R` | realizable rights (`claim_realizable_rights_v1`) | 0.01 | 49.45 | 655.37 |
| `s` | buyback bound; 0 on every genesis row (no line, no pair at launch). **The lock the fold posts takes `s` at its cap, `5%·E` (IA-3)**, so the lock values below are L-1 at `s = 0`, which `rcore_whole_gain_and_buyback` pins as the residual at `s = 0`; the armed price is about `s_cap / k` higher (≈ 53 MSK on `lock_3`, the test's 8k figure) and is not tabulated here | 0 | 0 | 0 |
| `G_res` | `w + R + s` | 0.12 | 543.77 | 60,398.31 |
| `G` | `E + G_res` | 3,200.96 | 3,744.62 | 63,599.16 |
| `3G` | action cap (m = 3) | 9,602.89 | 11,233.85 | 190,797.47 |
| `lock_3` / `lock_2` | per-signer lock at k = 3 / 2 | 106.74 / 160.11 | 306.08 / 459.12 | 22,252.74 / 33,379.11 |
| duty (v3) | `min(max(λ, lock_2), ⌊(w+E)/5⌋)`, λ-term 256.07 | 256.07 | **459.12** | **12,588.76** |
| withholding amplification | `5 × duty / (w+E)`, at `s = 0` (the 8k duty is `lock_2`, so it moves with the armed lock; IA-3) | 0.4000 | 0.6212 | 1.0000 |

---
