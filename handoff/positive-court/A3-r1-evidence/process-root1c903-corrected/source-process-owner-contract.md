# A3 — explicit V2 child/process owner and deadline adapters

Separate branch `codex/a3-wide-process-owner-v2` follows the tested bounded-header
increment and Root Hello2 integration evidence. Initial source is uncompiled;
no test result from the previous Root37 or artifact5 is assigned to this code.
The Core public Vec helper remains isolated on `codex/a3-public-vec-envelope-v1`.

## Owned process and Open source

`process::WideCellProcessV2<R,W,C>` owns an actual Child, its piped stdin/stdout,
the bounded Hello2 client and a mandatory exchange controller. There is no
blocking/no-deadline default, V1 fallback, CPU mirror or V1 backend registration.
`open_resident` is the only Open path; raw request accepts Step/Fixed/Hist/Close.
Hello, caller-built Open, wrong state/tag/version, refusal and I/O failure poison
the owner. Errors/terminate/Drop attempt direct-child kill/wait before dropping
successfully returned adapters. Close does not release process ownership or
reset any counters. A shared child owner also protects client-construction errors.

Callback construction may drop partial adapters before it returns Err/unwinds;
the callback must provide bounded self-cancellation/Drop/join. The concrete Unix
factory owns only native pipe handles and shared deadline state, with no threads,
queues or user I/O buffers. Partial construction therefore closes native handles
without joining workers. Arbitrary custom callbacks do not inherit that property.
Kill/wait may fail or be delayed by the OS; explicit errors report cleanup failure,
Drop can only attempt it. Descendant/process-group cleanup is not supplied.

`ResidentWideOpenV2` borrows independently authenticated original program bytes,
an unmodified `TirPlan::compile` result and stable matching resident native params.
These are caller/source-owner preconditions, not provenance established by a
constructor. It rejects lazy sources before get, checks dtype/length, compares
the literal program with a streaming writer, and serializes native LE integers
directly (including high-bit Idx). No Tensor export, full i128 mirror or row
materialization is used. I128 params remain forbidden by normal form.

## Logical backing and cumulative work

Client `2F+2D+8192+external` covers exact Open DTO+frame and reply DTO+frame.
Source/model/Command/environment owners remain in caller external inventory;
the owner adds its own inline header, shared child state and declared adapter
heap. Prior returned response capacities remain cumulative exports and are
checked before the next build/I/O. Parent, child-process and OS pipe memory are
separate owners, not one shared lease or an RSS claim.

Open metadata/copy work has one finite prepaid partition subtracted from client
max_work at spawn. Every build spends that partition cumulatively, including
failed/reopened builds; no refund or reset occurs. Client accepted work plus
build work stays within the original total. This is symbolic work, not elapsed
CPU time/instructions. All DTO/frame/native/aggregate parameter checks precede
reserves or payload iteration. The public low-level CPU methods' separate caller
admission precondition remains unchanged.

Pinned Rust1.93 macOS Mutex lazily allocates one pthread-mutex box; Linux futex
storage is inline. The child state and adapter state each account for their own
box. Both initialize serially before exposing adapters, preventing concurrent
first-use allocation. Adapter Arc backing is counted once; reader/writer/
controller inline headers are already in the process owner's typed size.

## Shared absolute I/O deadline

The Unix adapter source supports macOS/Linux. A mandatory controller begins one
absolute deadline before connect and every complete client.request. Read, write,
prefix/body handling, flush, poll, Interrupted and WouldBlock retries reuse that
deadline. They cannot reset it after short progress. Native pipe fds are set
O_NONBLOCK; poll computes remaining time, then owning standard pipe Read/Write
performs the transfer. Post-syscall checks also enforce expiry. Mutex lookups use
try_lock and no lock spans poll/I/O. Unsafe calls are limited to fcntl/poll.

Other platforms refuse this factory. Existing Rust Node/SDK SIGPIPE handling is
a pipe-write precondition; the adapter changes no global signal policy. Scheduling,
allocator behavior, full CPU time, kill/wait and physical RSS guarantees remain
outside these local caps/deadlines.

## Review, tests and remaining full task

The owned process, adapter and stacked tests have been reviewed as source only.
Planned tests cover five actual-pipe/deadline/partial-construction cases and three
actual-child owner cases. The latter use a fixed-octal raw peer which captures
known request frames and returns literal data; it never evaluates a model.
Native I8/Idx/i64 bytes, Open prepayment across Close, poisoning and direct-child
reaping are separate assertions from the existing prepared CPU execution tests.
No success count is assigned before exact-source execution.

This source does not implement a generic artifact loader/server, bounded local
program owner, startup plan/params admission, artifact-root authentication or
SDK/native route. Those remain required. No model eligibility, Court verdict,
Gate C, SecurityCertified or computation credit follows from raw process data.
