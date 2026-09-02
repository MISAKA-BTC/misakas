# testnet-11 Relaunch 5e — certification on the chain, and a price per class

Branch: `palw-adr0076-class-seed` (ADR-0075 + ADR-0076 over `main` `1bdbddea`).
Target identity: params fingerprint **`a7baab7957d27bbd2591cd24f70ee92b555ab26cd49ef425cbd7093f06e222d9`**
(devnet **`ecb408a97e183c8edf6922c20bb5aa8a2c482c9dddcdfcdf213d2ece4727358a`**),
genesis **`08e9c8a4…`** — unchanged, as at 5c and 5d.

> **Verify what a node ANNOUNCES, not what this page says.** This document has been wrong about a
> fingerprint before, on every previous relaunch, for the same reason: the value is computed at the
> end of an assembly and copied here by hand. Read it off the binary you are about to deploy.

**This is a re-genesis: every host wipes.** The state version moves to 16 (ADR-0075's certification
objects and the chunk table are in the state root), and a node started over a v15 datadir refuses
at boot and names the remedy.

## What this train carries

1. **ADR-0075 — certification is a consensus object.** A family's drill and a class's lane binding
   ride ordinary lifecycle transactions; the transition re-grades the evidence with the shipped
   court and records the family. An object above one carrier's bytes rides as ≤8 `ObjectChunk`s
   reassembled in state (Decision 14). A model can be certified, and seated, without a code change
   or a re-genesis — which is the whole point, and which this relaunch is the last one to need.
2. **ADR-0076 — every class's attempt lane is seeded from its own share and its own counted work.**
   5d gave all three classes one target and the floor took 100 % of the blocks for an hour. The new
   seeds:

   | class | share | `pwu_per_inference` | 5d | 5e |
   |---|---|---|---|---|
   | PALW-BASE-0 `f1c5635c…` | 22‰ | 7,708 | `MAX/2` | **`MAX/12,663`** |
   | PALW-QWEN25-A16 `71bbb755…` | 489‰ | 1,589,424 | `MAX/2` | **`MAX/2.76`** |
   | PALW-QWEN36 `5bd9ae3d…` | 489‰ | 2,685,360 | `MAX/2` | **`MAX/1.63`** |

   **The class ids do not move** — a class id is its profile's id and `initial_target` is
   economics, not identity — so every `--palw-producer-class=` flag on the fleet stays as it is.

## What to expect that is different from 5d

* **No start burst.** 5d's first minute was 27 blocks, all floor. At genesis `bits` the seeded
  table offers ~0.14 hits/s against 10.15, i.e. ~0.4 blocks/min against the 0.5/min the cadence set
  asks for. The chain should look roughly right from the first minute instead of converging for an
  hour.
* **The floor is slow on purpose.** ~12,664 draws per block before the `bits` gate. A floor block
  every few hours is the 22‰ table working, not a stall — the model tiers carry the cadence now.
* **A floor block weighs ~6,331× more** (`PALW_RC_FLOOR_DERIVED_PWU` 15,416 → 97,606,404). Weight
  per second is unchanged: `pwu = expected_attempts(target) × pwu_per_inference` rises exactly as
  fast as the block count falls.
* **QWEN36 still needs ~8 minutes to map its 33 GiB artifact** before its producer starts. Nothing
  in this train changes that.

## The swap

Ports, appdirs and launchers are unchanged from 5c/5d, and so are the producer class flags.

| host | unit | appdir | role |
|---|---|---|---|
| ibm `169.58.39.220` | `misaka-t11-node1` | `/root/.t11b` | floor producer, heartbeat, panel |
| ibm | `misaka-t11-node0` | `/root/.t11` | QWEN36 producer (`5bd9ae3d…`), panel |
| C `5.104.81.23` | `misaka-t11-seat2` | `/root/.t11` | A16 producer (`71bbb755…`), panel |
| `.113` `169.58.232.113` | `misaka-t11-node` | `/root/.t11` | public entry, panel |
| `.113` | `misaka-t11-seat4` | `/root/.t11c` | panel seat 4 |
| `.113` | `misaka-pool-slot@01` | `/var/lib/misaka-minerpool/slots/slot-01/appdir` | hosted producer |

1. **Stop every host first**, then wipe, then start — a surviving peer re-feeds the old chain by
   IBD, which is the 2026-08 lesson and has not stopped being true.
2. Archive each appdir to `<appdir>.old-e2b91c16-<date>`.
3. Install the binary on all three hosts (`/root/t11/kaspad`, previous kept as
   `kaspad.pre-r5e`). Build it once on ibm and `rsync` host-to-host — do not route it through an
   operator laptop.
4. **Rotate the per-genesis stores** on `.113`: the explorer database
   (`ALTER DATABASE kaspa_t11 RENAME TO kaspa_t11_old_e2b91c16_<date>; CREATE DATABASE kaspa_t11
   OWNER kaspa;`), the MTP ledger (`/var/lib/misaka-mtp/data`), and the miner-pool slot appdir. The
   faucet grant ledger on ibm (`/var/lib/misaka-faucet/granted.jsonl`) is NOT genesis-keyed and
   rotating it changes who may claim — operator's call, as at 5c.
5. **Start order**: ibm node1 → ibm node0 → C seat2 → `.113` node → `.113` seat4 → pool slot →
   minerpool → explorer filler + REST. Verify each node announces `a7baab79…` before the next.
6. **Verify, 20-30 minutes in**: every node on one fingerprint and one genesis; the public node
   accepting blocks (`journalctl -u misaka-t11-node --since "-10 min" | grep -c "Accepted block"`
   — zero on a producing chain is the whole test, and it is the check that would have caught the
   46-minute silent hang on 5d's swap); and **at least one block from each model tier**, which is
   the thing this relaunch exists to produce.

## After the swap

Public pages carry the new identity: `docs/testnet11-node-operator.md` and
`docs/testnet11-join-mining.md`. Every earlier fingerprint — `e2b91c16…` (5d), `d38abe44…` (5c),
`accaadce…`, `f0e50f83…`, `5ccdd684…` — names an archived ruleset and is refused at the handshake.
