# lane CERT — per-claim `SecurityCertified` on the kernel route (2026-10-26)

Branch `claude/g14-cert` (base `c3cb6c7b7` + SPINE `ebb8b911f`). No cryptographic proof of computation is used (PRINCIPLES §8). The model tiers are unchanged.

## What was wrong
`misaka_palw_kernel::ledger` reaches `ClaimStateV1::Final` from the Panel pass and the window alone, and pays the producer out of the job escrow in the same tick. There was no Gate C draw for a kernel-route claim. Kernel claims carry no consensus weight (`B = F = 0`, `palw_kernel_settlement_v1`), so the uncertified credit went to **settlement / reward**.

## What was connected (the existing RFC-0018 machinery, no new runtime)
- **A third subject kind in aux 136:** `'K' ‖ id` holds a `PalwClaimCertificationRowV1` for a kernel claim. The id is `palw_kernel_claim_certification_id_v1`, domain-separated. It is the same row, pool, draw, seed anchor, inner-75 attestation and `palw_certification_verdict_v1` as chain and native39 claims. A new versioned row, `'H'` → `PalwKernelCertifiedRewardHoldV1` (v1), stores the held reward. Both rows have codec arms (`generated.rs`, `wire_generated.rs`, `wire_kernel_aux.rs`).
- **Hold at Final** (`palw_kernel_route_fold_v1::apply_settlements_to_exec_vesting`): past `gate_c_at(now)`, the producer's escrow `FinalReward` is not paid. `hold_kernel_final_reward_v1` commits the `'K'` row and holds the amount. The poster's debit stands. Verifier check fees and the EXEC fee route are untouched.
- **Sweep** (`sweep_kernel_claim_certification_v1`): draws from a later anchor and decides. The facts come from the ledger: `still_final` means Final and not convicted, and an open demand counts as pending. At the decision `settle_kernel_reward_hold_v1` runs once:
  - **certified:** the producer is paid through the same path the released Final used (bond-budget recipient, RFC-0016 note, kernel payout);
  - **any other outcome** (NotCertified, Expired, NoPopulation, Withdrawn, including a post-Final conviction): the reward goes back to the poster in full.
- **Attestation admission** (`processor/palw.rs` preflight, fold `try_attest_claim_certification_v1`) now also accepts the `'K'` key.
- **Values:** Gate C stays refused at any real height (`validate_palw_rfc0018_v1`, the arming record list is empty). So the policy, m/k and beacon remain typed-missing in production. The tests use `TEST_ONLY_KERNEL_CERT_POLICY_V1` (m=5, k=3, short windows). These are **not approved values**.

## Found on the way
The kernel's `settle` drops zero-amount instructions. When an escrow is fully paid, it emits no `ReleaseJobEscrow`, so `palw_settlement_once_fold_v1::kernel_producer_reward_at_v1` (which needs the triple) does not see the producer reward. The `EscrowFinalReward` once-marker is therefore skipped on that shape. CERT uses its own reader (`kernel_producer_final_reward_v1`), which accepts both shapes. The once-only file belongs to Codex and was not edited.

## Tests (`cargo test -p kaspa-consensus --lib -- g14_cert_kernel_route g14_spine_panel1`, exit 101: only the spine table fails)
- `canonical::cert::g14_cert_kernel_route_final_pays_only_after_the_drawn_verifiers_certify_and_a_reorg_cannot_carry_the_payment`: **ok**. It checks:
  - Final pays 0 and the hold equals `claim_reward`;
  - k positives alone do not pay before the prosecution window;
  - a reorg onto a branch without the approval that crosses k leaves the claim undecided, unpaid and still held;
  - after the window: Certified, the producer is paid once and the hold is gone;
  - an IBD node runs `agree()` and reads the claim as certified.
- `…an_approval_shortfall_or_unavailable_verifiers_never_pay_the_final`: **ok**. One drawn bond reports `Unavailable` and the rest stay below k. Result: NotCertified past the deadline, the producer gets 0 and the poster gets the full reward back.
- Spine table: S3a' (both rows) **PASS**, S3a and S3c PASS. Both S3b rows still FAIL on the seat-charge assert SPINE already reported (`canonical.rs:676`, not this lane). The other stages are UNIMPLEMENTED (F02/F03/SPINE).

## Remaining
- EXEC kernel-fee route and V2-root rewards are not gated by a kernel claim's certification. This is out of the slice.
- RPC op 210/212 does not yet show the certification status of a kernel claim.
- Decided `'K'` rows are never retired (they remain as public status).
- Choice for 統括/user: on failure the reward is returned to the poster rather than burned.
- m/k and policy wait for VAL and the user. Gate C arming record and the `route_armed` family stay central.
