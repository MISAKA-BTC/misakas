# lane SPINE — G14 Panel=1 kernel-route vertical slice (2026-10-26)

Branch `claude/g14-spine` (from `3c5f4c24b`). Harness:
`consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/canonical/spine.rs`,
test `g14_spine_panel1_kernel_route_stage_table`. It FAILS while any stage is still open. Nothing in it is skipped or weakened.

## 1. The slice

- **Route:** the kernel route (`palw_kernel_route_v1`) in `Mode::Panel` (Panel-licensed; the interim Panel's every seat colludes and signs a pass).
- **Class:** the class card 1 registers in `canonical::world` on the canonical testnet-12 harness (`t12_with_harness_cards`). This is a genesis-designated tier (1.0); the model tiers are unchanged.
- **Job:** the single-prompt decode job of `canonical::claimed`, with one claim per producer.
- **Why this slice:** `canonical.rs` already pushes the most branches of one claim through the real virtual processor, fold, mempool/template and a cold node (IBD and pruned import): honest + wrong challenge, a lie before and after Final, withheld material leading to demand and default, plus non-interference. `recovery.rs` adds restart, reorg and IBD for the pursuit. No other route has all three outcomes on a cold node.
- The OPV variant stays out of the slice (Panel=0 is not armed).

## 2. Stage table (in the harness's order; rewritten 2026-10-27, lane G14-FIX, to the current harness output)

Run: `cargo test -p kaspa-consensus --lib g14_spine` on `claude/g14-spec1-s2` (base 13a745fed + G14-FIX). S1/S2/S3c''' use the
PRODUCTION kernel court admission schedule (`PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1`, pinned by G14-FIX SPEC-1, in force past
`palw_kernel_route_panel_one_v1`); no schedule override. The one remaining TEST-ONLY harness value is the Panel=1 external gain
bound (`f03k::TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI`): the processor's `cfg(test)` cap of 4 adjudications per block and the
unclocked (below X3) exposure make the censorship relation 140 MSK < 505 MSK, so the class would not register with the production
500 MSK bound. With the shipped 64 runs and X3 it holds (20,160 > 1,585 MSK).

| stage | status |
|---|---|
| S1+S2 admission reserves court capital + service book atomically; duties derived; supply recorded (production schedule) | PASS |
| S3a honest, wrong challenge dismissed, Final, paid once (IBD) | PASS |
| S3a' lottery approval: paid once, reorg takes it back, IBD agrees (TEST-ONLY cert policy) | PASS |
| S3a' approval shortfall: NotCertified, never paid, poster refunded less V3 burn (TEST-ONLY cert policy) | PASS |
| S3b lie, all seats colluding, outsider convicts before Final (pruned import) | PASS |
| S3b lie after Final, liability stands (IBD) | PASS |
| S3b' T05 before / after Final, Panel=1 (f03k::t05) | PASS / PASS |
| S3c silence → demand → deadline default (IBD) | PASS |
| S3c' kernel supply never missing under a full court | **FAIL** (see §5) |
| S3c'' premise None (TEST-ONLY) → no-slash hold; pinned premise → decided | PASS / PASS |
| S3c''' native bound claim: typed-missing → held; pinned premise → default | PASS / PASS |
| S4 honest / silence decided once across a reorg | PASS / PASS |

## 3. T01–T15 mapped onto the spine

| T | spine scenario | status |
|---|---|---|
| T01 | S3a + S1/S2 funded outcomes | partial (S3a); S1/S2 F02 |
| T02 | S3c + S2 supply | partial (deadline default); supply F02 |
| T03 | forged signatures / fake Panel report rejected before effect | existing `unbound_refused`, `signed_provider_challenge_v4.rs`; not yet a spine stage |
| T04 | flood intake | VAL/F03 (P1-5); not in the spine |
| T05 | S3b' | F03 |
| T06/T07 | last notice / ±1 at every deadline on S3c | SPINE + P1-1 clock; open |
| T08 | offline miner, distinct signer and relay | P1-3; open, no native kernel path |
| T09 | shared bond, shortest lease | P1-2; open |
| T10 | fold-only internal lie | existing `g14_canonical_g14v_*` lies; add as a stage |
| T11/T12 | concurrency and hard-boundary concentration | P1-5/VAL; open |
| T13 | oldest claim loses service, others progress, no false slash | S3c' + F02; open |
| T14 | S4 + existing IBD/pruned import | partial |
| T15 | fence boundary old/new | ACT; open |

## 4. Requests (2026-10-27)

- **S3c' owner:** the stage fails inside the harness's block insertion (`t12_round_lane_e2e.rs:381`: a heartbeat carrying the crowd filings is refused, "block persistent storage mass 666668 exceeds limit of 500000" — the fixture packs too many filings per block);
  see §5. It is not caused by the schedule values: the production schedule equals the earlier test-only estimate field for field.
- **Harness:** a world with the shipped 64-run cap would let S1/S2/S3c''' also drop the TEST-ONLY gain bound.

## 5. Build result (2026-10-27, lane G14-FIX, `cargo test -p kaspa-consensus --lib g14_spine`)

- 7 of 9 `g14_spine_*` tests pass; the table: 15 PASS, 1 FAIL (S3c'). `g14_spine_s3c_prime_*` fails on its own as well.
- Earlier results (2026-10-26, base 3c5f4c24b) are in §6 and §7 as history.

## 6. Second round (2026-10-26): origin of the S3b seat charge, S4, F02 premise, F03b

### Bisecting the S3b seat charge
The same test (`..._ibd_node_convicts_a_covered_lie_before_final`) fails at `canonical.rs:676` on all three bases: f4054fe39, 55510b56a and 3c5f4c24b. So it is not caused by RFC08/RFC07. There is no change to slash code in f4054..3c5f4.
- **Origin:** commit 5bdd427fb (2026-10-16, "close G-K1 ..."). At a conviction, `palw_g14_lane_closure_fold_v1::charge_kernel_signers_on_conviction_v1` charges the liability lock of each passing-receipt signer (seat). `palw_kernel_route_fold_v1.rs:1391`.
- **Judgement: an intended new rule.** Reasons:
  - It closes audit G-K1 ("a counted PASSING receipt locks the signer's liability ... charged at any conviction"; `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` §3.4).
  - It has its own E2E: `g14_lane_closure.rs::kernel_route_valid_signers_are_charged_on_conviction`.
  - The canonical assert ("the colluding seats are not charged") is from 2026-10-10 (c995dfcc3), from before G-K1.
- **Proposal to the canonical harness owner (G14C lane):** change the assert in `canonical()` so each seat's slashed amount rises by exactly that seat's G-K1 lock amount (`palw_g14_lane_closure_v1` lock relation). Also check that the producer's slash and the bounty are not changed by the seat charge. SPINE has not changed the assert.

### S4 (reorg across the deciding block)
- Implemented: `decided_across_a_reorg` with two tests, `g14_spine_s4_{honest_final,default}_is_decided_once_across_a_reorg`.
- **Result: FAIL on both.** `recovery::reorged_node` builds a heavier branch with empty heartbeats, and the "DNS reorg gate" (`palw_reorg_strict_economic_win` = 0 in `canonical_config`) refuses it: "B out-works A: Z reorgs onto B".
- The fix needed is a competing branch that also wins economically. For example, re-carry the same claim, Panel receipts and demand on B, then add heartbeats. That is SPINE's next task. The gate's behaviour itself is correct.

### F02 inclusion premise (merged 9803e043d)
- **S3c'' (premise None, the production value) FAIL.** The kernel route still turns the demand into `Unavailable { producer_defaulted: true }` at the deadline. `misaka_palw_kernel::ledger`'s default does not consult `court_service_inclusion_premise_v1`, so the F02 rule ("None ⇒ no default, held") is bypassed on the kernel route.
  - **Request to F02:** wire the premise into the kernel ledger's default.
- **S3c''' (with a test premise) UNIMPLEMENTED.** `palw_accept.rs` copies the const `PALW_COURT_SERVICE_INCLUSION_PREMISE_V1` directly, so a native E2E cannot inject a test premise.
  - **Request to F02 / central:** a `#[cfg(test)]` setter that only takes values with a `TEST_ONLY_` name.
- The existing S3c PASS is the kernel ledger's deadline-only default. It does not show F02's rule.

### F03b (cherry-picked 1298841fa) and a continuous S3b'
- I did not build the continuous run. The two F03b legs cannot be joined from the spine:
  - F03b is a core-fold unit test on the TIR public route, not the kernel route.
  - Capture/refill and bottom → `Voided{CourtHeldVerdict}` → slash use different fixtures: the frame fixture `(7,1),4` and the history fixture.
  - The fixture Panel has 3 seats.
- **Request to F03:** either serve both legs with one Panel=1 fixture/world, or add a shared frontier and paid-ingress `file_proof` to the kernel route. **To 統括:** a decision on whether to switch the slice to the TIR public route.

### Current stage table (`g14_spine_`, 4 tests run, all FAIL as designed or for a real gap)
PASS: S3a, S3c (deadline-only). FAIL: S3b×2 (G-K1 assert), S3c'' (premise bypassed), S4×2 (reorg gate). UNIMPLEMENTED: S1, S2, S3a' (CERT), S3b', S3c', S3c'''.

## 7. Third round (2026-10-26): coordinator's decisions applied

- **G-K1, canonical S3b assert updated by the G14 coordinator on the owner's (G14C) behalf.** The decision: 5bdd427fb is an intended rule.
  - `canonical()` now reads each covering seat's `PalwKernelSignerLockRowV1.amount` before the filing. It asserts that the seat is charged exactly that amount, and that every covering seat holds a lock.
  - The producer-slash and bounty asserts are unchanged.
  - Result: **S3b ×2 PASS**.
- **S4 PASS ×2** (`recarried_node`). The strict economic win rule is kept.
  - A reorg whose fork sits below the claim is refused by design. Measured result: `DominanceViolation`, an all-economic tie deeper than `PALW_REORG_SHALLOW_TIE_DAA_V1` = 2 DAA.
  - So the fork is the last selected-chain block below the deciding DAA. B re-mines A's blocks above the fork with the same non-coinbase transactions, then adds heartbeats until it out-works A.
  - On Z, the deciding branch is the same. Collateral and reservation are the same. Queued plus coinbase-delivered amounts are equal for the producer and the demander (paid once). A DAG-IBD node holds the same root.
  - **Limit:** a reorg of the claim and receipt blocks themselves (fork below the claim) cannot be won under the strict rule while the economics tie. Reaching that would need a branch that carries more economic content, which is outside the slice.
- **S3b'** goes to F03k (claude/g14-f03k). **S3c''** (premise wiring into the kernel default) goes to F02.
- Current status: PASS S3a, S3b×2, S3c, S4×2. FAIL S3c'' (F02). UNIMPLEMENTED S1, S2, S3c' (F02), S3a' (CERT), S3b' (F03k), S3c''' (F02/central).
