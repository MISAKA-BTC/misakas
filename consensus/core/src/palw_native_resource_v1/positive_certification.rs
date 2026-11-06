//! Bounded prefix-P schema appended to aux136; old S/K/V/N/F/H unchanged.
use super::wire::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, wire_bad, wire_struct};
use super::{NativeCoverageV1, NativeHeapV1, native_enum, native_struct};
use crate::palw_positive_certification_v2::*;
use crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1;
use borsh::BorshDeserialize;
use misaka_palw_positive_contract::{assignment::AssignmentId, certification::*, identity::*, rows::*};
use std::{
    io::{self, Read},
    mem::size_of,
};
native_struct!(PositiveCertificationHeaderV2;version,revision,assignment,subject,claim,result,case,scope,policy,policy_id,commit_origin,committed_span,worker,worker_operator,worker_key,worker_gain,liability);
wire_struct!(PositiveCertificationHeaderV2;u16,u16,AssignmentId,Digest,ClaimId,ResultId,CaseId,ChainScope,PalwSecurityCertificationPolicyV1,Digest,AcceptedOrigin,Option<u64>,NativeOutpoint,Digest,[u8;PUBKEY_BYTES],u64,AcceptedMaterial);
native_struct!(PositiveCertificationFinalV2;origin,witness,material);
wire_struct!(PositiveCertificationFinalV2;AcceptedOrigin,WitnessId,AcceptedMaterial);
native_struct!(PositiveCertificationFixedV2;header,finality,outcome,liability_release);
wire_struct!(PositiveCertificationFixedV2;PositiveCertificationHeaderV2,Option<PositiveCertificationFinalV2>,Option<(PositiveCertificationOutcomeV2,u64)>,Option<PositiveCertificationLiabilityReleaseV2>);
native_struct!(PositiveCertificationDrawOriginV2;block,daa,blue_score);
wire_struct!(PositiveCertificationDrawOriginV2;NativeHash64,u64,u64);
native_struct!(PositiveCertificationLiabilityReleaseV2;point,closed_material,latest_decision_daa);
wire_struct!(PositiveCertificationLiabilityReleaseV2;PositiveCertificationDrawOriginV2,AcceptedMaterial,u64);
native_struct!(PositiveCertificationDrawMetaV2;roster,roster_count,roster_stake,seed,anchor_span,anchor_block,execution_key,origin,deadline_daa,id);
wire_struct!(PositiveCertificationDrawMetaV2;Digest,u64,u128,Digest,u64,NativeHash64,NativeHash64,PositiveCertificationDrawOriginV2,u64,Digest);
native_struct!(PositiveCertificationDrawV2;meta,positions);
native_struct!(PositiveCertificationVoteV2;bond,key,statement,signature,origin,liability_amount);
wire_struct!(PositiveCertificationVoteV2;NativeOutpoint,[u8;PUBKEY_BYTES],PositiveCourtAttestationV2,[u8;SIGNATURE_BYTES],AcceptedOrigin,u64);
native_struct!(PositiveCertificationRowV2;fixed,draw,votes);
native_enum!(PositiveCertificationOutcomeV2;Certified,NoPopulation,Expired,NotCertified,Vetoed,Withdrawn);
impl NativeWireScanV1 for PositiveCertificationOutcomeV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        r.step()?;
        if r.byte()? > 5 {
            return Err(wire_bad("unknown P outcome"));
        }
        Ok(NativeWireShapeV1::default())
    }
}
// Existing common statement types currently have no native table owner.
native_enum!(PositiveCourtVerdictV2;PositiveChecked,Unavailable,NotReproducible);
impl NativeWireScanV1 for PositiveCourtVerdictV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        r.step()?;
        if r.byte()? > 2 {
            return Err(wire_bad("unknown P verdict"));
        }
        Ok(NativeWireShapeV1::default())
    }
}
native_struct!(PositiveCourtAttestationV2;version,revision,scope,claim,result,case,subject,policy,draw_anchor,witness,verifier_bond,verifier,verdict,checked_material);
wire_struct!(PositiveCourtAttestationV2;u16,u16,ChainScope,ClaimId,ResultId,CaseId,Digest,Digest,Digest,WitnessId,Digest,PrincipalId,PositiveCourtVerdictV2,Digest);
fn bounded_count<R: Read>(r: &mut NativeWireReaderV1<R>, maximum: u16) -> io::Result<u64> {
    let n = u64::from(r.word()?);
    if n > u64::from(maximum) {
        return Err(wire_bad("P vector exceeds original actual m/75 codec ceiling"));
    }
    Ok(n)
}
impl NativeWireScanV1 for PositiveCertificationDrawV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        PositiveCertificationDrawMetaV2::scan_native_wire_v1(r)?;
        let n = bounded_count(r, MAX_POSITIVE_DRAW_POSITIONS_V2)?;
        r.skip(n * 64)?;
        NativeWireShapeV1 { collection_entries: n, outer_entries: n, ..Default::default() }.allocation(n * 2, size_of::<Digest>())
    }
}
fn scan<R: Read>(key: Option<&[u8]>, r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
    if r.remaining() > MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2 as u64 {
        return Err(wire_bad("P row exceeds fixed codec ceiling"));
    }
    r.step()?;
    // All prefix fields are fixed/Option<fixed>, with exhaustive visitors above.
    // No Vec Deserialize occurs before its actual m/75 count check.
    let fixed = PositiveCertificationFixedV2::deserialize_reader(&mut *r).map_err(|_| wire_bad("P fixed prefix malformed"))?;
    fixed.header.validate().map_err(|_| wire_bad("P original policy/version invalid"))?;
    if key.is_some_and(|key| key.len() != 65 || key[0] != b'P' || key[1..] != fixed.header.subject.0) {
        return Err(wire_bad("P key/subject mismatch"));
    }
    let mut shape = NativeWireShapeV1::default();
    match r.byte()? {
        0 => {}
        1 => {
            PositiveCertificationDrawMetaV2::scan_native_wire_v1(r)?;
            let n = bounded_count(r, fixed.header.policy.m)?;
            r.skip(n * 64)?;
            shape = shape.allocation(n * 2, size_of::<Digest>())?;
            shape.collection_entries = n;
        }
        _ => return Err(wire_bad("P draw option malformed")),
    }
    let n = bounded_count(r, fixed.header.policy.m)?;
    for _ in 0..n {
        shape = shape.merge(PositiveCertificationVoteV2::scan_native_wire_v1(r)?)?;
    }
    shape.collection_entries = shape.collection_entries.checked_add(n).ok_or_else(|| wire_bad("P membership overflow"))?;
    shape.outer_entries = n;
    shape
        .allocation(n * 2, size_of::<PositiveCertificationVoteV2>())?
        .allocation(2, size_of::<PositiveCertificationFixedV2>())?
        .allocation(2, size_of::<PositiveCertificationVoteV2>())
}
impl NativeWireScanV1 for PositiveCertificationRowV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        scan(None, r)
    }
}
pub(super) fn scan_positive_certification_row_v2<R: Read>(key: &[u8], r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
    scan(Some(key), r)
}
