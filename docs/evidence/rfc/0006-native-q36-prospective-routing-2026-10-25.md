# RFC-0006 prospective Q36 FP/rider routing — 2026-10-25

Status: final core compilation passed; the actual FP factory refresh control passed 1/1 and the reader-router filter passed 6/6 on that final binary. Node metadata and the existing actual Q36 reader/layer execution control passed. A live native-store ancestry/reorg refresh and rider expiry controls remain pending. Pending rider handoff remains open. No public activation preset, arming validator or normal bootstrap fence changed; the 15 RFC-0012 campaign gates remain UNKNOWN and formal K3 materials remain uncreated.

## Actual execution and signing consumers

The existing block-attempt and accepted-claim routing owners were insufficient for prospective free-prompt work and riders: the FP caller executed without a native prospective owner, and a rider's lead/index-derived anchor could not use a block candidate's pre-PoW anchor. A new non-wire native work owner covers those two origins. It is neither an accepted claim nor selected-verifier approval.

The native factories read their own Params, accepted checkpoint header, matching native state, virtual DAA and stable import epoch. FP additionally checks its own accepted anchor and ancestor relation. Accepted header hash/DAA identity is kept unchanged. A second snapshot/import check refuses a moving read. Resource failures remain typed and distinct from identity refusal and unavailable native context.

The owner binds the real network, class, active bond, registered artifact/profile, native router row and all job fields. FP reconstructs the ordinary context from its complete descriptor and the native class semantics; inherited-prefix work is refused. Only the actual early-stop count may decrease within the declared positive decode budget. Rider work rederives the canonical lead/index anchor and checks the native lead, rider window, index, executor, class, retention, phase and absence of a prior rider. These checks do not synthesize an activation or authorize a missing selected-verifier approval.

The actual Qwen36 backend binds this owner to its existing per-layer reader-router plan before weights run. A different work owner, simultaneous accepted/block owner, artifact/profile mismatch, or later context substitution is refused. The owner and plan lease stay live through execution. This path uses the existing integer CPU Qwen36 implementation; no separate `Q36CPU` production symbol or private-source Qwen36 caller was found. The private kernel-work source is a separate runtime and is not labeled connected by this change.

The canonical node FP caller acquires the owner before offloaded execution, verifies the returned context and refreshes the native binding after execution and before signing. It carries the owner to the collector, which refreshes again before native submission. The rider caller acquires the native lead/index owner before execution and refreshes before its ML-DSA signature. The existing lead-attempt owner remains separate. Actual outputs, ordinary transaction inputs/outputs/fees and signature contexts are not replaced by a routing witness.

Reservations precede retained state/row/job clones and router-plan retention. Local resource failures defer work; they do not become invalid computation evidence or zero-cost success.

## Focused verification

The parent compiled the final core source successfully in 3m14s and ran:

| Scope | Actual result | Log |
| --- | --- | --- |
| Actual FP native factory refresh over synthetic native rows | 1/1 PASS, 0.02 s | `/tmp/misaka-rfc-native-work-refresh-core-tests-20261025.log` |
| Reader-router filter, including fixed FP identity/stop count and existing Q36/Kimi graph controls | 6/6 PASS, 0.21 s | `/tmp/misaka-rfc-routing-work-core-tests-final-20261025.log` |

The factory control permits normal tip progress with the same fixed work and rejects a changed import, backwards clock, substituted descriptor and currently frozen class. It calls the real prospective constructor over synthetic native state rows. It does not establish the native processor's ancestry read, live chain acceptance or actual model-weight execution. The earlier reader-router 6/6 run (0.23 s, `/tmp/misaka-rfc-routing-work-core-tests-20261025.log`) preceded the final refresh change and is not counted again.

The initial integrated node/consensus compile failed at two pruning-import error mappings (`/tmp/misaka-rfc-native-owners-node-check-retry1-20261025.log`). The general import-status API formats its store failures into `PruningImportError`; the routing path now reads the same import-session/stable-marker store under one guard and maps its original typed store error directly, preserving local-capacity refusal. This correction is not yet compiled.

Integrated base0/node compilation, the processor's canonical-ancestry/import decision control, rider expiry and full-node native refresh runtime controls are **PENDING**. No new Cargo job was started by this agent. No external network transfer, deployment, live arming or campaign was performed.

## Remaining handoff and scope

The rider owner currently lasts through execution, signing, local material retention and broadcast inside `produce_one`. The existing `riders_out`/`palw_rider_outbox` passes only a wire envelope, and the panel transfers that envelope into its court pending queue. The opaque native owner is then dropped. The court carrier signs and submits later without refreshing that owner. Closing execution/signing does not close this pending-carrier lifetime.

The intended next connection is a non-wire guarded batch through the in-process rider outbox and a court-outbox sidecar retained through drain, funding, signing, async submission and retry. It must reserve its bookkeeping before allocation, check the exact batch's rider identities and refresh before publication. No owner or token will be persisted as wire authority. Restart recovery must obtain and bind its own native context; the existing in-process rider outbox is lost on restart.

The initial refresh compared exact checkpoint/root/virtual DAA, which would reject long-running FP/rider work after ordinary tip progress. The final source corrects that liveness problem with a dedicated opaque-owner native refresh method. In one coherent current factory bracket it checks fallible native canonical-chain ancestry of the original pinned checkpoint, its validated header status, unchanged import epoch and current eligibility. Stable identity preserves the full original descriptor/job/profile/declaration/executor/network while allowing a forward current snapshot and clock. The original backend execution/plan owner remains alive. The core constructor/identity control above passed; the native processor code compiled and its canonical/import decision helper passed. A live native-store ancestry/reorg refresh has not been exercised. Reorg, import or plan changes continue to refuse.

Generic DA recovery/fraud/partial-resume routing, inherited-prefix descriptors, no-capture exact-count handling and missing genesis registrant cases remain separate work. All-model cost calibration, a real multi-process campaign and selected-verifier authentication are unproven here. Hashes/roots establish identity; they do not prove model computation correctness. The frozen model tiers remain unchanged.

The processor's canonical/import decision control passed 1/1 (0.00 s) in `/tmp/misaka-rfc-routing-work-native-bracket-tests-20261025.log`. It exercises the actual decision helper; it does not execute a native store/reorg refresh campaign. Node library/tests/private-node metadata passed in `/tmp/misaka-rfc-native-owners-node-check-retry5-20261025.log` (33.69 s). The existing actual Q36 reader/layer retarget execution control passed 1/1 (0.02 s), `/tmp/misaka-rfc-native-routing-q36-runtime-tests-20261025.log`. Build7m11s includes locks/dependencies and is not a model-cost measurement. This existing execution control and the synthetic-state native FP factory control are separate checks; they do not establish one combined live native factory-to-execution campaign. The changed GDN node consumer controls are recorded in the separate GDN evidence.

Implementation source commit: `596d166dc` (native body/GDN/prospective routing consumers). This identifies the implemented source scope, not completion of RFC-0006 or a campaign gate.
