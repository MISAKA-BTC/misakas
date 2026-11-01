# Loaded → existing Root wide Panel one-pass bridge (2026-11-02)

DRAFT SOURCE ONLY, /tmp only. No actual A3/Root checkout, Core, Cargo/rustc,
compiler/test binary, model, default backend, NativeRegistry or commit was changed
or executed. Rustfmt parsing and synthetic-union git apply --check are the only
checks performed. Two new test sources are UNRUN, not pass evidence.

Files:
- /tmp/a3-loaded-wide-panel-bridge.patch
- /tmp/a3-loaded-wide-panel-bridge.rs (new producer bridge file contents)
- /tmp/a3-loaded-wide-panel-bridge-tests.rs (new cold tests)
- /tmp/a3-loaded-wide-panel-bridge.notes.md (this file)

## Baseline / exact dependency distinction

A3 inspected Loaded baseline:
/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet,
90877a99f, module node/owned_artifact_params_v2/loaded.rs. Its pre-existing
Source/typed header/producer compile/owned native params dependencies must also
be present. Current A3 branch is the OLD45b-derived integration baseline and does
NOT contain the Root R5 Panel producer. Do NOT apply partial stale I32 code and
infer an absence of current wide A1 support from this checkout.

Authoritative producer inspected read-only:
/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet at e2d5a43e8,
with Root1c90300a4a8ff8f65b13e70c18c37856d9f0145f as the earlier R5 evidence base.
The capture/Work/runner/Builder signatures used by this bridge are IDENTICAL at
1c903 and e2d; git diff over those modules changes only claims_tape.rs.

Minimum production stack is the FULL dependency ancestry of:
- 9689b33692e35140bbfa5e333a99cd972dd37687: explicit wide bounded runner.
- 4753ee4beae5e245095954c6ef6e9b96fa05f361: authoritative Core wide expanded
  geometry / source-only full-relation independent checker.
- 10d52e6e9: actual one-pass wide Panel tape/material producer and runner hooks.
- cc22b0da1: checked collector arithmetic/initial retained diagnostics.
- d19d2800fe040d67a149f01f2e096e1662758523: required same-ledger capture/run RAM
  guards and separate per-material limits. Already in Root1c903.
- Loaded90877a99f plus its complete Source/header/compile/params ancestry.

The PRODUCTION bridge itself requires no later stream API. Its new cold integration
TEST explicitly uses the newer e2d stream path from:
- a93fee855812a2474ffd717e88444236736568c8: claims_tape.freeze_record / rewind /
  ClaimsReaderV2 and Core begin_stream with deferred independent Node auth.
This dependency is intentional; test source must not be called type-checked on
1c903 alone. Main/A0 must combine reviewed source stacks on their authorized branch,
not copy private/stale type stubs or edit A1 independently.

## Formal boundary: existing claim, NOT first-Miner startup

Loaded908 constructor REQUIRES an ALREADY EXISTING PalwTirStepBindingV1 including
trace root, step root/count and execution commitment plus opaque verified V2
binding and the original independently admitted context. This bridge keeps that
requirement. It neither zeros, suppresses nor synthesizes a fake production
binding. It is useful for existing-claim Panel replay/verification/measurement.
Ordinary trace-free first Miner forward, a new Miner Result, native job assignment
and native backend/registry activation are NOT implemented. Root's separate
metadata-only owner plan for first forward43/44 is outside this patch.

The cold tests derive REAL existing fixture commitments from already held original
fixture observations before candidate file creation. No61/62 placeholder roots,
zero trace, context prompt-hash repair, MaxContext change, tokenizer equality,
extra model replay or new registered class is introduced. Existing fixture setup's
reference oracle is unchanged; only the production run guard count is asserted1.

No cryptographic computation proof is added. G14 roots/openings authenticate data;
Panel claims remain untrusted until the independent A1 checker recomputes them.
Local matching/record/signature data does not grant consensus credit or replace
claim-level/native verifier selection/approval. No model-level review occurs.

## Current public paths / actual gap

Loaded currently exposes prepared_cell(), serve(), resident_open() only.
ResidentWideOpenV2 can send native params/program to a bounded Hello2 process but
has no public conversion to an authoritative WidePanelCapture or full-run/tape
producer. Rebuilding another public TirPlan or intercepting wire copies is not an
existing zero-copy producer adapter. Generic IPC execution alone therefore does
not make the private Loaded source usable in the existing one-pass Panel pipeline.

Authoritative Root already has the needed producers:

    node::positive_wide_material_v2::{
        CaptureExpectedV2, WidePanelWorkV2, capture_one_pass_v2,
        WidePanelBuilderV2, WidePanelOwnersV2, WideProviderEnvelopeV2
    }
    WidePanelBuilderV2::claims_tape(File)
    WideClaimsTapeV2::freeze_record(...), rewind(), ClaimsReaderV2
    Core FullRelationPreparationV2::{begin_stream, ...}

The8 original Root tests cover actual wide one-pass captures → materials → A1
including wrong Panel/Miner outcomes. New stream tests detach bounded claims and
preserve independent Node evaluation. Those are pre-existing Root evidence, not
new tests executed by this task. Old A3 narrow r1 CoveragePlan/I32 refusals remain
unchanged; no wide value/root is squeezed through those legacy interfaces.

## Minimal source changes

1. Loaded existing file: only cfg(feature="positive-panel") mod panel declaration.
2. New loaded/panel.rs: public methods on the EXISTING private owner:

    loaded.wide_panel_work(limits:WidePanelLimitsV2, disk:&WidePanelDiskReserverV2)
        -> Result<WidePanelWorkV2, LoadedWideErrorV2>
    loaded.capture_wide_panel_one_pass<'run>(
        &'run self, prompt:&'run[u32], run_limits:TirWideRunLimitsV2,
        work:&WidePanelWorkV2, tape:File, reserver:&TirMemoryReserverV1)
        -> Result<WidePanelCaptureV2<'run>, LoadedWideErrorV2>

3. Root WidePanelWork implementation: one cfg(positive-panel) pub(crate) helper
   admit_loaded_runner_v2(floor,class,context) validates existing immutable limits,
   original fixed scope and one-use state, then charges one extra existing metadata
   traversal before runner.new's structural equality. No public mutable/private
   Work fields, new session ledger, reset, refund or scalar authority is exposed.
4. Test crate: cfg already positive-panel; one child-module declaration + two
   UNRUN tests in positive_wide_material_v2/loaded_bridge_tests.rs.

No existing run/capture/claims/proof/Node algorithm, q formula or error ordering
inside those producers is modified. The local runner is BORROWED, not retained,
Arc'ed or cloned. It borrows self.compiled.plan()/self.params internally and
self.inputs.verified.space(). Only the current immutable class/artifact/context/
space pins are passed into CaptureExpectedV2. No public raw&TirPlan/TirParams,
public arbitrary plan closure or private source extraction is supplied.

No plan compile, parameter clone, second CPU run, new program encoding, new full
parameter byte source or all-i128 model mirror occurs in the bridge. Existing
producer identity hashing/streamed comparison and bounded native/tile conversion
remain exactly in the existing quoted producer/required guard paths.

## Ownership / additive floor / counters

Loaded.quote.operation_retained_bytes is the WHOLE prepaid startup allowance plus
its original external tickets/prior claims, already conservatively stronger than
actual retained component backing. wide_panel_work adds that known floor using
checked arithmetic to the supplied limits.external_retained_bytes. In THIS new
convenience method, supplied external means ADDITIONAL caller ownership: actual
prompt capacity/backing, provider/session/callback controls, preparations,
previous exports, record/signature owners and other independent live data.
No supplied maximum is raised; impossible floor>max_live_heap fails first.

Callers may instead use existing WidePanelWorkV2::new manually; capture method
checks its already configured absolute external envelope >= Loaded floor. It
also requires matching original class/context Hash64 scope, never a freshly
hashed replacement execution. Root Work's execution field is the original JOB
CONTEXT hash; it is not binding.committed_execution_root. Both original identities
are kept distinct, no old root meaning changes.

Work must be constructed ONCE by the independently admitted Panel attempt owner
and RETAINED/SHARED across capture, Builder, proof/material, tape and failures.
Clones share Session counters and started Cell. Neither this convenience nor the
existing Work constructor grants native funding or a permission to reset an
actual service/role budget by creating another local handle.

The additional prefix charge is the SAME existing Root metadata-visit currency
128*(MAX_PROGRAM_BYTES+1024), for ONE additional structural comparison in the new
borrowed runner constructor. It is charged before that constructor; it is neither
a new RAM factor nor CPU-instruction/time assertion. A too-low floor or wrong
scope refuses before this comparison. An already started Work refuses before the
prefix so repeated calls preserve the exact spent meter. Prefix refusal/runner
failure refunds nothing accepted; before CPU start any retry continues spending
the same finite visit balance. Existing capture sets started before its callbacks,
so success or an accepted failed tape/capture cannot start a second CPU run.

capture_one_pass_v2 still owns the only actual run_observed_fallible invocation.
It acquires the mandatory capture RAM lease and passes the SAME required reserver
to the runner's mandatory run lease. No optional-global fallback or duplicate run
reservation is added. Both guards remain in the returned capture/run. Returned
WidePanelCapture<'run> borrows Loaded and prompt, so Loaded's startup owner/tickets
cannot be dropped while that capture/Builder still needs the native source.
The temporary runner does NOT escape; no self-reference is created.

Original Loaded reservation may be from its separate startup callback. The new
capture's required ledger additionally counts its known Loaded external floor;
no aliasing/shared-ledger ownership proof is assumed or inferred. Parent/native
provider callbacks/errors and independent copies remain externally bounded by
their actual owners. Backend String errors are dropped by the bridge before a
fixed LoadedWideErrorV2::Prepared escapes; numeric overflow/limit remains fixed.

File is never reopened, read or restamped by the bridge. It uses the already
owned authenticated native params, which source-file mutation AFTER startup
cannot alter. Startup same-descriptor/mutation assumptions remain unchanged.

## G14 opening/provider contract remains independent

WidePanelBuilderV2::prepare still accepts the independently bounded existing
TirParamOpenerV1 and its explicit per-call IO/work/result/scratch envelopes. A1
still authenticates those openings against the canonical artifact root. This
bridge does NOT invent a new provider or silently construct TirHeldInventory from
TirParamsSource (which re-encodes whole instances). Source custody/authentication,
provider retained/scratch/work and subsequent record/signature/Node stage guards
stay explicit. A2/service-authenticated cached openings can satisfy that existing
provider contract without exposing Loaded params or rerunning a model.

The new cold tests use the SAME existing controlled small-fixture inventory
provider as Root's tests, externally reserved, independently of Loaded's native
source. Their test-only container writer serializes those original native params
before startup; this is not a production bridge API or an all-model mirror.

## Two actual new test sources — UNRUN

1. Existing correct Root window1/window3 fixture, unchanged class18/job0/seed/
   MaxContext/prompt commitment. Real existing step/trace/execution binding is
   computed only from already held fixture observations then opaque verified,
   BEFORE PALWTIR1 candidate creation. Native params are written once; File→Loaded
   uses full actual guarded startup. New Work adds Loaded floor; capture delegates
   the SAME private source once. Assert capture binding equals original existing
   binding, generated literals, startup/capture/run guards live and exactly ONE
   required production CPU-run reservation. Shared retry refuses unchanged meter.
   Existing Builder→claims tape→stream-record signing→INDEPENDENT A1 begin_stream→
   every actual material/relation→NeedEof→finish Matched. No all-claims Vec<Vec>
   materialization. Record/Node exports are under an independent guard and the
   stream external term includes the entire producer live cap plus test exports.
   Dropping capture/run releases their guards while Loaded's guard remains live;
   Loaded final Drop releases its startup guard.
2. Manually created shared Work with absolute external exactly LoadedFloor-1
   refuses BEFORE capture/run reservations and leaves meter unchanged. Correct
   Work + nonempty tape fails after accepted metadata prefix/capture guard but
   BEFORE CPU-run guard. Spent visits/one-use state survive via shared Work clone;
   retry does not call another guard or run. Loaded guard is still held.

These tests are sources only. No success/guard/Node/RSS result was measured.
Unknown Core supported-build profile prints an explicit refusal label and does
NOT claim this cold path ran. The original8 Root tests/stream tests are unchanged.

## Applicability and remaining boundaries

No single inspected checkout initially contained BOTH Loaded908 and Root R5.
applycheck PASS is ONLY against /tmp/a3-loaded-wide-panel-bridge-applycheck, a
synthetic union of the THREE unmodified baseline files (Loaded from A3, Work and
test parent from Roote2d), plus intended new files. It is not a claim that current
A3 lacking R5 compiles or that Root lacking Loaded was modified. A0/main must
combine the reviewed stacks before real apply/compile/test authorization.

This bridge does not add a native Result/witness wire body/version dispatch,
registration, assignment, Case/Service completion or consensus credit. A1's
independent checker remains separate from the producer. Global flags/default
backends/old IDs/legacy4-byte roots are untouched. Fixed greedy Flat is the existing
implemented contract; FP/V4/device/general-source paths are not asserted here.
Scan/NF/compile/IO/OS scheduling/RSS and arbitrary exported-alias ownership retain
their existing explicit limits; none is silently completed by this delegation.
