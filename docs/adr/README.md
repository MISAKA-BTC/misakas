# ADR index — what governs, and what was reversed

One list. Every ADR that has been superseded, in whole or in part, is named here together with the
ADR that superseded it and the clause that actually moved. If a decision you are about to rely on
is not in the "still governing" column, follow the arrow before you build against it.

The hazard this file exists to stop is real and has happened twice on this repo: an ADR number
reused by a non-ancestral lineage (ADR-0036 Decision 5), and two same-day ADRs claiming one number
(0035, then 0045). Reading by number alone gives you the wrong decision.

## The direction that governs the PALW lineage

**PALW is the consensus work, and PALW produces the blocks.** There is no hash lane on any PALW
network, no mixed-`algo_id` difficulty relation, and no hash floor to degrade to. Total PALW
unavailability halts the chain loudly rather than producing hash blocks. Liveness rests on
`PALW-BASE-0`, a portable integer-only class held permanently Active.

The load-bearing chain is **ADR-0038** (layer inversion) → **ADR-0039** (no hash path; Base class;
two-weight fork choice) → **ADR-0042** (one atomic mainnet-candidate ruleset) → **ADR-0043**
(state-root ordering) / **ADR-0045** (class economy as chain state) / **ADR-0046** (consensus-object
carriage). Anything that contradicts that chain is listed below as reversed.

## Superseded, in whole or in part

| ADR | What moved | Superseded by |
| --- | --- | --- |
| 0002 — ML-DSA-65 P2PKH | the signature scheme (the P2PKH structure stands) | [0019](0019-mldsa87-migration.md) |
| 0009 — DNS probabilistic finality | the voting-weight half | [0024](0024-verified-llm-token-weighted-bft.md) |
| 0012 — validator sortition commit-reveal | **the whole ADR** | [0017](0017-all-active-staker-attestation.md) |
| 0017 / 0018 §B | the voting-weight half | [0024](0024-verified-llm-token-weighted-bft.md) |
| 0021 — PALW LLM PoW (`algo_id = 4`) | reward / PoW **activation**; the Ollama output-text commitment was shown forgeable. The *lottery shape* is kept by 0038; the *verification shape* is replaced | [0026](0026-palw-v2-runtime-separated-verification.md), [0038](0038-palw-is-the-consensus-work.md) |
| 0026 / 0027 / 0028 | not reversed — **promoted**: the court, fraud proofs and sampling stop being credit machinery and become L1 machinery | [0038](0038-palw-is-the-consensus-work.md) |
| 0028 §4e — the credit-price remedy set (Remedy 1 rate cap, Remedy 2 subsidy fraction) | Remedy 1 was already recorded as non-existent at this panel; Remedy 2's variable — a subsidy fraction paid to an overlay job — has no referent once the block *is* the unit of credit. The admission quantity moves to a block-denominated per-class epoch budget; the `max_leverage` half moves to a per-**bond** exposure reserve | [0045](0045-palw-class-economy-on-chain.md) D2 (admission), [0042](0042-palw-mainnet-candidate-ruleset.md) D6 / P0-10 (leverage) |
| 0029 — V1 chain carriage | the Stage-1 shape is reused; the V2 object set replaces it | [0046](0046-palw-v2-consensus-object-carriage.md) |
| 0036 D2 — backup-lineage ADR-0041's mechanism (`palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale` / qwen-8.0 `mint.rs`) | **not adopted**, not ported. Two of that ADR's *conclusions* are adopted (new network identity; land→accept→mint) | — (0036 is itself the superseding record) |
| 0036 D4 — "mainnet MUST ship the permanent hash floor" | reversed: a lane that can always produce blocks is a permanent incentive to mine the lane instead of the work. The testnet half of D4 (no floor on TN11/devnet; a loud halt beats a silent fork) survives verbatim | [0039](0039-palw-only-block-production.md) D1/D2 (W6′) |
| 0037 D1 — PALW off the block-critical path (async model) | reversed the same day by the layer inversion. D2–D9 (the state machine and mint hygiene) are **carried**, re-seated under the new layer assignment | [0038](0038-palw-is-the-consensus-work.md) |
| 0038 W4, W6 | the last two hash paths to consensus participation: the anti-stall floor as a block-production path, and `spam_hash_work` as a fork-choice term | [0039](0039-palw-only-block-production.md) W4′, W6′ |
| 0039 D5 + its 2026-08-17 amendment | the epoch cap's **currency**: the amendment elected `pwu` and then proved in defect (e) that the election starves every above-mean class. Denominating in **blocks** closes (a)–(e) and unblocks the enforcement point that defect (c) had left open | [0045](0045-palw-class-economy-on-chain.md) D2 |
| 0039 D5 — share as a params constant | shares become chain state, granted at registration, conserved to 1000‰ | [0045](0045-palw-class-economy-on-chain.md) D3 |
| 0041 (live) D1 — sampled pruning-proof verification | withdrawn as unsound; verification is exhaustive and amortised | [0041](0041-palw-pruning-proof-verification.md) D1′ itself |

## Number hygiene — collisions and dangling references

| Number | What happened |
| --- | --- |
| 0003, 0008 | ADR-0019 links `0003-pq-address-format.md` and `0008-pq-genesis-premine.md`. Neither is resident; the numbers are held by [LtHash UTXO accumulator](0003-lthash-utxo-accumulator.md) and [Hash64 consensus identity](0008-hash64-consensus-identity.md) — different decisions. De-linked in place. |
| 0021, 0022 | ADR-0023 links `0021-fact-settlement-layer.md` and `0022-fsl-economic-design.md`. Neither is resident; the numbers are held by [PALW LLM PoW](0021-palw-llm-pow.md) and [pruned IBD / EVM overlay snapshot](0022-pruned-ibd-evm-overlay-snapshot.md). De-linked in place. |
| 0035 | Drafted twice the same day. The mainnet-activation draft was renumbered to [0036](0036-palw-mainnet-activation-model.md); [0035](0035-palw-public-testnet-strategy.md) is the public-testnet decision. |
| 0039–0048 | Held by the non-ancestral `main-backup-8107bfb-20260807` snapshot. Released to the live lineage by [ADR-0036](0036-palw-mainnet-activation-model.md) Decision 1; the live 0039–0046 are unrelated to the snapshot's. In particular the live [0041](0041-palw-pruning-proof-verification.md) (pruning-proof verification) is **not** the ADR-0041 that ADR-0036 supersedes. |
| 0045 | Drafted twice the same day on two branches — the class economy and the V2 consensus-object carriage. **Resolved 2026-08-20:** the class economy keeps [0045](0045-palw-class-economy-on-chain.md); the carriage ADR is [0046](0046-palw-v2-consensus-object-carriage.md). |

## Still governing, unreversed

0001, 0003–0008, 0010, 0011, 0013–0016, 0019, 0020, 0022–0025, 0030–0035, 0038 (as amended by
0039), 0039 (D1–D4, D6), 0040, 0041, 0042, 0043, 0044, 0045, 0046.

ADR-0007 (layered PoW) is unreversed and worth stating explicitly, because it is easy to read as
the hash-lane ADR: PALW is a Layer-1 `algo_id` variant, which is the extension point 0007 already
specifies — including the hard cut-off rule that two `algo_id` values never coexist on one network.
That rule is *why* ADR-0039 can say "no hash lane" without inventing a mixed-algo difficulty
relation.

Activation status is a separate axis from supersession and is not tracked here: most of the PALW
set is consensus-inert, with no shipped preset carrying `ConsensusV2`. See each ADR's own Status
line and [ADR-0042](0042-palw-mainnet-candidate-ruleset.md) for what a single atomic activation
must contain.
