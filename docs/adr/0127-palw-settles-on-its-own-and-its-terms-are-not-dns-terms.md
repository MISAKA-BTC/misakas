# ADR-0127 — PALW settles on its own, and its terms are not DNS terms

* Status: **ACCEPTED 2026-09-17, implementation in progress** on `feat/palw-exec-lane-and-validator-retirement`.
  No consensus rule changes and no fence: this ADR names things, states a property the code already has,
  and makes CI keep it.
* Operator's direction, in the operator's words: "PALW は DNS finality や BFT バリデーター依存ではないことを
  明記して"; "PALW Final ≠ DNS Final / PALW Settlement Anchor ≠ DNS Anchor / PALW Panel ≠ DNS Validator を
  コード・型・ドキュメント上でも完全に分けるべき — Anchor Finality という言葉も避けて PALW Settlement
  Finality / PALW Settlement Anchor / PALW Future Anchor くらいに統一"; "PALW settlement path から
  dns_confirmation、validator、beacon が一切参照されないことを CI で保証する".
* Builds on: [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the one fork-choice authority),
  [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D2. The double-spend defence
  built on these terms is [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md); the DNS
  overlay that runs beside PALW is [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md).

## 0. The sentence this ADR is

**A PALW chain block that carries an attempt is a PALW Settlement Anchor; it is settled when its claim is
`Final`, and so is everything its selected chain accepted at or below it; that path — producer, claim,
future anchor, panel, receipts, court, `Final`, safe frontier, fork choice — reads no DNS confirmation,
no validator and no beacon, and a test fails the day it does.**

## 1. The names, kept apart

| PALW term | means | is not |
|---|---|---|
| **PALW Settlement Anchor** | a selected-chain block carrying an attempt (an attempt-class algorithm id); its claim is the anchor's claim | a DNS anchor (an epoch's canonical lagged block the overlay attests) |
| **PALW Settlement Finality** | the anchor's claim reached `Final`; the anchor is at or below the chain's safe frontier | DNS finality (a validator vote, ADR-0128) |
| **PALW Future Anchor** | the chain block at `accepted_daa + anchor_delay` whose hash seeds a claim's panel draw `H(anchor ‖ claim ‖ bond)` | a beacon (nothing is folded or attested) |
| **PALW Panel** | seats drawn from PALW bonds that re-execute a claim and sign receipts | DNS validators |
| **settlement depth** | settled anchors at or after the block that accepted a payment (ADR-0129) | confirmations, blue score or DAA distance |

"Anchor Finality" is not used. Execution blocks (ADR-0125) and heartbeats are never anchors.

## 2. Decisions

**Decision 1 — settlement is `Final`.** The safe frontier is the deepest anchor whose claim is `Final`
(`PalwChainStateV2::safe_frontier`); fork choice orders candidates by it first
(`compare_palw_candidates_v1`), and a deep reorg must strictly win that order (`decide_deep_reorg_v2`).

**Decision 2 — the terms are the documentation's and the operator surfaces'.** ADRs, RPC documentation
and the CLI say PALW Settlement Anchor, PALW Settlement Finality and PALW Future Anchor for these things,
and reserve "anchor", "confirmed" and "final" without the PALW prefix for nothing on the PALW path.

**Decision 3 — the guard.** A consensus-core test reads the PALW V2 settlement modules —
`palw_state_v2`, `palw_panel_v2`, `palw_attempt_v2`, `palw_admission_v2`, `palw_fork_choice`,
`palw_fork_authority_v2`, `palw_panel_economy_v1`, `palw_reward_v2`, `palw_producer_v2`,
`palw_execution_lane_v1` — with comments and string literals removed, and fails naming the file and line
if any mentions `dns_finality`, `DnsParams`, `dns_params`, `DnsState`, `dns_confirm`, `vlt`/`Vlt`,
`StakeAttestation`, `StakeBond`/`stake_bond`, `ActiveBondView`, `validator`/`Validator` or
`beacon`/`Beacon`. It passes on the tree it landed on, and CI runs the suite it lives in.

## 3. PALW does not depend on DNS finality or on BFT validators

A PALW network with no overlay, or an overlay whose validators have all stopped, produces, licenses,
finalizes, settles and orders exactly as it does with them. Where a network runs the DNS overlay, its
stake reorg gate — a bonded-stake BFT vote past ADR-0128's height — can refuse a reorg that abandons a
DNS-final anchor; it selects no tip, makes no claim `Final`, moves no safe frontier and decides no block's
validity, and it lapses when its validators stop reaching quorum. That veto is layered on PALW; PALW is
not layered on it.

## 4. Where the separation is not yet complete (named, not hidden)

* The PALW **V1** lineage modules (`palw_credit`, `palw_facts`, `palw_job_panel`, `palw_carriage`,
  `palw_block_commitment`) still read overlay bond records. They are not on the V2 settlement path the
  guard covers; whether any network still validates history with them decides whether they are deleted.
* `palw_registry` derives a runtime class id with a helper that lives in `vlt.rs`, and kaspad's PALW
  producer, panel and round producer load their ML-DSA key with `kaspa-pq-validator-core`'s loader.
  Both are shared utilities under an overlay name, not dependencies on validators; moving them under a
  neutral name is housekeeping this ADR records.

## 5. Tests

The guard and its comment-and-literal stripper.
