# RFC08 mesh payment failure atomicity

Status: actual Core pre-object RED confirmed; production fix and both GREEN parent controls PASS. This is a fenced settlement invariant, not all-reward or actual-coinbase acceptance.

## Defect and actual boundary

`mesh_pay_from_reserve` writes a nonzero once-only settlement marker before `add_panel_payout`. It previously propagated NativeResource but converted every other payout error to `Ok(0)`. In particular, a checked-add Overflow left an unpaid obligation marked as settled. The actual mesh sweep treated zero as success and retired the audit row.

The repair propagates every payout error when `settlement_once_active_at(now)`. Existing object/pre-object fold boundaries already return that error and do not return the mutated candidate builder, preserving the parent and its open obligation. No new whole-state checkpoint clone is introduced. Duplicate obligations still skip before touching the payout queue, including a full queue. NativeResource propagation remains unchanged. Below the once fence, the exact historical error-to-zero behavior remains.

## Actual RED

The test uses the actual full pre-object fold, not only the payment helper. It constructs a stored audit/queue parent, with eight earlier payout rows consumed by the real queue drain, so the target panel queue survives into the real mesh sweep. The overflow balance is **artificial**: this control does not establish reachability from lawful signed native history, an exploitable public-chain loss, or native coinbase acceptance.

With the old production code, the diagnostic parent returned `Ok(())`, with exactly:

```text
MESH_ONCE_OVERFLOW unexpected_accepted markers=1 reserve=100 queue=18446744073709551615 audit_retained=false
```

The active-once assertion failed; the dormant historical-skip control passed. Diagnostic source build succeeded in 558.84 seconds with source/config identities unchanged. Fixed core executable SHA256: `64e16662c437e920652915b413fca81f26a2bc1f32b66b11ea2598f8c413e07c`. Record and full runtime are indexed in `/tmp/misaka-rfc-mesh-once-diagnostic-red-result-20261025.json` and `/tmp/misaka-rfc-mesh-once-diagnostic-red-runtime-20261025.log`. An earlier integrated RED also failed the active assertion but lacked actual-result diagnostics; it is not the source of the exact values above.

## Required GREEN controls

Filter: `palw_state_v2::tests::settlement_once_v1::mesh_once_atomicity_v1`, two parents.

- Same actual full fold must return `Overflow("panel payout")`, leave parent root/marker/reserve/audit unchanged, then allow the same obligation to pay once after repairing only the artificial queue headroom. Check the exact amount/reserve, marker, audit retirement, delta apply/revert and duplicate no-op with a full queue.
- Once-off actual fold must keep its historical skip, no marker and unchanged reserve/target queue, with delta apply/revert.

Existing once and actual mesh-fold controls must also pass. Runtime results and durable source/binary/log evidence will be added after execution. No public activation, fee policy, liability horizon, model tier or verifier approval change is included. A positive real signed native mesh payout through actual coinbase, restart and reorg remains separate acceptance work.

[Durable RED evidence index](0008-mesh-once-failure-atomicity-2026-10-25.json) embeds the command, source/config identities, executable SHA and full diagnostic log.

## Final local verification — 2026-10-26

Both actual preparation controls and all 23 once-registry controls passed (the two new tests are included, not added twice). Artificial parent and actual-coinbase scope limits remain. The fixed six-package build matched its source manifest and six executables were pinned in persistent artifacts. [Full source/binary/log evidence: 53 distinct parents](0006-0008-gdn-auth-mesh-once-verification-2026-10-26.json). This does not close the whole RFC or campaign.
