# A5 source due arithmetic proposal

`misaka/palw/positive-accounting-node/src/dues.rs` is a bounded, node-local DATA
calculator. It defines no wire format, identifier, domain, reservation ledger,
native constructor, funding permission, or state writer. A0 adopted the shared
intent in `c2e93ec49`; A4's actual native transaction remains required.

## Existing boundaries used

- A2 `consensus/core/src/palw_positive_source_v2.rs` emits a
  `ServiceReceiptRow` only for its first unique canonical unit. Its latest pure
  preparation already includes first-receipt source due in `AggregateUsage.fee`.
  A5 must recheck that complete update rather than increment its fee again.
- A2/A4 must classify logical duplicate op3/op4 only AFTER actual rooted
  authentication and literal checks and BEFORE Case charging or history append.
  Copying the same RequestId/signed literal op3, or an already accepted unit's
  identical literal op4, into a different carrier changes no history, retained
  bytes, work, transmitted bytes, fee or other usage. Carrier identity is not
  new consent. Only an actually reallocated one-shot opportunity can authorize
  a distinct chargeable operation. New/different authored semantic failures or
  retries are not blanket fee exempt.
  Ordinary transaction acceptance, native fees and transaction history remain
  independent of this Case-level NoOp.
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
The seven-field type now lives in `positive_contract::funding`; `dues` re-exports
it so existing node imports keep the identical shared type. It remains DATA with
no serialization, wire/domain allocation or native authority.

`prepare_source_due(result, case, funding, source_usage, receipt, existing, terms)`
returns `SourceDueProposal` containing optional delta plus expected/new usage and
receipt DTOs. It requires the pre-due fee to equal the old case fee, then adds
the source amount with checked arithmetic. Other source meters remain intact.
Duplicate receipts add zero and cannot replace the first provider, duty,
response, unit or settlement reference. A receipt already carrying a settlement
reference cannot create a new due. `settlement: None` is permissible but is not
delivery evidence. Court `EndedUnverified` does not erase delivered Miner source
service. No funding leg, settlement reference, paid amount or payer balance is
changed by this module.

`recheck_source_due_update(result, case, funding, SourceDueUpdate)` handles A2's
already-incremented next usage and shared fee delta. It projects only fee back
to the old case fee, independently recalculates via `prepare_source_due`, and
requires exact equality of all next usage fields and the optional shared delta.
A claimed fee of 12 remains 12, not 24. Altered provider, scope, amount or absent
delta is rejected. An existing receipt with old usage and no delta remains
unchanged. The struct groups these DATA inputs without introducing new wire.

Generic arithmetic preserves supplied source meters for a distinct new
authenticated one-shot opportunity; it does not authorize that opportunity.
Logical NoOp classification remains A2/A4-owned and cannot be inferred from
receipt existence or an A5 caller flag. An op3 NoOp needs no receipt/due call;
an op4 NoOp emits no source fee intent and must retain the entire old usage.

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
They also distinguish unchanged NoOp DATA from distinct-opportunity meter DATA,
and reject double-incremented or altered A2 update proposals. Actual cross-carrier
authentication and no history append must be tested by the A2/A4 owners.
Fixture origins are existing codec-example DATA with deliberately unsigned rows;
they do not model native acceptance. Build verification is assigned to the root's
single builder; this subtask ran only `rustfmt --edition 2024 --check`.
