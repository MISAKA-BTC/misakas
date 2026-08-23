# testnet-11 — what was verified, and what could not be

**Genesis:** `d25a80b9045abb97…` · **Consensus fingerprint:**
`048e69026e559e67584ded64f1b6279148e3459975ef9d710e029eaaed638ee0`

> **The suffix stops climbing here.** This network was minted as testnet-12 and relaunched under
> that name several times while its defects were being closed; none of those was ever a network
> anyone could join, and a suffix that increments per rebuild is a changelog wearing a network's
> name. It is testnet-11 now — nothing was running on 11 when it moved, measured across all four
> hosts — and `--netsuffix=12` is refused outright rather than aliased, because a node still
> configured for it should be told.
>
> Everything below the first table describes the same chain under its earlier names. The
> fingerprints quoted in those notes are the ones that were live at the time and are kept as a
> record of what moved and why, not as values to connect with. The only identity a joiner uses is
> the pair above.

> **Relaunched again 2026-08-22 with Phase 1** — the six builds the mainnet audit left open:
> the court's close binding and its tie to the disputed step, the pruning-point carriage, bond
> retirement, the challenger's stake, and the IBD fork-choice gate. `PALW_ATTEMPT_V2_VERSION`
> went 3 → 4 for the same reason it went 2 → 3: the close rules decide which `CourtClosed` objects
> apply, so two binaries that disagree must not share an identity. Fingerprint `048e6902…`.
>
> **Relaunched 2026-08-22 with the Phase 0 security fixes** (`75439aee…`, tree `e06d9409` plus the
> protocol-version bump). The fingerprint moved from `79a306ed…` deliberately: the rules changed,
> so `PALW_ATTEMPT_V2_VERSION` went 2 → 3 and the two networks no longer share an identity. A node
> built before that cannot peer with this one — which is the point, because it would otherwise pass
> the handshake and fork.

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
./target/release/kaspad --testnet --netsuffix=11 --utxoindex \
    --addpeer=169.58.39.220:26411 --addpeer=5.104.81.23:26411
```

The EVM lane is in the default build; add `--evm-rpc-listen=127.0.0.1:8545` for the Ethereum
JSON-RPC adapter. `--nodnsseed` is unnecessary and harmless: this network has no DNS seeders yet, so
peers come from `--addpeer` and the address manager.

**Check you are on the right network before anything else.** The startup banner prints
`Consensus params fingerprint: 048e69026e559e67584ded64f1b6279148e3459975ef9d710e029eaaed638ee0`.
A different value means a different ruleset, and the handshake will refuse rather than fork.
The genesis is `d25a80b9045abb97…`.

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
| **the exposure ceiling is gone, measured** | the chain crossed **611 concurrent claims** (`unresolved=611`) at DAA 771 with `exposure ceiling` holds: **0**. The previous binary's ceiling admitted exactly 601 and held there forever. This is the fix proven on a running chain rather than in a test |

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

---

## 2026-08-23 — going public: what a live network found that no test had

Five defect families, all of them measured on the fleet rather than reasoned about, and all
of them sharing one shape: **the symptom is silence.** Nothing errors, nothing retries,
nothing is logged. A component simply does not act, and "broken" is indistinguishable from
"has nothing to do".

### The two that keep a node out of its own network

`begin_decision` holds chain participation open while a proof-backed candidate is weighed.
It has one caller, and that caller hands off to a function with **no release path at all** —
so the hold was lifted only when a candidate FAILED. The ordinary outcome, this node's own
chain being heavier, leaked it every time. `consider_post_ibd_switch` now returns whether it
reserved an adoption, `#[must_use]`, and every other path releases.

Then the same node was still held, because `enter_candidate_review` sets its floor with
`fetch_max` and was called after **every** successful IBD — including the routine forward
syncs a node on a live chain performs constantly. Measured: a node holding 557 of the
chain's 558 blocks at load 0.4 ran 22 IBDs in 16 minutes, floor resetting to ~168s before
each expiry. `active_consensus_replaced` is the signal for "a different chain was adopted",
and `finish_ibd_after_success` was clearing it without reading it.

Why either one is a launch blocker and not a nuisance: a held node reports
`is_synced=false`. A DNS seeder gates on exactly that, and so does the explorer's DB filler
(`is_utxo_indexed and is_synced`). So a joiner pins itself, the seeder never advertises it,
and the network cannot grow reachable peers no matter how correctly DNS is configured.

### The one that loses transactions without an error

Every carrier a panel builds spends the previous one's change, so a panel's whole output is
ONE chain of dependent transactions, and a chain can only be mined in order. Nothing bounded
how far ahead it ran. A peer that has not seen a parent drops the child in relay, silently.

Measured: 791 carriers submitted, **zero** mempool refusals, 492 received by the producer,
302 mined — and of 300 `CourtOpened`, exactly one reached a block, while `ReceiptLicensed`
kept landing because those sat near the confirmed end of the chain. Capping the in-flight
depth turns silent loss into back-pressure, and because court moves are offered before
receipt quorums, the scarce slots go to the work that has a deadline.

### The one that makes funding unrecoverable

`resolve_fee_funding` knew two outpoints and both die. The configured one is a genesis
float, spendable once. The persisted one is the change of the last carrier submitted — a
promise the chain never made, since a carrier dropped in relay is an outpoint that will
never exist. After a rolling restart every seat filed receipts and every seat printed
`no fee UTXO resolves` forever.

Nothing needed remembering: every carrier pays change back to the same script, so the
panel's money is whatever the UTXO set holds under its own key. Recovery is now a pure
function of the chain and the keypair.

### The one that silences a seeder that could answer

`--anchors-only` serves the IPs an operator named, verified by TCP to the network's P2P
port. The co-located node could veto that — unreachable or `is_synced=false` returned before
a single anchor was checked. An anchor SERVER (a delegated nameserver host that hands out
entry points without being one) often cannot run a node of the network it serves at all:
`seeder3.misakascan.com` sits behind filtered egress. Under the old rule it could never
answer anything.

### And why all of this was hard to see

`object_name` had a `_ => "lifecycle object"` catch-all, so every court move logged under one
name. Six gates in the court arm `continue`d and five said nothing. An ACCEPTED object logged
nothing at all, so "this chain carries no courts" and "it carries them and something later
discards them" read identically. `retention_dir` was declared, documented with the
measurement that motivated it, wired through the daemon — and read by no code. There was no
`courts=` anywhere.

Every one of those is now named: per-variant object names, a per-tick stall summary with
reasons, a per-block record of what lifecycle traffic a block carried, and `courts=` on the
line an operator already watches.

### Operational note

Wiping a peered fleet host-by-host does not wipe the chain: whichever host has not been
wiped yet re-seeds it over IBD, and the "new" chain is the old one with its fee outpoints
already spent. Stop every node on every host, confirm zero, then wipe, then start.
