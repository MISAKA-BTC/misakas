# RFC06/08 pre-object auxiliary sweep caller reservations

Status: coordinated build and five focused parents PASS; see [final verification](0006-0007-0008-0012-court-aux-provider-round-verification-2026-10-25.md).

## Actual consumers

Four existing shared pre-object consumers constructed caller scratch before
their keyed writers could admit it: expired free-prompt keys, grouped panel
obligation retirement, due reporter reward copies, and expired reporter
commitment keys. Both actual block fold and pre-object rehearsal reach these
functions. Their existing gates, epoch placement, comparison boundaries,
grouped-retirement rules, awards, counters and journal order are unchanged.

The three single-Vec sweeps count their actual borrowed eligible rows before
collection, only when tracking is enabled. A typed envelope of four slots per
element (minimum eight for a nonempty collector) covers the existing geometric
growth, including simultaneous old/new buffers. No legacy collector or output
algorithm is replaced. Panel retirement quotes all actual locks and liabilities
before constructing its grouping map. One typed BTree membership per lock
conservatively bounds the distinct claim groups. Eight grouping entries per
lock cover singleton minimum capacities and larger groups' buffer growth;
separate envelopes cover the pruned-lock and pruned-liability vectors. Group
removal and iteration move their Vecs rather than cloning them.

The transition construction owner holds this scratch reservation through the
phase's decisions and downstream row/journal writes. Individual writers still
admit their own old/new values, indexes and journal. A refused phase request is
local NativeResource before any target sweep write or reward counter change.
Early dormant/epoch skips occur before reservation. Tracking off preserves the
old collectors and avoids the added counting pass.

## Prepared controls

Core lib filter: `palw_state_v2::tests::aux_sweep_workspace_v2`.

Four parents independently exercise the actual prompt, panel, reward and
commitment sweep. They measure Q for the phase scratch, require 0/Q-1 refusal
with unchanged state/root/journal, distinguish Q scratch admission from the
larger writer budget, and retry with sufficient capacity. Enabled/flags-off
state, root and complete journal bytes agree; actual delta apply/revert restores
the state roots. The fixtures include expiry boundary survivors, grouped and
orphan panel locks, a real legacy reward-row award and forgone counter, and
commitment index updates. Their construction owners remain charged after
collection consumption and release on builder drop. A fifth parent verifies
dormant/same-epoch skips under zero quota.

These are stored-row mechanism fixtures with the parent already constructed
outside the target quota. They are not signed claim/conviction admission,
public reporter funding, all reward-to-coinbase acceptance or an authenticated
court campaign. The legacy reporter fixture leaves native reporter-income and
settlement-once policies dormant; existing active-policy regressions remain
separate. Panel rows are explicit stored inputs, not evidence of independent
validator behavior.

## Remaining scope

This admits the four consumers' caller scratch. Nested reward/native-income
consumers, other court/aux callers, retained outputs and physical allocator/RSS
bounds have separate contracts. It does not close all models' costs or the
whole reward lifecycle. No public policy, activation, model tiers or selected
claim-verifier approval changes. K3 inputs remain uncreated, native39 settlement
mapping unformulated, and the 15 real campaign gates UNKNOWN.
