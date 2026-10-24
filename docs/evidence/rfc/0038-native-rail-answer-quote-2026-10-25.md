# RFC-0038 native answer-aware quote and rail emission — 2026-10-25

Status: this generation's focused runtime controls passed (PQ 13/13, RPC wire 2/2, rail 3/3, RPC service 1/1, node canonical/seat 6/6). Full-bin metadata and actual RPC integration controls remain pending. Earlier producer/seat controls are recorded in [the previous evidence](0038-producer-seat-answer-binding-2026-10-24.md); their results are not relabeled as results for this source generation. No public preset or arming validator is changed. All 15 RFC-0012 campaign gates remain UNKNOWN; formal K3 materials remain uncreated.

## Actual consumers

The native price reader now has an answer-aware entry. It passes the literal commitment's claim, inherited-prefix identity and actual answer into the same core accounting price used by the fold, only when the native accounting fence is active. A legacy quote with no answer is refused at that fence. The canonical node producer also uses this entry when accounting is active. Missing active quotes defer the claim; they cannot fall back to the legacy leaves price. A changed canonical publication DAA or public preclaim snapshot also defers it.

The reader pairs its native branch state with a virtual state whose selected parent is the same chain point. A changing branch returns no quote rather than mixing generations. The response carries that native chain point, DAA, fence decision, network domain and four scalar mass parameters from native Params. No client flag assigns activation authority.

The existing free-prompt price RPC explicitly writes transport version 2 and reads versions 1 and 2, rejecting unknown versions. New request fields carry a bounded literal FP payload and actual answer IDs. Primitive vector lengths are bounded before wRPC vector capacity allocation; the service also bounds inputs from other transports before decoding a retained original. It checks the repeated class, bond, prompt, executed count and work fields against that literal original before native state access. The typed original's reservation is retained through the blocking native quote call. This does not claim a new native resource owner for generic JSON/protobuf transport allocations.

The RPC response boundary distinguishes typed `NativeResource` and `NativeContextUnavailable` failures from semantic pricing refusals. These faults become RPC quote-unavailable errors, never successful `available=true, priced=false` responses; canonical production likewise defers them rather than declaring a chain refusal. `ZeroQuanta` and other semantic refusals retain their existing meaning.

The actual rail now asks for this answer-aware quote. A named node that lacks the new quote, returns no native state, or reports a different network or invalid chain point causes refusal before signing; it does not silently switch to a leaves estimate. The offline legacy route remains explicit and emits no inferred accounting carriage.

For an externally delegated public claim, the rail checks that the existing native preclaim family quote and answer-aware price quote use the same snapshot. Both readers resolve the native virtual DAA for the same selected parent. The preclaim reader keeps its real accepted state point unchanged and passes an independent prospective quote clock to its bounded policy/geometry calculations. The rail requires both snapshot and explicit quote DAA equality rather than reading a later independent DAG tip. Legacy preclaim responses have no quote clock and cannot satisfy that handshake. The existing local DEVNET public-builder fence could not authorize a real native quoted family; the rail now uses an explicit quoted-public builder which retains the same pledge identity, signature, window and wire checks. It synthesizes no armed Params. Native acceptance remains authoritative.

When the native quote requires accounting, the rail uses its own public execution header context, the sign-gated worker commitment/result and actual output IDs. The shared explicit accounting builder checks context/job pin/count/output-root identity, preserves the exact inner HFP1 bytes and actual inputs/outputs/fee, then signs the actual outer FPA1 transaction. Its wire/signing leases remain alive through file creation, relay or native submission. Legacy served FPA1 remains distinct.

The watch precheck also passes the actual worker answer. Its native-unavailable error is distinct from a successful quote's semantic refusal. Native capacity, transport, network, clock and binding failures retain queue files and attempt counters for retry; only an explicit native `priced=false` result follows the existing price-refusal retirement path. Wrong-network configuration remains visible and cannot silently submit. The one-shot error classifier likewise retains attempts for quote/binding failures.

Dynamic fee estimation now measures the actual signed probe transaction with the native quote's mass schedule. It no longer substitutes a synthetic transaction having only the same payload length. Any explicit insufficient fee remains a native rejection; no output is silently reduced by the wrapper.

## Focused verification results

| Scope | Actual result | Log |
| --- | --- | --- |
| PQ signing/builders, including delegated public accounting | 13/13 PASS, 0.60 s | `/tmp/misaka-rfc-answer-quote-rpc-pq-tests-20261025.log` |
| RPC v1/v2 wire, bounds and explicit preclaim clock | 2/2 PASS, 0.00 s | `/tmp/misaka-rfc-answer-quote-rpc-pq-tests-20261025.log` |
| Rail native quote, same snapshot/clock and real watch file/state decision | 3/3 PASS, 0.00 s | `/tmp/misaka-rfc-answer-quote-rail-tests-retry1-20261025.log` |
| RPC resource/context versus semantic price response | 1/1 PASS, 0.01 s | `/tmp/misaka-rfc-rpc-quote-fault-tests-20261025.log` |
| Node canonical price plus actual seat carrier controls | 6/6 PASS, 0.78 s | `/tmp/misaka-rfc-node-answer-quote-tests-final-20261025.log` |

The node binary is `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`, SHA-256 `d99ff3055ce45e90244775fd1a1db26415043882807c19a1252ae6bbbb1a7377`. The RPC service binary is `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_rpc_service-5b680e0baca40d8e`, SHA-256 `5eea0be3786d4335d458c4a4d23afb84ba5b1b8b0e7c3f494ecc8bb527edcdc4`. The combined PQ/RPC log identifies `kaspa_pq_validator_core-71221f43ae005f6c` and `kaspa_rpc_core-b29ea48fd5a4d4b6` under the same build directory.

These controls verify actual boundary behavior: resource/context faults never become successful `priced=false` RPC responses; ZeroQuanta stays semantic; good quotes preserve quanta/pwu/reservations. Rail watch native failures retain queue files, persisted attempts and unsettled state, while a semantic price refusal actually renames and settles the job. The live quote guards reject legacy/unavailable/wrong-network/malformed points and same-point/different-or-missing-clock combinations.

An initial rail compile found a stale four-argument watch call and an incompatible Result/Option `?`. The actual answer is now passed and native errors are propagated without conversion to an absent quote. Its consumer was corrected to retain jobs for native failures. The failed compile log remains `/tmp/misaka-rfc-answer-quote-rail-tests-20261025.log`; the successful corrected rerun is separately recorded above. An earlier canonical probe used an old binary and ran zero matching tests; that probe is not passing canonical evidence and is excluded from all result counts.

Actual RPC integration input controls are **NOT RUN**. Final metadata checks passed:
`cargo check -p kaspad --lib --tests --bin misaka-rfc12-private-node --features private-integration-config`
and `cargo check -p misaka-palw-gateway --bins`. Logs are
`/tmp/misaka-rfc-node-coinbase-quote-check-final-20261025.log` and
`/tmp/misaka-rfc-gateway-answer-quote-check-final-20261025.log`.
Compilation does not establish actual RPC integration or campaign acceptance.

No new external transfer, deployment or campaign was executed.

## Remaining scope

An active plain worker result carries no execution context. This rail refuses it rather than guessing runtime, shape or CU facts. Its inherited-prefix public producer route also refuses until the authoritative native policy handshake is supplied. Native context/backend resolution for those paths remains unfinished.

The rail emits its existing delegated HFP1 family. The shared explicit answer wrapper preserves existing plain/TFP1/HFP1/TFO1 bytes, but this change does not add a live TFP1 or multi-input funded-original producer to the rail. SDK/worker and other gateway emission paths remain unconnected unless explicitly described above.

The exact winning-carrier index required by the seat remains absent; multiple same-claim carriers fail closed. All-model calibrated resource and economics measurements, restart/campaign evidence and selected-verifier authentication remain separate requirements. Root identity and price calculation do not establish model computation correctness or replace selected-verifier approval. Model tiers and the prohibition on cryptographic model-computation proofs remain unchanged.
