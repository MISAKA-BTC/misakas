# ADR 番号順索引

[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。

**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。

| 番号 | 文書 | 状態の入口 |
| --- | --- | --- |
| ADR-0001 | [Network isolation from mainline Kaspa](0001-network-isolation.md) | 原文と後続改定を参照 |
| ADR-0002 | [ML-DSA-65 P2PKH as the only standard script](0002-mldsa65-p2pkh.md) | 原文と後続改定を参照 |
| ADR-0003 | [LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024)](0003-lthash-utxo-accumulator.md) | 原文と後続改定を参照 |
| ADR-0004 | [64-byte UTXO commitment via a dedicated UtxoCommitment64 type](0004-utxo-commitment64.md) | 原文と後続改定を参照 |
| ADR-0005 | [Mass / DoS policy for ML-DSA P2PKH transactions](0005-mass-policy.md) | 原文と後続改定を参照 |
| ADR-0006 | [RPC / WASM / SDK types for kaspa-pq](0006-rpc-wasm-sdk-types.md) | 原文と後続改定を参照 |
| ADR-0007 | [Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 ASIC-hard tag)](0007-layered-pow.md) | 原文と後続改定を参照 |
| ADR-0008 | [Full Hash64 consensus identity](0008-hash64-consensus-identity.md) | 原文と後続改定を参照 |
| ADR-0009 | [DNS Probabilistic Finality Overlay](0009-dns-probabilistic-finality.md) | 原文と後続改定を参照 |
| ADR-0010 | [Validator Node Architecture (operational supplement to ADR-0009)](0010-validator-node-architecture.md) | 原文と後続改定を参照 |
| ADR-0011 | [Validator Single-Host Deployment + Equivocation-Safety Operating Model](0011-validator-deployment-and-equivocation-safety.md) | 原文と後続改定を参照 |
| ADR-0012 | [Mainnet Validator Sortition via On-Chain Commit-Reveal](0012-mainnet-validator-sortition-commit-reveal.md) | 原文と後続改定を参照 |
| ADR-0013 | [Validator Reward Distribution](0013-validator-reward-distribution.md) | 原文と後続改定を参照 |
| ADR-0014 | [Coordinated-Failover Protocol for Validator Hosts](0014-coordinated-failover-protocol.md) | 原文と後続改定を参照 |
| ADR-0015 | [Remote-Signer / HSM Protocol for Validator Signing](0015-remote-signer-hsm-protocol.md) | 原文と後続改定を参照 |
| ADR-0016 | [Stake-locked bond UTXOs](0016-stake-locked-bond-utxos.md) | 原文と後続改定を参照 |
| ADR-0017 | [All-Active-Staker Attestation (Remove Sortition Committee)](0017-all-active-staker-attestation.md) | 原文と後続改定を参照 |
| ADR-0018 | [Quality-Gated StakeScore + Inclusion Economics (BFT-free)](0018-quality-gated-stakescore-inclusion-economics.md) | 原文と後続改定を参照 |
| ADR-0019 | [Migrate Signature Scheme from ML-DSA-65 to ML-DSA-87](0019-mldsa87-migration.md) | 原文と後続改定を参照 |
| ADR-0020 | [Selected-Parent EVM Execution Lane on L1](0020-selected-parent-evm-lane.md) | 原文と後続改定を参照 |
| ADR-0021 | [PALW LLM proof-of-work (algo_id = 4/5), at one block per 120 s](0021-palw-llm-pow.md) | 原文と後続改定を参照 |
| ADR-0022 | [Pruned-IBD support for the EVM lane and the DNS/PoS-v2 overlay](0022-pruned-ibd-evm-overlay-snapshot.md) | 原文と後続改定を参照 |
| ADR-0023 | [MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM)](0023-base-three-lane-execution.md) | 原文と後続改定を参照 |
| ADR-0024 | [Verified LLM Token-Weighted BFT for DNS finality](0024-verified-llm-token-weighted-bft.md) | 原文と後続改定を参照 |
| ADR-0025 | [Chain participation and IBD candidate selection](0025-chain-participation-and-ibd-candidate-selection.md) | 原文と後続改定を参照 |
| ADR-0026 | [Current PALW architecture on testnet-11](0026-current-palw-testnet11-architecture.md) | 原文と後続改定を参照 |
| ADR-0026 | [PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof](0026-palw-v2-runtime-separated-verification.md) | 原文と後続改定を参照 |
| ADR-0027 | [PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal](0027-palw-slash-unilateral-fraud-proofs.md) | 原文と後続改定を参照 |
| ADR-0028 | [PALW challenge sampling — a scheduler for re-execution, never a verdict](0028-palw-challenge-sampling-protocol.md) | 原文と後続改定を参照 |
| ADR-0029 | [PALW chain carriage — the objects ride the rails the fork already built](0029-palw-chain-carriage.md) | 原文と後続改定を参照 |
| ADR-0030 | [The PALW step function, pinned at tile granularity — shape profile v3](0030-palw-step-function-shape-profile.md) | 原文と後続改定を参照 |
| ADR-0031 | [Canonical transcendentals — exp and log are algorithms, not functions](0031-palw-canonical-transcendentals.md) | 原文と後続改定を参照 |
| ADR-0032 | [PALW fee-bond escrow — pricing calls and paying challengers without new covenants](0032-palw-fee-bond-escrow.md) | 原文と後続改定を参照 |
| ADR-0033 | [PALW V1 credit overlay（不採用）](0033-palw-credit-gate-wiring.md) | 原文と後続改定を参照 |
| ADR-0034 | [PALW re-verification routing — four execution-class families, five model bands, one decidi…](0034-palw-execution-class-model-band-routing.md) | 原文と後続改定を参照 |
| ADR-0035 | [The public PALW testnet is testnet-11, continued — and it pins its determinism class at th…](0035-palw-public-testnet-strategy.md) | 原文と後続改定を参照 |
| ADR-0036 | [PALW mainnet activation — lineage reconciliation and the model that governs](0036-palw-mainnet-activation-model.md) | 原文と後続改定を参照 |
| ADR-0037 | [PALW asynchronous job state machine — hash-floor direction withdrawn](0037-palw-async-job-state-machine.md) | 原文と後続改定を参照 |
| ADR-0038 | [PALW is the consensus work — sampled verification and a receipt-licensed weight ramp](0038-palw-is-the-consensus-work.md) | 原文と後続改定を参照 |
| ADR-0039 | [PALW-only block production — a Base class instead of a hash floor, and a two-weight fork c…](0039-palw-only-block-production.md) | 原文と後続改定を参照 |
| ADR-0040 | [PALW-BASE-0 — the integer-only arithmetic normative specification](0040-palw-base-0-integer-arithmetic.md) | 原文と後続改定を参照 |
| ADR-0041 | [PALW pruning-proof verification — exhaustive and amortised, not sampled](0041-palw-pruning-proof-verification.md) | 原文と後続改定を参照 |
| ADR-0042 | [The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerpri…](0042-palw-mainnet-candidate-ruleset.md) | 原文と後続改定を参照 |
| ADR-0043 | [PALW V2 state-root hash ordering (no challenge↔commitment cycle)](0043-palw-v2-state-root-ordering.md) | 原文と後続改定を参照 |
| ADR-0044 | [Free-prompt PALW — the user's own inference becomes the consensus work, certified before i…](0044-palw-free-prompt-receipts.md) | 原文と後続改定を参照 |
| ADR-0045 | [The class economy is chain state — derived PWU, block-denominated epoch budgets, and the r…](0045-palw-class-economy-on-chain.md) | 原文と後続改定を参照 |
| ADR-0046 | [PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts r…](0046-palw-v2-consensus-object-carriage.md) | 原文と後続改定を参照 |
| ADR-0047 | [The A16 activation tier — sixteen-bit activations for the classes int8 cannot carry](0047-palw-a16-activation-tier.md) | 原文と後続改定を参照 |
| ADR-0049 | [The adjudication contract — what a court opens, and the bound that makes it model-size-ind…](0049-palw-adjudication-contract.md) | 原文と後続改定を参照 |
| ADR-0050 | [The BASE-0 residual site — the narrowing that was never declared, and the amplification th…](0050-palw-base0-residual-site.md) | 原文と後続改定を参照 |
| ADR-0051 | [The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified…](0051-palw-metal-gguf-execution-family.md) | 原文と後続改定を参照 |
| ADR-0052 | [PALW-QWEN36 — the integer arithmetic for Qwen3.6's hybrid graph](0052-palw-qwen36-hybrid-class.md) | 原文と後続改定を参照 |
| ADR-0053 | [One execution family — Family M is withdrawn, and the court is not optional](0053-palw-one-execution-family.md) | 原文と後続改定を参照 |
| ADR-0054 | [A class's cadence share follows its own production](0054-palw-share-follows-production.md) | 原文と後続改定を参照 |
| ADR-0055 | [Chain position is earned, and the question is set by the block](0055-palw-position-is-earned-not-declared.md) | 原文と後続改定を参照 |
| ADR-0056 | [Permissionless class admission, and the share economy that survives it](0056-palw-permissionless-class-admission-and-share-economy.md) | 原文と後続改定を参照 |
| ADR-0057 | [BASE-0 runtime acceleration — backends below the semantic boundary](0057-palw-base0-runtime-acceleration.md) | 原文と後続改定を参照 |
| ADR-0058 | [Merged work is counted — the mergeset carries claims, not just the chain](0058-palw-merged-work-is-counted.md) | 原文と後続改定を参照 |
| ADR-0059 | [The 10B premine cap — genesis mints one number, everything else is a carve](0059-the-10b-premine-cap.md) | 原文と後続改定を参照 |
| ADR-0060 | [The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay](0060-the-liveness-doctrine.md) | 原文と後続改定を参照 |
| ADR-0061 | [Zero-seat genesis, and collateral sized by arithmetic instead of history](0061-zero-seat-genesis-and-right-sized-collateral.md) | 原文と後続改定を参照 |
| ADR-0062 | [The data-availability court: stop a vote from taking a bond](0062-data-availability-court.md) | 原文と後続改定を参照 |
| ADR-0063 | [The operator's half of the protocol is missing, and one gap locks money in](0063-operator-tooling-the-missing-half.md) | 原文と後続改定を参照 |
| ADR-0064 | [Trustless recovery from a total producer stop: the bond becomes usable in the block that r…](0064-trustless-recovery-from-a-total-stop.md) | 原文と後続改定を参照 |
| ADR-0065 | [A bond must be earned, and a failure is not a verdict](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) | 原文と後続改定を参照 |
| ADR-0066 | [The heartbeat lane out of header.bits, and the inactivity leak out of node memory](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) | 原文と後続改定を参照 |
| ADR-0067 | [Classes are chain data; only kernels are the build](0067-classes-are-chain-data-kernels-are-the-build.md) | 原文と後続改定を参照 |
| ADR-0068 | [The LLM-primary economy — the floor retires to the doctrine's minimum](0068-the-llm-primary-economy-and-the-floors-minimum.md) | 原文と後続改定を参照 |
| ADR-0069 | [End-to-end adjudicability is the price of weight](0069-e2e-adjudicability-is-the-price-of-weight.md) | 原文と後続改定を参照 |
| ADR-0070 | [The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them](0070-the-model-tiers-step-spaces-are-adjudicable.md) | 原文と後続改定を参照 |
| ADR-0071 | [The attempt lane's price, the ticket's bound, and who may judge a class](0071-the-attempt-lanes-price-and-the-tickets-bound.md) | 原文と後続改定を参照 |
| ADR-0072 | [The ticket is the execution: both lotteries priced in inferences](0072-the-ticket-is-the-execution.md) | 原文と後続改定を参照 |
| ADR-0073 | [Real-demand work bears the weight](0073-real-demand-work-bears-the-weight.md) | 原文と後続改定を参照 |
| ADR-0074 | [The attempt is a claim, drawn by the chain](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) | 原文と後続改定を参照 |
| ADR-0075 | [Certification is a consensus object](0075-certification-is-a-consensus-object.md) | 原文と後続改定を参照 |
| ADR-0076 | [The attempt lane's seed is the retarget's own equilibrium](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) | 原文と後続改定を参照 |
| ADR-0077 | [A prompt a person would type is a claim the court can try](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | 原文と後続改定を参照 |
| ADR-0078 | [What was made from it is committed; the thing itself never rides](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) | 原文と後続改定を参照 |
| ADR-0079 | [A pure function needs no permissions — the sandbox is for the host, and the chain never ta…](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | 原文と後続改定を参照 |
| ADR-0080 | [The answer is long; the verified unit is short](0080-the-answer-is-long-the-verified-unit-is-short.md) | 原文と後続改定を参照 |
| ADR-0081 | [Long context — the input is a state chain](0081-long-context-the-input-is-a-state-chain.md) | 原文と後続改定を参照 |
| ADR-0082 | [The close is flat in the context — attention is refuted by dissection, the capture is a fo…](0082-the-close-is-flat-in-the-context.md) | 原文と後続改定を参照 |
| ADR-0083 | [The difficulty window counts only rows priced by bits — heartbeat emitters are not work](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) | 原文と後続改定を参照 |
| ADR-0084 | [The ids ride, the capture stays home — a model-class claim serves its answer, never its hi…](0084-the-ids-ride-the-capture-stays-home.md) | 原文と後続改定を参照 |
| ADR-0085 | [The close is assembled from what the executor served — a disputed tile, not a capture](0085-the-close-is-assembled-from-what-was-served.md) | 原文と後続改定を参照 |
| ADR-0086 | [the opening carries the fold, not the leaves](0086-the-opening-carries-the-fold-not-the-leaves.md) | 原文と後続改定を参照 |
| ADR-0087 | [a position is bought from the curve and sold back to it](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | 原文と後続改定を参照 |
| ADR-0088 | [the class keeps its graph; a line keeps its owner, and the owner keeps publishing](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | 原文と後続改定を参照 |
| ADR-0089 | [the fold is the truth; the EVM is its window and its hand](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | 原文と後続改定を参照 |
| ADR-0090 | [The pair is seeded with real BILI, locked for good, and a position is whole](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | 原文と後続改定を参照 |
| ADR-0091 | [The reward buys the pair, and no holder is paid](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) | 原文と後続改定を参照 |
| ADR-0092 | [The ladder is minted once, and the wall clock is what binds](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) | 原文と後続改定を参照 |
| ADR-0093 | [The court can try a fused row; the responder is what is missing](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) | 原文と後続改定を参照 |
| ADR-0094 | [A seed is paid in as many transactions as it takes](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | 原文と後続改定を参照 |
| ADR-0095 | [A position is a membership, not an income](0095-a-position-is-a-membership-not-an-income.md) | 原文と後続改定を参照 |
| ADR-0096 | [The app you already use is the entrance, and the shape of the answer is committed](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) | 原文と後続改定を参照 |
| ADR-0097 | [A model's fit is a lookup, and the entrance says its limits before the first token](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) | 原文と後続改定を参照 |
| ADR-0098 | [The panel's coverage is a number, and a seat that found a lie files nothing else](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) | 原文と後続改定を参照 |
| ADR-0099 | [The adder measures, the chain recomputes, and a seat holds a shard](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) | 原文と後続改定を参照 |
| ADR-0100 | [A model is data: the one-move court, the held measurement and the licence per shard, and t…](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | 原文と後続改定を参照 |
| ADR-0101 | [A membership is proven by the chain and served by anyone, and a Position never moves betwe…](0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | 原文と後続改定を参照 |
| ADR-0102 | [The embedding lift is read per token, and a kernel a network has not armed is not in its i…](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) | 原文と後続改定を参照 |
| ADR-0103 | [The context is held off the chain, and the chain carries a root, an opening and a logarith…](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) | 原文と後続改定を参照 |
| ADR-0105 | [A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has …](0105-a-heartbeat-never-turns-a-bonded-block-red.md) | 原文と後続改定を参照 |
| ADR-0106 | [An inventory is a stream of leaves, not a copy of the model](0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) | 原文と後続改定を参照 |
| ADR-0107 | [A class's share grows on work that reached Final, not on blocks that were accepted](0107-a-share-grows-on-work-that-reached-final.md) | 原文と後続改定を参照 |
| ADR-0108 | [An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote](0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | 原文と後続改定を参照 |
| ADR-0109 | [A lock is its own claim, and finality is a label, not a pause](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) | 原文と後続改定を参照 |
| ADR-0110 | [A context limit is activated from reproducible public vectors, not the maintainer's workst…](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) | 原文と後続改定を参照 |
| ADR-0111 | [A seat may demand the committed leaf it needs to judge](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) | 原文と後続改定を参照 |
| ADR-0112 | [A class's weights are read within a budget the operator states, and the budget is a fifth …](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) | 原文と後続改定を参照 |
| ADR-0114 | [The owner's leg is five percent, and it arrives at a height](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) | 原文と後続改定を参照 |
| ADR-0115 | [A pending transaction is announced until it lands](0115-a-pending-transaction-is-announced-until-it-lands.md) | 原文と後続改定を参照 |
| ADR-0116 | [An attention history is the class's, and the held regime reduces over its own width](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) | 原文と後続改定を参照 |
| ADR-0117 | [A draw is one forward](0117-a-draw-is-one-forward.md) | 原文と後続改定を参照 |
| ADR-0118 | [The held regime arrives at a height, and a held class carries its own prompt form](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) | 原文と後続改定を参照 |
| ADR-0119 | [A held class is walked at the regime's ladder, and the chain records which classes those a…](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | 原文と後続改定を参照 |
| ADR-0120 | [The least seed is one million BILI, and it arrives at a height](0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | 原文と後続改定を参照 |
| ADR-0121 | [A held capture is served from its fold as the replay streams, and a node holds two ladders](0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) | 原文と後続改定を参照 |
| ADR-0122 | [Mining is a purpose: an operator runs one command and reads one work id](0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) | 原文と後続改定を参照 |
| ADR-0123 | [The epoch progressively releases unused class budget](0123-the-epoch-progressively-releases-unused-class-budget.md) | 原文と後続改定を参照 |
| ADR-0124 | [The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid fo…](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | 原文と後続改定を参照 |
| ADR-0125 | [The execution lane is a second lane inside the cadence, and it widens one permit at a time](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) | 原文と後続改定を参照 |
| ADR-0126 | [The validator carve drops to a fifth, and the stake reorg gate stays](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) | 原文と後続改定を参照 |
| ADR-0127 | [PALW settles on its own, and its terms are not DNS terms](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) | 原文と後続改定を参照 |
| ADR-0128 | [DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | 原文と後続改定を参照 |
| ADR-0129 | [A double spend needs the anchors, not the blocks](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) | 原文と後続改定を参照 |
| ADR-0130 | [BPS 1 is hardened before it is widened](0130-bps1-is-hardened-before-it-is-widened.md) | 原文と後続改定を参照 |
| ADR-0131 | [A claim is paid for the compute it cost, in economic compute, not leaves](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) | 原文と後続改定を参照 |
| ADR-0132 | [What a model is actually paid per forward it ran, and why the gap is liveness before it is…](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | 原文と後続改定を参照 |
| ADR-0133 | [Verification is its own clock: a class verifies over spans, and a starved class stops only…](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) | 原文と後続改定を参照 |
| ADR-0134 | [The compute overlay's committee beacon retires at a height, and its machinery goes](0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) | 原文と後続改定を参照 |
| ADR-0135 | [A model is data: the permissionless registry derives its profile, proves its panel, and wa…](0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | 原文と後続改定を参照 |
| ADR-0136 | [An artifact is mapped, not read, and a host holds one copy of it](0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) | 原文と後続改定を参照 |
| ADR-0137 | [A block buys one unit of work from any model, and a share is a result, not an input](0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) | 原文と後続改定を参照 |
| ADR-0138 | [The DAA score is the anchor's clock: a block advances it iff bits priced it](0138-the-daa-score-is-the-anchors-clock.md) | 原文と後続改定を参照 |
| ADR-0139 | [O13 decided: the execution lane's gas throughput scales with the lane, one budget a round](0139-the-execution-lanes-gas-is-one-budget-a-round.md) | 原文と後続改定を参照 |
| ADR-0140 | [The heartbeat is the emergency generator: it must not touch the economy or the difficulty …](0140-the-heartbeat-is-the-emergency-generator.md) | 原文と後続改定を参照 |
| ADR-0141 | [Can an inference be the ticket without a hash lottery?](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | 原文と後続改定を参照 |
| ADR-0142 | [The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not ad…](0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) | 原文と後続改定を参照 |
| ADR-0143 | [An artifact root has one owner on the chain, and competing weights stay permissionless](0143-an-artifact-root-has-one-owner-on-the-chain.md) | 原文と後続改定を参照 |
| ADR-0144 | [PALW pays for the inference you were going to run anyway](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) | 原文と後続改定を参照 |
| ADR-0145 | [Canonical work is derived, not declared; admission is earned, not registered](0145-canonical-work-is-derived-and-admission-is-earned.md) | 原文と後続改定を参照 |
| ADR-0146 | [A coefficient is justified by the arbitrage it permits, not by being right](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) | 原文と後続改定を参照 |
| ADR-0147 | [Independence is drawn, not declared](0147-independence-is-drawn-not-declared.md) | 原文と後続改定を参照 |
| ADR-0148 | [The free-prompt lane prices compute, and prices it the same for every class](0148-the-free-prompt-lane-prices-compute.md) | 原文と後続改定を参照 |
| ADR-0149 | [An attempt's pwu is the derivation, so the weight reads it directly](0149-an-attempts-pwu-is-the-derivation.md) | 原文と後続改定を参照 |
| ADR-0150 | [The fingerprint must see the RULE, not only the height](0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) | 原文と後続改定を参照 |
| ADR-0151 | [Liveness is structural; collateral covers fraud](0151-liveness-is-structural-collateral-covers-fraud.md) | 原文と後続改定を参照 |
| ADR-0152 | [A bond is standing stake, a claim reserves until its licence, and a reward vests until its…](0152-account-stake-staged-reserve-and-vested-rewards.md) | 原文と後続改定を参照 |
| ADR-0153 | [(draft) — 2M verification by shards: the implementation plan](0153-2m-verification-sharding-plan.md) | 原文と後続改定を参照 |
| ADR-0154 | [testnet-12's first post-launch flag day: thirteen fences at DAA 750](0154-testnet-12-flag-day-daa-750.md) | 原文と後続改定を参照 |
| ADR-0155 | [testnet-12's second post-launch flag day: two fences at DAA 1,300](0155-testnet-12-flag-day-daa-1300.md) | 原文と後続改定を参照 |
| ADR-0160 | [Claim capacity is separated from collateral price (v3): the bond is one shared guarantee s…](0160-claim-capacity-separation.md) | 原文と後続改定を参照 |
| ADR-0161 | [A claim is not an emission: multi-claim carriage under a fixed per-DAA reward budget](0161-a-claim-is-not-an-emission.md) | 原文と後続改定を参照 |
| ADR-0162 | [The pair opens on a virtual reserve](0162-the-pair-opens-on-a-virtual-reserve.md) | 原文と後続改定を参照 |
| ADR-0163 | [An adapter class is its parent plus an adapter, and is listed](0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) | 原文と後続改定を参照 |
| ADR-0165 | [Useful work carries the clock: BASE-0 remains the idle-only bonded floor (A″), REAL attemp…](0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) | 原文と後続改定を参照 |
| ADR-0166 | [Verifier unavailability is not producer fraud](0166-verifier-unavailability-is-not-producer-fraud.md) | 原文と後続改定を参照 |
| ADR-0167 | [The ×1000 capacity package: a fixed per-DAA PALW reward budget, riders, a lower-only break…](0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md) | 原文と後続改定を参照 |
| ADR-0168 | [An execution block is a third class, and the chain reaches it through an anchor, never a p…](0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) | 原文と後続改定を参照 |
| ADR-0169 | [RFC8 uses the existing EXEC lane; the former algo-11 design is retired](0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) | 原文と後続改定を参照 |
| ADR-0170 | [The seed anchor is a window, not a span: a merged attempt anchors, the anchor survives spa…](0170-the-seed-anchor-is-a-window-not-a-span.md) | 原文と後続改定を参照 |
| ADR-0171 | [Probabilistic constraint checks are the normal large-model verifier; the court resolves di…](0171-probabilistic-constraint-checks-and-court-on-dispute.md) | 原文と後続改定を参照 |
| ADR-0172 | [Model extensibility uses versioned kernels, not a universal VM](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) | 原文と後続改定を参照 |
| ADR-0173 | [A seat holds a root, not a class: possession is keyed (bond, class, root), a new root is N…](0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) | 原文と後続改定を参照 |
| ADR-0173 | [MISAKA's purpose: public-verifier dispute completeness](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) | 原文と後続改定を参照 |
| ADR-0174 | [Token name Misaka, ticker BILI; address prefixes stay unchanged](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) | 原文と後続改定を参照 |
| ADR-0175 | [新規モデル登録は自由、登録モデルは永久不変](0175-registered-models-are-permanently-immutable.md) | 原文と後続改定を参照 |
| ADR-0176 | [Bond bounds PALW claim capacity, blocks, rewards and Final weight](0176-bond-bounds-palw-production-rewards-and-final-weight.md) | 原文と後続改定を参照 |
| ADR-0177 | [Model-bond coinbase allocation without model-availability consensus](0177-model-bond-allocation-without-availability-consensus.md) | 原文と後続改定を参照 |
| ADR-0178 | [The fork choice heals a partition on bonded participation: one search over every tip, econ…](0178-fork-choice-heals-a-partition-on-bonded-participation.md) | 原文と後続改定を参照 |
| ADR-0179 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3](0179-model-tiers-genesis-annual-1-0-registered-0-3.md) | Accepted / 二階層 |
| ADR-0180 | [PALW のみの合意:BFT の validator を使わない](0180-palw-only-consensus-no-bft-validators.md) | Accepted / BFT不使用の設計 |
| ADR-0181 | [採用済み設計を原則凍結し、重大問題時のみ新規 RFC・ADR で変更する](0181-design-freeze-and-controlled-change.md) | Accepted / 原則凍結 |
| ADR-0182 | [画像・動画経路とモデル改善を mainnet 後へ延期し、当面無効にする](0182-defer-media-and-model-improvement-until-post-mainnet.md) | Accepted / 延期・無効化方針 |
| ADR-0183 | [G14 裁定容量・Panel 下限・裁定保持期間・検出方針(ユーザー決定の記録)](0183-g14-court-capacity-panel-floor-retention-detection.md) | 原文と後続改定を参照 |
| ADR-0184 | [native claim の責任期間、Panel-licensed Final の記録専用 event、認証報酬の単一 leg](0184-native-liability-horizon-panel-record-and-single-certified-leg.md) | 原文と後続改定を参照 |
| ADR-0185 | [paid claim の lock 乱数:後続 seal の commitment、最初の lock のみ固定 seed、承認済み paid claim を beacon sour…](0185-paid-lock-randomness-bootstrap-and-paid-sources.md) | 原文と後続改定を参照 |
| ADR-0186 | [未解決 claim の責任を保持し、それに依存しない claim を進める](0186-hold-unresolved-liability-advance-independent-claims.md) | 原文と後続改定を参照 |
| ADR-0187 | [native 経路の G14:枠なしの公開告発、期限切れ seal の解放、mining claim の Final 後の追及](0187-g14-native-public-filing-and-post-final-mining-accusation.md) | 原文と後続改定を参照 |
| ADR-0188 | [D2・X3 の休眠 fence を G14 の arm-readiness record で閉じ、X3 の期限を窓の合成で定める](0188-d2-x3-arm-records-composition-and-pause-credit.md) | 原文と後続改定を参照 |
| ADR-0189 | [TIR court read set and the conviction door: options (Proposed)](0189-tir-court-read-set-door-options.md) | 原文と後続改定を参照 |
| ADR-0190 | [TIR conviction door: sparse fetch, designated answers, tenure split (Proposed)](0190-tir-door-sparse-fetch-designated-answers-tenure-split.md) | 原文と後続改定を参照 |
| ADR-0191 | [TIR court: dispute-local decomposition for long-context claims (Proposed)](0191-tir-dispute-local-decomposition-long-context.md) | 原文と後続改定を参照 |
| ADR-0192 | [court の正規処理機会と共有訴追経路](0192-court-service-opportunity-and-shared-pursuit.md) | 原文と後続改定を参照 |
| ADR-0193 | [Panel の追及を支える Miner Account Bond: 最低10万と責任連動追加、過大拘束を避ける](0193-miner-account-bond-for-panel-incentives.md) | 原文と後続改定を参照 |
| ADR-0193 | [sequential witness filing の versioned clock](0193-sequential-witness-filing-clock.md) | 原文と後続改定を参照 |
| ADR-0194 | [休眠 G14 統合候補と service namespace](0194-dormant-g14-integration-and-service-schema.md) | 原文と後続改定を参照 |
| ADR-0195 | [native court 応答の前提受理時刻を保持する](0195-rooted-court-response-eligibility-history.md) | 原文と後続改定を参照 |
| ADR-0196 | [owner 非依存の TIR 要求と認証済み公開データ](0196-owner-independent-shared-tir-query-data.md) | 原文と後続改定を参照 |
| ADR-0197 | [court終了効果と未解決責任の保存方式](0197-court-end-effects-and-pending-liability-archive.md) | 原文と後続改定を参照 |
| ADR-0198 | [共有courtのinitial noticeの等号](0198-shared-court-initial-notice-boundary.md) | 原文と後続改定を参照 |
| ADR-0199 | [共有court終端materialと非独占提出方式](0199-shared-court-terminal-material-and-nonblocking-ingress.md) | 原文と後続改定を参照 |
| ADR-0200 | [D2 readiness rootの認証path供給](0200-d2-readiness-root-opening-availability.md) | 原文と後続改定を参照 |
| ADR-0201 | [archived closeの期限内実受付と責任解消](0201-archived-close-qualified-financial-discharge.md) | 原文と後続改定を参照 |
| ADR-0202 | [shared court originalの実原資と役割別同意](0202-court-original-funded-contract-and-role-consent.md) | 原文と後続改定を参照 |
| ADR-0203 | [認証付き PALW state map と行単位アクセス](0203-authenticated-palw-state-map-and-row-access.md) | 原文と後続改定を参照 |
| ADR-0204 | [IBD の有界 authenticated row stream と世代番号付き保存](0204-ibd-bounded-row-stream-and-storage-generations.md) | 原文と後続改定を参照 |
| ADR-0205 | [decode 済みの回答は二度支払わない](0205-a-decoded-answer-is-not-paid-twice.md) | 原文と後続改定を参照 |
| ADR-0206 | [PanelDa の G14 経路 — 期限付きの必須公開](0206-panel-da-mandatory-disclosure-g14-route.md) | 原文と後続改定を参照 |
| ADR-0207 | [通常 verifier fee と有限 provider 契約の判断](0207-finite-fee-consent-and-provider-payment-bounds.md) | 原文と後続改定を参照 |
| ADR-0208 | [再利用 prefix の生成元 identity を明示する案](0208-authenticated-prefix-work-dependencies.md) | 原文と後続改定を参照 |
| ADR-0209 | [genesis bond を廃止し、BASE 回復と通常 bond 登録で起動する](0209-genesis-bond-free-bootstrap.md) | 原文と後続改定を参照 |
