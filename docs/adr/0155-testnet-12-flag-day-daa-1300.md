# ADR-0155 — testnet-12's second post-launch flag day: two fences at DAA 1,300

* Status: **Accepted, armed at DAA 1,300.** Release `587cab2b0`, params fingerprint `24e1aec3…`. The
  public fleet was upgraded one node at a time on 2026-09-27 between 19:10 and 19:37 JST.
* Date: 2026-09-27
* Decided by: the operator (2026-09-27, with the live chain at DAA ~853)
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-10's refusal and SW-8's
  anchor-block void; L-3) and [ADR-0154](0154-testnet-12-flag-day-daa-750.md) D9.

## Context

Between DAA ~624 and 749 the seats ran out of room. SW-10's eligible-stake floor (875 ‰) refused the
panel draw, and claims voided at their anchor slot: 169–172 claims, and about 541,000–551,000 MSK of
escrowed worker reward burned, with no collateral taken. The DAA-750 fences restored the room. But
locks dated before 750 still expired at licence + 3,000. At DAA 816 each genesis seat held 959–1,583 of
them, so the room would close again at about DAA 1,350–1,575.

## Decision

`PALW_T12_POST_LAUNCH_FENCES_V2`, armed at `PALW_T12_POST_LAUNCH_FENCE_V2_DAA` = 1,300. No other fence
uses that height. The next flag day's fixes are added to this list.

- **D1 — `palw_floor_refusal_retry`.** A claim whose stake draw is refused for eligibility at every
  seed on the anchor block's pre-object base re-anchors at its next slot instead of voiding. The
  qualifying refusals are `InsufficientEligibleStake`, `InsufficientEligibleBonds` and `NoOutsider`.
  The existing deadlines still apply. → spec 07 §7.4 (PALW-LC-13).
- **D2 — `palw_final_lock_life_retro`.** At the crossing block, every seat lock of an honest `Final`
  claim is re-dated to `min(expiry, max(F + 1,000, H))`. After it, a `Final` dates its seat locks at
  exactly F + 1,000. The producer's `E` recovery (F + 3,000), the court's liability records and locks
  of claims with DA history are untouched. Its prerequisite is `palw_final_lock_life` (DAA 750).
  → spec 10 §10.3 (PALW-CO-18).

## Consequences

- From DAA 1,300, nodes on `c3dbaee3c` or older are refused at the handshake. The fence schedule
  becomes `750, 1000, 1300`, with schedule id `d263d7f2…`.
- The drill crossed both fences at DAA 190 on a salted chain. All 865 locks that were Final before the
  fence matched `min(old expiry, max(F + 1,000, 190))`, and there were no voids after the fence.
- The next flag day (DAA 1,500) gets its own ADR.

## Links

- Spec: [16 network parameters and fences](../spec/palw/16-network-parameters-and-fences.md) §16.4 ·
  [07](../spec/palw/07-claim-lifecycle.md) §7.4 · [10](../spec/palw/10-collateral-and-economics.md) §10.3
- Design: [design/palw/lineage.md](../design/palw/lineage.md) (flag days), [collateral.md](../design/palw/collateral.md) §6
- Records: [launch note §000](../t12-launch-2026-09-25.md)
- Code: `consensus/core/src/config/params.rs` (`PALW_T12_POST_LAUNCH_FENCES_V2`,
  `PALW_T12_POST_LAUNCH_FENCE_V2_DAA`, `palw_t12_arm_post_launch_fences_v2`)
