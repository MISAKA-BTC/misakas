> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 3965–4253 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §9 operator questions, the traceability appendix, the v3 and v3.1 review dispositions, v3.1 notes.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 9. Operator questions: decided, deferred, and new

### 9.1 Decided (2026-09-24)

Q1–Q8 are closed. Q4 is decided as the stake-weighted draw; the others take the default v3 shipped. v3's questions and
reasoning are in `0152-v3-snapshot.md` §9.

| # | question | decision | where it lands |
|---|---|---|---|
| Q1 | The RT#2 cap (D1) | **No cap** (`c_RT2 = ∞`): S0′ forfeits the whole commitment (3,200.95 MSK on the floor), only when a claim's **second** panel fails | SR-5, §3.6 S0′ |
| Q2 | C7 for classes registered later | **The rule:** a class whose verification window is ≥ `PALW_RCORE_C7_WINDOW_SPANS_V1` = 1,000 spans joins C7 at registration; no later class is listed by name. C7 = that rule **united with** `palw_rcore_conservative_classes` (`[2M]`) once R-core+ lands, and post-edit 5 keys the panel-room hold on it as well (T-2(b)) | SR-5, T-2(b), §6 |
| Q3 | `Sampled` pay | **Paid as a `Valid` is** (credited for seat pay); it never serves, never counts and is never liable | Q-1, SR-10 |
| Q4 | The undetectable-lie residual | **The panel draw is stake-weighted before launch** (§3.14), reversing ADR-0124 D5 / ADR-0130 on t12. The residual becomes 17.29M MSK of Sybil stake on the floor (133 × 130k operators, 69.7% of eligible panel stake; 17.16M on 8k and 2M), and 12.74M in the worst saturation state SW-10's floor admits, against v3's 2.60M; the coverage basis of 3 stays deferred (§9.2) | §3.14, §4.3 |
| Q5 | Execution-quantum maturity | **Unchanged:** round rights mature at F + 1,200 on t12; the matured residual stays named | V-2b |
| Q6 | FP licences and the second clock | **They do not count** (nor do S2 licences); an FP-only stretch halts attempt rows; O-6 counts it | V-8 |
| Q7 | A `BondRegistered` per-block cap | **None.** #12's rent and per-block cap stay on `ClassRegistered` only. Under the stake draw a flood of small bonds buys seats only in proportion to stake | B-2 |
| Q8 | DA bytes and draw grinding | **Both accepted:** up to 11.25 MiB of answers per claim in the worst case (36 sessions × 4 units × 80 KiB, DA-8), paid by the answering side's fees; a miner carrying its own accusation can grind the drawn units at one block's PoW per try | DA-3, DA-8 |

**Also decided on 2026-09-24, after v3.1** (§1.5 "Decisions of 2026-09-24 taken after v3.1"; table "Integration
amendments"). These are:
* the held-attention decisions (8k A-held in the gate; partial-mask signers not bound at launch);
* 2M closed at launch and U-D1…U-D10, with P-1's pruning depth ≈ 74,920 DAA without waiting for M12 (§3.9);
* drills and measurements after launch (§8.4);
* S-4's court default: the forfeit plus S2's action, and no `CourtConviction` record (S-4 deviation 2; §3.6). IA-9's
  other S-4 rules (the kind-3 record's contents, the live lock, the 9-entry strike list) come from the implementation,
  the audit and v3.1's arithmetic, not from the operator.
None of them reopens Q1–Q8.

### 9.2 Deferred (schedule these)

S5(a)/(b), and the vesting burn and ejection on equivocation (after Q1 of runtime nondeterminism); the **mainnet** Eq
value (D6); ADR-0153 and a measured 2M replay before `c_2M ≥ 2`; the P0-10 inference-bound ticket; a coverage basis of 3
(each partial seat replays two segments; single-panel threshold under the uniform model 32 operators / 4.16M MSK, v3
Q4(b)); K = 64 re-derivation; span rescaling for held-context classes; delivery-digest receipts (F8's alternative,
which would let a `Sampled`-style receipt serve); the F7 `reporter_reveal` design (the evidence slot stays empty until
its own fence); **a stake-weighted ADR-0147 admission jury** (SW-5, SW-A4: it would cut a registrant's 40 × 13k Sybil
capture from 0.9728 to 0.0034 per audit, but admit an honest class held by 20 × 130k ready operators with 0.1325 per
audit instead of 0.8769, and needs about 6.63M of ready non-genesis stake for even odds; it needs its own decision). The
stake-weighted per-operator draw has left this list (decided, Q4). **Added by the integration amendments (IA-1a):**
anchoring a panel on a committed seed after K heartbeats, so a claim can still bind through a heartbeat-only stretch.
It was considered and not taken at launch, and is to be weighed after launch against O-13's measurements. **Removed by the
post-edits:** "a panel fixed at the
anchor, which closes SW-8's drop-grinding surface" (SW-A2/R2: the panel is fixed at the anchor already, and
bind-only-in-the-anchor-block closes the retry path, SW-8).

### 9.3 New in v3.1

**Q10. The honest weight vector of the stake draw.** Under the stake-weighted draw the undetected-pair collusion
threshold depends on the eligible honest **weight vector**, not only its sum (SW-A5; the v3.1 draft said "proportional to
the eligible honest stake", which holds only for equal operators). At launch that is the eight genesis seats, 7,512,504
MSK in eight operators, so the attack turns EV-positive at 17.29M MSK of Sybil stake (8.32M with a free redraw; 12.74M
in the worst saturation state SW-10 admits). The same 7.51M as one operator would give only 0.52M. Single holders in the
`a27f8f44` community table already exceed the threshold (the two 100M rows), and the operator's main wallet holds most of
the 10B cap. Bonding more honest seats raises it: 16 genesis-size seats give 35.49M (17.16M); 100M bonded as one
operator gives 19.76M under the cap (73.19M uncapped), and as 100 operators at the cap 258.70M. Registering operator
bonds after launch needs no genesis change; they count for claims anchored after their registration (ADR-0147's cut).
**Recommendation:** the operator registers additional seats from the main wallet before t12 carries value (O-5), **as
several operators at or below the 1,000,000 MSK cap** (SW-2: above it an operator gains nothing by staying whole), sized
so that the eligible honest weight vector stays well above any single community holding that might bond, and O-2
reports the design-point threshold of the live eligible weight vector weekly. This is an operational choice, not a
consensus change; the ADR ships without it.

**Q11. How a producer below the U2 floor tops up (open; post-edit 12, B-4).** U2 says an honest 13,000 MSK producer
that takes one S0′ "tops up before producing again", and S-SPEC §10's test says "a top-up restores production", but
neither defines the top-up, and the code has no way to raise a bond's posted collateral: the registry is append-only,
`BondRegistered` refuses a bond key or operator identity that has ever held a bond (`DuplicateBond`,
`DuplicateBondKey`, `DuplicateOperator`, STATE:17236/17261/17278 at `f1dfb33b`), and the only non-test write to
`collateral` is the slash debit (STATE:13620). Two options:
* **(a) Re-registration (what the code allows today).** The operator registers a new bond under a new bond key and a
  new operator identity with ≥ 13,000 MSK; the old bond retires and exits by B-3's bound. No consensus change, no new
  fence. Cost: the operator's identity changes, and its old bond's collateral is locked until the exit bound.
* **(b) An in-place top-up object.** A new consensus object that adds collateral to an existing bond. It needs its own
  fence, a new object tag, a ledger rule (when the deposit counts as posted collateral, and how B-3's exit bound
  covers it) and a draw rule: SW-2's weight and SW-8's "never rises" are restated, while the one-state, anchor-block
  rule keeps a deposit from moving any panel anchored before it.

**Until the operator chooses, this ADR takes (a):** B-4, T17 and D-10 describe and test re-registration, and P6's
"top up" log line means it. Choosing (b) adds an object to S (or later) and a test to T17.

**Q12. Does the short drill stay a pre-launch gate? (ANSWERED 2026-09-24: (b), the operator — it moves after launch.)**
§8.3 item 3 gated the
regenesis on a short fleet drill, D-1…D-10. On 2026-09-24 the operator moved drills and measurements after launch, with
implementation first, and named the SEAT-0 drill, M5, M5b, M9, M12 and the 8k real-weight timing drill (§8.4). The short
drill was not named. Two readings:
* **(a) It stays a gate.** It drills the shipping binary before the regenesis, as item 3 says. Several steps need parts
  that are not on the integration line yet: D-4 and D-5 need P2-7 and P2-8, D-4 the drill hook `PalwDrillFaultV1`
  (M2 Phase 3), and D-8 needs SR-9 (object 56, S-5).
* **(b) It moves after launch.** It runs on public t12 with §8.4's program, like the drills the operator named.

**Answered: (b).** The operator folded the short drill into "drills and verification after launch" (2026-09-24);
§8.3 item 3 records it and §8.4 lists it.

---

## Appendix — traceability

| source | item | where |
|---|---|---|
| review v1 R1–R9 | as v1 | V-1/4, V-5, V-2, V-6, SR-4, L-1, A-1, V-7, B-2 |
| audit req. (1)–(7) | P0-10; door release; escape; per-door k; 2M; buyback; withholding | §3.9; SR-1; V-8; L-1/Q-3; T-2(b), T-4; V-2; §3.8 |
| X1–X30 (v1 review) | as v2 | v2 changes (history) |
| **F1** | job identity | J-1, J-2, J-4, J-5, J-6; T18, T18b–T18p, T-THREAD, T62 |
| **F2** | the gate | J-3, J-7; T46a–T46n (SPEC §3.6) |
| **F3** | the DA court | DA-1…DA-9; T64–T69 |
| **F4** | quorum counting | Q-1…Q-7, L-1; T70–T74 |
| **F5** | per strategy; filers | §3.9, §4.2, §7.3; O-3 |
| **F6** | q | §4.3; T06 |
| **F7** | front-running | R-3; T39 |
| **F8** | Incapable | SR-1 cond. 2; T27, T68 |
| **F9** | refuted cost | DA-6; T69 |
| **F10** | collected base | R-2; T39 |
| **F11** | p set by the first carrier | SR-1b; T74 |
| **F12** | capacity with f | T-2(d), SR-9; T08, T57 |
| **F13** | Eq ejection | B-4, §3.6 Eq; T76 |
| **F14** | duty ≤ 1 | L-4; T77 |
| **F15** | 2M X9 residual | L-4b, §4.2 #11; T78 |
| **F16** | j = k | R-6; T39 |
| **F17** | reward timing | R-4; T07, T75 |
| **F18** | trickle; FP licences | V-8; T37, O-6 |
| **F19** | B-3 gate | B-3; T23, T42 |
| **F20** | isolation wording | §6; T24, T80 |
| **F21** | zero root | V-5; T26 |
| **D1–D7** | operator decisions of 2026-09-24 | §1.5; see "v3 changes" |
| Phase 2 plan F1–F16, §5.3 | A-KEY; V-3 identity; round lane; V-7 backlog; drill salt | V-7; V-3; V-2b; V-7; §8.2 |
| `e93be0f2`, `f8c91f19` | the rate room; the hold, re-keyed on C7 (post-edit 5) | T-2(a)–(c) |
| v3 verifiers C1–C13 | decisions lens | "v3 changes", second table; disposition below |
| v3 verifiers V3S-01…13 | security lens | same |
| v3 verifiers IMPL-1…18 | implementability lens | same |
| audit spec `f2f1_spec.md` §3, §4 (v3.1) | F2/F1 names, types, discriminants, v22 fields, tests | J-1…J-7; §6 v22 rows 1, 7, 19, 20, 27, 28; T46a–n, T18b–p, T-THREAD |
| audit addendum `f1c_f1m_spec.md` §4-bis (post-edits) | `CoreV1`, J6/J7, 13 `PromptNotAnchored`, the finding's `forfeit`, admission, court door, SEAT-0/SEAT-R | J-3, J-5, J-8, Q-6, Q-7; §6 row 20; T18q–T18y, T18p-M |
| audit S spec `s_spec.md` (post-edit 9) and its user decisions U1–U3 | the v22 layout for S, mirrors, landing order; U1 escrow, U2 floor gate, U3 FP tier | §6; SR-1 cond. 4, B-4, §3.6 S3-FP; T17, T22 |
| `v3.1-postedits.md` items 1–13 | see "v3.1 post-edits" | PE-1…PE-13 |
| v3.1 review SW-A1…SW-A6, R1…R12 (body sync) | the stake draw | §3.14 SW-1…SW-10, §4.3, T85–T94, §9.2, §9.3 |
| v3.1 decisions 1–4 | F1-M, F1c in the gate; stake-weighted draw; Q1–Q8 | J-5, J-7, §8.3; §3.14, §4.3; §9.1 |
| v3.1 agreements with the audit | vesting-row copies; `ProducerWithholding` scope; Phase 2 filer | V-1 (N8); §3.6 X7, DA-7 (N9); §7.3 P2-8c (N10) |
| the integration line `rcore/int-2` at `8e1ce34a` (the S-4 merge `68f0d672` of `c3fe99cd`, plus the N1 test), 2026-09-24 | the stake draw integrated (`f422df49`), the vesting rows (`d3ece0d5`), S-4 (`c3fe99cd`, `479cdfa3`, merged at `68f0d672`), S-6/S-7 (`1f4b2b2a`), F4 part 1 (`36303abe`), P6 (`42ef9642`, `0613580f`), SEAT-R and SEAT-S (`b589622f5`, `1e20edd50`), SEAT-S2's real-replay test (`449fd892`), derive under `CoreV1` (`a4682a8d`) | "Integration amendments" IA-1…IA-15 and the sections each names |
| operator decisions of 2026-09-24 after v3.1 | held attention (4-ter); 2M closed and U-D1…U-D10 (4-quater); P-1 without M12; drills after launch; S-4's court default (S-4 deviation 2) | §1.5, §3.6, §3.9, §8.3, §8.4, §9.1 |

---

## v3 review disposition

Every finding of the three verifiers was applied except where a line below says otherwise. "Partial" means the
problem is closed but not by the finding's first required change; the reason is given.

**Rejected or partially adopted**

* **C3 (partial).** The recomputation is adopted in full (§4.3; the single-draw threshold is also corrected from "about
  27" to 20). The finding's first remedy, "require `basis_k ≥ 3` for Final on a redrawn claim", is **rejected**: it moves
  the free-redraw threshold only from 10 to 11 operators (the second panel's attacker then needs the two assigned
  attesters plus one more signer, P = 0.265 against P2 = 0.294 at m = 10), it cannot be met on 2M where a full replay
  does not fit, so every redrawn 2M claim would forfeit, and it does not touch the V1-withholding route. The second
  remedy, "charge part of E when the first panel fails with the material unserved", is **rejected** because "unserved"
  is not an objective chain fact: the only signal is an `Unavailable` receipt, which the attacker's own Sybils can sign
  for free on an honest claim. Instead the redraw is made non-free by node policy that the chain already rewards:
  an unserved seat files before its receipt window closes (P2-6), its session pauses the claim (DA-5), and the
  producer must answer (and be located by J-6) or default (S1) before any redraw. The ≈ 10-operator figure is kept as
  the no-filing case and put to the operator with the rest of the residual (§9 Q4).
* **C4 (partial).** Stage pricing is adopted. `S_stage` is defined as the stage's **reward base** (the producer's
  nominal debit), not "producer tier plus S4 on the live-locked signers", because V3S-12 removes signers' silence
  charges from the DA reward. Pricing the refuted cost on a base that no longer pays a reward would make the refuted
  cost about ten times the reward at the Licensed and FinalRow stages (floor ≈ 3,233 against ≈ 325) and deter honest
  post-licence accusers, contrary to D5. With this definition the refuted cost equals r times the reward base at every
  stage, which is D5's "≥ r·S, capped".
* **C7 (partial).** The mask rule is adopted (S4 only on signers covering an unanswered unit). The alternative "draw
  units only within segments that a live-locked honest signer can answer" is not adopted: which signers are honest is
  not a chain fact.
* **C8 (partial).** Adopted as "accusable while the claim record exists, retirement deferred while a session is open,
  bounded by trace retention" with the residual named (§4.2 #15). Extending accusability to the vesting row after
  retirement is not adopted: it would need the panel, the retention and the seat list after retirement, which the
  liability record and the row do not carry, for a gap that execution-proving convictions already cover (T28).
* **C10(b) (partial).** Row F6 is reworded instead of publishing q_min per door: the S3 arithmetic depends on the honest
  samplers, not on the licensing door, so a per-door q_min would repeat one number.
* **V3S-02 (option b).** The re-key at opening is adopted; option (a), "V-7 skips a row whose only failing condition is
  V-4(c)", is not: stop-never-skip (X30) is what makes the queue lemma and the filter-vs-fold agreement (T29) hold, and
  the re-key keeps the scan monotone without an exception.
* **V3S-03 (partial).** DA and court keys take no commitments, and R is removed from precondition (i). "Key the
  execution-proving reward per claim and accept a reveal only with a valid contradiction" is not adopted: only the
  first contradiction for a claim is ever verified on chain (the rest are refused `ClaimAlreadyConvicted`), so a reveal
  would have to re-run a verifier for a contradiction the chain otherwise never checks, and the offender could still
  pre-commit to its own. The ADR states instead that R pays nothing against a rational offender. "Fund honest filers
  another way" is DA-6's refund (V3S-06).
* **V3S-04 (partial).** Locks follow the row, and P2-6 accuses at the licence. "Refund the refuted exposure if the
  claim is later convicted" is adopted generally through V3S-06, not only for P2-6's early sessions.
* **V3S-05 (option b).** The V1 door is priced with P3 (no filing) and P5 (with filing), and V1's safety above 8
  operators is stated to rest on the unserved seats' DA filing and J-6's replay, not on S2/S3. Option (a), "the V1 set
  must include the assigned full seat's Valid", is **rejected**: at m = 9 it lowers the no-filing success only from
  0.563 to about 0.379, still above P* with a redraw, and it removes V1 as the liveness fallback exactly when the full
  seat is offline (then coverage and S2 are unavailable too). Retiring V1 on genesis classes is rejected under D2
  ("keep the doors enabled").
* **V3S-06 (partial).** The refund is triggered by **any** conviction of the claim before its record retires, not only
  by one "using a unit its session forced into the open": which unit enabled a conviction is not an objective chain
  fact. The cap is adopted through the new DA-6 formula.
* **V3S-08 (partial).** Adopted as "only seat sessions pause a pre-Final claim". The alternative, charging a refuted
  non-seat session at the claim's S0′ rate per paused DAA, is not needed once non-seat sessions pause nothing.
* **V3S-12 (partial).** Adopted as "a DA default's reward base is the producer's debit"; "pay no reward on a default
  where a signer is charged only for silence" is subsumed, since signers' debits never enter the base.
* **IMPL-1 (partial).** The V3 supplementary door is adopted (SR-10). "Otherwise keep S2 dormant on t12" is rejected
  under D2 ("keep the doors enabled").
* **IMPL-13 (partial).** T60's home is named (`misaka-palw-base0/tests`, with the t12 harness moved into a feature
  module); the audit session may choose the `kaspa-consensus` crate instead and must name it in the M1 commit.

**Verifier questions answered in the ADR (not put to the operator)**

* `Sampled` and served: holds to Final (D3 decides it).
* The abandon hold: kept (D1 decides it: never a free timeout).
* S2 and the second clock: S2 never ticks it (D2 decides it: S2 is never the basis for economic weight).
* Stage pricing of the refuted cost: priced per stage on the reward base (D5).
* A first-panel `NotReplayBacked`: redraws (D1 keeps the forfeit for the second failed panel only).
* `f8c91f19`: included in the base; it already cites T-2(b).
* Commitments on DA keys: refused; R is not cited as the filing incentive (D5's commit–reveal stays for
  execution-proving convictions).
* Refund of refuted exposure: yes, on any conviction of the claim before retirement.
* The V3 supplementary door: added in S (A), recount in M4 (B); S2 stays enabled (D2).
* Registration exposure and exit: not in the gate.
* Accuser exposure: on the free half, outside the 500‰ ceiling, and it holds exit.
* The withdrawal delay in the fold: a `#[borsh(skip)]` mirror; "already released" dropped.

**Put to the operator (v3):** §9 Q4 (the undetectable-lie residual: accept at launch, coverage basis 3, or a
stake-weighted operator draw). §9 Q1–Q3 and Q5–Q9 stood from the draft with updated figures. **All were decided on
2026-09-24; see §9.1.**

---

## v3.1 review disposition

The review of the v3.1 draft (stake-security SW-A1…SW-A6, spec-impl R1…R12) is mapped row by row in the third table of
"v3.1 changes"; the post-edits carried it into the body (§3.14 and the sections that depend on it). No finding was
rejected. These were adopted in part, or closed other than by removing the hole, and the reason is the rule's:

* **SW-A1 (partial: a halt, not a closure).** SW-10's floor does not make saturation harmless; it turns the cliff into
  `InsufficientEligibleStake`, a halt without forfeit. The worst state it admits still lowers the design point from
  17.29M to 12.74M (§4.3). A floor high enough to admit no saturated state (1000‰) would halt an honest-only population
  with a single saturated seat; 875‰ is the highest floor that tolerates one.
* **SW-A2 / R2 (adopted, with the draft's premise corrected).** The panel was already fixed at the anchor block; what
  the rule closes is the retry path. So SW-2's first reason and the §9.2 item "a panel fixed at the anchor" are
  withdrawn rather than implemented, and the 8.97M figure is kept as the label of the path that is now closed.
* **SW-A4 (adopted: the jury is not weighted).** Weighting it was outside Q4; the jury stays ADR-0147's, the trade-off
  and the kept residual are stated (SW-5), and the weighted jury is deferred (§9.2).
* **SW-A5 (partial: restated and bounded, not closed).** The residual is restated on the honest weight vector, and the
  cap removes any gain from staying whole above 1,000,000 MSK, but the threshold still depends on how the honest stake
  is split; that is the operator's lever (§9.3 Q10), reported by O-2.
* **SW-A6 (partial: bounded by the cap).** A class held only by small operators keeps the lever (40 → 6); it costs
  throughput, never safety, and is named (§4.2 #17).
* **R5 (adopted).** The anchors are given at `f1dfb33b`, where the P7 type landed. The `stake` field is B's; S's own
  change to the draw is the eligibility filter in `PalwPanelValidLockV1` (S-SPEC §3.3, P7), landed before M4.

## v3.1 notes: where this revision departs from its inputs, and why

* **The draw's weight is posted collateral, not free stake (SW-2).** The task that asked for this section suggested the
  operator's free slashable stake (`collateral − palw_bond_committed_v1`). It is rejected because idle Sybils would
  outweigh working honest seats by up to 2× (threshold 17.29M → 11.70M at the 1 claim/DAA coverage load, 8.84M at the
  ceiling), and because it buys nothing: the action tier is collected from the uncommitted half, which the ceiling keeps
  at 50% or more. The one ledger still gates eligibility. (The draft's further reason, that free stake "moves after the
  seed", is withdrawn by the review, SW-A2: the panel is derived and bound in its anchor block on one state.) **Post-edit
  6** asked again for the summed free stake per operator as the weight, in a `BTreeMap` policy type; it is superseded
  by this rule, the review's SW-A3/SW-A5 and the committed P7 type, and only its "read at the anchor" part is kept
  (SW-8).
* **The post-edits that depart from an input** (each recorded where it lands):
  * **SR-10's owner.** Post-edit 13 and the handoff put the V3 supplementary door in M4 (B); S-SPEC §3.5 and its commit
    S-5 still plan it in S. This revision follows post-edit 13; S keeps SR-1b in the V2 door and the `redraw_claim`
    helper, which Q-5 also calls.
  * **U3's rate.** Post-edit 12 and S-SPEC §10 fix the FP producer tier's cap (`m × G_fp`) but not its rate (S2's 10%
    or S3/S4's 25% of `C`); §3.6 records the cap and leaves the rate to S-4. **Resolved by S-4 (IA-9):** S3's rate,
    `min(25%·C₀, 3·G_fp)` (`palw_rcore_s3s4_action_v1`).
  * **A-1's text.** S keeps duties inside `reserved_exposure` (S-SPEC P2); the value A-1 defines is unchanged, the index
    it named is dropped (§6 row 29).
  * **X10's fence field.** S-SPEC declares no `palw_rcore_attributed_charging` at the regenesis; §6 and T02b follow it,
    and the field lands with M6.
  * **U2's top-up.** Post-edit 12 and S-SPEC §10 say "a top-up restores production", but no input defines a top-up and
    the code at `f1dfb33b` cannot raise a bond's collateral (append-only registry, `DuplicateBond` /
    `DuplicateBondKey` / `DuplicateOperator`, the slash debit the only write). This revision reads "top up" as
    re-registration under a new key and operator identity and records the in-place alternative as §9.3 Q11.
* **The `claim_rcore` side record is dropped (§6).** Its only reason was to leave `PalwClaimStateV2` literals untouched,
  and F1 already appends `job_identity` there. One record, one delta, one resolution order.
* **R-3's commitment now binds the evidence digest (N12).** The audit's ledger keys (one per (seat, claim), one per
  claim) are public at bind; committing to the key alone would let a speculator win every reward.
* **DA defaults and court convictions forfeit by claim, root 0 (V-2b).** v3 recorded the root for both. The audit's rule
  (a borrowed root is someone else's) applies to them as much as to `IdentityMismatch` and `OutputMismatch`.
* **Open DA sessions do not block a conviction past `palw_rcore_plus` (N13).** The audit's `ClaimUnderSession` guarded the
  v1 court's `held_da_missing`; R-core's sessions change no phase and are closed and refunded by the conviction.
  Post-edit 2 goes further: the audit's own rule now refuses only on an open court, below `palw_rcore_plus` too (J-3).
* **F1-M and F1c are in the gate (operator decisions).** The audit's spec pinned both as residuals (SPEC §4.7, §5 items
  1–2) and recommended keeping model classes held with RT#2; the operator overrode that. The model-class function
  names are the audit addendum's (`f1c_f1m_spec.md` §4-bis; J-5 cites them, R4).
* **Test renumbering.** Phase 2's T46 is T58, so the audit's T46a–n keep their file name.
* **Re-anchor.** v3.1 read the draw code at `f8c91f19` and the genesis at `5bf72b46` (`a27f8f44`); the audit's line
  references are to `wt-trial` 4064364e and are its own.
