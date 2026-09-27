# ADR inventory: the restructure plan, one row per file

This inventory plans the split described in [INDEX.md](../INDEX.md). It has one row per file in
`docs/adr/` on `main` at `55a7be02f`: 148 ADRs, 3 status audits, the index and one JSON census,
153 files in all. Each row gives the file's domain, the status of its decision, and where its content
goes when the ADR is cut down to a decision record. The classification comes from each ADR's title,
status line and section outline, checked against the supersede map in [README.md](README.md) and
against [architecture/overview.md](../architecture/overview.md). Phase 2 reads each PALW ADR in full
and may move a row. When it does, it corrects the row here.

## Legend

**Domain**

| Domain | Scope |
| --- | --- |
| PALW | Everything the PALW fold decides, from class registration to settlement, plus the model market that rides the fold |
| EVM | The EVM lane and its node policy |
| DNS-BFT | DNS finality, validators, their bonds, rewards and signing |
| bridge | The EVM bridge and its liveness |
| network | The base layer: post-quantum transactions and identity, network isolation, IBD, the params identity |
| wallet | RPC/SDK types, keys and operator tooling |

**Status.** This is the status of the *decision*, not whether a rule is active on a network. Activation
is per network and belongs in the Spec. testnet-12 arms at genesis nearly every rule that testnet-11
kept dormant.

| Status | Meaning |
| --- | --- |
| `active` | The decision governs, whether implemented, fenced or a doctrine |
| `amended` | It governs as amended. The cell names the amending ADRs or clauses |
| `partial →` | Part of it is superseded. The cell names which part, and by what |
| `superseded →` | It no longer governs. It stays as the record |
| `dormant` | A lineage that is not on the V2 path (the credit overlay) |
| `proposed` | Not accepted or not built |
| `record` | Decides nothing (ADR-0141) |
| `constitution` | ADR-0144 |

**Content goes to**

| Code | Destination |
| --- | --- |
| `S palw/NN` | A chapter of [spec/palw/](../spec/palw/00-index.md): 01 principles · 02 state, objects and carriage · 03 classes and registry · 04 execution semantics · 05 canonical work · 06 eligibility and block production · 07 claim lifecycle · 08 verification · 09 court and offences · 10 collateral and economics · 11 free-prompt lane · 12 execution lane · 13 fork choice and heartbeat · 14 node duties · 15 model lines and market · 16 network parameters and fences |
| `S <domain>` | `spec/<domain>/`, written in Phase 3 |
| `D <domain>/<name>` | `design/<domain>/<name>.md` ([design/README.md](../design/README.md)) |
| `keep` | Already a short decision record. It gains a Links section. Its normative sentences are copied into the Spec and stay here as the decision |
| `frozen` | Superseded, or a dormant lineage. The body is kept verbatim with a banner at the top ([INDEX.md](../INDEX.md) §4). It contributes nothing to the Spec. Its reasoning is cited from Design as history |

Every other row is **slimmed**: its normative clauses are written into the Spec, its body moves
verbatim to `design/palw/archive/NNNN-<slug>.md` (reasoning, measurements, review logs and security
amendments as written), the topic's Design document summarises and links it, and the ADR keeps
Context / Decision / Consequences / Links plus a "body moved" banner.

**Decisions of 2026-09-27 (Phase 1 review).** (1) Superseded ADRs are frozen: the body stays, a
banner goes at the top, nothing is shortened. (2) ADR-0152 is imported and split in one change.
(3) ADR-0160 waits for the int-6 integration and is imported in its final form; its row below is a
placeholder. (4) One short ADR per post-launch flag day (DAA 750, DAA 1,300; DAA 1,500 later).

## Summary

**By domain and status (148 ADRs)**

| Domain | active | amended | partial | superseded | dormant | proposed | record | constitution | total |
| --- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| PALW | 71 | 23 | 10 | 6 | 2 | 3 | 1 | 1 | **117** |
| DNS-BFT | 9 | 0 | 4 | 1 | 0 | 0 | 0 | 0 | **14** |
| network | 8 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | **10** |
| EVM | 3 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | **4** |
| wallet | 1 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | **2** |
| bridge | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | **1** |
| **all** | **93** | **23** | **15** | **7** | **2** | **6** | **1** | **1** | **148** |

The other five files are three status audits (`STATUS-AUDIT-*`), which move to `audit/`, the index
(`README.md`), and one evidence file, the 0134 census JSON.

**By treatment.** 112 ADRs are slimmed (91 of them PALW), 26 are kept as they are (17 PALW), and 10
are frozen (9 PALW). PALW ADRs hold 2.7 MB of the 3.2 MB of ADR text, not counting the index and the audits.

**PALW chapter load** (the number of ADRs feeding each chapter, before ADR-0152, which feeds 07–10):
01: 2 · 02: 4 · 03: 16 · 04: 16 · 05: 8 · 06: 16 · 07: 4 · 08: 14 · 09: 14 · 10: 13 · 11: 9 ·
12: 4 · 13: 11 · 14: 11 · 15: 10 · 16: 8.

**Largest ADRs** (KB), which are also the first to slim: 0082 (79) · 0103 (69) · 0122 (63) ·
0038 (55) · 0089 (54) · 0067 (53) · 0096 (51) · 0137 (51) · 0013 (48) · 0077 (45) · 0088 (44) ·
0062 (43).

## The ADRs

| ADR | Title | KB | Domain | Status | Content goes to |
| --- | --- | --: | --- | --- | --- |
| [0001](0001-network-isolation.md) | Network isolation from mainline Kaspa | 5 | network | active | S network · keep |
| [0002](0002-mldsa65-p2pkh.md) | ML-DSA-65 P2PKH as the only standard script | 6 | network | partial → 0019 (signature scheme; the P2PKH structure stands) | S network · keep |
| [0003](0003-lthash-utxo-accumulator.md) | LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024) | 8 | network | active | S network · keep |
| [0004](0004-utxo-commitment64.md) | 64-byte UTXO commitment via a dedicated `UtxoCommitment64` type | 7 | network | active | S network · keep |
| [0005](0005-mass-policy.md) | Mass / DoS policy for ML-DSA P2PKH transactions | 10 | network | active | S network (mass rules) · D network (calibration history) |
| [0006](0006-rpc-wasm-sdk-types.md) | RPC / WASM / SDK types for kaspa-pq | 10 | wallet | active | S wallet (RPC/WASM/SDK types) · D wallet |
| [0007](0007-layered-pow.md) | Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 ASIC-hard tag) | 19 | network | active (the algo_id extension point PALW and the heartbeat use) | S network (layers, algo_id cut-off rule) · D network |
| [0008](0008-hash64-consensus-identity.md) | Full Hash64 consensus identity | 13 | network | active | S network · D network |
| [0009](0009-dns-probabilistic-finality.md) | DNS Probabilistic Finality Overlay | 32 | DNS-BFT | partial → 0024 → 0128 (voting weight) | S dns-bft (overlay, reorg gate) · D dns-bft |
| [0010](0010-validator-node-architecture.md) | Validator Node Architecture (operational supplement to ADR-0009) | 18 | DNS-BFT | active | S dns-bft (validator-set commitment) · D dns-bft |
| [0011](0011-validator-deployment-and-equivocation-safety.md) | Validator Single-Host Deployment + Equivocation-Safety Operating Model | 20 | DNS-BFT | active | S dns-bft (equivocation safety) · D dns-bft (deployment) |
| [0012](0012-mainnet-validator-sortition-commit-reveal.md) | Mainnet Validator Sortition via On-Chain Commit-Reveal | 24 | DNS-BFT | superseded → 0017 | frozen |
| [0013](0013-validator-reward-distribution.md) | Validator Reward Distribution | 48 | DNS-BFT | active (validator share → 0126) | S dns-bft (reward distribution, addenda B–C) · D dns-bft |
| [0014](0014-coordinated-failover-protocol.md) | Coordinated-Failover Protocol for Validator Hosts | 21 | DNS-BFT | active | S dns-bft (failover protocol) · D dns-bft |
| [0015](0015-remote-signer-hsm-protocol.md) | Remote-Signer / HSM Protocol for Validator Signing | 21 | DNS-BFT | active (HSM backend deferred) | S dns-bft (signer protocol) · D dns-bft |
| [0016](0016-stake-locked-bond-utxos.md) | Stake-locked bond UTXOs | 9 | DNS-BFT | active | S dns-bft (bond UTXO) · S palw/10 cites it · keep |
| [0017](0017-all-active-staker-attestation.md) | All-Active-Staker Attestation (Remove Sortition Committee) | 9 | DNS-BFT | partial → 0024 → 0128 (voting weight) | S dns-bft · D dns-bft |
| [0018](0018-quality-gated-stakescore-inclusion-economics.md) | Quality-Gated StakeScore + Inclusion Economics (BFT-free) | 18 | DNS-BFT | partial → 0024 (voting weight), 0126 (§F share) | S dns-bft · D dns-bft |
| [0019](0019-mldsa87-migration.md) | Migrate Signature Scheme from ML-DSA-65 to ML-DSA-87 | 16 | network | active | S network · D network |
| [0020](0020-selected-parent-evm-lane.md) | Selected-Parent EVM Execution Lane on L1 | 12 | EVM | active ("opt-in" wording stale, 0089 §2) | S evm · D evm |
| [0021](0021-palw-llm-pow.md) | PALW LLM proof-of-work (`algo_id = 4`/`5`), at one block per 120 s | 13 | PALW | superseded → 0026, 0038, 0039 | frozen · D palw/lineage |
| [0022](0022-pruned-ibd-evm-overlay-snapshot.md) | Pruned-IBD support for the EVM lane and the DNS/PoS-v2 overlay | 13 | EVM | active | S evm + S network (IBD overlay snapshot) · D evm |
| [0023](0023-base-three-lane-execution.md) | MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM) | 33 | EVM | proposed (nothing built; reads as an RFC) | D evm · rfc/README lists it |
| [0024](0024-verified-llm-token-weighted-bft.md) | Verified LLM Token-Weighted BFT for DNS finality | 21 | DNS-BFT | partial → 0128 (VLT voting weight) | S dns-bft (dormant VLT params) · D dns-bft |
| [0025](0025-chain-participation-and-ibd-candidate-selection.md) | Chain participation and IBD candidate selection | 5 | network | proposed (file) — cited as governing by the overview | S network (IBD candidate selection) · keep |
| [0026](0026-palw-v2-runtime-separated-verification.md) | PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof | 25 | PALW | active (promoted by 0038; restored by 0053) | S palw/08, 09 · D palw/verification |
| [0027](0027-palw-slash-unilateral-fraud-proofs.md) | PALW-S — unilateral fraud proofs; no BFT, no challenge randomness, slash-terminal | 19 | PALW | active (promoted by 0038) | S palw/09 · D palw/court |
| [0028](0028-palw-challenge-sampling-protocol.md) | PALW challenge sampling — a scheduler for re-execution, never a verdict | 33 | PALW | amended (0133: windows restated) | S palw/08 · D palw/verification |
| [0029](0029-palw-chain-carriage.md) | PALW chain carriage — the objects ride the rails the fork already built | 16 | PALW | superseded → 0046 | frozen · D palw/lineage |
| [0030](0030-palw-step-function-shape-profile.md) | The PALW step function, pinned at tile granularity — shape profile v3 | 22 | PALW | active | S palw/04 (step function, tile pinning) · D palw/execution |
| [0031](0031-palw-canonical-transcendentals.md) | Canonical transcendentals — exp and log are algorithms, not functions | 6 | PALW | active | S palw/04 (exp/log algorithms) · keep |
| [0032](0032-palw-fee-bond-escrow.md) | PALW fee-bond escrow — pricing calls and paying challengers without new covenants | 5 | PALW | dormant (credit-overlay lineage, not on the V2 path) | frozen · D palw/lineage |
| [0033](0033-palw-credit-gate-wiring.md) | The credit gate, wired — how `credit(C)` becomes a consensus fact | 6 | PALW | dormant (credit-overlay lineage, not on the V2 path) | frozen · D palw/lineage |
| [0034](0034-palw-execution-class-model-band-routing.md) | PALW re-verification routing — four execution-class families, five model bands, one deciding… | 40 | PALW | amended (Metal family routing → 0053 D5) | S palw/08 (routing, bands) · D palw/verification |
| [0035](0035-palw-public-testnet-strategy.md) | The public PALW testnet is testnet-11, continued — and it pins its determinism class at the… | 12 | PALW | partial (D1 → 0042; D2 stands) | S palw/16 (testnet-11 row) · D palw/lineage |
| [0036](0036-palw-mainnet-activation-model.md) | PALW mainnet activation — lineage reconciliation and the model that governs | 19 | PALW | partial (D2 not adopted; D4 → 0039) | S palw/16 (mainnet activation) · D palw/lineage |
| [0037](0037-palw-async-job-state-machine.md) | PALW off the block-critical path — an asynchronous, budgeted job state machine over a perman… | 20 | PALW | partial → 0038 (D1); D2–D9 carried | S palw/07 (job state machine) · D palw/lineage |
| [0038](0038-palw-is-the-consensus-work.md) | PALW is the consensus work — sampled-verified LLM PoW, a receipt-licensed weight ramp, and a… | 55 | PALW | amended (W4/W6 → 0039; clock lane 0060/0066/0068) | S palw/06, 13 · D palw/lottery, palw/lineage |
| [0039](0039-palw-only-block-production.md) | PALW-only block production — a Base class instead of a hash floor, and a two-weight fork choice | 30 | PALW | partial (D5 → 0045, 0054, 0137); D1–D4, D6 active | S palw/06, 13 · D palw/lottery |
| [0040](0040-palw-base-0-integer-arithmetic.md) | `PALW-BASE-0` — the integer-only arithmetic normative specification | 27 | PALW | active (+ 0047) | S palw/04 (the BASE-0 arithmetic moves whole) · D palw/execution |
| [0041](0041-palw-pruning-proof-verification.md) | PALW pruning-proof verification — exhaustive and amortised, not sampled | 30 | PALW | active (D1 → D1′ in itself) | S palw/13 (pruning-proof verification) · D palw/liveness |
| [0042](0042-palw-mainnet-candidate-ruleset.md) | The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint | 38 | PALW | active (file says Proposed) | S palw/02, 06, 07, 10, 16 · D palw/lifecycle |
| [0043](0043-palw-v2-state-root-ordering.md) | PALW V2 state-root hash ordering (no challenge↔commitment cycle) | 13 | PALW | active | S palw/02 (state-root order) · keep |
| [0044](0044-palw-free-prompt-receipts.md) | Free-prompt PALW — the user's own inference becomes the consensus work, certified before it… | 35 | PALW | amended (D4 → 0073/0074, D6 → 0055, D7 → 0074, set → 0075) | S palw/06, 11 · D palw/free-prompt |
| [0045](0045-palw-class-economy-on-chain.md) | The class economy is chain state — derived PWU, block-denominated epoch budgets, and the reg… | 13 | PALW | amended (D1 → 0072/0076; D3 → 0054 → 0137) | S palw/05, 06 · D palw/work |
| [0046](0046-palw-v2-consensus-object-carriage.md) | PALW V2 consensus-object carriage: the registrations ride their collateral, the verdicts rid… | 12 | PALW | active | S palw/02 (subnetwork ids, validation layers, bond = collateral output) · D palw/state-and-carriage |
| [0047](0047-palw-a16-activation-tier.md) | The A16 activation tier — sixteen-bit activations for the classes int8 cannot carry | 6 | PALW | active | S palw/04 (A16 tier) · keep |
| [0049](0049-palw-adjudication-contract.md) | The adjudication contract — what a court opens, and the bound that makes it model-size-indep… | 27 | PALW | amended (D-C 2026-08-26; D-H → 0069, 0075) | S palw/09, 03 · D palw/court |
| [0050](0050-palw-base0-residual-site.md) | The BASE-0 residual site — the narrowing that was never declared, and the amplification that… | 10 | PALW | active | S palw/04 (residual site) · D palw/execution |
| [0051](0051-palw-metal-gguf-execution-family.md) | The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified b… | 22 | PALW | superseded → 0053 | frozen · D palw/lineage |
| [0052](0052-palw-qwen36-hybrid-class.md) | `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid graph | 15 | PALW | amended (same-day amendment) | S palw/04 (QWEN36 ops) · D palw/execution |
| [0053](0053-palw-one-execution-family.md) | One execution family — Family M is withdrawn, and the court is not optional | 20 | PALW | active (re-scoped by 0067) | S palw/04, 09 · D palw/execution |
| [0054](0054-palw-share-follows-production.md) | A class's cadence share follows its own production | 8 | PALW | superseded → 0137 (past palw_work_target); D2 → 0068 | frozen · D palw/lineage |
| [0055](0055-palw-position-is-earned-not-declared.md) | Chain position is earned, and the question is set by the block | 5 | PALW | active | S palw/06 (position earned, question set by the block) · keep |
| [0056](0056-palw-permissionless-class-admission-and-share-economy.md) | Permissionless class admission, and the share economy that survives it | 20 | PALW | amended (D4 withdrawn; D6 → 0088) | S palw/03 · D palw/registry |
| [0057](0057-palw-base0-runtime-acceleration.md) | BASE-0 runtime acceleration — backends below the semantic boundary | 9 | PALW | active | S palw/14 (backends below the semantic boundary) · D palw/execution |
| [0058](0058-palw-merged-work-is-counted.md) | Merged work is counted — the mergeset carries claims, not just the chain | 8 | PALW | active | S palw/02, 13 (merged claims count) · keep |
| [0059](0059-the-10b-premine-cap.md) | The 10B premine cap — genesis mints one number, everything else is a carve | 6 | PALW | active (item 4 → 0061) | S palw/10, 16 (premine cap) · keep |
| [0060](0060-the-liveness-doctrine.md) | The liveness doctrine — time is permissionless, weight is bonded, finality is an overlay | 21 | PALW | active (doctrine; D1/D2/D4 in 0066's form) | S palw/13 · D palw/liveness |
| [0061](0061-zero-seat-genesis-and-right-sized-collateral.md) | Zero-seat genesis, and collateral sized by arithmetic instead of history | 7 | PALW | partial → 0151/0152 on testnet-12 (collateral) | S palw/10 (testnet-11 values) · keep |
| [0062](0062-data-availability-court.md) | The data-availability court: stop a vote from taking a bond | 43 | PALW | amended on testnet-12 by 0152 DA (F3) | S palw/08 (DA) · D palw/verification |
| [0063](0063-operator-tooling-the-missing-half.md) | The operator's half of the protocol is missing, and one gap locks money in | 14 | wallet | proposed | D wallet (operator tooling) · S palw/14 cites it |
| [0064](0064-trustless-recovery-from-a-total-stop.md) | Trustless recovery from a total producer stop: the bond becomes usable in the block that reg… | 15 | PALW | partial (its own correction; Facts A and B stand) | S palw/10 (a bond is usable in its block) · D palw/liveness |
| [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) | A bond must be earned, and a failure is not a verdict | 33 | PALW | amended (D2 → D2a; D3 withdrawn) | S palw/10, 13 · D palw/collateral |
| [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) | The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory | 24 | PALW | amended (D2 slot rule → 0142) | S palw/13 · D palw/liveness |
| [0067](0067-classes-are-chain-data-kernels-are-the-build.md) | Classes are chain data; only kernels are the build | 53 | PALW | active | S palw/03, 04 (classes are chain data; the kernel set) · D palw/registry |
| [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) | The LLM-primary economy — the floor retires to the doctrine's minimum | 13 | PALW | active | S palw/10, 16 (the floor's minimum, attempt work) · D palw/lineage |
| [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) | End-to-end adjudicability is the price of weight | 38 | PALW | amended (by 0075; 0144 alignment) | S palw/03, 09 (the weight gate) · D palw/court |
| [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md) | The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them | 14 | PALW | active | S palw/03, 04 (model-tier commitments) · D palw/court |
| [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) | The attempt lane's price, the ticket's bound, and who may judge a class | 30 | PALW | amended (D1 withdrawn; D2 → 0072) | S palw/06, 08 (D1a, ticket bound, seat capability) · D palw/lottery |
| [0072](0072-the-ticket-is-the-execution.md) | The ticket is the execution: both lotteries priced in inferences | 26 | PALW | active | S palw/06 · D palw/lottery |
| [0073](0073-real-demand-work-bears-the-weight.md) | Real-demand work bears the weight | 24 | PALW | partial → 0137; D2 withdrawn (0144); ④ withheld | S palw/11 · D palw/free-prompt |
| [0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) | The attempt is a claim, drawn by the chain | 15 | PALW | partial → 0137 (status amendment) | S palw/05, 06 (beacon draw, quantum) · D palw/lottery |
| [0075](0075-certification-is-a-consensus-object.md) | Certification is a consensus object | 26 | PALW | active | S palw/03 (certification objects) · D palw/registry |
| [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) | The attempt lane's seed is the retarget's own equilibrium | 13 | PALW | superseded → 0137 (past palw_work_target) | frozen · D palw/lineage |
| [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | A prompt a person would type is a claim the court can try | 45 | PALW | amended (0144: public gateway out of scope) | S palw/11 (Phase A) · D palw/free-prompt |
| [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) | What was made from it is committed; the thing itself never rides | 35 | PALW | amended (D7 withdrawn by 0144) | S palw/11 (derived artifacts committed) · D palw/free-prompt |
| [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | A pure function needs no permissions — the sandbox is for the host, and the chain never take… | 38 | PALW | amended (0144: Done-when withdrawn) | S palw/14 (host sandbox) · D palw/node |
| [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) | The answer is long; the verified unit is short | 23 | PALW | partial → 0082 | frozen · D palw/court |
| [0081](0081-long-context-the-input-is-a-state-chain.md) | Long context — the input is a state chain | 25 | PALW | partial → 0082 | S palw/04 (the remainder) · D palw/held-context |
| [0082](0082-the-close-is-flat-in-the-context.md) | The close is flat in the context — attention is refuted by dissection, the capture is a fold… | 79 | PALW | active | S palw/04, 09, 11 · D palw/court, palw/held-context |
| [0083](0083-the-difficulty-window-counts-only-rows-priced-by-bits.md) | The difficulty window counts only rows priced by `bits` — heartbeat emitters are not work | 8 | PALW | active | S palw/06 (difficulty window) · keep |
| [0084](0084-the-ids-ride-the-capture-stays-home.md) | The ids ride, the capture stays home — a model-class claim serves its answer, never its history | 35 | PALW | active | S palw/11 (FPA1 served answer) · D palw/free-prompt |
| [0085](0085-the-close-is-assembled-from-what-was-served.md) | The close is assembled from what the executor served — a disputed tile, not a capture | 17 | PALW | active | S palw/09 (close assembly) · D palw/court |
| [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) | the opening carries the fold, not the leaves | 22 | PALW | active | S palw/09 (interval opening) · D palw/court |
| [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) | a position is bought from the curve and sold back to it | 33 | PALW | amended (0088, 0089, 0090, 0091, 0114) | S palw/15 · D palw/market |
| [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | the class keeps its graph; a line keeps its owner, and the owner keeps publishing | 44 | PALW | active | S palw/15, 03 (lines, roots in force) · D palw/market |
| [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) | the fold is the truth; the EVM is its window and its hand | 54 | PALW | active | S palw/15 + S evm (precompiles, writer) · D palw/market |
| [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md) | The pair is seeded with real MSK, locked for good, and a position is whole | 15 | PALW | amended (least seed → 0120) | S palw/15 · D palw/market |
| [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) | The reward buys the pair, and no holder is paid | 22 | PALW | active | S palw/15, 10 (5 % buys the pair at Final) · D palw/market |
| [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) | The ladder is minted once, and the wall clock is what binds | 18 | PALW | active | S palw/09 (court ladder, clock) · D palw/court |
| [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) | The court can try a fused row; the responder is what is missing | 28 | PALW | active | S palw/09, 14 (fused row, responder) · D palw/court |
| [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md) | A seed is paid in as many transactions as it takes | 13 | PALW | active | S palw/15 (multi-transaction seed) · D palw/market |
| [0095](0095-a-position-is-a-membership-not-an-income.md) | A position is a membership, not an income | 20 | PALW | proposed | D palw/market · S palw/15 when built |
| [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md) | The app you already use is the entrance, and the shape of the answer is committed | 51 | PALW | proposed (Part A built) | S palw/11, 14 (answer shape, local surface) · D palw/free-prompt |
| [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) | A model's fit is a lookup, and the entrance says its limits before the first token | 35 | PALW | active | S palw/14 (fit lookup) · D palw/node |
| [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md) | The panel's coverage is a number, and a seat that found a lie files nothing else | 20 | PALW | active | S palw/08 (coverage), 09 (the lie filer) · D palw/verification |
| [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) | The adder measures, the chain recomputes, and a seat holds a shard | 26 | PALW | active (D5 fence dormant) | S palw/03, 08 (recomputation, shard seats) · D palw/verification |
| [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | A model is data: the one-move court, the held measurement and the licence per shard, and the… | 32 | PALW | active | S palw/03, 08, 09 · D palw/registry, palw/court |
| [0101](0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) | A membership is proven by the chain and served by anyone, and a Position never moves between… | 13 | PALW | amended (D4/D6/D7 withdrawn by 0144) | S palw/15 (membership proof) · D palw/market |
| [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md) | The embedding lift is read per token, and a kernel a network has not armed is not in its ide… | 17 | PALW | active (fence dormant) | S palw/04 (embedding lift) · D palw/execution |
| [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) | The context is held off the chain, and the chain carries a root, an opening and a logarithm | 69 | PALW | active | S palw/04, 09 (held context) · D palw/held-context |
| [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md) | A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has la… | 43 | PALW | active | S palw/13 (bonded block never red; transparency) · D palw/liveness |
| [0106](0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) | An inventory is a stream of leaves, not a copy of the model | 16 | PALW | active | S palw/14 (inventory stream) · D palw/node |
| [0107](0107-a-share-grows-on-work-that-reached-final.md) | A class's share grows on work that reached Final, not on blocks that were accepted | 9 | PALW | superseded → 0137 | frozen · D palw/lineage |
| [0108](0108-an-extension-is-a-manifest-the-verifier-recomputes-and-a-receipt-is-evidence-not-a-vote.md) | An extension is a manifest the verifier recomputes, and a receipt is evidence, not a vote | 36 | PALW | active | S palw/08 (receipt = evidence), 14 (extension manifest) · D palw/node |
| [0109](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) | A lock is its own claim, and finality is a label, not a pause | 19 | bridge | active | S bridge · D bridge |
| [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) | A context limit is activated from reproducible public vectors, not the maintainer's workstation | 26 | PALW | active | S palw/03, 16 (context-limit activation) · D palw/held-context |
| [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) | A seat may demand the committed leaf it needs to judge | 23 | PALW | active | S palw/08, 09 (leaf demand) · D palw/verification |
| [0112](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) | A class's weights are read within a budget the operator states, and the budget is a fifth of… | 30 | PALW | active | S palw/14 (weights read budget) · D palw/node |
| [0114](0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) | The owner's leg is five percent, and it arrives at a height | 8 | PALW | active | S palw/15 (owner's leg) · keep |
| [0115](0115-a-pending-transaction-is-announced-until-it-lands.md) | A pending transaction is announced until it lands | 5 | EVM | active | S evm (pending-transaction announcement) · keep |
| [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md) | An attention history is the class's, and the held regime reduces over its own width | 11 | PALW | active | S palw/04 (attention history) · D palw/held-context |
| [0117](0117-a-draw-is-one-forward.md) | A draw is one forward | 17 | PALW | active | S palw/04, 06 (a draw is one forward) · D palw/execution |
| [0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) | The held regime arrives at a height, and a held class carries its own prompt form | 20 | PALW | active | S palw/03, 04, 16 (held regime) · D palw/held-context |
| [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | A held class is walked at the regime's ladder, and the chain records which classes those are | 17 | PALW | active | S palw/03, 09 (held ladder) · D palw/held-context |
| [0120](0120-the-least-seed-is-one-million-msk-and-it-arrives-at-a-height.md) | The least seed is one million MSK, and it arrives at a height | 5 | PALW | active | S palw/15 (least seed) · keep |
| [0121](0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) | A held capture is served from its fold as the replay streams, and a node holds two ladders | 20 | PALW | active | S palw/14 (held capture streaming) · D palw/held-context |
| [0122](0122-mining-is-a-purpose-an-operator-runs-one-command-and-reads-one-work-id.md) | Mining is a purpose: an operator runs one command and reads one work id | 63 | PALW | proposed (CLI partly built) | S palw/14 (operator interface) · D palw/node |
| [0123](0123-the-epoch-progressively-releases-unused-class-budget.md) | The epoch progressively releases unused class budget | 3 | PALW | active (dormant on shipped presets) | S palw/06 (epoch budget release) · keep |
| [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | The panel is paid out of the claim's reward, a seat holds exposure, and a claim is paid for… | 32 | PALW | amended (D5 lottery → 0152 SW on testnet-12) | S palw/08, 10 (panel economy) · D palw/verification |
| [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) | The execution lane is a second lane inside the cadence, and it widens one permit at a time | 27 | PALW | active | S palw/12 · D palw/exec-lane |
| [0126](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) | The validator carve drops to a fifth, and the stake reorg gate stays | 12 | DNS-BFT | active | S dns-bft (stake reorg gate) + S palw/10 (carve) · D dns-bft |
| [0127](0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md) | PALW settles on its own, and its terms are not DNS terms | 6 | PALW | active | S palw/01, 07 (PALW settles on its own) · keep |
| [0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate | 21 | DNS-BFT | active | S dns-bft · D dns-bft |
| [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) | A double spend needs the anchors, not the blocks | 9 | PALW | active | S palw/07, 12 (anchors, not blocks) · keep |
| [0130](0130-bps1-is-hardened-before-it-is-widened.md) | BPS 1 is hardened before it is widened | 20 | PALW | amended (D7–D8 deferred by 0144; lottery → 0152 SW) | S palw/08, 12 · D palw/exec-lane |
| [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) | A claim is paid for the compute it cost, in economic compute, not leaves | 16 | PALW | amended (shadow only; D3–D6 not to be armed) | S palw/05 (shadow note) · D palw/work |
| [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | What a model is actually paid per forward it ran, and why the gap is liveness before it is p… | 33 | PALW | active | S palw/06, 10 (single lottery, payout) · D palw/work |
| [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md) | Verification is its own clock: a class verifies over spans, and a starved class stops only i… | 41 | PALW | active | S palw/08 (verification V2, readiness V2) · D palw/verification |
| [0134](0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) | The compute overlay's committee beacon retires at a height, and its machinery goes | 10 | DNS-BFT | active | S dns-bft (overlay retirement) · keep |
| [0135](0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | A model is data: the permissionless registry derives its profile, proves its panel, and walk… | 34 | PALW | amended (D5 → 0137) | S palw/03 (registry, lifecycle) · D palw/registry |
| [0136](0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) | An artifact is mapped, not read, and a host holds one copy of it | 7 | PALW | active | S palw/14 (artifact mapping) · keep |
| [0137](0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) | A block buys one unit of work from any model, and a share is a result, not an input | 51 | PALW | active | S palw/05, 06, 10 (work target W, issuance) · D palw/work |
| [0138](0138-the-daa-score-is-the-anchors-clock.md) | The DAA score is the anchor's clock: a block advances it iff `bits` priced it | 10 | PALW | active | S palw/06 (anchor clock) · keep |
| [0139](0139-the-execution-lanes-gas-is-one-budget-a-round.md) | O13 decided: the execution lane's gas throughput scales with the lane, one budget a round | 7 | PALW | active | S palw/12 (gas per round) + S evm · keep |
| [0140](0140-the-heartbeat-is-the-emergency-generator.md) | The heartbeat is the emergency generator: it must not touch the economy or the difficulty wh… | 14 | PALW | active (doctrine; changes no rule) | S palw/13 (heartbeat invariant) · D palw/liveness |
| [0141](0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | Can an inference be the ticket without a hash lottery? | 7 | PALW | record (decides nothing; reads as an RFC) | D palw/lottery (open question) · rfc/README lists it |
| [0142](0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) | The consensus clock is a cursor: a heartbeat consumes a slot, and a block that does not adva… | 22 | PALW | active | S palw/06, 13 (clock cursor) · D palw/liveness |
| [0143](0143-an-artifact-root-has-one-owner-on-the-chain.md) | An artifact root has one owner on the chain, and competing weights stay permissionless | 13 | PALW | active | S palw/03 (artifact-root ownership) · D palw/registry |
| [0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) | PALW pays for the inference you were going to run anyway | 17 | PALW | constitution | S palw/01 (P1–P7, §3, §4) · D palw/principles |
| [0145](0145-canonical-work-is-derived-and-admission-is-earned.md) | Canonical work is derived, not declared; admission is earned, not registered | 14 | PALW | active (implemented, fenced) | S palw/03, 05, 11 · D palw/work |
| [0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) | A coefficient is justified by the arbitrage it permits, not by being right | 16 | PALW | active | S palw/05 (no scalar coefficient) · D palw/work |
| [0147](0147-independence-is-drawn-not-declared.md) | Independence is drawn, not declared | 13 | PALW | active | S palw/03, 08 (independent admission) · D palw/registry |
| [0148](0148-the-free-prompt-lane-prices-compute.md) | The free-prompt lane prices compute, and prices it the same for every class | 14 | PALW | active | S palw/05, 11 (free-prompt pricing) · D palw/work |
| [0149](0149-an-attempts-pwu-is-the-derivation.md) | An attempt's pwu is the derivation, so the weight reads it directly | 9 | PALW | active | S palw/05 (attempt pwu) · keep |
| [0150](0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) | The fingerprint must see the RULE, not only the height | 11 | network | active | S network + S palw/16 (rule manifest) · keep |
| [0151](0151-liveness-is-structural-collateral-covers-fraud.md) | Liveness is structural; collateral covers fraud | 23 | PALW | active (D1 runtime half open) | S palw/10, 13 · D palw/collateral |

## The other files

| File | KB | Domain | Status | Content goes to |
| --- | --: | --- | --- | --- |
| [0134-testnet-11-subnetwork-census-2026-09-17.json](0134-testnet-11-subnetwork-census-2026-09-17.json) | <1 | DNS-BFT | evidence (ADR-0134) | stays next to ADR-0134, or evidence/ with a link |
| [README.md](README.md) | 145 | all | index | slim in Phase 2 (below) |
| [STATUS-AUDIT-2026-09-18.md](STATUS-AUDIT-2026-09-18.md) | 114 | PALW | audit | **moved 2026-09-27** → [audit/2026-09-18-status.md](../audit/2026-09-18-status.md) · stub left |
| [STATUS-AUDIT-2026-09-19-llm-mining-reward.md](STATUS-AUDIT-2026-09-19-llm-mining-reward.md) | 14 | PALW | audit | **moved 2026-09-27** → [audit/2026-09-19-llm-mining-reward.md](../audit/2026-09-19-llm-mining-reward.md) · stub left |
| [STATUS-AUDIT-2026-09-20-reward-reaudit.md](STATUS-AUDIT-2026-09-20-reward-reaudit.md) | 11 | PALW | audit | **moved 2026-09-27** → [audit/2026-09-20-reward-reaudit.md](../audit/2026-09-20-reward-reaudit.md) · stub left |

## Cited, but not on this branch

Code and documents cite these numbers, but no file for them is on `main`. The first two carry the
rules testnet-12 runs today and the change it runs next, so they are Phase 2 prerequisites.

| Number | What it is | Where it is | Plan |
| --- | --- | --- | --- |
| **0152** | R-core+ v3.1: what testnet-12 runs (account stake, the staged reservation, vested rewards, seat locks, the action-based slash schedule, attribution J, the redesigned DA court, quorum counting Q, the deadline function DL, and the stake-weighted draw SW-1…SW-10) | **imported 2026-09-27** from `docs/adr-0152-v31-postedits` at `9ed1adced` | **Done:** short ADR at [0152](0152-account-stake-staged-reserve-and-vested-rewards.md); the full text is split verbatim into [design/palw/archive/0152/](../design/palw/archive/0152/README.md) (11 parts, reassembly checked); the rules are in spec 07–10, 13, 16 and 02; the reasoning is in [design/palw/collateral.md](../design/palw/collateral.md) and [verification.md](../design/palw/verification.md) |
| **0160** | Claim capacity separated from collateral price (v3): the bond is one shared guarantee split four ways, ρ is a risk tier, and a credited claim is paid only after an independent audit. An accepted design, being implemented (`rcore/cap-*`, the cap-s1 merges) | branch `rcore/cap-spec2` at `ccd5499c8` (2026-09-26). 121 KB, plus `docs/adr/0160-capacity/` (calculators and numbers) | **Placeholder — WAIT** (decision 2026-09-27): imported after the int-6 integration in its final form (stage 4 of the implementation departed from the design). Then: a short ADR-0160, the body to `D palw/claim-capacity`, its fences to `S palw/10` and `16` |
| 0153 | Reserved: the flag day that installs measured verification rows (long-D classes, the 2M split). Cited by `palw_class_verify_deadline_v1.rs`, `palw_state_v2.rs` and RFC-0001 | not written | Stays reserved |
| 0104 | Reserved for "a close too wide for one carrier is cut once" (the 0102 collision in README "Number hygiene"). ADR-0144 §9: "never written" | not written | Stays reserved |
| 0048 | Unused on the live lineage (README "Number hygiene") | — | Stays unused |
| 0113 | No file, and no citation in the tree | — | Unassigned. Record it in the index before anyone uses it |
| 0154, 0155 | **Taken 2026-09-27**: the DAA-750 and DAA-1,300 flag-day ADRs (decision 4) | [0154](0154-testnet-12-flag-day-daa-750.md), [0155](0155-testnet-12-flag-day-daa-1300.md) | Done |
| 0156–0159 | No file and no citation. 0160 is taken out of order | — | Free. The next number is 0156 (the DAA-1,500 flag day, when it comes) |
| RFC-0001 | PALW inference surface gaps (FP Job V4) | branch `rcore/fp-sampler` | Reserved in [rfc/README.md](../rfc/README.md). Not copied |

## Plan for `README.md` (Phase 2)

The index is 145 KB because it carries history as well as the index. It keeps the parts only an
index can hold, and the rest moves out:

| README section | Goes to |
| --- | --- |
| "The direction that governs the PALW lineage (2026-09-02)" | `D palw/lineage` |
| "Superseded, in whole or in part" (the supersede map) | **stays**, extended to 0150–0152 and 0160 |
| "Number hygiene" | **stays**, with 0104/0113/0153/0154 from the table above |
| "Activation axis (2026-09-06)" and the testnet-11 activation map | `S palw/16` (the testnet-11 section) and `history/testnet-11.md` |
| "What the current direction still owes" | `D palw/lineage` (open items). Items still open become rows in `spec/palw/divergences.md` or RFCs |
| "What to build next (0144-alignment)" | `D palw/principles` (order of work) |
| "Security amendments (2026-09-02)" | `D palw/lineage`. The amendments themselves stay at the end of each ADR |
| "Still governing, unreversed" and "Added after the 2026-09-02 pass" | replaced by one table: number, title, status, Spec chapter. The long status texts move into the ADRs' own banners |

## Gaps found in the record while taking this inventory

1. The index tables stop at ADR-0149. **0150 and 0151 are not indexed.**
2. **The rules testnet-12 runs (0152) and its next change (0160) are not on `main`.** The architecture
   overview says so for 0152. Until they land, `params.rs` and the launch note are the only written
   record.
3. **The 15 post-launch fences have no ADR**: the DAA-750 set of 13 and the DAA-1,300 set of 2.
   Neither do the strict economic win with its two-tick tie rule, the operator anchor and the
   floor-refusal retry. Their written record is the doc comments in
   `consensus/core/src/config/params.rs` (`PALW_T12_POST_LAUNCH_FENCES_V1` / `_V2`), the
   [launch note](../t12-launch-2026-09-25.md) §000–§00, `contrib/t12-deploy-kit/DAA750-ROLLOUT.md`
   and, for the panel seed, [t12-panel-seed-2026-09-25.md](../t12-panel-seed-2026-09-25.md). That
   last document is a correction to ADR-0152 SW-8 that was never copied into the ADR. `spec/palw/16`
   becomes their first normative home. One short ADR per flag day would close the gap.
4. Several status lines disagree with the index. 0025 and 0042 say *Proposed* but are cited as
   governing. 0050 says *Proposed* (blocked on 0049) but is listed as governing. 0122 says *Proposed*
   although its CLI exists.
5. **ADR-0144 has seven principles, not five**: P6 (eligibility is scarce and protocol-assigned) and
   P7 (admission is permissionless, and weight is earned through verified use) follow P1–P5. The
   PALW spec skeleton uses all seven.
6. Known and already recorded: 0020's "EVM is opt-in" is stale (0089 §2). 0065's filename keeps a
   withdrawn clause (README "Number hygiene").

## Progress (Phase 2)

One line per step. "Slimmed" means the body moved verbatim to `design/palw/archive/`, and the ADR is
now Context / Decision / Consequences / Links.

- **Mechanical moves.** The STATUS-AUDIT files moved to `audit/`, the ten frozen ADRs gained banners,
  and the index gained 0150/0151.
- **ADR-0152** imported and split: the archive is in `design/palw/archive/0152/`, the rules are in spec
  02/07–10/13/16.
- **Chapter 16** written. ADR-0154 and ADR-0155 were written. Slimmed: 0036, 0035.
