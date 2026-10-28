//! **RFC-0002 Tensor IR identity: the three roots that used to share one type** (lane rfc02-09, dormant).
//!
//! Three different objects were all spelled `Hash64`, and were confused on two separate paths (the 2026-09-23 class-root
//! incidents: an A16 `artifact_digest` and a TIR inventory root handed to each other's readers):
//!
//! * [`PalwContainerRootV1`] — the root of a distributed `PALWART3` container (`palwart_root_v3`); what the genesis-8 catalog
//!   names and what a class's container binding (`PalwTirContainerBindingV1`, tag 175) binds.
//! * [`PalwInventoryRootV1`] — the TIR inventory root the container commits as its weight section's inner root; what a TIR class
//!   registers as its `artifact_root` and what every court opens operands under.
//! * [`PalwArtifactDigestV1`] — the legacy A16 `artifact_digest` (a different hash over a different file).
//!
//! A value moves between them only through a checked door here: a container root becomes an inventory root only by recomputing
//! the opening and reading its ONE TIR-inventory weight section ([`palw_tir_inventory_of_container_v1`]); a registered class root
//! is read as an inventory root only for a class the chain holds a TIR record for ([`palw_registered_inventory_root_v1`]); and the
//! catalog key of a class ([`palw_catalog_container_root_of_class_v1`]) is the bound container root only where the chain recorded
//! the binding (past `palw_tir_container_identity_v1`, at or before the DAA asked). There is no `From<Hash64>` and no conversion
//! between the newtypes, so a swap is a type error at every boundary that adopts them.
//!
//! Hashes for identity only (PRINCIPLES §8): nothing here proves a computation.
use crate::Hash64;
use crate::palw_tir_container_v1::{
    PALWART_INNER_TIR_INVENTORY_ROOT_V3, PALWART_SECTION_TIR_INVENTORY_V3, PalwTirContainerBindingV1, palwart_section_is_weights_v3,
};

macro_rules! palw_root_newtype {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Hash64);

        impl $name {
            /// Name a raw hash as this root — the caller states which object it is; a zero hash is never a root.
            pub fn named(raw: Hash64) -> Option<Self> {
                (raw != Hash64::default()).then_some(Self(raw))
            }
            /// The raw hash, for the wire and the state rows that predate the type.
            pub fn hash(&self) -> Hash64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

palw_root_newtype!(
    /// The root of a `PALWART3` container (`palwart_root_v3`).
    PalwContainerRootV1
);
palw_root_newtype!(
    /// A TIR inventory root — a TIR class's registered `artifact_root`, the root its courts open operands under.
    PalwInventoryRootV1
);
palw_root_newtype!(
    /// A legacy A16 `artifact_digest`.
    PalwArtifactDigestV1
);

/// **Container root → inventory root, checked**: the opening must recompute the bound container root, hold exactly one weight
/// section, and that section must be a TIR inventory (kind and inner-root scheme) with a non-zero inner root and leaf count.
/// Nothing about the class is checked here (that is `check_tir_container_binding_v1`'s job); this is only the type boundary.
pub fn palw_tir_inventory_of_container_v1(
    binding: &PalwTirContainerBindingV1,
) -> Result<(PalwContainerRootV1, PalwInventoryRootV1), &'static str> {
    let container = PalwContainerRootV1::named(binding.container_root).ok_or("a zero container root")?;
    if binding.opening.root_v1() != binding.container_root {
        return Err("the container opening does not recompute the container root");
    }
    let mut weights = binding.opening.sections.iter().filter(|s| palwart_section_is_weights_v3(s.kind));
    let section = weights.next().ok_or("the container holds no weight section")?;
    if weights.next().is_some() {
        return Err("the container holds more than one weight section");
    }
    if section.kind != PALWART_SECTION_TIR_INVENTORY_V3 || section.inner_root_scheme != PALWART_INNER_TIR_INVENTORY_ROOT_V3 {
        return Err("the container's weight section is not a TIR inventory (an A16 digest or a PALWQ361 file is not an inventory root)");
    }
    if section.inner_leaf_count == 0 {
        return Err("the container's inventory is empty");
    }
    let inventory = PalwInventoryRootV1::named(section.inner_root).ok_or("a zero inner inventory root")?;
    Ok((container, inventory))
}

/// **A registered class's `artifact_root`, read as an inventory root** — only for a class the chain holds a TIR record for
/// (`None` otherwise: an A16 or PALWQ361 class's root is not an inventory root and is never handed out as one).
pub fn palw_registered_inventory_root_v1(state: &crate::palw_state_v2::PalwChainStateV2, class_id: &Hash64) -> Option<PalwInventoryRootV1> {
    state.tir_class_v1(class_id)?;
    PalwInventoryRootV1::named(state.class(class_id)?.artifact_root)
}

/// **The container root a TIR class is catalogued under at `daa`**: the bound container's root, when the chain recorded the
/// binding (it folds only past `palw_tir_container_identity_v1`) at or before `daa`. `None` for an unbound class, a non-TIR class,
/// or a DAA before the binding — the catalog then matches the registered root alone, exactly as before this lane.
pub fn palw_catalog_container_root_of_class_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    class_id: &Hash64,
    daa: u64,
) -> Option<PalwContainerRootV1> {
    palw_registered_inventory_root_v1(state, class_id)?;
    let row = crate::palw_tir_container_v1::palw_tir_bound_container_v1(state, *class_id)?;
    (row.bound_daa <= daa).then_some(())?;
    PalwContainerRootV1::named(row.container_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_container_v1::{PalwArtRootOpeningV1, PalwArtSectionOpeningV1, PALWART_VERSION_V3};

    fn binding(sections: Vec<PalwArtSectionOpeningV1>) -> PalwTirContainerBindingV1 {
        let opening = PalwArtRootOpeningV1 {
            version: PALWART_VERSION_V3,
            manifest_digest: Hash64::from_u64_word(1),
            program_kind: 3,
            program: vec![1, 2, 3],
            kernel_id: Hash64::from_u64_word(2),
            kernel_version: 0,
            tokenizer_id: Hash64::from_u64_word(3),
            sections,
        };
        PalwTirContainerBindingV1 { version: 1, class_id: Hash64::from_u64_word(4), container_root: opening.root_v1(), opening }
    }

    fn section(kind: u16, scheme: u16, inner: Hash64) -> PalwArtSectionOpeningV1 {
        PalwArtSectionOpeningV1 {
            kind,
            family: 1,
            name: "w".into(),
            inner_root_scheme: scheme,
            inner_root: inner,
            inner_leaf_count: 3,
            entries_root: Hash64::from_u64_word(9),
        }
    }

    #[test]
    fn a_container_root_becomes_an_inventory_root_only_through_its_tir_inventory_section() {
        let inv = Hash64::from_u64_word(0x1A);
        let b = binding(vec![section(PALWART_SECTION_TIR_INVENTORY_V3, PALWART_INNER_TIR_INVENTORY_ROOT_V3, inv)]);
        let (container, inventory) = palw_tir_inventory_of_container_v1(&b).unwrap();
        assert_eq!(container.hash(), b.container_root);
        assert_eq!(inventory.hash(), inv);
        assert_ne!(container.hash(), inventory.hash(), "the two roots name different objects");

        // An A16-digest weight section (kind 1) is not an inventory root, whatever its inner hash.
        let a16 = binding(vec![section(1, 1, inv)]);
        assert!(palw_tir_inventory_of_container_v1(&a16).is_err());
        // A container root the opening does not recompute.
        let mut forged = b.clone();
        forged.container_root = Hash64::from_u64_word(0xF0);
        assert!(palw_tir_inventory_of_container_v1(&forged).is_err());
        // Two weight sections, a zero inner root.
        let two = binding(vec![
            section(PALWART_SECTION_TIR_INVENTORY_V3, PALWART_INNER_TIR_INVENTORY_ROOT_V3, inv),
            section(PALWART_SECTION_TIR_INVENTORY_V3, PALWART_INNER_TIR_INVENTORY_ROOT_V3, inv),
        ]);
        assert!(palw_tir_inventory_of_container_v1(&two).is_err());
        let zero = binding(vec![section(PALWART_SECTION_TIR_INVENTORY_V3, PALWART_INNER_TIR_INVENTORY_ROOT_V3, Hash64::default())]);
        assert!(palw_tir_inventory_of_container_v1(&zero).is_err());
    }

    #[test]
    fn a_zero_hash_is_never_named_as_any_root() {
        assert!(PalwContainerRootV1::named(Hash64::default()).is_none());
        assert!(PalwInventoryRootV1::named(Hash64::default()).is_none());
        assert!(PalwArtifactDigestV1::named(Hash64::default()).is_none());
    }
}
