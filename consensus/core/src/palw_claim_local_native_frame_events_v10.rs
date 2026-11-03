//! Borrowed native-event prefix of an ACTUAL rooted F1 row. This is not a
//! frame/carriage decoder or an admission/credit capability. Callers retain
//! their authenticated state lease and apply their own owner/birth semantics.
use super::*;
use crate::palw_claim_local_v1::{
    PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalPolicyContextV1 as Policy, PalwClaimLocalRowOwnerV1 as Owner,
};
use crate::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, reserve_native_requests_v1};
use borsh::BorshDeserialize;
#[derive(Clone, Copy)]
pub(super) struct NativeEventBlobV10<'a> {
    pub bytes: &'a [u8],
    pub offset: u64,
}
#[derive(Clone, Copy)]
pub(super) struct NativeEventRefV10<'a> {
    pub owner: Owner,
    pub table: u8,
    pub key: NativeEventBlobV10<'a>,
    pub old: Option<NativeEventBlobV10<'a>>,
    pub new: Option<NativeEventBlobV10<'a>>,
}
fn bad(text: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(text.into())
}
fn wire_error(e: std::io::Error) -> PalwStateV2Error {
    // Same classification as the carrier native-prefix reader being factored.
    match e.kind() {
        std::io::ErrorKind::OutOfMemory | std::io::ErrorKind::TimedOut => PalwStateV2Error::NativeResource(
            crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Policy>("borrowed native frame prefix", &e.to_string()),
        ),
        _ => bad("borrowed native frame prefix codec"),
    }
}
fn prefix(raw: &[u8]) -> Result<(PalwBlockContextV2, NativeWireReaderV1<&[u8]>, u64), PalwStateV2Error> {
    let mut reader = NativeWireReaderV1::new(raw, raw.len() as u64);
    let point = PalwBlockContextV2::deserialize_reader(&mut reader).map_err(wire_error)?;
    reader.skip(64).map_err(wire_error)?; // original network + fired-fork digests
    Policy::scan_native_wire_v1(&mut reader).map_err(wire_error)?;
    BTreeSet::<Hash64>::scan_native_wire_v1(&mut reader).map_err(wire_error)?;
    let count = reader.word().map_err(wire_error)? as u64;
    Ok((point, reader, count))
}
fn blob<'a>(raw: &'a [u8], reader: &mut NativeWireReaderV1<&'a [u8]>) -> Result<NativeEventBlobV10<'a>, PalwStateV2Error> {
    let len = reader.word().map_err(wire_error)? as u64;
    let offset = (raw.len() as u64).checked_sub(reader.remaining()).ok_or_else(|| bad("borrowed event offset"))?;
    reader.skip(len).map_err(wire_error)?;
    let end = offset.checked_add(len).ok_or_else(|| bad("borrowed event range overflow"))?;
    let start = usize::try_from(offset).map_err(|_| bad("borrowed event offset overflow"))?;
    let end = usize::try_from(end).map_err(|_| bad("borrowed event length overflow"))?;
    let bytes = raw.get(start..end).ok_or_else(|| bad("borrowed event body range"))?;
    Ok(NativeEventBlobV10 { bytes, offset })
}
fn optional<'a>(raw: &'a [u8], reader: &mut NativeWireReaderV1<&'a [u8]>) -> Result<Option<NativeEventBlobV10<'a>>, PalwStateV2Error> {
    match reader.byte().map_err(wire_error)? {
        0 => Ok(None),
        1 => blob(raw, reader).map(Some),
        _ => Err(bad("borrowed native event option tag")),
    }
}
fn scan<'a>(
    raw: &'a [u8],
    expected_block: Hash64,
    mut visit: impl FnMut(PalwBlockContextV2, u64, NativeEventRefV10<'a>) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let (point, mut reader, count) = prefix(raw)?;
    if point.block != expected_block {
        return Err(bad("rooted native frame key differs from actual point"));
    }
    for ordinal in 0..count {
        let owner = match reader.byte().map_err(wire_error)? {
            0 => Owner::KernelRoute,
            1 => Owner::ExecV2,
            2 => Owner::ChainClaims,
            _ => return Err(bad("borrowed native event owner tag")),
        };
        let table = reader.byte().map_err(wire_error)?;
        let key = blob(raw, &mut reader)?;
        let old = optional(raw, &mut reader)?;
        let new = optional(raw, &mut reader)?;
        visit(point, ordinal, NativeEventRefV10 { owner, table, key, old, new })?;
    }
    // Intentionally stop at the end of the native journal. In particular do
    // not decode, allocate, copy, or project either embedded state carriage.
    Ok(())
}
/// Only native state is accepted, never producer-supplied frame bytes. The
/// surrounding state/checkpoint restore boundary remains responsible for full
/// frame integrity; this scoped reader does not replace that validation.
/// Callback errors stop before the next event is parsed or hydrated.
pub(super) fn visit_rooted_native_events_v10<'a>(
    state: &'a PalwChainStateV2,
    block: Hash64,
    visit: impl FnMut(PalwBlockContextV2, u64, NativeEventRefV10<'a>) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    let Some(route) = &state.kernel_route else {
        return Ok(false);
    };
    let mut key = [0u8; 65];
    key[0] = b'F';
    key[1..].copy_from_slice(block.as_byte_slice());
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let Some(raw) = route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())) else {
        return Ok(false);
    };
    scan(raw, block, visit)?;
    Ok(true)
}
/// Scoped count for a real rooted F1 row, without decoding a carriage.
pub(super) fn rooted_native_event_count_v10(state: &PalwChainStateV2, block: Hash64) -> Result<Option<u64>, PalwStateV2Error> {
    let Some(route) = &state.kernel_route else {
        return Ok(None);
    };
    let mut key = [0u8; 65];
    key[0] = b'F';
    key[1..].copy_from_slice(block.as_byte_slice());
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let Some(raw) = route.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec())) else {
        return Ok(None);
    };
    let (point, _, count) = prefix(raw)?;
    if point.block != block {
        return Err(bad("rooted native count key differs from actual point"));
    }
    Ok(Some(count))
}
/// Count only (not authority): global restore/index preparation already owns
/// the actual native F1 row. Counting does not expose events or a full context.
pub(super) fn native_event_count_v10(raw: &[u8]) -> Result<u64, PalwStateV2Error> {
    Ok(prefix(raw)?.2)
}
#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn prefix_fixture() -> Vec<u8> {
        let p = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap();
        let policy = Policy::from_rules_v1(&p, 2);
        let point = PalwBlockContextV2 { block: h(2), daa_score: 2, blue_score: 3, subsidy: 4 };
        let changes = vec![
            crate::palw_claim_local_v1::PalwClaimLocalNativeChangeV1 {
                owner: Owner::ChainClaims,
                table: 0,
                key: vec![1; 64],
                old: None,
                new: Some(vec![7, 8, 9]),
            },
            crate::palw_claim_local_v1::PalwClaimLocalNativeChangeV1 {
                owner: Owner::ExecV2,
                table: 1,
                key: vec![2; 64],
                old: Some(vec![3]),
                new: None,
            },
        ];
        let mut raw = borsh::to_vec(&(point, [0u8; 32], [0u8; 32], policy, BTreeSet::<Hash64>::new(), changes)).unwrap();
        // Deliberately NOT a full frame. Private parser test proves no tail
        // access; no production API treats these fixture bytes as authority.
        0xffff_ffffu32.serialize(&mut raw).unwrap();
        raw
    }
    #[test]
    fn native_prefix_borrows_exact_offsets_and_stops_before_carriage() {
        let raw = prefix_fixture();
        let mut calls = 0;
        scan(&raw, h(2), |point, ordinal, event| {
            assert_eq!(point.blue_score, 3);
            assert_eq!(ordinal, calls);
            calls += 1;
            for b in [Some(event.key), event.old, event.new].into_iter().flatten() {
                assert_eq!(&raw[b.offset as usize..b.offset as usize + b.bytes.len()], b.bytes)
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(native_event_count_v10(&raw).unwrap(), 2);
        assert!(scan(&raw, h(9), |_, _, _| panic!("wrong native frame key must fail before callback")).is_err());
    }
    #[test]
    fn native_prefix_callback_refusal_prevents_next_event_parse() {
        let raw = prefix_fixture();
        let mut calls = 0;
        assert!(
            scan(&raw, h(2), |_, _, _| {
                calls += 1;
                Err(bad("stop borrowed native visitor"))
            })
            .is_err()
        );
        assert_eq!(calls, 1);
    }
}
