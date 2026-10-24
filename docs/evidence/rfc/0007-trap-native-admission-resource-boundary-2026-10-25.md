# RFC-0007 paid trap: native admission capacity boundary (2026-10-25)

## Scope

The checked paid-base lookup added in `d6524d376` distinguishes genuine absence, malformed present bindings and native resource refusal. Its String admission wrapper previously erased the resource distinction before the actual acceptance walk: the walk could drop `TrapRevealedV1` and continue the block instead of returning its local retry fault. The producer rehearsal likewise reported `NotAccepted` before reaching the typed fold.

The native classified dispatcher now sends `TrapRevealedV1` directly through one dedicated shape → checked paid lookup → paid/ordinary mesh state rule → registered-key signature validator. It does not preflight and then repeat the String lookup. The original non-trap dispatch remains the existing String validation mapped to a semantic refusal. Native resource refusal returns through `PalwLocalTransitionFaultV1::Resource` in the actual walk, and `Refused(NativeResource)` in the actual rehearsal. Semantic faults retain the existing object-drop/`NotAccepted` behavior.

The String wrapper remains for compatibility and the local H1 gate. It does not drive this checked-trap accepted-state path. The other accepted-state validator call in `palw.rs`'s derived panel binding loop only constructs `PanelBound`; it cannot carry a trap and is unchanged. The false-valid filer only handles `ObjectiveOffence`; the writer comparison oracle is test-only. No wire schema, fence, preset or policy changes. Public activation remains absent and arming refusals remain intact. This is neither selected-verifier approval nor proof of model computation.

## Executed native admission control

`paid_trap_native_admission_capacity_is_retry_not_dropped_semantic_object` uses three isolated subprocess modes because the logical budget is process-global:

- Zero budget: actual classified native validator refuses; actual producer rehearsal returns typed `NativeResource`, rather than a semantic rejection. A malformed tile range still receives a semantic refusal before the lookup; a dormant paid-trap rule does not allocate its lookup key.
- Tight walk budget: the public typed heap envelope admits the actual retained state clone, leaving 64 bytes. A paid-base lookup requires its existing 65-byte key reservation. The control independently retains that copy and verifies the subsequent checked read refuses; after releasing it, the production acceptance walk must return its typed local resource fault. The previous String-drop implementation would instead finish with zero accepted objects. This private non-audit fixture disables pre-object sweeps to isolate the admission boundary; it is not evidence about a full audit campaign.
- Ample budget: a genuine absent paid-base index selects the ordinary mesh rule; a reveal opening no commitment remains a semantic refusal, producer `NotAccepted`, and an empty successful acceptance. No valid paid enrollment or model execution is fabricated. Existing signed mesh controls supply the normal mesh regression scope.

The empty route fixture is constructed through the public `PalwStateCarriageV2::from_state` and checked `into_state_v3` restore APIs before arming a budget; no private field setter or new core test authority is added. The local synthetic restore uses no peer root, and authenticates no chain state.

Both refusal modes preserve the input state root. The unsigned signature-shaped reveal is intentionally rejected by the state rule before signature validation; it is not evidence of an authentic successful reveal.

## Validation

Rust parser checks and `git diff --check`: PASS. Parent-coordinated Cargo completed successfully: **1/1 control PASS**, **0.94 s**, build **6 min 50 s**. The three isolated zero/tight/ample modes passed in 0.31/0.20/0.23 s; these children are included in the one parent control, not three additional tests. In particular, the tight-budget actual walk admitted its earlier state copy and returned typed capacity refusal at the paid-base read rather than silently dropping the object. Source is saved in `bdc05fb6a`, including the public-carriage fixture correction.

Log: `/tmp/misaka-rfc-trap-native-admission-tests-20261025.log`. Binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus-53c349c8ce803ba3`. SHA256: `65c92ec59cdb400ba37afc571c5bef29d53158b03ddf6f9a43a0e695baceecae`. No Cargo was launched by this agent.

Focused filters in `kaspa-consensus --lib`:

- `paid_trap_native_admission_capacity_is_retry_not_dropped_semantic_object`: **1/1 PASS** as recorded above.
- `t12_a_trap_is_signed_by_its_setter_drawn_by_the_lottery_and_dropped_below_its_fence`: **1/1 PASS, 0.59 s**, same compiled binary; log `/tmp/misaka-rfc-trap-native-mesh-regression-20261025.log`.
- `t54g_the_object_rehearsal_is_the_folds_own_answer`: **1/1 PASS, 3.46 s**, same compiled binary; log `/tmp/misaka-rfc-trap-native-rehearsal-regression-20261025.log`.

This closes a local capacity-classification defect only. K3 formal inputs remain uncreated; the RFC-0012 campaign gates remain UNKNOWN. No live network activation, execution campaign, pruning/restart evidence, or complete paid-trap rollout is claimed.
