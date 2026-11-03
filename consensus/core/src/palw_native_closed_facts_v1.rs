//! Compact canonical CLOSED work accounting. Values are derived from actual
//! accepted sources by owning folds; this namespace has no carrier/DTO ingress.
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1};
use crate::{
    Hash64,
    palw_native_settlement_v1::MatureUsefulWorkV1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateV2Error},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const NATIVE_CLOSED_FACTS_TABLE_V1: u8 = 195;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ClosedFactFamilyV1 {
    NativeMining39,
    LegacyAttempt,
    LegacyFp,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeClosedFactOriginV1 {
    pub family: ClosedFactFamilyV1,
    pub subject: Hash64,
    pub work: Hash64,
    pub ruleset: crate::Hash,
    pub canonical_work_daa: Option<u64>,
    pub quantum: Option<u32>,
    pub approval: Option<Hash64>,
    pub source_manifest: Hash64,
    pub once_obligation: Option<Hash64>,
    /// Actual source-capture journal point; the root below is that transition's
    /// immutable PARENT basis, not a circular post-row state root.
    pub source_point: PalwBlockContextV2,
    pub source_state_root: Hash64,
    pub terminal_daa: u64,
    pub material_until_daa: u64,
    pub quantum_maturity_daa: u64,
    pub retirement_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeClosedFactRowV1 {
    pub version: u16,
    pub fact: MatureUsefulWorkV1,
    pub origin: NativeClosedFactOriginV1,
    pub closed_at: PalwBlockContextV2,
}
crate::palw_native_resource_v1::native_enum!(ClosedFactFamilyV1;NativeMining39,LegacyAttempt,LegacyFp);
crate::palw_native_resource_v1::native_struct!(MatureUsefulWorkV1;identity,anchor,operator,class,anchor_blue,accepted_blue,anchor_daa,accepted_daa,matured_daa,work);
crate::palw_native_resource_v1::wire::wire_struct!(MatureUsefulWorkV1;Hash64,Hash64,Hash64,Hash64,u64,u64,u64,u64,u64,u128);
crate::palw_native_resource_v1::native_struct!(NativeClosedFactOriginV1;family,subject,work,ruleset,canonical_work_daa,quantum,approval,source_manifest,once_obligation,source_point,source_state_root,terminal_daa,material_until_daa,quantum_maturity_daa,retirement_daa);
crate::palw_native_resource_v1::native_struct!(NativeClosedFactRowV1;version,fact,origin,closed_at);
crate::palw_native_resource_v1::wire::wire_struct!(NativeClosedFactOriginV1;ClosedFactFamilyV1,Hash64,Hash64,crate::Hash,Option<u64>,Option<u32>,Option<Hash64>,Hash64,Option<Hash64>,PalwBlockContextV2,Hash64,u64,u64,u64,u64);
crate::palw_native_resource_v1::wire::wire_struct!(NativeClosedFactRowV1;u16,MatureUsefulWorkV1,NativeClosedFactOriginV1,PalwBlockContextV2);
impl crate::palw_native_resource_v1::NativeWireScanV1 for ClosedFactFamilyV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0..=2 => Ok(Default::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "closed fact source family")),
        }
    }
}
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
impl NativeClosedFactRowV1 {
    pub fn validate_v1(&self, key: Hash64) -> Result<(), PalwStateV2Error> {
        let f = &self.fact;
        let o = &self.origin;
        if self.version != 1
            || key != f.identity
            || f.identity == Hash64::default()
            || f.anchor == Hash64::default()
            || f.operator == Hash64::default()
            || f.class == Hash64::default()
            || f.work == 0
            || f.matured_daa > self.closed_at.daa_score
            || f.accepted_daa > self.closed_at.daa_score
            || f.accepted_blue > self.closed_at.blue_score
            || o.subject == Hash64::default()
            || o.source_manifest == Hash64::default()
            || o.source_state_root == Hash64::default()
            || o.approval.is_none_or(|approval| approval == Hash64::default())
            || o.once_obligation == Some(Hash64::default())
            || o.source_point.block == Hash64::default()
            || self.closed_at.block == Hash64::default()
            || f.anchor_daa > f.accepted_daa
            || f.anchor_blue > f.accepted_blue
            || o.source_point.daa_score < f.accepted_daa
            || o.source_point.blue_score < f.accepted_blue
            || o.source_point.daa_score > self.closed_at.daa_score
            || o.source_point.blue_score > self.closed_at.blue_score
        {
            return Err(bad("compact closed work identity/source/clock"));
        }
        if o.work == Hash64::default()
            || match o.quantum {
                Some(q) => f.identity != crate::palw_native_settlement_v1::fp_slice_identity(&o.work, q),
                None => f.identity != o.work,
            }
        {
            return Err(bad("closed fact canonical unsliced work/quantum differs"));
        }
        let quantum_at = match o.family {
            ClosedFactFamilyV1::LegacyAttempt => 0,
            ClosedFactFamilyV1::LegacyFp => f.accepted_daa.saturating_add(o.quantum_maturity_daa),
            ClosedFactFamilyV1::NativeMining39 => f.anchor_daa.saturating_add(o.quantum_maturity_daa),
        };
        let expected = o.material_until_daa.max(o.terminal_daa.saturating_add(o.retirement_daa)).max(quantum_at);
        if expected != f.matured_daa || o.terminal_daa > o.source_point.daa_score {
            return Err(bad("closed fact differs from exact source liability/maturity clocks"));
        }
        match o.family {
            ClosedFactFamilyV1::NativeMining39 if o.quantum.is_none() || o.approval.is_none() || o.once_obligation.is_none() => {
                return Err(bad("closed native work lost approval/actual issuance origin"));
            }
            ClosedFactFamilyV1::LegacyAttempt if o.quantum.is_some() => return Err(bad("closed attempt aliases an execution slice")),
            ClosedFactFamilyV1::LegacyFp if o.quantum.is_none() => return Err(bad("closed FP slice lost actual quantum identity")),
            _ => {}
        }
        Ok(())
    }
}
pub fn read_native_closed_facts_v1(
    state: &PalwChainStateV2,
) -> Result<NativeRetainedV1<Vec<(Hash64, NativeClosedFactRowV1)>>, PalwStateV2Error> {
    let count =
        state.kernel_route_state_v1().map_or(0, |route| route.aux.keys().filter(|(t, _)| *t == NATIVE_CLOSED_FACTS_TABLE_V1).count());
    let mut quote = TirNativeRequestsV1::new();
    quote.exact_v1::<(Hash64, NativeClosedFactRowV1)>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    let lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(quote.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<NativeClosedFactRowV1>(
            "closed canonical facts",
            "allocation refused",
        ))
    })?;
    if let Some(route) = state.kernel_route_state_v1() {
        for ((table, key), bytes) in &route.aux {
            if *table != NATIVE_CLOSED_FACTS_TABLE_V1 {
                continue;
            }
            let id = Hash64::from_bytes(key.as_slice().try_into().map_err(|_| bad("closed fact canonical key width"))?);
            let row =
                crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<NativeClosedFactRowV1, std::io::Error>(
                    bytes,
                    || borsh::from_slice(bytes),
                )
                .map_err(|e| match e {
                    NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                    NativeDecodeErrorV1::Decode(e) => bad(e),
                })?;
            row.validate_v1(id)?;
            rows.push((id, (*row).clone()));
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(rows, lease))
}

/// Semantic network context remains mandatory even after all heavy sources and
/// aux193 have retired. The authenticated pruning state carries the actual past
/// ledger decision; this is neither a producer assertion nor a computation proof.
pub fn validate_fact_archives_context_v1(
    state: &PalwChainStateV2,
    params: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    crate::palw_state_v2::validate_compact_sources_context_v1(state, params)?;
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        if read_native_closed_facts_v1(state)?.is_empty() {
            return Ok(());
        }
        return Err(bad("closed facts require actual V2 network rules"));
    };
    for (_, row) in read_native_closed_facts_v1(state)?.iter() {
        let o = &row.origin;
        if o.ruleset != params.consensus_params_id()
            || o.canonical_work_daa != params.palw_canonical_work_daa()
            || o.retirement_daa != bundle.state.claim_retirement_daa()
            || (o.family != ClosedFactFamilyV1::LegacyAttempt && o.quantum_maturity_daa != params.palw_exec_quantum_maturity_v1())
            || (o.family == ClosedFactFamilyV1::LegacyAttempt && o.quantum_maturity_daa != 0)
            || row.fact.class == bundle.state.base_class_id()
            || state.last_point().is_none_or(|p| row.closed_at.daa_score > p.daa_score || row.closed_at.blue_score > p.blue_score)
        {
            return Err(bad("closed fact differs from actual network pricing/maturity context"));
        }
        if o.family == ClosedFactFamilyV1::NativeMining39 {
            let mining = params
                .palw_native_mining_v39
                .as_ref()
                .filter(|f| f.activation.is_active(row.fact.anchor_daa))
                .ok_or_else(|| bad("closed native fact predates its actual mining rule"))?;
            let cert = bundle.state.rfc0018_v1().ok_or_else(|| bad("closed native fact approval policy absent"))?.certification;
            let until = o
                .terminal_daa
                .saturating_add(mining.policy.receipt_lifetime_daa)
                .saturating_add(cert.lifetime_daa())
                .saturating_add(cert.prosecution_window_daa);
            if row.fact.work != u128::from(mining.policy.network_quantum_mac)
                || o.material_until_daa != until
                || !params.palw_settlement_once_active_at(row.fact.anchor_daa)
            {
                return Err(bad("closed native fact differs from actual quantum/receipt liability/issuance rule"));
            }
        }
    }
    crate::palw_state_v2::palw_validate_legacy_pending_fact_context_v1(state, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn point(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h(n), daa_score: n, blue_score: n, subsidy: 0 }
    }
    fn row() -> NativeClosedFactRowV1 {
        let work = h(2);
        let identity = crate::palw_native_settlement_v1::fp_slice_identity(&work, 3);
        NativeClosedFactRowV1 {
            version: 1,
            fact: MatureUsefulWorkV1 {
                identity,
                anchor: h(10),
                operator: h(4),
                class: h(5),
                anchor_blue: 10,
                accepted_blue: 11,
                anchor_daa: 10,
                accepted_daa: 11,
                matured_daa: 40,
                work: 7,
            },
            origin: NativeClosedFactOriginV1 {
                family: ClosedFactFamilyV1::NativeMining39,
                subject: h(6),
                work,
                ruleset: crate::Hash::from_bytes([7; 32]),
                canonical_work_daa: Some(0),
                quantum: Some(3),
                approval: Some(h(8)),
                source_manifest: h(9),
                once_obligation: Some(h(12)),
                source_point: point(20),
                source_state_root: h(13),
                terminal_daa: 15,
                material_until_daa: 40,
                quantum_maturity_daa: 12,
                retirement_daa: 10,
            },
            closed_at: point(41),
        }
    }
    #[test]
    fn closed_fact_codec_preserves_unsliced_identity_and_exact_liability_clocks() {
        let original = row();
        original.validate_v1(original.fact.identity).unwrap();
        let bytes = borsh::to_vec(&original).unwrap();
        crate::palw_native_resource_v1::preflight_native_value_v1::<NativeClosedFactRowV1>(bytes.as_slice(), bytes.len() as u64)
            .unwrap();
        assert_eq!(borsh::from_slice::<NativeClosedFactRowV1>(&bytes).unwrap(), original);
        for mutation in 0..7 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.origin.quantum = Some(4),
                1 => changed.origin.work = h(99),
                2 => changed.fact.matured_daa += 1,
                3 => changed.origin.material_until_daa += 1,
                4 => changed.origin.approval = None,
                5 => changed.origin.once_obligation = None,
                _ => changed.origin.source_point = point(42),
            }
            assert!(changed.validate_v1(changed.fact.identity).is_err(), "invalid compact source mutation {mutation}");
        }
        assert!(original.validate_v1(h(99)).is_err(), "aux key cannot alias another work identity");
        let mut immature = original;
        immature.closed_at = point(39);
        assert!(immature.validate_v1(immature.fact.identity).is_err());
    }
}
