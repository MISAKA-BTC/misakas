# Minimal Source diagnostic bootstrap decision — draft

2026-11-01. /tmp only. No Core source/Root candidate/Cargo/cache change. This is a reviewable arithmetic fragment and implementation plan, not a ready public API or a completed allocation/CPU certification.

## Selected minimal direction

For ONLY the fixed borrowed-memory source_tir_program_shape_v1 call graph, prefer an audited typed diagnostic bootstrap before invoking the unchanged scanner. Preserve current grammar, preliminary wider quote arithmetic, errors and ledger/native pricing. Keep the shared-static-error grammar plan as fallback if the finite-branch containment or supported-build assumption cannot be established.

This narrows the problem to a finite known Rust1.93 diagnostic vocabulary and real typed backing. It does not generalize to arbitrary Read/custom Error input, replace the post-quote NF diagnostic audit or authorize startup/registration/credit.

## Concrete review files

- Arithmetic fragment: /tmp/misaka-source-diagnostic-bootstrap-draft.rs
- Unapplied fragment patch: /tmp/misaka-source-diagnostic-bootstrap-draft.patch
- Pure Vec-policy dependency: /tmp/a3-public-vec-envelope.patch (A3 owner draft, not applied by A1).
- Original exact Scan/Owner/work API: /tmp/misaka-source-local-plan-2026-11-01.md
- Shared grammar fallback/observation ports: /tmp/misaka-source-local-consolidated-2026-11-01.md
- Diagnostic inventory: /tmp/a3-source-diagnostic-audit.md

The fragment uses checked scalars and fixed local errors only. It constructs no io::Error, NativeCoverage, String, Vec, checker or native lease. It conservatively counts an outer failure header separately from NativeDecodeError<String>, concrete IO header, padded pinned Custom backing, boxed StringError header, growing buffers and all finite Resource failure branches. Component/type combinations are enveloped by MAX, never a sum over mutually exclusive failures.

## Mandatory unfinished wiring

1. A supported Rust1.93 build-profile detector/gate must select the private profile. The fragment enum itself is NOT that detector; no artifact/peer-supplied boolean or profile may bypass it. Unknown builds refuse with a static error before scanning. Actual sizeof/alignof/type_name lengths are used; hardcoded observed object sizes are forbidden.
2. The local facade must admit diagnostic floor + actual local headers and retain that logical floor with every escaping owned error/program owner. Zero/insufficient/overflow/unsupported cases return nonallocating errors. No global reserve_native_requests/NativeDemandChecker/NativeRetained is used.
3. Add private observed thin ports to existing nested/program/StepSpace/standalone/Source functions, preserving legacy wrappers and first-error order. Charge four reader maxima with the failing increment: L(n+4)+3L(n)+4. Retain actual per-pass steps and consumed bytes before error propagation; skip-refused bytes are not read. Never refund failed attempts/retries or double-debit observations.
4. Audit Borsh/reencode/NF diagnostic ownership against the existing Source q AFTER scan. The current four-writer formula is not established by this bootstrap fragment; public consuming decode must not claim complete coverage before that audit.
5. Scansteps/NFitems are not all-NF/compile CPU. A3 node owner separately supplies and retains typed Arc/header/Loaded/plan/map/weight/V2-space coexistence budgets. Core program-only quote is not all-startup cost certification.
6. Differential old quote/error/order/width checks, minimal-one-below budgets, malformed path allocation observations and execution remain unperformed. Root owns builds; A1 stays on hold.

## Primary sources for pinned layout assumptions

- https://github.com/rust-lang/rust/blob/1.93.0/library/std/src/io/error.rs (Custom fields/alignment, Error::_new, static EOF)
- https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/boxed/convert.rs (single-String boxed error)
- https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/string.rs (generic Display conversion owns a separate growing String)

No source code from these files was copied into the fragment; only their concrete ownership/layout facts inform the typed envelope. Repository toolchain pin is exact1.93.0; overriding it needs a fresh supported-profile audit.

## Frozen Vec-policy dependency update

A3 public helper source is now fixed at commit `9c010109dc56f51808c51dded67c4b93df10fa2c`, tree `622da67b22c6aa3b6f5c84670f8a4297fd3d7339`; source-only evidence is `89b1d6346`. Exact API is `kaspa_consensus_core::palw_native_resource_v1::growing_vec_request_capacity_v1<T>(count: u64) -> Option<u64>` (pub const fn). This replaces the draft-only dependency reference as the review baseline. A1 has not applied it to its preserved admission branch or Root, run Cargo, or accessed a build target/cache. Two new helper tests remain unexecuted. This dependency does not resolve NF diagnostics, build support, owner/error lifetime or whole-startup budgeting.
