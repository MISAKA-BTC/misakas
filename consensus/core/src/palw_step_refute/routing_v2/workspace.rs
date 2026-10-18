//! Concrete live row/path/kernel buffers and an allocation-admitting operand
//! oracle. This is a logical request bound, not physical allocator/RSS proof.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, native_value_requests_v1};
use std::cell::RefCell;

pub(super) fn reserve(
    r: &PalwExecutionStepRefutationV1,
    required: &[Vec<(u64, PalwStepCoordinateV1)>],
    node: &crate::palw_step::PalwStepNodeV1,
    router: &crate::palw_step::PalwStepNodeV1,
) -> Result<Option<NativeDemandLeaseV1>, PalwRoutingCourtErrorV2> {
    let b = &r.binding;
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(native_value_requests_v1(b)?)?;
    q.add_v1(native_value_requests_v1(&r.output_opening)?)?;
    q.add_v1(native_value_requests_v1(&r.output_preimage)?)?;
    q.vec_v1::<u8>(
        (borsh::object_length(&b.shape_profile).map_err(|_| evidence("cannot measure routing profile"))? as u64).max(1024),
    )?;
    q.vec_v1::<u8>((754 + b.job_context.network_id.len() as u64).max(1024))?;
    q.vec_v1::<u8>(512)?;
    q.vec_v1::<Vec<u32>>(required.len() as u64)?;
    let depth = 2 * u64::from(64 - b.step_leaf_count.leading_zeros());
    let mut maximum_input = 0u64;
    for row in required {
        let mut lanes = 0u64;
        for (_, c) in row {
            let (src, _) = b.shape_profile.resolve_node_slot(c.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
            lanes = lanes.checked_add(u64::from(src.tile_len)).ok_or(PalwStepRefuteError::Unadjudicable)?;
            q.vec_v1::<u8>((150 + 4 * u64::from(src.tile_len)).max(256))?;
        }
        maximum_input = maximum_input.max(lanes);
        q.vec_v1::<u32>(lanes)?; // membership-verified concatenated row
        q.vec_v1::<i32>(lanes)?; // kernel's narrowed/copied operand row
        q.vec_v1::<Hash64>(row.len() as u64)?;
        q.vec_v1::<(usize, usize)>(row.len() as u64)?;
        q.vec_v1::<Hash64>(row.len() as u64)?; // current range clone
        q.vec_v1::<Hash64>(depth)?;
        q.vec_v1::<u64>(row.len() as u64)?; // Q36 combine covering-tile reconstruction
    }
    let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = node.out_len else {
        return Err(evidence("routed output is not finite"));
    };
    let crate::palw_step::PalwStepOutLenV1::Fixed { elements: route } = router.out_len else {
        return Err(evidence("router output is not finite"));
    };
    let width = u64::from(elements);
    // `read_block_chunks` allocates its gathering capacity BEFORE it asks the
    // oracle for the first covering chunk. Reserve those concrete code,
    // exponent and parameter buffers here; callback admission below then owns
    // the individual served copies and derived decodes. Each chunk is at most
    // this node's tile width, and selected rows + a chunk <= twice that width.
    let covering_rows = TirNativeRequestsV1::product_v1(u64::from(node.tile_len), 2)?;
    q.vec_v1::<u8>(TirNativeRequestsV1::product_v1(covering_rows, maximum_input)?)?;
    q.vec_v1::<u8>(TirNativeRequestsV1::product_v1(
        covering_rows,
        maximum_input.div_ceil(crate::palw_qwen36_ops::QWEN36_WEIGHT_GROUP as u64),
    )?)?;
    q.vec_v1::<u8>(TirNativeRequestsV1::product_v1(covering_rows, crate::palw_base0_a16::A16QuantParams::WIRE_BYTES as u64)?)?;

    q.vec_v1::<i32>(width)?; // result accumulator, or compact expert slices
    q.vec_v1::<i32>(width)?; // per-block result / gated activation
    q.vec_v1::<i32>(width)?; // pure kernel result before u32 conversion
    q.vec_v1::<u32>(width)?; // returned direct-arithmetic row
    q.vec_v1::<usize>(u64::from(route) / 2)?;
    q.vec_v1::<i32>(u64::from(route))?; // routing ids/weights decoded
    q.vec_v1::<crate::palw_kimi_k3_ops::KimiK3RoutedExpert>(u64::from(route) / 2)?;
    q.vec_v1::<(u64, u64)>(u64::from(route) / 2)?;
    q.vec_v1::<u64>(node.input_refs.len() as u64)?;
    // At most the expert decimal u32 id, fixed ".ffn_expert." and the
    // longest ".weight.a16"/"_gated.a16" suffix extend the profile name.
    let name = node.weight_name.len() as u64 + 10 + 12 + 12;
    q.vec_v1::<u8>(name)?; // store base
    q.vec_v1::<u8>(name)?; // exponent / quantization suffix
    q.vec_v1::<u8>(name)?; // currently requested chunk name
    reserve_native_requests_v1(q.bytes_v1()).map_err(Into::into)
}

/// The old oracle returns Option, so record local refusal out-of-band and give
/// it precedence over every arithmetic result. Missing material remains an
/// evidence refusal. Native Resource never becomes guilt or false accusation.
pub(super) struct ReservedOracle<'a> {
    source: &'a dyn PalwWeightOracleV1,
    lease: RefCell<Option<NativeDemandLeaseV1>>,
    refusal: RefCell<Option<NativeCoverageV1>>,
}
impl<'a> ReservedOracle<'a> {
    pub(super) fn new(source: &'a dyn PalwWeightOracleV1) -> Self {
        Self { source, lease: RefCell::new(None), refusal: RefCell::new(None) }
    }
    pub(super) fn take_refusal(&self) -> Option<NativeCoverageV1> {
        self.refusal.borrow_mut().take()
    }
    fn reserve_operand(&self, count: u64) -> Result<(), NativeCoverageV1> {
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<u8>(count)?; // oracle's exact bytes copy
        // Per-expert gather requests covering chunks. Its initial capacity is
        // selected rows + one chunk, <= twice the covering bytes; contributions
        // of every requested chunk remain retained through the whole terminal.
        q.vec_v1::<u8>(TirNativeRequestsV1::product_v1(count, 2)?)?;
        q.vec_v1::<u8>(count)?; // narrowed gathered byte slice
        q.vec_v1::<i8>(count)?; // grouped codes/exponents decode
        q.vec_v1::<crate::palw_base0_a16::A16QuantParams>(count / crate::palw_base0_a16::A16QuantParams::WIRE_BYTES as u64)?;
        let mut lease = self.lease.borrow_mut();
        if let Some(lease) = lease.as_mut() {
            lease.grow_memory_v1(q.bytes_v1())?;
        } else {
            *lease = reserve_native_requests_v1(q.bytes_v1())?;
        }
        Ok(())
    }
}
impl PalwWeightOracleV1 for ReservedOracle<'_> {
    fn operand_bytes(&self, name: &str, layer: Option<u16>, offset: u32, len: u32) -> Option<Vec<u8>> {
        if self.refusal.borrow().is_some() {
            return None;
        }
        if let Err(e) = self.reserve_operand(u64::from(len)) {
            *self.refusal.borrow_mut() = Some(e);
            return None;
        }
        self.source.operand_bytes(name, layer, offset, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_operand_capacity_refuses_before_source_allocation_and_stays_typed() {
        struct Count(std::cell::Cell<usize>);
        impl PalwWeightOracleV1 for Count {
            fn operand_bytes(&self, _: &str, _: Option<u16>, _: u32, n: u32) -> Option<Vec<u8>> {
                self.0.set(self.0.get() + 1);
                Some(vec![0; n as usize])
            }
        }
        let checker = crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
            crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 },
        );
        let source = Count(std::cell::Cell::new(0));
        let oracle = ReservedOracle {
            source: &source,
            lease: RefCell::new(Some(checker.reserve_requests_v1(0, 0).unwrap())),
            refusal: RefCell::new(None),
        };
        assert!(oracle.operand_bytes("actual registered parameter", Some(0), 0, 17).is_none());
        assert_eq!(source.0.get(), 0);
        assert!(oracle.take_refusal().is_some());
    }
}
