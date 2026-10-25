//! Finite REAL authorization and authenticated one-use history, independent of claim retirement.
use super::*;
use crate::palw_work_slice_v2::{
    PALW_EXEC_V2_JOB_AUTHORIZATION_DAA_V1, PALW_EXEC_V2_JOB_USE_SWEEP_V1, PALW_EXEC_V2_MAX_JOB_USES_V1, PalwExecJobUseV1,
    PalwWorkRootDeclarationV2, PalwWorkRootRefusalV2,
};

pub(super) fn authorize(
    state: &PalwChainStateV2,
    declaration: &PalwWorkRootDeclarationV2,
    claim: &PalwClaimStateV2,
    now: u64,
) -> Result<PalwExecJobUseV1, PalwStateV2Error> {
    // Never consult local headers or accept a late supplied clock here. Legacy unrecorded claims are not fresh authorization.
    let row = state
        .exec_v2_origins
        .get(&declaration.root_claim_id)
        .ok_or_else(|| PalwStateV2Error::ExecV2Refused(PalwWorkRootRefusalV2::AuthorizationUnrecorded.to_string()))?;
    if row.source_block != claim.accepted_block {
        return Err(PalwStateV2Error::ExecV2Refused(PalwWorkRootRefusalV2::AuthorizationOrigin.to_string()));
    }
    authorize_at(declaration, claim, row.issued_daa, now)
}

fn authorize_at(
    declaration: &PalwWorkRootDeclarationV2,
    claim: &PalwClaimStateV2,
    issued: u64,
    now: u64,
) -> Result<PalwExecJobUseV1, PalwStateV2Error> {
    let refuse = |why: PalwWorkRootRefusalV2| PalwStateV2Error::ExecV2Refused(why.to_string());
    // These are different DAG clocks, not wall-clock timestamps with a proven order between them. Reaching both is necessary;
    // expiry remains tied solely to the original source, so later acceptance cannot renew an old authorization.
    if issued > now || claim.accepted_daa > now {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationOrigin));
    }
    let until = issued
        .checked_add(PALW_EXEC_V2_JOB_AUTHORIZATION_DAA_V1)
        .filter(|end| *end < u64::MAX)
        .ok_or_else(|| refuse(PalwWorkRootRefusalV2::AuthorizationExpired))?;
    if now >= until {
        return Err(refuse(PalwWorkRootRefusalV2::AuthorizationExpired));
    }
    Ok(PalwExecJobUseV1 {
        owner_root: declaration.root_claim_id,
        class_id: claim.class_id,
        canonical_job_id: declaration.canonical_job_id,
        input_root: declaration.input_root,
        source_block: claim.accepted_block,
        issued_daa: issued,
        expires_daa: until,
    })
}

/// Decode the standard Borsh map encoding without accepting an unbounded allocation or duplicate keys.
pub(super) fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<BTreeMap<Hash64, PalwExecJobUseV1>> {
    use borsh::BorshDeserialize;
    let count = u32::deserialize_reader(reader)? as usize;
    if count > PALW_EXEC_V2_MAX_JOB_USES_V1 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "EXEC job-use history exceeds its cap"));
    }
    let mut rows = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key = Hash64::deserialize_reader(reader)?;
        if previous.is_some_and(|old| old >= key) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "EXEC job-use history keys are not strictly ordered"));
        }
        let row = PalwExecJobUseV1::deserialize_reader(reader)?;
        rows.insert(key, row);
        previous = Some(key);
    }
    Ok(rows)
}

pub(super) fn rebuild(state: &mut PalwChainStateV2) {
    state.exec_v2_job_use_deadlines = state
        .exec_v2_job_uses
        .iter()
        .filter(|(_, row)| !state.exec_v2.roots.contains_key(&row.owner_root))
        .map(|(key, row)| (row.expires_daa, *key))
        .collect();
}

pub(super) fn index_replace(
    state: &mut PalwChainStateV2,
    key: Hash64,
    old: Option<&PalwExecJobUseV1>,
    new: Option<&PalwExecJobUseV1>,
) {
    if let Some(row) = old {
        state.exec_v2_job_use_deadlines.remove(&(row.expires_daa, key));
    }
    if let Some(row) = new
        && !state.exec_v2.roots.contains_key(&row.owner_root)
    {
        state.exec_v2_job_use_deadlines.insert((row.expires_daa, key));
    }
}

pub(super) fn root_changed(state: &mut PalwChainStateV2, root: &crate::palw_work_slice_v2::PalwWorkRootV2, present: bool) {
    if let Some(row) = state.exec_v2_job_uses.get(&root.job_work_id) {
        let point = (row.expires_daa, root.job_work_id);
        if present {
            state.exec_v2_job_use_deadlines.remove(&point);
        } else {
            state.exec_v2_job_use_deadlines.insert(point);
        }
    }
}

pub(super) fn sweep(builder: &mut TransitionBuilder<'_>, now: u64) {
    let due: Vec<_> = builder
        .state
        .exec_v2_job_use_deadlines
        .range(..=(now, Hash64::from_bytes([255; 64])))
        .take(PALW_EXEC_V2_JOB_USE_SWEEP_V1)
        .map(|(_, key)| *key)
        .collect();
    for key in due {
        builder.write_exec_job_use(key, None);
    }
}

impl TransitionBuilder<'_> {
    pub(super) fn write_exec_job_use(&mut self, key: Hash64, new: Option<PalwExecJobUseV1>) {
        let old = self.state.exec_v2_job_uses.get(&key).cloned();
        if let Some(row) = old {
            self.state.exec_v2_job_use_deadlines.remove(&(row.expires_daa, key));
        }
        if let Some(row) = &new
            && !self.state.exec_v2.roots.contains_key(&row.owner_root)
        {
            self.state.exec_v2_job_use_deadlines.insert((row.expires_daa, key));
        }
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_JOB_USES_V1, key, new, |s| &mut s.exec_v2_job_uses);
    }
}

impl PalwChainStateV2 {
    pub fn exec_v2_job_use_v1(&self, key: &Hash64) -> Option<&PalwExecJobUseV1> {
        self.exec_v2_job_uses.get(key)
    }
    pub fn exec_v2_job_use_count_v1(&self) -> usize {
        self.exec_v2_job_uses.len()
    }

    pub(super) fn assert_exec_job_uses(&self) -> Result<(), PalwStateV2Error> {
        let bad = |why: &str| PalwStateV2Error::CarriageInconsistent(why.to_owned());
        if self.exec_v2_job_uses.len() > PALW_EXEC_V2_MAX_JOB_USES_V1 {
            return Err(bad("EXEC job-use history exceeds its cap"));
        }
        let expected: BTreeSet<_> = self
            .exec_v2_job_uses
            .iter()
            .filter(|(_, row)| !self.exec_v2.roots.contains_key(&row.owner_root))
            .map(|(key, row)| (row.expires_daa, *key))
            .collect();
        if expected != self.exec_v2_job_use_deadlines {
            return Err(bad("EXEC job-use expiry index differs from its rows"));
        }
        for (key, row) in &self.exec_v2_job_uses {
            if row.work_id() != *key
                || self.exec_v2.roots.get(&row.owner_root).is_some_and(|root| root.job_work_id != *key)
                || row.issued_daa.checked_add(PALW_EXEC_V2_JOB_AUTHORIZATION_DAA_V1) != Some(row.expires_daa)
                || row.expires_daa == u64::MAX
                || self.last_point.is_none_or(|point| point.daa_score < row.issued_daa)
                || [row.owner_root, row.class_id, row.canonical_job_id, row.input_root, row.source_block].contains(&Hash64::default())
            {
                return Err(bad("EXEC job-use history has invalid authorization terms"));
            }
        }
        for (id, root) in &self.exec_v2.roots {
            let row =
                self.exec_v2_job_uses.get(&root.job_work_id).ok_or_else(|| bad("EXEC root has no authenticated job-use history"))?;
            let claim = self.claims.get(id).ok_or_else(|| bad("EXEC root has no native claim"))?;
            if row.owner_root != *id
                || row.class_id != root.class_id
                || row.canonical_job_id != root.canonical_job_id
                || row.source_block != claim.accepted_block
                || row.issued_daa > root.opened_daa
                || self
                    .exec_v2_origins
                    .get(id)
                    .is_some_and(|origin| origin.issued_daa != row.issued_daa || origin.source_block != row.source_block)
                || root.opened_daa < claim.accepted_daa
                || root.opened_daa >= row.expires_daa
            {
                return Err(bad("EXEC root differs from its immutable authorization history"));
            }
        }
        Ok(())
    }
}
