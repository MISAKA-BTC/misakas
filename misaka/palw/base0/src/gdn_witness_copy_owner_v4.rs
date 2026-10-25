//! Admission for the accused checkpoint witness's actual
//! second copy from held disclosures. This is not a whole-refutation envelope.
use kaspa_consensus_core::{
    palw_attn_court_v1::PalwAttnCheckpointAnchorV1,
    palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeHeapV1, reserve_native_requests_v1},
    palw_step_leg::PalwStepOpeningV1,
    palw_step_refute::memory_v2::{
        PalwGdnCheckpointWitnessV2, PalwGdnStateChunkWitnessV2, gdn_checkpoint_witness_from_disclosures_v2,
    },
};
use kaspa_hashes::Hash64;
use std::{mem::size_of, ops::Deref};

type Error = NativeDecodeErrorV1<String>;
const PATH: &str = "GDN held accused-witness copy";
fn invalid(why: &str) -> Error {
    Error::Decode(why.into())
}
fn overflow<T>() -> Error {
    Error::Resource(NativeCoverageV1::overflow::<T>(PATH))
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(overflow::<PalwGdnCheckpointWitnessV2>)
}
fn slots<T>(n: usize) -> Result<u64, Error> {
    u64::try_from(n).ok().and_then(|n| n.checked_mul(size_of::<T>() as u64)).ok_or_else(overflow::<T>)
}
fn native_heap<T: NativeHeapV1>(value: &T) -> Result<u64, Error> {
    let request = value.native_heap_v1(PATH);
    request.bytes().ok_or_else(|| Error::Resource(request))
}

/// Guard only for the accused witness. The consumer that transfers its witness
/// into a refutation must keep this guard through the court and output-ticket
/// acquisition. It must not call the other refutation roles reserved by this.
pub struct Base0GdnWitnessCopyOwnerV4 {
    _lease: Option<NativeDemandLeaseV1>,
    conv_owner: Option<kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnConvOwnerV3>,
    predecessor_owner: Option<kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnPredecessorOwnerV3>,
}

/// Field order retires the copied witness before its request. No implicit clone
/// or mutable dereference is provided. Explicit parts handoff is needed because
/// the native builder consumes the witness into its returned refutation.
pub struct Base0GdnRetainedCheckpointWitnessV4 {
    witness: PalwGdnCheckpointWitnessV2,
    owner: Base0GdnWitnessCopyOwnerV4,
}
impl Deref for Base0GdnRetainedCheckpointWitnessV4 {
    type Target = PalwGdnCheckpointWitnessV2;
    fn deref(&self) -> &Self::Target {
        &self.witness
    }
}
impl Base0GdnRetainedCheckpointWitnessV4 {
    /// The native builder consumes the witness into T. On refusal its consumed
    /// witness retires before this caller's guard; on success T carries the same
    /// guard. No freely separable raw value/ticket pair is exposed.
    pub fn try_build_v4<T>(
        self,
        build: impl FnOnce(PalwGdnCheckpointWitnessV2) -> Result<T, Error>,
    ) -> Result<Base0GdnWitnessBuiltV4<T>, Error> {
        let Self { witness, owner } = self;
        let value = build(witness)?;
        Ok(Base0GdnWitnessBuiltV4 { value, owner })
    }
}

/// Only the witness-copy role inside T is admitted. T's other input/row/path
/// allocations require their own coverage. Value-before-owner field order also
/// applies when the result is simply dropped after a semantic refusal.
pub struct Base0GdnWitnessBuiltV4<T> {
    value: T,
    owner: Base0GdnWitnessCopyOwnerV4,
}
impl<T> Deref for Base0GdnWitnessBuiltV4<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<T> Base0GdnWitnessBuiltV4<T> {
    pub(super) fn with_predecessor_owner_v3(mut self,
        owner: Option<kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnPredecessorOwnerV3>,
    ) -> Self {
        self.owner.predecessor_owner = owner;
        self
    }
    pub(super) fn with_conv_owner_v5(mut self, owner: kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnConvOwnerV3) -> Self {
        self.owner.conv_owner = Some(owner);
        self
    }
    /// Move the same witness-bearing value into a carrier without cloning it.
    pub fn map_v4<U>(self, map: impl FnOnce(T) -> U) -> Base0GdnWitnessBuiltV4<U> {
        let Self { value, owner } = self;
        Base0GdnWitnessBuiltV4 { value: map(value), owner }
    }
    /// Keeps this input guard live while the callback runs. R is unconstrained:
    /// this generic API does not itself guarantee retained output ownership.
    /// The actual node callback must check the court and admit its output before
    /// returning; that concrete caller owns the destination-ticket obligation.
    pub fn try_handoff_v4<R, E>(self, handoff: impl FnOnce(T) -> Result<R, E>) -> Result<R, E> {
        let Self { value, owner } = self;
        let result = handoff(value);
        drop(owner);
        result
    }
}

/// Allocation-free scan of actual borrowed native values. Exhaustive opening
/// destructuring makes a future field addition require review of this request.
/// Unknown leaf heap coverage is a typed Resource, never an assumed zero.
pub(crate) fn witness_copy_request_v4(demands: &[PalwHeldMissingV1], answers: &[PalwHeldDisclosureV1]) -> Result<u64, Error> {
    if demands.is_empty() || demands.len() != answers.len() {
        return Err(invalid("one held DA answer per demanded checkpoint chunk"));
    }
    let mut bytes = slots::<PalwGdnStateChunkWitnessV2>(answers.len())?;
    let mut first: Option<&PalwAttnCheckpointAnchorV1> = None;
    for (demand, answer) in demands.iter().zip(answers) {
        let (PalwHeldMissingV1::StateChunk { checkpoint, chunk }, PalwHeldDisclosureV1::StateChunk { anchor, chunk: opening }) =
            (demand, answer)
        else {
            return Err(invalid("a checkpoint witness is built only from state-chunk answers"));
        };
        if anchor.leaf.checkpoint_index != *checkpoint || opening.chunk_index != *chunk {
            return Err(invalid("a held DA answer is of another unit than its demand"));
        }
        match first {
            None => first = Some(anchor),
            Some(previous) if previous == anchor => {}
            Some(_) => return Err(invalid("the answers name different checkpoint leaves")),
        }
        // Existing native Vec::clone destinations request their source lengths;
        // source spare capacity and duplicate anchor paths are not cloned here.
        bytes = add(bytes, slots::<u8>(opening.chunk_bytes.len())?)?;
        bytes = add(bytes, slots::<Hash64>(opening.siblings.len())?)?;
    }
    let anchor = first.ok_or_else(|| invalid("no held DA answer"))?;
    let PalwStepOpeningV1 { leaf_index: _, leaf_hash: _, siblings } = &anchor.opening;
    bytes = add(bytes, slots::<Hash64>(siblings.len())?)?;
    add(bytes, native_heap(&anchor.leaf)?)
}

/// Reserve before the native constructor's outer Vec or any chunk/path clone.
/// Shape checks match its existing borrowed identity requirements; the native
/// implementation remains the authority that creates the witness.
pub fn base0_gdn_checkpoint_witness_retained_v4(
    demands: &[PalwHeldMissingV1],
    answers: &[PalwHeldDisclosureV1],
) -> Result<Base0GdnRetainedCheckpointWitnessV4, Error> {
    let bytes = witness_copy_request_v4(demands, answers)?;
    let owner = Base0GdnWitnessCopyOwnerV4 { _lease: reserve_native_requests_v1(bytes).map_err(Error::Resource)?, conv_owner: None, predecessor_owner: None };
    let witness =
        gdn_checkpoint_witness_from_disclosures_v2(demands, answers).map_err(crate::gdn_evidence_v2::base0_gdn_court_error_v3)?;
    // Validate the actual output capacities against the requested clone roles.
    // NativeHeap on each leaf also preserves unsupported/overflow coverage.
    let PalwGdnCheckpointWitnessV2 { leaf, opening, chunks } = &witness;
    let PalwStepOpeningV1 { leaf_index: _, leaf_hash: _, siblings } = opening;
    let mut actual = add(slots::<PalwGdnStateChunkWitnessV2>(chunks.capacity())?, native_heap(leaf)?)?;
    actual = add(actual, slots::<Hash64>(siblings.capacity())?)?;
    for chunk in chunks {
        let PalwGdnStateChunkWitnessV2 { chunk_index: _, bytes, siblings } = chunk;
        actual = add(actual, slots::<u8>(bytes.capacity())?)?;
        actual = add(actual, slots::<Hash64>(siblings.capacity())?)?;
    }
    if actual > bytes {
        return Err(Error::Resource(NativeCoverageV1::unsupported::<PalwGdnCheckpointWitnessV2>(
            PATH,
            "native witness exceeds its admitted clone capacities",
        )));
    }
    Ok(Base0GdnRetainedCheckpointWitnessV4 { witness, owner })
}
