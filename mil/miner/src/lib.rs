//! misaka-palw-miner — the PALW-only mining driver (ADR-0039): compute → k=2 → leaf.
//!
//! Ties a [`VerifiableInferenceBackend`] (the real Qwen backend or the CPU reference
//! runtime) to the on-chain leaf: run one job through TWO providers of the same runtime
//! class (k=2), and iff they exact-match, mint the candidate [`PalwPublicLeafV1`] from the
//! shared match key plus the miner's provider registration. This is the self-contained
//! compute half a node needs to mine algo-4 blocks; block-template construction, block
//! submission, and the beacon commit/reveal cycle live in the node (later phases). Carries
//! NO MIL job-market runtime (channel / attest).

pub mod audit;
pub mod authorization;
pub mod beacon;
pub mod da;
pub mod mining;
pub mod registration;

use kaspa_consensus_core::palw::{PalwPublicLeafV1, ticket_nullifier_commitment};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw::palw::ReplicaMatchKey;
use misaka_palw::palw_replica::{ReplicaK2Outcome, VerifiableInferenceBackend, dispatch_k2_backends};

/// ReplicaExactV1 — the only proof type this driver mints (design §20.2).
pub const PROOF_TYPE_REPLICA_EXACT_V1: u8 = 1;

/// The miner's on-chain provider identity: the fixed metadata a minted leaf carries beyond
/// the match-derived fields — the two provider bonds + one-time reward scripts, the ticket
/// authority, and the registration windows (all resolved from the provider registration TX).
#[derive(Clone, Debug)]
pub struct ProviderRegistration {
    pub provider_a_bond: TransactionOutpoint,
    pub provider_b_bond: TransactionOutpoint,
    /// ADR-0040 ECON-03 / CRITICAL-1: each reward script MUST equal
    /// `kaspa_consensus_core::palw::provider_bond_lock_spk(&owner_public_key)` of the corresponding
    /// provider bond — i.e. the 69-byte ML-DSA-87 P2PKH paying the bond's OWNER. `palw_work_reward_class`
    /// pays the 77% worker base only when `leaf.provider_{a,b}_reward_script ==
    /// provider_bond_lock_spk(bond.owner_public_key)`, binding payee ≡ collateral owner ≡ slashable
    /// party. A reward script paying anyone else — including a hot/cold split away from the bond key —
    /// makes the leaf resolve to zero collateral and earn nothing, exactly as a mismatched
    /// `ticket_authority_pk_hash` makes it unmineable. This is a REQUIREMENT the assembler must satisfy,
    /// not an option; nothing here can check it, because `ProviderRegistration` does not carry the bond
    /// owner key — the bond transaction does.
    pub provider_a_reward_script: ScriptPublicKey,
    pub provider_b_reward_script: ScriptPublicKey,
    /// ADR-0040 P1-6 (AUTH-03): the authority permitted to authorize blocks that spend this
    /// registration's tickets. MUST be [`authorization::TicketAuthority::pk_hash`] of a key the mining
    /// loop actually holds — body clause 7 checks it against the block's authorization, so a
    /// placeholder here makes every leaf minted under it unmineable.
    pub ticket_authority_pk_hash: Hash64,
    pub registered_epoch: u64,
    pub activation_epoch: u64,
    pub expiry_epoch: u64,
    pub leaf_bond_sompi: u64,
}

/// One inference job to mine: the batch slot + the actual work (job-set descriptor / prompt /
/// output salt) + the ticket authority's raw nullifier (disclosed only at the header, I-13).
#[derive(Clone, Debug)]
pub struct MiningJob {
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub job_set_descriptor: Vec<u8>,
    pub prompt: Vec<u8>,
    pub output_salt: [u8; 32],
    pub job_nullifier: Hash64,
    /// The raw ticket nullifier; the leaf commits to `ticket_nullifier_commitment(raw)`, and the
    /// winning header later reveals the raw value (I-13 winner-secrecy).
    pub raw_ticket_nullifier: Hash64,
    /// ADR-0045 D3-b — the PCPB leaf commitments for this job (dispatch anchor, snapshot roots,
    /// branch tag). Produced by the bridge's PCPB flow (A-commit anchoring / scheduler assignment);
    /// the miner copies them into the leaf verbatim — it can neither derive nor check them.
    pub pcpb: PcpbLeafFields,
}

/// The five D3-b leaf fields, as one producer-side unit (design memo §1). `external()` builds the
/// sentinel form for the beacon-assigned branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcpbLeafFields {
    pub a_commit: Hash64,
    pub a_commit_epoch: u64,
    pub provider_snapshot_root: Hash64,
    pub assignment_proof_root: Hash64,
    pub dispatch_kind: u8,
}

impl PcpbLeafFields {
    /// External branch: zero anchor sentinels + the epoch snapshot roots the scheduler assigned under.
    pub fn external(provider_snapshot_root: Hash64, assignment_proof_root: Hash64) -> Self {
        Self {
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root,
            assignment_proof_root,
            dispatch_kind: kaspa_consensus_core::palw::PALW_DISPATCH_KIND_BEACON_ASSIGNED,
        }
    }

    /// Self-serial branch: the anchored `a_commit` + its registry epoch + the `anchor − k` roots.
    pub fn self_serial(a_commit: Hash64, a_commit_epoch: u64, provider_snapshot_root: Hash64, assignment_proof_root: Hash64) -> Self {
        Self {
            a_commit,
            a_commit_epoch,
            provider_snapshot_root,
            assignment_proof_root,
            dispatch_kind: kaspa_consensus_core::palw::PALW_DISPATCH_KIND_SELF_SERIAL,
        }
    }
}

/// A minted candidate: the on-chain leaf, its hash, and the shared k=2 match key (kept for the
/// receipt / data-availability step).
#[derive(Clone, Debug)]
pub struct MintedLeaf {
    pub leaf: PalwPublicLeafV1,
    pub leaf_hash: Hash64,
    pub match_key: ReplicaMatchKey,
}

/// Why a job produced no leaf.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MineError {
    /// The two providers disagreed on at least one of the eight match fields — the audited-compute
    /// check that stops a faulty / dishonest provider from minting. No leaf, no ticket.
    #[error("k=2 replica mismatch — the two providers disagreed; no leaf minted")]
    ReplicaMismatch,
    /// ADR-0040 AUTH-03: this miner does not hold the ticket authority key its own registration names,
    /// so every leaf it mints would be unmineable. Refused BEFORE the k=2 inference runs.
    #[error(
        "registration names ticket authority {expected} but this miner holds {got} — leaves minted under it could never be authorized"
    )]
    UnsignableRegistration { expected: Hash64, got: Hash64 },
}

/// The PALW mining driver over two providers of the SAME runtime class (I-9). For a single
/// operator both are local backend instances; across the network they are two independent
/// providers whose byte-exact agreement IS the audited-compute guarantee.
pub struct PalwMiner<A: VerifiableInferenceBackend, B: VerifiableInferenceBackend> {
    provider_a: A,
    provider_b: B,
    reg: ProviderRegistration,
}

impl<A: VerifiableInferenceBackend, B: VerifiableInferenceBackend> PalwMiner<A, B> {
    pub fn new(provider_a: A, provider_b: B, reg: ProviderRegistration) -> Self {
        Self { provider_a, provider_b, reg }
    }

    /// The provider registration this miner mints under.
    pub fn registration(&self) -> &ProviderRegistration {
        &self.reg
    }

    /// ADR-0040 AUTH-03 preflight: refuse to mint under a registration whose ticket authority this node
    /// cannot sign for.
    ///
    /// This is the cheap check that has to happen before the expensive one. Minting a leaf runs the k=2
    /// inference — the single most costly step in the lane — and a leaf whose `ticket_authority_pk_hash`
    /// names a key the miner does not hold can never be turned into a block: clause 7 requires the
    /// block's authorization to be signed by exactly that authority. Without this preflight the failure
    /// surfaces at `authorize_for_leaf`, i.e. after the compute is already spent, after the leaf is
    /// registered on chain, and after the registration fee is paid.
    ///
    /// Note the asymmetry with the mining-time filter in
    /// [`crate::mining::select_eligible_ticket`]: that one drops FOREIGN tickets a miner happens to
    /// observe, this one rejects the miner's OWN misconfiguration. Both exist because they fire at
    /// different times and cost different amounts.
    pub fn assert_signable_by(&self, authority_pk_hash: &Hash64) -> Result<(), MineError> {
        if self.reg.ticket_authority_pk_hash == *authority_pk_hash {
            Ok(())
        } else {
            Err(MineError::UnsignableRegistration { expected: self.reg.ticket_authority_pk_hash, got: *authority_pk_hash })
        }
    }

    /// [`Self::produce_leaf`] gated on [`Self::assert_signable_by`] — the entry point a node with a
    /// ticket authority key in hand should call, so no inference is spent on an unmineable leaf.
    pub fn produce_leaf_for_authority(&self, job: &MiningJob, authority_pk_hash: &Hash64) -> Result<MintedLeaf, MineError> {
        self.assert_signable_by(authority_pk_hash)?;
        self.produce_leaf(job)
    }

    /// Run `job` through both providers (k=2). On exact-match, mint the candidate leaf from the
    /// shared key + the provider registration; on disagreement, no leaf ([`MineError::ReplicaMismatch`]).
    /// Pure over `(providers, job, registration)` — deterministic, no wall-clock — so a validator
    /// re-deriving the leaf fields from the same inputs gets the identical `leaf_hash`.
    pub fn produce_leaf(&self, job: &MiningJob) -> Result<MintedLeaf, MineError> {
        let key =
            match dispatch_k2_backends(&self.provider_a, &self.provider_b, &job.job_set_descriptor, &job.prompt, &job.output_salt) {
                ReplicaK2Outcome::Matched(k) => k,
                ReplicaK2Outcome::Mismatch => return Err(MineError::ReplicaMismatch),
            };
        let leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: job.batch_id,
            leaf_index: job.leaf_index,
            job_nullifier: job.job_nullifier,
            ticket_nullifier_commitment: ticket_nullifier_commitment(&job.raw_ticket_nullifier),
            model_profile_id: key.model_profile_id,
            runtime_class_id: key.runtime_class_id,
            shape_id: key.shape_id,
            quantum_count: key.quantum_count,
            proof_type: PROOF_TYPE_REPLICA_EXACT_V1,
            provider_a_bond: self.reg.provider_a_bond,
            provider_b_bond: self.reg.provider_b_bond,
            provider_a_reward_script: self.reg.provider_a_reward_script.clone(),
            provider_b_reward_script: self.reg.provider_b_reward_script.clone(),
            ticket_authority_pk_hash: self.reg.ticket_authority_pk_hash,
            // Bind the leaf to THIS exact k=2 GEMM execution (the private match commitment).
            private_match_commitment: key.canonical_gemm_trace_root,
            receipt_da_object_version: 1,
            receipt_da_root: Hash64::default(),
            // Filled together with `receipt_da_root` by `da::PalwDaProducerArtifact::bind_leaf`
            // before this candidate may enter a leaf chunk.
            receipt_da_object_len: 0,
            receipt_da_chunk_count: 0,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: self.reg.registered_epoch,
            activation_epoch: self.reg.activation_epoch,
            expiry_epoch: self.reg.expiry_epoch,
            leaf_bond_sompi: self.reg.leaf_bond_sompi,
            a_commit: job.pcpb.a_commit,
            a_commit_epoch: job.pcpb.a_commit_epoch,
            provider_snapshot_root: job.pcpb.provider_snapshot_root,
            assignment_proof_root: job.pcpb.assignment_proof_root,
            dispatch_kind: job.pcpb.dispatch_kind,
        };
        let leaf_hash = leaf.leaf_hash();
        Ok(MintedLeaf { leaf, leaf_hash, match_key: key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw::palw::{PalwRuntimeProfileV1, PalwSamplingParams, PalwTier};
    use misaka_palw::palw_replica::MockDeterministicRuntime;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn profile(tier: PalwTier, arch: u32) -> PalwRuntimeProfileV1 {
        PalwRuntimeProfileV1 {
            version: 1,
            tier,
            model_id: tier.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: arch,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        }
    }

    fn reg() -> ProviderRegistration {
        // ADR-0040 P0-4 (ECON-01): a leaf's reward scripts are emitted VERBATIM as coinbase outputs, so
        // leaf admission requires the exact 69-byte P2PKH ML-DSA-87 template. An arbitrary script is not
        // coinbase-representable and the leaf chunk is rejected.
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0xa0; 64]);
        ProviderRegistration {
            provider_a_bond: TransactionOutpoint::new(h(6), 0),
            provider_b_bond: TransactionOutpoint::new(h(7), 0),
            provider_a_reward_script: spk.clone(),
            provider_b_reward_script: spk,
            ticket_authority_pk_hash: h(8),
            registered_epoch: crate::registration::tests::FIXTURE_REGISTRATION_EPOCH,
            activation_epoch: 4,
            expiry_epoch: 1000,
            leaf_bond_sompi: 0,
        }
    }

    fn job() -> MiningJob {
        MiningJob {
            batch_id: h(0x10),
            leaf_index: 0,
            job_set_descriptor: b"job-set-1".to_vec(),
            prompt: b"the prompt".to_vec(),
            output_salt: [0x33; 32],
            job_nullifier: h(0x20),
            raw_ticket_nullifier: h(0xC0),
            pcpb: crate::PcpbLeafFields::external(Hash64::default(), Hash64::default()),
        }
    }

    #[test]
    fn two_honest_same_class_providers_mint_a_leaf() {
        let miner = PalwMiner::new(
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            reg(),
        );
        let minted = miner.produce_leaf(&job()).expect("two honest same-class providers must match");
        assert_eq!(minted.leaf.proof_type, PROOF_TYPE_REPLICA_EXACT_V1);
        assert_eq!(minted.leaf.batch_id, h(0x10));
        // The leaf binds to the shared match execution and commits the ticket nullifier.
        assert_eq!(minted.leaf.private_match_commitment, minted.match_key.canonical_gemm_trace_root);
        assert_eq!(minted.leaf.ticket_nullifier_commitment, ticket_nullifier_commitment(&h(0xC0)));
        assert_eq!(minted.leaf_hash, minted.leaf.leaf_hash());
        // Deterministic: mining the same job again yields the identical leaf hash.
        let again = miner.produce_leaf(&job()).unwrap();
        assert_eq!(again.leaf_hash, minted.leaf_hash);
    }

    #[test]
    fn cross_class_providers_do_not_match_no_leaf() {
        // Two DIFFERENT runtime classes (different gpu_arch_class) never exact-match → no leaf.
        let miner = PalwMiner::new(
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 200), 3, 2),
            reg(),
        );
        assert_eq!(miner.produce_leaf(&job()).unwrap_err(), MineError::ReplicaMismatch);
    }

    #[test]
    fn a_faulty_provider_breaks_the_match() {
        // Provider B computes a wrong answer for the same job ⇒ mismatch ⇒ no leaf.
        struct Faulty(MockDeterministicRuntime);
        impl VerifiableInferenceBackend for Faulty {
            fn profile(&self) -> &PalwRuntimeProfileV1 {
                self.0.profile()
            }
            fn infer_with_trace(
                &self,
                js: &[u8],
                prompt: &[u8],
                salt: &[u8; 32],
            ) -> misaka_palw::palw::DeterministicInferenceOutputV1 {
                // Perturb the prompt so the deterministic output (and thus the match key) differs.
                let mut p = prompt.to_vec();
                p.push(0xFF);
                self.0.infer_with_trace(js, &p, salt)
            }
        }
        let miner = PalwMiner::new(
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            Faulty(MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2)),
            reg(),
        );
        assert_eq!(miner.produce_leaf(&job()).unwrap_err(), MineError::ReplicaMismatch);
    }
    /// AUTH-03 preflight: the expensive k=2 inference must not run for a registration this node cannot
    /// authorize. The rejection is attributable — it names both hashes — and the matching authority
    /// still mints, which is what shows the gate discriminates rather than just failing.
    #[test]
    fn produce_leaf_for_authority_refuses_a_registration_it_cannot_sign() {
        let miner = PalwMiner::new(
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            MockDeterministicRuntime::new(profile(PalwTier::Quality, 100), 3, 2),
            reg(),
        );
        let ours = miner.registration().ticket_authority_pk_hash;
        let job = job();

        let stranger = h(0xF2);
        assert_ne!(stranger, ours);
        assert_eq!(
            miner.produce_leaf_for_authority(&job, &stranger).unwrap_err(),
            MineError::UnsignableRegistration { expected: ours, got: stranger }
        );
        assert_eq!(
            miner.assert_signable_by(&stranger).unwrap_err(),
            MineError::UnsignableRegistration { expected: ours, got: stranger }
        );

        // The rightful authority mints, and the leaf declares the authority that can authorize it.
        let minted = miner.produce_leaf_for_authority(&job, &ours).expect("the rightful authority mints");
        assert_eq!(minted.leaf.ticket_authority_pk_hash, ours);
    }
}
