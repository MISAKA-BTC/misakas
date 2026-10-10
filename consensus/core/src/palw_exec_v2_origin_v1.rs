//! Original REAL clock captured at native acceptance, then authenticated by state and carriage.
use super::*;
use crate::palw_work_slice_v2::{PalwExecOriginV1, PalwWorkRootRefusalV2};

pub(super) fn source_time(
    extras: &PalwTransitionExtrasV1,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
) -> Result<u64, PalwStateV2Error> {
    let refuse = |why: PalwWorkRootRefusalV2| PalwStateV2Error::ExecV2Refused(why.to_string());
    let issued = match extras.exec_v2_origin_headers.get(&claim.accepted_block) {
        Some(bytes) => {
            let header: crate::header::Header =
                borsh::from_slice(bytes).map_err(|_| refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))?;
            if crate::hashing::header::hash(&header) != claim.accepted_block
                || !crate::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id)
            {
                return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
            }
            let envelope = crate::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment)
                .map_err(|_| refuse(PalwWorkRootRefusalV2::AuthorizationOrigin))?;
            let anchor = crate::palw_attempt_v2::execution_anchor_v3(
                envelope.attempt.network_domain,
                crate::hashing::header::pre_pow_hash_64(&header),
                envelope.attempt.class_id,
                &envelope.attempt.executor_bond,
                header.nonce,
            );
            if crate::palw_attempt_v2::attempt_id_v2(&envelope.attempt) != claim_id
                || envelope.attempt.class_id != claim.class_id
                || envelope.attempt.executor_bond != claim.bond.0
                || (claim.job_identity != Hash64::default() && anchor != claim.job_identity)
            {
                return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
            }
            header.daa_score
        }
        None => {
            // Only an explicitly named arithmetic fixture seam may omit the original header.
            #[cfg(test)]
            if let Some(issued) = extras.exec_v2_test_origin_daa {
                return Ok(issued);
            }
            return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
        }
    };
    Ok(issued)
}

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
    pub(super) fn write_exec_origin(&mut self, id: Hash64, new: Option<PalwExecOriginV1>) {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_ORIGINS_V1, id, new, |s| &mut s.exec_v2_origins);
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
