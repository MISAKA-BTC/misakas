# A7 — independent Positive Court r1 acceptance

Workspace: `codex/positive-court-a7`. The earlier 110-test candidate is retained
as historical evidence. Parallel follow-up merges A0 through `6337ea50d`,
including native schema/funding reservations, A1 signed ingress/meter, the
accepted A2 bound inference plus protected-tail fix, A3 compatibility, and A5
node bridges. A7 owns only this test crate, audit evidence and runner.
New follow-up code is not treated as verified until its own pinned manifest
reports success; prior candidates' successes are never summed into that result.
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

Run the Python controller and manifest regressions separately with
`python3 -B -m unittest discover -s testing/positive-court-a7 -p 'test_*.py' -v`.
Discovery includes the real-LLM driver's 18 synthetic guard checks without
requiring `--driver` or starting a model. The standalone guard command still
accepts an explicit driver and emits its original JSON report:
`python3 -B testing/positive-court-a7/test_real_llm_process.py --driver testing/positive-court-a7/real_llm_process.py`.

Output paths must be fresh. The runner saves commands, exit codes, source/compiler/
binary/lock hashes, raw logs and explicit gaps. It starts each command in a new
session. Timeout or surviving descendants trigger TERM, then KILL if necessary;
the direct child is reaped and the whole group checked for live writers before
the log is closed or hashed. Zombie entries, if any, are recorded separately.
The process-control tests include a TERM-ignoring parent/grandchild, a wrapper
that leaves a writer, SIGTERM of the controller, and repeated SIGINT during
cleanup. Handler restoration and log stability are checked. SIGKILL of the
controller and a child deliberately escaping into a different session are
outside this process-group guarantee.

The A7 crate has a separate lock containing only versions already present in A0's
root lock. It avoids editing A0's workspace/feature/module files. `--locked` and
source hashes make dependency differences reviewable. Debug information and
incremental compilation are disabled; consensus-core uses opt-level=0. These are
test-profile observations, not production performance certification. Rust test
threads use an explicit 16 MiB `RUST_MIN_STACK`: the earlier unoptimized fence
run aborted on the default test-thread stack; its original failed log is retained.
The same fence binary passed all nine tests with the explicit test-stack setting.
The runner rejects an empty/ignored required test target individually, and
re-enumerates/rechecks sources, root manifests, locks, HEAD and both A7 test
binaries before writing PASS. Concurrent edits therefore cannot silently
change the candidate after its initial fingerprint.

Runner v4 requires the dedicated core `positive_court_fence_v2` target and the
A7 `pipeline` target to each execute at least eight tests and every explicitly
listed required test name in one summary. A library visitor, ignored test,
another target, or several partial summaries cannot satisfy that requirement.
The final suite check also rejects an omitted normal execution; `--no-run`
artifacts never substitute for it. The literal inventory is saved in the manifest.

`CARGO_TARGET_DIR` may select an existing shared cache. Its directory must already
exist; the runner does not create a new cache. When `ROCKSDB_LIB_DIR` selects an
existing compatible `librocksdb.a`, the runner records its path, streaming hash,
and library/static environment inputs, then checks them again before PASS.
The source snapshot includes the whole consensus tree and database adapter.
These preparations do not certify a Rust candidate. A fixed A0 candidate must
include the source-ready fixture's dependencies before the full suite runs.

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
The third embedding row has equally winning logits at indices 0 and 2, so the
independent oracle also checks the required lowest-index tie break.

Normal fixtures and the Panel subprocess now call A3's actual `PanelAdmission`
and `TirPanelBuilder` through the common `PanelBuilder` port. A7 signs A3's exact
witness and frames without rebuilding them, then A1 receives them through
`PreparedCheck` / `TirNodeChecker`. The false-arithmetic attack remains an
independently assembled A7 adversarial candidate; it does not rely on A3's
generator to decide what is false.

Other tests cover each corrupted leaf, every incomplete prefix, author/domain
substitution, false arithmetic signed by all actors, a valid full computation
with a wrong Miner output, malformed framing, ordering and complete-frame bytes.
Rejected Panel material must not be converted to a Miner computation conviction.
Every frame sent to A1 first authenticates the exact canonical bytes with A0's
`EvidenceFrame::authenticate` and `EVIDENCE_SIGN_CONTEXT`, then uses A1's actual
`check_authenticated_part` so signature bytes enter its meter. Third-party tampering
fails delivery before touching candidate progress; a deliberately bad Panel frame
is signed again by that Panel before testing witness rejection. Low evaluation
budgets produce `EndedUnverified`, not an arithmetic conviction. The signed frame
adds 4,627 bytes to each unsigned frame; native transaction/wrapper overhead is
still excluded and is not claimed to fit a production cap.

## Existing entitlements: source-ready follow-up, runtime unverified

The pipeline fixture now targets A5 `30f3fd23e4db6d494d4f511a2addad5e736df851`
(parent `dd8efe1ab`): `RootedCreditView::authorize_entitlement` and the appended
`WorkerPayment` purpose with `CreditConsumers::worker_payment`. These dependency
changes are not merged by this A7 fixture update. Rust typechecking and runtime
verification await the accepted full candidate; historical PASS evidence does
not cover this source change.
The current [source-ready handoff](../../handoff/positive-court/A7-entitlement-source-ready-2026-11-01.md)
supersedes the retained, historical pending-patch note for application status.

Existing primary rights are an explicit private oracle for the original seven
uses and deny WorkerPayment. A separate paid-worker contract oracle allows only
WorkerPayment and Settlement. Both check the complete binding; PositivePassed,
Gate C, readiness, fees and P do not create these rights. The real bound A2 → A3
→ A1 → TirCreditView tests retain literal eight-use expectations, reject every
use with a missing entitlement port, and check paid authorizations' exact purpose
and binding. Unassigned native WorkerPayment remains denied. Earned source-service
fees and safe refunds retain their separate existing conditions; they do not
acquire a successful-worker Positive/P prerequisite through this fixture.

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
setup; it is not a pure inference kernel benchmark. The role-process measurement
retains a small local Miner fixture. Separate pipeline tests call A2's actual
bound inference/signing and preserve that exact signature across A3/A1/A5.
A3's local PanelBuilder port is connected; its remote provider/delivery path is
tested separately as a private fixture rather than real network delivery.
One warm 4x4 sample is **not** a real LLM, cold/provider, minimum-hardware,
concurrency, p95/p99, storage-retention or native-network measurement.

The test-only prefix journal replays checked frames in a new process. It is not
A6's atomic service/request/attempt/funding journal, power-loss durability,
reorg, pruning or IBD. Missing those integrations stays unverified.

Remaining evidence: real provider/cold data delivery and native signed duties;
A4 rooted draw/carrier/deadline/funding admission;
A5 current Gate C and every actual reward/DAG/settlement/dependency read; A6 atomic
recovery; real LLM and concurrent-network limits and explicit production caps.
No stub grants native credit. Activation remains disabled.

The [trained-artifact profile audit](test-compatible-llm-audit.md) records why
the inspected existing trained artifacts cannot run under the current initial
all-node-i32 checker profile. It is claim-profile compatibility, not model
registration/quality/tier review. No real LLM throughput is inferred from toy
measurements or from older registration/conformance successes.

Known old-example handoff: the former `positive-court-llm` prototype example used
the yardstick context without filling the registered tokenizer. Nonzero-tokenizer
admission was reproduced as failing in the old trial. A0 retained this as a repair
requirement before that example may be used for real LLM measurements.
