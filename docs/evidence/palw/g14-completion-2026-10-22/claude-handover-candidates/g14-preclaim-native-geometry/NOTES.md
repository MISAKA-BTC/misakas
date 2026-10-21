# Actual preclaim native origin + opt-in geometry adapter (uncompiled)

`preclaim-native-geometry.patch` applies against the current repository; `tree/` stages the candidate. No repo edits/build. Actual old prepare_original_v1, native quote functions and install_original_v1 implementation stay unchanged. The old transaction-paired route adds no mandatory Source reconstruction, quota, graph enumeration or new feature/price coverage requirement.

## Actual production handoff before reservation

There was no opaque PreparedOriginal type: native prepare returned an ordinary decoded Frame type. `prepare_paired_original_v1` now returns a private-field, non-decodable/non-Clone `PreparedOriginalFrameV1`, built ONLY at its existing successful native preparation return. It captures the actual acceptance context, actual allocation rules (court, native class ladder, existing measured clock/config), and registration-DAA demand from real native params/class state. Missing general registration remains optional in the old holder rather than adding a new rejection to the old path; opt-in enrichment refuses it explicitly.

`objects.rs` receives this holder at the SAME point before `reserve_for_claim`, reads the existing Frame for retention, and consumes it at the SAME final install call. Only those two object lines change. Paired original checkpoint/restore, ordinary FP checks, reservation, budget writes and old native install guards remain intact. Old quote and prepare functions do not call the new optional geometry path, and this patch changes no current TFO filtering/refusal.

No external decoded Frame can call a public constructor to obtain this type. Existing public Frame preparation/quote methods returning candidate Frames are not factories for it. The wrapper's inline copies are not an assertion of new Frame heap admission: EXISTING original/Frame construction remains its caller-admitted legacy preparation contract. It is deliberately not labelled NativeRetained after allocation.

## Opt-in genuine preclaim geometry

The native holder's opt-in enrich method rechecks the same accepting point/network, actual court/class ladder and registration-time demand, and requires the claim/Frame still uninstalled. It then constructs Source with native typed resource reservation BEFORE Source allocation and matches the actual frame/producer pledge SourceID. This owns that separate Source ticket, preserving Resource versus malformed-state errors. It needs no rooted Claim/TirClaim/shared claim context.

From that actual native-origin owner, numerical geometry uses captured actual court/demand and actual current registered program/layout; no caller-provided court/body ID is authority. Traversal budgets remain explicitly local exploration limits, never adopted profiles. Source graph namespace is resolved from actual native service state, never a caller profile hash; missing service namespace has an explicit typed result. A namespace is still not the missing authoritative fee/capital/retention body. Both APIs preserve their existing bounded native resource behavior.

The optional enriched holder can be consumed through the same old ordinary install seam, keeping its Source ticket live until the install attempt returns. It does not skip actual ordinary reservation/Claim guards and does not establish funding/escrow/paid/all-family standing. Existing postclaim family owners and runtime guards remain unchanged.

## Actual fixture and limits

The existing real-key preclaim fixture generator now additionally calls transaction-paired native preparation on the actual fresh registered state with its literal actual signed envelope in native extras. Before any reservation/Claim install it obtains the opaque holder, checks legacy plan equality, enriches actual native Source, compares numerical root count with existing analytical inventory, checks native registered demand, wrong acceptance point refusal, typed local numerical-budget refusal, and explicit missing service namespace. State/entry/claim absence stays unchanged. This tiny actual program may have no numerical sites; this test does not relabel it as a nonempty numerical-family test.

Existing ordinary paired acceptance/failure/undo suites remain the main regression for old install behavior. Suggested filters: `g14_native_funded_candidate_fixture_generator_v1`, `g14_tir_frame_without_tfp1`, existing original paired acceptance and t12 identity/activation. All remain unexecuted in this candidate.

Remaining: source graph positive with authoritative native service namespace in genuine unclaimed state; native preclaim all-family aggregate adaptation (current semantic owners require postclaim state/material); accepted semantic supplier obligations and fee/capital/retention bodies; measured paid frame calibration; actual funded ordered coordinator/UTXO install/undo. No Complete flag, invented prices, new wire, activation or native funded-admitted result is introduced.
