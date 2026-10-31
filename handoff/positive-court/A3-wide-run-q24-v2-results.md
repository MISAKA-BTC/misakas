# A3 — explicit wide Q24 run: frozen-candidate validation

Source **`79b8804d9c0d5195ebf242c90abc8c7168217616`**, exact A0 base `5d14e927c`, branch
`codex/a3-wide-run-v2`. Required A1 source sequence: `41a00f590` →
`b048ce4e1` → `9a3e952e9` (local aliases `76c8b1ac5`, `fab0bd862`,
`3ff1a356d`). If these APIs are already present in Root, import only A3 source
`79b8804d9` and its evidence increment. Historical A1/A3 counts are not added.

**49 selected tests passed, zero failures**, on this exact source. Strict exec
library Clippy (`--no-deps -- -D warnings`) passed. The source is a local producer
run increment; it does not establish Root-current whole-system success, native
admission or public activation.

| Same-source suite | Passed |
|---|---:|
| New wide run / observer / ceilings / V1 refusal | 7 |
| New high-bit Idx nodes / legal signed state / illegal Idx state | 2 |
| New host lease lifecycle | 1 |
| Existing NodeRun / resume | 3 |
| Existing capture codec | 4 |
| Existing cell / process | 12 |
| Existing executor/tree memory | 1 |
| Existing retained-job memory | 1 |
| Existing typed capture | 9 |
| Existing typed legacy adapter | 8 |
| Shared memory arithmetic unit | 1 |

[Exact commands and raw-log/source/tree/file/binary/archive hashes](A3-r1-evidence/wide-run-q24-v2-root5d/validation.json)
pin the run. The existing A3 cache was reused with two jobs, debug0,
no incremental compilation, Core opt0 and the explicitly supplied existing
static RocksDB archive. No new target directory or C++ rebuild was requested.
Only the final outer summary per Cargo-launched binary is counted; nested child
repeats are excluded. Mutable preflights, including an initial compile error,
are retained separately and excluded.

The fresh wide fixture uses exact i64 MIN/MAX, negative-only rows, ties and
1082*2^24. Independent reference interpretation and hand-derived token IDs
`[2,0,1,2,0,1]` match all six native i64 rows without i32 conversion. A manual
step hash/tree oracle and A1's canonical producer/binding APIs agree on the
original-context roots. The window3 fixture has **42 leaves** and **40 actual
observed nodes**; quote node visits conservatively include post at every
position. The window1 fixture covers eviction behavior. Registered tokenizer18
and actual canonical job tokenizer0 remain independent, and the declared maximum
context stays16.

Separately declared paired narrow/wide programs have byte-identical non-final
node/Fixed/Hist preimages under their own original identities. Only final post
i64 logits use version2/eight-byte lanes. High-bit Idx **nodes** retain their
unsigned four-byte words; states remain I8/I16/I32 and Idx Fixed/Hist are rejected
by the unchanged validator. No fixture is mutated into an admitted model.

Observer callbacks match independent reference nodes, states and native outputs;
first-node and later final-node refusal prevent later callbacks/execution. The
source has one ordinary TirExecutor construction and one position loop, sharing
the V1 borrowed observer seam; it never calls a legacy run or re-executes the
model to form roots. Callback counts by themselves are not a physical execution
benchmark.

All five request ceilings are exercised at the exact quoted bound and one
below before callbacks. Additional cases refuse wrong context/prompt, out-of-
bound prompt IDs, unsupported Merkle prompt form, malformed wide profiles,
foreign class/scheme and unsupported old entrypoints. The V1 CPU device, process
client and actual IPC server reject wide before step/parameter transmission;
old narrow cell execution still succeeds. The raw V1 infallible i32 getter's
intentional wide misuse is caught with catch_unwind; the corresponding panic
line in the raw log is an expected rejection, not an unreported test failure.

The dedicated host callback test observes this **logical request**, not RSS:

| Component | Bytes |
|---|---:|
| Executor envelope | 16,877,696 |
| Retained rows / IDs / hashes | 3,048 |
| Simultaneous hash / tree / prompt / tile scratch | 80,614 |
| Total requested | 16,961,358 |

Its symbolic work quote is 8,192,288. An actual host lease is active during all
observer/leaf callbacks and after returning the immutable result, and becomes
zero after result Drop. Binding refusal releases its guard before execution;
host refusal and too-small limits produce no callbacks. These tests measure
reservation identity/lifecycle, not allocator use, elapsed performance, ML-DSA
scratch, network traffic or trained-LLM feasibility.

[API and ownership details](A3-wide-run-q24-v2.md) describe the fixed-length,
Flat-prompt, greedy, ordinary CPU subset and the additive memory/host work
inventory. Caller-owned plan/params/prompt and arbitrary callback copies/work
remain separate reservations. V1 DTO/IPC/capture bytes stay unchanged.

`PALWTRC2` capture, Hello2/cell, evidence/Court, SDK/native route integration and
non-Flat or non-greedy modes are separate later increments. Existing unsupported
public FP/V4/device routes remain closed to wide. No public arming, native
registration/admission receipt, credit, funding/fees, Gate C or activation is
supplied here. Real trained-cohort reproduction at its original maximum context
is still a later SDK/native-wide task; these toy results do not count as that.
