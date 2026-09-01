# testnet-11 Relaunch 5 — the LLM-primary economy (ADR-0068 Phase 2)

Branch: `palw-adr0068-phase2`. Target identity: params fingerprint **`272ed6a8…`** (devnet moves
to **`67785790…`** through the shared builder). **This is a re-genesis: every host wipes.**

> **The fingerprint moved three times after this document was first written**, and the value
> above is the only one to deploy against. `d7510c7a…` was the audit-free branch; the launch
> audit's remediation took it to `b1aad428…`; merging the model tiers' step-space work took it to
> `40d76c2c…`; and registering the corrected class rows with cadence took it here. A fingerprint
> is not the sum of its diffs, which is exactly why it is computed once at the end and read off
> `consensus_params_id()` rather than carried forward by hand. Verify what a node ANNOUNCES, not
> what a commit message said.

## What this train carries (all landed on the branch, all suites green)

1. **Floor reserve 500‰ → 20‰** (`BASE_CLASS_RESERVE_PERMILLE`, the one field every floor
   guard converged on). With the clock armed, the half-table reserve defended nothing the
   census + heartbeat don't; 20‰ keeps the entry ramp, the KAT class, and the census seed.
2. **Genesis shares 489/489** (QWEN36 / QWEN25-A16): the assembly's largest-remainder
   arithmetic lands the floor exactly at its reserve — the table the goal names
   ({floor ≈2%, LLM ≈98%} of blocks AND issuance, since budgets are blocks and the subsidy
   carve is class-blind).
3. **Fences armed from genesis** (`ForkActivation::always()` for `palw_heartbeat` — with the
   chain-exempt width rule — and `palw_attempt_work`). A fresh chain's first block may be a
   heartbeat; the drill's zero-bond devnet bootstrapped exactly that way. Mainnet's preset
   arms identically the day its genesis card is pinned.
4. **F4 fixed in the shared assembly**: pruning depth-consistency nudge (remainder k+1) for
   every V2 network — t11 leaves `6600 = 11 × 600`.

## Sequencing (do not reorder)

0. **Precondition**: ADR-0068 Phase 1 (main `12e3aa2d`) deployed and verified on the current
   chain — the train departs from a healthy Phase-1 fleet.
1. **Genesis authoring** (operator, on the authoring host):
   - Re-key **seat 5**: 160.16 is permanently lost (provider console gone), so bond :5 gets a
     fresh ML-DSA-87 keypair generated on a LIVE host (ibm or C) — `PALW_RC_GENESIS_BONDS`
     row 5's `bond_pubkey`/`operator_pubkey` swap. Eight seats stay eight (ADR-0065's
     seat_count+3 derivation untouched); unmanned count returns to ≤2 (seats 6,7).
   - Carry the community allocations exactly as Relaunch 4 did (premine tooling; the genesis
     tool's hash DISPLAY lies — verify by booting, per the t12 launch-traps note).
   - Update `PALW_RC_GENESIS_*` roots only if artifacts changed (they have not).
2. **Merge `palw-adr0068-phase2` → main and push** (the flag-day landing; until this moment
   main keeps building current-t11 binaries).
3. **Fleet wipe — stop EVERY host first, then wipe, then start** (the 2026-08 lesson: a
   surviving peer re-feeds the old chain by IBD):
   stop ibm node0/node1, C seats 2-4, .113 node + pool slots → archive datadirs
   (`mv .t11 .t11.old-8d2002cc-<date>`) → install the new binary everywhere → start node1
   (floor producer + heartbeat miner) first, node0 (QWEN36 producer), C seats serialized,
   .113 last.
4. **Producers/miners**: node0 `--palw-producer-class=5bd9ae3d…` (QWEN36), seat2
   `--palw-producer-class=71bbb755…` (QWEN25-A16), node1 the floor, seat3 the second miner.
   Same scripts as today — they survive the wipe.

   > **The class ids changed and the old ones will not dispatch.** `ec7b…` and `f942…` were the
   > `_v1` rows: this branch registers the CORRECTED graphs, because the originals were not
   > classes this build can prosecute (the hybrid's `profile_v1` fails `from_registered_profile`
   > on an unservable RouterTopk operand; the dense `_v1` declares a one-byte state map that
   > cannot describe an i32 cache, so `supports_court()` is false). The hybrid's corrected row is
   > **`graph-v3`**, not v2 — `graph-v2`'s spelling reached t11 first and a registered name cannot
   > be re-pointed — and registering `qwen36_profile_v2` instead would clear every other check and
   > mint a third id no node dispatches. `the_registered_model_classes_are_the_ones_this_build_serves_and_certified`
   > is the test that holds all three spellings (registration builder, SDK lineage, drill) to one
   > value; read the ids off it rather than from here if this document ages again.

5. **Heartbeat miners on EVERY host**, not one. `--palw-heartbeat-miner-address=<addr>` starts the
   service; without the flag a node has no clock at all (`daemon.rs`, `(None, _) => None`), and the
   runbook used to staff exactly one. That makes the lifeboat a single point of failure precisely
   when it is needed. Arming all of them is free and safe: `mine_one` sleeps until its slot opens
   rather than grinding, and while the bonded lane produces at 120 s the one-hour slot never opens;
   sibling floods are chunked at parent selection, so extra miners cost nothing but a warm process.
6. **Explorer**: truncate postgres `kaspa_t11` (6 tables) + filler restart; publish the new
   fingerprint on the misakascan banner; PANEL_SEATS roster in app.js gets seat 5's new
   host label.
7. **Verify day-one**: floor budget ≈ 20/epoch — a slow floor is CORRECT now; the model
   classes hold ~980‰ of cadence. Heartbeats appear only after ≥1 h of bonded silence.
   Watch the coinbase-drought pattern (faucet + floor producer keep the first epochs moving).

## What deliberately does NOT change

- Genesis-fixed policy stays: this is a Relaunch (t11's established mechanism), not a
  mainnet precedent — mainnet ships these values from its FIRST genesis.
- The 10B premine cap, the bond registry size, all lifecycle windows, the class artifacts.
