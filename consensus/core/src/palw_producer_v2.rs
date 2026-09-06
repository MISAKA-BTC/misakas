//! **What a block producer must read from chain state, and it reads it derived** (ADR-0042).
//!
//! A `ConsensusV2` attempt is refused unless six of its fields equal values the chain already
//! holds: the class's registered artifact root, the class target the per-class retarget maintains,
//! the pwu `palw_pwu_v1` computes from that target, the bond's registered verification key, the
//! operator id minted at registration, and — as a bound rather than an equality — what the bond's
//! collateral still has room to back. A producer that computes any of them from a second source
//! computes them wrong the first time the chain moves.
//!
//! So it does not compute them. [`PalwProducerFactsV2`] is assembled by the same code paths
//! admission uses, at the same chain point a block template builds on, and handed over whole. The
//! producer's only remaining freedom is its EXECUTION — which is the freedom the design means by
//! "work".
//!
//! # Why this is not an RPC type
//!
//! It lives beside the state it is read from because the derivation is the contract. Exposing the
//! ingredients (a target here, a rule there) and letting a miner multiply them would hand every
//! miner an independent chance to disagree with admission — the same shape of defect the audit
//! found five times over between the engine, the profile, the inventory and the court, and the
//! reason ADR-0046 wrote down "derive, never declare".

use crate::BlockHash;
use crate::palw_admission_v2::PalwAdmissionParamsV2;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwPwuRuleV2, PalwStateParamsV2};
use kaspa_hashes::Hash64;

/// The bond half — read only when a producer names the bond it intends to sign under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwProducerBondFactsV2 {
    /// The ML-DSA-87 verification key the bond registered. Admission item 2 compares the carried
    /// key against this one, so a producer whose local key does not match it can be told at
    /// startup instead of after a block dies.
    pub registered_pubkey: Vec<u8>,
    /// Minted at registration from the operator key; admission item 3 is an equality.
    pub operator_id: Hash64,
    pub collateral: u64,
    /// What this bond already backs.
    pub reserved_exposure: u128,
    /// `collateral × max_exposure_ratio_permille / 1000` — admission item 8's ceiling.
    pub exposure_ceiling: u128,
    /// What ONE attempt at [`PalwProducerFactsV2::pwu`] would add to `reserved_exposure`.
    pub claim_exposure: u128,
}

impl PalwProducerBondFactsV2 {
    /// Is there ceiling left for one more claim? Admission item 8 is `reserved + claim <= ceiling`.
    pub fn has_exposure_room(&self) -> bool {
        self.reserved_exposure.saturating_add(self.claim_exposure) <= self.exposure_ceiling
    }
}

/// Everything a producer needs from the chain, at one chain point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwProducerFactsV2 {
    /// The block these facts were read at — virtual's selected parent, which is the point a block
    /// template builds on. A producer that sees this move knows its template is stale without
    /// having to guess from a timestamp.
    pub chain_point: BlockHash,
    pub daa_score: u64,
    pub class_id: Hash64,
    /// Admission item 5: equality against the attempt's.
    pub artifact_root: Hash64,
    /// Admission item 6b: the attempt's `class_ticket_v3` (drawn from its execution under the
    /// header's anchor, ADR-0072) must land at or under this.
    pub class_target: u128,
    /// Admission item 6: an EQUALITY, and both factors are chain state. Derived here so the
    /// producer cannot pick.
    pub pwu: u64,
    /// **Is this class seated on the free-prompt lane** (ADR-0075 `ClassLaneCertified`, genesis
    /// set ∪ chain set)? What `FreePromptCommitted` refuses as `FreePromptLaneUncertified`, read
    /// off the same two sets the transition reads, so a gateway (ADR-0077 Decision 3) learns
    /// before it commits whether its class can take a free-prompt claim at all. A params set
    /// with no certified-class gate (the ungated test bundles) reads as certified.
    pub fp_certified: bool,
    pub epoch_index: u64,
    /// Admission item 7: blocks of this class this epoch may not exceed this.
    pub epoch_budget_blocks: u64,
    pub epoch_produced_blocks: u64,
    /// Is this the liveness floor? **The floor is EXEMPT from the epoch budget** — admission says so
    /// at `palw_admission_v2.rs:234`, and the exemption is what makes ADR-0039 W6′'s deadlock
    /// unrepresentable: DAA only advances when blocks are produced, so a floor that could be capped
    /// could stop the chain and then never reach the epoch that would uncap it.
    ///
    /// Carried because a producer that applied the cap anyway would re-create that deadlock on the
    /// CLIENT side — refusing to build a block the chain would have accepted. It did: the budget
    /// table is written for the TIP's epoch and looked up for the CANDIDATE's, so at every epoch
    /// boundary the lookup missed, `unwrap_or(0)` made the budget zero, and the producer held
    /// forever.
    pub is_base_class: bool,
    /// How long a producer must promise to keep its trace: [`palw_min_trace_retention_daa_v1`].
    /// The attempt's `trace_retention_daa` MUST be the block's own DAA score plus this — admission
    /// pins it by equality (ADR-0072 Decision 8), so it is not the producer's to get right but the
    /// chain's, and this is where a producer reads it.
    pub min_trace_retention_daa: u64,
    pub bond: Option<PalwProducerBondFactsV2>,

    /// **The chain's PALW weight, and how many claims are still unresolved** — the two numbers that
    /// say whether this network is doing PALW at all.
    ///
    /// `safe_weight` is what fork choice orders by. It leaves zero only when a claim reaches
    /// `Final`, which needs a panel, receipts, a quorum and a submitted `ReceiptLicensed` — the
    /// whole lattice. A network producing blocks with `safe_weight == 0` is indistinguishable from
    /// a hash chain wearing PALW's clothes, and until this field existed there was no way to see
    /// that from outside a debugger: nothing logged it, no RPC returned it, and a fleet could run
    /// for a day looking healthy while every claim it ever made was quietly voiding.
    pub safe_weight: u128,
    /// Claims created and not yet resolved. Rising without bound while `safe_weight` stays zero is
    /// the signature of a lattice that never turns over.
    pub unresolved_claims: u64,
    /// **How many disputes are open right now.** Not decoration: a network whose challengers are
    /// working and whose responders are not looks, from every other number here, exactly like a
    /// network with nothing to dispute. Two drills were spent reading "the responder made no move"
    /// as a responder bug when the sessions may not have existed at all — this is the number that
    /// tells those apart from the log an operator already watches.
    pub open_courts: u64,
    /// Claims that have reached `Final` — the count of work this chain has actually certified.
    pub final_claims: u64,
    /// `safe_weight` plus the bounded immature contribution — the THIRD key of the fork-choice
    /// order (`palw_fork_choice::PalwCandidateOrderV1`), and the only one that can move on a
    /// young chain.
    ///
    /// A claim cannot finalize before `window_challenge` has passed, so `safe_weight` and the
    /// safe frontier are both zero for the whole first stretch of a network's life — at the
    /// frozen 120 s cadence, more than a day. That is by construction, not a fault, and this is
    /// how an operator tells the two apart: `live_total` climbing while `safe_weight` sits at
    /// zero is a chain ordering on immature PALW work exactly as designed, whereas both at zero
    /// is a chain whose lifecycle never started.
    pub live_total: u128,
}

impl PalwProducerFactsV2 {
    /// Is the epoch budget spent? A producer that keeps mining past it produces blocks admission
    /// refuses — burning an inference each time and learning nothing.
    pub fn has_epoch_room(&self) -> bool {
        // The floor is exempt, exactly as admission exempts it. See `is_base_class`.
        self.is_base_class || self.epoch_produced_blocks < self.epoch_budget_blocks
    }

    /// Every stateful precondition a producer can check BEFORE running an inference, in one
    /// answer. `Ok(())` is not a promise the block lands — the chain can move underneath it —
    /// but each `Err` is a reason it certainly would not have.
    pub fn ready_to_produce(&self, local_pubkey: &[u8]) -> Result<(), &'static str> {
        let bond = self.bond.as_ref().ok_or("the named bond is not registered on this chain")?;
        if bond.registered_pubkey != local_pubkey {
            return Err("the local signing key is not the one this bond registered");
        }
        if !self.has_epoch_room() {
            return Err("this class's epoch budget is already spent");
        }
        if !bond.has_exposure_room() {
            return Err("the bond's exposure ceiling leaves no room for another claim");
        }
        Ok(())
    }
}

/// Read the facts for `class_id` (and optionally a bond) out of a state snapshot.
///
/// `daa_score` is the candidate's, because the epoch index admission uses is the CANDIDATE's, not
/// the tip's — a producer handed the tip's epoch at an epoch boundary would check its budget
/// against the wrong epoch and mine into a refusal.
/// **The retention a producer owes, and the only one admission accepts** (ADR-0072 Decision 8):
/// the four lattice windows a claim can be asked inside — bind, receipt, challenge, court. A
/// promise shorter than this discards the evidence before anyone can ask for it; a promise longer
/// was harmless and free to change, which made `trace_retention_daa` a draw. One spelling, read
/// by the facts a producer builds from and by the pin admission checks them against.
pub fn palw_min_trace_retention_daa_v1(state_params: &PalwStateParamsV2) -> u64 {
    state_params
        .window_bind()
        .saturating_add(state_params.window_receipt())
        .saturating_add(state_params.window_challenge())
        .saturating_add(state_params.window_court())
}

pub fn palw_producer_facts_v2(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    chain_point: BlockHash,
    daa_score: u64,
    class_id: Hash64,
    bond: Option<&PalwBondKeyV2>,
) -> Option<PalwProducerFactsV2> {
    let class = state.class(&class_id)?;
    let class_target = state.class_target(&class_id)?.target;
    let pwu = match class.pwu_rule {
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => crate::palw_pwu::palw_pwu_v1(class_target, pwu_per_inference),
        PalwPwuRuleV2::MaxPerAttempt(cap) => cap,
    };
    let epoch_index = daa_score / state_params.epoch_length();
    let epoch_budget_blocks = state
        .epoch_budgets()
        .filter(|b| b.epoch_index == epoch_index)
        .and_then(|b| b.budget_blocks.get(&class_id).copied())
        .unwrap_or(0);
    let epoch_produced_blocks = match state.epoch_counter(&class_id) {
        Some(counter) if counter.epoch_index == epoch_index => counter.produced_blocks,
        _ => 0,
    };
    let bond = bond.and_then(|key| {
        let bond_state = state.bond(key)?;
        Some(PalwProducerBondFactsV2 {
            registered_pubkey: bond_state.pubkey.clone(),
            operator_id: bond_state.operator_id,
            collateral: bond_state.collateral,
            reserved_exposure: state.reserved_exposure(key),
            exposure_ceiling: (bond_state.collateral as u128).saturating_mul(admission.max_exposure_ratio_permille() as u128) / 1000,
            // The SAME derivation admission applies, or the producer's own headroom prediction
            // disagrees with the rule that refuses it.
            claim_exposure: (crate::palw_state_v2::palw_exposure_pwu_v1(class, pwu) as u128)
                .saturating_mul(class.slash_value_per_pwu as u128),
        })
    });
    Some(PalwProducerFactsV2 {
        is_base_class: class_id == state_params.base_class_id(),
        fp_certified: state_params.fp_certified_classes().is_none_or(|set| set.contains(&class_id))
            || state.fp_lane_certification(&class_id).is_some(),
        min_trace_retention_daa: palw_min_trace_retention_daa_v1(state_params),
        chain_point,
        daa_score,
        class_id,
        artifact_root: class.artifact_root,
        class_target,
        pwu,
        epoch_index,
        epoch_budget_blocks,
        epoch_produced_blocks,
        bond,
        safe_weight: state.safe_weight(),
        live_total: state.safe_weight().saturating_add(state.bounded_immature()),
        unresolved_claims: state.claims_iter().filter(|(_, c)| !c.phase.is_terminal()).count() as u64,
        open_courts: state.court_sessions_len() as u64,
        final_claims: state
            .claims_iter()
            .filter(|(_, c)| matches!(c.phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }))
            .count() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_admission_v2::check_palw_attempt_admission_v2;
    use crate::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    use crate::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2, apply_palw_transition_v2, palw_operator_id_v2};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const NET: u64 = 0x4E45_5457;

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(500, 100, 100, 100, 100, 1_000, h64(1), 4, 1_000, 1_000, 100, 100).unwrap()
    }

    fn bond_outpoint() -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }
    }

    /// One class on the DERIVED pwu rule — because a `MaxPerAttempt` class would let a producer
    /// guess the pwu and still be admitted, which is precisely the case this contract is not for.
    fn state() -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
                initial_target: u128::MAX / 4,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint()),
                pubkey: vec![7; 4],
                operator_pubkey: vec![0x21; 8],
                collateral: 1_000_000,
                payout_payload: h64(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(1), daa_score: 100, blue_score: 1, subsidy: 0 };
        apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx, &objects, None).unwrap().0
    }

    /// **Build an attempt from NOTHING but the facts, and see whether the chain takes it.**
    ///
    /// This is the round trip the audit kept finding defects with: two sides that were reviewed
    /// separately and asked to agree only here. Every field below is either the producer's own
    /// (its execution, its keys) or copied straight out of `facts` — nothing is re-derived, so if
    /// the contract were missing a fact the attempt could not be built at all, and if a fact were
    /// derived differently from admission's the attempt would be refused.
    #[test]
    fn an_attempt_built_only_from_the_facts_is_admitted() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let facts =
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), 101, h64(1), Some(&bond_key))
                .expect("the class is registered, so it has facts");

        assert_eq!(facts.ready_to_produce(&[7; 4]), Ok(()), "the producer is clear to run an inference");
        assert_eq!(
            facts.pwu,
            crate::palw_pwu::palw_pwu_v1(facts.class_target, 7),
            "the pwu is the derivation, handed over rather than left to be recomputed"
        );

        let mut env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(NET),
                challenge: challenge_v2(h64(NET), h64(0x5050_4800), 7, 1, facts.class_id, &bond_outpoint()),
                class_id: facts.class_id,
                executor_bond: bond_outpoint(),
                executor_pubkey: facts.bond.as_ref().unwrap().registered_pubkey.clone(),
                operator_id: facts.bond.as_ref().unwrap().operator_id,
                artifact_root: facts.artifact_root,
                // The producer's own: what its execution produced.
                trace_root: h64(31),
                output_root: h64(32),
                execution_root: h64(41),
                pwu: facts.pwu,
                trace_manifest_root: crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(31), 1),
                trace_chunk_count: 1,
                trace_retention_daa: 999_999,
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        // The class lottery, run the way a producer runs it — over its own execution, under the
        // anchor the header derives (ADR-0072).
        let anchor = execution_anchor_v3(h64(NET), h64(0x5050_4800), facts.class_id, &bond_outpoint(), 1);
        let mut won = false;
        for n in 0u64..100_000 {
            env.attempt.trace_root = h64(0x3100_0000_0000_0000u64.wrapping_add(n));
            if class_ticket_v3(&env.attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "a quarter-of-the-space target is winnable in 1e5 tries");

        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(2), daa_score: 101, blue_score: 2, subsidy: 0 };
        check_palw_attempt_admission_v2(&state, &params, &admission, &ctx, &env, false).expect("the chain takes it");
        crate::palw_admission_v2::check_palw_class_lottery_v3(&state, &env.attempt, anchor).expect("and its draw wins");
    }

    /// **Every fact is load-bearing.** Move one and admission refuses — which is what makes this a
    /// contract rather than a convenience: a producer that sourced any of them elsewhere would be
    /// sourcing the thing that decides whether its block exists.
    #[test]
    fn moving_any_single_fact_is_refused() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let facts =
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), 101, h64(1), Some(&bond_key))
                .unwrap();
        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(2), daa_score: 101, blue_score: 2, subsidy: 0 };

        let build = |mutate: &dyn Fn(&mut PalwAttemptUnsignedV2)| {
            let mut env = PalwAttemptEnvelopeV2 {
                attempt: PalwAttemptUnsignedV2 {
                    version: PALW_ATTEMPT_V2_VERSION,
                    network_domain: h64(NET),
                    challenge: challenge_v2(h64(NET), h64(0x5050_4800), 7, 1, facts.class_id, &bond_outpoint()),
                    class_id: facts.class_id,
                    executor_bond: bond_outpoint(),
                    executor_pubkey: facts.bond.as_ref().unwrap().registered_pubkey.clone(),
                    operator_id: facts.bond.as_ref().unwrap().operator_id,
                    artifact_root: facts.artifact_root,
                    trace_root: h64(31),
                    output_root: h64(32),
                    execution_root: h64(41),
                    pwu: facts.pwu,
                    trace_manifest_root: crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(31), 1),
                    trace_chunk_count: 1,
                    trace_retention_daa: 999_999,
                },
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            };
            let anchor = execution_anchor_v3(h64(NET), h64(0x5050_4800), facts.class_id, &bond_outpoint(), 1);
            for n in 0u64..100_000 {
                env.attempt.trace_root = h64(0x3100_0000_0000_0000u64.wrapping_add(n));
                mutate(&mut env.attempt);
                if class_ticket_v3(&env.attempt, anchor) <= facts.class_target {
                    break;
                }
            }
            env
        };

        // The artifact root, the pwu, and the key: one each, and each refused for its own reason.
        for (name, mutate) in [
            ("artifact root", &(|a: &mut PalwAttemptUnsignedV2| a.artifact_root = h64(0xBAD)) as &dyn Fn(&mut _)),
            ("pwu", &(|a: &mut PalwAttemptUnsignedV2| a.pwu = a.pwu.wrapping_add(1)) as &dyn Fn(&mut _)),
            ("executor key", &(|a: &mut PalwAttemptUnsignedV2| a.executor_pubkey = vec![9; 4]) as &dyn Fn(&mut _)),
            ("operator id", &(|a: &mut PalwAttemptUnsignedV2| a.operator_id = palw_operator_id_v2(&[0xEE; 8])) as &dyn Fn(&mut _)),
        ] {
            let env = build(mutate);
            assert!(
                check_palw_attempt_admission_v2(&state, &params, &admission, &ctx, &env, false).is_err(),
                "a producer that got the {name} from anywhere but the facts is a producer with no blocks"
            );
        }
    }

    /// **The floor is exempt from the epoch budget, and the producer must agree with admission.**
    ///
    /// The budget table is written for the TIP's epoch and read for the CANDIDATE's, so at every
    /// epoch boundary the lookup misses and `unwrap_or(0)` makes the budget zero. Admission does not
    /// care — it exempts the floor (`palw_admission_v2.rs:234`) precisely so the ADR-0039 W6′
    /// deadlock is unrepresentable — but the producer applied the cap anyway and held forever,
    /// re-creating on the client side the chain-stopping deadlock `58291251` removed from consensus.
    ///
    /// This is the whole failure in one assertion: a floor with a ZERO budget is still producible.
    #[test]
    fn the_liveness_floor_is_never_capped_by_an_epoch_budget() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());

        // An epoch the chain has written no budget for — every epoch boundary, in other words.
        let far = params.epoch_length() * 9_999 + 1;
        let facts =
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), far, h64(1), Some(&bond_key))
                .expect("the class is still registered");
        assert!(facts.is_base_class, "h64(1) is this fixture's floor");
        assert_eq!(facts.epoch_budget_blocks, 0, "and the chain has written no budget for this epoch");
        assert!(facts.has_epoch_room(), "a zero budget must not stop the floor — that is the deadlock");
        assert_eq!(facts.ready_to_produce(&[7; 4]), Ok(()), "so the producer builds the epoch's first block");

        // A NON-floor class is still capped, because the cap is what Decision 2 is for. Nothing was
        // loosened; the exemption is exactly the one admission already makes.
        let entrant =
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), far, h64(2), Some(&bond_key));
        assert!(entrant.is_none(), "this fixture registers no entrant; the floor is the only class");
    }

    /// The pre-flight answers are the ones admission would give, not a second opinion: an    /// The pre-flight answers are the ones admission would give, not a second opinion: an
    /// unregistered bond has no facts to be ready with.
    #[test]
    fn a_bond_the_chain_does_not_know_is_not_ready() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let stranger = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xDEAD), index: 0 });
        let facts =
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), 101, h64(1), Some(&stranger))
                .unwrap();
        assert!(facts.bond.is_none());
        assert_eq!(facts.ready_to_produce(&[7; 4]), Err("the named bond is not registered on this chain"));
        // And a class the chain does not know has no facts at all — there is nothing to be told.
        assert!(
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), 101, h64(0xBAD), None).is_none()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The panel's half of the contract
// ---------------------------------------------------------------------------------------------

/// One seat duty this node holds: a claim whose panel names a bond it can sign for.
///
/// The claim's committed roots ride along, because that is what a seat DECIDES against — a seat
/// that had to fetch them separately could be handed a different pair than the chain holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatDutyV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub claim_id: Hash64,
    /// **Which class this claim is of** — and therefore which graph a seat re-executes.
    /// Carried rather than looked up: a seat that resolved the class separately could verify
    /// material against a class the chain does not say the claim is of. It comes off the claim
    /// record, so it is the chain's answer and not the seat's.
    pub class_id: Hash64,
    /// The class's registered artifact root, for the same reason the producer is handed one: the
    /// seat must hold the SAME weights, and "the same" is this value.
    pub artifact_root: Hash64,
    pub seat_bond: PalwBondKeyV2,
    /// The producer whose material this seat must judge. Never this seat: `derive_panel_v2`
    /// excludes a claim's own executor by bond, by operator and by key.
    pub executor_bond: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The claim's committed `output_root` (ADR-0084 Decision 1): what a seat binds a served
    /// answer envelope's ids to before it recomputes anything from them. Read off the claim
    /// record beside the two roots above, for the same reason they ride — a seat that fetched it
    /// separately could be handed a different value than the chain holds.
    pub output_root: Hash64,
    /// When the panel was bound. The receipt window runs from here, and a receipt signed outside
    /// it is refused by `validate_receipt_quorum_v2`.
    pub bound_daa: u64,
    /// The last DAA at which a receipt for this claim still counts.
    pub receipt_deadline: u64,
    /// **The beacon the panel was drawn from** — the block at the claim's anchor slot, which did
    /// not exist when the commitment was fixed (ADR-0044 F4/F5). A free-prompt seat draws the
    /// checkpoint intervals it must open from this and its own seat index
    /// (`palw_fp_interval_draw_v1`, ADR-0077 Decision 8), so the executor cannot know which
    /// intervals will be checked when it commits.
    pub panel_anchor: Hash64,
    /// This seat's position in the bound panel — the second input to the interval draw, so two
    /// seats of one panel open different intervals.
    pub seat_index: u8,
    /// **What the chain PRICED this claim at** — the seat's only handle on "is the material I was
    /// served the work this claim was paid for".
    ///
    /// The roots alone cannot answer that. A commitment's `execution_root` is carried verbatim
    /// from its payload and related to nothing the chain can recompute (the chain has no leg
    /// roots), while its `cu`/`quanta`/`pwu` are derived from the job shape the payload DECLARES.
    /// So a producer may declare a hundred-thousand-token job, serve a one-token material whose
    /// roots are genuinely that material's, and a seat comparing only roots certifies it — block
    /// work bought with recycled collateral instead of inference, which is the one property this
    /// lane exists to establish. A seat re-prices what it actually executed and compares against
    /// these, which is the check the chain cannot make for it.
    pub pwu: u64,
    /// Quanta the claim was opened for; `0` on the attempt lane, which has none.
    pub quanta: u32,
    /// **Which lane this claim's material speaks** — and therefore how a seat verifies it.
    ///
    /// An attempt claim's material is the run's own rows; the seat re-hashes them under the job
    /// the ANCHOR implies. A free-prompt claim's job is the CALLER's, underivable from any
    /// anchor: its material is the job itself ([`crate::palw_freeprompt_v3::PalwFpMaterialV1`])
    /// and the seat re-executes it — the replay `PublicDa` was named for. A seat that fed one
    /// lane's material to the other lane's verifier would file `Unavailable` against every
    /// honest free-prompt executor, and a quorum of those DEFAULTS the producer — the panel
    /// would convict the lane's every user for using it.
    pub free_prompt: bool,
    /// The leaf count the free-prompt claim was priced from (ADR-0074 Decision 5): a seat's one
    /// pricing check is that the capture it authenticated has exactly this many leaves. Zero on
    /// an attempt.
    pub work_leaves: u64,
}

/// **Every seat duty this node holds at one chain point** (launch blockers §2).
///
/// Nothing in the tree ever filed a `ReceiptLicensed`, so no claim reached `Final`: every panel
/// voided at `ReceiptTimeout` with all its seats slashed, `safe_weight` stayed zero, and the
/// escrowed worker carve of every block was burned. A seat cannot act on a duty it cannot see, and
/// this is where it sees them.
///
/// `mine` is the set of bonds this node can sign for. Derived from the state the chain holds rather
/// than assembled by the caller, for the same reason `palw_producer_facts_v2` is: a seat that
/// computed its own deadline would eventually disagree with the quorum check about it.
/// **A claim a challenger could still dispute** — licensed, not yet final, and with no session of
/// this bond's already open against it.
///
/// The court had no opener either: `CourtOpened` was constructed nowhere, so the only disputes on
/// any chain were the ones a test wrote by hand. Deciding WHETHER to dispute is the challenger's
/// (it costs the claim's own stake now), but finding the claims it could is a question about state,
/// and belongs here beside the seat and court duty lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDisputableClaimV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    pub licensed_daa: u64,
    /// The claim is a free-prompt commitment (ADR-0073 Decision 1d): its job is the USER's, fixed
    /// on chain as `fp_job_id_v3(job)` with a hash-bound prompt, so a challenger re-executes THAT
    /// job — never `job_for_anchor`, whose answer is a job nobody asked and whose roots differ
    /// from every honest free-prompt claim's.
    pub free_prompt: bool,
}

pub fn palw_disputable_claims_v2(state: &PalwChainStateV2, mine: &[PalwBondKeyV2]) -> Vec<PalwDisputableClaimV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        let crate::palw_state_v2::PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = claim.phase else { continue };
        // Never our own work: `derive_panel_v2` excludes the executor from its own panel for the
        // same reason, and `validate_court_opened_v2` refuses a self-challenge outright.
        if mine.contains(&claim.bond) {
            continue;
        }
        // One session per (claim, challenger) — the id is derived from both, so a second open
        // would collide rather than stack.
        if state.court_sessions_iter().any(|(_, s)| s.claim == *claim_id && mine.contains(&s.challenger_bond)) {
            continue;
        }
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        out.push(PalwDisputableClaimV2 {
            accepted_block: claim.accepted_block,
            claim_id: *claim_id,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            licensed_daa,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
        });
    }
    out
}

/// **What a party owes in a court session it is a party to** — the court's half of
/// [`palw_seat_duties_v2`].
///
/// Nothing in this tree constructed a `CourtDisclosed`, so a dispute could be opened and never
/// answered. That was not a missing feature so much as a missing QUESTION: the ladder knows whose
/// turn it is and what interval is open, and no code ever asked it on behalf of a node that holds
/// one of the two bonds. This is the asking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCourtDutyV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub session_id: Hash64,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The bond the claim was produced under — the RESPONDER, who discloses.
    pub executor_bond: PalwBondKeyV2,
    /// The bond that opened the session — the CHALLENGER, who posts verdicts.
    pub challenger_bond: PalwBondKeyV2,
    /// Which of the two this node is. Both is possible only in a self-challenge, which
    /// `validate_court_opened_v2` refuses, so exactly one side is ours.
    pub i_am_responder: bool,
    pub round: u32,
    /// The open interval, `[lo, hi)`.
    pub interval: (u64, u64),
    /// The index a disclosure must answer about, when it is our turn to disclose.
    pub midpoint: Option<u64>,
    /// `Some(index)` once the ladder has narrowed to one step — the index a close must adjudicate.
    pub terminal_index: Option<u64>,
    /// The rung the responder last answered, `(midpoint, disclosed state)` — what a challenger's
    /// verdict is a comparison against.
    pub last_disclosure: Option<(u64, Hash64)>,
    /// Whose move it is, so a caller does not have to re-derive the turn from the interval.
    pub turn: crate::palw_bisect::PalwBisectTurnV1,
    pub rung_deadline_daa: u64,
    pub session_deadline_daa: u64,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    /// The disputed claim is a free-prompt commitment (ADR-0073 Decision 1b). Its anchor is not
    /// derived from the accepted block — the user set the question — but read as
    /// `fp_job_id_v3(job)` off the claim's job material, and the prover is handed the user's
    /// prompt rather than deriving one. `PalwSeatDutyV2` carries the same bit for the same reason.
    pub free_prompt: bool,
}

/// Every open session in which `mine` holds the executor's bond or the challenger's.
pub fn palw_court_duties_v2(state: &PalwChainStateV2, mine: &[PalwBondKeyV2]) -> Vec<PalwCourtDutyV2> {
    let mut out = Vec::new();
    for (session_id, session) in state.court_sessions_iter() {
        let Some(claim) = state.claim(&session.claim) else { continue };
        let i_am_responder = mine.contains(&claim.bond);
        let i_am_challenger = mine.contains(&session.challenger_bond);
        if !i_am_responder && !i_am_challenger {
            continue;
        }
        // A claim whose class has left the registry cannot be adjudicated by anyone, so it yields
        // no duty rather than a duty nobody can discharge — the same rule the seat list uses.
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        let (lo, hi) = session.ladder.interval();
        // **Whose move it is, through the SAME helper the fold and the deadline index use**
        // (ADR-0082 Decision 2; mainnet audit 2026-09-06, H-5 item d). This view read the LADDER's
        // turn and the LADDER's rung deadline unconditionally, and `session.dissection` was never
        // looked at — so a session with an open phase reported `Terminal` to the responder that
        // owed the phase's next round, and a fused terminal reported `Terminal` to a responder the
        // chain was already clocking as `AwaitDisclosure`. Every court arm in the panel switches on
        // `duty.turn`, so this is the second half of the missing responder: it would misroute even
        // a correct one.
        let (turn, rung_deadline_daa) = crate::palw_state_v2::court_turn_and_rung_deadline_v2(
            session,
            crate::palw_state_v2::court_session_class_is_fused_v2(state, session),
        );
        // The round is the PHASE's once one is open, for the same reason the turn is.
        let round = session.dissection.as_ref().map_or_else(|| session.ladder.round(), |phase| phase.round());
        out.push(PalwCourtDutyV2 {
            accepted_block: claim.accepted_block,
            session_id: *session_id,
            claim_id: session.claim,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            challenger_bond: session.challenger_bond,
            i_am_responder,
            round,
            interval: (lo, hi),
            midpoint: (hi.saturating_sub(lo) > 1).then(|| lo + (hi - lo) / 2),
            terminal_index: session.ladder.terminal_index(),
            last_disclosure: session.ladder.last_disclosure(),
            turn,
            rung_deadline_daa,
            session_deadline_daa: session.deadline_daa,
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
        });
    }
    out
}

/// **A data-availability accusation this node must answer** (ADR-0062 D3; mainnet audit
/// 2026-09-05): a claim it produced is under an open accusation, and the event named must be
/// opened out of the capture it retains before `disclose_deadline_daa`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaDutyV2 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The bond the claim was produced under — the one that must sign the disclosure.
    pub executor_bond: PalwBondKeyV2,
    /// The packed `(row, tile)` the accusation names — `palw_da_event_index_parts_v1` unpacks it.
    pub missing_event_index: u32,
    pub accused_daa: u64,
    /// `accused_daa + W_disclose`: silence past it confirms the default (SA-3/SA-5).
    pub disclose_deadline_daa: u64,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    pub free_prompt: bool,
}

/// The claims under accusation whose producing bond is in `mine`.
pub fn palw_da_duties_v2(state: &PalwChainStateV2, state_params: &PalwStateParamsV2, mine: &[PalwBondKeyV2]) -> Vec<PalwDaDutyV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        let crate::palw_state_v2::PalwClaimPhaseV2::DefaultDisputed { accused_daa, missing_event_index, .. } = claim.phase else {
            continue;
        };
        if !mine.contains(&claim.bond) {
            continue;
        }
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        out.push(PalwDaDutyV2 {
            claim_id: *claim_id,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            missing_event_index,
            accused_daa,
            disclose_deadline_daa: accused_daa.saturating_add(crate::palw_state_v2::palw_da_disclose_window_daa_v1(state_params)),
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
        });
    }
    out
}

pub fn palw_seat_duties_v2(state: &PalwChainStateV2, state_params: &PalwStateParamsV2, mine: &[PalwBondKeyV2]) -> Vec<PalwSeatDutyV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        // Only a bound panel owes receipts; every other phase is somebody else's edge.
        let crate::palw_state_v2::PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
            continue;
        };
        let Some(panel) = state.panel(claim_id) else { continue };
        // The class's registered root, read where the claim is read. A claim whose class is gone
        // from the registry is not judgeable by anyone, so it yields no duty rather than a duty
        // nobody can act on.
        let Some(class_artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else {
            continue;
        };
        for (seat_index, seat) in panel.seats.iter().enumerate() {
            if !mine.contains(&seat.bond) {
                continue;
            }
            out.push(PalwSeatDutyV2 {
                panel_anchor: panel.anchor,
                seat_index: seat_index as u8,
                accepted_block: claim.accepted_block,
                claim_id: *claim_id,
                class_id: claim.class_id,
                artifact_root: class_artifact_root,
                seat_bond: seat.bond,
                executor_bond: claim.bond,
                execution_root: claim.execution_root,
                trace_root: claim.trace_root,
                output_root: claim.output_root,
                bound_daa,
                receipt_deadline: bound_daa.saturating_add(state_params.window_receipt()),
                pwu: claim.pwu,
                quanta: match claim.source {
                    crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => quanta,
                    _ => 0,
                },
                free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
                work_leaves: claim.work_leaves,
            });
        }
    }
    out
}
