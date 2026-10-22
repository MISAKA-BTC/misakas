//! **Test support for the source-reading guards** (`#[doc(hidden)]`; nothing in consensus calls it).
//!
//! The virtual state processor (`consensus/src/pipeline/virtual_processor/processor.rs`) was split by responsibility on 2026-10-13
//! into child files. A guard that reads "the processor" reads [`palw_processor_source_v1`]: the parent file followed by every
//! split file, read at run time (an `include_str!` from here would rebuild this crate on every processor edit). A later split adds
//! its file to [`PALW_PROCESSOR_SOURCE_FILES_V1`].

/// The processor's source files, relative to `consensus/src/pipeline/virtual_processor/`.
#[doc(hidden)]
pub const PALW_PROCESSOR_SOURCE_FILES_V1: &[&str] = &[
    "processor.rs",
    "processor/palw_validate.rs",
    "processor/palw_validate/panel.rs",
    "processor/palw_validate/execution.rs",
    "processor/palw_validate/reporting.rs",
    "processor/palw_validate/court.rs",
    "processor/palw_validate/model_market.rs",
    "processor/palw_validate/class_registration.rs",
    "processor/palw_validate/bond_readiness.rs",
    "processor/palw_validate/kernel_material.rs",
    "processor/palw_validate/improvement.rs",
    "processor/palw_validate/structural.rs",
    "processor/palw_accept.rs",
    "processor/palw.rs",
    "processor/palw_h1_guard.rs",
    "processor/evm.rs",
    "processor/dns.rs",
    "processor/template.rs",
];

/// **The virtual state processor's source as one text** (see the module doc).
#[doc(hidden)]
pub fn palw_processor_source_v1() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/pipeline/virtual_processor");
        PALW_PROCESSOR_SOURCE_FILES_V1
            .iter()
            .map(|file| std::fs::read_to_string(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}")))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// **`config/params.rs` as one text** — the file and the `impl Params` parts split out of it on 2026-10-13 (`identity.rs`,
/// `validation.rs`, `activation.rs`, `mirror.rs`; its test modules are not included). Same crate, so `include_str!`.
#[doc(hidden)]
pub fn palw_params_source_v1() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        [
            include_str!("config/params.rs"),
            include_str!("config/params/identity.rs"),
            include_str!("config/params/validation.rs"),
            include_str!("config/params/activation.rs"),
            include_str!("config/params/mirror.rs"),
            include_str!("config/params/rc_genesis_bonds.rs"),
        ]
        .join("\n")
    })
}
