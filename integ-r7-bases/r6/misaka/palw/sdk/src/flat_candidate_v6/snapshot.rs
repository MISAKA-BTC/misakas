//! Protected candidate/seal reads from one complete op-211 snapshot.
//! The caller authenticates the anchor to its local chain or a state proof. Matching
//! an anchor copied from the remote response establishes only internal consistency.
use crate::{
    onboarding_snapshot::{KernelRowsPageV1, KernelRowsSnapshotV1, MAX_SNAPSHOT_BYTES_V1},
    runtime_pack::commit::Refusal,
};
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_seal_v6::FlatNativeSealRowV6,
    palw_flat_model_vector_v3::{FlatModelVectorHeaderV3, FlatModelVectorViewV3},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_state_v2::PalwBondKeyV2,
};
use serde::{Deserialize, Serialize};

/// Supplied separately from remote pages. No constructor authenticates chain membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlatSnapshotAnchorV6 {
    pub tip_daa: u64,
    pub network_domain: Hash64,
    pub ruleset_digest: Hash64,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

pub struct FlatPublicSnapshotV6 {
    anchor: FlatSnapshotAnchorV6,
    route: PalwKernelRouteStateV1,
}

impl FlatPublicSnapshotV6 {
    pub fn anchor(&self) -> &FlatSnapshotAnchorV6 {
        &self.anchor
    }
    pub fn from_snapshot(snapshot: KernelRowsSnapshotV1, anchor: FlatSnapshotAnchorV6) -> Result<Self, Refusal> {
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
    ) -> Result<(FlatModelVectorHeaderV3, FlatModelVectorViewV3), Refusal> {
        let h = self.route.flat_model_vector_header_v3(&vector).ok_or_else(|| Refusal::new("NO_FLAT_VECTOR", "candidate absent"))?;
        if h.class != class || h.binding != binding || !self.route.flat_model_vector_protected_v6(&vector) {
            return Err(Refusal::new("FLAT_SNAPSHOT_BINDING", "the requested protected vector, class and binding differ"));
        }
        let view = self.route.flat_model_vector_view_v3(&vector).map_err(|e| Refusal::new("FLAT_SNAPSHOT_CONTEXT", e))?;
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
    ) -> Result<FlatNativeSealRowV6, Refusal> {
        let (h, _) = self.candidate(vector, class, binding)?;
        let now = self.anchor.tip_daa;
        if h.refuted || self.route.flat_vector_unavailable_v3(&vector) || now >= h.liability_until {
            return Err(Refusal::new("FLAT_SNAPSHOT_CLOSED", "protected candidate is no longer open at the anchored tip"));
        }
        let row = self
            .route
            .flat_model_seal_v6(&vector, &palw_kernel_bond_id_v1(bond))
            .ok_or_else(|| Refusal::new("NO_FLAT_SEAL", "this signing bond has no authenticated protected seal"))?;
        if row.bond != *bond || row.seal.spent || now < row.seal.mature_daa || now >= row.seal.expires_daa {
            return Err(Refusal::new("FLAT_SNAPSHOT_SEAL", "own seal is spent, immature or expired at the anchored tip"));
        }
        Ok(row)
    }
}

/// JSON of one getPalwKernelRows response, without a dependency on the RPC transport.
/// Byte/count/hex bounds precede decoding additional binary copies. This is a local
/// reader ceiling, not a consensus or peak-memory resource guarantee.
pub const MAX_FLAT_RPC_PAGE_JSON_V6: usize = 2 * MAX_SNAPSHOT_BYTES_V1 + (1 << 20);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WirePage {
    available: bool,
    tip_daa: u64,
    ledger_root: Hash64,
    aux_root: Hash64,
    header: String,
    rows: Vec<WireRow>,
    more: bool,
    next_table: u32,
    next_key: String,
    total_rows: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRow {
    table: u32,
    key: String,
    row: String,
}

fn unhex(s: &str) -> Result<Vec<u8>, Refusal> {
    if !s.len().is_multiple_of(2) || !s.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(Refusal::new("SNAPSHOT_HEX", "RPC page contains malformed hex"));
    }
    let digit = |x: u8| if x.is_ascii_digit() { x - b'0' } else { x.to_ascii_lowercase() - b'a' + 10 };
    Ok(s.as_bytes().chunks_exact(2).map(|b| digit(b[0]) * 16 + digit(b[1])).collect())
}

pub fn flat_rpc_page_v6(json: &[u8]) -> Result<KernelRowsPageV1, Refusal> {
    let bad = |why: &str| Refusal::new("SNAPSHOT_TOO_LARGE", why);
    if json.len() > MAX_FLAT_RPC_PAGE_JSON_V6 {
        return Err(bad("RPC JSON exceeds the local page ceiling"));
    }
    let page: WirePage = serde_json::from_slice(json).map_err(|e| Refusal::new("SNAPSHOT_JSON", e.to_string()))?;
    if !page.available {
        return Err(Refusal::new("NO_KERNEL_ROWS", "node serves no kernel route rows"));
    }
    let bytes = page.rows.iter().try_fold(page.header.len(), |n, r| n.checked_add(r.key.len())?.checked_add(r.row.len()));
    if page.header.len() > 2 * (64 << 10)
        || page.total_rows > (MAX_SNAPSHOT_BYTES_V1 / 32) as u64
        || page.rows.len() as u64 > page.total_rows
        || bytes.is_none_or(|n| n > 2 * MAX_SNAPSHOT_BYTES_V1)
        || page.next_key.len() > 2 * MAX_SNAPSHOT_BYTES_V1
    {
        return Err(bad("RPC page exceeds declared rows or the local decoded ceiling"));
    }
    let table = |n| u8::try_from(n).map_err(|_| Refusal::new("SNAPSHOT_TABLE", "RPC table exceeds u8"));
    let next = if page.more {
        Some((table(page.next_table)?, unhex(&page.next_key)?))
    } else {
        if page.next_table != 0 || !page.next_key.is_empty() {
            return Err(Refusal::new("SNAPSHOT_CURSOR", "terminal page has a stray cursor"));
        }
        None
    };
    Ok(KernelRowsPageV1 {
        tip_daa: page.tip_daa,
        ledger_root: page.ledger_root,
        aux_root: page.aux_root,
        header: unhex(&page.header)?,
        total_rows: page.total_rows,
        rows: page.rows.into_iter().map(|r| Ok((table(r.table)?, unhex(&r.key)?, unhex(&r.row)?))).collect::<Result<_, Refusal>>()?,
        next,
    })
}
