# Separate sandboxed Node diagnostic — 2026-11-02

This is test-only progress toward stages 3 and 7 of
[the complete seven-stage objective](../COMPLETION-REQUIREMENTS.md).
All eight original and current workspace tips remain ancestors of the
integration branch; see `workspace-ancestry.json`.

## Actual implementation

The parent performs the original ordinary Miner inference and one Panel
capture, builds the signed record, then starts a new executable process for
Node checking. The child receives metadata, signed commitments and one bounded
source frame at a time over a Unix socket. It independently reconstructs the
fixed local job/class/artifact/worker expectations and calls the existing Core
preparation, full-relation submission and EOF/root/signature verification. The
child does not call the executor or load the original model/fixture. The same
test executable still contains dormant executor code; this is not a separate
production Node binary with the executor entirely absent. It uses a
new address space and a tested macOS sandbox denies reading the original
cohort directory. An independently created denial sentinel is also checked;
the trained path requires three actual PermissionDenied observations. The
parent checks FD_CLOEXEC on its original model descriptor before spawning.

This is private diagnostic IPC with a trusted framing sender. Its bounded
outer packets and actual IO budgets do NOT make generic Borsh decoding a
production adversarial peer decoder. The child uses local fixture authority,
not an accepted native job, selected P certification or production admission.
No production activation, wire ID, arithmetic, root or credit rule changes.

The existing whole-operation request ledger remains 8 GiB. Before spawn the
parent reserves 128 MiB for its encoder and the child's complete 2 GiB
subledger on that same counter; the child lease remains until exact child
exit. The child reserves its 1,744,830,464-byte external envelope BEFORE
configuration/header/frame decoding: 1 GiB direct evaluation, 256 MiB source
authentication, 256 MiB valid-frame decode and 128 MiB preparation/exports.
Core stream metadata/retained state is additive within its 2 GiB subledger.
All child payloads drop before the external lease is released. These are
software logical request envelopes, not kernel physical memory guarantees.

Only the isolated parent uses 256 MiB producer metadata/workspace and 64 MiB
record authentication envelopes; Core continues checking its derived floors.
The producer does not run numerical evaluation. Its existing shared-process
1 GiB envelopes and Node's 1 GiB evaluation bound remain. The independent
Node's own 64 MiB record authentication envelope is additive.

Packets are bounded before raw body allocation (header 1 MiB, frame 16 MiB,
final report 64 KiB); each direction has a finite 16 GiB delivered-byte cap,
and each endpoint has a cumulative 2^26 IO-call cap. No IO failure refunds a
call. Original claims are streamed in canonical order. Early Panel rejection
requests only the remaining claim bytes and still authenticates the complete
signed record before returning WitnessRejected. The parent closes its
Command-held child socket duplicate after spawn, checks child exit success,
and kills/waits the exact child on failure. No child receives model file FDs.

## Preserved validation

`runtime-first` failed compilation (module path / fixed-job return refactor).
`runtime-r2` compiled but failed the source-denial assertion because a /var
sentinel path needed canonical /private/var resolution; a Command-held socket
duplicate also delayed parent EOF. Both defects are fixed and failed records
remain failures. `runtime-r3` was the first successful subprocess run.
`runtime-r4` additionally verifies payload lifetimes and requires real work
observations. `clippy-first` failed five style lints; those are corrected.

Current `runtime-r5` exits zero: 11 full-relation + 4 record + 23 wide-material
outer tests = 38 selected passes, with five explicitly gated cases ignored.
Three additional nested child processes execute within the wide-material
actor test; these are not added to the outer test count. Honest and false
Miner cases check all 63 fixture relations; false Panel material checks no
successful relation and authenticates/drains all 63 signed claims before
WitnessRejected. All three independently observe sentinel denial; every
parent request lease drops, and the monitored process group is empty. Actual
synthetic aggregate logical peak is 3,806,385,558 bytes; each child's peak is
1,829,779,341 bytes. These fixture measurements do not qualify the trained job.

Strict selected Clippy (`clippy-r2`) exits zero with `--no-deps -- -D warnings`
on exactly the runtime-r5 source map. The earlier lint failure remains.

The actual trained isolated measurement is recorded separately below. A
running or failed execution is never a completed trained outcome. The
trained parent and child each have a finite 1,440-second process CPU limit;
this is not a shared 1,440-second aggregate CPU quota. The outer monitor caps
the whole process group at 1,800 seconds wall time and samples its combined
RSS against 4 GiB, with a 512 MiB disk floor. Existing 320 MiB tape and the
trained parent's 8 GiB / 2^24-call actual provider IO limits remain. The
child's model source is the finite IPC stream, not that provider file.

## Qualification boundaries

Only Node is separated here. Miner ordinary inference and Panel capture/material
preparation still run sequentially in the parent address space; their wall
phases and combined parent CPU are observable, not independently isolated
Miner/Panel CPU/RSS costs. Parent phase reports also contain process CPU
checkpoints; differences are sequential parent intervals, not independent
role-process measurements. The legacy `ROLE_MEASURE` helper retains its
hardcoded `node_process_isolated:false` parent telemetry field. The actual
subprocess observations are the distinct-PID `ROLE_ISOLATED` records and
sandbox/descriptor evidence, not that inherited helper field.
`live-fd-inventory-first.json` records an actual
live OS descriptor observation: the parent has its cohort model open, the
child has no cohort path. This is a point-in-time observation alongside the
three actual sandbox denials, not a claim about every future process.

The original short job is noncanonical 16-prompt/1-decode, not canonical
255/2 or full 2048-context execution. Hashing the model before measurement
warms OS caches. A model-unprepared new Node process plus denied model access
does not establish cold physical caches. Process-group RSS is sampled; the
reported getrusage CPU/RSS values are checkpoints before final report/teardown.
The final IPC report logs checked count, CPU, wire and request peak; cumulative
elements/terms/source-read diagnostics are emitted at 1,024-relation progress
checkpoints. The last such prefix is not an exact final counter observation.
Kernel socket buffers and runtime executable/shared-library pages are not a
kernel memory admission system. Real native authority, funding/holds, P under
an explicit honest-fraction assumption, all credit readers, real-node
restart/reorg/IBD/pruning/dependencies and approved finite activation remain
required. A local Matched is not consensus credit authentication.

## Complete original trained isolated short job

Code commit `95110addf0d47f5223453f3e46f11e042a59f821` is exactly the
runtime-r5 / strict-clippy-r2 / actual-run source map. The actual parent PID
90405 and distinct sandboxed Node PID 90429 both exit zero; the final process
group is empty. The original artifact/fixture and compiled binary SHA values
match before/after. See [the checked actual result](actual-isolated-short-job-first.json),
raw `pythia-isolated-first.log/json`, `input-sha-isolated-first.json` and
`source-validation.json`. Both actual libtest results are 1 pass, zero failures;
these two results are parent/child executions, not two independent model jobs.

| Observed quantity | Actual isolated short job |
| --- | --- |
| Full relation checks and signed record finish | 213,298 / 213,298, local Matched, EOF/root/signature accepted |
| Entire monitored parent process wall time | 982.395623 s |
| Material preparation | 544.235207 s |
| Child numerical submissions | 423.387476 s |
| Transfer through exact child exit | 977.246858 s |
| Child process CPU checkpoint | 424.235323 s user + 3.726312 s system |
| Parent process CPU checkpoint | 533.119454 s user + 21.271250 s system |
| Delivered parent-to-Node wire bytes | 5,795,421,305 |
| Parent global logical request peak, including child whole subledger | 8,505,605,695 bytes, within unchanged 8 GiB |
| Independent child logical request peak | 1,829,793,136 bytes, within its 2 GiB subledger |
| Child getrusage RSS checkpoint, macOS native bytes | 15,089,664 |
| Parent getrusage RSS checkpoint, macOS native bytes | 174,391,296 |
| Actual original model / fixture / sentinel denied reads | 3 |

The original generated token is 273 and its output root matches the earlier
unchanged original job. Ordinary inference reports zero witness runs; Panel
capture reports 14,157 leaves and 1,308,868 retained-run request bytes. Aggregate
capture/claims tape is 305,797,608 bytes. Parent and child retain all relation,
source authentication, output comparison and final signed-record checks. The
last diagnostic work checkpoint is 212,992 relations, explicitly a prefix;
the separately authenticated final checked count is 213,298. Final actual
provider IO/call counters and final cumulative evaluator work were not emitted
by this isolated branch, and are not fabricated from the earlier shared run.

This completes this private original 16/1 isolated-Node local measurement.
The qualification boundaries above and every native completion requirement
remain. It does not approve a production resource profile or activation.

Sampled whole process-group RSS peak: 187,793,408 bytes.
Minimum observed disk free: 1,311,666,176 bytes. Neither is a kernel quota.
