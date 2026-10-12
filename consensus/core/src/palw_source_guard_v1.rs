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
    "processor/palw_accept.rs",
    "processor/palw.rs",
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
