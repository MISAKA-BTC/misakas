//! **A remote attempt: the miner computes and signs; the node supplies a template and relays the finished block.**
//!
//! `kaspad`'s producer did everything in one process — template, inference, draw, signature, block, material retention. RFC-0009 splits that
//! at the seam the chain already draws: the *assembly* of the attempt from an execution is one consensus-core function
//! ([`kaspa_consensus_core::palw_attempt_v2::palw_attempt_from_execution_v1`], which the node's producer now calls too), and everything
//! else a remote miner needs is here as traits it supplies:
//!
//! ```text
//!   node(s)  ── template + facts ──▶  check_templates (quorum, fresh, held key)       [crate::template]
//!   miner    ── anchor ─▶ AttemptExecutor::execute  (the inference; the miner's own machine)
//!            ── assemble (shared fn) ─▶ class draw ─▶ network draw ─▶ AttemptSigner::sign (ONCE, only on a win)
//!            ── recheck_before_submit ─▶ header with the signed carriage
//!   node(s)  ◀─ the finished block, idempotently                                      [crate::template::submit_block_idempotent]
//!   provider ◀─ the material, as an evidence manifest + chunks                          [crate::evidence]
//! ```
//!
//! **Nothing is signed before the draw is won** (RFC §3.2): the signature is outside the priced bytes, so signing per nonce would be an
//! ML-DSA-87 operation thrown away almost every time, and an early signature would be a signed statement about a template that may go stale.
//!
//! **What this does not supply** is the executor itself — the model backend (`kaspad`'s `palw_backends`) stays where it is; a remote miner
//! links whichever it uses behind [`AttemptExecutor`]. The node's own producer is unchanged except that it builds its attempt through the
//! shared assembly function.

use crate::template::{AcceptedTemplate, TemplatePolicy, TemplateRefusal, recheck_before_submit};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptChainFactsV1, PalwAttemptEnvelopeV2,
    PalwAttemptExecutionV1, PalwAttemptUnsignedV2, attempt_id_v2, class_ticket_v3, palw_attempt_from_execution_v1, palw_job_anchor_v1,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

/// What the miner's executor returns for one draw: the roots the attempt commits and the capture bytes it must keep serving (or place with
/// providers — stage B).
#[derive(Clone, Debug)]
pub struct ExecutedAttempt {
    pub execution: PalwAttemptExecutionV1,
    pub material: Vec<u8>,
}

/// The miner's inference. `anchor` is the job anchor the template and bucket imply; the executor derives the job from it exactly as the chain's
/// verifiers do. Runs on the miner's machine.
pub trait AttemptExecutor {
    fn execute(&mut self, anchor: Hash64) -> Result<ExecutedAttempt, String>;
    /// **Audit C-05: the capture must exist and commit the roots the attempt will claim**, checked before the bond key is
    /// touched. The default refuses an absent capture (a signed claim whose roots nobody can serve). An executor that links a
    /// backend overrides this to decode the capture's step binding and compare its committed execution root (and the trace
    /// roots it derives) with `executed.execution`; an executor without one cannot decode a capture, and in the PAV3 custody
    /// form every roster provider rebuilds the bundle from the capture against the attempt's roots before it signs anything.
    fn check_capture(&self, anchor: Hash64, executed: &ExecutedAttempt) -> Result<(), String> {
        let _ = anchor;
        check_capture_present_v1(executed)
    }
}

/// The backend-free half of [`AttemptExecutor::check_capture`]: a capture is present.
pub fn check_capture_present_v1(executed: &ExecutedAttempt) -> Result<(), String> {
    if executed.material.is_empty() {
        return Err("the executor returned no capture: an attempt whose roots nobody can serve is never signed".into());
    }
    Ok(())
}

/// The miner's bond key. Called at most once per mounted attempt, and only after both lotteries are won.
pub trait AttemptSigner {
    fn public_key(&self) -> Vec<u8>;
    fn sign_attempt_id(&self, attempt_id: &Hash64) -> Result<Vec<u8>, String>;
    /// The call [`mount_attempt`] makes: the attempt itself rides along so a signer can keep the equivocation journal the node's producer keeps
    /// (one challenge, one attempt id). The default signs the id.
    fn sign_attempt(&self, attempt: &PalwAttemptUnsignedV2, attempt_id: &Hash64) -> Result<Vec<u8>, String> {
        let _ = attempt;
        self.sign_attempt_id(attempt_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptParams {
    pub network_id: String,
    pub network_domain: Hash64,
    pub bond: TransactionOutpoint,
    pub operator_id: Hash64,
    /// The class target (`facts.class_target`): the class lottery's bar.
    pub class_target: u128,
    pub witness_chunks: u32,
    /// `true` where the chain admits an attempt header's digest unconditionally (`palw_single_lottery`); the adapter reads it from the fence.
    pub single_lottery: bool,
    /// The wire length of a signature, for the dummy the network draw is made against.
    pub signature_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AttemptError {
    #[error("the template refused: {0}")]
    Template(#[from] TemplateRefusal),
    #[error("the miner's executor failed: {0}")]
    Executor(String),
    #[error("the miner's signer failed: {0}")]
    Signer(String),
    #[error("the template's header is not an attempt-lane header (algo {0})")]
    NotAnAttemptLane(u8),
}

/// A won draw, finished and signed: the header to put on the template's block, the identity the material is kept under, and the capture.
#[derive(Clone, Debug)]
pub struct MountedAttempt {
    pub header: Header,
    pub attempt_id: Hash64,
    pub attempt: PalwAttemptUnsignedV2,
    pub material: Vec<u8>,
    pub nonce_bucket: u64,
}

/// `Ok(None)`: the draw lost (class lottery, or the network's) — the normal case; the caller walks to the next bucket. `cursor` is the same
/// resume rule the node's producer keeps: a template with the same pre-PoW hash resumes at the bucket after the last one drawn.
#[allow(clippy::too_many_arguments)]
pub fn mount_attempt(
    template: &AcceptedTemplate,
    template_header: &Header,
    params: &AttemptParams,
    facts_pwu: u64,
    min_trace_retention_daa: u64,
    artifact_root: Hash64,
    class_id: Hash64,
    cursor: &mut Option<(Hash64, u64)>,
    executor: &mut dyn AttemptExecutor,
    signer: &dyn AttemptSigner,
    network_draw: &dyn Fn(&Header, u64, bool) -> bool,
) -> Result<Option<MountedAttempt>, AttemptError> {
    if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(template_header.pow_algo_id) {
        return Err(AttemptError::NotAnAttemptLane(template_header.pow_algo_id));
    }
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(template_header);
    let nonce_bucket = match *cursor {
        Some((at, next)) if at == pre_pow && next < (1u64 << (64 - PALW_TICKET_NONCE_BUCKET_LOG2)) => next,
        _ => 0,
    };
    *cursor = Some((pre_pow, nonce_bucket + 1));
    let nonce = nonce_bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
    let anchor = palw_job_anchor_v1(params.network_domain, pre_pow, class_id, &params.bond, nonce_bucket);

    let executed = executor.execute(anchor).map_err(AttemptError::Executor)?;
    executor.check_capture(anchor, &executed).map_err(AttemptError::Executor)?;
    let facts = PalwAttemptChainFactsV1 {
        class_id,
        artifact_root,
        pwu: facts_pwu,
        min_trace_retention_daa,
        witness_chunks: params.witness_chunks,
        operator_id: params.operator_id,
    };
    let attempt = palw_attempt_from_execution_v1(
        params.network_domain,
        pre_pow,
        template_header.timestamp,
        nonce,
        params.bond,
        signer.public_key(),
        &facts,
        &executed.execution,
        template_header.daa_score,
    );
    // The class lottery first: a function of the one execution, decided here once.
    if class_ticket_v3(&attempt, anchor) > params.class_target {
        return Ok(None);
    }
    // Then the network's, against a header carrying a dummy signature of the right length (the signature is outside the priced bytes).
    let mut probe = template_header.clone();
    probe.nonce = nonce;
    probe.palw_commitment =
        PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature: vec![0u8; params.signature_len] }.encode_wire();
    if !network_draw(&probe, nonce, params.single_lottery) {
        return Ok(None);
    }
    // Both won: sign the attempt id ONCE.
    let attempt_id = attempt_id_v2(&attempt);
    let signature = signer.sign_attempt(&attempt, &attempt_id).map_err(AttemptError::Signer)?;
    let mut header = template_header.clone();
    header.nonce = nonce;
    header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature }.encode_wire();
    header.finalize();
    let _ = template; // the accepted template is the caller's re-check subject, not an input to the bytes
    Ok(Some(MountedAttempt { header, attempt_id, attempt, material: executed.material, nonce_bucket }))
}

/// **RFC-0008 PAV3: what a remote miner supplies past `palw_monetary_material_v1`.** Every answer must come from an AUTHENTICATED
/// view (op 242 pages finished by `MonetaryMaterialProofPagerV1` plus the proven bond table); a forecast or an unauthenticated RPC row
/// cannot select a session, because the fold re-selects with the same canonical rule and refuses a substitute. The provider wire is
/// the provider service's (`native_custody_exchange_v3::PalwNativeReadyRequestV3`, audit C-03).
pub trait NativeCustodyV3 {
    /// The canonical session for `attempt` at `daa` (consensus's rule over the authenticated rows), or `None` (hold).
    fn session(
        &self,
        attempt: &PalwAttemptUnsignedV2,
        daa: u64,
    ) -> Option<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>;
    /// The complete one-scope manifest of this capture's typed bundle and the prompt its anchor implies, as every roster provider
    /// rebuilt them under its own node's authority from `request` (the DRAW envelope: the placeholder manifest the ticket is priced
    /// with). Called only after both draws are won and the attempt is signed (the authority read verifies that signature).
    fn manifest(
        &mut self,
        session: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
        request: &crate::native_custody_exchange_v3::PalwNativeReadyRequestV3,
    ) -> Result<(kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialManifestV1, Vec<u32>), String>;
    /// Every roster provider's signature over `Ready::id` of `request.envelope.ready`, each given only after that provider rebuilt
    /// the bundle from the capture, found the manifest identical and durably published it.
    fn collect(
        &mut self,
        request: &crate::native_custody_exchange_v3::PalwNativeReadyRequestV3,
    ) -> Result<std::collections::BTreeMap<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2, Vec<u8>>, String>;
    /// The executor's seal (same key as the attempt signature) over `message` under `context`.
    fn seal(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String>;
}

/// [`mount_attempt`] in the owned PAV3 custody form. The class and network draws price `priced_execution` (session, consent and roster,
/// never the manifest or a signature) on the DRAW envelope (`palw_native_draw_manifest_v3`, the in-node producer's), so they are
/// decided BEFORE any provider is asked; the attempt is signed, then the providers rebuild the manifest and sign the exact Ready,
/// and the sealed carriage is re-checked to price exactly what won. The pre-entropy checkpoint the providers' authority reads is the
/// template's chain point, which must be a direct parent of the header. A missing session, manifest or roster signature mounts
/// nothing — never a PAV2 fallback the chain refuses past the fence.
#[allow(clippy::too_many_arguments)]
pub fn mount_native_attempt_v3(
    template: &AcceptedTemplate,
    template_header: &Header,
    params: &AttemptParams,
    facts_pwu: u64,
    min_trace_retention_daa: u64,
    artifact_root: Hash64,
    class_id: Hash64,
    cursor: &mut Option<(Hash64, u64)>,
    executor: &mut dyn AttemptExecutor,
    signer: &dyn AttemptSigner,
    custody: &mut dyn NativeCustodyV3,
    network_draw: &dyn Fn(&Header, u64, bool) -> bool,
) -> Result<Option<MountedAttempt>, AttemptError> {
    use crate::native_custody_exchange_v3::{PALW_NATIVE_READY_EXCHANGE_VERSION_V3, PalwNativeReadyRequestV3};
    use kaspa_consensus_core::palw_attempt_material_v3::{
        PalwAttemptMaterialEnvelopeV3, draft_native_ready_v3, palw_native_draw_manifest_v3,
    };
    if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(template_header.pow_algo_id) {
        return Err(AttemptError::NotAnAttemptLane(template_header.pow_algo_id));
    }
    let checkpoint = template.observation.facts.chain_point;
    if !template_header.direct_parents().contains(&checkpoint) {
        return Err(AttemptError::Executor(
            "the template's chain point is not a direct parent of its header: no provider checkpoint".into(),
        ));
    }
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(template_header);
    let nonce_bucket = match *cursor {
        Some((at, next)) if at == pre_pow && next < (1u64 << (64 - PALW_TICKET_NONCE_BUCKET_LOG2)) => next,
        _ => 0,
    };
    *cursor = Some((pre_pow, nonce_bucket + 1));
    let nonce = nonce_bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
    let anchor = palw_job_anchor_v1(params.network_domain, pre_pow, class_id, &params.bond, nonce_bucket);
    let executed = executor.execute(anchor).map_err(AttemptError::Executor)?;
    executor.check_capture(anchor, &executed).map_err(AttemptError::Executor)?;
    let facts = PalwAttemptChainFactsV1 {
        class_id,
        artifact_root,
        pwu: facts_pwu,
        min_trace_retention_daa,
        witness_chunks: params.witness_chunks,
        operator_id: params.operator_id,
    };
    let attempt = palw_attempt_from_execution_v1(
        params.network_domain,
        pre_pow,
        template_header.timestamp,
        nonce,
        params.bond,
        signer.public_key(),
        &facts,
        &executed.execution,
        template_header.daa_score,
    );
    let session = custody
        .session(&attempt, template_header.daa_score)
        .ok_or_else(|| AttemptError::Executor("no eligible prior signed native custody session".into()))?;
    let draw_manifest = palw_native_draw_manifest_v3(&session, &attempt).map_err(AttemptError::Executor)?;
    let draw_ready = draft_native_ready_v3(&session, &attempt, anchor, draw_manifest).map_err(AttemptError::Executor)?;
    let dummy = vec![0u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let provisional = |base: PalwAttemptEnvelopeV2, ready| PalwAttemptMaterialEnvelopeV3 {
        version: 3,
        base,
        ready,
        provider_signatures: vec![dummy.clone(); session.providers.iter().filter(|p| !p.charged).count()],
        seal: dummy.clone(),
    };
    let draw = provisional(PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature: dummy.clone() }, draw_ready.clone());
    if draw.class_ticket(anchor) > params.class_target {
        return Ok(None);
    }
    let mut probe = template_header.clone();
    probe.nonce = nonce;
    probe.palw_commitment = draw.encode_wire();
    if !network_draw(&probe, nonce, params.single_lottery) {
        return Ok(None);
    }
    let attempt_id = attempt_id_v2(&attempt);
    let signature = signer.sign_attempt(&attempt, &attempt_id).map_err(AttemptError::Signer)?;
    let base = PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature };
    let mut request = PalwNativeReadyRequestV3 {
        version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3,
        checkpoint,
        header: probe,
        envelope: provisional(base.clone(), draw_ready.clone()),
        capture: executed.material.clone(),
        prompt: Vec::new(),
    };
    let (manifest, prompt) = custody.manifest(&session, &request).map_err(AttemptError::Executor)?;
    let ready = draft_native_ready_v3(&session, &attempt, anchor, manifest).map_err(AttemptError::Executor)?;
    request.envelope = provisional(base.clone(), ready.clone());
    request.prompt = prompt;
    let signatures = custody.collect(&request).map_err(AttemptError::Executor)?;
    let envelope = crate::native_ready_v3::producer_assemble_native_ready_v3(base, ready, &signatures, |message, context| {
        custody.seal(message, context)
    })
    .map_err(AttemptError::Signer)?;
    // The sealed carriage prices exactly what won (the manifest and every signature are outside the priced bytes).
    if envelope.priced_execution(anchor) != draw.priced_execution(anchor) {
        return Err(AttemptError::Executor("the sealed native carriage prices another execution than the draw".into()));
    }
    let mut header = template_header.clone();
    header.nonce = nonce;
    header.palw_commitment = envelope.encode_wire();
    header.finalize();
    Ok(Some(MountedAttempt { header, attempt_id, attempt, material: executed.material, nonce_bucket }))
}

/// The check made after the (possibly long) inference and before the block leaves: a fresh quorum must still stand on the template's chain
/// point and within its freshness bound.
pub fn ready_to_publish(
    accepted: &AcceptedTemplate,
    fresh: &[crate::template::TemplateObservation],
    policy: &TemplatePolicy,
    view_daa: u64,
) -> Result<(), AttemptError> {
    recheck_before_submit(accepted, fresh, policy, view_daa).map_err(AttemptError::from)
}

/// The ML-DSA-87 signature the shipped producer makes, as a ready [`AttemptSigner`] for a bond key held in-process (the binaries; a signer
/// sidecar implements the trait instead).
pub struct MlDsaAttemptSigner {
    pub keypair: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
}

impl AttemptSigner for MlDsaAttemptSigner {
    fn public_key(&self) -> Vec<u8> {
        self.keypair.verification_key.as_ref().to_vec()
    }
    fn sign_attempt_id(&self, attempt_id: &Hash64) -> Result<Vec<u8>, String> {
        libcrux_ml_dsa::ml_dsa_87::sign(
            &self.keypair.signing_key,
            attempt_id.as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .map(|s| s.as_ref().to_vec())
        .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{ProducerFactsSummary, TemplateObservation, check_templates};
    use std::cell::Cell;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn header(daa: u64) -> Header {
        Header::new_finalized(
            1,
            vec![vec![h(1)]].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            1_700_000,
            0x1d00ffff,
            0,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            daa,
            0u64.into(),
            0,
            h(5),
        )
    }

    struct Exec {
        calls: Cell<u32>,
        anchors: std::cell::RefCell<Vec<Hash64>>,
    }
    impl AttemptExecutor for Exec {
        fn execute(&mut self, anchor: Hash64) -> Result<ExecutedAttempt, String> {
            self.calls.set(self.calls.get() + 1);
            self.anchors.borrow_mut().push(anchor);
            Ok(ExecutedAttempt {
                execution: PalwAttemptExecutionV1 {
                    trace_root: h(10),
                    output_root: h(11),
                    execution_root: h(12),
                    trace_manifest_root: h(13),
                    trace_chunk_count: 1,
                },
                material: vec![7; 64],
            })
        }
    }
    struct Signer {
        signed: Cell<u32>,
    }
    impl AttemptSigner for Signer {
        fn public_key(&self) -> Vec<u8> {
            vec![9; 8]
        }
        fn sign_attempt_id(&self, id: &Hash64) -> Result<Vec<u8>, String> {
            self.signed.set(self.signed.get() + 1);
            Ok(id.as_byte_slice()[..8].to_vec())
        }
    }

    fn params(class_target: u128) -> AttemptParams {
        AttemptParams {
            network_id: "testnet-12".into(),
            network_domain: h(20),
            bond: TransactionOutpoint::new(h(21), 0),
            operator_id: h(22),
            class_target,
            witness_chunks: 0,
            single_lottery: true,
            signature_len: 8,
        }
    }

    fn accepted() -> AcceptedTemplate {
        let obs = |node: &str| TemplateObservation {
            node: node.into(),
            network_id: "testnet-12".into(),
            pruning_point: h(1),
            version: 1,
            pow_algo_id: 6,
            bits: 0x1d00ffff,
            daa_score: 100,
            palw_state_root: h(2),
            parents: vec![h(1)],
            facts: ProducerFactsSummary {
                chain_point: h(5),
                class_id: h(30),
                artifact_root: h(31),
                class_target: u128::MAX,
                pwu: 7_708,
                min_trace_retention_daa: 3_000,
                bond_pubkey: vec![9; 8],
                not_ready_reason: String::new(),
            },
        };
        let policy = TemplatePolicy { min_agree: 2, max_template_age_daa: 30, held_pubkey: vec![9; 8], held_artifact_root: h(31) };
        check_templates(&[obs("a"), obs("b")], &policy, 105).unwrap()
    }

    #[test]
    fn a_won_draw_is_signed_once_and_carries_the_shared_assembly_of_the_attempt() {
        let t = accepted();
        let (exec, signer) = (&mut Exec { calls: Cell::new(0), anchors: Default::default() }, Signer { signed: Cell::new(0) });
        let hdr = header(100);
        let mut cursor = None;
        let mounted =
            mount_attempt(&t, &hdr, &params(u128::MAX), 7_708, 3_000, h(31), h(30), &mut cursor, exec, &signer, &|_, _, _| true)
                .unwrap()
                .expect("both lotteries pass");
        assert_eq!(signer.signed.get(), 1, "signed exactly once, after the draw");
        assert_eq!(exec.calls.get(), 1);
        // The header carries the signed envelope the chain decodes, bound to THIS position.
        let env = PalwAttemptEnvelopeV2::decode_wire(&mounted.header.palw_commitment).unwrap();
        assert_eq!(env.attempt, mounted.attempt);
        assert_eq!(env.signature, mounted.attempt_id.as_byte_slice()[..8].to_vec());
        assert_eq!(mounted.attempt.trace_retention_daa, 100 + 3_000, "the chain's retention pin: the block's own DAA plus the window");
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&hdr);
        assert_eq!(
            mounted.attempt.challenge,
            kaspa_consensus_core::palw_attempt_v2::challenge_v2(h(20), pre_pow, hdr.timestamp, 0, h(30), &params(0).bond)
        );
        assert_eq!(exec.anchors.borrow()[0], palw_job_anchor_v1(h(20), pre_pow, h(30), &params(0).bond, 0));
    }

    #[test]
    fn a_lost_draw_signs_nothing_and_the_cursor_walks_to_the_next_bucket() {
        let t = accepted();
        let (exec, signer) = (&mut Exec { calls: Cell::new(0), anchors: Default::default() }, Signer { signed: Cell::new(0) });
        let hdr = header(100);
        let mut cursor = None;
        // The class lottery loses (target 0).
        assert!(
            mount_attempt(&t, &hdr, &params(0), 7_708, 3_000, h(31), h(30), &mut cursor, exec, &signer, &|_, _, _| true)
                .unwrap()
                .is_none()
        );
        // The network lottery loses.
        assert!(
            mount_attempt(&t, &hdr, &params(u128::MAX), 7_708, 3_000, h(31), h(30), &mut cursor, exec, &signer, &|_, _, _| false)
                .unwrap()
                .is_none()
        );
        assert_eq!(signer.signed.get(), 0, "a lost draw is never signed");
        // The same template resumed at the next bucket each time: two draws, two distinct anchors; a different template starts over.
        let anchors = exec.anchors.borrow().clone();
        assert_eq!(anchors.len(), 2);
        assert_ne!(anchors[0], anchors[1]);
        assert_eq!(cursor.map(|(_, next)| next), Some(2));
    }

    /// **audit(verify-3) C-05**: an executor that returns NO capture (the CLI's `material_file` omitted ⇒ `Vec::new()`) must be refused
    /// before the bond key is touched — the signed claim names roots nobody can serve. Today the attempt is signed and mounted.
    #[test]
    fn audit_v3_c05_an_attempt_without_a_capture_is_never_signed() {
        struct EmptyCapture;
        impl AttemptExecutor for EmptyCapture {
            fn execute(&mut self, _anchor: Hash64) -> Result<ExecutedAttempt, String> {
                Ok(ExecutedAttempt {
                    execution: PalwAttemptExecutionV1 {
                        trace_root: h(10),
                        output_root: h(11),
                        execution_root: h(12),
                        trace_manifest_root: h(13),
                        trace_chunk_count: 1,
                    },
                    material: Vec::new(),
                })
            }
        }
        let t = accepted();
        let signer = Signer { signed: Cell::new(0) };
        let r = mount_attempt(
            &t,
            &header(100),
            &params(u128::MAX),
            7_708,
            3_000,
            h(31),
            h(30),
            &mut None,
            &mut EmptyCapture,
            &signer,
            &|_, _, _| true,
        );
        let mounted = matches!(r, Ok(Some(_)));
        assert!(
            matches!(r, Err(AttemptError::Executor(_))) && signer.signed.get() == 0,
            "C-05: expected Executor refusal with 0 signatures for an empty capture; got mounted={mounted} signatures={}",
            signer.signed.get()
        );
    }

    /// Audit C-05 boundary: an executor whose capture check finds the capture committing other roots (a backend-linked check)
    /// is refused before the key is touched, on the PAV2 path as on the empty capture.
    #[test]
    fn audit_fixcd_c05_a_capture_that_commits_other_roots_is_never_signed() {
        struct Mismatch(Exec);
        impl AttemptExecutor for Mismatch {
            fn execute(&mut self, anchor: Hash64) -> Result<ExecutedAttempt, String> {
                self.0.execute(anchor)
            }
            fn check_capture(&self, _anchor: Hash64, executed: &ExecutedAttempt) -> Result<(), String> {
                check_capture_present_v1(executed)?;
                // A backend would decode the step binding; this double says it commits h(99), not the reported root.
                if executed.execution.execution_root != h(99) {
                    return Err("the capture commits another execution root than the executor reported".into());
                }
                Ok(())
            }
        }
        let signer = Signer { signed: Cell::new(0) };
        let mut exec = Mismatch(Exec { calls: Cell::new(0), anchors: Default::default() });
        let r = mount_attempt(
            &accepted(),
            &header(100),
            &params(u128::MAX),
            7_708,
            3_000,
            h(31),
            h(30),
            &mut None,
            &mut exec,
            &signer,
            &|_, _, _| true,
        );
        assert!(matches!(r, Err(AttemptError::Executor(ref why)) if why.contains("another execution root")), "{r:?}");
        assert_eq!(signer.signed.get(), 0);
    }

    #[test]
    fn a_header_that_is_not_an_attempt_lane_is_refused_before_any_inference() {
        let t = accepted();
        let (exec, signer) = (&mut Exec { calls: Cell::new(0), anchors: Default::default() }, Signer { signed: Cell::new(0) });
        let mut hdr = header(100);
        hdr.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH;
        let r = mount_attempt(&t, &hdr, &params(u128::MAX), 1, 1, h(31), h(30), &mut None, exec, &signer, &|_, _, _| true);
        assert!(matches!(r, Err(AttemptError::NotAnAttemptLane(_))));
        assert_eq!(exec.calls.get(), 0, "no inference was spent");
    }

    #[test]
    fn publication_waits_for_a_fresh_quorum_on_the_same_chain_point() {
        let t = accepted();
        let policy = TemplatePolicy { min_agree: 2, max_template_age_daa: 30, held_pubkey: vec![9; 8], held_artifact_root: h(31) };
        let obs = |daa: u64, point: u8| {
            ["a", "b"].map(|n| TemplateObservation {
                node: n.into(),
                network_id: "testnet-12".into(),
                pruning_point: h(1),
                version: 1,
                pow_algo_id: 6,
                bits: 0x1d00ffff,
                daa_score: daa,
                palw_state_root: h(2),
                parents: vec![h(1)],
                facts: ProducerFactsSummary {
                    chain_point: h(point),
                    class_id: h(30),
                    artifact_root: h(31),
                    class_target: u128::MAX,
                    pwu: 7_708,
                    min_trace_retention_daa: 3_000,
                    bond_pubkey: vec![9; 8],
                    not_ready_reason: String::new(),
                },
            })
        };
        assert!(ready_to_publish(&t, &obs(100, 5), &policy, 120).is_ok());
        assert!(matches!(
            ready_to_publish(&t, &obs(100, 5), &policy, 140),
            Err(AttemptError::Template(TemplateRefusal::Stale { .. }))
        ));
        assert!(matches!(ready_to_publish(&t, &obs(120, 6), &policy, 125), Err(AttemptError::Template(TemplateRefusal::ChainMoved))));
    }
}
