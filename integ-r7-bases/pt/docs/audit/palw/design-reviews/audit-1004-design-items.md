# Audit 2026-10-04: design items P-F2, C-F3, B-4 and the EVM boundary notes (lane PA)

Findings are `lanes/evidence/palw-fatal-audit-1004/FINDINGS.md`; the consensus rules that ARE implemented sit behind `palw_audit_1004_v1`
(`consensus/core/src/palw_audit_1004_v1.rs`). The items below are proposals, not implemented, because each needs more than a refusal or a
skipped write (a new object form, a payout-schedule change, or a relay policy).

## P-F2 (High): a seat licensed by vertex tally cannot be convicted of a false `Valid`

Today `palw_check_panel_false_valid_v2` (`palw_offence_attribution_v1.rs`) accepts evidence only as a signed `Full`/`Segmented`/`Windowed`
receipt. A `Valid` counted from a verification vertex (RFC-0007) is an unsigned receipt (`palw_vertex_receipts_of_v1` writes
`signature: Vec::new()`), so the seat is licensed by a vertex leaf and no accuser can produce the receipt the check wants.

**Proposed rule: a `Vertex` evidence form** in `PalwPanelFalseValidEvidenceV2` (new `receipt` variant, version 3, behind the same fence as the
rest of the offence funnel): `{ header: PalwVertexHeaderV1 (the signed header: seat, round, signed_daa, leaf_count, leaves_root, signature),
leaf_index, leaves_path: Merkle path from the `Verdict` leaf to `leaves_root`, leaf: PalwVertexLeafV1::Verdict }`. The check: (1) the
header's ML-DSA-87 signature verifies under the accused seat's registered key through `palw_vertex_verify_signature_v1` (the same message
the acceptance walk verified, `palw_vertex_message_v1`); (2) the leaf opens under `leaves_root` by `palw_vertex_root_of_leaves_v1`'s tree;
(3) the leaf names the accused claim (full id or the compact form against the claim's bind DAA) with verdict `Valid`; (4) the claim's tally
actually counted that seat (`vertex_tally_of_v1` or, once licensed, the seat's lock `slashable_lock(seat, claim)` exists) so the leaf is the
one that licensed it, not an ignored one; (5) the rest of the false-valid check unchanged (contradiction proof, liability by `Site`, open
court). Evidence size is bounded by `PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES` (the path is log2(leaves) hashes plus one 4.6 KB signature). The
reporter reveal and reward routes are shared. It needs the lane that owns the offence funnel to add the variant to the evidence enum, the
SDK builder and the explorer's decoder; it is a new carriage shape, so it takes a state-free object-form number (COMMON.md: next free is
not allocated here).

## C-F3 (Medium, plausible): the payout drain is 8 a block regardless of rho

`PALW_V2_MAX_PAYOUTS_PER_BLOCK = 8` and `PALW_V2_VESTING_LEGS_PER_BLOCK` are constants while the capacity stage multiplies the number of
claims by rho (up to x1000), so the pending payout queue's inflow rises with rho and its drain does not.

**Proposed rule**: (a) aggregate by payee: one coinbase output per payee per block, summing every pending leg for that payee (the queue is
already keyed by payload, `palw_panel_payout_key_v1`), which bounds the output count by the number of DISTINCT payees, not legs; and (b)
scale the per-block cap with the capacity stage's tier, `cap(rho) = 8 * min(rho, 64) / ...` bounded by block mass, with the coinbase
extra-output allowance (`PALW_V2_COINBASE_EXTRA_OUTPUTS`) derived from the same function so mass and validation agree. Both change the
coinbase layout, hence the coinbase-mass and storage-mass limits, so they need their own fence and a drill that fills the queue at rho 25 and
rho 100; neither is a refusal. Until then the breaker's backlog signal (`ReceiptVoid`/audit-overdue) is what lowers rho when the queue lags.

## B-4 (Low/Medium): attempt-lane headers pass the header stage on a signature alone

Design note only: an attempt-lane header is admitted at the header stage by its ML-DSA signature (a single lottery); a flood of
well-signed, never-winning headers costs each relaying node a verify. Proposal: relay-side limits per peer and per bond key per window
(node policy, no consensus change), and charging the verify to the peer's flow budget in the same place the material lane's
`reserve_serve_budget` is. No rule change.

## EVM boundary (`evm-boundary-audit-1004/REPORT.md`)

* **EVM-01** (executed-then-skipped transactions pay no gas, no duplicate-tx rule): not changed by lane PA. The fix touches the EVM
  execution and the block's gas accounting (a skipped transaction must still pay for the work done to decide to skip it, or be refused
  before execution) and is not dormant-fenceable without moving the EVM state transition; it needs its own fence in the EVM lane.
* **EVM-05** (ML-DSA verify cap only against the base 30M gas): not changed; a cap by verifies per block belongs beside the round-budget
  accounting.
