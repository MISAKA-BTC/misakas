use super::*;
use crate::constants;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::statuses::StatusesStoreReader;
use kaspa_consensus_core::BlockLevel;
use kaspa_consensus_core::blockhash::BlockHashExtensions;
use kaspa_consensus_core::blockstatus::BlockStatus::StatusInvalid;
use kaspa_consensus_core::header::Header;
use kaspa_core::time::unix_now;
use kaspa_database::prelude::StoreResultExt;

impl HeaderProcessor {
    /// Validates the header in isolation including pow check against header declared bits.
    /// Returns the block level as computed from pow state or a rule error if such was encountered
    pub(super) fn validate_header_in_isolation(&self, header: &Header) -> BlockProcessResult<BlockLevel> {
        self.check_header_version(header)?;
        self.check_pow_algo_id(header)?;
        self.check_block_timestamp_in_isolation(header)?;
        self.check_parents_limit(header)?;
        Self::check_parents_not_origin(header)?;
        self.check_pow_and_calc_block_level(header)
    }

    pub(super) fn validate_parent_relations(&self, header: &Header) -> BlockProcessResult<()> {
        self.check_parents_exist(header)?;
        self.check_parents_incest(header)?;
        Ok(())
    }

    /// kaspa-pq EVM Lane v0.4 (ADR-0020 §4.3): the header version is fork-gated
    /// on the header's declared DAA score (the same pattern as
    /// `check_pow_algo_id`; the declared score is itself consensus-validated
    /// post-GHOSTDAG). Before activation only `BLOCK_VERSION` (v1) is admitted;
    /// at/after activation only `EVM_HEADER_VERSION` (v2) is — every
    /// post-activation block must carry the two EVM commitments, so the
    /// selected-parent EVM lane has no gaps. Inert on every current network
    /// (`evm_activation_daa_score = u64::MAX` ⇒ the rule stays `== v1`).
    fn check_header_version(&self, header: &Header) -> BlockProcessResult<()> {
        let expected = if header.daa_score >= self.evm_activation_daa_score {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            constants::BLOCK_VERSION
        };
        if header.version != expected {
            return Err(RuleError::WrongBlockVersion(header.version, expected));
        }
        // audit R2-#2: the two EVM commitment fields are excluded from the v0/v1
        // header preimage (hashing/header.rs), so on a pre-activation header they
        // are hash-invisible — non-zero values there would let a peer mint
        // distinct serialized headers sharing one block id (malleability in the
        // header store / relay / IBD / orphan paths, before the body ever
        // arrives). Enforce zero in HEADER-ONLY validation (body validation keeps
        // the same check as defense-in-depth).
        if expected < kaspa_consensus_core::constants::EVM_HEADER_VERSION {
            let zero = kaspa_hashes::Hash64::default();
            if header.evm_payload_hash != zero || header.evm_commitment_root != zero {
                return Err(RuleError::NonZeroEvmHeaderFieldsBeforeActivation);
            }
        }
        Ok(())
    }

    /// kaspa-pq Layer-0 (PR-9.5d / ADR-0007): a header MUST declare the exact Layer-1 `algo_id` the
    /// network mandates at its DAA score — `algo_id = 3` (BLAKE2b-512 ∥ SHA3-512) once the Phase-3 fork
    /// is active, else `algo_id = 1` (kHeavyHash). Enforces the single-algo invariant (no mixed-`algo_id`
    /// DAG) and is checked before the PoW seed (which consumes `algo_id`) is derived. Genesis — the
    /// parentless trusted root — is exempt (its PoW is never validated; it may carry either id).
    fn check_pow_algo_id(&self, header: &Header) -> BlockProcessResult<()> {
        if header.direct_parents().is_empty() {
            return Ok(());
        }
        let blake2b_sha3_active = self.pow_blake2b_sha3_activation.is_active(header.daa_score);
        kaspa_consensus_core::pow_layer0::check_algo_id(header.pow_algo_id, blake2b_sha3_active)
            .map_err(|_| RuleError::UnknownPowAlgoId(header.pow_algo_id))
    }

    fn check_block_timestamp_in_isolation(&self, header: &Header) -> BlockProcessResult<()> {
        // Timestamp deviation tolerance is in seconds so we multiply by 1000 to get milliseconds (without BPS dependency)
        let max_block_time = unix_now() + self.timestamp_deviation_tolerance * 1000;
        if header.timestamp > max_block_time {
            return Err(RuleError::TimeTooFarIntoTheFuture(header.timestamp, max_block_time));
        }
        Ok(())
    }

    fn check_parents_limit(&self, header: &Header) -> BlockProcessResult<()> {
        if header.direct_parents().is_empty() {
            return Err(RuleError::NoParents);
        }

        let max_block_parents = self.max_block_parents as usize;
        if header.direct_parents().len() > max_block_parents {
            return Err(RuleError::TooManyParents(header.direct_parents().len(), max_block_parents));
        }

        Ok(())
    }

    fn check_parents_not_origin(header: &Header) -> BlockProcessResult<()> {
        if header.direct_parents().iter().any(|&parent| parent.is_origin()) {
            return Err(RuleError::OriginParent);
        }

        Ok(())
    }

    fn check_parents_exist(&self, header: &Header) -> BlockProcessResult<()> {
        let mut missing_parents = Vec::new();
        for parent in header.direct_parents() {
            match self.statuses_store.read().get(*parent).optional().unwrap() {
                None => missing_parents.push(*parent),
                Some(StatusInvalid) => {
                    return Err(RuleError::InvalidParent(*parent));
                }
                Some(_) => {}
            }
        }
        if !missing_parents.is_empty() {
            return Err(RuleError::MissingParents(missing_parents));
        }
        Ok(())
    }

    fn check_parents_incest(&self, header: &Header) -> BlockProcessResult<()> {
        let parents = header.direct_parents();
        for parent_a in parents.iter() {
            for parent_b in parents.iter() {
                if parent_a == parent_b {
                    continue;
                }

                if self.reachability_service.is_dag_ancestor_of(*parent_a, *parent_b) {
                    return Err(RuleError::InvalidParentsRelation(*parent_a, *parent_b));
                }
            }
        }

        Ok(())
    }

    fn check_pow_and_calc_block_level(&self, header: &Header) -> BlockProcessResult<BlockLevel> {
        // PR-8.6: kaspa-pq Layer 0 PoW (BLAKE2b-512, 512-bit target) replaces the
        // legacy 32-byte kHeavyHash check. `StateLayer0` wraps the Phase-1
        // (algo_id=1) kHeavyHash inner loop inside the domain-separated Layer 0
        // finalizer; the block level is derived from the 512-bit pow value
        // (ADR-0007 / ADR-0008).
        let state = kaspa_pow::StateLayer0::new(header, &self.network_id);
        let (passed, pow_512) = state.check_pow_layer0(header.nonce).map_err(|_| RuleError::InvalidPoW)?;
        if passed || self.skip_proof_of_work {
            Ok(kaspa_pow::calc_level_from_pow_512(pow_512, self.max_block_level))
        } else {
            Err(RuleError::InvalidPoW)
        }
    }
}
