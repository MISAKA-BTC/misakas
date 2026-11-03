# Original trained job in three role processes — 2026-11-02

Code: `723cfd79d5934a6e250babdfe0d9fab808cd2ecf`.
This is progress toward stages 4 and 7 of the
[full objective](../COMPLETION-REQUIREMENTS.md), not native completion.

## Role separation and resource ownership

The parent is now Panel only. It starts a separate Miner executable process
before loading model metadata. Miner loads the same pinned original artifact,
runs the existing ordinary inference API once, signs its literal result and
returns that result/key/signature/generated IDs. Its entry does not call the
Panel builder, capture or witness APIs. Panel independently reconstructs the
fixed local job/worker expectations and checks key, signature, every bound
identity, execution settings and the generated-ID commitment. It does not
re-sign the received result. Panel then loads its own original weights,
captures once, produces the signed record/materials and uses the existing
separate sandboxed Node process for all direct relation checks and record
EOF/root/signature verification.

Miner exits successfully BEFORE Panel starts. The parent reserves 16 MiB
for all returned-role/control/IPC owners and the Miner child's complete
`8 GiB - 16 MiB` subledger before spawning. These fill the same 8 GiB request
ledger; the child enforces that real subledger and all its library owners.
After exact child exit its whole lease drops; the 16 MiB control owner remains
through Panel/Node completion. The earlier Panel/Node 128 MiB encoder and
2 GiB Node subledger remain additive in the same global 8 GiB ledger. Every
lease survives its actual owned data/process lifetime and must release.
These are hierarchical software request bounds, not physical/kernel quotas.

Miner CPU is capped at 240 s. Panel and Node each have 1,440 s CPU caps;
these are per-process limits, not a shared CPU quota. The outer full-job
monitor has a 1,800 s wall limit, samples whole-process-group RSS against
4 GiB and enforces a 512 MiB disk floor. Existing 320 MiB aggregate tape,
8 GiB / 2^24-call provider IO and 16 GiB-direction / 2^26-call IPC limits remain.
All original relations, arithmetic, input, class/artifact/layout identities
and max context are preserved. The fixed job is noncanonical 16/1, not
canonical 255/2 or actual 2048-context execution.

Node's final IPC report now emits actual completed elements/terms/source reads,
workspace and request peaks, numerical submit and final authentication times,
CPU/RSS checkpoints and wire bytes/read-write calls. Parent reports final
provider actual read bytes/calls/openings/attempts and aggregate tape bytes.
Private IPC fields are not a new native wire format. Its outer bounds do not
make generic Borsh decoding a production adversarial peer decoder.

## Tests and preserved failures

`runtime-first` failed compilation (a missing direct serde dependency and a
u32/u64 count comparison). No dependency was added; an explicit primitive JSON
diagnostic projection and checked-width comparison corrected those errors.
The formatter's unrelated child-module formatting was reverted. This failed
source map/log is preserved and is not presented as a successful run.

`runtime-r2` first passed 39 selected outer tests. `runtime-r3` additionally
checks authentic re-signed Miner seed/prefill/local-contract substitutions.
It passes 11 full-relation + 4 record + 24 wide-material tests = 39 selected
outer passes; eight explicitly gated cases are ignored. Three additional
small sandboxed child scenarios run inside the actor test and are not added
to the outer count. Bad Panel material remains WitnessRejected, and bad Miner
output is distinguished only after full checking. Strict selected Clippy
(`--no-deps -- -D warnings`) passes on exactly runtime-r3's source map.

`miner-process-first` is an actual original trained ordinary Miner probe. Both
parent and child pass and exit; it produces token 273, authenticates its actual
result signature and logs zero witness runs, a 3,790,308,013-byte child request
peak, 402,760 us user + 21,274 us system CPU checkpoint and 128,008,192-byte
macOS getrusage RSS checkpoint. The parent receives 8,351 bytes. Its aggregate
logical peak is 8 GiB because it reserves the child's entire capacity up front;
this is not actual physical allocation. Both tests are one actual model job.
Source/input/binary hashes and the empty process group are recorded.

The complete three-process original short-job measurement is recorded below.
Earlier running prefixes are not completion evidence.

## Native boundary and a reproduced legacy gap

`native-p-boundary-audit.json` records the source-confirmed distinction between
old S/K/V rows/verdicts and new P. Old `certifies()` also accepts
PublicCheckCertified, so wrapping that helper cannot grant P credit. The new
P value/draw/writer is unassigned; operation7 remains external validation,
and the independent native assignment RFC/ADR are unadopted. No new authority,
P row, activation or success default was introduced here.

`legacy-cert-scan.json` and `operator-scripts/legacy-cert-scan.rs` separately
reproduce a legacy shape-compatibility omission in the exact current compiled
Core: under the same zero diagnostic header, empty/S/V shapes pass while
canonical existing K/H row types fail with `unallocated aux row namespace`.
The exact rlib SHA values are unchanged. This tests shape dispatch, not an
actual accepted block/funding/recovery history. It justifies the next legacy
compatibility correction and does not define a P codec. This 1.49-second
standalone diagnostic compilation ran concurrently on the host; the model
measurement is not a claim of an otherwise exclusive host.

## Remaining completion obligations

Three processes share one local test executable, which contains dormant
executor code even on Node. Only Node's original model/fixture reads are
sandbox-denied; it independently checks pinned metadata and all supplied
relations. `live-fd-inventory-first.json` is a point-in-time actual OS observation
of the parent/Node descriptors and the departed Miner, not all future behavior.
The model SHA checks warm OS caches; physically cold-cache costs are unproven.

These roles use the existing private local diagnostic AdmissionContext with
fixed dependency/service commitments d(21)/d(22). Their equality is not native
authority, dependency resolution or a real source-duty contract. Panel's
bounded local file provider is not a connected native Miner/source service.
No real UTXO holds, accepted writer, P-selected Certified, per-use entitlement,
reward, DAG weight or settlement is created. Real-node restart/reorg/IBD/
pruning/dependencies, canonical/full-context and concurrency/tail costs,
protected Fraud capacity and reviewed finite activation remain required.
All A01–A12/T01–T44 native requirements remain unproven. A complete local
Matched never substitutes for selected-validator approval under its stated
honest-fraction premise. Earlier shared/Node-only measurements retain their
own source pins and qualification boundaries.

## Complete three-process original short job

[The audited actual result](actual-three-process-short-job-first.json) verifies
all ten completion checks against the raw log, runtime, strict Clippy and
original input/binary pins. Parent Panel PID 97153, Miner PID 97156 and Node
PID 97188 are distinct. All three libtest processes pass and exit zero, and
the monitored process group is empty. These are three role-process results
for ONE original model job. Source/input/binary SHA values match before/after.

| Actual observation | Full original three-process 16/1 job |
| --- | --- |
| Overall monitored wall time | 988.973319 s |
| Miner CPU checkpoint | 0.392704 s user + 0.020681 s system |
| Panel CPU checkpoint | 534.952300 s user + 21.320422 s system |
| Node CPU checkpoint | 428.393051 s user + 3.683281 s system |
| Panel material preparation | 546.128178 s |
| Node numerical submissions | 427.499100 s |
| Node final EOF/root/signature finish | 0.006931333 s |
| Completed mandatory relations | 213,298 / 213,298, local Matched |
| Completed evaluator elements / terms | 184,572,795 / 3,036,075,176 |
| Evaluator logical source reads | 3,156,969,214 |
| Maximum evaluation workspace quote | 867,668,864 bytes |
| Node logical whole-operation request peak | 1,829,793,136 bytes |
| Global request peak including reserved child subledgers | 8,589,934,592 bytes (8 GiB) |
| Miner / Panel / Node getrusage RSS checkpoint, macOS bytes | 127,827,968 / 173,178,880 / 15,597,568 |
| Sampled sum of process-group RSS | 186,433,536 bytes |
| Miner result received by parent | 8,351 bytes |
| Parent-to-Node delivered wire bytes | 5,795,421,305 |
| Node-to-parent delivered wire bytes | 213,440 |
| Parent socket read/write calls | 1,066,496 |
| Node socket calls before final report | 1,669,208 |
| Provider requested file-read bytes / calls | 4,447,335,950 / 8,492,230 |
| Provider openings / attempts | 149,358 / 1 |
| Aggregate capture + claims tapes | 305,797,608 bytes |
| Minimum monitored disk free | 1,177,440,256 bytes |

The generated token remains 273 and the original output commitment is unchanged.
Actual final evaluator totals agree with the earlier shared-process original
job. All relation checks, original-root authentication and the final signed
record remain mandatory. Node-to-parent bytes and parent calls include the
final report; Node's emitted call count explicitly precedes that report.
Provider counters measure requested reads at the local API, not physical
disk traffic through OS caches. Metadata and owned-weight reads have separate
bounded startup phase observations; this provider counter is not a total of
all host storage IO. No new native profile, payment, credit or activation is
inferred. Every remaining full-objective obligation above is still required.
