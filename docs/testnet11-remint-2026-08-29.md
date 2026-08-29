# testnet-11 re-mint onto the audit3 build (2026-08-29)

**What moves.** The consensus fingerprint, from what the fleet runs today to what this build
announces:

| | |
|---|---|
| fleet today | `15bab795442ec3efc3a58e02dd9c7a6f3015ff0634bc4a50a7af589338857ad0` |
| this build | `95265934e8965e91f3c22281af735bcd38527b5ee89fa09a05290db566d444a3` |
| branch | `palw-audit3-2026-08-29` @ `7457cff3` |
| `PALW_STATE_V2_VERSION` | 10 → 12 |

**What does NOT move: the genesis.** `git diff 8e982b7e 7457cff3 -- consensus/core/src/config/genesis.rs consensus/core/src/config/premine.rs`
is empty. The genesis block, the 13B premine and the **347,000,000 MSK community allocation across
nine addresses** are byte-identical. Participants keep their block-0 coins; what is discarded is the
chain built on top of them.

So this is a **wipe and restart from the same genesis**, not a new genesis. Nobody needs a new
address and nobody's allocation changes.

## Why it cannot be a rolling upgrade

`/root/deploy-t11.sh` gates on the only question that decides it: *the candidate must have synced
this chain from nothing and disqualified NOTHING.* This build cannot pass that gate and should not
— three rules changed about which blocks and which state are valid (S-03, S-04, H4 under
`PALW_STATE_V2_VERSION` 12), so it necessarily refuses blocks the live chain accepted. The script
refusing is the script working.

Nodes on the two fingerprints will not peer, which is the correct failure direction: they find out
at the handshake instead of at consensus.

## The fleet, as measured on 2026-08-29

| host | unit | appdir | reachable from here |
|---|---|---|---|
| `169.58.39.220` (ibm) | `misaka-t11-node0` | `/root/.t11` | yes, directly |
| `169.58.39.220` (ibm) | `misaka-t11-node1` | `/root/.t11b` | yes, directly |
| `160.16.131.119` (A) | `misaka-t11-node-b` | `/home/ubuntu/.t11` | yes, via ibm as `ubuntu` |
| `169.58.232.113` | `misaka-t11-node` | `/root/.t11` | yes, via ibm as `root` |
| `5.104.81.23` (C) | — | — | **NO** — publickey denied as `root` and `ubuntu` |
| `169.58.232.114` | — | — | **NO** — publickey denied |

Plus community nodes, seen as inbound peers at the producer: `113.155.23.105`, `133.18.141.168`,
`183.176.36.141`, `207.180.230.3`, `217.178.131.170`, `60.114.127.4`.

**A partial wipe is worse than no wipe.** An un-wiped peer re-supplies the old chain by IBD to every
host that was wiped, and the fee-outpoint damage that follows is not self-healing. Every host stops
before any host is wiped.

## Preconditions that are NOT met yet

1. **C and .114 cannot be driven from this session.** Someone with credentials for them must run
   the same stop/wipe/deploy, in the same window.
2. **Community participants must be told**, and must wipe their own datadir. Without that they hit
   the startup genesis-mismatch guard — or, worse, keep a fork alive that new nodes can still reach.
3. **ibm's root disk is at 96%** (13 GB free on 290 GB). A fresh datadir plus the retained build
   tree needs headroom that is not there today.
4. **`misaka-t11-node1` is in an OOM restart loop** — `NRestarts=50`, `Failed with result
   'oom-kill'`. Re-minting does not fix it; it will resume looping on the new chain.
5. **Downstream state on .113 references the old chain**: the explorer DB filler, the REST API, the
   DNS seeder and the MTP service. The explorer DB and the MTP ledger need their own reset, or they
   will serve rows for a chain that no longer exists.

## The sequence, once those are settled

```bash
# 1. stage the candidate on every host (non-destructive, do it first and verify)
/root/host-build-from-branch.sh palw-audit3-2026-08-29
grep -E '^HEAD=|^EXIT=|^STAGED' /root/deploy-build.log
/root/t11/kaspad.candidate --testnet --netsuffix=11 --version   # must announce 95265934…

# 2. STOP EVERY HOST FIRST. kaspad ignores SIGTERM and handles SIGINT.
systemctl kill -s INT misaka-t11-node0 misaka-t11-node1     # ibm
systemctl stop misaka-t11-node0 misaka-t11-node1
#   …and the same on A (misaka-t11-node-b), .113 (misaka-t11-node), C, .114

# 3. only when every host is confirmed down: wipe
rm -rf /root/.t11/misaka-testnet-11 /root/.t11b/misaka-testnet-11        # ibm
#   /home/ubuntu/.t11/misaka-testnet-11 on A, /root/.t11/misaka-testnet-11 on .113

# 4. swap the binary
cp -a /root/t11/kaspad.candidate /root/t11/kaspad.incoming && mv /root/t11/kaspad.incoming /root/t11/kaspad

# 5. start, and confirm the announced fingerprint on each node before starting the next
systemctl start misaka-t11-node0
grep -a 'Consensus params fingerprint' /root/.t11/misaka-testnet-11/logs/rusty-kaspa.log | tail -1
```

`cp`, not `mv`, at step 4: the running process held that inode, and replacing the file under a live
mapping is how a node comes back as something nobody built.

## Also worth clearing in the same window

A node at `169.58.13.16` answers to the testnet-11 network name on a **different genesis**
(`d25a80b9…` against the fleet's `c664a224…`) and retries the handshake about every two seconds. It
is being refused correctly, but it is a stale deployment that somebody should stop.

## What the re-mint carries

Everything in `docs/palw-mainnet-audit3-2026-08-29.md`: four criticals, eleven highs, the M2-10
producer gate, and the R-3 acceptance test made deterministic. Two items are explicitly recorded as
fixed-but-untested, in the source rather than in a table — S-04's refusing half (needs a two-bond
harness) and S-01's court round trip (needs a two-node one).
