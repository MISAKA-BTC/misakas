# RFC-0006 held-GDN candidate/demand geometry — 2026-10-25

## Actual consumer and ownership

The actual held-filer open path previously found candidate heads by repeatedly constructing
native demand vectors until a helper refused them. It silently stopped each head loop after
4096, and neither the candidate vector nor returned demand vectors retained logical admission.
This continuation uses the existing borrowed `PalwGdnStateGeometryV2` and actual qualified Q36
node slots before allocating the candidate inventory. Transition-first, then convolution,
global-slot/head order is preserved. An unsupported geometry produces typed Resource instead
of a partial inventory interpreted as exhausted or honest. The silent head limit is removed.

For the profile's actual recurrence geometry, let H be its value-head count, Nδ the number of
layer-qualified Q36 gated-delta slots, and Nc the number of layer-qualified Q36 convolution
slots. Candidate count is `Nδ*H + Nc*Hc`, where Hc is one for FullConvV1 and H otherwise. Each
slot's layer must occur in the signed recurrence-layer iterator. Candidate bytes are this
checked count times the actual `Base0GdnCheckpointCourtV2` size, admitted before fallible exact
allocation. The persistent owner remains in the case and drops after the candidate vector.
No demand helper is called to discover candidate head bounds.

At actual candidate loading, the selected demands have count
`dδ=ceil(v_dim/delta_rows_per_chunk)` or `dc=ceil(conv_kernel/conv_rows_per_chunk)`. Both divisors
come from the existing checked native geometry. The envelope admits the actual returned
`PalwHeldMissingV1` vector, an intermediate selected `u32` index vector, and two layer-list
`u16` vectors bounded by the actual profile layer count. Each push-built role uses Rust's
pinned Vec minimum nonzero capacity, power-of-two growth and old/new-buffer overlap; these
are typed array roles, not a serialized-byte multiplier or physical allocator bound. Native
result cardinality and returned capacity are checked against the quote before publishing it.

Admission precedes the actual Base0 demand helper, whose pure demand construction reads these
indices/lists. The temporary owner remains live throughout that helper. Persistent demand and
existing answer-slot owners survive its return in the case. New demands, slots and all owners
are constructed before replacing the cursor/demands/answers/demand clock, so a refused next
candidate preserves the current case for retry. Initial candidate Resource propagates out of
open as an error, never as an honest `None` or a settled/exhausted case. Historical String
wrappers retain their compatibility; the retained caller preserves typed Resource.

## Verification status

The parent's coordinated pinned actual node binary passed the complete held-filer group
**18/18 in 58.41 seconds**, including all three new geometry parent cases, in
`/tmp/misaka-rfc-gdn-geometry-full-held-tests-20261025.log`. The geometry quota child passed
**1/1 in 1.22 seconds** and is included within its parent count. Older answer/action children
also run within their existing parent cases; they are not additional distinct geometry cases.
Actual pinned binary: `/tmp/misaka-rfc-gdn-geometry-fp-context-node-20261025`, SHA-256
`9d5c014927bc2251e5fda4ca18608a6c5ba505307c7e91b5a6dda9d6e850b3e8`.

No Cargo invocation was started by this worker. Parser checks for both existing modified
files, rustfmt on only the two new leaves, and scoped `git diff --check` passed. The docs
checker passed with 43 baseline missing references and zero new regressions; that is not a
clean strict audit.

Exact new filter: `palw_gdn_held_filer::tests::geometry_owner_tests` (three parent cases; one
isolated quota child is included within its parent count). The first uses the actual tiny
producer and real open/load path, compares the full multihead transition/convolution inventory
against native demand readers, and drives last-head transition and convolution loading. The
second explicitly tests only borrowed geometry/inventory: FullConv head collapse and an
inventory with 4098 heads retain the final head instead of silently truncating. Its modified
profile/map is not claimed to be a registered backend execution or native acceptance result.
The third uses a real opened case with authenticated answers, exhausts quota before candidate
construction and actual demand loading, checks no kernel invocation or cursor/answer/clock
change, then releases quota, retries successfully and checks persistent owner lifetimes through
case retirement. The recorded runtime result above is from the pinned binary at this source generation;
previous answer/action binary results are not substituted for these new controls.

## Explicit remaining boundary

This closes the named candidate and demand-array roles only. Earlier Base0 leg/capture
reconstruction, initial forged-prefix discovery, recomputation/state-chunk copies, authentication
scratch, refutation construction and its nested tile/path rows, and generic case-book/host-history
allocation remain separate work. Holding the demand-array ticket across Base0 replay does not
quote that replay's allocation. No bound is invented for an unknown prefix or full refutation,
and existing capture/output/answer owners do not retroactively cover those roles.

The controls are small mechanism tests, not K3 or all-model cost measurements. Formal K3
specification/manifest/weights/signed inputs remain uncreated; the fifteen campaign gates remain
UNKNOWN. Public activation remains None, ordinary selected-source/V6 bootstrap refusal and the
model tiers are unchanged, and claim-level selected-verifier approval remains mandatory.
