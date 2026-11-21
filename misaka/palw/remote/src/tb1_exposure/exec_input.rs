//! Private RFC-0059/0062 and RFC-0066 input stage. The close block commits to the entire
//! supplied registration batch. This reader does not admit funded mining work.
use super::{
    AnchorState, anchor_state, invalid,
    publication::{NativePublicationGate, PublicationLimits, PublishedRelease},
};
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    palw_prompt_ids_v1::prompt_token_ids_root_v1,
    palw_tb1::{
        freeze::{Tb1MiningClosure, Tb1MiningPurpose, Tb1MiningRound},
        qualification::Tb1Qualification,
    },
};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey};
use sha3::{Digest, Sha3_256, Sha3_512};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub mod moe2;
use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::Moe2PreDrawQuote;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature;

pub const REGISTRATION_DOMAIN: &[u8] = b"MISAKA/private/FOW1/input-owner/v2";

/// The legacy input uses BOS=0, POW=1, SEP=2 and ByteToken[b]=3+b. QF1 uses
/// its separately registered HEX64 frame. Both bind the model, class and tokenizer.
pub struct ExecInputTerms<'a> {
    pub qf1: Option<super::qf1::Qf1Use<'a>>,
    pub registration_anchor: Hash64,
    pub candidate_set: [u8; 64],
    pub beneficiary: [u8; 64],
    pub model_root: Hash64,
    pub class_id: Hash64,
    pub tokenizer: Hash64,
    pub job_tokens: &'a [u32],
    pub vocab: u32,
    pub max_positions: u32,
    pub decode_tokens: u32,
    pub max_leaves: u64,
    pub nonce: [u8; 32],
    /// Digests of the generation/checker/material policy, difficulty snapshot
    /// and accounting/purpose/transition contract in the enclosing private profile.
    pub policy: [u8; 64],
    pub difficulty: [u8; 64],
    pub target: [u8; 33],
    pub accounting: [u8; 64],
    /// Optional private quote fixed before EXEC. No reward entitlement.
    pub moe2_quote: Option<Moe2PreDrawQuote>,
    pub open_daa: u64,
    pub deadline_exclusive_daa: u64,
}

impl ExecInputTerms<'_> {
    fn threshold(&self) -> io::Result<kaspa_consensus_core::palw_an1_input_binding_and_panel_court_v1::An1TargetThreshold257V1> {
        use kaspa_consensus_core::palw_an1_input_binding_and_panel_court_v1::An1TargetThreshold257V1 as Threshold;
        let [high, low @ ..] = &self.target;
        match *high {
            0 => Ok(Threshold::BelowTwoPow256(*low)),
            1 if low.iter().all(|&b| b == 0) => Ok(Threshold::MaxTwoPow256),
            _ => Err(invalid("invalid private EXEC target")),
        }
    }

    fn context(
        &self,
        preparation: Hash64,
        rounds: &QualifiedExecPanel,
        owner: &MLDSA65VerificationKey,
        max_tokens: usize,
    ) -> io::Result<Vec<u8>> {
        let n = u32::try_from(self.job_tokens.len()).map_err(|_| invalid("TB1 job too long"))?;
        let frame = self.qf1.map(|requested| requested.admitted()).transpose()?;
        let input_len = if let Some(frame) = frame {
            if self.tokenizer != frame.tokenizer
                || self.job_tokens != frame.job_tokens()
                || self.vocab != kaspa_consensus_core::palw_qf1::VOCAB
                || self.max_positions != kaspa_consensus_core::palw_qf1::CONTEXT
            {
                return Err(invalid("QF1 registration differs from bound frame or geometry"));
            }
            let len = frame.length()?;
            kaspa_consensus_core::palw_qf1::check_capacity(len, self.decode_tokens).map_err(invalid)?;
            Some(len)
        } else {
            n.checked_add(35)
        };
        let positions = input_len.and_then(|n| n.checked_add(self.decode_tokens.checked_sub(1)?));
        if self.vocab < 259
            || self.max_leaves == 0
            || (n == 0 && self.qf1.is_none())
            || self.job_tokens.len() > max_tokens
            || positions.is_none_or(|n| n > self.max_positions)
            || self.job_tokens.iter().any(|&id| id >= self.vocab)
            || rounds.panel.release_from_daa >= self.deadline_exclusive_daa
            || self.open_daa >= self.deadline_exclusive_daa
        {
            return Err(invalid("invalid private EXEC registration context"));
        }
        self.threshold()?;
        let job = prompt_token_ids_root_v1(self.job_tokens).map_err(|_| invalid("invalid job token commitment"))?;
        let mut c = Vec::new();
        if let Some(frame) = frame {
            c.extend_from_slice(b"MISAKA/private/FOW1/QF1-Q3-8B-A16-8K-v1/local-inference/context/v1");
            c.extend_from_slice(frame.instance_descriptor.as_byte_slice());
            c.extend_from_slice(&frame.source_digest);
        } else {
            c.extend_from_slice(b"MISAKA/private/FOW1/TIR-input/012-byte3/v2");
        }
        for field in [
            preparation.as_bytes(),
            rounds.commitment(),
            self.registration_anchor.as_bytes(),
            self.candidate_set,
            self.beneficiary,
            self.model_root.as_bytes(),
            self.class_id.as_bytes(),
            self.tokenizer.as_bytes(),
            job.as_bytes(),
            self.policy,
            self.difficulty,
            self.accounting,
        ] {
            c.extend_from_slice(&field);
        }
        c.extend_from_slice(owner.as_slice());
        c.extend_from_slice(&self.nonce);
        c.extend_from_slice(&self.target);
        for value in [n, self.vocab, self.max_positions, self.decode_tokens] {
            c.extend_from_slice(&value.to_be_bytes());
        }
        for value in [self.max_leaves, self.open_daa, self.deadline_exclusive_daa] {
            c.extend_from_slice(&value.to_be_bytes());
        }
        if let Some(quote) = self.moe2_quote {
            if quote.model != self.class_id
                || quote.artifact_root != self.model_root
                || quote.deadline == u64::MAX
                || quote.deadline < quote.valid_from
                || quote.price_limits.max_output_slippage_bps > 10_000
                || quote.limits.min_out > quote.limits.max_purchase_units
            {
                return Err(invalid("invalid MOE2 quote for signed EXEC model"));
            }
            c.extend_from_slice(b"MISAKA/private/FOW1/MOE2/pre-draw-quote/v1");
            c.extend_from_slice(&borsh::to_vec(&quote)?);
        }
        Ok(c)
    }

    pub fn registration_key(
        &self,
        preparation: Hash64,
        rounds: &QualifiedExecPanel,
        owner: &MLDSA65VerificationKey,
        max_tokens: usize,
    ) -> io::Result<[u8; 64]> {
        Ok(reg_key(&self.context(preparation, rounds, owner, max_tokens)?))
    }
}

fn reg_key(c: &[u8]) -> [u8; 64] {
    Sha3_512::new().chain_update(b"MISAKA/private/FOW1/reg-key/v1").chain_update(c).finalize().into()
}

/// Slot and funding reference belong to the allocation envelope, outside C.
pub struct SignedExecInput<'a> {
    pub terms: &'a ExecInputTerms<'a>,
    pub owner: &'a MLDSA65VerificationKey,
    pub signature: &'a MLDSA65Signature,
    pub slot: u64,
    pub funding_ref: [u8; 64],
    pub quote_consent: Option<&'a MLDSA87Signature>,
}

/// The two signed qualification cuts of the private input profile. Dealer
/// termination and resource references are inputs to qualification. The
/// commitment authenticates those inputs as of the preparation block.
#[derive(Clone, Copy)]
pub struct QualifiedExecPanel {
    exec: Tb1MiningRound,
    panel: Tb1MiningRound,
    exec_qual: [u8; 64],
    #[cfg(feature = "private-tb1-tir")]
    panel_qual: [u8; 64],
    marker: [u8; 64],
}

impl QualifiedExecPanel {
    pub fn new(
        exec: Tb1MiningRound,
        exec_q: &Tb1Qualification<'_>,
        panel: Tb1MiningRound,
        panel_q: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        super::check_panel_schedule(Tb1MiningPurpose::Exec, &exec, exec_q, &panel, panel_q)?;
        let marker = Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/qualified-exec-panel/v1")
            .chain_update(exec.id())
            .chain_update(exec_q.root())
            .chain_update(panel.id())
            .chain_update(panel_q.root())
            .finalize()
            .into();
        Ok(Self {
            exec,
            panel,
            exec_qual: exec_q.root(),
            #[cfg(feature = "private-tb1-tir")]
            panel_qual: panel_q.root(),
            marker,
        })
    }

    pub fn commitment(&self) -> [u8; 64] {
        self.marker
    }
}

struct RegisteredInput<'a> {
    context: Vec<u8>,
    terms: &'a ExecInputTerms<'a>,
    #[cfg(feature = "private-tb1-tir")]
    owner: &'a MLDSA65VerificationKey,
    #[cfg(feature = "private-tb1-tir")]
    funding_ref: [u8; 64],
}

pub struct ExecInputBatch<'a> {
    root: [u8; 64],
    rounds: QualifiedExecPanel,
    preparation: Hash64,
    entries: BTreeMap<u64, RegisteredInput<'a>>,
}

impl<'a> ExecInputBatch<'a> {
    /// Validate each signed preimage, reject any slot or RegKey alias and
    /// hash in slot order. ML-DSA signing randomness cannot change a launch.
    pub fn from_signed(
        native: &dyn ConsensusApi,
        preparation: Hash64,
        rounds: &QualifiedExecPanel,
        rows: &[SignedExecInput<'a>],
        max_records: usize,
        max_job_tokens: usize,
    ) -> io::Result<Self> {
        if rows.len() > max_records {
            return Err(invalid("EXEC batch exceeds registration budget"));
        }
        let exec = &rounds.exec;
        let tip = native.get_sink();
        let header = native.get_header(preparation).map_err(io::Error::other)?;
        if header.blue_score >= exec.close_blue_score
            || header.daa_score >= exec.release_from_daa
            || !native.is_chain_ancestor_of(preparation, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(exec.chain_id, preparation, &rounds.marker).map_err(io::Error::other)?
        {
            return Err(invalid("missing joint qualification before EXEC close"));
        }
        let count = u32::try_from(rows.len()).map_err(|_| invalid("EXEC registration count overflow"))?;
        let mut entries = BTreeMap::new();
        let mut keys = BTreeSet::new();
        let mut allocations = BTreeMap::new();
        for row in rows {
            let anchor = row.terms.registration_anchor;
            if !native.is_chain_ancestor_of(preparation, anchor).map_err(io::Error::other)?
                || !native.is_chain_ancestor_of(anchor, tip).map_err(io::Error::other)?
            {
                return Err(invalid("registration anchor outside prepared selected history"));
            }
            let c = row.terms.context(preparation, rounds, row.owner, max_job_tokens)?;
            let key = reg_key(&c);
            let registered = RegisteredInput {
                context: c,
                terms: row.terms,
                #[cfg(feature = "private-tb1-tir")]
                owner: row.owner,
                #[cfg(feature = "private-tb1-tir")]
                funding_ref: row.funding_ref,
            };
            if !keys.insert(key) || entries.insert(row.slot, registered).is_some() {
                return Err(invalid("duplicate EXEC slot or RegKey"));
            }
            ml_dsa_65::verify(row.owner, &key, REGISTRATION_DOMAIN, row.signature)
                .map_err(|_| invalid("invalid EXEC owner signature"))?;
            match (&row.terms.moe2_quote, row.quote_consent) {
                (Some(quote), Some(consent)) => moe2::verify_consent(native, preparation, row, key, quote, consent)?,
                (None, None) => {}
                _ => return Err(invalid("EXEC quote and funder consent must be present together")),
            }
            allocations.insert(row.slot, (key, row.funding_ref));
        }
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/FOW1/exec-input-set/v2")
            .chain_update(rounds.marker)
            .chain_update(preparation.as_bytes())
            .chain_update(count.to_be_bytes());
        for (slot, (key, funding)) in allocations {
            h.update(slot.to_be_bytes());
            h.update(key);
            h.update(funding);
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC registration view moved"));
        }
        Ok(Self { preparation, rounds: *rounds, root: h.finalize().into(), entries })
    }

    pub fn root(&self) -> [u8; 64] {
        self.root
    }

    fn check_close(&self, native: &dyn ConsensusApi, close: Hash64, closure: &Tb1MiningClosure) -> io::Result<()> {
        if closure.round.purpose != Tb1MiningPurpose::Exec
            || closure.round.id() != self.rounds.exec.id()
            || closure.qual_root != self.rounds.exec_qual
            || closure.consumer_set_root != self.root
        {
            return Err(invalid("EXEC input set differs from closure"));
        }
        let header = native.get_header(close).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(self.preparation, parent).map_err(io::Error::other)? {
            return Err(invalid("EXEC close does not descend from its preparation"));
        }
        for entry in self.entries.values() {
            let terms = entry.terms;
            if terms.policy != closure.policy_root
                || !(terms.open_daa..terms.deadline_exclusive_daa).contains(&header.daa_score)
                || terms.moe2_quote.is_some_and(|quote| !(quote.valid_from..=quote.deadline).contains(&header.daa_score))
                || !native.is_chain_ancestor_of(terms.registration_anchor, parent).map_err(io::Error::other)?
            {
                return Err(invalid("EXEC input outside its registration history or policy"));
            }
        }
        Ok(())
    }

    pub fn observe(
        self,
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
    ) -> io::Result<FrozenExecInputs<'a>> {
        self.check_close(native, close, closure)?;
        let gate = NativePublicationGate::observe(native, close, closure, q)?;
        Ok(FrozenExecInputs { batch: self, gate, close })
    }
}

pub struct FrozenExecInputs<'a> {
    close: Hash64,
    batch: ExecInputBatch<'a>,
    gate: NativePublicationGate,
}

pub struct DerivedExecInput<'a> {
    close: Hash64,
    certificate: Hash64,
    attempt: [u8; 32],
    registration_key: [u8; 64],
    #[cfg(feature = "private-tb1-tir")]
    context_digest: Hash64,
    #[cfg(feature = "private-tb1-tir")]
    owner: &'a MLDSA65VerificationKey,
    #[cfg(feature = "private-tb1-tir")]
    slot: u64,
    tokens: Vec<u32>,
    #[cfg(feature = "private-tb1-tir")]
    chain: [u8; 64],
    terms: &'a ExecInputTerms<'a>,
}

impl DerivedExecInput<'_> {
    /// Recheck the original certificate on the current selected history and the
    /// job deadline when resuming work or returning a result from a long run.
    pub fn check_live(&self, native: &dyn ConsensusApi) -> io::Result<()> {
        let (state, daa, tip) = anchor_state(native, self.close)?;
        if !matches!(state, AnchorState::Finalized)
            || !(self.terms.open_daa..self.terms.deadline_exclusive_daa).contains(&daa)
            || !native.is_chain_ancestor_of(self.certificate, tip).map_err(io::Error::other)?
        {
            return Err(invalid("derived EXEC input lost its live certificate or original window"));
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC input generation moved"));
        }
        Ok(())
    }
    pub fn attempt(&self) -> &[u8; 32] {
        &self.attempt
    }
    pub fn registration_key(&self) -> &[u8; 64] {
        &self.registration_key
    }
    pub fn tokens(&self) -> &[u32] {
        &self.tokens
    }
    pub fn terms(&self) -> &ExecInputTerms<'_> {
        self.terms
    }
}

impl<'a> FrozenExecInputs<'a> {
    /// Use the gate checked against these exact consumers for holder signing and publication.
    pub fn publication_gate(&self) -> &NativePublicationGate {
        &self.gate
    }

    pub fn derive(
        &self,
        native: &dyn ConsensusApi,
        slot: u64,
        publication: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
    ) -> io::Result<DerivedExecInput<'a>> {
        let entry = self.batch.entries.get(&slot).ok_or_else(|| invalid("unknown closed EXEC slot"))?;
        let (c, terms) = (&entry.context, entry.terms);
        let tip = native.get_sink();
        let header = native.get_header(tip).map_err(io::Error::other)?;
        if !(terms.open_daa..terms.deadline_exclusive_daa).contains(&header.daa_score) {
            return Err(invalid("EXEC job is outside its original window"));
        }
        let seed = self.gate.read_seed(native, publication, certificate, rows, q, limits)?;
        let (attempt, tokens) = if let Some(mode) = terms.qf1 {
            let frame = mode.admitted()?;
            let attempt = kaspa_consensus_core::palw_qf1::attempt(c, &seed).map_err(invalid)?;
            (attempt, frame.frame(&attempt))
        } else {
            let attempt: [u8; 32] = Sha3_256::new()
                .chain_update(b"MISAKA/private/FOW1/attempt/v1")
                .chain_update(c)
                .chain_update(seed.as_bytes())
                .finalize()
                .into();
            let mut tokens = Vec::with_capacity(35 + terms.job_tokens.len());
            tokens.extend([0, 1]);
            tokens.extend(attempt.iter().map(|&b| 3 + u32::from(b)));
            tokens.push(2);
            tokens.extend_from_slice(terms.job_tokens);
            (attempt, tokens)
        };
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC job generation moved"));
        }
        Ok(DerivedExecInput {
            close: self.close,
            certificate,
            attempt,
            registration_key: reg_key(c),
            #[cfg(feature = "private-tb1-tir")]
            context_digest: Hash64::from_bytes(*blake2b_simd::blake2b(c).as_array()),
            #[cfg(feature = "private-tb1-tir")]
            owner: entry.owner,
            #[cfg(feature = "private-tb1-tir")]
            slot,
            tokens,
            terms,
            #[cfg(feature = "private-tb1-tir")]
            chain: self.batch.rounds.exec.chain_id,
        })
    }
}

#[cfg(feature = "private-tb1-tir")]
impl DerivedExecInput<'_> {
    /// Execute the held TIR model after the private input gate. The result is
    /// an ordinary traced inference, with no FOW1 readout, ticket or credit.
    pub fn replay_tir(
        &self,
        native: &dyn ConsensusApi,
        artifact: &misaka_palw_tir_exec::node::TirArtifactV1,
    ) -> Result<misaka_palw_tir_exec::node::TirJobRunV1, String> {
        use kaspa_consensus_core::{
            palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_job_context_v1},
            palw_tir_step_v1::PalwTirStepSpaceV1,
        };
        use misaka_palw_tir_exec::node::TirClassRunnerV1;
        self.check_live(native).map_err(|e| e.to_string())?;
        let terms = self.terms;
        if terms.qf1.is_some() {
            return Err("QF1 requires its registered 36-layer A16 program and readout".into());
        }
        let (root, _) = artifact.inventory_root()?;
        let class = artifact.class()?;
        let class_id = class.class_id(&root);
        if root != terms.model_root || class_id != terms.class_id || class.tokenizer_id != terms.tokenizer {
            return Err("held TIR artifact differs from signed EXEC input".into());
        }
        let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
        let facts = PalwTirJobFactsV1::of(&class, &space.program, class_id);
        if facts.token_bound != terms.vocab || facts.max_context != terms.max_positions {
            return Err("held TIR geometry differs from signed EXEC input".into());
        }
        let prefill = u32::try_from(self.tokens.len()).map_err(|_| "EXEC prefill overflow")?;
        let mut ctx = palw_tir_job_context_v1(&facts, (prefill, terms.decode_tokens));
        ctx.network_id = self.chain.to_vec();
        ctx.job_id = Hash64::from_bytes(self.registration_key);
        ctx.execution_seed = self.attempt;
        ctx.tokenizer_id = class.tokenizer_id;
        ctx.prompt_token_ids_hash = prompt_token_ids_root_v1(&self.tokens).map_err(|e| e.to_string())?;
        let run = TirClassRunnerV1::new(&space, artifact.plan(), artifact.params(), class_id)?.run(
            &ctx,
            &self.tokens,
            terms.max_leaves,
            true,
            &mut |_| {},
        )?;
        self.check_live(native).map_err(|e| e.to_string())?;
        Ok(run)
    }
}

#[cfg(feature = "private-tb1-tir")]
impl DerivedExecInput<'_> {
    /// Replay the owner-pinned instance on the authenticated QF1 input. Recheck
    /// the native window and certificate after numerical execution.
    pub fn replay_qf1_local(
        &self,
        native: &dyn ConsensusApi,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    ) -> Result<misaka_palw_tir_exec::node::qf1::LocalAnswer, String> {
        self.check_qf1_instance(native, instance)?;
        let terms = self.terms;
        let answer = instance.replay(
            &self.tokens,
            terms.decode_tokens,
            terms.max_leaves,
            Hash64::from_bytes(self.registration_key),
            self.attempt,
        )?;
        self.check_live(native).map_err(|e| e.to_string())?;
        Ok(answer)
    }
    fn check_qf1_instance(
        &self,
        native: &dyn ConsensusApi,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    ) -> Result<(), String> {
        self.check_live(native).map_err(|e| e.to_string())?;
        let terms = self.terms;
        let frame = terms.qf1.ok_or("signed EXEC input has no QF1 frame")?.admitted().map_err(|e| e.to_string())?;
        let id = instance.identity();
        if (id.descriptor, id.inventory, id.class, id.tokenizer)
            != (frame.instance_descriptor, terms.model_root, terms.class_id, terms.tokenizer)
        {
            return Err("local QF1 instance differs from owner-signed registration".into());
        }
        Ok(())
    }

    /// Produce from the signed registration and live EXEC without a declared T.
    pub fn produce_qf1_original(
        &self,
        native: &dyn ConsensusApi,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
        open: Option<u64>,
    ) -> Result<LocalOriginal<'_, '_>, String> {
        self.check_qf1_instance(native, instance)?;
        let trace = instance.produce_trace(
            &self.tokens,
            self.terms.decode_tokens,
            self.terms.max_leaves,
            Hash64::from_bytes(self.registration_key),
            self.attempt,
            self.context_digest,
            open,
        )?;
        self.check_live(native).map_err(|e| e.to_string())?;
        Ok(LocalOriginal { input: self, trace })
    }

    /// Re-execute and disclose a typed trace leaf against the same owner-pinned
    /// job. T comes from the original output; truncation or extension fails.
    pub fn replay_qf1_original(
        &self,
        native: &dyn ConsensusApi,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
        answer_len: u32,
        open: Option<u64>,
    ) -> Result<LocalOriginal<'_, '_>, String> {
        self.check_qf1_instance(native, instance)?;
        let trace = instance.replay_trace(
            &self.tokens,
            self.terms.decode_tokens,
            self.terms.max_leaves,
            Hash64::from_bytes(self.registration_key),
            self.attempt,
            misaka_palw_tir_exec::node::qf1::TraceRequest { context_digest: self.context_digest, answer_len, open },
        )?;
        self.check_live(native).map_err(|e| e.to_string())?;
        Ok(LocalOriginal { input: self, trace })
    }
}

#[cfg(feature = "private-tb1-tir")]
mod original;
#[cfg(feature = "private-tb1-tir")]
pub use original::{LOCAL_ORIGINAL_DOMAIN, LocalOriginal, LocalOriginalBody};

#[cfg(feature = "private-tb1-tir")]
pub mod panel_cut;

#[cfg(feature = "private-tb1-tir")]
pub mod stop_case;

#[cfg(feature = "private-tb1-tir")]
pub mod end_case;

#[cfg(feature = "private-tb1-tir")]
pub mod terminal_case;
