> **Archived verbatim from ADR-0152 v3.1** (branch `docs/adr-0152-v31-postedits` at `9ed1adced`,
> lines 3704–3964 of `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md`): §8 tests, the launch gate and the observation program.
> Not normative. The rules are in [spec/palw](../../../../spec/palw/00-index.md); the decision record is
> [ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md); the reading guide is [README.md](README.md).

## 8. Tests, the launch gate, and the observation program

### 8.1 Tests

"A" and "B" name the owner; the phase is in brackets. Every test has a fence-off twin where a dormant branch exists.

| # | test | owner |
|---|---|---|
| T01 | Staged lifecycle: a quorum or coverage licence with every seat `Valid` and `basis_k ≥ 2` releases `E`; Withheld/Incapable/`Sampled`/missing, redrawn and S2 hold; **a C7 (2M) claim holds to Final whatever its licence, and an 8k claim releases like the floor (U1, post-edit 12)**; Final releases `w`; revert and IBD roots match; reload shows no `CarriageInconsistent` | A [S] |
| T02 | Charging at launch: second `ReceiptTimeout` forfeits the commitment (S0′, no strike, no reward); `BindTimeout`, `NoCapablePanel` and the first `ReceiptTimeout` are 0 | A [S] |
| T02c | The free-prompt abandon hold on t12 (a T02 twin; C2): a free-prompt `BindTimeout` keeps its reservation for 600 DAA past `palw_rcore_plus` (released in the first block whose score exceeds `voided + 600`), is re-derived on load at the last point, and restarts mid-hold with equal roots; extends `dos_repro_2`'s fold test | A [S] |
| T02b | **Restated (post-edit 9, S-SPEC §1e):** at the regenesis no X10 fence exists, and RT#2 is S0′ on every class. The fence-crossing twin (with `palw_rcore_attributed_charging` at height h, RT#2 before h forfeits and after h is S0; C7 stays S0′ on both sides) is written with M6's field | A [S; the twin at M6] |
| T03 | Row creation and exact move; counters; the restated coinbase identity (buyback slice, `panel_reserve_sompi`, reporter and market mints excluded) | A/B [S/P2] |
| T04 | Burn: an execution-proving conviction deletes the row; ConflictingPermit one share; row and lock predicates agree at F+2,999 / F+3,000 | A [S] |
| T05 | No escape at the state and UTXO layers | A/B [S/P2] |
| T06 | EV grid over `(P_k, q, q_f)` at m = 3 on the real **stake-weighted** draw (§3.14; executor excluded, the genesis seats and 130k Sybil operators): the coverage lie, the V1-withholding strategy (P3, caught by filing), and the re-roll strategy with and without first-panel filing; reproduces §4.3's stake thresholds (17.29M / 13.39M / 8.32M / 6.63M / 50.83M on the floor) and, with genesis seats saturated under SW-10's floor, the worst admitted state (12.74M / 9.88M / 7.15M / 5.72M / 25.61M), or replaces them; **with the executor term (IA-1b) the free-redraw worst is 6.63M beside an executor bond at the cap**, which T06 reproduces or replaces; with `stake: None` it reproduces v3's uniform thresholds (20 / 16 / 10 / 8 / 56) | B [M4] (A reviews) |
| T07 | Reorg fuzz: per bond, Σ recoverable (net of R) ≥ Σ extractable; **a twin where two copies of one conviction carry different reporters** (F17) | A [M5] |
| T08 | Capacity as a function of p and f; fold = processor = node to the sompi | A [S] |
| T09 | Finding 12: `max(duty, lock)` per (seat, claim); `dos_l5_4b` passes | A [S] |
| T10 | **Retired as a pre-launch test.** Replaced by the short drill D-1…D-8 (§8.3) and the observation program O-1…O-12 (§8.4) | — |
| T11 | A row outlives claim retirement | A [S] |
| T12 | A same-block conviction and maturity: the burn comes first | A [S] |
| T13 | Buyback excluded and priced; FP rights in `G_res` | A [S] |
| T14 | Monotone staging; `escrow_released` never un-flips | A [S] |
| T15 | Lock prices per recounted k; the flat-/3 counterexample | A/B [S/M4] |
| T16 | V-7 budget in new keys with the market reserve; stop, never skip; a carried row is burnable; **a session held open on the head row for 1,200 DAA does not stop later latched rows** (V3S-02) | A [S/M3] |
| T17 | Floors: 129,999.99 MSK never drawn; 13,000 registers. **U2's producer floor gate (post-edit 12):** a 13,000 MSK producer after one S0′ is refused at attempt admission (ADM and `apply_attempt`) and as an FP executor; its operator's re-registration (a new bond key and a new operator identity with ≥ 13,000 MSK, B-4) passes the gate once folded, while the old bond stays refused (the code has no in-place top-up; §9.3 Q11 — if Q11 adds one, T17 gains "a top-up restores production"); an own attempt refused by the gate (`ProducerBelowFloor`) does not invalidate the block; `palw_bond_producer_floor_shortfall_v1` is `None` below the fence and `Some(floor)` for an unknown bond; kaspad's pre-check (P6) agrees with the fold | A [S] (P6: B) |
| T18 | P0-10 naive: without material, a DA default is S1; with no accusation, RT#2 forfeits at launch | A [M2] |
| T18b | SPEC §4.7 T18b, borrowed root: honest run R (claim C0) and C1 reusing R's roots are both admitted; a disclosure of R is refused by the identity rule and C1 voids `ProducerWithholding`; kind 4 `IdentityMismatch` on a third claim C2 gives `CourtFraud` and debits the executor; C0's phase, lock, schedule rows and state entries are byte-identical; kind 3 against a colluding `Valid` signer. Reverses probe `borrowed_roots_…` | A [M2] |
| T18c | SPEC §4.7 T18c: before licence, kind 4 convicts (i) arithmetic garbage leaves and (iii) a 4096-leaf `Shape`; (ii) a root with no preimage defaults (`ProducerWithholding`); **(iv) garbage logits on an honest step tree convict through `LogitsNotStepOutput` (F1c, in the gate; SPEC pinned it as a residual)**; a correct-job garbage commitment on the floor is convicted before licence along J-6's garbage path (S2). Reverses probe `a_garbage_flat_commitment_…` | A [M2] |
| T18d | SPEC T18d, output grind: `OutputMismatch` convicts; the honest `output_root` is refused | A [M2] |
| T18e | SPEC T18e, relabel: the producer runs `ctx(anchor_A)` with B's `job_id` and seed; J1 and J3 pass and the full-context check convicts **on the floor, and (F1-M, in the gate) on 8k and 2M**, where SPEC pinned a residual | A [M2] |
| T18f | SPEC T18f: a trace-root mismatch (J4) convicts | A [M2] |
| T18g | SPEC T18g: `job_identity == 0` never convicts | A [M2] |
| T18h | SPEC T18h: an FP pin mismatch convicts; `fp_pin_spellings_agree` | A [M2] |
| T18k | SPEC T18k: forfeiture by claim leaves C0's minted rows intact, and a later snapshot excludes the voided claim | A [M2] |
| T18m | **F1-M (new in v3.1):** `ForgedOutputTiled` convicts a forged token on a real 8k tiled/A16 decode and on a 2M fixture, by kind 3 (full-mask signers) and kind 4, before and after Final; an honest tiled decode never convicts | A [M2] |
| T18p | SPEC T18p: a partial kaspad seat refuses a borrowed-root claim. It no longer licenses `AnyValid` by itself: the addendum moves that to T18p-M plus SEAT-S1, SEAT-S3 and SEAT-S4 (Q-6, ADDENDUM §4-bis.7) | A [M2] |
| T18q–T18y | **The addendum's tests (post-edits 1, 2, 4; ADDENDUM "Tests"):** T18q 12 on the model fixtures (rows × argmax bends × lanes, dense and fold, ragged vocab 8,292); T18r 11 `NotSelected` and `OutOfVocab`; T18s 10 on model attempts **and an FP model claim** (the unified rendered rule), forfeiture by claim leaving a lender's rows intact; T18t J6, J7 and J5a; **T18u the heavy budget: a second `Whole` 13 in a block is dropped before compute, the block stands, the root is identical**; T18v the court door; **T18w admission: a non-formula canonical, a failing head predicate, Float32 and a Kimi-kernel class are refused, the genesis rows pass**; T18x V1 parity for tags 9–13; **T18y a claim under a DA session is voided by kind 4**, sweep, reservation and rows consistent, `reloads()` | A [M2] |
| T18p-M | **SEAT-R's seat duty (post-edit 3):** every drill fault (`PalwDrillFaultV1`) gives no `Valid` from any route; honest runs get `Valid` only by replay or the S1 resume; a replay mismatch is terminal; the material, interval and capture-sample arms never exit `Valid`; a pooled capture is used by S3 only after `verify_material(..) == Matches` (SEAT-S3); plus the source-structure test of the arms. `PalwDrillFaultV1` is A's (§7.1 "Seat fixes"). It gates SEAT-R and decides the `AnyValid` sites of 9 and 13 (Q-6) | B [M4] (A reviews) |
| T-THREAD | SPEC T-THREAD: a block mined through the pipeline on the t12 harness records `job_identity ==` the producer's anchor, for own work and for a merged blue from the blue's own header; with `f1_job_identity_survives_reorg_across_admission` and the v22 `reloads()` round trip | A [M2] |
| T20 | **C7 (post-edit 5):** C7 is exactly {2M} at t12 genesis, by the params list and by the ≥ 1,000-span rule alike (2M window 2,799; 8k window 3); the 2M class owes every claim until Final and is refused past `max_inflight_claims` (`c_2M = 1`); **the 8k class is not held**: its licence frees its replay and its static cap is not consulted past the grace; a later class with a window ≥ 1,000 spans joins C7 by the rule; a non-empty C7 list outside `PALW_T12_GENESIS_HELD_ROWS` is refused; RT#2 on 2M is S0′ before and after X10; amplification 1.00 | A [S] (the room half on the launch line: B, A reviews) |
| T21 | The rate room (`e93be0f2` + `f8c91f19`, the hold re-keyed on C7) under R-core+: the per-bond share; a class's replay freed at licence **for every class outside C7, 8k included** (`panel_room_short_class_is_released_at_licence`: after each 8k licence (owed, room) = (0, 5), and a 6th claim folds; the room re-review's 8k empty-panel room pinned at 12 ready seats: 8, against 5 while held); a 2M licence does not free its replay; a court re-charges it; a non-seat DA session neither pauses nor charges; utilization holds no class; the two HELD-2 cases; t11 parity | A [S/M3] (the C7 cases on the launch line: B, A reviews) |
| T22 | Every tier's amount and cap at m = 3; the strike epoch rule; the per-claim forfeit always applies; **U3 (post-edit 12): an FP claim convicted after Final debits the executor by the capped tier (`≤ 3 × G_fp`, G from the liability's `g_res_sompi + escrowed_reward`), the signers take S4, the root is forfeited as before, and a revert restores the state root**. **As integrated (IA-9; `rcore_s4_conviction_funnel`):** U3's tier is `min(25%·C₀, 3·G_fp)`; a court default is charged the forfeit plus S2's action and writes no kind-6 record; a kind-3 record's `amount` and `collected` include the producer's leg; a listed seat with no lock is convicted for 0 | A [S] |
| T23 | B-3: the gate is the non-registration part of `palw_bond_committed_v1 > 0`, `accuser_exposure > 0`, plus unmatured rows; a retire-while-bound seat is held (F19); **a registrant whose class lives exits by the bound** (IMPL-8); **the exit bound pinned as a function (post-edit 10(d)):** `since + palw_v2_bond_withdrawal_delay_at_v1` (7,500 + the DA lattice 5,400 = 12,900 on t12), 18,900 hard with the second clock stalled, `F + 9,000` for locks; the UTXO half (P2-1) | A/B [S/P2] |
| T24 | Fingerprints: t11/devnet/mainnet pinned (their ruleset ids and fence schedule ids do not move; the `#[borsh(skip)]` mirrors must not move them), **their V2 params ids re-pinned once for v22** (post-edit 10(b)), t12 moves; `fork_id_v1`; a negative case per prerequisite, naming `palw_economic_safety`, `palw_panel_exposure_floor` (F20) **and `palw_operator_id_unique`** (SW-A3; with a second bond under one operator id refused `DuplicateOperator`); negative cases for a mirror mismatch (including `withdrawal_delay_daa` without the DA lattice), a non-V5 context root under `palw_rcore_plus`, a non-empty C7 without `palw_rcore_plus`, and `palw_shard_licensing` armed with it; `PALW_RCORE_REPORTER_REWARD_BPS_V1` equals `DnsParams.slashing_reporter_reward_bps` on t12 | A [S] |
| T25 | Decision A separation | B [P2] |
| T26 | Past `palw_offence_attribution`, `CourtExecutorGuilty` and `ExecutorEquivocation` as kind-3 contradictions are refused `ContradictionNotAdmitted` (SPEC T46k); below it, an unrelated-job equivocation plus `CourtExecutorGuilty` is refused and a consumed offence with root 0 never binds (F21) | A [M1/S] |
| T27 | X2 release: 3 Valid + 2 missing, or + Unavailable, holds; **3 Valid + 2 Incapable holds** (F8; on a non-floor class, since the floor refuses `Incapable`, `palw_seat_may_plead_incapable_v2`); **3 Valid + 2 `Sampled` seats that received only their S3 sites hold** (C1); 5 `Valid` releases | A [S/M3] |
| T28 | A conviction at F+3,001…F+9,000 under a held second clock after retirement burns the row; `basis_k` read from the row | A [S] |
| T29 | Filter-vs-fold agreement with the queue at 1,016 and maturity at 3d | A/B [S/P2] |
| T30 | Market rows fill the queue: ≥ 1 row moves per block while mature; queue ≤ 1,032 | A [S] |
| T31 | v3.1: `ConflictingPermit` is refused by name past `palw_offence_attribution` and kind 1 is refused `SupersededOnThisNetwork` (SPEC T46h, T46k), so S4′ never fires on t12; below the fence v3's case (the claim stays Final, one share burns, `reloads()` round-trips) still holds | A [M1/S] |
| T32 | X7: producer silent on a licensed claim → S1 and signers S4; one signer answers by `MaterialDisclosedV2` → refuted, the accuser pays | A [M3] |
| T33 | Backed subset: 4 backed + 1 unbacked licenses | A/B [S/P2] |
| T34 | DA on every class; automatic DA (node half); `dos_repro_4` with automatic accusation | A/B [M3/P2] |
| T35 | `withholding_strikes` rooted, carried, reverted, pruned at 7,500; up to 9 live entries (IA-9) | A [S] |
| T36 | No escalation; no status change; no tombstone | A [S] |
| T37 | Licence halt by three genesis retirements, `NoCapablePanel`, a heartbeat-only stretch; **an FP-only stretch counts as halted** (F18); **an S2 licence that never upgrades does not advance `settled_attempt_finals`**, and its upgrade does (C5) | A [S] |
| T38 | Heartbeat carriers: every H-1 object folded in heartbeat blocks; miner includes them; relay keeps them | A/B [S/P2] |
| T39 | Reporter reward: `collected` base; a pre-drained bond; a bond whose `palw_bond_collateral_is_locked_v6` is false at the conviction DAA gives 0 (the withdrawal mirror); `Σ R ≤ r·Σ collected`; self-conviction < 0 over a grid; commit–reveal (earliest commitment wins, a copier after the conviction loses, unrevealed goes to `reporter_forgone_sompi`); `ReporterCommitted` on a DA or court key is refused; a DA default pays its earliest defaulted accuser on the producer's debit only; **a court default opens no reward, and a basis that does not fit the record's kind is refused (IA-10)**; a commitment older than an open pending conviction is not pruned; 2M j < k rows **expected-FAIL until ADR-0153** | A [S/M3] |
| T40 | Load-time re-derivation after revert, with released and held licensed claims and open DA sessions; **DL-1: restart mid-session and mid-gate on `PanelBound`, `ReceiptLicensed` (`basis_k` 1 and ≥ 2) and Final claims, after an upgrade in block U ≥ floor, and mid-abandon-hold; no `CarriageInconsistent`** (IMPL-2) | A [M5] |
| T41 | v22 golden vectors, empty and inhabited (every new map populated) | A [M5] |
| T42 | Withdrawal completes by `max(since + 12,900, last F + 9,000)` (12,900 = the delay plus the DA lattice, `palw_v2_bond_withdrawal_delay_at_v1`; post-edit 10(d)), with and without a court and a DA session; an accuser that retires with a session open is held until the session and its `refuted_held` resolve (IMPL-4) | A [S/M3] |
| T43 | Trickle regime: rows mature at F + 9,000 | A [S] |
| T44 | Liability not pruned while its row exists; the latch survives an escape re-arm | A [S] |
| T45 | The collector prefers a set with `basis_k ≥ 2` over S2 | B [M4] |
| T46a–T46n | **The audit's F2 tests (SPEC §3.6)**, in `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`: the red V1 rule, an injected step fault convicted before Final (full seat and the partial holder of the leaf's segment; the other partials `SiteNotAttested`), a forged output (full mask only), a `Shape` fault, a conviction after Final, after retirement, an FP claim, kind 1 refused past the fence, an honest run, a wrong domain, the refused kinds, one offence per (seat, claim), reorg and restart, an open session (**restated by post-edit 2:** an open court refuses with `ClaimUnderSession` and writes nothing; a held DA session does not block, the conviction lands; J-7). Every offence passes `palw_v2_validate_objects`, appears in `palw_v2_accepted_objects_for_tests` and is folded | A [M1] |
| T47–T56, T58 | As in `phase2-plan.md` §4, with the plan's queue-lemma test renumbered **T58** (v3.1, so the audit's T46a–n keep their file): T58 queue lemma; T47 `0xFF` key (A-KEY); T48 reorg of mints; T49 IBD inside a backlog; T50 EVM twin; T51 RPC; T52 CLI; T53 drill isolation; T54a/b/c node end-to-end (+ **T54d** automatic `PanelFalseValid`, **T54e** seat roles: Sampled vs S1 Valid, **T54f** a node names the divergent `StepLeaf` its replay found as a DA unit (P2-8d), **T54g** a node routes a fused-attention leaf to a held dissection (P2-8e)); T55 ledger; T56 supply | B [P2] (T47 A [S]) |
| T57 | SR-9: 3 Unavailable on the first panel redraw early; on the second they void `UnavailableQuorum` (S0′ at launch) | A [S] |
| T60 | **Retired in v3.1** into T46a–T46n (SPEC §3.6), which build the claim through the real producer in the crate that has both the producer and the gate | — |
| T61 | **Retired in v3.1** into T18b, T18d–T18h and T46g. T61c's premise (`job_id == claim_id` on an FP commitment) was false: FP is a fixed point too, and T46g asserts `job_id == fp_job_id_v3 != claim_id` | — |
| T62 | The unique path (J-6) as a property: every conviction resolves claim → root → job → index → signer → fault → target through the claim, the liability record (SPEC's resolver) and, past `palw_rcore_plus`, the vesting row with its copied fields (N8), including a retired claim | A [M2/S] |
| T63 | **Retired in v3.1** into T-THREAD and `f1_job_identity_survives_reorg_across_admission`. Its refusal cases are withdrawn: a zero identity is stored, never refused (SPEC §2) | — |
| T64 | Drawn units: seeded by the accepting block; every drawn unit is inside the committed run (pins `exact_decode_tokens = 1` on the t12 attempt lane); held units for held-context classes; all units must be answered; a Flat answer covers every in-run row, and an out-of-run named unit is answered by `OutOfRange` | A [M3] |
| T65 | No monopoly: a Sybil session at L+1 blocks no seat (reverses probe `adr0152v2_r1`); the non-seat cap 3; the non-seat lifetime cap 16; **after 16 refuted non-seat sessions every seat can still accuse**; a seat's 5th session is refused; a second accuser is accepted during an open session; a non-seat may name a `StepLeaf`; a `NeedsDissection` leaf is refused as a unit | A [M3] |
| T66 | After licence and after Final: a session on a Final claim with an unmatured row; the row is re-keyed at opening and cannot latch while open; a default burns the row + S3 + S4 on covering signers only; **a 3-round path concluding at F + 4,000 still charges S4** (locks follow the row, V3S-04); **a coverage licence with the full seat colluding, where a drawn unit outside an honest partial seat's segment defaults, does not charge that seat** (C7); **the floor X2 end-to-end** (an honest seat locates a divergent leaf and convicts) | A [M3] |
| T67 | Pause credit: a seat session's answer at L+1,200 does not finalize the next block; Final at the shifted anchor; overlapping sessions; **a non-seat session pauses nothing** (V3S-08); bounded by retention | A [M3] |
| T68 | Served: Incapable, Unavailable, `Sampled` and missing hold; only `Valid` serves (C1, D3) | A [S/M3] |
| T69 | Session costs: `min(⌈r·S_P(stage)⌉, 13,000)` for `Live`, `Licensed` and `FinalRow` sessions (floor 320.10 / 320.10 / 325.00 at a 13k producer; 2M 6,294.38 live); refuted exposure is held, refunded when the claim is convicted, burned at retirement otherwise; accusing every claim costs ≥ 320.10 per refuted floor session unless the claim is convicted | A [M3] |
| T70 | `Sampled`: an S3 seat signs it; it maps to `PalwSeatAnswerV2::Sampled`; it counts in no quorum or coverage; no lock; **not served**; never dissent-slashed; **an outsider that signs `Sampled` does not satisfy ADR-0147's veto**; refused below the fence | B [M4] |
| T71 | The recount: one per-segment function; V1 = `min(3, #Valid)`; coverage = 2 in the shipped geometry; a mixed set with a V2 full-replay `Valid`; `lock_{max(k, 2)}`; `palw_shard_licensing` is `None` on t12 (and, if armed later, `quorum_per_shard ≥ 2`) | B [M4] |
| T72 | S2 fast path: licenses, keeps its replay charged, holds escrow, does not tick the anchor; an upgrade through SR-10's V3 door (and through a V2 full-replay `Valid`) to ≥ 2 finalizes at `max(L + 120, U)`, frees the replay and ticks the anchor in the same block; no upgrade on the first panel → redraw; on the second → `NotReplayBacked` (S0′ at launch) | B [M4] |
| T72b | V3S-01: one silent or `Sampled` partial seat, and a third party carries the S2 licence of an honest floor claim; another seat's V2 full-replay `Valid` upgrades it, or, with all four partial seats silent, it redraws; the producer forfeits nothing | B [M4] |
| T73 | Q-6 (v3.1): a `Sampled` receipt is never liable under the audit's adjudicator; each lock's recorded mask equals the licensed receipt's assigned mask; DA-7 charges only covering signers by lock mask, under the (seat, claim) key, so a later `ProducerWithholding` filing is a no-op | B [M4] |
| T74 | SR-1b, with a licence at L ≤ bound + window_receipt − 60: a completing supplementary receipt at L+60 flips the release; at L+61 it does not; a `Sampled` supplementary receipt never flips; no un-flip. The V2-door half is S's; **the V3 half through SR-10 is M4's** (post-edit 13) | A [S] (V2 door); B [M4] (V3 door) |
| T75 | Reward timing: sweep (step 2) → `reporter_rewards` → 3d; sweep-time convictions enter `reward_pending` at step 2; a reorg twin; **a speculative commitment to a claim's DA key at admission is refused and cannot win a DA default** (V3S-03) | A [S/M5] |
| T76 | Eq on t12 takes `min(C, 3·G_eq)` with `G_eq = palw_eq_cap_basis_v1` (genesis values pinned, the function asserted); two genesis Eq slashes, then licensing continues (F13) | A [S] |
| T77 | `duty_bind` per class; the ≤ 1 bound; 8k 0.6212; 2M 1.0000 | A [S] |
| T78 | 2M: the top-up at licence; the draw's `lock_2` eligibility; the named void (≥ 3 over-committed seats → RT#2). **The FP lane (post-edit 10(c)):** eligibility is `max(duty_bind, lock_2)` with `lock_2` priced on `rr` (`escrowed_reward` 0), so an FP `Valid` is backed although its lock exceeds its duty | A [S] |
| T80 | kaspad refuses testnet-11 parameters and a testnet-11 datadir at startup | A [S] |
| T81 | `ExecutorRefuted` (= 4), a post-Final DA default (`DaDefault` = 5) and a court conviction (`CourtConviction` = 6) each write a consumed offence with `claim_id` and `collected`; the root is recorded and forfeited by root only for contradictions 5, 6, 8, 11, 12, and is 0 with forfeiture by claim for 9, 10, DA defaults and court convictions (V-2b); the Final is reversed; the matured residual stays as named | A [S/M3] |
| T82 | V-7 after a 6,000-DAA halt with 8 genesis payees: ≥ 1 row moves per block; with no market row waiting the mean is about 1.29 (model); market slots = 8 − non-market keys used (≥ 2) while it waits; a backlog of N rows drains in ≤ N blocks plus reporter rows; a session held open on a row does not stop the drain | A [S] |
| T83 | Restart mid-session, mid-reveal window and mid-backlog: roots equal an uninterrupted replay | A/B [M5] |
| T84 | A-6: an accuser whose 500‰ is full of locks can still accuse on its free half; `palw_accuser_exposure_v1` is re-derived on load; a court challenger's reservation leaves `reserved_exposure` past the fence (C11, IMPL-4) | A [S/M3] |
| T85 | SW-3: `palw_draw_neg_log2_q64_v1` golden vectors (u = 0, 1, 2^63 − 1, 2^63, 2^64 − 2, 2^64 − 1 and 64 pinned pseudo-random values), monotone in u, no u128 overflow; the key comparison `L_i·W_j < L_j·W_i` with ties by `operator_id`; **`W` capped at `weight_cap_msk`** (`PALW_DRAW_WEIGHT_CAP_MSK_V1` = 1,000,000; an operator above it weighs exactly the cap); the two draw domains distinct and in `PALW_PANEL_V2_ALL_DOMAINS` | B [M4] |
| T86 | SW-3/SW-6: the draw's law — for small populations (for example 8 × 939k and 3 × 130k) the empirical inclusion over 2^16 anchors matches the exact successive-sampling inclusion within 4σ; the empirical P2 over the S1 assignment equals `E[A(A − 1)] / 20`; equal weights give today's uniform law; S3 sites and the S1 assignment are unchanged | B [M4] |
| T87 | SW-2/SW-7 (rewritten for SW-A3): **an operator's weight is its one bond's posted collateral, capped at 1,000,000 MSK**; a second bond under the same operator id is refused (`DuplicateOperator`, with T24's negative case); a bond registered at or after the anchor adds nothing; **the one ledger never moves a weight**: a bond's commitments change its eligibility (headroom, Valid lock) and never its key; splitting an operator into two leaves P(at least one seat) unchanged (statistically) and the first-key law identical; removing or adding any other operator never changes an operator's key | B [M4] |
| T88 | SW-1: with `stake: None` the class draw and the outsider equal ADR-0130's and ADR-0147's outputs byte for byte over a corpus of states (t11, devnet and mainnet params, and t12 with the fence forced off), and `stake: None` never refuses on stake; the draw adds no params field, so no fingerprint moves beyond `palw_rcore_plus`'s own (v22's one params-id re-pin is T24's, post-edit 10(b)) | B [M4] |
| T89 | SW-1/SW-3/SW-5/SW-8 (restated, R5): build = accept = fold on **one state** — the assembler's `PanelBound`, `validate_panel_bound_v2_with_policy` and the fold agree, each reading the pre-object base acceptance reads advanced by the block's earlier bindings in claim-id order, with `stake` resolved by `palw_panel_draw_policy_at` at the anchor DAA; including an outsider-judged claim, the Valid-lock filter excluding a bond, two claims bound in one block (the second sees the first's duties), a redraw (second anchor), and a reorg across the bind; **a `PanelBound` offered after its anchor block is refused and the claim voids `BindTimeout` (S0)**. **As integrated (IA-1a, IA-1c):** the anchor block is the first attempt block at or past the slot; a heartbeat at the slot binds nothing (its `sw8_anchor_delay` is `None`), and the fence-off twin binds there; the anchor block voids a claim it does not bind at its own DAA with no bond's collateral or `slashed` moved (`t12_stake_draw_integration`) | B [M4] (A reviews) |
| T90 | SW-5 (rewritten, SW-A4): **the admission jury stays ADR-0147's past the fence (unweighted), and below it**: `palw_admission_jury_v1` and `admission_jury_seated` give the same jury with `palw_rcore_plus` on and off over a corpus of states and never read `stake`; the kept residual is documented, not asserted as a rate (40 registrant Sybils of 13,000 MSK win a jury majority in 0.9728 per audit; a bought class's claims still need the stake-weighted outsider's `Valid`, T89) | B [M4] |
| T91 | SW-9: `ready_eff = min(ready operators, max(seat_count, ⌊ΣW / W_max⌋))` over **capped** weights: 8 genesis → 8; 8 genesis + 40 × 130k → 13; the same plus one ready 20M operator → 13 (capped); one 20M operator + 8 genesis → 8; 40 × 130k → 40, plus one ready operator at the cap → 6 (the SW-A6 residual, §4.2 #17); the floor ungated; 2M (C7) capped as before; **the 8k row room-governed after post-edit 5 and reading 8 at genesis**; the fold, `panel_rate_v1`, `panel_room_v1` and op 186 agree | B [M4] (A reviews) |
| T92 | SW-9: `InsufficientEligibleBonds` under exactly the same condition as the operator lottery over a corpus (where SW-10's floor holds); an operator at the cap sits once and the panel fills | B [M4] |
| T93 | SW-8 (rewritten, SW-A2/SW-A3): nothing after the anchor moves the panel — the attacker's own claims, a carried licence that locks a seat, a slash of an unrelated bond, and a retirement of the attacker's own seated Sybil all leave the bound panel unchanged, because it is derived and bound in its anchor block on one state; a retry on a later state is impossible (a later `PanelBound` is refused, the claim voids `BindTimeout`); the one ledger moves eligibility, never a weight | B [M4] |
| T94 | **SW-10 (new, SW-A1):** with capped weights, a draw whose eligible weight is exactly 875‰ of its base weight binds and one at 874‰ refuses `InsufficientEligibleStake` (`eligible × 1000 ≥ base × 875`, no rounding); an honest-only population of 8 × 939k binds with one seat saturated (7/8) and refuses with two (6/8); with 6 of 8 genesis seats saturated and idle 130k Sybils the draw binds only once the Sybils' weight meets the floor (58 operators of 130k, 7.54M), and with 5 of 8 only at 116 (15.08M) (`v31_review_numbers.py`); a refused claim binds nothing and voids `BindTimeout` (S0, no forfeit; the FP abandon hold applies); build = accept = fold on the refusal; `stake: None` never refuses on stake; M4 pins whether a bought class's outsider population is checked on its own. **As integrated (IA-1b, IA-1c):** the executor's capped weight on both sides — a genesis executor's claim binds with one other seat saturated (7/8) and refuses with two (6/8), the outsider's floor likewise; the refusal voids in the anchor block itself; still to add: the producer's reservation released at that void | B [M4] (A reviews) |

T19 was merged into T37. T58 is Phase 2's queue lemma (renumbered from T46 in v3.1). T59 and T79 are unused. T02c, T72b
and T84 were new in v3; T18d–T18p, T-THREAD and T46a–T46n are the audit's (SPEC); T18m and T85–T93 are new in v3.1;
**the post-edits add T94 (SW-10) and bring in the addendum's T18q–T18y and T18p-M**, rewrite T20, T21, T87, T89, T90,
T91 and T93, restate T02b and T46n, and extend T01, T06 (the worst-state thresholds of SW-10's floor), T17, T22,
T23, T24, T42, T74, T78, T85 (the weight cap and the two ticket domains), T88 (the jury dropped from its draws, T90 has
it; `stake: None` never refuses on stake) and T92 (SW-10's floor clause); T60, T61 and T63 are retired.

### 8.2 The drill replay rule

Any private or drill chain uses a **salted genesis** (the runtime drill salt of P2-12, which moves the premine and
community txids and the genesis `utxo_commitment`) and **drill-only keys** for miners, heartbeats, payouts and bonds.
No card key's premine is ever spent on a drill or private chain: signatures and txids are not bound to the chain, so
such a spend replays onto every chain that shares the card. T53 asserts that a drill chain's registrations, attempts
and convictions are refused on public t12, and that equal blue scores with drill-only miner scripts give distinct
coinbase txids.

### 8.3 The launch gate (replaces "T10 before launch")

The regenesis goes ahead when all of these hold, on the commit that ships:

1. **M1–M5 GREEN** (§7.1). v3.1 makes this include F1-M (T18e on 8k and 2M, T18m), F1c (T18c(iv)) and the stake-weighted
   draw (T06 on the stake draw, T85–T94); the post-edits add the addendum's T18q–T18y (13's heavy budget, the admission
   rule, a claim voided under a DA session), T18p-M, SR-10 in M4 (T74's V3 half) and U2/U3 in S (T17, T22).
2. The kaspa-consensus battery passes twice, with default features and with `--features evm`.
3. **A short drill** on the shipping binary with the drill salt, on ≥ 4 fleet hosts, never on this Mac and never on a
   host that runs a public t12 node. Steps are numbered by position; evidence comes only from logs written after each
   action. **The drilled binary carries P6** (post-edit 11: a hard precondition for any drill of an S-bearing build, or
   kaspad mines attempts the fold refuses) **and SEAT-R** (item 6).
   * **D-1.** A pre-R-core+ binary is refused by `consensus_params_id` / `fork_id`; a public-t12 peer is refused by the
     genesis hash.
   * **D-2.** Floor claims: acceptance → full-service licence (committed drops by `E`) → Final (a row, no payout).
   * **D-3.** A Withheld-seen claim, a claim with a `Sampled` seat, and an S2-licensed claim hold `w + E`; the S2 claim
     upgrades through the V3 supplementary door (SR-10, M4) and finalizes. An 8k claim's licence frees its replay (the
     C7 re-key, post-edit 5).
   * **D-4.** A staged fault: a capture-arm `ExecutorRefuted` before Final (S2). After a Final, a staged
     `PanelFalseValidV2` burns a row. The committer is paid by commit–reveal. A staged 8k relabel is convicted by
     `ExecutorRefuted{IdentityMismatch}` (F1-M).
   * **D-5.** Two accusers on one claim; the drawn units are answered by `MaterialDisclosedV2` from a Valid signer.
   * **D-6.** A heartbeat-only stretch in which a conviction and a reveal are carried and folded.
   * **D-7.** A node restarted mid-session; a reorg across a Final; IBD root agreement.
   * **D-8.** A staged silent panel sinks both panels and forfeits at RT#2 (S0′); SR-9 fires on 3 Unavailable.
   * **D-9.** With one 130,000 MSK drill-only seat registered before the first anchor beside the drill's genesis-size
     seats, every node recomputes every `PanelBound` under the stake draw (no refusal), every `PanelBound` is carried in
     its anchor block (SW-8), and the small seat's panel count is logged against its stake (reported, not a pass
     criterion; O-2 carries the measurement).
   * **D-10.** A 13,000 MSK drill producer that takes one S0′ stops producing, kaspad logs "holding: top up …", and a
     re-registration of that operator (a new drill-only bond key and operator identity, ≥ 13,000 MSK; B-4, the only
     top-up route the code has until §9.3 Q11) resumes production, with no block refused (U2, P6; post-edits 11–12).

   The short drill needs about a day at 200 s/DAA **(estimate)**: the first Final comes about 147 DAA after
   acceptance. A DA **default** (1,200 DAA ≈ 2.8 d at 200 s) and a row **maturity** (≈ 7.3 d) are left to O-7 and O-5.
   **Decided (the operator, 2026-09-24, §9.3 Q12 (b)): the short drill is NOT a pre-launch gate.** It moves after
   launch with the other drills and measurements (implementation first) and runs on public t12 with §8.4's program;
   the regenesis does not wait for D-1…D-10. The steps above stay the drill's content.
4. T41's golden vectors are those of the shipping commit.
5. **Phase 2's load-bearing parts GREEN on the shipping commit (C12):** P2 complete, with T03, T05, T23 (both halves;
   B-3 enforced at the UTXO layer through `palw_v2_locked_bond_outpoints` and `palw_v2_bond_burn_obligations`, not only
   in the fold), T25 (Decision A separation), T47–T53 and T58 (T53 is drill isolation, T58 the queue lemma) GREEN.
   Drill steps D-4 and D-5 need P2-7 and P2-8 in any case.
6. **SEAT-R ships in the same binary as F2's fence (post-edit 3)**, with T18p-M GREEN, and so do the seat fixes
   SEAT-S1…SEAT-S4 and the drill hook `PalwDrillFaultV1` (§7.1 "Seat fixes"; the addendum's §0 lists "the seat fixes
   S-1 to S-4" among the prerequisites the gate cannot ship without). The `AnyValid` sites of 9 and 13 stand only if
   SEAT-S1, SEAT-S3 and SEAT-S4 are in that binary and T18p-M is GREEN, and otherwise flip to `Whole` before the build
   (Q-6). SEAT-0 is on the launch line (post-edit 4).
7. **The `AttnFused` held-class gap is closed as decided (post-edit 8; decided 2026-09-24, §3.9 "Decided").** On 8k,
   A-held (C1–C5 and object 57 `CourtAttnRootClaimedHeld`) convicts an attention lie on the 8k fixtures, the kaspad
   producer answers a held dispute with object 57 inside its response deadline, and the 8k real-weight timing drill
   shows the windowed re-execution fits that deadline. On 2M, the accepted bound (C) holds: `c_2M = 1`, the escrow held
   to Final and RT#2's forfeiture kept. **Until then this gate is not met.** **Amended (IA-12):** the operator moved
   drills and measurements after launch, so the 8k real-weight timing drill runs on public t12 after launch (§8.4). And
   2M is **closed** at launch (U-D1), not accepted under (C): 4-quater's closure is in the shipping commit (item 9).
8. **The launch-line fixes are in the shipping commit:** the licence-stall fix, the panel-room hold re-keyed on C7
   (T20, T21; post-edit 5), and the shard court's one-move rule (`8be0f661`, J-8; post-edit 7).
9. **Ship conditions (IA-14; a checklist, each item on the commit that ships).**
   * [ ] **F2's and F1's fences** (`palw_offence_attribution`) ship only with **SEAT-S1**, **SEAT-S2** and **SEAT-R**
     (the ship condition of the audit's M2 review, its H-1, not the Phase 3 review's H-1 below; §7.2). SEAT-S2's kaspad half compares `output_root` under `CoreV1` in the
     production replay step, `palw_seat_replay_step_v1`, attempt and FP. `449fd892` tests it on a real model-class
     replay (the held A16 v7 row): the honest `CoreV1` claim licenses, and the same roots under a Legacy producer's
     `output_root` are refuted. A floor test cannot show this, since the floor's root is the same under both rules.
   * [ ] **The `AnyValid` fence (M2 Phase 3)** also needs two fixes from the audit's M2 Phase 3 review. **H-1** (the
     S3 layer-sample path must not sign `Valid` before `verify_material`) **is met on the integration line:** past
     SEAT-R a partial seat's resume abstains before the S3 arm (`palw_v2_try_partial_resume_v1` returns `Abstain`
     whenever `palw_seat_r_in_force_v1` holds, which on t12 is from the first block; test
     `c1_s3_never_attests_past_the_fence_without_verify_material`, "S3 is below the fence only"; `1e20edd50`).
     **H-2 is pending:** the FP S1 resume must require `palw_fp_job_pin_of_context_v1(&ctx) == duty.fp_job_pin_v1()`.
     It needs M2's `d675423d`, and `kaspad/src/palw_panel.rs` on the integration line has no such check. Until H-2 ships
     with T18p-M GREEN, the identity sites stay `Whole` (Q-6).
   * [ ] **The producer's V2 DA responder ships in every build that arms `palw_rcore_plus`**, whatever the flag below
     says: duties from `da_sessions`, answered with `MaterialDisclosedV2`, with a processor end-to-end test that an
     honest producer answers an accusation and is not charged. **Why unconditional:** DA-7's producer charge (S1 or
     S3, the `DaDefault` record, the reward) does not wait for the flag (`da_default_charge_v1`;
     `palw_da_signer_liability_armed_v1` gates only the signer half, IA-11). On the integration line kaspad's
     `palw_da_duties_v2` lists only `DefaultDisputed` claims, so no R-core DA session is answered and an honest
     producer is charged at every accusation's default (the M3 review's F2, rated CRITICAL).
   * [ ] **`PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` goes `true` only** in the commit that lands the seat DA auto-answer
     (P2-7, `MaterialDisclosedV2` from a covering signer). This is the condition for the **signer half**. On the
     integration line the constant is `false` and kaspad still builds the v1 answers past the fence. Until it is
     `true`, DA-7's signer S4 and N9 stay dormant (IA-11). P2-7 is in flight on `rcore/p2-panel` (`b34df2ff`,
     `fc5e30d8`). It sets the flag, and its duty list (`palw_disclosure_duties_v1`) covers the sessions on the node's own
     claims as well as those its locks cover, so it is also the candidate for the producer's responder above. It is not
     merged into the integration line; the end-to-end test above is the check.
   * [ ] **`misaka-palw-derive` and the stranger script recompute `output_root` under the network's attempt rule**
     (`CoreV1` on t12; `a4682a8d`). Otherwise an honest t12 model claim reads as an output mismatch. The FP workers set
     the network's rule too.
   * [ ] **`PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`** with S-4's funnel on the shipping commit (IA-7). kaspad refuses
     to start otherwise.
   * [ ] **4-quater's consensus half and P-1** (`palw_class_verify_deadline`; the pruning depth ≈ 74,920 DAA, D_cap
     16,000) are in the genesis params, so 2M is closed by rule (U-D1). The depth can be set only at the regenesis.
   * [ ] **A-held and the shard court** (`feat/t12-aheld`, over `8be0f661`) are merged after the audit's review,
     with B's routing, object-57 auto-answer and N4 wiring (item 7).
   * **Not a launch condition (IA-13):** SEAT-S4's forged-sibling residual, where a forged sibling path costs a whole
     segment replay in the one C7 slot (bound `PALW_SEAT_S4_CANDIDATES_PER_SEGMENT_V1` = 8, then a silent end). It is a
     **2M flag-day item** needing a family API change (return the recomputed range fold, or take several paths). C7 is
     closed at launch.

### 8.4 The post-launch observation program on public t12 (replaces v2's T10 soak)

Run on public t12 from genesis. Owner B (the analyzer and reports), A (review). Each item has a live pass criterion.

| # | observation | pass criterion |
|---|---|---|
| O-1 | Licence cadence | ≥ 30 licences within 3,000 DAA of the first Final (else the second-clock depth is retuned by a fence before any value) |
| O-2 | f, p and the door histogram (`getPalwVesting.licence_histogram`); coverage capacity of the genesis seats against the floor lane (T-2(e)); 8k throughput under the rate room, released at licence (T-2(a), post-edit 5); the share of S2 licences that redraw (Q-5); **each operator's panel inclusion against its stake, the share of panels with a dead seat, the design-point threshold of the live eligible weight vector (SW-A5; 17.29M at launch), the count of `InsufficientEligibleStake` refusals and the lowest eligible share seen (SW-10), and each non-C7 class's `ready_eff` against its ready operators (SW-9, §4.2 #17)** | reported weekly; figures replace the §3.7 model rows |
| O-3 | `q_f` against an adversarial producer (P2-13; naive, garbage, borrowed) | every attempt charged in its window. For M6 (X10): every attempt **attributed** (S1/S2 through DA or `ExecutorRefuted` by honest automatic filers), per strategy |
| O-4 | A staged post-Final conviction burns a row before maturity | the row burns; the earliest committer is paid |
| O-5 | The first natural maturity | latch → move → mint; refused by the mempool at mint + 599 and spent at mint + 600 with 0 anchors; the second-clock depth retuned from the measured cadence |
| O-6 | FP-busy, attempt-quiet stretches | count of halted stretches caused only by FP traffic (F18, §9.1 Q6) |
| O-7 | A live DA default by a withholding test producer | S1 + strike at the deadline; reward to the earliest accuser |
| O-8 | A heartbeat-only stretch with a carried conviction | folded in a heartbeat block |
| O-9 | Market load while rows mature (T30 live) | queue ≤ 1,032; ≥ 1 row per block while mature; the market gets its 2 slots |
| O-10 | A retirement completes its withdrawal | by the B-3 bound |
| O-11 | 2M's closure at launch (U-D1; IA-12; was "one 2M claim end to end, or its named hold") | every 2M attempt and FP claim is refused (`ClassDeadlineUnmeasured`, the design's T-D2) until 2M's flag day; until 4-quater lands, as T20 |
| O-12 | A fresh node's IBD inside a vesting backlog | roots and the next coinbases equal an archival node's |
| O-13 | **SW-8's anchor lane (IA-1a):** the count of `BindTimeout` voids per anchor block (step 4c) against the claims that reached their slot, the voids at the window's backstop (no attempt block in the window), and the attempt-block ratio (attempt blocks over all chain blocks, and the longest heartbeat-only stretch in DAA) | reported; the bind window (600) is re-derived from the measured attempt-block rate, by a fence if it must move |

**Deferred to after launch (the operator, 2026-09-24: drills and measurements after launch, implementation first;
IA-12).** These run on public t12 together with the program above, not before the regenesis:
* the SEAT-0 mixed-version live drill (its kit is prepared on the drill host);
* the deadline design's M5 (the 8k court turn), M5b (the panel-loop latency during a replay), M9 (Qwen3.6's I/O) and
  M12 (block bytes, for P-1's storage and IBD cost at ≈ 74,920; P-1 goes in without it);
* the 8k real-weight timing drill (§3.9, §8.3 item 7);
* §8.3 item 3's short drill, D-1…D-10 (§9.3 Q12 (b));
* the 8k licence rate under the V1 fallback, and replay durations;
* O-13's `BindTimeout`-by-anchor count and attempt-block ratio.

**Failures after launch.** A failed O-item is fixed by a fence on public t12, because rows, sessions and rewards are
state and carry across. A regenesis is reserved for a structural defect (a root or encoding error), under the rule
"regenesis takes only proven structural changes". A broken maturity path delays payouts but loses nothing: rows stay
in state until a fixed move lands. **t12 carries no value before O-5 passes.** The operator announces this.

**X10 (M6).** Arm `palw_rcore_attributed_charging` at a public flag day only after M1–M5 and O-3's attribution
criterion, with a drill that crosses the flag-day height on the shipping binary.

---
