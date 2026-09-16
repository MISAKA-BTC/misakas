# ADR-0126 — The validator overlay retires at a height, and PALW pays no validator

* Status: **IMPLEMENTED 2026-09-17, dormant** on `feat/palw-exec-lane-and-validator-retirement`.
  `Params::palw_validator_overlay_retirement` is `None` on every shipped preset, so no fingerprint and
  no block moves until a network names its height. Testnet-11's height is the operator's to choose.
* Operator's direction: "DNS/VLT の committee beacon の使用されてない経路 コードの削除も追加で完了して
  — これはバリデータに依存してる — PALW はバリデーターを巻き込まないように進めて — 現在の PoW を LLM に
  置き換えるの方針からずれているため使用しない".
* Builds on: [0009](0009-dns-probabilistic-finality.md) / [0018](0018-quality-gated-stakescore-inclusion-economics.md)
  (the overlay and its carve — what retires), [0022](0022-pruned-ibd-evm-overlay-snapshot.md) (the header's
  overlay root), [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the PALW worker carve and its escrow),
  [0109](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) (the bridge does not
  pause on overlay finality). Amends nothing below the height. Supersedes, at and past the height, the
  overlay's consensus role.

## 0. The sentence this ADR is

**The validator overlay stops being consensus at one height, and nothing below it changes.** Past
the height no block pays validators, carries an overlay transaction, commits an overlay root, locks
or slashes an overlay bond, or asks stake which tip to prefer; the coinbase pays each producer the
PALW worker carve and every fee, and the shares the overlay paid validators and includers are not
minted. Below the height every rule is byte-identical, so a chain that ran the overlay validates its
own history with the same binary that has retired it.

## 1. Why a height and not a deletion

Testnet-11 runs the overlay. Its tip at DAA 5,709 on 2026-09-17 carries six attestation-shard
transactions and a coinbase of seven outputs, validator rewards among them; the overlay's carve has
shaped every coinbase since genesis (worker base 62 %, inclusion 8 %, validator 30 % of the subsidy,
normal fees 90/10). An uncommitted cleanup deleted those rules outright. A node built from it
computes a different coinbase for every such block, refuses the attestation subnetwork, and — because
no fingerprint moved — still peers with the network it can no longer follow. That is a silent fork of
a live chain, which the project's doctrine forbids ("consensus changes by activation, not
regenesis").

So the rules that the chain's history depends on stay, gated below a height; the code that no shipped
network runs is what can be deleted outright (§4).

## 2. Decisions

**Decision 1 — one fence.** `palw_validator_overlay_retirement: Option<ForkActivation>`, a top-level
field hashed Some-only into the params id and the schedule id and visited as a fence, so a preset that
leaves it unset fingerprints exactly as before. `Params::palw_validator_overlay_retirement_fence`
answers it only where the network runs an overlay at all. It is resolved at each block's own DAA
score.

**Decision 2 — past the height the overlay reads as absent.** Every overlay gate in the virtual
processor already had an arm for a network without an overlay (`dns_params = None`). The retirement
is that arm, height-indexed: `dns_params_at(daa)` answers `None` at and past the height, and every
consensus site that took the overlay's params from the processor now takes them at the block's DAA —
the bond-spend view and the slashing side effects, the attestation eligibility and evidence checks,
the unbond authorisation, the compute-challenge slashes, the §E participation outputs, the quality
sub-pool, the audit fee and the reserve drip, the stake bond mutations, the overlay state and the
epoch accumulator, the template's mandatory deficits, the stake-preferred tip and the stake reorg gate
(the PALW deep-reorg authority is not the overlay's and keeps deciding).

**Decision 3 — the coinbase carves PALW.** `CoinbaseCarve` names the three carves a merged block's
reward can take: the whole reward (a network without an overlay), the overlay's split, and — past the
retirement on a PALW network — `PalwStateParamsV2::worker_carve(subsidy)` plus every fee. That is the
one function a claim's escrow is sized by, so the escrow the coinbase withholds and the share it carves
remain one number; the rest of the subsidy is not minted, exactly as the overlay's unspent validator
pool was not. There is no validator pool and no inclusion bounty past the height. A non-PALW network
that retires its overlay pays the whole reward.

**Decision 4 — the header commits no overlay.** Past the height a block's `overlay_commitment_root`
must be zero (`BadOverlayCommitment` otherwise); the template writes zero. The field stays in the
header and in its hash, so no identity moves.

**Decision 5 — overlay transactions are refused.** At and past the height the header-context
transaction rule — which both block validation and the mempool run, at the block's and the virtual
DAA respectively — refuses every overlay subnetwork (stake bond, attestation shard, slashing evidence,
unbond, precommit, compute objects) as `SubnetworksDisabled`. Nothing past the height would read one.
An overlay bond's collateral is an ordinary output from then on: no gate locks it and no evidence can
burn it.

**Decision 6 — the edges follow.** The EVM bridge treats overlay finality as fresh past the height (it
must not pause on a clock that has stopped); the mempool's coinbase settlement policy stops waiting on
an overlay confirmation; a pruning point past the height captures no overlay snapshot, and a pruned
sync whose witness block is past the height installs none.

## 3. What does not change

Every block below the height; the header layout and every hash; PALW's claim lattice, escrow and
payouts; fork choice among PALW chains; testnet-11 until its height is set.

## 4. What is deleted, and what waits

* **Deleted with this ADR** (no shipped network runs them): the in-node validator service and the VLT
  compute worker, the validator binary and its CLI wiring, the operator scripts and runbooks for
  running validators; and the ML-DSA-87 primitives PALW shares (key and signature lengths, the P2PKH
  script, the key id) move out of the overlay module into `mldsa87_primitives`, so PALW imports nothing
  from the overlay.
* **Waits for the height to be buried:** the rules below the height — the overlay carve and its
  payouts, attestation and evidence validation, the bond set, the overlay root, the stake reorg gate —
  and the stores they read. Once a network's pruning point is past its retirement height nothing a
  node validates reaches them; deleting them then is a code change with no consensus effect on that
  network, and a mainnet that retires the overlay from genesis never needed them.

## 5. Security amendments

* **SA-1 — a flag day, stated as one.** A node without this rule keeps paying validators past the
  height, so every node must run a build that schedules it before the chain reaches it. The fence
  moves the fork-id's schedule, which is what lets peers refuse a build that disagrees.
* **SA-2 — no new issuance.** The retired shares are not redirected: the PALW producer is paid what
  its escrow was always sized on, and fees, which the overlay split 90/10, now go wholly to the
  producer.
* **SA-3 — a straddling reorg is judged per block.** Every gate reads the block's own DAA score, and a
  selected chain's DAA scores never decrease, so a reorg across the height re-validates each block
  under the rule of its own side.
* **SA-4 — the pruned sync cannot be fed a forged overlay past the height** because none is installed
  there; below it the witness-root check is unchanged.

## 6. Tests

`adr0126_the_validator_overlay_retires_at_a_height` (an overlay-live hash network: the overlay's
worker share below the height and the whole subsidy past it, the overlay root committed then zero, a
funded stake bond admitted then refused by the mempool and in a block, a block committing an overlay
past the height disqualified, the chain going on) and `adr0126_a_palw_chain_crosses_the_retirement`
(a ConsensusV2 chain over the production overlay produces and validates attempt blocks across the
height, construction and validation agreeing on the PALW carve and the zero root).

## 7. Number hygiene

0126 was free on this branch when written (the README's residency line said so); the next free
number is 0127.
