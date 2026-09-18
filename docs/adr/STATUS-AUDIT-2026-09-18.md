# ADR status audit — 2026-09-18 (mechanical pass; see the reviewed notes at the end)

Classification per Decision / Phase / Follow-up section and per keyword line (future, follow-up, deferred, not built/shipped, TODO, candidate, Phase 2, V2). Evidence: backticked identifiers checked with `git grep -w` against the code — `name:code_hits/test_hits`. Fences: `ACTIVE(fence=height)` when the testnet-11 preset arms it, `DORMANT(fence)` when it exists but no preset arms it. Rows with no identifier are `UNCLASSIFIED` and were reviewed by hand where the ADR is recent (0124–0137).

| ADR | section | class | fence | evidence (identifier:code/tests) | kw lines |
|---|---|---|---|---|---|
| 0001 | Decision | DEFERRED |  |  | 3 |
| 0001 | Implementation notes for Phase 2 | IMPLEMENTED |  | `Params`:933/38, `genesis_block`:27/0, `UtxoCommitment64`:3/0, `NetworkType`:388/12, `NetworkId`:424/10, `Prefix`:329/12 | 0 |
| 0001 | Acceptance criteria (Phase 2) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0002 | ADR-0002: ML-DSA-65 P2PKH as the only standard script | SUPERSEDED |  | `MlDsa65`:0/0 | 2 |
| 0002 | Negative | DEFERRED |  |  | 2 |
| 0002 | Implementation notes for Phase 4 | IMPLEMENTED |  | `Version::PubKeyHashMlDsa65`:0/0, `OP_CHECKSIG_MLDSA65`:1/0, `pay_to_pub_key_hash_mldsa65`:0/0, `ScriptClass`:103/6, `MAX_SCRIPT_ELEMENT_SIZE`:16/14, `calc_mldsa65_signature_hash`:0/0, `Mldsa65SigCacheKey`:1/0 | 0 |
| 0002 | Acceptance criteria (Phase 4) | IMPLEMENTED |  | `verify`:645/55, `MAX_SCRIPT_ELEMENT_SIZE`:16/14, `Mldsa65SigCacheKey`:1/0 | 0 |
| 0003 | ADR-0003: LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024) | DEFERRED |  |  | 1 |
| 0003 | Alternatives considered | DEFERRED |  |  | 1 |
| 0003 | Implementation notes for Phase 3 | IMPLEMENTED |  | `MuHash`:145/6, `LtHashUtxoAccumulator`:0/0, `add_element`:20/0, `remove_element`:11/0 | 0 |
| 0003 | Acceptance criteria (Phase 3) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0004 | Phase 7 PR-7.6 delivery | IMPLEMENTED |  | `UtxoCommitment64`:3/0, `From`:498/4, `EMPTY_MUHASH_64`:0/0, `RpcUtxoCommitment64`:0/0, `test_empty_hash_64`:0/0, `Header::utxo_commitment`:161/5, `Hash`:499/10 | 1 |
| 0004 | Positive | DEFERRED |  |  | 1 |
| 0004 | Implementation notes for Phase 3/4 | IMPLEMENTED-BUT-UNVERIFIED |  | `UtxoCommitment64`:3/0 | 0 |
| 0005 | PR-19-S7 (Phase 7) recalibration — ML-DSA-87 (supersedes the ML-DSA-65 | IMPLEMENTED |  | `verify`:645/55, `ml_dsa_87::verify`:645/55, `secp256k1::schnorr::Signature::verify`:645/55, `libcrux_ml_dsa::ml_dsa_87::verify`:645/55, `libcrux_ml_dsa::ml_dsa_87::portable::verify`:645/55 | 0 |
| 0005 | (historical) Phase 6 calibration result — ML-DSA-65 PoC, superseded by | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0005 | Phase 6 calibration result | IMPLEMENTED |  | `secp256k1::schnorr::Signature::verify`:645/55, `libcrux_ml_dsa::ml_dsa_65::verify`:645/55, `libcrux_ml_dsa::ml_dsa_65::portable::verify`:645/55 | 0 |
| 0005 | Frozen shape | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0005 | Implementation notes for Phase 6 | IMPLEMENTED |  | `verify`:645/55, `mass_per_sig_op`:39/0, `max_block_mass`:57/0 | 0 |
| 0005 | Acceptance criteria (Phase 6) | IMPLEMENTED-BUT-UNVERIFIED |  | `mass_per_sig_op`:39/0 | 0 |
| 0006 | 1. Scope | DEFERRED |  |  | 2 |
| 0006 | 2. gRPC proto changes | DEFERRED |  |  | 1 |
| 0006 | 4. WASM bindings | DESIGN-ONLY |  |  | 1 |
| 0006 | Positive | DEFERRED |  |  | 1 |
| 0006 | Implementation order (Phase 7 PR sequence) | IMPLEMENTED |  | `RpcMlDsa65PublicKey`:0/0, `RpcMlDsa65Signature`:0/0, `RpcUtxoCommitment`:0/0, `KaspaPqRpcService`:5/0, `AddressVersion`:16/0, `reserved`:425/13, `RpcUtxoCommitment64`:0/0 | 0 |
| 0007 | ADR-0007: Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 A | SUPERSEDED |  |  | 1 |
| 0007 | Context | DEFERRED |  |  | 2 |
| 0007 | Layer 1 — ASIC-resistance tag (`algo_id`-driven) | IMPLEMENTED |  | `algo_id`:246/12 | 1 |
| 0007 | `BlueWorkType` width choice | DEFERRED |  |  | 1 |
| 0007 | Public claim discipline | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0007 | Positive | IMPLEMENTED |  | `algo_id`:246/12 | 1 |
| 0007 | Neutral | DEFERRED |  |  | 1 |
| 0007 | Implementation order (Phase 8 PR sequence) | IMPLEMENTED |  | `Uint512`:26/0, `Uint576`:18/3, `Uint640`:0/0, `compact_target_bits_512`:0/0, `compact_target_bits`:21/0, `POW_FINALIZER_DOMAIN`:4/0, `POW_FINALIZER_BYTES`:11/0, `POW_ALGO_ID_KHEAVYHASH`:69/3, `pow_finalizer_blake2b_512` | 3 |
| 0007 | Phase 2 (deferred): `pow_algo_id` wire support (audit H-04) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0007 | Phase 2 — Argon2id (`algo_id = 2`): LANDED, then SUPERSEDED | IMPLEMENTED |  | `pow_layer0::argon2id_l1_tag_v1`:12/0, `argon2id_l1_tag_v1`:12/0, `StateLayer0`:55/17, `check_algo_id_known`:19/0 | 2 |
| 0007 | Phase 3 — compute-only BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`): ACTIVE | IMPLEMENTED-BUT-UNVERIFIED |  | `pow_layer0::blake2b_sha3_l1_tag_v1`:11/0 | 0 |
| 0008 | algo_id = 1 (kHeavyHash) seed derivation | DEFERRED |  |  | 1 |
| 0008 | Address payload width | IMPLEMENTED |  | `Version::PubKey`:73/2 | 1 |
| 0008 | Implementation order (revised 9-phase plan) | IMPLEMENTED |  | `Hash64`:6342/362, `pow_finalizer_blake2b_512`:32/0, `pre_pow_hash`:142/3, `l1_seed32_for_kheavyhash_v1`:11/0, `Header`:374/12, `Transaction`:823/58, `TransactionOutpoint`:1111/132, `Hash`:499/10 | 2 |
| 0008 | Relationship to the previously-deferred PR-8.4 / PR-8.5 / PR-8.6 | IMPLEMENTED |  | `pre_pow_hash`:142/3, `BlockPrePowHash64`:8/0 | 0 |
| 0009 | ADR-0009: DNS Probabilistic Finality Overlay | DEFERRED |  |  | 2 |
| 0009 | Decision | DESIGN-ONLY |  |  | 1 |
| 0009 | What the overlay adds | DESIGN-ONLY |  |  | 5 |
| 0009 | Phase-specific behaviour | DESIGN-ONLY |  |  | 1 |
| 0009 | Long-range bound | SUPERSEDED |  |  | 1 |
| 0009 | Validator selection (sortition) | DEFERRED |  |  | 1 |
| 0009 | Positive | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0009 | Phase 10 implementation order | IMPLEMENTED |  | `DnsParams`:118/1, `subnetwork_id`:224/8, `TxKind`:5/0, `StakeScore`:202/20, `DnsConfirmation`:13/0 | 6 |
| 0009 | Addendum A — Phase 10 implementation conventions (binding) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0009 | A.6 Revised implementation order (supersedes the PR table above for Ph | IMPLEMENTED |  | `verify_mldsa65_with_context`:0/0, `stake_attestation_message`:36/4, `network_id`:820/10, `bond_outpoint`:659/74, `DnsState`:56/11, `check_dns_reorg_rule`:37/0, `sink_search_algorithm`:5/2, `RuleError::DnsFinalityReorgRe | 0 |
| 0009 | Addendum B — Per-block active-bond view + reward-eligibility (binding) | DEFERRED |  |  | 1 |
| 0010 | ADR-0010: Validator Node Architecture (operational supplement to ADR-0 | DEFERRED |  |  | 1 |
| 0010 | Node-role separation (binary stays single) | DEFERRED |  |  | 1 |
| 0010 | Subsystem file layout | DEFERRED |  |  | 1 |
| 0010 | Header vs body validation split | IMPLEMENTED |  | `StakeAttestationShardPayload`:37/2, `StakeScore`:202/20, `DnsConfirmation`:13/0 | 1 |
| 0010 | synced / bond_status / current_epoch / eligible_this_epoch | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0010 | Phase 10 PR plan (refined per this ADR) | IMPLEMENTED |  | `stake_registry`:1/0, `stake_score`:1/0, `validator_service`:13/0, `StakeAttestation`:40/6, `DnsConfirmation`:13/0 | 13 |
| 0010 | Negative | DEFERRED |  |  | 1 |
| 0011 | ADR-0011: Validator Single-Host Deployment + Equivocation-Safety Opera | DEFERRED |  |  | 1 |
| 0011 | Context | DEFERRED |  |  | 1 |
| 0011 | Two supported deployment shapes | DEFERRED |  |  | 2 |
| 0011 | Validator status enum (`ValidatorStatus`) | IMPLEMENTED |  | `getValidatorStatus`:8/0, `NodeNotSynced`:8/0, `BondNotFound`:5/0, `BondPending`:5/0, `ActiveIdle`:9/0, `ActiveEligible`:5/0, `signed_epoch_db`:12/0, `SignedThisEpoch`:5/0, `Unbonding`:66/0, `Slashed`:52/0, `SlashingEvid | 3 |
| 0011 | Signed-epoch persistence (`SignedEpochRecord`) | DESIGN-ONLY |  |  | 2 |
| 0011 | Slashing scope (binding) | DEFERRED |  |  | 2 |
| 0011 | Auto-startup ordering | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0011 | Hardware sizing (informative) | IMPLEMENTED |  | `algo_id`:246/12 | 1 |
| 0011 | /etc/systemd/system/kaspa-pq-validator.service | DEFERRED |  |  | 1 |
| 0011 | Positive | DEFERRED |  |  | 2 |
| 0011 | Negative | DEFERRED |  |  | 1 |
| 0011 | Phase 12 PR plan | IMPLEMENTED-BUT-UNVERIFIED |  | `signed_epoch`:2/0, `check_signed_epoch_record`:11/0, `getValidatorStatus`:8/0 | 5 |
| 0012 | ADR-0012: Mainnet Validator Sortition via On-Chain Commit-Reveal | SUPERSEDED |  |  | 1 |
| 0012 | Context | DEFERRED |  |  | 3 |
| 0012 | Public-claim discipline (binding) | DEFERRED |  |  | 1 |
| 0012 | Neutral | DEFERRED |  |  | 1 |
| 0012 | Phase 13 PR plan (this ADR's slot) | DEFERRED |  | `SortitionMode::CommitReveal`:0/0 | 2 |
| 0012 | References | DEFERRED |  |  | 1 |
| 0013 | ADR-0013: Validator Reward Distribution | DEFERRED |  |  | 1 |
| 0013 | Public-claim discipline (binding) | DEFERRED |  |  | 2 |
| 0013 | Negative | DEFERRED |  |  | 1 |
| 0013 | Phase 13 PR plan (this ADR's slot) | IMPLEMENTED |  | `RewardParams`:11/0, `compute_attestation_reward_payouts`:11/0, `compute_slashing_distribution`:17/9, `RewardParams::per_attestation_reward_sompi`:21/0 | 2 |
| 0013 | Security analysis | DEFERRED |  |  | 1 |
| 0013 | Wire-format compatibility | DEFERRED |  |  | 2 |
| 0013 | Implementation slots (supersedes the PR-10.5′ row above) | IMPLEMENTED |  | `owner_reward_spk_payload`:49/6, `StakeBondPayload`:19/6, `StakeBondRecord`:141/2, `stake_bond_record_from_payload`:15/0, `p2pkh_mldsa65_spk`:0/0, `pay_to_address_script`:74/2, `RewardParams`:11/0, `DnsParams`:118/1 | 2 |
| 0013 | C.1.2 Per-transaction exemption (shared validator) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0013 | Implementation slots (PR-10.12) | IMPLEMENTED |  | `reporter_reward_spk_payload`:23/7 | 1 |
| 0013 | The gap C.1 left open (why this supersedes "reporter on the tx") | DEFERRED |  |  | 1 |
| 0013 | Decision: the reporter reward is a side-effect, not a transaction outp | DEFERRED |  |  | 1 |
| 0014 | ADR-0014: Coordinated-Failover Protocol for Validator Hosts | DEFERRED |  |  | 2 |
| 0014 | Context | IMPLEMENTED-BUT-UNVERIFIED |  | `sign_ctx`:5/0 | 6 |
| 0014 | `host_id` derivation | DEFERRED |  |  | 1 |
| 0014 | `ValidatorStatus` extension | IMPLEMENTED-BUT-UNVERIFIED |  | `AwaitingTakeoverToken`:3/0 | 2 |
| 0014 | Negative | DEFERRED |  |  | 1 |
| 0014 | Phase 13 PR plan (this ADR's slot) | IMPLEMENTED-BUT-UNVERIFIED |  | `HostId`:17/0, `TakeoverToken`:17/0, `takeover_token_message`:15/0, `verify_takeover_token`:0/0, `ValidatorStatus::AwaitingTakeoverToken`:3/0 | 2 |
| 0014 | References | DEFERRED |  |  | 1 |
| 0015 | ADR-0015: Remote-Signer / HSM Protocol for Validator Signing | IMPLEMENTED-BUT-UNVERIFIED |  | `SignerMessageDigest`:16/0, `Unbond`:34/0, `TakeoverToken`:17/0, `purpose_matches_digest`:6/0, `Permissive`:6/0, `AuditOnly`:3/0, `Strict`:9/0, `SignedEpochStore`:4/0, `SignerClient`:0/0, `SignerError`:16/0 | 3 |
| 0015 | Context | DEFERRED |  |  | 2 |
| 0015 | Topology and transport | DEFERRED |  |  | 3 |
| 0015 | Negative | DEFERRED |  |  | 1 |
| 0015 | Phase 13 PR plan (this ADR's slot) | IMPLEMENTED-BUT-UNVERIFIED |  | `SignerProtocolVersion`:0/0, `SignerCapabilities`:0/0, `SignerRequest`:13/0, `SignerResponse`:5/0, `SigningPurpose`:27/0, `SignerMetadata`:10/0, `SignerError`:16/0, `SignerPolicy`:9/0, `SignerAuditRecord`:8/0, `compute_s | 4 |
| 0015 | References | DEFERRED |  |  | 2 |
| 0016 | ADR-0016: Stake-locked bond UTXOs | DEFERRED |  |  | 1 |
| 0016 | Context | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0016 | Negative | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0017 | ADR-0017: All-Active-Staker Attestation (Remove Sortition Committee) | DEFERRED |  |  | 1 |
| 0017 | Context | DEFERRED |  |  | 2 |
| 0017 | Negative / open | DEFERRED |  |  | 2 |
| 0018 | ADR-0018: Quality-Gated StakeScore + Inclusion Economics (BFT-free) | DEFERRED |  |  | 1 |
| 0018 | §A — Explicitly NOT adopted (BFT-free invariant, binding) | DEFERRED |  |  | 2 |
| 0018 | §C — DNS health / degraded mode (non-blocking signal) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0018 | §F — Fee split (Worker / Validator / Service) | DEFERRED |  |  | 1 |
| 0018 | §G — Attestation lane (liveness-first by default) | IMPLEMENTED-BUT-UNVERIFIED |  | `stake_event_quality_floor_bps`:14/0 | 3 |
| 0018 | §H — Two-dimensional reorg dominance (mainnet path) | DESIGN-ONLY |  |  | 1 |
| 0018 | Negative / open | DEFERRED |  |  | 1 |
| 0019 | Status | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0019 | Revision 1.2 — md2 (PQ-only design v2.0) alignment (2026-06-01) | SUPERSEDED |  | `MlDsa65`:0/0, `MlDsa87`:8/0, `mldsa87`:107/1, `PubKeyHashMlDsa87`:54/0, `signTransactionMlDsa87`:6/0, `mldsa65`:13/0, `OP_BLAKE2B_512`:14/1, `kaspa_hashes::blake2b_512_address_payload`:21/31, `utxo_commitment`:161/5, `h | 4 |
| 0019 | Revised from 1.0 | SUPERSEDED |  | `MlDsa65`:0/0 | 1 |
| 0019 | Net-new consensus/wallet work (8 phases; design doc §17, §22 order) | IMPLEMENTED |  | `main`:0/0, `secp256k1`:768/6, `secp256k1::Message`:52/10, `lints`:50/0, `sign_with_multiple_v3`:5/0, `kaspad`:0/0, `MlDsa65`:0/0 | 2 |
| 0019 | Normative launch-scope values (design doc §16.2) | IMPLEMENTED |  | `calc_mldsa87_signature_hash`:19/13 | 3 |
| 0019 | 5. Naming policy (minimal-churn for this pass) | DEFERRED |  | `MlDsa65`:0/0, `signTransactionMlDsa65`:0/0 | 1 |
| 0019 | Negative / operational | DEFERRED |  | `MlDsa65`:0/0 | 1 |
| 0019 | Alternatives Considered | DEFERRED |  |  | 1 |
| 0020 | Status | SUPERSEDED |  | `EVM_HEADER_VERSION`:34/12, `u64::MAX`:0/0, `EvmLaneRequiresEvmBuild`:3/0, `evm_payload_hash`:68/15, `evm_commitment_root`:70/17 | 2 |
| 0020 | Frozen parameters (P0) | SUPERSEDED |  | `SpecId::SHANGHAI`:5/0, `pruning_point`:418/6, `EvmPayload64`:3/0, `EvmExecutionPayload`:100/14, `evm_payload_hash`:68/15, `EvmCommitment64`:3/0, `EvmExecutionHeader`:59/0, `evm_commitment_root`:70/17, `MISAKA_EVM_COMMIT | 4 |
| 0020 | P1 surface (implemented) | DEFERRED |  |  | 1 |
| 0021 | ADR-0021: PALW LLM proof-of-work (`algo_id = 4`/`5`), at one block per | SUPERSEDED |  |  | 3 |
| 0021 | Context | DEFERRED |  |  | 1 |
| 0021 | Decision | SUPERSEDED |  |  | 1 |
| 0021 | Consequences | SUPERSEDED |  |  | 4 |
| 0022 | ADR-0022 — Pruned-IBD support for the EVM lane and the DNS/PoS-v2 over | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0022 | 1. Problem | IMPLEMENTED |  | `stake_bonds_store`:25/5 | 2 |
| 0022 | 3.1 Definition | IMPLEMENTED-BUT-UNVERIFIED |  | `epoch_accumulator_store`:8/0, `epoch_tallies`:0/0 | 2 |
| 0022 | 3.3 Where it is computed and verified | DEFERRED |  |  | 1 |
| 0022 | 4. Header change | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0022 | 9. Future work | IMPLEMENTED |  | `overlay_commitment_root`:60/2 | 0 |
| 0023 | Status | IMPLEMENTED-BUT-UNVERIFIED |  | `EvmFlatAccount`:4/0, `EvmBlockStateRoot`:2/0, `EvmLatestStatePtr`:17/0, `flat_or_reconstruct_parent_snapshot`:8/0, `false`:0/0 | 0 |
| 0023 | The current single-lane reality, grounded in code | IMPLEMENTED-BUT-UNVERIFIED |  | `MISAKA_EVM_COMMITMENT_V2`:1/0 | 1 |
| 0023 | The only consensus changes | IMPLEMENTED |  | `execution_payloads_root`:0/0, `execution_results_root`:0/0, `Hash64`:6342/362, `evm_payload_hash`:68/15, `evm_commitment_root`:70/17 | 1 |
| 0023 | Frozen design decisions | DEFERRED |  |  | 4 |
| 0023 | Relationship to existing ADRs | SUPERSEDED |  |  | 1 |
| 0023 | Release gates (§21.2 — P0..P3, distinct from the phase rollout) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0023 | Phased activation (§18 — Phase 0 is the gate) | DESIGN-ONLY |  | `execution_payloads_root`:0/0, `execution_results_root`:0/0, `PqEvmTransactionV1`:0/0 | 1 |
| 0023 | Open decisions (§22, O-01..O-15 — all carried; plus the F003 contentio | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0023 | Consequences | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0024 | ADR-0024: Verified LLM Token-Weighted BFT for DNS finality | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0024 | Activation | DESIGN-ONLY |  | `u64::MAX`:0/0 | 2 |
| 0024 | Activation runbook | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0024 | Two rounds: prevote, then lock and precommit | DEFERRED |  |  | 2 |
| 0024 | The denominator is pinned, not per-branch | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0025 | What this changes | IMPLEMENTED-BUT-UNVERIFIED |  | `CandidateAdoptionPermit`:8/0 | 1 |
| 0026 | ADR-0026: PALW v2 verification architecture — borrow Ambient's shape,  | SUPERSEDED |  |  | 5 |
| 0026 | Where the v2 scheme stands | IMPLEMENTED-BUT-UNVERIFIED |  | `PALW_EXECUTION_ALGO_ID_V2`:3/0, `PalwCapabilityDeclarationV2`:1/0 | 4 |
| 0026 | 1. Borrow Ambient's architecture; keep the scheme kernel out of the ru | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0026 | 2. Prove deeper than logits — logits + activations + GEMM trace | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0026 | 3. Exactness inside a pinned class — never a tolerance in the slashing | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0026 | 4. Commit → post-commit challenge → recompute → quorum (the flow, made | DEFERRED |  |  | 2 |
| 0026 | 6. Verification is asynchronous; PALW never gates block validity, and  | DEFERRED |  |  | 2 |
| 0026 | 7. Open kernel; self-originated jobs (no auction); tokenizer outside t | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0026 | Consequences | DEFERRED |  |  | 8 |
| 0027 | ADR-0027: PALW-S — unilateral fraud proofs; no BFT, no challenge rando | IMPLEMENTED |  | `reserved_exposure`:134/4 | 4 |
| 0027 | What the premises force | DEFERRED |  |  | 1 |
| 0027 | 3. What replaces `P_detect`: one funded honest re-execution, and f-ind | DEFERRED |  |  | 2 |
| 0027 | 4. Amendments to the v0.1 specification | DEFERRED |  |  | 2 |
| 0027 | Consequences | IMPLEMENTED |  | `FailedChallenge`:3/0, `ForgedReceipt`:9/0, `shape_profile_id`:443/57 | 3 |
| 0028 | ADR-0028: PALW challenge sampling — a scheduler for re-execution, neve | SUPERSEDED |  |  | 3 |
| 0028 | 1. Every credited job is fully re-executed; attestation allocates cred | DEFERRED |  |  | 4 |
| 0028 | 2. Assignment: the `select_verifiers` ticket, adopted as the duty lott | DEFERRED |  |  | 3 |
| 0028 | 3. Windows: DAA-denominated, stall-tolerant, pruning-constrained — and | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0028 | 4. `q`, funding, and the inequality that must hold before any reward e | SUPERSEDED |  | `base_subsidy_permille`:25/0 | 5 |
| 0028 | 5. The audit layer: opening calls as the DA heartbeat — answerable, no | IMPLEMENTED-BUT-UNVERIFIED |  | `check_legs_opening_answer_v1`:8/0 | 1 |
| 0028 | 6. Stage mapping — what each stage newly requires from this ADR | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0028 | What this ADR deliberately does not decide | SUPERSEDED |  |  | 5 |
| 0028 | Consequences | DEFERRED |  |  | 6 |
| 0029 | Facts this design stands on (verified in code, 2026-08-16) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0029 | 1. Two stages, one body format | DEFERRED |  |  | 1 |
| 0029 | 2. The five kinds and their bodies | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0029 | 3. Mass budget — every kind sized against the real constants | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0029 | 5. Stage 0 realization — the drill that fills the §12 artifacts | DEFERRED |  |  | 2 |
| 0029 | 6. The object that does not fit, and what that forces | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0029 | Assumptions that remain (stated so they can be attacked) | DEFERRED |  |  | 1 |
| 0030 | ADR-0030: The PALW step function, pinned at tile granularity — shape p | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0030 | Facts this design stands on (verified in the pinned tree, 2026-08-16) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0030 | Facts, second pass (kernel internals, read 2026-08-16 after the first  | IMPLEMENTED-BUT-UNVERIFIED |  | `SET_ROWS`:1/0 | 3 |
| 0030 | 2. Shape profile v3 — what `shape_profile_id` binds | DESIGN-ONLY |  | `reference_arithmetic_ruleset_id`:0/0 | 3 |
| 0030 | 3. The step leg — execution-commitment v2 | IMPLEMENTED |  | `state_root`:324/22 | 10 |
| 0030 | 4. Adjudication — `ExecutionStepRefutationV1` becomes implementable | IMPLEMENTED |  | `NoFaultFound`:135/4 | 1 |
| 0030 | 5. Validation gates — before any class registers a v3 profile | DESIGN-ONLY |  |  | 2 |
| 0030 | Consequences | DEFERRED |  |  | 3 |
| 0031 | ADR-0031: Canonical transcendentals — exp and log are algorithms, not  | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0031 | Facts (read from the pinned tree and the fleet's libc lineage, 2026-08 | IMPLEMENTED-BUT-UNVERIFIED |  | `rms_norm`:38/0, `l2_norm`:6/0 | 3 |
| 0031 | Decision | DESIGN-ONLY |  |  | 2 |
| 0032 | ADR-0032: PALW fee-bond escrow — pricing calls and paying challengers  | DEFERRED |  |  | 1 |
| 0032 | Phase E1 (Stage 1) — fees are fees, bounties are consensus credits | IMPLEMENTED-BUT-UNVERIFIED |  | `W_answer`:4/0, `DATA_WITHHOLDING`:1/0, `slash_id`:1/0 | 0 |
| 0032 | Phase E2 (Stage 2+) — the audit-call bond, as a bond | IMPLEMENTED-BUT-UNVERIFIED |  | `AUDIT_CALL`:0/0, `F_audit`:1/0 | 0 |
| 0033 | ADR-0033: The credit gate, wired — how `credit(C)` becomes a consensus | DEFERRED |  |  | 1 |
| 0033 | 3. The predicate, verbatim from ADR-0028 §1 | DEFERRED |  |  | 1 |
| 0033 | Consequences | DEFERRED |  |  | 1 |
| 0034 | ADR-0034: PALW re-verification routing — four execution-class families | SUPERSEDED |  | `derive_runtime_class_id`:16/0 | 2 |
| 0034 | 1. The three keys, and what each is allowed to touch | DESIGN-ONLY |  |  | 2 |
| 0034 | 2. Draft vocabulary → this fork's identifiers | IMPLEMENTED |  | `model_id`:337/28, `model_root`:0/0, `model_profile_id`:101/5, `ModelDefinitionV1`:12/0, `VerifierCapabilityV1`:0/0, `PalwVerifierCapabilityV1`:6/0, `PalwComputeCapability`:16/0 | 3 |
| 0034 | 3. The registry: definitions, bindings, and the row that already exist | DESIGN-ONLY |  |  | 1 |
| 0034 | 4. Model band: derived from the binding, and capped by the pruning hor | DEFERRED |  |  | 2 |
| 0034 | 5. Receipts carry the keys; the registry, not the miner, gives them me | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0034 | 6. Verifier capability: two layers, and an agent that registers itself | IMPLEMENTED-BUT-UNVERIFIED |  | `ready_binding_root`:7/0 | 3 |
| 0034 | 7. What routing may decide — and the draft state that is rejected | DESIGN-ONLY |  |  | 1 |
| 0034 | 9. Adjudication is band-independent, bounded, and per-binding in depth | SUPERSEDED |  | `LOAD_FRAGMENT`:0/0, `MUL_INT`:0/0 | 2 |
| 0034 | 10. Coverage: a binding nobody can replay does not get to exist quietl | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0034 | Required tests (the draft's §25, in this vocabulary — plus the routing | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0034 | Consequences | SUPERSEDED |  |  | 1 |
| 0035 | ADR-0035: The public PALW testnet is testnet-11, continued — and it pi | SUPERSEDED |  |  | 2 |
| 0035 | 1. Context | SUPERSEDED |  |  | 1 |
| 0035 | 2. Decision 1 — the public PALW testnet is testnet-11, the *current ch | SUPERSEDED |  |  | 1 |
| 0035 | 3. Decision 2 — class admission is pinned in code, not in a runbook | IMPLEMENTED |  | `POW_L1_PALW_OLLAMA_CALIBRATION_V1`:4/0, `POW_L1_PALW_PROBE_SEED_V1`:3/0, `POW_L1_PALW_WORKER_CALIBRATION_TN11_V1`:3/0, `palw_l1_tag`:4/6, `Params`:933/38, `dns_seeders`:31/0 | 1 |
| 0035 | 4. Decision 3 — the participation model is honest about who can join | DEFERRED |  |  | 1 |
| 0035 | 6. Operator items (deliberately NOT decided in code) | DESIGN-ONLY |  |  | 1 |
| 0035 | Status of the gate-6 checklist, measured 2026-08-18 | IMPLEMENTED |  | `quarantined`:58/2 | 2 |
| 0035 | 7. Consequences | DEFERRED |  |  | 1 |
| 0036 | ADR-0036: PALW mainnet activation — lineage reconciliation and the mod | SUPERSEDED |  |  | 3 |
| 0036 | Relationship to ADR-0037 and ADR-0038 (added 2026-08-17) | DEFERRED |  |  | 1 |
| 0036 | Decision | SUPERSEDED |  |  | 9 |
| 0036 | What this ADR does not decide | DEFERRED |  |  | 1 |
| 0036 | Consequences | DEFERRED |  |  | 5 |
| 0037 | ADR-0037: PALW off the block-critical path — an asynchronous, budgeted | SUPERSEDED |  |  | 3 |
| 0037 | Decision 1 — Three layers; PALW is never block-critical on a value-bea | IMPLEMENTED-BUT-UNVERIFIED |  | `PALW_runtime_available`:0/0, `PALW_full_inference_matches`:0/0, `palw_credit`:51/0 | 1 |
| 0037 | Decision 2 — Jobs are state, not history | IMPLEMENTED-BUT-UNVERIFIED |  | `compute_palw_credit_outputs`:2/0, `Unadjudicable`:310/0 | 1 |
| 0037 | Decision 3 — Identity and signatures are fully bound, verified at ever | IMPLEMENTED |  | `job_id`:261/3, `committed_root`:114/0 | 3 |
| 0037 | Decision 4 — Panel selection: future anchor, real snapshot, dual deadl | IMPLEMENTED |  | `select_replay_panel_v1`:31/0, `Active`:560/43, `execution_class_id`:122/1, `bond_outpoint`:659/74 | 1 |
| 0037 | Decision 5 — Sampled verification is the fast path, never the final ru | IMPLEMENTED |  | `ProvisionalAccepted`:0/0, `job_id`:261/3, `Unadjudicable`:310/0 | 1 |
| 0037 | Decision 6 — Two-tier hardware taxonomy; classes qualify by calibratio | DESIGN-ONLY |  | `ModelBandId`:0/0, `ExecutionClassId`:0/0, `CompatibilityGroupId`:0/0 | 0 |
| 0037 | Decision 7 — Mint is a carve of scheduled subsidy, never an append | IMPLEMENTED |  | `compute_palw_credit_outputs`:2/0, `max_outputs`:13/0, `executor_bond_outpoint`:42/4, `verifier_bond_outpoint`:23/0, `validator_pubkey_hash`:102/6 | 0 |
| 0037 | Decision 8 — `P_check` and self-declared capacity are out of the safet | IMPLEMENTED-BUT-UNVERIFIED |  | `P_check`:10/0, `last_credited_daa_by_class`:8/0, `credited_amount_this_epoch`:9/0, `active_unfinalized_exposure`:15/0, `max_leverage`:6/0 | 0 |
| 0037 | Decision 9 — On-chain class registry; freeze halts credit, not the cha | IMPLEMENTED-BUT-UNVERIFIED |  | `PalwExecutionClassState`:0/0, `activation_epoch`:8/0, `freeze_reason`:13/0, `class_frozen`:10/0, `libm_arithmetic_digest`:7/0 | 0 |
| 0037 | Decision 10 — Reconciliation and the 10 BPS question | SUPERSEDED |  | `W_challenge`:5/0 | 1 |
| 0037 | Implementation order and current status (2026-08-17) | SUPERSEDED |  | `PalwWorkerFailed`:3/0, `PalwJobStateV3`:14/0, `job_id`:261/3, `job_context_hash`:196/0, `PalwCreditBatch`:0/0, `ExecutionStepRefutation`:0/0 | 2 |
| 0038 | ADR-0038: PALW is the consensus work — sampled-verified LLM PoW, a rec | SUPERSEDED |  |  | 1 |
| 0038 | Context — why ADR-0037 Decision 1 was wrong | IMPLEMENTED |  | `bits`:813/29 | 0 |
| 0038 | Implementation status — Decision A, 2026-08-18 | IMPLEMENTED |  | `Params::palw_block_commitment`:46/2, `None`:0/0, `palw_commitment`:159/52, `valid_header`:1/0, `palw_commitment_root`:1/0, `pow_layer0::check_palw_commitment_shape`:46/0, `PalwBlockCommitmentV1::validate_executor_bond_v | 0 |
| 0038 | What Decision A still needs, in dependency order | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0038 | Finality that eroded with nothing but time, and the property that catc | DEFERRED |  |  | 1 |
| 0038 | The third unbounded carriage arm: a late `Open` un-matured a `Final` b | IMPLEMENTED |  | `Open`:96/2 | 1 |
| 0038 | Decision C's freeze clause — implemented, and the block was not where  | DEFERRED |  |  | 1 |
| 0038 | Decision C — Verification: assigned sampling is the alarm, the court i | IMPLEMENTED-BUT-UNVERIFIED |  | `select_replay_panel_v1`:31/0 | 2 |
| 0038 | Decision D — Multi-class difficulty: per-class DAA, static PWU only in | SUPERSEDED |  |  | 1 |
| 0038 | Decision E — What hash still does | DESIGN-ONLY |  |  | 2 |
| 0038 | Decision G — What survives from ADR-0037, re-seated | SUPERSEDED |  |  | 2 |
| 0038 | What actually blocks Decisions B and C: the carriage store cannot name | IMPLEMENTED |  | `PalwCarriageRecord::accepted_block`:90/2 | 4 |
| 0038 | External audit, 2026-08-19: NO-GO — and what the status table below do | IMPLEMENTED |  | `None`:0/0, `Open`:96/2 | 1 |
| 0038 | Implementation status — Decisions A–D and H, 2026-08-19 | IMPLEMENTED |  | `sign_palw_block_commitment_v1`:0/0, `order_tips_v1`:0/1, `Params`:933/38, `palw_credit`:51/0, `palw_block_commitment`:46/2, `palw_schedule`:60/0, `palw_ramp`:16/0, `palw_fork_choice`:34/1, `None`:0/0, `absent_schedule_a | 2 |
| 0038 | Consequences | DEFERRED |  |  | 1 |
| 0038 | Invariants v2 (release-blocking; supersede ADR-0037's I1/I2/I12, carry | SUPERSEDED |  |  | 2 |
| 0038 | Audit closure — do the criticals recur? | SUPERSEDED |  |  | 1 |
| 0038 | Residual assumption set (what a signer of this ADR accepts) | DESIGN-ONLY |  |  | 1 |
| 0039 | ADR-0039: PALW-only block production — a Base class instead of a hash  | SUPERSEDED |  |  | 1 |
| 0039 | The impossibility, stated rather than finessed | DEFERRED |  |  | 1 |
| 0039 | Decision 1 — The floor is a CLASS, not a hash: `PALW-BASE-0` | IMPLEMENTED |  | `libm`:55/5 | 1 |
| 0039 | Decision 2 — W6′ (supersedes W6): PALW-only liveness | DEFERRED |  |  | 2 |
| 0039 | Decision 3 — W4′ (supersedes W4): two derived weights, one fork choice | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0039 | Decision 4 — The ticket is not a hash puzzle | DEFERRED |  | `b385803`:0/0 | 3 |
| 0039 | Decision 5 — Per-class share, epoch caps, and how caps are enforced | SUPERSEDED |  | `denom_c`:7/0 | 2 |
| 0039 | Decision 5 amendment (2026-08-17) — the clause as written cannot be en | SUPERSEDED |  | `weight`:1488/73, `safe`:381/18, `live`:990/47, `chain_weights_v1`:22/0, `palw_pwu`:46/5, `pwu_c`:4/0, `StarvedClass`:6/0, `class_epoch_budgets_v1`:25/0 | 6 |
| 0039 | Decision 6 — Bonded is not permissioned | SUPERSEDED |  |  | 1 |
| 0040 | ADR-0040: `PALW-BASE-0` — the integer-only arithmetic normative specif | SUPERSEDED |  |  | 1 |
| 0041 | ADR-0041: PALW pruning-proof verification — exhaustive and amortised,  | SUPERSEDED |  |  | 1 |
| 0041 | Context — one inference per proof header does not scale, and the block | DEFERRED |  |  | 1 |
| 0041 | The decision | SUPERSEDED |  |  | 1 |
| 0041 | Decision 1 — WITHDRAWN: sampling the PoW checks is UNSOUND in this pro | IMPLEMENTED |  | `compare_proofs_inner`:7/0, `blue_work_diff`:3/0, `pow_passes`:2/0, `bits`:813/29, `calc_work`:20/1, `header_level`:4/0, `level_work`:29/0 | 0 |
| 0041 | Decision 1′ — Amortise the per-header cost (this is the real lever) | DEFERRED |  |  | 1 |
| 0041 | Fleet measurement 2026-08-18 (`misaka-ibm`, 8-vCPU KVM EPYC @ 2.0 GHz, | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0041 | Decision 2 — Parallelise verification during IBD — **landed, and it bu | IMPLEMENTED-BUT-UNVERIFIED |  | `SPAWN_GATE`:0/0, `MISAKA_PALW_CONCURRENCY`:2/0, `vmstat`:0/0, `palw_agent_concurrency`:1/0 | 0 |
| 0041 | Decision 3 — Hard header cap, enforced before any inference | IMPLEMENTED |  | `daa_score`:1446/139 | 0 |
| 0041 | Decision 4 — Cheap checks gate the inference — **already landed** | IMPLEMENTED-BUT-UNVERIFIED |  | `check_proof_header_shape`:8/0 | 0 |
| 0041 | Consequences | SUPERSEDED |  |  | 1 |
| 0042 | ADR-0042: The PALW mainnet-candidate ruleset — one atomic activation,  | SUPERSEDED |  |  | 5 |
| 0042 | The two-network split this ADR assumes | DESIGN-ONLY |  |  | 1 |
| 0042 | Decision 1 — One atomic activation bundle, not five fences | IMPLEMENTED |  | `Option`:0/0, `ConsensusV2`:583/137 | 2 |
| 0042 | Decision 2 — The block state machine, and why a fresh tip is weighable | IMPLEMENTED |  | `None`:0/0, `UnresolvedBlock`:4/0, `Provisional`:152/5, `Final`:530/35, `Voided`:179/2, `Unresolved`:18/0 | 3 |
| 0042 | Decision 3 — `PalwAttemptEnvelopeV2`, a new algo id, and identity by ` | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0042 | 3a. The commitment binds to the PoW, or the PoW is free | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0042 | 3c. Block identity is `attempt_id`, not the raw signature — **deferred | IMPLEMENTED |  | `block_id`:0/0, `palw_v2_a_bond_holders_own_resignature_buys_a_block_but_never_a_second_claim`:0/1, `decode_wire`:13/10, `validate_shape_v2`:2/0 | 1 |
| 0042 | 3d. A new algo id, so no old node re-interprets a V2 block | IMPLEMENTED |  | `POW_ALGO_ID_PALW_COMMITTED_V2`:115/18, `POW_ALGO_ID_PALW_LLM`:43/2, `POW_ALGO_ID_PALW_OLLAMA`:16/0 | 2 |
| 0042 | Decision 4 — The full node runs no model (closes W1 / the runtime half | IMPLEMENTED |  | `misakad`:0/0, `consensus`:2101/857, `PalwWorkerFailed`:3/0 | 0 |
| 0042 | Decision 5 — Candidate-scoped PALW state, and an authenticated commitm | IMPLEMENTED |  | `bond_view`:71/4 | 3 |
| 0042 | Decision 6 — Admission split, and per-bond exposure (closes P0-2, P0-1 | IMPLEMENTED |  | `attempt_id`:27/7, `Provisional`:152/5, `ReceiptLicensed`:228/17, `Final`:530/35, `Voided`:179/2, `timeout`:277/3 | 3 |
| 0042 | Decision 7 — Panel, data availability, and no-show (closes the wiring  | IMPLEMENTED |  | `validator_pubkey_hash`:102/6, `operator_root`:11/0, `None`:0/0, `operator_id`:252/24, `min_collateral_sompi`:117/5, `Provisional`:152/5, `unavailable`:98/1 | 1 |
| 0042 | Decision 8 — The BASE-0 court is complete and proof-carrying (closes P | IMPLEMENTED |  | `PalwWeightOracleV1`:31/0, `PalwNoWeightsV1`:5/0, `None`:0/0, `Unadjudicable`:310/0, `d1891333`:1/0, `a7be964e`:0/0, `PalwCourtParamsV2::max_step_leaf_count`:477/27, `window_court`:222/13 | 2 |
| 0042 | Decision 9 — One fork-choice authority (closes the other half of P0-5) | IMPLEMENTED |  | `header_download_hint`:0/0, `header_selected_tip`:0/0, `skip_adding_genesis`:8/5 | 2 |
| 0042 | Decision 10 — Reward is not spendable before `Final` (closes reward-be | IMPLEMENTED |  | `Provisional`:152/5, `Voided`:179/2, `Final`:530/35 | 1 |
| 0042 | Decision 11 — The ruleset fingerprint, committed to genesis (RC == mai | IMPLEMENTED |  | `palw_ruleset_id`:7/0, `network_domain`:426/69, `network_id`:820/10, `class_catalog_root`:43/2, `court_catalog_root`:31/0 | 0 |
| 0042 | The release gate — the audit's 12 conditions, as one checklist | DESIGN-ONLY |  |  | 1 |
| 0042 | A1 — The court races `window_court`, not the challenge window (Decisio | SUPERSEDED |  | `Final`:530/35, `PalwCourtParamsV2`:284/16, `verify_against_catalog`:14/0, `a7be964e`:0/0 | 1 |
| 0042 | A2 — Decision 3c must not land without a mutated-witness path | IMPLEMENTED |  | `attempt_id`:27/7, `palw_commitment`:159/52, `commitment_root_v2`:16/1, `palw_v2_commitment_mutation_invalidates_pow`:5/1 | 0 |
| 0042 | A3 — Decision 7's Sybil guarantee, restated as the bound it is | IMPLEMENTED |  | `operator_id`:252/24, `min_collateral_sompi`:117/5 | 0 |
| 0042 | What this ADR does not decide | IMPLEMENTED |  | `PalwConsensusParamsV2`:121/12 | 2 |
| 0042 | Number hygiene | DEFERRED |  |  | 1 |
| 0043 | ADR-0043 — PALW V2 state-root hash ordering (no challenge↔commitment c | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0043 | 1. What a V2 header commits, and why there is no cycle | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0043 | 2. The root derivation, frozen | IMPLEMENTED |  | `epoch_budgets`:43/10, `d4890a78`:0/0, `retired_safe_weight`:43/12, `bb62f1fc`:0/0, `PalwCandidateOrderV1::candidate`:1012/93, `Hash64`:6342/362 | 6 |
| 0043 | 4. The carriage, and the None-root rule | IMPLEMENTED |  | `PalwStateCarriageV2`:116/25 | 1 |
| 0043 | 6. Number hygiene | DEFERRED |  |  | 1 |
| 0043 | Amendment 2026-09-02 — version 17 (ADR-0078): the derivation table | IMPLEMENTED |  | `derived_artifacts`:39/1 | 1 |
| 0044 | ADR-0044: Free-prompt PALW — the user's own inference becomes the cons | SUPERSEDED |  | `a460cdd7`:1/0, `PWU_PER_QUANTUM`:2/0 | 7 |
| 0044 | Context — what was asked, and the two flaws in the straightforward ans | DEFERRED |  |  | 1 |
| 0044 | Flaw 1 — "future block hash" randomness is free to grind once blocks n | DEFERRED |  |  | 2 |
| 0044 | Flaw 2 — any executor-chosen field the inference does not consume is a | IMPLEMENTED |  | `receipt_id`:11/2 | 2 |
| 0044 | What certification reuses | DEFERRED |  |  | 1 |
| 0044 | Decision 1 — Two work sources, one atomic bundle | SUPERSEDED |  | `ConsensusV2`:583/137, `POW_ALGO_ID_PALW_COMMITTED_V2`:115/18, `attempt_share_permille`:0/0, `POW_ALGO_ID_PALW_RECEIPT_V3`:68/14, `palw_ruleset_id_v2`:119/0 | 1 |
| 0044 | Decision 2 — The free-prompt job: user tokens in, nothing appended, to | IMPLEMENTED |  | `PalwClassRegistrationV1`:26/0, `prompt_token_ids`:404/5, `new_job_input`:1/0 | 0 |
| 0044 | Decision 3 — The execution commitment, and certification through the e | IMPLEMENTED |  | `FreePromptCommitted`:52/5, `Provisional`:152/5, `derive_panel_v2`:40/0, `validate_receipt_quorum_v2`:12/1, `palw_court_v2`:135/8, `DoesNotAdjudicate`:6/0, `Voided`:179/2 | 1 |
| 0044 | Decision 4 — The beacon rule: only attempt blocks carry randomness | IMPLEMENTED |  | `PalwAnchorFactV2`:17/0, `prev_attempt_daa`:36/3 | 1 |
| 0044 | Decision 5 — Quantized one-shot tickets | IMPLEMENTED |  | `Final`:530/35, `job_nonce`:42/2, `palw_ticket_admits_v1`:18/2, `max_quanta_per_receipt`:19/1, `MAX_PWU_PER_RECEIPT`:0/0 | 1 |
| 0044 | Decision 6 — The receipt block (algo 7): admission a full node runs wi | IMPLEMENTED |  | `spend_id`:10/0, `PalwChainStateV2`:562/39, `pwu_per_quantum`:1/1, `Final`:530/35, `Provisional`:152/5, `palw_reward_v2`:28/0, `nonce`:933/157, `is_palw_algo`:0/0 | 4 |
| 0044 | Decision 7 — Pricing: CU from the executed shape, conservative by cons | IMPLEMENTED |  | `decode_token_limit`:91/2, `EndOfGeneration`:34/3, `decode_tokens_executed`:151/2 | 2 |
| 0044 | Decision 8 — Data availability and privacy, v1 | IMPLEMENTED |  | `PublicDa`:36/1, `max_prompt_tokens`:35/1 | 1 |
| 0044 | Decision 9 — Bundle extension, invariants, and what moves | IMPLEMENTED |  | `PalwConsensusParamsV2`:121/12, `retarget_over_span_v1`:21/0, `None`:0/0, `palw_ruleset_id_v2`:119/0 | 0 |
| 0044 | Decision 10 — The user pipeline: one inference, an answer and a commit | IMPLEMENTED |  | `job_nonce`:42/2 | 4 |
| 0044 | Invariants | DESIGN-ONLY |  |  | 1 |
| 0044 | Threat disposition | DEFERRED |  |  | 1 |
| 0044 | Implementation ladder | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0044 | Number hygiene | DEFERRED |  |  | 2 |
| 0045 | ADR-0045: The class economy is chain state — derived PWU, block-denomi | SUPERSEDED |  |  | 4 |
| 0045 | Thesis | DESIGN-ONLY |  |  | 1 |
| 0045 | Decision 1 — pwu has exactly one legal value (the ADR-0039 derivation  | IMPLEMENTED |  | `PalwPwuRuleV2`:148/25, `DerivedV1`:63/9, `class_target`:140/31, `check_pwu_claim_v1`:10/1, `PwuClaimNotDerived`:5/0, `pwu_per_inference`:253/30, `ZeroPwuPerInference`:6/0, `PalwCourtParamsV2::max_step_leaf_count`:477/27 | 2 |
| 0045 | Decision 2 — the epoch budget's currency is the block | IMPLEMENTED-BUT-UNVERIFIED |  | `StarvedClass`:6/0, `max_factor`:42/0, `ZeroBudget`:5/0, `class_epoch_budgets_v1`:25/0 | 3 |
| 0045 | Decision 3 — the share table is chain state, granted at registration,  | IMPLEMENTED |  | `PalwChainStateV2`:562/39, `PalwClassDaaV2Params`:1/0, `PalwStateParamsV2`:267/26, `base_class_id`:146/37, `class_daa_max_factor`:13/0, `budget_tolerance_permille`:11/0, `ClassRegistered`:207/44, `share_permille`:205/27, | 1 |
| 0045 | What this ADR does not decide | DEFERRED |  |  | 1 |
| 0046 | ADR-0046 — PALW V2 consensus-object carriage: the registrations ride t | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0046 | Problem | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0046 | Decision 1 — one subnetwork id per kind, band 0x50, Borsh body, no mag | IMPLEMENTED |  | `ClassRegistered`:207/44, `ClassFrozen`:26/0, `ClassUnfrozen`:4/0, `verify_against_catalog`:14/0, `BindTimeout`:75/2, `ReceiptTimeout`:31/1, `apply_palw_transition_v2`:264/21 | 1 |
| 0046 | Decision 2 — two validation layers with two different failure meanings | IMPLEMENTED |  | `InvalidDnsOverlayPayload`:10/0, `apply_palw_transition_v2`:264/21 | 1 |
| 0046 | Decision 3 — the bond IS its collateral output | IMPLEMENTED |  | `SUBNETWORK_ID_PALW_V2_BOND`:0/0, `StakeBondPayload`:19/6, `collateral`:916/60, `BondOutputValueMismatch`:5/0, `attempt_id`:27/7, `PALW_V2_BOND_BURN_SPK`:0/0, `SUBNETWORK_ID_PALW_V2_RETIRE`:0/0 | 1 |
| 0046 | Decision 4 — panels are derived, receipts are counted, courts carry th | IMPLEMENTED |  | `PALW_V2_BIND`:0/0, `derive_panel_v2`:40/0, `PanelBound`:164/15, `validate_panel_bound_v2`:14/0, `PALW_V2_RECEIPTS`:0/0, `validate_receipt_quorum_v2`:12/1, `signed_daa`:39/18, `Unavailable`:165/0, `Licensed`:18/0, `Recei | 2 |
| 0046 | Decision 5 — order is acceptance order | IMPLEMENTED |  | `attempt`:1988/217 | 0 |
| 0046 | What this ADR does not decide | IMPLEMENTED |  | `PalwCourtVerdictProofV2`:80/5 | 1 |
| 0046 | Number hygiene | DEFERRED |  |  | 3 |
| 0047 | ADR-0047: The A16 activation tier — sixteen-bit activations for the cl | IMPLEMENTED |  | `artifact_root`:552/60 | 2 |
| 0047 | Context: the measured ceiling | DESIGN-ONLY |  |  | 1 |
| 0047 | What remains open (recorded, not hidden) | IMPLEMENTED-BUT-UNVERIFIED |  | `a16_row`:1/0 | 1 |
| 0049 | ADR-0049: The adjudication contract — what a court opens, and the boun | DEFERRED |  |  | 3 |
| 0049 | Amendment, 2026-08-26 — the ceilings are numbers now, and the metric t | DEFERRED |  |  | 2 |
| 0049 | Decision E — decode is adjudicable, by challenging the argmax rather t | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0050 | (no decision/keyword sections) | — |  |  | 0 |
| 0051 | ADR-0051: The Metal/GGUF execution family — native-speed inference as  | SUPERSEDED |  |  | 1 |
| 0051 | Context — what one real model cost, and what it bought | DEFERRED |  |  | 1 |
| 0051 | Why not bit-exact on Metal | DEFERRED |  |  | 1 |
| 0051 | Decision 1 — Two families, one economy, half each | IMPLEMENTED |  | `relu2`:0/0, `budget_blocks`:54/10 | 1 |
| 0051 | Decision 2 — A Family-M class is defined by what it pins, not how it c | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0051 | Decision 3 — The commitment is the v2 trace scheme, which already exis | IMPLEMENTED-BUT-UNVERIFIED |  | `full_logits_trace_root_v2`:32/0, `output_token_ids_hash_v2`:10/0 | 2 |
| 0051 | Decision 4 — Verification is teacher-forced spot replay by the panel t | IMPLEMENTED |  | `ReceiptLicensed`:228/17, `Final`:530/35, `PalwSeatReceiptV2`:58/16, `ReceiptTimeout`:31/1, `Unavailable`:165/0, `ProducerWithholding`:25/0, `gemm_trace_root`:35/0, `Valid`:260/10, `execution_root`:359/33, `trace_root`:4 | 0 |
| 0051 | Decision 5 — What this family cannot do, said out loud | IMPLEMENTED-BUT-UNVERIFIED |  | `ComputationMismatch`:10/0, `DecodeTokenMismatch`:14/0 | 0 |
| 0051 | Decision 6 — The one structural change: per-class panel parameters | SUPERSEDED |  | `PalwPanelParamsV2`:57/0 | 1 |
| 0051 | Decision 7 — pwu, and why "work coefficient" games dissolve | IMPLEMENTED |  | `DerivedV1`:63/9, `pwu_per_inference`:253/30 | 0 |
| 0051 | Decision 8 — UX: the use IS the work | IMPLEMENTED-BUT-UNVERIFIED |  | `l1_tag`:13/0 | 0 |
| 0051 | What this walks back, and what it does not | DEFERRED |  |  | 1 |
| 0051 | Risks, stated | DEFERRED |  |  | 1 |
| 0051 | Measured after writing this ADR (2026-08-22/23, Apple M4 Pro) | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0052 | ADR-0052: `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid  | SUPERSEDED |  |  | 3 |
| 0052 | The court → the catalog is complete and the admission gate passes | DEFERRED |  |  | 1 |
| 0052 | Still open | IMPLEMENTED |  | `Qwen36Engine`:58/5 | 1 |
| 0053 | ADR-0053: One execution family — Family M is withdrawn, and the court  | SUPERSEDED |  |  | 2 |
| 0053 | And it could not have run — for a reason no operator could fix | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0053 | Decision 1 — There is one execution family, and it is not a value | SUPERSEDED |  | `PalwExecutionFamilyV1`:63/0 | 2 |
| 0053 | Decision 2 — `PalwClassTermsV2` is deleted, and the class record shrin | IMPLEMENTED |  | `PalwRuntimePinsV2`:0/0, `min_class_panel`:2/0, `min_panel_seats`:0/0, `min_panel_quorum`:0/0, `PalwRegistrationTermsV2`:32/0, `PALW_STATE_V2_VERSION`:35/1, `the_version_8_state_root_golden_vectors`:0/0 | 0 |
| 0053 | Decision 1a — What "one gate" does and does not claim, at genesis | IMPLEMENTED |  | `verify_class_admission_v2`:37/0, `validate_shape`:177/3, `verify_palw_genesis_v2`:66/7, `PalwClassCatalogV2`:36/7, `verify_against_catalog`:14/0, `derive_court_cost_v1`:61/0, `max_opening_bytes`:1/0, `max_terminal_macs` | 0 |
| 0053 | Decision 3 — One panel: the network's | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0053 | Decision 4 — `--palw-register-class` survives, pointed at a class the  | SUPERSEDED |  | `family_m_post_genesis_registration_v1`:1/0, `palw_post_genesis_registration_v1`:12/0, `pwu_per_inference`:253/30 | 1 |
| 0053 | Decision 5 — ADR-0034's `Metal` routing family becomes reserved | SUPERSEDED |  | `Metal`:53/0, `family_is_reserved_v1`:3/0, `Cuda`:9/0, `Rocm`:5/0 | 2 |
| 0053 | Decision 6 — What is deleted, in full | IMPLEMENTED |  | `MetalBackend`:0/0, `kaspad`:0/0, `PalwExecutionFamilyV1`:63/0, `PalwClassTermsV2`:1/0, `PalwRuntimePinsV2`:0/0, `terms`:239/14, `PalwClassStateV2`:31/0, `ClassRegistered`:207/44, `palw_metal_class_admission_v1`:0/0, `va | 0 |
| 0053 | Consequences | DEFERRED |  |  | 2 |
| 0054 | ADR-0054: A class's cadence share follows its own production | SUPERSEDED |  |  | 3 |
| 0054 | Decision 1 — production earns cadence, silence returns it | IMPLEMENTED |  | `class_growth_permille`:17/2 | 1 |
| 0054 | Decision 2 — the floor keeps a reserve, and it is a number | IMPLEMENTED |  | `base_class_reserve_permille`:5/2, `with_class_share_growth_v1`:6/4 | 0 |
| 0054 | Decision 3 — where it runs, and what it reads | IMPLEMENTED-BUT-UNVERIFIED |  | `apply_class_retargets`:4/0, `ensure_epoch_budgets`:15/0 | 1 |
| 0055 | ADR-0055 — Chain position is earned, and the question is set by the bl | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0055 | Context | DESIGN-ONLY |  |  | 2 |
| 0055 | Decision | DEFERRED |  |  | 1 |
| 0055 | Consequences | IMPLEMENTED |  | `ConsensusV2`:583/137 | 1 |
| 0056 | ADR-0056: Permissionless class admission, and the share economy that s | SUPERSEDED |  |  | 2 |
| 0056 | Decision 1 — The constitution: admission is arithmetic, and only arith | IMPLEMENTED |  | `ClassRegistered`:207/44 | 0 |
| 0056 | Decision 2 — The kernel boundary: what needs a binary, and what does n | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0056 | Decision 3 — Registration exposure: entry is priced in bonded collater | IMPLEMENTED |  | `ClassRegistered`:207/44, `ClassFrozen`:26/0, `verify_admission_v2`:0/0, `REGISTRATION_EXPOSURE_SOMPI`:3/0, `Registered`:83/8, `Active`:560/43, `max_exposure_ratio_permille`:31/1 | 0 |
| 0056 | Decision 4 — Withdrawn: the share walk is ADR-0054's, not this one's | SUPERSEDED |  | `reclaim_epochs`:11/0, `min_base_class_share_permille`:18/1 | 1 |
| 0056 | Decision 5 — Reclamation: dead classes give the network back | IMPLEMENTED |  | `apply_class_reclamation`:5/0, `reclaim_epochs`:11/0, `activate_due_classes`:6/3, `RECLAIM_EPOCHS`:2/0, `REGISTRATION_EXPOSURE`:0/0, `Registered`:83/8, `ClassRegistered`:207/44, `class_id`:1870/155, `DuplicateClass`:8/0 | 3 |
| 0056 | Decision 6 — Duplicates are priced, not policed | IMPLEMENTED |  | `artifact_root`:552/60 | 0 |
| 0056 | Decision 7 — What the chain does not judge | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0056 | The attack table | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0057 | Context — the question, and the wrong answer already taken once | SUPERSEDED |  |  | 1 |
| 0057 | Decision 1 — the semantic boundary is the catalogued kernel | IMPLEMENTED |  | `kernel_semantics_id`:102/2 | 0 |
| 0057 | Decision 2 — no kernel certificates | IMPLEMENTED-BUT-UNVERIFIED |  | `ref2`:4/0 | 0 |
| 0057 | Decision 3 — the gate is differential, per backend, and it must fire | DEFERRED |  |  | 1 |
| 0057 | Decision 4 — the order of work | IMPLEMENTED-BUT-UNVERIFIED |  | `sdot`:11/0, `i8mm`:3/0, `dp4a`:0/0 | 0 |
| 0057 | Decision 5 — what the survey suggested that is refused by name | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0057 | Decision 6 — fusion stops at the committed row | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0058 | The defect, measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0058 | The decision | DEFERRED |  |  | 2 |
| 0058 | Rejected alternatives | DESIGN-ONLY |  |  | 1 |
| 0059 | Why the defect this closes was invisible | DEFERRED |  |  | 1 |
| 0059 | What deliberately does not move | SUPERSEDED |  |  | 1 |
| 0060 | ADR-0060: The liveness doctrine — time is permissionless, weight is bo | IMPLEMENTED |  | `u64::MAX`:0/0, `palw_rc_arm_phase1`:16/0, `Params::palw_heartbeat`:56/8 | 2 |
| 0060 | 1. The failure family, measured | DESIGN-ONLY |  |  | 1 |
| 0060 | 3. Decision 1 — the heartbeat lane (new) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0060 | What implementation added to Decision 1 | IMPLEMENTED |  | `palw_state_root`:62/10, `NonPalwHeaderCarriesPalwCommitment`:5/0, `UncommittedPalwStateRoot`:2/0, `processes::heartbeat_evidence`:0/0, `heartbeat_adapt_block_template`:7/13 | 0 |
| 0060 | 4. Decision 2 — the emergency ramp (new) | DEFERRED |  |  | 1 |
| 0060 | 5. Decision 3 — producer-bond self-healing is now unconditional (mostl | IMPLEMENTED |  | `palw_v2_collateral_for_claim_lifetime_v1`:15/11, `min_collateral_sompi`:117/5 | 1 |
| 0060 | 6. Decision 4 — the finality inactivity leak (new; overlay-scoped) | SUPERSEDED |  | `Params::palw_inactivity_leak`:29/0, `DnsParams`:118/1, `u64::MAX`:0/0, `InactivityLeakViewV1`:0/0, `total_active_stake_by_epoch`:8/2, `total_voting_weight_by_epoch`:0/0 | 4 |
| 0060 | 7. Decision 5 — refusal gates decay (partially landed) | IMPLEMENTED |  | `dns_veto_ttl_daa_score`:42/6, `e05a8699`:0/0 | 1 |
| 0060 | 9. Implementation staging | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0060 | 11. Implementation record (2026-08-30) | IMPLEMENTED |  | `palw_heartbeat_v1`:28/8, `processes::heartbeat_evidence`:0/0, `accepts_algo_id`:35/0, `own_subsidy`:7/0, `expected_coinbase_transaction`:18/1, `heartbeat_adapt_block_template`:7/13, `PALW_STATE_V2_VERSION`:35/1, `Inacti | 1 |
| 0060 | 12. What the audit changed (2026-08-30, same day) | DESIGN-ONLY |  |  | 5 |
| 0061 | ADR-0061: Zero-seat genesis, and collateral sized by arithmetic instea | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0061 | The two decisions | DEFERRED |  |  | 2 |
| 0062 | ADR-0062 — The data-availability court: stop a vote from taking a bond | IMPLEMENTED-BUT-UNVERIFIED |  | `Params::palw_da_court`:87/0 | 2 |
| 0062 | Second security amendment (2026-09-03) — SA-7: what an accusation cost | IMPLEMENTED-BUT-UNVERIFIED |  | `CourtOpened`:55/0 | 1 |
| 0062 | Implementation, 2026-09-02 — the amended form, behind `palw_da_court` | IMPLEMENTED |  | `Params`:933/38, `None`:0/0 | 3 |
| 0062 | SA-7, 2026-09-03 — the same fence, widened | IMPLEMENTED-BUT-UNVERIFIED |  | `DefaultDisputed`:46/0 | 3 |
| 0062 | Implementation, 2026-09-06 — the disclosure the shipped commitments al | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0062 | Arming on testnet-11, 2026-09-06 — a scheduled fence, and what the sta | DEFERRED |  |  | 1 |
| 0063 | The defect that locks money in | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0063 | D3. `misaka bond status` — the outpoint, from the chain | IMPLEMENTED |  | `getPalwProducerFacts`:16/1, `locked_outpoints`:3/0, `bond_registered_pubkey`:29/0 | 0 |
| 0063 | Record (2026-09-05) | DEFERRED |  |  | 1 |
| 0064 | ADR-0064 — Trustless recovery from a total producer stop: the bond bec | SUPERSEDED |  |  | 5 |
| 0064 | Fact A — "the network was silent" is not a checkable predicate. This o | DEFERRED |  |  | 2 |
| 0064 | Fact B — a zero-weight lane hands fork choice to the block hash, for f | SUPERSEDED |  | `palw_tip_weights_v1`:0/0, `None`:0/0 | 2 |
| 0064 | Decision — no new lane. Move one lookup. | SUPERSEDED |  | `BondRegistered`:153/20 | 3 |
| 0064 | The objection this exposes — and it is a P0 that exists TODAY | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0064 | Staging | SUPERSEDED |  | `consensus_identity_id`:268/1, `None`:0/0 | 5 |
| 0065 | ADR-0065 — A bond must be earned, and a failure is not a verdict | SUPERSEDED | DORMANT(palw_bond_maturity) | `None`:0/0, `palw_bond_maturity`:53/0 | 4 |
| 0065 | The single root | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0065 | Decisions | SUPERSEDED |  | `prev_sink`:49/16, `FrontierProvenanceViolation`:3/2, `Unavailable`:165/0, `artifact_root`:552/60, `palw_bond_collateral_is_locked_v2`:15/0, `since_daa`:36/0 | 12 |
| 0065 | Correction, 2026-08-31 — the rule was enforceable and unarmable at the | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0066 | ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity | IMPLEMENTED-BUT-UNVERIFIED |  | `Params::palw_attempt_work_fence`:6/0, `None`:0/0 | 3 |
| 0066 | Why the first implementation failed, sorted by cause | IMPLEMENTED-BUT-UNVERIFIED |  | `MAX_DIFFICULTY_TARGET`:37/0 | 2 |
| 0066 | Decision 1 — the lane gets its own algorithm id, and its price never t | IMPLEMENTED |  | `palw_state_root`:62/10, `Header`:374/12, `POW_ALGO_ID_HEARTBEAT_V1`:23/0, `pow_algo_id`:231/26 | 0 |
| 0066 | Decision 2 — the slot rule is one block deep, and the evidence walk is | DESIGN-ONLY |  | `heartbeat_evidence`:0/0 | 0 |
| 0066 | Decision 3 — ε stops competing with a V2 block's work | IMPLEMENTED |  | `bits`:813/29 | 2 |
| 0066 | Decision 4 — the inactivity leak needs committed per-validator state | IMPLEMENTED |  | `last_attestation_daa_by_validator`:0/0, `overlay_commitment_root`:60/2, `PruningPointOverlaySnapshot`:26/0, `component_digests`:2/0, `dns_reorg_outcome`:8/7 | 1 |
| 0066 | The trap in the constant, and why both fences must be top level | IMPLEMENTED |  | `consensus_params_id`:297/13, `palw_ruleset_id_v2`:119/0 | 3 |
| 0066 | What landed, 2026-08-31 — and what did not | UNCLASSIFIED(no identifier) |  |  | 4 |
| 0066 | Security amendment (2026-09-02) — Decision 4's committed table, before | SUPERSEDED | DORMANT(palw_inactivity_leak) | `VirtualStateProcessor::palw_leak_table_provenance`:0/0, `SelfComputed`:0/0, `stake_score_window_blue_score`:38/20, `dns_finality::leak_table_provenance_from_walk_v1`:0/0, `Unverified`:5/1, `PruningPointOverlaySnapshot`: | 2 |
| 0067 | ADR-0067 — Classes are chain data; only kernels are the build | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0067 | Decision 1 — the chain state is the class catalog; the compiled table  | IMPLEMENTED |  | `resolve`:338/7, `registration_candidate`:9/0, `class_id`:1870/155 | 0 |
| 0067 | Decision 2 — execution from the registered profile | IMPLEMENTED |  | `PalwShapeProfileV3`:618/24 | 0 |
| 0067 | Decision 3 — the kernel set is the consensus surface, and it is irredu | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0067 | Decision 4 — distribution and service stay off-chain, and the chain ke | IMPLEMENTED |  | `ClassRegistered`:207/44 | 0 |
| 0067 | Decision 5 — the interpreter ships behind a fence, and the fence's arm | IMPLEMENTED |  | `resolve`:338/7 | 1 |
| 0067 | Decision 6 — four storage tiers, and node storage is never consensus s | IMPLEMENTED-BUT-UNVERIFIED |  | `Incapable`:59/0, `is_live_at`:0/0 | 0 |
| 0067 | What landed (2026-08-31) | IMPLEMENTED |  | `verify`:645/55 | 1 |
| 0067 | Why the fourth is not merely unbuilt (2026-09-01) | SUPERSEDED |  | `structural_diff_v1_v2`:2/0 | 20 |
| 0067 | What an adversarial audit found afterwards (2026-08-31) | DESIGN-ONLY |  |  | 1 |
| 0067 | Record (2026-09-05) — the fourth item | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0068 | ADR-0068: The LLM-primary economy — the floor retires to the doctrine' | IMPLEMENTED |  | `max_per_mergeset`:46/6 | 8 |
| 0068 | 3. The three phases | SUPERSEDED |  | `min_base_class_share_permille`:18/1 | 4 |
| 0068 | 4. Phase 1's two closures (implemented on this branch) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0068 | F2 — the attempt lane's blue work leaves `calc_work(bits)` | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0068 | F3a — sibling heartbeat width is bounded where `mergeset_size_limit` l | DEFERRED |  |  | 1 |
| 0069 | Decision 1 — Two adjudicability properties, named apart | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0069 | Decision 2 — E2E certification is a build fact, committed like the cat | IMPLEMENTED |  | `court_catalog_root`:31/0, `court_e2e_root`:53/2, `PalwReachableKernelSetV1`:27/0 | 0 |
| 0069 | Decision 3 — The certification drill, defined | IMPLEMENTED |  | `execute`:282/13, `adjudicate_court_close_v2`:40/1, `check_step_refutation_v1`:18/1, `Base0CheckpointCaptureV1::push_chunks`:14/0, `next_geometry`:12/0, `attn`:207/7, `post`:466/13 | 0 |
| 0069 | Decision 4 — The graph must not lie about the engine (ADR-0049 Decisio | IMPLEMENTED |  | `check_graph`:0/0, `base0_check_graph_v1`:7/0, `A16Engine`:74/2 | 0 |
| 0069 | Decision 5 — The admission gate grants weight only to certified famili | IMPLEMENTED |  | `liveness_only`:0/0, `verify_class_admission_v2`:37/0, `granted_share_table_v2`:18/1, `court_e2e_root`:53/2, `family`:1568/104 | 0 |
| 0069 | Decision 6 — Permissionlessness and the doctrine are preserved | IMPLEMENTED |  | `court_e2e_root`:53/2, `court_catalog_root`:31/0 | 1 |
| 0069 | 6. Invariants to verify at each step | DEFERRED |  |  | 1 |
| 0069 | Security amendment (2026-09-02) — the open item is a fork-choice hole, | ACTIVE (IMPLEMENTED) | ACTIVE(palw_uncertified_weightless=ForkActivation::always()) | `initial_target`:159/19, `pwu_per_inference`:253/30, `Final`:530/35, `safe`:381/18, `live`:990/47, `chain_weights_v1`:22/0, `palw_tip_weights_v1`:0/0, `safe_weight`:114/41, `bounded_immature`:56/12, `retired_safe_weight` | 13 |
| 0070 | ADR-0070: The model tiers' step spaces are adjudicable — end to end, a | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0070 | 1. The problem, stated the way the chain would have met it | IMPLEMENTED |  | `qwen25_a16_profile_v2`:65/5 | 1 |
| 0070 | 2. Decision — the acceptance test IS the property | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0070 | 4. The consensus changes, and why they are one version | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0070 | 7. What this ADR does NOT decide | IMPLEMENTED-BUT-UNVERIFIED |  | `forward_token_probed`:31/0 | 2 |
| 0071 | ADR-0071 — The attempt lane's price, the ticket's bound, and who may j | SUPERSEDED |  |  | 3 |
| 0071 | 1. Why these four are one ADR | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0071 | 2. What already landed, so the scope is honest | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0071 | 3. Decision 1 — The attempt lane's price comes off `header.bits` | IMPLEMENTED |  | `bits`:813/29, `POW_ALGO_ID_HEARTBEAT_V1`:23/0, `POW_ALGO_ID_PALW_COMMITTED_V2`:115/18, `expected_attempts`:24/2, `class_ticket_v2`:4/0 | 1 |
| 0071 | Decision 1 — WITHDRAWN after Relaunch 5 measured it (2026-09-02) | IMPLEMENTED |  | `PALW_ATTEMPT_BLUE_WORK_LOG2`:23/6, `bits`:813/29, `retarget_over_span_v1`:21/0, `DifficultyManager::calculate_difficulty_bits`:14/0, `BindTimeout`:75/2, `CandidateReview`:8/0, `converge_idle_target_v1`:14/0, `PALW_V2_AT | 3 |
| 0071 | Decision 1a — AMENDED at implementation: the expectation stays relativ | IMPLEMENTED |  | `retarget_over_span_v1`:21/0, `max_factor`:42/0, `ZeroPreviousTarget`:11/0, `floor_price`:7/0, `palw_class_daa::converge_idle_target_v1`:14/0, `continue`:926/34 | 1 |
| 0071 | 4. Decision 2 — The ticket is bound to executions, not to tries | IMPLEMENTED |  | `palw_job_anchor_v1`:6/0, `palw_ticket_v1`:6/0, `palw_admission_v2`:54/4, `consensus_params_id`:297/13, `palw_pwu_v1`:48/4, `kaspad`:0/0, `pwu_per_inference`:253/30, `expected_attempts`:24/2, `bits`:813/29, `DerivedV1`:6 | 0 |
| 0071 | 5. Decision 3 — A panel seat must be able to run the class it judges | ACTIVE (IMPLEMENTED) | ACTIVE(palw_unavailable_abstains=ForkActivation::always()) | `derive_panel_v2_with_maturity`:16/0, `PalwBondStateV2`:47/0, `pubkey`:430/109, `operator_id`:252/24, `collateral`:916/60, `slashed`:288/23, `status`:985/24, `registered_daa`:43/1, `payout_payload`:152/32, `palw_unavaila | 5 |
| 0071 | 7. Invariants to verify at each step | IMPLEMENTED |  | `bits`:813/29, `VirtualState::from_genesis`:24/2 | 2 |
| 0071 | What landed | IMPLEMENTED |  | `validate_palw_v2`:364/39 | 2 |
| 0071 | Security amendment (2026-09-02) — Decision 3 gets a bound and a price, | IMPLEMENTED |  | `BondCapabilityDeclared`:18/0, `palw_bond_capability_message_v2`:7/0, `CAPABILITY_EXPOSURE_SOMPI`:0/0, `Retiring`:50/5 | 0 |
| 0072 | ADR-0072 — The ticket is the execution: both lotteries priced in infer | SUPERSEDED |  | `bits`:813/29 | 1 |
| 0072 | 2. Decisions | SUPERSEDED |  | `bits`:813/29 | 3 |
| 0072 | 3. What this costs, stated before it is measured — and the rollout cho | SUPERSEDED |  |  | 2 |
| 0072 | 5. Supersession | SUPERSEDED |  |  | 1 |
| 0072 | Security amendment (2026-09-02) — the mainnet activation path, before  | DEFERRED |  |  | 2 |
| 0072 | Security amendment, second pass (2026-09-03) — what the implementation | IMPLEMENTED |  | `validate_stateless_v2`:16/0, `algorithm_id`:28/0, `PalwRulesetV2::validate`:362/23 | 5 |
| 0072 | What still argues against arming this fence | DEFERRED |  |  | 1 |
| 0072 | Security amendment, third pass (2026-09-03) — the gate the sweep could | IMPLEMENTED |  | `StatusDisqualifiedFromChain`:31/4 | 2 |
| 0073 | 2. Decisions | SUPERSEDED |  | `PalwFpCuWeightsV3`:0/0, `QUANTUM_CU`:1/1, `PWU_PER_QUANTUM`:2/0 | 1 |
| 0073 | 6. Supersession | SUPERSEDED |  |  | 2 |
| 0073 | Security amendment (2026-09-02) — preconditions on Phase ④ (Decision 4 | IMPLEMENTED |  | `derive_beacon_fact_v3`:27/0, `derive_beacon_fact_to_genesis_v3`:10/0, `fold_k`:10/0, `beacon_block`:59/12, `fp_beacon_fold_v3`:14/2, `beacon_daa`:48/4, `prev_attempt_daa`:36/3, `None`:0/0, `for_each_fence`:52/0, `consen | 1 |
| 0074 | ADR-0074: The attempt is a claim, drawn by the chain | SUPERSEDED |  |  | 1 |
| 0074 | 1. Why a beacon, and why not a validator | DESIGN-ONLY |  |  | 1 |
| 0074 | 2. Decisions | SUPERSEDED |  | `PalwFpCuWeightsV3`:0/0, `fp_cu_v3`:1/0, `QUANTUM_CU`:1/1, `PWU_PER_QUANTUM`:2/0 | 4 |
| 0074 | 5. Supersession | SUPERSEDED |  |  | 1 |
| 0074 | 7. What landed (2026-09-02, branch `palw-adr0073-fp-weight`) | SUPERSEDED |  | `step_leaf_count`:473/4, `QUANTUM_CU`:1/1, `PWU_PER_QUANTUM`:2/0 | 2 |
| 0075 | 2. Decisions | IMPLEMENTED-BUT-UNVERIFIED |  | `verify_class_admission_v3`:14/0 | 3 |
| 0075 | 6. The mainnet route for a model this build never pinned (Decision 8) | IMPLEMENTED |  | `PalwConsensusMode::Disabled`:54/0, `palw_v2_params_on_base`:13/0, `covering_rc_family_v1`:10/2, `FamilyCertified`:93/36, `every_catalog_class_on_a_registered_graph_is_covered_by_a_drillable_family`:1/0, `PalwRegistratio | 1 |
| 0075 | 7. Mainnet: the rules of operation (Decisions 9–13) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0075 | Security amendment (2026-09-02) — the permissionless lane's griefing b | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0076 | 3. Decision 1 — the seed is `share · pwu`, against one pinned scale | IMPLEMENTED |  | `retarget_over_span_v1`:21/0, `pwu_per_inference`:253/30 | 0 |
| 0076 | 4. Decision 2 — the scale is spent on headroom | IMPLEMENTED |  | `bits`:813/29, `u128::MAX`:0/0 | 1 |
| 0076 | 5. Decision 3 — the seed is written at genesis assembly, against the E | IMPLEMENTED |  | `share_permille`:205/27, `apply_palw_transition_v2`:264/21, `assemble_palw_rc_identity_v2`:19/0, `ClassRegistered`:207/44, `consensus_params_id`:297/13 | 0 |
| 0076 | 5b. Decision 4 — a class being SEATED is a class being priced | IMPLEMENTED |  | `ClassLaneCertified`:55/24, `min_grantable_share_permille`:44/6, `pwu_per_inference`:253/30 | 0 |
| 0076 | 7. Consequences | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0076 | Amendment (2026-09-02) — §8 restated against the shipped rule | DEFERRED |  |  | 1 |
| 0077 | ADR-0077: A prompt a person would type is a claim the court can try | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0077 | 1. What was measured | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0077 | Why eight and sixteen: the ceilings are the court's, and both are rule | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0077 | 2. The requirement, and the principle | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0077 | Phase A — the executor side, consensus-inert | IMPLEMENTED |  | `PalwFpWorkerRequestV3`:15/0, `PalwFpWorkerResultV3`:15/0, `n_ctx`:1227/94, `output_token_ids`:137/0, `class_id`:1870/155, `ClassLaneCertified`:55/24, `registered`:1590/104, `fp_certified`:68/0, `bond_active`:7/0, `expos | 8 |
| 0077 | Phase B — the court prices the checkpoint, not the context (one rulese | SUPERSEDED |  | `interval`:1733/19, `integer_kv_state_chunk_map_id_v2`:43/1, `gdn_core_genesis_replay`:13/0, `derive_court_cost_v1`:61/0, `positions`:1106/18, `max_close_bytes`:201/12, `COURT_MAX_STEP_LEAVES`:43/1, `PalwCourtParamsV2::w | 6 |
| 0077 | Phase C — the weight (sequenced, not re-decided) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0077 | Phase D — a prompt that is not published | IMPLEMENTED |  | `PanelDa`:88/2, `prompt_token_ids_hash`:198/7, `request_palw_material`:8/0, `Valid`:260/10, `ProducerDefaulted`:51/2, `PublicDa`:36/1 | 0 |
| 0077 | 4. What this costs, stated before it is measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0077 | 5. Invariants the tests must hold | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0077 | 6. Order of work | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0077 | 7. Supersession | IMPLEMENTED |  | `PublicDa`:36/1, `PanelDa`:88/2 | 2 |
| 0077 | 8. What is deliberately not decided | DEFERRED |  |  | 1 |
| 0077 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0077 | Implementation note, 2026-09-06 — Decision 16's transport (`f7363db9`) | IMPLEMENTED |  | `false`:0/0, `palw_fp_privacy_mode_peek_v1`:4/0, `PalwGossipAdmit::Private`:26/1, `PalwChainStateV2::claim_readers_v2`:4/0, `panel_da_armed`:41/0 | 1 |
| 0078 | 3. Decisions | IMPLEMENTED |  | `output_root`:327/28, `simulation`:43/3 | 7 |
| 0078 | 4. What this costs, stated before it is measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0078 | 8. What is deliberately not decided | DEFERRED |  |  | 2 |
| 0078 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0079 | 2. The line: determinism already writes the permission list; the OS sh | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0079 | Phase A — the doctrine (consensus-inert by construction) | IMPLEMENTED |  | `security_policy_hash`:1/0, `execution_commitment_v3`:38/0, `PalwFreePromptCommitmentV3`:25/0, `DerivedArtifactV1`:48/2, `JobFailed`:7/0 | 0 |
| 0079 | Phase B — the processes (privilege separation as a refusal) | IMPLEMENTED |  | `check_opening_request_shape`:18/0, `MISAKA_PALW_GGUF`:37/0, `MISAKA_PALW_GOLDEN`:15/0, `PATH`:38/0, `seccomp`:22/0, `socket`:59/3, `connect`:248/6, `execve`:38/0, `Landlock`:18/0, `none`:698/34, `RLIMIT_AS`:2/0, `PALW_W | 0 |
| 0079 | Phase C — the inputs (what a stranger may cause) | IMPLEMENTED |  | `agent`:96/6, `pinned_model_path_v2`:2/1, `max_decode_cap`:0/0, `none`:698/34, `code`:1325/35, `contract`:177/6, `SOURCE_DATE_EPOCH`:0/0 | 1 |
| 0079 | Phase D — what the operator can see | IMPLEMENTED |  | `none`:698/34 | 0 |
| 0079 | 7. Disposition of the proposal, item by item | IMPLEMENTED |  | `agent`:96/6 | 1 |
| 0079 | Security amendment (2026-09-02) — corrections found reading the ADR ag | DEFERRED |  |  | 2 |
| 0080 | ADR-0080: The answer is long; the verified unit is short | SUPERSEDED |  |  | 2 |
| 0080 | 1. What was measured, and why it is not a compression problem | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0080 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0080 | 5. Invariants the tests must hold | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0080 | 6. Order of work | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0080 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0081 | ADR-0081: Long context — the input is a state chain | SUPERSEDED | ACTIVE(palw_prompt_ids_merkle=ForkActivation::always()) | `palw_prompt_ids_v1`:480/13, `palw_prompt_ids_merkle`:67/0, `palw_kv_checkpoint_opening_bytes_v1`:18/0 | 8 |
| 0081 | 1.1 Prefill is not decode, and the asymmetry is the whole problem | DESIGN-ONLY |  |  | 1 |
| 0081 | 3. Decisions | DEFERRED |  |  | 1 |
| 0081 | 8. What is deliberately not decided | DESIGN-ONLY |  |  | 1 |
| 0081 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0081 | Implementation note, 2026-09-06 — Decision 3 wired (`f7363db9`) | IMPLEMENTED |  | `validate_palw_v2`:364/39, `trace_format_version`:21/0, `PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS`:11/0, `prompt_token_ids_match_v1`:26/0, `PalwCourtVerdictProofV2::ArithmeticOpened`:19/0, `check_close_speaks_the_networks | 0 |
| 0082 | ADR-0082: The close is flat in the context — attention is refuted by d | SUPERSEDED |  |  | 1 |
| 0082 | 1.1 The refutation of ADR-0080/0081 stands, and it was correct | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | 1.2 The graph-v4 tile flattens the opening and not the close, and the  | IMPLEMENTED |  | `n_ctx`:1227/94, `_for_map_v1`:3/0 | 6 |
| 0082 | 1.4 What 5f landed, and what it did not | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | 1.6 What earns, what is chosen, what the lane carries | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | Part A — the court is flat in the context | DEFERRED |  |  | 10 |
| 0082 | Part B — the executor is flat in the context | IMPLEMENTED-BUT-UNVERIFIED |  | `PALW_BASE0_SPARSE_RETAIN_LEVEL_V1`:21/0 | 1 |
| 0082 | 6. Order of work | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | 7. Supersession | SUPERSEDED |  | `n_ctx`:1227/94 | 4 |
| 0082 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0082 | 10. Implementation record — 2026-09-03 / 2026-09-04 | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0082 | 10.1 The close: one carrier, and the arity is part of the number | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | 10.2 Decision 4 amended: a checkpoint at every position, and one route | ACTIVE (IMPLEMENTED) | ACTIVE(palw_kary_court=ForkActivation::always()) | `the_folds_retention_is_constant_and_the_alternative_is_quadratic`:1/0, `KCacheWrite`:40/0, `VCacheWrite`:36/3, `every_position_is_checkpointed`:11/0, `the_cache_write_route_is_refused_by_name_where_every_position_is_che | 1 |
| 0082 | 10.3 What the capture costs — the half Decision 4 asserted and did not | IMPLEMENTED |  | `n_ctx`:1227/94, `the_per_position_capture_touches_one_tile_a_position`:2/0 | 1 |
| 0082 | 10.4 The ladder, the clock and the arity, derived jointly | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0082 | 10.5 What a fused row costs with NO dissection court | ACTIVE (IMPLEMENTED) | ACTIVE(palw_kary_court=ForkActivation::always()) | `palw_kary_court`:116/8 | 2 |
| 0082 | 10.6 The audit of 2026-09-03, and what closed it | ACTIVE (IMPLEMENTED) | ACTIVE(palw_prompt_ids_merkle=ForkActivation::always()) | `palw_fp_decode_rules`:56/3, `palw_prompt_ids_merkle`:67/0 | 2 |
| 0082 | 10.7 Status, and what is open — by name | DORMANT (IMPLEMENTED) | DORMANT(palw_fp_decode_rules) | `a16_graph_v5_row_v1`:7/17, `n_ctx`:1227/94, `palw_kary_court`:116/8, `palw_context_ladder`:299/18, `palw_uncertified_weightless`:53/6, `palw_prompt_ids_merkle`:67/0, `palw_fp_decode_rules`:56/3, `MODEL_ID`:39/0, `DEFAUL | 3 |
| 0083 | ADR-0083: The difficulty window counts only rows priced by `bits` — he | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0083 | 4. Decision 1 — count only bits-priced rows; an empty count answers MA | IMPLEMENTED |  | `None`:0/0, `consensus_params_id`:297/13, `for_each_fence`:52/0, `algo_id_is_priced_by_bits`:10/0, `kaspa_pow::State::new`:0/0, `max_difficulty_target`:39/0, `bits`:813/29, `heartbeat_rows_no_longer_tighten_the_bits_past | 2 |
| 0083 | 6. What this does not decide | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0084 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0084 | 4. What this costs, stated before it is measured | DEFERRED |  |  | 1 |
| 0084 | 7. Implementation record (2026-09-04, `palw-adr0084-served-answer`, fr | IMPLEMENTED |  | `FPA1`:19/0, `PalwFpAnswerV1`:4/0, `palw_fp_committed_output_ids_decode_v1`:11/0, `FPC1`:26/0, `palw_fp_job_material_decode_v1`:11/0, `fp_output_root_v1`:9/0, `serve_material_or_answer`:2/0, `PALW_MATERIAL_MAX_BYTES`:31/ | 0 |
| 0084 | 7.2 What the same material says about the opening lane, measured | DEFERRED |  |  | 1 |
| 0084 | 7.3 Decision 7 on the devnet (run 5, 23:22 JST, `3c6ea28b`) | IMPLEMENTED-BUT-UNVERIFIED |  | `c24c3b9d`:0/0, `replay_licenses_v1`:10/0 | 0 |
| 0084 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0085 | 2. What a close actually needs, term by term | IMPLEMENTED |  | `kv_checkpoint`:25/1, `Base0FpCheckpointClaimV1`:9/0 | 1 |
| 0085 | 3. Decisions | IMPLEMENTED |  | `Base0FpIntervalOpeningV3`:9/0, `PalwStepTileLeafV1`:102/2 | 3 |
| 0085 | 5. Invariants the tests must hold | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0085 | 7. Implementation record (2026-09-04, `palw-adr0084-served-answer`) | IMPLEMENTED |  | `Base0FpIntervalOpeningV3`:9/0, `MSKFPIV3`:1/0, `base0_fp_interval_close_annex_v1`:7/0, `step_opening_from_range_v1`:6/1, `step_range_siblings_from_range_v1`:4/1, `base0_fp_replay_interval_tiles_v1`:2/0, `Base0FpInterval | 4 |
| 0085 | 9. Number hygiene | DEFERRED |  |  | 1 |
| 0086 | ADR-0086 — the opening carries the fold, not the leaves | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0086 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0086 | 5. Invariants the tests must hold | IMPLEMENTED-BUT-UNVERIFIED |  | `MSKFPIV4`:3/0, `Digests`:15/0, `Unverifiable`:78/0 | 3 |
| 0086 | 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`) | IMPLEMENTED |  | `Base0FpFoldRangeOpeningV1`:20/0, `FaultInRange`:27/0, `Base0FpBlockLeavesV1`:23/0, `cut_v1`:10/0, `folds_to_v1`:7/0, `name_the_leaf_v1`:4/0, `base0_fp_range_with_served_block_v1`:4/0, `retain_level`:133/0, `COURT_MAX_ST | 2 |
| 0087 | ADR-0087 — a position is bought from the curve and sold back to it | IMPLEMENTED |  | `Final`:530/35, `buyback_sompi`:29/0, `retired_units`:38/0 | 2 |
| 0087 | 0.1 How the LLM's added value reaches a position, and there is no seco | DESIGN-ONLY |  |  | 2 |
| 0087 | 1. What exists, and what a market can therefore see | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0087 | 3. Decisions | SUPERSEDED |  | `Active`:560/43, `closed_to_buys`:24/0 | 2 |
| 0087 | 4. What this costs, stated before it is measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0087 | 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`) | ACTIVE (IMPLEMENTED) | ACTIVE(palw_model_market=ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA)) | `PalwModelMarketV1`:47/0, `palw_model_buy_quote_v1`:15/0, `palw_model_sell_quote_v1`:17/0, `model_markets`:24/0, `model_positions`:22/0, `PalwStateCarriageV2Legacy`:7/0, `Active`:560/43, `pending_payouts`:46/0, `sink_ind | 3 |
| 0088 | ADR-0088 — the class keeps its graph; a line keeps its owner, and the  | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0088 | 1. What exists, and the wall this ADR goes through | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0088 | 2. The requirement, and the shape that answers it | IMPLEMENTED-BUT-UNVERIFIED |  | `ModelVersionPublished`:13/0 | 1 |
| 0088 | 3. Decisions | SUPERSEDED |  |  | 8 |
| 0088 | 4. What this costs, stated before it is measured | SUPERSEDED |  |  | 1 |
| 0088 | 6. Invariants the tests must hold | SUPERSEDED |  | `until_daa`:21/0 | 4 |
| 0088 | 7. Order of work | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0088 | 8. Implementation record (2026-09-05, `palw-adr0088-0089-impl`) | ACTIVE (IMPLEMENTED) | ACTIVE(palw_model_market=ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA)) | `bf4ed1d8`:0/0, `PalwModelLineV1`:18/0, `PalwModelVersionV1`:17/0, `PalwModelProposalV1`:10/0, `PalwModelEvaluationV1`:9/0, `founding_line_v1`:6/0, `model_line_id_v1`:9/0, `model_proposal_id_v1`:7/0, `split_owner_leg_v1` | 0 |
| 0088 | 9. What the first draft decided, and why it was withdrawn | SUPERSEDED |  |  | 1 |
| 0088 | 11. Number hygiene | SUPERSEDED |  |  | 1 |
| 0089 | 2. What exists here | IMPLEMENTED |  | `calculate_utxo_state_relatively`:15/4, `calculate_utxo_state`:19/3 | 1 |
| 0089 | 4. Decisions | IMPLEMENTED-BUT-UNVERIFIED |  | `closed_to_buys`:24/0 | 4 |
| 0089 | 6. Security — the four principles, checked before it is built | DEFERRED |  |  | 1 |
| 0089 | 9. Implementation record (2026-09-05, `palw-adr0088-0089-impl`) | ACTIVE (IMPLEMENTED) | ACTIVE(palw_model_market=ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA)) | `bf4ed1d8`:0/0, `system_address`:5/0, `facade_address_v1`:6/0, `PalwEvmMarketActionV1`:12/0, `PalwEvmSettlementV1`:16/0, `PalwEvmMarketFencesV1`:9/0, `PalwEvmViewV1`:7/0, `synthetic_market_sink_txid`:7/0, `evm_settlement | 2 |
| 0090 | ADR-0090 — The pair is seeded with real MSK, locked for good, and a po | IMPLEMENTED |  | `Final`:530/35 | 1 |
| 0090 | 3. Decisions | DEFERRED |  |  | 1 |
| 0090 | 5. Security — the four principles, checked | SUPERSEDED |  |  | 1 |
| 0090 | 8. Implementation record (2026-09-05, `palw-adr0088-0089-impl`) | IMPLEMENTED |  | `seed_v1`:7/0, `ModelSeed`:22/0, `model_seed_v1`:3/0, `Seed`:37/0, `action`:185/0, `send_action_seed_calldata`:2/0, `carries_escrow`:2/0, `Seeded`:6/1, `SeedTooSmall`:0/0, `seedSompi`:1/0, `seededBy`:1/0, `seedMinSompi`: | 0 |
| 0090 | Item 6 executed, 2026-09-06 — the three fences are scheduled on testne | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0091 | ADR-0091 — The reward buys the pair, and no holder is paid | DESIGN-ONLY |  |  | 1 |
| 0091 | 3. Decisions | SUPERSEDED |  | `Final`:530/35 | 9 |
| 0091 | 4. The arithmetic, worked | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0091 | 6. Invariants the tests must hold | IMPLEMENTED-BUT-UNVERIFIED |  | `retired_units`:38/0 | 1 |
| 0091 | 8. Implementation record (2026-09-06, `palw-adr0088-0089-impl`) | IMPLEMENTED |  | `buyback_sompi`:29/0, `retired_units`:38/0, `palw_model_buyback_slice_v1`:5/0, `palw_model_buyback_quote_v1`:12/0, `None`:0/0, `the_reward_slice_is_worked_as_the_adr_table`:1/0, `the_product_never_falls_under_the_reward_ | 7 |
| 0092 | 0. The sentence this ADR is | DEFERRED |  |  | 1 |
| 0092 | 3. The wall, and where it actually is | DEFERRED |  |  | 1 |
| 0092 | 4. Decisions | IMPLEMENTED |  | `verify_class_admission_v5`:22/4 | 4 |
| 0092 | 7. What is deliberately not decided | SUPERSEDED |  |  | 2 |
| 0092 | 8. Implementation record (2026-09-06) | ACTIVE (IMPLEMENTED) | ACTIVE(palw_court_ladder=ForkActivation::new(PALW_RC_COURT_LADDER_FENCE_DAA)) | `window_court`:222/13, `max_step_leaf_count`:477/27, `palw_attn_court_admits_row_v1`:23/3, `the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context`:0/1, `palw_court_arity_v1`:32/0, `palw_attn | 0 |
| 0093 | 9. As built (2026-09-11) | IMPLEMENTED-BUT-UNVERIFIED |  | `attn_tile_claim`:1/0, `supports_dissection`:12/0 | 1 |
| 0093 | 10. Amended 2026-09-11: Decisions 6 and 7 | IMPLEMENTED-BUT-UNVERIFIED |  | `the_anchored_root_claim_opens_past_its_fence_and_the_plain_one_is_refused_there`:1/0 | 2 |
| 0094 | 3. Decisions | IMPLEMENTED-BUT-UNVERIFIED |  | `msk_seed`:28/0 | 1 |
| 0094 | 5. Security — the four principles, checked | SUPERSEDED |  | `seeded_by`:24/0 | 2 |
| 0094 | 6. Invariants the tests must hold | IMPLEMENTED-BUT-UNVERIFIED |  | `seeded_by`:24/0 | 1 |
| 0094 | 8. Implementation record | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0095 | 4.4 The lead is a consensus rule, not a promise — **this is the revisi | DEFERRED |  |  | 1 |
| 0095 | 5. Security — the four principles, checked | IMPLEMENTED |  | `tiers`:162/9 | 1 |
| 0096 | 1.1 The entrance refuses what a stock client sends | IMPLEMENTED |  | `Params::palw_fp_decode_rules`:56/3, `sampling_from_request`:0/0 | 1 |
| 0096 | 1.3 Four tables say what to install, and one of them names a binary th | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0096 | 1.7 The shape of the answer is nobody's | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0096 | Part A — the entrance (host-side; no consensus object moves) | DEFERRED |  |  | 3 |
| 0096 | Part B — the shape of the answer (a ruleset move, behind one fence) | IMPLEMENTED-BUT-UNVERIFIED |  | `render_answer_v1`:17/0, `constraint_id`:10/0 | 4 |
| 0096 | Part C — distribution, settings, the door | IMPLEMENTED |  | `effective`:71/2 | 1 |
| 0096 | 4. What this costs, stated before it is measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0096 | 5. Invariants the tests must hold | IMPLEMENTED-BUT-UNVERIFIED |  | `response_format`:4/0 | 2 |
| 0096 | 6. Order of work | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0096 | 7. Supersession | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0096 | 8. Number hygiene | DEFERRED |  |  | 1 |
| 0096 | 9. Implementation record | IMPLEMENTED |  | `e65ccf20`:0/0, `b62e89c2`:0/0, `Params::palw_fp_decode_constraint`:34/0, `None`:0/0, `validate_palw_v2`:364/39, `GetPalwProducerFactsResponse`:29/0, `fp_decode_constraint_armed`:17/0, `render_answer_v2`:1/0, `kaspad`:0/ | 9 |
| 0097 | ADR-0097 — A model's fit is a lookup, and the entrance says its limits | DESIGN-ONLY |  |  | 1 |
| 0097 | 1.1 The walls, and where each ceiling lives | DORMANT (IMPLEMENTED) | DORMANT(palw_context_ladder) | `palw_context_ladder`:299/18, `Flat`:330/13 | 1 |
| 0097 | 1.2 The rows the chain carries, and the widest context each wall admit | DESIGN-ONLY |  |  | 1 |
| 0097 | 1.5 What a seat must hold | DESIGN-ONLY |  |  | 1 |
| 0097 | 4. What this costs | DESIGN-ONLY |  |  | 1 |
| 0097 | 7. Supersession | DESIGN-ONLY |  |  | 1 |
| 0097 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `main`:0/0, `KIMI_K3_AS_HYBRID_V1`:13/7, `limits_body`:0/0, `refusal_body`:0/0, `context_refusal`:0/0, `prompt_bytes_refusal`:0/0, `error_body`:2/0, `Lane::send`:287/1, `surface::error_body`:2/0, `n_ctx`:1227/94, `max_ou | 1 |
| 0097 | Appendix A — the sixty-item checklist against this tree | DEFERRED |  |  | 3 |
| 0098 | ADR-0098 — The panel's coverage is a number, and a seat that found a l | DEFERRED |  |  | 1 |
| 0098 | 3. Decisions | DEFERRED |  |  | 3 |
| 0098 | 5. Invariants the tests hold | DEFERRED |  |  | 1 |
| 0098 | 9. Number hygiene and implementation record | DEFERRED |  |  | 1 |
| 0099 | ADR-0099 — The adder measures, the chain recomputes, and a seat holds  | DEFERRED |  |  | 1 |
| 0099 | 5. Invariants the tests hold | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0099 | 9. Number hygiene and implementation record | DEFERRED |  |  | 2 |
| 0100 | ADR-0100 — A model is data: the one-move court, the held measurement a | DEFERRED |  |  | 2 |
| 0100 | 1.1 The one-move court is a consensus object | ACTIVE (IMPLEMENTED-BUT-UNVERIFIED) | ACTIVE(palw_signature_contexts_v2=ForkActivation::always()) | `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`:13/0, `palw_signature_contexts_v2`:46/0 | 3 |
| 0100 | 1.3 The held measurement, on the real artifact | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0100 | 3. Decisions | ACTIVE (IMPLEMENTED) | ACTIVE(palw_shard_court=ForkActivation::always()) | `validate_palw_v2`:364/39, `palw_shard_court`:76/5, `ReceiptLicensed`:228/17, `ProducerDefaulted`:51/2 | 5 |
| 0100 | 4. What this costs | IMPLEMENTED |  | `signature_contexts_root`:44/1 | 1 |
| 0100 | 5. Invariants the tests hold | DEFERRED |  |  | 6 |
| 0100 | 6. Order of work | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0100 | 7. Supersession | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0100 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `main`:0/0, `ModelLineBenefitsDeclared`:14/0, `ShardCourtAccused`:36/1, `the_one_move_courts_borsh_tag_is_the_last_and_pinned`:1/0, `shard_court_ladder`:9/0, `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`:13/0, `shared`:361/19 | 1 |
| 0101 | ADR-0101 — A membership is proven by the chain and served by anyone, a | DEFERRED |  |  | 1 |
| 0101 | 0. The sentence this ADR is | DEFERRED |  |  | 1 |
| 0101 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `palw_derived_v1`:72/2 | 0 |
| 0102 | ADR-0102 — The embedding lift is read per token, and a kernel a networ | IMPLEMENTED-BUT-UNVERIFIED |  | `court_catalog_root`:31/0 | 3 |
| 0102 | 0. The sentence this ADR is | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0102 | 1.2 Why: three readings of one store | IMPLEMENTED-BUT-UNVERIFIED |  | `KDESC_A16_REQUANTIZE`:56/0, `Hidden`:87/0 | 1 |
| 0102 | 1.3 The flag day this does not take | IMPLEMENTED-BUT-UNVERIFIED |  | `court_catalog_root`:31/0 | 1 |
| 0102 | 1.4 The held measurement under graph-v6, on the real artifact | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0102 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0102 | 5. Invariants the tests hold | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0102 | 6. Order of work | DESIGN-ONLY |  |  | 1 |
| 0102 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `KDESC_A16_REQUANTIZE_BY_TOKEN`:14/1, `Qwen36Op::RequantizeByToken`:4/0, `KERNEL_CATALOG_FENCED_V1`:3/0, `fenced_kernel_ids_v1`:6/0, `QWEN36_PRE_IR_V6`:3/0, `qwen36_profile_v6`:12/0, `qwen36_artifact_row_profile_v6`:9/1, | 0 |
| 0103 | 1.1 Six terms, and the order each one grows with the context | IMPLEMENTED |  | `tiled_kv_state_geometry_v3`:25/0, `PALW_STEP_LEG_MAX_STATE_CHUNKS`:33/1 | 1 |
| 0103 | 1.2 The same six at 2M, in the tree's own predicates | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0103 | 1.3 What the tree already holds for each term | IMPLEMENTED |  | `ShardCourtAccused`:36/1, `void_and_slash`:28/1, `NeedsDissection`:9/0 | 1 |
| 0103 | 3. Decisions | SUPERSEDED |  | `c9be7676`:0/0 | 4 |
| 0103 | 4. What this costs, stated before it is measured | IMPLEMENTED |  | `CheckpointAccused`:13/1 | 1 |
| 0103 | 7. Supersession | SUPERSEDED |  |  | 2 |
| 0103 | 9. Number hygiene | DEFERRED |  |  | 2 |
| 0103 | 10. Implementation record (2026-09-11) | IMPLEMENTED |  | `c9be7676`:0/0, `c204bf0f`:0/0, `None`:0/0, `shipped_presets_have_pinned_fingerprints`:19/0, `palw_held_context_mint_v1`:11/6 | 0 |
| 0103 | 10.1 What each Decision became | IMPLEMENTED |  | `CheckpointAccused`:13/1, `CourtAttnRootClaimedAnchored`:23/0, `CourtOpened`:55/0, `PalwConsensusParamsV2::validate`:362/23, `Params::validate_palw_v2`:364/39, `PalwShardCourtAccusationV1::prompt_ids_opening`:46/1, `palw | 1 |
| 0103 | 10.3 Corrections to this ADR, found while building it | DEFERRED |  |  | 3 |
| 0103 | 10.6 Decision 2's price, amended (2026-09-11): a replay pays for its h | DORMANT (IMPLEMENTED) | DORMANT(palw_context_ladder) | `u64::MAX`:0/0, `Recompute`:26/1, `palw_held_interval_positions_v1`:18/2, `palw_shard_resume_ms_v1`:4/0, `window_receipt`:103/6, `document_id`:27/1, `resume_bytes`:6/0, `palw_context_ladder`:299/18, `palw_court_deadline` | 0 |
| 0105 | 4. The candidates | DEFERRED |  |  | 5 |
| 0105 | 5.1 Decision 1 (consensus, fenced) — a heartbeat never turns a bonded  | ACTIVE (IMPLEMENTED) | ACTIVE(palw_model_benefits=ForkActivation::new(PALW_RC_MODEL_BENEFITS_FENCE_DAA)) | `Params::palw_heartbeat_transparent`:33/7, `LaneColoring`:14/0, `check_bounded_merge_depth`:5/1, `merge_depth`:36/2, `ViolatingBoundedMergeDepth`:2/1, `palw_lane_blue_work_v1`:7/0, `GhostdagManager::with_level`:6/0, `pal | 10 |
| 0105 | 5.2 Decision 2 (node policy, ships now) — the heartbeat miner steps as | IMPLEMENTED |  | `palw_heartbeat_v1::heartbeat_yield_hint_v1`:11/0, `Option`:0/0, `None`:0/0, `BondedSelectedParent`:8/2, `NothingToYieldTo`:11/1, `YieldUntil`:15/1 | 0 |
| 0105 | 5.3 Decision 3 (operators) | IMPLEMENTED |  | `getDnsConfirmation`:8/2, `workDepth`:2/1, `requiredWorkDepth`:2/1 | 0 |
| 0105 | 6. Safety and liveness, checked against the doctrine | DEFERRED |  |  | 1 |
| 0105 | 8. Invariants the tests hold | DEFERRED |  |  | 4 |
| 0105 | 10. Number hygiene and implementation record | IMPLEMENTED |  | `main`:0/0, `ea2fd48e`:0/0, `validate_palw_v2`:364/39, `HeartbeatYieldHintV1`:37/5, `heartbeat_yield_hint_v1`:11/0, `LaneColoring`:14/0, `HeartbeatTransparency`:11/0, `heartbeat_yield_hint`:5/4, `ConsensusApi`:67/14, `Co | 1 |
| 0106 | ADR-0106 — An inventory is a stream of leaves, not a copy of the model | DEFERRED |  |  | 1 |
| 0106 | 1.4 The fixture record | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0106 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `b805fc3d`:0/0, `main`:0/0, `bd280d15`:0/0, `artifact_leaf_parts_v1`:11/0, `PalwArtifactLeafHasherV1`:5/0, `PalwArtifactMerkleFrontierV1`:5/0, `PalwInventoryLayoutCheckerV1`:4/0, `PalwArtifactRowDigestV1`:20/0, `PalwArti | 1 |
| 0107 | 2. Decision | DEFERRED |  |  | 2 |
| 0107 | 5. Activation | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0107 | 6. Tests | IMPLEMENTED |  | `BindTimeout`:75/2 | 3 |
| 0108 | ADR-0108 — An extension is a manifest the verifier recomputes, and a r | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0108 | 1. What exists, measured on `main` `40ac431b` (2026-09-11) | IMPLEMENTED |  | `consensus_identity_id`:268/1 | 1 |
| 0108 | 2. The boundary, stated as three tiers | DESIGN-ONLY |  |  | 1 |
| 0108 | Decision 1 — One manifest, canonical, with a derived identity | IMPLEMENTED |  | `PalwExtensionManifestV1`:7/0, `misaka_palw_derive::canon_json::canonicalize_json`:2/0, `transformer_id_v1`:12/0, `consensus_params_id`:297/13 | 1 |
| 0108 | Decision 2 — Every answer names its tier | IMPLEMENTED |  | `verify_extension_v1`:4/15, `PalwExtensionReportV1`:13/3, `classification`:39/42, `PASS`:54/0, `NodeExtension`:5/4, `Expressible`:17/20, `Refused`:236/11 | 0 |
| 0108 | Decision 3 — Verification has three depths, and the report says which  | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0108 | Decision 4 — A receipt is evidence of reproduction, signed by whoever  | ACTIVE (IMPLEMENTED) | ACTIVE(palw_signature_contexts_v2=ForkActivation::always()) | `PalwExtensionReceiptV1`:7/3, `extension_id`:21/4, `ruleset_id`:24/2, `signature_contexts_root`:44/1, `palw_signature_contexts_v2`:46/0, `kaspad`:0/0, `Expressible`:17/20 | 1 |
| 0108 | Decision 5 — Preflight and submit go through the objects that already  | IMPLEMENTED |  | `preflight`:52/1, `verify`:645/55, `Params`:933/38, `palw_admission_shape_at_v1`:29/20, `submit`:268/9, `ClassRegistered`:207/44, `PalwClassSdk::build_post_genesis_registration`:6/0, `FamilyCertified`:93/36, `ClassLaneCe | 1 |
| 0108 | Decision 6 — A ruleset candidate is described and costed; it is never  | IMPLEMENTED |  | `Params`:933/38, `consensus_params_id`:297/13, `consensus_identity_id`:268/1, `consensus_schedule_id`:86/1, `validate_palw_v2`:364/39 | 4 |
| 0108 | Decision 7 — A context width is a class inside the ladder and a rulese | IMPLEMENTED |  | `n_ctx`:1227/94 | 0 |
| 0108 | Decision 8 — The chain-class seal stays; the manifest tells the operat | IMPLEMENTED |  | `resolve_chain_registered`:24/2, `Expressible`:17/20 | 0 |
| 0108 | Decision 9 — Publication and discovery are off-chain, and the id is wh | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0108 | 7. Invariants the tests hold | DEFERRED |  |  | 1 |
| 0108 | 8. What is deliberately not decided, and what is not verified | IMPLEMENTED-BUT-UNVERIFIED |  | `getPalwRegistrationTerms`:7/0 | 1 |
| 0108 | 9. Number hygiene and implementation record | IMPLEMENTED |  | `main`:0/0, `Debug`:2081/4, `palw_fences_v1`:40/0, `preflight_admission`:13/0, `kaspad`:0/0, `consensus`:2101/857, `n_ctx`:1227/94, `inspect`:32/0, `verify`:645/55, `submit`:268/9, `ObjectChunk`:59/14, `ClassLaneCertifie | 4 |
| 0109 | Decision 1 — Every accepted lock is claimed by every producer, unasked | IMPLEMENTED |  | `EVM_DEPOSIT_LOCK`:41/6, `commit_virtual_state`:5/0, `prepare_deposit_claims`:7/0 | 0 |
| 0109 | Decision 2 — Finality is a label the reader asks for, not a pause the  | IMPLEMENTED |  | `Config::evm_bridge_finality`:15/2, `Label`:16/4, `Pause`:10/4, `submitEvmDepositClaim`:4/1, `safe`:381/18, `latest`:159/22 | 0 |
| 0109 | Decision 3 — A spend the chain will refuse is refused at the mempool | IMPLEMENTED-BUT-UNVERIFIED |  | `validate_mempool_transaction`:11/0, `SpendsNonReleasableBond`:3/0, `submitTransaction`:3/0 | 0 |
| 0109 | Decision 4 — `safe` is the DNS-confirmed anchor | IMPLEMENTED |  | `latest`:159/22, `finalized`:238/5, `safe`:381/18 | 0 |
| 0109 | Decision 5 — The claim RPC and the relay lane stay, as accelerators | IMPLEMENTED |  | `submitEvmDepositClaim`:4/1 | 0 |
| 0109 | 7. What is deliberately not decided | DEFERRED |  |  | 1 |
| 0109 | 8. Number hygiene and implementation record | DESIGN-ONLY |  | `main`:0/0 | 0 |
| 0110 | 2. Decisions | DEFERRED |  |  | 1 |
| 0110 | 4. The CLI | DEFERRED |  |  | 1 |
| 0110 | 9. Implementation record (2026-09-11) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0110 | 9.5 How the 2M vector runs (2026-09-11) | DEFERRED |  |  | 1 |
| 0111 | 4. Invariants the tests must hold | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0111 | 7. Number hygiene | DEFERRED |  |  | 1 |
| 0111 | 8. Implementation record (2026-09-11) | IMPLEMENTED-BUT-UNVERIFIED |  | `e251c751`:0/0, `None`:0/0, `shipped_presets_have_pinned_fingerprints`:19/0 | 0 |
| 0111 | 8.2 What the live drill found (both fixed, both pinned) | IMPLEMENTED-BUT-UNVERIFIED |  | `FaultInRange`:27/0 | 1 |
| 0111 | 8.3 The drill runs | IMPLEMENTED |  | `ShardCourtAccused`:36/1, `PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`:3/0, `DefaultAccusedHeld`:14/0, `MaterialDisclosedHeld`:15/1, `court_fraud`:17/0 | 2 |
| 0112 | 1. What was measured | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0112 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0112 | 7. Supersession | SUPERSEDED |  |  | 1 |
| 0112 | 8. What is deliberately not decided | DEFERRED |  |  | 1 |
| 0112 | 10. Implementation record (2026-09-11) | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0114 | 1. Decisions | IMPLEMENTED-BUT-UNVERIFIED |  | `PALW_MODEL_OWNER_LEG_PERMILLE_V2`:2/0 | 1 |
| 0114 | 2. The numbers (pinned in `palw_model_market_v1.rs`) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0114 | 5. Arming it on testnet-11 — done at DAA 3,500 (2026-09-11) | DEFERRED |  |  | 1 |
| 0115 | 2. Decisions | DEFERRED |  |  | 1 |
| 0116 | 7. What is deliberately not decided | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0116 | 9. Implementation record (2026-09-11) | IMPLEMENTED |  | `palw_state_chunk_map::palw_attn_history_bound_v1`:14/0, `a16_attn_values_within`:12/0, `a16_attn_fused_reference_within_v1`:5/0, `palw_step_refute::qwen36_row`:2/0, `AttnFused`:96/2, `AttnValues`:11/0, `A16ProfilePlanV1 | 0 |
| 0117 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0117 | 5. Invariants the tests hold | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0117 | 9. Implementation record (2026-09-11) | IMPLEMENTED-BUT-UNVERIFIED |  | `Params::palw_prefill_draw`:41/0, `palw_attempt_v2::palw_attempt_job_v1`:28/0, `palw_producer::produce_one`:6/0, `palw_panel::attempt_job_for_claim`:7/0, `Qwen36Engine::forward_prefill_planned`:13/0, `qwen36_execute_stre | 0 |
| 0117 | 9.1 The dense tier's one pass (2026-09-12, at the operator's instructi | IMPLEMENTED-BUT-UNVERIFIED |  | `the_one_pass_prefill_is_the_position_by_position_one`:4/0 | 1 |
| 0118 | ADR-0118 — The held regime arrives at a height, and a held class carri | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0118 | 1. What was found | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0118 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0118 | 6. Supersession | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0118 | 7. What is deliberately not decided | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0118 | 8. Number hygiene | DEFERRED |  |  | 1 |
| 0118 | 9. Implementation record (2026-09-12) | DORMANT (IMPLEMENTED) | DORMANT(palw_share_growth_final) | `Params::validate_palw_v2`:364/39, `Params::consensus_params_id`:297/13, `palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1`:14/3, `palw_class_admission_v2`:206/16, `palw_model_fit_v1::palw_model_fit_v2`:9/4, `palw_co | 0 |
| 0119 | 1. What was found | DORMANT (IMPLEMENTED-BUT-UNVERIFIED) | DORMANT(palw_fp_ruleset_caps) | `palw_fp_ruleset_caps`:30/0 | 1 |
| 0119 | 3. Decisions | DEFERRED |  |  | 2 |
| 0120 | (no decision/keyword sections) | — |  |  | 0 |
| 0121 | D4 — What is refused rather than built | DEFERRED |  |  | 1 |
| 0122 | 1. What an operator does today | DESIGN-ONLY |  | `kaspad`:0/0 | 1 |
| 0122 | 3.4 Transitions | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0122 | 6.2 `misaka mining stop`: claims are defended until they end | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0122 | 6.4 What `status` and `rewards` read (existing reads first) | IMPLEMENTED |  | `getServerInfo`:16/0, `getBlockDagInfo`:2/0, `getConnectedPeerInfo`:1/0, `not_ready_code`:0/0, `getPalwNodeStatus`:12/0, `getPalwFreePromptClaim`:7/0, `getPalwClasses`:14/0, `getUtxosByAddresses`:3/0, `getPalwClaims`:14/ | 0 |
| 0122 | 11. Screens | DEFERRED |  |  | 1 |
| 0122 | 14. Alternatives not taken | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0122 | 15. Open questions | IMPLEMENTED-BUT-UNVERIFIED |  | `getPalwClasses`:14/0, `DuplicateBondKey`:13/0 | 0 |
| 0122 | 16. Implementation notes (P1–P6, 2026-09-12) | DEFERRED |  |  | 1 |
| 0123 | ADR-0123 — The epoch progressively releases unused class budget | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0123 | 2. Decision | DEFERRED |  |  | 1 |
| 0123 | 3. Consequences | DESIGN-ONLY |  |  | 1 |
| 0124 | ADR-0124 — The panel is paid out of the claim's reward, a seat holds e | IMPLEMENTED |  | `Final`:530/35 | 3 |
| 0124 | 3. Decisions | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0124 | 9. What is deliberately not decided | IMPLEMENTED-BUT-UNVERIFIED |  | `panel_reserve_sompi`:30/0 | 3 |
| 0124 | 10. Implementation record (2026-09-17, `feat/adr-0124-panel-reward-and | IMPLEMENTED |  | `palw_panel_split_v1`:15/0, `palw_work_priced_reward_v1`:12/0, `palw_seat_exposure_v1`:9/0, `palw_panel_collateral_floor_v1`:10/0, `palw_seat_has_headroom_v1`:12/0, `PalwSeatEconomyV1`:12/0, `panel_duties`:30/0, `panel_r | 1 |
| 0124 | 12. Corrections | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0125 | 1. What the operator asked, in the operator's words | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0125 | 3. Decisions | IMPLEMENTED |  | `palw_execution_credit_v1`:10/0, `permits_per_round`:51/3, `widenings`:46/4 | 3 |
| 0125 | 4. What is built | DESIGN-ONLY |  |  | 1 |
| 0125 | 8. Corrections | DEFERRED |  |  | 1 |
| 0125 | 9. The drill, run (2026-09-17) | IMPLEMENTED |  | `Final`:530/35 | 2 |
| 0126 | ADR-0126 — The validator carve drops to a fifth, and the stake reorg g | DESIGN-ONLY |  |  | 3 |
| 0126 | 1. Why the overlay stays | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0126 | 2. Decisions | IMPLEMENTED |  | `validate_palw_v2`:364/39, `dns_params`:287/75 | 1 |
| 0126 | 4. What was deleted on 2026-09-17, and what came back | DEFERRED |  |  | 1 |
| 0126 | 8. Corrections | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0127 | ADR-0127 — PALW settles on its own, and its terms are not DNS terms | DEFERRED |  |  | 2 |
| 0127 | 0. The sentence this ADR is | IMPLEMENTED |  | `Final`:530/35 | 1 |
| 0127 | 1. The names, kept apart | DEFERRED |  |  | 1 |
| 0127 | 2. Decisions | DEFERRED |  |  | 2 |
| 0127 | 4. Where the separation is not yet complete (named, not hidden) | IMPLEMENTED |  | `palw_block_commitment`:46/2 | 2 |
| 0128 | ADR-0128 — DNS validators vote BFT by bonded stake, and that vote deci | DESIGN-ONLY |  |  | 1 |
| 0128 | 1. What exists | IMPLEMENTED |  | `dns_reorg_outcome`:8/7 | 2 |
| 0128 | 2. Decisions | IMPLEMENTED-BUT-UNVERIFIED |  | `ConfirmedAnchorStale`:9/0 | 4 |
| 0128 | 4. Security amendments | DEFERRED |  |  | 1 |
| 0128 | 8. Implementation record (2026-09-17) | ACTIVE (IMPLEMENTED) | ACTIVE(dns_bft_gate=DnsBftGateV1 { activation: flag_day_6001, t_leak_daa: 5_040,) | `lock_consistent_precommits`:9/0, `held_precommit_lock`:4/0, `f0cade50`:0/0, `DnsBftRuntime`:3/0, `update_dns_state`:18/2, `dns_reorg_outcome`:8/7, `Consensus::get_precommit_duty`:4/2, `pick_precommit_due`:3/0, `precommi | 7 |
| 0129 | ADR-0129 — A double spend needs the anchors, not the blocks | DESIGN-ONLY |  |  | 1 |
| 0129 | 2. Decisions | DEFERRED |  |  | 1 |
| 0129 | 4. Security amendments | DEFERRED |  |  | 1 |
| 0130 | ADR-0130 — BPS 1 is hardened before it is widened | DEFERRED |  |  | 2 |
| 0130 | 1. What exists, and where it fails at width 1 | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0130 | 2. Decisions | DEFERRED |  |  | 4 |
| 0130 | 3. testnet-11's capital, measured (2026-09-17, DAA 5,774) | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0130 | 5. Deferred, by name | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0130 | 7. Implementation record (2026-09-17) | ACTIVE (IMPLEMENTED) | ACTIVE(palw_panel_economy=flag_day_6001) | `fc4325d9`:0/0, `da837d62`:0/0, `ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2`:1/0, `Some`:0/0, `None`:0/0, `adr0130_the_seat_exposure_floor_is_dormant_everywhere_and_refused_where_it_cannot_floor`:1/ | 5 |
| 0131 | 1. What testnet-11 pays today (read from its genesis objects and live  | IMPLEMENTED |  | `Final`:530/35 | 4 |
| 0131 | 7. Implementation record and measurements (2026-09-17) | IMPLEMENTED |  | `PALW_ECONOMIC_COST_TABLE_V1`:11/0, `u128`:0/0, `palw_class_census_v1`:6/0, `getPalwClassEconomics`:6/0, `palw_expected_attempts_v1`:26/0, `palw_expected_attempts_q32_v1`:21/0, `palw_attempted_compute_q32_per_claim_v1`:1 | 4 |
| 0131 | 3. Deferred | UNCLASSIFIED(no identifier) |  |  | 0 |
| 0131 | 4. Status | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0132 | 4. Proposals, compared | IMPLEMENTED-BUT-UNVERIFIED |  | `EconomicAttempted`:16/0, `Incapable`:59/0 | 4 |
| 0132 | 6. What is built (shadow, node-local) | IMPLEMENTED-BUT-UNVERIFIED |  | `EconomicAttempted`:16/0 | 2 |
| 0132 | 7. Implementation record | ACTIVE (IMPLEMENTED) | ACTIVE(palw_economic_payout=PalwEconomicPayoutV1 {
        activation: flag_day_6001,
  ) | `palw_economic_compute_v1`:27/0, `bits`:813/29, `palw_economics_ledger_v1`:11/0, `Final`:530/35, `kaspad::palw_economics`:10/0, `rpc_tests::sanity_test`:0/0, `shipped_presets_have_pinned_fingerprints`:19/0, `None`:0/0, ` | 1 |
| 0132 | 7.6 The short challenge window, made a fence (2026-09-18) | IMPLEMENTED-BUT-UNVERIFIED |  | `PalwStateParamsV2::short_challenge_window_from_daa`:10/0, `Params::set_palw_short_challenge_window`:11/0 | 2 |
| 0133 | 3. Decisions | SUPERSEDED |  | `PalwVerificationProfileV1`:10/0 | 3 |
| 0133 | 4. The artifact options, compared | DEFERRED |  |  | 1 |
| 0133 | 9a. The order of work (the operator's, 2026-09-17) | UNCLASSIFIED(no identifier) |  |  | 3 |
| 0133 | 10. What is built (shadow) | DEFERRED |  |  | 1 |
| 0133 | 11. Implementation record | IMPLEMENTED |  | `palw_verification_profile_v1`:19/0, `Final`:530/35, `rpc_tests::sanity_test`:0/0, `shipped_presets_have_pinned_fingerprints`:19/0 | 0 |
| 0134 | 3. Decisions | DEFERRED |  |  | 1 |
| 0134 | 6. Implementation record | IMPLEMENTED-BUT-UNVERIFIED |  | `dd805c9f2c4e9db3c0d6ffa2d87fa6ffb4263078ab8b7eb857f8fb11f8aa010c`:0/0, `the_shipped_schedules_are_measured_not_assumed`:3/0, `adr0134_the_compute_overlay_retires_at_its_own_height`:1/0, `adr0134_the_five_compute_subnetw | 0 |
| 0135 | ADR-0135 — A model is data: the permissionless registry derives its pr | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0135 | 2. Decisions | DEFERRED |  |  | 1 |
| 0135 | 4. What this changes in ADR-0132 and ADR-0133 | SUPERSEDED |  | `warm_p99`:1/0, `cold_p99`:1/0 | 1 |
| 0135 | 6. The road | ACTIVE (IMPLEMENTED-BUT-UNVERIFIED) | ACTIVE(palw_economic_payout=PalwEconomicPayoutV1 {
        activation: flag_day_6001,
  ) | `palw_economic_payout`:65/0 | 1 |
| 0135 | 7. What is built — Protocol Upgrade A, behind a dormant fence (2026-09 | IMPLEMENTED |  | `priced_share_permille`:18/1, `PREFETCHING`:7/0, `REGISTERED`:44/5, `artifact_bytes`:159/7 | 14 |
| 0136 | 2. Decisions | DEFERRED |  |  | 1 |
| 0136 | 3. Results | UNCLASSIFIED(no identifier) |  |  | 2 |
| 0136 | 4. What this ADR does not build, and the final form | DEFERRED |  |  | 1 |
| 0137 | ADR-0137 — A block buys one unit of work from any model, and a share i | DEFERRED |  |  | 2 |
| 0137 | 1. Where share sits today — the formulas as shipped | IMPLEMENTED |  | `min_grantable_share_permille`:44/6 | 1 |
| 0137 | 2. Why the ticket was 3.589 × 10⁻³ | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0137 | 3.6 The dormant usage loop | DORMANT (IMPLEMENTED) | DORMANT(palw_share_growth_final) | `derive_class_share_growth_v1`:9/1, `palw_share_growth_final`:43/1 | 1 |
| 0137 | 4. Permissionless registration under the shipped rule — the floor-shar | DEFERRED |  |  | 1 |
| 0137 | 7. The design — one work target | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0137 | 15. Migration — at a height, nothing regenerated | UNCLASSIFIED(no identifier) |  |  | 1 |
| 0137 | 17. The smallest implementation, in order | DEFERRED |  |  | 1 |
| 0137 | 18. What becomes legacy | DORMANT (IMPLEMENTED) | DORMANT(palw_share_growth_final) | `palw_share_growth_final`:43/1, `admission_permille`:8/0, `palw_class_shares_from_admission_v1`:4/0, `priced_share_permille`:18/1, `cap_utilization_permille`:27/1, `bits`:813/29 | 6 |
| 0137 | 22.1 The dormant fence, built (2026-09-18, §17 step 2 and 3 together) | IMPLEMENTED |  | `never`:4405/170 | 1 |
| 0137 | 21. Number hygiene | DEFERRED |  |  | 1 |

## Named in an ADR, absent from the code (constructor / responder / operator path candidates)

- ADR-0002 · Implementation notes for Phase 4 · `Version::PubKeyHashMlDsa65`
- ADR-0009 · A.6 Revised implementation order (supersedes the PR table ab · `RuleError::DnsFinalityReorgRejected`
- ADR-0012 · Phase 13 PR plan (this ADR's slot) · `SortitionMode::CommitReveal`
- ADR-0020 · Status · `u64::MAX`
- ADR-0023 · Phased activation (§18 — Phase 0 is the gate) · `PqEvmTransactionV1`
- ADR-0024 · Activation · `u64::MAX`
- ADR-0034 · 2. Draft vocabulary → this fork's identifiers · `VerifierCapabilityV1`
- ADR-0038 · Implementation status — Decisions A–D and H, 2026-08-19 · `sign_palw_block_commitment_v1`
- ADR-0038 · Implementation status — Decisions A–D and H, 2026-08-19 · `palw_tip_weights_v1`
- ADR-0053 · Decision 2 — `PalwClassTermsV2` is deleted, and the class re · `PalwRuntimePinsV2`
- ADR-0053 · Decision 6 — What is deleted, in full · `palw_metal_class_admission_v1`
- ADR-0056 · Decision 3 — Registration exposure: entry is priced in bonde · `verify_admission_v2`
- ADR-0060 · ADR-0060: The liveness doctrine — time is permissionless, we · `u64::MAX`
- ADR-0060 · What implementation added to Decision 1 · `processes::heartbeat_evidence`
- ADR-0060 · 6. Decision 4 — the finality inactivity leak (new; overlay-s · `InactivityLeakViewV1`
- ADR-0064 · Fact B — a zero-weight lane hands fork choice to the block h · `palw_tip_weights_v1`
- ADR-0066 · Security amendment (2026-09-02) — Decision 4's committed tab · `VirtualStateProcessor::palw_leak_table_provenance`
- ADR-0066 · Security amendment (2026-09-02) — Decision 4's committed tab · `dns_finality::leak_table_provenance_from_walk_v1`
- ADR-0069 · Security amendment (2026-09-02) — the open item is a fork-ch · `palw_tip_weights_v1`
- ADR-0069 · Security amendment (2026-09-02) — the open item is a fork-ch · `PalwCandidateOrderV1::new`
- ADR-0073 · 2. Decisions · `PalwFpCuWeightsV3`
- ADR-0074 · 2. Decisions · `PalwFpCuWeightsV3`
- ADR-0076 · 4. Decision 2 — the scale is spent on headroom · `u128::MAX`
- ADR-0083 · 4. Decision 1 — count only bits-priced rows; an empty count  · `kaspa_pow::State::new`
- ADR-0084 · 7. Implementation record (2026-09-04, `palw-adr0084-served-a · `fp_interval_seat_outcome_v1`
- ADR-0089 · 9. Implementation record (2026-09-05, `palw-adr0088-0089-imp · `PalwPayoutKindV2::EvmCredit`
- ADR-0103 · 10.6 Decision 2's price, amended (2026-09-11): a replay pays · `u64::MAX`
- ADR-0118 · 9. Implementation record (2026-09-12) · `FpWorkerRuntime::new`
- ADR-0118 · 9. Implementation record (2026-09-12) · `ChainFacts::public_ids_cannot_ride`
- ADR-0130 · 7. Implementation record (2026-09-17) · `rpc_tests::sanity_test`
- ADR-0132 · 7. Implementation record · `rpc_tests::sanity_test`
- ADR-0133 · 11. Implementation record · `rpc_tests::sanity_test`
- ADR-0134 · 6. Implementation record · `rpc_tests::sanity_test`

134 ADRs scanned; 826 sections/keyword groups; 33 identifiers named but absent.

## Reviewed notes (2026-09-18, by hand — the mechanical table above is the index, these are the verdicts)

### A. Activation table, testnet-11 (fingerprint `a8f99dac…`, schedule `…, 6000, 6001, 6100, 6201, 6900, …`)

| height | fences | ADR | class |
|---|---|---|---|
| 6,000 | `palw_held_context`, `palw_shard_court`, `palw_fp_da_pins`, `palw_audit_2026_09_11_deep` | 0118/0119/0121, 0100, the 09-11 audit | ACTIVE at 6,000 — the compatibility boundary (the deployed 7,000 release is refused here); no PALW upgrade rule fires |
| 6,001 | `palw_panel_economy`, `palw_work_priced_reward`, `palw_execution_lane`, `palw_overlay_carve`, `dns_bft_gate`, `palw_model_registry`, `palw_economic_payout`, `palw_work_target`, `palw_single_lottery`, `palw_short_challenge_window` | 0124, 0125, 0126, 0128, 0130 M1/M2, 0135 A, 0132 C, 0137 (with the rooted W), 0132 S, 0132 §7.6 | ACTIVE at 6,001 — the one PALW upgrade day; pinned by `t11_daa_6000_is_the_compatibility_boundary_and_6001_the_one_palw_upgrade_flag_day` (every fence of the preset classified; 5999/6000/6001/6002 table) |
| 6,100 | `palw_verification_v2` | 0133 S1 (protocol half) | ACTIVE at 6,100 |
| 6,201 | `palw_compute_overlay_retired` | 0134 | ACTIVE at 6,201 |
| 6,900 | `palw_model_seed_v2` | 0120 | ACTIVE at 6,900 |
| never | `palw_share_growth_final` (0107), the λ seat-exposure floor (0130), every other fence the flag-day table leaves unclassified | — | DORMANT (built, not scheduled) or below 6,000 (already live) |

`ClassManifestV2` (0135 manifest V2) is consensus-critical (a rooted row, a lifecycle profile) and rides `palw_model_registry`: in the 6,001 bundle by construction. `ReceiptLicensedV2` rides `palw_verification_v2` (6,100); a full mask is V1's receipt, so 6,100 changes no licence until seats file partial masks.

### B. The current program, ADR-0124 → 0137, by decision

| ADR | item | class | evidence |
|---|---|---|---|
| 0124 | panel 80/20, seat exposure 3×, work price | ACTIVE 6,001 | `palw_panel_economy`, `palw_work_priced_reward`; params + state tests |
| 0125 | execution lane | ACTIVE 6,001 | `palw_execution_lane`; devnet drills PASS ×2 (09-17) |
| 0126 | validator overlay 20 %, PALW escrow carve | ACTIVE 6,001 | `palw_overlay_carve` |
| 0127 | settlement on the Final anchor | IMPLEMENTED (no fence) | ops 182/183/184, merged |
| 0128 | DNS BFT gate, T_leak | ACTIVE 6,001 | `dns_bft_gate` |
| 0130 | M1 operator lottery, M2 scheduler v2 | ACTIVE 6,001 | round lane (`palw_round_producer.rs`) |
| 0130 | λ seat-exposure floor | DORMANT | fence exists, `None` (operator's call) |
| 0130 | D7/D8 | DESIGN-ONLY | no constructor |
| 0131 | economic compute (CCU) shadow | IMPLEMENTED | op 185, `palw_economic_compute_v1.rs`; the payout's basis |
| 0132 | Upgrade C (rate payout, accept-time snapshot, panel share clamp, cap rule) | ACTIVE 6,001 | `palw_economic_payout`; 21 tests |
| 0132 | S single lottery | ACTIVE 6,001 | `palw_single_lottery`; pow/difficulty/admission/state tests; producer honours it (`check_pow_layer0_v2`) |
| 0132 | §7.6 short challenge window | ACTIVE 6,001 | `palw_short_challenge_window` (was `6fdf6ba7`'s bare constant) |
| 0132 | per-model multipliers | N/A | forbidden by rule |
| 0133 | verification profile, class-local gate, capacity | IMPLEMENTED via 0135's rows | registry rows: window/prefetch/cap/required seats |
| 0133 | V2 = S1 protocol (assignment, masks, coverage licence) | ACTIVE 6,100 | `palw_verification_v2.rs`, `validate_receipt_coverage_v2`, `ReceiptLicensedV2` |
| 0133 | S1 runtime half (checkpoints, resume, segment replay, court resume) | DEFERRED | §11.1 (1)–(4); no constructor yet |
| 0133 | S2 optimistic licensing, S3 layer sampling | DESIGN-ONLY | order set: after S1 |
| 0133 | S4 zero-knowledge, S5 redundancy | NOT ADOPTED | no ZK, by policy |
| 0134 | compute overlay retirement | ACTIVE 6,201 | `palw_compute_overlay_retired` |
| 0135 | Upgrade A (registry rows, readiness proofs, gate, NoCapablePanel) | ACTIVE 6,001 | `palw_model_registry`; drills 09-17/09-18 |
| 0135 | node: readiness submitter, producer pre-check | IMPLEMENTED (release binary) | `kaspad/src/palw_panel.rs::readiness_duties`, `palw_producer.rs::registry_holds_class`; proofs on the drill chain |
| 0135 | manifest V2 object | ACTIVE 6,001 (registry fence) | `ClassManifestV2`; CLI `model-class` route |
| 0135 | node-side manifest route (`--palw-register-class` follows with a manifest) | DEFERRED | operator carries `<name>.class-manifest.borsh` |
| 0135 | D5 shares from admission | SUPERSEDED past 6,001 | ADR-0137 |
| 0136 | mapped artifact, host memory budget | IMPLEMENTED (node) | PSS measured |
| 0136 | one runtime a host, pin/LRU | DEFERRED | not built |
| 0137 | work target fence, rooted W, no share/budget/seat price past it | ACTIVE 6,001 | `palw_work_target`; 26 state tests; the producer reads no budget past it |
| 0137 | shadow (W, CCU/W, Final work share, panel room) | IMPLEMENTED | tail 0xAC, op 186 |

### C. "Named in an ADR, absent from the code" — verdicts on the 33

Renamed or generic (false positives of the exact-name check): `CommitReveal` (0012, exists), `VerifierCapabilityV1` (0034, exists under the capability declaration), `InactivityLeakViewV1` (0060, the leak view exists under ADR-0128's names), `fp_interval_seat_outcome_v1` (0084, exists), `FpWorkerRuntime::new` / `ChainFacts::public_ids_cannot_ride` (0118, exist), `PalwCandidateOrderV1::new` (0069, exists), `u64::MAX` / `u128::MAX` (0020/0024/0076/0103, literals), `rpc_tests::sanity_test` (0130/0132/0133/0134, a test name), `kaspa_pow::State::new` (0083, now `StateLayer0::new`).

Truly absent, by design: `PalwRuntimePinsV2`, `palw_metal_class_admission_v1` (0053 — deleted, the ADR says so), `PalwFpCuWeightsV3` (0073/0074 — the CU weight table was replaced by ADR-0131's economic cost table), `sign_palw_block_commitment_v1` / `palw_tip_weights_v1` (0038/0064/0069 — fork choice is `palw_fork_choice.rs::compare_palw_candidates_v1`; the block commitment is the attempt envelope's signature), `processes::heartbeat_evidence` (0060 — the heartbeat lane is `palw_heartbeat_v1.rs`, evidence-free by ADR-0066), `VirtualStateProcessor::palw_leak_table_provenance` / `dns_finality::leak_table_provenance_from_walk_v1` (0066 — the leak walk is ADR-0128's chain-block vote memo), `PalwPayoutKindV2::EvmCredit` (0089 — the EVM credit rides the position window, no payout kind), `Version::PubKeyHashMlDsa65` (0002 — only `PubKeyHashMlDsa87` shipped), `PqEvmTransactionV1` (0023 — the EVM lane's transaction is the EVM's own), `SortitionMode::CommitReveal` (0012 — commit-reveal exists, not as that enum), `RuleError::DnsFinalityReorgRejected` (0009 — the stake reorg gate is ADR-0128's), `verify_admission_v2` (0056 — `verify_class_admission_v2` is the constructor).

**Constructor / responder / operator paths an ADR names that do not exist:** ADR-0133 §11.1 (1)–(4) (checkpoint publication, runtime resume, segment replay, court resume); ADR-0135's node-side manifest route; ADR-0136's one-runtime-a-host and pin/LRU; ADR-0130 D7/D8; ADR-0119 §7 (resume transfer, fused evidence, worker frame, RoPE width check). Everything else the current program names has a path in the tree.

### D. Statements corrected today

* ADR-0054, 0073, 0074, 0076, 0107: status amendment — past 6,001 the share is a result (ADR-0137); the class target / class DAA / epoch budget / admission share / seat price are not read.
* ADR-0028: status amendment — no claim verifies inside a block interval; verification is its own clock (ADR-0133); V2 at 6,100.
* ADR-0133: the operator's order V1 → S1 → S3 → S2, no ZK; §11.1 built/unbuilt halves.
* ADR-0132 §7.5/§7.6, ADR-0135 (manifest V2 consensus-critical), ADR-0137 §22.2/§22.3, README rows for 0132/0135/0137 and the share rows.

### E. Method

The table above is mechanical: sections headed Decision/Phase/Follow-up and every line with a status keyword, the backticked identifiers checked by `git grep -w` (code hits / test hits), fences matched against the preset's arming. Its limits: an identifier renamed since the ADR reads as absent (see C), a `#[cfg(test)]` module inside a source file counts as code, and prose without an identifier is `UNCLASSIFIED`. ADR-0124–0137 were reviewed by hand (B); older ADRs stand on the mechanical pass plus the supersede map in `README.md`.
