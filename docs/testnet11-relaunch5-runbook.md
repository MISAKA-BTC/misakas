# testnet-11 Relaunch 5 — the LLM-primary economy (ADR-0068 Phase 2)

Branch: `palw-adr0068-phase2`. Target identity: params fingerprint **`f0e50f83…`** (devnet moves
to **`873a5ae8…`** through the shared builder), genesis **`08e9c8a4…`**.

> **`accaadce…` was deployed and taken back the same evening.** It froze the attempt lane's Layer-0
> target (ADR-0071 Decision 1) and the live network measured what that costs: the floor produced
> ~50 blocks a minute against a 0.5/min target, flat, because block interval is
> `calculate_difficulty_bits`'s job and the per-class retarget is a deliberate no-op on a
> single-class network. Claims voided every block and the public entry node could never leave
> `CandidateReview`. The freeze is reverted; the genesis is unchanged.
**This is a re-genesis: every host wipes.**

> **Verified 2026-09-02 by running the release binary, not by reading this file.**
> `./target/release/kaspad --testnet --netsuffix=11 --palw-dump-classes` announces:
> ```
> PALW court certified end-to-end for: PALW-BASE-0, PALW-QWEN36, PALW-QWEN25-A16
>   (court_e2e_root 581466da…)
> Consensus params fingerprint: accaadce562c120da9d7dd972c46903dfa59a607d50f335af55e2c3bccfdfeb2 (network testnet-11)
> ```
> and `PALW_RC_GENESIS.hash` is `08e9c8a4cb59714574bc76e25e4dc16bb24e213fc2f0f6c8c6fd5d8c4a25ef70d…`.
> The **live** fleet answers on genesis `8d2002cc…` (Relaunch 4), so the two do not meet: this is a
> wipe, not a rolling upgrade, and a half-deployed fleet is two networks wearing one name.

> **The fingerprint has now moved SEVEN times since this document was first written**, and the
> value above is the only one to deploy against. `d7510c7a…` was the audit-free branch; the launch
> audit's remediation took it to `b1aad428…`; the model tiers' step-space work to `40d76c2c…`;
> registering the corrected class rows with cadence to `272ed6a8…` — **which is the number this
> document told you to deploy against until 2026-09-02, and by then it was three moves stale.**
> The 120 s cadence set took devnet to `84f15819…`; ADR-0071 Decision 1 took both presets to
> `ce79c069…`/`771371ea…`; Decision 2 to `cc65f3b4…`/`8c92f2a3…`; Decision 3 to the pair above.
> A fingerprint is not the sum of its diffs, which is exactly why it is computed once at the end
> and read off `consensus_params_id()` rather than carried forward by hand. **Verify what a node
> ANNOUNCES, not what this page says** — this page has been wrong about it before.

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


## Readiness, measured 2026-09-02 — GREEN on code, blocked on five operational facts

The implementation is sound: every suite green, the binary certifies all three families end to end,
and it announces the identity above. What is not ready is the launch, and each item below was
measured rather than assumed.

1. **This wipes a live, producing network with third-party participants.** `.113`'s node accepted
   37 blocks in the half hour before this was written, and its log shows outside peers
   (`13.140.185.225`, `111.67.115.228`) failing handshake with genesis values of their own — people
   are running builds against t11 today. A re-genesis strands every one of them until they upgrade,
   and there is no channel from here that reaches them.
2. **Two of the four shipped DNS seeder names do not resolve at all.** `seeder2.misakascan.com` and
   `seeder4.misakascan.com` return NXDOMAIN on every discovery round; only `seeder1` and `seeder3`
   answer (both → `169.58.39.220`, `169.58.232.113`). Those two are also the only ones this fleet
   can reconfigure — the other pair answers from hosts it does not administer. Shipping four names
   of which two are dead is a discovery configuration decision, not a code fix; `dns_seeders` is
   deliberately outside `consensus_params_id`, so removing them is a plain edit and not a flag day.
3. ~~**`169.58.39.220` is in the seeder answer set and in no inventory here.**~~ **RESOLVED
   2026-09-02: it is `misaka-ibm` itself** — `hostname` returns `vmi3450148`, the same host the
   node0 journal is written by. It was an unknown only because the inventory recorded the ssh alias
   and the seeder records the address. `169.58.232.114` is likewise not a participant: it answers
   ssh as `vmi3527649`, runs no misaka unit and has 26311 closed. The only non-fleet peer touching
   t11 is `111.67.115.228`, which handshake-fails on genesis and is a third party.
4. **Twelve commits are unmerged to `main`.** Step 2 below is the flag-day landing and has not
   happened; until it does, `main` builds Relaunch-4 binaries.
5. **The key custody findings from the fleet preflight are unresolved.** Seats 2,3,4,5 — half the
   registry — hold their bond AND operator keys on host C alone, which is the same single-host
   exposure that permanently lost the original seat 5; host C also runs `MemoryMax=infinity` on
   its seats. Seat 7 has no key on any host (within ADR-0065's "at most two unmanned", together
   with seat 6 whose key is on `.113`).

Seat key inventory as measured: ibm `0,1` · C `2,3,4,5` · `.113` `6` · seat `7` unmanned.

### Two producer-class ids were pointing at classes this chain does not register

Found while staging the relaunch, corrected on the hosts (`.bak-relaunch5` beside each launcher):

| host | launcher | was | now | what it is |
|---|---|---|---|---|
| ibm | `ibm-node0.sh` | `ec7bbcbf…` | `5bd9ae3d…` | QWEN36 — `ec7bbcbf…` is `qwen36_class_id_v1`, the graph this build's backend REFUSES |
| C | `c-seat2.sh` | `f942e268…` | `71bbb755…` | QWEN25-A16 |

Read off the binary, which is the only authority: the registered set for this genesis is
BASE-0 `f1c5635c…` (root `bcf2d9eb…`), QWEN36 `5bd9ae3d…` (root `f4aad4fd…`, declared 957‰ so it
lands at 489‰ after the dense tier's dilution), QWEN25-A16 `71bbb755…` (root `c00faa48…`, 489‰).

> An earlier revision of this page printed `71bbb755…` for A16 and a later one struck it out as
> unverifiable. **The value was right and the objection was still right**: no literal pins it, so
> the page had no way to be sure, and a number that happens to be correct is not the same as a
> number you can check. It is stated here only because it was read off this build.

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
4. **Producers/miners**: node0 `--palw-producer-class=5bd9ae3d…` (QWEN36 — the ONLY id this
   document is allowed to state, because it is the only one pinned as a literal in source:
   `palw_qwen36_profile.rs:1134`). seat2 takes the QWEN25-A16 id, node1 is the floor, seat3 the
   second miner.

   > **Read the A16 id off the binary, not off this page.** An earlier revision of this document
   > printed `71bbb755…` for it. That value appears nowhere in the tree — it was transcribed from a
   > measurement someone else had run, which is precisely the move the paragraph below tells you not
   > to make, made by the person writing the paragraph. The registration builder derives the id; no
   > literal pins it; so the authority is the running binary:
   > ```
   > ./kaspad --testnet --netsuffix=11 --palw-dump-classes
   > ```
   > Take the dense tier's id from that output on the SAME binary you are about to deploy, and put
   > it in seat2's flag. If the dump and this page ever disagree, the page is wrong.
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
