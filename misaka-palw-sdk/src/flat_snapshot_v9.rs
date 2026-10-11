//! Protected candidate/seal reads from one complete op-211 snapshot.
//! The caller authenticates the anchor to its local chain or a state proof. Matching
//! an anchor copied from the remote response establishes only internal consistency.
use crate::{onboarding_snapshot::KernelRowsSnapshotV1, runtime_pack::commit::Refusal};
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_vector_v3::FlatModelVectorHeaderV3,
    palw_flat_model_vector_v9::{FlatModelVectorViewV9, FlatNativeSealRowV9},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_state_v2::PalwBondKeyV2,
};
use serde::{Deserialize, Serialize};

/// Supplied separately from remote pages. No constructor authenticates chain membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlatSnapshotAnchorV9 {
    pub tip_daa: u64,
    pub network_domain: Hash64,
    pub ruleset_digest: Hash64,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

pub struct FlatPublicSnapshotV9 {
    anchor: FlatSnapshotAnchorV9,
    route: PalwKernelRouteStateV1,
}

impl FlatPublicSnapshotV9 {
    pub fn reads(&self, vector: Hash64) -> FlatSnapshotReadsV9<'_> {
        FlatSnapshotReadsV9 { snapshot: self, vector }
    }
    pub fn anchor(&self) -> &FlatSnapshotAnchorV9 {
        &self.anchor
    }
    pub fn from_snapshot(snapshot: KernelRowsSnapshotV1, anchor: FlatSnapshotAnchorV9) -> Result<Self, Refusal> {
        let (tip, route) = snapshot.into_route_v1()?;
        if tip != anchor.tip_daa
            || route.ledger_root() != anchor.ledger_root
            || route.aux_root() != anchor.aux_root
            || route.header.policy.network_domain != anchor.network_domain.as_bytes()
            || route.header.policy.ruleset_digest != anchor.ruleset_digest.as_bytes()
        {
            return Err(Refusal::new("SNAPSHOT_ANCHOR", "served snapshot differs from the separately supplied chain anchor"));
        }
        Ok(Self { anchor, route })
    }

    /// Rebuild the same typed context and resource scope the native court uses.
    pub fn candidate(
        &self,
        vector: Hash64,
        class: Hash64,
        binding: Hash64,
    ) -> Result<(FlatModelVectorHeaderV3, FlatModelVectorViewV9), Refusal> {
        let h = self.route.flat_model_vector_header_v3(&vector).ok_or_else(|| Refusal::new("NO_FLAT_VECTOR", "candidate absent"))?;
        if h.class != class || h.binding != binding || !self.route.flat_model_vector_protected_v9(&vector) {
            return Err(Refusal::new("FLAT_SNAPSHOT_BINDING", "the requested protected vector, class and binding differ"));
        }
        let view = self.route.flat_model_vector_view_v9(&vector).map_err(|e| Refusal::new("FLAT_SNAPSHOT_CONTEXT", e))?;
        Ok((h, view))
    }

    /// Export the exact own mature row at the anchored tip. Exact private proof/salt
    /// checks still happen in reveal preparation, and native acceptance checks current state.
    pub fn reveal_row(
        &self,
        vector: Hash64,
        class: Hash64,
        binding: Hash64,
        bond: &PalwBondKeyV2,
    ) -> Result<FlatNativeSealRowV9, Refusal> {
        let (h, _) = self.candidate(vector, class, binding)?;
        let now = self.anchor.tip_daa;
        if h.refuted || self.route.flat_vector_unavailable_v9(&vector) || now >= h.liability_until {
            return Err(Refusal::new("FLAT_SNAPSHOT_CLOSED", "protected candidate is no longer open at the anchored tip"));
        }
        let row = self
            .route
            .flat_model_seal_v9(&vector, &palw_kernel_bond_id_v1(bond))
            .ok_or_else(|| Refusal::new("NO_FLAT_SEAL", "this signing bond has no authenticated protected seal"))?;
        if row.bond != *bond || row.seal.spent || now < row.seal.mature_daa || now >= row.seal.expires_daa {
            return Err(Refusal::new("FLAT_SNAPSHOT_SEAL", "own seal is spent, immature or expired at the anchored tip"));
        }
        Ok(row)
    }
}

pub struct FlatSnapshotReadsV9<'a> {
    snapshot: &'a FlatPublicSnapshotV9,
    vector: Hash64,
}
impl FlatSnapshotReadsV9<'_> {
    fn answer(
        &self,
        read: misaka_palw_kernel::flat_detect5::FlatReadV5,
        kind: kaspa_consensus_core::palw_flat_model_vector_da_v9::FlatVectorUnitKindV9,
    ) -> Option<kaspa_consensus_core::palw_flat_model_vector_da_v9::FlatVectorAnswerV9> {
        use kaspa_consensus_core::palw_flat_model_vector_da_v9::*;
        let unit = FlatVectorUnitV9 { position: read.position, occurrence: read.occurrence, node: read.node, kind };
        decode_flat_vector_answer_v9(&self.snapshot.route.flat_vector_public_answer_v9(&self.vector, &unit)?).ok()
    }
}
impl misaka_palw_kernel::flat_detect5::FlatPublicReadsV5 for FlatSnapshotReadsV9<'_> {
    fn node(&self, read: misaka_palw_kernel::flat_detect5::FlatReadV5) -> Option<misaka_palw_kernel::seg::NodeOpeningV1> {
        use kaspa_consensus_core::palw_flat_model_vector_da_v9::*;
        if let FlatVectorAnswerV9::Hash(a) = self.answer(read, FlatVectorUnitKindV9::Node)? { Some(a.opening) } else { None }
    }
    fn hash(&self, read: misaka_palw_kernel::flat_detect5::FlatReadV5) -> Option<misaka_palw_kernel::merkle5::FlatLeafHashV5> {
        use kaspa_consensus_core::palw_flat_model_vector_da_v9::*;
        if let FlatVectorAnswerV9::Hash(a) = self.answer(read, FlatVectorUnitKindV9::Hash(read.leaf?))? { a.leaf } else { None }
    }
}
impl misaka_palw_kernel::flat_detect9::FlatPublicReadsV9 for FlatSnapshotReadsV9<'_> {
    fn value(&self, read: misaka_palw_kernel::flat_detect9::FlatValueReadV9) -> Option<Vec<u8>> {
        use kaspa_consensus_core::palw_flat_model_vector_da_v9::*;
        if let FlatVectorAnswerV9::Value(a) = self.answer(read.node, FlatVectorUnitKindV9::Value(read.element))? {
            (a.scope == read.scope).then(|| a.to_bytes().ok()).flatten()
        } else {
            None
        }
    }
    fn header(&self, read: misaka_palw_kernel::flat_detect9::FlatHeaderReadV9) -> Option<Vec<u8>> {
        use kaspa_consensus_core::palw_flat_model_vector_da_v9::*;
        if let FlatVectorAnswerV9::Header { value, .. } = self.answer(read.node, FlatVectorUnitKindV9::Header)? {
            (value.scope == read.scope).then(|| value.to_bytes().ok()).flatten()
        } else {
            None
        }
    }
}
