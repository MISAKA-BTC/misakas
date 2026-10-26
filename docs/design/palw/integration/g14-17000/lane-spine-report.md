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

## 2. Stage table (in the harness's order)

| stage | status | owner / missing |
|---|---|---|
| S1 original admission: atomic funding/resources | UNIMPLEMENTED | F02: `palw_funded_original_coordinator_v1` is not wired into the kernel route's admission fold. Today only `Rules::admission_cost` is debited |
| S2 duties derived from the original + actual supply | UNIMPLEMENTED | F02: kernel-route duties via `palw_court_service_v1`, with the accepting block's supply recorded |
| S3a honest, wrong challenge dismissed → Final → paid once (IBD) | run by the harness (existing `canonical` driver) | — |
| S3a' per-claim lottery approval `SecurityCertified` gates Final/settlement | **UNIMPLEMENTED, no lane owns it** | `misaka_palw_kernel::ledger` reaches `ClaimStateV1::Final` from the Panel pass + window alone. No `palw_claim_security_certified_v1` / Gate C draw exists for kernel-route claims (only native39 `owning39/master/node_e2e.rs` has it) |
| S3b lie, all seats colluding, outsider convicts before Final, slash (pruned import) | run | — |
| S3b lie after Final, conviction, liability stands (IBD) | run | — |
| S3b' conviction through the public paid ingress under refilling collusion (T05) | UNIMPLEMENTED | F03: kernel `file_proof` via `palw_court_paid_ingress_v1`. The schedule is typed-missing until injected |
| S3c silence: demand → default, fixed penalty, no fraud slash (IBD) | run | — |
| S3c' no supply → no slash, held | UNIMPLEMENTED | F02: a supply-absent branch in the default fold (today the default is deadline-only) |
| S4 reorg across the deciding DAA of S3a–c | UNIMPLEMENTED | SPINE (next): reorg + `agree()` on the canonical slice. Restart/IBD/pruned import are already covered per outcome by `start_node` and by `recovery.rs` |

The PASS/FAIL of the "run" rows is in the build result recorded in §5.

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

## 4. Requests

- **F02:** close S1, S2 and S3c' on the kernel route (call sites: the kernel route's admission and default fold). Expose a callable entry that the spine can invoke from `canonical`.
- **F03:** a kernel-route `Rules::file_proof_paid(...)` through the paid ingress, plus a refilling-coalition fixture.
- **ACT:** the 17,000 record must name exactly this slice's family. `route_armed` has no kernel-route family today (`PalwG14RouteFamilyV1` lists only Legacy*/Fp*/NonPublicEvidence), so it needs either a new family or an explicit mapping.
- **VAL:** provisional values for `default_penalty`, the dismissal fee and the window on this slice.
- **統括 (central):** decide who adds per-claim `SecurityCertified` to the kernel ledger (S3a'). Until that exists, the slice can reach Final without a lottery approval, which violates CLAUDE.md, so the slice must not be armed. The missing kernel route family in `route_armed` also needs a central decision.

## 5. Build result (2026-10-26, `cargo test -p kaspa-consensus --lib g14_spine_panel1`, base 3c5f4c24b)

- PASS: S3a (honest), S3c (default)
- **FAIL: both S3b runs** at `canonical.rs:676`: "the colluding seats are not charged by the producer's conviction". Each seat's slashed amount went from 100000000 to 283333333 (and to 783333333). On this base, a conviction also charges the colluding seats (seat-covered liability). The existing `canonical` assert expects the opposite.
  - Either the seat charge is the intended new rule (RFC08 lifecycle / Codex `f4054fe39`), and the assert must be updated deliberately by its owner, or it is a regression.
  - SPINE did not change the assert. **統括 decides which.**
- UNIMPLEMENTED: S1, S2, S3a', S3b', S3c', S4 (see §2). The table test is red, as intended.

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
