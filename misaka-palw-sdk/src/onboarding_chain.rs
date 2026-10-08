//! **Model onboarding against the chain** (G14 onboarding P0): the signed registration envelope (tag 108), the conformance evidence
//! object (tag 109), and the fresh verifier that rebuilds the chain's conformance verdict from public reads alone.
//!
//! # Detached signing (tag 108)
//!
//! ```text
//! build: SignedRegistrationRequestV1::new(registration, signer bond, valid_until, network, ruleset)  ── no key ──► JSON
//! sign:  where the key is, the request is RE-DERIVED from its own fields (the exported message is never trusted), checked
//!        against the signer's expectations, then signed over `palw_signed_registration_message_v1` under its context
//! file:  the envelope (`PalwConsensusObjectV2::SignedRegistrationV1`) is an ordinary onboarding object: `misaka palw submit-object`
//! ```
//!
//! The library never holds a seed: signing goes through [`EnvelopeSigner`] (the CLI's key, a hardware signer, a remote one).
//! The envelope dies at `valid_until_daa` and is valid on exactly one ruleset (`consensus_params_id`) — RFC-0009 G-EXPIRY /
//! G-RULESET.
//!
//! # The fresh verifier
//!
//! [`fresh_verify_from_reads_v1`] takes what a node serves publicly — op 231's attempt and evidence rows, op 212's Final facts,
//! the class's program and registered artifact root — plus, optionally, the artifact from its public source, and re-derives the
//! beacon, the seed, the selection and the evidence exactly as the chain's fold does (consensus-core's one implementation), then
//! re-reads every selected leaf from the artifact. It needs no node-private state and no producer state.

use crate::runtime_pack::commit::{Refusal, hex};
use kaspa_consensus_core::palw_conformance_evidence_v1::{
    ConformanceEvidenceActionV1, ConformanceEvidencePostV1, FreshInputV1, FreshVerdictV1, SelectedLeafV1, fresh_verify_v1,
    palw_onboarding_challenge_policy_v1,
};
use kaspa_consensus_core::palw_onboarding_v1::{
    ConformanceAttemptRowV1, PALW_ONBOARDING_MLDSA87_CONTEXT_V1, PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1,
    palw_onboarding_message_v1, palw_signed_registration_message_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, palw_class_registration_buyer_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::{Hash, Hash64};
use misaka_palw_challenge::WorkFinalEventV1;
use serde_json::{Value, json};
use std::path::Path;

/// The JSON schema of an unsigned envelope request.
pub const SIGNED_REGISTRATION_REQUEST_SCHEMA_V1: &str = "misaka.palw.signed-registration-request.v1";

/// **Whoever holds the bond's key.** ML-DSA-87 over `message` under `context`; the library never sees the seed.
pub trait EnvelopeSigner {
    fn public_key(&self) -> Vec<u8>;
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String>;
}

/// **An unsigned tag-108 envelope**: the wrapped registration, its signer, the last DAA it may be accepted at, and the network and
/// ruleset it is for. Its message is a function of these fields alone ([`Self::message`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedRegistrationRequestV1 {
    pub registration: PalwConsensusObjectV2,
    pub signer: PalwBondKeyV2,
    pub valid_until_daa: u64,
    /// `palw_network_domain_v2_for(network name, genesis)`.
    pub network_domain: Hash64,
    /// The ruleset's `Params::consensus_params_id()`.
    pub consensus_params_id: Hash,
}

impl SignedRegistrationRequestV1 {
    /// **The one envelope builder.** Refuses a wrapped object that is not a bought class registration of `signer`'s own bond (the
    /// acceptance walk would drop it).
    pub fn new(
        registration: PalwConsensusObjectV2,
        signer: PalwBondKeyV2,
        valid_until_daa: u64,
        network_domain: Hash64,
        consensus_params_id: Hash,
    ) -> Result<Self, Refusal> {
        if palw_class_registration_buyer_v1(&registration) != Some(signer) {
            return Err(Refusal::new(
                "ENVELOPE_NOT_A_REGISTRATION",
                "the envelope wraps a bought class registration (ClassRegistered / ClassRegisteredTirV1) of the signer's own bond",
            ));
        }
        Ok(Self { registration, signer, valid_until_daa, network_domain, consensus_params_id })
    }

    /// The message the bond signs (`palw_signed_registration_message_v1`).
    pub fn message(&self) -> Hash64 {
        let bytes = borsh::to_vec(&self.registration).expect("an object serializes");
        palw_signed_registration_message_v1(self.network_domain, self.consensus_params_id, self.valid_until_daa, &self.signer, &bytes)
    }

    /// The envelope with `signature` attached (no check: see [`Self::sign`] and [`verify_signed_registration_v1`]).
    pub fn with_signature(&self, signature: Vec<u8>) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::SignedRegistrationV1 {
            registration: Box::new(self.registration.clone()),
            valid_until_daa: self.valid_until_daa,
            consensus_params_id: self.consensus_params_id,
            signer: self.signer,
            signature,
        }
    }

    /// **Sign** with `signer` and check the signature verifies under `signer`'s own public key before returning the envelope.
    pub fn sign(&self, signer: &dyn EnvelopeSigner) -> Result<PalwConsensusObjectV2, Refusal> {
        let message = self.message();
        let signature = signer
            .sign_with_context(message.as_byte_slice(), PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1)
            .map_err(|e| Refusal::new("SIGNER_FAILED", e))?;
        let envelope = self.with_signature(signature);
        verify_signed_registration_v1(&envelope, self.network_domain, self.consensus_params_id, &signer.public_key())?;
        Ok(envelope)
    }

    /// The detached-signing record: every field the signer re-derives the message from, and the message for display only.
    pub fn to_json(&self) -> Value {
        json!({
            "schema": SIGNED_REGISTRATION_REQUEST_SCHEMA_V1,
            "note": "UNSIGNED. The signer re-derives the message from these fields; `message` is shown, never trusted.",
            "registration_borsh": hex(&borsh::to_vec(&self.registration).expect("an object serializes")),
            "signer": format!("{}:{}", self.signer.0.transaction_id, self.signer.0.index),
            "valid_until_daa": self.valid_until_daa,
            "network_domain": self.network_domain.to_string(),
            "consensus_params_id": self.consensus_params_id.to_string(),
            "message": self.message().to_string(),
        })
    }

    /// Read a detached-signing record. Every field is parsed; `message` is ignored (it is recomputed).
    pub fn from_json(text: &str) -> Result<Self, Refusal> {
        let bad = |why: String| Refusal::new("REQUEST_MALFORMED", why);
        let v: Value = serde_json::from_str(text).map_err(|e| bad(e.to_string()))?;
        if v["schema"].as_str() != Some(SIGNED_REGISTRATION_REQUEST_SCHEMA_V1) {
            return Err(bad(format!("schema is not {SIGNED_REGISTRATION_REQUEST_SCHEMA_V1}")));
        }
        let field = |k: &str| v[k].as_str().ok_or_else(|| bad(format!("{k} is missing")));
        let raw = unhex(field("registration_borsh")?).map_err(bad)?;
        let registration: PalwConsensusObjectV2 = borsh::from_slice(&raw).map_err(|e| bad(format!("registration_borsh: {e}")))?;
        let signer = parse_bond(field("signer")?).map_err(bad)?;
        let valid_until_daa = v["valid_until_daa"].as_u64().ok_or_else(|| bad("valid_until_daa is missing".into()))?;
        let network_domain: Hash64 = field("network_domain")?.parse().map_err(|_| bad("network_domain".into()))?;
        let consensus_params_id: Hash = field("consensus_params_id")?.parse().map_err(|_| bad("consensus_params_id".into()))?;
        Self::new(registration, signer, valid_until_daa, network_domain, consensus_params_id)
    }
}

/// **Check a signed envelope** as the acceptance walk will (minus the chain-state parts: the fence, the bond being Active, the
/// expiry against the block): the wrapped object is a registration of the signer, it names `consensus_params_id`, and the signature
/// verifies under `pubkey` over the message re-derived from its fields.
pub fn verify_signed_registration_v1(
    envelope: &PalwConsensusObjectV2,
    network_domain: Hash64,
    consensus_params_id: Hash,
    pubkey: &[u8],
) -> Result<(), Refusal> {
    let PalwConsensusObjectV2::SignedRegistrationV1 { registration, valid_until_daa, consensus_params_id: named, signer, signature } =
        envelope
    else {
        return Err(Refusal::new("ENVELOPE_MALFORMED", "not a signed registration envelope (tag 108)"));
    };
    if *named != consensus_params_id {
        return Err(Refusal::new("ENVELOPE_OTHER_RULESET", "the envelope names another ruleset (G-RULESET)"));
    }
    if palw_class_registration_buyer_v1(registration) != Some(*signer) {
        return Err(Refusal::new("ENVELOPE_NOT_A_REGISTRATION", "the envelope wraps no bought class registration of its signer"));
    }
    let bytes = borsh::to_vec(registration.as_ref()).expect("an object serializes");
    let message = palw_signed_registration_message_v1(network_domain, *named, *valid_until_daa, signer, &bytes);
    match kaspa_txscript::verify_mldsa87_with_context(
        pubkey,
        message.as_byte_slice(),
        signature,
        PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1,
    ) {
        Ok(true) => Ok(()),
        _ => Err(Refusal::new("ENVELOPE_SIGNATURE", "the signature does not verify under the signer's key")),
    }
}

// ---- tag 109 -----------------------------------------------------------------------------------------------------------------

/// **A signed conformance evidence object (tag 109)**: `action` (the registrant's `Post`, or anyone else's `Refute`) for `v2_class`,
/// signed by `signer`'s key over `palw_onboarding_message_v1(network, 109, signer, borsh(v2_class, action))`.
pub fn conformance_evidence_object_v1(
    network_domain: Hash64,
    v2_class: Hash64,
    action: ConformanceEvidenceActionV1,
    signer_bond: PalwBondKeyV2,
    signer: &dyn EnvelopeSigner,
) -> Result<PalwConsensusObjectV2, Refusal> {
    let payload = borsh::to_vec(&(v2_class, &action)).expect("an action serializes");
    let message = palw_onboarding_message_v1(network_domain, 109, &signer_bond, &payload);
    let signature = signer
        .sign_with_context(message.as_byte_slice(), PALW_ONBOARDING_MLDSA87_CONTEXT_V1)
        .map_err(|e| Refusal::new("SIGNER_FAILED", e))?;
    Ok(PalwConsensusObjectV2::ConformanceEvidenceV1 { v2_class, action: Box::new(action), signer: signer_bond, signature })
}

/// The `Post` action for a computed run (the runtime pack's evidence, scope and outcomes).
pub fn post_action_v1(post: ConformanceEvidencePostV1) -> ConformanceEvidenceActionV1 {
    ConformanceEvidenceActionV1::Post(Box::new(post))
}

// ---- the fresh verifier ---------------------------------------------------------------------------------------------------

/// **What a node serves publicly about one class's conformance** (op 231's rows, op 212's facts, op 230 / the registry's class).
pub struct PublicConformanceReadsV1 {
    /// Table 39's row, raw (op 231 `attemptRow`).
    pub attempt_row: Vec<u8>,
    /// Table 40's row, raw (op 231 `evidenceRow`), if evidence was posted.
    pub evidence_row: Option<Vec<u8>>,
    /// Every Final fact with a beacon event (op 212 `workFinalEvent`), any order.
    pub events: Vec<WorkFinalEventV1>,
    /// The DAA the reads were taken at.
    pub tip_daa: u64,
    /// The class's canonical program bytes (op 231 `program`).
    pub program: Vec<u8>,
}

/// The fresh verifier's report: its own verdict and the chain's state it should agree with.
#[derive(Debug)]
pub struct FreshReportV1 {
    pub verdict: FreshVerdictV1,
    pub attempt: ConformanceAttemptRowV1,
    /// The chain's record says the attempt passed (or the class is past it) — compare with `verdict.posted`.
    pub chain_says_passed: bool,
    /// The verifier's verdict and the chain's record agree.
    pub agrees: bool,
    pub why: String,
}

/// **Rebuild the verdict from public reads alone**, re-reading every selected leaf from `artifact` (a `PALWTIR1` container) when
/// given. Agreement: the chain says CONFORMANCE_PASSED (or later) exactly when the verifier finds bound, passing evidence whose
/// window has closed and no leaf it re-read contradicts it; an open attempt agrees when the verifier finds nothing to refute yet.
pub fn fresh_verify_from_reads_v1(reads: &PublicConformanceReadsV1, artifact: Option<&Path>) -> Result<FreshReportV1, Refusal> {
    use misaka_palw_challenge::OnboardingStateV1 as S;
    let attempt: ConformanceAttemptRowV1 =
        borsh::from_slice(&reads.attempt_row).map_err(|e| Refusal::new("ROW_MALFORMED", format!("attempt row: {e}")))?;
    let post: Option<ConformanceEvidencePostV1> = match &reads.evidence_row {
        Some(bytes) => Some(borsh::from_slice(bytes).map_err(|e| Refusal::new("ROW_MALFORMED", format!("evidence row: {e}")))?),
        None => None,
    };
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&reads.program)
        .map_err(|e| Refusal::new("PROGRAM_MALFORMED", e.to_string()))?;
    let policy = palw_onboarding_challenge_policy_v1();
    if policy.id() != attempt.commitment.challenge_policy_id {
        return Err(Refusal::new(
            "POLICY_SUBSTITUTED",
            "the attempt's commitment names another policy than this build's network policy",
        ));
    }
    let ctx = attempt.beacon_context(&policy);
    let container = match artifact {
        Some(p) => Some(
            misaka_palw_tir_artifact::PalwTirContainerV1::open(p).map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e.to_string()))?,
        ),
        None => None,
    };
    // The artifact is the class's only if its inventory roots to the registered artifact root the commitment names (one streamed
    // pass): a re-read leaf is then authenticated, and a contradiction is a refutation the chain will take.
    if let Some(p) = artifact {
        let (root, _) =
            crate::tir_stream::palw_tir_inventory_root_of_file_v1(p).map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e))?;
        if root.as_bytes() != attempt.commitment.artifact_root {
            return Err(Refusal::new(
                "ARTIFACT_NOT_THE_CLASS",
                "the artifact's inventory root is not the class's registered artifact root",
            ));
        }
    }
    let ranges = match container.as_ref() {
        Some(c) => Some(crate::tir_stream::ContainerRanges::open(c).map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e))?),
        None => None,
    };
    let leaf_source = |leaf: &SelectedLeafV1| -> Option<Vec<u8>> {
        use crate::tir_stream::PalwTirRangeSourceV1;
        let r = ranges.as_ref()?;
        let mut out = vec![0u8; leaf.len as usize];
        r.read_range(leaf.param, leaf.layer, leaf.row_start as u64..leaf.row_start as u64 + leaf.len as u64, &mut out).ok()?;
        Some(out)
    };
    let verdict = fresh_verify_v1(&FreshInputV1 {
        commitment: &attempt.commitment,
        policy: &policy,
        ctx: &ctx,
        events: &reads.events,
        tip_daa: reads.tip_daa,
        program: &program,
        post: post.as_ref(),
        leaf_source: if artifact.is_some() { Some(&leaf_source) } else { None },
    });
    let chain_says_passed = matches!(attempt.record.state, S::ConformancePassed | S::G14Eligible | S::ActiveRewardable);
    let window_closed = attempt.evidence.is_some_and(|e| reads.tip_daa >= e.window_end_daa);
    let verifier_pass = matches!(verdict.posted, Some(Ok(()))) && verdict.leaf_faults.is_empty();
    let (agrees, why) = if chain_says_passed {
        (verifier_pass && window_closed, "the chain passed it: the verifier must find bound, passing, unrefuted evidence".to_string())
    } else if attempt.open() {
        (
            verdict.leaf_faults.is_empty() || attempt.evidence.is_some(),
            "the attempt is open: the chain decides when the window closes (a leaf fault found here is a refutation to file)"
                .to_string(),
        )
    } else {
        (true, format!("the attempt ended: {:?}", attempt.last_end))
    };
    Ok(FreshReportV1 { verdict, attempt, chain_says_passed, agrees, why })
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not hex".into());
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())).collect()
}

fn parse_bond(s: &str) -> Result<PalwBondKeyV2, String> {
    let (txid, index) = s.split_once(':').ok_or_else(|| format!("{s}: not <txid>:<index>"))?;
    let txid: TransactionId = txid.parse().map_err(|_| format!("{txid}: not a transaction id"))?;
    let index: u32 = index.parse().map_err(|_| format!("{index}: not an index"))?;
    Ok(PalwBondKeyV2(TransactionOutpoint::new(txid, index)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway key (never a real one): the test's seed is a constant.
    struct TestKey(kaspa_pq_validator_core::ValidatorKey);
    impl EnvelopeSigner for TestKey {
        fn public_key(&self) -> Vec<u8> {
            self.0.public_key().to_vec()
        }
        fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
            Ok(self.0.sign_with_context(message, context).to_vec())
        }
    }

    /// A bought IR registration of `bond` (its content is not judged here: the envelope binds whatever it wraps).
    fn registration(bond: PalwBondKeyV2) -> PalwConsensusObjectV2 {
        use kaspa_consensus_core::palw_tir_class_v1::{
            PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirAdmissionCarriageV1, PalwTirClassV1, PalwTirLayoutV1,
        };
        let z = Hash64::from_bytes([0; 64]);
        let canonical = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
            version: 2,
            network_id: b"test".to_vec(),
            job_id: z,
            job_nullifier: z,
            assignment_id: z,
            execution_seed: [0; 32],
            model_profile_id: z,
            runtime_manifest_hash: z,
            runtime_class_id: z,
            shape_profile_id: z,
            trace_scheme_id: z,
            cu_ruleset_id: z,
            tokenizer_id: z,
            prompt_token_ids_hash: z,
            declared_prefill_tokens: 1,
            exact_decode_tokens: 1,
            max_context_tokens: 8,
        };
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: vec![1, 2, 3],
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 8,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
            tokenizer_id: Hash64::from_bytes([7; 64]),
        };
        PalwConsensusObjectV2::ClassRegisteredTirV1 {
            class_id: Hash64::from_bytes([1; 64]),
            artifact_root: Hash64::from_bytes([2; 64]),
            slash_value_per_pwu: 1,
            pwu_rule: kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: Box::new(PalwTirAdmissionCarriageV1 { class, canonical, registrant_bond: bond, signature: vec![] }),
        }
    }

    /// The envelope is built, exported unsigned, re-read where the key is, signed, and verifies; every field the message covers
    /// changes the message; a forged signature, another ruleset and another signer's object are refused.
    #[test]
    fn the_envelope_round_trips_through_detached_signing_and_binds_every_field() {
        let key = TestKey(kaspa_pq_validator_core::ValidatorKey::from_seed([0x5A; 32]));
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(9), 0));
        let (domain, ruleset) = (Hash64::from_bytes([3; 64]), Hash::from_bytes([4; 32]));
        let req = SignedRegistrationRequestV1::new(registration(bond), bond, 500, domain, ruleset).unwrap();
        let exported = req.to_json().to_string();
        let reread = SignedRegistrationRequestV1::from_json(&exported).unwrap();
        assert_eq!(reread, req, "the detached record round-trips");
        let envelope = reread.sign(&key).unwrap();
        verify_signed_registration_v1(&envelope, domain, ruleset, &key.public_key()).unwrap();
        // Every covered field moves the message.
        let m = req.message();
        for other in [
            SignedRegistrationRequestV1 { valid_until_daa: 501, ..req.clone() },
            SignedRegistrationRequestV1 { network_domain: Hash64::from_bytes([5; 64]), ..req.clone() },
            SignedRegistrationRequestV1 { consensus_params_id: Hash::from_bytes([6; 32]), ..req.clone() },
        ] {
            assert_ne!(other.message(), m);
        }
        assert!(
            verify_signed_registration_v1(&envelope, domain, Hash::from_bytes([6; 32]), &key.public_key()).is_err(),
            "another ruleset"
        );
        let PalwConsensusObjectV2::SignedRegistrationV1 { mut signature, .. } = envelope.clone() else { unreachable!() };
        signature[0] ^= 1;
        assert!(verify_signed_registration_v1(&req.with_signature(signature), domain, ruleset, &key.public_key()).is_err(), "forged");
        let stranger = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(10), 0));
        assert!(
            SignedRegistrationRequestV1::new(registration(bond), stranger, 500, domain, ruleset).is_err(),
            "not the signer's registration"
        );
        // A tampered export: the message is recomputed, so editing it changes nothing; editing a field changes what is signed.
        let mut v: Value = serde_json::from_str(&exported).unwrap();
        v["message"] = Value::String("00".repeat(64));
        assert_eq!(SignedRegistrationRequestV1::from_json(&v.to_string()).unwrap().message(), m, "the shown message is never trusted");
    }
}
