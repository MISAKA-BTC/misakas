# Legacy K/H certification aux codec correction — 2026-11-02

This advances old-G14 compatibility (stage 1) of the
[full objective](../COMPLETION-REQUIREMENTS.md). It does not assign a new
Positive P value or introduce certification/admission/payment authority.

The pre-fix [compiled-Core diagnostic](../three-roles-20261102/legacy-cert-scan.json)
uses the same zero shape-only header for empty/S/V/K/H cases. Empty/S/V
preflight succeeds, while existing K/H value types fail with
`unallocated aux row namespace`. Its source, compiler command and rlib SHA
pins are preserved; it does not prove a failure in a real accepted history.

The correction adds existing `CERTIFICATION_KERNEL_CLAIM_PREFIX_V1` to the
claim-row family and an exact `CERTIFICATION_KERNEL_HOLD_PREFIX_V1` value
scanner for the already defined reward-hold version 1. H retains its existing
338-byte Borsh order (u16, five 64-byte identities, two u64s); no prefix,
commitment, signature, hold/payment rule or policy value changes. The fixed
H heap inventory includes all eight actual owner fields. Its manual scanner
rejects other versions and every malformed extent before typed restore.
P and all unknown namespace/key forms remain unsupported. Successful syntax
inspection is not accepted-state, funding, payout or Gate C authority.

The inventory/wire generators contain the same owner registration, exact
K/H dispatch and reviewed manual adapter. `generator-consistency.json`
compares the owner fields and normalized generated adapter to the actual
source. This is targeted consistency evidence; a full unrelated regeneration
was not run.

Validation and terminal results are recorded separately. Running compilation
is not a passed test. The earlier actual three-role model measurement keeps
its exact pre-correction source pins; this codec-only correction is not
retroactively substituted into that measurement.

## Verified results and interrupted broader checks

Code commit: `27a3712686eb47588c5b6188ceda0cadf1021ad0`.
`runtime-first` failed test compilation because the new fixture called the
carriage API name instead of the standalone value API. Its failed record/log
remain. `runtime-r2` corrects that call and passes three selected tests: exact
S/V/K/H byte roundtrips and heap coverage; malformed key/body/extents/unknown
version/P rejection; and streaming refusal of an impossible nested K draw
count. The same original public preflight counterexample is then recompiled
with the corrected Core in `legacy-cert-scan-after.json`: empty/S/V/K/H all
pass, compared with K/H-only failure before the correction. This proves the
bounded codec correction, not real accepted-history/fund/credit correctness.

`exec-runtime-first` ends with SIGTERM at the 512 MiB disk floor, before any
of the selected regression tests execute. `core-clippy-first` also ends at
that floor while waiting for the build-directory lock. Neither is passed.
Source maps are unchanged. The immediate first shutdown snapshot still lists
a terminating Rust compiler; later OS observation finds both old process
groups empty before any new attempt is launched.

The shared `target/debug/deps`, separately reused native archive directory
and earlier temporary operator files subsequently disappear; available space
becomes about 133 GB. `shared-target-disappearance.json` records that state.
Two attempted transparent-compression entry points stop at missing-path
checks before modifying an artifact; no compression record/output is created.
No absent artifact is restored as a fresh Cargo success. Original source,
raw evidence and recorded before/after SHA facts remain; the historical
binary/archive paths no longer imply current availability.

## Complete fresh original regression and static validation

The original selected Cargo test sources are now rebuilt normally from locked
sources, including newly compiled RocksDB/Snappy dependencies. This uses the
dedicated fresh target, actual Apple clang/clang++/SDK, explicit bindgen
sysroot, two build jobs and functional opt0 overrides for Core/TIR/Exec.
Cargo output confirms opt0, debug assertions and overflow checks for all
three packages. There is no pinned-link override to an absent archive and no
compiler refusal wrapper. `fresh-native-inputs.json` and
`fresh-native-output-pins.json` record actual source metadata/environment and
the newly emitted archives/bindings; all current output SHA values match.
These functional settings do not qualify a performance or activation profile.

All four terminal records exit zero with unchanged source maps and empty
process groups:

- `exec-fresh-functional-first`: 11 + 4 + 24 = 39 selected outer passes; eight
  explicit gated cases ignored. Three nested sandboxed children are not added
  to that outer count.
- `core-fresh-functional-first`: all three legacy codec tests pass on the fresh
  dependency build.
- `core-fresh-clippy-first`: strict selected Core Clippy, `--no-deps -- -D warnings`.
- `exec-fresh-clippy-first`: strict selected Exec Clippy on the same source map.

[The final audit](final-source-validation.json) binds the current source and
both generators to code commit `27a371268`, verifies all 42 outer passes,
actual profile flags, native output hashes and all empty groups. Previous
compile/disk-floor failures remain failures. The earlier running snapshot
and live-process records are historical observations, superseded by these
terminal results. Temporary operator wrappers lost with the external cleanup
are not falsely claimed retained; their original full commands, source maps
and raw outputs remain. The fresh operators are saved directly here.

This completes this existing K/H bounded codec correction and its selected
functional validation. Full old-G14 accepted-history/fund/recovery comparison
and all seven Positive Court objective stages remain required. No native P,
accepted Positive writer, new funding capability, consumer credit or public
activation is implemented. The earlier actual three-role model measurement
keeps its exact pre-correction source and performance qualification.
