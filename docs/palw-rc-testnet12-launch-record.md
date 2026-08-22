# testnet-12 — what was verified, and what could not be

**Date:** 2026-08-22 · **Binary:** `kaspad v1.1.0-c1e612a9`, sha256 `87235442e1e1096166498b6a…`
**Genesis:** `28a44a680be0fb35…` · **Consensus fingerprint:** `79a306edbd84e3c4e59c5e585deaf8e9c06a440feb37699303a3e73de05d9794`

This is the record a reader should be able to check rather than believe. It exists because the
same fleet had already been declared healthy twice on evidence that turned out not to mean what it
looked like.

---

## The fleet

| host | roles | bonds | P2P 26411 from outside |
|---|---|---|---|
| 169.58.39.220 | producer + seat | 0, 1 | **reachable** |
| 5.104.81.23 | seat, seat, seat | 2, 3, 4 | **reachable** |
| 160.16.131.119 | seat | 5 | outbound only |

Six registry rows, six running nodes — the registry has no spare seat (§4 of the runbook), so this
is a requirement and not a preference. All six report the same consensus fingerprint; a node that
shipped a different card would be refused at the handshake rather than fork.

## Verified live

| claim | evidence |
|---|---|
| blocks are real inference | `produced block` from block 1; 0 disqualifications, 0 panics across 443 blocks |
| one ruleset across the fleet | fingerprint `79a306ed…` identical on all six nodes |
| the genesis commits to its own premine | node imports pruning point `28a44a68…`, which is the recomputed pin, and `all_networks_genesis_constants_match_premine` passes |
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

- **Host B is at 98% disk** (18 GB free). `p0-e2e-appdir` (267 GB) and `kpq-testnet.bak-pre-hotfix`
  (64 GB) are prior artifacts; nothing was deleted, because deleting them is not this task's call.
- A fourth host (95.111.236.186) is out of the fleet: its egress reaches exactly one fleet member,
  the block is upstream rather than a local rule, and its memory belongs to a live testnet-10 node.
- **Nothing has been published.** The branch is not on the public remote and no binaries are
  distributed, so no third party can join yet — that step is a publication decision, not a
  configuration one.
