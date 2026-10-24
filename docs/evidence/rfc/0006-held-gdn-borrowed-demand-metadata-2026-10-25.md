# RFC-0006 held-GDN borrowed demand and recurrence metadata — 2026-10-25

## Named production boundary

The retained v4 convolution builder previously called the native demand collector only to
compare its newly allocated result with demands already borrowed from the held case. This
created a selected-index Vec and a replacement-demand Vec before the complete convolution
role request. Computing its recurrence offset also constructed native attention/recurrence
layer metadata without a separate ticket. The previous
[convolution input-owner record](0006-held-gdn-convolution-input-owners-2026-10-25.md) explicitly
left this boundary open; its historical results are not results for this continuation.

The native collector and the new borrowed check share a single chunk visitor: the same actual
convolution source layer, block order, native chunk index and checked recurrence offset. The
borrowed check compares each exact `StateChunk { checkpoint, chunk }` in place, including length
and final ordinal. It neither sorts nor constructs replacement units. A mismatch remains an
Evidence refusal rather than an honest verdict. Unsupported native layouts retain their typed
refusal; new request/index arithmetic overflow is Resource, with no fallback layout.

The actual `base0_gdn_conv_window_evidence_from_disclosures_retained_v4` calls this check before
its full convolution role quote. Legacy collectors remain available and use the same visitor;
unrelated historical callers do not acquire a second metadata ticket through that interface.
No wire encoding, Params, identity, cadence or public activation rule changes.

## Metadata admission and lifetime

The native recurrence dispatcher and the new retained metadata helper share one map-kind
selection. The helper first borrows native geometry, derives actual attention-layer count A
and recurrence-layer count G, and admits the native `u16` layer-vector envelope before invoking
the existing layout constructor. Held and hybrid layouts account for A and G; composed layouts
account for A; the solo layout has no layer vectors. Existing dispatch determines offset/count
and remains authoritative for covered-cadence refusal.

For a nonzero layer count n, the logical bound is `2 * (C + C/2)` bytes when
`C = next_power_of_two(max(n, 4)) > 4`, or `2*C` otherwise. Zero needs no reservation. This
type-sized conservative push-growth envelope includes old/new allocation overlap. It is not
an exact allocator overhead, minimal allocation, RSS or model-cost measurement. Addition and
capacity arithmetic are checked. Independent attention/recurrence envelopes are added before
construction. Held returned layout values are declared before the lease and therefore drop
first; temporary hybrid/composed metadata retires inside the native constructor while that
lease remains live. No metadata owner crosses an await or a mutable source handoff.

## Verification status

The parent's Core build passed in **3m23s** in
`/tmp/misaka-rfc-conv-demands-output-ids-core-build-20261025.log`. The nine frozen Core Rust
source hashes were unchanged through that build. Pinned binary:
`/tmp/misaka-rfc-conv-demands-output-ids-core-20261025`, SHA-256
`ff4a5c6f48db93792c0bd7518097c3ced5979822f5f7ae656956e65740c1e123`.
The metadata filter passed **2/2 in 0.02 seconds**, including its isolated quota child
(0.00 seconds), in `/tmp/misaka-rfc-demand-ids-core-layout-tests-20261025.log`. The child is
included in the parent count, not an additional independent test.

The parent's node no-run build passed in **5m07s**, with all fourteen frozen source hashes
unchanged. Pinned binary: `/tmp/misaka-rfc-borrowed-demand-node-20261025`, SHA-256
`61e7ca3978dd3d4a0a7bf54ba0720906e0d2c3922de1823385d0fe3ea765883b`.
The focused node filter passed **2/2 in 2.78 seconds**, including its isolated child
(1/1 in 1.43 seconds), in `/tmp/misaka-rfc-borrowed-demand-node-focused-20261025.log`.
The full held-filer regression passed **28/28 in 75.91 seconds** on the same pinned node binary,
in `/tmp/misaka-rfc-borrowed-demand-node-held-regression-20261025.log`. This full group includes
the two new focused tests; the overlapping focused/group counts are not added. Child results
are included in their parents, not independent additional counts.

Build duration is not a model cost measurement. This worker has not invoked Cargo. New leaves were
formatted individually, modified production files passed parser-only checks, and
`git diff --check` passed. Docs checker passed with 43 baseline missing references and zero new
regressions; that is not a clean strict audit. Static checks alone are not runtime acceptance.

Core filter: `recurrence_layout_owner::tests`, two parent tests with an isolated quota child
included. A geometry-only constructor uses the existing Qwen36 profile definition and native
covered-position predicate; it does not load weights or run model work. It compares retained
offset/count/layer arrays with the existing native dispatcher and rejects an unknown map and
request overflow. The quota child calls the metadata helper itself with a fully occupied budget,
requires typed Resource and unchanged profile, then retries and checks that returned native
arrays retain their ticket until the layout retires. An earlier Base0 leg refusal cannot satisfy
this targeted control.

Node filter: `palw_gdn_held_filer::tests::borrowed_demand_tests`, two parent tests with an isolated
child included. The actual graph-v8 held producer fixture compares first/last-head native
collector output with the borrowed check, rejects absent/extra units and foreign checkpoint,
chunk or head, and uses its genuine answered convolution case. Its complete-head geometry has
**dc=1**. This is not a multi-block reorder runtime test.

The actual retained-step child passes a foreign demand with authenticated stored answers,
requires refusal without candidate/demand/answer/clock changes or signing, restores the original
unit, then requires typed Resource and unchanged case under a full quota. This caller quota
control may refuse at an earlier batch/leg; it is explicitly separate from the Core metadata
control. After budget release the real retained caller must reach Accuse and sign once. These
are small mechanism controls, not multi-process native acceptance or an executed campaign.

## Remaining scope

Initial leg rebuild and rank-cache admission retain their independent tickets. Complete
convolution input/range-opening roles and accused-witness copies retain their existing owners
through actual court and signed-output handoff. This continuation removes the demand-equality
copies and covers the named recurrence metadata construction only. It does not claim that the
whole filer is reserved before its first allocation.

Transition predecessor replay/witness construction, arbitrary prefix state construction,
authentication scratch outside the quoted constructors, generic case-book/history maps and
PerDecodeCall served-chunk/frontier reconstruction remain separate boundaries. No arbitrary
prefix bound or all-model cost is assumed. Formal K3 specification/manifest/weights/signed inputs
remain uncreated; all fifteen campaign gates remain UNKNOWN. Public activation stays None,
ordinary selected-source/V6 bootstrap refusal remains, and the frozen model tiers and selected
verifier claim authentication are unchanged.
