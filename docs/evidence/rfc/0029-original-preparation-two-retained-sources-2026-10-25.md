# RFC-0029 original preparation: two retained Source construction stages

Status: focused runtime PASS (2/2); integrated no-run build PASS. No activation, new authority, wire change, or campaign completion.

## Actual connection and ownership

Actual FP acceptance calls `prepare_paired_original_v1` from the FreePromptCommitted object fold, then `prepare_original_v1`. The native preclaim quote calls the same preparation. Its first direct Source constructor now uses existing `Source::new_native_v1`, mapping typed Resource to `PalwStateV2Error::NativeResource` and Decode to the existing semantic refusal. The owner remains through custody contract construction (its final source-ID use) and drops before the second Source stage.

The Frame constructor has an additive `prepare_native_source_v2` entrypoint. It performs the same allocation derivation check, then uses the existing retained Source constructor. A shared private finish routine preserves exact panel/liability, original-object pledge authentication, descriptor, custody and full frame checks. The Source owner stays alive through the finish call, including actual pledge verifier and source-ID descriptor use, and drops on return/error. Legacy `prepare_native_v1` still calls the original raw Source constructor and returns its existing String errors; it is not a wrapper that converts new Resource errors into String.

The admission quote is the existing `Source::native_construction_requests_v1`: actual program codec/analysis/StepSpace roles, layout/header clones, request windows and actual hash writers. This is logical requested-heap coverage, not allocator RSS measurement. No new authority, policy, wire tag, model rule or activation flag is introduced.

## Prepared runtime controls

`preparation_sources_owned` in Core lib: two outer tests, each with one isolated child (children included in parent counts). Both parents passed on the final immutable Core binary below.

The fixture reuses the existing funded-candidate generator's real ML-DSA key-bearing job context, actual execution/header, signed FP commitment, real signed pledge, registered class/layout, native plan and fresh unclaimed native state. It does not inject a selected approval, credited receipt, unadopted schedule or shared context installer.

1. The actual paired preparation and native quote first establish equal positive results. With real quota armed before builder construction, the test finds the first direct Source stage admission threshold and requires one constructor attempt, no successful first Source, and no second-stage attempt at one byte below. The returned failure must be native Resource, state/journal unchanged, and ample same-input retry must succeed. Earlier builder/plan refusal cannot substitute for the Source-stage observation.
2. Direct additive Frame API separately targets its second Source constructor with the actual quoted `q`: `q-1` headroom must produce typed Resource after this constructor's attempt and before pledge verification. Exact `q` headroom must authenticate with the actual ML-DSA verifier, while an additional byte reservation fails during that verifier callback. After Frame return, all `q` bytes must be reclaimable. A changed trace-root header stays semantic, and the original input retries successfully. This does not claim that whole-caller quota can independently stop the second constructor after the first has released an identical quote.

Test-only stage counters observe control flow and never inject Resource or bypass authentication. The quota and actual verifier callback establish admission/owner lifetime. Full-quota reacquisition checks release after operation.

## Existing reservations and explicit remaining boundaries

TransitionBuilder parent-copy admission and `try_checkpoint` cover their existing state copies. The staged writer admits completed new rows and state/journal copies; these are separate from the Source reservation, and this change does not add a duplicate reservation for them.

The earlier `check_tir_public_allocation_native_v1 -> derive_tir_public_allocation_v1` path also constructs a legacy Source and derivation geometry. That existing construction precedes these two direct constructors and remains a separate boundary. Frame argument cloning, header-digest scratch, derived Preparation/Owned/frame construction, signing/serialization, and allocator/runtime internals are not claimed covered by these retained Source owners. This record is limited to the two directly connected construction stages, not all preclaim resources or a mined paid-admission campaign.

Public activation remains None, normal bootstrap fences and arming refusal remain, selected verifier approval remains necessary, and absent K3 inputs / fifteen campaign UNKNOWN gates remain unchanged.

## Runtime evidence

Integrated Core / Consensus / Node (private/EVM) no-run build passed in 4m26s. Frozen 20-file wave snapshot: `/tmp/misaka-rfc-root-preparation-canonicality-wave-source-20261025`. Immutable Core binary: `/tmp/misaka-rfc-root-preparation-canonicality-core-20261025`, SHA256 `37499332a9c6c9da225f52fa5851323c71264ad99e12e04b45bb42355635b3bb`.

`preparation_sources_owned`: **2/2 PASS**, .81s, log `/tmp/misaka-rfc-root-preparation-canonicality-core-tests-20261025.log`. Direct typed Frame second-Source child .20s; actual paired/quote first-Source child .60s. Children are included in the two parent tests. This verifies actual quota refusal / retry and source-owner release at the explicitly separate stages above; it does not convert the direct second-stage test into whole-caller second-stage refusal evidence.

Same immutable binary Output regression: **2/2 PASS**, .22s (quota child .15s included), log `/tmp/misaka-rfc-root-preparation-canonicality-output-regression-20261025.log`. Same immutable binary Source-three-consumer regression: **4/4 PASS**, 1.85s (children install .17s / grant 1.01s / OwnerOpen .43s included), log `/tmp/misaka-rfc-root-preparation-canonicality-source3-regression-20261025.log`. Existing funded-candidate generator regression: **1/1 PASS**, 2.51s, log `/tmp/misaka-rfc-root-preparation-canonicality-funded-regression-20261025.log`. No earlier derivation or Frame clone/scratch boundary is claimed solved by these results.

A future typed retained constructor in the earlier allocation-derivation path will change where a quota refusal is first observed. Equal Source quotes can prevent independent later-stage refusal under whole-caller quota. Such a follow-up must update stage-specific controls truthfully; the present direct second-stage and whole-caller first-stage evidence applies to this source snapshot and does not authorize synthetic Resource injection.
