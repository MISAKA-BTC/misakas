# Positive Court — full completion requirements

Goal received 2026-11-02: implement the complete seven-stage G14/Positive Court
flow in the attached `pasted-text-1.txt`, not merely merge workspaces or pass
the leaf tests. The attached proposal uses its original RFC-0042 name; this
checkout's Positive Court implementation is RFC-0044 / ADR-0212. Existing
RFC-0042 and its wire meanings remain separate.

The eight-workspace integration is `ece4dc472`; its subsequent runner fix is
`f8bb847c9` and evidence checkpoint is `fc8ee811a`. Their selected regression
results are preserved in [the integration record](workspace-integration-20261102/README.md).
Those results do not prove this full objective complete.

The governing RFC's twelve activation gates A01–A12 and forty-four acceptance
cases T01–T44 are recorded individually, with their required evidence and
unproven native status, in
[completion-requirements-20261102.json](completion-requirements-20261102.json).
It pins the objective attachment and specification hashes. Conditional direct
probabilistic profiles must remain explicitly unsupported until their own
conditions are implemented; the initial EXACT path is not permission to omit
native credit, recovery, role or measurement requirements.

## Required end state and evidence

| Stage | Requirement | Authoritative evidence required | Current gap |
| --- | --- | --- | --- |
| 1 | Preserve old G14 signatures, adjudication, responsibility, settlement and recovery when Positive is disabled | Old-claim/native histories replay identically at the integrated implementation; actual selected source, commands and state/fund comparisons | Earlier selected regressions exist; final integrated native regression and full consumer/history comparison remain required |
| 2 | Separate Miner Result, Panel Witness and finite source obligations; authenticate each role; atomically reserve capacity, funds and state | Actual native admission from independent registered job/worker/policy, real signatures, UTXO locks and one journal/checkpoint; rejected/duplicate attempts leave consistent funds and reservations | Shared DTOs and local adapters exist. Actual independent job/role authority and the one native writer are missing |
| 3 | Cold Node independently derives and checks all mandatory relations, connectivity and final output | Accepted native flow using A1, without an executor/model store on the Node; last-relation omission, changed intermediate, model/input/branch substitutions rejected | Pure narrow/wide checkers, role fixtures and all 213,298 original short-job checks now complete in a separate sandboxed Node process with original model/fixture access denied. Actual native accepted context, production role services and all required enclosing reservations remain unconnected |
| 4 | Miner performs ordinary inference plus bounded source responses; Panel replays once, records and submits; resume/transfer preserves global budgets | Actual Miner/Panel services and G14 carriers, instrumented zero mandatory Miner witness construction, direct-to-Node source completion, bounded canonical request unions and durable local spent counters | The original short job now completes in distinct Miner/Panel/Node processes, preserving the Miner signature, zero Miner witness runs and one Panel capture. Kaspad producer/verifier and actual native source-duty service remain unconnected to an accepted flow |
| 5 | Integrate Fraud/Positive intake, transport, deadlines, responsibility, fees, Gate C and every credit reader | Actual P-selected certification plus full local coverage and existing per-use entitlement at reward, receipt mint/spend, DAG live/safe/final weight, Rule E, settlement/EVM, prefix and dependency reads; unresolved fraud and maturity remain effective | `CreditConsumers` is an adapter API. Native factory, P lifecycle and production consumers are not connected; local DTOs/flags cannot fill the gap |
| 6 | Multiple Nodes, restart, Panel replacement, reorg, IBD/pruning and dependent-claim cancellation reproduce state and money exactly | Real accepted block/carrier history, shared atomic undo and DB restart/IBD/pruning delivery; no repeated payments, defaults or replenished request/attempt budgets | Real DB journal fixtures exist, but no full node admission/role/fund writer to exercise all these transitions |
| 7 | Measure actual LLM Miner/Panel/Node costs and enable only explicitly approved finite profiles | Pinned original model/job/arithmetic; separate cold/warm role CPU/RAM/IO/wire/storage/recovery costs, maxima and tail latency, global block/epoch/concurrency and Fraud protection; actual permitted activation after review | Original Pythia ordinary inference, one-pass Panel capture, complete signed record and all 213,298 checks plus EOF/root/signature complete in shared and separate sandboxed-Node short-job measurements. Miner/Panel/Node CPU/RSS and final Node work/provider/wire counters are observed in three address spaces. Physically cold caches, canonical/full-context, native/global/concurrency/recovery and tail costs plus approved activation remain absent |

All seven stages remain part of the goal. A small paid Canonical Assignment
path, if adopted, is an intermediate native connection. It does not by itself
finish primary lightweight mining or grant issuance, DAG weight, Rule E or
other entitlements. No permanently unavailable adapter, success stub, toy
reservation, empty dependency fallback or saved pass bit is completion evidence.

## Mandatory cross-stage acceptance cases

- Honest Miner: Result and necessary source data only; Panel builds/submits the
  witness; model-unprepared Node checks all required relations to completion.
- Malicious Panel: authenticated false intermediate is `WitnessRejected`,
  without Miner calculation guilt. Panel abandonment after successful source
  delivery does not reverse completion or create Miner delivery default.
- Collusion: false signed output from registrant/Miner/all Panel cannot receive
  credit; omission, reordered/duplicate relations, coherent foreign model/input,
  and another branch's pass are independently rejected.
- Request attack: overlapping ranges, changing Panel/provider keys and retries
  do not increase the all-Panel source union, work, bytes, fees or attempt caps.
  Response authenticity is established at the Node without a private Panel ACK.
- Admission failure: resources, funding, rows, receipt/once markers and journal
  commit or undo together, including failures after a partial internal prepare.
- Capacity protection: consumed usage plus unfinished source and other-role
  reservations fits each actual pool; interval/deadline opportunities and
  physical leases remain valid. Positive load cannot consume protected Fraud
  capacity. Panel failure/replacement cannot release pending source duties.
- Recovery: requests, real local spent I/O, attempts, reservations, source
  completion, fees, payments and responsibility retain their distinct lifetimes
  through restart/reorg/IBD/pruning and dependent-claim invalidation.
- Credit: Positive is necessary and insufficient. P-selected `Certified`,
  independent complete verification, dependency, fraud, maturity/liability,
  carrier/funding and use-specific existing entitlement must all hold. Source
  service fees and safe refunds follow their separate earned-right predicates.
- Compatibility and delivery: the native admission ruleset selects the new
  form; a Miner flag or legacy fallback cannot bypass Positive. Its control
  carriers must be funded and usable before its own computation credit exists.
- Observable results: settlement, RPC/SDK status and audit logs preserve
  `PositivePassed`, `WitnessRejected`, `ClaimRefuted`, `EndedUnverified` and
  independently qualified service/default outcomes, including their authors.

## Source-confirmed native connection gaps

These are executable-path observations, not conclusions based only on handoffs:

1. `processor.rs` retains `PositiveCarrierPhaseTwoV2` in
   `_positive_native_phase_two` and has no native Court consumer. Its own comment
   explicitly states that no admission, lock, row mutation or credit is created.
2. `ConsensusApi::palw_positive_source_duty_v2` has an unavailable default. The
   current Kaspad source adapter calls it; no actual consensus implementation
   overrides it to produce a qualified current-view source session.
3. The old Kaspad kernel producer still constructs its legacy committed trace
   and evidence. The old verifier's `Clean` path is not native Positive intake.
4. `OriginalAggregateDraftV1::reservation_readiness_v1` remains typed
   unresolved. It must not be changed to a fabricated `Ok` or treated as an
   admitted escrow/service capability.
5. The new credit consumers explicitly depend on an assigned native result
   binding. `UnavailableNativeCredit` refuses all eight uses; a local
   `RootedCreditView` does not supply a production native factory.
6. The bounded `LoadedWideArtifactV2` Panel bridge requires an already verified
   full step binding before model startup. The trace-free metadata/parameter
   owner supports ordinary inference and a private dense comparison, and
   now delegates directly to the existing R5 one-pass producer without a prior
   dense oracle. The independent synthetic cold-file path completes full Node
   checking with one Panel run. Native service integration and actual trained
   Panel/Node measurements remain required; this local bridge is not native
   Assignment authority. See [trace-free bridge validation](trace-free-panel-20261102/README.md).

## Current implementation increment

The source/Court shared-budget gap is now guarded inside
the existing replay API by a required historical remaining-capacity resolver and
checked post-transition usage plus outstanding demand. Actual source replay
and arithmetic regressions are recorded separately under
[remaining-capacity-20261102](remaining-capacity-20261102/).
That local enforcement does not supply the missing native policy/role/funding,
physical capacity, service-time opportunity or accepted writer.

## Native-assignment design adopted — 2026-11-02

The user explicitly answered “両案を採択し、実装を進める” to the paired
native-assignment RFC/ADR adoption question. The adopted documents are
[RFC-0045](../../docs/rfc/0045-positive-court-canonical-assignment.md) and
[ADR-0213](../../docs/adr/0213-positive-court-canonical-assignment.md).
This authorizes implementation of the bounded Canonical TIR Assignment
authority, role consent, purpose funding/reservation, single Result consumption
and poster-funded payment described by both documents. It does not authorize
public activation or declare the full seven-stage objective complete.
The previous drafts remain as historical source. Namespace collision checks,
finite policy/preimages, actual native atomic state/fund/history integration,
full coverage plus separate P certification, recovery and measurements remain
required. Adoption is no longer a blocker for this implementation.

## Actual trained measurement checkpoint

The original pinned Pythia short job completes ordinary inference and one Panel
capture with matching output. Its first checkpoint refused before record/Node;
that original result remains in
[trained-roles-20261102](trained-roles-20261102/README.md).

The required working-to-retained-output handoff now releases completed executor
backing without changing the outputs or legacy API behavior. Actual retained
run requests fall from 2,349,455,644 to 1,308,868 bytes. The complete signed
213,298-relation Panel record now fits its real owners and writes 305,797,608
aggregate tape bytes. Actual Node submission recomputes a prefix; the entire
Node verdict, isolated cold-role costs, native claim and activation remain
unqualified. Each actual refusal/progress result and the full remaining scope
are preserved in [retained-output-20261102](retained-output-20261102/README.md).
These private finite operator settings do not supply native resource/funding
capabilities or approve any production activation.


## Exact-evaluation budget diagnosis

The original model's relation-1,752 `Budget` refusal is now traced during its
same single direct evaluation: 3,881 computed elements, 131,376 reduction terms,
135,038 source reads and 617,184,640 workspace request bytes. The finite
131,072-term local ceiling missed upstream work; this is neither a completed
Node result nor an arithmetic fault. Explicit diagnostics preserve old errors,
coverage and cumulative charges, including false-Panel/false-Miner and closed
retry behavior. The private caller's next 262,144-term bound remains inside
its existing 1-GiB workspace owner and the same 8-GiB global ledger. No native
policy/credit/activation is supplied. See [evaluation-diagnostics-20261102](evaluation-diagnostics-20261102/README.md)
for the actual failed run, regression results, later measurements and all
qualification boundaries.

The measured term allowance crosses the former failure and logs at least 3,072
of 213,298 numeric checks before SIGXCPU under the unchanged 240-second CPU
limit. Its prefix records 102.305 s of producer material preparation and 51.407 s
of Node submission. No complete record authentication, full Node verdict or
native outcome was produced; performance of the complete unchanged checking
path is the next required work, not a reduced relation set or relaxed credit.

## Meaning-preserving runtime improvement

Private opt2 builds preserve debug assertions and overflow checks. Under the
same 240-second CPU limit, the unchanged ordered-membership path logs 26,624
numeric relations; producer-only hash membership logs 40,960, both out of
213,298. At their common 26,624 prefix, producer preparation falls from
172.502 s to 98.332 s and Node submission remains approximately 56.8 s.
Canonical material/cumulative-charge regressions, 35 selected tests and strict
selected Clippy pass. Both actual trained executions still terminate with
SIGXCPU and produce no full Node verdict or native outcome. Source/parameter
ordering, arithmetic, coverage, owners, finite work caps and credit conditions
are preserved. See [optimized-runtime-20261102](optimized-runtime-20261102/README.md)
for exact source/build/input pins, failed build and disk admission records,
retired generated-cache provenance, raw logs and qualification limits. No
production setting or activation was changed; all seven stages remain required.

## Profile-guided empty scratch reuse

A ten-second actual trained-worker sample identifies repeated producer table
growth and membership hashing as significant work. The builder now retains
only empty pending/table capacity under its unchanged simultaneous frame
owner. Success and each early return clear both structures. Reused versus
fresh-builder/seed materials and all per-material charge fields agree; an
actual partial frame-cap failure spends the same work again on retry. All 36
selected tests and strict selected Clippy pass.

Under the same finite caps and 240-second CPU limit, the actual unchanged
trained job logs 49,152 / 213,298 numeric relations before SIGXCPU. At the common
40,960 prefix, material preparation improves from 150.472 s to 115.060 s while
Node submit remains approximately 87.5 s, with equal provider use, evaluator
counter peaks and logical requests. It still produces no complete Node or
native outcome. See [the actual CPU sample](source-profile-20261102/README.md)
and [scratch-reuse evidence](trace-scratch-20261102/README.md). This advances
measurement/performance work while preserving every native, recovery, role,
certification, physical-cost and activation requirement above.

## Separate full-cost diagnostic and complete-plan copy allowance

An additional, explicitly gated private test retains all 213,298 original
relations while allowing finite 1,440-second CPU, 8-GiB actual file reads and
2^24 read calls. The standard 240-second window remains. Actual cumulative
completed elements/terms/source reads and final EOF/signature timing are now
observable without replaying evaluations. These diagnostic bounds do not
change any production profile or grant native rights.

The first actual extended execution completes exactly 65,529 numeric relations
and then refuses a material's conservative copy charge against the old 2-TiB
cumulative ceiling; exit 101 and the failed test remain preserved. All spent
charges are retained. The next extended window derives
`213,298 * (2 * 16 MiB) + existing 2 TiB` from the original expanded plan,
yielding 9,356,116,492,288 symbolic copy/IO bytes. These are not actual file
traffic, RAM or native fees. It preserves all other evaluation, source,
attempt, tape, actual-read and resource bounds. Corrected selected runtime
tests (36) and strict Clippy pass. The corrected actual run is monitored
separately; its historical running snapshot does not prove a complete outcome.
See [extended-cost evidence](extended-cost-20261102/README.md). Full trained
checking, isolated role costs and every native/integration/activation condition
above remain required.

## Actual late-position evaluation limit

The corrected-copy extended run completes exactly 106,198 numeric relations,
then its same direct evaluation attempts 16,385 elements against 16,384 at
position 9 / block 1 / node 163. Actual terms (5,280), source reads (10,339),
authentication and workspace quote (807,985,600 bytes) remain within their
bounds. Exit 101 and incomplete record authentication are retained; this is a
work-budget refusal, not an arithmetic fault or native result.

Only the extended private diagnostic now permits 32,768 elements, with the same
real 1-GiB workspace owner and 8-GiB whole-operation ledger. The pinned formula
and compiled memo envelope put its conservative full-target request below
that owner. The standard window and all other bounds remain. All 36 selected
regressions and strict selected Clippy pass. A fresh corrected measurement is
running with exact source/input/process pins, without native authority or an
approved activation profile. The full completion conditions are unchanged;
see [extended-cost evidence](extended-cost-20261102/README.md).

## Complete local trained short-job measurement

The actual corrected r3 process exits zero after 976.730 s and checks all
213,298 original relations, reads every claim, verifies the exact record
EOF/root/signature and returns local `Matched`. It performs ordinary Miner
inference without a witness run, then one Panel capture. Actual material
preparation is 546.715 s, Node submission 424.063 s and final record
authentication 0.00691 s. Original model/fixture and compiled binary SHA values
match before/after, source matches runtime/Clippy, all guards release and the
process group is empty. Earlier budget failures remain failures in their
own records. See [the complete short-job result](extended-cost-20261102/complete-short-job-r3.json).

This completes the local original noncanonical 16/1 job measurement under its
explicit diagnostic bounds. It does not qualify isolated cold-role costs,
canonical/full-context execution, actual native authority/role/funding/P,
every credit consumer, real-node recovery/dependencies or activation. All
seven stages and every native gate/acceptance case remain part of the goal;
a complete local replay does not replace selected-validator certification
under its explicit honest-fraction assumption.

## Separate sandboxed Node: complete original short job

The same original noncanonical 16/1 job now completes all 213,298 relations and
record EOF/root/signature in a new Node process whose original model/fixture
reads are denied by the OS. The parent and child exit zero after 982.396 s;
source/input/binary pins agree with 38 selected regressions and strict selected
Clippy. Observed Node submit time is 423.387 s, its CPU checkpoint 427.962 s,
and parent-to-Node delivered bytes 5,795,421,305. The unchanged 8-GiB global
request ledger includes the child's real 2-GiB whole-operation subledger;
all leases release and no process remains. Failed compile, sandbox and lint
attempts stay preserved as failures. See [isolated-Node evidence](cold-process-20261102/README.md).

Only Node is a separate process; dormant executor code remains in the test
binary, the parent still hosts Miner/Panel sequentially, OS caches are not
physically cold, and private framing is not an adversarial peer codec. Final
provider/evaluator cumulative counters are not emitted by this isolated
branch. The full native authority/funds/P/all-consumer/recovery/dependency
flow and approved activation remain unfinished. All seven stages and all
individual native A01–A12/T01–T44 obligations above remain in scope. No saved
local Matched or model availability observation replaces selected-validator
certification or authorizes a new native success path.

## Three role processes and final work/IO counters

The unchanged original 16/1 job completes in separate Miner, Panel and
sandboxed Node processes at code `723cfd79d`. Miner signs its own result and
performs zero witness runs; Panel authenticates it against independently
fixed local expectations, preserves that signature and captures once. Node
checks all 213,298 relations and final EOF/root/signature. All three exit zero
after 988.973 s; 39 selected outer tests and strict selected Clippy use the
same source map, original inputs and binary match, and all leases/processes
release. Actual CPU checkpoints are Miner 0.413385 s, Panel 556.272722 s and
Node 432.076332 s. Final evaluator totals, provider reads and complete wire
counters are now emitted, rather than inferred from a prefix. See
[three-role evidence](three-roles-20261102/README.md).

This remains a local trusted diagnostic using fixed dependency/service
commitments, with no native source contract, funds/P or accepted writer. Full
canonical/context, native costs, recovery/concurrency/tails, all consumers and
activation remain required. All seven stages and A01–A12/T01–T44 stay in scope.

A separate public-preflight diagnostic reproduced an old compatibility gap:
under the same shape-only header, existing S/V certification rows decode while
existing K/H rows fail `unallocated aux row namespace`. The actual compiled
Core and SHA pins are recorded in `three-roles-20261102/legacy-cert-scan.json`.
It is not a real accepted-history proof. The next safe stage-1 correction is
complete existing K/H bounded codecs/inventory plus meaningful malformed-row
and legacy regression coverage. This does not authorize a new P row or change
old signatures, IDs, roles, policy or money. The independent native assignment
RFC/ADR adoption question remains pending.

## Existing K/H correction and initially interrupted verification

The exact existing kernel certification K and held-reward H row types now have
bounded codecs/inventory at `27a371268`. Three selected Core tests and the
recompiled original preflight counterexample pass before the shared generated
artifacts disappear. K/H bytes and IDs retain their existing meanings; unknown
P/version/malformed shapes refuse. This is codec compatibility, not full legacy
accepted-history/fund/recovery equivalence or native Positive admission.

Broader regression and static checks hit the actual disk floor and did not
pass. Shared debug/native artifacts subsequently disappear and the free space
recovers. A fresh original Cargo regression build, including actual native
dependencies, is live; its exact process/handle and still-required checks are
recorded in [the codec evidence](legacy-cert-aux-20261102/README.md). No missing
binary/archive is restored and declared fresh. The earlier complete three-role
measurement retains its pre-correction source qualification. The full seven
stages and every A01–A12/T01–T44 native obligation remain unproven/in scope.

## K/H correction: fresh verification complete

After the shared generated artifacts disappear, the original regression
sources and native dependencies are rebuilt normally in a new target with
functional opt0, debug assertions and overflow checks. All 42 selected outer
runtime cases (39 existing + 3 codec cases) and strict selected Core/Exec
Clippy pass on the same source map at `27a371268`. Native archives/bindings
are actual new compiler outputs with recorded SHA values, not missing cache
files backfilled as fresh. All groups are empty; failed compile/disk-floor
attempts remain failures. See [the final source audit](legacy-cert-aux-20261102/final-source-validation.json).

This verifies the existing K/H codec compatibility increment. It does not
prove full native legacy money/history/recovery equivalence or any new
Positive authority/P/consumer/activation. All seven stages and individual
A01–A12/T01–T44 native requirements remain in scope and unproven. The older
three-role model performance measurement retains its own exact source pins.


## Native legacy G14 checkpoint — 2026-11-02

Eight selected real Consensus/RocksDB legacy signature/fraud/certification/
settlement/reorg/IBD/restart cases pass, including a new regression that closes
the final DB owner and reopens persisted files in pending and settled states.
Canonical state bytes, K/H rows, actual UTXOs/queues, collateral and monetary
conservation are checked. Test-only erased heap-future boundaries fix stack
overflows without changing default stack or production rules. All initial
failures and exact final sources are preserved in
[legacy-native-history-20261102](legacy-native-history-20261102/README.md).
This does not qualify every legacy history or complete any new native Positive
authority, P/credit, service/fund writer, recovery or activation obligation.


## Adopted Assignment contract/metadata increment — 2026-11-02

RFC-0045 / ADR-0213 now have result-free terms/bundle/three-role signature
contracts, A versus future-C bounds, a nonce exhaustion transition and an
independent native-registry metadata/assigned-Result reader. Four contract
cases and eleven Core metadata/binding cases pass on one final source map;
strict selected library/test Clippy passes. The default-stack opt2 Core build
and all actual binaries are pinned in
[assignment-contract-20261102](assignment-contract-20261102/README.md).
These are data derivation/authentication checks: accepted-prefix eligibility,
policy/fence/carrier/row/restore schema, actual leases, native funding locks,
Assignment single-writer/once/history, Case/P/consumer integration, recovery,
measurements and activation remain required. Staged policy/fence/consent code
outside the checkout is preparation, not implemented or tested runtime code.


## Assignment finite policy/consent/fence increment — 2026-11-02

The complete fixed Assignment policy, separate fired ruleset recipe, bounded
source/prover declaration preimages and dormant Some-only Params fence are
implemented. Six contract cases and three fence cases, plus strict selected
library/test Clippy, pass on one source map. Six shipped preset fingerprint
triples exactly match the actual pre-change binary baseline, including T12.
See [assignment-policy-20261102](assignment-policy-20261102/README.md).
This does not resolve native capacity/service/P branch bounds, actual carrier
locks, atomic accepted rows/history, Result single consumption, P/credit/fund
consumers, native recovery, full measurements or activation. The full goal is
active and incomplete; a valid dormant policy is not native success.


## Assignment/r1 rule-scope compatibility correction — 2026-11-02

The separate Assignment ruleset candidate at 67dd5f28b was incompatible with
existing Result-row validation and is withdrawn. The native admission-clock
resolver commits all fired policy contents and omits future companion values,
then uses the exact unchanged r1 native ruleset recipe. Six contract plus five
Core fence/row regressions and strict selected Clippy pass on one source map.
The old candidate is explicitly rejected by the unchanged Result-row check;
see [the correction evidence](assignment-ruleset-correction-20261102/README.md).
This corrects the earlier policy increment; it supplies no accepted native
Assignment or funds/Case/P/credit/activation. All remaining stages persist.


## Assignment codec/funds/rows/DB and native boundary checkpoint — 2026-11-03

The shared contract (68), focused Core targets (62), and actual Consensus
legacy/storage/filter targets (11) pass: 141 executed cases, with exact source
maps and 19 actual executable hashes in
[the integration evidence](assignment-native-integration-20261102/README.md).
Strict contract/Core/Consensus library Clippy, selected test Clippy and all
Consensus test-target typechecking pass. Native tests use default opt2,
debug assertions/overflow checks and the normal stack. Native 11 comprises
OLD G14 8, actual RocksDB DATA persistence 2 and lock-rejection 1; none is a
successful new Assignment/Case/P/payment acceptance test.

Append-only operations8–13, aux151 rows10–18, typed A/C/prover UTXO qualification
and shared locks, role/nonce/use/accounting structural restore, Assignment DB
journal and private checkpoint writer are implemented. The actual native
reservation/P/Case owners remain missing and reject. The original-policy
custody, typed real construction lease, per-ordinal raw final checkpoint and
same-UTXO-batch material outcome changes in `candidates/` are UNAPPLIED and
UNCOMPILED; the 141 passed cases do not cover those candidates. A newly found
metadata-helper assumption rejects distinct registered work/operator keys;
the correction/regression is a candidate, not a passed runtime fix.

A further actual payout gap is documented: ordinary payer P2PKH scripts still
need a real future signature. Safe balance/floor calculations cannot generate
that signature or guarantee withdrawal when the payer stops cooperating.
The [relay RFC draft](../../docs/design/palw/integration/positive-native-relay-rfc-draft-2026-11-03.md)
and [paired ADR draft](../../docs/design/palw/integration/positive-native-relay-adr-draft-2026-11-03.md)
propose separate prior paid/custodied signature material, operation14 and
bounded terminal fallback/conditional partial progression. Both remain
UNADOPTED until an explicit paired decision under PRINCIPLES§11. Public
activation and old-account migration are separate decisions. Work on already
adopted Case/P/source/funding/recovery integration continues; the full goal and
RFC45 remain incomplete.
