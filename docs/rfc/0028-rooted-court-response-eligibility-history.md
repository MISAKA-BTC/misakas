# RFC-0028 — rooted court response eligibility history

- Status: **Draft / Proposed / Review Required** (2026-10-21).
- Decision companion: [ADR-0195](../adr/0195-rooted-court-response-eligibility-history.md).
- Governing contract: [RFC-0026](0026-court-policy-master-contract.md), [ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md).
- Implementation status: design and information-loss counterexample; marker runtime path is not implemented by this document.

## 1. Required fact

For a reserved packet describing a concrete native response stage, the chain must be able to determine whether that stage's prerequisites were accepted before the first supply block for its assigned DAA began. A current response cursor, aggregate unused quota, wall clock, mempool observation or producer flag does not establish this fact. Demand opening time does not timestamp a later accepted Begin/Part.

The new native duty view identifies the actual original claim/allocation/source, owner-scoped demand/turn, literal Ready custodian and current body cursor. It intentionally has no invented predecessor-acceptance timestamp. The retained slot/chunk formats do not currently contain that timestamp. An external replay of retained block journals could reconstruct it, but is not supplied by the current state-only adapter.

## 2. Proposed separate marker

The recommended candidate schema adds a separate versioned marker, leaving service v2 and old native formats unchanged:

| Namespace | Proposed append-only addition |
| --- | --- |
| Staged key tag 13 | `CourtDutyClock(duty_id: Hash64)` |
| Staged row tag 19 | `CourtDutyClockV1` |
| Payload version | 1 under its own marker domain; not service payload v1/v2 reinterpretation |

These are local candidate reservations, not public wire/schema adoption. X3 container key 11 / row 17, service key 12 / row 18, lifecycle tag 230 and every old variant remain unchanged. There is no new lifecycle kind or carrier signature authority in this proposal. New readers continue to accept old supported snapshots without a marker; they must classify marker-dependent historical eligibility as unavailable. Old readers are not claimed to understand snapshots containing the new appended variants.

The fixed-size marker needs the following data, derived by the native fold rather than submitted as a producer DTO:

- marker version, service profile ID and concrete duty ID;
- original claim, native allocation/source and actual acquisition key;
- literal answerer, actual demand ID and turn;
- actual predecessor accepting block ID and DAA;
- first eligible service DAA, derived by checked successor of that accepting DAA;
- original response deadline, original hard bound and original material-retention horizon.

Exact field order must be fixed with Borsh/typed-heap/allocation-free scanner tests before candidate implementation is considered verified. An auxiliary marker authenticates history/data identity, never correctness of model computation.

## 3. Recording rule

For a claim already bound to the candidate service profile, compare actual native response duties immediately before and after a successfully accepted transition. When Demand/Reacquire opens a response or accepted Begin/Part changes its next concrete stage, record that stage's marker in the SAME native journal transaction as the original row/chunk changes.

No marker is created merely by observing an unchanged cursor again, by a later unrelated WorkDebit, by reading a snapshot or by a node's local RPC. Existing markers are immutable; a different purported prerequisite for the same duty must not replace them. Finish/collection or actor completion must not be read as evidence that an earlier unused supply slot was eligible. The original completion and liability rules remain separate.

A binding installed after an old stage already exists does not reveal its historical prerequisite DAA. Without a verified historical derivation, mark it unavailable; do not stamp the installation time as the old acceptance time. Supporting preparation/default duties and non-ORIGINAL routes needs the corresponding actual transition provenance, not this response adapter's name reused for them.

## 4. First-block convention

The existing reservation component counts only the first accepting block at each DAA. A response stage whose prerequisite becomes accepted at DAA `d` cannot be certified as block-start eligible in that same DAA's first supply block. Its earliest counted service unit is `d + 1`, checked for overflow. This is a service-accounting convention, not a new minimum response latency: already-valid multiple native controls may still be accepted at the same DAA or block under the existing protocol.

Additional blocks at the same DAA cannot retroactively add another counted first opportunity. Skipped DAA have no supply. Absolute clock progress continues even when supply or a marker is missing. A valid response accepted too late to leave another service unit before its unchanged deadline may yield a marker with no usable future interval; that must not reject the valid response, clip/extend a deadline or count a nonexistent opportunity.

## 5. Packet correspondence and responsibility

A positive aggregate `ServiceReservations::assess` is resource accounting only. Before normal-opportunity reasoning, a future adopted adapter must bind the reserved packet/template to the concrete logical request, role, stage, cost and branch, and check its marker against the assigned supply unit. The marker's profile, original scope, predecessor and service interval must match. Unknown marker, missing supply or ineligible start cannot be silently promoted to provided normal opportunity.

Markers do NOT authorize slash or prove that a response could have traversed the COMPLETE legal path. A party's deliberate delay of an earlier stage cannot automatically become a defense just because the final stage has no later eligible unit. Conversely a party cannot be blamed for capacity or authenticated prerequisite failures outside its adopted duties. The full causal graph and end-to-end obligation, every legal latest-response branch, actual reserved supplies, admitted CPU/IO/memory/codec costs and explicit delivery/inclusion assumptions remain necessary. Current denial-only default stays in place until that contract is reviewed and adopted.

Original acceptance still needs complete reservation including all permitted hostile branches. Incremental reservation only after a stage appears is insufficient for F03. An owner-scoped response marker is not an owner-independent shared-frontier packet. The complete canonical template and public shared ingress are separate required work.

## 6. Atomicity, retention and load validation

Native action writes and new marker writes must either both commit or both roll back. A quota, heap, scanner or index failure after computing a marker cannot expose a partially applied response or silently omit its marker. Preflight the entire change set or use a verified native savepoint; tests must force failure between the planned writes.

Use rooted journal entries and deterministic index updates. Undo/replay of Demand/Begin/Part must undo/replay marker provenance, including block ID, with no local-cache dependency. Reorgs that accept the same logical stage at a different block/DAA must recover the corresponding selected history. New marker readers must enforce fixed version/key/scope/clock shape; exact current-stage validation at creation and contextual scope checks on snapshot load are both required.

Retain required marker history while the original service binding is unresolved, including past ordinary TTL where the existing hold requires it. Retaining a historical fact does not extend a foreign signed custody promise. Terminal archive/release and pruning cannot be invented here; F07/F08 still govern them. Every marker write, lookup, scan, index and retained byte must be priced from the complete profile; there is no free hidden journal or zero-cost assumption. Batch validation can be optimized only with equivalent scope/atomicity checks.

## 7. Required verification

1. Execute the early/late accepting-context counterexample through native response transitions, showing equal retained TIR cursor/current duty but different prerequisite timing relative to a service unit. Keep fixture/node-history scope explicit.
2. Preserve all existing key/row encodings and t12 identity. Verify exact new marker codec, preallocation scan and typed heap together.
3. Record markers only on real stage changes; no restamping on unchanged state, reread, restart or WorkDebit. Old snapshots/bindings remain explicitly unavailable where provenance is absent.
4. Test same-block controls, repeated DAA, no block/skipped DAA, exact deadline boundaries and a valid late response with no remaining future interval. No deadline/cap increases.
5. Force quota/index/codec failure and prove atomic action/marker rollback. Reorg, native carriage reload and complete/pruned IBD retain the same required facts under their respective actual storage scopes.
6. Test packet-stage/profile/actor/turn/branch substitution refusal. Do not count aggregate supply before a prerequisite as normal opportunity; do not equate late prerequisite with innocence without its causal path.
7. Keep preparation, shared ingress, full graph, original admission, physical calibration, signer availability, R-D2-NODE and T01–T15 open until their own evidence proves completion.

The current quiet-devnet observations do not calibrate the new costs or a guaranteed reserved service rate. No numeric fee, turn duration, mass limit, model restriction or public activation is introduced by this Draft.
