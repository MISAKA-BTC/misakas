//! **RFC-0008: the five capacity classes of an acceptance path, kept apart.**
//!
//! A node admits work against FIVE independent budgets. No class's headroom ever authorizes another class's allocation, and a
//! refusal always names the class that refused, so a local capacity limit is never mistaken for (or masked by) another limit or by
//! a semantic verdict:
//!
//! | class           | what it bounds                                                                   | enforced by (source)                                                                                   |
//! |-----------------|----------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------|
//! | `RawMaterial`   | retained model material BYTES (provider store total, per-consent signed cap)     | `PalwMonetaryMaterialProviderStoreV1::admit_*` (`total_capacity`, `max_bytes`; signed cap is semantic)  |
//! | `Metadata`      | retained material METADATA entries/bytes (contracts, manifests, prior-route rows) | the store's contract count (`PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1`), the provider's shared metadata lease |
//! | `TypedRam`      | native typed allocation requests (state copies, writers, decoders, ledgers)      | `NativeDemandCheckerV1` memory (`palw_native_resource_v1`: transition owner, retained owners, ledgers)   |
//! | `DbHistory`     | persisted rows, WAL, history and compaction on disk                               | `NativeHardDatabaseQuotaV1` / `reserve_native_install_in_database_v1`, tracked store write tickets       |
//! | `Worker`        | concurrent decode/verify/spill workers and their permits                          | snapshot decode worker and spill permits (protocol flows), provider worker pools                        |
//!
//! **Acceptance path → classes it consumes** ([`PALW_ACCEPTANCE_CAPACITY_MAP_V1`], the reviewable list):
//! * block/object fold (`TransitionBuilder`, settlement registry, kernel ledger mutations): `TypedRam`; its store commit `DbHistory`.
//!   It never reads a material class.
//! * snapshot receive / pruning import: `Worker` (decode worker + spill permit), `TypedRam` (preflight + restoration +
//!   consistency/root workspace), `DbHistory` (install quota). Encoded wire bytes are a codec limit, not any of the five.
//! * kernel material provider ingress (240/242) and replacement: `RawMaterial` (local store total; the signed per-consent cap is a
//!   semantic refusal), `Metadata` (retained contracts/manifests, prior-route metadata), `TypedRam` (owner growth), `DbHistory`
//!   (durable provider store).
//! * material query/answer (purpose 3) and checker retrieval: `TypedRam` (returned-bytes owner), `RawMaterial` (read limits),
//!   `Worker` (checker fetch slots).
//! * IBD / virtual resolution: `TypedRam` (retained walks, candidate states), `DbHistory` (history readers), `Worker`.
//!
//! A path that lists a class must enforce it with that class's own source; a path that does not list a class must not consult it.
//! [`PalwCapacityRefusalV1`] carries the refusing class through every layer that used to flatten these refusals into one string.
use crate::palw_material_capacity_v1::PalwMaterialCapacityErrorV1;

/// The five independent capacity classes (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwCapacityClassV1 {
    RawMaterial,
    Metadata,
    TypedRam,
    DbHistory,
    Worker,
}

impl PalwCapacityClassV1 {
    pub const ALL: [Self; 5] = [Self::RawMaterial, Self::Metadata, Self::TypedRam, Self::DbHistory, Self::Worker];

    pub const fn label(self) -> &'static str {
        match self {
            Self::RawMaterial => "raw-material",
            Self::Metadata => "metadata",
            Self::TypedRam => "typed-ram",
            Self::DbHistory => "db-history",
            Self::Worker => "worker",
        }
    }
}

impl std::fmt::Display for PalwCapacityClassV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// **An acceptance refusal, classified.** `Local` is one class's local capacity (retryable, never peer/object invalidity);
/// `Semantic` is a verdict about the input itself (its signed caps, substitution, authentication).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwCapacityRefusalV1 {
    Local { class: PalwCapacityClassV1, path: &'static str, detail: String },
    Semantic { path: &'static str, detail: String },
}

impl PalwCapacityRefusalV1 {
    pub fn local(class: PalwCapacityClassV1, path: &'static str, detail: impl std::fmt::Display) -> Self {
        Self::Local { class, path, detail: detail.to_string() }
    }

    pub fn semantic(path: &'static str, detail: impl std::fmt::Display) -> Self {
        Self::Semantic { path, detail: detail.to_string() }
    }

    /// The refusing local class, if this is a local capacity refusal.
    pub fn local_class(&self) -> Option<PalwCapacityClassV1> {
        match self {
            Self::Local { class, .. } => Some(*class),
            Self::Semantic { .. } => None,
        }
    }

    /// A material store refusal, by what actually refused (the signed cap is the input's own property: semantic).
    pub fn of_material_v1(path: &'static str, error: &PalwMaterialCapacityErrorV1) -> Self {
        match error {
            PalwMaterialCapacityErrorV1::LocalRawMaterialCapacity => Self::local(PalwCapacityClassV1::RawMaterial, path, error),
            PalwMaterialCapacityErrorV1::LocalMetadataCapacity => Self::local(PalwCapacityClassV1::Metadata, path, error),
            other => Self::semantic(path, other),
        }
    }
}

impl std::fmt::Display for PalwCapacityRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local { class, path, detail } => write!(f, "local {class} capacity refused {path}: {detail}"),
            Self::Semantic { path, detail } => write!(f, "{path} refused: {detail}"),
        }
    }
}

impl std::error::Error for PalwCapacityRefusalV1 {}

/// **The acceptance path → capacity class map** (module doc), the one reviewable statement of which budgets each path consumes.
pub const PALW_ACCEPTANCE_CAPACITY_MAP_V1: &[(&str, &[PalwCapacityClassV1])] = &[
    ("block/object fold", &[PalwCapacityClassV1::TypedRam, PalwCapacityClassV1::DbHistory]),
    (
        "snapshot receive / pruning import",
        &[PalwCapacityClassV1::Worker, PalwCapacityClassV1::TypedRam, PalwCapacityClassV1::DbHistory],
    ),
    (
        "kernel material provider ingress (240/242) and replacement",
        &[
            PalwCapacityClassV1::RawMaterial,
            PalwCapacityClassV1::Metadata,
            PalwCapacityClassV1::TypedRam,
            PalwCapacityClassV1::DbHistory,
        ],
    ),
    (
        "material query/answer and checker retrieval",
        &[PalwCapacityClassV1::TypedRam, PalwCapacityClassV1::RawMaterial, PalwCapacityClassV1::Worker],
    ),
    ("IBD / virtual resolution", &[PalwCapacityClassV1::TypedRam, PalwCapacityClassV1::DbHistory, PalwCapacityClassV1::Worker]),
];

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (not compiled or executed in this lane).
    use super::*;

    #[test]
    fn the_classes_are_distinct_and_every_path_lists_each_class_once() {
        let mut labels: Vec<_> = PalwCapacityClassV1::ALL.iter().map(|c| c.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), PalwCapacityClassV1::ALL.len());
        for (path, classes) in PALW_ACCEPTANCE_CAPACITY_MAP_V1 {
            let mut unique = classes.to_vec();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), classes.len(), "{path} names a class twice");
            assert!(!classes.is_empty(), "{path} consumes no class");
        }
        // The fold never consults a material class; material ingress never stands in for typed RAM (it lists both, separately).
        let fold = PALW_ACCEPTANCE_CAPACITY_MAP_V1.iter().find(|(p, _)| *p == "block/object fold").unwrap().1;
        assert!(!fold.contains(&PalwCapacityClassV1::RawMaterial) && !fold.contains(&PalwCapacityClassV1::Metadata));
    }

    #[test]
    fn a_material_refusal_names_the_cap_that_refused() {
        let raw = PalwCapacityRefusalV1::of_material_v1("ingress", &PalwMaterialCapacityErrorV1::LocalRawMaterialCapacity);
        let meta = PalwCapacityRefusalV1::of_material_v1("ingress", &PalwMaterialCapacityErrorV1::LocalMetadataCapacity);
        let signed = PalwCapacityRefusalV1::of_material_v1("ingress", &PalwMaterialCapacityErrorV1::Capacity);
        assert_eq!(raw.local_class(), Some(PalwCapacityClassV1::RawMaterial));
        assert_eq!(meta.local_class(), Some(PalwCapacityClassV1::Metadata));
        assert_eq!(signed.local_class(), None, "the signed per-consent cap is the input's own property: a semantic refusal");
        assert_ne!(raw, meta);
    }
}
