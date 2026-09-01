# testnet-11 Relaunch 5 — the LLM-primary economy (ADR-0068 Phase 2)

Branch: `palw-adr0068-phase2`. Target identity: params fingerprint `c096a627…` (devnet moves
to `3f13411b…` through the shared builder). **This is a re-genesis: every host wipes.**

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
4. **Producers/miners**: node0 `--palw-producer-class=ec7b…` (QWEN36), seat2 `--palw-produce
   f942…` (QWEN25-A16), node1 floor + `--palw-heartbeat-miner-address`, seat3 the second
   miner. Same scripts as today — they survive the wipe.
5. **Explorer**: truncate postgres `kaspa_t11` (6 tables) + filler restart; publish the new
   fingerprint on the misakascan banner; PANEL_SEATS roster in app.js gets seat 5's new
   host label.
6. **Verify day-one**: floor budget ≈ 20/epoch — a slow floor is CORRECT now; the model
   classes hold ~980‰ of cadence. Heartbeats appear only after ≥1 h of bonded silence.
   Watch the coinbase-drought pattern (faucet + floor producer keep the first epochs moving).

## What deliberately does NOT change

- Genesis-fixed policy stays: this is a Relaunch (t11's established mechanism), not a
  mainnet precedent — mainnet ships these values from its FIRST genesis.
- The 10B premine cap, the bond registry size, all lifecycle windows, the class artifacts.
