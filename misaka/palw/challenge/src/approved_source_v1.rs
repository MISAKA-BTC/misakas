//! A new collector for claim-level bootstrap approvals. V1 PanelIndependent and
//! PanelLicensed IDs are intentionally not reused. Inputs must come from the
//! consumer's own authenticated native history; bytes returned by a remote RPC
//! are observations, not authority. Neither a timeout nor matching replay can
//! create the approval/settlement fields below.
use crate::hash::{Digest, h};
use borsh::{BorshDeserialize, BorshSerialize};

/// Fixed provenance of one native selected-quorum source event. A cohort is a
/// weighted voting population, not independent operators. All works of an epoch
/// share its compromise risk; this collector does not multiply that risk down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedSourceEventV1 {
    pub version: u16,
    pub network: Digest,
    pub bootstrap_epoch: Digest,
    pub bootstrap_policy: Digest,
    pub frozen_population: Digest,
    pub frozen_block: Digest,
    pub anchor: Digest,
    pub anchor_position: u64,
    pub source_profile: Digest,
    pub checker_scope: Digest,
    pub claim: Digest,
    pub canonical_work: Digest,
    pub statement: Digest,
    pub material_scope: Digest,
    pub execution_commitment: Digest,
    pub approval_root: Digest,
    pub producer: Digest,
    pub consumer: Digest,
    pub accepted_position: u64,
    pub final_position: u64,
    pub settlement_position: u64,
    pub approved_slots: u16,
    pub required_slots: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedSourceCollectorPolicyV1 {
    pub version: u16,
    pub network: Digest,
    pub bootstrap_epoch: Digest,
    pub bootstrap_policy: Digest,
    pub source_profile: Digest,
    pub checker_scope: Digest,
    pub required_sources: u32,
    pub maximum_events: u32,
    pub anchor_delay: u64,
    pub window: u64,
    pub settlement_depth: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ApprovedSourceConsumerV1 {
    pub commitment: Digest,
    pub commitment_position: u64,
    pub target_profile: Digest,
    /// Actual native policy/assignment lineage identity of this subject. A
    /// self-asserted empty dependency list cannot supply this identity.
    pub lineage: Digest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovedSourceRefusalV1 {
    Policy,
    Context,
    Capacity,
    Lineage,
    Overflow,
}
#[derive(Debug)]
pub enum ApprovedSourceCollectorErrorV1<E> {
    Contract(ApprovedSourceRefusalV1),
    Capacity(E),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ApprovedSourceStateV1 {
    Pending { found: u32, required: u32 },
    Unavailable { found: u32, required: u32 },
    Locked { context: Digest, root: Digest, output: Digest, sources: u32, locked_at: u64 },
}

/// No allocation, sort or encoded population clone: canonical ordering is
/// selected by bounded scans of authenticated events. Rewrapped work IDs count
/// once and the first native occurrence wins. The explicit event cap bounds the
/// O(n*k+n^2) work; sizing it is part of post-implementation calibration.
/// `depends_on` must resolve the real transitive native profile/seed graph. A
/// source whose checker or anchor depends on this consumer is refused, not
/// re-labelled independent. The older collector's circularity guard stays put.
pub fn collect_approved_sources_v1(
    policy: &ApprovedSourceCollectorPolicyV1,
    consumer: &ApprovedSourceConsumerV1,
    events: &[ApprovedSourceEventV1],
    tip: u64,
    mut depends_on: impl FnMut(&ApprovedSourceEventV1, &ApprovedSourceConsumerV1) -> bool,
) -> Result<ApprovedSourceStateV1, ApprovedSourceRefusalV1> {
    if policy.version != 1
        || policy.required_sources == 0
        || policy.maximum_events < policy.required_sources
        || policy.window == 0
        || [policy.network, policy.bootstrap_epoch, policy.bootstrap_policy, policy.source_profile, policy.checker_scope]
            .contains(&[0; 64])
    {
        return Err(ApprovedSourceRefusalV1::Policy);
    }
    if events.len() > policy.maximum_events as usize {
        return Err(ApprovedSourceRefusalV1::Capacity);
    }
    if [consumer.commitment, consumer.target_profile, consumer.lineage].contains(&[0; 64]) {
        return Err(ApprovedSourceRefusalV1::Context);
    }
    let start = consumer.commitment_position.checked_add(policy.anchor_delay).ok_or(ApprovedSourceRefusalV1::Overflow)?;
    let end = start.checked_add(policy.window).ok_or(ApprovedSourceRefusalV1::Overflow)?;
    let eligible = |e: &ApprovedSourceEventV1| {
        e.version == 1
            && e.network == policy.network
            && e.bootstrap_epoch == policy.bootstrap_epoch
            && e.bootstrap_policy == policy.bootstrap_policy
            && e.source_profile == policy.source_profile
            && e.checker_scope == policy.checker_scope
            && e.source_profile != consumer.target_profile
            && e.anchor_position < consumer.commitment_position
            && e.accepted_position >= start
            && e.accepted_position <= e.final_position
            && e.final_position < e.settlement_position
            && e.settlement_position >= start
            && e.settlement_position < end
            && e.settlement_position <= tip
            && e.required_slots > 0
            && e.approved_slots >= e.required_slots
            && ![
                e.frozen_population,
                e.frozen_block,
                e.anchor,
                e.statement,
                e.material_scope,
                e.execution_commitment,
                e.approval_root,
                e.canonical_work,
                e.claim,
                e.producer,
                e.consumer,
            ]
            .contains(&[0; 64])
    };
    // Reject cycles even in events that later fall outside the freshness window:
    // silently skipping a cycle would make the selected source set caller-shaped.
    for event in events {
        if depends_on(event, consumer) {
            return Err(ApprovedSourceRefusalV1::Lineage);
        }
    }
    let context = identity(b"MISAKA/PALW/APPROVED-SOURCE-CONTEXT/V1", &(policy, consumer));
    let mut mix = h(b"MISAKA/PALW/APPROVED-SOURCE-INITIAL/V1", &context);
    let mut found = 0u32;
    let mut last: Option<(u64, Digest)> = None;
    let mut locked_at = 0u64;
    while found < policy.required_sources {
        let next = events
            .iter()
            .filter(|e| eligible(e))
            .filter(|e| last.is_none_or(|k| (e.settlement_position, e.canonical_work) > k))
            .filter(|e| {
                !events.iter().any(|other| {
                    other.canonical_work == e.canonical_work && (other.accepted_position, other.claim) < (e.accepted_position, e.claim)
                })
            })
            .min_by_key(|e| (e.settlement_position, e.canonical_work));
        let Some(event) = next else { break };
        let item = identity(b"MISAKA/PALW/APPROVED-SOURCE-ITEM/V1", event);
        let mut material = [0; 128];
        material[..64].copy_from_slice(&mix);
        material[64..].copy_from_slice(&item);
        mix = h(b"MISAKA/PALW/APPROVED-SOURCE-MIX/V1", &material);
        found += 1;
        last = Some((event.settlement_position, event.canonical_work));
        locked_at = event.settlement_position;
    }
    if found < policy.required_sources {
        return Ok(if tip >= end {
            ApprovedSourceStateV1::Unavailable { found, required: policy.required_sources }
        } else {
            ApprovedSourceStateV1::Pending { found, required: policy.required_sources }
        });
    }
    locked_at = locked_at.checked_add(policy.settlement_depth).ok_or(ApprovedSourceRefusalV1::Overflow)?;
    if tip < locked_at {
        return Ok(ApprovedSourceStateV1::Pending { found, required: policy.required_sources });
    }
    Ok(ApprovedSourceStateV1::Locked {
        context,
        root: mix,
        output: h(b"MISAKA/PALW/APPROVED-SOURCE-OUTPUT/V1", &mix),
        sources: found,
        locked_at,
    })
}
fn identity<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let n = borsh::object_length(value).expect("fixed source contract encoding");
    let mut w = Writer(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    w.0.update(&(n as u64).to_le_bytes());
    value.serialize(&mut w).expect("infallible hash writer");
    w.0.finalize().as_bytes().try_into().expect("64-byte digest")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ApprovedSourceCollectorPolicyV1, ApprovedSourceConsumerV1, ApprovedSourceEventV1) {
        // Contract mechanics only. These values are not reviewed policy and
        // these hand-built events do not exercise native approval admission.
        let policy = ApprovedSourceCollectorPolicyV1 {
            version: 1,
            network: [1; 64],
            bootstrap_epoch: [2; 64],
            bootstrap_policy: [3; 64],
            source_profile: [4; 64],
            checker_scope: [5; 64],
            required_sources: 2,
            maximum_events: 4,
            anchor_delay: 1,
            window: 10,
            settlement_depth: 2,
        };
        let context =
            ApprovedSourceConsumerV1 { commitment: [6; 64], commitment_position: 20, target_profile: [7; 64], lineage: [8; 64] };
        let event = ApprovedSourceEventV1 {
            version: 1,
            network: policy.network,
            bootstrap_epoch: policy.bootstrap_epoch,
            bootstrap_policy: policy.bootstrap_policy,
            frozen_population: [9; 64],
            frozen_block: [10; 64],
            anchor: [11; 64],
            anchor_position: 19,
            source_profile: policy.source_profile,
            checker_scope: policy.checker_scope,
            claim: [12; 64],
            canonical_work: [13; 64],
            statement: [14; 64],
            material_scope: [15; 64],
            execution_commitment: [16; 64],
            approval_root: [17; 64],
            producer: [18; 64],
            consumer: [19; 64],
            accepted_position: 21,
            final_position: 22,
            settlement_position: 23,
            approved_slots: 2,
            required_slots: 2,
        };
        (policy, context, event)
    }
    #[test]
    fn collector_orders_native_events_and_deduplicates_rewraps() {
        let (p, c, a) = fixture();
        let mut b = a;
        b.canonical_work = [20; 64];
        b.claim = [21; 64];
        b.settlement_position = 24;
        let mut wrapped = a;
        wrapped.claim = [22; 64];
        wrapped.accepted_position = 22;
        let one = collect_approved_sources_v1(&p, &c, &[a, b, wrapped], 30, |_, _| false).unwrap();
        let shuffled = collect_approved_sources_v1(&p, &c, &[wrapped, b, a], 30, |_, _| false).unwrap();
        assert_eq!(one, shuffled);
        assert!(matches!(one, ApprovedSourceStateV1::Locked { sources: 2, locked_at: 26, .. }));
        assert!(matches!(
            collect_approved_sources_v1(&p, &c, &[a, wrapped], 31, |_, _| false).unwrap(),
            ApprovedSourceStateV1::Unavailable { found: 1, .. }
        ));
    }
    #[test]
    fn missing_approval_and_cycles_never_become_sources_by_expiry() {
        let (mut p, c, mut a) = fixture();
        p.required_sources = 1;
        a.approved_slots = 0;
        assert!(matches!(
            collect_approved_sources_v1(&p, &c, &[a], 100, |_, _| false).unwrap(),
            ApprovedSourceStateV1::Unavailable { found: 0, .. }
        ));
        a.approved_slots = 2;
        a.approval_root = [0; 64];
        assert!(matches!(
            collect_approved_sources_v1(&p, &c, &[a], 100, |_, _| false).unwrap(),
            ApprovedSourceStateV1::Unavailable { found: 0, .. }
        ));
        assert_eq!(collect_approved_sources_v1(&p, &c, &[a], 100, |_, _| true), Err(ApprovedSourceRefusalV1::Lineage));
    }
    #[test]
    fn a_late_anchor_or_foreign_epoch_cannot_be_substituted() {
        let (mut p, c, mut a) = fixture();
        p.required_sources = 1;
        a.anchor_position = c.commitment_position;
        assert!(matches!(
            collect_approved_sources_v1(&p, &c, &[a], 100, |_, _| false).unwrap(),
            ApprovedSourceStateV1::Unavailable { found: 0, .. }
        ));
        a.anchor_position = 19;
        a.bootstrap_epoch = [99; 64];
        assert!(matches!(
            collect_approved_sources_v1(&p, &c, &[a], 100, |_, _| false).unwrap(),
            ApprovedSourceStateV1::Unavailable { found: 0, .. }
        ));
    }
}
