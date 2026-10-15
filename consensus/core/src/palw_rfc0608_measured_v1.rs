//! **The measured-value gate of the RFC-0006/0007/0008 fences** (testnet-12 DAA 14,000 release plan, 2026-10-15).
//!
//! Before this module each of these fences refused every activation with a fixed "cannot be armed" message, so no measurement
//! could ever arm one without editing its validator. The release plan instead requires a measured-value gate per fence: a fence
//! arms on a shipping network only when (a) it has no open blocker listed here and (b) its exact policy, on that exact network,
//! matches a pinned record carrying the digests of the measurement evidence and of its independent review. Both lists are
//! changed only by a reviewed change. The record list is **EMPTY**, so every fence below stays dormant on every preset — the
//! behaviour is unchanged from the old fixed refusal, but the door to activation is now the record, never a provisional value.
//!
//! The gate adds no consensus behaviour, no object, no table and no credit path: it only decides whether `Params` validates.
//! Consensus credit still flows only through RFC-0018 Gate C; nothing here is a proof of computation (PRINCIPLES §8).
use misaka_palw_kernel::hash::{Digest, object_id};

/// The fences this gate governs. The discriminant is part of the policy root, so a record pins one fence only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwRfc0608FenceV1 {
    /// RFC-0008: the signed monetary material covenant (`palw_monetary_material_v1`, tags 240/242).
    MonetaryMaterialV1 = 1,
    /// RFC-0008: bounded release recovery V2 (`palw_material_release_recovery_v2`, aux 137, tag 241).
    MaterialReleaseRecoveryV2 = 2,
    /// RFC-0007: paid claim challenge V2/V3 (`palw_claim_challenge_v2`).
    ClaimChallengeV2 = 3,
    /// RFC-0007: opt-in beacon source (`palw_beacon_source_opt_in_v1`).
    BeaconSourceOptInV1 = 4,
    /// RFC-0007: funded beacon source fee (`palw_beacon_source_fee_v1`).
    BeaconSourceFeeV1 = 5,
    /// RFC-0006: anchored GDN step court and per-reader router binding (`palw_memory_routing_court_v2`, tags 243/244, aux 139).
    MemoryRoutingCourtV2 = 6,
    /// RFC-0008: the once-only settlement registry across settlement consumers (`palw_settlement_once_v1`, aux 140).
    SettlementOnceV1 = 7,
}
impl PalwRfc0608FenceV1 {
    pub const fn name(self) -> &'static str {
        match self {
            Self::MonetaryMaterialV1 => "palw_monetary_material_v1",
            Self::MaterialReleaseRecoveryV2 => "palw_material_release_recovery_v2",
            Self::ClaimChallengeV2 => "palw_claim_challenge_v2",
            Self::BeaconSourceOptInV1 => "palw_beacon_source_opt_in_v1",
            Self::BeaconSourceFeeV1 => "palw_beacon_source_fee_v1",
            Self::MemoryRoutingCourtV2 => "palw_memory_routing_court_v2",
            Self::SettlementOnceV1 => "palw_settlement_once_v1",
        }
    }
}

/// **Open blockers**: known code or design gaps that make arming the fence unsafe (not mere coverage gaps and not values to
/// measure — those belong to the record). While a fence has an entry here no record can arm it. Each entry names the
/// remaining-work checkbox (`docs/design/palw/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md`) that closes it.
pub const PALW_RFC0608_OPEN_BLOCKERS_V1: &[(PalwRfc0608FenceV1, &str)] = &[
    (
        PalwRfc0608FenceV1::ClaimChallengeV2,
        "RFC07-5: lazy-verifier (T6) deterrence is unresolved — a drawn checker can approve without checking and still be paid",
    ),
    (
        PalwRfc0608FenceV1::SettlementOnceV1,
        "RFC08-2 (unsourced terms): past the fence the combined liability predicate refuses, by name, every leg whose terms \
         have no authenticated source: verifier check fees (RFC07-5: no verifier liability horizon), material provider fees \
         (event-relative covenant, no finite horizon), native mining receipts and native funded/text/job/mining admissions (no \
         signed hard deadline), kernel claims outside a paid V2 enrollment (no producer-signed lifetime), beacon source-fee \
         promise payouts (no claim reservation, X3 deadline or material TTL) and audit answers paid before their trap window \
         (palw_audit_1004_v1 off)",
    ),
];

/// **A measured-value record** for one fence's exact policy on one network, with the digests of the evidence (harness commit,
/// params, hardware, network conditions, raw data, derivation) and of its independent review.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0608MeasuredRecordV1 {
    pub fence: PalwRfc0608FenceV1,
    pub network_domain: Digest,
    pub policy_root: Digest,
    pub evidence_digest: [u8; 32],
    pub review_digest: [u8; 32],
    /// The harness and scenario set the record came from (e.g. `scripts/drills/rfc08_*`, `scripts/drills/rfc12_*`).
    pub harness: &'static str,
}

/// **The pinned records — EMPTY.** Arming any RFC-0006/0007/0008 fence on a shipping network is refused.
pub const PALW_RFC0608_MEASURED_RECORDS_V1: &[PalwRfc0608MeasuredRecordV1] = &[];

/// The root a record pins: the fence and its exact policy (borsh), domain-separated.
pub fn palw_rfc0608_policy_root_v1<T: borsh::BorshSerialize>(fence: PalwRfc0608FenceV1, policy: &T) -> Digest {
    object_id(b"misaka-palw/rfc0608/measured-policy/v1", &(fence, policy))
}

/// The gate itself, with the lists as parameters (the validators pass the pinned constants).
pub fn palw_rfc0608_measured_gate_v1(
    fence: PalwRfc0608FenceV1,
    network_domain: &Digest,
    policy_root: &Digest,
    records: &[PalwRfc0608MeasuredRecordV1],
    blockers: &[(PalwRfc0608FenceV1, &str)],
) -> Result<(), String> {
    if let Some((_, reason)) = blockers.iter().find(|(f, _)| *f == fence) {
        return Err(format!("{} stays dormant: open blocker — {reason}", fence.name()));
    }
    let measured = records.iter().any(|r| {
        r.fence == fence
            && r.network_domain == *network_domain
            && r.policy_root == *policy_root
            && r.evidence_digest != [0; 32]
            && r.review_digest != [0; 32]
            && r.evidence_digest != r.review_digest
            && !r.harness.is_empty()
    });
    if !measured {
        return Err(format!(
            "{} stays dormant: no pinned measured-value record for this exact policy on this network \
             (PALW_RFC0608_MEASURED_RECORDS_V1)",
            fence.name()
        ));
    }
    Ok(())
}

impl crate::config::params::Params {
    /// This network's domain as the records pin it.
    pub fn palw_rfc0608_network_domain_v1(&self) -> Digest {
        crate::palw_attempt_v2::palw_network_domain_v2_for(self.net.to_string().as_bytes(), Some(self.genesis.hash)).as_bytes()
    }
    /// The gate with the pinned lists.
    pub fn palw_rfc0608_measured_v1<T: borsh::BorshSerialize>(
        &self,
        fence: PalwRfc0608FenceV1,
        policy: &T,
    ) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        palw_rfc0608_measured_gate_v1(
            fence,
            &self.palw_rfc0608_network_domain_v1(),
            &palw_rfc0608_policy_root_v1(fence, policy),
            PALW_RFC0608_MEASURED_RECORDS_V1,
            PALW_RFC0608_OPEN_BLOCKERS_V1,
        )
        .map_err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS};
    const ALL: [PalwRfc0608FenceV1; 7] = [
        PalwRfc0608FenceV1::MonetaryMaterialV1,
        PalwRfc0608FenceV1::MaterialReleaseRecoveryV2,
        PalwRfc0608FenceV1::ClaimChallengeV2,
        PalwRfc0608FenceV1::BeaconSourceOptInV1,
        PalwRfc0608FenceV1::BeaconSourceFeeV1,
        PalwRfc0608FenceV1::MemoryRoutingCourtV2,
        PalwRfc0608FenceV1::SettlementOnceV1,
    ];
    fn record(fence: PalwRfc0608FenceV1, domain: Digest, root: Digest) -> PalwRfc0608MeasuredRecordV1 {
        PalwRfc0608MeasuredRecordV1 {
            fence,
            network_domain: domain,
            policy_root: root,
            evidence_digest: [1; 32],
            review_digest: [2; 32],
            harness: "test",
        }
    }

    #[test]
    fn pinned_list_is_empty_and_every_preset_keeps_every_fence_none() {
        assert!(PALW_RFC0608_MEASURED_RECORDS_V1.is_empty());
        let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        for p in [MAINNET_PARAMS, TESTNET_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS, t12] {
            assert!(p.palw_monetary_material_v1.is_none());
            assert!(p.palw_material_release_recovery_v2.is_none());
            assert!(p.palw_claim_challenge_v2.is_none());
            assert!(p.palw_beacon_source_opt_in_v1.is_none());
            assert!(p.palw_beacon_source_fee_v1.is_none());
            assert!(p.palw_memory_routing_court_v2.is_none());
            assert!(p.palw_settlement_once_v1.is_none());
            for fence in ALL {
                assert!(p.palw_rfc0608_measured_v1(fence, &0u8).is_err(), "{} armed without a record", fence.name());
            }
        }
    }

    #[test]
    fn a_record_arms_only_its_own_fence_policy_and_network_and_a_blocker_overrides_it() {
        let domain = [7; 64];
        let fence = PalwRfc0608FenceV1::MonetaryMaterialV1;
        let root = palw_rfc0608_policy_root_v1(fence, &42u64);
        let records = [record(fence, domain, root)];
        assert!(palw_rfc0608_measured_gate_v1(fence, &domain, &root, &records, &[]).is_ok());
        // Another network, another policy, another fence: refused.
        assert!(palw_rfc0608_measured_gate_v1(fence, &[8; 64], &root, &records, &[]).is_err());
        assert!(palw_rfc0608_measured_gate_v1(fence, &domain, &palw_rfc0608_policy_root_v1(fence, &43u64), &records, &[]).is_err());
        let other = PalwRfc0608FenceV1::MaterialReleaseRecoveryV2;
        assert_ne!(palw_rfc0608_policy_root_v1(other, &42u64), root);
        assert!(palw_rfc0608_measured_gate_v1(other, &domain, &root, &records, &[]).is_err());
        // Zero or equal digests and an empty harness are not records.
        for bad in [
            PalwRfc0608MeasuredRecordV1 { evidence_digest: [0; 32], ..records[0] },
            PalwRfc0608MeasuredRecordV1 { review_digest: [0; 32], ..records[0] },
            PalwRfc0608MeasuredRecordV1 { review_digest: [1; 32], ..records[0] },
            PalwRfc0608MeasuredRecordV1 { harness: "", ..records[0] },
        ] {
            assert!(palw_rfc0608_measured_gate_v1(fence, &domain, &root, &[bad], &[]).is_err());
        }
        // An open blocker wins over a matching record.
        assert!(palw_rfc0608_measured_gate_v1(fence, &domain, &root, &records, &[(fence, "x")]).is_err());
    }

    #[test]
    fn listed_blockers_name_a_checkbox() {
        for (fence, reason) in PALW_RFC0608_OPEN_BLOCKERS_V1 {
            assert!(reason.starts_with("RFC0"), "{}: {reason}", fence.name());
        }
    }
}
