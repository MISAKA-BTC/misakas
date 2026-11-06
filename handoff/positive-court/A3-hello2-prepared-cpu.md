# A3 — explicit Hello2, prepared resident CPU subset

Branch `codex/a3-wide-cell-ipc-v2`, following capture2 evidence `5b362a75b`.
Codec/quota source is `31739101d`; CPU source is `33f01a1c8`. This increment
is source-only until its frozen candidate has separate recorded checks. Earlier
18-test capture and 49-test run results do not validate this increment.

## Protocol and execution boundary

`node::cell_ipc_v2` introduces explicit Hello2/Ready2 before Open. Request tags,
field order and message meanings follow V1. The new Stepped logits vector has
signed i64 lanes, eight bytes each. Generic CellValue/carry lanes retain i128
wire width16; narrow state query lanes retain width4. Old Hello1, DTOs, wire
bytes, process/device dispatch and i32 consumers remain unchanged. Neither side
falls back across versions. Invalid order, wrong version, malformed input,
refusal or I/O failure ends server processing or poisons the client.

`PreparedWideCellV2` borrows an independently admitted V2 binding/space,
original job context, plan and resident parameters. It pins class/context and
program identity. The caller must supply an unmodified `TirPlan::compile` result
and matching resident parameters; the prepared API does not authenticate all
public compiled-plan fields. This trusted-source precondition is separate from
wire-data checks. Open streams comparison against original program and
parameter bytes. It does not decode a wire program, build a plan, materialize
wire parameter tensors or fetch lazy rows. A provider with a row source is
refused before range refinement can materialize it. This is a plan-specific
resident source seam, not generic model-artifact startup.

Each opened CPU cell owns one ordinary `TirExecutor::new_cell`. A requested Step
calls `step_cell` once. Its range follows the original prefill/post schedule;
token bound, carry count/shape/dtype, committed response backing and work are
checked first. Logits are read directly and checked into i64. No i32 logits
adapter or matching local replay is used. Cells start at position0; post-only
cells with multi-token prefill need a future explicit resume API and are refused.
Fixed/Hist queries retain original narrow state types and restrict scope and
canonical history geometry.

`WideCellClientV2` returns bounded raw data. It is not a `KernelBackendV1`,
`TirDeviceV1`, native verifier or arithmetic acceptance path. Coherent wire data
alone does not establish model correctness, authenticated expected roots,
SecurityCertified, Gate C, Court outcome or credit. No backend registry or
default public dispatch is changed; GPU, FP and V4 are outside this increment.

## Ownership and cumulative admission

All limits are explicit; there is no successful unlimited/default constructor.
The client base is `2*max_frame + 2*max_decoded + 8192 + external_retained`.
The server adds the original-context executor workspace derived by the shared
wide storage helper. Two native owners and two frames conservatively cover an
input plus collector/response and their input/output copies. Each nested Vec,
shape, byte string and lane vector is checked against wire/native/aggregate caps
before reserve; fixed-width scalar parsing has no generic program decoder.

These are logical caps, not a full-session host lease or RSS measurement. An
opened CPU cell separately obtains the optional existing host memory guard for
its workspace, under the original class/context and role `tir-wide-cell-v2`.
That guard outlives the executor and is released on Close/error. It covers no
transport, parameter, caller or other-process ownership. The caller must reserve
and declare the plan, admitted space, parameters, previous requests, retained
frames, copied outputs and other independent owners in `external_retained_bytes`.
Client and child-process budgets are separate; one cannot stand in for the other.

Accepted read/write bytes include the eight-byte frame prefix; body ceilings
exclude it. Cumulative bytes, messages, steps and symbolic work never reset on
Hello/Open/Close. Frame admission precedes allocation and charges byte work;
bounded metadata work precedes encoding; CPU/query work precedes execution.
Rejected admissions do not mutate counters; accepted charges survive subsequent
decode, execution or I/O failure. These are admission counters, not physical I/O
measurements. A Refused reply is attempted only within remaining quotas.

The cumulative work/step/IO guarantees above apply to the session entry point.
The public low-level CPU source/stepper does not own a connection budget: direct
callers must admit `open_work` before `open_cell`, admit each kernel/query work
quote and maintain their own cumulative step/work counters. `step_data` checks
scope, carry and local response backing but does not itself consume cumulative
work or steps. Direct CPU tests establish those local properties only. A future
generic startup/native owner must enforce admission through one owned entry;
it must not treat possession of a prepared source as authority to bypass it.

The client conservatively accumulates actual native backing of every returned
response, including retained Ready data, without assuming the caller freed any
earlier response. Before a later operation it checks base plus all prior exports.
Arbitrary user copies and retained request buffers remain external owners.

## Required remaining work and validation

The full task still requires a generic real-model-artifact V2 process/Open path,
bounded program decoding/admission, an artifact/startup verified-source loader,
and SDK/native generic process integration. The prepared-plan API and toy
subprocess tests do not complete those requirements. Lazy-row operation, explicit
resume, generic verifier routing and public activation also have no implementation
claim here. None of these gaps may be hidden by changing the task to this subset.

Planned checks cover literal wide/V1 wire bytes, hostile lengths/native expansion,
strict CPU scheduling and typed carry, state queries, client poisoning/export
retention, cumulative quotas and workspace guards. Actual OS subprocess checks
use a loopback stream separate from test-harness stdout, bounded deadlines and
child cleanup. A pre-admitted toy fixture tests transport and cell execution;
it is not a trained-model artifact loader. Source review comes first; Cargo
requires A0's build slot release and uses only the existing A3 target/static
RocksDB archive. Test evidence must pin its exact source and preserve failures.
