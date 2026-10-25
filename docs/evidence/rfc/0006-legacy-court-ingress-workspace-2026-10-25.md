# RFC06 Legacy court ingress workspace — 2026-10-25

## Status

The root-coordinated six-package build and all three focused parent tests now
**PASS**. Starting committed HEAD is `1257f0ae8`. The preparation lane did not
run Cargo or access a database; root subsequently built, pinned and executed the
results below. No deployment, policy approval or activation was performed.

## Actual allocation and consumption path

The `Legacy` arm of `court_ingress_admission_v1` invokes either
`paid_court_cap_admits_v1` or `legacy_court_cap_admits_v1`, then burns the adopted
or explicitly private test ingress fee. It does not use the previous wave's
active-capacity pool/scheduler phase.

Before this change, the paid cap built an eligible inflight
`Vec<(u64, Hash64)>`, sorted/truncated it and collected reserved `Vec<Hash64>`.
The follow-on cap built a `BTreeSet<(claim, inside_side)>` over actual sessions.
Each inside-side check also rebuilt a producer/panel-seat `Vec`, including in the
session scans. Finally `slash_bond` copied the opener bond before its individually
admitted keyed writer.

The real Legacy ingress arm now admits these caller temporaries before invoking
the existing cap functions or fee writer. Its transition construction owner keeps
the ticket through their actual consumption, the fee journal and existing output
splitting. Raw cap functions retain their API and authority; raw callers outside
this specific mutable ingress remain separate work. The active-capacity arm does
not receive this Legacy reservation.

The producer/seat side is now borrowed, preserving direct bond membership and the
same registered operator-id or payout-payload linkage test. No side `Vec` needs
reservation. The original inflight filter is factored into one borrowed iterator
shared by the collector and its preflight count; sort, earliest-claim truncation,
reserved-slot rule and follow-on exemption remain unchanged. The absent-lane raw
collector still returns immediately without scanning claims.

There is **no funded-claim set** on this Legacy call graph. Funded-original
admission and other callers of the historical funded set are not silently included
in this scope. The earlier active-capacity borrowed membership change is retained.

## Type-sized request

The new leaf uses actual eligible claim count A, reserved count
`min(A, existing reserved_claim_cap)` and actual running-session count S. Separate
existing typed Vec envelopes admit old/replacement tuple and reserved-key buffers;
the possible output allocation is charged even if the standard library reuses the
input buffer. At most S first-side BTree memberships are admitted. A private
nonzero fee schedule also admits the actual opener bond's deep input copy. The
keyed bond writer and journal keep their own separate reservation.

This is a conservative branch envelope: an early reserved-slot success can avoid
a later first-side set, but admission does not assume that success in advance.
Overall-cap refusal still precedes every allocation/fee and needs no caller scratch.
Below the outsider lane, or direct evidence with the paid schedule absent, the
unchanged no-allocation path needs no reservation. Counts/types are not a serialized
ratio, a minimal resident-heap estimate or an all-model physical memory bound.

A target phase quota refusal occurs before any state, fee or journal write. Later
public object/transition errors publish no partial builder. No in-place rollback
of a private builder after a later writer/allocator failure is claimed.

## Focused controls (runtime PASS)

Package `kaspa-consensus-core`, lib filter:

```text
palw_state_v2::tests::legacy_court_ingress_workspace_v2
```

Three parents exercise:

1. The actual Legacy ingress with and without a private fee schedule. Native folded
   registry/attempt rows plus a stored running outsider session force follow-on
   accounting; the real inflight/reserved list is asserted nonempty. An already
   constructed builder receives the target checker. Capacity 0 and Q-1 preserve
   state/root/journal and charge nothing. Q admits caller scratch only, not the
   writer/journal budget. A sufficient-capacity retry admits and burns exactly the
   fee specified for that single admission; tickets release on builder drop.
2. Active Legacy ingress with local tracking off/on has identical result, fee,
   state/root and journal Borsh bytes. Actual delta replay/undo restores roots.
   Legacy ingress creates no active-capacity row.
3. Borrowed side classification matches the old Vec implementation, including
   duplicate seats, operator-only and payout-only linked identities and a missing
   bond. Zero-quota dormant/direct-evidence cases retain their no-fee behavior;
   overall-cap refusal remains semantic and unchanged even with a private schedule.

The fixture's extra bonds register distinct payout destinations through the actual
registration objects, so the session/opener are real outsiders under the existing
classification rule. Stored session/panel variations test accounting semantics;
they are not authenticated court acceptance or a completed verdict/campaign. The
private fee override is thread-local and reset by an RAII guard. Fixture construction
and parent copying precede target quota installation and are not covered by this
phase's request.

## Remaining boundaries

Other raw cap/claim-admission callers and funded-original set consumers remain
separate. This patch does not expand active-capacity admission, replace writer
reservation, approve an ingress amount or claim all auxiliary/court RAM coverage.
Public activation None, normal bootstrap/unapproved policy refusals, fixed model
tiers and selected-verifier claim approval are unchanged. K3 formal inputs remain
uncreated, native39 settlement mapping unformulated and campaign gates UNKNOWN.

## Root execution verification

Six-package no-run (Core/Consensus/Node/EthRPC/SDK/EVM, same EVM/private features and Core opt0 as the prior wave) **PASS 249.81s**. All 16 source/config identities matched before and after compilation. Six executable copies were independently preserved with APFS copy-on-write and checked by SHA before runtime. Existing Cargo target files can change without changing these pins.

Three new Legacy parents **3/3 PASS**, zero failed or ignored. Seven related groups also pass: outsider-lane semantics5, paid ingress schedule2, court capacity4, active admission3, deadline2, funded reservation1 and historical open/close1. Exact binary/name deduplication yields **20 distinct PASS**, including the three new tests. The funded-reservation test overlaps the capacity selection. Outsider semantics ran in21.15s; remaining groups each completed within0.03s harness.

Source/command record: `/tmp/misaka-rfc-legacy-court-ingress-wave-source-20261025/record.json`. Build log: `/tmp/misaka-rfc-legacy-court-ingress-wave-build-20261025.log`. Immutable binary SHA manifest: `/tmp/misaka-rfc-legacy-court-ingress-pins-20261025.json`. Per-group exact test names and results: `/tmp/misaka-rfc-legacy-court-ingress-runtime-results-20261025.json`.

Core immutable executable: `/tmp/misaka-rfc-legacy-court-ingress-kaspa_consensus_core-20261025`, SHA-256 `c2aa2c53cc295e4f199b5d71a535adb0cdb88248699dd9b5c7b7e6988fb22b39`. This actual Legacy ingress coverage closes these caller temporaries; it does not close funded-original/attempt raw cap consumers or all-model/physical memory/campaign scope.
