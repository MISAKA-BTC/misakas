# Native owner ordering audit — 2026-11-03

This is an implementation audit under accepted RFC-0045/ADR-0213 and
RFC-0046/ADR-0214, not a new design or an activation decision. All observations
refer to the A0 integration checkout at this snapshot. They identify required
connections; no missing owner is treated as successful native admission.

## Metadata fee precedes coinbase validation

`utxo_validation.rs` records the actual accepted fee and its residual after
mandatory bond/escrow destruction in `palw_record_accepted_carrier_fees_v1`.
It then adds `palw_collectible_transaction_fee_v1` to the source block's reward
and derives the finality fee from the same collectible amount when the old
fence requires it. The accepted-transaction sequence and these rewards populate
`ctx.mergeset_acceptance_data` / `ctx.mergeset_rewards`.

`verify_expected_utxo_state` uses those rewards for validator/reserve allocation
and `verify_coinbase_transaction`. In `processor.rs` the new raw Assignment
rehearsal/final native fold currently follows that verification. Therefore a
registration burn written only into the later PALW delta cannot implement the
RFC-0046 fee rule. The miner would already have been checked against the wrong
collectible fee. A raw R fee declaration or a floor in an aux row is insufficient.

Required connection: the actual current-prefix native R qualification and
successful/idempotent charge allocation must be known before all consumers of
collectible fees (including source-block reward, finality classification,
round-income accounting, template construction and expected coinbase). Its
actual retained outcome must then be rebound in the final native checkpoint,
with the UTXO/state/rows/raw material. A failed R keeps its ordinary fee semantics;
an exact successful replay must not allocate the registration charge twice.
The available amount is residual native fee after other mandatory destruction,
not raw input-minus-output reused by two owners. Local resource/history failure
must abort/defer, never become semantic refusal or a fabricated charge.

This does not authorize bypassing normal Full scripts or changing the old fee
rule on any shipped None preset. No R fee or live R admission is implemented by
the current pure consent/policy/template code.

## Original contract routing survives current intake selection

`NativeAssignmentRulesV2::resolve` currently selects the current intake fence and
body limits. Original policies are already retained in the Open material, but
routing future op9/joins/settlement/Case events solely through current intake
would still refuse old contracts after a changed or absent current selection.

The completed implementation must distinguish new intake from continuing an
actual rooted contract. Continuations need a bounded identity scan followed by
actual original material/history lookup, the original codec/budget/profile and
fresh current-prefix role/funds/P/duty validation. Missing original material is
Unavailable. The live contract's funding relay profile is fixed once at original
A intake: later relay activation neither upgrades old cooperative A/C/join nor
removes RequiresR from an old contract. No fallback based on current Params is
permitted. The still-closed A/C path currently accepts no such contract.

## Future verifier registry work

The existing pool predicate cuts registration maturity at actual C commit,
not draw DAA: `registered_daa.saturating_add(maturity) <= commit_daa`.
For exclusive Result cutoff `R`, the latest valid commit is `R-1`. If this is
below maturity, no registration can be eligible. Otherwise define
`cutoff = (R-1)-maturity` with checked arithmetic and require
`cutoff < signed_anchor_daa` for this resource profile.

Then later accepting blocks (even with DAA equal to the anchor) cannot add a new
potentially eligible registration. The resource universe is every registry row
at the actual immutable anchor, including currently inactive/zero-stake rows.
At-intake eligible count is not this bound. The actual draw still filters its
current status/stake/operator and uses the unchanged globally sorted weighted
draw; the universe must never become a frozen approved roster.

This arithmetic is only DATA. Native still needs authenticated anchor ancestry
and state, monotone accepting chronology, an audit of every registration/import
route, priced actual universe/index/replay work and real global/local leases.
The sole production `BondRegistered` constructor in `objects.rs` uses the
accepting `ctx.daa_score`; retirement/capability/slash writers clone existing
records. Genesis/snapshot/delta restoration and all alternate imports still need
complete provenance review. A registration-date range index can bound draw
queries; a digest-ordered index scanning every later registration cannot.
The inherited partial P-index candidate is not applied.

## Accounting session must also cover nonpositive outcomes

The private Case cache's current `session()` extraction requires a checked
terminal fact and positive Final. That is adequate only as a prerequisite for
the positive P approval path. No-Witness, EndedUnverified, failed/vetoed/expired P,
failed candidates and safe service/refund accounting must have a genuine replay
session without inventing a positive fact. Original finite closed-ingress fraud,
retention and liability owners remain required. Do not make refunds depend on
fabricating PositivePassed or shorten existing P/fraud windows to get a session.
