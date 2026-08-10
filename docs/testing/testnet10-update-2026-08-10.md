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
both (the nodes run as raw processes; exact command lines are preserved on each host as
`cmdline.recorded` next to the binary). `misaka-dnsseeder` units: active on both.

## What is being installed

One build, made once on host C from a fresh clone pinned to `00d1294` (workspace tests all
green at that commit; RC7's regression series still running at install time — the operator
chose not to wait, and the residual risk is recorded here rather than hidden). sha256 of
`kaspad` recorded below at install time; every node runs that exact file.

| artifact | sha256 |
|---|---|
| kaspad (linux x86_64, C-built) | recorded in `cand-build.sha256` on each host at install |

Restart command lines: each host's `cmdline.new` — the recorded production flags plus
`--enforce-chain-participation` (the gate under soak) and `--enable-unsynced-mining`
(cold-start: the network has no miner and a halted DAA; the gate + F3's explicit
`--mine-when-not-synced` miner override are the tested pair for exactly this state), and
`--addpeer` rows completing the three-node mesh. The dead `217.178.101.111` addpeer is
kept on purpose: if that host ever returns it is a live straggler, and its refusal at
handshake is a soak observation.

## Order of operations

1. **B first, with the rollback rehearsal.** B is the worse-off node (stuck sink) and A
   still holds the heavier chain while B is down. Install → start → verify RPC → stop →
   restore `.prev` → start old → verify RPC → reinstall candidate. After this, rollback is
   a rehearsed operation, not a document.
2. **A.** Same, minus rehearsal. From here A(candidate) ↔ B(candidate) peer, and B must
   IBD onto A's heavier chain — the first live exercise of the fixed comparator (F1).
3. **C = 5.104.81.23.** Fresh node, fresh appdir, syncing from genesis against A+B under
   the gate — the from-genesis IBD path, on the real chain, under the candidate.
4. **Seeders** (`misaka-dnsseeder` units on A and B) updated only if their running binary
   differs from the candidate build's.
5. **Cold-start mining** on A (candidate `misaminer --mine-when-not-synced`, operator's
   own payout address) until `is_synced=true` fleet-wide, then the override is dropped.

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
