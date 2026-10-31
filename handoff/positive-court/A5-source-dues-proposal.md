# A5 source due arithmetic proposal

`misaka/palw/positive-accounting-node/src/dues.rs` is a bounded, node-local DATA
calculator. It defines no wire format, identifier, domain, reservation ledger,
native constructor, funding permission, or state writer. A0 adoption of any
shared intent and A4's actual native transaction remain required.

## Existing boundaries used

- A2 `consensus/core/src/palw_positive_source_v2.rs` emits a
  `ServiceReceiptRow` only for its first unique canonical unit. Repeated response
  transport may consume counters without a second receipt. Its current pure
  event preparation preserves `AggregateUsage.fee`.
- Identity is exactly the adopted
  `NativeRowKey::ServiceReceipt { case, kind, source, unit }`. Provider, Panel,
  relay and witness attempt never expand the payment identity.
- `ServiceTerms` and `FeeScheduleCeilings` are existing accounting DTOs. Supplying
  them does not establish the adopted schedule preimage or a canonical rate.
- Common Result/Case/Funding shape and cheap scope, funding, policy and plan
  links are checked. DTO shape does not authenticate the accepted prefix,
  signatures, registration, current ancestry or live funding.

## Proposed interface

`SourceFeeDeltaIntent` carries the existing receipt key, full `ResultId`,
`FundingScopeId`, schedule digest, actual-provider proposal, amount, and accepted
response reference. It has no serialization. It is emitted only for a proposed
first receipt; even a zero-rate unit retains once-only receipt identity.

`prepare_source_due(result, case, funding, source_usage, receipt, existing, terms)`
returns `SourceDueProposal` containing optional delta plus expected/new usage and
receipt DTOs. It requires A2's proposed fee to equal the old case fee, then adds
the source amount with checked arithmetic. Other source meters remain intact.
Duplicate receipts add zero and cannot replace the first provider, duty,
response, unit or settlement reference. A receipt already carrying a settlement
reference cannot create a new due. `settlement: None` is permissible but is not
delivery evidence. Court `EndedUnverified` does not erase delivered Miner source
service. No funding leg, settlement reference, paid amount or payer balance is
changed by this module.

`check_source_quote(result, case, funding, whole_schedule_ceilings)` checks the
WHOLE source schedule against the signed source budget, and all Court fees and
burn against total fee consent. It must be used before a native source request,
with an actual worst-case reservation by the owner. It must also be checked on
the resulting case after a due proposal. It is not a per-delivered-unit quote,
and successful arithmetic does not reserve anything.

## A4 application requirements

Within the same native transaction, A4 must resolve the canonical signed history,
first eligible duty, authenticated G14 response and unit, accepted-prefix origin,
registered expectations, drawn/delegated actual provider, adopted schedule/rate,
current funding and remaining available amounts. Reusing one response reference
under another unit must fail its actual G14 decode, even if a raw DTO passes the
calculator's identity checks. Control, carrier, completion and burn fees must not
be inserted as source terms.

The owner must compare its private state generation and the expected old case
usage and receipt absence/value, then atomically install the source rows, due,
funding change, recipient payment, settlement, reservation and journal changes
as applicable. Snapshot DTO equality alone cannot reject rollback ABA. Partial
failure has no native effects. Missing owners cannot be replaced with a caller
flag or a successful callback.

The pure tests cover initial due, duplicate and immutable first-receipt conflicts,
settlement replay, source/court fee separation, meter preservation and regression,
pre-increment rejection, stale snapshot, binding substitution, caps and overflow.
Fixture origins are existing codec-example DATA with deliberately unsigned rows;
they do not model native acceptance. Build verification is assigned to the root's
single builder; this subtask ran only `rustfmt --edition 2024 --check`.
