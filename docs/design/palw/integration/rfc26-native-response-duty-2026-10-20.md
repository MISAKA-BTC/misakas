# RFC-0026 native response-duty binding — 2026-10-20

Status: isolated implementation/verification candidate. No service/default authorization, adoption, arming or deployment.

## Read the actual owed response

`tir_native_response_duty_v1` reads a retained ORIGINAL frame and accepted `TirAcquisition` row from native state. The caller supplies only the acquisition key, literal actor, namespace profile and native accepting context; it cannot supply a substitute source, turn, cursor, response clock, custodian set or leaf ceiling. Source/allocation and preparation/Ready consistency use the same native validation helpers as carriage processing, with the ceiling taken from the frame. No legacy-bundle fallback is used by this ORIGINAL-route adapter; other claim routes still need their own corresponding duty derivation.

Only the row's actual `response_custodians_v1` owe these responses. An owner's allowed voluntary answer is not a literal custody debt. Draw membership alone is not substituted for qualified Ready membership. Missing original frame/preparation or inconsistent accepted context refuses. There is no model-level review or new model restriction.

A pending, uncollected response is observed as:

- **Begin:** no accepted slot yet; the response graph supplies the existing native body upper bound.
- **Part:** the accepted body hash/total/head and current offset determine the next canonical chunk. The remaining response-stage upper bound includes the remaining Parts and Finish.
- **Finish:** native accepted EOF exists but body authentication has not completed.

A finished slot or already collected/no active response returns no outstanding response for that actor/row. This does not resolve a service binding, authorize Final, release evidence/collateral, or discharge another actor. Unfulfilled duties stay visible after expiration: the derived clock class distinguishes before-response, response-window, response-expired and collection-closed. The actual original response deadline is exposed without clipping or extension; comparison with original acceptance +10,000 is derived, not a carrier-controlled override.

## Scope and remaining resources

The duty ID binds logical-request scope, the existing owner-scoped key, literal answerer, actual native demand ID/turn, original response interval/hard bound and current accepted response stage/body cursor. It does not include the current observation DAA: passage of time changes clock status, not the owed identity. A newly accepted Begin/Part changes the stage/cursor identity. Different profile/owner/demand/actor/stage context is not pooled by this ID.

This is an **owner-scoped current native duty ID**, not an owner-independent shared-frontier packet ID or a replacement wire signature. Its response-stage count does not include all work-debit/restart/other-control interleavings. Remaining native moves are read from the accepted prepaid envelope and actual spent counter, not assigned a new quota. Before Begin, response stages are an upper bound over declared-length branches. Comparing that bound with remaining moves alone cannot establish that a particular response is impossible or physically deliverable.

The accepting context is the native caller's `PalwBlockContextV2`, not a payload header timestamp, sampled RPC DAA, sender timestamp or wall clock. This matters because the recovered devnet log records different header and observation DAAs, and supplies no native court-acceptance timeline.

## Connection to the default guard

For a service-bound acquisition's native default effect, the guard derives each purported unpaid actor's actual outstanding response duty. Missing duties or deadlines beyond original hard deny the default. Then the existing `CourtServiceStateV2::default_hold` continues to deny default for unresolved service assumptions, even if quota was supplied. Preparation defaults retain the existing denial path; this new response adapter does not pretend to cover preparation duties.

Unbound claims preserve the previous route. Nothing converts quota or this view into personal fault. The reservation book still lacks adopted packet-to-duty admission and actual reserved supply/inclusion assumptions; the complete graph, owner-independent public ingress and terminal effects remain open. The view is read-only and adds no state row, lifecycle tag, default permission flag, fee or clock parameter.

## Verification scope

The existing native producer-response fixtures now read the duty at every generated response stage and compare its scope/stage to actual dispatched controls. After Finish they check that this actor's response is no longer outstanding. Native body/clock/custody fixture caveats remain; this is not node T05/T08 or full G14.

A dedicated native-state test checks the exact response deadline boundary, continued visibility after response/collection expiration, stable owed identity as time advances, profile separation, refusal of a voluntary owner as custodian debt, and missing frame/preparation/key rejection. Service-bound native default denial remains covered by the existing guard and undo tests. Results and raw commands are recorded in the candidate evidence after execution; synthetic fixture values are not operating profiles or measurements.

## Authority and cost limits

Like other native state adapters, this function assumes its caller supplies validated native state and the real accepting context. Its returned data is not a portable admission certificate and cannot authenticate a caller-fabricated state or clock. The default guard calls it directly on its current native builder state; no externally supplied eligibility flag or duty DTO is accepted.

Source/Ready validation and duty hashing add work to the candidate guard. The present per-actor implementation may repeat validation for multiple unpaid actors. This work is not priced as zero and is not covered by an invented scalar allowance: production cost/admission records must include it (or a verified shared-validation implementation) before activation. All real fences and record lists remain unchanged and unadopted.

Visibility of expired pending metadata does not extend a signed promise or permit a late Begin/Part/Finish. Original native clock checks still reject those actions. Retaining an unresolved record, discharging it, and assigning economic responsibility remain separate native/end-policy decisions; the view alone decides none of them.
