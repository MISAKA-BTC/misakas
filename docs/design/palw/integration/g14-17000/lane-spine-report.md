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
