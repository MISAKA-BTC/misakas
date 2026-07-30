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

/// Header-v4's objective floor is intentionally a pure isolation-stage check. Keeping it outside
/// the GHOSTDAG-dependent validator makes the ordering testable and prevents cheap algo-4 headers
/// from reaching reachability or accumulator work.
fn validate_palw_base_stamp(params: kaspa_consensus_core::palw_antispam::PalwSpamParams, header: &Header) -> BlockProcessResult<()> {
    if params.is_inert() {
        return Ok(());
    }
    let actual_bits = kaspa_consensus_core::palw_antispam::palw_spam_leading_zero_bits(header);
    if actual_bits < params.base_stamp_bits {
        return Err(RuleError::PalwSpamBaseStampTooWeak { required_bits: params.base_stamp_bits, actual_bits });
    }
    Ok(())
}

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
    /// selected-parent EVM lane has no gaps. The shipped presets resolve to base v1 on
    /// mainnet/simnet, EVM v2 on ordinary testnet/devnet, and PALW v3 on suffix 110/111.
    /// All remain pre-v4 because every shipped `palw_spam` configuration is inert.
    fn check_header_version(&self, header: &Header) -> BlockProcessResult<()> {
        // kaspa-pq ADR-0039: the header **schema version** is decoupled from lane *activation*. The
        // required version is the highest active lane's schema at the header's DAA score (PALW
        // anti-spam v4 > PALW v3 > EVM v2 > base v1), and each lane's SEMANTIC validity is gated on
        // its OWN activation score (not on `version >= X`). The PALW presets activate the v3 schema
        // from genesis even though algo-4 acceptance remains default-off; no shipped preset enables v4.
        let evm_active = header.daa_score >= self.evm_activation_daa_score;
        let palw_active = header.daa_score >= self.palw_activation_daa_score;
        // ADR-MA: the Compute Set registry schema (v5) sits atop the anti-spam schema (v4); its
        // fence is its own activation score (u64::MAX on every shipped preset ⇒ unreachable).
        let compute_registry_active = header.daa_score >= self.palw_compute_registry_activation_daa_score;
        let expected = if palw_active && compute_registry_active {
            kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION
        } else if palw_active && !self.palw_spam.is_inert() {
            kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
        } else if palw_active {
            kaspa_consensus_core::constants::PALW_HEADER_VERSION
        } else if evm_active {
            kaspa_consensus_core::constants::EVM_HEADER_VERSION
        } else {
            constants::BLOCK_VERSION
        };
        // Exact match (never accept an unknown future version and hash only the fields we know — that
        // would compute a different preimage for a header carrying fields we ignore).
        if header.version != expected {
            return Err(RuleError::WrongBlockVersion(header.version, expected));
        }
        // audit R2-#2: the two EVM commitment fields are excluded from the v0/v1 header preimage
        // (hashing/header.rs), so while EVM is INACTIVE they are hash-invisible — non-zero values would
        // let a peer mint distinct serialized headers sharing one block id (malleability in the header
        // store / relay / IBD / orphan paths, before the body ever arrives). Gated on EVM activation
        // (DAA), NOT on `version < EVM_HEADER_VERSION` — else a v3 PALW header (version 3 ≥ 2) on a net
        // where EVM is NOT active would skip the check.
        let zero = kaspa_hashes::Hash64::default();
        if !evm_active && (header.evm_payload_hash != zero || header.evm_commitment_root != zero) {
            return Err(RuleError::NonZeroEvmHeaderFieldsBeforeActivation);
        }
        // ADR-0039 §13: the ten PALW fields are excluded from the pre-v3 preimage, so while PALW is
        // inactive they are hash-invisible — enforce them zero for the same anti-malleability reason.
        // On every current network `palw_active` is false, so this rejects any non-zero PALW field;
        // honest headers carry all-zero PALW fields (inert), so the rule is a no-op on real traffic.
        if !palw_active && header.has_nonzero_palw_fields() {
            return Err(RuleError::NonZeroPalwHeaderFieldsBeforeActivation);
        }
        // Header-v4 fields are hash-invisible on v3. Every existing PALW preset remains v3/inert, so
        // force these bytes to zero there and prevent serialized-header/block-id malleability.
        if header.version < kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION
            && (header.palw_spam_accumulator_commitment != zero || header.palw_spam_nonce != 0)
        {
            return Err(RuleError::NonZeroPalwHeaderFieldsBeforeActivation);
        }
        // ADR-MA: the three Header-v5 Compute Set references are hash-invisible below v5 — same
        // anti-malleability rule, next version boundary.
        if header.version < kaspa_consensus_core::constants::PALW_COMPUTE_SET_HEADER_VERSION
            && (header.palw_compute_set_id != zero || header.palw_compute_policy_id != zero || header.palw_allocation_plan_id != zero)
        {
            return Err(RuleError::NonZeroPalwHeaderFieldsBeforeActivation);
        }
        // ADR-0040 **P1-12 (SHAPE-01)** — the POST-activation half of the shape rule, which was missing.
        //
        // Before activation the fields must be zero (above). After activation nothing constrained them
        // on the HASH lane: an algo-3 v3 header could carry arbitrary `palw_batch_id` /
        // `palw_ticket_nullifier` / `palw_authorization_hash`, because `check_palw_ticket` returns early
        // for `pow_algo_id != 4`. Those fields DO enter the v3 hash preimage, so unconstrained values are
        // header malleability — the same reason the pre-activation rule exists, just on the other side of
        // the fence. An algo-3 block carries no ticket, so its ticket fields must be zero.
        if palw_active && header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA {
            let ticket_fields_zero = header.palw_batch_id == zero
                && header.palw_leaf_index == 0
                && header.palw_ticket_nullifier == zero
                && header.palw_epoch_certificate_hash == zero
                && header.palw_chain_commit == zero
                && header.palw_target_daa_interval == 0
                && header.palw_authorization_hash == zero
                && header.palw_proof_type == 0
                && header.palw_spam_nonce == 0;
            if !ticket_fields_zero {
                return Err(RuleError::NonZeroPalwHeaderFieldsBeforeActivation);
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
        // kaspa-pq ADR-0039 PALW (§5.1): once the compute lane is active this is a MIXED-lane policy —
        // the permanent hash floor (algo-3) and the replica lane (algo-4) coexist, so a header may
        // declare either. Before activation the single-algo cut-over rule below runs unchanged —
        // byte-identical — which is mainnet / testnet-10 / simnet / devnet (`u64::MAX`), NOT every
        // shipped preset: testnet-palw-110 / devnet-palw-111 ship 0 and take the mixed-lane branch.
        // The algo-4 ACCEPT check further down (`palw_algo4_accept`) is what closes the lane there.
        // (An accepted algo-4 header is then eligibility-
        // verified against the PALW overlay stores in the post-parents / body stages; that wiring +
        // the algo-4 PoW branch land with the §18 overlay stores.)
        if header.daa_score >= self.palw_activation_daa_score {
            // ADR-0040 P0-3 — the ACCEPTANCE lever, checked here and not later, on purpose.
            //
            // This is the earliest point at which the lane is knowable: `check_pow_algo_id` runs inside
            // `validate_header_in_isolation`, i.e. BEFORE GHOSTDAG, before reachability, and before
            // `commit_header` performs its header-stage store writes (headers, relations, statuses, depth,
            // and the O(nullifier-retention) PALW active-set clone+prune+persist that runs PER HEADER).
            //
            // Rejecting here is what makes DOS-01 unreachable while the gates are closed: an algo-4 header
            // is exempt from the Layer-0 hash floor (`check_pow_and_calc_block_level` returns `Ok(0)` for
            // it), and `palw_compute_work_scale = 0` on the shipped PALW presets means the compute cap can
            // never fire — so neither PoW nor the cap bounds algo-4 header volume. This lever does.
            if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA && !self.palw_algo4_accept {
                return Err(RuleError::PalwAlgo4NotAccepted);
            }
            return kaspa_consensus_core::pow_layer0::check_live_algo_id(header.pow_algo_id, true)
                .map(|_| ())
                .map_err(|_| RuleError::UnknownPowAlgoId(header.pow_algo_id));
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
        // ADR-0039 §5.1 — the algo-4 (PALW replica) lane's proof-of-work is the replica-exact GEMM match +
        // the body-stage clause-9 eligibility draw, whose nonce is PINNED to `low64(nullifier)` — NOT the
        // Layer-0 BLAKE2b-SHA3 hash floor (which would be unsatisfiable at that pinned nonce). So an
        // algo-4 header is EXEMPT from the hash-floor check here and takes the lane floor block level (0);
        // its GHOSTDAG credit comes from the compute lane (`normalize_palw_work`) and its anti-spam is the
        // eligibility draw + k=2 exact-match + provider bonds, all verified downstream. Gated on activation
        // AND the algo-4 lane id, so it is byte-identical while PALW is inert (no algo-4 header can exist —
        // `check_pow_algo_id` rejects id 4 pre-activation). This is the "algo-4 PoW branch" that comment
        // deferred, and it removes the `skip_proof_of_work` crutch a live PALW net (testnet-palw) needs.
        if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA
            && header.daa_score >= self.palw_activation_daa_score
        {
            // Header-v4 pays a non-zero objective floor before GHOSTDAG, reachability, accumulator
            // reads, or any header-stage write. This closes the cheap-sibling/orphan flood path.
            validate_palw_base_stamp(self.palw_spam, header)?;
            return Ok(0);
        }
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

#[cfg(test)]
mod palw_spam_tests {
    use super::*;
    use kaspa_consensus_core::{
        BlueWorkType,
        header::{Header, PalwHeaderFields},
        palw_antispam::{PalwSpamParams, mine_palw_spam_stamp, palw_spam_leading_zero_bits},
        pow_layer0::POW_ALGO_ID_PALW_REPLICA,
    };
    use kaspa_hashes::Hash64;

    fn v4_header() -> Header {
        Header::new_finalized(
            kaspa_consensus_core::constants::PALW_ANTISPAM_HEADER_VERSION,
            vec![vec![Hash64::from_bytes([1; 64])]].try_into().unwrap(),
            Hash64::from_bytes([2; 64]),
            Hash64::from_bytes([3; 64]),
            Hash64::from_bytes([4; 64]),
            5,
            0x1f00ffff,
            7,
            POW_ALGO_ID_PALW_REPLICA,
            8,
            BlueWorkType::from(9u64),
            10,
            Hash64::from_bytes([11; 64]),
        )
        .with_palw_fields(PalwHeaderFields { palw_spam_accumulator_commitment: Hash64::from_bytes([12; 64]), ..Default::default() })
    }

    #[test]
    fn objective_floor_rejects_weak_header_in_isolation_and_accepts_grind() {
        let mut header = v4_header();
        let actual = palw_spam_leading_zero_bits(&header);
        let weak_params =
            PalwSpamParams { window_daa: 1, replicas_per_hash: 1, burst: 1, base_stamp_bits: actual + 1, max_stamp_bits: actual + 1 };
        assert!(matches!(validate_palw_base_stamp(weak_params, &header), Err(RuleError::PalwSpamBaseStampTooWeak { .. })));

        let params = PalwSpamParams { base_stamp_bits: 4, max_stamp_bits: 4, ..weak_params };
        mine_palw_spam_stamp(&mut header, params.base_stamp_bits, 0, 100_000).unwrap();
        validate_palw_base_stamp(params, &header).unwrap();
        validate_palw_base_stamp(PalwSpamParams::INERT, &v4_header()).unwrap();
    }
}
