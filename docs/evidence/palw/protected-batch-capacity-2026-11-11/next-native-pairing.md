# TFP1 / PublicOriginal native pairing

## Current boundary

The current prospective plain Batch compiler binds immutable TFP1 facts to the
companion's versioned service obligation. Op 248 accepts the unsigned complete
FP commitment and native binding header. It has no companion field and no
protected Batch planner caller today. Keep it pre-sign. Exact signed TFP1 bytes,
SignedPublicOriginalV1 bytes and actual accepted transaction identity belong to
native ingress and durable pairing. The old TFP1 body, signature and claim ID
must continue to have their existing meanings.

## Smallest carrier and accepting-fold patch

1. Introduce a distinct bounded funded wrapper magic/version, after checking
   uniqueness. Its inner is an exact supported funded-original carrier
   (existing TFO1/TFO2 bytes and their literal TFP1), plus a fixed-length
   SignedPublicOriginalV1 (WIRE_BYTES = 8,479). Use a fixed read for the
   companion and the existing
   inner bounds before allocating; require strict EOF. Preserve old decoders.
   Structural scan the new wrapper before native allocation and bound both
   framing/mass and physical work before any signature/parse. Gate the new
   wrapper and treat recognized malformed framing as a refusal rather than
   a legacy original. Do not change shipped activation values.
2. Extend one typed dispatch used by all accepted-carrier views: UTXO fee/rent
   check (palw_tir_public_fold_v1::palw_funded_original_candidate_inner_v1),
   funded payout extraction (palw_funded_escrow_v1),
   palw_funded_original_coordinator_v1::palw_funded_original_pairing_payload_v1
   and the node accepted extractor (processor/palw_accept.rs around 3440).
   Keep FPA1/FPR2 wrapper recognition consistent if those outer forms are
   permitted for this profile. TFO1 inspect_carrier_v1 currently unwraps them
   and accepts only literal TFO1 or TFO2; add new wrapper to exact matching.
   Retain the actual native Transaction, not a synthetic rewritten inner
   transaction, for txid/UTXO outputs/funding inspection. Unwrapped bytes are
   only an extractor view. The preflight handoff should borrow or retain
   ownership of exactly one checked instance per actual carrier.
3. Use the shared preallocation geometry from the exact native FP object,
   bound class/container, authenticated tokenizer registration, native class
   record and layout. Derive complete relative graph and profile independently
   of future signatures/companion ID. Quote exposes versioned obligation,
   bounds and full schedule; recompute at actual admitted point with immutable
   relative identity and current service book. The absolute graph uses the
   accepting scope/clock, and quote only succeeds for the complete book. Hold
   oldest valid custody/material/fee constraints on the same claim.
4. At the native per-original checkpoint (palw_state_v2/objects.rs original
   arm around prepare_paired_original_v1/admit_original_v1), construct
   PublicOriginalAdmissionV1 from native facts. Its job/class/artifact are the
   registered binding. Expected miner principal is from the actual FP bond
   key. Scope/prompt/initial state/dependency digests are derived by the
   authorized original profile, never copied from the companion. Check the
   companion's output, trace, context hash, artifact and obligation against
   NativeTfp1PublicOriginalFactsV1; separately retain committed execution root
   and complete fp_claim_id. Resolve PublicOriginalReservationV1 from the
   actual compiled paid graph, native source/supply obligation, existing
   aggregate economics and accepted transaction origin. Existing
   prepare_tfp1_signed_bytes_v1 authenticates and bounds it. Carry its typed
   TFP1 obligation and prepared original together to the link writer. This
   preparation does not book capacity.
5. Use the paired checkpoint to install TFP1 frame, paid book and escrow,
   PublicOriginal admission and an immutable versioned native link, as one
   outcome. The link binds the exact FP claim ID, native Source ID, committed
   execution root, admitted plan/profile and Public OriginalId, exact signed
   TFP1 inner digest, companion carrier digest, accepting block/blue/DAA and
   actual txid/accepted ordinal/event ordinal. Do not rely on separately
   supplied extras that can be written even after the FP object is rejected.
   Compute the raw TFP1 digest from literal bytes, never a reconstructed job
   or the outer transaction containing the companion. Keep the generic
   PublicOriginal.Execution field as job context hash. The new obligation
   commits immutable semantics before signatures; the native link retains
   the accepted carriers afterwards.
6. Add exact cross-row checks to canonical snapshot import, incremental
   journal validation and restore. On restore verify raw carriers against
   canonical accepted history or the authenticated retained exact digests and
   paid rows according to the adopted retention mechanism. Reorg removes
   the whole pairing. Snapshot/undo reject independently supplied admission,
   foreign TFP1 source/root, changed ordinal/txid and same ID with other
   signed bytes. Terminal/profile activation remains contingent on the full
   protected and DA graph.

## Trace subject and inner logits identity

The existing tir_trace_material_codec_v1 is an index plus canonical native
step leaf values. The current wide capture producer's public_trace_material
sets both Manifest Trace subject and content_root to the aggregate committed
execution root. ManifestV1 checks that subject against OriginalCommit.trace_root.
PublicOriginalTraceIndexV1::authenticate recomputes palw_tir_execution_root_v1
from the job context hash, native class, full logits trace root, step leaf
count and step Merkle root and matches the same aggregate subject. Native
TFP1's verified Source has all of those header fields, and Source ID hashes the
entire header.

Keep the companion's trace_root equal to the exact TFP1 committed execution
root for this codec. Retain exact full logits trace root separately in the
immutable TFP1 obligation and authenticated header, and require Trace index
plus each step/logits opening to satisfy the existing native source predicates.
The companion output still matches the exact FP output root. This reuses the
existing generic codec interpretation; it does not make bare FP logits root a
Manifest Trace subject. Current remote trace publisher/provider is wide-TIR;
the ordinary TFP1 profile still needs the corresponding native manifest
publisher/provider and booked exact selection material route.

## Draw and protected rights after the paired original

The native G14 post-commit fold (palw_model_panel_one_fold_v1 immediately after
write_panel) knows exact accepting block, Blue and DAA and validated later
round-seed anchor, unlike the legacy Panel row which stores only seed and bound
DAA. It may supply this profile's draw if its one-seat roster and eligibility
match the adopted rules and draw deadline. Derive principal from the literal
bond key there and associate each deterministic outpoint rank with its own
prebooked reserve. Prepare draw and retain its membership provenance within the
same native checkpoint, with no old Panel fallback. No ordinary supply role
can confer claimant authority. A later manifest request must reauthenticate
its current case-specific authority and keep its prebooked rank independent
of that request's coordinate. Assign finite outsider and defense rights from
their own native ledgers; do not consume another case's slots.

## Focused regressions

Use the local paid native test path with exact signed FP and companion bytes.
Prove untouched old codec/IDs; unsigned and actual plans agree on the binding
for the same facts; source/output/trace/execution/companion/service/profile
substitutions fail even when re-signed; a stale or changed native accepted
point fails; low capacity and missing protected lanes mutate neither ledger;
successful fold creates both records and exact origin; replay and reorg
restore both together. This fixture is local evidence.
