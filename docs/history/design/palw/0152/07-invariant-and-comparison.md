> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 3096–3395 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §4 the invariant per attacker strategy, §5 comparison table.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 4. The invariant, per attacker strategy

Owner B (the text), A (the tests). Tests T06, T07, T39.

### 4.1 The invariant and its preconditions

**Invariant:** *max definitely-extractable ≤ definitely-recoverable (net of reporter reward)*.

**Preconditions**, each named in §8 or §9:
* **(i) Filed.** At launch a failed attempt is charged without filing (S0′). After X10 a charge needs a filing in its
  window. **Filing rests on Phase 2's automatic filers** (P2-6 DA, P2-7 disclosure, P2-8/8b/8c refutations), run by
  honest nodes as node policy, and on DA-6's refund, which makes a correct filing cost the filer nothing. It does
  **not** rest on R: a rational offender can pre-commit its own contradiction and capture R (R-3, V3S-03). O-3
  measures filing live.
* **(ii) Provable.** By a contradiction the audit's adjudicator admits (J-3…J-5: arithmetic, structural, forged output
  plain or tiled, `IdentityMismatch`, `OutputMismatch`, `LogitsNotStepOutput`, `PromptNotAnchored`) or a DA default
  (DA-7). With F1-M and F1c in the gate this holds on the floor, 8k and 2M for relabels and forged tiled tokens (on 2M
  a prompt root with no preimage through 13, post-edit 1), on FP model claims for a forged output (10, post-edit 4),
  and on the floor for garbage logits. Since F3, the X2 class is provable after Final while its row is unmatured and
  inside DA-8's window. **Not yet provable (post-edit 8, a launch gate):** an arithmetic lie in an `AttnFused` step leaf
  on a held-context class (8k, 2M) (§4.2 #18).
* **(iii) Detection latency fits `window_court`.** Floor and 8k replays take minutes; 2M does not fit (T-2(b)).
* **(iv) Heartbeat carriers** (H-1).
* **(v) Runtime determinism across seats** (Q1). S2/S3/S4 by arithmetic kinds depend on it on float lanes.
  `IdentityMismatch`, `OutputMismatch` and DA defaults do not.
* **(vi) Weight is valued at `w`.** False for 2M (T-4).
* **(vii) The second-clock depth is 30**, a starting point (`params.rs:14244-14255`; retuned from O-5).
* **(viii) The draw is not captured.** **v3.1:** the draw is stake-weighted (§3.14): each eligible operator's chance
  follows the posted collateral of its one bond, capped at 1,000,000 MSK, by successive sampling without replacement,
  one seat per operator, on one state in the anchor block (SW-8), and only while ≥ 875‰ of the base weight is eligible
  (SW-10). On t12 an operator id is one bond and its key possession is proven, but a key is free (B-4 as corrected by
  SW-A3), so an attacker splits its stake into operators at the 130,000 MSK seat floor, and the probabilities below use
  that best split against the eight genesis operators at 939,063 MSK each (verified against the code for the unweighted
  draw it replaces; the executor's own operator is excluded, which T06 applies on the real draw). v3 used a uniform
  draw of 5 from `N = 8 + m` operators, which the script reproduces for comparison.

### 4.2 Per strategy (F5)

"Launch" means X10 dormant (S0′ in force). Actions assume a 13,000 MSK producer and 130,000 MSK seats at m = 3.

| # | strategy | extractable | recovered at launch | recovered after X10 |
|---|---|---|---|---|
| 1a | **naive** failed attempt (no material) | 0 | `w + E` at RT#2, or S1 + strike on a DA default | S1 if filed; 0 if not |
| 1b | **garbage** failed attempt (correct job, garbage trace, answers DA) | 0 | `w + E` at RT#2, or `w + E + min(10%·C, 3G)` if refuted first (J-6) | S2 if refuted; 0 if not |
| 1c | **borrowed** failed attempt (another job's roots, or a relabelled honest run) | 0 | `w + E` at RT#2, or S1 at the first DA answer (the identity rule, J-5), or S2 by `ExecutorRefuted{IdentityMismatch}` | S1 / S2 if filed |
| 2 | **colluding licence** (§4.3) | `G` if undetected | an honest assigned attester of the lied segment detects (served) or files DA (unserved); undetected only if the full seat and the lied segment's holder are both the attacker's | same |
| 3 | caught before licence | 0 | as 1, with S1 or S2 | as 1 |
| 4 | caught L→F | 0 | full-service: `w + Σlocks` (floor 320.32); held (Withheld, Incapable, `Sampled`, missing, redrawn, S2, or `basis_k < 2`): `w + E + Σlocks` (floor 3,521.17); plus S2 on the producer and S4 on each covering signer | same |
| 5 | caught after Final, row unmatured (including after retirement) | `G_res` | row `E_v` + `Σlocks` + actions − R; floor V1 attacker net **−35,579.62** (§3.6 R) | same |
| 6 | **X2** (a quorum that serves only itself) | `G_res` if undetected | the unserved seats' sessions pause the claim and force disclosure or S1 (DA-9); a default charges the commitment plus S4 on covering signers; after Final: row burn + S3 + S4, the locks kept live (L-3) | same |
| 7 | withholding | 0 | DA-confirmed: the commitment, amplification 0.40 / 0.62 / 1.00 (at `s = 0`, §4.4); unaccused: S0′ if both panels fail | DA-confirmed only |
| 8 | a colluding quorum running N frauds | per claim | per claim, independently; each conviction binds only its own claim (X1) | same |
| 9 | Sybil seat | — | the row burn + the lock + S4 (capped at 3G); the duty is not slashable (A-4); `Sampled` is never slashed | same |
| 10 | licence halt | — | rows do not mature; recovery during a halt is the row alone: `E_v ≥ G_res` for floor and 8k, **not** for 2M | same |
| 11 | **2M top-up void** (F15): ≥ 3 seats of an honest 2M claim over-commit between bind and licence | — | the honest producer forfeits `w + E` (62,943.79) when the second panel fails too (D7) | same (C7 keeps S0′) |
| 12 | **silent-quorum griefing** (D1's accepted trade-off) | — | an honest producer forfeits `w + E` when **both** panels fail silently; the review's figure is 800 MSK per floor claim at 8 Sybils (not recomputed). A first failed panel, including an S2 claim that cannot upgrade, only redraws (V3S-01) | S0: free for the producer |
| 13 | **the undetectable coverage lie** (C3, V3S-05): both assigned attesters of the lied segment are the attacker's, every other seat served and replays clean segments | `G` | nothing but S3 sampling can see it (`q_min`, §4.3). **v3.1, stake-weighted draw:** EV-positive from **17.29M MSK of Sybil stake** (133 operators at 130k, 69.7% of eligible panel stake beside the eight genesis seats), and from **12.74M** in the worst saturation state SW-10's 875‰ floor admits (6 of 8 genesis seats eligible); from **8.32M** (worst 6.63M with IA-1b's executor term, 7.15M before it) if a failed first panel's honest attester files nothing (a free redraw); **13.39M** (worst 9.88M) if it fails to file only when offline (30%, model). v3's uniform draw: 20 operators (2.60M), 10 (1.30M), 16 (2.08M) | same |
| 14 | **row delay by accusation** (V3S-02's fix) | — | an accuser delays an honest row by up to `W_disclose + window_challenge_at` = 1,320 DAA per session, at its exposure (325.00 at FinalRow, 13k producer) burned at retirement unless the claim is convicted | same |
| 15 | **rows past DA's reach** (C8) | per row | a row unmatured beyond DA-8's window (≈ F + 4,053) is reachable only by execution-proving convictions (T28), not by DA | same |
| 16 | **partial seats cannot answer outside their segment** (C7) | — | with the full seat colluding and the producer silent, a DA default charges the producer and the covering (colluding) signers; honest partial seats are not charged. **As integrated (IA-11), only full-mask signers cover a unit**, so a colluding partial seat is not charged either (under-charge, never an over-charge), and the signer half waits for `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` | same |
| 17 | **one heavy ready operator shrinks a class's room** (SW-A6, SW-9) | — | the cap bounds it (8 genesis + 40 × 130k stays at `ready_eff` 13 whatever one operator holds); a class held only by small operators still drops from 40 to 6 when one ready operator at the cap joins. Throughput, not safety: the room refuses, never over-admits | same |
| 18 | **an arithmetic lie in an `AttnFused` step leaf on a held-context class** (8k, 2M; post-edit 8) | `G` if undetected | **no conviction route today**: the checker returns `NeedsDissection`, the named fused leaf is refused as a DA unit (DA-3), and no held-dissection close convicts it (J-8). **Open; it gates launch** (§8.3 item 7). The audit's design pass weighs (A) a chunked attention proof, (B) K/V rows as committed outputs plus held-DA (touches producer/engine commitments and P2-7), (C) an economic bound. **Decided 2026-09-24 (§3.9):** 8k by A-held (C1–C5, object 57), in the gate; 2M accepted as (C) — 63,599 MSK per claim uncovered, `c_2M = 1`, at most 9.35M MSK a month. **Superseded for 2M by U-D1 (IA-12): 2M is closed at launch**, and opens only when attributable and provable within D | same |

**Branch 6, the X2 break-even at m = 3.** The L→F catch probability above which an X2 fraud loses money, with the
escrow held and the three colluding Valid signers charged S4 under X7 (v3 script):

| | floor | 8k | 2M |
|---|---|---|---|
| v2 (no S4, m = 1) | 0.476 | 0.448 | 0.329 |
| **v3** | **0.0901** | **0.0890** | **0.2187** |

* **Where the catch lands (V3S-04).** With P2-6 an unserved seat accuses at the licence and its session pauses the
  L→F deadline (DA-5), so the catch is an L→F catch and the row above applies, provided DA-9's rounds fit. The draft's
  P2-6 waited 600 DAA from bind, after an X2 quorum's Final (bind + 127), which made the L→F break-even unreachable.
* **A catch after Final** keeps S4 because the locks follow the re-keyed row (L-3): the post-Final break-even stays
  `q*post` of §4.3 (floor V1 0.0900). If the locks had died at F + 3,000 it would be 0.4962 (floor), 0.5805 (8k) and
  9.86 (2M, never deterred) (`v3_numbers_r2.py`).
* With F3 the catch is "one honest seat that replays and files" (DA-9), proven on the floor by T66 and **UNVERIFIED**
  on 8k and 2M.

### 4.3 Success probability per door, and q (F6, D4, C3, V3S-05)

**What the invariant rests on.** D4 says safety must not rest on S2/S3 sampling. A lie in segment i is **undetectable
without sampling** only if nobody honest replays segment i and nobody honest was withheld from: then no honest node can
refute it (a served honest attester of segment i replays it and refutes; an unserved honest seat files DA, DA-9). In
every door this means **the full seat and segment i's holder are both the attacker's** (probability P2), because those
two are the only seats that attest segment i in the shipped geometry, and any other route leaves an honest seat
either served (and detecting) or unserved (and filing):
* **Coverage** needs every seat's `Valid`, so the three other seats are served and replay clean segments: success = P2.
* **V1** (≥ 3 V2 `Valid`s, any seats; genesis classes have no outsider) licenses with probability P3 when the attacker
  withholds from the honest seats, but those seats file at the licence (P2-6), their sessions pause the claim, and the
  disclosed binding lets them locate the lie (J-6). V1 is undetectable only when all five seats are the attacker's
  (P5), or, serving everyone, when the full seat and segment i's holder are among its signers (≤ P2).
* **A redraw is not free.** A lying producer whose first panel is unfavourable must withhold from an honest required
  attester, which files before its receipt window closes (P2-6); the seat session pauses the claim, so the producer
  answers (and is located by J-6) or defaults (S1) before any redraw. The redraw is free only when that attester
  files nothing.

So the binding quantity is the attacker's success probability against the failed-attempt break-even `P*`, not a
post-licence detection probability, and it depends on whether honest unserved seats file:

**P\* (branch 2, a failed attempt is charged).** Floor 0.500 at RT#2 (S0′) and 0.584 with `ExecutorRefuted` (S2);
8k 0.497 and 0.572; 2M 0.497 and 0.503.

**P2, P3 and P5 under the stake-weighted draw (v3.1; §3.14, §4.1 (viii)).** The attacker holds stake S in
`m = ⌊S / 130,000⌋` equal operators (its best split, SW-7) beside the eight genesis operators (939,063 MSK each, the
honest base of 7,512,504 MSK). `A` is the number of its seats in a successive-sampling draw of 5. Then
`P2 = E[A(A − 1)] / 20` (exact under any draw, SW-6), `P3 = P(A ≥ 3)` and `P5 = P(A = 5)`. EV per attempt at RT#2 is
`P·G − (1 − P)·(w + E)`, in MSK, with no post-licence detection counted (`v31_stake_draw.py`, exact enumeration). The
attacker also needs its 13,000 MSK producer bond, which is not counted (as in v3).

| S (operators) | share of eligible stake | P2 | P3 | P5 | floor EV: P2 / P2 with a free redraw / P3 (no filing) | 8k, same | 2M, same |
|---|---|---|---|---|---|---|---|
| 1.30M (10) | 0.148 | 0.0267 | 0.0278 | 0.0000 | −3,030 / −2,863 / −3,023 | −3,496 / −3,303 / −3,488 | −59,559 / −56,265 / −59,423 |
| 2.60M (20) | 0.257 | 0.0789 | 0.1325 | 0.0007 | −2,696 / −2,231 / −2,353 | −3,108 / −2,568 / −2,709 | −52,964 / −43,770 / −46,178 |
| 5.20M (40) | 0.409 | 0.1875 | 0.3858 | 0.0098 | −2,001 / −1,026 / −731 | −2,301 / −1,167 / −825 | −39,222 / −19,948 / −14,124 |
| 6.63M (51) | 0.469 | 0.2408 | 0.5003 | 0.0203 | −1,659 / −489 / **+2** | −1,903 / −543 / **+27** | −32,469 / −9,333 / **+363** |
| 8.32M (64) | 0.526 | 0.2969 | 0.6069 | 0.0371 | −1,300 / **+37** / +685 | −1,486 / **+67** / +820 | −25,367 / **+1,051** / +13,860 |
| 10.01M (77) | 0.571 | 0.3462 | 0.6879 | 0.0574 | −985 / +464 / +1,203 | −1,120 / +564 / +1,423 | −19,138 / +9,503 / +24,103 |
| 13.39M (103) | 0.641 | 0.4276 | 0.7963 | 0.1041 | −464 / +1,103 / +1,897 | −514 / +1,307 / +2,229 | −8,836 / +22,136 / +37,816 |
| 17.16M (132) | 0.696 | 0.4981 | 0.8665 | 0.1590 | −12 / +1,588 / +2,346 | **+11** / +1,871 / +2,752 | **+90** / +31,726 / +46,709 |
| 17.29M (133) | 0.697 | 0.5003 | 0.8683 | 0.1608 | **+2** / +1,602 / +2,358 | +27 / +1,887 / +2,765 | +360 / +31,996 / +46,939 |
| 26.00M (200) | 0.776 | 0.6115 | 0.9412 | 0.2779 | +714 / +2,235 / +2,824 | +854 / +2,621 / +3,307 | +14,432 / +44,496 / +56,155 |

Bold marks the first positive entry of each column (the free-redraw column turns at 8.32M on the floor, 8.19M on 8k and
2M; the rows show 8.32M).

**Thresholds (smallest attacker stake with EV > 0), against v3's uniform draw:**

| model | stake-weighted: floor (operators) | 8k and 2M | share of eligible stake | v3, uniform (operators) | ratio |
|---|---|---|---|---|---|
| **P2, honest unserved seats file (the design point)** | **17.29M (133)** | 17.16M (132) | 0.697 | 2.60M (20) | **6.65×** |
| P2, the first panel's honest attester files only when online (30% offline, model) | 13.39M (103) | 13.26M (102) | 0.641 | 2.08M (16) | 6.4× |
| P2 with a free redraw (no filing on a failed first panel) (C3) | 8.32M (64) | 8.19M (63) | 0.526 | 1.30M (10) | 6.4× |
| P3, the V1 door with no filing at all (V3S-05) | 6.63M (51) | 6.63M (51) | 0.469 | 1.04M (8) | 6.4× |
| P5, all five seats (V1 undetectable even with filing) | 50.83M (391) | 50.31M / 50.44M | 0.871 | 7.28M (56) | 7.0× |

The 8k and 2M thresholds sit one operator lower because their `P*` is 0.497, not 0.500.

**The worst state SW-10 admits (post-edits; `v31_review_numbers.py`).** The thresholds above assume every genesis seat
is eligible. When the attacker may choose which genesis seats are saturated (and so ineligible) and keeps all of its own
operators idle, the smallest winning stake under the 875‰ floor is:

| model | floor class (the saturated state) | 8k and 2M |
|---|---|---|
| **P2, honest unserved seats file** | **12.74M (98; 6 of 8 eligible)** | 12.61M |
| P2, files only when online (30% offline, model) | 9.88M (6 of 8) | 9.75M |
| P2 with a free redraw | **6.63M (51; 6 of 8 eligible, beside an executor bond posted at the cap; IA-1b)**; before the executor term 7.15M (55; 7 of 8) | not re-run with the executor term (7.02M before it) |
| P3, the V1 door with no filing | 5.72M (7 of 8) | 5.72M |
| P5, all five seats | 25.61M (4 of 8) | 25.35M |

**The executor term (IA-1b; SW-10).** In total attacker capital no row of this table moves: `X` relaxes the floor exactly
as the same capital posted as an idle Sybil would. Counted in Sybil stake, a **floor-bound** entry falls by at most `X`
≤ 1,000,000 MSK. The code re-ran two rows (the doc of `palw_panel_stake_executor_bonds_judging_v1`). P2 with filing
stays **12.74M**, because k = 6 is bound by the race, not the floor. The free redraw at 6 of 8 eligible was floor-bound
at 58 operators / 7.54M (the race alone needs 5.98M) and falls to **51 / 6.63M** beside an executor bond at the cap,
below the 7.15M this table stated. The 8k/2M column, the 30%-offline row and P3/P5 were **not re-run** (UNVERIFIED
whether any of them is floor-bound). **This 6.63M is a separate quantity** from P3's design-point threshold (6.63M, 51
operators, above) and from the weighted admission jury's even-odds stake (6.63M, SW-5), which it equals only by
coincidence. It is also unrelated to the held-attention bounds of §3.9.

**The saturation cliff, and what the floor does to it (SW-A1, SW-10; floor class, P2 with filing).** Idle Sybils carry
no locks and stay eligible; saturated honest seats drop out. Per number of eligible genesis seats (operators at 130k in
brackets):

| genesis seats eligible | no floor (the v3.1 draft) | no floor, free redraw | under the 875‰ floor: Sybil weight the floor needs / weight that wins → binding |
|---|---|---|---|
| 8 of 8 | 17.29M (133) | 8.32M (64) | 0 / 17.29M → **17.29M** (133) |
| 7 of 8 | 15.08M (116) | 7.15M (55) | 0 / 15.08M → **15.08M** (116) |
| 6 of 8 | 12.74M (98) | 5.98M (46) | 7.54M / 12.74M → **12.74M** (98), the worst admitted state |
| 5 of 8 | 10.53M (81) | 4.81M (37) | 15.08M / 10.53M → **15.08M** (116) |
| 4 of 8 | 8.19M (63) | 3.77M (29) | 22.62M / 8.19M → **22.62M** (174) |
| 3 of 8 | 5.85M (45) | 2.47M (19) | 30.16M / 5.85M → **30.16M** (232) |
| 2 of 8 | 3.38M (26) | 0.39M (3) | 37.57M / 3.38M → **37.57M** (289) |
| 1 of 8 | 0.52M (4) | 0.52M (4) | 45.11M / 0.52M → **45.11M** (347) |
| 0 of 8 | any 5 idle Sybils (0.65M) fill every panel; P2 = P5 = 1 | same | 52.65M / 0.65M → **52.65M** (405) |

**IA-1b:** beside an executor bond at the cap, every floor-bound entry of the last column falls by at most `X` ≤
1,000,000 MSK. The code re-ran 5 of 8: 116 / 15.08M becomes 108 / 14.04M. The 4…0 of 8 rows were not re-run, but a
fall of at most 1,000,000 MSK keeps each of them above 12.74M, so the worst admitted state for P2 with filing is
unchanged.

Without the floor the threshold falls to 0.52M at 1 of 8 and to any five Sybils at 0 of 8. With it, a state with 5 or
fewer genesis seats eligible binds only if the attacker itself supplies the missing eligible weight, which costs more
than winning did; at 6 of 8 the floor needs less than winning does, so 12.74M is the worst. An honest-only population
with two saturated seats (750‰) halts (SW-10) instead of binding.

**Sensitivity of the design point (floor, P2; free-redraw figure in brackets):**

| honest base | threshold |
|---|---|
| the eight genesis seats (launch) | 17.29M (8.32M) |
| 6 of the 8 genesis seats eligible (two saturated or offline; the worst state SW-10 admits for P2) | 12.74M (free redraw: 7.54M, where the floor binds; 5.98M without it; **6.63M** beside an executor bond at the cap, IA-1b) |
| 16 seats of 939k (the operator bonds eight more) | 35.49M (17.16M) |
| + 100M bonded as **one** operator, capped at 1,000,000 MSK (SW-2) | 19.76M (9.49M) |
| + 100M as one operator, uncapped (the v3.1 draft) | 73.19M (16.90M) |
| + 100M as 100 operators at the cap | 258.70M (126.23M) |
| + 100M as 769 operators of 130k | 259.61M (126.88M) |
| the same 7.51M as **one** honest operator | 0.52M (0.52M) |
| the rejected free-stake weight, honest seats at the 1 claim/DAA coverage load (67% free) | 11.70M (5.59M) |
| the rejected free-stake weight, every honest seat at its 500‰ ceiling | 8.84M (4.29M) |
| **the retry path SW-8 closes** (the draft's drop-grinding upper bound: the attacker carries the bind, retires any of its own seated Sybils after seeing the anchor, and the panel is re-derived on a later state) | about 8.97M (uniform: about 1.43M, 11 operators; Monte Carlo). **Closed on t12:** a panel binds only in its anchor block, on one state |

**v3's uniform draw, kept for comparison.** P_k for a uniform draw of 5 from `N = 8 + m` operators; the same script
reproduces every row (`v3_numbers_r2.py`, the verifiers' `ev_v1door.py`):

| m (stake) | P2 | P3 | P5 | floor EV, P2 / P2 with a free redraw / P3 (no filing) | 8k, same | 2M, same |
|---|---|---|---|---|---|---|
| 4 (520k) | 0.0909 | 0.1515 | 0 | −2,619 / −2,090 / −2,231 | −3,019 / −2,404 / −2,568 | −51,440 / −40,982 / −43,771 |
| 9 (1.17M) | 0.2647 | 0.5633 | 0.0204 | −1,506 / −260 / +406 | −1,726 / −278 / +496 | −29,447 / −4,817 / +8,344 |
| 10 (1.30M) | 0.2941 | 0.6176 | 0.0294 | −1,318 / **+11** / +753 | −1,507 / **+38** / +900 | −25,725 / **+547** / +15,215 |
| 16 (2.08M) | 0.4348 | 0.8142 | 0.1028 | −418 / +1,156 / +2,012 | −460 / +1,368 / +2,363 | −7,925 / +23,172 / +40,091 |
| 20 (2.60M) | 0.5026 | 0.8769 | 0.1578 | **+17** / +1,617 / +2,413 | **+44** / +1,904 / +2,829 | **+662** / +32,297 / +48,025 |
| 27 (3.51M) | 0.5899 | 0.9335 | 0.2487 | +576 / +2,124 / +2,775 | +694 / +2,493 / +3,250 | +11,706 / +42,319 / +55,179 |

**Corrections to the v3 draft** (kept). The draft said the coverage lie turns EV-positive "at about 27 uniform Sybils
(3.51M MSK)". It sampled only m ∈ {3, 4, 9, 27}; the first positive m for a single draw is 20. It also ignored the free
redraw (C3) and priced the V1 door with P2 (V3S-05).

**The residual, named (§4.2 #13, §9.3 Q10).** Under the stake-weighted draw the undetected coverage lie needs 17.29M MSK
of Sybil stake: 2.3 times the genesis seats' 7.51M, and 69.7% of eligible panel stake, against 2.60M (20 operators)
under v3's uniform draw, and 12.74M in the worst saturation state the floor admits. It is now a residual in capital,
not in identities. **It depends on the honest weight vector, not its sum (SW-A5):** the same 7.51M as one honest
operator gives only 0.52M; 16 genesis-size seats give 35.49M; 100M added as one operator gives 19.76M under the cap
(73.19M uncapped), and as 100 operators at the cap 258.70M. It falls when honest stake is ineligible (the cliff table
above, bounded by SW-10). Single community allocations larger than the threshold exist (the two 100M rows of
`a27f8f44`), so the honest weight vector is the operator's lever (§9.3 Q10), and O-2 reports the design-point
threshold of the live eligible weight vector. A coverage basis of 3 stays deferred (§9.2). T06 runs the re-roll strategy with and
without filing, and the V1-withholding strategy, on the real stake draw.

**q\* (post-licence detection break-even, m = 3).** `q*post = G / (E_v + k·lock_k + A_P + k·A_S)`; "sc" means the
offender captures its own reward; `q*pre` is the same before Final (v3 script). The locks are live at a post-Final
catch under DA because they follow the row (L-3):

| class | door (k) | seats | q*post | q*post (sc) | q*pre |
|---|---|---|---|---|---|
| floor | V1 (3) | 130k or 939k | 0.0900 | 0.0990 | 0.0869 |
| floor | coverage (2) | 130k or 939k | 0.1232 | 0.1351 | 0.1176 |
| 8k | V1 (3) | 130k or 939k | 0.0912 | 0.1003 | 0.0864 |
| 8k | coverage (2) | 130k or 939k | 0.1255 | 0.1375 | 0.1166 |
| 2M | V1 (3) | 130k / 939k | 0.3726 / 0.0985 | 0.3975 / 0.1083 | 0.2177 / 0.0829 |
| 2M | coverage (2) | 130k / 939k | 0.4602 / 0.1398 | 0.4864 / 0.1530 | 0.2450 / 0.1104 |

S2 has no row: it is never the basis for Final (Q-5). The shard rows of the draft are not applicable on t12
(`palw_shard_licensing` dormant, Q-3). At m = 1 the review's figures were 0.196 / 0.323 (floor V1 / S2).

**q_min, the post-licence detection that sampling alone delivers** (the review's S3 arithmetic, a one-site lie, 8
sites per honest sampler): floor 0.552 with 4 honest samplers and 0.331 with 2; 8k ≤ 0.0039; 2M ≤ 1.5 × 10⁻⁵. It is
given per class, not per door: S3 sampling does not depend on the licensing door, only on the honest samplers. On the
floor `q_min > q*`. On 8k and 2M it is not. The invariant does **not** rest on `q_min`: it rests on the success
probability above staying below `P*`, and on DA plus replay by honest seats (§3.11), which O-3 measures.

### 4.4 Per-class t12 numbers (MSK; v3)

| | floor | 8k | 2M |
|---|---|---|---|
| G | 3,200.96 | 3,744.62 | 63,599.16 |
| pre-licence `w + E` | 3,200.95 | 3,695.17 | 62,943.79 |
| post-licence, every seat `Valid` and `basis_k ≥ 2` (else `w + E` to Final) | 0.11 | 494.32 | **62,943.79 (`w + E` to Final: C7, U1)**; v3 had 59,742.94 |
| duty / `lock_3` / `lock_2` (at `s = 0`; see below) | 256.07 / 106.74 / 160.11 | 459.12 / 306.08 / 459.12 | 12,588.76 / 22,252.74 / 33,379.11 |
| 1. failed attempt, attributed: recovered | 3,200.95 (+1,300 S2) | 3,695.17 (+1,300) | 62,943.79 (+1,300) |
| 2. P* at q_f = 1: RT#2 / `ExecutorRefuted` | 0.500 / 0.584 | 0.497 / 0.572 | 0.497 / 0.503 |
| 4. L→F definite: full-service / held | 320.32 / 3,521.17 | 1,412.55 / 4,613.40 | held only (U1): 129,702.01 |
| 5. post-Final extract / recover, locks + row | 0.12 / 3,521.06 | 543.77 / 4,119.08 | 60,398.31 / 69,959.07 |
| 5. the same, net of R (locks only, j = k) | 3,489.05 | 4,081.63 | 69,323.08 |
| 6. X2 break-even L→F catch | 0.0901 | 0.0890 | 0.2187 |
| q*post V1 / coverage (130k seats) | 0.090 / 0.123 | 0.091 / 0.126 | 0.373 / 0.460 |
| 7. withholding pinned ÷ forfeit (at `s = 0`; see below) | 0.40 | 0.62 | 1.00 |
| 10. halt: row covers `G_res`? | yes | yes | **no** |
| 13. undetectable lie EV-positive from (Sybil stake, stake-weighted draw; v3 uniform in brackets; after "/", the worst saturation state SW-10 admits) | 17.29M (2.60M) / 12.74M; 8.32M with a free redraw (1.30M) / 6.63M (7.15M before IA-1b's executor term) | 17.16M (2.60M) / 12.61M; 8.19M (1.30M) / 7.02M (not re-run with IA-1b) | same as 8k |
| **invariant satisfied (given i–viii, below branch 13's threshold)** | **yes** | **yes, except #18** (the `AttnFused` gap, a launch gate) | **NO** (vi, iii, 10, R-6 j < k; and #18) |

**The lock, duty and row-7 figures are at `s = 0` (IA-3).** The armed lock prices the buyback bound at its cap, so it
is about `s_cap / k′` higher (≈ 53 MSK on 8k's `lock_3`, `rcore_whole_gain_and_buyback`). Where `lock_2` binds the
duty, as on 8k, the duty and row 7 move with it. These figures are not recomputed here. Under the armed price the
floor and 8k locks still sit within the duty, so a licence there needs no top-up (`8e1ce34a`, L-4).

"Yes" for floor and 8k holds **below branch 13's threshold** and given precondition (i)'s filing: with honest unserved
seats filing, the undetectable lie needs the two assigned attesters of the lied segment and, under the stake-weighted
draw, is EV-positive from 17.29M MSK of Sybil stake (69.7% of eligible panel stake beside the eight genesis seats), or
12.74M in the worst saturation state SW-10 admits; it is not satisfied above that stake without a larger honest base
(§9.3 Q10) or a coverage basis of 3 (§9.2). On 8k it also waits on the `AttnFused` design (post-edit 8, §4.2 #18),
which gates launch.

---

## 5. Comparison table (floor, 13,000 MSK)

| 案 | 1 claim の reserve | 正常系の reserve 占有時間 | 13,000 MSK での同時 claim 数 | 月間 claim 数（120 s / 200 s） | 1,000 本時の未カバー最大損失 | 報酬の待ち時間 | 再び開く攻撃 | 判定 |
|---|---|---|---|---|---|---|---|---|
| 現状（option A） | 3,200.95 を Final まで | 142〜147 DAA | 2 | 294 / 176（t11 の Withheld 率では 218 / 131） | 上限 2 本なので成立しない | F+1 で mint、F+601 から使用可 | 台帳が 150%。lock 容量 0.41〜0.68 本/DAA | 不変量は OK、資本効率が律速 |
| W（運用者の初案） | 6.5〜13 | 同上 | 500〜1,000 | lane の上限まで | 約 3.19M | 現状と同じ | P0-10（1 日 4,680）、閾値 6.1%、増幅 約 250 倍 | 不合格 |
| V（panel 案） | 約 0.01 | 約 0 | 無制限 | lane の上限まで | 3.2M。発覚しても損 0 | F+3,000 | P0-10 が無料、増幅 10⁵ 倍超 | 不合格 |
| R-core+ v1 | licence まで 3,200.95、以降 0.11 | 21〜26 | 未 licence 2 + licence 済み 約 900 | 1,662 / 997 | 0（期限内の有罪が前提） | 4.4〜5.2 日（120 s） | X1〜X14 | 要修正（監査） |
| R-core+ v2 | v1 + Withheld・redraw・S2・shard は Final まで | 解放 21〜26 / 保持 142〜147 | 未 licence 2 + 解放分 | p=1 で 1,662 / 997、p=0.24 で 366 / 220 | 0 と主張 | 名目 7.3 日で mint | F1〜F4（偽 root が無料、gate が block-lane を拒否、DA が 1 index、S3 が quorum に入る） | 要修正（第 2 監査） |
| **R-core+ v3.1（採用）** | **licence まで 3,200.95。全 seat が Valid、k ≥ 2 の licence 後は 0.11（min(L+60, 受領期限) までの補完でも解放）。それ以外（Sampled を含む）と 2M（C7、U1）は Final まで** | **解放 21〜26 / 保持 142〜147（DA 中は停止分だけ延長）** | **未 licence 2 + 解放分** | **t11 の Withheld 率で 256 / 154、早期 redraw 込みで 360 / 216（model）** | **0**（F1（F1-M・F1c を含む）〜F4 と stake 加重の抽選が GREEN、期限内の提出が前提）。X10 までは失敗試行を RT#2 で没収 | **名目 7.3 日で mint、8.7 日で使用可（200 s）**。trickle 22.6 日、2M 21.6〜35.5 日 | 担当 2 者の共謀による検出不能な lie は、stake 加重の抽選で Sybil stake 17.29M MSK（130k の operator 133 個、抽選に出られる stake の 69.7%）から EV 正（875‰ の下限が許す最悪の状態で 12.74M。v3 の一様な抽選では 20 個・2.60M）。1 枚目の panel の誠実な seat が提出しなければ 8.32M。2M は不変量外。2 枚の panel がともに沈む silent griefing は受容。held class の `AttnFused` の嘘は未解決（起動 gate） | **採用**（2026-09-24 決定 1〜7、v3.1 決定 1〜4、post-edit の決定）。§8.3 の起動 gate が条件 |
| O8 / O9（D を下げる） | 800 / 400 | 21〜26 | 8 / 16 | 約 6,600 / 13,300 | 1,000 × (G − D) | 同上 | 閾値 32.7% / 25.7% | 対象外。P0-10 が閉じるまで不可 |

---
