# ADR-0129 — A double spend needs the anchors, not the blocks

* Status: **ACCEPTED 2026-09-17, implementation in progress** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule and no fence of its own: it states which existing rules defend a payment, pins each
  with a test, and adds the read that counts confirmations in anchors. testnet-11 runs it from DAA 7,001
  with the execution lane at one block a second (ADR-0125 §7.5) and the seat exposure (ADR-0124) armed at
  the same height.
* Operator's direction, in the operator's words: "『モデルを交互にする』だけを二重支払い防止の主役にしない
  方がいい"; "Anchor Finality / Panel quorum certificate / Execution block の Finality weight = 0 / 同一 UTXO の
  競合を DAG 順序で一意化 / Producer・Panel equivocation slash / model・operator share cap / 高額決済の追加
  confirmation"; "10 BPS は UX/throughput、120 秒 PALW Anchor は security/finality と役割を完全に分ける";
  "これを ADR129 として記述して実装して — DAA 7001 から BPS1 になることを想定して進めている".
* Builds on: [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (execution blocks), [0127](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md)
  (the settlement anchor and its names), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (what a seat holds and loses), [0072](0072-the-ticket-is-the-execution.md) (an attempt is its header
  position), [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority).

## 0. The sentence this ADR is

**A payment is as final as the settled PALW anchors at or after the block that accepted it — never
the blocks — so a thousand execution blocks are zero confirmations; settling a conflicting branch takes
both the compute that produces its anchors and a licensing quorum of the seats drawn on it; spends that
conflict inside the DAG are ordered, not raced; and large payments wait for more anchors.** Alternating
models is not a defence: one operator can run every model.

## 1. The attacks

1. **The burst.** Produce many fast blocks and present them as confirmations.
2. **The private branch.** Pay a merchant on the public chain, build a branch that spends the same
   output elsewhere, and publish it once the merchant has delivered.
3. **The race.** Put two spends of one output into two blocks that arrive together.
4. **The double signature.** Sign two blocks for one permit, or two receipts that license two histories.
5. **The monopoly.** Hold enough of the lane to order its transactions at will.
6. **The large payment.** Any of the above, against a sum worth the attacker's capital.

## 2. Decisions

**Decision 1 — execution blocks carry no finality.** A round block (ADR-0125) is never a selected
parent, always red, outside the DAA set, of zero blue work and zero subsidy, and is never a claim; a
heartbeat is ADR-0066's. None of them moves a safe frontier, a safe weight, a live total, a blue work or
a DAA score, so no number of them is a confirmation. Pinned: merging permitted round blocks leaves the
sink's fork-choice inputs exactly as they are without them.

**Decision 2 — confirmations count settled anchors.** A PALW settlement anchor is a chain block carrying
an attempt; it is settled when its claim is `Final` (ADR-0127). A payment accepted by the chain block at
DAA `d` has **settlement depth** = the settled anchors at or after `d` on the node's chain.
`getPalwSettlement` (op 182) answers it with the anchors still pending; `misaka palw settlement --daa d
--min-depth N` prints it and exits non-zero until it is reached; `misaka wallet utxo list` shows each
output's depth. A thousand execution blocks and no new settled anchor is depth 0.

**Decision 3 — fork choice follows settlement.** The safe frontier (the deepest settled anchor) is the
comparator's first key and a deep reorg must strictly win the comparator (ADR-0042 Decision 9), so a
branch replaces the public chain only by settling deeper than it.

**Decision 4 — the panel quorum signs each anchor's state.** A seat's receipt signs the claim id; the id
is the attempt's; the attempt carries `challenge_v2(network, pre_pow_hash, timestamp, nonce, class,
bond)`, which admission checks against the carrying header; and `pre_pow_hash` covers the header's
transaction merkle root, accepted-id merkle root and UTXO commitment. A licensing quorum is therefore a
quorum over the anchor's transactions and the state they leave — at one block a second that is the
roughly 120 execution blocks the anchor merged at testnet-11's cadence — and it cannot be moved to a
block with other contents. Pinned (ADR-0127's binding test).

**Decision 5 — a settled double spend takes two resources.** To make a conflicting branch settle, an
attacker must (a) produce that branch's anchors — winning attempt draws, each an inference, because a
block's draw is its header position (ADR-0072) — and (b) license each of their claims with a quorum of
the seats drawn for it *on that branch*: honest seats judge only claims on the chain they follow, so an
unpublished branch settles only with three of five seats the attacker holds, claim after claim, until
its frontier passes the public one. The seats are drawn one ticket per bond above ten producer floors
(ADR-0124 Decisions 4–5; 100,000 MSK a seat on mainnet), so the second resource is bonded capital
across many bonds, and the draw's anchor on the branch is a block whose hash costs a winning draw to
re-roll.
*The limit, stated:* receipts for honest inferences on a private branch are not slashable on the public
chain — the seat capital is priced by the draw and the floor, not burned by it. What burns is a licence
of a bad inference (the court) and a seat that contradicts its quorum (three times the claim's exposure,
ADR-0124 Decision 3).

**Decision 6 — conflicting spends are ordered by the DAG.** A chain block accepts its mergeset's
transactions in GHOSTDAG order and the UTXO set admits the first valid spend; a round block's
transactions are accepted only through the chain block that merges it and only under a granted permit.
Two blocks carrying spends of one output produce one accepted spend. Pinned.

**Decision 7 — double signatures.** A permit signed twice is not relayed, is evidence, burns the permit
and slashes the bond's floor (ADR-0125 SA-2). A producer cannot sign one execution into two blocks: each
block is its own challenge and its own draw. A seat's receipts license one claim, which is one block
(Decision 4), so they cannot license two histories; contradicting verdicts are refused within one object
and unusable across objects (ADR-0124 SA-3) and are not slashed by name, because an honest seat can
sign both: the panel service answers a `(claim, bound_daa)` once but keeps that record in memory, so a
seat that answered `Unavailable` and restarted after the material arrived answers `Valid`, and a seat
redrawn onto a claim's second panel answers that panel afresh.

**Decision 8 — shares are capped, as auxiliaries.** A security domain holds at most 45 % of a span's
lane, a third of a round and never two consecutive rounds; an operator holds at most one permit a round
(tighter than the two in ten the design proposed). These bound a monopoly of ordering; they are not what
settles a payment (Decisions 2–5), and a model alternation would bind nothing an operator running every
model could not satisfy.

**Decision 9 — large payments wait for more anchors.** The depth a payment needs is its receiver's
policy, read from Decision 2. On testnet-11 past DAA 7,000 an anchor's claim settles a challenge window
(120 DAA) after its licence, so a payment's first settled anchor arrives some four hours after
acceptance at the 120-second cadence, and each later anchor adds one. A receiver of a large sum waits
for a depth whose anchors it would cost more to produce and license on a private branch than the sum is
worth; `--min-depth` is the switch a script or an exchange waits on.

## 3. What does not change

Every consensus rule, fingerprint and block; the lane's width (one permit a round on testnet-11 from DAA
7,001; nothing here depends on it); the DNS overlay, whose stake reorg gate (ADR-0128) can add a veto on
a network that runs it and is not needed for anything above.

## 4. Security amendments

* **SA-1 — depth is a lower bound beyond retention.** Settled claims retire from the state; past that
  horizon the read says the depth is at least what it counts.
* **SA-2 — the burst is inert by construction, and the pin is what keeps it so.** A change that gave an
  execution block blue work, a DAA score or a claim would turn Decision 1 off silently; the fork-choice
  pin fails first.
* **SA-3 — Decision 5 is only as strong as the draw is unpredictable.** On a private branch the panel's
  future anchor is the attacker's own block; re-rolling its hash is a new winning draw, which is compute
  per try, not a free nonce.
* **SA-4 — no validator is involved.** Nothing here reads the DNS overlay (ADR-0127's guard).

## 5. Tests

Execution blocks leave the fork-choice inputs unchanged (Decision 1); the binding chain from receipt to
UTXO commitment (Decision 4); two blocks spending one output accept one spend (Decision 6); the
settlement read, its RPC round trip and `--min-depth` (Decision 2); and the existing permit-equivocation,
one-ticket draw and frontier-first comparator tests (Decisions 3, 5, 7).
