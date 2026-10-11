# G14 semantic court follow-up

Full G14 scope and ADR-0177 D7 remain unchanged. This checkpoint does not prove full family coverage or permit shipping activation.

## Public FP out-of-vocabulary IDs

The pre-existing decode-rules refusal covered every `ForgedOutputTiled` arm, including `OutOfVocab`. It also refused flat `ForgedOutput` for all FP claims. An ID outside the registered vocabulary violates sampling and constrained decoding equally, so the public-filer block-context rule now admits this domain proof. The height-free helpers and blocks below the existing public-filer fence retain their prior refusal. Tiled `NotSelected` and flat in-vocabulary greedy selection proofs remain inadmissible for FP under decode rules. No wire discriminant or activation fence is changed.

The node filer constructs flat tag 8 or tiled tag 11 directly from the authenticated initial public event's token pin. It needs no tokenizer-table rendering for an invalid ID and no selected later tile. A producer cannot evade the proof by making the invalid ID impossible to render.

Source checks pass. Core and node execution are pending coordinated build capacity; tests must not be reported as passing before their terminal result is recorded. Added core cases require producer and all five full Valid signers' adjudication, legacy refusal, valid-ID refusal, and tampered-ID refusal. Existing public builder tests now include position-2 invalid IDs for flat and tiled schemes.

Remaining semantic work includes registered model/tokenizer/table association, committed output after a legitimate stop, initial/inherited prefix state, and all active family relations through the actual node path. Registered tiled actual service is being verified independently by the parent agent. These unit and builder tests alone do not establish conviction, collection, or network recovery for those remaining requirements.

## Constrained output after stop

A second concrete gap is closed in source: the court previously cleared a legitimate terminal EOG even if its authenticated token pin included subsequent IDs. Public-filer-aware flat and tiled checker variants now convict at the first EOG whenever a suffix follows. Legacy height-free checkers keep the old reading. The admission path supplies the existing block session rule; constrained job/claim, prefix renderings, EOG list and token pin are authenticated before the suffix is judged. The proof uses existing tags 14/15 and requires no admitted-lane witness when the stop itself was legitimate.

The node flat builder no longer returns early for that stop, and tiled planning nominates the first EOG with a suffix rather than abandoning the claim. Added core cases distinguish honest terminal stop, legitimate stop plus suffix, malformed pin/EOG opening, and a challenge after the dead prefix. The tiled builder covers a terminal empty-string constraint, first-position EOG, exactly one selected event, no replay nominee and no witness. Execution of these tests remains pending the integrated build.

## Independent registered semantics checkpoint

`palw_class_semantics_v1` has its own ConsensusV2 fence, separately fingerprinted and dormant on all shipped presets. At native signed class registration, the fold retains the exact canonical model/runtime/tokenizer/CU identity statement and its `bound_from_daa`. This retention is independent of public-job body retention, so a registration before the public-filer fence can survive pruning when the semantic fence is armed earlier.

A new claim snapshots the registered semantic statement once. Later prospective table metadata cannot rewrite an earlier claim's court rule. Snapshots survive claim retirement while a liability or vesting row remains and retire with the final owning record. Borsh carriage tail F3 carries both maps; deltas 204/206 journal them. Existing tag9 IdentityMismatch gains J8 for an authenticated claim context violating its stamped identity statement; every Valid seat attests this job fault. Unstamped historical claims retain their previous court semantics.

This checkpoint does **not** yet implement a signed prospective descriptor for old registrations, registered canonical token-table derivation, or false initial/inherited prefix-state conviction. Native service evidence and new fold/core tests are pending execution; parser and diff checks only have passed.

The subsequent source checkpoint adds `ClassSemanticsDeclaredV1` (tag164) under its own `palw_class_semantics_v1` owning fence. The original class registrant signs an explicit semantic/table descriptor. Native identities and a previously filled table root cannot change; an unlisted legacy registration gains only a prospective assignment at the accepting block. Existing claim snapshots do not change. Sentinel registrations without a signing registrant remain an explicit migration gap. The table declaration is a normative registered binding; authenticated derivation/correspondence and objective table-mismatch prosecution remain pending. The tag164 transitive wire pin was computed with the repository's exact std-only manifest reader; cargo/service execution remains pending.

## Public snapshot and registered table mismatch

Existing RPC204 now publishes the exact immutable claim-semantic snapshot as Borsh hex and its commitment, when present; unqualified historical observations omit these fields. The ordinary local outsider already acquires its target with `palw_fraud_filer_target_v1`, which resolves the same rooted snapshot, and its direct-filing path checks J8 before computation replay. Historical duty-only target helpers remain unqualified.

Appended contradiction16 `RegisteredTableMismatch` carries only the claim's authenticated binding and FP job. It authenticates the job's pin/context and compares the selected constraint-table root to the claim-stamped registered canonical root. No table bytes, logits selection witness or producer state are needed. An unqualified snapshot refuses the proof; the public-filer block rule also fences interpretation. The claim-proving fault attributes every Valid seat and forfeits the claim's rights. V1 readers retain their refusal; opaque evidence interpretation is classified and source-pinned under the separate semantic fence.

Node source regression coverage includes a mismatch, honest registered root, unqualified historical target, forged job substitution, direct filing and AnyValid attribution. Execution and native signed-descriptor service prosecution remain pending. This fixes the relationship to an explicit registered canonical table assignment; proving that the assignment itself was derived from the registered tokenizer bytes, and large rendering/state streaming, remain separate obligations.

## Prefix consistency court checkpoint (pending execution)

The new attribution-only contradiction 17 authenticates the exact prefix-state job and every dense integer K/V cache-write tile, in deterministic layer/K/V/position/tile order. It reproduces the documented A16 cache digest and rejects a named prefix root that disagrees with the claim's committed prefix writes. V11 and inherited V12 use the leaf context each coordinate belongs to. Missing, duplicated, reordered, misaddressed, ragged or unauthenticated tiles refuse the proof. Existing direct-carrier and owned-terminal evidence bounds still apply; below the public-filer fence and on V1 offence routes it is refused.

This is an objective consistency court, not a claim that the cache writes are correct model computation: a false write is still refuted by the arithmetic court. A partial seat checks the named prefix state before its sampled interval, so this state receipt fault is a claim obligation for every Valid signer. Initial non-KV/recurrence state needs its own court. Maximum prefix KV evidence can exceed the finite terminal budget; an incremental authenticated digest continuation and its service/session/resource guarantees remain unresolved. The new tests are source checkpoints only until the integrated compiler and selected tests execute.
