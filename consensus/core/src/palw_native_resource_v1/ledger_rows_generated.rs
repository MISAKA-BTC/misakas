// Generated actual ledger-table row dispatch.
fn scan_ledger_row_v1<R: Read>(table: u8, r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
    match table {
        0 => <([u8; 64], misaka_palw_kernel::flat_candidate7::FlatCandidateRecordV7) as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(
                1,
                size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ([u8; 64], misaka_palw_kernel::flat_candidate7::FlatCandidateRecordV7)>>
                    > (),
            ),
        1 => <misaka_palw_kernel::ledger::BondRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::BondRowV1>> > ()),
        2 => <misaka_palw_kernel::state::ClassRecordV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::ClassRowV1>> > ()),
        3 => <misaka_palw_kernel::state::PipelineClassRecordV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::PipelineClassRowV1>> > ()),
        4 => <misaka_palw_kernel::job::KernelJobV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::job::KernelJobV1>> > ()),
        5 => <misaka_palw_kernel::pipeline_public::PipelineJobPostV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::pipeline_public::PipelineJobPostV1>> > (),
        ),
        6 => <misaka_palw_kernel::ledger::ClaimRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::ClaimRowV1>> > ()),
        7 => <misaka_palw_kernel::ledger::DemandRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<misaka_palw_kernel::ledger::DemandKeyV1, misaka_palw_kernel::ledger::DemandRowV1>>
                > (),
        ),
        8 => {
            <misaka_palw_kernel::public::ServedPositionV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
                1,
                size_of::<
                    typed::BTreeNodeEnvelopeV1<misaka_palw_kernel::ledger::DemandKeyV1, misaka_palw_kernel::public::ServedPositionV1>,
                > > (),
            )
        }
        9 => <() as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>> > ()),
        10 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], [u8; 64]>> > ()),
        11 => <misaka_palw_kernel::ledger::SealRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<([u8; 64], [u8; 64]), misaka_palw_kernel::ledger::SealRowV1>> > ()),
        12 => {
            <() as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>> > ())
        }
        13 => {
            <() as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ()>> > ())
        }
        14 => <misaka_palw_kernel::opv::OpvClaimRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::opv::OpvClaimRowV1>> > ()),
        15 => <misaka_palw_kernel::ledger::SealRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<([u8; 64], [u8; 64]), misaka_palw_kernel::ledger::SealRowV1>> > ()),
        16 => <misaka_palw_kernel::ledger::JobEscrowRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::JobEscrowRowV1>> > ()),
        17 => <Vec<([u8; 64], u64)> as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<misaka_palw_kernel::ledger::DemandKeyV1, Vec<([u8; 64], u64)>>> > ()),
        18 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], [u8; 64]>> > ()),
        19 => <([u8; 64], misaka_palw_kernel::flat_candidate9::FlatCandidateRecordV9) as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(
                1,
                size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ([u8; 64], misaka_palw_kernel::flat_candidate9::FlatCandidateRecordV9)>>
                    > (),
            ),
        20 => <misaka_palw_kernel::seg::TiledJobRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::seg::TiledJobRowV1>> > ()),
        21 => <misaka_palw_kernel::seg_da::SegProgressV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<misaka_palw_kernel::ledger::DemandKeyV1, misaka_palw_kernel::seg_da::SegProgressV1>>
                > (),
        ),
        22 => <misaka_palw_kernel::spec::ComputationSpecV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::spec::SpecClassRowV1>> > ()),
        23 => <misaka_palw_kernel::spec::SpecJobV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::spec::SpecJobV1>> > ()),
        24 => <misaka_palw_kernel::spec::memory::MemoryLineV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::spec::memory::MemoryLineV1>> > ()),
        25 => <[u8; 64] as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], [u8; 64]>> > ()),
        26 => <misaka_palw_kernel::ledger::ForfeitedSealRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<([u8; 64], [u8; 64], u64), misaka_palw_kernel::ledger::ForfeitedSealRowV1>> > (),
        ),
        27 => <misaka_palw_kernel::verifier_pay::VerifierPayRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<(u8, [u8; 64]), misaka_palw_kernel::verifier_pay::VerifierPayRowV1>> > (),
        ),
        28 => <misaka_palw_kernel::court_scope::CourtScopeRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::court_scope::CourtScopeRowV1>> > ()),
        29 => <([u8; 64], misaka_palw_kernel::state::ClassRecordV1) as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], ([u8; 64], misaka_palw_kernel::ledger::ClassRowV1)>> > ()),
        30 => <misaka_palw_kernel::beacon_source::BeaconSourceRowV1 as NativeWireScanV1>::scan_native_wire_v1(r)?.allocation(
            1,
            size_of::<typed::BTreeNodeEnvelopeV1<([u8; 64], [u8; 64]), misaka_palw_kernel::beacon_source::BeaconSourceRowV1>> > (),
        ),
        31 => <misaka_palw_kernel::exec_reward::ExecRewardHoldV1 as NativeWireScanV1>::scan_native_wire_v1(r)?
            .allocation(1, size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::exec_reward::ExecRewardHoldV1>> > ()),
        _ => Err(wire::wire_bad("unknown persisted ledger table")),
    }
}
