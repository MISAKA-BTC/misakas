//! # misaka-palw-extension — ADR-0108
//!
//! **A person who made something — with a model, a converter, a drill, or by hand — writes ONE
//! manifest that says what it is, what it depends on, and what recomputing it must produce; any
//! node's verifier recomputes it and answers with exactly one of three classifications — *expressible
//! now* (the chain already has the object; admission is permissionless and the manifest says which
//! object), *a node extension* (the chain never sees it; a node that lacks the code answers
//! "unverifiable here", never "valid"), or *a ruleset change* (a fence; the verifier says what
//! fingerprint the arming build would print and whether arming it is a flag day, and activation
//! stays a coordinated release); and a receipt of that recomputation is evidence someone else can
//! reproduce, signed by whoever ran it, that no consensus path reads and no count of which admits
//! anything.** The chain keeps naming identities it can recompute. The manifest is how a stranger
//! learns what an identity means. Nothing about the generator of the thing — a person, Claude, GPT,
//! Kimi — is recorded, asked, or trusted.
//!
//! ```text
//!                      fixed verifier: this build's consensus-core + SDK + derive
//!                                           │
//!         ┌─────────────────────────────────┼─────────────────────────────────┐
//!         │                                 │                                 │
//!    A. EXPRESSIBLE NOW              B. NODE EXTENSION                C. RULESET CHANGE
//!    the chain has the object;       the chain never sees it;        a fence; accept/reject or a
//!    this build recomputes it        a build must carry the code     state root moves
//!         │                                 │                                 │
//!    permissionless admission        permissionless on chain,        a coordinated release:
//!    (existing object, existing      verifiable only where the       ADR-0072 SA-2's gate, ADR-0105
//!    gate, existing fee)             code is; SA-5 says publish      §7's notice, one host at a time
//! ```
//!
//! The fourth answer, `Refused`, is for a manifest that is wrong about itself — an id that does not
//! recompute, a bound that is zero, a vector whose expected root the recomputation does not produce.
//! There is no bare pass.
//!
//! What this crate is NOT: consensus. No object, acceptance rule, fence, parameter or fingerprint
//! moves for it; no code path in `kaspad`, `kaspa-consensus`, `kaspa-consensus-core` or the SDK
//! reads a receipt (ADR-0108 I-9 — `cargo tree -i misaka-palw-extension` names only the CLI).

pub mod kinds;
pub mod manifest;
pub mod receipt;
pub mod report;
pub mod verify;

pub use manifest::{
    PALW_EXTENSION_ID_DOMAIN_V1, PALW_EXTENSION_MANIFEST_V1, PALW_EXTENSION_MAX_CANONICAL_BYTES, PALW_EXTENSION_MAX_FENCES,
    PALW_EXTENSION_MAX_VECTORS, PALW_EXTENSION_RESERVED_KINDS, PalwExtensionAdmissionV1, PalwExtensionArtifactV1,
    PalwExtensionDeclaresV1, PalwExtensionError, PalwExtensionKindV1, PalwExtensionManifestV1, PalwExtensionRequiresV1,
    PalwExtensionSourceV1, PalwExtensionTransformerV1, PalwExtensionVerificationV1, PalwFenceRequestV1, PalwParsedManifestV1,
    PalwTransformerVectorV1, canonical_json, extension_id_v1,
};
pub use receipt::{
    PALW_EXTENSION_RECEIPT_ID_DOMAIN_V1, PALW_EXTENSION_RECEIPT_MLDSA87_CONTEXT, PALW_EXTENSION_RECEIPT_V1, PalwExtensionReceiptV1,
    PalwExtensionReceiptVerifierV1, PalwReceiptVerdictV1, receipt_id_v1, sign_receipt_v1, verify_receipt_v1,
};
pub use report::{
    PalwExtensionCheckV1, PalwExtensionClassificationV1, PalwExtensionDepthV1, PalwExtensionOutcomeV1, PalwExtensionReportV1,
    PalwExtensionServingV1, PalwFenceDeltaV1, PalwWouldPrintV1,
};
pub use verify::{
    PALW_EXTENSION_GENESIS_TERMS, PalwExtensionEnvV1, genesis_registration_terms_v1, params_for, verify_extension_v1, verify_parsed_v1,
};
