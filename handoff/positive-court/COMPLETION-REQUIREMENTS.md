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
| 3 | Cold Node independently derives and checks all mandatory relations, connectivity and final output | Accepted native flow using A1, without an executor/model store on the Node; last-relation omission, changed intermediate, model/input/branch substitutions rejected | Pure narrow/wide checkers and role fixtures exist. Actual native accepted context and all required enclosing reservations are not connected |
| 4 | Miner performs ordinary inference plus bounded source responses; Panel replays once, records and submits; resume/transfer preserves global budgets | Actual Miner/Panel services and G14 carriers, instrumented zero mandatory Miner witness construction, direct-to-Node source completion, bounded canonical request unions and durable local spent counters | API/process fixtures and short trained ordinary runs exist. Kaspad producer/verifier and native source-duty service are not connected to the new accepted flow |
| 5 | Integrate Fraud/Positive intake, transport, deadlines, responsibility, fees, Gate C and every credit reader | Actual P-selected certification plus full local coverage and existing per-use entitlement at reward, receipt mint/spend, DAG live/safe/final weight, Rule E, settlement/EVM, prefix and dependency reads; unresolved fraud and maturity remain effective | `CreditConsumers` is an adapter API. Native factory, P lifecycle and production consumers are not connected; local DTOs/flags cannot fill the gap |
| 6 | Multiple Nodes, restart, Panel replacement, reorg, IBD/pruning and dependent-claim cancellation reproduce state and money exactly | Real accepted block/carrier history, shared atomic undo and DB restart/IBD/pruning delivery; no repeated payments, defaults or replenished request/attempt budgets | Real DB journal fixtures exist, but no full node admission/role/fund writer to exercise all these transitions |
| 7 | Measure actual LLM Miner/Panel/Node costs and enable only explicitly approved finite profiles | Pinned original model/job/arithmetic; separate cold/warm role CPU/RAM/IO/wire/storage/recovery costs, maxima and tail latency, global block/epoch/concurrency and Fraud protection; actual permitted activation after review | Short ordinary/dense self-consistency is recorded separately. Full Panel/Node trained-model measurements and approved activation profiles remain absent |

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
