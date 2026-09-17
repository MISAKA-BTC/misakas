# ADR-0126 — The validator carve drops to a fifth, and the stake reorg gate stays

* Status: **REVISED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`.
  testnet-11 schedules it at DAA 6,001 (§6). The first version of this ADR (same day, never armed anywhere)
  retired the whole validator overlay at a height; the operator reversed that before any network
  scheduled it, and this text replaces it (§8).
* Operator's direction, in the operator's words: "DNS/VLT の committee beacon の使用されてない経路 コードの
  削除も追加で完了して — これはバリデータに依存してる — PALW はバリデーターを巻き込まないように進めて";
  then "DNS stake reorg gate は設計上残すように変更して — coinbase の validator 向け 30% から 20% にして
  DNS stake reorg gate は残して", with the freed tenth going to the PALW escrow and the retirement
  machinery deleted rather than kept dormant.
* Builds on: [0018](0018-quality-gated-stakescore-inclusion-economics.md) §F (the overlay's split),
  [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 10 (the PALW worker carve and its escrow),
  [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the escrow is split 80 / 20 at `Final`), [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md)
  (what the validators the gate needs now vote).

## 0. The sentence this ADR is

**From one height the validator pool is 20 % of a block's subsidy instead of 30 %, and the tenth it
gives up is escrowed for the PALW claim of the block that earned it — 72 % instead of 62 % — so it is
paid at `Final`, 80 % to the producer and 20 % to its panel; the overlay, its bonds, its attestations
and its stake reorg gate keep running, and nothing below the height changes.**

## 1. Why the overlay stays

The first version retired the overlay because PALW must not depend on validators. It does not: PALW's
claims, panels, `Final` and safe frontier read nothing of the overlay (ADR-0127 pins it). What the
operator keeps is the overlay's one consensus role that is not a dependency — the stake reorg gate, a
veto on reorgs past a validator-confirmed anchor, which ADR-0128 turns into a bonded-stake BFT vote.
A veto needs voters, so validators keep bonding, attesting and being paid, and a smaller share of the
subsidy pays them.

## 2. Decisions

**Decision 1 — one fence, two numbers.** `Params::palw_overlay_carve: Option<PalwOverlayCarveV1 {
activation, subsidy_validator_bps, worker_carve_permille }>`, a top-level fence hashed Some-only into
the params id and the schedule id with its two numbers beside the height, visited as a fence, named to
the fork-id gate as `palw_overlay_carve`, answered only on a `ConsensusV2` network that runs an
overlay. `None` on every shipped preset but testnet-11 (§6).

**Decision 2 — the overlay's split, from the height.** Where the full split is in force
(`DnsParams::reward_fee_split` at the block's DAA), past the fence the split's `subsidy_validator_bps`
is the fence's; the worker base, the split's primary, takes the remainder as it always has, so the
four parts still sum to the subsidy exactly. The bootstrap split, normal-fee and finality-fee splits
are untouched. Every reader of the split — the coinbase carve, `coinbase_validator_pool`, the quality
sub-pool, the audit fee and reserve drip — reads the one height-resolved split, on the build path and
the validation path alike.

**Decision 3 — the escrow grows with the worker base.** Past the fence a claim escrows
`⌊subsidy × worker_carve_permille / 1000⌋` of the block that carried its attempt, and the coinbase
withholds exactly that for the block — both resolved at **the lower of two DAA scores: the block that
carried the attempt, and the block whose coinbase pays it** (`palw_overlay_escrow_carve_at_v1`), so the
carve the fold records and the carve the coinbase withholds remain one number (ADR-0042 Decision 10,
B-1). For a block's own attempt the payer is its selected-chain child and the lower score is the attempt
block's. For merged work the lower score is what keeps a grown carve out of a split that has not been
lowered: nothing bounds a merged block's score by its merger's, and a 72 % escrow withheld from a 62 %
base would be released in full while only the base was withheld. ADR-0124's work price, buyback and
80 / 20 split read the escrow and follow it.

**Decision 4 — refused at start** (`validate_palw_v2`): a fence without `dns_params` or a V2 bundle; an
activation below `full_reward_split_daa_score`; `subsidy_validator_bps` above the network's full-split
share (the fence only lowers it); and `worker_carve_permille × 10` above the worker base the new split
leaves (`10,000 − inclusion − validator − service`) — the carve must fit the share it is carved from,
the invariant this repository already refuses a bundle on; and a non-zero
`min_slash_permille_of_escrow` beside the fence, because admission's collateral gate sizes the escrow
at the bundle's carve (zero, and so inert, on every bundle).

**Decision 5 — the retirement is deleted, not kept.** The first version's machinery — the
`palw_validator_overlay_retirement` fence, `dns_params_at`, the PALW coinbase carve, the refusal of
overlay and token transactions, the zero overlay root, the bridge's and the mempool's retirement arms,
the pruned-sync skip and the wallet's retirement-aware bond release — is removed. No network armed it,
so no fingerprint and no block moves.

## 3. What does not change

Every block below the height; the header and every hash; the overlay root; attestations, bonds,
unbonding and slashing; the inclusion bounty (8 %) and the fee splits; PALW's claim lattice and fork
choice.

## 4. What was deleted on 2026-09-17, and what came back

**Deleted, because no shipped network runs it** (every fence below is unset or `u64::MAX` on mainnet,
testnet-10, testnet-11, devnet and simnet; a `DnsParams` field is never removed — the struct is hashed
whole into the fingerprint — so where a field armed deleted code, node start refuses a network that
sets it):
* **PALW's dependence on validators**: the V1 fork choice (tip weights over the overlay's bond view),
  the V1 credit gate and its audit-call bond gate, the V1 equivocation and step-conviction slashes of
  overlay bonds, and their write-only indexes. The four V1 fences are refused at start. The ML-DSA-87
  primitives PALW shares moved to `mldsa87_primitives`.
* **Hard mandatory attestation inclusion**: the block rule, the template snapshot, the mining lane
  that covered deficits, and the miner's wait; an overlay whose inclusion fence is set is refused.
* **The legacy own-body bond spend gate**; an overlay whose mergeset gate starts later is refused.
* **The token overlay** (`89c8fb0d`): its fold, ledger store, emission settlement and reads; token RPCs
  answer `available: false`. Transfer and burn payloads keep their stateless check; any `tkn` fence is
  refused.
* **VLT voting weight and its shadow bookkeeping** (`4568b082`): the compute-weighted BFT round, frozen
  voting snapshots, finality certificates, the activation state machine, VLT credits and metrics — the
  "committee beacon" the operator named. `vlt_activation_daa_score` other than `u64::MAX` is refused.
  The compute overlay's consensus half (capability declarations, certificate resolution and committee
  draws, audit-fee outputs, challenge slashes) stays: testnet-10 and testnet-11 run its shadow, and
  their history needs it.
* **The window-bound inactivity leak** (`4568b082`), which could not be armed (ADR-0128 §1). ADR-0128
  re-implements a leak that reads the evidence it needs; `palw_inactivity_leak` stays refused.
* **The VLT compute worker** and the private-devnet switches that armed the VLT and token fences.

**Came back**, because validators keep attesting (§1): the in-node validator service, the
`kaspa-pq-validator` sidecar, `misaka validator` and the setup purpose, and the validator runbook —
without the compute worker, compute declarations or verdicts, and without VLT status.

## 5. Security amendments

* **SA-1 — a flag day, stated as one.** A node without the fence pays validators 30 % past the height
  and refuses every coinbase that pays 20 %; every node must run a build that schedules it. The height
  is its own (6,001 beside 6,000's flag day) so the fork-id gate separates the builds.
* **SA-2 — no new issuance.** The tenth moves from one line of the split to another and is paid, like
  the rest of the escrow, only if the claim reaches `Final`; a voided claim's escrow is never minted.
* **SA-3 — construction equals validation** because every split reader and both escrow sites resolve
  the fence at the same DAA score (the paying block's for the split, the lower of the attempt block's
  and the paying block's for the escrow).
* **SA-4 — the escrow fits its base in any DAG.** A lowered carve is resolved only where the paying
  block's own split is lowered, so `escrow ≤ worker base` is a property of the resolution, not of DAA
  scores growing along merges; pinned over every pair of scores around the height.

## 6. testnet-11

`palw_overlay_carve` = `{ activation: 6,001, subsidy_validator_bps: 2,000, worker_carve_permille: 720 }`:
worker base 72 %, inclusion 8 %, validator 20 %, service 0. At testnet-11's 4,445.62 MSK block the
escrow grows from 2,756.28 to 3,200.85 MSK. Scheduled in the preset on 2026-09-17 (`d6c46f25`, at DAA
≈5,773) with ADR-0124, ADR-0125 and ADR-0128 at the same height; fingerprint `4787b92a…`, re-pinned to
`ab4e7b9c…` when ADR-0130 joined the height the same day.

## 7. Tests

The split below and past the height, summing to the subsidy; the escrow and the withhold on both
sides of the height, and the carve fitting the payer's base for every pair of attempt and paying scores
around it, the inverted pair included; the refusals, the escrow-backing pair among them; Some-only
hashing and the schedule id; a pipeline chain crossing the height with template and validation agreeing
and the validator pool at 20 %.

## 8. Corrections

* **The first version retired the overlay.** It read "PALW must not depend on validators" as "no
  validator may be consensus". The operator's direction was narrower: PALW does not depend on them,
  and the stake reorg gate they vote for stays by design. The retirement was deleted before any network
  scheduled it; its deletion list (§4) was kept, and the validator's operational code it had taken with
  it was restored.
* **Decision 3 first resolved the escrow at the attempt block's own score** for merged work too, which
  holds the escrow inside its base only while a merged block's score stays at or below its merger's. The
  integration review found nothing enforcing that, and the resolution became the lower of the two scores
  before any network armed the fence. The same review found admission's escrow-backing gate reading the
  bundle's carve; it is inert at zero and now refused beside the fence (Decision 4).
