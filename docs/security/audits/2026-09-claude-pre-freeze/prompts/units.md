# Audit units

Each unit was reviewed by one fresh agent that could not see any other unit's work. Units in the
same group ran in the same workflow; the three groups ran as three separate workflows. After each
group, a completeness critic (given only each unit's scope statement and finding *titles*) proposed
up to three gap units, which were run the same way; they are listed in the findings files.

| group | unit | domain | title |
|---|---|---|---|
| A | `CONS-U1` | Consensus | Block and transaction validation pipeline |
| A | `CONS-U2` | Consensus | Virtual processor, PALW fold application, state roots and reorg |
| A | `CONS-U3` | Consensus | Fork choice, clock, DAA, difficulty, heartbeat, GHOSTDAG |
| A | `CONS-U4` | Consensus | Parameters, activation fences, fingerprint, genesis, handshake, pruning and IBD |
| A | `CONS-U5` | Consensus | DNS finality, BFT votes and the stake reorg gate |
| A | `CONS-U6` | Consensus | EVM lane, execution lane, precompiles and the bridge ledger |
| A | `CRYP-U1` | Crypto | ML-DSA-87 use, txscript checksig, sighash, addresses, Hash64 |
| A | `CRYP-U2` | Crypto | Randomness, seeds, commitments and cross-object domain separation in PALW |
| B | `PALW-U1` | PALW | Attempt, lottery, work derivation and admission |
| B | `PALW-U2` | PALW | Panels, seats, receipts, readiness and licences |
| B | `PALW-U3` | PALW | Court, disputes, bisection, slashing and offence attribution |
| B | `PALW-U4` | PALW | Settlement, rewards, vesting, bond and stake accounting, issuance |
| B | `PALW-U5` | PALW | Model registry, class admission, lifecycle, activation pool and artifacts |
| B | `PALW-U6` | PALW | Model market, stores, positions and benefits |
| B | `PALW-U7` | PALW | Free prompts, held context, prompt canonicalization, output commitment, gateway |
| B | `PALW-U8` | PALW | BASE-0 deterministic arithmetic, step and leg execution |
| C | `NET-U1` | Network | P2P protocol, flows, IBD and PALW gossip relay |
| C | `NET-U2` | Network | Mempool, block templates and PALW carriers |
| C | `NET-U3` | Network | RPC surfaces (wRPC, gRPC, eth JSON-RPC) |
| C | `NET-U4` | Network | Validator, remote signer, node duties and secrets |
| C | `REL-U1` | Release | CI, release pipeline, reproducibility and supply chain |
| C | `THREAT-U1` | Threat | Architecture and threat-model review |
| C | `PRIOR-U1` | Closure | Prior-audit closure: September 2026 audits |
| C | `PRIOR-U2` | Closure | Prior-audit closure: June-August 2026 audits and external reviews |

## CONS-U1 — Block and transaction validation pipeline

**Files.** consensus/src/pipeline/header_processor/, consensus/src/pipeline/body_processor/, consensus/src/processes/transaction_validator/, consensus/src/processes/coinbase.rs, consensus/core/src/{tx.rs,block.rs,header.rs,coinbase.rs,mass/,hashing/,merkle.rs,muhash.rs,subnets.rs,token.rs,blockhash.rs,errors/}

**Focus.** Acceptance of invalid blocks / transactions; coinbase and issuance checks; mass and DoS limits; header fields added by MISAKA (EVM roots, PALW commitments) and whether each is validated before use; UTXO commitment (LtHash / 64-byte commitment); transaction and header hashing identity (Hash64) and malleability; unknown version / unknown payload handling; checks done in isolation vs in UTXO context and anything skipped on one path.

## CONS-U2 — Virtual processor, PALW fold application, state roots and reorg

**Files.** consensus/src/pipeline/virtual_processor/processor.rs, consensus/src/pipeline/virtual_processor/utxo_validation.rs, consensus/src/consensus/mod.rs, consensus/src/model/stores/palw_state_v2.rs, consensus/src/processes/palw_state_v2_sync.rs, consensus/core/src/{palw_v2.rs,palw_carriage.rs,palw_facts.rs,palw_block_commitment.rs}, the block-apply entry points of consensus/core/src/palw_state_v2.rs

**Focus.** How a block's PALW objects are folded into state and how the fold is undone on reorg; whether the fold is a pure function of chain data (no HashMap/HashSet iteration order, floats, wall clock, thread timing, local node config reaching the result); which object errors drop the object vs invalidate the block, and whether that choice is identical on every node; state-root commitment ordering (ADR-0043) and whether every stored field is committed; virtual-state recomputation after reorg / IBD; caches that could survive a reorg.

## CONS-U3 — Fork choice, clock, DAA, difficulty, heartbeat, GHOSTDAG

**Files.** consensus/core/src/{palw_fork_choice.rs,palw_chain_weight.rs,palw_weight.rs,palw_clock_cursor_v1.rs,palw_heartbeat_v1.rs,palw_heartbeat_carriers_v1.rs,palw_work_target_v1.rs,palw_class_daa.rs,palw_fork_authority_v2.rs,pow_layer0.rs,daa_score_timestamp.rs}, consensus/src/processes/{difficulty.rs,ghostdag/,window.rs,parents_builder.rs,reachability/}, consensus/pow/

**Focus.** Fork-choice weight and whether any input to it is attacker-chosen or cheap (ADR-0137, ADR-0142, ADR-0105, ADR-0083); timestamp and clock rules including the 132 s lead cap (a node-local clock reaching a consensus decision is a divergence risk — check it is only a transient reject that is never cached); difficulty / work-target retarget manipulation; heartbeat weight vs bonded work; merge-depth and finality-depth rules. The heartbeat-transparency double spend is KNOWN; look for other fork-choice or clock attacks, or show the known one is broader.

## CONS-U4 — Parameters, activation fences, fingerprint, genesis, handshake, pruning and IBD

**Files.** consensus/core/src/config/{params.rs,genesis.rs,premine.rs,trusted_checkpoint.rs,constants.rs,bps.rs}, consensus/core/src/{fork_id_v1.rs,palw_rule_manifest_v1.rs,palw_rc_identity_v2.rs,palw_genesis_v2.rs,network.rs,pruning.rs}, consensus/src/processes/pruning_proof/, consensus/src/pipeline/pruning_processor/, consensus/src/processes/sync/, protocol/flows/src/v8/ (handshake / fork id), kaspad/src/daemon.rs (identity lines)

**Focus.** Does consensus_params_id() / the fingerprint cover every value and every rule that affects consensus (ADR-0150: the fingerprint must see the rule, not only the height)? Find any consensus-affecting parameter or code-path switch that is NOT in the fingerprint, so two nodes with the same fingerprint could run different rules. Fence comparisons (>= vs >, DAA score vs blue score, activation in the block that crosses it) and their consistency across call sites. Genesis / premine pinning. Pruning proof and IBD: can a syncing node be fed a pruning point / PALW state snapshot that it accepts without re-deriving? Old-node / new-node interaction at a fence.

## CONS-U5 — DNS finality, BFT votes and the stake reorg gate

**Files.** consensus/core/src/{dns_finality.rs,dns_bft_v1.rs,vlt.rs}, consensus/src/pipeline/virtual_processor/dns_bft.rs, the attestation / precommit verification paths they call

**Focus.** Reorg-gate bypass; stake counting (who counts, double counting, stake that moved or unbonded, equivocation); attestation / precommit replay across anchors, epochs or networks; signature and context checks on votes; the Bootstrap state (KNOWN that the gate is not in force until validators are funded) — check the exit condition cannot be forced, faked or reached inconsistently by different nodes; any path where a node-local view (arrival order, local time) decides the gate.

## CONS-U6 — EVM lane, execution lane, precompiles and the bridge ledger

**Files.** consensus/core/src/evm/, consensus/src/processes/evm/mod.rs, consensus/src/model/stores/evm.rs, kaspa-evm/src/, consensus/core/src/{palw_execution_lane_v1.rs,palw_execution_quanta_v1.rs}, kaspad/src/palw_round_producer.rs

**Focus.** EVM state transition determinism (revm config, gas schedule, block env derived from chain data only); precompiles that read or write the PALW fold — authorization, reentrancy, value conservation between UTXO/fold and EVM balances (the bridge ledger); gas one-budget-a-round (ADR-0139); round-block permits; replay of EVM transactions across networks (chain id); pruned-IBD EVM overlay snapshot trust (ADR-0022). Bridge audit BR-1..BR-7 fixes are KNOWN as scheduled; report only what is not already one of those, or where you cannot tell, say so.

## CRYP-U1 — ML-DSA-87 use, txscript checksig, sighash, addresses, Hash64

**Files.** consensus/core/src/{mldsa87_primitives.rs,sign.rs,hashing/}, crypto/txscript/, crypto/addresses/, crypto/hashes/, crypto/merkle/, crypto/muhash/, kaspa-pq-validator-core/ (signing contexts), wallet/keys/ and wallet/pq-cli/ (key generation only)

**Focus.** Signature verification bypass or malleability; length / encoding checks on keys and signatures; the ML-DSA context string and any domain prefix per purpose (transaction, attestation, receipt, round permit, unbond, takeover…) — is every signed object type separated from every other?; sighash coverage (what a signature does NOT commit to); address derivation (keyed BLAKE2b-512) and version bytes; Merkle tree second-preimage / odd-leaf duplication; LtHash / MuHash set commitment edge cases; verification caching keyed on something weaker than (key, message, signature, context).

## CRYP-U2 — Randomness, seeds, commitments and cross-object domain separation in PALW

**Files.** consensus/core/src/palw_* seed / draw / beacon / commitment code (grep: seed, beacon, draw, vrf, sortition, domain, DOMAIN, tag, personal, blake2b, sha3), consensus/core/src/{palw_block_commitment.rs,palw_rc_identity_v2.rs,palw_job_identity.rs,palw_prompt_ids_v1.rs,palw_fp_beacon_v3.rs,palw_receipt.rs}

**Focus.** Every randomness source feeding a consensus decision (lottery, panel draw, sampling, court challenge selection, beacon) — who can bias it, grind it or withhold to re-roll it (panel-seed grinding by the anchor producer is KNOWN; find others or show the known one reaches further, e.g. lottery or court sampling). Hash-input framing for PALW objects: fixed vs variable-length fields without length prefixes, shared domain tags between object types, identifiers that two different objects can share. Receipt / claim signature messages: does the signed message bind network, class, job, claim and role?

## PALW-U1 — Attempt, lottery, work derivation and admission

**Files.** consensus/core/src/{palw_attempt_v2.rs,palw_attempt_rules_v1.rs,palw_admission_v2.rs,palw_canonical_work_v1.rs,palw_economic_compute_v1.rs,palw_pwu.rs,palw_job_identity.rs,palw_job_state.rs,palw_job_ledger.rs,palw_producer_v2.rs,palw_mode_v2.rs,palw_routing.rs,palw_schedule.rs,palw_prompt_ids_v1.rs}, kaspad/src/palw_producer.rs, and their call sites in palw_state_v2.rs

**Focus.** Job and claim uniqueness (can the same job / prompt / attempt be claimed twice, by the same or different bonds, across blocks or across a reorg?); work forgery (does admission re-derive pwu / canonical work, ADR-0145 / ADR-0149, or trust a declared value anywhere?); lottery ticket validity vs the work target; producer eligibility and bond checks at admission; grinding of attempts.

## PALW-U2 — Panels, seats, receipts, readiness and licences

**Files.** consensus/core/src/{palw_panel_v2.rs,palw_panel_var_v1.rs,palw_panel_view_v1.rs,palw_panel_da_v1.rs,palw_panel_economy_v1.rs,palw_job_panel.rs,palw_receipt.rs,palw_seat_coverage_v1.rs,palw_readiness_escalation_v1.rs,palw_shard_panel_v1.rs,palw_shard_plan_v1.rs,palw_shard_licensing_v1.rs,palw_optimistic_licence_v2.rs,palw_verification_v2.rs,palw_verification_profile_v1.rs,palw_exposure.rs}, kaspad/src/{palw_panel.rs,palw_receipt_pool.rs,palw_readiness_escalation.rs}

**Focus.** Receipt uniqueness and replay (same receipt counted twice, a receipt for claim A counted for claim B, a receipt from a seat not drawn for that claim); seat independence (ADR-0065 a seat must be someone else, ADR-0147 independence is drawn) — can a producer sit on its own panel via a second bond?; licence issuance conditions; readiness proofs; the 500‰ exposure budget accounting; the stake-weighted draw. Panel-seed grinding and V04 are KNOWN.

## PALW-U3 — Court, disputes, bisection, slashing and offence attribution

**Files.** consensus/core/src/{palw_court_v2.rs,palw_court_deadline.rs,palw_dispute.rs,palw_bisect.rs,palw_step_refute.rs,palw_replay_refute_v1.rs,palw_shard_court_v1.rs,palw_da_rcore_v1.rs,palw_held_da_v1.rs,palw_offence_v1.rs,palw_offence_attribution_v1.rs,palw_slash.rs,palw_false_valid_filing_v1.rs,palw_attn_court_v1.rs,palw_attn_responder_v1.rs,palw_attn_dissect.rs,palw_checkpoint_court_v1.rs,palw_leaf_evidence_v1.rs,palw_terminal.rs,palw_adversarial.rs}, kaspad/src/{palw_reporter_filer.rs,palw_filer_*.rs}

**Focus.** Court state machine: can a dispute be opened twice, closed early, answered by the wrong party, or timed out in the liar's favour? Can a guilty claim reach Final and be paid while a valid dispute is pending? Can an honest party be slashed (bad attribution, a responder who is "missing" because of censorship)? Is a slash applied exactly once and to the right bond, and can the bond be withdrawn / unbonded before the slash lands? Deadlines vs DAA clock manipulation. Bisection termination and the single-step adjudication (does the court recompute the step from committed data only?).

## PALW-U4 — Settlement, rewards, vesting, bond and stake accounting, issuance

**Files.** consensus/core/src/{palw_settlement_v1.rs,palw_reward_v2.rs,palw_reward_properties_v1.rs,palw_vesting_v1.rs,palw_vesting_read_v1.rs,palw_economic_payout_v1.rs,palw_economics_ledger_v1.rs,palw_economic_locus_v1.rs,palw_economic_safety_v1.rs,palw_credit.rs,palw_credit_batch.rs,palw_arbitrage_search_v1.rs}, bond / stake / staged-reserve rows in palw_state_v2.rs, consensus/src/processes/coinbase.rs, consensus/core/src/config/premine.rs

**Focus.** Conservation of supply: every sompi paid must come from the schedule, an escrow, a fee or a slash, exactly once. Double payment of a claim / panel / validator carve; payment before Final; rounding direction and remainders (dust that is minted, or lost, or split differently on different nodes); overflow in u64/u128 products; vesting release before the schedule; bond withdrawal while exposure or a dispute is outstanding; the staged reserve and account stake (ADR-0152, not committed — the code is the only record).

## PALW-U5 — Model registry, class admission, lifecycle, activation pool and artifacts

**Files.** consensus/core/src/{palw_model_registry_v1.rs,palw_model_registration_v1.rs,palw_class_admission_v2.rs,palw_activation_pool_v1.rs,palw_class_identity_v1.rs,palw_class_verify_deadline_v1.rs,palw_lifecycle_objects_v2.rs,palw_artifact.rs,palw_registry.rs,palw_public_model_source_v1.rs,palw_model_fit_v1.rs,palw_measured_model_v1.rs,palw_resource_profile_v1.rs,palw_catalog_coverage.rs,palw_e2e_adjudicability.rs,palw_derived_v1.rs,palw_service_descriptor_v1.rs}, consensus/core/src/config/class-manifests/

**Focus.** Malicious model owner: can a registrant choose metadata that moves price, weight or share (the F1-F7 class of the 2026-09-19 audit — verify those closures hold on testnet-12 AS SHIPPED), register a class it cannot be judged on, take over another owner's artifact root (ADR-0143), skip lifecycle stages, or starve other classes (Activation Pool silence reclamation, verify deadlines)? Is every derived profile recomputed by consensus rather than declared?

## PALW-U6 — Model market, stores, positions and benefits

**Files.** consensus/core/src/{palw_model_market_v1.rs,palw_model_lines_v1.rs,palw_model_benefits_v1.rs}, consensus/core/src/evm/model_market.rs, kaspa-evm/src/model_market.rs, contracts/misaka-model/

**Focus.** Curve arithmetic (buy / sell price, rounding direction, a buy-then-sell round trip that returns more than it cost), the 5% burn and 5% owner legs applied exactly once, the locked seed (ADR-0090 / ADR-0120) — can it be withdrawn?, whole positions (ADR-0090 / 0095 / 0101) — can a position be split, transferred or double-sold?, reward buy-in (ADR-0091) — can a holder be paid?, store opening deposits, and consistency between the fold and the EVM view / hand (ADR-0089). Position fixes (sink binding, quoteSell gross) are KNOWN as scheduled.

## PALW-U7 — Free prompts, held context, prompt canonicalization, output commitment, gateway

**Files.** consensus/core/src/{palw_freeprompt_v3.rs,palw_fp_admission_v3.rs,palw_fp_beacon_v3.rs,palw_fp_execution_v3.rs,palw_fp_objects_v3.rs,palw_fp_interval_v1.rs,palw_fp_devnet_v3.rs,palw_decode_constraint_v1.rs,palw_decode_select_v2.rs,palw_context_ladder.rs,palw_held_context_v1.rs,palw_segment_resume_v1.rs,palw_layer_sample_v3.rs}, misaka-palw-gateway/, misaka-palw-fp-submit/, misaka-palw-constraint/, kaspad/src/palw_fp_seat.rs

**Focus.** Prompt canonicalization (two different byte strings that canonicalize to one prompt id, or one prompt with two ids — double credit / cache credit abuse); model output commitment (can the producer change the answer after commitment, or commit to an answer the court cannot check?); held context root / opening / logarithm (ADR-0103) — can an opening be forged or be missing without penalty?; free-prompt lane pricing (ADR-0148) — is declared work ever an input?; the gateway as a public entrance parsing attacker text (resource exhaustion, env / secret leakage to the worker, SECURITY.md §3 claims).

## PALW-U8 — BASE-0 deterministic arithmetic, step and leg execution

**Files.** misaka-palw-base0/src/, consensus/core/src/{palw_base0.rs,palw_base0_a16.rs,palw_base0_ops.rs,palw_base0_profile.rs,palw_step.rs,palw_step_leg.rs,palw_legs.rs,palw_transcendental.rs,palw_state_chunk_map.rs,palw_backend.rs,palw_reference.rs,palw_qwen25_profile.rs,palw_qwen36_profile.rs,palw_qwen36_ops.rs,palw_kimi_k3_*.rs}

**Focus.** Cross-platform determinism of the canonical integer arithmetic (x86_64 vs aarch64, debug vs release overflow behaviour, wrapping vs checked, shifts by >= width, signed rounding / division towards zero vs floor, any f32/f64 or libm reaching a consensus value, SIMD / feature-detected code paths, rayon / thread-count dependent reductions). A disagreement here is a consensus split in the court. Also: can attacker-supplied step inputs make the court panic or allocate unboundedly (a remotely triggered crash of every node that adjudicates)?

## NET-U1 — P2P protocol, flows, IBD and PALW gossip relay

**Files.** protocol/p2p/, protocol/flows/src/ (v7/, v8/, ibd/, flowcontext/, palw_gossip.rs, palw_heartbeat_relay.rs, palw_round_relay.rs, flow_context.rs), components/addressmanager/, components/connectionmanager/, components/consensusmanager/

**Focus.** Remote crash / resource exhaustion from peer messages (MISAKA-added message types first: PALW gossip, heartbeat relay, round relay); message size and count limits; caches keyed by attacker data; handshake and fork-id / consensus_params_id checks; IBD manipulation (a peer choosing what we sync); ban-score logic that can be turned against honest peers; relay of objects that the fold later refuses (amplification).

## NET-U2 — Mempool, block templates and PALW carriers

**Files.** mining/src/ (mempool/, block_template/, evm_mempool.rs, manager.rs), how PALW carriers and H-1 carriers enter the mempool and templates (grep carrier in mining/ and kaspad/), consensus/core/src/palw_carriage.rs as it relates to admission

**Focus.** Can an attacker make an honest producer build an invalid block (a template including an object the fold refuses and that invalidates the block rather than dropping it)? Mempool DoS (orphans, high-mass, carriers that are cheap to submit and expensive to validate), eviction policy, EVM mempool nonce / replacement rules, the per-block removal of fold-refused carriers.

## NET-U3 — RPC surfaces (wRPC, gRPC, eth JSON-RPC)

**Files.** rpc/core/, rpc/service/, rpc/grpc/server/, rpc/wrpc/server/, rpc/eth/, kaspad/src/eth_rpc.rs, kaspad/src/args.rs (defaults: bind addresses, unsafe RPC, CORS), evm-indexer/service/

**Focus.** Which RPC methods change node state or submit to consensus, and whether each is gated when the RPC is public (unsafe-RPC flags, defaults); resource exhaustion (unbounded ranges, large responses, subscriptions); panics on malformed input; eth_* methods that could be abused (eth_call gas limits, debug/trace methods, filters); whether an RPC-submitted object skips checks that the P2P path applies; default bind addresses and whether a fresh node exposes more than documented.

## NET-U4 — Validator, remote signer, node duties and secrets

**Files.** kaspa-pq-signer/, kaspa-pq-validator/, kaspa-pq-validator-core/, kaspad/src/{validator_service.rs,palw_duties.rs,palw_heartbeat_miner.rs,palw_round_producer.rs,palw_memory_ledger.rs,palw_retention.rs,palw_backends.rs}, misaka-cli/ (key, bond, wallet commands), wallet/pq-cli/, misaka-endpoints/, SECURITY.md

**Focus.** Verify every accepted-by-design claim in SECURITY.md against the code (socket dir perms, umask-before-bind, SO_PEERCRED / getpeereid, handshake timeout, 255-byte context refusal, poison-tolerant lock, env_clear + allowlist, no PATH). Validator / signer privilege escalation; key material on disk (permissions, zeroization, logging, command-line args visible in ps); node-duty logic that could make an honest node double-sign or commit a slashable offence (the launch note says running one bond in two processes gets slashed — is there any guard? could a restart / crash-recovery do the same?).

## REL-U1 — CI, release pipeline, reproducibility and supply chain

**Files.** .github/workflows/ (ci.yaml, deploy.yaml, musl-toolchain.yaml), scripts/{ci-gates.sh,ci-gates-selftest.sh,pq-ci-guard.sh,ci-toolchain-pin-check.py,misaka-release-info.py,misaka-release-smoke.py,misaka-ci-summary.py,verify-artifacts-independently.py}, release.json, docs/release-process.md, deny.toml, Cargo.lock (git / path / patched deps), third_party_manifest.toml, every build.rs in the workspace, docker/, musl-toolchain/, contrib/t12-deploy-kit/ (build-release-local.sh)

**Focus.** Pinning (actions by SHA, containers by digest, toolchain, git deps by rev); workflows that expose secrets or write tokens to untrusted code; build scripts that download at build time; whether signing / SBOM / provenance verify what they claim; whether the release identity smoke can pass for a binary built from a different commit or with different consensus params; deny.toml advisories ignored and why; anything making two builds of one commit differ.

## THREAT-U1 — Architecture and threat-model review

**Files.** docs/palw-rc-threat-model.md, docs/architecture/overview.md, SECURITY.md, docs/palw-registry-map.md, docs/mainnet-readiness.md, the governing ADRs cited in overview.md §2, and the code locations they name

**Focus.** Trust boundaries and where each is enforced; documented-but-unenforced security claims; attacker classes and assets not covered by any threat model (P2P, RPC, operator hosts, release pipeline, EVM users, model owners, panel collusion, validator cartel); design-level risks that are not bugs.

## PRIOR-U1 — Prior-audit closure: September 2026 audits

**Files.** docs/palw-audit-2026-09-18-6001.md, docs/palw-daa-clock-audit-2026-09-18.md, docs/palw-release-6001-verdict-2026-09-18.md, docs/adr/STATUS-AUDIT-2026-09-18.md, docs/adr/STATUS-AUDIT-2026-09-19-llm-mining-reward.md, docs/adr/STATUS-AUDIT-2026-09-20-reward-reaudit.md, docs/palw-mainnet-audit-2026-09-05.md, docs/palw-mainnet-audit-2026-09-06.md

**Focus.** Every Critical / High in these documents.

## PRIOR-U2 — Prior-audit closure: June-August 2026 audits and external reviews

**Files.** docs/security/MISAKA-Audit-Remediation-Response-2026-06-23.md (+ the CSV matrix beside it), docs/palw-external-audit-2026-08-21.md, docs/palw-critical-audit-2026-08-19-ja.md, docs/palw-only-v4-audit-2026-08-17-ja.md, docs/palw-algo4-forgery-audit-2026-08-16.md, docs/palw-mainnet-readiness-audit-2026-08-22.md, docs/palw-mainnet-audit-2026-08-28.md, docs/palw-mainnet-audit3-2026-08-29.md, docs/palw-mainnet-reaudit-2026-08-29.md, docs/palw-mainnet-audit-2026-08-30.md

**Focus.** Every Critical / High in these documents. Many target code that has since been superseded (e.g. testnet-11 era fences); for those, determine whether the vulnerable code path still exists at this commit and whether it is reachable on testnet-12 as shipped.
