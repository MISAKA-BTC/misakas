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
| 3 | Cold Node independently derives and checks all mandatory relations, connectivity and final output | Accepted native flow using A1, without an executor/model store on the Node; last-relation omission, changed intermediate, model/input/branch substitutions rejected | Pure narrow/wide checkers, role fixtures and complete local checking of the original trained short job exist. Isolated cold trained Node, actual native accepted context and all required enclosing reservations are not connected |
| 4 | Miner performs ordinary inference plus bounded source responses; Panel replays once, records and submits; resume/transfer preserves global budgets | Actual Miner/Panel services and G14 carriers, instrumented zero mandatory Miner witness construction, direct-to-Node source completion, bounded canonical request unions and durable local spent counters | API/process fixtures and short trained ordinary runs exist. Kaspad producer/verifier and native source-duty service are not connected to the new accepted flow |
| 5 | Integrate Fraud/Positive intake, transport, deadlines, responsibility, fees, Gate C and every credit reader | Actual P-selected certification plus full local coverage and existing per-use entitlement at reward, receipt mint/spend, DAG live/safe/final weight, Rule E, settlement/EVM, prefix and dependency reads; unresolved fraud and maturity remain effective | `CreditConsumers` is an adapter API. Native factory, P lifecycle and production consumers are not connected; local DTOs/flags cannot fill the gap |
| 6 | Multiple Nodes, restart, Panel replacement, reorg, IBD/pruning and dependent-claim cancellation reproduce state and money exactly | Real accepted block/carrier history, shared atomic undo and DB restart/IBD/pruning delivery; no repeated payments, defaults or replenished request/attempt budgets | Real DB journal fixtures exist, but no full node admission/role/fund writer to exercise all these transitions |
| 7 | Measure actual LLM Miner/Panel/Node costs and enable only explicitly approved finite profiles | Pinned original model/job/arithmetic; separate cold/warm role CPU/RAM/IO/wire/storage/recovery costs, maxima and tail latency, global block/epoch/concurrency and Fraud protection; actual permitted activation after review | Original Pythia ordinary inference, one-pass Panel capture, complete signed record and all 213,298 local Node checks plus EOF/root/signature now complete in the shared-process short-job measurement. Isolated/cold role, canonical/full-context, native/global/concurrency/recovery costs and approved activation remain absent |

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

## Design decision awaiting the user

The existing [native-assignment RFC draft](../../docs/design/palw/integration/positive-native-assignment-rfc-draft-2026-10-31.md)
and [ADR draft](../../docs/design/palw/integration/positive-native-assignment-adr-draft-2026-10-31.md)
are concrete proposals for independent job/worker/funding authority. They remain
unadopted. The repository's [freeze procedure](../../docs/governance/design-freeze.md)
requires explicit adoption of both before the new authority can be implemented
as an accepted native success path. The active goal does not turn a draft into
an already adopted design. The adoption question is pending; work within
existing accepted contracts continues.

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
