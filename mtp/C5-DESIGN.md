# C5 under ConsensusV2 — what LLM mining earns, and how it is counted

Not an ADR. ADRs decide what the chain does; this decides what an off-chain scorer reads out of
what the chain already did. Nothing here is a consensus rule, nothing here needs a fence, and
getting it wrong costs a wrong leaderboard rather than a fork.

## Why the old design stopped working

C5's original shape was **k=2 replicas, exact match**: two workers ran the same job, the pair
agreed, and the matched pair was one creditable unit, deduplicated by execution nullifier. The
reader for it, `misaka mtp palw-leaves`, walks blocks for **algo-4 leaf registrations** and resolves
each leaf's Receipt-DA object from a spool.

ConsensusV2 has neither. Measured on testnet-11 on 2026-09-07: the last 400 blocks are algo 6 (340)
and algo 8 (60), no algo-4 at all, and no node on the fleet runs `--palw-da-import-dir`. Run against
the live chain the reader refuses outright — correctly, saying an empty result is not the same as
"no replica work". So C5 has a weight of 30 %, the largest of the five categories, and has never
scored anybody.

What replaced the replica pair is better evidence, not worse: a claim, a panel of five bonded seats
drawn by the chain, and a licence at three matching receipts.

## The one decision this design makes: both halves are paid, and separately

A replica pair had one kind of participant. ConsensusV2 has two, and both spend the GPU that C5
exists to buy:

* the **producer** ran the inference that made the block;
* each **panel seat** re-derived the same execution from the served material and signed a verdict.

Crediting only the producer prices verification at zero, and a class is minable only while at least
three seats hold its artifact — so the seats are the supply that keeps a class alive, not a
formality. Crediting them together in one pool makes the split invisible and hides which side is
short.

**C5 is therefore two sub-pools inside the one category, 70/30 producer/seat.** The producer's share
is larger because it carries the model and the failure risk (a voided claim burns its reward and
slashes); the seats' share is real because without three of them the class stops. The ratio is a
placeholder in exactly the sense the rest of `AllocationRules` is: it decides a share of a pool that
settles nothing until `c5_token_settlement_enabled()` opens.

## What is creditable

**One claim that reached `Final`.** Not `Provisional` (nothing has judged it), not `PanelBound`
(judgement is in flight), and not `ReceiptLicensed` (still challengeable — a court can void it).
`Final` is the chain's own statement that the work was done, judged, and no longer disputable, and
it is the same event the fold already pays the miner at.

A `Voided` claim credits nobody, on either side. That is not a penalty; a voided claim is work the
chain decided did not happen as claimed.

## How much one claim is worth

`work_leaves` on the claim record, not block count and not `pwu`.

* Block count would pay a 2-token attempt job the same as a 256-token free-prompt answer.
* `pwu` is the class's *price*, set by the share table and the retarget, so scoring by it lets the
  registry decide the leaderboard.
* `work_leaves` is what the execution actually walked. It is the closest thing on chain to "how much
  inference happened", and it is already the number the court would re-walk.

Each seat that filed a `Valid` receipt on a claim earns that claim's `work_leaves` into the seat
sub-pool. `Unavailable` and `Incapable` earn nothing — they are honest answers, and honest answers
about work you did not do are C3's business, not C5's.

## Attribution

Both sides identify by **bond**, and a bond resolves to an MTP identity the same way today's C1
does: `PalwBondKeyV2` → the bond record → `payout_payload` → the registered `addr:` id. Since the
2026-08-02 policy change any well-formed `addr:` id scores without registration, so this needs no
handshake.

* producer: `claim.bond`
* seat: `receipt.seat_bond` for each receipt whose verdict is `Valid`

A bond whose payout payload matches no known address still scores — under its own `addr:` id. The
leaderboard is allowed to contain strangers.

## Deduplication

The claim id. One claim is one unit forever, on both sides, and a seat is counted at most once per
claim however many receipts its bond filed.

This replaces the execution nullifier and is stronger: a nullifier stopped one computation being
*presented* twice, while a claim id is the chain's own key and cannot be minted twice at all. The
`global job-nullifier dedup` precondition is met by construction here, and should be recorded as met
rather than left open against a mechanism that no longer exists.

## The window

A claim counts in the epoch whose window contains its **`Final` DAA score**, converted to a
timestamp by the accepting block's header. Not `accepted_daa`: a claim accepted on Sunday and
finalized on Tuesday is work whose verdict landed on Tuesday, and scoring it in the earlier epoch
would score a judgement that had not happened when that epoch closed.

This means a claim near an epoch boundary lands in the later epoch. That is the conservative
direction — the alternative pays for work whose licence might still be voided.

## Caps, unchanged in spirit

The existing anti-farming levers apply to C5 as they do to every category, with one addition:

* the per-id epoch cap (`per_id_cap_bps`) applies to the C5 total, producer and seat summed;
* **a bond may not earn on both sides of one claim.** The chain already forbids it, and by three
  keys rather than one: `palw_panel_v2.rs`'s draw excludes the executor's bond, the executor's
  operator id AND the executor's pubkey, all read from one registry so no second namespace exists
  for them to diverge in. The reader still asserts it instead of trusting it — a rule that holds
  today is not a rule the scorer may assume tomorrow, and the assertion costs one comparison.

## What the indexer reads

Everything is already on chain and already reachable over the node's RPC:

| what | where |
|---|---|
| claim record — `bond`, `class_id`, `work_leaves`, `pwu`, `accepted_block`, `phase` | `claims` in the PALW state |
| the seats drawn | `panels`, keyed by claim id |
| the signed verdicts | `ReceiptLicensed { receipts: Vec<PalwSeatReceiptV2> }` — each carries `seat_bond`, `verdict`, `signed_daa` |
| a bond's payout address | the bond record's `payout_payload` |
| the timestamp of a DAA score | the accepting block's header |

So the reader is a walk over blocks collecting claims that entered `Final` in the window, plus the
receipts licensed on them. No DA spool, no Receipt-DA object resolution, no `--da-dir`. That whole
layer existed to make an algo-4 leaf trustworthy; here the chain's own phase transition is the
trust.

## What replaces `palw-leaves`

A new subcommand — `misaka mtp palw-claims` — emitting one JSONL line per finalized claim:

```
{ "network", "claim_id", "class_id", "work_leaves", "pwu",
  "final_daa_score", "final_at_ms",
  "producer": { "bond", "owner_address" },
  "seats": [ { "bond", "owner_address", "verdict" } ] }
```

and `misaka-mtp-service ingest-palw-claims` turning each line into facts. `palw-leaves` stays where
it is, refusing: it is the correct reader for an algo-4 chain and there is no reason to make it lie
about this one.

The hourly `mtp-collect.sh` gains one call. Its existing `ingest-chain` line is the model to copy —
same finality-buried walk, same idempotent re-ingest on overlap.

## What this does NOT change

`c5_token_settlement_enabled()` stays `false`. Three of its four preconditions are about what a
point is *worth*, and this design does not decide that. What it does is close the one precondition
that was about measurement — "k=2 replica exact-match passed" is now "the claim reached `Final`",
which is a stronger statement made by the chain rather than by a pair of workers.

`AllocationRules` stays outside `rules_hash`. Publishing a C5 total still promises no share.
