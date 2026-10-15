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
}
/// No raw bytes, growing chunk list, or deserialized authenticated marker.
/// Only native authenticated journal apply/revert may restore a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicOwnedCheckpointV1 {
    contract_id: Hash64,
    pending: Option<PalwSharedCourtV1<PalwTirPublicPendingV1>>,
    spent: PalwTirPublicSpentV1,
    history: History,
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
}
impl PalwTirPublicOwnedV1 {
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
    pub fn history_v1(&self) -> (u64, Option<Hash64>) {
        (self.history.count, self.history.root())
    }
    pub fn checkpoint_v1(&self) -> Result<PalwTirPublicOwnedCheckpointV1, String> {
        Ok(PalwTirPublicOwnedCheckpointV1 {
            contract_id: self.contract_id_v1()?,
            pending: self.pending.clone(),
            spent: self.spent,
            history: self.history.clone(),
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
        candidate.check_v1(source)?;
        *self = candidate;
        Ok(())
    }
    /// For native authenticated journal ingress only. This prevents an unrelated
    /// large metadata vector from being decoded under a small owner's permit;
    /// the checkpoint itself still is NOT a public admission credential.
    pub fn decode_checkpoint_v1(&self, bytes: &[u8]) -> Result<PalwTirPublicOwnedCheckpointV1, String> {
        if bytes.len() as u64 > self.contract.plan.envelope.row_bytes {
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
        self.source(source)?;
        let (bytes, _) = request_bound(source, request)?;
        let slots = (self.contract.custodians.len() as u64).checked_add(1).ok_or("TIR slot count overflow")?;
        let moves = sum(&[product(slots, bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 2)?, 1])?;
        sum(&[self.contract.plan.config.acquisition_daa, product(moves, self.contract.plan.config.prices.inclusion_margin_daa)?])
    }
    pub fn check_v1(&self, source: &PalwTirPublicSourceV1) -> Result<(), String> {
        self.contract.check_v1(source)?;
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
                || p.slots.len() > self.contract.custodians.len() + 1
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
                if (*signer != self.contract.owner && !self.contract.custodians.contains(signer))
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
        if signer != self.contract.owner
            || self.active_v1().is_some_and(|p| {
                !p.collected && self.contract.custodians.iter().any(|s| p.slots.get(s).is_none_or(|slot| slot.finished.is_none()))
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
            || (signer != self.contract.owner && !self.contract.custodians.contains(&signer))
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
            .contract
            .custodians
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
mod tests {
    use super::*;
    use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
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
