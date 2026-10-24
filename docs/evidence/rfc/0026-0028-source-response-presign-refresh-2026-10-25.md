# RFC-0026/0028 paid source-response presign refresh — 2026-10-25

Status: focused responder compilation and runtime controls PASS (5/5, 2.18 s), source committed in `ebe5a0ab4`. No live native-store, response-delivery, inclusion, default or campaign acceptance is claimed. Public presets, wire formats, fees and clocks are unchanged; all 15 RFC-0012 campaign gates remain UNKNOWN and formal K3 inputs remain uncreated.

## Actual consumer change

The Flat, Float32 and TIR-prefix responders previously read an accepted duty, awaited source polling or disclosure, then signed using the old duty/cursor/clock. Their queued-packet pruning preceded that asynchronous work. A response that expired or was superseded during the worker could reach the signer before the next tick pruned it.

Each actual responder now performs a fresh own native read after its last await and directly gates the actual signer. The shared validator checks the literal claim/owner/reservation/bundle or seed, actor, source identity, phase, whole accepted binding/job context, current cursor or TIR ordinal, original deadline and current exclusive deadline. TIR also compares the whole accepted header and custody/terms. Normal forward snapshot/clock progress with the same duty is permitted; a changed clock is not substituted into a historical header.

Unavailable native reads remain errors for the existing local retry path. Missing or superseded duties discard only this local prepared packet without signing/enqueueing, inventing a response, a conviction or selected-verifier approval. A matching local read is not model-computation correctness evidence.

Flat/Float retain the existing admitted snapshot workspace inside the blocking worker. On success the worker returns `(answer, guard)`, transferring that same guard to the fresh read/sign/enqueue scope. Cancellation of the async waiter cannot release this workspace while the worker still runs. The snapshot-worker transfer pattern is source-inspected; the separate shared persistent-reader lifetime has a passing actual-owner thread control described below. Expected source contexts use existing shared Arcs, and the borrowed validators allocate no clones or substitute native tokens.

Flat/Float persistent readers now share one `Arc<OwnedSourceReaderV1>` containing the reader, its complete existing host reservation and a typed pre-admitted metadata ticket. Both the book and blocking worker clone that whole owner. Reader field order releases the reader before either ticket, including constructor refusal. Float factory work now also holds its host guard inside the blocking factory and returns `(reader, guard)`; cancellation cannot drop a parent-only guard while that factory still runs.

## Executed focused controls

The `response_presign` filter contains five controls:

- Actual BASE-0 execution/capture binding and public Flat-session construction exercise a real accepted cursor transition, forward clock, exclusive deadline equality and source/owner/bundle/context changes. The actual signer-gating callback is counted and remains uncalled for rejected preparations, native absence and read errors.
- The Flat movement's claim/owner/reservation/bundle/literal actor cannot be replaced; voluntary owner availability is not paid custodian duty.
- A public Float-session constructor over a recommitted identity fixture tests the same forward-clock/deadline/binding rules. This is a Float data-identity fixture, not Float arithmetic, native class approval or an integer-to-Float migration.

The fourth control constructs a finite TIR program/layout with the public builders, creates the actual prefix stream and paid duty through public constructors, and advances its ordinal through an authenticated boundary-leaf opening. It checks signer counts for ordinal advancement, exact expiry, changed seed/header/context and literal actor. The committed boundary bytes are a data-identity/opening fixture; they do not assert honest model execution, native admission or a passing arithmetic verdict. This control passed.

The fifth control uses the actual production shared-reader owner and a blocking thread. Dropping the book owner while the worker holds its clone must retain an exact 128-byte host reservation and refuse a replacement allocation. Releasing the worker drops the reader while its reservation is still live, then returns the budget to zero. It does not replace the production worker with a copied owner implementation. This control passed.

Parent-coordinated Cargo completed the focused `response_presign` filter: **5 passed, 0 failed**, 927 filtered out, runtime **2.18 s**; build **4 min 38 s**. Log: `/tmp/misaka-rfc-source-response-presign-tests-20261025.log`. Binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`. SHA256: `58151cf03f4f60fb30ce2b5c1b019f34591d1e8cb88757f80ef716eb008becb9`.

No Cargo job was launched by this worker. Syntax parsing and `git diff --check` also passed before source freeze. The result covers the five scoped controls above; it is not a live native-store or delivery campaign.

## Remaining limits

Native state can change after a packet is signed and queued. The later source-response carrier change is recorded separately in [its evidence record](0026-0028-source-response-carrier-refresh-2026-10-25.md); this earlier presign test result alone does not establish that boundary. Existing carrier serialization, packet storage, unrelated reader/cache and transport resource boundaries remain separate. The new shared owner closes the specified persistent-reader/book/poll lifetime only; it is not a whole-node/RSS budget. Response delivery/inclusion, full response graph/default permission, adopted service assumptions and physical cost calibration are unproven here. No activation or model-tier change is introduced.
