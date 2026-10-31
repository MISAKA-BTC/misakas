# A7 — independent Positive Court r1 acceptance

Workspace: `codex/positive-court-a7`, based on A0 `4dede1aff` plus the A1
`ef8e4e87d` / `b3a61454c` adapters and A0 author-frame authentication
`5901eff3f`. A7 owns only this test crate and runner.
No shared production module, native activation, model tier, or model-review rule
is changed by A7. Computation is checked directly; cryptography authenticates data
and authors only.

The superseded private prototype trial is preserved on
`codex/a7-prototype-history` at `41755f1b8`. Its 9 A7 tests and 25 executor
regressions passed; the full run was deliberately stopped after A0's audit, with
38/39 core Positive tests complete. It is **not an integration PASS**. Its trust
fixture and process timeout gaps are addressed here rather than hiding the old logs.

## Reproduce

```sh
python3 testing/positive-court-a7/run.py --out /absolute/path/to/fresh-run
```

Output paths must be fresh. The runner saves commands, exit codes, source/compiler/
binary/lock hashes, raw logs and explicit gaps. It starts each command in a new
session. Timeout or surviving descendants trigger TERM, then KILL if necessary;
the direct child is reaped and the whole group checked for live writers before
the log is closed or hashed. Zombie entries, if any, are recorded separately.
The three process-control tests include a TERM-ignoring parent and grandchild,
and a wrapper that exits successfully but leaves a writer behind.

The A7 crate has a separate lock containing only versions already present in A0's
root lock. It avoids editing A0's workspace/feature/module files. `--locked` and
source hashes make dependency differences reviewable. Debug information and
incremental compilation are disabled; consensus-core uses opt-level=0. These are
test-profile observations, not production performance certification. Rust test
threads use an explicit 16 MiB `RUST_MIN_STACK`: the earlier unoptimized fence
run aborted on the default test-thread stack; its original failed log is retained.
The same fence binary passed all nine tests with the explicit test-stack setting.

## Independent trust and arithmetic

`Trusted` contains the simulated rooted class/artifact, canonical job/input,
scope and assigned Miner. The trusted initializer fixes it **before** reading any
candidate and serializes it separately from `Packet`. Node uses these independent
values for A0 `AdmissionContext` and A1 `PreparedCheck`. No expected registration,
job or input is reconstructed from a received candidate.

Replacement attacks use a fully self-consistent, newly signed alternate model,
artifact, job/input, witness, plan and frame set. Each alternate must first pass
under its own trusted metadata, then fail under the original fixed metadata.
The same attack is exercised in a separate Node process.

A four-row embedding followed by integer scaling is registered as a **new all-node
commit i32 program**. Its scalar oracle uses plain multiplication and strict
comparison, independent of the TIR executor, Panel evidence builder and Node
checker. Every logit and output token is compared. For input 1 the expected
sequence is `[2,0,0]`. The initial joint profile does not claim wide/sparse support.

Other tests cover each corrupted leaf, every incomplete prefix, author/domain
substitution, false arithmetic signed by all actors, a valid full computation
with a wrong Miner output, malformed framing, ordering and complete-frame bytes.
Rejected Panel material must not be converted to a Miner computation conviction.
Every frame sent to A1 first authenticates the exact canonical bytes with A0's
`EvidenceFrame::authenticate` and `EVIDENCE_SIGN_CONTEXT`. Third-party tampering
fails delivery before touching candidate progress; a deliberately bad Panel frame
is signed again by that Panel before testing witness rejection. Low evaluation
budgets produce `EndedUnverified`, not an arithmetic conviction. The signed frame
adds 4,627 bytes to each unsigned frame; native transaction/wrapper overhead is
still excluded and is not claimed to fit a production cap.

## Process measurement and unfinished evidence

The toy process run separates baseline, Miner, Panel, Node, prefix, restart,
wrong-result, bad-Panel and coherent foreign-packet scenarios. The Node directory
receives only separately fixed public trusted metadata and candidate/journal
frames. It does not construct a model or executor. This is API/process separation,
not an OS sandbox or evidence of real network delivery.

The raw separately fixed metadata, honest and coherent foreign signed packets,
and prefix frames are saved as Borsh test artifacts and hashed in `run.json`.
These test files are not a new native wire codec.

Each role reports elapsed/CPU/peak RSS and unsigned and signed frame bytes. RSS/CPU are
process-lifetime counters; stage timings include fixture, codec and signing work.
These scopes cannot be subtracted to claim a certified Miner overhead ratio.
The fixture initializer's artifact-index preparation and trusted-metadata read
are outside the role-stage timer. The baseline also includes local plan/params
setup; it is not a pure inference kernel benchmark. Neither A2's actual Miner
adapter nor A3's actual PanelBuilder is called by this warm fixture.
One warm 4x4 sample is **not** a real LLM, cold/provider, minimum-hardware,
concurrency, p95/p99, storage-retention or native-network measurement.

The test-only prefix journal replays checked frames in a new process. It is not
A6's atomic service/request/attempt/funding journal, power-loss durability,
reorg, pruning or IBD. Missing those integrations stays unverified.

Remaining evidence: A3's actual `PanelBuilder` connection; A2 provider/cold data
delivery and signed duties; A4 rooted draw/carrier/deadline/funding admission;
A5 current Gate C and every actual reward/DAG/settlement/dependency read; A6 atomic
recovery; real LLM and concurrent-network limits and explicit production caps.
No stub grants native credit. Activation remains disabled.

Known old-example handoff: the former `positive-court-llm` prototype example used
the yardstick context without filling the registered tokenizer. Nonzero-tokenizer
admission was reproduced as failing in the old trial. A0 retained this as a repair
requirement before that example may be used for real LLM measurements.
