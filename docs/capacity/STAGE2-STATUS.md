# ADR-0160 stage 2 (rcore/cap-s1) — status: gate PASS

Same branch and base as stage 1 (`rcore/cap-s1` off `c3dbaee3c`; nothing armed anywhere; the t12 pins
`dbbc9104…` / `5de80e64…` / `7c652212…` unmoved). Commits: `091856e1b` (consensus: F-Q, F-S, the credit
under the door), `9901010ff` (kaspad: the operator's audit duty), and this status.

## What stage 2 is

The user's plan (2026-09-26/27): **Q** = the audit door (ADR-0160 v3 D-23, §5.9) and **S** = issuance slots
with outstanding / burst / rate caps (§6), "DoS/queue controls only, never the safety mechanism"; gate =
"mass claims keep liability, rate and outstanding controlled; detection must be an on-chain observable
(audit receipts)". Each rule behind its own dormant fence in `PALW_T12_CAPACITY_FENCES_V1`.

## Q — F-Q `palw_capacity_audit_door` (prereqs F-L, F-B, lane A)

* **Credited** ⟺ accepted past F-Q ∧ attempt ∧ `E > 0` ∧ attributable (not on the C7 list) ∧ its F-L
  step's credit reaches `q_seat` (250‰). A credited claim commits `m_c = ⌈E/ρ⌉` (v3 §5.3) and reaches
  `Final` only with `k_aud` receipts (1 below ρ 250, 2 from ρ 250) from distinct members of its **pool**:
  lane A's operator bonds that are neither its producer nor its panel's seats, nor frozen, nor excluded.
  An unaudited credited claim waits (licensed, J-1-capped): never voided, burned or charged; its `Final`
  deadline re-arms at `max(floor, audit_daa)`.
* **Detection is on chain**: `AuditReceiptBatchV1 { auditor, entries: [(claim, reproduced_root)],
  signature }` (object tag 60, ML-DSA-87 by the auditor's genesis key, checked at acceptance). The fold
  refuses a batch by a non-operator or an excluded auditor or with a root that is not the claim's; skips
  entries the door does not keep; stores kept ones in the rooted `audit_receipts`.
* **Accountability** (§5.9 (g)): convicting a receipted claim puts every receipting auditor in the rooted
  `excluded_auditors` for good.
* **Backlog** (§5.9 (f)): a credited admission is refused `AuditBacklogFull` while `A_max` credited
  claims await audit (queue bound; uncredited claims never refused by it).
* **AS-1′** (v3): only a credited claim's seats are priced by the step; an uncredited claim's duty and
  lock are today's. `validate_palw_v2` refuses a credited step with no door at or below it.
* **Node duty** (`kaspad/src/palw_audit_duty.rs`, always on by identity): a pool member replays each
  credited licensed claim on its turn (first `k_aud` members at once, one more every 30 DAA), batches
  the reproductions (≤ 256 a batch), signs and carries them; a refuted replay posts nothing and says so.

## S — F-S `palw_capacity_issuance_slots` (prereqs F-W, F-L, F-E)

`u = ⌊C/6,500⌋`, `N_out = u·ρ`, bucket depth `B = max(4, ⌈uρ/25⌉)`, refill `r = uρ/20` a DAA (milli
fixed point); rooted `issuance_buckets`; a slot is held from acceptance to a counted licence, `Final`, a
conviction void or E-4's hold's end (S.5). One reading at the fold, admission and the producer's facts
(`IssuanceCapped` / `AuditBacklogFull`: skipped for the block's own attempt, refused at admission, named
in the producer's verdict and the CLI catalog). State: one Some-only root block `capacity_qs/v1`, one
carriage tail `0xB9`, delta entries 83–85.

## Gate — PASS (`palw_capacity_stage2_q_s.rs`, 10 tests; `palw_capacity_stage2_is_t12_only.rs`)

| requirement | evidence |
|---|---|
| outstanding controlled | 13k at ρ 100, 12 attempts a DAA for 30 DAA: 200 admitted, `outstanding ≤ N_out = 200` at every DAA; a counted licence frees a slot; a void holds its slot to `h_obl` |
| rate controlled | 8 a DAA for 25 DAA, then 0 (the cap) — never above the burst |
| liability kept | J-1's `W_cap` at every DAA; no credited claim paid (no vesting row) before its audit; stage 1's liability/forfeiture unchanged (lane suites restated, all green) |
| detection on chain | a credited claim licensed at DAA 1,004 waits unaudited to 1,405 and is `Final` one DAA after its receipt; `k_aud` 2 from ρ 250; forged/out-of-pool receipts refused/skipped; a conviction of a receipted claim excludes its auditor; the duty's read offers each claim to its pool only |
| determinism | a tape with receipts reversed on a fork: revert-to-base, IBD, restarts — same roots |
| dormant | every t12 pin unmoved; F-Q/F-S `never()` collapse; fork id moves only when a height is set |

## Findings for the user

1. **The bucket never reaches the ADR's rate** (§6.1/§6.2). The fold refills at DAA granularity, so a
   bond spends at most `B` a DAA; `B = ⌈uρ/25⌉ < r = uρ/20` whenever `uρ > 100`. 13k at ρ 100: **8 a DAA,
   not 10** (640 per 80 DAA, not 762); every row from ρ 50 (13k) / ρ 10 (100k, 1M) is 80 % of its `r`.
   DoS-only, no safety effect. Implemented as written; if 10 a DAA is wanted, `B = max(4, ⌈uρ/20⌉)` is the
   one-line change (before the fence is set).
2. **`A_max` = 4,800 is a placeholder** until stage 0's measured operator audit rate (ADR: rate × `W_aud`
   × ½); a change is a new fence value.
3. **Not in stage 2**: the K1′/K6 counters (lane B, stage 6) and A15 (audit-pool outage, a drill). The
   pool is t12's eight genesis operator cards minus the producer and the panel's seats: a claim whose
   producer and panel cover all eight has no pool and waits (K6 would trip).

## Where a merge of rcore/int-5 touches this

Stage 2 appends delta entries 83–85, carriage tail `0xB9` and object tag 60 (stage 4 adds 86–87 and
`0xBA`): if int-5 appends its own, renumber ours at the merge (dormant, nothing stored).

## Batteries (stage-2 head)

| crate / target | result |
|---|---|
| `kaspa-consensus-core` lib + all 201 integration binaries | 3,791 passed, 0 failed, 41 ignored |
| `kaspa-consensus` lib | 538 passed, 0 failed, 21 ignored (re-run: the battery's run aborted in a `should_panic` test's cleanup on RocksDB's "No locks available" with the disk at 97%; 19 GB of stale incremental sessions pruned from the shared target) |
| `kaspad` lib | 428 passed, 0 failed (the audit duty's tests included) |
| `kaspa-rpc-core`, `kaspa-grpc-core`, `kaspa-rpc-service`, `misaka-cli`, `misaka-palw-extension` libs | 249 passed, 0 failed |
| `kaspa-testing-integration` (`cargo check --tests`) | compiles |
| clippy (`--tests`, the eight crates) | 1 finding on this branch's lines (a constant `min` in a unit test, fixed), 154 on the release base |
