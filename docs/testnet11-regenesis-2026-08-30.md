# testnet-11 regenesis — the audited union build (2026-08-30)

**This is a regenesis, not an upgrade.** The genesis block itself moves: the 10B premine cap
(ADR-0059) rewrites the UTXO set every network is born with, and the coinbase marker goes to
`11,3`. `deploy-t11.sh` gates on "the candidate synced this chain and disqualified nothing", which
is the right gate for a rolling upgrade at an unchanged genesis and is **unsatisfiable here by
construction**. Do not try to make it pass.

## What moves

| | live now | after |
|---|---|---|
| consensus fingerprint | `95265934…` | **`f3bf86b4e9327f8b02ab2ad1d121d62ecd11bd78cca1455d8bcd7372595153d8`** |
| genesis hash | `c664a224…` | **`d2789338d7a0a93c…`** |
| coinbase marker | `11,2` | `11,3` |
| genesis premine | 13,547,000,000 MSK (58 UTXOs) | **10,000,000,000 MSK exactly** |
| main wallet (spendable) | 8,852,999,400 | **9,452,939,400** |
| bond collateral | 6 × 100,000,000 MSK | 6 × 10,000 MSK |
| `PALW_STATE_V2_VERSION` | 12 | 13 |

Community allocations are **unchanged**: the same 11 addresses, the same 547M MSK, at the same
indices on the same sentinel txid. Nobody needs a new address and no allocation moves. What is
discarded is the chain built on top of them.

Two features in this build ship OFF and it matters operationally: the **heartbeat lane** and the
**finality inactivity leak** (see `palw-mainnet-audit-2026-08-30.md`). The practical consequence
is that this network still depends on a floor producer for liveness — there is no bondless clock
yet — so the DAA-360 stall of 2026-08-30 remains a live risk if the floor producer stops.

## The fleet, measured 2026-08-30

| host | units | appdir | reachable |
|---|---|---|---|
| `169.58.39.220` (ibm) | `misaka-t11-node0` (producer+panel), `misaka-t11-node1` (failed/OOM), `misaka-faucet` | `/root/.t11`, `/root/.t11b` | directly |
| `169.58.232.113` | `misaka-t11-node`, `misaka-dnsseeder`, `misaka-minerpool`, `misaka-pool-slot@01`, `misaka-mtp` | `/root/.t11` | via ibm as root |
| `160.16.131.119` (A) | `misaka-t11-node-b`, `misaka-t11-hub-tunnel` | `/home/ubuntu/.t11` | via ibm as ubuntu |
| `5.104.81.23` (C) | — | — | **NO** — its operator must act |

Community nodes seen as peers in the last 12 h and orphaned by this change:
`5.104.81.228`, `217.178.131.170`, `13.140.185.225`, `60.114.127.4`, `169.58.13.16`,
`207.180.230.3`, `183.176.36.141`, `113.155.23.105`.

## Order, and why

Stop **every** node before wiping **any** node. At a moved fingerprint the handshake would refuse
an un-wiped peer anyway (`PALW_STATE_V2_VERSION` survives `consensus_identity_id`'s normalisation,
so the two rulesets carry different identities) and the genesis moved on top of that — but the
minute this costs is cheaper than the hour a re-seeded old chain costs.

```bash
# 0. stage — non-destructive, verify before touching anything
/root/host-build-from-branch.sh palw-bond-economics-2026-08-30
grep -E '^(HEAD|EXIT|STAGED)' /root/deploy-build.log

# 1. ask the CANDIDATE what it will announce (--version does not answer this)
#    boot it on a throwaway appdir and read the fingerprint line; it must be f3bf86b4…

# 2. stop everything, everywhere (kaspad ignores SIGTERM, handles SIGINT)
systemctl kill -s INT misaka-t11-node0 misaka-t11-node1
ssh root@169.58.232.113 'systemctl kill -s INT misaka-t11-node; systemctl stop misaka-minerpool misaka-pool-slot@01 misaka-mtp misaka-dnsseeder'
ssh ubuntu@160.16.131.119 'sudo systemctl kill -s INT misaka-t11-node-b'

# 3. wipe every datadir
rm -rf /root/.t11/misaka-testnet-11 /root/.t11b/misaka-testnet-11
ssh root@169.58.232.113 'rm -rf /root/.t11/misaka-testnet-11'
ssh ubuntu@160.16.131.119 'rm -rf /home/ubuntu/.t11/misaka-testnet-11'

# 4. install the candidate on every host, then start the PRODUCER first
#    (it mints block 1; the others sync from it)

# 5. verify: the running fingerprint is f3bf86b4…, and sink_daa is MOVING
```

`scripts/regenesis-t11.sh` in the operator's home carries this with the checks wired in
(fingerprint cross-check, a typed confirmation before the wipe, and a post-start liveness read).

## After

* **C and every community node** must wipe and rebuild from this branch. Until they do they keep
  running the old chain among themselves — a communications problem, not a contamination one,
  because no upgraded node will peer with them.
* **Downstream state on `.113` references the old chain**: the explorer DB filler, the REST API,
  the DNS seeder and the MTP ledger. Reset the explorer DB and the MTP ledger or they will serve
  rows for a chain that no longer exists.
* **The floor producer must be running.** `ibm-node0.sh` deliberately omits
  `--palw-producer-class` so node0 produces the FLOOR class; both model classes spent their
  first-epoch budget on 2026-08-30 and the chain stopped at DAA 360 with nothing producing the
  floor. With the heartbeat lane off, that failure mode is still live.
* Announce the new fingerprint and genesis hash to participants.
