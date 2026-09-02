# testnet-11 Relaunch 5 — the LLM-primary economy (ADR-0068 Phase 2)

Branch: `palw-adr0068-phase2`. Target identity: params fingerprint **`d38abe44…`** (devnet stays
**`873a5ae8…`** — it registers no A16 tier), genesis **`08e9c8a4…`**.

> **`f0e50f83…` ran the revert of the frozen target and measured it working** — the floor went
> 77 → 4.7 blocks/min in 20 minutes and reached 95–119 s inter-block gaps by 01:04 CEST. What it
> also measured: the QWEN25-A16 producer refused its own artifact nine times, because the genesis
> card pinned `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT` as the file's flat digest (`c00faa48…`)
> while the graph-v2 row resolves by the operand-inventory root (`1a7457f1…`) — the two-mappings
> defect the resolver's own comment names. Re-pinned; the fingerprint moves, the genesis does not.
> Verify with `misaka-palw-base0/tests/a16_root_probe.rs` (`PALW_A16_PATH=… --ignored`).

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

## Relaunch 5c — the A16 root re-pin, and seat 4 moves off host C (2026-09-02)

Third whole-fleet swap of the day; the genesis does not move. Two host changes ride it:

* **seat 4 (bond `:4`) runs on `.113` from now on, not on C.** C (23 GB RAM) was OOM-killing its
  three seats every few minutes — the heap burst at artifact load is ~9–15 GB per process and
  three of them map the same 33 GiB. Dropping QWEN36 from seats 3/4 instead was ruled out by
  arithmetic: QWEN36-capable seats would be `{0,1,2}` and a claim by node0 draws its panel from
  `{1,2}` — two seats against a quorum of three, so no QWEN36 claim could ever license. `.113`
  has 19 GB free RAM and 267 GB disk. Staged there: `/etc/misaka/t12/t12-bond-4.key` +
  `t12-operator-4.key` (sha-verified against C), `/root/t11/seat4.sh` (C's launcher verbatim —
  same appdir `/root/.t11c`, ports 26331–26333, `--palw-panel`, fee outpoint `:45`), unit
  `misaka-t11-seat4.service` installed but **disabled until the swap**. C's `misaka-t11-seat4`
  is stopped AND disabled at the swap so the bond never runs twice.
* **`.113` gains the artifacts its bond was declared for.** Its node held only
  `qwen25-coder-a16.palwart`, whose root is not the registered A16 root, and no QWEN36 — so bond 6
  could judge floor claims only. `qwen25-1.5b-a16.palwart` (sha `a8c4e53e…`) and
  `qwen36.palwq36` are staged into `/root/palw-class/`; `node.sh` gains
  `--palw-class-artifact=/root/palw-class/qwen25-1.5b-a16.palwart` (`.bak-r5c` beside it).

Start order for 5c: ibm node0 → ibm node1 (restart node1 once node0's P2P is up — its address
manager backs off while node0 spends ~12 min mapping the model) → C seat2, seat3 → `.113` node →
`.113` seat4 (last; it maps 33 GiB too) → explorer filler/REST/MTP/minerpool/slot. Verify each
node announces `d38abe44…` before starting the next host.

### 5c — what actually happened (2026-09-02, CEST)

* 01:29:51 ibm node0 started on `d38abe44…` (`v1.1.0-b4a98a937`, sha `1fcac716…`); P2P opened
  ~01:43 after the 33 GiB map. 01:43:31 node1 restarted to clear its address-manager backoff —
  established 6, producing. 01:43:29 / 01:43:59 C seat2 / seat3 up on `d38abe44…`. .113 node up
  01:43:30.
* **bond 4 handoff**: `qwen36.palwq36` landed on .113 by direct `rsync` from ibm (ibm→.113 ssh
  already works; do not route 34 GB through an operator laptop — 13 MB/s vs 4 MB/s). sha `7a944595…`
  matched ibm's. 01:44:06 `.113 misaka-t11-seat4` active+enabled on `d38abe44…`; C's
  `misaka-t11-seat4` inactive **and disabled**. Bond 4 runs on exactly one host.
* .113 also holds `qwen25-1.5b-a16.palwart` (sha `a8c4e53e…`) and `node.sh` passes it, so bond 6
  can judge the registered A16 class.
* seat2 (A16 producer) logged **0 refusals** after the restart; the old build refused within seconds
  of the producer loop starting.

### 5c verification (six read-only probes, 01:56–02:06 CEST) — NOT yet publishable

* **Identity clean**: all seven kaspad processes (ibm node0/node1, C seat2/seat3, .113 node/seat4/
  pool-slot) announce `d38abe44…` / `08e9c8a4…` / `581466da…`, binary `1fcac716…`; `f0e50f83…` on none.
* **Claims license, nothing voids**: `voided`/`BindTimeout`/`InsufficientEligibleBonds` = 0 on every
  log; ReceiptLicensed 53 → 84 in five minutes; `final_claims=0` is expected (≥ 2400 DAA ≈ 80 h).
* **Cadence converging**: 96 → 5 blocks/min in 22 min, difficulty +67 % in 4 min, same shape as the
  revert build. Not at target inside the window; re-sample at ~02:30.
* **QWEN36**: first block 02:04:17, 41 min after its producer started (33 GiB paged from disk).
* **A16 — the point of this deploy — VERIFIED at 02:19:58**, after a detour: seat2 logged **zero**
  refusals on the re-pinned root (the old build refused within seconds), but its first instance was
  **OOM-killed at 02:00:27** (anon-rss 13.8 GB, C's 4 GB swap 100 % full) while mapping the 33 GiB
  Qwen3.6 artifact a SECOND time — once for the producer role, once for the panel role. The restart
  came up in a persisted candidate-review ("an IBD left it on a chain it could not vouch for"),
  finished IBD at 02:13:09, printed no floor ticks at all, and then resolved on its own: bond 2's
  producer started 02:13:05 and **produced block #1 `f6d1f72b…` at 02:19:58**, which ibm node0,
  ibm node1 and the .113 public node all **accepted via relay at 02:21:06**. The genesis card's A16
  row now names the root the court-capable graph-v2 registration computes; the two-mappings defect
  (`c00faa48…`, ADR-0071 §5 record) is closed on the live chain.
* Host C still cannot hold seat2 + seat3 (~14 GB + ~11 GB peaks on 24 GB); .113 has no margin
  (22.9 / 24.6 GB, node 10.3 + seat4 11.5 GB, no swap).

**Action taken (host-only, reversible, consensus-neutral)**: 16 GB swapfiles `/swapfile-r5c` added
to C (4 → 20 GB) and .113 (0 → 16 GB), `vm.swappiness=20`, persisted in fstab. Zero OOM kills
since. **Code follow-ups filed**: (1) a process holding both roles maps and hashes the same 33 GiB
artifact twice — share one mapping; (2) candidate-review nodes drop fleet peers with
`HandleRelayInvsFlow: expected Payload::Block but got IbdCandidateSummary` (seat2 ×7, seat3 ×15,
pool-slot ×11; zero on Ready nodes).

### Public surface at 02:24 CEST — what a newcomer meets

* **Seeders**: `seeder1`/`seeder3.misakascan.com` answer with `169.58.232.113` and `169.58.39.220`,
  the two nodes the seeder verifies on this fingerprint. `seeder2`/`seeder4` stay in the shipped
  list and stay dead. `5.104.81.23` (host C) does not accept inbound P2P and was removed from the
  join page's fallback list.
* **Explorer / API / wallet**: `misakascan.com/info/blockdag` reports `misaka-testnet-11` on the new
  chain; `wallet.misakascan.com` answers. The explorer DB begins at blue score 498 (01:53:41): the
  filler has no start-hash knob and starts from the tip once its node reports synced, so the first
  497 blocks of the 01:43–01:53 burst are not indexed. Cosmetic; a backfill needs a filler change.
* **Newcomer pages** `testnet11-node-operator.md` and `testnet11-join-mining.md` now carry
  `d38abe44…` / `08e9c8a4…`, the three live class ids, and every archived fingerprint a stale node
  might still announce (`61af5296`, on `main`).
* **Cadence 02:27 → 03:00**: floor #655 → #666 in six minutes at 02:27 (gaps 33/34/38/13/88/33/8/16 s),
  then per five minutes 7, 6, 4, 4, 7, 4 blocks (02:34–02:59) = 64/hour against the 30/hour
  target, last twelve gaps 39/124/68/45/49/8/39/73/31/39/146/75 s. Still converging 76 min after
  the empty-chain start; the first ten minutes put ~500 blocks into the difficulty window and the
  rate settles as they age out. QWEN36: two blocks (02:04:17, ~02:42). No condition here blocks
  publication; the number to re-check later is the hourly floor count.

### Relaunch 5d — announced, NOT deployed (fingerprint received 2026-09-02 ~03:10 CEST)

The 5c chain runs untouched until the user calls the 5d flag day. The peer session building
ADR-0072+D8 / ADR-0073 ① / ADR-0074 ③ reported, from the pin test on its rebased tree
(`origin/main 4c98717a` + branch `palw-adr0073-fp-weight` at `9d038706`, 8 commits, **not
pushed** — merge and push are the user's decision):

| preset | 5d fingerprint (their tree, "final for this tree") |
|---|---|
| testnet-11 | `f6afe2e237604b83a0cd03fe8b94be428e4aceabc12d7834b45bb455c664154a` |
| devnet | `fd041c409b6fda1d70df60d7b9c0827349d83f74e7c0e45ef4ee5d62943bbf09` |

What moves the value: free-prompt wire version 3 → 4 (job `prompt_mode`, commitment
`work_leaves`), the bundle's free-prompt params (a quantum is ⅛ of the class's canonical job; CU
weights gone), the certified free-prompt set (floor only), state version 15. Classes, artifact
roots, the A16 inventory-root pin and the genesis hash (`08e9c8a4…`) are unchanged — so 5d is a
fingerprint move on the same genesis, which the handshake still treats as a different network:
every node wipes, exactly as 5c.

**Pre-check done 03:20 CEST**: this session re-derived the value from the peer's tip `9d038706` in
an isolated detached worktree with its own target dir — `shipped_presets_have_pinned_fingerprints`
ok, pinned testnet-11 `f6afe2e2…` / devnet `fd041c40…`, the A16 inventory-root pin present and
`a16_root_probe` green. Step (2) below is therefore already satisfied *if* the pushed commit is
`9d038706`; any other commit is re-derived again.

**5d go/no-go gate** (all three, in order): (1) the user merges/pushes the tree and says when;
(2) this session re-derives the fingerprint from the pushed commit in an isolated worktree and it
equals the value above; (3) the stop-ALL → archive → rotate the four per-genesis stores → install
→ per-host start order below is run as one window. Nobody "restarts everything".

### Four stores are per-genesis and must be rotated at every regenesis — one was missed today

| store | where | rotation |
|---|---|---|
| explorer DB | postgres `kaspa_t11` on .113 | `ALTER DATABASE kaspa_t11 RENAME TO kaspa_t11_old_<fp>_<date>; CREATE DATABASE kaspa_t11 OWNER kaspa;` then start the filler |
| MTP ledger | `/var/lib/misaka-mtp/data` on .113 | `mv` to `data.old-<fp>-<date>` |
| miner-pool slot | `/var/lib/misaka-minerpool/slots/slot-01/appdir` on .113 | `mv` like any appdir |
| **faucet grant ledger** | `/var/lib/misaka-faucet/granted.jsonl` on ibm | `mv` to `granted.jsonl.old-<fp>-<date>`, `systemctl restart misaka-faucet` |

The faucet one is **not genesis-keyed**: `/opt/misaka-faucet.py` appends `{address,…}` rows and
loads them all into `PAID` at start, so an address that claimed on any earlier chain is refused on
every later one until the file is rotated. Measured 2026-09-02: 10 rows, last written Sep 1 11:59
— all from the `8d2002cc` era. **Not rotated today** (it changes who can claim on the live
network; operator's call), so those ten addresses are currently refused on 5c too.

### Disk, under the operator's "unused for two weeks may be deleted" rule (2026-09-02)

* **ibm was at 97 %** — `/root/palw-class` alone is ~209 GB. Removed: the datadir archives of the
  two chains that no longer exist anywhere (`*.old-8d2002cc-*`, `*.old-accaadce-*`, 32.5 GB),
  `/root/x-tools` (March), `palw-unified-476d73b.tar.gz` (Aug 16), and `qwen36-run2.palwq36`
  (34.8 GB, **sha-identical** to the referenced `qwen36.palwq36` — a duplicate, not data). Kept:
  `.rustup`/`.cargo` (builds need them), the `*.old-f0e50f83-*` archives (most recent wiped chain),
  and — because they are under two weeks old — `huihui-30b.palwq36` 30.6 GB, the abliterated Q4
  GGUF 22.8 GB, `huihui-q4km.gguf` 17.7 GB, `probe-1layer.palwq36` 1.8 GB (none referenced by any
  launcher on this genesis; next candidates). 97 % → 75 %.
* **C**: removed six >14-day directories referenced by no unit (`misaka-cand-00d1294`,
  `misaka-regression`, `palw-drill-build`, `valbuild`, `/opt/misakas`, `palw-drill`; 10.5 GB).
  **Not removed**: `/var/lib/misaka` (56 GB, mtime July) — it is testnet-10's datadir and contains
  `validator/`; `kaspad-tn` is inactive but `misaka-validator-c` / `misaka-miner-c` still run.
  An operator decision, not a cleanup.
* .113: 14 %, nothing eligible.

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
