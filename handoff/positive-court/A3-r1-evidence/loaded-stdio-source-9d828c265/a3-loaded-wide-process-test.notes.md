# Loaded actual-child join — TEST ONLY, source draft

Status: /tmp ONLY, UNAPPLIED and UNRUN. No Cargo/rustc was invoked. The base is
actual Loaded/Source/compiler/owned-Params freeze90877a99f. A0 authorized the
harness=false/node-required manifest target as normal validation scope, subject
to source freeze, independent review and the continuing shared Cargo HOLD.

Patch: `/tmp/a3-loaded-wide-process-test.patch`, EXACTLY TWO repository files:

1. Exec Cargo.toml: eight lines, explicit [[test]] named
   `loaded-wide-process-v2`, `harness=false`, `required-features=["node"]`.
2. Exec tests/loaded_wide_process_v2.rs: standalone OS-main parent/child test
   executable,723 source lines, no production API/default/backend change.

The existing six Loaded unit tests are untouched. This source is not a new
SDK loader/registry route, whole Court/training-model integration, model review,
computation certificate, native admission or consensus-credit decision.

## Why an explicit test target

The current libtest executable always emits its preamble/footer to stdout. The
existing Hello2 integration therefore uses a separate TCP transport; it cannot
be substituted for WideCellProcessV2's actual ChildStdin/ChildStdout ownership.
This target supplies its OWN OS main, dispatching current_exe into the constant
`--a3-loaded-wide-child-v2` plus parent-controlled environment marker. It never
strips/discards libtest bytes, uses fake replies or treats a socket as those
pipes. No shell, unsafe fd2 restore or pre_exec hook is used. Production's
stderr-null setting is preserved.

THIS USES THE OS MAIN STACK IN BOTH PROCESSES. It must NEVER be counted as a
Rust test-thread/default-stack PASS. The dedicated parent success marker
includes `stack=os-main`. Unknown Source profiles exit77 with an UNSUPPORTED
marker; dispatch refusal/missing child markers/failures never emit PASS.

Future validation command (NOT RUN):
`cargo test -p misaka-palw-tir-exec --features node --test loaded-wide-process-v2`
This custom harness takes no libtest CLI flags; do not append `-- --nocapture`
or `--exact`. A0 remains responsible for actual scheduling and evidence.

## True parent and child source paths

Both independently construct the original literal normal-form program, expected
native G14 root, class/binding/opaque verified V2 and original context from test
source, BEFORE considering candidate file bytes. Parameters are native I8
MIN/MAX, Idx0/u32MAX and I64MIN/MAX; original program is a pre block with two
commits and a wide post reshape commit. Registered tokenizer18, actual job
context tokenizer0, max_context16 and original empty/undeclared header layout
remain unchanged. Synthetic coherent trace/step fields pin data association;
they do not certify any computation.

Parent writes the literal artifact, opens it as File, and actually executes
`load_wide_artifact_v2` once. It retains the real private Source/compiler/native
Params owner and optional existing32-MiB host allowance guard. Its resident_open
borrows those exact owners; no second local model execution/reference run is
used to manufacture the expected response.

Parent spawns current_exe through WideCellProcessV2 and the mandatory concrete
Unix nonblocking deadline adapters (including shared backing prepayment once).
Child dispatches outside libtest, independently rebuilds literal pins and opens
THE PARENT'S artifact path EXACTLY ONCE as File, then runs actual
`load_wide_artifact_v2` -> `Loaded.serve` -> existing Prepared CPU/session code
on native stdin/stdout. It never loads the wire program as executable state or
uses V1 fallback. Native original values, not canned frames, produce responses.

Parent performs TWO explicit Open/Step/Close cycles, exercising cumulative
counter preservation across Close/Open. Each explicit Step returns all three
original typed values and exactly `[i64::MIN,i64::MAX]` logits. There is no
hidden model rerun; these are two deliberately requested operations.

## Framing, native and cumulative assertions

Hand-pinned request inventory:

- Hello framed13 bytes;
- bounded Open framedP+88 bytes for original program lengthP, three None-layer
  parameter headers and26 native payload bytes;
- empty-carry Step framed25 bytes, Close framed9 bytes;
- complete input13+2*(P+122).

Hand-pinned response inventory: Ready42 bytes, Opened9, Stepped212 (body204,
including three two-lane i128 wire values and two8-byte i64 logits), Closed9;
complete output502 bytes. Child Read::take is capped at the EXACT admitted real
request bytes. It skips no input. Only after both actual Close requests does
that cap report EOF; existing serve returns normally and the child drops its
actual Loaded guard before publishing completion.

Parent pins exact read/write/message/step counters after each cycle, exported
native response backing from actual capacities, exact cumulative prepaid Open
build work, total partitioned work and poison state. Child reports independent
server steps2/messages14, exact reverse-direction read/write totals, nonzero
symbolic work, server exports0, one Source/Params startup,26 native read bytes,
three inventory leaves and one startup allowance claim. Guard calls stay1;
child reports held0 only AFTER dropping Loaded, while parent retains its own
allowance until its native owner drops. Startup retained claims never refund.

## Diagnostic/cleanup control and bounds

Child stdout contains ONLY Hello2 protocol frames. Stderr remains null. Trusted
local PID/completion data are published to the parent-controlled TempDir by
complete file+rename, not mixed with protocol data. PID is exactly8 bytes;
completion is32 fixed u64 fields (256 bytes): fixed kind/profile/usage/context
flags, never backend diagnostic text or model output. Parent File reads have a
4-KiB hard file cap and fixed arrays. Missing/wrong/malformed completion/profile
markers cause failure, not a successful skip.

Control polls and every connect/request exchange use finite10-second deadlines;
no retry restarts an individual exchange clock. Existing process-owner RAII
handles failures and direct-child kill/wait. The extra test cleanup guard signals
ONLY after waitpid(WNOHANG)==0 establishes its specific positive PID is still
our own live child. After completion, parent drops the actual process owner and
requires ECHILD for that exact PID, distinguishing owner reaping from the test
reaping it. On panic/error RAII still attempts cleanup. No other PID is signaled.

Parent waits for the completed child record BEFORE Process Drop/terminate, so
child data/host-guard destruction is observed independently of kill. The
production owner's abrupt kill/wait policy may still coincide with the tiny
runtime shutdown interval; no successful normal-exit status, process-group
cleanup, wall-time bound for OS kill/wait or total Source/compile CPU guarantee
is asserted. Deadline/finite logical resource checks are not native/RSS proof.

The declared8-MiB external TEST allowance explicitly covers independent pins,
program/layout/native literals and copies, Command/environment/TempDir paths,
fixed PID/completion files/arrays, callback/counter/budget owners, expected and
received response backing, std stdin/stdout buffering/runtime metadata. It is a
finite declaration for this tiny fixture, not measured RSS, a generic model
byte factor or a global host reservation. Existing Loaded allowance and IPC
workspace/frame/native caps remain separately enforced. No fake host lease is
created when an existing optional provider is absent.

## Source checks

Project-config rustfmt parsing passed. `git apply --check` passed against
90877a99f. No actual source/manifest was edited, no Cargo/rustc or process/test
execution occurred, and no runtime PASS is claimed. The only success marker
exists INSIDE the proposed parent after every File/Loaded/CPU/counter/guard/PID
assertion. It requires the audited Source profile and explicitly labels the
OS-main stack. Runtime validation remains HOLD pending independent review.
