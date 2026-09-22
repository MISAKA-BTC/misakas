# testnet-12 regenesis — deployment record, 2026-09-23

Build `de857a71` (`kaspad v1.1.0-de857a71`), a release build of `feat/testnet-12-regenesis`.

## The network

| | |
|---|---|
| genesis hash | `a8cabac47b96fe30d9675ce08a355f62e6d57aac84865b0d6295c024c6d8ff61fb2d93943952e8da4c36ff87ea7f17f6c8de643fa7b02ecf7512892d590777dd` |
| params fingerprint | `fb8f378df6373455e0c14d08c835184b86717ae3fc983039f9ded633be7fa38d` |
| fence schedule | **`1000`** — one height, ADR-0065 D1's bond-maturity window (t11's was `1150, 1900, 2150, 2400, 3500, 4000, 6900, 7100, 7101, 7200, 7301, 8000, 2125000`) |
| rule manifest | `… palw_work_target=1 palw_independence=1` (digest `9def81a1…`) |
| premine | 33 outputs, exactly 10B MSK: 8 collateral + 8 fee floats + **16 community (758M)** + main wallet |
| bond collateral | 60,088.18407600 MSK a seat (8 seats = 0.0048 % of the cap) |
| classes | BASE-0 floor + held Qwen3.6 `e108e736…`@512 + held Qwen2.5 `74c67e63…`@2,097,152, **each at the minimum grantable share** |
| P2P port | 26311 — unchanged from t11, by the operator's decision |

## Hosts

| host | unit | appdir | listen | bond / fee |
|---|---|---|---|---|
| 169.58.232.113 | `misaka-t12-node` | `.t12` | 0.0.0.0:26311 | 6 / 47 |
| 169.58.39.220 (ibm) | `misaka-t12-node1` | `.t12b` | 0.0.0.0:26321 | 1 / 42 |
| 5.104.81.23 | `misaka-t12-seat2..5` | `.t12`, `.t12b`, `.t12c`, `.t12e` | 127.0.0.1:26311/21/31/41 | 2..5 / 43..46 |
| 95.111.236.186 | seeder only | — | — | — |

Seeders, all four: `misaka-dnsseeder-t12` (`--network-id testnet-12`), binary `1174b965…`.

**The t11 units, launch scripts and datadirs are all still in place.** Rollback is: stop the t12 unit,
`systemctl enable --now` the t11 one.

## What the switch did NOT change, and why that matters

Every PALW flag on every host carried across untouched — `--palw-producer-class=74c67e63…`, the 2M
class artifact, `--palw-producer-key=/etc/misaka/t12/t12-bond-N.key`, `--palw-producer-bond=…:N`,
`--palw-fee-outpoint=…:4N`, and all ports. testnet-12 re-uses the same eight genesis bond cards, the
same premine sentinel txid and indices, and registers the very class the fleet was already mining.
Only the network's name, its genesis and its datadir changed. `the_fleets_premine_outpoints_are_unchanged`
pins the bond↔float pairing (1→42 … 6→47) so a future change cannot break the submitters silently.

## Three things this deployment caught

1. **The old DNS seeder advertises the wrong port on testnet-12.** Measured, not assumed:
   `misaka-dnsseeder --network-id testnet-11` health-checks anchors on `:26311`, and the same binary
   with `testnet-12` checks `:26411` — the `None | Some(_)` fallback in the pre-regenesis
   `NetworkId::default_p2p_port`. Switching the seeders' flag without replacing the binary would have
   pointed every new user at a dead port. The rebuilt seeder answers `:26311` for both.
2. **Stale August `testnet-12` datadirs, with consensus data.** `/root/.t12*` existed on 5.104.81.23
   (64M + 48M + 53M) and 95.111.236.186 (87M) from the internal relaunches the suffix was minted for
   in August. Moved aside as `.aug2026-regenesis-bak-<ts>`, never deleted. Starting on one would have
   meant either a genesis-mismatch refusal or, worse, a silent resume of a dead chain.
3. **A plain `SIGTERM` can leave kaspad hung after it has closed its listeners.** Observed in the
   drill: the node logged `SIGTERM - shutting down…`, stopped its P2P/gRPC/wRPC servers, and then sat
   in state `S` for 19 minutes ignoring both SIGTERM and SIGINT — a process that looks alive and
   serves nothing. The live units are already protected (`KillSignal=SIGINT` + `TimeoutStopSec`, which
   escalates to SIGKILL), and the switch confirmed it: **no hung t11 node was left on any host.** A
   bare `kill` in a script is not protected.

## Drill evidence

Same binary as shipped. `consensus/tests/palw_t12_liveness.rs` and
`consensus/core/tests/t12_economic_safety_drill.rs` cover the rule-level half; this is the live half.

* **boot** — both nodes loaded the genesis, peered, and `[palw-heartbeat-miner] heartbeat #1 … the
  clock ticked` inside 35 s. "**bondless** heartbeat lane (ADR-0060), fee-only" is ADR-0151 D3 in the
  node's own words: the lane that carries this chain's clock takes no bond, so no collateral figure
  can stop it.
* **restart** on the same datadir — ticks continued, 0 errors.
* **IBD** — a third node with no datadir accepted 8 blocks and started minting, 0 errors.
* **partition** — with its only peer gone, the survivor kept ticking (76 beats alone). This is the
  liveness property the collateral reduction rests on, observed under partition.
* **join as a user would** — a fresh node with no `--addpeer` and no `--connect` queried the four DNS
  seeders, got addresses from seeder1 and seeder3 (seeder2/seeder4 are not delegated in public DNS),
  connected to `169.58.232.113:26311`, and reached the tip (`challenger_work=21 defender_work=21`),
  0 network mismatches.

**Still owed** (ADR-0151 §4): a forced reorg, and the class-at-cap states on a fleet that is actually
producing model blocks. The arithmetic for every registered class is covered by the drill test; the
reachable-state search over a live reorg is not.

## Operational notes

* **External t11 users are now refused, by design.** ibm logged
  `handshake failed … Network mismatch - local: misaka-testnet-12, remote: misaka-testnet-11` from
  three Japanese IPs within minutes of the switch. The network-id gate is working; those participants
  need this build. **An announcement is the operator's to make.**
* Only `169.58.232.113:26311` is reachable at the default P2P port. ibm's node listens on 26321, so a
  client that picks it from a seeder answer and dials the default port fails and must retry. This is
  the topology t11 had, not a regression, but it means one entry point.
* `/root/misakas-stale-consensus-diagnosis` is still running a t11 fixture node on 5.104.81.23 on its
  own ports and datadir. It belongs to another session's work and was left alone.
