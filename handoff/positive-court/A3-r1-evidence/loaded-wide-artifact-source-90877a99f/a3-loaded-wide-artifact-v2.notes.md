# LoadedWideArtifactV2 — concrete source draft, /tmp only

Status: UNAPPLIED and UNRUN. No Cargo, rustc, model execution, allocator/RSS
measurement or production startup result is claimed. Root's existing owned
parameters source is the base. The independent producer compile quote patch
`/tmp/a3-startup-plan-quote.patch` is an explicit stacked dependency.

## Concrete ownership/API

`node::owned_artifact_params_v2::loaded::{LoadedWideArtifactV2,
LoadedWideInputsV2, LoadedWideLimitsV2, LoadedWideStartupBudgetV2,
load_wide_artifact_v2}`. The input File is taken by value; no path reopen,
try_clone, mmap, lazy source, Tensor/full-i128 mirror or model rerun is used.
The private owner moves in the original File, bounded header, immutable Core
Source owner, opaque private compiled owner, owned native TirParams, actual
G14 root, quotes and one optional existing Node host lease. Input references
are ONLY caller-owned independent expected binding, opaque verified V2,
original context, and an explicitly retained external ticket. No self-reference
or borrowed local header/plan survives the constructor.

The existing Params public function is factored without changing its order:
start → quote → budget.admit → private admitted loader. Loaded's child can
call that private seam AFTER both the Params budget and aggregate startup
checks. Its temporary borrowed wrapper is privately destructured to move the
native values, root and quote; no public move-out constructor is added.

The expected/verified/context association is a CALLER ADMISSION PRECONDITION.
It is not reauthenticated from candidate metadata, and there is no unquoted
class/context CanonicalWriter hash added to this constructor. The whole Loaded
owner retains that same external association/ticket through use. Literal class
program/layout/registered-tokenizer checks and expected actual-values root use
the existing Params checks. Empty header layout remains undeclared; only the
independent expected original layout is used. Registration tokenizer18 stays
independent from actual job tokenizer0. No model eligibility/review/credit or
native activation follows from any identity match.

## Bounds and no-refund sequencing

One optional existing `TirMemoryReserverV1` callback is acquired BEFORE header
backend work, for the finite configured `max_new_owner_bytes` allowance. It
is retained last in the returned owner and is not fabricated when absent.
Provider guard allocation/bookkeeping/error bounds remain the caller's existing
external contract. The logical caps do not become host/native/global authority.

Every accepted startup attempt atomically preclaims that entire allowance,
including FAILED attempts; there is no reset/refund/Clone. This stronger policy
avoids guessing which earlier owners/errors/exports are still alive. Cumulative
attempt/read/work/retained caps and persistent Core scanner/Params counters
are kept in one owner. Each phase is precharged before its corresponding walk,
read, copy or compile. Currency sums existing header-parser work, scanner steps,
compile quote visits and Params metadata/data work, not all NF/compile CPU.

Named co-live stages:

1. aggregate typed root + configured header frame/native + existing parser
   SCRATCH_BYTES + actual error-root/literal census, plus independent external
   owners and prior startup allowance claims;
2. actual header native + aggregate root + configured Source native ceiling;
3. same bounded header diagnostic envelope + Source retained envelope during
   split literal table validation;
4. producer compile operation quote, which automatically includes original
   Source envelope, one additional Source revalidation envelope, compiler
   retained data/scratch, plus actual header/root/external/prior claims;
5. header + Source + compiler retained + whole new Params peak and prior claims.

Every stage checks max_new_owner and max_operation BEFORE its allocating
backend; sub-operation maxima are never raised. Nested external fields must be
zero at budget construction and are derived explicitly. Params prior accepted
retained claims are subtracted only from the already counted prior startup
allowances, preventing duplicate external inventory while preserving all
current owners. The actual quote retains component full inline roots in
addition to the aggregate root: explicit conservative duplication, not a tight
stack-layout claim or a guessed byte multiplier.

## Header diagnostics and supported profile dependency

The separate minimal Core public `require_source_program_local_profile_v1()`
checks the existing PRIVATE build result without scanning, allocating or
consuming work; Source itself continues independently checking the same gate.
Loaded requires it BEFORE metadata/header backends. This is a new proposed
public facade dependency, not an already integrated Core API or caller token.

Artifact's new `bounded_v1::startup_quote` is pure fixed-error arithmetic with
no Core dependency. It enumerates all48 current static bounded_v1 diagnostic
literals (maximum56 bytes), the actual error enum root, existing parser scratch,
configured maximum native owner and capped possible frame length. Owned
Header/Table/Program errors copy exactly one static str; they never interpolate
untrusted values or format input. No private std Custom layout is guessed.
The helper is RESTRICTED to native regular std::fs::File under the required
Core audited std profile plus immutable same-byte canonical Source program.
File read/seek failures are inline OS/constant EOF errors; arbitrary Read or
provider errors are excluded. Same-byte validated Borsh object_length/Compare
cannot fail or manufacture a Custom diagnostic on this route. Reusing this
helper for arbitrary mismatched programs/Read would require a separate census.
The helper still never raises caller header max_operation/max_work caps.

## File and public prepared bridges

Native File stamp checks dev/ino/len/mtime/ctime before/after, refuses nonregular
files, and operates on the same supplied descriptor. Caller excludes concurrent
writers and descriptor-offset aliases. Timestamps are supplemental consistency,
not an atomic snapshot or authentication. Actual already owned values still
must match independent G14 root. Later file mutation does not alter native
resident values; `recheck_file_stamp()` only reports current metadata mismatch.

No public raw TirPlan getter exists: public OnceLock lazy fields cannot be
reached from Loaded. prepared_cell checks external floor BEFORE existing
PreparedWideCellV2::new and its context hashing. That floor is the WHOLE prepaid
startup allowance plus original external/prior claims, stronger than actual
retained typed component bytes. ResidentWideOpenV2 is built from the same private
plan, original header bytes/native params and derived floor, never a caller's
underdeclared number. The public serve bridge calls the existing session to
retain mandatory cumulative work/step/IO accounting. Direct prepared CPU methods
remain existing low-level methods requiring caller admission. Workspace,
transport/deadline adapters, copied messages/exports and callbacks still require
separate caller reservation. No blocking transport default, process group or
kill/wait walltime guarantee is introduced.

## Verification and remaining integration

Source formatted with rustfmt only, patch passes git apply --check against the
current already-applied owned-Params tree. Compile-quote public symbols are the
coordinated producer API, not placeholders. Test additions will be a SEPARATE
stacked patch so production review can remain frozen. No unsupported-success
stub, unchecked mutable constructor, authority, default backend or CLI route is
added. Root must independently review and schedule any integration/runtime tests.

## Independent-review deltas and separate UNRUN tests

The borrow-only checked header is left to NLL; no `drop_non_drop` suppression or
explicit drop is used. Terminal serve now returns fixed-size
`LoadedWideServeErrorV2 { kind, usage }`: accepted upstream connection usage is
copied before dropping the owned diagnostic locally. Admission failures before
session entry report explicitly zero usage. This preserves journaling/retry
accounting without allowing a backend String to escape.

`/tmp/a3-loaded-wide-artifact-tests.patch` is stacked AFTER production, with six
UNRUN tests (484 lines):

- actual regular temporary File → bounded header → same-byte Source → private
  compiler → native owned params/G14 root → Prepared ordinary CPU step, exactly
  `[i64::MIN,i64::MAX]`; signed-I8 and high-Idx literals are original native inputs;
- whole optional host guard remains held while data/prepared/cell borrows are
  live and after they end, releases only with the data owner; no global setter;
- registration18/actual job0 plus empty-vs-declared original layout and original
  encoded program/context remain unchanged;
- header, Source, compiler and Params refusals preserve each accepted startup
  allowance/work/read claim, release guard and return no partial owner;
- bad actual weight root retries retain two full accepted read/work/claim charges,
  then attempt cap rejects without another guard; truncated extent fails split
  table validation before any native payload is admitted;
- too-low bridge external floor refuses with untouched transport/zero session
  usage; literal duplicate-Hello2 frames cause terminal failure whose copied
  usage exactly matches existing server and pins read26/steps0/written length.

Tests use independent builder/literal payload/root setup BEFORE candidate file
writing. The finite8-MiB declared external TEST allowance explicitly includes
binding/opaque space/context, program/layout/payload copies, TempDir/path,
counters/callbacks/budget and response buffers. It is not measured RSS or a
production byte factor. Unsupported-profile tests explicitly label refusal and
never claim the full startup/CPU path ran. No test, Cargo or rustc was executed.
The compile quote dependency is now already in the parent tree; combined
production+tests git apply --check succeeds there. A combined check redundantly
including the already-applied dependency correctly does not apply that dependency
again; no source was changed by either check.
