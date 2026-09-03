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
        self.validate_header_in_isolation_sans_pow(header)?;
        self.check_pow_and_calc_block_level(header)
    }

    /// The cheap, parent-independent isolation checks — everything in
    /// [`Self::validate_header_in_isolation`] EXCEPT the Layer-0 PoW.
    ///
    /// Split out so the ordinary path ([`super::HeaderProcessor::validate_header`]) can run parent
    /// validation between these and the PoW. On a PALW network the PoW is one full LLM inference
    /// under a global spawn gate, so a header whose parents do not exist must be rejected *before*
    /// it is paid for — otherwise a peer buys an inference (and stalls every other header behind the
    /// gate) with a fabricated, parentless header (mainnet-readiness audit P0-3). These checks read
    /// only the header, so ordering them ahead of the PoW changes nothing about what they accept.
    pub(super) fn validate_header_in_isolation_sans_pow(&self, header: &Header) -> BlockProcessResult<()> {
        self.check_header_version(header)?;
        self.check_pow_algo_id(header)?;
        self.check_block_timestamp_in_isolation(header)?;
        self.check_parents_limit(header)?;
        Self::check_parents_not_origin(header)?;
        Ok(())
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
    /// network mandates at its DAA score — `algo_id = 4` (PALW deterministic LLM) once the Phase-4
    /// fork is active, else `algo_id = 3` (BLAKE2b-512 ∥ SHA3-512) once the Phase-3 fork is active,
    /// else `algo_id = 1` (kHeavyHash). Enforces the single-algo invariant (no mixed-`algo_id`
    /// DAG) and is checked before the PoW seed (which consumes `algo_id`) is derived. Genesis — the
    /// parentless trusted root — is exempt (its PoW is never validated; it may carry either id).
    fn check_pow_algo_id(&self, header: &Header) -> BlockProcessResult<()> {
        // NOTE the predicate. This is `direct_parents()`, which reports "parentless" both for a real
        // root (no levels) and for a header whose level-0 run exists but is EMPTY — and for the
        // latter the PoW does NOT short-circuit (see
        // `pow_layer0::pow_short_circuits_as_parentless_root`). That mismatch is a remote-panic
        // vector wherever nothing else rejects the empty-run shape, and it was exactly the bug in
        // the first pruning-proof gate.
        //
        // It is safe HERE only because `check_parents_limit` rejects `direct_parents().is_empty()`
        // with `RuleError::NoParents`, and it runs in the same pre-PoW group
        // (`validate_header_in_isolation_sans_pow`) — so the empty-run header never reaches the
        // finalizer on this path. Moving `check_parents_limit` after the PoW, or dropping it, would
        // re-open the hole; a gate without that backstop must use the shared predicate instead.
        if header.direct_parents().is_empty() {
            return Ok(());
        }
        let palw_ollama_active = self.pow_palw_ollama_activation.is_active(header.daa_score);
        let palw_active = self.pow_palw_activation.is_active(header.daa_score);
        let blake2b_sha3_active = self.pow_blake2b_sha3_activation.is_active(header.daa_score);
        let heartbeat_open = header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
            && self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(header.daa_score));
        // **ADR-0072 SA-3/SA-4 — which attempt lane is open at this height.**
        //
        // `Unfenced` on every shipped preset, and every branch below is written so that arm is a
        // no-op: `attempt_algo_id()` is 6, `admits_attempt_algo_id` accepts 6 and refuses 9 exactly
        // as the bundle already does, and `attempt_version()` is the compiled-in one.
        let attempt_lane = kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1::from_fence(
            self.palw_attempt_activation.map(|fence| fence.is_active(header.daa_score)),
        );
        // The bundle's two ids are 6 and 7 and it cannot see a top-level fence, so — exactly as the
        // heartbeat lane does — the new attempt id is ORed in here. Only past the fence: an algo-9
        // header below it is not a lane.
        let exec_lane_open = header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3
            && attempt_lane == kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1::ExecutionArm;
        // Accepts, not demands: a V2 network admits its receipt lane as well as its attempt
        // lane, and asking only for the demanded id refused every block on the first one.
        kaspa_consensus_core::pow_layer0::check_algo_id_for_mode_accepting(
            header.pow_algo_id,
            self.palw_required_algo_id,
            // ADR-0066: the bundle answers for its own two lanes; the heartbeat is a TOP-LEVEL
            // fence, so it is ORed in here rather than inside a bundle that cannot see it.
            // ADR-0072 SA-4 adds the second attempt id on the same terms.
            self.palw_consensus_mode.accepts_algo_id(header.pow_algo_id).map(|a| a || heartbeat_open || exec_lane_open),
            palw_ollama_active,
            palw_active,
            blake2b_sha3_active,
        )
        .map_err(|_| RuleError::UnknownPowAlgoId(header.pow_algo_id))?;
        // **And the other half of SA-4: the OLD id closes when the new one opens.**
        //
        // The gate above is an accept-list, so it would keep admitting algo-6 past the fence — and
        // an un-upgraded producer would keep making algo-6 blocks that an upgraded node accepted
        // under the new rule, which is the silent fork the fence exists to prevent, arrived at from
        // inside one binary. Exactly one attempt id is a lane at any height.
        if !attempt_lane.admits_attempt_algo_id(header.pow_algo_id) {
            return Err(RuleError::UnknownPowAlgoId(header.pow_algo_id));
        }
        // MISAKA ADR-0038: structural shape rule for the post-PoW palw_commitment field.
        // The NON-PALW arm is not behind any fence — a hash-invisible non-empty field there is
        // block-hash malleability and is refused at the door on every network. The PALW arm is
        // fenced (Decision A): unset, the field must be empty exactly as before; set, it must be a
        // well-formed PBC1 commitment from the fence's DAA.
        let commitment_bound = self.palw_block_commitment.is_some_and(|fence| fence.is_bound(header.daa_score));
        // ADR-0072 SA-3: the admissible envelope version travels with the lane, so pre-fence
        // history validates under the old version and post-fence blocks under the new, in one
        // binary. `Unfenced` supplies the compiled-in version, which is every shipped preset.
        kaspa_consensus_core::pow_layer0::check_palw_commitment_shape_at(
            header.pow_algo_id,
            &header.palw_commitment,
            commitment_bound,
            attempt_lane,
        )
        .map_err(|e| RuleError::BadPalwCommitmentShape(e.to_string()))?;
        // The `palw_state_root` shape rule, on the `palw_commitment` pattern: the field is
        // hash-visible exactly on the lanes that commit state — the V2 lineage (6/7), and, since
        // ADR-0060, a heartbeat (algo-3) header on a `ConsensusV2` network. Everywhere else it is
        // hash-INVISIBLE, so a non-zero value there is block-hash malleability (two serialized
        // headers, one identity) and is refused at the door. This also closes the pre-ADR-0060
        // gap: nothing previously refused a stuffed root on an algo-1..5 header at all.
        let root_committing_lane = kaspa_consensus_core::pow_layer0::is_palw_v2_algo_id(header.pow_algo_id) || heartbeat_open;
        if !root_committing_lane && header.palw_state_root != kaspa_hashes::ZERO_HASH64 {
            return Err(RuleError::UncommittedPalwStateRoot(header.pow_algo_id));
        }
        self.check_palw_carriage_stateless(header, attempt_lane)
    }

    /// ADR-0042 Decision 6's stateless list (Unit A) and ADR-0044's (Unit B), at the header stage.
    ///
    /// The shape gate above proved the carriage DECODES; this proves it belongs to THIS header.
    /// The two cannot be separated in time: the finalizer expands the carriage into the tag, so a
    /// header whose carriage decoded but was never checked against its own position is a solved
    /// PoW that can be re-announced elsewhere — audit P0-1, arriving through the new lanes.
    ///
    /// **The SIGNATURE is verified here, on every PALW lane** — this paragraph used to say the
    /// opposite, and the arms below have disagreed with it since launch blockers §5. "Every lane"
    /// is spelled as the shared predicate `is_palw_attempt_algo_id` rather than as a list of
    /// constants, because the list is what went stale: ADR-0072 added a second attempt id and the
    /// arm below kept naming only the first, which put every post-fence attempt header back
    /// through the `_ => Ok(())` this doc was written about.
    ///
    /// The original reasoning was that a signature is only meaningful beside the stateful fact
    /// that the carried key IS the named bond's key, so it belonged in Unit C's admission and the
    /// ~1 ms of ML-DSA per header was not worth spending twice. That is true of what the signature
    /// PROVES and false about what its absence COSTS. The proof-of-work pre-image excludes
    /// `palw_commitment` while the block identity includes it, so an unverified signature is free
    /// bytes inside the identity and outside the work: one solved block becomes unbounded distinct
    /// blocks, each accepted, stored and relayed by every peer. The attempt lane was fixed; the
    /// receipt lane was written afterwards from the paragraph rather than from the code, and
    /// inherited the hole. Both are checked now, and the stateful admission checks the signature
    /// again beside the bond — the two answer different questions and neither replaces the other.
    fn check_palw_carriage_stateless(
        &self,
        header: &Header,
        attempt_lane: kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1,
    ) -> BlockProcessResult<()> {
        // **The SAME domain the acceptance layer verifies under** (re-audit R-8).
        //
        // Audit M2-18 bound the network domain to the genesis so a class-registration signature
        // names one incarnation, and moved every site in the virtual processor to
        // `palw_network_domain_v2_for(.., Some(genesis))` — and the producer signs with it. This
        // site, the STATELESS relay-path check, was missed and kept deriving the domain from the
        // network name alone. The two verifiers then answered different questions about the same
        // bytes: an attempt signed by the shipped producer failed here, so no PALW block ever
        // reached `StatusUTXOValid`. The header processor already holds the genesis it needs.
        let network_domain =
            kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(&self.network_id, Some(self.genesis.hash));
        palw_carriage_stateless_v1(header, attempt_lane, network_domain)
            .map_err(|reason| RuleError::BadPalwCarriageAdmission { algo_id: header.pow_algo_id, reason })
    }
}

/// [`HeaderProcessor::check_palw_carriage_stateless`]'s rule, as a function of the header, the lane
/// and the domain — the three things it actually reads.
///
/// Free rather than a method so the dispatch is REACHABLE BY A TEST. That is not tidiness: the
/// dispatch is what went wrong (ADR-0072 added a second attempt id and the arm kept naming only the
/// first, so every post-fence attempt header skipped both the envelope binding and the ML-DSA
/// signature check through the `_ => Ok(())` below), and a rule whose only spelling is inside a
/// method on a processor that needs six stores to construct is a rule nothing can hold to account.
/// `the_signature_is_checked_on_every_lane_the_shape_gate_demands_a_carriage_for` is what holds it
/// now, and it quantifies over the lanes rather than listing ids.
pub(crate) fn palw_carriage_stateless_v1(
    header: &Header,
    attempt_lane: kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1,
    network_domain: kaspa_hashes::Hash64,
) -> Result<(), String> {
    use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_RECEIPT_V3, is_palw_attempt_algo_id};
    let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    match header.pow_algo_id {
        // **EITHER attempt id** (ADR-0072 SA-4). Naming only algo-6 here is how the launch
        // blockers §5 hole comes back on the new lane: past the fence every attempt block
        // carries algo-9, and an arm that does not name it falls to the `_` below — no
        // envelope binding and, worse, no signature check, which is the unbounded-blocks-per-
        // solve attack this function's own doc describes. Which id is a lane at this height
        // is not asked again here; the gate above already refused the closed one by id.
        algo_id if is_palw_attempt_algo_id(algo_id) => {
            kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment)
                .map_err(|e| e.to_string())
                .and_then(|envelope| {
                    envelope
                            // **The LANE's version, not the compiled-in one** (SA-3). The fenced
                            // shape gate 45 lines above already took it; this call taking
                            // `PALW_ATTEMPT_V2_VERSION` instead made the fence a no-op on the
                            // relay path — the gate admitted a legacy envelope and this refused
                            // it, so an armed network could not validate its own pre-fence
                            // history at the header stage.
                            .validate_stateless_v2_at_version(
                                attempt_lane.attempt_version(),
                                network_domain,
                                pre_pow_hash,
                                header.timestamp,
                                header.nonce,
                            )
                            .map_err(|e| e.to_string())?;
                    // **The signature, on the RELAY path** (launch blockers §5).
                    //
                    // It was verified only on the chain walk, and the signature sits OUTSIDE
                    // `commitment_root_v2` (ADR-0042 Decision 3c, deliberately) while the block-identity
                    // digest hashes the raw carrier bytes. So anyone could take a solved block, write
                    // arbitrary bytes of the right length into `signature`, and mint an unbounded number
                    // of distinct blocks that every peer accepted, stored and relayed from ONE proof of
                    // work — a byte flip and a re-hash each. They never became chain, which is why the
                    // chain-walk check was thought sufficient; they did not need to, because the cost of
                    // making one was zero and the cost of carrying one was everybody else's.
                    //
                    // Checkable here because the attempt carries its OWN key: whether that key is the
                    // named bond's is admission item 2's stateful question, but whether the carrier
                    // authored this attempt with the key it claims needs no state at all.
                    envelope
                        .validate_signature_v2(|key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        })
                        .map_err(|e| e.to_string())
                })
        }
        POW_ALGO_ID_PALW_RECEIPT_V3 => {
            kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment)
                .map_err(|e| e.to_string())
                .and_then(|envelope| {
                    envelope
                        .validate_stateless_v3(network_domain, pre_pow_hash, header.timestamp, header.nonce)
                        .map_err(|e| e.to_string())?;
                    // **The same signature, on the same relay path, for the same reason** — the
                    // arm above learned this and this one was written without it.
                    //
                    // `validate_stateless_v3` checks the signature's LENGTH and recomputes the
                    // challenge; it never verifies the bytes. The challenge commits to
                    // `pre_pow_hash`, `timestamp` and `nonce` but not to the signature, and the
                    // signature is not inside the spend id it signs — so flipping one byte of it
                    // leaves the stateless check passing, the proof of work untouched, and the
                    // block a different block. One solve, unbounded distinct valid blocks, each
                    // one accepted, stored and relayed by every peer. That is verbatim the attack
                    // the attempt lane documents ten lines above, and the receipt lane inherited
                    // its shape without its fix.
                    envelope
                        .validate_signature_v3(|key, message, sig, context| {
                            kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
                        })
                        .map_err(|e| e.to_string())
                })
        }
        _ => Ok(()),
    }
}

impl HeaderProcessor {
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

    pub(super) fn check_pow_and_calc_block_level(&self, header: &Header) -> BlockProcessResult<BlockLevel> {
        // PR-8.6: kaspa-pq Layer 0 PoW (BLAKE2b-512, 512-bit target) replaces the
        // legacy 32-byte kHeavyHash check. `StateLayer0` wraps the Phase-1
        // (algo_id=1) kHeavyHash inner loop inside the domain-separated Layer 0
        // finalizer; the block level is derived from the 512-bit pow value
        // (ADR-0007 / ADR-0008).
        // **One implementation, called — not two a comment claims are identical.**
        //
        // This used to reproduce `kaspa_pow::calc_block_level_check_pow_layer0`'s error arms and
        // assert in a comment that the two "cannot drift". They had already drifted: the shared
        // function clamps an algo-7 receipt header to level 0 — "deriving a level from a free
        // digest would sell hierarchy position, the pruning-proof structure, for the price of one
        // signature" — and this copy called `calc_level_from_pow_512` unconditionally. The ORDINARY
        // block path runs this one, so the unclamped level is what reached the headers store, while
        // the pruning-proof and trusted-import paths used the clamped one. The test named for the
        // property asserts it through the shared function, which is not the path that stores it.
        //
        // Delegating is the fix rather than copying the missing arm, because the defect was never
        // the arm — it was that a duplicated invariant has no way to stay true.
        //
        // The shared function's parentless-root short-circuit is unreachable from here:
        // `check_parents_limit` rejects an empty parent set and runs before this on both callers
        // (`validate_header_in_isolation` and the ordinary `validate_header`).
        let (level, passed) = kaspa_pow::calc_block_level_check_pow_layer0(header, &self.network_id, self.max_block_level);
        if passed || self.skip_proof_of_work { Ok(level) } else { Err(RuleError::InvalidPoW) }
    }
}

#[cfg(test)]
mod palw_carriage_lane_tests {
    use super::palw_carriage_stateless_v1;
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1,
        challenge_v2,
    };
    use kaspa_consensus_core::pow_layer0::{
        POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, PalwAttemptLaneV1,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::{Hash64, ZERO_HASH64};

    const TS: u64 = 1_700_000;
    const NONCE: u64 = 42;

    fn domain() -> Hash64 {
        Hash64::from_u64_word(0xD0)
    }

    fn header_with(algo_id: u8, carriage: Vec<u8>) -> Header {
        let mut header = Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            ZERO_HASH64,
            ZERO_HASH64,
            ZERO_HASH64,
            TS,
            0x207fffff,
            NONCE,
            algo_id,
            0,
            BlueWorkType::from_u64(0),
            0,
            ZERO_HASH64,
        );
        header.palw_commitment = carriage;
        header.finalize();
        header
    }

    fn keypair() -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        static KP: std::sync::OnceLock<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair> = std::sync::OnceLock::new();
        KP.get_or_init(|| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xC1u8; 32]))
    }

    /// A carriage bound to `header`'s position and really signed, at `version`.
    fn signed_carriage(header: &Header, version: u16) -> Vec<u8> {
        let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0);
        let class_id = Hash64::from_u64_word(0xC5);
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let trace_root = Hash64::from_u64_word(0x7A);
        let attempt = PalwAttemptUnsignedV2 {
            version,
            network_domain: domain(),
            challenge: challenge_v2(domain(), pre_pow, header.timestamp, header.nonce, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: keypair().verification_key.as_ref().to_vec(),
            operator_id: Hash64::from_u64_word(0x0B),
            artifact_root: Hash64::from_u64_word(0xA7),
            trace_root,
            output_root: Hash64::from_u64_word(0x00),
            execution_root: Hash64::from_u64_word(0x4E),
            pwu: 7,
            trace_manifest_root: attempt_trace_manifest_root_v1(
                trace_root,
                kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS,
            ),
            trace_chunk_count: kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 1_000,
        };
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair().signing_key,
            attempt_id_v2(&attempt).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("ML-DSA-87 sign over a 64-byte attempt id")
        .as_ref()
        .to_vec();
        PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire()
    }

    /// **Every id that carries an attempt reaches a verifier — quantified over the lanes, not
    /// listed** (ADR-0072 SA-4, launch blockers §5).
    ///
    /// The dispatch was `POW_ALGO_ID_PALW_COMMITTED_V2 =>`, and ADR-0072's fence makes the open
    /// attempt id 9 past it. An algo-9 header therefore fell to `_ => return Ok(())`: no envelope
    /// binding and no signature check, on the relay path, for every block on the lane the fence
    /// opens. Listing ids is what went stale, so this asks the lane resolver which id each arm of
    /// the fence carries and requires that each one is checked.
    #[test]
    fn every_attempt_lane_id_reaches_the_stateless_carriage_check() {
        for lane in [PalwAttemptLaneV1::Unfenced, PalwAttemptLaneV1::LegacyArm, PalwAttemptLaneV1::ExecutionArm] {
            let algo_id = lane.attempt_algo_id();
            // No carriage at all is the cheapest thing a verifier must refuse. An arm that never
            // runs returns `Ok(())` here, which is the defect stated as an assertion.
            let bare = header_with(algo_id, Vec::new());
            assert!(
                palw_carriage_stateless_v1(&bare, lane, domain()).is_err(),
                "{lane:?} carries algo-{algo_id}; a header with no carriage must not pass the relay gate"
            );
        }
        // The negative side, so the test cannot pass by refusing everything: a non-PALW header has
        // no carriage to check and must still be admitted here.
        assert_eq!(
            palw_carriage_stateless_v1(&header_with(POW_ALGO_ID_KHEAVYHASH, Vec::new()), PalwAttemptLaneV1::Unfenced, domain()),
            Ok(())
        );
    }

    /// **One solved PoW may not become unbounded blocks on the new lane either.**
    ///
    /// The exact failure the finding describes: the signature is outside the Layer-0 digest and
    /// inside the block identity, so an unchecked signature is free bytes anyone can re-roll. With
    /// the dispatch naming only algo-6 this returned `Ok(())` for a tampered algo-9 carriage.
    #[test]
    fn a_tampered_signature_is_refused_on_both_attempt_ids() {
        for (lane, algo_id) in
            [(PalwAttemptLaneV1::Unfenced, POW_ALGO_ID_PALW_COMMITTED_V2), (PalwAttemptLaneV1::ExecutionArm, POW_ALGO_ID_PALW_EXEC_V3)]
        {
            let mut header = header_with(algo_id, Vec::new());
            header.palw_commitment = signed_carriage(&header, lane.attempt_version());
            header.finalize();
            assert_eq!(palw_carriage_stateless_v1(&header, lane, domain()), Ok(()), "the honest carriage on algo-{algo_id}");

            let mut envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).unwrap();
            envelope.signature[0] ^= 0x01;
            let mut tampered = header.clone();
            tampered.palw_commitment = envelope.encode_wire();
            tampered.finalize();
            assert_ne!(tampered.hash, header.hash, "a signature byte really is a different block identity");
            assert!(
                palw_carriage_stateless_v1(&tampered, lane, domain()).is_err(),
                "algo-{algo_id}: a flipped signature byte must not mint a second block from one solve"
            );
        }
    }

    /// **SA-3 binds on the relay path, not only at the shape gate.**
    ///
    /// The fenced shape check took the lane's version and the stateless check 45 lines later took
    /// the compiled-in one, so on an armed network below its fence the two disagreed about every
    /// block the chain already held: admitted by the gate, refused by the check. A fence nobody can
    /// sync under is not a fence.
    #[test]
    fn the_admissible_envelope_version_is_the_lanes_and_not_the_binarys() {
        let legacy = PalwAttemptLaneV1::LegacyArm;
        let mut header = header_with(legacy.attempt_algo_id(), Vec::new());
        header.palw_commitment = signed_carriage(&header, legacy.attempt_version());
        header.finalize();
        assert_eq!(
            palw_carriage_stateless_v1(&header, legacy, domain()),
            Ok(()),
            "below the fence the chain's own pre-ADR-0072 history must validate"
        );
        // And the same bytes at a position where the lane demands the current version are refused,
        // so the check is reading the lane rather than accepting anything.
        assert!(palw_carriage_stateless_v1(&header, PalwAttemptLaneV1::Unfenced, domain()).is_err());
    }
}
