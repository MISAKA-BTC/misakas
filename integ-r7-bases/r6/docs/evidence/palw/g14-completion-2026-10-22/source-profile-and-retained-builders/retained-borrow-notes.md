# Retained semantic material → existing private native builders

Apply only `semantic-retained-borrow.patch` (cumulative final candidate); `tokenizer-retained-borrow.patch` is an earlier partial artifact. Repository untouched, apply-check passed, no compilation/test execution.

## Real contract preserved

Constraint originals can legitimately register only a table: `raw_tokenizer_bytes=None`, no rooted raw EOF. Requiring the first full raw owner there would invent a new prerequisite. Candidate adds a distinct opaque `SemanticTableMaterialV1` with no raw field or EOF ID. It contains actual current source/claim/scope/immutable semantic table binding and all actual ID openings + EOG. No fake/zero/default EOF is constructed. Common allocation-free table scanner, checked output quote, actual table decode/authentication and current native context/Source prelude are factored and reused by full raw+EOF owner. Full raw owner preserves its existing EOF and flat-hash checks and allocation/lease semantics. Table counts.raw_bytes=0 denotes no raw payload in this separate owner type, not authenticated empty tokenizer bytes.

## Actual builder connection

`build_constraint_terminal_retained_v1` and `build_tokenizer_terminal_retained_v1` borrow NativeRetained payloads directly and return the existing primitive evidence tied to that borrow lifetime. No raw/rendering copies or unleased extraction. Existing signed intent/material identity/current source/current actor/rules/root/EOF checks and full replay/recheck remain in the original builders. Tokenizer proof producer payloads remain separately borrowed/admitted. These adapters do not reserve existing builder reconstruction/signature/intent-clone/parser/evidence scratch; that previous caller contract remains explicit. No financially paid entry, new retention/wire/price/cap, Complete flag or original guard change.

## Meaningful tests

- Expanded `shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks`: actual existing registered constrained program+real signed semantic custody bundle and native shared source; explicitly no raw semantic length/no EOF. Table-only owner positive, missing ID refusal, same signed intent existing evidence/recheck comparison, wrong signed scope refusal.
- Expanded `shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material`: actual original+rooted EOF; full retained owner into existing target parser/evidence with same signed intent; result equal to original borrowed path; changed signed EOF/scope/out-of-vocab target and missing compact proof refusals.
- `semantic_table_only_owner_quota_has_no_raw_or_eof_obligation`: generated real table auth with existing native checker exact quote hold/second-reservation refusal/drop release. Local Scope fixture is not native state provenance; integration above covers current state binding.
- Existing full raw owner negative/scanner/quota tests continue through factored shared table code.

None of these claims full original admission, all target/parser work closure, arbitrary proof portability, global supply/availability, or funded settlement. Evidence lifetimes preserve the material ticket; separately owned proof/Store/source inputs need their own tickets. The old builder remains recheck authority.

## EOG scope

The shared owner retains the existing native EOG stream's nonempty/in-range requirement. Existing canonical table file decoder (`palw_fp_constraint_job_v1.rs`) already rejects empty/out-of-range EOG, and existing Constraint primitive needs nonempty EOG. Root snapshot authentication alone does not prove that unseen malicious table bodies satisfy this canonical contract. This candidate does not claim to close every malformed-table original challenge branch; existing per-target Tokenizer primitive remains independent. No new empty-EOG rule is adopted here.
