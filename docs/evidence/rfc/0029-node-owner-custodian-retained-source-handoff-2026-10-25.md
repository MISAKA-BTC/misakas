# RFC-0029 node owner / custodian retained Source handoff

Status: integrated no-run build and focused/regression runtime PASS. This is an actual source consumer connection and reservation-lifetime change, not an activation, model-execution or campaign completion record.

## Actual connection and typed refusal

Core adds `palw_tir_public_source_for_row_owned_v2`, projecting the existing native row helper into public `NativeDecodeErrorV1<String>`. Resource stays Resource; semantic identity/refusal stays Decode. Existing public raw/String APIs remain unchanged. The caller still supplies its actual accepted-state row and native ceiling; this API creates no portable authority or installer.

The actual outside-owner and demand-responder read callbacks now use this API. Their session API is fixed to `FnMut(&State) -> Result<(), String>`, so a shared `defer_source_read_v2` captures Source errors into an outer typed Option and stops planning with `Ok(())`. Outside the read, typed Resource explicitly defers the tick; it is not string-classified or converted to a semantic result. Existing semantic pending/refusal behavior is preserved. This Source refusal does not set the failing owner case's `done` or responder case's `failed`, submit a move, or fabricate a conviction. Earlier successful case work in the same planning pass is not presented as transactionally rolled back.

Owner `Cone` and responder `Body` steps now store the whole `NativeRetainedV1<Source>` directly. The owner's bare Source clone/Box and responder's bare Source Box are removed. Both actual blocking workers call the same `run_owned_source_v2`: it moves the entire owner into `spawn_blocking` before awaiting. Cancellation of the async waiter leaves the ticket with queued/running blocking work through its final borrow. Source fields drop with their reservation owner on worker completion/error.

## Executed verification

Core filter `public_row_owned_source_api`: one outer test with one isolated child. It reuses the existing real-key source-three fixture and a row installed by actual signed owner Open after actual producer Capture/Ready. Native registered class/layout/source checks are exercised at q-1 Resource, exact q retention/release, changed ceiling/descriptor semantic refusal and same-input retry. The original accepted claim/panel are the existing explicit fixture assumptions; this is not a newly mined paid claim or selected-approval campaign.

Node filter `tir_source_worker_owned`: three outer tests, each one isolated child included in its parent's count:

- Same production blocking handoff with a real typed NativeRetained byte payload. A worker is held at a channel barrier, the actual async waiter is aborted, and a real extra-byte reservation still refuses until that worker returns. Exact capacity is then reclaimed; ordinary successful work also holds the reservation through its callback.
- The actual owner case fixture and actual shared read decision receive a real native clone-capacity refusal; no work starts, `done`, demand count and pending move remain unchanged. Semantic refusal remains typed and pending; ample retry returns an owner. The existing case's unused replay methods provide no model result or accepted claim.
- The actual responder pending-case type and the same production decision receive the real capacity refusal; no work starts, `failed`, body and pending move are unchanged. Semantic refusal and ample retry are separate controls. The local case metadata does not assert an accepted chain claim.

These generic handoff/decision tests deliberately use typed bytes rather than fabricating a valid Source. Their exact clone admission requirement is inferred through the actual public `NativeRetained::try_from_clone_v1` under the named local ceiling, not through an exported private quote or invented tariff. Core's separate accepted-row test supplies Source authentication coverage. These results are not a live node/store/model campaign.

## Remaining scope

Registered-record/layout/header clones, accepted material harvesting, response-byte/step-container allocation, later body-builder String error taxonomy, physical allocator/runtime internals and other raw snapshot/shared-material callers remain separate resource boundaries. The reopened-reader/body host reservations and retained evidence/replay owners keep their existing handling. No missing shared-context installer, native approval, fee authority, selected verifier, model tier or public preset is added. Public activation None, normal bootstrap/arming refusal, absent K3 and campaign UNKNOWN gates remain unchanged.

## Final runtime evidence

Integrated Core / Consensus / Node no-run build passed in 4m37s. All 23 source identities matched `/tmp/misaka-rfc-node-source-view-api-wave-source-20261025/record.json`.

Immutable binaries:

- Core `/tmp/misaka-rfc-node-source-view-api-core-20261025`, SHA256 `516d13e184e7c9f2db5d3db2748a2ce739f79bffd785a1c39a7045f92a485654`.
- Node `/tmp/misaka-rfc-node-source-view-api-node-20261025`, SHA256 `4d566619f1b8679328228fa11c616bfe2460f43f160c27a4bcc0495a5134d2b7`.

Focused controls:

- Core accepted-row `public_row_owned_source_api`: **1/1 PASS**, .24s (child .22s included), `/tmp/misaka-rfc-node-source-view-api-public_row_owned_source_api-tests-20261025.log`.
- Node `tir_source_worker_owned`: **3/3 PASS**, .16s (children .00s/.02s/.00s included), `/tmp/misaka-rfc-node-source-view-api-tir_source_worker_owned-tests-20261025.log`.

Related regressions on the same immutable artifacts:

- Core continuation **3/3 PASS**, 1.40s, `/tmp/misaka-rfc-node-source-view-api-continuation_source_owned-tests-20261025.log`.
- Node owner replay **2/2 PASS**, .00s, `/tmp/misaka-rfc-node-source-view-api-tir_owner_replay_owned-regression-20261025.log`.
- Node Cone consumer **4/4 PASS**, .71s, `/tmp/misaka-rfc-node-source-view-api-tir_cone_owned_consumer-regression-20261025.log`.
- Node custodian ownership **3/3 PASS**, .00s, `/tmp/misaka-rfc-node-source-view-api-tir_custodian_owned-regression-20261025.log`.

Children are included in parent counts. Core authenticates the actual native accepted-row fixture; Node tests exercise the same production generic typed-byte handoff and local pending-case decisions without inventing an accepted Source or model result. The successful tests do not extend coverage to the explicit remaining boundaries above.
