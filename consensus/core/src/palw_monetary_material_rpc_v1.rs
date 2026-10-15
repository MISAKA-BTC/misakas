//! A flat authenticated aux commitment needs every row, even when a client only
//! asks for monetary contracts. Pages bound transport bytes and fragments permit
//! a large individual row. A subset is never called an authenticated proof.
use crate::{Hash64, header::Header, palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1, palw_state_v2::PalwChainStateV2};
pub const PALW_MONETARY_RPC_MAX_PAGE_BYTES_V1: usize = 1 << 20;
pub const PALW_MONETARY_RPC_MAX_FRAGMENTS_V1: usize = 128;
pub const PALW_MONETARY_RPC_MAX_KEY_BYTES_V1: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMonetaryAuxFragmentV1 {
    pub row: u64,
    pub table: u8,
    pub key: Vec<u8>,
    pub row_bytes: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct PalwMonetaryAuxPageV1 {
    pub header: Header,
    pub parent_header: Header,
    pub state_preimage: Vec<u8>,
    pub fork_choice: Option<PalwForkChoiceOpeningV1>,
    /// Where the serving node's writer began the `kernel-route/v1` block in `state_preimage`
    /// (`PalwChainStateV2::state_root_preimage_located_v1`). Never trusted: the client derives the offset from the authenticated
    /// preimage alone (`PalwChainStateV2::kernel_route_offset_of_preimage_v1`) and refuses any page that names another.
    pub kernel_route_offset: u64,
    pub aux_root: Hash64,
    pub total_rows: u64,
    pub start_row: u64,
    pub start_offset: u64,
    pub next_row: u64,
    pub next_offset: u64,
    pub fragments: Vec<PalwMonetaryAuxFragmentV1>,
}
pub fn monetary_aux_fragments_v1(
    state: &PalwChainStateV2,
    row: u64,
    offset: u64,
    limit: usize,
) -> Result<(Hash64, u64, u64, u64, Vec<PalwMonetaryAuxFragmentV1>), String> {
    if !(1024..=PALW_MONETARY_RPC_MAX_PAGE_BYTES_V1).contains(&limit) {
        return Err("monetary RPC page byte limit must be 1024..1048576".into());
    }
    let route = state.kernel_route().ok_or("authenticated monetary RPC needs the kernel auxiliary commitment")?;
    let total = route.aux.len() as u64;
    if row > total || (row == total && offset != 0) {
        return Err("monetary RPC cursor is outside the pinned snapshot".into());
    }
    let mut next_row = row;
    let mut next_offset = offset;
    let mut used = 0usize;
    let mut fragments = Vec::new();
    for ((table, key), value) in route.aux.iter().skip(row as usize) {
        if key.len() > PALW_MONETARY_RPC_MAX_KEY_BYTES_V1 || value.len() > u32::MAX as usize || next_offset > value.len() as u64 {
            return Err("monetary RPC row/key/cursor exceeds its wire bound".into());
        }
        let framing = 64 + key.len();
        let remaining = limit.saturating_sub(used);
        if remaining <= framing || fragments.len() == PALW_MONETARY_RPC_MAX_FRAGMENTS_V1 {
            break;
        }
        let available = value.len() - next_offset as usize;
        let len = available.min(remaining - framing);
        fragments.push(PalwMonetaryAuxFragmentV1 {
            row: next_row,
            table: *table,
            key: key.clone(),
            row_bytes: value.len() as u64,
            offset: next_offset,
            bytes: value[next_offset as usize..next_offset as usize + len].to_vec(),
        });
        used += framing + len;
        next_offset += len as u64;
        if next_offset == value.len() as u64 {
            next_row += 1;
            next_offset = 0;
        } else {
            break;
        }
    }
    if row < total && fragments.is_empty() {
        return Err("monetary RPC requested page cannot fit its next row header".into());
    }
    Ok((route.aux_root(), total, next_row, next_offset, fragments))
}
