# testnet-10 update — 2026-08-10, candidate `00d1294`

The operator directed the update of the three testnet-10 hosts on 2026-08-10, while the
RC7 regression series was still in flight on the same machines. This file records what the
network looked like immediately before the update, what was installed, and how to go back —
so that "the network moved" can always be told apart from "the update moved it".

## Why this is a flag day, not a rolling upgrade

The candidate carries `consensus_params_id` in the P2P version handshake and fails closed:
a peer that does not state its rules (every pre-candidate build) is refused at handshake
(`WrongGenesis`, since such a peer sends the genesis field empty too). There is no fence,
flag, or grace period. Consequently there is no configuration in which a candidate node and
a current-release node stay peered — the fleet moves together or splits. The soak plan's
in-band `base` role is unsatisfiable on this network; the baseline is this document.

## The network immediately before (recorded 2026-08-10 00:50–00:52 UTC)

Two nodes served p2p. Neither advanced its virtual DAA between two samples 60 s apart —
the network was already halted (no miner running), still split from the 2026-08 partition.
A third historical peer, `217.178.101.111:26211`, was unreachable.

| | A = 160.16.131.119 | B = 95.111.236.186 |
|---|---|---|
| binary sha256 | `4decb38c9c91e2c9…` (shared) | `4decb38c9c91e2c9…` (shared) |
| virtual_daa_score | 28,212,468 | 26,846,047 |
| sink_blue_work | 5,054,193,886,343 | 4,966,177,557,653 |
| pruning_point | `8857716…aec0c` | `8857716…aec0c` (same) |
| sink | `eb19e72…25781` | == its own pruning point |
| is_synced | false | false |
| tip_count | 2 | 4 |

Same pruning point, so the split is within one pruning period; A holds the heavier chain
(+1.37 M DAA, +88 G blue work); B's sink equals its own pruning point — the stuck state the
F1/F2 fixes exist for. `misaka-kaspad`/`misaka-validator`/`misaka-miner` units: inactive on
both. Both production nodes run under systemd as **`kaspad-tn.service`** (`Restart=always` —
a raw `kill` is answered by a respawn, discovered the hard way during B's cutover; every binary
swap goes through `systemctl stop`). Exact command lines preserved per host as
`cmdline.recorded`, unit backups as `kaspad-tn.service.bak-pre-candidate-20260810`.
`misaka-dnsseeder` units: active on both, running pre-F5 binaries (`daa1143c…`) to be updated
to the candidate seeder (`b4c71dba…`) once the nodes are stable.

**Third seeder zone, 2026-08-10.** Until now both seeder zones (`misakascan.com`,
`misakachain.com`) delegated to the same two hosts that back the fleet's own nodes, so
bootstrap discovery was single-operator AND single-pair: losing either host took a
disproportionate share of the discovery path with it. `seeder1.misakastake.com` is delegated
(NS → `ns-seeder1.misakastake.com`, glue A → `5.104.81.23`) to host C, the third machine,
which now runs `misaka-dnsseeder` (`b4c71dba…`, same binary as A and B) bound to the public
IP — `systemd-resolved` owns `127.0.0.53/54:53` there, so `0.0.0.0:53` would collide. ufw
opened UDP/53 only; 26211/tcp was already allowed.

Adding a seeder is NOT a flag day: `consensus_params_id` excludes `dns_seeders`, and the
pinned-fingerprint test passing unchanged is the proof. The record answers `NOERROR` with
zero A records today — F5 fail-closed, because C's backing node is still doing its
from-genesis IBD (`refresh failed … reports is_synced=false`). That is the wanted behaviour:
a recovering node must not be advertised as a bootstrap target. It starts answering when C
syncs, which is also the moment its answer is worth having.

## What is being installed

One build, made once on host C from a fresh clone pinned to `00d1294` (workspace tests all
green at that commit; RC7's regression series still running at install time — the operator
chose not to wait, and the residual risk is recorded here rather than hidden). sha256 of
`kaspad` recorded below at install time; every node runs that exact file.

| artifact | sha256 | commit |
|---|---|---|
| kaspad (superseded same-day) | `5ed2dce364015fdd81ca2412ee0193cf2d5d4cb5737fe4652301687fbf458e8d` | `00d1294` |
| **kaspad (running)** | `1cfcf4a09aca82ba05f1118c1219ed889fad1f099f2d4ee0458f9068f15300e1` | `203d2c6` |
| misaka-dnsseeder | `b4c71dba09d1cda0c1347084ae52e89611c1de502f7ce2bda390fb2b504977c7` | unchanged between the two |
| misaminer | `30624eb3161e333584942cb2ac4a07c80ff425cd69970e223bcbe5cd5af0dc3a` | unchanged between the two |

The same-day supersede (operator-directed) carries the stake preference (testnet 2), the
settlement policy layer (testnet 30_000 DAA), the wallet pending display, and the
consensus-settlement fence (`u64::MAX`, inert). During the recovery all three
additions are inert or abstaining on the live fork by construction.

**Third build the same day — the EVM feature had been dropped.** Batches 1 and 2 were
built plain (`cargo build --release -p kaspad`), but the production nodes have always run
`--features evm` builds; the deficient builds parsed the EVM flags and then served no EVM
lane at all. The visible symptom was already in the wild before the flag day and got
reported by a community operator as the pruned-IBD loop — `peer cannot serve the pruning
point EVM state required for pruned IBD on this network` — and reproduced inside the island
(A refused by B, 84 handshake-adjacent refusals). Batch 3 (`1b5de1376e37f3ae…`, commit
`2221e8a`, WITH `--features evm`) replaced batches 1/2 on all three hosts; the binary
parity check now includes the feature set, not just the digest. With the EVM build in
place, `--evm-materialize-pp-anchor` (F2c) ran its one-shot backfill: **A** reverse-replayed
1.37 M diffs and reports the pruning-point anchor **materialized and VERIFIED**, so A can
serve pruned IBD — the community operator's loop resolves against A. **B** reports the
honest terminal state `no committed EVM header at the pruning point` (its wedged datadir
never processed EVM up to the pp): B cannot anchor and will simply resync via A2 like
everyone else. Binary trail per host now: `kaspad.prev-noevm-1cfcf4a0`, `kaspad.prev` =
`5ed2dce…`, `kaspad.prev-4decb38c` = pre-flag-day.

(also in `cand-build.sha256` on each host; the pre-update `kaspad` on A and B was
`4decb38c9c91e2c9…`, kept as `kaspad.prev` beside the installed candidate)

Restart command lines: each host's `cmdline.new` — the recorded production flags plus
`--enforce-chain-participation` (the gate under soak) and `--enable-unsynced-mining`
(cold-start: the network has no miner and a halted DAA; the gate + F3's explicit
`--mine-when-not-synced` miner override are the tested pair for exactly this state), and
`--addpeer` rows completing the three-node mesh. The dead `217.178.101.111` addpeer is
kept on purpose: if that host ever returns it is a live straggler, and its refusal at
handshake is a soak observation.

**Miner-side F3 coverage caveat (found 2026-08-10, fixed same day).** The "tested pair"
above was true of `kaspa-pq-miner`, where F3 (0719a49) landed — but the binary the
production unit template actually runs is `misaminer`, which at deploy time had NO
`is_synced` check at all and mined unconditionally (the exact runaway that built Branch A).
The staged cold-start miner (`30624eb3…`) therefore needs no override flag to mine the
halted network — and equally would not have refused to re-mine a dead branch. F3 has now
been ported to `misaminer` (same refusal, same `--mine-when-not-synced` override, same 30 s
warn throttle, refusal predicate unit-tested); a miner rebuilt at or after that commit
requires the explicit flag for the cold start, and the runaway path is closed for both
miner binaries.

## Branch M, and why the order below is the order

A's 28.2 M chain is **Branch A** — the difficulty-floor branch it self-mined while
isolated; the true majority chain (**Branch M**, ~29.7 M) lives on three external
old-build peers (`169.58.39.220/13.16/3.28`). The flag day severs the fleet from those
peers, so Branch M must be inside the fleet *before* the fleet flips. That copy is the
P0 E2E node already running on host A (appdir `/home/ubuntu/p0-e2e-appdir`, private ports
361xx/366xx, `--connect` to the three Branch M peers, pre-fingerprint fix build): at
planning time it stood at DAA 29.30 M with its blue work already above Branch A's. When
it completes, its appdir is a full Branch M datadir owned by the fleet — it becomes node
**A2**, restarted under the candidate on `0.0.0.0:36211`.

Binary swaps on A use `mv` + `cp` (rename, never copy-onto): the E2E process runs from
the same path being replaced, and an in-place copy would truncate the running inode.

## Order of operations

1. **B first, with the rollback rehearsal.** Install → start → verify RPC → stop →
   restore `.prev` → start old → verify RPC → reinstall candidate. After this, rollback
   is a rehearsed operation, not a document. B then idles peerless — expected, until A2.
2. **E2E completes → A2.** Stop the E2E cleanly, restart its appdir under the candidate
   (`cmdline.a2`). B begins adopting Branch M from A2 — the first live exercise of the
   fixed comparator (F1), against the real majority chain.
3. **A.** Swap the production binary (`mv`-then-`cp`), restart on its existing Branch A
   datadir with `cmdline.new`. A must abandon its own mined branch for Branch M via the
   deep-reorg/IBD path — the exact incident scenario, run live on the node that caused it.
4. **C = 5.104.81.23.** Fresh node, fresh appdir, syncing from genesis against the island
   under the gate — the from-genesis IBD path, on the real chain, under the candidate.
5. **Seeders** (`misaka-dnsseeder` units on A and B) updated only if their running binary
   differs from the candidate build's.
6. **Cold-start mining** against A2's Branch M tip (candidate `misaminer
   --mine-when-not-synced`, operator's own payout address) until `is_synced=true`
   fleet-wide, then the override is dropped.

## Rollback (rehearsed in step 1)

On the affected host: stop kaspad → `mv kaspad.prev kaspad` → start with
`cmdline.recorded`. Datadirs are not touched by install or rollback; A additionally holds
a pre-restart datadir backup (`misaka-testnet-10.pre-t10-restart-backup`) from 2026-08-07.
Rolling back re-splits the fleet at the handshake (old ↔ new cannot peer), so a rollback
is also fleet-wide or not at all.

## Recovery findings (2026-08-10, the deep-reorg adoption of node A)

Node A's adoption of Branch M — a ~1.6M-block-deep reorg off its isolated Branch A — surfaced
four things worth keeping, three of them defects and one a design edge behaving as designed:

1. **Quarantine had no operator exit (FIXED, `cca8b1e`).** B and C were both driven to
   `Quarantined` by IBDs that failed after `staging.commit()`; A joined them via the
   unresolved-candidates commit barrier. ADR-0025 said "until an operator intervenes" but shipped
   no interface — the only exit was hand-deleting the meta-DB key. Added `--clear-quarantine`
   (one-shot, WARNs each boot, clears quarantine only, keeps the switch counter).

2. **The deep-reorg IBD OOM-loops on a co-tenant host (ROOT CAUSE of A's non-convergence).**
   With `highest_known_syncer_chain_hash == None` (no shared chain segment past the pruning point
   — the deep-fork case), `determine_ibd_type` takes `DownloadHeadersProof`, which stages the
   ENTIRE header set in a fresh staging consensus before the commit barrier. On A that reached
   anon-rss 10.5 GB; sharing a 16 GB host with A2 (the canonical server, ~6 GB) the kernel
   `global_oom` killed A mid-validation every ~40 min, and staging was discarded, so every retry
   restarted from the Jul-30 negotiation point. NOT a logic loop — a memory ceiling. Fix in place:
   `--ram-scale=0.25` + systemd `MemoryHigh=5G`/`MemoryMax=6G` on A's unit, so the two nodes no
   longer contend and any OOM hits A alone, before the host. The real lesson for the soak: the
   headers-proof staging peak is unbounded by fork depth and needs a documented per-host RAM floor
   (or a staged/streamed header import) before a from-scratch deep reorg is run beside another node.

3. **The unresolved-candidates barrier can self-wedge on a stale candidate set.** Repeatedly-severed
   IBDs (here, caused by #2 and by operator restarts) leave provisional candidates in the registry;
   the barrier then refuses to commit ("N other chain candidate(s) ... none could be verified in
   time") and quarantines. Correct-by-design (arrival-order commit is what fixes a partition in
   place), but the exit is operator-only: `--trusted-checkpoint <daa>:<hash>:<params-id>` naming
   the DNS-confirmed anchor, which is exactly the history the operator can vouch for.

4. **A2 as the explorer/mining/validator anchor is what kept the network live throughout.** None of
   the above touched A2: it served the canonical chain, fed the miner, and carried the re-bonded 20M
   validator while A thrashed. Splitting the "canonical server" from the "node being recovered" onto
   separate processes is why misakascan never stopped advancing during A's four failed adoptions.

## Known log artifacts the soak baseline inherits

B's log carries exactly one `panicked at` line, 2026-08-10 03:17:12 +02:00
(`conn_builder.rs:167`, RocksDB LOCK contention): the first candidate start raced the
`Restart=always` respawn of the old unit and lost the DB lock. It is a deploy-window
artifact of the discovered watchdog, not candidate runtime behaviour; the soak verdict
reads count *rises* from this baseline.

## Evidence status at the moment of the decision

RC7 regression at `23ad1ed`: darwin 120/120 ×2 series + 120/120 (seeds 480–599), linux
120/120, seeds 100–299 and 300–479 and 600–719 in flight with zero failures so far;
restart injection 7/7; VPS adversarial 10/10; control 10/10. The candidate `00d1294`
additionally carries the merged unbond-authz acceptance fix and the three merge-collision
fixes (DB prefix 236 split, fingerprint pins, test-race fix), workspace-green locally.
The soak verdict machinery (`soak_verdict.sh`) was validated against synthetic fleets
before this deployment.
