//! Prospective paid TIR acquisition state, not a native admission/conviction door.
//! Raw chunks MUST be separate immutable consensus-authenticated rows, atomically
//! written with the returned metadata. An unrooted local CAS is only a cache.
//! This module neither changes legacy reservations nor creates custody grants.
//! Independent own-model execution/indexes, generic semantic/dissection/terminal
//! SUM, native global-map amplification and inclusion capacity are not established
//! by this acquisition quote. Without those integrations it must remain dormant.
use crate::palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_staged_court_state_v1::PalwSharedCourtV1;
use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1, step_opening_root_capped_v1};
use crate::palw_tir_public_source_v1::{
    PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1, PalwTirPublicAuthenticatedIdsV1, PalwTirPublicAuthenticatedUnitV1, PalwTirPublicDemandScopeV1,
    PalwTirPublicIdsAssemblerV1, PalwTirPublicRequestV1, PalwTirPublicResponseV1, PalwTirPublicSourceV1,
};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1: usize = 16_384;
const BODY_DOMAIN: &[u8] = b"misaka-palw/tir-public-body/v1";
fn digest(domain: &[u8], bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(bytes).as_bytes().try_into().expect("64"))
}
fn exact(value: u128) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| "TIR acquisition resource envelope overflows u64".into())
}
fn product(a: u64, b: u64) -> Result<u64, String> {
    exact(u128::from(a) * u128::from(b))
}
fn sum(values: &[u64]) -> Result<u64, String> {
    values.iter().try_fold(0u64, |total, value| total.checked_add(*value).ok_or_else(|| "TIR resource sum overflow".into()))
}
fn depth(count: u64) -> u8 {
    if count <= 1 { 0 } else { (64 - (count - 1).leading_zeros()) as u8 }
}
fn logical(request: PalwTirPublicRequestV1) -> u64 {
    if let PalwTirPublicRequestV1::StepRun { count, .. } = request { u64::from(count) } else { 1 }
}

/// Typed cumulative work, not interchangeable with a host-memory reservation.
/// Arithmetic is the registered canonical MAC-equivalent dimension, not CPU
/// time; traffic is kept separately instead of treating a byte as an operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicWorkV1 {
    pub attempts: u64,
    pub io_bytes: u64,
    pub hash_bytes: u64,
    pub copy_bytes: u64,
    pub parser_steps: u64,
    pub id_copy_words: u64,
    pub tree_hashes: u64,
    pub replay_leaves: u64,
    pub replay_positions: u64,
    pub replay_id_words: u64,
    pub arithmetic_work: u128,
    pub traffic_bytes: u128,
    pub eval_elements: u64,
    pub eval_terms: u64,
    pub source_reads: u64,
    pub target_requests: u64,
}
impl PalwTirPublicWorkV1 {
    pub fn checked_mul_v1(self, count: u64) -> Result<Self, String> {
        let mut next = Self::default();
        macro_rules! mul { ($($f:ident),+ $(,)?) => { $(next.$f = self.$f.checked_mul(count).ok_or("TIR cumulative work multiplication overflow")?;)+ }; }
        mul!(
            attempts,
            io_bytes,
            hash_bytes,
            copy_bytes,
            parser_steps,
            id_copy_words,
            tree_hashes,
            replay_leaves,
            replay_positions,
            replay_id_words,
            eval_elements,
            eval_terms,
            source_reads,
            target_requests
        );
        next.arithmetic_work = self.arithmetic_work.checked_mul(u128::from(count)).ok_or("TIR arithmetic multiplication overflow")?;
        next.traffic_bytes = self.traffic_bytes.checked_mul(u128::from(count)).ok_or("TIR traffic multiplication overflow")?;
        Ok(next)
    }
    pub fn checked_add_v1(self, other: Self) -> Result<Self, String> {
        let mut next = self;
        macro_rules! add { ($($f:ident),+ $(,)?) => { $(next.$f = next.$f.checked_add(other.$f).ok_or("TIR cumulative work overflow")?;)+ }; }
        add!(
            attempts,
            io_bytes,
            hash_bytes,
            copy_bytes,
            parser_steps,
            id_copy_words,
            tree_hashes,
            replay_leaves,
            replay_positions,
            replay_id_words,
            arithmetic_work,
            traffic_bytes,
            eval_elements,
            eval_terms,
            source_reads,
            target_requests
        );
        Ok(next)
    }
    pub fn covers_v1(&self, used: &Self) -> bool {
        macro_rules! covers { ($($f:ident),+ $(,)?) => { true $(&& self.$f >= used.$f)+ }; }
        covers!(
            attempts,
            io_bytes,
            hash_bytes,
            copy_bytes,
            parser_steps,
            id_copy_words,
            tree_hashes,
            replay_leaves,
            replay_positions,
            replay_id_words,
            arithmetic_work,
            traffic_bytes,
            eval_elements,
            eval_terms,
            source_reads,
            target_requests
        )
    }
}

/// Frozen by a future native full-SUM quote. These are not operational defaults
/// provided by the backend, nor an extension of historical court acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirPublicWorkRulesV1 {
    pub max_step_leaf_count: u64,
    pub prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    pub max_eval_elements: u64,
    pub max_eval_terms: u64,
    pub max_target_requests: u64,
    pub max_source_reads: u64,
    pub maximum_probe_copy_bytes: u64,
    /// Native full planner's executor restoration/working-set + canonical spool
    /// I/O bound. It must not come from a host Need or a signed caller ceiling.
    pub maximum_replay_io_bytes: u64,
}
impl BorshSerialize for PalwTirPublicWorkRulesV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.max_step_leaf_count.serialize(writer)?;
        let form = match self.prompt_form {
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => 0u8,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => 1u8,
        };
        form.serialize(writer)?;
        self.max_eval_elements.serialize(writer)?;
        self.max_eval_terms.serialize(writer)?;
        self.max_target_requests.serialize(writer)?;
        self.max_source_reads.serialize(writer)?;
        self.maximum_probe_copy_bytes.serialize(writer)?;
        self.maximum_replay_io_bytes.serialize(writer)
    }
}
impl BorshDeserialize for PalwTirPublicWorkRulesV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let max_step_leaf_count = BorshDeserialize::deserialize_reader(reader)?;
        let prompt_form = match u8::deserialize_reader(reader)? {
            0 => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            1 => crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR work prompt form is not canonical")),
        };
        Ok(Self {
            max_step_leaf_count,
            prompt_form,
            max_eval_elements: BorshDeserialize::deserialize_reader(reader)?,
            max_eval_terms: BorshDeserialize::deserialize_reader(reader)?,
            max_target_requests: BorshDeserialize::deserialize_reader(reader)?,
            max_source_reads: BorshDeserialize::deserialize_reader(reader)?,
            maximum_probe_copy_bytes: BorshDeserialize::deserialize_reader(reader)?,
            maximum_replay_io_bytes: BorshDeserialize::deserialize_reader(reader)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicWorkAllocationV1 {
    pub source_id: Hash64,
    pub rules: PalwTirPublicWorkRulesV1,
    pub maximum: PalwTirPublicWorkV1,
    /// Independent prepaid actor lanes. A colluding responder cannot spend the
    /// outside owner's preparation/probe allowance or another literal slot's.
    pub per_actor: PalwTirPublicWorkV1,
    pub maximum_custodians: u64,
    pub maximum_restarts: u64,
    /// ADDITIONAL work/carrier storage. Admission must buy acquisition + this
    /// envelope as a SUM, not reinterpret an old acquisition allocation.
    pub envelope: PalwRenderingResourcePlanV1,
}

/// Registered-program/source-derived quote component only. It does not admit a
/// sidecar, obtain custody grants or establish the full semantic/terminal SUM.
pub fn derive_tir_public_work_allocation_v1(
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    source: &PalwTirPublicSourceV1,
    plan: &PalwTirPublicOwnedPlanV1,
    rules: PalwTirPublicWorkRulesV1,
) -> Result<PalwTirPublicWorkAllocationV1, String> {
    if source.header_v1() != header
        || source.source_id_v1() != plan.source_id
        || rules.max_step_leaf_count < source.leaf_count_v1()
        || rules.maximum_probe_copy_bytes == 0
        || *plan != PalwTirPublicOwnedPlanV1::derive_v1(source, plan.maximum_custodians, plan.config)?
    {
        return Err("TIR work quote changes native source/geometry/frozen rules".into());
    }
    let binding = header.resolve_registered_layout_v1(record, layout, header.committed_execution_root, rules.max_step_leaf_count)?;
    let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, rules.max_step_leaf_count).map_err(|e| e.to_string())?;
    let shape = crate::palw_tir_work_v1::palw_tir_work_shape_v1(&verified.space.program).map_err(|e| e.to_string())?;
    let facts = crate::palw_canonical_work_v1::PalwCanonicalExecutionFactsV1::uncached(
        header.job_context.declared_prefill_tokens,
        header.job_context.exact_decode_tokens,
    );
    let model = shape.work_v1(&facts).map_err(|e| e.to_string())?;
    let passes = plan.config.prices.reorg_replay_moves.checked_add(1).ok_or("TIR restart allowance overflow")?;
    let slots = plan.maximum_custodians.checked_add(1).ok_or("TIR work slots overflow")?;
    let preparations = product(slots, passes)?;
    let n = source.leaf_count_v1();
    let d = u64::from(source.decode_count_v1());
    let p = u64::from(header.job_context.declared_prefill_tokens);
    let positions = p.checked_add(d).and_then(|v| v.checked_sub(1)).ok_or("TIR work positions overflow")?;
    let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let probes = product(n.checked_add(1).ok_or("TIR work probes overflow")?, passes)?;
    let body_records = plan.envelope.maximum_stream_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64);
    let attempts = product(
        16,
        sum(&[
            plan.envelope.maximum_moves,
            product(preparations, sum(&[n, product(d, tiles)?, positions, d, 32])?)?,
            product(body_records, passes)?,
            product(probes, 4)?,
        ])?,
    )?;
    let arithmetic_work = model.arithmetic_mac_eq().checked_mul(u128::from(preparations)).ok_or("TIR arithmetic work overflow")?;
    let traffic_bytes = model.traffic_bytes().checked_mul(u128::from(preparations)).ok_or("TIR traffic work overflow")?;
    let io = sum(&[
        product(plan.envelope.maximum_stream_bytes, passes)?,
        exact(traffic_bytes)?,
        product(rules.maximum_replay_io_bytes, passes)?,
    ])?;
    // Durable publication/reopen may use 16KiB slices even when a native leaf
    // or a registered model cell is much larger. Pre-debit and local validation
    // attempts must cover those slices too, not only logical source responses.
    let attempts = sum(&[attempts, product(io.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64), product(passes, 4)?)?, 32])?;
    let program_parse = product(probes, record.program.len() as u64)?;
    let copy = sum(&[product(io, 8)?, product(product(probes, rules.maximum_probe_copy_bytes)?, 2)?, product(program_parse, 8)?])?;
    let replay_id_words = product(product(positions, p.checked_add(d).ok_or("TIR replay IDs overflow")?)?, preparations)?;
    let per_actor = PalwTirPublicWorkV1 {
        attempts,
        io_bytes: io,
        copy_bytes: copy,
        hash_bytes: sum(&[product(io, 16)?, product(program_parse, 16)?])?,
        parser_steps: sum(&[product(io, 16)?, product(attempts, 65_536)?, product(program_parse, 16)?])?,
        id_copy_words: sum(&[product(product(d, plan.maximum_turns)?, product(slots, 8)?)?, product(replay_id_words, 4)?])?,
        tree_hashes: io.div_ceil(64),
        replay_leaves: product(n, preparations)?,
        replay_positions: product(positions, preparations)?,
        replay_id_words,
        arithmetic_work,
        traffic_bytes,
        eval_elements: product(probes, rules.max_eval_elements)?,
        eval_terms: product(probes, rules.max_eval_terms)?,
        source_reads: product(probes, rules.max_source_reads)?,
        target_requests: product(probes, rules.max_target_requests)?,
    };
    let maximum = per_actor.checked_mul_v1(slots)?;
    let parser = sum(&[
        maximum.parser_steps,
        maximum.io_bytes,
        maximum.copy_bytes,
        exact(maximum.arithmetic_work)?,
        maximum.eval_elements,
        maximum.eval_terms,
        maximum.source_reads,
        maximum.target_requests,
        product(maximum.id_copy_words, 4)?,
    ])?;
    // One global last receipt plus allocation/aggregate fields, and one spent
    // record + last receipt per protected actor. Each receipt may contain a full
    // 64-sibling history path (>4KiB); 512-byte actor estimates are insufficient.
    let row = sum(&[8192, product(slots, 8192)?])?;
    let bytes = product(maximum.attempts, sum(&[16_384, product(row, 8)?])?)?;
    let envelope = PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: source.vocab_v1(),
        maximum_stream_bytes: bytes,
        maximum_stream_leaves: maximum.attempts,
        maximum_eog_bytes: 0,
        maximum_moves: maximum.attempts,
        maximum_parser_steps: parser,
        maximum_blake_blocks: maximum.hash_bytes.div_ceil(128),
        inclusion_daa: product(maximum.attempts, plan.config.prices.inclusion_margin_daa)?,
        rent_sompi: sum(&[
            product(bytes, plan.config.prices.rent_per_stream_byte)?,
            product(row, plan.config.prices.rent_per_row_byte)?,
            product(parser, plan.config.prices.rent_per_parser_step)?,
        ])?,
        deposit_sompi: product(maximum.attempts, plan.config.prices.deposit_per_move)?,
        row_bytes: row,
        resident_ram_bytes: sum(&[product(row, 8)?, product(rules.maximum_probe_copy_bytes, 2)?])?,
    };
    Ok(PalwTirPublicWorkAllocationV1 {
        source_id: source.source_id_v1(),
        rules,
        maximum,
        per_actor,
        maximum_custodians: plan.maximum_custodians,
        maximum_restarts: plan.config.prices.reorg_replay_moves,
        envelope,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirPublicWorkPhaseV1 {
    Capture,
    Response,
    Ids,
    BodyRead,
    BodyAuthenticate,
    ConeProbe,
    Planning,
    OwnReplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicWorkScopeV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub turn: u64,
    pub request: PalwTirPublicRequestV1,
    pub actor: PalwBondKeyV2,
    pub phase: PalwTirPublicWorkPhaseV1,
    pub input_head: Hash64,
    pub input_offset: u64,
    pub slice: u64,
    /// Local preparation cursor commitment, not an authenticated source fact.
    pub cursor_id: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicWorkDebitV1 {
    pub expected_nonce: u64,
    pub previous_head: Hash64,
    pub scope: PalwTirPublicWorkScopeV1,
    pub work: PalwTirPublicWorkV1,
    pub restart: bool,
    pub history: Option<PalwStepOpeningV1>,
}
impl BorshDeserialize for PalwTirPublicWorkDebitV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let expected_nonce = BorshDeserialize::deserialize_reader(reader)?;
        let previous_head = BorshDeserialize::deserialize_reader(reader)?;
        let scope = BorshDeserialize::deserialize_reader(reader)?;
        let work = BorshDeserialize::deserialize_reader(reader)?;
        let restart = BorshDeserialize::deserialize_reader(reader)?;
        let history = match u8::deserialize_reader(reader)? {
            0 => None,
            1 => {
                let leaf_index = BorshDeserialize::deserialize_reader(reader)?;
                let leaf_hash = BorshDeserialize::deserialize_reader(reader)?;
                let count = u32::deserialize_reader(reader)?;
                if count > 64 {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR work history path exceeds u64 tree"));
                }
                let mut siblings = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    siblings.push(BorshDeserialize::deserialize_reader(reader)?);
                }
                Some(PalwStepOpeningV1 { leaf_index, leaf_hash, siblings })
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR work history option is noncanonical")),
        };
        Ok(Self { expected_nonce, previous_head, scope, work, restart, history })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicWorkReceiptV1 {
    pub debit: PalwTirPublicWorkDebitV1,
    pub accepted_daa: u64,
    pub cumulative: PalwTirPublicWorkV1,
    /// Derived aggregate ordinal, NOT a signed shared-lane precondition.
    pub ordinal: u64,
}
impl PalwTirPublicWorkReceiptV1 {
    pub fn hash_v1(&self) -> Result<Hash64, String> {
        Ok(digest(b"misaka-palw/tir-work-receipt/v1", &borsh::to_vec(self).map_err(|e| e.to_string())?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicWorkStateV1 {
    pub allocation: PalwTirPublicWorkAllocationV1,
    pub spent: PalwTirPublicWorkV1,
    pub nonce: u64,
    pub head: Hash64,
    pub restarts: u64,
    pub last: Option<PalwTirPublicWorkReceiptV1>,
    pub actors: BTreeMap<PalwBondKeyV2, PalwTirPublicActorWorkV1>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicActorWorkV1 {
    pub spent: PalwTirPublicWorkV1,
    pub grants: u64,
    pub restarts: u64,
    /// Another actor's grant cannot overwrite this actor's unclaimed receipt.
    pub last: Option<PalwTirPublicWorkReceiptV1>,
}

fn apply_work_debit_v1(
    state: &mut PalwTirPublicWorkStateV1,
    signer: PalwBondKeyV2,
    now: u64,
    debit: &PalwTirPublicWorkDebitV1,
) -> Result<PalwTirPublicWorkReceiptV1, String> {
    let actor = state.actors.get(&signer).cloned().unwrap_or_default();
    let head = actor.last.as_ref().map(|r| r.hash_v1()).transpose()?.unwrap_or_default();
    if debit.expected_nonce != actor.grants || debit.previous_head != head || debit.work.attempts == 0 {
        return Err("TIR work debit repeats or changes its protected actor nonce/head".into());
    }
    let cumulative = state.spent.checked_add_v1(debit.work)?;
    let nonce = state.nonce.checked_add(1).ok_or("TIR work nonce overflow")?;
    let restarts = state.restarts.checked_add(u64::from(debit.restart)).ok_or("TIR restart overflow")?;
    let mut actor = PalwTirPublicActorWorkV1 {
        spent: actor.spent.checked_add_v1(debit.work)?,
        grants: actor.grants.checked_add(1).ok_or("TIR actor nonce overflow")?,
        restarts: actor.restarts.checked_add(u64::from(debit.restart)).ok_or("TIR actor restart overflow")?,
        last: None,
    };
    if !state.allocation.maximum.covers_v1(&cumulative)
        || nonce > state.allocation.envelope.maximum_moves
        || !state.allocation.per_actor.covers_v1(&actor.spent)
        || actor.grants > state.allocation.per_actor.attempts
        || actor.restarts > state.allocation.maximum_restarts
        || restarts
            > product(
                state.allocation.maximum_restarts,
                state.allocation.maximum_custodians.checked_add(1).ok_or("TIR work slots overflow")?,
            )?
    {
        return Err("TIR original cumulative work/restart allocation is exhausted".into());
    }
    let receipt = PalwTirPublicWorkReceiptV1 { debit: debit.clone(), accepted_daa: now, cumulative, ordinal: nonce };
    let head = receipt.hash_v1()?;
    actor.last = Some(receipt.clone());
    state.spent = cumulative;
    state.nonce = nonce;
    state.head = head;
    state.restarts = restarts;
    state.last = Some(receipt.clone());
    state.actors.insert(signer, actor);
    Ok(receipt)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicPreparationPlanV1 {
    pub source_id: Hash64,
    pub maximum_custodians: u64,
    pub maximum_restarts: u64,
    pub maximum_slices_per_actor: u64,
    pub maximum_prepared_bytes: u64,
    pub preparation_daa: u64,
    /// Ready/default metadata and carriers only. Full admission separately pays
    /// claim-wide cold work AND outside-owner work; their spent pools differ.
    pub envelope: PalwRenderingResourcePlanV1,
}
pub fn derive_tir_public_preparation_plan_v1(
    source: &PalwTirPublicSourceV1,
    work: &PalwTirPublicWorkAllocationV1,
    acquisition: &PalwTirPublicOwnedPlanV1,
) -> Result<PalwTirPublicPreparationPlanV1, String> {
    if work.source_id != source.source_id_v1()
        || acquisition.source_id != source.source_id_v1()
        || work.maximum_custodians != acquisition.maximum_custodians
    {
        return Err("TIR preparation quote changes original source or native custody upper".into());
    }
    let p = u64::from(source.header_v1().job_context.declared_prefill_tokens);
    let d = u64::from(source.decode_count_v1());
    let positions = p.checked_add(d).and_then(|n| n.checked_sub(1)).ok_or("TIR preparation position overflow")?;
    let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let one_pass = sum(&[source.leaf_count_v1().max(positions), product(d, tiles)?, p.div_ceil(1024), d.div_ceil(1024), 32])?;
    let durable_slices = product(work.per_actor.io_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64), 2)?;
    let maximum_slices_per_actor =
        product(sum(&[one_pass, durable_slices])?, work.maximum_restarts.checked_add(1).ok_or("TIR preparation retries overflow")?)?;
    if product(maximum_slices_per_actor, 2)? > work.per_actor.attempts {
        return Err("TIR work quote does not cover cold/publication pre-debits and validation slices".into());
    }
    let upper = acquisition.maximum_custodians;
    let prep_moves = product(upper, maximum_slices_per_actor.checked_add(1).ok_or("TIR Ready moves overflow")?)?;
    let moves = sum(&[prep_moves, upper])?;
    let row = sum(&[4096, product(upper, 1024)?])?;
    let bytes = product(moves, sum(&[16_384, product(row, 8)?])?)?;
    let parser = product(bytes, 32)?;
    let margin = acquisition.config.prices.inclusion_margin_daa;
    let preparation_daa = sum(&[acquisition.config.acquisition_daa, work.envelope.inclusion_daa, product(prep_moves, margin)?])?;
    let envelope = PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: source.vocab_v1(),
        maximum_stream_bytes: bytes,
        maximum_stream_leaves: moves,
        maximum_eog_bytes: 0,
        maximum_moves: moves,
        maximum_parser_steps: parser,
        maximum_blake_blocks: parser.div_ceil(128),
        inclusion_daa: product(moves, margin)?,
        row_bytes: row,
        resident_ram_bytes: product(row, 8)?,
        rent_sompi: sum(&[
            product(bytes, acquisition.config.prices.rent_per_stream_byte)?,
            product(row, acquisition.config.prices.rent_per_row_byte)?,
            product(parser, acquisition.config.prices.rent_per_parser_step)?,
        ])?,
        deposit_sompi: product(moves, acquisition.config.prices.deposit_per_move)?,
    };
    Ok(PalwTirPublicPreparationPlanV1 {
        source_id: source.source_id_v1(),
        maximum_custodians: upper,
        maximum_restarts: work.maximum_restarts,
        maximum_slices_per_actor,
        maximum_prepared_bytes: work.per_actor.io_bytes,
        preparation_daa,
        envelope,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicPreparationContractV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub original_accepted_daa: u64,
    pub preparation_until_daa: u64,
    pub collect_until_daa: u64,
    pub material_until_daa: u64,
    pub producer: PalwBondKeyV2,
    pub custodians: BTreeSet<PalwBondKeyV2>,
    pub drawn_panel: BTreeSet<PalwBondKeyV2>,
    pub plan: PalwTirPublicPreparationPlanV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicReadyV1 {
    pub descriptor: crate::palw_backend::PalwTirPublicPreparedDescriptorV1,
    pub accepted_daa: u64,
    pub work_receipt: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicPreparationV1 {
    contract: PalwTirPublicPreparationContractV1,
    work: PalwTirPublicWorkStateV1,
    ready: BTreeMap<PalwBondKeyV2, PalwTirPublicReadyV1>,
    defaulted: BTreeSet<PalwBondKeyV2>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicPreparationDefaultV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub actor: PalwBondKeyV2,
    pub original_accepted_daa: u64,
    pub preparation_until_daa: u64,
}
impl PalwTirPublicPreparationV1 {
    /// Mandatory contextual ingress: cap the carried bytes BEFORE decoding and
    /// check every collection count against the caller's actual native upper,
    /// not the self-declared plan. Generic snapshot Deserialize is not this API.
    pub fn decode_bounded_v1(bytes: &[u8], native_upper: u64, native_row_bytes: u64) -> Result<Self, String> {
        if bytes.len() as u64 > native_row_bytes {
            return Err("TIR preparation exceeds native byte cap before decoding".into());
        }
        fn set<R: std::io::Read>(r: &mut R, upper: u64) -> Result<BTreeSet<PalwBondKeyV2>, String> {
            let count = u32::deserialize_reader(r).map_err(|e| e.to_string())?;
            if u64::from(count) > upper {
                return Err("TIR preparation signer count exceeds native upper".into());
            }
            let mut result = BTreeSet::new();
            for _ in 0..count {
                let key = PalwBondKeyV2::deserialize_reader(r).map_err(|e| e.to_string())?;
                if result.last().is_some_and(|last| *last >= key) {
                    return Err("TIR preparation signers are not canonical".into());
                }
                result.insert(key);
            }
            Ok(result)
        }
        fn map<R: std::io::Read, V: BorshDeserialize>(r: &mut R, upper: u64) -> Result<BTreeMap<PalwBondKeyV2, V>, String> {
            let count = u32::deserialize_reader(r).map_err(|e| e.to_string())?;
            if u64::from(count) > upper {
                return Err("TIR preparation map exceeds native upper before allocation".into());
            }
            let mut result = BTreeMap::new();
            for _ in 0..count {
                let key = PalwBondKeyV2::deserialize_reader(r).map_err(|e| e.to_string())?;
                if result.last_key_value().is_some_and(|(last, _)| *last >= key) {
                    return Err("TIR preparation map is not canonical".into());
                }
                result.insert(key, V::deserialize_reader(r).map_err(|e| e.to_string())?);
            }
            Ok(result)
        }
        let mut r = bytes;
        macro_rules! read {
            () => {
                BorshDeserialize::deserialize_reader(&mut r).map_err(|e| e.to_string())?
            };
        }
        let contract = PalwTirPublicPreparationContractV1 {
            claim: read!(),
            allocation_id: read!(),
            source_id: read!(),
            original_accepted_daa: read!(),
            preparation_until_daa: read!(),
            collect_until_daa: read!(),
            material_until_daa: read!(),
            producer: read!(),
            custodians: set(&mut r, native_upper)?,
            drawn_panel: set(&mut r, native_upper)?,
            plan: read!(),
        };
        if contract.plan.maximum_custodians > native_upper {
            return Err("TIR preparation plan exceeds actual native signer capacity".into());
        }
        let allocation: PalwTirPublicWorkAllocationV1 = read!();
        if allocation.maximum_custodians > native_upper {
            return Err("TIR preparation work exceeds actual native signer capacity".into());
        }
        let work = PalwTirPublicWorkStateV1 {
            allocation,
            spent: read!(),
            nonce: read!(),
            head: read!(),
            restarts: read!(),
            last: read!(),
            actors: map(&mut r, native_upper)?,
        };
        let ready = map(&mut r, native_upper)?;
        let defaulted = set(&mut r, native_upper)?;
        if !r.is_empty() {
            return Err("TIR preparation has trailing native metadata bytes".into());
        }
        let value = Self { contract, work, ready, defaulted };
        value.check_v1()?;
        Ok(value)
    }
    /// Pure structural constructor, NOT custody qualification or native Open.
    /// Native admission must authenticate the ORIGINAL full allocation and the
    /// new P/Valid grants before retaining this original-clock contract.
    pub fn new_v1(
        contract: PalwTirPublicPreparationContractV1,
        source: &PalwTirPublicSourceV1,
        allocation: PalwTirPublicWorkAllocationV1,
        acquisition: &PalwTirPublicOwnedPlanV1,
    ) -> Result<Self, String> {
        if contract.claim == Hash64::default()
            || contract.allocation_id == Hash64::default()
            || contract.source_id != source.source_id_v1()
            || contract.plan != derive_tir_public_preparation_plan_v1(source, &allocation, acquisition)?
            || contract.original_accepted_daa.checked_add(contract.plan.preparation_daa) != Some(contract.preparation_until_daa)
            || contract.preparation_until_daa >= contract.collect_until_daa
            || contract.collect_until_daa > contract.material_until_daa
            || !contract.custodians.contains(&contract.producer)
            || contract.custodians.len() as u64 > contract.plan.maximum_custodians
            || contract.drawn_panel.len() as u64 > contract.plan.maximum_custodians
            || contract.custodians.iter().any(|actor| *actor != contract.producer && !contract.drawn_panel.contains(actor))
        {
            return Err("TIR preparation changes original claim/clock/qualified literal custody/quote".into());
        }
        Ok(Self {
            contract,
            work: PalwTirPublicWorkStateV1 {
                allocation,
                spent: Default::default(),
                nonce: 0,
                head: Hash64::default(),
                restarts: 0,
                last: None,
                actors: BTreeMap::new(),
            },
            ready: BTreeMap::new(),
            defaulted: BTreeSet::new(),
        })
    }
    pub fn contract_v1(&self) -> &PalwTirPublicPreparationContractV1 {
        &self.contract
    }
    pub fn pending_signers_v1(&self) -> Vec<PalwBondKeyV2> {
        self.contract
            .custodians
            .iter()
            .copied()
            .filter(|actor| !self.ready.contains_key(actor) && !self.defaulted.contains(actor))
            .collect()
    }
    pub fn check_v1(&self) -> Result<(), String> {
        let c = &self.contract;
        if c.claim == Hash64::default()
            || c.allocation_id == Hash64::default()
            || c.source_id != c.plan.source_id
            || c.original_accepted_daa.checked_add(c.plan.preparation_daa) != Some(c.preparation_until_daa)
            || c.preparation_until_daa >= c.collect_until_daa
            || c.collect_until_daa > c.material_until_daa
            || c.custodians.len() as u64 > c.plan.maximum_custodians
            || !c.custodians.contains(&c.producer)
            || c.drawn_panel.len() as u64 > c.plan.maximum_custodians
            || c.custodians.iter().any(|a| *a != c.producer && !c.drawn_panel.contains(a))
            || self.work.allocation.maximum_custodians != c.plan.maximum_custodians
            || self.work.allocation.maximum_restarts != c.plan.maximum_restarts
            || self.work.allocation.source_id != c.source_id
            || self.work.allocation.maximum
                != self
                    .work
                    .allocation
                    .per_actor
                    .checked_mul_v1(c.plan.maximum_custodians.checked_add(1).ok_or("TIR preparation work slots overflow")?)?
            || self.work.nonce > self.work.allocation.envelope.maximum_moves
            || self.ready.keys().chain(self.defaulted.iter()).any(|a| !c.custodians.contains(a))
            || self.defaulted.iter().any(|a| self.ready.contains_key(a))
            || self.work.actors.len() > c.custodians.len()
            || !self.work.allocation.maximum.covers_v1(&self.work.spent)
        {
            return Err("TIR claim-wide preparation changes fixed contract, duties or cumulative work".into());
        }
        let mut spent = PalwTirPublicWorkV1::default();
        let mut grants = 0u64;
        let mut restarts = 0u64;
        for (actor, lane) in &self.work.actors {
            if !c.custodians.contains(actor)
                || !self.work.allocation.per_actor.covers_v1(&lane.spent)
                || lane.grants > c.plan.maximum_slices_per_actor
                || lane.restarts > c.plan.maximum_restarts
                || lane.last.as_ref().is_none_or(|r| {
                    r.debit.scope.actor != *actor
                        || r.debit.scope.phase != PalwTirPublicWorkPhaseV1::Capture
                        || r.debit.scope.source_id != c.source_id
                        || r.debit.scope.claim != c.claim
                        || r.debit.scope.bundle_id != c.allocation_id
                        || r.debit.scope.owner != c.producer
                        || r.debit.scope.reserved_daa != c.original_accepted_daa
                        || r.debit.scope.turn != 0
                        || r.debit.scope.request != (PalwTirPublicRequestV1::StepLeaf { index: 0 })
                        || r.debit.scope.input_head != c.source_id
                        || r.debit.history.is_some()
                        || r.ordinal == 0
                        || r.ordinal > self.work.nonce
                        || !self.work.spent.covers_v1(&r.cumulative)
                        || r.debit.expected_nonce.checked_add(1) != Some(lane.grants)
                        || r.accepted_daa < c.original_accepted_daa
                        || r.accepted_daa >= c.preparation_until_daa
                })
            {
                return Err("TIR preparation work changes literal signer or original preparation phase".into());
            }
            spent = spent.checked_add_v1(lane.spent)?;
            grants = grants.checked_add(lane.grants).ok_or("TIR preparation grants overflow")?;
            restarts = restarts.checked_add(lane.restarts).ok_or("TIR preparation restarts overflow")?;
        }
        if spent != self.work.spent
            || grants != self.work.nonce
            || restarts != self.work.restarts
            || self.work.last.as_ref().map(|r| r.hash_v1()).transpose()?.unwrap_or_default() != self.work.head
            || self.work.last.as_ref().is_some_and(|r| r.ordinal != self.work.nonce || r.cumulative != self.work.spent)
            || (self.work.nonce == 0) != self.work.last.is_none()
        {
            return Err("TIR preparation work receipt/spent reconstruction mismatch".into());
        }
        for (actor, ready) in &self.ready {
            if ready.accepted_daa < c.original_accepted_daa
                || ready.accepted_daa >= c.preparation_until_daa
                || ready.descriptor.source_id != c.source_id
                || ready.descriptor.material_until_daa != c.material_until_daa
                || ready.descriptor.version != 1
                || ready.descriptor.manifest_digest == Hash64::default()
                || ready.descriptor.step_leaf_count == 0
                || ready.descriptor.vocab == 0
                || ready.descriptor.retained_bytes == 0
                || ready.descriptor.retained_bytes > c.plan.maximum_prepared_bytes
                || self.work.actors.get(actor).and_then(|a| a.last.as_ref()).map(|r| r.hash_v1()).transpose()?
                    != Some(ready.work_receipt)
            {
                return Err("TIR Ready changes fixed source/retention/debit/original deadline".into());
            }
        }
        Ok(())
    }
    pub fn check_source_v1(&self, source: &PalwTirPublicSourceV1, acquisition: &PalwTirPublicOwnedPlanV1) -> Result<(), String> {
        self.check_v1()?;
        if self.contract.source_id != source.source_id_v1()
            || self.contract.plan != derive_tir_public_preparation_plan_v1(source, &self.work.allocation, acquisition)?
        {
            return Err("TIR preparation differs from original native source/clock quote".into());
        }
        let h = source.header_v1();
        for ready in self.ready.values() {
            let d = &ready.descriptor;
            if d.class_id != h.job_context.shape_profile_id
                || d.artifact_root != h.artifact_root
                || d.execution_root != h.committed_execution_root
                || d.trace_root != h.full_logits_trace_root
                || d.step_root != h.step_merkle_root
                || d.step_leaf_count != source.leaf_count_v1()
                || d.decode_count != source.decode_count_v1()
                || d.vocab != source.vocab_v1()
            {
                return Err("TIR Ready descriptor changes original authenticated roots or index geometry".into());
            }
        }
        Ok(())
    }
    pub fn work_v1(&self) -> &PalwTirPublicWorkStateV1 {
        &self.work
    }
    pub fn ready_v1(&self, actor: &PalwBondKeyV2) -> Option<&PalwTirPublicReadyV1> {
        self.ready.get(actor)
    }
    pub fn defaulted_v1(&self, actor: &PalwBondKeyV2) -> bool {
        self.defaulted.contains(actor)
    }
    pub fn capture_debit_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        debit: &PalwTirPublicWorkDebitV1,
    ) -> Result<PalwTirPublicWorkReceiptV1, String> {
        let c = &self.contract;
        let s = &debit.scope;
        source.check_request_v1(&s.request)?;
        if now < c.original_accepted_daa
            || now >= c.preparation_until_daa
            || !c.custodians.contains(&signer)
            || self.ready.contains_key(&signer)
            || self.defaulted.contains(&signer)
            || s.phase != PalwTirPublicWorkPhaseV1::Capture
            || s.claim != c.claim
            || s.bundle_id != c.allocation_id
            || s.source_id != c.source_id
            || source.source_id_v1() != c.source_id
            || s.turn != 0
            || s.actor != signer
            || s.owner != c.producer
            || s.reserved_daa != c.original_accepted_daa
            || s.input_head != c.source_id
            || s.request != (PalwTirPublicRequestV1::StepLeaf { index: 0 })
            || debit.history.is_some()
            || self.work.actors.get(&signer).map_or(0, |a| a.grants) >= c.plan.maximum_slices_per_actor
        {
            return Err("TIR cold preparation is outside its fixed original claim clock/literal source/actor/slice budget".into());
        }
        apply_work_debit_v1(&mut self.work, signer, now, debit)
    }
    pub fn mark_ready_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        descriptor: crate::palw_backend::PalwTirPublicPreparedDescriptorV1,
    ) -> Result<(), String> {
        let c = &self.contract;
        let h = source.header_v1();
        if now < c.original_accepted_daa
            || now >= c.preparation_until_daa
            || !c.custodians.contains(&signer)
            || self.ready.contains_key(&signer)
            || self.defaulted.contains(&signer)
            || source.source_id_v1() != c.source_id
            || descriptor.version != 1
            || descriptor.source_id != c.source_id
            || descriptor.class_id != h.job_context.shape_profile_id
            || descriptor.artifact_root != h.artifact_root
            || descriptor.execution_root != h.committed_execution_root
            || descriptor.trace_root != h.full_logits_trace_root
            || descriptor.step_root != h.step_merkle_root
            || descriptor.step_leaf_count != source.leaf_count_v1()
            || descriptor.decode_count != source.decode_count_v1()
            || descriptor.vocab != source.vocab_v1()
            || descriptor.material_until_daa != c.material_until_daa
            || descriptor.manifest_digest == Hash64::default()
            || descriptor.retained_bytes == 0
            || descriptor.retained_bytes > c.plan.maximum_prepared_bytes
        {
            return Err(
                "TIR Ready changes original source/index geometry/material horizon or misses fixed preparation deadline".into()
            );
        }
        let receipt =
            self.work.actors.get(&signer).and_then(|a| a.last.as_ref()).ok_or("TIR Ready has no paid preparation debit")?.hash_v1()?;
        self.ready.insert(signer, PalwTirPublicReadyV1 { descriptor, accepted_daa: now, work_receipt: receipt });
        Ok(())
    }
    /// A single outside bonded observer can collect EACH missing signer. No
    /// all-Panel readiness vote or outside-owner preparation promise is needed.
    /// The native caller still authenticates the collector's active bond.
    pub fn collect_v1(
        &mut self,
        collector: PalwBondKeyV2,
        actor: PalwBondKeyV2,
        now: u64,
    ) -> Result<PalwTirPublicPreparationDefaultV1, String> {
        let c = &self.contract;
        if collector == c.producer
            || c.drawn_panel.contains(&collector)
            || !c.custodians.contains(&actor)
            || now < c.preparation_until_daa
            || now >= c.collect_until_daa
            || self.ready.contains_key(&actor)
            || self.defaulted.contains(&actor)
        {
            return Err("TIR preparation collect is early/late/repeated, self-Panel or names no missing literal duty".into());
        }
        self.defaulted.insert(actor);
        Ok(PalwTirPublicPreparationDefaultV1 {
            claim: c.claim,
            allocation_id: c.allocation_id,
            source_id: c.source_id,
            actor,
            original_accepted_daa: c.original_accepted_daa,
            preparation_until_daa: c.preparation_until_daa,
        })
    }
}

/// Conservative full-source acquisition, NOT a proof of the canonical sparse
/// closure or generic terminal bound. Checked overflow is an explicit missing
/// coverage case, never an activation/profile restriction disguised as success.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicOwnedPlanV1 {
    pub source_id: Hash64,
    pub maximum_custodians: u64,
    pub maximum_fresh_logical_units: u64,
    pub maximum_reacquire_logical_units: u64,
    pub maximum_turns: u64,
    pub maximum_unit_bytes: u64,
    pub config: PalwStagedPublicCourtConfigV1,
    pub envelope: PalwRenderingResourcePlanV1,
}
pub fn request_body_bound_v1(source: &PalwTirPublicSourceV1, request: PalwTirPublicRequestV1) -> Result<(u64, u64), String> {
    source.check_request_v1(&request)?;
    if let PalwTirPublicRequestV1::Ids { offset } = request {
        if offset != 0 {
            return Err("owned IDs acquire the complete original sequence, not an unauthenticated fragment".into());
        }
        let mut bytes = 0u64;
        let mut ram = 0u64;
        for offset in (0..source.decode_count_v1()).step_by(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
            let bound = source.unit_bound_v1(&PalwTirPublicRequestV1::Ids { offset })?;
            bytes = bytes.checked_add(bound.maximum_response_bytes).ok_or("TIR ID body overflow")?;
            ram = ram.max(bound.resident_ram_bytes);
        }
        return Ok((bytes, ram.checked_add(bytes.checked_mul(8).ok_or("TIR ID RAM overflow")?).ok_or("TIR ID RAM overflow")?));
    }
    let bound = source.unit_bound_v1(&request)?;
    Ok((bound.maximum_response_bytes, bound.resident_ram_bytes))
}
fn request_bound(source: &PalwTirPublicSourceV1, request: PalwTirPublicRequestV1) -> Result<(u64, u64), String> {
    request_body_bound_v1(source, request)
}
impl PalwTirPublicOwnedPlanV1 {
    pub fn derive_v1(
        source: &PalwTirPublicSourceV1,
        maximum_custodians: u64,
        config: PalwStagedPublicCourtConfigV1,
    ) -> Result<Self, String> {
        if maximum_custodians == 0 || config.acquisition_daa == 0 || config.prices.inclusion_margin_daa == 0 {
            return Err("TIR acquisition requires native custody and positive original response/inclusion windows".into());
        }
        let n = source.leaf_count_v1();
        let d = u64::from(source.decode_count_v1());
        let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let mut candidates =
            vec![PalwTirPublicRequestV1::StepLeaf { index: 0 }, PalwTirPublicRequestV1::StepRun { first: 0, count: 1 }];
        if n > 1 {
            candidates.push(PalwTirPublicRequestV1::StepNode { level: depth(n).min(10), index: 0 });
        }
        if source.supports_tiled_trace_v1() && d > 0 {
            candidates.extend([PalwTirPublicRequestV1::Ids { offset: 0 }, PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 }]);
            if d > 1 {
                candidates.push(PalwTirPublicRequestV1::RowsNode { level: depth(d).min(10), index: 0 });
            }
            if tiles > 1 {
                candidates.push(PalwTirPublicRequestV1::TileNode { row: 0, level: depth(tiles).min(10), index: 0 });
            }
        }
        let mut atom = 0u64;
        for request in candidates {
            atom = atom.max(request_bound(source, request)?.0);
        }
        // Leaf-by-leaf canonical operands fit without assuming a backend meter's
        // operational target limit. Trace allowance covers every tile and its
        // interior localization, not a caller-chosen subset or u8 tile ceiling.
        let fresh = exact(
            u128::from(n)
                + u128::from(source.step_descent_units_v1())
                + if source.supports_tiled_trace_v1() { u128::from(d) * (u128::from(tiles) * 3 + 2) + 1 } else { 0 },
        )?;
        let reacquire = exact(u128::from(fresh) * u128::from(config.prices.reorg_replay_moves))?;
        let turns = fresh.checked_add(reacquire).ok_or("TIR acquisition turn budget overflow")?;
        let slots = maximum_custodians.checked_add(1).ok_or("TIR literal slots overflow")?;
        let attempts = product(turns, slots)?;
        let moves = sum(&[product(attempts, atom.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 3)?, product(turns, 2)?, 1])?;
        let raw = product(attempts, atom)?;
        let row = sum(&[product(slots, 1536)?, 16_384])?;
        let carrier = product(moves, PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64 + 16_384)?;
        // Small mutable metadata only; body chunks are separate immutable rows.
        // Eight raw copies cover the immutable rooted row, delta210 old/new
        // framing, retained journal and bounded encode/reorg scratch. This is
        // not a bound on unrelated global BTreeMap clone/root amplification.
        let raw_storage = product(raw, 8)?;
        let disk = sum(&[
            raw_storage,
            product(moves, 1024)?,
            product(attempts, 1024 + 64 * 64)?,
            product(product(moves, row)?, 2)?,
            crate::palw_prefix_history_v1::PALW_STAGED_CACHE_LEASE_STORAGE_BYTES_V1,
        ])?;
        let work = sum(&[product(sum(&[raw_storage, carrier, product(moves, row)?])?, 32)?, product(moves, 65_536)?])?;
        let maximum_unit_bytes = atom.checked_mul(n.min(256)).ok_or("TIR run body upper bound overflow")?;
        let envelope = PalwRenderingResourcePlanV1 {
            prefix_tokens: 0,
            vocab: source.vocab_v1(),
            maximum_stream_bytes: sum(&[disk, carrier])?,
            maximum_stream_leaves: attempts,
            maximum_eog_bytes: 0,
            maximum_moves: moves,
            maximum_parser_steps: work,
            maximum_blake_blocks: work.div_ceil(128),
            inclusion_daa: sum(&[product(moves, config.prices.inclusion_margin_daa)?, product(turns, config.acquisition_daa)?])?,
            rent_sompi: sum(&[
                product(sum(&[disk, carrier])?, config.prices.rent_per_stream_byte)?,
                product(row, config.prices.rent_per_row_byte)?,
                product(work, config.prices.rent_per_parser_step)?,
            ])?,
            deposit_sompi: product(moves, config.prices.deposit_per_move)?,
            row_bytes: row,
            resident_ram_bytes: sum(&[product(row, 8)?, product(maximum_unit_bytes, 16)?, 262_144])?,
        };
        envelope.rent_sompi.checked_add(envelope.deposit_sompi).ok_or("TIR acquisition capital overflow")?;
        Ok(Self {
            source_id: source.source_id_v1(),
            maximum_custodians,
            maximum_fresh_logical_units: fresh,
            maximum_reacquire_logical_units: reacquire,
            maximum_turns: turns,
            maximum_unit_bytes,
            config,
            envelope,
        })
    }
}

/// Native caller supplies the actual producer and already qualified literal
/// Valid signers, never infers grants from Panel membership. This structural
/// record is NOT proof that those signatures/claim admission were checked.
/// Native admission MUST compare plan.config to its actual native parameters;
/// self-consistent wire-supplied prices are not proof of the required payment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicContractV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub terms: PalwRenderingClaimTermsV1,
    pub plan: PalwTirPublicOwnedPlanV1,
    pub producer: PalwBondKeyV2,
    pub custodians: BTreeSet<PalwBondKeyV2>,
}
impl BorshDeserialize for PalwTirPublicContractV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let claim = BorshDeserialize::deserialize_reader(r)?;
        let owner = BorshDeserialize::deserialize_reader(r)?;
        let reserved_daa = BorshDeserialize::deserialize_reader(r)?;
        let bundle_id = BorshDeserialize::deserialize_reader(r)?;
        let source_id = BorshDeserialize::deserialize_reader(r)?;
        let terms = BorshDeserialize::deserialize_reader(r)?;
        let plan = PalwTirPublicOwnedPlanV1::deserialize_reader(r)?;
        let producer = BorshDeserialize::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)?;
        if u64::from(count) > plan.maximum_custodians {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR literal custody exceeds allocated geometry"));
        }
        // Do not deserialize through an eagerly allocated length-prefixed Vec.
        let mut custodians = BTreeSet::new();
        for _ in 0..count {
            if !custodians.insert(PalwBondKeyV2::deserialize_reader(r)?) {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "duplicate TIR literal custodian"));
            }
        }
        Ok(Self { claim, owner, reserved_daa, bundle_id, source_id, terms, plan, producer, custodians })
    }
}
impl PalwTirPublicContractV1 {
    pub fn check_native_config_v1(
        &self,
        source: &PalwTirPublicSourceV1,
        config: PalwStagedPublicCourtConfigV1,
        maximum_custodians: u64,
    ) -> Result<(), String> {
        if self.plan.config != config || self.plan.maximum_custodians != maximum_custodians {
            return Err("TIR wire quote substitutes native prices or admitted custody upper bound".into());
        }
        self.check_v1(source)
    }
    pub fn check_v1(&self, source: &PalwTirPublicSourceV1) -> Result<(), String> {
        if self.claim == Hash64::default()
            || self.bundle_id == Hash64::default()
            || self.source_id != source.source_id_v1()
            || self.plan != PalwTirPublicOwnedPlanV1::derive_v1(source, self.plan.maximum_custodians, self.plan.config)?
            || self.custodians.len() as u64 > self.plan.maximum_custodians
            || !self.custodians.contains(&self.producer)
            || self.custodians.contains(&self.owner)
            || self.reserved_daa < self.terms.accepted_daa
            || self.reserved_daa >= self.terms.open_until_daa
            || !self.terms.plan.covers_v1(self.plan.envelope)
            || self
                .terms
                .open_until_daa
                .checked_add(self.plan.envelope.inclusion_daa)
                .is_none_or(|end| end > self.terms.stream_until_daa)
            || self.terms.stream_until_daa >= self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
        {
            return Err("TIR acquisition contract changes immutable native geometry, funding, custody or clocks".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicChunkRecordV1 {
    pub demand_id: Hash64,
    pub answerer: PalwBondKeyV2,
    pub previous: Hash64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
impl BorshDeserialize for PalwTirPublicChunkRecordV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            demand_id: BorshDeserialize::deserialize_reader(r)?,
            answerer: BorshDeserialize::deserialize_reader(r)?,
            previous: BorshDeserialize::deserialize_reader(r)?,
            offset: BorshDeserialize::deserialize_reader(r)?,
            bytes: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1)?,
        })
    }
}
impl PalwTirPublicChunkRecordV1 {
    pub fn head_v1(&self) -> Result<Hash64, String> {
        if self.bytes.is_empty() || self.bytes.len() > PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 {
            return Err("TIR chunk length is not bounded".into());
        }
        Ok(digest(b"misaka-palw/tir-public-chunk/v1", &borsh::to_vec(self).map_err(|e| e.to_string())?))
    }
}
pub fn palw_tir_public_body_hash_v1(body: &[u8]) -> Hash64 {
    let mut hash = LeafHashContinuationV1::new(BODY_DOMAIN);
    hash.update(&(body.len() as u64).to_le_bytes());
    hash.update(body);
    hash.finish()
}
/// Canonical logical ID body is the concatenation of bounded response chunks.
/// No chunk separately becomes an authenticated/answered custody obligation.
pub fn palw_tir_public_ids_body_v1(source: &PalwTirPublicSourceV1, rows_root: Hash64, ids: &[u32]) -> Result<Vec<u8>, String> {
    if ids.len() != source.decode_count_v1() as usize || !source.supports_tiled_trace_v1() {
        return Err("TIR ID body changes native count/scheme".into());
    }
    let mut body = Vec::new();
    for ids in ids.chunks(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
        body.extend(borsh::to_vec(&PalwTirPublicResponseV1::Ids { rows_root, ids: ids.to_vec() }).map_err(|e| e.to_string())?);
    }
    Ok(body)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicAcceptedBodyRefV1 {
    pub source_id: Hash64,
    pub demand_id: Hash64,
    pub turn: u64,
    pub request: PalwTirPublicRequestV1,
    pub answerer: PalwBondKeyV2,
    pub body_hash: Hash64,
    pub total_bytes: u64,
    pub chunk_head: Hash64,
    pub opened_daa: u64,
    pub respond_until_daa: u64,
}
impl PalwTirPublicAcceptedBodyRefV1 {
    pub fn hash_v1(&self) -> Result<Hash64, String> {
        Ok(digest(b"misaka-palw/tir-public-accepted/v1", &borsh::to_vec(self).map_err(|e| e.to_string())?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicSlotV1 {
    pub body_hash: Hash64,
    pub total_bytes: u64,
    pub accepted_offset: u64,
    pub head: Hash64,
    pub finished: Option<PalwTirPublicAcceptedBodyRefV1>,
    hash: LeafHashContinuationV1,
}
impl BorshDeserialize for PalwTirPublicSlotV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let body_hash = BorshDeserialize::deserialize_reader(r)?;
        let total_bytes = BorshDeserialize::deserialize_reader(r)?;
        let accepted_offset = BorshDeserialize::deserialize_reader(r)?;
        let head = BorshDeserialize::deserialize_reader(r)?;
        let finished = BorshDeserialize::deserialize_reader(r)?;
        // Bound the continuation's only Vec before calling its legacy codec.
        let words = <[u64; 8]>::deserialize_reader(r)?;
        let compressed = u128::deserialize_reader(r)?;
        let buffer = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, 128)?;
        let encoded = borsh::to_vec(&(words, compressed, buffer))?;
        Ok(Self { body_hash, total_bytes, accepted_offset, head, finished, hash: borsh::from_slice(&encoded)? })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicPendingV1 {
    pub request: PalwTirPublicRequestV1,
    pub demand_id: Hash64,
    pub turn: u64,
    pub opened_daa: u64,
    pub respond_until_daa: u64,
    pub reacquire_nonce: Option<u64>,
    pub slots: BTreeMap<PalwBondKeyV2, PalwTirPublicSlotV1>,
    pub collected: bool,
}
impl BorshDeserialize for PalwTirPublicPendingV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let request = BorshDeserialize::deserialize_reader(r)?;
        let demand_id = BorshDeserialize::deserialize_reader(r)?;
        let turn = BorshDeserialize::deserialize_reader(r)?;
        let opened_daa = BorshDeserialize::deserialize_reader(r)?;
        let respond_until_daa = BorshDeserialize::deserialize_reader(r)?;
        let reacquire_nonce = BorshDeserialize::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)?;
        let mut slots = BTreeMap::new();
        for _ in 0..count {
            let signer = PalwBondKeyV2::deserialize_reader(r)?;
            let slot = PalwTirPublicSlotV1::deserialize_reader(r)?;
            if slots.insert(signer, slot).is_some() {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "duplicate TIR protected slot"));
            }
        }
        Ok(Self {
            request,
            demand_id,
            turn,
            opened_daa,
            respond_until_daa,
            reacquire_nonce,
            slots,
            collected: BorshDeserialize::deserialize_reader(r)?,
        })
    }
}
/// Fixed 64 peaks of the ORIGINAL step Merkle tree, with body-reference hashes
/// as leaf preimages. Native history can supply logarithmic membership openings.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct History {
    count: u64,
    peaks: [Option<Hash64>; 64],
}
impl History {
    fn root(&self) -> Option<Hash64> {
        self.peaks.iter().flatten().fold(None, |right, left| Some(right.map_or(*left, |right| step_merkle_node_v1(left, &right))))
    }
    fn append(&mut self, value: Hash64) -> Result<u64, String> {
        let index = self.count;
        let next = index.checked_add(1).ok_or("TIR accepted history count overflow")?;
        let mut at = index;
        let mut node = step_merkle_leaf_v1(index, &value);
        for peak in &mut self.peaks {
            if at & 1 == 0 {
                *peak = Some(node);
                self.count = next;
                return Ok(index);
            }
            node = step_merkle_node_v1(&peak.take().ok_or("TIR accepted frontier inconsistent")?, &node);
            at >>= 1;
        }
        Err("TIR accepted frontier overflow".into())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicSpentV1 {
    pub fresh_logical: u64,
    pub reacquire_logical: u64,
    pub turns: u64,
    pub moves: u64,
    pub reacquire_nonce: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicOwnedV1 {
    contract: PalwSharedCourtV1<PalwTirPublicContractV1>,
    pending: Option<PalwSharedCourtV1<PalwTirPublicPendingV1>>,
    spent: PalwTirPublicSpentV1,
    history: History,
    work: Option<PalwTirPublicWorkStateV1>,
    preparation: Option<PalwTirPublicPreparationReferenceV1>,
    prepared_responders: Option<BTreeSet<PalwBondKeyV2>>,
}
/// No raw bytes, growing chunk list, or deserialized authenticated marker.
/// Only native authenticated journal apply/revert may restore a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicOwnedCheckpointV1 {
    contract_id: Hash64,
    pending: Option<PalwSharedCourtV1<PalwTirPublicPendingV1>>,
    spent: PalwTirPublicSpentV1,
    history: History,
    work: Option<PalwTirPublicWorkStateV1>,
    preparation: Option<PalwTirPublicPreparationReferenceV1>,
    prepared_responders: Option<BTreeSet<PalwBondKeyV2>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicPreparationReferenceV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub original_accepted_daa: u64,
    pub preparation_until_daa: u64,
    pub material_until_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicDefaultV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub demand_id: Hash64,
    pub opened_daa: u64,
    pub deadline_daa: u64,
    pub missing_custodians: BTreeSet<PalwBondKeyV2>,
}
pub enum PalwTirPublicFinishedV1 {
    Unit(PalwTirPublicAuthenticatedUnitV1),
    Ids(PalwTirPublicAuthenticatedIdsV1),
}
pub struct PalwTirPublicFinishEffectV1 {
    pub authenticated: PalwTirPublicFinishedV1,
    pub body: PalwTirPublicAcceptedBodyRefV1,
    pub history_index: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwTirPublicOwnedActionV1 {
    Demand { request: PalwTirPublicRequestV1 },
    Reacquire { previous: PalwTirPublicAcceptedBodyRefV1, proof: PalwStepOpeningV1, nonce: u64 },
    Begin { demand_id: Hash64, answerer: PalwBondKeyV2, total_bytes: u64, body_hash: Hash64 },
    Part { demand_id: Hash64, answerer: PalwBondKeyV2, offset: u64, bytes: Vec<u8> },
    Finish { demand_id: Hash64, answerer: PalwBondKeyV2 },
    Collect { demand_id: Hash64 },
    WorkDebit(PalwTirPublicWorkDebitV1),
    PreparationReady { descriptor: crate::palw_backend::PalwTirPublicPreparedDescriptorV1 },
    PreparationCollect { actor: PalwBondKeyV2 },
}
impl BorshDeserialize for PalwTirPublicOwnedActionV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(match u8::deserialize_reader(r)? {
            0 => Self::Demand { request: BorshDeserialize::deserialize_reader(r)? },
            1 => {
                let previous = BorshDeserialize::deserialize_reader(r)?;
                let index = u64::deserialize_reader(r)?;
                let leaf_hash = Hash64::deserialize_reader(r)?;
                let length = u32::deserialize_reader(r)?;
                if length > 64 {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR history membership path exceeds u64 tree"));
                }
                let mut siblings = Vec::with_capacity(length as usize);
                for _ in 0..length {
                    siblings.push(Hash64::deserialize_reader(r)?);
                }
                Self::Reacquire {
                    previous,
                    proof: PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings },
                    nonce: BorshDeserialize::deserialize_reader(r)?,
                }
            }
            2 => Self::Begin {
                demand_id: BorshDeserialize::deserialize_reader(r)?,
                answerer: BorshDeserialize::deserialize_reader(r)?,
                total_bytes: BorshDeserialize::deserialize_reader(r)?,
                body_hash: BorshDeserialize::deserialize_reader(r)?,
            },
            3 => Self::Part {
                demand_id: BorshDeserialize::deserialize_reader(r)?,
                answerer: BorshDeserialize::deserialize_reader(r)?,
                offset: BorshDeserialize::deserialize_reader(r)?,
                bytes: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1)?,
            },
            4 => Self::Finish {
                demand_id: BorshDeserialize::deserialize_reader(r)?,
                answerer: BorshDeserialize::deserialize_reader(r)?,
            },
            5 => Self::Collect { demand_id: BorshDeserialize::deserialize_reader(r)? },
            6 => Self::WorkDebit(BorshDeserialize::deserialize_reader(r)?),
            7 => Self::PreparationReady { descriptor: BorshDeserialize::deserialize_reader(r)? },
            8 => Self::PreparationCollect { actor: BorshDeserialize::deserialize_reader(r)? },
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown TIR acquisition action")),
        })
    }
}
pub enum PalwTirPublicOwnedEffectV1 {
    Demanded(Hash64),
    Begun,
    Chunk(PalwTirPublicChunkRecordV1),
    Finished(PalwTirPublicFinishEffectV1),
    Default(PalwTirPublicDefaultV1),
    WorkDebited(PalwTirPublicWorkReceiptV1),
    PreparationReady(PalwBondKeyV2),
    PreparationDefault(PalwTirPublicPreparationDefaultV1),
}
impl PalwTirPublicOwnedV1 {
    /// Only original-deadline Ready actors inherit short response duties.
    /// Non-Ready actors have the separate, once-only preparation default.
    pub fn response_custodians_v1(&self) -> &BTreeSet<PalwBondKeyV2> {
        self.prepared_responders.as_ref().unwrap_or(&self.contract.custodians)
    }
    pub fn check_preparation_v1(&self, preparation: &PalwTirPublicPreparationV1) -> Result<(), String> {
        let c = preparation.contract_v1();
        let reference = self.preparation.ok_or("TIR owner has no shared preparation reference")?;
        if reference.claim != c.claim
            || reference.allocation_id != c.allocation_id
            || reference.source_id != c.source_id
            || reference.original_accepted_daa != c.original_accepted_daa
            || reference.preparation_until_daa != c.preparation_until_daa
            || reference.material_until_daa != c.material_until_daa
            || self.prepared_responders.as_ref() != Some(&preparation.ready.keys().copied().collect())
        {
            return Err("TIR response duties differ from current original-deadline Ready state".into());
        }
        Ok(())
    }
    pub fn preparation_v1(&self) -> Option<PalwTirPublicPreparationReferenceV1> {
        self.preparation
    }
    pub fn bind_preparation_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        preparation: &PalwTirPublicPreparationV1,
    ) -> Result<(), String> {
        self.source(source)?;
        preparation.check_v1()?;
        let c = preparation.contract_v1();
        if self.preparation.is_some()
            || self.pending.is_some()
            || self.spent.moves != 1
            || c.claim != self.contract.claim
            || c.allocation_id != self.contract.bundle_id
            || c.source_id != self.contract.source_id
            || c.original_accepted_daa != self.contract.terms.accepted_daa
            || c.material_until_daa != self.contract.terms.material_until_daa
            || self.contract.reserved_daa < c.preparation_until_daa
            || self.contract.custodians != c.custodians
            || self.contract.producer != c.producer
        {
            return Err("TIR owner cannot reset preparation or substitute original claim-wide readiness scope".into());
        }
        self.preparation = Some(PalwTirPublicPreparationReferenceV1 {
            claim: c.claim,
            allocation_id: c.allocation_id,
            source_id: c.source_id,
            original_accepted_daa: c.original_accepted_daa,
            preparation_until_daa: c.preparation_until_daa,
            material_until_daa: c.material_until_daa,
        });
        self.prepared_responders = Some(preparation.ready.keys().copied().collect());
        Ok(())
    }
    /// A future native full-SUM admission may install this ONCE before any
    /// source movement. This is not a public action or a live admission door.
    /// State-aware readers must rederive it from the retained registered class.
    pub fn install_work_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        allocation: PalwTirPublicWorkAllocationV1,
    ) -> Result<(), String> {
        self.source(source)?;
        let c = &self.contract;
        let full = c.plan.envelope.checked_sum_v1(allocation.envelope)?;
        if self.work.is_some()
            || self.pending.is_some()
            || self.spent.moves != 1
            || allocation.source_id != c.source_id
            || allocation.maximum.attempts == 0
            || allocation.maximum_restarts != c.plan.config.prices.reorg_replay_moves
            || !c.terms.plan.covers_v1(full)
            || c.terms.open_until_daa.checked_add(full.inclusion_daa).is_none_or(|end| end > c.terms.stream_until_daa)
        {
            return Err("TIR work requires an original independent fully funded SUM before first movement".into());
        }
        self.work = Some(PalwTirPublicWorkStateV1 {
            allocation,
            spent: Default::default(),
            nonce: 0,
            head: Hash64::default(),
            restarts: 0,
            last: None,
            actors: BTreeMap::new(),
        });
        Ok(())
    }

    fn work_scope_v1(
        &self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        scope: &PalwTirPublicWorkScopeV1,
    ) -> Result<(), String> {
        let c = &self.contract;
        source.check_request_v1(&scope.request)?;
        if scope.claim != c.claim
            || scope.owner != c.owner
            || scope.reserved_daa != c.reserved_daa
            || scope.bundle_id != c.bundle_id
            || scope.source_id != c.source_id
            || scope.actor != signer
            || (signer != c.owner && !c.custodians.contains(&signer))
        {
            return Err("TIR work changes literal source/owner/actor/request/reservation scope".into());
        }
        Ok(())
    }

    /// Only an independently authenticated native carrier may call this fold.
    /// The receipt records a debit, never proof that the off-chain work ran or
    /// that its local output authenticated. None/error cannot refund it.
    pub fn work_debit_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        debit: &PalwTirPublicWorkDebitV1,
    ) -> Result<PalwTirPublicWorkReceiptV1, String> {
        self.source(source)?;
        self.original_clock(now)?;
        self.work_scope_v1(source, signer, &debit.scope)?;
        if self.preparation.is_some() && debit.scope.phase == PalwTirPublicWorkPhaseV1::Capture {
            return Err("TIR owner DA turn cannot reopen claim-wide cold preparation".into());
        }
        if matches!(
            debit.scope.phase,
            PalwTirPublicWorkPhaseV1::Capture | PalwTirPublicWorkPhaseV1::Response | PalwTirPublicWorkPhaseV1::Ids
        ) {
            let p = self.active_v1().ok_or("TIR preparation work has no original native demand")?;
            let request = if matches!(debit.scope.request, PalwTirPublicRequestV1::Ids { .. }) {
                PalwTirPublicRequestV1::Ids { offset: 0 }
            } else {
                debit.scope.request
            };
            if p.collected || now < p.opened_daa || now >= p.respond_until_daa || p.request != request {
                return Err("TIR preparation work changes current request or original response deadline".into());
            }
            self.response_clock(signer, p.demand_id, now)?;
        }
        let turn = self.active_v1().map_or(self.spent.turns, |p| p.turn);
        let state = self.work.as_ref().ok_or("TIR work has no original funded native allocation")?;
        let actor = state.actors.get(&signer).cloned().unwrap_or_default();
        let actor_head = actor.last.as_ref().map(|r| r.hash_v1()).transpose()?.unwrap_or_default();
        if debit.scope.turn != turn
            || debit.expected_nonce != actor.grants
            || debit.previous_head != actor_head
            || debit.work.attempts == 0
            || debit.work.attempts > state.allocation.maximum.attempts
        {
            return Err("TIR work debit is stale, repeated or unbound to the exact native turn/head/nonce".into());
        }
        let cumulative = state.spent.checked_add_v1(debit.work)?;
        let nonce = state.nonce.checked_add(1).ok_or("TIR work nonce overflow")?;
        let restarts = state.restarts.checked_add(u64::from(debit.restart)).ok_or("TIR work restart overflow")?;
        let mut actor = PalwTirPublicActorWorkV1 {
            spent: actor.spent.checked_add_v1(debit.work)?,
            grants: actor.grants.checked_add(1).ok_or("TIR actor work nonce overflow")?,
            restarts: actor.restarts.checked_add(u64::from(debit.restart)).ok_or("TIR actor restart overflow")?,
            last: None,
        };
        if !state.allocation.maximum.covers_v1(&cumulative)
            || nonce > state.allocation.envelope.maximum_moves
            || restarts
                > product(
                    state.allocation.maximum_restarts,
                    state.allocation.maximum_custodians.checked_add(1).ok_or("TIR work slots overflow")?,
                )?
            || !state.allocation.per_actor.covers_v1(&actor.spent)
            || actor.grants > state.allocation.per_actor.attempts
            || actor.restarts > state.allocation.maximum_restarts
        {
            return Err("TIR original cumulative native work/restart allocation is exhausted".into());
        }
        let receipt = PalwTirPublicWorkReceiptV1 { debit: debit.clone(), accepted_daa: now, cumulative, ordinal: nonce };
        actor.last = Some(receipt.clone());
        let head = receipt.hash_v1()?;
        let state = self.work.as_mut().ok_or("TIR work allocation disappeared")?;
        state.spent = cumulative;
        state.nonce = nonce;
        state.head = head;
        state.restarts = restarts;
        state.last = Some(receipt.clone());
        state.actors.insert(signer, actor);
        Ok(receipt)
    }
    /// Native integration must obtain config/upper from accepted native state,
    /// not forward those fields from the incoming owner contract.
    pub fn admit_native_v1(
        contract: PalwTirPublicContractV1,
        source: &PalwTirPublicSourceV1,
        drawn_panel: &BTreeSet<PalwBondKeyV2>,
        config: PalwStagedPublicCourtConfigV1,
        maximum_custodians: u64,
        now: u64,
        capital: u64,
    ) -> Result<Self, String> {
        contract.check_native_config_v1(source, config, maximum_custodians)?;
        Self::admit_v1(contract, source, drawn_panel, now, capital)
    }
    pub fn admit_v1(
        contract: PalwTirPublicContractV1,
        source: &PalwTirPublicSourceV1,
        drawn_panel: &BTreeSet<PalwBondKeyV2>,
        now: u64,
        capital: u64,
    ) -> Result<Self, String> {
        contract.check_v1(source)?;
        if now < contract.reserved_daa
            || now >= contract.terms.open_until_daa
            || drawn_panel.contains(&contract.owner)
            || contract.custodians.iter().any(|b| *b != contract.producer && !drawn_panel.contains(b))
            || capital
                < contract
                    .terms
                    .plan
                    .rent_sompi
                    .checked_add(contract.terms.plan.deposit_sompi)
                    .ok_or("TIR allocation funding overflow")?
        {
            return Err("TIR acquisition lacks original opening, independent owner or full paid allocation".into());
        }
        Ok(Self {
            contract: Box::new(contract).into(),
            pending: None,
            spent: PalwTirPublicSpentV1 { moves: 1, ..Default::default() },
            history: History { count: 0, peaks: [None; 64] },
            work: None,
            preparation: None,
            prepared_responders: None,
        })
    }
    pub fn contract_v1(&self) -> &PalwTirPublicContractV1 {
        &self.contract
    }
    pub fn active_v1(&self) -> Option<&PalwTirPublicPendingV1> {
        self.pending.as_deref()
    }
    pub fn slot_v1(&self, signer: &PalwBondKeyV2) -> Option<&PalwTirPublicSlotV1> {
        self.active_v1()?.slots.get(signer)
    }
    pub fn spent_v1(&self) -> PalwTirPublicSpentV1 {
        self.spent
    }
    pub fn work_v1(&self) -> Option<&PalwTirPublicWorkStateV1> {
        self.work.as_ref()
    }
    pub fn history_v1(&self) -> (u64, Option<Hash64>) {
        (self.history.count, self.history.root())
    }
    pub fn checkpoint_v1(&self) -> Result<PalwTirPublicOwnedCheckpointV1, String> {
        Ok(PalwTirPublicOwnedCheckpointV1 {
            contract_id: self.contract_id_v1()?,
            pending: self.pending.clone(),
            spent: self.spent,
            history: self.history.clone(),
            work: self.work.clone(),
            preparation: self.preparation,
            prepared_responders: self.prepared_responders.clone(),
        })
    }
    pub fn restore_checkpoint_v1(
        &mut self,
        cp: &PalwTirPublicOwnedCheckpointV1,
        source: &PalwTirPublicSourceV1,
    ) -> Result<(), String> {
        if cp.contract_id != self.contract_id_v1()? {
            return Err("TIR checkpoint changes original immutable contract".into());
        }
        let mut candidate = self.clone();
        candidate.pending = cp.pending.clone();
        candidate.spent = cp.spent;
        candidate.history = cp.history.clone();
        if candidate.work.as_ref().map(|w| &w.allocation) != cp.work.as_ref().map(|w| &w.allocation) {
            return Err("TIR checkpoint changes immutable work allocation".into());
        }
        candidate.work = cp.work.clone();
        if candidate.preparation != cp.preparation || candidate.prepared_responders != cp.prepared_responders {
            return Err("TIR checkpoint changes original shared preparation reference".into());
        }
        candidate.check_v1(source)?;
        *self = candidate;
        Ok(())
    }
    /// For native authenticated journal ingress only. This prevents an unrelated
    /// large metadata vector from being decoded under a small owner's permit;
    /// the checkpoint itself still is NOT a public admission credential.
    pub fn decode_checkpoint_v1(&self, bytes: &[u8]) -> Result<PalwTirPublicOwnedCheckpointV1, String> {
        let row_bytes = self
            .contract
            .plan
            .envelope
            .row_bytes
            .checked_add(self.work.as_ref().map_or(0, |w| w.allocation.envelope.row_bytes))
            .ok_or("TIR work checkpoint byte allocation overflow")?;
        if bytes.len() as u64 > row_bytes {
            return Err("TIR checkpoint exceeds immutable metadata allocation before decoding".into());
        }
        let checkpoint: PalwTirPublicOwnedCheckpointV1 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
        if checkpoint.contract_id != self.contract_id_v1()? {
            return Err("TIR checkpoint changes paid owner contract".into());
        }
        Ok(checkpoint)
    }
    fn contract_id_v1(&self) -> Result<Hash64, String> {
        Ok(digest(b"misaka-palw/tir-public-contract/v1", &borsh::to_vec(self.contract.as_ref()).map_err(|e| e.to_string())?))
    }
    fn source(&self, source: &PalwTirPublicSourceV1) -> Result<(), String> {
        if source.source_id_v1() != self.contract.source_id {
            return Err("TIR action substitutes original source".into());
        }
        Ok(())
    }
    fn debit(&mut self) -> Result<(), String> {
        let moves = self.spent.moves.checked_add(1).ok_or("TIR move counter overflow")?;
        if moves > self.contract.plan.envelope.maximum_moves {
            return Err("TIR prepaid physical moves exhausted".into());
        }
        self.spent.moves = moves;
        Ok(())
    }
    fn original_clock(&self, now: u64) -> Result<(), String> {
        if now < self.contract.reserved_daa || now >= self.contract.terms.stream_until_daa {
            return Err("TIR action is outside original paid stream clock".into());
        }
        Ok(())
    }
    pub fn response_span_v1(&self, source: &PalwTirPublicSourceV1, request: PalwTirPublicRequestV1) -> Result<u64, String> {
        // Bound owner rows inherit the fixed ORIGINAL preparation barrier and
        // price warm response work below. Unbound legacy rows retain their old
        // span and cannot be reinterpreted as the new native full-SUM route.
        self.source(source)?;
        let (bytes, _) = request_bound(source, request)?;
        let slots = (self.response_custodians_v1().len() as u64).checked_add(1).ok_or("TIR slot count overflow")?;
        let moves = sum(&[product(slots, bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 2)?, 1])?;
        let work_moves = if self.preparation.is_some() {
            // Warm reopen authenticates only the durable manifest/IDs, never N
            // cold leaves or a model replay. Include one pre-debit per warm
            // slice, response build/body hash and each Begin/Part/Finish build.
            let warm = sum(&[u64::from(source.decode_count_v1()).div_ceil(1024), 8])?;
            let ids = if matches!(request, PalwTirPublicRequestV1::Ids { offset: 0 }) {
                product(u64::from(source.decode_count_v1()).div_ceil(1024), 2)?
            } else {
                0
            };
            product(slots, sum(&[warm, ids, bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64), 6])?)?
        } else {
            0
        };
        sum(&[
            self.contract.plan.config.acquisition_daa,
            product(sum(&[moves, work_moves])?, self.contract.plan.config.prices.inclusion_margin_daa)?,
        ])
    }
    pub fn check_v1(&self, source: &PalwTirPublicSourceV1) -> Result<(), String> {
        self.contract.check_v1(source)?;
        if self.preparation.is_some() != self.prepared_responders.is_some()
            || self.prepared_responders.as_ref().is_some_and(|s| !s.is_subset(&self.contract.custodians))
        {
            return Err("TIR inherited response set changes original preparation scope".into());
        }
        if self.preparation.is_some_and(|p| {
            p.claim != self.contract.claim
                || p.allocation_id != self.contract.bundle_id
                || p.source_id != self.contract.source_id
                || p.original_accepted_daa != self.contract.terms.accepted_daa
                || p.material_until_daa != self.contract.terms.material_until_daa
                || self.contract.reserved_daa < p.preparation_until_daa
        }) {
            return Err("TIR owner changes its immutable original claim-wide preparation reference".into());
        }
        if let Some(work) = &self.work {
            let full = self.contract.plan.envelope.checked_sum_v1(work.allocation.envelope)?;
            if work.allocation.source_id != self.contract.source_id
                || !self.contract.terms.plan.covers_v1(full)
                || !work.allocation.maximum.covers_v1(&work.spent)
                || work.nonce > work.allocation.envelope.maximum_moves
                || work.allocation.maximum_custodians != self.contract.plan.maximum_custodians
                || work.allocation.maximum
                    != work
                        .allocation
                        .per_actor
                        .checked_mul_v1(work.allocation.maximum_custodians.checked_add(1).ok_or("TIR work slot overflow")?)?
                || work.restarts
                    > product(
                        work.allocation.maximum_restarts,
                        work.allocation.maximum_custodians.checked_add(1).ok_or("TIR work slots overflow")?,
                    )?
                || work.actors.len() > self.contract.custodians.len() + 1
                || (work.nonce == 0) != (work.head == Hash64::default())
                || (work.nonce == 0) != work.last.is_none()
            {
                return Err("TIR rooted work allocation/spent/head is inconsistent".into());
            }
            let mut total = PalwTirPublicWorkV1::default();
            let mut grants = 0u64;
            let mut restarts = 0u64;
            for (actor, paid) in &work.actors {
                if (*actor != self.contract.owner && !self.contract.custodians.contains(actor))
                    || !work.allocation.per_actor.covers_v1(&paid.spent)
                    || paid.grants > work.allocation.per_actor.attempts
                    || paid.restarts > work.allocation.maximum_restarts
                    || paid.last.as_ref().is_none_or(|last| {
                        last.debit.scope.actor != *actor
                            || last.debit.expected_nonce.checked_add(1) != Some(paid.grants)
                            || last.ordinal == 0
                            || last.ordinal > work.nonce
                            || !work.spent.covers_v1(&last.cumulative)
                    })
                {
                    return Err("TIR protected work lane changes literal actor or exceeds its allocation".into());
                }
                total = total.checked_add_v1(paid.spent)?;
                grants = grants.checked_add(paid.grants).ok_or("TIR actor grants overflow")?;
                restarts = restarts.checked_add(paid.restarts).ok_or("TIR actor restarts overflow")?;
            }
            if total != work.spent || grants != work.nonce || restarts != work.restarts {
                return Err("TIR protected work lanes do not sum to rooted spent".into());
            }
            if let Some(last) = &work.last {
                self.work_scope_v1(source, last.debit.scope.actor, &last.debit.scope)?;
                if last.hash_v1()? != work.head
                    || last.cumulative != work.spent
                    || last.ordinal != work.nonce
                    || last.accepted_daa < self.contract.reserved_daa
                    || last.accepted_daa >= self.contract.terms.stream_until_daa
                {
                    return Err("TIR rooted work receipt changes original scope/nonce/clock/spent".into());
                }
            } else if work.spent != PalwTirPublicWorkV1::default() || work.restarts != 0 {
                return Err("TIR empty work journal has nonzero spent".into());
            }
        }
        if self.spent.moves == 0
            || self.spent.moves > self.contract.plan.envelope.maximum_moves
            || self.spent.fresh_logical > self.contract.plan.maximum_fresh_logical_units
            || self.spent.reacquire_logical > self.contract.plan.maximum_reacquire_logical_units
            || self.spent.turns > self.contract.plan.maximum_turns
            || self.spent.reacquire_nonce > self.spent.reacquire_logical
            || self.history.count > self.spent.moves
            || self.history.peaks.iter().enumerate().any(|(i, p)| p.is_some() != (self.history.count & (1u64 << i) != 0))
        {
            return Err("TIR cumulative prepaid work or history frontier inconsistent".into());
        }
        if let Some(p) = self.active_v1() {
            request_bound(source, p.request)?;
            if p.turn.checked_add(1) != Some(self.spent.turns)
                || p.opened_daa < self.contract.reserved_daa
                || p.opened_daa >= p.respond_until_daa
                || p.opened_daa.checked_add(self.response_span_v1(source, p.request)?) != Some(p.respond_until_daa)
                || p.respond_until_daa > self.contract.terms.stream_until_daa
                || p.slots.len() > self.response_custodians_v1().len() + 1
            {
                return Err("TIR pending identity/clock inconsistent".into());
            }
            if source.demand_id_v1(
                &PalwTirPublicDemandScopeV1 {
                    claim: self.contract.claim,
                    owner: self.contract.owner,
                    reserved_daa: self.contract.reserved_daa,
                    bundle_id: self.contract.bundle_id,
                    turn: p.turn,
                    opened_daa: p.opened_daa,
                    deadline_daa: p.respond_until_daa,
                },
                &p.request,
            )? != p.demand_id
            {
                return Err("TIR pending demand hash changes immutable source/owner/turn/clock".into());
            }
            for (signer, slot) in &p.slots {
                if (*signer != self.contract.owner && !self.response_custodians_v1().contains(signer))
                    || slot.total_bytes == 0
                    || slot.total_bytes > request_bound(source, p.request)?.0
                    || slot.accepted_offset > slot.total_bytes
                    || slot.hash.byte_count() != Some(128 + 8 + u128::from(slot.accepted_offset))
                    || (slot.accepted_offset == 0) != (slot.head == Hash64::default())
                    || slot.finished.as_ref().is_some_and(|r| {
                        r.demand_id != p.demand_id
                            || r.answerer != *signer
                            || r.source_id != self.contract.source_id
                            || r.turn != p.turn
                            || r.request != p.request
                            || r.chunk_head != slot.head
                            || r.body_hash != slot.body_hash
                            || r.opened_daa != p.opened_daa
                            || r.respond_until_daa != p.respond_until_daa
                            || r.total_bytes != slot.total_bytes
                            || slot.accepted_offset != slot.total_bytes
                            || slot.hash.finish() != slot.body_hash
                    })
                {
                    return Err("TIR protected slot changes accepted metadata".into());
                }
            }
        }
        Ok(())
    }
    fn new_demand(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        request: PalwTirPublicRequestV1,
        nonce: Option<u64>,
    ) -> Result<Hash64, String> {
        self.source(source)?;
        self.original_clock(now)?;
        if self.preparation.is_some_and(|p| now < p.preparation_until_daa) {
            return Err("TIR demand precedes the original claim-wide preparation deadline".into());
        }
        if self.response_custodians_v1().is_empty() {
            return Err("TIR source has no original Ready responder; collect literal preparation defaults instead".into());
        }
        if signer != self.contract.owner
            || self.active_v1().is_some_and(|p| {
                !p.collected && self.response_custodians_v1().iter().any(|s| p.slots.get(s).is_none_or(|slot| slot.finished.is_none()))
            })
        {
            return Err("TIR demand must be the outside owner and cannot erase an unpaid original obligation".into());
        }
        let span = self.response_span_v1(source, request)?;
        let until = now
            .checked_add(span)
            .filter(|end| *end < self.contract.terms.stream_until_daa)
            .ok_or("late TIR demand has no full prepaid response span")?;
        let amount = logical(request);
        let counter = if nonce.is_some() { self.spent.reacquire_logical } else { self.spent.fresh_logical };
        let next = counter.checked_add(amount).ok_or("TIR logical counter overflow")?;
        if next
            > if nonce.is_some() {
                self.contract.plan.maximum_reacquire_logical_units
            } else {
                self.contract.plan.maximum_fresh_logical_units
            }
            || self.spent.turns >= self.contract.plan.maximum_turns
        {
            return Err("TIR immutable request acquisition budget exhausted".into());
        }
        let turn = self.spent.turns;
        let id = source.demand_id_v1(
            &PalwTirPublicDemandScopeV1 {
                claim: self.contract.claim,
                owner: signer,
                reserved_daa: self.contract.reserved_daa,
                bundle_id: self.contract.bundle_id,
                turn,
                opened_daa: now,
                deadline_daa: until,
            },
            &request,
        )?;
        self.debit()?;
        if nonce.is_some() {
            self.spent.reacquire_logical = next;
        } else {
            self.spent.fresh_logical = next;
        }
        self.spent.turns += 1;
        self.pending = Some(
            Box::new(PalwTirPublicPendingV1 {
                request,
                demand_id: id,
                turn,
                opened_daa: now,
                respond_until_daa: until,
                reacquire_nonce: nonce,
                slots: BTreeMap::new(),
                collected: false,
            })
            .into(),
        );
        Ok(id)
    }
    pub fn demand_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        request: PalwTirPublicRequestV1,
    ) -> Result<Hash64, String> {
        self.new_demand(source, signer, now, request, None)
    }
    pub fn reacquire_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        previous: &PalwTirPublicAcceptedBodyRefV1,
        proof: &PalwStepOpeningV1,
        nonce: u64,
    ) -> Result<Hash64, String> {
        if nonce != self.spent.reacquire_nonce
            || previous.source_id != self.contract.source_id
            || previous.turn >= self.spent.turns
            || proof.siblings.len() > 64
            || proof.leaf_hash != previous.hash_v1()?
            || Some(step_opening_root_capped_v1(self.history.count, proof, u64::MAX).map_err(|e| e.to_string())?)
                != self.history.root()
        {
            return Err("TIR reacquisition lacks exact accepted history/nonce authority".into());
        }
        let next = nonce.checked_add(1).ok_or("TIR reacquisition nonce overflow")?;
        let id = self.new_demand(source, signer, now, previous.request, Some(nonce))?;
        self.spent.reacquire_nonce = next;
        Ok(id)
    }
    pub(crate) fn response_clock(&self, signer: PalwBondKeyV2, demand: Hash64, now: u64) -> Result<(), String> {
        self.original_clock(now)?;
        let p = self.active_v1().ok_or("TIR source has no active request")?;
        if p.demand_id != demand
            || p.collected
            || now < p.opened_daa
            || now >= p.respond_until_daa
            || (signer != self.contract.owner && !self.response_custodians_v1().contains(&signer))
        {
            return Err("TIR answer substitutes signer/demand or expires its original response clock".into());
        }
        Ok(())
    }
    pub fn begin_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        demand: Hash64,
        now: u64,
        total_bytes: u64,
        body_hash: Hash64,
    ) -> Result<(), String> {
        self.source(source)?;
        self.response_clock(signer, demand, now)?;
        let p = self.active_v1().unwrap();
        if p.slots.contains_key(&signer) || total_bytes == 0 || total_bytes > request_bound(source, p.request)?.0 {
            return Err("TIR protected Begin repeats or exceeds exact native body bound".into());
        }
        let mut hash = LeafHashContinuationV1::new(BODY_DOMAIN);
        hash.update(&total_bytes.to_le_bytes());
        self.debit()?;
        self.pending.as_mut().unwrap().slots.insert(
            signer,
            PalwTirPublicSlotV1 { body_hash, total_bytes, accepted_offset: 0, head: Hash64::default(), finished: None, hash },
        );
        Ok(())
    }
    pub fn part_v1(
        &mut self,
        signer: PalwBondKeyV2,
        demand: Hash64,
        now: u64,
        offset: u64,
        bytes: &[u8],
    ) -> Result<PalwTirPublicChunkRecordV1, String> {
        self.response_clock(signer, demand, now)?;
        let slot = self.slot_v1(&signer).ok_or("TIR Part has no accepted Begin")?;
        if slot.finished.is_some()
            || offset != slot.accepted_offset
            || offset >= slot.total_bytes
            || bytes.len() as u64 != (slot.total_bytes - offset).min(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)
        {
            return Err("TIR Part is not its exact signer/offset/canonical bounded chunk".into());
        }
        let record =
            PalwTirPublicChunkRecordV1 { demand_id: demand, answerer: signer, previous: slot.head, offset, bytes: bytes.to_vec() };
        let head = record.head_v1()?;
        self.debit()?;
        let slot = self.pending.as_mut().unwrap().slots.get_mut(&signer).unwrap();
        slot.hash.update(bytes);
        slot.accepted_offset += bytes.len() as u64;
        slot.head = head;
        Ok(record)
    }
    /// Records are newest first and must come from authenticated immutable chunk
    /// rows/history. Even a cache/peer response is verified against the exact head.
    pub fn finish_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        demand: Hash64,
        now: u64,
        records: &[PalwTirPublicChunkRecordV1],
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) -> Result<PalwTirPublicFinishEffectV1, String> {
        self.source(source)?;
        self.response_clock(signer, demand, now)?;
        let p = self.active_v1().unwrap();
        let slot = self.slot_v1(&signer).ok_or("TIR Finish has no accepted Begin")?;
        if slot.finished.is_some()
            || slot.accepted_offset != slot.total_bytes
            || slot.hash.finish() != slot.body_hash
            || records.len() as u64 != slot.total_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)
        {
            return Err("TIR Finish is not complete original accepted multipart EOF".into());
        }
        let size = usize::try_from(slot.total_bytes).map_err(|_| "TIR body exceeds native usize")?;
        let mut body = Vec::new();
        body.try_reserve_exact(size).map_err(|_| "TIR body allocation failed")?;
        body.resize(size, 0);
        let (mut head, mut offset) = (slot.head, slot.total_bytes);
        for record in records {
            if record.demand_id != demand
                || record.answerer != signer
                || record.head_v1()? != head
                || record.offset.checked_add(record.bytes.len() as u64) != Some(offset)
            {
                return Err("TIR restored chunk substitutes demand/signer/head/offset".into());
            }
            body[record.offset as usize..offset as usize].copy_from_slice(&record.bytes);
            head = record.previous;
            offset = record.offset;
        }
        if offset != 0 || head != Hash64::default() || palw_tir_public_body_hash_v1(&body) != slot.body_hash {
            return Err("TIR restored body does not reproduce its accepted checksum/head".into());
        }
        let authenticated = if let PalwTirPublicRequestV1::Ids { offset: 0 } = p.request {
            let mut assembler = PalwTirPublicIdsAssemblerV1::default();
            let mut at = 0usize;
            let mut sealed = None;
            for offset in (0..source.decode_count_v1()).step_by(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
                let request = PalwTirPublicRequestV1::Ids { offset };
                let length = source.unit_bound_v1(&request)?.maximum_response_bytes as usize;
                let end = at.checked_add(length).filter(|end| *end <= body.len()).ok_or("TIR IDs body is truncated")?;
                let response = source.decode_response_v1(&request, &body[at..end])?;
                sealed = assembler.push_v1(source, &request, &response)?;
                at = end;
            }
            if at != body.len() {
                return Err("TIR IDs body has trailing bytes".into());
            }
            PalwTirPublicFinishedV1::Ids(sealed.ok_or("TIR IDs never authenticated exact original EOF")?)
        } else {
            let response = source.decode_response_v1(&p.request, &body)?;
            PalwTirPublicFinishedV1::Unit(source.authenticate_v1(&p.request, &response, ids)?)
        };
        let reference = PalwTirPublicAcceptedBodyRefV1 {
            source_id: self.contract.source_id,
            demand_id: demand,
            turn: p.turn,
            request: p.request,
            answerer: signer,
            body_hash: slot.body_hash,
            total_bytes: slot.total_bytes,
            chunk_head: slot.head,
            opened_daa: p.opened_daa,
            respond_until_daa: p.respond_until_daa,
        };
        let mut history = self.history.clone();
        let index = history.append(reference.hash_v1()?)?;
        self.debit()?;
        self.history = history;
        self.pending.as_mut().unwrap().slots.get_mut(&signer).unwrap().finished = Some(reference.clone());
        Ok(PalwTirPublicFinishEffectV1 { authenticated, body: reference, history_index: index })
    }
    pub fn collect_v1(&mut self, signer: PalwBondKeyV2, demand: Hash64, now: u64) -> Result<PalwTirPublicDefaultV1, String> {
        let p = self.active_v1().ok_or("TIR collect has no literal pending obligation")?;
        if signer != self.contract.owner
            || demand != p.demand_id
            || p.collected
            || now < p.respond_until_daa
            || now >= self.contract.terms.collect_until_daa
        {
            return Err("TIR collect is stale, premature, repeated or outside original collection clock".into());
        }
        let missing = self
            .response_custodians_v1()
            .iter()
            .copied()
            .filter(|signer| p.slots.get(signer).is_none_or(|slot| slot.finished.is_none()))
            .collect();
        let effect = PalwTirPublicDefaultV1 {
            claim: self.contract.claim,
            owner: signer,
            reserved_daa: self.contract.reserved_daa,
            bundle_id: self.contract.bundle_id,
            source_id: self.contract.source_id,
            demand_id: demand,
            opened_daa: p.opened_daa,
            deadline_daa: p.respond_until_daa,
            missing_custodians: missing,
        };
        self.debit()?;
        self.pending.as_mut().unwrap().collected = true;
        Ok(effect)
    }
    /// The enclosing native carrier MUST authenticate `signer`, original claim
    /// authority and paid activation. `records` are rooted chunk rows, not an
    /// unauthenticated checkpoint or private producer-state credential.
    pub fn apply_v1(
        &mut self,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        now: u64,
        action: &PalwTirPublicOwnedActionV1,
        records: &[PalwTirPublicChunkRecordV1],
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) -> Result<PalwTirPublicOwnedEffectV1, String> {
        self.source(source)?;
        use PalwTirPublicOwnedActionV1 as A;
        match action {
            A::WorkDebit(debit) => Ok(PalwTirPublicOwnedEffectV1::WorkDebited(self.work_debit_v1(source, signer, now, debit)?)),
            A::Demand { request } => Ok(PalwTirPublicOwnedEffectV1::Demanded(self.demand_v1(source, signer, now, *request)?)),
            A::Reacquire { previous, proof, nonce } => {
                Ok(PalwTirPublicOwnedEffectV1::Demanded(self.reacquire_v1(source, signer, now, previous, proof, *nonce)?))
            }
            A::Begin { demand_id, answerer, total_bytes, body_hash } if *answerer == signer => {
                self.begin_v1(source, signer, *demand_id, now, *total_bytes, *body_hash)?;
                Ok(PalwTirPublicOwnedEffectV1::Begun)
            }
            A::Part { demand_id, answerer, offset, bytes } if *answerer == signer => {
                Ok(PalwTirPublicOwnedEffectV1::Chunk(self.part_v1(signer, *demand_id, now, *offset, bytes)?))
            }
            A::Finish { demand_id, answerer } if *answerer == signer => {
                Ok(PalwTirPublicOwnedEffectV1::Finished(self.finish_v1(source, signer, *demand_id, now, records, ids)?))
            }
            A::Collect { demand_id } => Ok(PalwTirPublicOwnedEffectV1::Default(self.collect_v1(signer, *demand_id, now)?)),
            _ => Err("TIR outer signer differs from literal answer slot".into()),
        }
    }
}

#[cfg(test)]
pub(crate) fn palw_tir_public_preparation_fixture_v1()
-> (PalwTirPublicSourceV1, PalwTirPublicPreparationV1, crate::palw_tir_public_state_v1::PalwTirPublicSourceDescriptorV1) {
    tests::preparation_fixture()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }
    pub(super) fn preparation_fixture()
    -> (PalwTirPublicSourceV1, PalwTirPublicPreparationV1, crate::palw_tir_public_state_v1::PalwTirPublicSourceDescriptorV1) {
        // Structural/source kernel fixture, not a native full-SUM acceptance.
        let (source, owner, _) = work_fixture();
        let c = owner.contract_v1();
        let work = owner.work_v1().unwrap().allocation.clone();
        let plan = derive_tir_public_preparation_plan_v1(&source, &work, &c.plan).unwrap();
        let total = c
            .plan
            .envelope
            .checked_sum_v1(work.envelope)
            .unwrap()
            .checked_sum_v1(work.envelope)
            .unwrap()
            .checked_sum_v1(plan.envelope)
            .unwrap();
        let opening = plan.preparation_daa.checked_add(c.plan.config.acquisition_daa).unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(
            c.terms.accepted_daa,
            total,
            opening,
            c.plan.config.collection_daa,
            c.plan.config.reorg_retention_daa,
        )
        .unwrap();
        let contract = PalwTirPublicPreparationContractV1 {
            claim: c.claim,
            allocation_id: c.bundle_id,
            source_id: c.source_id,
            original_accepted_daa: c.terms.accepted_daa,
            preparation_until_daa: c.terms.accepted_daa.checked_add(plan.preparation_daa).unwrap(),
            collect_until_daa: terms.collect_until_daa,
            material_until_daa: terms.material_until_daa,
            producer: c.producer,
            custodians: c.custodians.clone(),
            drawn_panel: BTreeSet::from([bond(2), bond(3)]),
            plan,
        };
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let descriptor = crate::palw_tir_public_state_v1::PalwTirPublicSourceDescriptorV1 {
            class_id: record.facts.class_id,
            layout_digest: record.layout_digest,
            header_digest: digest(b"misaka-palw/tir-source-header/v1", &borsh::to_vec(source.header_v1()).unwrap()),
            source_id: c.source_id,
            maximum_leaves: work.rules.max_step_leaf_count,
            native_config: c.plan.config,
            native_custodian_upper: c.plan.maximum_custodians,
            work_rules: Some(work.rules),
        };
        let preparation = PalwTirPublicPreparationV1::new_v1(contract, &source, work, &c.plan).unwrap();
        (source, preparation, descriptor)
    }
    fn capture_debit(prep: &PalwTirPublicPreparationV1, actor: PalwBondKeyV2) -> PalwTirPublicWorkDebitV1 {
        let c = prep.contract_v1();
        let lane = prep.work_v1().actors.get(&actor);
        PalwTirPublicWorkDebitV1 {
            expected_nonce: lane.map_or(0, |a| a.grants),
            previous_head: lane.and_then(|a| a.last.as_ref()).map(|r| r.hash_v1().unwrap()).unwrap_or_default(),
            scope: PalwTirPublicWorkScopeV1 {
                claim: c.claim,
                owner: c.producer,
                reserved_daa: c.original_accepted_daa,
                bundle_id: c.allocation_id,
                source_id: c.source_id,
                turn: 0,
                request: PalwTirPublicRequestV1::StepLeaf { index: 0 },
                actor,
                phase: PalwTirPublicWorkPhaseV1::Capture,
                input_head: c.source_id,
                input_offset: 0,
                slice: 0,
                cursor_id: Hash64::from_u64_word(9),
            },
            work: PalwTirPublicWorkV1 { attempts: 1, parser_steps: 1, ..Default::default() },
            restart: false,
            history: None,
        }
    }
    fn ready_descriptor(
        source: &PalwTirPublicSourceV1,
        prep: &PalwTirPublicPreparationV1,
    ) -> crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
        let h = source.header_v1();
        crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
            version: 1,
            source_id: source.source_id_v1(),
            class_id: h.job_context.shape_profile_id,
            artifact_root: h.artifact_root,
            execution_root: h.committed_execution_root,
            trace_root: h.full_logits_trace_root,
            step_root: h.step_merkle_root,
            step_leaf_count: source.leaf_count_v1(),
            decode_count: source.decode_count_v1(),
            vocab: source.vocab_v1(),
            material_until_daa: prep.contract.material_until_daa,
            manifest_digest: Hash64::from_u64_word(50),
            retained_bytes: 4096,
        }
    }
    #[test]
    fn claim_preparation_accepts_capture_before_any_owner_demand_but_never_resets_original_clock() {
        let (source, mut prep, _) = preparation_fixture();
        let now = prep.contract.original_accepted_daa + 1;
        let actor = prep.contract.producer;
        let debit = capture_debit(&prep, actor);
        prep.capture_debit_v1(&source, actor, now, &debit).unwrap();
        let descriptor = ready_descriptor(&source, &prep);
        prep.mark_ready_v1(&source, actor, now, descriptor).unwrap();
        assert!(prep.ready_v1(&actor).is_some(), "a signed retention promise, not a computed-correctness finding");
        let snapshot = prep.clone();
        let next = capture_debit(&prep, actor);
        assert!(prep.capture_debit_v1(&source, actor, now, &next).is_err());
        assert!(prep.mark_ready_v1(&source, actor, prep.contract.preparation_until_daa, descriptor).is_err());
        assert_eq!(prep, snapshot);
        prep.check_v1().unwrap();
    }
    #[test]
    fn one_outside_observer_collects_each_missing_preparer_without_panel_vote_or_owner_ready() {
        let (_, mut prep, _) = preparation_fixture();
        let deadline = prep.contract.preparation_until_daa;
        let producer = prep.contract.producer;
        assert!(prep.collect_v1(bond(7), producer, deadline - 1).is_err());
        assert!(prep.collect_v1(bond(2), producer, deadline).is_err());
        let first = prep.collect_v1(bond(7), producer, deadline).unwrap();
        assert_eq!(first.preparation_until_daa, deadline);
        prep.collect_v1(bond(7), bond(2), deadline).unwrap();
        assert!(prep.pending_signers_v1().is_empty());
        assert!(prep.collect_v1(bond(7), bond(7), deadline).is_err(), "outside owner never owes preparation Ready");
        assert!(prep.collect_v1(bond(7), producer, deadline).is_err());
        prep.check_v1().unwrap();
    }
    #[test]
    fn preparation_context_decode_uses_native_bounds_not_self_declared_plan() {
        let (_, prep, _) = preparation_fixture();
        let bytes = borsh::to_vec(&prep).unwrap();
        assert_eq!(PalwTirPublicPreparationV1::decode_bounded_v1(&bytes, 3, bytes.len() as u64).unwrap(), prep);
        assert!(PalwTirPublicPreparationV1::decode_bounded_v1(&bytes, 1, bytes.len() as u64).is_err());
        assert!(PalwTirPublicPreparationV1::decode_bounded_v1(&bytes, 3, bytes.len() as u64 - 1).is_err());
    }
    #[test]
    fn missing_ready_has_only_preparation_default_not_repeated_short_da_duties() {
        let (source, mut prep, _) = preparation_fixture();
        let actor = bond(2);
        let now = prep.contract.original_accepted_daa + 1;
        let debit = capture_debit(&prep, actor);
        prep.capture_debit_v1(&source, actor, now, &debit).unwrap();
        let descriptor = ready_descriptor(&source, &prep);
        prep.mark_ready_v1(&source, actor, now, descriptor).unwrap();
        let deadline = prep.contract.preparation_until_daa;
        let producer = prep.contract.producer;
        prep.collect_v1(bond(7), producer, deadline).unwrap();

        let (_, old, request) = work_fixture();
        let mut contract = old.contract_v1().clone();
        let work = old.work_v1().unwrap().allocation.clone();
        let total = contract
            .plan
            .envelope
            .checked_sum_v1(work.envelope)
            .unwrap()
            .checked_sum_v1(work.envelope)
            .unwrap()
            .checked_sum_v1(prep.contract.plan.envelope)
            .unwrap();
        contract.reserved_daa = deadline;
        contract.terms = PalwRenderingClaimTermsV1::derive(
            contract.terms.accepted_daa,
            total,
            prep.contract.plan.preparation_daa + contract.plan.config.acquisition_daa,
            contract.plan.config.collection_daa,
            contract.plan.config.reorg_retention_daa,
        )
        .unwrap();
        let capital = contract.terms.plan.rent_sompi.checked_add(contract.terms.plan.deposit_sompi).unwrap();
        let mut owned = PalwTirPublicOwnedV1::admit_v1(contract, &source, &prep.contract.drawn_panel, deadline, capital).unwrap();
        owned.bind_preparation_v1(&source, &prep).unwrap();
        assert_eq!(owned.response_custodians_v1(), &BTreeSet::from([actor]));
        owned.check_preparation_v1(&prep).unwrap();
        let owner = owned.contract.owner;
        for _ in 0..2 {
            let open = owned.active_v1().map_or(deadline, |p| p.respond_until_daa);
            let demand = owned.demand_v1(&source, owner, open, request).unwrap();
            assert!(owned.begin_v1(&source, producer, demand, open, 1, Hash64::default()).is_err());
            let until = owned.active_v1().unwrap().respond_until_daa;
            let effect = owned.collect_v1(owner, demand, until).unwrap();
            assert_eq!(effect.missing_custodians, BTreeSet::from([actor]));
            assert!(!effect.missing_custodians.contains(&producer));
        }
        let mut reorg = prep.clone();
        reorg.ready.clear();
        assert!(owned.check_preparation_v1(&reorg).is_err(), "a saved Ready set cannot override current rooted history");
        let mut empty =
            PalwTirPublicOwnedV1::admit_v1(owned.contract_v1().clone(), &source, &prep.contract.drawn_panel, deadline, capital)
                .unwrap();
        empty.bind_preparation_v1(&source, &reorg).unwrap();
        let before = empty.clone();
        assert!(empty.demand_v1(&source, owner, deadline, request).is_err());
        assert_eq!(empty, before, "no empty short-DA defaults or debit when all literal actors lack Ready");
        assert!(prep.collect_v1(bond(7), producer, deadline).is_err());
        owned.check_v1(&source).unwrap();
    }
    fn work_fixture() -> (PalwTirPublicSourceV1, PalwTirPublicOwnedV1, PalwTirPublicRequestV1) {
        // Pure preadmitted kernel fixture, not a full native admission test.
        let (source, old, request, _) = fixture();
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let rules = PalwTirPublicWorkRulesV1 {
            max_step_leaf_count: 1 << 40,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            max_eval_elements: 1024,
            max_eval_terms: 1024,
            max_target_requests: 64,
            max_source_reads: 1024,
            maximum_probe_copy_bytes: 65_536,
            maximum_replay_io_bytes: 0,
        };
        let allocation = derive_tir_public_work_allocation_v1(
            &record,
            &x.binding.class.layout,
            source.header_v1(),
            &source,
            &old.contract.plan,
            rules,
        )
        .unwrap();
        let mut contract = old.contract.as_ref().clone();
        contract.terms = PalwRenderingClaimTermsV1::derive(
            10,
            contract.plan.envelope.checked_sum_v1(allocation.envelope).unwrap(),
            contract.plan.config.acquisition_daa,
            contract.plan.config.collection_daa,
            contract.plan.config.reorg_retention_daa,
        )
        .unwrap();
        let capital = contract.terms.plan.rent_sompi.checked_add(contract.terms.plan.deposit_sompi).unwrap();
        let mut row = PalwTirPublicOwnedV1::admit_v1(contract, &source, &BTreeSet::from([bond(2), bond(3)]), 11, capital).unwrap();
        row.install_work_v1(&source, allocation).unwrap();
        (source, row, request)
    }
    fn work_debit(
        row: &PalwTirPublicOwnedV1,
        source: &PalwTirPublicSourceV1,
        actor: PalwBondKeyV2,
        request: PalwTirPublicRequestV1,
        phase: PalwTirPublicWorkPhaseV1,
    ) -> PalwTirPublicWorkDebitV1 {
        let c = row.contract_v1();
        let lane = row.work_v1().unwrap().actors.get(&actor);
        PalwTirPublicWorkDebitV1 {
            expected_nonce: lane.map_or(0, |a| a.grants),
            previous_head: lane.and_then(|a| a.last.as_ref()).map(|r| r.hash_v1().unwrap()).unwrap_or_default(),
            scope: PalwTirPublicWorkScopeV1 {
                claim: c.claim,
                owner: c.owner,
                reserved_daa: c.reserved_daa,
                bundle_id: c.bundle_id,
                source_id: c.source_id,
                turn: row.active_v1().map_or(row.spent.turns, |p| p.turn),
                request,
                actor,
                phase,
                input_head: source.source_id_v1(),
                input_offset: 0,
                slice: 0,
                cursor_id: Hash64::from_u64_word(7),
            },
            work: PalwTirPublicWorkV1 { attempts: 1, parser_steps: 1, ..Default::default() },
            restart: false,
            history: None,
        }
    }

    #[test]
    fn work_actor_nonce_survives_responder_interleaving_without_shared_head_preconditions() {
        let (source, mut row, request) = work_fixture();
        let owner = row.contract.owner;
        row.demand_v1(&source, owner, 12, request).unwrap();
        let outside = work_debit(&row, &source, owner, request, PalwTirPublicWorkPhaseV1::Planning);
        let responder = work_debit(&row, &source, bond(1), request, PalwTirPublicWorkPhaseV1::Response);
        row.work_debit_v1(&source, bond(1), 13, &responder).unwrap();
        let receipt = row.work_debit_v1(&source, owner, 13, &outside).unwrap();
        assert_eq!(receipt.ordinal, 2);
        assert_eq!(receipt.debit.expected_nonce, 0);
        assert_eq!(row.work_v1().unwrap().actors[&owner].grants, 1);
        let before = row.clone();
        assert!(row.work_debit_v1(&source, owner, 13, &outside).is_err());
        assert_eq!(row, before);
        row.check_v1(&source).unwrap();
    }

    #[test]
    fn rooted_work_spent_restores_exactly_and_cannot_refund_none_or_errors() {
        let (source, mut row, request) = work_fixture();
        let owner = row.contract.owner;
        let before = row.checkpoint_v1().unwrap();
        let debit = work_debit(&row, &source, owner, request, PalwTirPublicWorkPhaseV1::Planning);
        row.work_debit_v1(&source, owner, 12, &debit).unwrap();
        // The external operation may now return None or Err: neither is a fold
        // action, and neither can subtract from this rooted cumulative debit.
        let after = row.checkpoint_v1().unwrap();
        let encoded = borsh::to_vec(&after).unwrap();
        assert_eq!(row.decode_checkpoint_v1(&encoded).unwrap(), after);
        assert_eq!(row.work_v1().unwrap().spent, debit.work);
        row.restore_checkpoint_v1(&before, &source).unwrap();
        assert_eq!(row.work_v1().unwrap().nonce, 0, "native reorg restores canonical spends, NOT orphaned physical work");
        row.restore_checkpoint_v1(&after, &source).unwrap();
        assert_eq!(row.work_v1().unwrap().spent, debit.work);
        let mut forged = after;
        forged.work.as_mut().unwrap().spent.parser_steps = 0;
        let snapshot = row.clone();
        assert!(row.restore_checkpoint_v1(&forged, &source).is_err());
        assert_eq!(row, snapshot);
    }

    #[test]
    fn protected_work_lane_exhaustion_does_not_spend_outside_owner_allocation() {
        let (source, mut row, request) = work_fixture();
        let owner = row.contract.owner;
        row.demand_v1(&source, owner, 12, request).unwrap();
        let mut responder = work_debit(&row, &source, bond(1), request, PalwTirPublicWorkPhaseV1::Response);
        responder.work = row.work_v1().unwrap().allocation.per_actor;
        row.work_debit_v1(&source, bond(1), 13, &responder).unwrap();
        let next = work_debit(&row, &source, bond(1), request, PalwTirPublicWorkPhaseV1::Response);
        let snapshot = row.clone();
        assert!(row.work_debit_v1(&source, bond(1), 13, &next).is_err());
        assert_eq!(row, snapshot);
        let outside = work_debit(&row, &source, owner, request, PalwTirPublicWorkPhaseV1::Planning);
        row.work_debit_v1(&source, owner, 13, &outside).unwrap();
        assert_eq!(row.work_v1().unwrap().actors[&owner].spent, outside.work);
    }

    #[test]
    fn maximum_history_receipts_fit_every_actor_and_full_checkpoint_metadata_quote() {
        let (source, mut row, request) = work_fixture();
        let owner = row.contract.owner;
        row.demand_v1(&source, owner, 12, request).unwrap();
        let actors: Vec<_> = row.contract.custodians.iter().copied().chain(std::iter::once(owner)).collect();
        for actor in actors {
            let mut debit = work_debit(&row, &source, actor, request, PalwTirPublicWorkPhaseV1::Response);
            // Maximum codec geometry only. These siblings are NOT claimed to
            // authenticate a body; Response does not interpret a history path.
            debit.history = Some(PalwStepOpeningV1 {
                leaf_index: 0,
                leaf_hash: Hash64::from_u64_word(3),
                siblings: vec![Hash64::from_u64_word(4); 64],
            });
            row.work_debit_v1(&source, actor, 13, &debit).unwrap();
        }
        let work = row.work_v1().unwrap();
        for actor in work.actors.values() {
            assert!(borsh::object_length(actor).unwrap() <= 8192);
            assert_eq!(actor.last.as_ref().unwrap().debit.history.as_ref().unwrap().siblings.len(), 64);
        }
        let cp = row.checkpoint_v1().unwrap();
        let bytes = borsh::to_vec(&cp).unwrap();
        let bound = row.contract.plan.envelope.row_bytes.checked_add(work.allocation.envelope.row_bytes).unwrap();
        assert!(bytes.len() as u64 <= bound);
        assert_eq!(row.decode_checkpoint_v1(&bytes).unwrap(), cp);
        row.check_v1(&source).unwrap();
    }
    fn fixture() -> (PalwTirPublicSourceV1, PalwTirPublicOwnedV1, PalwTirPublicRequestV1, Vec<u8>) {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (registered, _) =
            crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let mut header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        // A genuinely committed but structurally invalid native-format leaf:
        // enough bytes to require two parts, no dense model allocation. The
        // acquisition only authenticates it; a semantic court is still required.
        let mut preimage = x.preimages[0].clone();
        preimage.values_le = vec![0xA5; 16_384];
        let mut hashes = x.hashes.clone();
        hashes[0] =
            crate::palw_step_leg::step_tile_leaf_hash_v1(&header.job_context.context_hash(), &registered.facts.class_id, &preimage);
        header.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
        header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &header.full_logits_trace_root,
            &registered.facts.class_id,
            header.step_leaf_count,
            &header.step_merkle_root,
        );
        let source = PalwTirPublicSourceV1::new_v1(
            &registered,
            &x.binding.class.layout,
            &header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            1 << 40,
        )
        .unwrap();
        let plan = PalwTirPublicOwnedPlanV1::derive_v1(&source, 3, PalwStagedPublicCourtConfigV1::network_v1()).unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(
            10,
            plan.envelope,
            plan.config.acquisition_daa,
            plan.config.collection_daa,
            plan.config.reorg_retention_daa,
        )
        .unwrap();
        let contract = PalwTirPublicContractV1 {
            claim: Hash64::from_u64_word(21),
            owner: bond(7),
            reserved_daa: 10,
            bundle_id: Hash64::from_u64_word(22),
            source_id: source.source_id_v1(),
            terms,
            plan,
            producer: bond(1),
            custodians: BTreeSet::from([bond(1), bond(2)]),
        };
        let capital = terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).unwrap();
        let row = PalwTirPublicOwnedV1::admit_v1(contract, &source, &BTreeSet::from([bond(2), bond(3)]), 11, capital).unwrap();
        let response = PalwTirPublicResponseV1::StepLeaf { preimage, opening: step_opening_v1(&hashes, 0).unwrap() };
        (source, row, PalwTirPublicRequestV1::StepLeaf { index: 0 }, borsh::to_vec(&response).unwrap())
    }
    fn upload(
        row: &mut PalwTirPublicOwnedV1,
        source: &PalwTirPublicSourceV1,
        signer: PalwBondKeyV2,
        demand: Hash64,
        now: u64,
        body: &[u8],
    ) -> Vec<PalwTirPublicChunkRecordV1> {
        row.begin_v1(source, signer, demand, now, body.len() as u64, palw_tir_public_body_hash_v1(body)).unwrap();
        let mut records = Vec::new();
        for (i, part) in body.chunks(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            records.push(row.part_v1(signer, demand, now, (i * PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64, part).unwrap());
        }
        records.reverse();
        records
    }
    #[test]
    fn protected_slots_require_original_authentication_and_all_wrong_signer_stale_or_wrong_bytes_fail_atomically() {
        let (source, mut row, request, body) = fixture();
        let owner = row.contract_v1().owner;
        let demand = row.demand_v1(&source, owner, 12, request).unwrap();
        let original = row.clone();
        assert!(row.begin_v1(&source, bond(90), demand, 13, body.len() as u64, palw_tir_public_body_hash_v1(&body)).is_err());
        assert_eq!(row, original);
        let records = upload(&mut row, &source, owner, demand, 13, &body);
        assert!(row.slot_v1(&owner).unwrap().finished.is_none());
        assert!(records.len() >= 2);
        let before = row.clone();
        let mut bad = records.clone();
        bad[0].answerer = bond(2);
        assert!(row.finish_v1(&source, owner, demand, 13, &bad, None).is_err());
        assert_eq!(row, before);
        let mut bad = records.clone();
        bad[0].bytes[0] ^= 1;
        assert!(row.finish_v1(&source, owner, demand, 13, &bad, None).is_err());
        assert_eq!(row, before);
        assert!(row.finish_v1(&source, owner, Hash64::from_u64_word(999), 13, &records, None).is_err());
        assert_eq!(row, before);
        let result = row.finish_v1(&source, owner, demand, 13, &records, None).unwrap();
        assert!(matches!(result.authenticated, PalwTirPublicFinishedV1::Unit(_)));
        assert_eq!(row.slot_v1(&owner).unwrap().finished.as_ref(), Some(&result.body));
        let after = row.clone();
        assert!(row.finish_v1(&source, owner, demand, 13, &records, None).is_err());
        assert_eq!(row, after);
        row.check_v1(&source).unwrap();
        // A custodian's wrong body checksum can reach its own EOF without ever
        // becoming an authenticated answer or consuming the owner's slot.
        let mut false_body = body.clone();
        false_body[40] ^= 1;
        let false_records = upload(&mut row, &source, bond(2), demand, 13, &false_body);
        let before = row.clone();
        assert!(row.finish_v1(&source, bond(2), demand, 13, &false_records, None).is_err());
        assert_eq!(row, before);
        assert!(row.slot_v1(&bond(2)).unwrap().finished.is_none());
    }
    #[test]
    fn original_deadline_collects_only_unanswered_literal_custody_once_and_cannot_be_reset_by_reacquisition() {
        let (source, mut row, request, body) = fixture();
        let owner = row.contract_v1().owner;
        let terms = row.contract_v1().terms;
        let demand = row.demand_v1(&source, owner, 12, request).unwrap();
        let records = upload(&mut row, &source, owner, demand, 13, &body);
        let accepted = row.finish_v1(&source, owner, demand, 13, &records, None).unwrap().body;
        let deadline = row.active_v1().unwrap().respond_until_daa;
        let before = row.clone();
        assert!(row.collect_v1(owner, demand, deadline - 1).is_err());
        assert_eq!(row, before);
        assert!(row.begin_v1(&source, bond(1), demand, deadline, 1, Hash64::default()).is_err());
        assert_eq!(row, before);
        let effect = row.collect_v1(owner, demand, deadline).unwrap();
        assert_eq!(effect.missing_custodians, BTreeSet::from([bond(1), bond(2)]));
        assert!(!effect.missing_custodians.contains(&owner));
        let before = row.clone();
        assert!(row.collect_v1(owner, demand, deadline).is_err());
        assert_eq!(row, before);
        let opening = step_opening_v1(&[accepted.hash_v1().unwrap()], 0).unwrap();
        let spent = row.spent_v1();
        let next = row.reacquire_v1(&source, owner, deadline, &accepted, &opening, 0).unwrap();
        assert_ne!(next, demand);
        assert_eq!(row.contract_v1().terms, terms);
        assert_eq!(row.spent_v1().fresh_logical, spent.fresh_logical);
        assert_eq!(row.spent_v1().reacquire_logical, spent.reacquire_logical + 1);
        assert_eq!(row.spent_v1().reacquire_nonce, 1);
        let before = row.clone();
        assert!(row.reacquire_v1(&source, owner, deadline, &accepted, &opening, 0).is_err());
        assert_eq!(row, before);
        assert!(row.begin_v1(&source, owner, demand, deadline, body.len() as u64, palw_tir_public_body_hash_v1(&body)).is_err());
        assert_eq!(row, before);
        let mut late = before.clone();
        let now = late.contract_v1().terms.stream_until_daa - 1;
        late.collect_v1(owner, next, now).unwrap();
        let before = late.clone();
        assert!(late.demand_v1(&source, owner, now, request).is_err());
        assert_eq!(late, before);
        let mut expired = row.clone();
        assert!(expired.collect_v1(owner, next, terms.collect_until_daa).is_err());
    }
    #[test]
    fn raw_chunk_rows_are_separate_from_small_cow_checkpoints_and_replay_restores_exact_metadata() {
        let (source, mut row, request, body) = fixture();
        let owner = row.contract_v1().owner;
        let demand = row.demand_v1(&source, owner, 12, request).unwrap();
        row.begin_v1(&source, owner, demand, 13, body.len() as u64, palw_tir_public_body_hash_v1(&body)).unwrap();
        let checkpoint = row.checkpoint_v1().unwrap();
        let before = row.clone();
        let shared = row.clone();
        assert!(std::ptr::eq(row.active_v1().unwrap(), shared.active_v1().unwrap()));
        let record = row.part_v1(owner, demand, 13, 0, &body[..PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1]).unwrap();
        assert!(!std::ptr::eq(row.active_v1().unwrap(), shared.active_v1().unwrap()));
        assert_eq!(shared, before);
        let bytes = borsh::to_vec(&row.checkpoint_v1().unwrap()).unwrap();
        assert!(bytes.len() < PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1, "checkpoints contain no logical body/chunk list");
        assert!(borsh::to_vec(&record).unwrap().len() > PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1);
        let restored: PalwTirPublicOwnedV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        restored.check_v1(&source).unwrap();
        assert_eq!(restored, row);
        row.restore_checkpoint_v1(&checkpoint, &source).unwrap();
        assert_eq!(row, before);
        assert_eq!(row.part_v1(owner, demand, 13, 0, &body[..PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1]).unwrap(), record);
        let mut foreign = checkpoint;
        foreign.contract_id = Hash64::default();
        let before = row.clone();
        assert!(row.restore_checkpoint_v1(&foreign, &source).is_err());
        assert_eq!(row, before);
    }
    #[test]
    fn all_allocation_dimensions_full_capital_literal_independence_and_native_overflow_are_checked() {
        let (source, row, _, _) = fixture();
        let contract = row.contract_v1().clone();
        let capital = contract.terms.plan.rent_sompi + contract.terms.plan.deposit_sompi;
        let panel = BTreeSet::from([bond(2), bond(3)]);
        assert!(PalwTirPublicOwnedV1::admit_v1(contract.clone(), &source, &panel, 11, capital - 1).is_err());
        assert!(PalwTirPublicOwnedV1::admit_v1(contract.clone(), &source, &BTreeSet::from([bond(7)]), 11, capital).is_err());
        macro_rules! deficit {($($field:ident),+ $(,)?) => {$({
            let mut short=contract.clone(); short.terms.plan.$field=short.terms.plan.$field.saturating_sub(1);
            assert!(short.check_v1(&source).is_err(), "{} must remain prepaid", stringify!($field));
        })+};}
        // Zero source dimensions (prefix/EOG) still participate in covers_v1;
        // rendering_resources' exhaustive regression tests those independently.
        deficit!(
            vocab,
            maximum_stream_bytes,
            maximum_stream_leaves,
            maximum_moves,
            maximum_parser_steps,
            maximum_blake_blocks,
            inclusion_daa,
            rent_sompi,
            deposit_sompi,
            row_bytes,
            resident_ram_bytes
        );
        assert!(PalwTirPublicOwnedPlanV1::derive_v1(&source, u64::MAX, contract.plan.config).is_err());
        let mut config = contract.plan.config;
        config.prices.reorg_replay_moves = u64::MAX;
        assert!(PalwTirPublicOwnedPlanV1::derive_v1(&source, 3, config).is_err());
        let mut config = contract.plan.config;
        config.prices.rent_per_parser_step = u64::MAX;
        assert!(PalwTirPublicOwnedPlanV1::derive_v1(&source, 3, config).is_err());
        assert!(contract.check_native_config_v1(&source, config, contract.plan.maximum_custodians).is_err());
        assert!(contract.check_native_config_v1(&source, contract.plan.config, contract.plan.maximum_custodians + 1).is_err());
    }
    #[test]
    fn history_frontier_matches_original_step_merkle_and_part_decoder_bounds_before_allocation() {
        let mut frontier = History { count: 0, peaks: [None; 64] };
        let mut leaves = Vec::new();
        for n in 0..65 {
            let value = Hash64::from_u64_word(n + 1);
            leaves.push(value);
            frontier.append(value).unwrap();
            assert_eq!(frontier.root(), Some(step_merkle_root_v1(&leaves).unwrap()));
        }
        let action = PalwTirPublicOwnedActionV1::Part {
            demand_id: Hash64::default(),
            answerer: bond(1),
            offset: 0,
            bytes: vec![0; PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 + 1],
        };
        assert!(borsh::from_slice::<PalwTirPublicOwnedActionV1>(&borsh::to_vec(&action).unwrap()).is_err());
        let record = PalwTirPublicChunkRecordV1 {
            demand_id: Hash64::default(),
            answerer: bond(1),
            previous: Hash64::default(),
            offset: 0,
            bytes: vec![0; PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 + 1],
        };
        assert!(borsh::from_slice::<PalwTirPublicChunkRecordV1>(&borsh::to_vec(&record).unwrap()).is_err());
    }
    #[test]
    fn original_ids_only_become_answered_at_complete_outer_authentication_and_run_rounding_is_prepaid() {
        let (source, mut row, _, _) = fixture();
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let root = crate::palw_step_refute::tiled_logits_rows_root_v1(&x.binding.job_context, &x.rows).unwrap();
        let request = PalwTirPublicRequestV1::Ids { offset: 0 };
        let body = palw_tir_public_ids_body_v1(&source, root, &x.generated).unwrap();
        assert!(body.len() as u64 <= request_body_bound_v1(&source, request).unwrap().0);
        let owner = row.contract_v1().owner;
        let demand = row.demand_v1(&source, owner, 12, request).unwrap();
        let records = upload(&mut row, &source, owner, demand, 13, &body);
        assert!(row.slot_v1(&owner).unwrap().finished.is_none());
        let effect = row.finish_v1(&source, owner, demand, 13, &records, None).unwrap();
        let PalwTirPublicFinishedV1::Ids(ids) = effect.authenticated else { panic!("original IDs") };
        assert_eq!(ids.source_id_v1(), source.source_id_v1());
        assert_eq!(ids.ids_v1(), x.generated);
        row.check_v1(&source).unwrap();
        let plan = &row.contract_v1().plan;
        let atom = plan.maximum_unit_bytes / source.leaf_count_v1().min(256);
        for count in 1..=source.leaf_count_v1().min(256) as u32 {
            let request = PalwTirPublicRequestV1::StepRun { first: 0, count };
            let bytes = request_body_bound_v1(&source, request).unwrap().0;
            assert!(bytes <= atom * u64::from(count));
            assert!(
                bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 2
                    <= u64::from(count) * (atom.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 3)
            );
            assert!(row.response_span_v1(&source, request).unwrap() <= plan.envelope.inclusion_daa);
        }
    }
}
