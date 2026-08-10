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

## What is being installed

One build, made once on host C from a fresh clone pinned to `00d1294` (workspace tests all
green at that commit; RC7's regression series still running at install time — the operator
chose not to wait, and the residual risk is recorded here rather than hidden). sha256 of
`kaspad` recorded below at install time; every node runs that exact file.

| artifact | sha256 |
|---|---|
| kaspad | `5ed2dce364015fdd81ca2412ee0193cf2d5d4cb5737fe4652301687fbf458e8d` |
| misaka-dnsseeder | `b4c71dba09d1cda0c1347084ae52e89611c1de502f7ce2bda390fb2b504977c7` |
| misaminer | `30624eb3161e333584942cb2ac4a07c80ff425cd69970e223bcbe5cd5af0dc3a` |

(also in `cand-build.sha256` on each host; the pre-update `kaspad` on A and B was
`4decb38c9c91e2c9…`, kept as `kaspad.prev` beside the installed candidate)

Restart command lines: each host's `cmdline.new` — the recorded production flags plus
`--enforce-chain-participation` (the gate under soak) and `--enable-unsynced-mining`
(cold-start: the network has no miner and a halted DAA; the gate + F3's explicit
`--mine-when-not-synced` miner override are the tested pair for exactly this state), and
`--addpeer` rows completing the three-node mesh. The dead `217.178.101.111` addpeer is
kept on purpose: if that host ever returns it is a live straggler, and its refusal at
handshake is a soak observation.

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

## Evidence status at the moment of the decision

RC7 regression at `23ad1ed`: darwin 120/120 ×2 series + 120/120 (seeds 480–599), linux
120/120, seeds 100–299 and 300–479 and 600–719 in flight with zero failures so far;
restart injection 7/7; VPS adversarial 10/10; control 10/10. The candidate `00d1294`
additionally carries the merged unbond-authz acceptance fix and the three merge-collision
fixes (DB prefix 236 split, fingerprint pins, test-race fix), workspace-green locally.
The soak verdict machinery (`soak_verdict.sh`) was validated against synthetic fleets
before this deployment.
