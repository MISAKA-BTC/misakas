//! Private RFC-0059/0062 input stage. The close block commits to the entire
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

pub const REGISTRATION_DOMAIN: &[u8] = b"MISAKA/private/FOW1/input-owner/v1";

/// The fixed token interface of this private profile is BOS=0, POW=1, SEP=2,
/// ByteToken[b]=3+b. The exact model/class and its tokenizer are bound before release.
pub struct ExecInputTerms<'a> {
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
    pub open_daa: u64,
    pub deadline_exclusive_daa: u64,
}

impl ExecInputTerms<'_> {
    fn context(
        &self,
        exec: &Tb1MiningRound,
        panel: &Tb1MiningRound,
        owner: &MLDSA65VerificationKey,
        max_tokens: usize,
    ) -> io::Result<Vec<u8>> {
        let n = u32::try_from(self.job_tokens.len()).map_err(|_| invalid("TB1 job too long"))?;
        let positions = n.checked_add(35).and_then(|n| n.checked_add(self.decode_tokens.checked_sub(1)?));
        if exec.purpose != Tb1MiningPurpose::Exec
            || panel.purpose != Tb1MiningPurpose::Panel
            || exec.chain_id != panel.chain_id
            || panel.close_blue_score <= exec.close_blue_score
            || self.vocab < 259
            || self.max_leaves == 0
            || n == 0
            || self.job_tokens.len() > max_tokens
            || positions.is_none_or(|n| n > self.max_positions)
            || self.job_tokens.iter().any(|&id| id >= self.vocab)
            || self.open_daa >= self.deadline_exclusive_daa
            || self.target[0] > 1
            || (self.target[0] == 1 && self.target[1..].iter().any(|&b| b != 0))
        {
            return Err(invalid("invalid private EXEC registration context"));
        }
        let job = prompt_token_ids_root_v1(self.job_tokens).map_err(|_| invalid("invalid job token commitment"))?;
        let mut c = Vec::new();
        c.extend_from_slice(b"MISAKA/private/FOW1/TIR-input/012-byte3/v1");
        for field in [
            exec.id(),
            panel.id(),
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
        Ok(c)
    }

    pub fn registration_key(
        &self,
        exec: &Tb1MiningRound,
        panel: &Tb1MiningRound,
        owner: &MLDSA65VerificationKey,
        max_tokens: usize,
    ) -> io::Result<[u8; 64]> {
        Ok(reg_key(&self.context(exec, panel, owner, max_tokens)?))
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
}

pub struct ExecInputBatch<'a> {
    root: [u8; 64],
    round: [u8; 64],
    #[cfg(feature = "private-tb1-tir")]
    chain: [u8; 64],
    entries: BTreeMap<u64, (Vec<u8>, &'a ExecInputTerms<'a>)>,
}

impl<'a> ExecInputBatch<'a> {
    /// Validate each signed preimage, reject any slot or RegKey alias and
    /// hash in slot order. ML-DSA signing randomness cannot change a launch.
    pub fn from_signed(
        exec: &Tb1MiningRound,
        panel: &Tb1MiningRound,
        rows: &[SignedExecInput<'a>],
        max_records: usize,
        max_job_tokens: usize,
    ) -> io::Result<Self> {
        if rows.len() > max_records {
            return Err(invalid("EXEC batch exceeds registration budget"));
        }
        let count = u32::try_from(rows.len()).map_err(|_| invalid("EXEC registration count overflow"))?;
        let mut entries = BTreeMap::new();
        let mut keys = BTreeSet::new();
        let mut allocations = BTreeMap::new();
        for row in rows {
            let c = row.terms.context(exec, panel, row.owner, max_job_tokens)?;
            let key = reg_key(&c);
            if !keys.insert(key) || entries.insert(row.slot, (c, row.terms)).is_some() {
                return Err(invalid("duplicate EXEC slot or RegKey"));
            }
            ml_dsa_65::verify(row.owner, &key, REGISTRATION_DOMAIN, row.signature)
                .map_err(|_| invalid("invalid EXEC owner signature"))?;
            allocations.insert(row.slot, (key, row.funding_ref));
        }
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/FOW1/exec-input-set/v1")
            .chain_update(exec.id())
            .chain_update(count.to_be_bytes());
        for (slot, (key, funding)) in allocations {
            h.update(slot.to_be_bytes());
            h.update(key);
            h.update(funding);
        }
        Ok(Self {
            root: h.finalize().into(),
            round: exec.id(),
            #[cfg(feature = "private-tb1-tir")]
            chain: exec.chain_id,
            entries,
        })
    }

    pub fn root(&self) -> [u8; 64] {
        self.root
    }

    pub fn observe(
        self,
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
    ) -> io::Result<FrozenExecInputs<'a>> {
        if closure.round.purpose != Tb1MiningPurpose::Exec
            || closure.round.id() != self.round
            || closure.consumer_set_root != self.root
        {
            return Err(invalid("EXEC input set differs from closure"));
        }
        let header = native.get_header(close).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        for (_, terms) in self.entries.values() {
            if terms.policy != closure.policy_root
                || !(terms.open_daa..terms.deadline_exclusive_daa).contains(&header.daa_score)
                || !native.is_chain_ancestor_of(terms.registration_anchor, parent).map_err(io::Error::other)?
            {
                return Err(invalid("EXEC input outside its registration history or policy"));
            }
        }
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
        let (c, terms) = self.batch.entries.get(&slot).ok_or_else(|| invalid("unknown closed EXEC slot"))?;
        let tip = native.get_sink();
        let header = native.get_header(tip).map_err(io::Error::other)?;
        if !(terms.open_daa..terms.deadline_exclusive_daa).contains(&header.daa_score) {
            return Err(invalid("EXEC job is outside its original window"));
        }
        let seed = self.gate.read_seed(native, publication, certificate, rows, q, limits)?;
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
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC job generation moved"));
        }
        Ok(DerivedExecInput {
            close: self.close,
            certificate,
            attempt,
            registration_key: reg_key(c),
            tokens,
            terms,
            #[cfg(feature = "private-tb1-tir")]
            chain: self.batch.chain,
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
