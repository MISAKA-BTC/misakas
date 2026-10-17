//! Original REAL clock captured at native acceptance, then authenticated by state and carriage.
use super::*;
use crate::palw_work_slice_v2::{PalwExecOriginV1, PalwWorkRootRefusalV2};

pub(super) fn source_time(
    extras: &PalwTransitionExtrasV1,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<u64, PalwStateV2Error> {
    match extras.exec_v2_origin_headers.get(&claim.accepted_block) {
        Some(bytes) => verify_header_bytes(claim_id, claim, bytes),
        None => {
            #[cfg(test)]
            if let Some(issued) = extras.exec_v2_test_origin_daa {
                return Ok(issued);
            }
            Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))
        }
    }
}

fn refuse(why: PalwWorkRootRefusalV2) -> PalwStateV2Error {
    PalwStateV2Error::ExecV2Refused(why.to_string())
}

/// Decode vectors against the bytes actually present, and validate compressed parents before any expanded hashing.
/// The Borsh layout is the ordinary Header layout; the cached hash is never authority.
fn decode_header(mut bytes: &[u8]) -> std::io::Result<crate::header::Header> {
    use borsh::BorshDeserialize;
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed EXEC original header");
    let hash = Hash64::deserialize(&mut bytes)?;
    let version = u16::deserialize(&mut bytes)?;
    let runs = u32::deserialize(&mut bytes)? as usize;
    if runs > u8::MAX as usize || runs > bytes.len() / 5 {
        return Err(bad());
    }
    let mut parents = Vec::with_capacity(runs);
    for _ in 0..runs {
        let level = u8::deserialize(&mut bytes)?;
        let count = u32::deserialize(&mut bytes)? as usize;
        if count > bytes.len() / 64 {
            return Err(bad());
        }
        let mut hashes = Vec::with_capacity(count);
        for _ in 0..count {
            hashes.push(Hash64::deserialize(&mut bytes)?);
        }
        parents.push((level, hashes));
    }
    let parents_by_level = crate::header::CompressedParents::try_from(parents).map_err(|_| bad())?;
    macro_rules! read {
        () => {
            BorshDeserialize::deserialize(&mut bytes)?
        };
    }
    let mut header = crate::header::Header {
        hash,
        version,
        parents_by_level,
        hash_merkle_root: read!(),
        accepted_id_merkle_root: read!(),
        utxo_commitment: read!(),
        timestamp: read!(),
        bits: read!(),
        nonce: read!(),
        pow_algo_id: read!(),
        daa_score: read!(),
        blue_work: read!(),
        blue_score: read!(),
        pruning_point: read!(),
        evm_payload_hash: read!(),
        evm_commitment_root: read!(),
        overlay_commitment_root: read!(),
        palw_state_root: read!(),
        palw_commitment: Vec::new(),
    };
    let len = u32::deserialize(&mut bytes)? as usize;
    // A PAV3 native custody carriage (only past `palw_monetary_material_v1`) is bounded by its own cap, not the PAV2 commitment
    // cap: two provider signatures and the executor seal alone exceed 8 KiB, so the PAV2 bound made every PAV3 REAL claim's
    // original header unreadable (validation 2026-10-17). Below that fence no accepted header carries the PAV3 magic.
    let cap = if crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::is_carriage(bytes) {
        crate::palw_attempt_material_v3::PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3
    } else {
        crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES
    };
    if len > cap || len != bytes.len() {
        return Err(bad());
    }
    header.palw_commitment = bytes.to_vec();
    Ok(header)
}

/// The ordinary envelope bytes, with allocation limited by bytes actually present. Do not add today's admission
/// shape/version/signature policy here: a legacy claim's accepted identity remains the authority for its original header.
fn decode_attempt(mut bytes: &[u8]) -> std::io::Result<crate::palw_attempt_v2::PalwAttemptEnvelopeV2> {
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_WIRE_MAGIC, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2};
    use borsh::BorshDeserialize;
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed EXEC original attempt");
    // A PAV3 native custody carriage (never a legacy claim: it exists only past `palw_monetary_material_v1`) is bounded at
    // `PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3` before Borsh allocates; its signed base attempt is the original identity.
    if crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::is_carriage(bytes) {
        return crate::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3::decode_wire(bytes).map(|e| e.base).map_err(|_| bad());
    }
    bytes = bytes.strip_prefix(&PALW_ATTEMPT_V2_WIRE_MAGIC).ok_or_else(bad)?;
    fn byte_vec(bytes: &mut &[u8]) -> std::io::Result<Vec<u8>> {
        let len = u32::deserialize(bytes)? as usize;
        if len > bytes.len() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "original attempt vector exceeds bytes"));
        }
        let (value, remaining) = bytes.split_at(len);
        *bytes = remaining;
        Ok(value.to_vec())
    }
    macro_rules! read {
        () => {
            BorshDeserialize::deserialize(&mut bytes)?
        };
    }
    let attempt = PalwAttemptUnsignedV2 {
        version: read!(),
        network_domain: read!(),
        challenge: read!(),
        class_id: read!(),
        executor_bond: read!(),
        executor_pubkey: byte_vec(&mut bytes)?,
        operator_id: read!(),
        artifact_root: read!(),
        trace_root: read!(),
        output_root: read!(),
        pwu: read!(),
        trace_manifest_root: read!(),
        trace_chunk_count: read!(),
        trace_retention_daa: read!(),
        execution_root: read!(),
    };
    let envelope = PalwAttemptEnvelopeV2 { attempt, signature: byte_vec(&mut bytes)? };
    if !bytes.is_empty() {
        return Err(bad());
    }
    Ok(envelope)
}

fn verify_header_bytes(id: Hash64, claim: &PalwClaimStateV2, bytes: &[u8]) -> Result<u64, PalwStateV2Error> {
    verify_header_bytes_with_hashers(
        id,
        claim,
        bytes,
        crate::hashing::header::hash_with_parent_run_cache,
        crate::hashing::header::pre_pow_hash_with_parent_run_cache,
    )
}

// Private pipeline also permits tests to count digest invocations without timing thresholds or global counters.
pub(super) fn verify_header_bytes_with_hashers(
    id: Hash64,
    claim: &PalwClaimStateV2,
    bytes: &[u8],
    identity: impl FnOnce(&crate::header::Header) -> Hash64,
    pre_pow: impl FnOnce(&crate::header::Header) -> Hash64,
) -> Result<u64, PalwStateV2Error> {
    let header = decode_header(bytes).map_err(|_| refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))?;
    if !crate::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
    }
    let envelope = decode_attempt(&header.palw_commitment).map_err(|_| refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))?;
    // These cheap checks may reject, never authenticate, the carried bytes. The accepted identity and anchor follow.
    if envelope.attempt.class_id != claim.class_id
        || envelope.attempt.executor_bond != claim.bond.0
        || crate::palw_attempt_v2::attempt_id_v2(&envelope.attempt) != id
    {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
    }
    if identity(&header) != claim.accepted_block {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
    }
    let anchor = crate::palw_attempt_v2::execution_anchor_v3(
        envelope.attempt.network_domain,
        pre_pow(&header),
        envelope.attempt.class_id,
        &envelope.attempt.executor_bond,
        header.nonce,
    );
    if claim.job_identity != Hash64::default() && anchor != claim.job_identity {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
    }
    Ok(header.daa_score)
}

#[cfg(test)]
#[path = "palw_exec_v2_origin_v1/tests.rs"]
mod tests;

/// Before every native financial write. Riders do not carry their own REAL header and are not root authorizations.
pub(super) fn capture(
    builder: &TransitionBuilder<'_>,
    id: Hash64,
    claim: &PalwClaimStateV2,
    rider: bool,
) -> Result<Option<PalwExecOriginV1>, PalwStateV2Error> {
    if !builder.params.exec_v2_active_at(claim.accepted_daa) || rider {
        return Ok(None);
    }
    let issued_daa = source_time(builder.extras, id, claim)?;
    Ok(Some(PalwExecOriginV1 { source_block: claim.accepted_block, issued_daa }))
}

impl TransitionBuilder<'_> {
    pub(super) fn write_exec_origin(&mut self, id: Hash64, new: Option<PalwExecOriginV1>) -> Result<(), PalwStateV2Error> {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_ORIGINS_V1, id, new, |s| &mut s.exec_v2_origins).map(drop)
    }
}

/// No new independent population: there is at most one clock row per live native claim.
pub(super) fn deserialize<R: std::io::Read>(reader: &mut R, max: usize) -> std::io::Result<BTreeMap<Hash64, PalwExecOriginV1>> {
    use borsh::BorshDeserialize;
    let count = u32::deserialize_reader(reader)? as usize;
    if count > max {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "EXEC origin count exceeds live claims"));
    }
    let mut rows = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key = Hash64::deserialize_reader(reader)?;
        if previous.is_some_and(|old| old >= key) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "EXEC origin keys are not strictly ordered"));
        }
        rows.insert(key, PalwExecOriginV1::deserialize_reader(reader)?);
        previous = Some(key);
    }
    Ok(rows)
}

impl PalwChainStateV2 {
    /// Cheap admission metadata only: no header decode, digest or authentication authority.
    /// Used to charge the block's decode/hash budget before the expensive reader; the reader rechecks it.
    pub fn exec_v2_origin_witness_metadata_admissible_v1(
        &self,
        params: &PalwStateParamsV2,
        now: u64,
        witness: &crate::palw_work_slice_v2::PalwExecOriginWitnessV1,
    ) -> Result<(), PalwStateV2Error> {
        if !params.exec_v2_active_at(now) {
            return Err(refuse(PalwWorkRootRefusalV2::Dormant));
        }
        if witness.header.is_empty() || witness.header.len() > crate::palw_work_slice_v2::PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1 {
            return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
        }
        let claim = self.claims.get(&witness.claim).ok_or_else(|| refuse(PalwWorkRootRefusalV2::NoClaim))?;
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            return Err(refuse(PalwWorkRootRefusalV2::ClaimNotOpenable));
        }
        if self.exec_v2_origins.contains_key(&witness.claim) {
            return Err(refuse(PalwWorkRootRefusalV2::AuthorizationRecorded));
        }
        Ok(())
    }

    /// Permissionless, self-contained legacy provenance. Neither local headers nor extras can supply its clock.
    pub fn exec_v2_origin_witness_admissible_v1(
        &self,
        params: &PalwStateParamsV2,
        now: u64,
        witness: &crate::palw_work_slice_v2::PalwExecOriginWitnessV1,
    ) -> Result<PalwExecOriginV1, PalwStateV2Error> {
        self.exec_v2_origin_witness_metadata_admissible_v1(params, now, witness)?;
        let claim = &self.claims[&witness.claim];
        let row = PalwExecOriginV1 {
            source_block: claim.accepted_block,
            issued_daa: verify_header_bytes(witness.claim, claim, &witness.header)?,
        };
        if let Some(root) = self.exec_v2.roots.get(&witness.claim) {
            let history =
                self.exec_v2_job_uses.get(&root.job_work_id).ok_or_else(|| refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))?;
            if history.source_block != row.source_block || history.issued_daa != row.issued_daa {
                return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
            }
        }
        Ok(row)
    }

    pub fn exec_v2_origin_v1(&self, id: &Hash64) -> Option<&PalwExecOriginV1> {
        self.exec_v2_origins.get(id)
    }

    pub fn exec_v2_origin_count_v1(&self) -> usize {
        self.exec_v2_origins.len()
    }

    pub(super) fn assert_exec_origins(&self) -> Result<(), PalwStateV2Error> {
        for (id, row) in &self.exec_v2_origins {
            let claim =
                self.claims.get(id).ok_or_else(|| PalwStateV2Error::CarriageInconsistent("EXEC origin has no native claim".into()))?;
            if !matches!(claim.source, PalwClaimSourceV2::Attempt)
                || row.source_block != claim.accepted_block
                || row.source_block == Hash64::default()
            {
                return Err(PalwStateV2Error::CarriageInconsistent("EXEC origin differs from its native claim".into()));
            }
        }
        Ok(())
    }
}
