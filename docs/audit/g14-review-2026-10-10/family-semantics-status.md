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
