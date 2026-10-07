//! **ADR-0173 — the three conditions a model root meets, and the dormant skeleton of the two the chain does not yet enforce**
//! (lane MU; no state, no object tag, no fence read: nothing here is wired to the fold).
//!
//! * **Possession** — the seats hold the root: `(bond, class, root)` rows and the root's floor. Enforced past `palw_audit_1004_v1`
//!   (`palw_state_v2.rs`, `palw_class_seating_v1.rs`); not in this file.
//! * **Validation** — the root is an allowed, safe artifact: TIR/data-only formats, a canonicalised root, and an attestation by
//!   independent operators over the format, the manifest and the behaviour checks they ran. A publisher alone cannot activate.
//!   This file is the attestation's shape, its signed message and the counting rule — a skeleton.
//! * **Activation** — `Candidate → Canary (claim-limited) → Active`, with a per-root immediate revoke and a rollback to the previous
//!   root. This file is the stage machine as a pure function — a skeleton.
//!
//! The open questions (what behaviour checks a canary runs; who the validators are; the canary's limit) are the ADR's §7.

use crate::Hash64;

/// How many independent validation attestations a root needs before it may leave `Candidate` (an operator counts once).
pub const PALW_ROOT_ATTESTATIONS_REQUIRED_V1: u16 = 3;
/// The share of a class's claim capacity a root in `Canary` may take, in permille (open question Q2: the ADR proposes 100).
pub const PALW_ROOT_CANARY_LIMIT_PERMILLE_V1: u16 = 100;
/// The least DAA a root stays in `Canary` (open question Q2).
pub const PALW_ROOT_CANARY_MIN_DAA_V1: u64 = 2_000;

/// The formats a validator may attest (data-only: no executable code rides an artifact — ADR-0067).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwRootFormatV1 {
    /// A TIR (RFC-0002) lowered graph plus its tensor inventory.
    TirInventory = 1,
    /// Safetensors / GGUF data lowered to the same inventory.
    DataOnlyWeights = 2,
}

/// **One validator's statement about one root** — what it checked, signed by its bond over [`palw_root_attestation_message_v1`].
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRootAttestationV1 {
    pub class_id: Hash64,
    pub root: Hash64,
    pub format: PalwRootFormatV1,
    /// The manifest the validator read (layer shapes, dtypes, byte count), as a digest.
    pub manifest_digest: Hash64,
    /// The digest of the behaviour-check results (open question Q1: which checks), `None` when none were run.
    pub behaviour_digest: Option<Hash64>,
    /// `true` when the validator found the root allowed and safe.
    pub pass: bool,
    pub signed_daa: u64,
}

/// The message a validator signs (domain-separated; covers every field, so a pass cannot be replayed for another root or manifest).
pub fn palw_root_attestation_message_v1(network_domain: Hash64, validator_operator: &Hash64, a: &PalwRootAttestationV1) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/root-attestation/v1").to_state();
    state.update(network_domain.as_byte_slice());
    state.update(validator_operator.as_byte_slice());
    state.update(&borsh::to_vec(a).expect("an attestation is borsh-serializable"));
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The counting rule**: distinct operators other than the publisher's, each counted once, every attestation naming the same
/// `(class, root, format, manifest)`; a single failing attestation from an independent operator blocks (fail closed).
/// `attestations` pairs each attestation with its signer's operator id (the signature is the caller's to verify).
pub fn palw_root_validated_v1(
    class_id: &Hash64,
    root: &Hash64,
    publisher_operator: Option<&Hash64>,
    attestations: &[(Hash64, PalwRootAttestationV1)],
) -> bool {
    let mut passing: Vec<&Hash64> = Vec::new();
    let reference = attestations.iter().find(|(_, a)| a.class_id == *class_id && a.root == *root).map(|(_, a)| (a.format, a.manifest_digest));
    let Some(reference) = reference else { return false };
    for (operator, a) in attestations {
        if a.class_id != *class_id || a.root != *root || Some(operator) == publisher_operator {
            continue;
        }
        if !a.pass || (a.format, a.manifest_digest) != reference {
            return false;
        }
        if !passing.contains(&operator) {
            passing.push(operator);
        }
    }
    passing.len() >= usize::from(PALW_ROOT_ATTESTATIONS_REQUIRED_V1)
}

/// **A root's activation stage.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwRootStageV1 {
    /// Published; claims refused. Possession and Validation accumulate.
    Candidate,
    /// Possessed and validated; claims allowed up to the canary limit until `since_daa + PALW_ROOT_CANARY_MIN_DAA_V1`.
    Canary { since_daa: u64 },
    Active,
    /// Immediately out of force; claims already accepted keep the root they named. `fallback` is the root to return to.
    Revoked { at_daa: u64 },
}

/// What moves a stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRootSignalV1 {
    /// The root's possession floor and validation are both met.
    Ready { daa: u64 },
    /// The canary window passed with no failed check.
    CanaryClean { daa: u64 },
    /// A failed behaviour check or a safety finding (per-root; no other root is touched).
    Revoke { daa: u64 },
}

/// **The stage machine** (pure): `Candidate → Canary → Active`; `Revoke` from anywhere but `Revoked` is immediate; `Revoked` is
/// terminal for the root (a corrected root is a NEW root); a signal that does not apply changes nothing.
pub fn palw_root_stage_next_v1(stage: PalwRootStageV1, signal: PalwRootSignalV1) -> PalwRootStageV1 {
    use PalwRootSignalV1::*;
    use PalwRootStageV1::*;
    match (stage, signal) {
        (Revoked { .. }, _) => stage,
        (_, Revoke { daa }) => Revoked { at_daa: daa },
        (Candidate, Ready { daa }) => Canary { since_daa: daa },
        (Canary { since_daa }, CanaryClean { daa }) if daa >= since_daa.saturating_add(PALW_ROOT_CANARY_MIN_DAA_V1) => Active,
        _ => stage,
    }
}

/// May a claim naming a root in `stage` be accepted, given the root's share of the class's in-flight claims, in permille?
pub fn palw_root_stage_admits_claim_v1(stage: PalwRootStageV1, share_permille: u16) -> bool {
    match stage {
        PalwRootStageV1::Candidate | PalwRootStageV1::Revoked { .. } => false,
        PalwRootStageV1::Canary { .. } => share_permille < PALW_ROOT_CANARY_LIMIT_PERMILLE_V1,
        PalwRootStageV1::Active => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn attest(pass: bool, manifest: u8) -> PalwRootAttestationV1 {
        PalwRootAttestationV1 {
            class_id: h(1),
            root: h(2),
            format: PalwRootFormatV1::TirInventory,
            manifest_digest: h(manifest),
            behaviour_digest: None,
            pass,
            signed_daa: 10,
        }
    }

    #[test]
    fn a_publisher_alone_cannot_validate_and_three_independent_operators_can() {
        let publisher = h(9);
        let one = vec![(publisher, attest(true, 3)), (publisher, attest(true, 3)), (publisher, attest(true, 3))];
        assert!(!palw_root_validated_v1(&h(1), &h(2), Some(&publisher), &one));
        let two_ops = vec![(h(10), attest(true, 3)), (h(10), attest(true, 3)), (h(11), attest(true, 3))];
        assert!(!palw_root_validated_v1(&h(1), &h(2), Some(&publisher), &two_ops), "an operator counts once");
        let mut three = vec![(h(10), attest(true, 3)), (h(11), attest(true, 3)), (h(12), attest(true, 3))];
        assert!(palw_root_validated_v1(&h(1), &h(2), Some(&publisher), &three));
        three.push((h(13), attest(false, 3)));
        assert!(!palw_root_validated_v1(&h(1), &h(2), Some(&publisher), &three), "one independent failure blocks");
        let split = vec![(h(10), attest(true, 3)), (h(11), attest(true, 3)), (h(12), attest(true, 4))];
        assert!(!palw_root_validated_v1(&h(1), &h(2), Some(&publisher), &split), "attestations must agree on the manifest");
    }

    #[test]
    fn the_message_covers_every_field_and_the_signer() {
        let a = attest(true, 3);
        let base = palw_root_attestation_message_v1(h(5), &h(6), &a);
        assert_ne!(base, palw_root_attestation_message_v1(h(5), &h(7), &a));
        assert_ne!(base, palw_root_attestation_message_v1(h(8), &h(6), &a));
        assert_ne!(base, palw_root_attestation_message_v1(h(5), &h(6), &attest(false, 3)));
        assert_ne!(base, palw_root_attestation_message_v1(h(5), &h(6), &attest(true, 4)));
    }

    #[test]
    fn the_stage_machine_canaries_before_activating_and_revokes_at_once() {
        use PalwRootSignalV1::*;
        use PalwRootStageV1::*;
        let canary = palw_root_stage_next_v1(Candidate, Ready { daa: 100 });
        assert_eq!(canary, Canary { since_daa: 100 });
        assert_eq!(palw_root_stage_next_v1(canary, CanaryClean { daa: 100 + PALW_ROOT_CANARY_MIN_DAA_V1 - 1 }), canary, "too early");
        assert_eq!(palw_root_stage_next_v1(canary, CanaryClean { daa: 100 + PALW_ROOT_CANARY_MIN_DAA_V1 }), Active);
        assert_eq!(palw_root_stage_next_v1(Candidate, CanaryClean { daa: 9_999 }), Candidate, "no skipping the canary");
        assert_eq!(palw_root_stage_next_v1(Active, Revoke { daa: 7 }), Revoked { at_daa: 7 });
        assert_eq!(palw_root_stage_next_v1(Revoked { at_daa: 7 }, Ready { daa: 8 }), Revoked { at_daa: 7 }, "terminal");
        assert!(!palw_root_stage_admits_claim_v1(Candidate, 0));
        assert!(palw_root_stage_admits_claim_v1(canary, 99) && !palw_root_stage_admits_claim_v1(canary, 100));
        assert!(palw_root_stage_admits_claim_v1(Active, 1_000) && !palw_root_stage_admits_claim_v1(Revoked { at_daa: 1 }, 0));
    }
}
