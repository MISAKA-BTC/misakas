# Consolidated local Source-program port plan

2026-11-01. Design/port map only, held in /tmp. Not an applied implementation patch, source freeze, compile/test result or startup-fee certification. Preserve A1 admission branch and Root execution candidate.

## Exact interface baseline

Use the proposed Core module `palw_tir_source_program_local_v1` and full public API in `/tmp/misaka-source-local-plan-2026-11-01.md`.

- SourceProgramLocalLimitsV1: max_program_bytes, max_scan_steps, max_nf_items, max_native_requests.
- LocalScanWorkBudgetV1: persistent private charged/observed counters, non-Clone, no reset/refund; passed mutably by actual node owner across attempts.
- scan_source_program_local_v1(bytes, limits, work) -> private ScannedSourceProgramLocalV1 borrowing those exact immutable bytes.
- Consuming decode(self) -> immutable LocalSourceProgramOwnerV1; readonly program()/quote() only. No supplied program/ProgramInfo/numeric quote can reconstruct a scanned owner.
- Error: fixed enum/static codes on pre-q path; legacy io/String diagnostics remain only behind their original adapter.

These are local data-accounting objects. No NativeRetained, native lease/checker/global flag, role/credit/registration authority or artifact-to-Core dependency.

## Shared-engine patch order

1. Extract the COMPLETE actual generated TirProgramV1 transitive type closure into one common grammar, using `scripts/drills/gen_rfc06_native_wire.py`. Keep tag/field/cfg/skip order and String structural-scan behavior. Do not rewrite all unrelated NativeWireScan implementations or duplicate a handwritten subset parser.
2. Add private cursor/error interfaces, fixed-error borrowed SliceCursor and legacy Read cursor adapter. Existing NativeWireReaderV1::new/Read/default limit/error messages remain as the reference wrapper. Local cursor must never construct io::Error/NativeCoverage/String before mapping a fixed error.
3. Share checked NativeWireShape merge/allocation and TirNativeRequests scalar/capacity arithmetic with distinct error adapters. Preserve first-failure ordering and the legacy named wrapper/type_name diagnostic, not a generic Self name that changes messages.
4. Share the exact nested_blob -> program_blob -> StepSpace -> standalone -> Source pipeline via private cursor/error/quote/observation ports. Retain preliminary wider program_blob arithmetic and its error ordering even when the final Source quote, rather than standalone wider q, is returned. Do not charge the wider profile as the Source profile or silently skip its preliminary failure.
5. Add the new local facade and immutable owner only after its complete pre-q path is fixed-error. Export additive module only. Keep old public/native facade signatures, all quote values, widths, native pricing and failure messages/order unchanged.
6. Tests source: differential legacy/adapted shape/quote/error order; exact and one-below caps; all four exits; samebytes provenance; malformed lengths/ranks/tags/UTF8/canonical trailing data; failed-attempt/retry accounting; original narrow/wide equality and width preservation. No execution has been authorized here.

## Cumulative scan observations

For n original bytes, L(n)=8*n+4096 is the existing intrinsic step limit. step() increments BEFORE its over-limit error, so each possibly failing reader is charged L+1.

    charged step envelope = L(n+4) + 3*L(n) + 4
    charged reader bytes  = 4*n + 4

Use checked arithmetic and admit the whole envelope before any pass; never refund it on malformed data, resource error or decode/NF failure. Keep actual observed counters separate from admitted worst-case counters; do not double-debit them. Record nested child observations BEFORE propagating its error, and do the same for every outer/later pass. Observed bytes are actual initial_remaining - remaining; refused skip lengths are not consumed. Fixed-width partial EOF behavior must be specified and tested separately from skip prechecks so the local cursor does not claim unread bytes as work. Counter updates must not leave a half-updated aggregate on overflow.

The same actual node owner retains the work budget across retries. A copied quote or reconstructed budget is not a prescribed bypass. Budget counters are local data, not authenticated network accounting.

## Native/header coexistence boundary

Reuse the existing Source program+normal-form typed q, not a codec-size-to-RAM multiplier or global standalone native capability. Add actual local owner/quote/budget/temporary header requests using checked typed sizes and documented overlapping lifetime. Retain private scanned provenance.

A3/node startup must explicitly retain and quote raw-input/Arc, bounded artifact-header owner, parameter maps/plan, resident weights, V2 space and Loaded owner coexistence with program decode. Core program-only q does not cover them. Artifact keeps no Core dependency; node startup is the glue that owns both typed quotes. Do not expose a public shape/count-alone constructor or call any sum of these inputs an all-startup permit.

## Remaining readiness blockers

- Scanner static-error extraction is not implemented yet. A fixed public error enum alone is insufficient while internal io::Error/NativeCoverage/String allocations remain.
- Borsh decode -> canonical re-encode -> normal-form validate still allocates error Strings. Prove their exact owner/format/clone coexistence is contained by admitted Source q before claiming bounded decode; scanner staticization does not establish this.
- Scan steps and actual NF structural-item cap do not establish total NF CPU, subsequent compile/plan CPU, physical RSS or OOM recoverability. Those remain separate audits/guards and must stay explicit in API docs and A3 glue.
- Old legacy quote/error/type_name differential checks, controlled malformed pre-q allocation observation and strict typecheck/test execution remain pending grants. No source freeze or actual local decode readiness is claimed here.

## Provenance

A1 design input SHA256: acc5480920ab9df3a57044de9e3d9c5a58bbe1fd965bd4e2243e89db42cd16d4
A0 positive_roles input SHA256: b21af78449e864d891350884bd1fc3d9456b727a6ff05c5ec60aa018a25d0a84
No repository source, Root candidate, Cargo target or cache is modified by this consolidation.
