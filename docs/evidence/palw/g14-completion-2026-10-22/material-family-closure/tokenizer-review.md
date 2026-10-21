# Independent read review — Tokenizer family (initial review)

Reviewed module.rs/test-block.rs/NOTES.md and parser/producer modifications against actual native TableStream, pending_overrun, producer::generate and existing Store retained builder. No repo edits/build.

## Fix requested before approval: current native key-work bound provenance

`Generated::artifact_binding_v1()` authenticates actual flat EOF hash and length, but does not bind the native `signed_key_bound` of the base Court used by the producer. `generate` invokes `pending_overrun`; after that signed bound the parser legitimately stops producing key proofs while still consuming/sealing/authenticating all raw bytes. Thus the same immutable raw artifact can produce a shorter proof owner under a smaller bound, and the new family constructor currently accepts it against a different actual immutable `tokenizer_key_events` bound.

This does NOT bypass runtime native replay: it still rechecks current actual signed context/parser/material and can reject missing proofs. There is no present paid/Invalid/conviction authority granted by construction. Nevertheless, the new constructor cannot treat that owner's native proof-production provenance as sufficient for all current target contexts on rawhash/length alone.

Requested fix: retain producer's actual native signed key-work bound with the existing authenticated artifact scalars, require equality to actual immutable semantics.tokenizer_key_events (missing -> typed out-of-coverage), and exercise mismatched-bound provenance. Same-bound WorkBoundExceeded/Invalid must remain usable as authenticated local fault material; do not require JSON-valid success or invent a Complete boolean. Shared raw key event order is target/context independent only under the same native parser/rule/bound.

## Other inspected boundaries

- actual current frame equality, context/source/header, immutable table semantics, prospective Tokenizer stage, rooted EOF at/before acceptance, actual vocabulary/raw length, retained material scope are all checked;
- opaque Generated has no caller constructor/Deserialize, and new hash/length scalars are set only after existing complete raw authentication/seal/finish;
- all-V range is compact, no caller-selected subset/count constructor; actual each target must independently supply a current signed context and run existing native builder/fresh recheck;
- returned evidence borrows retained raw/rendering/proof owners; no new bulk clone; fixed metadata/source/frame scratch reservation occurs before reconstruction;
- unresolved supply is explicit and creates no fake supplier Finish, funding/retention obligation or whole-original authority;
- fixture performs every actual V target in distinct current signed contexts, alongside quota/drop, foreign source, EOF/target and current-key negatives. These remain uncompiled/unrun.

Review pending keybound correction. No other concrete blocker found by read.

## Key-work-bound correction re-review

Reviewed regenerated module/test-block/patch. The requested provenance defect is closed by read review:

- `Generated` now privately stores the actual producer Court's `signed_key_bound` after the same successful raw authentication/finish, and exposes a read-only scalar getter. Existing inline sizeof reservation includes the field; no new payload clone or decoder/caller constructor appears.
- Family requires actual immutable `sem.tokenizer_key_events` (missing is Resource/out-of-coverage) and exact equality with that generated scalar, in addition to actual EOF hash/length/scope. It still does not equate proof count with V or require a JSON-valid verdict, so same-bound authenticated Invalid/WorkBoundExceeded remains valid local fault material.
- The new counterexample uses the real native Court constructor with a different qualified statement's lower bound zero, then the real producer over the same raw bytes. It asserts actual same authenticated artifact binding and fewer proofs/different actual bound, and requires refusal against unchanged current state semantics. It does not construct private Generated fields or mutate the current state to hide the mismatch.
- Existing actual V/context replay and current-signer/EOF/material guards remain unchanged.

No remaining concrete blocker found in the corrected candidate by read. The initial requested fix above is resolved; compile and runtime fixture success remain unverified. The postclaim component/preclaim-native-authority limitation from the aggregate audit still applies.
