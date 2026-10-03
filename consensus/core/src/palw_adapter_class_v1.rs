//! **RFC-0001 §2.10 (ADR-0163): the adapter class listing; dormant behind `palw_adapter_class_v1`.**
//!
//! (The module's logic follows its fence: this first section is the fence's own plumbing — the accessor, the
//! prerequisites `validate_palw_v2` asks by name, and the drill entry a salted chain arms it with.)

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms `palw_adapter_class_v1` with** (`--palw-drill-adapter-class-v1-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_ADAPTER_CLASS_V1_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 {
        name: "palw_adapter_class_v1",
        set: |params, at| {
            params.palw_adapter_class_v1 = at;
            params.sync_palw_adapter_class_v1();
        },
    };

/// The drill's one-entry list.
pub const PALW_DRILL_ADAPTER_CLASS_V1_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_ADAPTER_CLASS_V1_ENTRY];

impl Params {
    /// `palw_adapter_class_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_adapter_class_v1_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_adapter_class_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_adapter_class_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_adapter_class_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`adapter_class_from_daa`), which the fold reads. Written
    /// here and nothing else; `None` where the fence is not armed (or is `never()`). Call it wherever the fence is set
    /// on an assembled ruleset; [`Self::validate_palw_adapter_class_v1_v1`] refuses a ruleset whose copy disagrees.
    pub fn sync_palw_adapter_class_v1(&mut self) {
        let from_daa = self.palw_adapter_class_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_adapter_class_from_daa(from_daa);
        }
    }

    /// **`palw_adapter_class_v1`'s own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule over its prerequisites,
    /// each in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_adapter_class_v1_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.adapter_class_from_daa(),
            _ => None,
        };
        let armed = self.palw_adapter_class_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if armed.is_some() && !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_adapter_class_v1 is armed on a network that is not ConsensusV2"));
        }
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_adapter_class_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_adapter_class_v1 \
                 after the bundle is assembled",
            ));
        }
        let Some(fence) = self.palw_adapter_class_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_adapter_class_v1 is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_tir_v1.map(|fence| fence.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_adapter_class_v1 needs palw_tir_v1 in force at or below it: an adapter class is an IR class (as its fence activation)",
            ));
        }
        if !at_or_below(self.palw_improvement_v1.map(|fence| fence.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_adapter_class_v1 needs palw_improvement_v1 in force at or below it: the composite machinery lives behind it (as its fence activation)",
            ));
        }
        Ok(())
    }
}

// =================================================================================================
// The listing (RFC-0001 §2.10; ADR-0163): `AdapterClassListed`, object tag 94
// =================================================================================================

use crate::Hash64;
use crate::palw_improve_composite_v1::PalwTirCompositeRefV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use borsh::{BorshDeserialize, BorshSerialize};

/// Key of [`palw_adapter_listing_message_v1`].
pub const PALW_ADAPTER_LISTING_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/adapter-class/listed/message/v1";
/// The ML-DSA-87 context a lister's bond signs an `AdapterClassListed` under.
pub const PALW_ADAPTER_LISTING_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw-adapter-class-v1";

/// **The `AdapterClassListed` payload** (tag 94): a registered IR class `class_id` whose artifact is the composite
/// `artifact` (parent + adapter section, RFC-0004 §6.3), with the class's layout carried because the `tir_classes`
/// record keeps only its digest (admission rebuilds the class over the composite root to recheck the class id).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAdapterClassListingV1 {
    pub class_id: Hash64,
    pub artifact: PalwTirCompositeRefV1,
    pub layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
}

/// **The message a lister bond signs**: the network domain, the payload whole and the lister — so a listing can
/// be neither replayed on another network nor lifted onto another bond.
pub fn palw_adapter_listing_message_v1(network_domain: &Hash64, payload: &PalwAdapterClassListingV1, lister: &PalwBondKeyV2) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_ADAPTER_LISTING_MESSAGE_DOMAIN_V1).to_state();
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(payload).expect("a payload is borsh-serializable"));
    state.update(&borsh::to_vec(lister).expect("a bond key is borsh-serializable"));
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The listing's own form**, state-free: an adapter section of at least one param past the parent's `p` (`p` > 0:
/// a composite whose parent section is empty is a full-weight class under another name), and a class that is not its
/// own parent.
pub fn palw_adapter_listing_shape_v1(payload: &PalwAdapterClassListingV1) -> Result<(), &'static str> {
    if payload.artifact.p == 0 {
        return Err("an adapter listing whose parent section is empty (p = 0) is a full-weight class, not an adapter");
    }
    if payload.artifact.parent_class == payload.class_id {
        return Err("an adapter class is its own parent");
    }
    Ok(())
}

impl PalwChainStateV2 {
    /// **The acceptance layer's half of an `AdapterClassListed`, over this state**: the class and its parent are
    /// registered IR classes, and the composite passes [`crate::palw_improve_composite_v1::palw_tir_candidate_artifact_admits_v1`]
    /// (the parent's family, the composite rule, the reference, every terminal close carriable in the composite
    /// form) — the SAME admission a governed line's composite candidate meets, with full weights not allowed.
    /// Heavy: one a block (the acceptance walk's sizing slot, shared with IR registrations and composite candidates).
    pub fn adapter_class_acceptance_v1(
        &self,
        payload: &PalwAdapterClassListingV1,
        rules: crate::palw_improve_composite_v1::PalwTirCompositeAdmissionV1,
    ) -> Result<(), String> {
        palw_adapter_listing_shape_v1(payload).map_err(str::to_string)?;
        let record = self.tir_class_v1(&payload.class_id).ok_or("the listed class is not a registered IR class")?;
        let artifact_root = self.class(&payload.class_id).ok_or("the listed class has no row")?.artifact_root;
        let parent = payload.artifact.parent_class;
        let parent_record = self.tir_class_v1(&parent).ok_or("the parent is not a registered IR class")?;
        let parent_root = self.class(&parent).ok_or("the parent's class has no row")?.artifact_root;
        let facts = crate::palw_improve_composite_v1::PalwTirCandidateFactsV1 {
            record,
            artifact_root,
            layout: &payload.layout,
            parent_class_id: parent,
            parent_record,
            parent_root,
            full_weights_allowed: false,
            rules,
        };
        crate::palw_improve_composite_v1::palw_tir_candidate_artifact_admits_v1(
            &payload.class_id,
            crate::palw_improve_composite_v1::PalwTirCandidateArtifactV1::Composite(&payload.artifact),
            &facts,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod listing_tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn payload() -> PalwAdapterClassListingV1 {
        PalwAdapterClassListingV1 {
            class_id: h(2),
            artifact: PalwTirCompositeRefV1 { parent_class: h(1), parent_root: h(3), adapter_root: h(4), p: 5 },
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: 1,
                max_context: 8,
                checkpoint_interval: 1,
                h_tile: 1,
                commit_tiles: vec![],
                state_tiles: vec![],
            },
        }
    }

    fn bond(i: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: h(i), index: 0 })
    }

    #[test]
    fn the_signed_message_binds_the_network_the_payload_and_the_lister() {
        let m = palw_adapter_listing_message_v1(&h(7), &payload(), &bond(1));
        assert_ne!(m, palw_adapter_listing_message_v1(&h(8), &payload(), &bond(1)), "another network");
        assert_ne!(m, palw_adapter_listing_message_v1(&h(7), &payload(), &bond(2)), "another lister");
        let mut other = payload();
        other.artifact.adapter_root = h(9);
        assert_ne!(m, palw_adapter_listing_message_v1(&h(7), &other, &bond(1)), "another adapter");
        let wire = borsh::to_vec(&payload()).unwrap();
        assert_eq!(borsh::from_slice::<PalwAdapterClassListingV1>(&wire).unwrap(), payload(), "the wire round-trips");
    }

    #[test]
    fn the_form_refuses_an_empty_parent_section_and_a_class_that_is_its_own_parent() {
        assert_eq!(palw_adapter_listing_shape_v1(&payload()), Ok(()));
        let mut empty = payload();
        empty.artifact.p = 0;
        assert!(palw_adapter_listing_shape_v1(&empty).unwrap_err().contains("p = 0"));
        let mut own = payload();
        own.artifact.parent_class = own.class_id;
        assert!(palw_adapter_listing_shape_v1(&own).unwrap_err().contains("its own parent"));
    }
}
