//! Exact typed checkpoint coverage. These are allocation and byte-shape
//! visitors; semantic authority comes from the executed liability fold.
use crate::palw_claim_local_v1::{
    PalwClaimLiabilityFrameV1, PalwClaimLocalCheckpointV1, PalwClaimLocalInputV1, PalwClaimLocalNativeChangeV1,
    PalwClaimLocalRecordV1, PalwClaimLocalRewardHoldV1, PalwClaimLocalRowOwnerV1,
};
use crate::palw_native_resource_v1::wire::wire_struct;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, preflight_native_carriage_v1,
};
use crate::palw_native_resource_v1::{native_enum, native_struct};
use crate::{Hash64, palw_state_v2::PalwBlockContextV2};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

native_struct!(PalwClaimLocalRewardHoldV1; accepted_daa, accepted_blue_score, until);
wire_struct!(PalwClaimLocalRewardHoldV1; u64, u64, u64);
native_struct!(PalwClaimLocalRecordV1; claim, accepted_daa, accepted_blue_score, accepting_block, anchors, inputs,
    execution_root, reservations, final_claim, certified, voided, liability_closed, material);
native_struct!(PalwClaimLocalCheckpointV1; point, records, liability_state, material);
native_enum!(PalwClaimLocalRowOwnerV1; KernelRoute, ExecV2, ChainClaims);
impl NativeWireScanV1 for PalwClaimLocalRowOwnerV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        match reader.byte()? {
            0 | 1 | 2 => Ok(NativeWireShapeV1::default()),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown liability row owner")),
        }
    }
}
native_struct!(PalwClaimLocalNativeChangeV1; owner, table, key, old, new);
wire_struct!(PalwClaimLocalNativeChangeV1; PalwClaimLocalRowOwnerV1, u8, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);
wire_struct!(PalwClaimLocalRecordV1; Hash64, u64, u64, Hash64, bool, BTreeSet<Hash64>, Hash64, BTreeSet<Hash64>,
    bool, bool, bool, bool, BTreeSet<Hash64>);
wire_struct!(PalwClaimLocalCheckpointV1; PalwBlockContextV2, BTreeMap<Hash64, PalwClaimLocalRecordV1>, Vec<u8>, BTreeMap<Hash64, Vec<u8>>);

native_struct!(PalwClaimLiabilityFrameV1; point, network_identity, fired_forks, policy, claims, native_changes, prior_state, current_state, accepted_objects, origin_headers, inputs);
native_enum!(PalwClaimLocalInputV1;
    Attempt { block, execution_key, envelope }, ReceiptSpend { block, spend },
    History { claim, pledge }, PublicBinding { claim, header }, CustodyDelegation { claim, delegation }, TirOriginal { claim, envelope }, FreePromptNative { claim, job_pin, public_job, eval });

impl NativeWireScanV1 for PalwClaimLocalInputV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        let tag = reader.byte()?;
        if tag > 6 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown liability input kind"));
        }
        let mut sum = <Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
        let payload = match tag {
            0 => {
                sum = sum.merge(<Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                <Box<crate::palw_attempt_v2::PalwAttemptEnvelopeV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?
            }
            1 => <Box<crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
            2 => <Box<crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?,
            3 => <Box<crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1> as NativeWireScanV1>::scan_native_wire_v1(
                reader,
            )?,
            4 => <Box<crate::palw_history_custody_v1::PalwProducerCustodyDelegationV1> as NativeWireScanV1>::scan_native_wire_v1(
                reader,
            )?,
            5 => <Box<crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1> as NativeWireScanV1>::scan_native_wire_v1(
                reader,
            )?,
            6 => {
                sum = sum.merge(<Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
                sum = sum.merge(
                    <Option<Box<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>> as NativeWireScanV1>::scan_native_wire_v1(
                        reader,
                    )?,
                )?;
                <Option<Box<(crate::palw_improve_eval_v1::PalwEvalJobV1, crate::palw_improve_eval_v1::PalwEvalClaimTailV1)>> as NativeWireScanV1>::scan_native_wire_v1(reader)?
            }
            _ => unreachable!(),
        };
        sum.merge(payload)
    }
}

thread_local! { static SCANNING_FRAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) fn frame_scan_active_v1() -> bool {
    SCANNING_FRAME.get()
}
struct FrameScanGuard;
impl Drop for FrameScanGuard {
    fn drop(&mut self) {
        SCANNING_FRAME.set(false);
    }
}

impl NativeWireScanV1 for PalwClaimLiabilityFrameV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        if SCANNING_FRAME.replace(true) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "a liability frame cannot contain another frame"));
        }
        let _guard = FrameScanGuard;
        let mut sum = <PalwBlockContextV2 as NativeWireScanV1>::scan_native_wire_v1(reader)?;
        sum = sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum =
            sum.merge(<crate::palw_claim_local_v1::PalwClaimLocalPolicyContextV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(<BTreeSet<Hash64> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(<Vec<PalwClaimLocalNativeChangeV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        // Each raw state frame has its own exact byte boundary. Scan the real
        // carriage, not just Vec<u8>, before any embedded Borsh allocation.
        for _ in 0..2 {
            let bytes = u64::from(reader.word()?);
            if bytes == 0 || bytes > reader.remaining() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "liability state exceeds its frame"));
            }
            // Erase the nested reader type: otherwise the generic carriage ->
            // aux -> frame dispatch instantiates infinitely nested Take<R>.
            let mut bounded = (&mut *reader).take(bytes);
            let nested = preflight_native_carriage_v1(&mut bounded as &mut dyn Read, bytes)?;
            let memory = nested.memory_requests_v1()?;
            let demand = bytes
                .checked_mul(2)
                .and_then(|n| n.checked_add(memory))
                .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "liability frame demand overflow"))?;
            sum = sum.merge(NativeWireShapeV1::default().allocation(demand, 1)?)?;
        }
        sum = sum.merge(<Vec<crate::palw_state_v2::PalwConsensusObjectV2> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum = sum.merge(<BTreeMap<Hash64, Vec<u8>> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
        sum.merge(<Vec<PalwClaimLocalInputV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)
    }
}
