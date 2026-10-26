//! Borrowed held-StateChunk authentication, admitted before hashing/layout.
//! No disclosure copy or computation approval is performed here.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, reserve_native_requests_v1};
use std::mem::size_of;
const PATH: &str = "held StateChunk answer authentication workspace";
type Error = NativeDecodeErrorV1<PalwHeldDaError>;
fn overflow() -> Error {
    Error::Resource(NativeCoverageV1::overflow::<PalwHeldDisclosureV1>(PATH))
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_mul(b).ok_or_else(overflow)
}
/// Old + replacement buffer upper envelope, pinned to the actual writer's
/// initial capacity. Conservative across sequential roles, not RSS/minimum.
fn writer(initial: u64, span: u64) -> Result<u64, Error> {
    if span <= initial {
        return Ok(initial);
    }
    mul(span.checked_next_power_of_two().ok_or_else(overflow)?, 2)
}
fn layers(n: u64) -> Result<u64, Error> {
    if n == 0 {
        return Ok(0);
    }
    let cap = n.max(4).checked_next_power_of_two().ok_or_else(overflow)?;
    mul(if cap > 4 { add(cap, cap / 2)? } else { cap }, size_of::<u16>() as u64)
}
/// A request carries only borrowed inputs/scalars; construction does not hash,
/// build layout vectors, copy a disclosure, or validate a different algorithm.
pub struct PalwHeldDaStateChunkAuthRequestV2<'a> {
    root: &'a Hash64,
    missing: &'a PalwHeldMissingV1,
    binding: &'a PalwStepBindingV2,
    disclosure: &'a PalwHeldDisclosureV1,
    ladder: u64,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    bytes: u64,
}
impl PalwHeldDaStateChunkAuthRequestV2<'_> {
    pub fn required_bytes_v2(&self) -> u64 {
        self.bytes
    }
    pub fn check_v2(self) -> Result<(), Error> {
        let _workspace = reserve_native_requests_v1(self.bytes).map_err(Error::Resource)?;
        // The entire existing semantic order and all canonical hash functions
        // remain authoritative, once, after admission. Their temporaries die
        // before this ticket; an owned output copy needs its independent owner.
        palw_held_da_check_disclosure_v1(self.root, self.missing, self.binding, self.disclosure, self.ladder, self.form)
            .map_err(Error::Decode)
    }
}
pub fn palw_held_da_state_chunk_auth_request_v2<'a>(
    root: &'a Hash64,
    missing: &'a PalwHeldMissingV1,
    binding: &'a PalwStepBindingV2,
    disclosure: &'a PalwHeldDisclosureV1,
    ladder: u64,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwHeldDaStateChunkAuthRequestV2<'a>, Error> {
    if !matches!(missing, PalwHeldMissingV1::StateChunk { .. })
        || !crate::palw_state_chunk_map::palw_profile_is_held_v4(&binding.shape_profile)
    {
        return Err(Error::Resource(NativeCoverageV1::unsupported::<PalwHeldDisclosureV1>(
            PATH,
            "native authentication workspace covers held-map StateChunk demands",
        )));
    }
    let h = size_of::<Hash64>() as u64;
    let u32b = size_of::<u32>() as u64;
    let u16b = size_of::<u16>() as u64;
    // context_hash: u16 + network's u32 length + eleven Hash64 + fixed32
    // execution seed + three u32. This counts the real CanonicalWriter input.
    let context = add(add(add(u16b, u32b)?, add(mul(11, h)?, 32)?)?, mul(3, u32b)?)?;
    let context = add(context, binding.job_context.network_id.len() as u64)?;
    let profile = borsh::object_length(&binding.shape_profile).map_err(|_| {
        Error::Resource(NativeCoverageV1::unsupported::<PalwStepBindingV2>(PATH, "cannot measure profile canonical bytes"))
    })? as u64;
    // verify_binding executes twice in the old checker. These roles are
    // sequential; summing both is safe over-admission, never a minimum claim.
    let mut identity = add(writer(512, context)?, writer(1024, profile)?)?;
    identity = add(identity, writer(256, add(add(u16b, u32b)?, h)?)?)?; // checkpoint profile
    identity = add(identity, writer(256, add(add(u16b, mul(3, h)?)?, 8)?)?)?; // step leg
    identity = add(identity, writer(256, add(add(u16b, mul(4, h)?)?, mul(2, u32b)?)?)?)?; // checkpoint leg
    identity = add(identity, writer(256, add(u16b, mul(5, h)?)?)?)?; // execution commitment
    let mut bytes = mul(identity, 2)?;
    // The native held constructors allocate these actual layer lists. They
    // are used one layout at a time by accusation/count/leaf/membership. Count
    // recurrence conservatively even at an attention-only checkpoint.
    let p = &binding.shape_profile;
    let a = crate::palw_state_chunk_map::held_state_layer_indices_v4(p, crate::palw_step::PalwLayerKindV1::Attention).count() as u64;
    let g =
        crate::palw_state_chunk_map::held_state_layer_indices_v4(p, crate::palw_step::PalwLayerKindV1::GatedDeltaNet).count() as u64;
    bytes = add(bytes, add(layers(a)?, layers(g)?)?)?;
    if let PalwHeldDisclosureV1::StateChunk { chunk, .. } = disclosure {
        // checkpoint_leaf_hash_v2: u16 + five Hash64 + three u32.
        bytes = add(bytes, writer(256, add(add(u16b, mul(5, h)?)?, mul(3, u32b)?)?)?)?;
        bytes = add(bytes, writer(256, add(8, h)?)?)?; // checkpoint step Merkle leaf
        // held chunk leaf: map Hash64, slice/block u32, bytes' u32 length.
        let span = add(add(h, mul(3, u32b)?)?, chunk.chunk_bytes.len() as u64)?;
        bytes = add(bytes, writer(256, span)?)?;
        bytes = add(bytes, writer(256, add(mul(2, u32b)?, h)?)?)?; // slice top leaf
        // The two membership paths fold borrowed siblings/scalars; no Vec copy.
    }
    Ok(PalwHeldDaStateChunkAuthRequestV2 { root, missing, binding, disclosure, ladder, form, bytes })
}

#[cfg(test)]
#[path = "palw_held_da_auth_workspace_tests_v2.rs"]
mod tests;
