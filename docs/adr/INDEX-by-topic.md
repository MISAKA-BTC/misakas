# ADR index by topic

> 番号順の一覧は [README.md](README.md)、各 ADR の長い履歴は [HISTORY.md](HISTORY.md)、決定が実装・検証・有効化のどこまで進んだかは [../STATUS.md](../STATUS.md)。
> 1 つの ADR は主題を 1 つだけ選んで載せています(複数の話題にまたがるものは、その ADR の本文で確認してください)。† は同じ番号に 2 つのファイルがあることを示します([NUMBERING.md](NUMBERING.md))。
> **Accepted は「決定された」だけを意味します。** 実装済み・検証済み・有効化済みとは別です([../STATUS.md](../STATUS.md))。

## まず読む — PALW とモデルの規則

設計の前提は [../PRINCIPLES.md](../PRINCIPLES.md)(§8 暗号学的な計算証明を使わない、§9 モデルの二階層、§10 bond が採掘速度と報酬総額の上限を決める)です。ADR はその下にあります。

1. [ADR-0175](0175-registered-models-are-permanently-immutable.md) — New model registration is free; a registered model is permanently immutable. _(Status: Accepted design, 2026-10-09. preに独立fenceとfold/CLIを実装済み（休眠）。)_
1. [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md) — Bond bounds claim capacity, blocks, rewards and Final weight (the design premise for PALW economics). _(Status: Accepted as a design premise at the user's request, 2026-10-10)_
1. [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) — Model-bond coinbase allocation without model-availability consensus. _(Status: Accepted design direction at the user's request, 2026-10-10, after the Seeder/FPR amendments)_
1. [ADR-0179](0179-model-tiers-genesis-annual-1-0-registered-0-3.md) — Model tiers: genesis / annual-fork models 1.0, every later registered model fixed at 0.3. _(Status: Accepted(ユーザー決定 2026-10-15 第 4 回・第 5 回、判断記録 §7b・§7c)。)_
1. [ADR-0180](0180-palw-only-consensus-no-bft-validators.md) — PALW-only consensus: no BFT validators (user decision 2026-10-16). _(Status: Accepted(ユーザー決定 2026-10-16)。RFC-0012 の採用理由を記録する。)_

RFC 側の読む順は [../rfc/INDEX-by-topic.md](../rfc/INDEX-by-topic.md)(RFC-0016、RFC-0017、RFC-0018)です。

## Post-quantum crypto and UTXO / hash identity

- [ADR-0002 — ML-DSA-65 P2PKH as the only standard script](0002-mldsa65-p2pkh.md) — Accepted (Phase 1). Signature scheme superseded by ADR-0019 — ML-DSA-65 → ML-DSA-87, full PQ-only.
- [ADR-0003 — LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024)](0003-lthash-utxo-accumulator.md) — Accepted (Phase 1); REVISED (audit QM-1) — migrated to LtHash32_1024.
- [ADR-0004 — 64-byte UTXO commitment via a dedicated UtxoCommitment64 type](0004-utxo-commitment64.md) — Accepted (Phase 1 freeze; PR-7.6 introduced the type + finalize path) Date: 2026-05-28 Supersedes: —
- [ADR-0005 — Mass / DoS policy for ML-DSA P2PKH transactions](0005-mass-policy.md) — Accepted — PR-19-S7 (Phase 7) recalibrated mass_per_sig_op = 10_000 for ML-DSA-87 (Phase 1 froze the shape; the earlier ML-DSA-65 PoC set 6000)…
- [ADR-0008 — Full Hash64 consensus identity](0008-hash64-consensus-identity.md) — Accepted (Phase 9 design freeze; PR-9.1 — PR-9.3 land the foundations) Date: 2026-05-28 Supersedes: in part, ADR-0004…
- [ADR-0019 — Migrate Signature Scheme from ML-DSA-65 to ML-DSA-87](0019-mldsa87-migration.md) — Accepted (2026-05-30); Revised 1.1 — 2026-05-31 (scope expanded to PQ-only completion)

## Consensus / BlockDAG / clock / fork choice

- [ADR-0007 — Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 ASIC-hard tag)](0007-layered-pow.md) — Accepted (Phase 1 design freeze; PR-8.1 — PR-8.3 land the foundations).
- [ADR-0036 — PALW mainnet activation — lineage reconciliation and the model that governs](0036-palw-mainnet-activation-model.md) — Proposed (governance decision), with Decision 4's hash floor SUPERSEDED.
- [ADR-0037 — PALW asynchronous job state machine — hash-floor direction withdrawn](0037-palw-async-job-state-machine.md) — Superseded in part by ADR-0038 (same day).
- [ADR-0038 — PALW is the consensus work — sampled verification and a receipt-licensed weight ramp](0038-palw-is-the-consensus-work.md) — Accepted (architecture decision), amended.
- [ADR-0039 — PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice](0039-palw-only-block-production.md) — Proposed, with Decision 5 and its 2026-08-17 amendment SUPERSEDED IN PART by ADR-0045.
- [ADR-0042 — The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint](0042-palw-mainnet-candidate-ruleset.md) — Proposed. Activates nothing, moves no fence, changes no shipped preset.
- [ADR-0043 — PALW V2 state-root hash ordering (no challenge↔commitment cycle)](0043-palw-v2-state-root-ordering.md) — Accepted (2026-08-20)
- [ADR-0055 — Chain position is earned, and the question is set by the block](0055-palw-position-is-earned-not-declared.md) — Accepted
- [ADR-0058 — Merged work is counted — the mergeset carries claims, not just the chain](0058-palw-merged-work-is-counted.md) — Accepted
- [ADR-0060 — The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay](0060-the-liveness-doctrine.md) — Accepted as doctrine; implemented 2026-08-30, SHIPPED OFF the same day, and RE-IMPLEMENTED 2026-08-31 per ADR-0066…
- [ADR-0066 — The heartbeat lane out of header.bits, and the inactivity leak out of node memory](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) — Decisions 1, 2 and 4's fence LANDED (2026-08-31), both dormant.
- [ADR-0074 — The attempt is a claim, drawn by the chain](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) — PROPOSED (2026-09-02), decided by the user on two points that this ADR only spells out: (1) the attempt lane adopts the…
- [ADR-0076 — The attempt lane's seed is the retarget's own equilibrium](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) — ACCEPTED (2026-09-02), implemented for the Relaunch 5e re-genesis (every preset's fingerprint moves).
- [ADR-0083 — The difficulty window counts only rows priced by bits — heartbeat emitters are not work](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) — PROPOSED (2026-09-04), implemented on palw-daa-bits-priced-rows with the fence dormant on every preset
- [ADR-0105 — A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has landed](0105-a-heartbeat-never-turns-a-bonded-block-red.md) — PROPOSED 2026-09-11 on fix/heartbeat-trap-slow-producers (from main at a5f1bdf7).
- [ADR-0109 — A lock is its own claim, and finality is a label, not a pause](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) — PROPOSED 2026-09-11, at the operator's request ("今の EVM ブリッジや送金がスムーズに liveness に行かない根本的原因を分析して liveness を優先にする設計に修正を行なって")…
- [ADR-0129 — A double spend needs the anchors, not the blocks](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) — ACCEPTED and IMPLEMENTED 2026-09-17 on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0130 — BPS 1 is hardened before it is widened](0130-bps1-is-hardened-before-it-is-widened.md) — ACCEPTED 2026-09-17; Decisions 2–6 IMPLEMENTED the same day, Decision 1 built dormant, Decisions 7–8 not built — on…
- [ADR-0137 — A block buys one unit of work from any model, and a share is a result, not an input](0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement, design with a reproducible simulation…
- [ADR-0138 — The DAA score is the anchor's clock: a block advances it iff bits priced it](0138-the-daa-score-is-the-anchors-clock.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement
- [ADR-0140 — The heartbeat is the emergency generator: it must not touch the economy or the difficulty while the…](0140-the-heartbeat-is-the-emergency-generator.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0141 — Can an inference be the ticket without a hash lottery?](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0142 — The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not advance the…](0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0150 — The fingerprint must see the RULE, not only the height](0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) — ACCEPTED, 2026-09-20. Implemented behind no fence: the manifest is a description of the build, and adding it re-pins…
- [ADR-0165 — Useful work carries the clock: BASE-0 remains the idle-only bonded floor (A″), REAL attempts can…](0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) — Direction amended 2026-10-07 (revision 5, user decision).
- [ADR-0170 — The seed anchor is a window, not a span: a merged attempt anchors, the anchor survives span…](0170-the-seed-anchor-is-a-window-not-a-span.md) — PROPOSED 2026-10-03 on anchor/window (lane P2), for the DAA-5,300 flag day, the user's decision of the same evening…
- [ADR-0178 — The fork choice heals a partition on bonded participation: one search over every tip, economic keys…](0178-fork-choice-heals-a-partition-on-bonded-participation.md) — ACCEPTED — the user adopted rule E (2026-10-09).
- [ADR-0180 — PALW のみの合意:BFT の validator を使わない](0180-palw-only-consensus-no-bft-validators.md) — Accepted(ユーザー決定 2026-10-16)。RFC-0012 の採用理由を記録する。

## EVM and execution lanes

- [ADR-0020 — Selected-Parent EVM Execution Lane on L1](0020-selected-parent-evm-lane.md) — Accepted & implemented through design v0.4 (mergeset delayed acceptance)
- [ADR-0022 — Pruned-IBD support for the EVM lane and the DNS/PoS-v2 overlay](0022-pruned-ibd-evm-overlay-snapshot.md) — Accepted (implementing) — 2026-06-15 Supersedes: none.
- [ADR-0023 — MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM)](0023-base-three-lane-execution.md) — 不採用（将来の実装対象外）。
- [ADR-0089 — the fold is the truth; the EVM is its window and its hand](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) — PROPOSED 2026-09-05, design only (no implementation yet).
- [ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) — IMPLEMENTED 2026-09-17 on feat/palw-exec-lane-and-validator-retirement (from main at 6fdf6ba7, after ADR-0124)…
- [ADR-0127 — PALW settles on its own, and its terms are not DNS terms](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) — ACCEPTED and IMPLEMENTED 2026-09-17 on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0139 — O13 decided: the execution lane's gas throughput scales with the lane, one budget a round](0139-the-execution-lanes-gas-is-one-budget-a-round.md) — PROPOSED 2026-09-18 on feat/palw-exec-lane-and-validator-retirement
- [ADR-0168 — An execution block is a third class, and the chain reaches it through an anchor, never a parent](0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) — (superseded for the unified RFC8 EXEC lane by the amendment above): PROPOSED 2026-10-03, REVISED TWICE (2026-10-03 14:00, user)…
- [ADR-0169 — RFC8 uses the existing EXEC lane; the former algo-11 design is retired](0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) — SUPERSEDED DIRECTION, 2026-10-08.

## Network, IBD, operations and flag days

- [ADR-0001 — Network isolation from mainline Kaspa](0001-network-isolation.md) — Accepted (Phase 1) Date: 2026-05-28 Supersedes: —
- [ADR-0006 — RPC / WASM / SDK types for kaspa-pq](0006-rpc-wasm-sdk-types.md) — Accepted (Phase 7 scope freeze) Date: 2026-05-28 Supersedes: — Depends on: ADR-0002, ADR-0003, ADR-0004
- [ADR-0025 — Chain participation and IBD candidate selection](0025-chain-participation-and-ibd-candidate-selection.md) — proposed Date: 2026-08-09 Supersedes nothing.
- [ADR-0035 — The public PALW testnet is testnet-11, continued — and it pins its determinism class at the door](0035-palw-public-testnet-strategy.md) — Accepted (implemented 2026-08-17; operator items listed in §6)
- [ADR-0041 — PALW pruning-proof verification — exhaustive and amortised, not sampled](0041-palw-pruning-proof-verification.md) — Landed. Activates nothing on a shipped preset.
- [ADR-0063 — The operator's half of the protocol is missing, and one gap locks money in](0063-operator-tooling-the-missing-half.md) — Proposed (2026-08-30).
- [ADR-0115 — A pending transaction is announced until it lands](0115-a-pending-transaction-is-announced-until-it-lands.md) — PROPOSED 2026-09-11, from the operator's report ("また購入してもトランザクションやチャートに反映されてない 原因は" / "何回購入しても balance が減らない") and…
- [ADR-0122 — Mining is a purpose: an operator runs one command and reads one work id](0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) — PROPOSED 2026-09-12 on feat/adr-0122-operator-ux, at the operator's request: turn PALW mining from a set of components…
- [ADR-0136 — An artifact is mapped, not read, and a host holds one copy of it](0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) — PROPOSED 2026-09-17 on feat/palw-exec-lane-and-validator-retirement, with its first two decisions implemented the same…
- [ADR-0154 — testnet-12's first post-launch flag day: thirteen fences at DAA 750](0154-testnet-12-flag-day-daa-750.md) — Accepted and armed. The fences activated on public testnet-12 at DAA 750 on 2026-09-27 (about 07:29 JST), with release…
- [ADR-0155 — testnet-12's second post-launch flag day: two fences at DAA 1,300](0155-testnet-12-flag-day-daa-1300.md) — Accepted, armed at DAA 1,300. Release 587cab2b0, params fingerprint 24e1aec3….
- [ADR-0174 — Token name Misaka, ticker BILI; address prefixes stay unchanged](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) — Accepted naming and compatibility decision, 2026-10-07.

## DNS finality and validators (historical lineage; DNS-BFT is retired — see ADR-0180)

- [ADR-0009 — DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0010 — Validator Node Architecture (operational supplement to ADR-0009)](0010-validator-node-architecture.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety Operating Model](0011-validator-deployment-and-equivocation-safety.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0012 — Mainnet Validator Sortition via On-Chain Commit-Reveal](0012-mainnet-validator-sortition-commit-reveal.md) — Superseded — commit-reveal 案は 2026-05-30 に ADR-0017 で撤回。
- [ADR-0013 — Validator Reward Distribution](0013-validator-reward-distribution.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0014 — Coordinated-Failover Protocol for Validator Hosts](0014-coordinated-failover-protocol.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0015 — Remote-Signer / HSM Protocol for Validator Signing](0015-remote-signer-hsm-protocol.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0016 — Stake-locked bond UTXOs](0016-stake-locked-bond-utxos.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0017 — All-Active-Staker Attestation (Remove Sortition Committee)](0017-all-active-staker-attestation.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0018 — Quality-Gated StakeScore + Inclusion Economics (BFT-free)](0018-quality-gated-stakescore-inclusion-economics.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0024 — Verified LLM Token-Weighted BFT for DNS finality](0024-verified-llm-token-weighted-bft.md) — Retired — VLT validator 投票重み・compute overlay は廃止済み。
- [ADR-0126 — The validator carve drops to a fifth, and the stake reorg gate stays](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0128 — DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) — 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。
- [ADR-0134 — The compute overlay's committee beacon retires at a height, and its machinery goes](0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) — ACCEPTED and IMPLEMENTED 2026-09-17 on feat/palw-exec-lane-and-validator-retirement.

## Economics, rewards, bonds and the Position market

- [ADR-0032 — PALW fee-bond escrow — pricing calls and paying challengers without new covenants](0032-palw-fee-bond-escrow.md) — Accepted (design; activates nothing).
- [ADR-0033 — PALW V1 credit overlay（不採用）](0033-palw-credit-gate-wiring.md) — Withdrawn — ADR-0037 / ADR-0038 により旧 credit overlay 方針を撤回。
- [ADR-0045 — The class economy is chain state — derived PWU, block-denominated epoch budgets, and the…](0045-palw-class-economy-on-chain.md) — Accepted (implemented on the V2 lineage; consensus-inert — no shipped preset carries ConsensusV2).
- [ADR-0054 — A class's cadence share follows its own production](0054-palw-share-follows-production.md) — Accepted. Adds the share-raise path ADR-0045 deferred ("automatic share re-allocation from class health") and closes…
- [ADR-0056 — Permissionless class admission, and the share economy that survives it](0056-palw-permissionless-class-admission-and-share-economy.md) — Accepted and implemented (2026-08-27).
- [ADR-0059 — The 10B premine cap — genesis mints one number, everything else is a carve](0059-the-10b-premine-cap.md) — Accepted
- [ADR-0061 — Zero-seat genesis, and collateral sized by arithmetic instead of history](0061-zero-seat-genesis-and-right-sized-collateral.md) — Accepted; implemented 2026-08-30 (palw-genesis-10b-cap branch, same re-mint train as ADR-0059/0060)
- [ADR-0064 — Trustless recovery from a total producer stop: the bond becomes usable in the block that registers…](0064-trustless-recovery-from-a-total-stop.md) — Partially superseded by its own correction (2026-08-30).
- [ADR-0065 — A bond must be earned, and a failure is not a verdict](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) — D1, D2 and D4 LANDED (all dormant)
- [ADR-0068 — The LLM-primary economy — the floor retires to the doctrine's minimum](0068-the-llm-primary-economy-and-the-floors-minimum.md) — Accepted; Phase 1 implemented AND drilled 2026-09-01.
- [ADR-0071 — The attempt lane's price, the ticket's bound, and who may judge a class](0071-the-attempt-lanes-price-and-the-tickets-bound.md) — IMPLEMENTED WITH DECISION 1 WITHDRAWN (2026-09-02; proposed 2026-09-01).
- [ADR-0072 — The ticket is the execution: both lotteries priced in inferences](0072-the-ticket-is-the-execution.md) — IMPLEMENTED (2026-09-02); Decision 7's rollout DECIDED the same day — see §3.
- [ADR-0073 — Real-demand work bears the weight](0073-real-demand-work-bears-the-weight.md) — PROPOSED (2026-09-02). Decision 2 is DECIDED
- [ADR-0087 — a position is bought from the curve and sold back to it](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) — PROPOSED 2026-09-05, design only (no implementation yet).
- [ADR-0090 — The pair is seeded with real BILI, locked for good, and a position is whole](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) — PROPOSED 2026-09-05; IMPLEMENTED 2026-09-05 on palw-adr0088-0089-impl (§8)
- [ADR-0091 — The reward buys the pair, and no holder is paid](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) — PROPOSED 2026-09-06 (design first, at the operator's word; the implementation follows on palw-adr0088-0089-impl and is recorded in §8 when it lands)…
- [ADR-0092 — The ladder is minted once, and the wall clock is what binds](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) — ACCEPTED 2026-09-06 (design first, at the operator's word; §8 records the implementation)
- [ADR-0094 — A seed is paid in as many transactions as it takes](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) — PROPOSED 2026-09-07; IMPLEMENTED 2026-09-07 (§8)
- [ADR-0095 — A position is a membership, not an income](0095-a-position-is-a-membership-not-an-income.md) — PROPOSED 2026-09-07
- [ADR-0107 — A class's share grows on work that reached Final, not on blocks that were accepted](0107-a-share-grows-on-work-that-reached-final.md) — PROPOSED 2026-09-11 on fix/share-growth-counts-final-work (from main at 3c78e898).
- [ADR-0114 — The owner's leg is five percent, and it arrives at a height](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) — PROPOSED 2026-09-11 at the operator's request ("追加で現在の売買の burn 5% 開発者への手数料 1% から burn 5% 開発者への手数料 5% にあげて つまり売って買うと 20% は fee として持ってかれるようにして")…
- [ADR-0120 — The least seed is one million BILI, and it arrives at a height](0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) — PROPOSED 2026-09-12 at the operator's request ("misaka position のモデル追加の際に流動性として ロックして引き出せないようにする misaka の数を 1M として 今の 100000 枚から引き上げて")…
- [ADR-0123 — The epoch progressively releases unused class budget](0123-the-epoch-progressively-releases-unused-class-budget.md) — PROPOSED and IMPLEMENTED 2026-09-12 on feat/adr-0123-epoch-budget-release, at the operator's request after the…
- [ADR-0124 — The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid for the…](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) — PROPOSED and IMPLEMENTED 2026-09-17 on feat/adr-0124-panel-reward-and-compute-weight (from main at 6fdf6ba7), at the…
- [ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) — PROPOSED 2026-09-17; Decisions 1 and 2 IMPLEMENTED the same day, in shadow on…
- [ADR-0132 — What a model is actually paid per forward it ran, and why the gap is liveness before it is price](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) — PROPOSED 2026-09-17; the end-to-end shadow (§6) built the same day on feat/palw-exec-lane-and-validator-retirement.
- [ADR-0144 — PALW pays for the inference you were going to run anyway](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) — CONSTITUTION, 2026-09-19. This ADR fixes what PALW is FOR.
- [ADR-0146 — A coefficient is justified by the arbitrage it permits, not by being right](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) — SEARCH IMPLEMENTED 2026-09-21.
- [ADR-0151 — Liveness is structural; collateral covers fraud](0151-liveness-is-structural-collateral-covers-fraud.md) — D2, D3, D4 and D5 in force on testnet-12 from genesis
- [ADR-0152 — A bond is standing stake, a claim reserves until its licence, and a reward vests until its…](0152-account-stake-staged-reserve-and-vested-rewards.md) — Accepted for testnet-12 by the operator, 2026-09-24 (v3.1, with its 13 post-edits and the integration amendments IA-1…IA-15)…
- [ADR-0160 — Claim capacity is separated from collateral price (v3): the bond is one shared guarantee split four…](0160-claim-capacity-separation.md) — PROPOSED v3 (2026-09-26 night).
- [ADR-0161 — A claim is not an emission: multi-claim carriage under a fixed per-DAA reward budget](0161-a-claim-is-not-an-emission.md) — Proposed (rcore/cap-s1, stage 5 of the user's capacity plan).
- [ADR-0162 — The pair opens on a virtual reserve](0162-the-pair-opens-on-a-virtual-reserve.md) — PROPOSED 2026-10-01 at the operator's request
- [ADR-0167 — The ×1000 capacity package: a fixed per-DAA PALW reward budget, riders, a lower-only breaker, and…](0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md) — implemented on rcore/cap-1000 (testnet-12 only, int-11 release, H = 5,300).
- [ADR-0176 — Bond bounds PALW claim capacity, blocks, rewards and Final weight](0176-bond-bounds-palw-production-rewards-and-final-weight.md) — Accepted as a design premise at the user's request, 2026-10-10.
- [ADR-0177 — Model-bond coinbase allocation without model-availability consensus](0177-model-bond-allocation-without-availability-consensus.md) — Accepted design direction at the user's request, 2026-10-10, after the Seeder/FPR amendments.

## PALW verification: court, fraud proofs, determinism and adjudication

- [ADR-0021 — PALW LLM proof-of-work (algo_id = 4/5), at one block per 120 s](0021-palw-llm-pow.md) — Superseded — 旧 algo 4/5 の報酬・PoW 有効化方針は撤回。
- [ADR-0026 † — Current PALW architecture on testnet-11](0026-current-palw-testnet11-architecture.md) — Accepted
- [ADR-0026 † — PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof](0026-palw-v2-runtime-separated-verification.md) — Accepted (architecture). Restored in full 2026-08-26
- [ADR-0027 — PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal](0027-palw-slash-unilateral-fraud-proofs.md) — Accepted (architecture); PROMOTED by ADR-0038 from credit machinery to L1 machinery.
- [ADR-0028 — PALW challenge sampling — a scheduler for re-execution, never a verdict](0028-palw-challenge-sampling-protocol.md) — Accepted (architecture), with the §4e remedy set SUPERSEDED on the V2 lineage.
- [ADR-0029 — PALW chain carriage — the objects ride the rails the fork already built](0029-palw-chain-carriage.md) — Proposed (draft for review). Activates nothing.
- [ADR-0030 — The PALW step function, pinned at tile granularity — shape profile v3](0030-palw-step-function-shape-profile.md) — Accepted (schema frozen; every class-specific value is registration-measured).
- [ADR-0031 — Canonical transcendentals — exp and log are algorithms, not functions](0031-palw-canonical-transcendentals.md) — Accepted (programs land consensus-inert; every binding is registration-measured).
- [ADR-0034 — PALW re-verification routing — four execution-class families, five model bands, one deciding binding](0034-palw-execution-class-model-band-routing.md) — Accepted (architecture; activates nothing).
- [ADR-0040 — PALW-BASE-0 — the integer-only arithmetic normative specification](0040-palw-base-0-integer-arithmetic.md) — Proposed. Activates nothing and registers no class.
- [ADR-0044 — Free-prompt PALW — the user's own inference becomes the consensus work, certified before it mines](0044-palw-free-prompt-receipts.md) — Proposed. Activates nothing, moves no shipped preset, changes no live network.
- [ADR-0046 — PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts ride their…](0046-palw-v2-consensus-object-carriage.md) — Accepted (2026-08-20)
- [ADR-0049 — The adjudication contract — what a court opens, and the bound that makes it model-size-independent](0049-palw-adjudication-contract.md) — Proposed. Activates nothing, registers no class, and changes no proof-of-work.
- [ADR-0050 — The BASE-0 residual site — the narrowing that was never declared, and the amplification that was](0050-palw-base0-residual-site.md) — Proposed. Changes no op, amends no catalog, and activates nothing.
- [ADR-0051 — The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified beside…](0051-palw-metal-gguf-execution-family.md) — Superseded — 2026-08-26 に ADR-0053 で撤回。
- [ADR-0052 — PALW-QWEN36 — the integer arithmetic for Qwen3.6's hybrid graph](0052-palw-qwen36-hybrid-class.md) — Proposed. Registers no class and activates nothing.
- [ADR-0053 — One execution family — Family M is withdrawn, and the court is not optional](0053-palw-one-execution-family.md) — Accepted. Supersedes ADR-0051.
- [ADR-0057 — BASE-0 runtime acceleration — backends below the semantic boundary](0057-palw-base0-runtime-acceleration.md) — Accepted. Consensus-inert: no class id, no catalog entry, no ruleset field and no fingerprint moves under anything this…
- [ADR-0062 — The data-availability court: stop a vote from taking a bond](0062-data-availability-court.md) — Implemented behind Params::palw_da_court, dormant (the amended form landed 2026-09-02; SA-7 widened the same fence 2026-09-03 — see the implementation sections below)…
- [ADR-0069 — End-to-end adjudicability is the price of weight](0069-e2e-adjudicability-is-the-price-of-weight.md) — ACCEPTED — IMPLEMENTED (2026-09-01; reviewed and amended 2026-09-02).
- [ADR-0070 — The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them](0070-the-model-tiers-step-spaces-are-adjudicable.md) — ACCEPTED (implementation on palw-step-space-e2e; arming is a deployment decision, §7) Date: 2026-09-01…
- [ADR-0075 — Certification is a consensus object](0075-certification-is-a-consensus-object.md) — PROPOSED (2026-09-02), implemented on branch palw-adr0073-fp-weight for the Relaunch 5e re-genesis (the state version and the genesis free-prompt set move, so the fingerprint moves)…
- [ADR-0077 — A prompt a person would type is a claim the court can try](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) — PROPOSED (2026-09-02). Written against a measurement, not a design review: the network's stated purpose is a person…
- [ADR-0078 — What was made from it is committed; the thing itself never rides](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) — PROPOSED (2026-09-02). Written to draw a line ADR-0077 needed drawn: implementing ADR-0077 makes an inference at a…
- [ADR-0079 — A pure function needs no permissions — the sandbox is for the host, and the chain never takes its…](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) — PROPOSED (2026-09-02). Written against a proposal that ADR-0078 is not enough to run a local LLM in practice, and that…
- [ADR-0080 — The answer is long; the verified unit is short](0080-the-answer-is-long-the-verified-unit-is-short.md) — Superseded in part — 分割 claim 案は 2026-09-03 に撤回。
- [ADR-0081 — Long context — the input is a state chain](0081-long-context-the-input-is-a-state-chain.md) — Superseded in part — prefill 分割案は 2026-09-03 に撤回。
- [ADR-0082 — The close is flat in the context — attention is refuted by dissection, the capture is a fold, and…](0082-the-close-is-flat-in-the-context.md) — IMPLEMENTED (2026-09-04) on palw-adr0082-impl, pending the testnet-11 Relaunch 5f cut.
- [ADR-0084 — The ids ride, the capture stays home — a model-class claim serves its answer, never its history](0084-the-ids-ride-the-capture-stays-home.md) — PROPOSED (2026-09-04), implemented on palw-adr0084-served-answer (see §7 for what landed and what did not).
- [ADR-0085 — The close is assembled from what the executor served — a disputed tile, not a capture](0085-the-close-is-assembled-from-what-was-served.md) — PROPOSED (2026-09-04). §6 items 1–3 LANDED 2026-09-04 (see §7)
- [ADR-0086 — the opening carries the fold, not the leaves](0086-the-opening-carries-the-fold-not-the-leaves.md) — PROPOSED 2026-09-05, implemented consensus-inert on palw-adr0084-served-answer (no fence; the fingerprint does not move)…
- [ADR-0093 — The court can try a fused row; the responder is what is missing](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) — PROPOSED 2026-09-06 (design only).
- [ADR-0098 — The panel's coverage is a number, and a seat that found a lie files nothing else](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) — PROPOSED 2026-09-10. Decisions 1–3 IMPLEMENTED the same day (§9): Decision 1 is a report over the draw the seats run
- [ADR-0099 — The adder measures, the chain recomputes, and a seat holds a shard](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) — PROPOSED 2026-09-10. Decisions 1–4 and 6 IMPLEMENTED the same day, consensus-inert (§9): types, pure functions, a…
- [ADR-0101 — A membership is proven by the chain and served by anyone, and a Position never moves between holders](0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) — PROPOSED 2026-09-10. Decisions 2, 3 and 5 IMPLEMENTED the same day, consensus-inert (§9): which grant needs whom, the…
- [ADR-0102 — The embedding lift is read per token, and a kernel a network has not armed is not in its identity](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) — PROPOSED 2026-09-10 on feat/adr-0099-sharded-seat
- [ADR-0103 — The context is held off the chain, and the chain carries a root, an opening and a logarithm](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) — PROPOSED 2026-09-11 on docs/adr-0103-the-context-is-held-off-the-chain (from feat/adr-0099-sharded-seat at 3e9c3b59)
- [ADR-0106 — An inventory is a stream of leaves, not a copy of the model](0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) — PROPOSED 2026-09-11 on feat/adr-0099-sharded-seat
- [ADR-0108 — An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote](0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) — PROPOSED 2026-09-11, at the operator's request ("このような拡張をパーミッションレスで行える 基盤を作成するADRを作成してから実装して").
- [ADR-0111 — A seat may demand the committed leaf it needs to judge](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) — PROPOSED 2026-09-11 on feat/adr-0103-held-context (continuing ADR-0103 at 36f115c4), written from the operator's…
- [ADR-0116 — An attention history is the class's, and the held regime reduces over its own width](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) — PROPOSED and IMPLEMENTED 2026-09-11 on feat/adr-0103-held-context, at the operator's instruction.
- [ADR-0117 — A draw is one forward](0117-a-draw-is-one-forward.md) — PROPOSED and IMPLEMENTED 2026-09-11 on feat/adr-0103-held-context, at the operator's instruction.
- [ADR-0118 — The held regime arrives at a height, and a held class carries its own prompt form](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) — PROPOSED and IMPLEMENTED 2026-09-12 on feat/adr-0103-held-context, at the operator's instruction…
- [ADR-0119 — A held class is walked at the regime's ladder, and the chain records which classes those are](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) — PROPOSED and, for its consensus half, IMPLEMENTED 2026-09-12 on feat/adr-0103-held-context, at the operator's…
- [ADR-0121 — A held capture is served from its fold as the replay streams, and a node holds two ladders](0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) — PROPOSED and IMPLEMENTED 2026-09-12 on feat/adr-0103-held-context, at the operator's instruction…
- [ADR-0133 — Verification is its own clock: a class verifies over spans, and a starved class stops only itself](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) — PROPOSED 2026-09-17; the profile, the capacity arithmetic, the simulation and the observability built in shadow the…
- [ADR-0147 — Independence is drawn, not declared](0147-independence-is-drawn-not-declared.md) — IMPLEMENTED behind Params::palw_admission_independence, which is one of the three fences of the ADR-0145 economic…
- [ADR-0148 — The free-prompt lane prices compute, and prices it the same for every class](0148-the-free-prompt-lane-prices-compute.md) — IMPLEMENTED behind the ADR-0145 economic bundle: Params::palw_canonical_work sets the UNIT (a free-prompt claim accepted at or past it is priced in compute)…
- [ADR-0149 — An attempt's pwu is the derivation, so the weight reads it directly](0149-an-attempts-pwu-is-the-derivation.md) — IMPLEMENTED behind Params::palw_canonical_work, one of the three fences of the ADR-0145 economic bundle…
- [ADR-0153 — (draft) — 2M verification by shards: the implementation plan](0153-2m-verification-sharding-plan.md) — Draft plan (rcore/cap-s1, stage 7 of the user's capacity plan, 2026-09-27).
- [ADR-0166 — Verifier unavailability is not producer fraud](0166-verifier-unavailability-is-not-producer-fraud.md) — PROPOSED 2026-10-03 at the operator's request
- [ADR-0171 — Probabilistic constraint checks are the normal large-model verifier; the court resolves disputes…](0171-probabilistic-constraint-checks-and-court-on-dispute.md) — Design direction selected at the operator's request, 2026-10-06.
- [ADR-0172 — Model extensibility uses versioned kernels, not a universal VM](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) — Design direction selected by the operator, 2026-10-06.
- [ADR-0173 † — A seat holds a root, not a class: possession is keyed (bond, class, root), a new root is NotReady…](0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) — PROPOSED 2026-10-04 on rcore/post5300-audit (lane MU), the user's request after the DAA-5,300 release.
- [ADR-0173 † — MISAKA's purpose: public-verifier dispute completeness](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) — Accepted as the project's design mandate at the user's request, 2026-10-07.

## Model onboarding, registry and tiers

- [ADR-0047 — The A16 activation tier — sixteen-bit activations for the classes int8 cannot carry](0047-palw-a16-activation-tier.md) — Accepted (2026-08-21). Amends ADR-0040's catalog by addition
- [ADR-0067 — Classes are chain data; only kernels are the build](0067-classes-are-chain-data-kernels-are-the-build.md) — Decisions 1–3 and 5 LANDED for the dense (A16) container (2026-08-31), fenced and dormant
- [ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps publishing](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) — PROPOSED 2026-09-05, revised the same day — design only (no implementation yet).
- [ADR-0096 — The app you already use is the entrance, and the shape of the answer is committed](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) — PROPOSED 2026-09-10. Part A (the entrance) and Part C (distribution, settings, the model-request intake) are host-side…
- [ADR-0097 — A model's fit is a lookup, and the entrance says its limits before the first token](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) — PROPOSED 2026-09-10; IMPLEMENTED the same day, consensus-inert (§9).
- [ADR-0100 — A model is data: the one-move court, the held measurement and the licence per shard, and the…](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) — PROPOSED 2026-09-10. Decisions 1, 2, 3, 4 and 6 IMPLEMENTED the same day, consensus-inert on every shipped preset (§9)…
- [ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) — PROPOSED 2026-09-11 on feat/adr-0103-held-context, written from the operator's decision of the same day (ADR-0103 §10.5 answered by ADR-0111; this is the other half of that answer)…
- [ADR-0112 — A class's weights are read within a budget the operator states, and the budget is a fifth of the…](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) — PROPOSED 2026-09-11 on feat/adr-0103-held-context, from the operator's decision of the same day after the testnet-11…
- [ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its…](0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) — PROPOSED 2026-09-17; the rule set built in shadow the same day, and Protocol Upgrade A built behind a dormant fence the…
- [ADR-0143 — An artifact root has one owner on the chain, and competing weights stay permissionless](0143-an-artifact-root-has-one-owner-on-the-chain.md) — IMPLEMENTED 2026-09-18, withdrawn from the 7,100 flag day and REWORKED after the adversarial audit of 2026-09-19 found…
- [ADR-0145 — Canonical work is derived, not declared; admission is earned, not registered](0145-canonical-work-is-derived-and-admission-is-earned.md) — DESIGN 2026-09-19; §1–§5 implemented dormant (ADR-0147/0148/0149, the economic bundle, None on every preset).
- [ADR-0163 — An adapter class is its parent plus an adapter, and is listed](0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) — PROPOSED 2026-10-03 (RFC-0001 §2.10, lane U)
- [ADR-0175 — 新規モデル登録は自由、登録モデルは永久不変](0175-registered-models-are-permanently-immutable.md) — Accepted design, 2026-10-09. preに独立fenceとfold/CLIを実装済み（休眠）。
- [ADR-0179 — モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3](0179-model-tiers-genesis-annual-1-0-registered-0-3.md) — Accepted(ユーザー決定 2026-10-15 第 4 回・第 5 回、判断記録 §7b・§7c)。

## このディレクトリのその他

- `STATUS-AUDIT-2026-09-18.md`、`STATUS-AUDIT-2026-09-19-llm-mining-reward.md`、`STATUS-AUDIT-2026-09-20-reward-reaudit.md` — 日付付きの status 監査(当時の記録)
- `evidence/` — ADR ごとの検証記録(JSON・md)、`0160-capacity/` — ADR-0160 の補助資料
