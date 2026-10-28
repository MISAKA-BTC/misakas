//! Bounded transport pages of the original flat auxiliary table. A page is
//! deliberately not a collection proof: only the complete ordered stream can
//! authenticate the original aux root. Old audit32 wire and caps are unchanged.
use crate::{
    palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1, PalwStateOpeningV1},
    palw_state_v2::PalwChainStateV2,
};
use std::ops::Bound::{Excluded, Unbounded};
pub const AUX_PAGE_PREFIX_V39: &str = "kernel-aux-page-v39/";
pub const AUX_PAGE_BYTES_V39: usize = 32 << 20;
pub const AUX_PAGE_ROWS_V39: usize = 1024;
pub fn parse_aux_cursor_v39(request: &[u8]) -> Result<Option<(u8, Vec<u8>)>, String> {
    let text = std::str::from_utf8(request).map_err(|_| "audit39 page request UTF8")?;
    let cursor = text.strip_prefix(AUX_PAGE_PREFIX_V39).ok_or("audit39 page request prefix")?;
    if cursor == "-" {
        return Ok(None);
    }
    if cursor.len() < 2
        || cursor.len() > 8194
        || cursor.len() % 2 != 0
        || !cursor.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("audit39 page cursor must be canonical bounded lower hex".into());
    }
    let byte = |s: &str| u8::from_str_radix(s, 16).map_err(|_| "audit39 cursor byte".to_string());
    let table = byte(&cursor[..2])?;
    let key = (2..cursor.len()).step_by(2).map(|i| byte(&cursor[i..i + 2])).collect::<Result<Vec<_>, _>>()?;
    Ok(Some((table, key)))
}
fn cursor_string(table: u8, key: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = format!("{table:02x}");
    for b in key {
        write!(&mut s, "{b:02x}").expect("String writer");
    }
    s
}
pub fn prove_native_aux_page_v39(state: &PalwChainStateV2, request: &[u8]) -> Result<PalwFactProofV1, String> {
    let cursor = parse_aux_cursor_v39(request)?;
    let route = state.kernel_route().ok_or("audit39 original kernel route absent")?;
    if route.aux.len() > crate::palw_native_audit_proof_v39::NATIVE_AUX_ROWS_MAX_V39 {
        return Err("audit39 original row count exceeds profile".into());
    }
    if let Some(key) = &cursor
        && !route.aux.contains_key(key)
    {
        return Err("audit39 page cursor is not an original row in the pinned state".into());
    }
    let opening = PalwStateOpeningV1::of(state);
    if opening.preimage.len() > 4 << 20 {
        return Err("audit39 original state preimage exceeds profile".into());
    }
    let mut range = route.aux.range((cursor.clone().map(Excluded).unwrap_or(Unbounded), Unbounded)).peekable();
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    while let Some(((table, key), value)) = range.peek().copied() {
        let row_bytes = 5usize
            .checked_add(key.len())
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_add(value.len()))
            .ok_or("audit39 page extent overflow")?;
        if key.len() > 4096 || row_bytes > AUX_PAGE_BYTES_V39 {
            return Err("audit39 original row exceeds bounded whole-row transport profile".into());
        }
        if rows.len() == AUX_PAGE_ROWS_V39 || bytes.checked_add(row_bytes).is_none_or(|n| n > AUX_PAGE_BYTES_V39) {
            break;
        }
        let k = borsh::to_vec(&(*table, key)).map_err(|e| e.to_string())?;
        rows.push((k, value.clone()));
        bytes += row_bytes;
        range.next();
    }
    let done = range.peek().is_none();
    let last = rows
        .last()
        .map(|(k, _)| cursor_string(k[0], &k[5..]))
        .or_else(|| cursor.as_ref().map(|(t, k)| cursor_string(*t, k)))
        .unwrap_or_else(|| "-".into());
    let label = format!("{AUX_PAGE_PREFIX_V39}{}/{}/{last}", route.aux.len(), u8::from(done)).into_bytes();
    Ok(PalwFactProofV1 { state: opening, collection: PalwCollectionOpeningV1 { label, rows } })
}
