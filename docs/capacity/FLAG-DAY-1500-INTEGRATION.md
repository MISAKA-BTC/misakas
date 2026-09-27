# The DAA-1,500 flag day: what the integrator needs from rcore/cap-s1

The capacity lane's part of the 1,500 flag day is `rcore/cap-s1` at `03ae8a31b`: stages 1, 2 and 4 plus
stage 3's fixed ρ = 10. It is all DORMANT at that commit, and testnet-12's pins are int-5's
(`24e1aec3…` / `5de80e64…` / `d263d7f2…`). Stages 5–7 (riders, the breaker, ρ 1000) are NOT on the
branch and do not ride 1,500.

## 1. The entry list, in this order (`consensus/core/src/config/params.rs`)

```rust
PALW_T12_CAPACITY_WEIGHT_CAP_V1,                 // palw_capacity_weight_cap        F-W
PALW_T12_CAPACITY_ESCROW_AT_LICENCE_V1,          // palw_capacity_escrow_at_licence F-E
PALW_T12_CAPACITY_AGGREGATE_LIABILITY_RHO10_V1,  // palw_capacity_aggregate_liability F-L: one step (H, ρ 10, q 250‰)
PALW_T12_CAPACITY_BATCH_LICENCE_V1,              // palw_capacity_batch_licence     F-B
PALW_T12_CAPACITY_VERIFY_ROOM_V1,                // palw_capacity_verify_room       F-R
PALW_T12_CAPACITY_AUDIT_DOOR_V1,                 // palw_capacity_audit_door        F-Q
PALW_T12_CAPACITY_ISSUANCE_SLOTS_V1,             // palw_capacity_issuance_slots    F-S
PALW_T12_CAPACITY_NETWORK_ROOM_V1,               // palw_capacity_network_room      F-N
```

`PALW_T12_CAPACITY_RHO10_FENCES_V1` is the same eight entries as a slice. **Do not use
`PALW_T12_CAPACITY_FENCES_V1` here**: that is the stage-1 proof list, with F-L at ρ = 1.

* **All eight arm at one height.** `validate_palw_v2` refuses a list that is missing any one of them,
  with one exception: F-N. Nothing else depends on F-N, so the validator will not catch it missing. The
  list must still carry it, because F-N is the stage-4 fair-share property.
* **Arm them on the ASSEMBLED ruleset**, in the same place as the 750 and 1,300 lists and after them,
  before `validate_palw_v2`. `sync_palw_capacity_stage2` reads lane A's operator bonds (F-Q's pool) and
  the bundle's panel anchor delay. The prerequisites F-W needs (strict-win and lane A at or below it)
  are the 750 list's.
* A drill moves the entries through their own `set`. Moving the ρ = 10 F-L entry keeps any later
  appended step.

## 2. Tests the flag-day build must flip or re-pin

* `consensus/core/tests/palw_capacity_stage3_rho10.rs::testnet12_ships_the_package_dormant` asserts that
  testnet-12 as shipped arms no capacity entry. Flip it, or delete it.
* These `*_is_t12_only.rs` files each pin `T12_RELEASE` and assert their fence is `None` on testnet-12
  as shipped: `weight_cap`, `escrow_at_licence`, `aggregate_liability`, `batch_licence`, `verify_room`,
  `stage2` (F-Q + F-S) and `network_room`. The triples are registered in `scripts/t12_repin.py` (CAP*),
  so `t12-repin.sh` handles the pins. The "`None` on testnet-12" asserts have to be edited by hand.
* **Fixtures that treat `palw_t12_shipped_params()` as the unarmed baseline.** Examples are
  `capacity_stage1_common.rs::params_for(class, false)`, the stage-1 state diff's shipped twin, and the
  release twin in the stage-3 crossing test. Once 1,500 is armed these baselines are armed from 1,500.
  Scenarios that fold past DAA 1,500 will then see the package. The simplest fix is for `params_for` to
  set every entry of `PALW_T12_CAPACITY_RHO10_FENCES_V1` to `None` first. The stage-3 test's `release()`
  already does this.
* Any other test that runs the shipped testnet-12 ruleset past DAA 1,500 changes behaviour too. In
  particular, credited floor claims wait for an audit receipt before they can go `Final`.

## 3. Live consequence to watch at the crossing

From DAA 1,500, a floor attempt claim goes `Final` only after `k_aud = 1` pool member has posted an
`AuditReceiptBatchV1` (tag 60) whose root matches the claim's. The pool is the operator cards that
neither produced the claim nor sit on its panel. On t12 that is 3 of the 8 cards, or 2 when the producer is itself a card. The audit duty lives
in `kaspad/src/palw_audit_duty.rs` and runs automatically on operator nodes, so **the fleet's operator
nodes must run this build**. If no receipt lands, credited claims wait: they are not voided or charged,
but they are not paid either.

Check on the drill:

* receipts appear on chain;
* the audit backlog (`palw_capacity_audit_backlog_v1`) drains;
* floor claims go `Final` a few DAA after their licence.

8k and 2M claims are never credited (D-18), so they are unaffected by the door.

## 4. `s_t5_a8`: a flaky test, not a regression

* **Test:** crate `kaspa-consensus` (`--lib`), in
  `consensus/src/pipeline/virtual_processor/tests/t12_capacity_shadow.rs`, named
  `pipeline::virtual_processor::tests::t12_capacity_shadow::s_t5_a8_the_alarm_fires_when_the_auditors_stop_and_clears_when_they_run`.
  The assertion that fails is at line 218: `!…bonds[0]…frozen_would_be`, i.e. card 0 is not frozen.
* **What it depends on:** the test drives the real node pipeline. Card 0 ends up frozen when it is
  charged as a covering signer of claim B, and whether it is charged depends on which cards the
  pipeline draws as B's panel seats. That draw changes from run to run (block timing and hashes).
* **Evidence it is flaky:** it failed in the stage-4 battery and in stage 3's. Running the same
  binary alone three times gave fail, pass, pass.
* **Why stage 3 cannot cause it:** the test runs the shipped ruleset with every capacity fence dormant,
  so the stage-3 gain scale is the identity there.
* **For the integrator:** re-run it alone. A fix would pin the assertion to the drawn covering signers
  rather than to card 0.
