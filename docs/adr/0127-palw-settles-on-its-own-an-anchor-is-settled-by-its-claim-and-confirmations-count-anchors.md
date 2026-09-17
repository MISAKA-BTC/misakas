# ADR-0127 — PALW settles on its own: an anchor is settled by its claim, and confirmations count anchors

* Status: **ACCEPTED 2026-09-17, implementation in progress** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule changes (§2 Decision 2 found the binding already in force), so there is no fence;
  what ships with testnet-11's DAA-7,001 build is the settlement read, the pins and the guard.
* Operator's direction, in the operator's words: "PALW は DNS finality や BFT バリデーター依存ではないことを
  明記して"; "PALW Final ≠ DNS Final / PALW Settlement Anchor ≠ DNS Anchor / PALW Panel ≠ DNS Validator を
  コード・型・ドキュメント上でも完全に分けるべき"; "PALW settlement path から dns_confirmation、validator、
  beacon が一切参照されないことを CI で保証する"; and the double-spend defence it asked to be written down
  and built: "10 BPS は UX/throughput、120 秒 PALW Anchor は security/finality と役割を完全に分ける" —
  execution blocks carry no finality, confirmations are counted in anchors, a panel quorum signs the
  anchor's state, conflicting spends are ordered by the DAG, equivocation is slashed, share caps are
  auxiliary, and large payments wait for more anchors.
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority),
  [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D2, [0072](0072-the-ticket-is-the-execution.md)
  (an attempt is priced at its header position), [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (execution blocks), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (what a panel risks). Stands beside [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md),
  whose veto it does not need.

## 0. The sentence this ADR is

**A PALW chain block that carries an attempt is a settlement anchor; it is settled when its claim is
`Final`, and so is everything its selected chain accepted at or below it; a transaction's security
is the number of settled anchors at or after the block that accepted it — never a count of blocks —
and nothing in that path reads a DNS confirmation, a validator or a beacon.**

## 1. The names, kept apart

| PALW | means | is not |
|---|---|---|
| **PALW Settlement Anchor** | a selected-chain block carrying an attempt (an attempt-class algorithm id); its claim is the anchor's claim | a DNS anchor (an epoch's canonical lagged block the overlay attests) |
| **PALW Settlement Finality** | the anchor's claim reached `Final`; the anchor is at or below the chain's safe frontier | DNS finality (a validator vote, ADR-0128) |
| **PALW Future Anchor** | the chain block at `accepted_daa + anchor_delay` whose hash seeds a claim's panel draw `H(anchor ‖ claim ‖ bond)` | a beacon (nothing is folded or attested) |
| **PALW Panel** | seats drawn from PALW bonds that re-execute a claim and sign receipts | a DNS validator |
| **settlement depth** | settled anchors at or after the accepting block | confirmations, blue score or DAA distance |

Execution blocks (ADR-0125 round blocks) and heartbeat blocks are never anchors: they add no PWU, no
blue work, no DAA score and no finality, however many there are.

## 2. Decisions

**Decision 1 — settlement is `Final`.** The safe frontier is the deepest anchor whose claim is `Final`
(`PalwChainStateV2::safe_frontier`); fork choice orders candidates by that frontier first
(`compare_palw_candidates_v1`), and a deep reorg must strictly win that order
(`decide_deep_reorg_v2`). A private branch cannot move its frontier past the fork point without claims
that panels licensed and that survived their challenge window on that branch.

**Decision 2 — the panel quorum signs the anchor's state, and already did.** A seat signs
`H(network ‖ claim ‖ verdict ‖ signed_daa)`, where `claim` is the attempt's id; the attempt id covers the
attempt's `challenge` (`attempt_id_v2` hashes the whole unsigned attempt); the challenge is
`challenge_v2(network, pre_pow_hash, timestamp, nonce, class, bond)` and stateless admission refuses an
attempt whose challenge is not its carrying header's (`ChallengeMismatch`); and `pre_pow_hash` covers
the header's transaction merkle root, accepted-id merkle root and UTXO commitment. So a licensing quorum
is a quorum over the anchor block's transactions and resulting state: the same attempt cannot be carried
by a block with other contents, and receipts cannot be replayed onto a branch that re-orders a spend.
This ADR pins that chain of bindings with a test rather than adding a field that would say it twice.

**Decision 3 — confirmations count settled anchors.** For a transaction accepted by the chain block at
DAA `d`, the node answers from its sink's PALW state: `settled` (the safe frontier's DAA ≥ `d`), and
`depth` = the number of `Final` attempt claims the state retains whose `accepted_daa ≥ d`, with the
anchors at or after `d` still pending and whether older anchors have retired from the state (in which
case `depth` is a lower bound). `getPalwSettlement` (op 182) serves it; `misaka palw settlement`
prints it and `--min-depth N` exits non-zero until it is reached (scripts and exchanges wait on it);
`misaka wallet utxo list` shows each output's settlement depth. A thousand execution blocks and zero new
settled anchors is depth 0.

**Decision 4 — conflicting spends are ordered, not raced.** Parallel blocks — two round blocks, a round
block and a chain block — may carry transactions spending one output. The merging chain block accepts
its mergeset's transactions in GHOSTDAG's order and the UTXO set admits the first valid spend only; a
round block's transactions are accepted only through its merging chain block and only under a granted
permit (ADR-0125 Decision 5). Pinned by a pipeline test.

**Decision 5 — equivocation.** A permit signed twice is burned and slashes the bond's floor (ADR-0125
SA-2). A producer cannot sign one execution into two blocks: each block is its own challenge and so its
own draw (ADR-0072). A seat's contradicting receipts are refused within one object and made unusable
across objects by the phase gate (ADR-0124 SA-3); a false licence is what the court and the dissent
slash price (ADR-0124 Decision 3) — and, by Decision 2, no receipt licenses a different block.

**Decision 6 — shares are capped, as an auxiliary.** One security domain holds at most 45 % of a span's
lane and a third of a round, never two consecutive rounds, and one operator one permit a round
(ADR-0125). These bound monopoly; they are not what makes a spend final — Decision 1 is.

**Decision 7 — the guard.** A test reads the PALW V2 settlement path's sources — `palw_state_v2`,
`palw_panel_v2`, `palw_attempt_v2`, `palw_admission_v2`, `palw_fork_choice`, `palw_fork_authority_v2`,
`palw_panel_economy_v1`, `palw_reward_v2`, `palw_producer_v2`, `palw_execution_lane_v1` — with comments
and string literals removed, and fails if any names `dns_finality`, `DnsParams`, `dns_params`, `DnsState`,
`dns_confirm`, `vlt`/`Vlt`, `StakeAttestation`, `StakeBond`/`stake_bond`, `ActiveBondView`,
`validator`/`Validator`, or `beacon`/`Beacon`. It runs in the consensus-core suite CI already runs.

## 3. What does not change

Every consensus rule, every fingerprint and every block. The DNS overlay, its stake reorg gate and its
BFT vote (ADR-0128) run beside PALW on networks that configure them; they can refuse a reorg and cannot
settle, select or order anything on PALW's behalf.

**PALW does not depend on DNS finality or on BFT validators.** A PALW network with no overlay, or an
overlay with no live validators, produces, licenses, finalizes, settles and orders exactly as described
here.

## 4. Where the separation is not yet complete (named, not hidden)

* The PALW **V1** lineage modules (`palw_credit`, `palw_facts`, `palw_job_panel`, `palw_carriage`,
  `palw_block_commitment`) still read overlay bond records. They are not on the V2 settlement path the
  guard covers; whether any network still validates history with them decides whether they are deleted.
* `palw_registry` derives a runtime class id with a helper that lives in `vlt.rs`, and kaspad's PALW
  producer, panel and round producer load their ML-DSA key with `kaspa-pq-validator-core`'s loader.
  Both are shared utilities under an overlay name, not dependencies on validators; moving them under a
  neutral name is housekeeping this ADR records.

## 5. Tests

The binding chain of Decision 2 (a header whose UTXO commitment or merkle root differs refuses the
attempt); settlement depth over retained claims, pending anchors and a retired horizon; the RPC's
round trip; the CLI's `--min-depth` exit; a pipeline test in which two permitted round blocks spend one
output and the merging block accepts exactly one; the guard.
