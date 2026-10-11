//! **RFC-0002 P0 (lane RFC02-T, 2026-10-11): protected refutations of candidate statements** — a versioned, actor-bound
//! seal/reveal statement for the three bounty-paying candidate courts (model artifact binding v2, model vector v2, flat model vector
//! v3), behind the dormant fence `palw_model_refutation_seal_v1` (refused when armed).
//!
//! **The failure it closes** (codex's `flat_seal6` counterexample): the frozen refutations (inner 23 / 27 / 32) pay the 49 % bounty to
//! whichever authentic filing is included FIRST. A copyist who reads an honest finder's filing in the mempool can re-encode the same
//! relation (a redundant authentic prompt tile changes the carrier bytes, not the conviction) and be included first: it collects the
//! bounty and the original finder collects zero. An exact-bytes seal (G14-R4's GAP-R7) cannot close this, since the copy has other
//! bytes.
//!
//! **The protected path.** Past the fence a refutation is two objects:
//!
//! * `SealModelRefutationV4 { slot, seal }` (inner 36): `seal = H(domain; kind ‖ target ‖ actor ‖ salt ‖ H(proof))` — the target,
//!   the proof and the salt are hidden; the actor is the AUTHENTICATED signer, never a field. One row per `(bond, slot)` in kernel
//!   route aux table [`MODEL_REFUTATION_SEAL_TABLE_V4`], at most [`MODEL_REFUTATION_SEAL_SLOTS_V4`] slots per bond (the shared
//!   processing quota: fixed-size rows, constant-key reads, no scan). A live row reserves the route policy's `seal_deposit` of the
//!   bond's ORIGINAL collateral (it is in `palw_bond_committed_v1` and the withdrawal gate), so one capital cannot back unbounded seals
//!   across bonds' catalogs; posting it spends the block's route budget like every metadata object.
//! * `RevealModelRefutationV4 { slot, kind, target, salt, proof }` (inner 37): admissible only with the signer's OWN seal in that slot,
//!   at least `claim_seal_delay_daa` old, unexpired (`seal_ttl_daa`), opening to exactly these bytes. Then the kind's existing court
//!   runs unchanged (its block budget, dismissal fee, stored envelope, same-operator refusal, atomic slash-and-pay and `refuted` flag —
//!   so the payout is once only). The row is consumed when a court ran (proven or dismissed), freeing the slot and the deposit.
//!
//! There is **no unsealed fallback** and no beneficiary search over other sealers: past the fence the frozen inner 23 / 27 / 32 are
//! refused by name. A copyist must hold a mature seal of its OWN bytes before the finder's reveal is included — it cannot, because it
//! learns the bytes only from that reveal; to win it must keep the finder's reveal out of every block for `claim_seal_delay_daa`
//! (a censorship assumption, stated, not removed).
//!
//! **Economics** (see `docs/design/palw/tir/rfc0002-implementation.md`, RFC02-T section): the honest finder's costs are the carrier
//! fees and a refundable deposit; a proven reveal pays the unchanged 49 % of the poster's reservation. A poster refuting itself
//! through a bond of another operator loses 51 % of its reservation (plus fees); one of the same operator is refused. If a poster
//! pre-empts an honest finder with its own sealed self-refutation, the finder's reveal is refused before any court (no fee): it loses
//! only its carrier fees and keeps its deposit.
use crate::{palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::Digest;

/// Kernel route aux table: `(bond, slot) → ModelRefutationSealRowV4`.
pub const MODEL_REFUTATION_SEAL_TABLE_V4: u8 = 64;
/// Seal slots per bond (the shared processing quota; fixed-size rows).
pub const MODEL_REFUTATION_SEAL_SLOTS_V4: u8 = 4;
/// The abstract route-budget work one seal posting spends (a fixed-metadata object: one row read, one row write).
pub const MODEL_REFUTATION_SEAL_WORK_V4: u64 = 1 << 12;
/// The largest proof a reveal can carry (the largest frozen candidate court carrier, inside one kernel route object).
pub const MODEL_REFUTATION_MAX_PROOF_BYTES_V4: usize = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 - 256;

/// The target kinds (the three bounty-paying candidate courts).
pub const MODEL_REFUTATION_KIND_ARTIFACT_V2: u8 = 1;
pub const MODEL_REFUTATION_KIND_VECTOR_V2: u8 = 2;
pub const MODEL_REFUTATION_KIND_FLAT_VECTOR_V3: u8 = 3;

const PROOF_DIGEST_DOMAIN_V4: &[u8] = b"misaka-palw/model-refutation/proof/v4";
const SEAL_DOMAIN_V4: &[u8] = b"misaka-palw/model-refutation/seal/v4";

/// `H(domain; proof)` — the exact carrier bytes.
pub fn model_refutation_proof_digest_v4(proof: &[u8]) -> Digest {
    misaka_palw_kernel::hash::id(PROOF_DIGEST_DOMAIN_V4, proof)
}

/// **The seal a finder posts**: `H(domain; kind ‖ target ‖ actor ‖ salt ‖ H(proof))`. The salt is the finder's secret (it hides
/// the target and the proof until the reveal); `actor` is the finder's own bond, which consensus takes from the signature.
pub fn model_refutation_seal_v4(kind: u8, target: &Digest, actor: &PalwBondKeyV2, salt: &Digest, proof: &[u8]) -> Digest {
    seal_of_digest_v4(kind, target, actor, salt, &model_refutation_proof_digest_v4(proof))
}

pub fn seal_of_digest_v4(kind: u8, target: &Digest, actor: &PalwBondKeyV2, salt: &Digest, digest: &Digest) -> Digest {
    let actor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(actor);
    misaka_palw_kernel::hash::object_id(SEAL_DOMAIN_V4, &(4u16, kind, *target, actor, *salt, *digest))
}

/// One live seal (fixed size).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelRefutationSealRowV4 {
    pub version: u16,
    pub seal: Digest,
    pub posted_daa: u64,
    pub mature_daa: u64,
    pub expires_daa: u64,
    /// The original collateral the row reserves while live (the route policy's `seal_deposit` at posting).
    pub deposit: u64,
}

impl ModelRefutationSealRowV4 {
    pub fn live_at(&self, daa: u64) -> bool {
        daa < self.expires_daa
    }
}

/// What a candidate court did with a refutation (the reveal consumes its seal only when a court ran).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelRefutationOutcomeV4 {
    /// The block's route budget could not take the court: nothing happened (the object may be carried again).
    NotRun,
    /// The court ran and dismissed the proof: the filer paid its dismissal fee.
    Dismissed,
    /// The court convicted: the poster's reservation was slashed and the bounty paid, once.
    Proven,
}

pub fn model_refutation_seal_key_v4(bond: &PalwBondKeyV2, slot: u8) -> Vec<u8> {
    borsh::to_vec(&(bond, slot)).expect("a bond key serializes")
}

impl PalwKernelRouteStateV1 {
    /// The seal in `bond`'s `slot`, if any (live or expired).
    pub fn model_refutation_seal_v4(&self, bond: &PalwBondKeyV2, slot: u8) -> Option<ModelRefutationSealRowV4> {
        if slot >= MODEL_REFUTATION_SEAL_SLOTS_V4 {
            return None;
        }
        self.aux_row(MODEL_REFUTATION_SEAL_TABLE_V4, &model_refutation_seal_key_v4(bond, slot))
    }

    /// The original collateral `bond`'s live seals reserve at `daa` (0 with no row: every older ruleset is unchanged).
    pub fn model_refutation_seal_reserved_v4(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        (0..MODEL_REFUTATION_SEAL_SLOTS_V4)
            .filter_map(|slot| self.model_refutation_seal_v4(bond, slot))
            .filter(|row| row.live_at(daa))
            .map(|row| row.deposit as u128)
            .sum()
    }
}

/// A reveal names one of the three target kinds; any other is refused before any court.
pub fn known_model_refutation_kind_v4(kind: u8) -> bool {
    matches!(kind, MODEL_REFUTATION_KIND_ARTIFACT_V2 | MODEL_REFUTATION_KIND_VECTOR_V2 | MODEL_REFUTATION_KIND_FLAT_VECTOR_V3)
}

impl crate::config::params::Params {
    /// The fence's activation, where the network declares one (`None`: absent or `never()`).
    pub fn palw_model_refutation_seal_activation(&self) -> Option<u64> {
        use crate::config::params::ForkActivation;
        self.palw_model_refutation_seal_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// **The fence's refusal**: prerequisites by name first (the kernel route and OPV at or below it — the candidate statements exist
    /// only there), then the blanket refusal: it is armed only by the single full-activation release.
    pub fn validate_palw_model_refutation_seal_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::config::params::ForkActivation;
        use crate::palw_mode_v2::PalwModeV2Error;
        let Some(at) = self.palw_model_refutation_seal_activation() else { return Ok(()) };
        let route = self.palw_probabilistic_constraints_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if !route.is_some_and(|r| r <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_refutation_seal_v1 requires palw_probabilistic_constraints_v1 (the kernel route) armed at or below it",
            ));
        }
        let opv =
            self.palw_panel_free_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score());
        if !opv.is_some_and(|o| o <= at) {
            return Err(PalwModeV2Error::Invalid("palw_model_refutation_seal_v1 requires palw_panel_free_v1 armed at or below it"));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_model_refutation_seal_v1 cannot be armed: the protected candidate refutation is armed only by the single full-activation \
             release",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{ForkActivation, MAINNET_PARAMS, TESTNET_PARAMS};
    use crate::tx::TransactionOutpoint;

    #[test]
    fn palw_model_refutation_seal_the_fence_is_dormant_everywhere_refused_when_armed_and_hashed_some_only() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_model_refutation_seal_v1, None);
            p.validate_palw_model_refutation_seal_v1().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_model_refutation_seal_v1", None)));
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_model_refutation_seal_v1 = Some(ForkActivation::never());
        p.validate_palw_model_refutation_seal_v1().unwrap();
        assert_eq!(p.palw_model_refutation_seal_activation(), None);
        p.palw_model_refutation_seal_v1 = None;
        assert_eq!((p.consensus_params_id(), p.consensus_schedule_id()), (id, schedule), "None is unhashed");
        p.palw_model_refutation_seal_v1 = Some(ForkActivation::new(1_000));
        assert!(p.validate_palw_model_refutation_seal_v1().is_err(), "refused when armed");
        assert_ne!(p.consensus_params_id(), id, "an armed height is a different network");
        assert_ne!(p.consensus_schedule_id(), schedule);
    }

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([n; 64]), 0))
    }

    #[test]
    fn a_seal_binds_kind_target_actor_salt_and_the_exact_bytes() {
        let base = model_refutation_seal_v4(3, &[1; 64], &bond(1), &[2; 64], b"proof");
        assert_ne!(base, model_refutation_seal_v4(2, &[1; 64], &bond(1), &[2; 64], b"proof"), "kind");
        assert_ne!(base, model_refutation_seal_v4(3, &[9; 64], &bond(1), &[2; 64], b"proof"), "target");
        assert_ne!(base, model_refutation_seal_v4(3, &[1; 64], &bond(2), &[2; 64], b"proof"), "actor");
        assert_ne!(base, model_refutation_seal_v4(3, &[1; 64], &bond(1), &[3; 64], b"proof"), "salt");
        assert_ne!(base, model_refutation_seal_v4(3, &[1; 64], &bond(1), &[2; 64], b"proof "), "an equivalent re-encoding");
        assert_eq!(base, seal_of_digest_v4(3, &[1; 64], &bond(1), &[2; 64], &model_refutation_proof_digest_v4(b"proof")));
    }

    #[test]
    fn the_row_is_fixed_size_and_round_trips() {
        let row = ModelRefutationSealRowV4 { version: 4, seal: [7; 64], posted_daa: 1, mature_daa: 2, expires_daa: 3, deposit: 4 };
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(bytes.len(), 2 + 64 + 8 * 4);
        assert_eq!(borsh::from_slice::<ModelRefutationSealRowV4>(&bytes).unwrap(), row);
        assert!(row.live_at(2) && !row.live_at(3));
    }
}
