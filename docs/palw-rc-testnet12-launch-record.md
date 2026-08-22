# testnet-12 — what was verified, and what could not be

**Date:** 2026-08-22 · **Binary:** `kaspad v1.1.0-c1e612a9`, sha256 `87235442e1e1096166498b6a…`
**Genesis:** `28a44a680be0fb35…` · **Consensus fingerprint:** `79a306edbd84e3c4e59c5e585deaf8e9c06a440feb37699303a3e73de05d9794`

This is the record a reader should be able to check rather than believe. It exists because the
same fleet had already been declared healthy twice on evidence that turned out not to mean what it
looked like.

---

## The fleet

Six registry rows, six running nodes across three hosts — the registry has no spare seat (§4 of
the runbook), so this is a requirement and not a preference. All six report the same consensus
fingerprint; a node that shipped a different card would be refused at the handshake rather than
fork.

**Public P2P entry points** (the addresses a joining node needs):

    169.58.39.220:26411
    5.104.81.23:26411

**A known limitation, stated because it is real whether or not it is written down:** the five panel
seats are not spread across five operators. They sit on three hosts, and one of those hosts carries
enough of them to hold a quorum by itself. The panel's independence assumption is therefore not met
on this fleet — it is a release-candidate testnet run by one operator, and the seat-to-host map is
deliberately not published. Treat panel verdicts here as a working lifecycle, not as an adversarial
guarantee. Distributing seats across independent operators is the thing that makes it one.

## Joining

The source is the `palw-base0-depth` branch of `github.com/MISAKA-BTC/misakas`. A joining node
needs no permission and no bond — bonds are the panel's, not a participant's.

```bash
git clone -b palw-base0-depth https://github.com/MISAKA-BTC/misakas.git
cd misakas
cargo build --release -p kaspad
./target/release/kaspad --testnet --netsuffix=12 --utxoindex \
    --addpeer=169.58.39.220:26411 --addpeer=5.104.81.23:26411
```

The EVM lane is in the default build; add `--evm-rpc-listen=127.0.0.1:8545` for the Ethereum
JSON-RPC adapter. `--nodnsseed` is unnecessary and harmless: this network has no DNS seeders yet, so
peers come from `--addpeer` and the address manager.

**Check you are on the right network before anything else.** The startup banner prints
`Consensus params fingerprint: 79a306edbd84e3c4e59c5e585deaf8e9c06a440feb37699303a3e73de05d9794`.
A different value means a different ruleset, and the handshake will refuse rather than fork.
The genesis is `28a44a680be0fb35…`.

**Do not expect `weight` to be non-zero yet.** See the two-stage reading below and in the runbook:
before DAA 1200 the number that moves is `live_total`.

## Verified live

| claim | evidence |
|---|---|
| blocks are real inference | `produced block` from block 1; 0 disqualifications, 0 panics across 443 blocks |
| one ruleset across the fleet | fingerprint `79a306ed…` identical on all six nodes |
| the genesis commits to its own premine | node imports pruning point `28a44a68…`, which is the recomputed pin, and `all_networks_genesis_constants_match_premine` passes |
| **the tool and the node agree** | the shipped `palw-rc-genesis --rows` re-derives `28a44a68…` and `consensus_params_id 79a306ed…` — the same genesis the node imported and the same fingerprint all six nodes report. Two earlier defects lived in exactly this gap: a tool that printed a params id no node would log, and one that printed a genesis hash from its own compiled premine rather than the card it had just built |
| panels bind and seats answer | receipts filed: seat 1 → 394, seat 2 → 216, seat 3 → 116, seat 4 → 116 |
| the executor never sits on its own panel | bond 0 files **zero** receipts while producing every block |
| bond 4 answers | 116 receipts and 3 `ReceiptLicensed` submissions from the seat re-minted onto a reachable host |
| quorums reach the chain | `submitted ReceiptLicensed for claim … in tx …` |
| **the fork-choice key is live** | `live_total` climbs monotonically at exactly `unresolved × 1580` — 0 → 628,840 over 398 claims |
| the EVM lane is queryable | `eth_chainId` → `0x4d534b` ("MSK"), `eth_blockNumber` → `0x1b7`, tracking the PALW chain height |
| the submitter no longer floods | `no fee UTXO resolves` warnings: **24,014 in 15 minutes → 33 across the whole run** |
| **claims stop voiding once the fleet is synced** | terminal claims plateaued at **135** and stayed there while DAA advanced 579 → 591 and `unresolved` rose 444 → 456 — every claim made after the seats caught up is still live |
| the exposure ceiling never fires | `exposure ceiling` holds: **0**, where the previous binary held forever from block 601 |

## One thing that looked like a fault and was not

The first ~135 claims voided, destroying their escrow. The cause was the launch order, not the
network: trace material is gossiped once and never replayed, so the three seats started after the
producer could not verify claims made while they were still syncing. Each filed **exactly 158**
`Unavailable` verdicts over the same claims — the correct answer to "can you verify this?" — and
three such verdicts are a quorum for `ProducerWithholding`. From the moment they caught up they
filed nothing but `Valid`, and the terminal count stopped moving. The runbook now says to bring
every seat to a synced tip before producing.

A second thing that reads worse than it is: the `voided claims holding … sompi` warning is emitted
inside the virtual-resolution walk, so a claim can be counted by several candidate walks. The
authoritative figures are `unresolved` and `final_claims` from the producer facts, which are read
at virtual's selected parent.

## Verified by construction, NOT observable in a launch window

Two properties cannot be watched on a chain younger than its own windows. Both are proven at the
layer that owns them, and the reasoning is stated so a reader can disagree with it.

**The first `Final` lands at DAA 1200 — about 40 hours in.** The cadence is frozen at 120 s
(ADR-0038 Decision H, refused at `Params` construction), and `window_challenge` is 1200 DAA. Until
then `safe_weight` and `final_claims` are zero on *every* healthy chain, which is why `live_total`
exists and why the runbook now reads the weight line in two stages.

**The exposure ceiling admits 5,401 concurrent claims**, so honest production cannot reach it before
claims begin releasing. `honest_production_reaches_the_first_final_before_it_reaches_the_ceiling`
pins it against the shipped params, and against the old sizing it prints what the fleet printed:
*the ceiling admits 601 concurrent claims, but no claim can finalize before block 1200*.

## Not carried

The free-prompt receipt **spend** lane (algo 7). Not a fence this deployment set — a ConsensusV2
network is algo-6 by definition. See runbook §5b.

## Test state at this commit

`MISAKA_PALW_POW_FIXTURE=1 cargo test --workspace --lib` — **68 suites, 2,448 tests, 0 failures.**

## Open, and owned by the operator

- One fleet host is near its disk limit, and one candidate host was left out because its egress to
  the fleet is filtered upstream. Both are operator-side and neither affects a joining node.
- **The panel seats are concentrated** — see above. This is the gap between "the lifecycle works"
  and "the lifecycle is adversarially sound", and closing it needs independent operators, not code.
