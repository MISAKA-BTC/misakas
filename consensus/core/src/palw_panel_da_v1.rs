//! **`PanelDa` — a prompt that is not published** (ADR-0077 Decision 16, order-of-work P-16).
//!
//! Privacy mode 2. The job carries `prompt_token_ids_hash` as it always did; the commitment
//! transaction carries NO ids. The ids travel with the capture the executor already serves its
//! panel (`<claim>.material`, `request_palw_material`), a seat checks `H(ids) == hash` before it
//! reads anything else, and a court close that addresses a gather carries the ids as it does now
//! — so a disputed prompt becomes public.
//!
//! # Where the rules actually live
//!
//! This module is the mode's doorway, not a second copy of it. The rules are enforced where the
//! objects are:
//!
//! * the arming — `Params::palw_panel_da`, dormant on every shipped preset, with the height-free
//!   `palw_panel_da_admissible` for the transaction door and `palw_panel_da_at` for the walk;
//! * the payload rule — [`crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3`] refuses a
//!   mode-2 payload that carries ids, by name
//!   ([`crate::palw_freeprompt_v3::PalwFpV3Error::PanelDaPayloadCarriesPrompt`]);
//! * the binding a seat must establish first —
//!   [`crate::palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1`] and its inner
//!   [`crate::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1`], which the material and capture
//!   decoders call so the seat and the decoders cannot come to disagree about "the ids bind";
//! * what withholding reaches — [`palw_panel_da_withholding_arm_v1`], below.
//!
//! # What this mode is NOT (ADR-0077 SA-5)
//!
//! **Its enforcement is a licence, not a punishment, until ADR-0062 lands.** With ADR-0065
//! Decision 4 armed — every network that ships a V2 ruleset — a producer that withholds the ids
//! reaches abstention, not conviction: the seats file `Unavailable`, `Unavailable` decides
//! nothing, no quorum is reached, the claim redraws once and then voids at `ReceiptTimeout`. The
//! escrow is destroyed and NOBODY is slashed. A network that wants withholding to cost the
//! producer needs the data-availability court ADR-0062 describes; arming mode 2 does not buy it,
//! and reading the arming as if it did is the mistake this module exists to prevent.
//!
//! And: *nothing here logs a prompt id or a prompt's text.* "Private unless disputed" is false if
//! the default log is a disclosure — so the refusals this lane produces name counts, hashes and
//! claim ids, and never a token.

use crate::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA;

/// **The sentence a gateway shows before a first `PanelDa` use, verbatim** (ADR-0077 Decision 16,
/// carrying ADR-0044 Decision 8's obligation; ADR-0077 SA-5's second clause).
///
/// Not a consensus rule — a consensus-OWNED string, and it lives beside the mode it describes for
/// one reason: a product surface that paraphrases this is describing a guarantee the protocol does
/// not make. Five seats read the prompt to verify the claim and a dispute puts it in a court close
/// on chain, so the honest word is *private*, never *confidential*.
///
/// The second sentence is SA-5's, and it is part of the disclosure rather than a footnote because
/// it is the half a user is most likely to assume the other way round: withholding the prompt from
/// the panel does not get the producer punished, it gets the claim thrown away. A user choosing
/// this mode is choosing who sees the prompt, not buying an enforcement.
pub const PALW_FP_PANEL_DA_DISCLOSURE_V1: &str = "Private unless disputed: five seats see this prompt to verify the work, \
     and a dispute publishes it on chain. This is not confidentiality. Withholding it from the panel does not punish the \
     executor — the claim is simply void, and nobody is slashed.";

/// **What a withheld prompt actually reaches** (ADR-0077 Decision 16 and SA-5, over ADR-0065 D4).
///
/// One spelling of a question the seat half, the gateway and this crate's tests all ask, so the
/// three cannot answer it differently. The input is the network's ADR-0065 D4 position —
/// `Params::palw_unavailable_abstains` resolved at the block being folded, the same value the
/// transition is built with — because that fence, and not this mode's, is what decides whether a
/// panel that got nothing can convict anybody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwPanelDaWithholdingArmV1 {
    /// **ADR-0065 D4 armed — SA-5's licence.** `Unavailable` is an abstention: it reaches no
    /// quorum, the claim redraws once and voids at `ReceiptTimeout`, the escrow is destroyed and
    /// no bond is debited. `ProducerDefaulted` is refused by the transition itself
    /// (`PalwStateV2Error::ProducerDefaultRetired`), so this is not a policy a caller may skip.
    AbstainThenReceiptTimeoutVoid,
    /// **ADR-0065 D4 not armed** — the pre-fence rule, and the only configuration in which
    /// withholding reaches `ProducerDefaulted`: a quorum of `Unavailable` voids the claim as
    /// `ProducerWithholding` and takes `claim.reserved` from the producer's bond.
    ProducerDefaulted,
}

/// See [`PalwPanelDaWithholdingArmV1`]. `unavailable_abstains` is the network's ADR-0065 D4
/// position at the block in question.
pub fn palw_panel_da_withholding_arm_v1(unavailable_abstains: bool) -> PalwPanelDaWithholdingArmV1 {
    if unavailable_abstains {
        PalwPanelDaWithholdingArmV1::AbstainThenReceiptTimeoutVoid
    } else {
        PalwPanelDaWithholdingArmV1::ProducerDefaulted
    }
}

/// Does this privacy mode keep the prompt off the commitment transaction?
///
/// A predicate rather than an open-coded `== 2` at each site: the payload rule, the seat's fetch
/// and the gateway's disclosure all branch on the same fact, and a fourth site open-coding it is
/// how a fifth site comes to open-code it wrongly.
pub fn palw_fp_privacy_keeps_prompt_off_chain_v1(privacy_mode: u8) -> bool {
    privacy_mode == PALW_FP_PRIVACY_PANEL_DA
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS};
    use crate::palw_fp_objects_v3::validate_palw_fp_commitment_tx_under_v3;
    use crate::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpCommitmentTxPayloadV3, PalwFpStopReasonV3, PalwFpV3Error,
        PalwFreePromptCommitmentV3, PalwFreePromptJobV3, fp_trace_manifest_v3, palw_fp_capture_decode_v1, palw_fp_capture_encode_v1,
        palw_fp_material_decode_v1, palw_fp_material_encode_v1, palw_fp_prompt_ids_admit_v1, palw_fp_seat_prompt_admit_v1,
    };
    use crate::palw_state_v2::{
        PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2,
        PalwStateV2Error, PalwVoidReasonV2, apply_palw_transition_v2_with_verdict_policy,
    };
    use crate::tx::{TransactionId, TransactionOutpoint};
    use crate::{Hash64, palw_state_v2::PalwBlockContextV2};
    use kaspa_hashes::Hash64 as H;

    fn h64(v: u64) -> Hash64 {
        H::from_u64_word(v)
    }

    fn net() -> Hash64 {
        h64(0x4E)
    }

    fn prompt() -> Vec<u32> {
        vec![7, 8, 9, 10]
    }

    fn job(privacy_mode: u8) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: net(),
            class_id: h64(1),
            executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 },
            executor_pubkey: vec![7; 4],
            operator_id: h64(90),
            anchor_block: h64(0xA0),
            anchor_daa: 100,
            job_nonce: [3; 32],
            tokenizer_id: h64(0x70),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&prompt()),
            prompt_tokens: prompt().len() as u32,
            decode_token_limit: 16,
            max_context_tokens: 64,
            privacy_mode,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        }
    }

    fn payload(privacy_mode: u8, carried: Vec<u32>) -> PalwFpCommitmentTxPayloadV3 {
        let job = job(privacy_mode);
        let events: Vec<Hash64> = (0..4u64).map(|i| h64(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(h64(0xB1), &events);
        let commitment = PalwFreePromptCommitmentV3 {
            job,
            trace_root: h64(41),
            output_root: h64(42),
            schedule_root: h64(0x5C),
            execution_root: h64(43),
            decode_tokens_executed: 4,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 60,
            trace_manifest_root: manifest_root,
            trace_chunk_count: chunk_count,
            trace_retention_daa: 999_999,
        };
        PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment,
            prompt_token_ids: carried,
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    // ---------------------------------------------------------------- the fence

    /// **Dormant everywhere, and dormant means byte-identical** (brief rule 4; ADR-0077 P-16).
    ///
    /// The fence is Some-only in `consensus_params_id`, so an unset one writes nothing: every
    /// shipped preset fingerprints exactly as it did before this field existed, and no pin moves.
    /// Asserted rather than assumed, because "it should not have moved" is the claim a re-pin is
    /// made of.
    #[test]
    fn the_panel_da_fence_is_dormant_on_every_shipped_preset_and_moves_no_fingerprint() {
        for (name, preset) in
            [("mainnet", MAINNET_PARAMS), ("testnet", TESTNET_PARAMS), ("simnet", SIMNET_PARAMS), ("devnet", DEVNET_PARAMS)]
        {
            assert!(preset.palw_panel_da.is_none(), "{name} must leave ADR-0077 Decision 16's PanelDa dormant");
            assert!(!preset.palw_panel_da_at(0), "{name}: and it must not be in force at genesis");
            assert!(!preset.palw_panel_da_at(u64::MAX), "{name}: nor at any height");
            assert!(!preset.palw_panel_da_admissible(), "{name}: nor may the transaction door open");
        }

        let shipped = MAINNET_PARAMS;
        let mut scheduled = MAINNET_PARAMS;
        scheduled.palw_panel_da = Some(ForkActivation::new(9_000_000));
        assert_eq!(
            shipped.consensus_identity_id(),
            scheduled.consensus_identity_id(),
            "scheduling it for a future height must keep old and new builds peers — the whole reason it is not in the V2 bundle"
        );
        let mut never = MAINNET_PARAMS;
        never.palw_panel_da = Some(ForkActivation::never());
        assert_eq!(never.consensus_identity_id(), shipped.consensus_identity_id(), "`Some(never())` is absence");
    }

    /// **The door is never stricter than the walk** — the asymmetry `validate_palw_fp_commitment_tx`
    /// is built around, now that both ends of it read the same fence.
    ///
    /// `palw_panel_da_admissible` is `is_some()`; `palw_panel_da_at(h)` is `is_active(h)`. The
    /// implication must hold at every height, including the ones nobody will run.
    #[test]
    fn the_transaction_door_is_weaker_than_the_walk_at_every_height() {
        let v2 = crate::palw_mode_v2::tests::conforming_bundle();
        let mut armed = DEVNET_PARAMS;
        armed.palw_consensus_mode = crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(v2);
        for fence in [None, Some(ForkActivation::always()), Some(ForkActivation::new(1_000))] {
            armed.palw_panel_da = fence;
            for h in [0u64, 1, 999, 1_000, 1_001, u64::MAX] {
                assert!(
                    !armed.palw_panel_da_at(h) || armed.palw_panel_da_admissible(),
                    "the walk admitted mode 2 at {h} while the door refused it — a carrier the walk would credit could not get in"
                );
            }
        }
    }

    /// **The mode exists only where the free-prompt lane does.** A hash-only network that somehow
    /// carried the fence must still answer "no": there is no commitment for the rule to admit.
    #[test]
    fn the_fence_is_folded_with_the_v2_mode() {
        let mut hash_only = MAINNET_PARAMS;
        hash_only.palw_panel_da = Some(ForkActivation::always());
        assert!(!hash_only.palw_panel_da_at(0), "PALW is Disabled here, so mode 2 admits nothing");
        assert!(!hash_only.palw_panel_da_admissible(), "and the door stays shut");
    }

    // ------------------------------------------------- Decision 16, the payload

    /// **A `PanelDa` commitment carries the hash and not the ids** (ADR-0077 Decision 16).
    ///
    /// Both halves, because either alone reads as the mode working: the job still binds one
    /// prompt (so a claim cannot be re-pointed at another), and the transaction carries none of it
    /// (so the chain never published what the user asked to keep off it).
    #[test]
    fn a_panel_da_commitment_carries_the_hash_and_not_the_ids() {
        let p = payload(PALW_FP_PRIVACY_PANEL_DA, Vec::new());
        assert!(p.prompt_token_ids.is_empty(), "the transaction carries no prompt");
        assert_eq!(
            p.commitment.job.prompt_token_ids_hash,
            crate::palw_v2::prompt_token_ids_hash_v2(&prompt()),
            "and the job still binds exactly one prompt"
        );
        assert_eq!(p.commitment.job.prompt_tokens as usize, prompt().len(), "including its length");
        p.validate_stateless_under_v3(net(), true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("armed, this is an admissible payload");

        // The same claim under PublicDa is the mode that publishes: the ids ride the transaction.
        let public = payload(PALW_FP_PRIVACY_PUBLIC_DA, prompt());
        public
            .validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("PublicDa is admissible with no arming at all");
        assert!(palw_fp_privacy_keeps_prompt_off_chain_v1(PALW_FP_PRIVACY_PANEL_DA));
        assert!(!palw_fp_privacy_keeps_prompt_off_chain_v1(PALW_FP_PRIVACY_PUBLIC_DA));
    }

    /// **Refused by name, and the name is the fix.** "This build cannot do mode 2" and "this
    /// network has not armed mode 2" are different facts, and collapsing them sent a mode-2
    /// executor off to write an ADR instead of scheduling a fence.
    #[test]
    fn an_unarmed_network_refuses_panel_da_by_its_own_name() {
        let p = payload(PALW_FP_PRIVACY_PANEL_DA, Vec::new());
        assert_eq!(
            p.validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaNotArmed)
        );
        assert_eq!(p.validate_shape_v3(crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat), Err(PalwFpV3Error::PanelDaNotArmed));
        assert_eq!(
            p.validate_stateless_under_v3(net(), false, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaNotArmed)
        );
        // …and a mode nothing implements still says so.
        let alien = payload(3, Vec::new());
        assert_eq!(
            alien.validate_stateless_under_v3(net(), true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::UnsupportedPrivacyMode(3))
        );
    }

    /// **A mode-2 payload that publishes the prompt anyway is refused, not trimmed.** An executor
    /// that put the ids on chain has already done the harm; a claim built on that payload would be
    /// one the chain quietly blessed.
    #[test]
    fn a_panel_da_payload_that_carries_the_prompt_is_refused_by_name() {
        let p = payload(PALW_FP_PRIVACY_PANEL_DA, prompt());
        assert_eq!(
            p.validate_stateless_under_v3(net(), true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaPayloadCarriesPrompt(4))
        );
        // Refused even where the arming is absent — the shape rule needs no ruleset, so a network
        // that has not armed the mode still refuses this in the ONE way it can act on.
        assert_eq!(
            p.validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaNotArmed),
            "arming is asked first"
        );
        // And the door refuses it on an ARMED build, where the arming question passes.
        let bytes = borsh::to_vec(&p).unwrap();
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&bytes, true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaPayloadCarriesPrompt(4)),
            "the door checks the shape rule under the arming it was given"
        );
    }

    /// **Fence off is byte-identical at the door.** The one property the brief's rule 4 asks for:
    /// nothing this build accepts into a block was unacceptable to the last one.
    #[test]
    fn with_the_fence_off_the_door_answers_exactly_as_it_did_before_the_mode_existed() {
        let mode_two = borsh::to_vec(&payload(PALW_FP_PRIVACY_PANEL_DA, Vec::new())).unwrap();
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&mode_two, false, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PanelDaNotArmed),
            "a mode-2 carrier is refused at admission on every shipped preset, as it always was"
        );
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&mode_two, false, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            validate_palw_fp_commitment_tx_under_v3(&mode_two, false, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
        );
        // On a build whose ruleset carries the rule, the SHAPE is admitted — the coordinated
        // release, with the height still governing the effect.
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&mode_two, true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Ok(())
        );

        // A PublicDa carrier is untouched in every position, which is what "byte-identical" means
        // for the traffic that actually exists today.
        let public = borsh::to_vec(&payload(PALW_FP_PRIVACY_PUBLIC_DA, prompt())).unwrap();
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&public, false, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Ok(())
        );
        assert_eq!(
            validate_palw_fp_commitment_tx_under_v3(&public, true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Ok(())
        );
    }

    // ------------------------------------------------------- W8, the seat's side

    /// **W8 clause 1: a seat holding no ids can verify nothing, so it files no `Valid`.**
    ///
    /// Stated as the refusal a seat gets when it asks, because the alternative shape — a bool the
    /// caller may ignore — is how "somebody else checks it" gets written down.
    #[test]
    fn a_seat_that_holds_no_ids_is_refused_before_it_can_file_valid() {
        let job = job(PALW_FP_PRIVACY_PANEL_DA);
        assert_eq!(
            palw_fp_seat_prompt_admit_v1(&job, None, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PromptIdsUnavailable)
        );
        palw_fp_seat_prompt_admit_v1(&job, Some(&prompt()), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("the served prompt is this claim's");
    }

    /// **W8 clause 2: a hash mismatch is refused by name**, and so is a length mismatch — two
    /// facts a seat must be able to tell apart from "nothing was served", because the first two
    /// are a producer serving somebody else's work and the third is a producer serving nothing.
    #[test]
    fn a_served_prompt_that_is_not_this_claims_is_refused_by_name() {
        let job = job(PALW_FP_PRIVACY_PANEL_DA);
        let mut wrong = prompt();
        wrong[0] ^= 1;
        assert_eq!(
            palw_fp_prompt_ids_admit_v1(&job, &wrong, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PromptIdsHashMismatch)
        );
        assert_eq!(
            palw_fp_prompt_ids_admit_v1(&job, &prompt()[..3], crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PromptIdsCountMismatch { got: 3, declared: 4 })
        );
        assert_eq!(
            palw_fp_seat_prompt_admit_v1(&job, Some(&wrong), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwFpV3Error::PromptIdsHashMismatch)
        );
    }

    /// **One spelling, and the decoders use it.** The seat and the material/capture decoders must
    /// answer "do these ids bind" identically; the way they stay identical is that there is one
    /// function and not two implementations of one sentence.
    #[test]
    fn the_material_and_capture_decoders_bind_the_ids_through_the_same_predicate() {
        let job = job(PALW_FP_PRIVACY_PANEL_DA);
        let good = palw_fp_material_encode_v1(&job, &prompt());
        assert!(
            palw_fp_material_decode_v1(&good, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_some(),
            "the honest material decodes"
        );
        let mut wrong = prompt();
        wrong[1] ^= 0xFF;
        let bad = palw_fp_material_encode_v1(&job, &wrong);
        assert!(
            palw_fp_material_decode_v1(&bad, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).is_none(),
            "material whose ids are not the claim's is not material"
        );

        let capture = palw_fp_capture_encode_v1(&job, &prompt(), &[1, 2, 3]);
        let decoded = palw_fp_capture_decode_v1(&capture, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
            .expect("the honest capture decodes");
        assert_eq!(decoded.material.prompt_token_ids, prompt());
        // The ids are checked BEFORE the capture is looked at, which is the order Decision 16
        // makes load-bearing: a capture read first would be a replay of a prompt nobody has shown
        // is this claim's.
        assert!(
            palw_fp_capture_decode_v1(
                &palw_fp_capture_encode_v1(&job, &wrong, &[1, 2, 3]),
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .is_none()
        );
    }

    /// **Nothing in this lane's refusals carries a prompt id or a prompt's text** (ADR-0077 SA-5,
    /// third clause: "private unless disputed" is false if the default log is a disclosure).
    ///
    /// The error type is what a node prints, so it is what is checked. Counts and names are
    /// allowed — they are what an operator needs; the token values are not.
    #[test]
    fn no_refusal_on_this_lane_prints_a_prompt_id() {
        let job = job(PALW_FP_PRIVACY_PANEL_DA);
        let mut wrong = prompt();
        wrong[0] = 0xDEAD_BEEF;
        let rendered = [
            palw_fp_seat_prompt_admit_v1(&job, None, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).unwrap_err().to_string(),
            palw_fp_prompt_ids_admit_v1(&job, &wrong, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).unwrap_err().to_string(),
            palw_fp_prompt_ids_admit_v1(&job, &prompt()[..2], crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .unwrap_err()
                .to_string(),
            payload(PALW_FP_PRIVACY_PANEL_DA, prompt())
                .validate_stateless_under_v3(net(), true, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .unwrap_err()
                .to_string(),
            payload(PALW_FP_PRIVACY_PANEL_DA, Vec::new())
                .validate_stateless_v3(net(), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .unwrap_err()
                .to_string(),
        ];
        for line in rendered {
            for id in prompt().iter().chain(std::iter::once(&0xDEAD_BEEFu32)) {
                assert!(!line.contains(&id.to_string()), "a refusal printed the prompt id {id}: {line}");
            }
        }
    }

    /// The disclosure a gateway owes a first-time user says both halves of the honest name — and
    /// SA-5's, which is the half a user is most likely to assume the other way round.
    #[test]
    fn the_disclosure_says_private_not_confidential_and_names_the_licence() {
        let d = PALW_FP_PANEL_DA_DISCLOSURE_V1.to_ascii_lowercase();
        assert!(d.contains("private unless disputed"), "the honest name");
        assert!(d.contains("not confidentiality"), "and the word it is not");
        assert!(d.contains("publishes it"), "a dispute publishes the prompt");
        assert!(d.contains("nobody is slashed"), "ADR-0077 SA-5: the enforcement is a licence");
    }

    // ------------------------------------------- SA-5, driven through the state

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 800, 0).unwrap().with_fp_quanta(8, 64).unwrap()
    }

    fn bond_op(v: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 160 },
                initial_target: u128::MAX,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_op(1)),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 1_000,
                payout_payload: H::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ]
    }

    const CLAIM: u64 = 0xFC;

    /// A `PanelDa` claim with a bound panel, staged under this network's ADR-0065 D4 position.
    /// The commitment object is the same one `PublicDa` produces — Decision 16 changes where the
    /// ids travel, not what the chain records — which is itself the point: the transition holds a
    /// `prompt_token_ids_hash` and has never held ids.
    fn bound_claim(abstains: bool) -> (PalwChainStateV2, PalwStateParamsV2) {
        let p = state_params();
        let apply = |s: &PalwChainStateV2, c: &PalwBlockContextV2, objs: &[PalwConsensusObjectV2]| {
            apply_palw_transition_v2_with_verdict_policy(s, &p, c, objs, None, abstains).expect("the fixture applies")
        };
        let (s1, _) = apply(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &registrations());
        let commit = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(CLAIM),
            class_id: h64(1),
            bond: PalwBondKeyV2(bond_op(1)),
            executor_pubkey: vec![7; 4],
            work_leaves: 60,
            // The whole of what a `PanelDa` claim publishes about its prompt.
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&prompt()),
            decode_tokens_executed: 8,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
        };
        let (s2, _) = apply(&s1, &ctx(2, 101, 2), &[commit]);
        let seats = vec![PalwPanelSeatV2 { bond: PalwBondKeyV2(bond_op(1)), operator_id: h64(90) }];
        let (s3, _) = apply(&s2, &ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: h64(CLAIM), anchor: h64(77), seats }]);
        assert!(matches!(s3.claim(&h64(CLAIM)).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "the panel is bound");
        (s3, p)
    }

    /// **ADR-0077 SA-5: withholding the ids is an abstention, and it slashes nobody.**
    ///
    /// The chain a withheld `PanelDa` prompt actually walks, past ADR-0065 D4: the seats file
    /// `Unavailable`, no quorum is reachable, the sweep redraws once and the claim voids at
    /// `ReceiptTimeout` — escrow destroyed, bond untouched. And the object that WOULD take the
    /// producer's stake is refused by its own name, so this is a rule and not a convention.
    ///
    /// Driven forward one block at a time rather than jumping to a computed height: the point is
    /// the terminal the claim reaches on its own, and a fixture that hard-codes the sweep's
    /// arithmetic asserts the arithmetic instead of the outcome.
    #[test]
    fn adr_0077_sa5_withholding_a_panel_da_prompt_voids_without_slashing_anybody() {
        let (bound, p) = bound_claim(true);
        let staked = bound.bond(&PalwBondKeyV2(bond_op(1))).expect("registered").collateral;
        assert!(bound.claim(&h64(CLAIM)).expect("accepted").reserved > 0, "the claim must reserve something, or 'no slash' is empty");

        // The conviction arm is not merely unreached — it is refused.
        let err = apply_palw_transition_v2_with_verdict_policy(
            &bound,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ProducerDefaulted { claim: h64(CLAIM), receipts: Vec::new() }],
            None,
            true,
        )
        .expect_err("past ADR-0065 D4 nothing may convict a producer of a failure the chain cannot observe");
        assert!(matches!(err, PalwStateV2Error::ProducerDefaultRetired(c) if c == h64(CLAIM)), "refused by name: {err:?}");
        assert_eq!(palw_panel_da_withholding_arm_v1(true), PalwPanelDaWithholdingArmV1::AbstainThenReceiptTimeoutVoid);

        // Nobody serves anything, ever. The claim is left to the lattice, and the lattice's answer
        // is the whole of SA-5: no quorum → ONE redraw → `ReceiptTimeout`.
        //
        // The second panel is bound when the redraw happens rather than at a computed height,
        // because the point is the terminal the claim reaches and not the sweep's arithmetic. It
        // has to be bound by something: a redrawn claim nobody binds dies at `BindTimeout`, which
        // also slashes nobody but is a different fact — the panel that was never seated, not the
        // panel that was never fed.
        let mut state = bound;
        let mut phase = None;
        let mut redrawn = false;
        for block in 4u64..80 {
            let objects: Vec<PalwConsensusObjectV2> = match state.claim(&h64(CLAIM)).map(|c| c.phase.clone()) {
                Some(PalwClaimPhaseV2::Provisional) if !redrawn => {
                    redrawn = true;
                    let seats = vec![PalwPanelSeatV2 { bond: PalwBondKeyV2(bond_op(1)), operator_id: h64(90) }];
                    vec![PalwConsensusObjectV2::PanelBound { claim: h64(CLAIM), anchor: h64(78), seats }]
                }
                _ => Vec::new(),
            };
            let (next, _) =
                apply_palw_transition_v2_with_verdict_policy(&state, &p, &ctx(block, 100 + block, block), &objects, None, true)
                    .expect("an empty block always applies");
            state = next;
            match state.claim(&h64(CLAIM)) {
                Some(c) if c.phase.is_terminal() => {
                    phase = Some(c.phase.clone());
                    break;
                }
                // Retired out from under us would also be a terminal, and a silent one.
                None => break,
                Some(_) => {}
            }
        }
        assert!(redrawn, "an unfed panel must be redrawn once before anything voids — that redraw IS the licence");
        match phase {
            Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }) => {}
            other => panic!("a withheld PanelDa claim must void at ReceiptTimeout, got {other:?}"),
        }
        let bond = state.bond(&PalwBondKeyV2(bond_op(1))).expect("still registered");
        assert_eq!(bond.collateral, staked, "SA-5: the producer's stake is where it was");
        assert_eq!(bond.slashed, 0, "and nothing was recorded as a slash");
    }

    /// **…and `ProducerDefaulted` is reached only where the ADR says it is** — a network that has
    /// not armed ADR-0065 D4. Asserted so the SA-5 half above cannot be read as "the arm is gone":
    /// it is gone past the fence, and the fence is what this build ships armed on a new genesis.
    #[test]
    fn without_adr_0065_d4_the_same_withholding_still_reaches_producer_defaulted() {
        let (bound, p) = bound_claim(false);
        let staked = bound.bond(&PalwBondKeyV2(bond_op(1))).expect("registered").collateral;
        let reserved = u64::try_from(bound.claim(&h64(CLAIM)).expect("accepted").reserved).expect("fits");
        let (after, _) = apply_palw_transition_v2_with_verdict_policy(
            &bound,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ProducerDefaulted { claim: h64(CLAIM), receipts: Vec::new() }],
            None,
            false,
        )
        .expect("before the fence a quorum of Unavailable defaults the producer");
        match after.claim(&h64(CLAIM)).expect("still present").phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. } => {}
            ref other => panic!("expected ProducerWithholding, got {other:?}"),
        }
        assert_eq!(after.bond(&PalwBondKeyV2(bond_op(1))).expect("registered").collateral, staked - reserved);
        assert_eq!(palw_panel_da_withholding_arm_v1(false), PalwPanelDaWithholdingArmV1::ProducerDefaulted);
    }

    /// **A court close still carries the ids** (ADR-0077 Decision 16, W8 clause 4).
    ///
    /// The close's own carrier is the lifecycle band's, and what makes "a dispute publishes the
    /// prompt" true is that the material a close addresses is the material a seat was served —
    /// the same `PalwFpMaterialV1`, ids included, whatever privacy mode the job declared. Asserted
    /// where it can be: the material encoding is mode-independent, so a `PanelDa` claim's close
    /// carries exactly what a `PublicDa` claim's does.
    #[test]
    fn a_court_close_carries_the_ids_under_panel_da_too() {
        let panel_da = palw_fp_material_encode_v1(&job(PALW_FP_PRIVACY_PANEL_DA), &prompt());
        let public = palw_fp_material_encode_v1(&job(PALW_FP_PRIVACY_PUBLIC_DA), &prompt());
        assert_eq!(
            palw_fp_material_decode_v1(&panel_da, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("decodes")
                .prompt_token_ids,
            prompt(),
            "the ids a dispute publishes are the ids the executor ran"
        );
        assert_ne!(panel_da, public, "the two modes are different jobs — the mode is inside the job id");
        assert_eq!(
            palw_fp_material_decode_v1(&panel_da, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("decodes")
                .prompt_token_ids,
            palw_fp_material_decode_v1(&public, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
                .expect("decodes")
                .prompt_token_ids,
            "but what a close carries is the same in both"
        );
    }

    /// The `Params` field is a fence and nothing else — no companion value to drag into the
    /// identity, and no second spelling of the arming anywhere in the tree.
    #[test]
    fn the_arming_has_exactly_one_spelling() {
        let mut p: Params = DEVNET_PARAMS;
        let v2 = crate::palw_mode_v2::tests::conforming_bundle();
        p.palw_consensus_mode = crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(v2);
        p.palw_panel_da = Some(ForkActivation::new(500));
        assert!(!p.palw_panel_da_at(499));
        assert!(p.palw_panel_da_at(500));
        assert!(p.palw_panel_da_admissible(), "the door opens with the ruleset, not with the height");
    }
}
