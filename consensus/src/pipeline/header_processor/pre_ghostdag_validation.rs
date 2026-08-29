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
        // Accepts, not demands: a V2 network admits its receipt lane as well as its attempt
        // lane, and asking only for the demanded id refused every block on the first one.
        kaspa_consensus_core::pow_layer0::check_algo_id_for_mode_accepting(
            header.pow_algo_id,
            self.palw_required_algo_id,
            self.palw_consensus_mode.accepts_algo_id(header.pow_algo_id),
            palw_ollama_active,
            palw_active,
            blake2b_sha3_active,
        )
        .map_err(|_| RuleError::UnknownPowAlgoId(header.pow_algo_id))?;
        // MISAKA ADR-0038: structural shape rule for the post-PoW palw_commitment field.
        // The NON-PALW arm is not behind any fence — a hash-invisible non-empty field there is
        // block-hash malleability and is refused at the door on every network. The PALW arm is
        // fenced (Decision A): unset, the field must be empty exactly as before; set, it must be a
        // well-formed PBC1 commitment from the fence's DAA.
        let commitment_bound = self.palw_block_commitment.is_some_and(|fence| fence.is_bound(header.daa_score));
        kaspa_consensus_core::pow_layer0::check_palw_commitment_shape(header.pow_algo_id, &header.palw_commitment, commitment_bound)
            .map_err(|e| RuleError::BadPalwCommitmentShape(e.to_string()))?;
        self.check_palw_carriage_stateless(header)
    }

    /// ADR-0042 Decision 6's stateless list (Unit A) and ADR-0044's (Unit B), at the header stage.
    ///
    /// The shape gate above proved the carriage DECODES; this proves it belongs to THIS header.
    /// The two cannot be separated in time: the finalizer expands the carriage into the tag, so a
    /// header whose carriage decoded but was never checked against its own position is a solved
    /// PoW that can be re-announced elsewhere — audit P0-1, arriving through the new lanes.
    ///
    /// **The SIGNATURE is verified here, on both PALW lanes** — this paragraph used to say the
    /// opposite, and the arms below have disagreed with it since launch blockers §5.
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
    fn check_palw_carriage_stateless(&self, header: &Header) -> BlockProcessResult<()> {
        use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};
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
        let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let reason = match header.pow_algo_id {
            POW_ALGO_ID_PALW_COMMITTED_V2 => {
                kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment)
                    .map_err(|e| e.to_string())
                    .and_then(|envelope| {
                        envelope
                            .validate_stateless_v2(network_domain, pre_pow_hash, header.timestamp, header.nonce)
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
            _ => return Ok(()),
        };
        reason.map_err(|reason| RuleError::BadPalwCarriageAdmission { algo_id: header.pow_algo_id, reason })
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
