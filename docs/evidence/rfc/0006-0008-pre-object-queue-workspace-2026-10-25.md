# RFC06/08 pre-object consumed queue workspace — 2026-10-25

## Status

Coordinated build and new **3/3** runtime parents PASS; existing source-economics
**9/9** controls also PASS. See [verification record](0006-0008-0012-queue-session-verification-2026-10-25.md).
This follows committed `764afef77`; its prior capacity/parent/head runtime did
not validate this new source. No deployment or activation was performed.

## Actual consumer and preserved semantics

`palw_state_v2/block_preparation.rs::prepare_block_objects_v1` serves both
`palw_v2_pre_object_base_retained_v1` and the actual block fold. It previously
collected payout prefix keys and all EVM settlement keys into `Vec`s before the
individually admitted row writers. Writer reservations could not authorize those
earlier allocations.

The new `queue_drain_workspace_v2` admits a typed construction request before the
first key buffer is allocated:

- `P = min(actual pending_payouts.len(), PALW_V2_MAX_PAYOUTS_PER_BLOCK)` and exactly
  `P * size_of::<Hash64>()` for payout keys;
- `E = actual evm_settlements.len()` and exactly `E * size_of::<u32>()` for settlement
  keys.

Both `Vec`s use `try_reserve_exact` before `extend`, so there is no growing
collector whose capacity policy must be guessed. The request arithmetic uses the
existing checked typed request builder. These are key arrays, with no nested heap
payload. The buffers are sequential; the conservative **sum** remains admitted in
the transition construction owner until that owner finishes or drops. This is
logical admission, not a minimal simultaneous-heap or RSS estimate.

The order remains payout prefix in BTree key order, existing source verifier
fee/refund ACK, then all EVM settlement rows in BTree key order. The ACK still
reads the same consumed payout journal and keeps its separate existing workspace
and validation. No positive economics policy or new ACK authority is introduced.
Each payout/settlement writer still reserves its own journal/mutation. At finish,
the existing transition owner splits actual state/delta outputs; this patch does
not replace that ownership contract.

A quota refusal of the new phase request occurs before any drain write or ACK.
The public transition/rehearsal returns no new published state on any later error;
private builders are discarded on failure. This does not claim that a physical
allocator or later row-writer failure rolls back an already-mutated private
builder in place.

## Focused controls (runtime PASS)

Package `kaspa-consensus-core`, existing lib target; filter:

```text
palw_state_v2::tests::queue_drain_workspace_v2
```

Three parents cover:

1. An already-constructed builder receives the independent target-phase checker.
   Actual phase rejection at 0 and Q-1 preserves state/root/journal. Q admits only
   key scratch; it is explicitly not the full row/journal/block budget. A retry
   with sufficient capacity drains the real obligations and releases the held
   construction ticket when dropped.
2. Cap+2 stored payout rows and three reverse-inserted EVM rows compare the actual
   phase against the pre-change flags-off collector. Prefix, remaining backlog,
   settlement order and old-row values, journal Borsh bytes and state roots agree.
   The construction owner stays live after both key buffers are consumed.
3. Public pre-object rehearsal and actual block fold consume the same stored
   obligations. Actual delta replay/undo restores their states/roots; an empty
   phase succeeds at zero quota.

The fixture contains stored owed queue rows and refused EVM settlement rows. It is
not an actual market fill or a source-economics fee/refund funding fixture. ACK
placement is preserved by the actual caller, but these new parents do not prove
a new positive economics ACK scenario. The existing private economics test helper
`source_verifier_economics_v2.rs::common_prefix` now calls this same actual drain
instead of reproducing the payout loop and ACK. Its existing positive principal
refund and fee-queue saturation/exactly-once ACK controls will therefore exercise
the shared consumer in the forthcoming regression. They remain existing synthetic
private mechanism tests, not three additional new tests or approved policy/campaign
evidence. Their runtime is **PASS**; the relevant existing group is
`palw_source_verifier_economics_fold_v2::tests`.

Initial fixture construction is outside
the target phase's quota, deliberately, so earlier parent allocation cannot stand
in for the tested queue admission.

## Remaining scope

This closes the two consumed-key buffers only. Parent state copies, individual
writer/journal reservations, source ACK scratch and retained result owners have
separate existing contracts. Whole payout/aux/court lifetime closure, physical
memory costs and all-model bounds remain broader work. K3 formal inputs remain
uncreated, formal native39 settlement mapping remains unformulated, and the
15-gate campaign remains UNKNOWN. Public activation stays None, normal bootstrap
and unapproved policy refusals, two model tiers and selected-verifier claim
approval are unchanged.
