# Native legacy G14 history checks — 2026-11-02

Source at the first attempts: `8fe2131b503f4978a884ed9ff5e56ae09c7f55a1`.
The eight original/current A0–A7 workspace tips are already ancestors of this
integrated A0 branch; see the existing workspace-integration record. The dirty
main checkout is not changed by this work.

## Failed diagnostic build, retained

`native-cert-first.json` / `.log` preserve the actual default-stack execution
of the two selected existing native K-certification tests with private opt0
compiler overrides. The first test aborts with stack overflow (Cargo exit101,
SIGABRT); the second does not execute. No test is counted as passing. Source
maps match before/after, and the process group is empty. The exact test binary
and own macOS diagnostic report are pinned in
`native-cert-first-failure-analysis.json`; the report is retained verbatim.

The abort occurs in the outer async test poll. Disassembly of that binary
shows approximately 0xedaf0 bytes of stack in its poll, 0xf6cb0 in the shortfall
body, 0x1473c0 in its uncertified-final helper, and 0x238d00 in the positive
reorg/IBD body. These are the private opt0 build's generated frames, not a
measurement of production execution or a reason to enlarge the global stack.

The repository itself declares opt2 for consensus and consensus-core in its
dev profile. The next attempt removes the private compiler overrides, retaining
debug assertions, overflow checks, the same tests and default thread stack.
It has its own command/source/artifact/output record. That build-only attempt was deliberately stopped after the new Core dependency
compiled, before runtime tests, so the prepared DB-reopen test can join the same
final candidate without compiling consensus tests twice. It exits -15 with no
test results, unchanged source maps and no remaining group members.
`repository-profile-build-checkpoint.json` records the verified owned group and
operator reason; it is not a passing run. The final attempt has a separate
`native-cert-reopen-final` record and must be assessed from that actual result.

## Qualification

These are old claim-level K approvals and H reward holds, with Positive disabled
and existing private test-only Gate C policy. They are not the new P authority,
a publicly activated network, or evidence that the seven-stage Positive Court
goal is complete. The opt0 failure remains a failure even if the repository
profile passes. No Rust source, consensus policy or activation was modified
for these first two attempts.


The exact eight-role original/current ancestry is rechecked in
`merge-ancestry-recheck.json`; every input is an ancestor. A6 intentionally
still points at the earlier A0 integration tip. This check does not move or
rewrite any workspace branch.


## Added legacy recovery regression (runtime pending)

The test-only network restart helper now accepts an opener called after the
old chain/processors drop; the existing Arc-based restart delegates to it.
The certification helper also accepts an already constructed legacy Net, with
Positive explicitly disabled. The added test owns the temp directory separately,
moves the only DB Arc into the node, and requires the old DB Weak to have zero
owners before each fresh `create_if_missing(false)` open.

Both pending K/H and settled K/no-H states compare the sink, PALW root, full
canonical carriage bytes, served kernel rows, the existing kernel-money
accounting view, eight payees' actual minted/unspent outputs and pending queue,
and actor funding UTXOs. It then checks no repeat payment, IBD and pruning-sidecar
state/fund equality. The pruning helper uses the existing local verified-header
import fixture, not a full production P2P pruning session. This test is new and
must not be reported as passing before its actual runtime completes.


## Repository-profile failure and scoped correction

The first actual repository-profile candidate also aborts before the first
case completes. `native-cert-reopen-final.json` records opt2 for the test
crate, debug assertions and overflow checks enabled, default stack, exit101,
no passing tests, equal source maps and an empty process group. Its own
`.ips`, failure analysis and exact failed-source patch against `8fe2131b5`
are preserved. All three reconstructed changed-source hashes were checked
against that failed record. Generated binary paths can be reused by Cargo;
the recorded SHA identifies the binary at capture time, not an immutable
retained executable.

The fault is in the nested call to the preset constructor, whose own frame
is only about 0x12000 bytes. The surrounding `honest_final_uncertified` poll
retains approximately 0xd0000 bytes, with more enclosing polls/config frames.
The failure therefore does not establish a faulty preset value or an oversized
registry constructor by itself. The one-second own-compiler sample separately
records active LLVM ThinLTO optimization during the long build; it is not a
runtime Court measurement.

The correction heap-pins child test futures at the existing world/final/scenario
boundaries and the new reopen test entry. Assertions, transitions, production
Params types/values, default stack and public activation are unchanged.
`native-cert-reopen-heap-futures` is the subsequent actual attempt; its runtime
result is pending and must not be inferred from this implementation description.


## Concrete heap futures: actual result and next candidate

`native-cert-reopen-heap-futures.json` / `.log` record the completed
repository-profile build and failed runtime. The shortfall case printed `ok`;
the reorg/IBD case aborted in ML-DSA signing while registering the class. The
reopen case never ran. There is no passing suite summary. The crash report,
failed cert/reopen source reconstructed against the exact recorded SHA-256,
unchanged network-helper patch and disassembly are pinned by
`native-cert-reopen-heap-futures-failure-analysis.json`.

The outer reorg test poll still reserves approximately `0xd5000 + 0x9e0`
bytes, despite `Box::pin`; the seated helper adds approximately
`0x5d000 + 0x9b0`, then World registration and signing add their own frames.
A concrete boxed child does not prohibit compiler inlining of its poll.
The next correction uses non-inlined constructors returning type-erased
heap futures at the same private test boundaries. Construction returns before
the child is polled; the assertions, real signatures, funds, transitions,
production types/settings and default thread stack remain unchanged.

`native-cert-reopen-erased-futures` is the next actual command/source/binary
record. Its result must be read from that record; the code change alone is
not evidence that any of the three selected cases passes.


## First completed certification/reopen suite

`native-cert-reopen-erased-futures.json` records exit 0, all three selected
cases passing, identical source maps and an empty owned process group. The
crate uses repository opt2, debug assertions and overflow checks, with no
RUSTFLAGS or RUST_MIN_STACK override. Both pending and settled fresh DB
connections compare canonical state bytes, rows and actual funds. The log
includes both real close/reopen checkpoints, plus the existing reorg/IBD test.
This qualifies the selected legacy cases at its own pinned source only.

## Additional legacy signature/fraud/money/restart suite

`native-legacy-fraud-money-final` built from the same source and binary. Its
first three signature/fraud cases printed ok; the fourth, the escrow/reorg/
redemption scenario, aborted during nested World::new/Params construction, so
the restart case did not run and no suite success is claimed. The failure
analysis pins its actual crash and failed network/recovery sources.

The test-only correction also erases the shared World construction future
and the two large money/restart scenario boundaries. No assertion, production
rule or stack limit is changed. `native-legacy-eight-erased-futures` reruns the
three certification cases and all five additional cases as one source-pinned
suite; its runtime result is pending.


## Final integrated legacy checkpoint: 8 / 8 pass

`native-legacy-eight-erased-futures.json` / `.log` record all eight selected
cases passing on one unchanged source map with repository opt2, debug
assertions/overflow checks enabled, normal stack and no remaining process
group members. `final-validation.json` pins the record and final source patch.
Only test helpers, scenario future boundaries and the new closed-DB reopen
regression differ from `8fe2131b5`; production Rust is unchanged.

The eight cases cover shortfall/unavailable nonpayment, selected approval and
reorg/IBD, pending/settled closed-DB reopen and pruning-sidecar/fund equality,
completed-chunk signature rejection, outsider fraud before/after Final,
escrow/reorg/replay/redemption single payment, and resumed fraud filing after
node restart. The pruning fixture verifies the existing local header/state
import seam, not a full P2P pruning exchange. This is selected legacy coverage,
not equivalence for every historical claim and not new Positive native intake,
P authority, full credit consumers or public activation. All failed attempts
above remain failed. Subsequent Assignment implementation has its own source
and verification checkpoint; these results are not silently transferred.
